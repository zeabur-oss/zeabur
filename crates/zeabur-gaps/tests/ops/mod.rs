//! The operations of the `gaps` fixture area, each with its parameter type.

use k8s_openapi::jiff::Timestamp;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, from_value, to_value};
use zeabur_gaps::{BUILTIN, ClusterTraits, Event, GatewayRef, Input, Namespace, Rule, evaluate};
use zeabur_graph::{Snapshot, project};
use zeabur_model::HttpRoute;
use zeabur_test_support::fixtures::{Fixture, FixtureOp, run_op};

/// Runs a case through the operation it names.
///
/// # Errors
///
/// When the operation is unknown, its parameters do not fit, or the input does not.
pub(crate) fn run(fixture: &Fixture) -> Result<Value, String> {
    match fixture.case.operation.as_str() {
        Diagnose::NAME => run_op::<Diagnose>(fixture),
        Rules::NAME => run_op::<Rules>(fixture),
        other => Err(format!("{}: no runner for operation {other}", fixture.name)),
    }
}

/// The objects of a case as a cluster reader holds them: the snapshot, the events and the
/// `HTTPRoutes`.
struct Read {
    snapshot: Snapshot,
    events: Vec<Event>,
    http_routes: Vec<Value>,
}

fn read(input: &[Value], namespace: &str, now: Option<Timestamp>) -> Result<Read, String> {
    let mut snapshot = Snapshot {
        namespace: namespace.to_owned(),
        now,
        ..Snapshot::default()
    };
    let mut events = Vec::new();
    let mut http_routes = Vec::new();
    for object in input {
        if snapshot.insert(object.clone()).map_err(|e| e.to_string())? {
            continue;
        }
        match object["kind"].as_str() {
            Some("Event") => {
                let event = from_value(object.clone()).map_err(|e| e.to_string())?;
                events.push(Event::from_kube(&event));
            }
            Some("HTTPRoute") => http_routes.push(object.clone()),
            kind => return Err(format!("unsupported kind {kind:?}")),
        }
    }
    Ok(Read {
        snapshot,
        events,
        http_routes,
    })
}

const fn readable() -> bool {
    true
}

fn demo() -> String {
    "demo".to_owned()
}

struct Diagnose;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct DiagnoseParams {
    now: Timestamp,
    /// The workload diagnosed, when the input has several.
    #[serde(default)]
    service: Option<String>,
    #[serde(default = "demo")]
    namespace: String,
    #[serde(default = "readable")]
    services_readable: bool,
}

impl FixtureOp for Diagnose {
    const NAME: &'static str = "gaps.diagnose";
    type Params = DiagnoseParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let read = read(input, &params.namespace, Some(params.now))?;
        let project = project(&read.snapshot);
        let service = match &params.service {
            Some(name) => project.services.iter().find(|s| &s.workload.name == name),
            None if project.services.len() == 1 => project.services.first(),
            None => return Err("several workloads and no service named".to_owned()),
        };
        let service = service.ok_or_else(|| format!("no workload {:?}", params.service))?;
        let namespace = Namespace::new(&read.snapshot, &read.events, &read.http_routes)
            .services_readable(params.services_readable);
        to_value(namespace.diagnose(service)).map_err(|e| e.to_string())
    }
}

/// A list that may be absent (none), `null` (could not be listed) or given.
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
#[serde(deny_unknown_fields)]
struct GatewayParam {
    namespace: String,
    name: String,
}

struct Rules;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RulesParams {
    /// The ids of the rules to run.
    rules: Vec<String>,
    #[serde(default)]
    retired_ingress_classes: Vec<String>,
    #[serde(default)]
    gateways: Listed<Vec<GatewayParam>>,
    #[serde(default = "readable")]
    services_readable: bool,
}

impl FixtureOp for Rules {
    const NAME: &'static str = "gaps.rules";
    type Params = RulesParams;

    fn run(input: &[Value], params: Self::Params) -> Result<Value, String> {
        let read = read(input, "demo", None)?;
        let project = project(&read.snapshot);
        let mut rules: Vec<&dyn Rule> = Vec::new();
        for rule in BUILTIN.iter().copied() {
            if params.rules.iter().any(|id| id == rule.id()) {
                rules.push(rule);
            }
        }
        if rules.len() != params.rules.len() {
            return Err(format!("unknown rule among {:?}", params.rules));
        }
        let gateways = match params.gateways {
            Listed::Absent => Some(Vec::new()),
            Listed::Unreadable => None,
            Listed::Items(list) => {
                let mut refs = Vec::new();
                for gateway in list {
                    refs.push(GatewayRef {
                        namespace: gateway.namespace,
                        name: gateway.name,
                    });
                }
                Some(refs)
            }
        };
        let cluster = ClusterTraits {
            retired_ingress_classes: params.retired_ingress_classes.into_iter().collect(),
            gateways,
        };
        let mut routes: Vec<HttpRoute> = Vec::new();
        for route in &read.http_routes {
            routes.push(from_value(route.clone()).map_err(|e| e.to_string())?);
        }
        let gaps = evaluate(
            &rules,
            &Input {
                project: &project,
                snapshot: &read.snapshot,
                http_routes: &routes,
                services_readable: params.services_readable,
                cluster: &cluster,
            },
        );
        to_value(gaps).map_err(|e| e.to_string())
    }
}
