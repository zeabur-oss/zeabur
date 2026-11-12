//! The operations of the `graph` and `projection` fixture areas, each with its parameter type.

use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::Deserialize;
use serde_json::{Value, from_value, json, to_value};
use zeabur_graph::{
    Snapshot, Workload, attribution, classify, deployment_phase, env, env_from,
    is_machinery_secret, mounts, pods_of, port_protocol, project, provenance, workloads,
};
use zeabur_test_support::fixtures::{Fixture, FixtureOp, NoParams, run_op};

/// Runs a case through the operation it names.
///
/// # Errors
///
/// When the operation is unknown, its parameters do not fit, or the input does not.
pub(crate) fn run(fixture: &Fixture) -> Result<Value, String> {
    match fixture.case.operation.as_str() {
        Classify::NAME => run_op::<Classify>(fixture),
        ResolveGroup::NAME => run_op::<ResolveGroup>(fixture),
        Attribution::NAME => run_op::<Attribution>(fixture),
        DeploymentPhase::NAME => run_op::<DeploymentPhase>(fixture),
        IsMachinerySecret::NAME => run_op::<IsMachinerySecret>(fixture),
        Env::NAME => run_op::<Env>(fixture),
        EnvFrom::NAME => run_op::<EnvFrom>(fixture),
        Mounts::NAME => run_op::<Mounts>(fixture),
        PortProtocol::NAME => run_op::<PortProtocol>(fixture),
        Connections::NAME => run_op::<Connections>(fixture),
        Pods::NAME => run_op::<Pods>(fixture),
        ContainerResources::NAME => run_op::<ContainerResources>(fixture),
        SelectorAndPorts::NAME => run_op::<SelectorAndPorts>(fixture),
        Routes::NAME => run_op::<Routes>(fixture),
        Provenance::NAME => run_op::<Provenance>(fixture),
        other => Err(format!("{}: no runner for operation {other}", fixture.name)),
    }
}

/// The namespace of the cases whose operation takes none.
const DEMO: &str = "demo";

/// Builds the snapshot a cluster reader would: every input object typed by its kind, and
/// Secrets stripped to metadata and type.
///
/// # Errors
///
/// When an object does not parse or is of a kind the snapshot does not hold.
pub(crate) fn snapshot(input: &[Value], namespace: &str) -> Result<Snapshot, String> {
    let mut snapshot = Snapshot {
        namespace: namespace.to_owned(),
        ..Snapshot::default()
    };
    for object in input {
        let known = snapshot.insert(object.clone()).map_err(|e| e.to_string())?;
        if !known {
            return Err(format!("unsupported kind {}", object["kind"]));
        }
    }
    Ok(snapshot)
}

/// The case's only workload.
fn workload(snapshot: &Snapshot) -> Result<Workload<'_>, String> {
    match workloads(snapshot).as_slice() {
        [one] => Ok(*one),
        all => Err(format!("expected exactly one workload, got {}", all.len())),
    }
}

fn wire<T: serde::Serialize>(value: T) -> Result<Value, String> {
    to_value(value).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NamespaceParams {
    namespace: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContainerParams {
    container: String,
}

struct Classify;

impl FixtureOp for Classify {
    const NAME: &'static str = "graph.classify";
    type Params = NoParams;

    fn run(input: &[Value], _: Self::Params) -> Result<Value, String> {
        let snapshot = snapshot(input, DEMO)?;
        wire(classify(workload(&snapshot)?))
    }
}

struct ResolveGroup;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveGroupParams {
    labels: BTreeMap<String, String>,
}

impl FixtureOp for ResolveGroup {
    const NAME: &'static str = "graph.resolveGroup";
    type Params = ResolveGroupParams;

    fn run(_: &[Value], params: Self::Params) -> Result<Value, String> {
        Ok(json!(zeabur_model::adapters::group::resolve_group(Some(
            &params.labels
        ))))
    }
}

struct Attribution;

impl FixtureOp for Attribution {
    const NAME: &'static str = "graph.attribution";
    type Params = NamespaceParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        wire(attribution(&snapshot(input, &params.namespace)?))
    }
}

struct DeploymentPhase;

impl FixtureOp for DeploymentPhase {
    const NAME: &'static str = "graph.deploymentPhase";
    type Params = NoParams;

    fn run(input: &[Value], _: Self::Params) -> Result<Value, String> {
        let snapshot = snapshot(input, DEMO)?;
        let [deployment] = snapshot.deployments.as_slice() else {
            return Err("expected one Deployment".to_owned());
        };
        wire(deployment_phase(deployment))
    }
}

struct IsMachinerySecret;

impl FixtureOp for IsMachinerySecret {
    const NAME: &'static str = "graph.isMachinerySecret";
    type Params = NoParams;

    fn run(input: &[Value], _: Self::Params) -> Result<Value, String> {
        let snapshot = snapshot(input, DEMO)?;
        let [secret] = snapshot.secrets.as_slice() else {
            return Err("expected one Secret".to_owned());
        };
        Ok(json!(is_machinery_secret(secret)))
    }
}

struct Env;

impl FixtureOp for Env {
    const NAME: &'static str = "projection.env";
    type Params = ContainerParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let snapshot = snapshot(input, DEMO)?;
        let projected = env(workload(&snapshot)?, &params.container);
        wire(projected.ok_or_else(|| format!("no container {}", params.container))?)
    }
}

