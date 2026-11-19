//! What differs between an Ingress and an `HTTPRoute` when a plan adds, renames or removes a
//! hostname: where the hostnames are, and the patch that changes them.
//!
//! An Ingress lists a hostname in each of its `rules` and in its `tls` entries. An `HTTPRoute`
//! lists them in `hostnames`, and its certificates are on the Gateway's listener.

use std::collections::BTreeSet;

use k8s_openapi::api::networking::v1::Ingress;
use serde_json::{Value, json};
use zeabur_model::HttpRoute;

use crate::change::{RouteKind, Target};
use crate::checks::ResourceRef;
use crate::json::{remove, replace, test};
use crate::plan::{http_route_target, ingress_target};

impl RouteKind {
    /// The kind as Kubernetes names it.
    pub(crate) const fn kind_name(self) -> &'static str {
        match self {
            Self::Ingress => "Ingress",
            Self::HttpRoute => "HTTPRoute",
        }
    }

    /// The object named `name`, as a plan addresses it.
    pub(crate) fn target(self, name: &str) -> Target {
        match self {
            Self::Ingress => ingress_target(name),
            Self::HttpRoute => http_route_target(name),
        }
    }

    pub(crate) fn resource(self) -> ResourceRef {
        let (group, resource) = match self {
            Self::Ingress => ("networking.k8s.io", "ingresses"),
            Self::HttpRoute => ("gateway.networking.k8s.io", "httproutes"),
        };
        ResourceRef {
            group: group.to_owned(),
            version: "v1".to_owned(),
            resource: resource.to_owned(),
        }
    }

    /// The `spec` field that lists the hostnames.
    pub(crate) const fn hosts_field(self) -> &'static str {
        match self {
            Self::Ingress => "rules",
            Self::HttpRoute => "hostnames",
        }
    }

    /// The hostnames of `object`, in order.
    pub(crate) fn hosts(self, object: &Value) -> Vec<String> {
        match self {
            Self::Ingress => read_ingress(object)
                .spec
                .into_iter()
                .flat_map(|s| s.rules.into_iter().flatten())
                .filter_map(|r| r.host)
                .collect(),
            Self::HttpRoute => read_http_route(object).spec.hostnames,
        }
    }

    /// The names of the Services `object` sends to.
    pub(crate) fn services(self, object: &Value) -> BTreeSet<String> {
        match self {
            Self::Ingress => read_ingress(object)
                .spec
                .into_iter()
                .flat_map(|s| s.rules.into_iter().flatten())
                .flat_map(|r| r.http.into_iter().flat_map(|h| h.paths))
                .filter_map(|p| p.backend.service.map(|s| s.name))
                .collect(),
            Self::HttpRoute => read_http_route(object)
                .spec
                .rules
                .into_iter()
                .flat_map(|r| r.backend_refs)
                .map(|b| b.name)
                .filter(|name| !name.is_empty())
                .collect(),
        }
    }

    /// Whether `hostname` is all that `object` routes: an Ingress whose every rule is for it and
    /// that has no default backend, or an `HTTPRoute` with only this hostname. Without it the
    /// Ingress would be invalid and the `HTTPRoute` would answer every host.
    pub(crate) fn is_last_host(self, object: &Value, hostname: &str) -> bool {
        match self {
            Self::Ingress => {
                let spec = read_ingress(object).spec.unwrap_or_default();
                let rules = spec.rules.unwrap_or_default();
                !rules.is_empty()
                    && rules.iter().all(|r| r.host.as_deref() == Some(hostname))
                    && spec.default_backend.is_none()
            }
            Self::HttpRoute => {
                matches!(read_http_route(object).spec.hostnames.as_slice(), [only] if only == hostname)
            }
        }
    }

    /// The patch that renames `from` to `to` in `object`, and whether a `tls` entry changed.
    pub(crate) fn rename_host(self, object: &Value, from: &str, to: &str) -> (Vec<Value>, bool) {
        let mut operations = Vec::new();
        let mut tls_changed = false;
        let field = match self {
            Self::Ingress => "/spec/rules",
            Self::HttpRoute => "/spec/hostnames",
        };
        let hosts: Vec<Option<String>> = match self {
            Self::Ingress => read_ingress(object)
                .spec
                .into_iter()
                .flat_map(|s| s.rules.into_iter().flatten())
                .map(|r| r.host)
                .collect(),
            Self::HttpRoute => read_http_route(object)
                .spec
                .hostnames
                .into_iter()
                .map(Some)
                .collect(),
        };
        operations.extend(
            hosts
                .iter()
                .enumerate()
                .filter(|(_, host)| host.as_deref() == Some(from))
                .flat_map(|(at, _)| {
                    let path = match self {
                        Self::Ingress => format!("{field}/{at}/host"),
                        Self::HttpRoute => format!("{field}/{at}"),
                    };
                    [test(&path, json!(from)), replace(&path, json!(to))]
                }),
        );
        if self == Self::Ingress {
            let tls = tls_hosts(object, from);
            tls_changed = !tls.is_empty();
            operations.extend(
                tls.into_iter()
                    .map(|(at, h, _)| replace(&format!("/spec/tls/{at}/hosts/{h}"), json!(to))),
            );
        }
        (operations, tls_changed)
    }

    /// The patch that removes `hostname` from `object`; empty when it does not route it.
    pub(crate) fn remove_host(self, object: &Value, hostname: &str) -> Vec<Value> {
        let mut operations = Vec::new();
        match self {
            Self::Ingress => {
                let rules = read_ingress(object)
                    .spec
                    .unwrap_or_default()
                    .rules
                    .unwrap_or_default();
                operations.extend(
                    rules
                        .iter()
                        .enumerate()
                        .rev()
                        .filter(|(_, rule)| rule.host.as_deref() == Some(hostname))
                        .flat_map(|(at, _)| {
                            [
                                test(&format!("/spec/rules/{at}/host"), json!(hostname)),
                                remove(&format!("/spec/rules/{at}")),
                            ]
                        }),
                );
                if operations.is_empty() {
                    return operations;
                }
                operations.extend(tls_hosts(object, hostname).into_iter().rev().map(
                    |(at, h, alone)| {
                        remove(&if alone {
                            format!("/spec/tls/{at}")
                        } else {
                            format!("/spec/tls/{at}/hosts/{h}")
                        })
                    },
                ));
            }
            Self::HttpRoute => {
                let hostnames = read_http_route(object).spec.hostnames;
                if let Some(at) = hostnames.iter().position(|v| v == hostname) {
                    operations.push(test(&format!("/spec/hostnames/{at}"), json!(hostname)));
                    operations.push(remove(&format!("/spec/hostnames/{at}")));
                }
            }
        }
        operations
    }
}

/// Each `tls` entry of Ingress `object` that lists `hostname`: the entry's index, the host's index
/// in it, and whether it is the entry's only host.
fn tls_hosts(object: &Value, hostname: &str) -> Vec<(usize, usize, bool)> {
    let tls = read_ingress(object).spec.unwrap_or_default().tls;
    tls.iter()
        .flatten()
        .enumerate()
        .filter_map(|(at, tls)| {
            let hosts = tls.hosts.as_deref().unwrap_or_default();
            hosts
                .iter()
                .position(|v| v == hostname)
                .map(|h| (at, h, hosts.len() == 1))
        })
        .collect()
}

pub(crate) fn read_ingress(object: &Value) -> Ingress {
    serde_json::from_value(object.clone()).unwrap_or_default()
}

pub(crate) fn read_http_route(object: &Value) -> HttpRoute {
    serde_json::from_value(object.clone()).unwrap_or_default()
}
