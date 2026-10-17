//! Whether traffic can reach a long-running service, read from its Services, its pods and the
//! `HTTPRoutes` to it.
//!
//! - **No Service.** Only a Service gives a workload's pods an address other pods and an Ingress or
//!   `HTTPRoute` can send to [k8s-service]. A declared container port is no sign one is wanted: the
//!   list is "primarily informational" [k8s-container-ports], and workers, controllers and
//!   exporters declare metrics and health ports that a scraper reaches on the pod's own IP. So it
//!   is said only when something sends to the workload by its name and no Service has that name: an
//!   Ingress or `HTTPRoute` backend, or another workload's variable naming it as a host, as
//!   [`engawa_graph::connections`] reads one. The name is the workload's, since a chart names a
//!   workload and its Service alike (`helm create`'s `<chart>.fullname`) [helm-create].
//! - **`targetPort`.** A Service sends to its `targetPort` (its `port` when unset). A name must be
//!   the name of a container port, or the Service has nowhere to send; a number reaches any port
//!   the program listens on, declared or not, so a number is only named when the containers declare
//!   ports and none of them is it [k8s-service-ports].
//! - **No ready pod.** A Service sends only to ready pods; with none ready it has no endpoint
//!   [k8s-service].
//! - **Route not accepted.** A Gateway accepts an `HTTPRoute`, and resolves its backends, per
//!   parent; `Accepted` or `ResolvedRefs` set to `False` means its requests do not reach the
//!   Service [gateway-route-status].
//!
//! [k8s-service]: https://kubernetes.io/docs/concepts/services-networking/service/
//! [k8s-container-ports]: https://kubernetes.io/docs/reference/kubernetes-api/workload-resources/pod-v1/#ports
//! [helm-create]: https://helm.sh/docs/helm/helm_create/
//! [k8s-service-ports]: https://kubernetes.io/docs/concepts/services-networking/service/#field-spec-ports
//! [gateway-route-status]: https://gateway-api.sigs.k8s.io/reference/spec/#routeconditiontype

use engawa_graph::{Snapshot, Workload, env, ingress_backends, names_host, workloads};
use engawa_model::{
    DiagnosisKind, Endpoint, Health, HttpRoute, Message, Origin, Service, WorkloadKind,
};
use serde_json::Value;

use super::{Found, parse, truncate};

/// What sends to `workload` by its name while no Service has that name: Ingress and `HTTPRoute`
/// backends, then other workloads' variables naming it as a host. Each with the name of what
/// sends and the field that says so.
fn senders(
    snapshot: &Snapshot,
    workload: Workload<'_>,
    http_routes: &[Value],
) -> Vec<(String, Origin)> {
    let name = workload.name();
    if snapshot
        .services
        .iter()
        .any(|s| s.metadata.name.as_deref() == Some(name))
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    for ingress in &snapshot.ingresses {
        let ingress_name = ingress.metadata.name.as_deref().unwrap_or_default();
        for (backend, path) in ingress_backends(ingress) {
            if backend == name {
                out.push((
                    ingress_name.to_owned(),
                    Origin::field("Ingress", ingress_name, path),
                ));
            }
        }
    }
    for route in http_routes {
        let to_it = serde_json::from_value::<HttpRoute>(route.clone())
            .is_ok_and(|route| route.routes_to(|backend| backend == name));
        if to_it {
            let route_name = route
                .pointer("/metadata/name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            out.push((
                route_name.to_owned(),
                Origin::field("HTTPRoute", route_name, "spec.rules".to_owned()),
            ));
        }
    }
    for other in workloads(snapshot) {
        if other.kind() == workload.kind() && other.name() == name {
            continue;
        }
        let containers = other.pod_spec().into_iter().flat_map(|spec| {
            spec.init_containers
                .iter()
                .flatten()
                .chain(&spec.containers)
        });
        for container in containers {
            for variable in env(other, &container.name).into_iter().flatten() {
                if variable
                    .value
                    .as_deref()
                    .is_some_and(|value| names_host(value, &snapshot.namespace, name))
                {
                    out.push((other.name().to_owned(), variable.origin));
                }
            }
        }
    }
    out
}

/// The ports a workload's containers declare: name and number, with the container.
fn declared(workload: Workload<'_>) -> Vec<(String, Option<String>, i32)> {
    workload
        .pod_spec()
        .into_iter()
        .flat_map(|spec| &spec.containers)
        .flat_map(|container| {
            container.ports.iter().flatten().map(|port| {
                (
                    container.name.clone(),
                    port.name.clone(),
                    port.container_port,
                )
            })
        })
        .collect()
}

