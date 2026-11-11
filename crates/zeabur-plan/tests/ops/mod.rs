//! The operations of the `mutation` fixture area, each with its parameter type.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::ReplicaSet;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, from_value, json, to_value};
use zeabur_graph::Snapshot;
use zeabur_model::WorkloadRef;
use zeabur_plan::checks::{
    ProbeRequest, ResourceRef, VolumeRequest, owned_by_stateful_set, probe_config,
    reconcile_identity, resource_allowed, validate_storage,
};
use zeabur_plan::rules::{EnvEdit, diff, route_name, validate_env, validate_hostname};
use zeabur_plan::{
    Attachable, CheckedCertificate, Fetched, Input, IssuerRead, PlanRequest, Release, ServedKind,
    StorageClassInfo, plan,
};
use zeabur_test_support::fixtures::{Fixture, FixtureOp, NoParams, Strict, run_op};

/// Runs a case through the operation it names.
///
/// # Errors
///
/// When the operation is unknown, its parameters do not fit, or the input does not.
pub(crate) fn run(fixture: &Fixture) -> Result<Value, String> {
    match fixture.case.operation.as_str() {
        ValidateEnv::NAME => run_op::<ValidateEnv>(fixture),
        ValidateHostname::NAME => run_op::<ValidateHostname>(fixture),
        RouteName::NAME => run_op::<RouteName>(fixture),
        Diff::NAME => run_op::<Diff>(fixture),
        ValidateStorage::NAME => run_op::<ValidateStorage>(fixture),
        ProbeConfig::NAME => run_op::<ProbeConfig>(fixture),
        ResourceAllowed::NAME => run_op::<ResourceAllowed>(fixture),
        ReconcileIdentity::NAME => run_op::<ReconcileIdentity>(fixture),
        OwnedByStatefulSet::NAME => run_op::<OwnedByStatefulSet>(fixture),
        Plan::NAME => run_op::<Plan>(fixture),
        other => Err(format!("{}: no runner for operation {other}", fixture.name)),
    }
}

fn outcome<T: serde::Serialize, E: std::fmt::Display>(
    result: Result<T, E>,
) -> Result<Value, String> {
    match result {
        Ok(value) => to_value(value).map_err(|e| e.to_string()),
        Err(error) => Ok(json!({"error": error.to_string()})),
    }
}

fn ok<E: std::fmt::Display>(result: Result<(), E>) -> Value {
    match result {
        Ok(()) => json!({"ok": true}),
        Err(error) => json!({"error": error.to_string()}),
    }
}

struct ValidateEnv;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateEnvParams {
    changes: Strict<Vec<EnvEdit>>,
}

impl FixtureOp for ValidateEnv {
    const NAME: &'static str = "mutation.validateEnv";
    type Params = ValidateEnvParams;

    fn run(_: &[Value], params: Self::Params) -> Result<Value, String> {
        // An unknown op still parses, so its refusal comes from the rule.
        Ok(ok(validate_env(&params.changes.0)))
    }
}

struct ValidateHostname;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostnameParams {
    hostname: String,
}

impl FixtureOp for ValidateHostname {
    const NAME: &'static str = "mutation.validateHostname";
    type Params = HostnameParams;

    fn run(_: &[Value], params: Self::Params) -> Result<Value, String> {
        Ok(ok(validate_hostname(&params.hostname)))
    }
}

struct RouteName;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteNameParams {
    workload: String,
    hostname: String,
}

impl FixtureOp for RouteName {
    const NAME: &'static str = "mutation.routeName";
    type Params = RouteNameParams;

    fn run(_: &[Value], params: Self::Params) -> Result<Value, String> {
        Ok(json!(route_name(&params.workload, &params.hostname)))
    }
}

struct Diff;

impl FixtureOp for Diff {
    const NAME: &'static str = "mutation.diff";
    type Params = NoParams;

    fn run(input: &[Value], _: Self::Params) -> Result<Value, String> {
        let changes = match input {
            [after] => diff(None, after),
            [before, after] => diff(Some(before), after),
            _ => return Err("one or two objects".to_owned()),
        };
        to_value(changes).map_err(|e| e.to_string())
    }
}

struct ValidateStorage;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateStorageParams {
    request: Strict<VolumeRequest>,
}

