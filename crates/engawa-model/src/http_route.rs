//! The fields of a Gateway API `HTTPRoute` that engawa reads.
//!
//! k8s-openapi has no Gateway API types and the `gateway-api` crate builds on another kube and
//! k8s-openapi release, so this view declares only what is read and ignores everything else.
//! Routes are read as JSON values; deserialise one into this view to ask where it sends, which
//! Gateways it asks to attach to and which of them accepted it. Field defaults follow the
//! Gateway API reference [gateway-spec].
//!
//! [gateway-spec]: https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/

use serde::Deserialize;

/// An `HTTPRoute`, as far as engawa reads one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct HttpRoute {
    /// Its name and namespace.
    pub metadata: HttpRouteMetadata,
    /// Its parents, hostnames and rules.
    pub spec: HttpRouteSpec,
    /// What the controllers of its parents wrote about it.
    pub status: RouteStatus,
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
#[serde(default, rename_all = "camelCase")]
pub struct HttpRouteSpec {
    /// The objects, usually Gateways, the route asks to attach to.
    pub parent_refs: Vec<ParentReference>,
    /// The default Gateways it asks to attach to (experimental): `All`, or `None` and unset for
    /// none (<https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#commonroutespec>).
    pub use_default_gateways: Option<String>,
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
    /// Its share of the rule's requests; absent is 1, and 0 forwards none
    /// (<https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#httpbackendref>).
    pub weight: Option<i32>,
}

impl BackendRef {
    /// Whether requests are forwarded to it: "If weight is set to 0, no traffic should be
    /// forwarded for this entry. If unspecified, weight defaults to 1."
    #[must_use]
    pub fn forwards(&self) -> bool {
        self.weight != Some(0)
    }
}

/// A parent the route asks to attach to (Gateway API `ParentReference`,
/// <https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#parentreference>).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ParentReference {
    /// The API group; absent is `gateway.networking.k8s.io`, empty is the core group.
    pub group: Option<String>,
    /// The kind; absent is `Gateway`.
    pub kind: Option<String>,
    /// The object's namespace; absent is the route's own.
    pub namespace: Option<String>,
    /// The object's name.
    pub name: String,
    /// A listener of the Gateway (a port name of a Service); absent is the whole object.
    pub section_name: Option<String>,
    /// The port the route targets.
    pub port: Option<i32>,
}

/// The Gateway API group.
pub const GATEWAY_GROUP: &str = "gateway.networking.k8s.io";

impl ParentReference {
    /// The namespace and name of the Gateway this names, `route_namespace` standing in for an
    /// absent namespace; `None` when the parent is another kind, such as a mesh's Service.
    #[must_use]
    pub fn gateway<'a>(&'a self, route_namespace: &'a str) -> Option<(&'a str, &'a str)> {
        let group = self.group.as_deref().unwrap_or(GATEWAY_GROUP);
        let kind = self.kind.as_deref().unwrap_or("Gateway");
        (group == GATEWAY_GROUP && kind == "Gateway").then(|| {
            (
                self.namespace.as_deref().unwrap_or(route_namespace),
                self.name.as_str(),
            )
        })
    }

    /// Whether `other`, a status entry's `parentRef`, is this parent of the spec: the same
    /// group, kind, namespace and name once the defaults are filled in, and exactly the same
    /// `sectionName` and `port`, set or not. A status entry's `parentRef` "corresponds with a
    /// `ParentRef` in the spec" [gateway-routeparentstatus], and an unset `sectionName` is not a
    /// wildcard: "When unspecified (empty string), this will reference the entire resource"
    /// [gateway-parentreference]. So an entry left from `sectionName: https` is not the entry
    /// for the whole Gateway the spec now names.
    ///
    /// [gateway-routeparentstatus]: https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#routeparentstatus
    /// [gateway-parentreference]: https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#parentreference
    #[must_use]
    pub fn names(&self, other: &Self, route_namespace: &str) -> bool {
        let same = |a: &Option<String>, b: &Option<String>, default: &str| {
            a.as_deref().unwrap_or(default) == b.as_deref().unwrap_or(default)
        };
        same(&self.group, &other.group, GATEWAY_GROUP)
            && same(&self.kind, &other.kind, "Gateway")
            && same(&self.namespace, &other.namespace, route_namespace)
            && self.name == other.name
            && same(&self.section_name, &other.section_name, "")
            && self.port == other.port
    }
}

/// A route's status (Gateway API `RouteStatus`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RouteStatus {
    /// One entry per parent a controller manages and could resolve; empty means the route is
    /// attached to none.
    pub parents: Vec<RouteParentStatus>,
}

/// The route's status with respect to one parent (Gateway API `RouteParentStatus`,
/// <https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#routeparentstatus>).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RouteParentStatus {
    /// The parent, as the spec names it.
    pub parent_ref: ParentReference,
    /// The controller that wrote the entry.
    pub controller_name: String,
    /// Its conditions, `Accepted` among them.
    pub conditions: Vec<RouteCondition>,
}

