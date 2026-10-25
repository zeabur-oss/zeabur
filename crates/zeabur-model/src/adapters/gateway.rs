//! Which Gateway API `Gateway` listeners accept an `HTTPRoute` from a namespace [gw-listener].
//!
//! A listener accepts a route when its `allowedRoutes.namespaces` admits the route's namespace
//! (`Same` by default, `All`, or a `Selector` of namespace labels) and its `allowedRoutes.kinds`
//! lists `HTTPRoute` or is empty. An `HTTPRoute` hostname that does not match the listener's
//! hostname is ignored [gw-hostnames].
//!
//! [gw-listener]: https://gateway-api.sigs.k8s.io/reference/spec/#allowedroutes
//! [gw-hostnames]: https://gateway-api.sigs.k8s.io/reference/spec/#httproutespec

use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use serde_json::Value;

use crate::adapters::selector;

const GATEWAY_GROUP: &str = "gateway.networking.k8s.io";

/// A Gateway with the listeners that accept an `HTTPRoute` from one namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachable {
    /// The Gateway's namespace.
    pub namespace: String,
    /// The Gateway's name.
    pub name: String,
    /// The listeners that accept the route, in the Gateway's order.
    pub listeners: Vec<Listener>,
}

/// A Gateway listener: its name (a parent's `sectionName`) and its hostname.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    /// The listener's name.
    pub name: String,
    /// The listener's hostname; `None` accepts every hostname.
    pub hostname: Option<String>,
}

impl Attachable {
    /// Whether listener `section` (any listener when `None`) serves `hostname`.
    #[must_use]
    pub fn serves(&self, section: Option<&str>, hostname: &str) -> bool {
        self.listeners
            .iter()
            .filter(|l| section.is_none_or(|s| s == l.name))
            .any(|l| {
                l.hostname
                    .as_deref()
                    .is_none_or(|h| intersects(h, hostname))
            })
    }
}

/// The Gateways among `gateways` (as JSON) that accept an `HTTPRoute` from `namespace`, whose
/// labels are `labels`. A Gateway with no such listener is left out.
#[must_use]
pub fn attachable(
    gateways: &[Value],
    namespace: &str,
    labels: &BTreeMap<String, String>,
) -> Vec<Attachable> {
    let mut out: Vec<Attachable> = gateways
        .iter()
        .filter_map(|gateway| {
            let own = gateway.pointer("/metadata/namespace")?.as_str()?;
            let listeners: Vec<Listener> = gateway
                .pointer("/spec/listeners")?
                .as_array()?
                .iter()
                .filter(|l| admits_namespace(l, own, namespace, labels) && admits_kind(l))
                .filter_map(|l| {
                    Some(Listener {
                        name: l.get("name")?.as_str()?.to_owned(),
                        hostname: l.get("hostname").and_then(Value::as_str).map(str::to_owned),
                    })
                })
                .collect();
            (!listeners.is_empty()).then(|| Attachable {
                namespace: own.to_owned(),
                name: gateway
                    .pointer("/metadata/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                listeners,
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
    out
}

fn admits_namespace(
    listener: &Value,
    gateway_namespace: &str,
    namespace: &str,
    labels: &BTreeMap<String, String>,
) -> bool {
    let namespaces = listener.pointer("/allowedRoutes/namespaces");
    match namespaces
        .and_then(|n| n.get("from"))
        .and_then(Value::as_str)
        .unwrap_or("Same")
    {
        "All" => true,
        "Same" => gateway_namespace == namespace,
        "Selector" => namespaces
            .and_then(|n| n.get("selector"))
            .and_then(|s| serde_json::from_value::<LabelSelector>(s.clone()).ok())
            .is_some_and(|s| selector::matches(&s, labels)),
        _ => false,
    }
}

fn admits_kind(listener: &Value) -> bool {
    let kinds = listener
        .pointer("/allowedRoutes/kinds")
        .and_then(Value::as_array);
    match kinds {
        None => true,
        Some(kinds) if kinds.is_empty() => true,
        Some(kinds) => kinds.iter().any(|k| {
            k.get("kind").and_then(Value::as_str) == Some("HTTPRoute")
                && k.get("group")
                    .and_then(Value::as_str)
                    .unwrap_or(GATEWAY_GROUP)
                    == GATEWAY_GROUP
        }),
    }
}

/// Whether two hostnames, each possibly a wildcard such as `*.example.com`, match a common host.
/// A wildcard label matches one or more labels and not the bare suffix.
#[must_use]
pub fn intersects(a: &str, b: &str) -> bool {
    let covers = |wild: &str, other: &str| {
        wild.strip_prefix('*').is_some_and(|suffix| {
            other.ends_with(suffix) && other.len() > suffix.len()
                || other.strip_prefix('*').is_some_and(|o| o.ends_with(suffix))
        })
    };
    a == b || covers(a, b) || covers(b, a)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn listeners_accept_by_namespace_kind_and_hostname() {
        let gateways = [json!({
            "metadata": {"namespace": "gateway", "name": "public"},
            "spec": {"listeners": [
                {"name": "same", "port": 80},
                {"name": "all", "port": 80, "hostname": "*.example.com",
                 "allowedRoutes": {"namespaces": {"from": "All"}}},
                {"name": "tcp", "port": 9000,
                 "allowedRoutes": {"namespaces": {"from": "All"}, "kinds": [{"kind": "TCPRoute"}]}},
                {"name": "projects", "port": 443,
                 "allowedRoutes": {"namespaces": {"from": "Selector",
                   "selector": {"matchLabels": {"zeabur.com/project": "true"}}}}}
            ]}
        })];
        let labels = BTreeMap::from([("zeabur.com/project".to_owned(), "true".to_owned())]);
        let found = attachable(&gateways, "shop", &labels);
        let names: Vec<&str> = found[0].listeners.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["all", "projects"]);
        assert!(
            attachable(&gateways, "shop", &BTreeMap::new())[0].serves(Some("all"), "a.example.com")
        );
        assert!(!found[0].serves(Some("all"), "example.com"));
        assert!(found[0].serves(None, "example.org"));
        let own = attachable(&gateways, "gateway", &BTreeMap::new());
        let names: Vec<&str> = own[0].listeners.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["same", "all"]);
    }

    #[test]
    fn wildcards_match_one_or_more_labels() {
        for (a, b, both) in [
            ("*.example.com", "a.example.com", true),
            ("*.example.com", "a.b.example.com", true),
            ("*.example.com", "example.com", false),
            ("*.example.com", "*.a.example.com", true),
            ("a.example.com", "b.example.com", false),
        ] {
            assert_eq!(intersects(a, b), both, "{a} {b}");
            assert_eq!(intersects(b, a), both, "{b} {a}");
        }
    }
}