impl FixtureOp for ValidateStorage {
    const NAME: &'static str = "mutation.validateStorage";
    type Params = ValidateStorageParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let paths: Vec<&str> = input
            .first()
            .and_then(|object| object.pointer("/spec/template/spec/containers/0/volumeMounts"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|mount| mount["mountPath"].as_str())
            .collect();
        Ok(ok(validate_storage(&params.request.0, &paths)))
    }
}

struct ProbeConfig;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeConfigParams {
    container: String,
    probe: Strict<ProbeRequest>,
}

impl FixtureOp for ProbeConfig {
    const NAME: &'static str = "mutation.probeConfig";
    type Params = ProbeConfigParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let containers = input
            .first()
            .and_then(|object| object.pointer("/spec/template/spec/containers"))
            .and_then(Value::as_array)
            .ok_or("the workload has no containers")?;
        let ports: Vec<i32> = containers
            .iter()
            .filter(|c| c["name"] == params.container.as_str())
            .flat_map(|c| c["ports"].as_array().into_iter().flatten())
            .filter_map(|p| p["containerPort"].as_i64())
            .filter_map(|p| i32::try_from(p).ok())
            .collect();
        outcome(probe_config(&params.probe.0, &ports))
    }
}

struct ResourceAllowed;

impl FixtureOp for ResourceAllowed {
    const NAME: &'static str = "mutation.resourceAllowed";
    type Params = Strict<ResourceRef>;

    fn run(_: &[Value], params: Self::Params) -> Result<Value, String> {
        to_value(resource_allowed(&params.0)).map_err(|e| e.to_string())
    }
}

struct ReconcileIdentity;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconcileIdentityParams {
    resource: Strict<ResourceRef>,
    kind: String,
    namespace: String,
    name: String,
    body: Value,
}

impl FixtureOp for ReconcileIdentity {
    const NAME: &'static str = "mutation.reconcileIdentity";
    type Params = ReconcileIdentityParams;

    fn run(_: &[Value], params: Self::Params) -> Result<Value, String> {
        Ok(ok(reconcile_identity(
            &params.resource.0,
            &params.kind,
            &params.namespace,
            &params.name,
            &params.body,
        )))
    }
}

struct OwnedByStatefulSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct OwnedByStatefulSetParams {
    claim: String,
    stateful_set: String,
}

impl FixtureOp for OwnedByStatefulSet {
    const NAME: &'static str = "mutation.ownedByStatefulSet";
    type Params = OwnedByStatefulSetParams;

    fn run(_: &[Value], params: Self::Params) -> Result<Value, String> {
        Ok(json!(owned_by_stateful_set(
            &params.claim,
            &params.stateful_set
        )))
    }
}

