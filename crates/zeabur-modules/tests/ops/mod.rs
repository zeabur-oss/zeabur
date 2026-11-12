//! The operations of the `modules` fixture area, each with its parameter type.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, from_value, to_value};
use zeabur_kube::DynamicObject;
use zeabur_modules::{backups, cloudnative_pg, k8up, network_policy, velero};
use zeabur_test_support::fixtures::{Fixture, FixtureOp, run_op};

/// Runs a case through the operation it names.
///
/// # Errors
///
/// When the operation is unknown, its parameters do not fit, or the input does not.
pub(crate) fn run(fixture: &Fixture) -> Result<Value, String> {
    match fixture.case.operation.as_str() {
        NetworkReachability::NAME => run_op::<NetworkReachability>(fixture),
        NetworkGaps::NAME => run_op::<NetworkGaps>(fixture),
        Backups::NAME => run_op::<Backups>(fixture),
        BackupGaps::NAME => run_op::<BackupGaps>(fixture),
        other => Err(format!("{}: no runner for operation {other}", fixture.name)),
    }
}

fn wire<T: serde::Serialize>(value: T) -> Result<Value, String> {
    to_value(value).map_err(|e| e.to_string())
}

/// The input objects of a kind whose `apiVersion` is in `api_group`.
fn objects(input: &[Value], kind: &str, api_group: &str) -> Result<Vec<DynamicObject>, String> {
    let mut found = Vec::new();
    for object in input {
        let version = object["apiVersion"].as_str().unwrap_or_default();
        if object["kind"] == kind && version.starts_with(api_group) {
            found.push(from_value(object.clone()).map_err(|e| e.to_string())?);
        }
    }
    Ok(found)
}

fn of_kind<'a>(input: &'a [Value], kind: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
    input.iter().filter(move |object| object["kind"] == kind)
}

/// A string map such as labels; absent is empty.
fn strings(value: Option<&Value>) -> Result<BTreeMap<String, String>, String> {
    match value {
        None | Some(Value::Null) => Ok(BTreeMap::new()),
        Some(map) => from_value(map.clone()).map_err(|e| e.to_string()),
    }
}

fn name(object: &Value) -> Result<String, String> {
    let name = object.pointer("/metadata/name").and_then(Value::as_str);
    name.map(str::to_owned)
        .ok_or_else(|| "an object without a name".to_owned())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetworkParams {
    namespace: String,
    /// Caller and callee, by Deployment name.
    connections: Vec<(String, String)>,
}

fn reach(
    input: &[Value],
    params: &NetworkParams,
) -> Result<(Vec<network_policy::Workload>, network_policy::Reachability), String> {
    let namespace = &params.namespace;
    let id = |name: &str| format!("projects/{namespace}/services/deployment/{name}");
    let mut workloads = Vec::new();
    for deployment in of_kind(input, "Deployment") {
        workloads.push(network_policy::Workload {
            id: id(&name(deployment)?),
            labels: strings(deployment.pointer("/spec/template/metadata/labels"))?,
        });
    }
    let mut connections = Vec::new();
    for (from, to) in &params.connections {
        connections.push((id(from), id(to)));
    }
    let policies = objects(input, "NetworkPolicy", "networking.k8s.io")?;
    let reach = network_policy::reachability(network_policy::Input {
        namespace,
        workloads: &workloads,
        policies: &policies,
        connections: &connections,
    });
    Ok((workloads, reach))
}

struct NetworkReachability;

impl FixtureOp for NetworkReachability {
    const NAME: &'static str = "modules.networkReachability";
    type Params = NetworkParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let (_, reach) = reach(input, &params)?;
        wire(reach)
    }
}

struct NetworkGaps;

impl FixtureOp for NetworkGaps {
    const NAME: &'static str = "modules.networkGaps";
    type Params = NetworkParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let (workloads, reach) = reach(input, &params)?;
        wire(network_policy::gaps(&params.namespace, &reach, &workloads))
    }
}

fn coverage(input: &[Value], namespace: &str) -> Result<Vec<backups::Coverage>, String> {
    let mut claims = Vec::new();
    for claim in of_kind(input, "PersistentVolumeClaim") {
        claims.push(k8up::Volume {
            name: name(claim)?,
            labels: strings(claim.pointer("/metadata/labels"))?,
            annotations: strings(claim.pointer("/metadata/annotations"))?,
        });
    }
    let names: Vec<String> = claims.iter().map(|claim| claim.name.clone()).collect();
    let databases = cloudnative_pg::databases(&objects(input, "Cluster", "postgresql.cnpg.io")?);
    let findings = [
        k8up::findings(
            namespace,
            &objects(input, "Schedule", "k8up.io")?,
            &objects(input, "Backup", "k8up.io")?,
            &claims,
        ),
        velero::findings(
            namespace,
            &objects(input, "Schedule", "velero.io")?,
            &objects(input, "Backup", "velero.io")?,
            &names,
        ),
        cloudnative_pg::findings(
            namespace,
            &objects(input, "ScheduledBackup", "postgresql.cnpg.io")?,
            &objects(input, "Backup", "postgresql.cnpg.io")?,
        ),
    ];
    let targets = backups::targets(namespace, &names, &databases);
    Ok(backups::coverage(&targets, &findings))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupsParams {
    namespace: String,
}

struct Backups;

impl FixtureOp for Backups {
    const NAME: &'static str = "modules.backups";
    type Params = BackupsParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        wire(coverage(input, &params.namespace)?)
    }
}

/// Which backup components the cluster runs.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstalledParam {
    k8up: bool,
    velero: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BackupGapsParams {
    namespace: String,
    installed: InstalledParam,
}

struct BackupGaps;

impl FixtureOp for BackupGaps {
    const NAME: &'static str = "modules.backupGaps";
    type Params = BackupGapsParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let installed = backups::Installed {
            k8up: params.installed.k8up,
            velero: params.installed.velero,
        };
        wire(backups::gaps(
            &coverage(input, &params.namespace)?,
            installed,
        ))
    }
}
