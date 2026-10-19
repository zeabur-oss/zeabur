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

use std::collections::{BTreeMap, BTreeSet};

use engawa_graph::{Snapshot, Workload, endpoint, env, ingress_backends, named_hosts};
use engawa_model::{
    DiagnosisKind, Endpoint, Health, HttpRoute, Message, Origin, Service, WorkloadKind,
};
use serde_json::Value;

use super::{Found, parse, truncate};

/// What sends to what in a namespace, read once for all its services: its Services, the Ingress and
/// `HTTPRoute` backends, and the hosts the workloads' variables name.
#[derive(Debug)]
pub(super) struct Traffic<'a> {
    /// The Services by name as endpoints, the first of each name. A service names its own
    /// ([`Service::endpoint_names`]); they are looked up here.
    endpoints: BTreeMap<&'a str, Endpoint>,
    /// Whether the Services could be listed
    /// ([`Namespace::services_readable`](super::Namespace::services_readable)); when not,
    /// `endpoints` is empty and none is known to be missing.
    services_readable: bool,
    /// By backend name, the Ingresses that send to it and the field that says so, in the order
    /// of the Ingresses and their backends.
    ingresses: BTreeMap<&'a str, Vec<(String, Origin)>>,
    /// The `HTTPRoutes`, as given.
    routes: &'a [Value],
    /// By Service name, the positions in `routes` of those that send to it
    /// ([`HttpRoute::routes_to`]); each is deserialised once. One that is not an `HTTPRoute`
    /// sends to none.
    routed: BTreeMap<String, BTreeSet<usize>>,
    /// By workload name, the workloads whose variables name it as a host
    /// ([`engawa_graph::names_host`]) and the variable's field, in the order of the workloads,
    /// their containers and their variables.
    hosts: BTreeMap<String, Vec<(WorkloadKind, &'a str, Origin)>>,
}

impl<'a> Traffic<'a> {
    /// `workloads` are the snapshot's, sorted as [`engawa_graph::workloads`] sorts them.
    pub(super) fn new(
        snapshot: &'a Snapshot,
        workloads: &[Workload<'a>],
        http_routes: &'a [Value],
    ) -> Self {
        let mut ingresses: BTreeMap<&str, Vec<(String, Origin)>> = BTreeMap::new();
        for ingress in &snapshot.ingresses {
            let ingress_name = ingress.metadata.name.as_deref().unwrap_or_default();
            for (backend, path) in ingress_backends(ingress) {
                ingresses.entry(backend).or_default().push((
                    ingress_name.to_owned(),
                    Origin::field("Ingress", ingress_name, path),
                ));
            }
        }
        let mut routed: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (position, route) in http_routes.iter().enumerate() {
            let Ok(route) = serde_json::from_value::<HttpRoute>(route.clone()) else {
                continue;
            };
            for backend in route.local_services() {
                routed
                    .entry(backend.to_owned())
                    .or_default()
                    .insert(position);
            }
        }
        // Only a workload's name is ever looked up, so each value is read once against them.
        let names: BTreeMap<&str, &str> = workloads.iter().map(|w| (w.name(), w.name())).collect();
        let mut hosts: BTreeMap<String, Vec<(WorkloadKind, &str, Origin)>> = BTreeMap::new();
        for &other in workloads {
            let containers = other.pod_spec().into_iter().flat_map(|spec| {
                spec.init_containers
                    .iter()
                    .flatten()
                    .chain(&spec.containers)
            });
            for container in containers {
                for variable in env(other, &container.name).into_iter().flatten() {
                    let Some(value) = variable.value.as_deref() else {
                        continue;
                    };
                    for host in named_hosts(value, &snapshot.namespace, &names) {
                        hosts.entry(host.to_owned()).or_default().push((
                            other.kind(),
                            other.name(),
                            variable.origin.clone(),
                        ));
                    }
                }
            }
        }
        let mut endpoints = BTreeMap::new();
        for object in &snapshot.services {
            if let Some(name) = object.metadata.name.as_deref() {
                endpoints.entry(name).or_insert_with(|| endpoint(object));
            }
        }
        Self {
            endpoints,
            services_readable: true,
            ingresses,
            routes: http_routes,
            routed,
            hosts,
        }
    }

    /// Forgets the Services read: they could not be listed, so none is known to be missing.
    pub(super) const fn services_unreadable(&mut self) {
        self.services_readable = false;
    }

    /// What sends to `workload` by its name while no Service has that name: Ingress and
    /// `HTTPRoute` backends, then other workloads' variables naming it as a host. Each with the
    /// name of what sends and the field that says so. None when the Services could not be
    /// listed.
    fn senders(&self, workload: Workload<'_>) -> Vec<(String, Origin)> {
        let name = workload.name();
        if !self.services_readable || self.endpoints.contains_key(name) {
            return Vec::new();
        }
        let mut out = self.ingresses.get(name).cloned().unwrap_or_default();
        for route in self.routes_to([name]) {
            let route_name = route
                .pointer("/metadata/name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            out.push((
                route_name.to_owned(),
                Origin::field("HTTPRoute", route_name, "spec.rules".to_owned()),
            ));
        }
        for (kind, other, origin) in self.hosts.get(name).into_iter().flatten() {
            if !(*kind == workload.kind() && *other == name) {
                out.push(((*other).to_owned(), origin.clone()));
            }
        }
        out
    }

    /// The `HTTPRoutes` that send to one of `names`, each once, in the order given.
    fn routes_to<'n>(&self, names: impl IntoIterator<Item = &'n str>) -> Vec<&'a Value> {
        let positions: BTreeSet<usize> = names
            .into_iter()
            .filter_map(|name| self.routed.get(name))
            .flatten()
            .copied()
            .collect();
        positions
            .into_iter()
            .filter_map(|position| self.routes.get(position))
            .collect()
    }
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
    traffic: &Traffic<'_>,
    workload: Workload<'_>,
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

    let endpoints: Vec<&Endpoint> = service
        .endpoint_names
        .iter()
        .filter_map(|name| traffic.endpoints.get(name.as_str()))
        .collect();
    if endpoints.is_empty() {
        let senders = traffic.senders(workload);
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

    for endpoint in &endpoints {
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
            &[("endpoint", service.endpoint_names.join(", "))],
            endpoints.iter().map(|e| e.origin.clone()).collect(),
            None,
        );
    }

    for route in traffic.routes_to(service.endpoint_names.iter().map(String::as_str)) {
        refused_route(&mut add, route);
    }
}

/// An `HTTPRoute` to the service that a parent did not accept, or whose backends it did not
/// resolve.
fn refused_route(
    add: &mut impl FnMut(&str, &[(&str, String)], Vec<Origin>, Option<k8s_openapi::jiff::Timestamp>),
    route: &Value,
) {
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