struct EnvFrom;

impl FixtureOp for EnvFrom {
    const NAME: &'static str = "projection.envFrom";
    type Params = ContainerParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let snapshot = snapshot(input, DEMO)?;
        let projected = env_from(&snapshot, workload(&snapshot)?, &params.container);
        wire(projected.ok_or_else(|| format!("no container {}", params.container))?)
    }
}

struct Mounts;

impl FixtureOp for Mounts {
    const NAME: &'static str = "projection.mounts";
    type Params = ContainerParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let snapshot = snapshot(input, DEMO)?;
        let attribution = attribution(&snapshot);
        let projected = mounts(
            &snapshot,
            &attribution,
            workload(&snapshot)?,
            &params.container,
        );
        wire(projected.ok_or_else(|| format!("no container {}", params.container))?)
    }
}

struct PortProtocol;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PortParams {
    /// The port's name.
    port: String,
}

impl FixtureOp for PortProtocol {
    const NAME: &'static str = "projection.portProtocol";
    type Params = PortParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let snapshot = snapshot(input, DEMO)?;
        let [service] = snapshot.services.as_slice() else {
            return Err("expected one Service".to_owned());
        };
        let ports = service.spec.as_ref().and_then(|spec| spec.ports.as_ref());
        let mut found = None;
        for port in ports.into_iter().flatten() {
            if port.name.as_deref() == Some(params.port.as_str()) {
                found = Some(port);
                break;
            }
        }
        let port = found.ok_or_else(|| format!("no port {}", params.port))?;
        wire(port_protocol(port))
    }
}

struct Connections;

impl FixtureOp for Connections {
    const NAME: &'static str = "projection.connections";
    type Params = NamespaceParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        wire(project(&snapshot(input, &params.namespace)?).connections)
    }
}

struct Pods;

impl FixtureOp for Pods {
    const NAME: &'static str = "projection.pods";
    type Params = NoParams;

    fn run(input: &[Value], _: Self::Params) -> Result<Value, String> {
        let snapshot = snapshot(input, DEMO)?;
        wire(pods_of(&snapshot, workload(&snapshot)?))
    }
}

struct ContainerResources;

impl FixtureOp for ContainerResources {
    const NAME: &'static str = "projection.containerResources";
    type Params = ContainerParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let snapshot = snapshot(input, DEMO)?;
        let mut services = project(&snapshot).services;
        if services.len() != 1 {
            return Err(format!("expected one service, got {}", services.len()));
        }
        for container in services.remove(0).containers {
            if container.name == params.container {
                return wire(container.resources);
            }
        }
        Err(format!("no container {}", params.container))
    }
}

struct SelectorAndPorts;

impl FixtureOp for SelectorAndPorts {
    const NAME: &'static str = "projection.selectorAndPorts";
    type Params = NoParams;

    fn run(input: &[Value], _: Self::Params) -> Result<Value, String> {
        let snapshot = snapshot(input, DEMO)?;
        let mut services = project(&snapshot).services;
        if services.len() != 1 {
            return Err(format!("expected one service, got {}", services.len()));
        }
        let service = services.remove(0);
        let mut ports = BTreeMap::new();
        for container in service.containers {
            ports.insert(container.name, container.ports);
        }
        Ok(json!({ "selector": service.selector, "ports": ports }))
    }
}

struct Routes;

impl FixtureOp for Routes {
    const NAME: &'static str = "projection.routes";
    type Params = NoParams;

    fn run(input: &[Value], _: Self::Params) -> Result<Value, String> {
        let projected = project(&snapshot(input, DEMO)?);
        // The routes of the first service in the projection's order.
        let service = projected.services.first().ok_or("no service")?;
        let routes: Vec<_> = projected.routes_of(service).collect();
        wire(routes)
    }
}

struct Provenance;

impl FixtureOp for Provenance {
    const NAME: &'static str = "projection.provenance";
    type Params = NoParams;

    fn run(input: &[Value], _: Self::Params) -> Result<Value, String> {
        let [object] = input else {
            return Err("expected one object".to_owned());
        };
        let meta: ObjectMeta = from_value(object["metadata"].clone()).map_err(|e| e.to_string())?;
        wire(provenance(&meta))
    }
}
