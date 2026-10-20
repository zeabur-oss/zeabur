//! Runs every case of the `graph` and `projection` fixture areas against the projection.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use k8s_openapi::ByteString;
    use k8s_openapi::api::apps::v1::Deployment;
    use k8s_openapi::api::core::v1::{EnvFromSource, Secret, SecretEnvSource};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
    use serde_json::{Value, from_value, json, to_value};
    use zeabur_graph::{
        Snapshot, Workload, attribution, classify, deployment_phase, env, env_from,
        is_machinery_secret, mounts, pods_of, port_protocol, project, provenance, workloads,
    };
    use zeabur_model::WorkloadRef;
    use zeabur_test_support::fixtures::{Fixture, compare, load};

    /// Builds the snapshot a cluster reader would: every input object typed by its kind, and
    /// Secrets stripped to metadata and type.
    fn snapshot(fixture: &Fixture) -> Snapshot {
        let mut snapshot = Snapshot {
            namespace: fixture.case.params["namespace"]
                .as_str()
                .unwrap_or("demo")
                .to_owned(),
            ..Snapshot::default()
        };
        for object in &fixture.input {
            let known = snapshot.insert(object.clone()).unwrap();
            assert!(
                known,
                "{}: unsupported kind {}",
                fixture.name, object["kind"]
            );
        }
        snapshot
    }

    /// The case's only workload.
    fn workload(snapshot: &Snapshot) -> Workload<'_> {
        let all = workloads(snapshot);
        assert_eq!(all.len(), 1, "expected exactly one workload");
        all[0]
    }

    fn param<'a>(fixture: &'a Fixture, name: &str) -> &'a str {
        fixture.case.params[name]
            .as_str()
            .unwrap_or_else(|| panic!("{}: no string param {name}", fixture.name))
    }

    /// Runs the case's operation and returns its result in wire form.
    fn run(fixture: &Fixture) -> Value {
        let snapshot = snapshot(fixture);
        let operation = fixture.case.operation.as_str();
        match operation {
            "graph.classify" => to_value(classify(workload(&snapshot))).unwrap(),
            "graph.resolveGroup" => {
                let labels: BTreeMap<String, String> =
                    from_value(fixture.case.params["labels"].clone()).unwrap();
                json!(zeabur_adapters::group::resolve_group(Some(&labels)))
            }
            "graph.attribution" => to_value(attribution(&snapshot)).unwrap(),
            "graph.deploymentPhase" => {
                to_value(deployment_phase(&snapshot.deployments[0])).unwrap()
            }
            "graph.isMachinerySecret" => json!(is_machinery_secret(&snapshot.secrets[0])),
            "projection.env" => {
                to_value(env(workload(&snapshot), param(fixture, "container")).unwrap()).unwrap()
            }
            "projection.envFrom" => to_value(
                env_from(&snapshot, workload(&snapshot), param(fixture, "container")).unwrap(),
            )
            .unwrap(),
            "projection.mounts" => {
                let attribution = attribution(&snapshot);
                to_value(
                    mounts(
                        &snapshot,
                        &attribution,
                        workload(&snapshot),
                        param(fixture, "container"),
                    )
                    .unwrap(),
                )
                .unwrap()
            }
            "projection.portProtocol" => {
                let name = param(fixture, "port");
                let port = snapshot.services[0]
                    .spec
                    .as_ref()
                    .and_then(|spec| spec.ports.as_ref())
                    .and_then(|ports| ports.iter().find(|port| port.name.as_deref() == Some(name)))
                    .unwrap();
                to_value(port_protocol(port)).unwrap()
            }
            "projection.connections" => to_value(project(&snapshot).connections).unwrap(),
            "projection.pods" => to_value(pods_of(&snapshot, workload(&snapshot))).unwrap(),
            "projection.containerResources" => {
                let name = param(fixture, "container");
                let service = project(&snapshot).services.remove(0);
                let container = service
                    .containers
                    .into_iter()
                    .find(|container| container.name == name)
                    .unwrap();
                to_value(container.resources).unwrap()
            }
            "projection.routes" => {
                let projected = project(&snapshot);
                let routes: Vec<_> = projected.routes_of(&projected.services[0]).collect();
                to_value(routes).unwrap()
            }
            "projection.provenance" => {
                let meta: ObjectMeta = from_value(fixture.input[0]["metadata"].clone()).unwrap();
                to_value(provenance(&meta)).unwrap()
            }
            other => panic!("{}: no runner for operation {other}", fixture.name),
        }
    }

    #[test]
    fn graph_and_projection_fixtures_pass() {
        let mut failures = Vec::new();
        let mut total = 0;
        for area in ["graph", "projection"] {
            for fixture in load(area).unwrap() {
                total += 1;
                let actual = run(&fixture);
                match compare(&fixture.expected, &actual) {
                    Ok(()) => println!("pass {}", fixture.name),
                    Err(difference) => {
                        println!("FAIL {}: {difference}", fixture.name);
                        failures.push(format!("{}: {difference}", fixture.name));
                    }
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

    /// The claim in `Snapshot`'s documentation: a Secret's values never reach a projected view,
    /// even when a reader leaves them in the snapshot.
    #[test]
    fn secret_values_never_reach_the_projection() {
        let value = "s3cr3t-value";
        let mut snapshot = Snapshot {
            namespace: "demo".to_owned(),
            ..Snapshot::default()
        };
        snapshot.secrets.push(Secret {
            metadata: ObjectMeta {
                name: Some("db".to_owned()),
                namespace: Some("demo".to_owned()),
                ..ObjectMeta::default()
            },
            data: Some(BTreeMap::from([(
                "password".to_owned(),
                ByteString(value.as_bytes().to_vec()),
            )])),
            string_data: Some(BTreeMap::from([("token".to_owned(), value.to_owned())])),
            type_: Some("Opaque".to_owned()),
            ..Secret::default()
        });
        let mut deployment: Deployment = from_value(json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": {"name": "api", "namespace": "demo"},
            "spec": {
                "selector": {"matchLabels": {"app": "api"}},
                "template": {
                    "metadata": {"labels": {"app": "api"}},
                    "spec": {
                        "containers": [{
                            "name": "app",
                            "env": [{
                                "name": "PASSWORD",
                                "valueFrom": {"secretKeyRef": {"name": "db", "key": "password"}}
                            }],
                            "volumeMounts": [{"name": "creds", "mountPath": "/creds"}]
                        }],
                        "volumes": [{"name": "creds", "secret": {"secretName": "db"}}]
                    }
                }
            }
        }))
        .unwrap();
        let container = &mut deployment
            .spec
            .as_mut()
            .unwrap()
            .template
            .spec
            .as_mut()
            .unwrap()
            .containers[0];
        container.env_from = Some(vec![EnvFromSource {
            secret_ref: Some(SecretEnvSource {
                name: "db".to_owned(),
                optional: None,
            }),
            ..EnvFromSource::default()
        }]);
        snapshot.deployments.push(deployment);

        let projected = serde_json::to_string(&project(&snapshot)).unwrap();
        assert!(!projected.contains(value), "{projected}");
        assert!(
            !projected.contains("czNjcjN0"),
            "base64 of the value: {projected}"
        );
        assert!(
            !projected.contains("token"),
            "a key name of the Secret: {projected}"
        );
        // The Secret is still attributed: references are read, values are not.
        assert_eq!(
            attribution(&snapshot).secrets["db"].owners,
            vec![WorkloadRef {
                kind: zeabur_model::WorkloadKind::Deployment,
                name: "api".to_owned()
            }]
        );
    }

    /// The project of the attribution fixture: one service per workload, the shared ConfigMap
    /// under both, and the Service as the n8n Deployment's endpoint, listed once in the project.
    #[test]
    fn project_groups_resources_under_workloads() {
        let fixture = load("graph")
            .unwrap()
            .into_iter()
            .find(|fixture| fixture.name == "graph/attribution-shared-configmap-and-orphan-pvc")
            .unwrap();
        let project = to_value(project(&snapshot(&fixture))).unwrap();
        compare(
            &json!({
                "namespace": "demo",
                "services": [
                    {
                        "kind": "Deployment",
                        "name": "n8n",
                        "group": "default",
                        "containers": [{
                            "name": "app",
                            "importedEnv": [{
                                "source": {"kind": "ConfigMap", "name": "app-config"},
                                "keyCount": 0,
                                "readOnly": false,
                                "origin": {
                                    "kind": "Deployment",
                                    "name": "n8n",
                                    "fieldPath": "spec.template.spec.containers[name=app].envFrom[0]"
                                }
                            }]
                        }],
                        "endpointNames": ["n8n"]
                    },
                    {"kind": "Deployment", "name": "worker", "endpointNames": []}
                ],
                "endpoints": [{"name": "n8n", "origin": {"kind": "Service", "name": "n8n"}}],
                "resources": [
                    {"kind": "ConfigMap", "name": "app-config", "attribution": {"ownership": "shared"}},
                    {"kind": "PersistentVolumeClaim", "name": "leftover-data", "attribution": {"ownership": "orphan"}},
                    {"kind": "Service", "name": "n8n", "attribution": {"ownership": "owned"}}
                ]
            }),
            &project,
        )
        .unwrap();
    }
}