impl RouteParentStatus {
    /// The parent's `Accepted` condition, when its controller set one.
    #[must_use]
    pub fn accepted(&self) -> Option<&RouteCondition> {
        self.conditions.iter().find(|c| c.type_ == "Accepted")
    }
}

/// A condition of a route's parent status, as far as it is read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RouteCondition {
    /// The condition, such as `Accepted` or `ResolvedRefs`.
    #[serde(rename = "type")]
    pub type_: String,
    /// `True`, `False` or `Unknown`.
    pub status: String,
    /// Why, as one `CamelCase` word.
    pub reason: String,
    /// Why, in words.
    pub message: String,
}

impl HttpRoute {
    /// The names of the Kubernetes Services the route sends to in its own namespace: backends of
    /// kind `Service` (the default) in the core group (the default), whose namespace is absent or
    /// the route's own, and whose weight is not 0 ([`BackendRef::forwards`]). A backend in
    /// another namespace is another Service, however it is named
    /// (<https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#backendref>).
    pub fn local_services(&self) -> impl Iterator<Item = &str> {
        self.local_service_refs().map(|(_, name)| name)
    }

    /// [`Self::local_services`], each with the field path naming its backend
    /// (`spec.rules[0].backendRefs[1]`).
    pub fn local_service_refs(&self) -> impl Iterator<Item = (String, &str)> {
        self.local_backends()
            .filter(|(_, backend)| backend.forwards())
            .map(|(path, backend)| (path, backend.name.as_str()))
    }

    /// The names of every Service of the route's namespace it names as a backend, weight 0
    /// included: one edit to the weight away from receiving its requests.
    pub fn named_local_services(&self) -> impl Iterator<Item = &str> {
        self.local_backends()
            .map(|(_, backend)| backend.name.as_str())
    }

    /// The backends that are Services of the route's namespace, with their field paths.
    fn local_backends(&self) -> impl Iterator<Item = (String, &BackendRef)> {
        let namespace = self.metadata.namespace.as_deref();
        self.spec
            .rules
            .iter()
            .enumerate()
            .flat_map(|(rule, value)| {
                value
                    .backend_refs
                    .iter()
                    .enumerate()
                    .map(move |(index, backend)| (rule, index, backend))
            })
            .filter(move |(_, _, backend)| {
                backend.kind.as_deref().is_none_or(|kind| kind == "Service")
                    && backend.group.as_deref().is_none_or(str::is_empty)
                    && backend
                        .namespace
                        .as_deref()
                        .is_none_or(|n| Some(n) == namespace)
            })
            .map(|(rule, index, backend)| {
                (format!("spec.rules[{rule}].backendRefs[{index}]"), backend)
            })
    }

    /// The Gateways among `spec.parentRefs` ([`ParentReference::gateway`]); a mesh's Service
    /// parent is left out.
    pub fn gateway_parents(&self) -> impl Iterator<Item = &ParentReference> {
        let namespace = self.metadata.namespace.as_deref().unwrap_or_default();
        self.spec
            .parent_refs
            .iter()
            .filter(move |parent| parent.gateway(namespace).is_some())
    }

    /// Whether the route asks for every default Gateway (`useDefaultGateways: All`); "If unset
    /// (the default) or set to None, the Route will not be attached to any default Gateway"
    /// (<https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#commonroutespec>). An
    /// unknown value is taken as none.
    #[must_use]
    pub fn uses_default_gateways(&self) -> bool {
        self.spec.use_default_gateways.as_deref() == Some("All")
    }

    /// The entries of `status.parents` whose parent is a Gateway, whatever the spec names: for a
    /// route attached through [`Self::uses_default_gateways`], the default Gateways that claimed
    /// it. How a claimed default Gateway appears there is not stated in the reference; an entry
    /// naming that Gateway, as for any parent, is assumed.
    pub fn default_gateway_statuses(&self) -> impl Iterator<Item = &RouteParentStatus> {
        let namespace = self.metadata.namespace.as_deref().unwrap_or_default();
        self.status
            .parents
            .iter()
            .filter(move |status| status.parent_ref.gateway(namespace).is_some())
    }