/// A list that may be absent, or `null` for "could not be read".
#[derive(Default)]
enum Listed<T> {
    #[default]
    Absent,
    Unreadable,
    Items(T),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Listed<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Option::<T>::deserialize(deserializer)?.map_or(Self::Unreadable, Self::Items))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct StorageClassParam {
    name: String,
    #[serde(default)]
    allow_volume_expansion: bool,
    #[serde(default)]
    default: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KindParam {
    group: String,
    kind: String,
    plural: String,
    namespaced: bool,
}

/// What a `ClusterIssuer` read gave.
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum IssuerParam {
    Dns01,
    Http01,
    Missing,
    Unreadable,
}

impl From<IssuerParam> for IssuerRead {
    fn from(read: IssuerParam) -> Self {
        match read {
            IssuerParam::Dns01 => Self::Found { wildcards: true },
            IssuerParam::Http01 => Self::Found { wildcards: false },
            IssuerParam::Missing => Self::Missing,
            IssuerParam::Unreadable => Self::Unreadable,
        }
    }
}

impl From<KindParam> for ServedKind {
    fn from(kind: KindParam) -> Self {
        Self {
            group: kind.group,
            kind: kind.kind,
            plural: kind.plural,
            namespaced: kind.namespaced,
        }
    }
}

impl From<StorageClassParam> for StorageClassInfo {
    fn from(class: StorageClassParam) -> Self {
        Self {
            name: class.name,
            allow_expansion: class.allow_volume_expansion,
            default: class.default,
        }
    }
}

fn demo() -> String {
    "demo".to_owned()
}

fn now() -> String {
    "2026-10-15T00:00:00Z".to_owned()
}

struct Plan;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PlanParams {
    request: Strict<PlanRequest>,
    #[serde(default = "demo")]
    namespace: String,
    #[serde(default = "now")]
    now: String,
    #[serde(default)]
    reloader: Option<bool>,
    /// The Gateway objects of every namespace.
    #[serde(default)]
    gateways: Listed<Vec<Value>>,
    #[serde(default)]
    namespace_labels: BTreeMap<String, String>,
    #[serde(default)]
    releases: Strict<Vec<(WorkloadRef, Vec<Release>)>>,
    #[serde(default)]
    storage_classes: Listed<Vec<StorageClassParam>>,
    #[serde(default)]
    secret_values: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    reclaim_policies: BTreeMap<String, String>,
    #[serde(default)]
    kinds: Vec<KindParam>,
    #[serde(default)]
    cluster_issuer: Option<String>,
    #[serde(default)]
    issuers: BTreeMap<String, IssuerParam>,
    #[serde(default)]
    certificates: BTreeMap<String, String>,
    #[serde(default)]
    build_registry: Option<String>,
}

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

/// Every object of the input, as read for a YAML edit or a deletion.
fn fetched(input: &[Value]) -> Vec<Fetched> {
    input
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
        .collect()
}

impl FixtureOp for Plan {
    const NAME: &'static str = "mutation.plan";
    type Params = PlanParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let mut snapshot = Snapshot {
            namespace: params.namespace.clone(),
            ..Snapshot::default()
        };
        let mut routes = Vec::new();
        for object in input {
            // Other kinds are read only for YAML edits (`Fetched`).
            let inserted = snapshot.insert(object.clone()).map_err(|e| e.to_string())?;
            if !inserted && object["kind"] == "HTTPRoute" {
                routes.push(object.clone());
            }
        }
        // Gateway objects, of which the plan sees those that accept a route from the namespace
        // (labelled `namespaceLabels`); none when they cannot be listed.
        let gateways: Option<Vec<Attachable>> = match &params.gateways {
            Listed::Unreadable => None,
            Listed::Items(list) => Some(zeabur_plan::attachable(
                list,
                &snapshot.namespace,
                &params.namespace_labels,
            )),
            Listed::Absent => Some(Vec::new()),
        };
        let secret_values: BTreeMap<String, BTreeMap<String, Vec<u8>>> = params
            .secret_values
            .iter()
            .map(|(name, keys)| {
                let keys = keys
                    .iter()
                    .map(|(key, value)| (key.clone(), value.as_bytes().to_vec()))
                    .collect();
                (name.clone(), keys)
            })
            .collect();
        let releases: BTreeMap<WorkloadRef, Vec<Release>> = params.releases.0.into_iter().collect();
        let storage_classes: Option<Vec<StorageClassInfo>> = match params.storage_classes {
            Listed::Unreadable => None,
            Listed::Items(list) => Some(list.into_iter().map(Into::into).collect()),
            Listed::Absent => Some(Vec::new()),
        };
        let objects = fetched(input);
        let replica_sets: Vec<ReplicaSet> = input
            .iter()
            .filter(|o| o["kind"] == "ReplicaSet")
            .map(|o| from_value(o.clone()).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        let kinds: Vec<ServedKind> = params.kinds.into_iter().map(Into::into).collect();
        let issuers: BTreeMap<String, IssuerRead> = params
            .issuers
            .into_iter()
            .map(|(name, read)| (name, read.into()))
            .collect();
        let certificates: BTreeMap<String, CheckedCertificate> = params
            .certificates
            .into_iter()
            .map(|(host, not_after)| {
                let checked = CheckedCertificate {
                    not_after,
                    certificate: format!("CERT {host}"),
                    key: format!("KEY {host}"),
                };
                (host, checked)
            })
            .collect();
        let planned = plan(
            Input {
                snapshot: &snapshot,
                http_routes: &routes,
                gateways: gateways.as_deref(),
                secret_values: &secret_values,
                now: &params.now,
                releases: &releases,
                storage_classes: storage_classes.as_deref(),
                objects: &objects,
                replica_sets: &replica_sets,
                reclaim_policies: &params.reclaim_policies,
                namespace: None,
                reloader: params.reloader,
                kinds: &kinds,
                deletion: None,
                cluster_issuer: params.cluster_issuer.as_deref(),
                issuers: &issuers,
                certificates: &certificates,
                platform_namespaces: false,
                build_registry: params.build_registry.as_deref(),
                node_capacity: None,
            },
            &params.request.0,
        );
        match planned {
            Ok(planned) => to_value(planned).map_err(|e| e.to_string()),
            Err(error) => Ok(json!({"error": error.code})),
        }
    }
}