pub(super) fn unreachable(
    found: &mut Found,
    service: &Service,
    snapshot: &Snapshot,
    workload: Workload<'_>,
    http_routes: &[Value],
) {
    let kind = workload.kind();
    if matches!(kind, WorkloadKind::Job | WorkloadKind::CronJob) {
        return;
    }
    let name = workload.name();
    let ports = declared(workload);
    let mut add = |cause: &str, params: &[(&str, String)], evidence: Vec<Origin>, when| {
        let sentence = |part: &str| {
            params.iter().fold(
                Message::new(&format!("diagnosis.unreachable.{part}.{cause}"))
                    .with("service", name),
                |message, (key, value)| message.with(key, value.clone()),
            )
        };
        found.add(
            DiagnosisKind::Unreachable,
            "",
            "",
            (sentence("cause"), sentence("why"), sentence("fix")),
            evidence,
            when,
        );
    };

    if service.endpoints.is_empty() {
        let senders = senders(snapshot, workload, http_routes);
        if let Some((from, _)) = senders.first() {
            add(
                "noService",
                &[("from", from.clone())],
                senders.into_iter().map(|(_, origin)| origin).collect(),
                None,
            );
        }
        return;
    }

    for endpoint in &service.endpoints {
        for port in &endpoint.ports {
            let target = port
                .target_port
                .clone()
                .unwrap_or_else(|| port.port.to_string());
            let matches = target.parse::<i32>().map_or_else(
                |_| {
                    ports
                        .iter()
                        .any(|(_, port_name, _)| port_name.as_deref() == Some(target.as_str()))
                },
                |number| ports.is_empty() || ports.iter().any(|(_, _, p)| *p == number),
            );
            if !matches {
                let declared: Vec<String> = ports
                    .iter()
                    .map(|(_, port_name, number)| {
                        port_name
                            .as_ref()
                            .map_or_else(|| number.to_string(), |n| format!("{n} ({number})"))
                    })
                    .collect();
                add(
                    "targetPort",
                    &[
                        ("endpoint", endpoint.name.clone()),
                        ("targetPort", target),
                        ("ports", declared.join(", ")),
                    ],
                    vec![port.origin.clone()],
                    None,
                );
            }
        }
    }

    let wanted = service.workload.replicas.is_none_or(|r| r.desired > 0);
    if wanted
        && !service.pods.is_empty()
        && !service.pods.iter().any(|pod| pod.ready)
        && service.status.health != Health::Progress
    {
        add(
            "noReadyEndpoints",
            &[("endpoint", names(&service.endpoints))],
            service.endpoints.iter().map(|e| e.origin.clone()).collect(),
            None,
        );
    }

    for route in http_routes {
        refused_route(&mut add, route, &service.endpoints);
    }
}

fn names(endpoints: &[Endpoint]) -> String {
    endpoints
        .iter()
        .map(|e| e.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether `route` sends to one of `endpoints` ([`HttpRoute::local_services`]).
fn routes_to(route: &Value, endpoints: &[Endpoint]) -> bool {
    serde_json::from_value::<HttpRoute>(route.clone())
        .is_ok_and(|route| route.routes_to(|name| endpoints.iter().any(|e| e.name == name)))
}

/// An `HTTPRoute` to the service that a parent did not accept, or whose backends it did not
/// resolve.
fn refused_route(
    add: &mut impl FnMut(&str, &[(&str, String)], Vec<Origin>, Option<k8s_openapi::jiff::Timestamp>),
    route: &Value,
    endpoints: &[Endpoint],
) {
    if !routes_to(route, endpoints) {
        return;
    }
    let route_name = route
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let refused = route
        .pointer("/status/parents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|parent| {
            parent
                .get("conditions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find(|condition| {
            let field = |key: &str| condition.get(key).and_then(Value::as_str);
            matches!(field("type"), Some("Accepted" | "ResolvedRefs"))
                && field("status") == Some("False")
        });
    if let Some(condition) = refused {
        let field = |key: &str| {
            condition
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        // The controller's message names what it could not resolve ("service port 8081 not
        // found"); some leave it empty.
        let message = field("message");
        let origin = Origin::field("HTTPRoute", route_name, "status.parents".to_owned());
        add(
            "routeNotAccepted",
            &[
                ("route", route_name.to_owned()),
                ("condition", field("type")),
                ("reason", field("reason")),
            ],
            vec![if message.is_empty() {
                origin
            } else {
                origin.with_value(truncate(&message))
            }],
            parse(condition.get("lastTransitionTime").and_then(Value::as_str)),
        );
    }
}