    /// The entries of `status.parents` about a Gateway the spec names now (see
    /// [`ParentReference::names`]). An entry for a parent removed from `spec.parentRefs` is left
    /// out: its controller "should ensure that entries to status populated with their
    /// `ControllerName` are cleaned up when they are no longer necessary", but until it does, the
    /// entry says nothing about where the route attaches
    /// (<https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#routeparentstatus>).
    pub fn gateway_parent_statuses(&self) -> impl Iterator<Item = &RouteParentStatus> {
        let namespace = self.metadata.namespace.as_deref().unwrap_or_default();
        self.status.parents.iter().filter(move |status| {
            self.gateway_parents()
                .any(|parent| parent.names(&status.parent_ref, namespace))
        })
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
                parent_refs: Vec::new(),
                use_default_gateways: None,
                hostnames: Vec::new(),
                rules: vec![HttpRouteRule {
                    backend_refs: backends
                        .iter()
                        .map(|(name, namespace, group, kind)| BackendRef {
                            group: group.map(str::to_owned),
                            kind: kind.map(str::to_owned),
                            name: (*name).to_owned(),
                            namespace: namespace.map(str::to_owned),
                            weight: None,
                        })
                        .collect(),
                }],
            },
            status: RouteStatus::default(),
        }
    }

    #[test]
    fn a_parent_is_a_gateway_of_the_routes_namespace_unless_it_says_otherwise() {
        let parent =
            |group: Option<&str>, kind: Option<&str>, namespace: Option<&str>| ParentReference {
                group: group.map(str::to_owned),
                kind: kind.map(str::to_owned),
                namespace: namespace.map(str::to_owned),
                name: "edge".to_owned(),
                ..ParentReference::default()
            };
        assert_eq!(
            parent(None, None, None).gateway("shop"),
            Some(("shop", "edge"))
        );
        assert_eq!(
            parent(Some(GATEWAY_GROUP), Some("Gateway"), Some("infra")).gateway("shop"),
            Some(("infra", "edge"))
        );
        // The core group's Service is a mesh parent, not a Gateway.
        assert_eq!(
            parent(Some(""), Some("Service"), None).gateway("shop"),
            None
        );
        assert_eq!(
            parent(None, Some("ListenerSet"), None).gateway("shop"),
            None
        );
    }

    #[test]
    fn a_zero_weight_backend_is_named_but_sent_nothing() {
        let mut route = route(&[("live", None, None, None), ("parked", None, None, None)]);
        if let Some(rule) = route.spec.rules.first_mut() {
            for (backend, weight) in rule.backend_refs.iter_mut().zip([None, Some(0)]) {
                backend.weight = weight;
            }
        }
        assert_eq!(route.local_services().collect::<Vec<_>>(), ["live"]);
        assert_eq!(
            route.named_local_services().collect::<Vec<_>>(),
            ["live", "parked"]
        );
        assert!(!route.routes_to(|name| name == "parked"));
    }

    #[test]
    fn only_status_entries_for_a_current_gateway_parent_count() {
        let parent = |name: &str, namespace: Option<&str>, section: Option<&str>| ParentReference {
            name: name.to_owned(),
            namespace: namespace.map(str::to_owned),
            section_name: section.map(str::to_owned),
            ..ParentReference::default()
        };
        let mesh = ParentReference {
            group: Some(String::new()),
            kind: Some("Service".to_owned()),
            ..parent("web", None, None)
        };
        let mut route = route(&[]);
        route.spec.parent_refs = vec![
            parent("edge", None, Some("https")),
            parent("whole", None, None),
            mesh.clone(),
        ];
        route.status.parents = [
            // The same parent with its defaults written out.
            ParentReference {
                group: Some(GATEWAY_GROUP.to_owned()),
                kind: Some("Gateway".to_owned()),
                ..parent("edge", Some("shop"), Some("https"))
            },
            parent("edge", None, Some("http")),
            parent("old", None, None),
            parent("whole", Some("other"), None),
            // Left from when the spec named one listener: not the entry for the whole Gateway.
            parent("whole", None, Some("https")),
            parent("whole", None, None),
            // A mesh's Service parent is no Gateway.
            mesh,
        ]
        .into_iter()
        .map(|parent_ref| RouteParentStatus {
            parent_ref,
            ..RouteParentStatus::default()
        })
        .collect();
        let current: Vec<(&str, Option<&str>)> = route
            .gateway_parent_statuses()
            .map(|s| {
                (
                    s.parent_ref.name.as_str(),
                    s.parent_ref.section_name.as_deref(),
                )
            })
            .collect();
        assert_eq!(current, [("edge", Some("https")), ("whole", None)]);
        assert_eq!(
            route
                .gateway_parents()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["edge", "whole"]
        );
    }

    #[test]
    fn backends_carry_their_field_path() {
        let mut route = route(&[("web", None, None, None)]);
        route.spec.rules.push(HttpRouteRule {
            backend_refs: vec![
                BackendRef {
                    name: "other".to_owned(),
                    kind: Some("ServiceImport".to_owned()),
                    ..BackendRef::default()
                },
                BackendRef {
                    name: "api".to_owned(),
                    ..BackendRef::default()
                },
            ],
        });
        assert_eq!(
            route.local_service_refs().collect::<Vec<_>>(),
            [
                ("spec.rules[0].backendRefs[0]".to_owned(), "web"),
                ("spec.rules[1].backendRefs[1]".to_owned(), "api"),
            ]
        );
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
