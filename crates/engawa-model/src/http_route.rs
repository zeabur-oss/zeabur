//! The fields of a Gateway API `HTTPRoute` that Engawa reads.
//!
//! k8s-openapi has no Gateway API types and the `gateway-api` crate builds on another kube and
//! k8s-openapi release, so this view declares only what is read and ignores everything else.
//! Routes are read as JSON values; deserialise one into this view to ask where it sends.

use serde::Deserialize;

/// An `HTTPRoute`, as far as Engawa reads one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct HttpRoute {
    /// Its name and namespace.
    pub metadata: HttpRouteMetadata,
    /// Its rules.
    pub spec: HttpRouteSpec,
}

/// The part of an `HTTPRoute`'s metadata that is read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct HttpRouteMetadata {
    /// The route's name.
    pub name: Option<String>,
    /// The route's namespace.
    pub namespace: Option<String>,
}

/// The part of an `HTTPRoute`'s spec that is read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct HttpRouteSpec {
    /// The hostnames it answers; none is every host.
    pub hostnames: Vec<String>,
    /// Its rules, in order.
    pub rules: Vec<HttpRouteRule>,
}

/// One rule of an `HTTPRoute`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HttpRouteRule {
    /// Where the rule sends requests.
    pub backend_refs: Vec<BackendRef>,
}

/// A rule's backend (Gateway API `HTTPBackendRef`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct BackendRef {
    /// The API group; absent or empty is the core group.
    pub group: Option<String>,
    /// The kind; absent is `Service`.
    pub kind: Option<String>,
    /// The object's name.
    pub name: String,
    /// The object's namespace; absent is the route's own.
    pub namespace: Option<String>,
}

impl HttpRoute {
    /// The names of the Kubernetes Services the route sends to in its own namespace: backends of
    /// kind `Service` (the default) in the core group (the default), whose namespace is absent or
    /// the route's own. A backend in another namespace is another Service, however it is named
    /// (<https://gateway-api.sigs.k8s.io/reference/spec/#backendref>).
    pub fn local_services(&self) -> impl Iterator<Item = &str> {
        let namespace = self.metadata.namespace.as_deref();
        self.spec
            .rules
            .iter()
            .flat_map(|rule| &rule.backend_refs)
            .filter(move |backend| {
                backend.kind.as_deref().is_none_or(|kind| kind == "Service")
                    && backend.group.as_deref().is_none_or(str::is_empty)
                    && backend
                        .namespace
                        .as_deref()
                        .is_none_or(|n| Some(n) == namespace)
            })
            .map(|backend| backend.name.as_str())
    }

    /// The route's name; empty when it has none (kube-apiserver never returns such an object).
    #[must_use]
    pub fn name(&self) -> &str {
        self.metadata.name.as_deref().unwrap_or_default()
    }

    /// Whether the route sends to a Service of its own namespace that `is_one` accepts by name.
    pub fn routes_to(&self, is_one: impl FnMut(&str) -> bool) -> bool {
        self.local_services().any(is_one)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A backend's name, then its namespace, group and kind when set.
    type Backend<'a> = (&'a str, Option<&'a str>, Option<&'a str>, Option<&'a str>);

    fn route(backends: &[Backend<'_>]) -> HttpRoute {
        HttpRoute {
            metadata: HttpRouteMetadata {
                name: Some("web".to_owned()),
                namespace: Some("shop".to_owned()),
            },
            spec: HttpRouteSpec {
                hostnames: Vec::new(),
                rules: vec![HttpRouteRule {
                    backend_refs: backends
                        .iter()
                        .map(|(name, namespace, group, kind)| BackendRef {
                            group: group.map(str::to_owned),
                            kind: kind.map(str::to_owned),
                            name: (*name).to_owned(),
                            namespace: namespace.map(str::to_owned),
                        })
                        .collect(),
                }],
            },
        }
    }

    #[test]
    fn only_core_services_of_the_routes_namespace_count() {
        let route = route(&[
            ("plain", None, None, None),
            ("same-namespace", Some("shop"), Some(""), Some("Service")),
            ("other-namespace", Some("billing"), None, None),
            ("other-group", None, Some("example.com"), Some("Service")),
            ("other-kind", None, None, Some("ServiceImport")),
        ]);
        assert_eq!(
            route.local_services().collect::<Vec<_>>(),
            ["plain", "same-namespace"]
        );
        assert!(route.routes_to(|name| name == "plain"));
        assert!(!route.routes_to(|name| name == "other-namespace"));
    }
}
