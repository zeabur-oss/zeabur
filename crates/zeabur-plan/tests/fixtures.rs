//! Runs the cases of the `mutation` fixture area that plans and their writes rest on.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use k8s_openapi::api::apps::v1::ReplicaSet;
    use serde_json::{Value, from_value, json, to_value};
    use zeabur_fixtures::{Fixture, compare, load};
    use zeabur_graph::Snapshot;
    use zeabur_model::WorkloadRef;
    use zeabur_plan::checks::{
        ProbeRequest, ResourceRef, VolumeRequest, owned_by_stateful_set, probe_config,
        reconcile_identity, resource_allowed, validate_storage,
    };
    use zeabur_plan::rules::{EnvEdit, diff, route_name, validate_env, validate_hostname};
    use zeabur_plan::{
        Fetched, GatewayRef, Input, PlanRequest, Release, ServedKind, StorageClassInfo, plan,
    };

    /// The resource (plural) of a kind, for the fixtures' objects.
    fn plural(kind: &str) -> String {
        let lower = kind.to_ascii_lowercase();
        if lower.ends_with('y') {
            format!("{}ies", lower.trim_end_matches('y'))
        } else if lower.ends_with('s') {
            format!("{lower}es")
        } else {
            format!("{lower}s")
        }
    }

    /// The mount paths of the first container of the fixture's first object.
    fn mount_paths(object: &Value) -> Vec<String> {
        object
            .pointer("/spec/template/spec/containers/0/volumeMounts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| m["mountPath"].as_str().map(str::to_owned))
            .collect()
    }

    fn snapshot(fixture: &Fixture) -> (Snapshot, Vec<Value>) {
        let mut snapshot = Snapshot {
            namespace: fixture.case.params["namespace"]
                .as_str()
                .unwrap_or("demo")
                .to_owned(),
            ..Snapshot::default()
        };
        let mut routes = Vec::new();
        for object in &fixture.input {
            // Other kinds are read only for YAML edits (`Fetched`).
            if !snapshot.insert(object.clone()).unwrap() && object["kind"] == "HTTPRoute" {
                routes.push(object.clone());
            }
        }
        (snapshot, routes)
    }

    fn outcome<T: serde::Serialize, E: std::fmt::Display>(result: Result<T, E>) -> Value {
        match result {
            Ok(value) => to_value(value).unwrap(),
            Err(error) => json!({"error": error.to_string()}),
        }
    }

    fn ok<E: std::fmt::Display>(result: Result<(), E>) -> Value {
        match result {
            Ok(()) => json!({"ok": true}),
            Err(error) => json!({"error": error.to_string()}),
        }
    }

    /// `params.kinds`: the kinds discovery names.
    fn served_kinds(params: &Value) -> Vec<ServedKind> {
        params
            .get("kinds")
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |list| {
                list.iter()
                    .map(|k| ServedKind {
                        group: k["group"].as_str().unwrap().to_owned(),
                        kind: k["kind"].as_str().unwrap().to_owned(),
                        plural: k["plural"].as_str().unwrap().to_owned(),
                        namespaced: k["namespaced"].as_bool().unwrap(),
                    })
                    .collect()
            })
    }

    /// `mutation.plan`: the request planned against the fixture's objects.
    fn run_plan(fixture: &Fixture) -> Value {
        let params = &fixture.case.params;
        let (snapshot, routes) = snapshot(fixture);
        let request: PlanRequest = from_value(params["request"].clone()).unwrap();
        let gateways: Option<Vec<GatewayRef>> = match params.get("gateways") {
            Some(Value::Null) => None,
            Some(list) => Some(
                list.as_array()
                    .unwrap()
                    .iter()
                    .map(|g| GatewayRef {
                        namespace: g["namespace"].as_str().unwrap().to_owned(),
                        name: g["name"].as_str().unwrap().to_owned(),
                    })
                    .collect(),
            ),
            None => Some(Vec::new()),
        };
        let secret_values: BTreeMap<String, BTreeMap<String, Vec<u8>>> = params
            .get("secretValues")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(name, keys)| {
                let keys = keys
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap().as_bytes().to_vec()))
                    .collect();
                (name.clone(), keys)
            })
            .collect();
        let releases: BTreeMap<WorkloadRef, Vec<Release>> =
            params.get("releases").map_or_else(BTreeMap::new, |list| {
                let entries: Vec<(WorkloadRef, Vec<Release>)> = from_value(list.clone()).unwrap();
                entries.into_iter().collect()
            });
        let storage_classes: Option<Vec<StorageClassInfo>> = match params.get("storageClasses") {
            Some(Value::Null) => None,
            Some(list) => Some(
                list.as_array()
                    .unwrap()
                    .iter()
                    .map(|c| StorageClassInfo {
                        name: c["name"].as_str().unwrap().to_owned(),
                        allow_expansion: c["allowVolumeExpansion"].as_bool().unwrap_or(false),
                        default: c["default"].as_bool().unwrap_or(false),
                    })
                    .collect(),
            ),
            None => Some(Vec::new()),
        };
        let objects: Vec<Fetched> = fixture
            .input
            .iter()
            .filter_map(|object| {
                let api_version = object["apiVersion"].as_str()?;
                let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
                Some(Fetched {
                    resource: ResourceRef {
                        group: group.to_owned(),
                        version: version.to_owned(),
                        resource: plural(object["kind"].as_str()?),
                    },
                    name: object.pointer("/metadata/name")?.as_str()?.to_owned(),
                    object: object.clone(),
                })
            })
            .collect();
        let replica_sets: Vec<ReplicaSet> = fixture
            .input
            .iter()
            .filter(|o| o["kind"] == "ReplicaSet")
            .map(|o| from_value(o.clone()).unwrap())
            .collect();
        let reclaim_policies: BTreeMap<String, String> = params
            .get("reclaimPolicies")
            .map_or_else(BTreeMap::new, |p| from_value(p.clone()).unwrap());
        let kinds = served_kinds(params);
        let input = Input {
            snapshot: &snapshot,
            http_routes: &routes,
            gateways: gateways.as_deref(),
            secret_values: &secret_values,
            now: params["now"].as_str().unwrap_or("2026-10-15T00:00:00Z"),
            releases: &releases,
            storage_classes: storage_classes.as_deref(),
            objects: &objects,
            replica_sets: &replica_sets,
            reclaim_policies: &reclaim_policies,
            namespace: None,
            reloader: params.get("reloader").and_then(Value::as_bool),
            kinds: &kinds,
        };
        match plan(input, &request) {
            Ok(plan) => to_value(plan).unwrap(),
            Err(error) => json!({"error": error.code}),
        }
    }

    fn run(fixture: &Fixture) -> Value {
        let params = &fixture.case.params;
        let first = fixture.input.first();
        match fixture.case.operation.as_str() {
            "mutation.validateEnv" => {
                // An unknown op still parses, so its refusal comes from the rule.
                let changes: Vec<EnvEdit> = from_value(params["changes"].clone()).unwrap();
                ok(validate_env(&changes))
            }
            "mutation.validateHostname" => {
                ok(validate_hostname(params["hostname"].as_str().unwrap()))
            }
            "mutation.routeName" => json!(route_name(
                params["workload"].as_str().unwrap(),
                params["hostname"].as_str().unwrap()
            )),
            "mutation.diff" => match fixture.input.as_slice() {
                [after] => to_value(diff(None, after)).unwrap(),
                [before, after] => to_value(diff(Some(before), after)).unwrap(),
                _ => panic!("{}: one or two objects", fixture.name),
            },
            "mutation.validateStorage" => {
                let request: VolumeRequest = from_value(params["request"].clone()).unwrap();
                let paths = first.map_or_else(Vec::new, mount_paths);
                let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
                ok(validate_storage(&request, &paths))
            }
            "mutation.probeConfig" => {
                let request: ProbeRequest = from_value(params["probe"].clone()).unwrap();
                let container = params["container"].as_str().unwrap();
                let ports: Vec<i32> = first
                    .unwrap()
                    .pointer("/spec/template/spec/containers")
                    .and_then(Value::as_array)
                    .unwrap()
                    .iter()
                    .filter(|c| c["name"] == container)
                    .flat_map(|c| c["ports"].as_array().cloned().unwrap_or_default())
                    .filter_map(|p| p["containerPort"].as_i64())
                    .map(|p| i32::try_from(p).unwrap())
                    .collect();
                outcome(probe_config(&request, &ports))
            }
            "mutation.resourceAllowed" => {
                let resource: ResourceRef = from_value(params.clone()).unwrap();
                to_value(resource_allowed(&resource)).unwrap()
            }
            "mutation.reconcileIdentity" => {
                let resource: ResourceRef = from_value(params["resource"].clone()).unwrap();
                ok(reconcile_identity(
                    &resource,
                    params["kind"].as_str().unwrap(),
                    params["namespace"].as_str().unwrap(),
                    params["name"].as_str().unwrap(),
                    &params["body"],
                ))
            }
            "mutation.ownedByStatefulSet" => json!(owned_by_stateful_set(
                params["claim"].as_str().unwrap(),
                params["statefulSet"].as_str().unwrap()
            )),
            "mutation.plan" => run_plan(fixture),
            other => panic!("{}: no runner for operation {other}", fixture.name),
        }
    }

    #[test]
    fn mutation_fixtures_pass() {
        let mut failures = Vec::new();
        let mut total = 0;
        for fixture in load!("mutation").unwrap() {
            total += 1;
            let actual = run(&fixture);
            // A mask shown in place of a Secret value is never written.
            for object in actual["objects"].as_array().into_iter().flatten() {
                let write = object["write"].to_string();
                assert!(
                    !write.contains("«redacted»")
                        && !write.contains("(hidden)")
                        && !write.contains('•'),
                    "{}: a write carries a mask: {write}",
                    fixture.name
                );
            }
            match compare(&fixture.expected, &actual) {
                Ok(()) => println!("pass {}", fixture.name),
                Err(difference) => {
                    println!("FAIL {}: {difference}\n  actual: {actual}", fixture.name);
                    failures.push(format!("{}: {difference}", fixture.name));
                }
            }
        }
        assert!(total > 0, "no fixtures found");
        assert!(
            failures.is_empty(),
            "{} of {total} failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
