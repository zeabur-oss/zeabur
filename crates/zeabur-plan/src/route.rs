//! What differs between an Ingress and an `HTTPRoute` when a plan adds, renames or removes a
//! hostname: where the hostnames are, and the patch that changes them.
//!
//! An Ingress lists a hostname in each of its `rules` and in its `tls` entries. An `HTTPRoute`
//! lists them in `hostnames`, and its certificates are on the Gateway's listener.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::change::{RouteKind, Target};
use crate::checks::ResourceRef;
use crate::json::{remove, replace, test};
use crate::plan::{array, http_route_target, ingress_target};

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
        let values = match self {
            Self::Ingress => array(object, "/spec/rules")
                .iter()
                .filter_map(|r| r.get("host").cloned())
                .collect(),
            Self::HttpRoute => array(object, "/spec/hostnames"),
        };
        values
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    }

    /// The names of the Services `object` sends to.
    pub(crate) fn services(self, object: &Value) -> BTreeSet<String> {
        let name = |v: &Value, at: &str| v.pointer(at).and_then(Value::as_str).map(str::to_owned);
        match self {
            Self::Ingress => array(object, "/spec/rules")
                .iter()
                .flat_map(|r| array(r, "/http/paths"))
                .filter_map(|p| name(&p, "/backend/service/name"))
                .collect(),
            Self::HttpRoute => array(object, "/spec/rules")
                .iter()
                .flat_map(|r| array(r, "/backendRefs"))
                .filter_map(|b| name(&b, "/name"))
                .collect(),
        }
    }

    /// Whether `hostname` is all that `object` routes: an Ingress whose every rule is for it and
    /// that has no default backend, or an `HTTPRoute` with only this hostname. Without it the
    /// Ingress would be invalid and the `HTTPRoute` would answer every host.
    pub(crate) fn is_last_host(self, object: &Value, hostname: &str) -> bool {
        let host = Some(hostname);
        match self {
            Self::Ingress => {
                let rules = array(object, "/spec/rules");
                !rules.is_empty()
                    && rules
                        .iter()
                        .all(|r| r.get("host").and_then(Value::as_str) == host)
                    && object.pointer("/spec/defaultBackend").is_none()
            }
            Self::HttpRoute => {
                matches!(array(object, "/spec/hostnames").as_slice(), [only] if only.as_str() == host)
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
        for (at, entry) in array(object, field).iter().enumerate() {
            let (host, path) = match self {
                Self::Ingress => (entry.get("host"), format!("{field}/{at}/host")),
                Self::HttpRoute => (Some(entry), format!("{field}/{at}")),
            };
            if host.and_then(Value::as_str) == Some(from) {
                operations.push(test(&path, json!(from)));
                operations.push(replace(&path, json!(to)));
            }
        }
        if self == Self::Ingress {
            for (at, h, _) in tls_hosts(object, from) {
                operations.push(replace(&format!("/spec/tls/{at}/hosts/{h}"), json!(to)));
                tls_changed = true;
            }
        }
        (operations, tls_changed)
    }

    /// The patch that removes `hostname` from `object`; empty when it does not route it.
    pub(crate) fn remove_host(self, object: &Value, hostname: &str) -> Vec<Value> {
        let mut operations = Vec::new();
        match self {
            Self::Ingress => {
                for (at, rule) in array(object, "/spec/rules").iter().enumerate().rev() {
                    if rule.get("host").and_then(Value::as_str) == Some(hostname) {
                        operations.push(test(&format!("/spec/rules/{at}/host"), json!(hostname)));
                        operations.push(remove(&format!("/spec/rules/{at}")));
                    }
                }
                if operations.is_empty() {
                    return operations;
                }
                for (at, h, alone) in tls_hosts(object, hostname).into_iter().rev() {
                    operations.push(remove(&if alone {
                        format!("/spec/tls/{at}")
                    } else {
                        format!("/spec/tls/{at}/hosts/{h}")
                    }));
                }
            }
            Self::HttpRoute => {
                let hostnames = array(object, "/spec/hostnames");
                if let Some(at) = hostnames.iter().position(|v| v.as_str() == Some(hostname)) {
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
    array(object, "/spec/tls")
        .iter()
        .enumerate()
        .filter_map(|(at, tls)| {
            let hosts = array(tls, "/hosts");
            hosts
                .iter()
                .position(|v| v.as_str() == Some(hostname))
                .map(|h| (at, h, hosts.len() == 1))
        })
        .collect()
}
