//! Reading any resource the cluster serves, as the person.
//!
//! Discovery, metadata-only lists, whole lists and single objects, for kinds Zeabur has no typed
//! binding for (custom resources, and the built-in kinds when a page shows an object as it is).
//!
//! # Not installed is not forbidden
//!
//! kube-apiserver authorizes a request before it looks the resource up, so reading a resource
//! whose API is not installed answers `403`, not `404`, to someone without the permission
//! [authz]. Every read here asks discovery first, which every authenticated person may read
//! through the default `system:discovery` role [discovery-rbac]: a group version that is not
//! served is reported as not installed (`Ok(None)`), and only a served one can be refused.
//!
//! # Secret values stay here
//!
//! Only [`Cluster::reveal_secret`](crate::Cluster::reveal_secret) returns a Secret's values. A
//! Secret read through this path has every `data` and `stringData` value replaced by an empty
//! string (its keys stay), and its `kubectl.kubernetes.io/last-applied-configuration`
//! annotation, which `kubectl apply` fills with the whole object including the values
//! [kubectl-apply], replaced by a mark. Metadata-only lists of Secrets get the same treatment of
//! the annotation.
//!
//! # Listing every kind of a namespace
//!
//! A page listing every kind already holds discovery's answer, so
//! [`Cluster::list_served_metadata`](crate::Cluster::list_served_metadata) skips the per-kind
//! discovery check and asks with `resourceVersion=0` and `resourceVersionMatch=NotOlderThan`:
//! kube-apiserver may then answer from its watch cache instead of a quorum read of etcd, which
//! costs less but may be slightly behind the latest write [rv-semantics]. A listing of what
//! exists is a fair use of that; a page about one object reads it fresh.
//!
//! [`Cluster::access_rules`](crate::Cluster::access_rules) asks one `SelfSubjectRulesReview`
//! which kinds the person may list in a namespace [ssrr], so a page can mark the kinds it would
//! only be refused. The answer is a hint: it may be incomplete when another authorizer (a
//! webhook) decides too, and then a kind is listed to find out.
//!
//! [authz]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/
//! [rv-semantics]: https://kubernetes.io/docs/reference/using-api/api-concepts/#semantics-for-get-and-list
//! [ssrr]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access
//! [discovery-rbac]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#discovery-roles
//! [kubectl-apply]: https://kubernetes.io/docs/tasks/manage-kubernetes-objects/declarative-config/#how-apply-calculates-differences-and-merges-changes

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, DynamicObject, ListParams};
use kube::core::{ApiResource, GroupVersionKind};
use kube::discovery::{Discovery, Scope};

use crate::port::ClusterError;
use crate::wide::serves;

/// The annotation `kubectl apply` writes the last applied object into.
pub const LAST_APPLIED: &str = "kubectl.kubernetes.io/last-applied-configuration";

/// What replaces a hidden value.
pub const HIDDEN: &str = "(hidden)";

/// A resource kube-apiserver serves, as discovery describes it (its preferred version).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServedResource {
    /// The API group; empty for the core group.
    pub group: String,
    /// The version (`v1`).
    pub version: String,
    /// The plural resource name used in paths (`deployments`).
    pub resource: String,
    /// The kind (`Deployment`).
    pub kind: String,
    /// `true` for a namespaced resource.
    pub namespaced: bool,
    /// The verbs it supports (`get`, `list`, …).
    pub verbs: Vec<String>,
}

impl ServedResource {
    /// A resource that can be read (`get`, `list`, `watch`).
    #[must_use]
    pub fn readable(
        group: &str,
        version: &str,
        resource: &str,
        kind: &str,
        namespaced: bool,
    ) -> Self {
        Self {
            group: group.to_owned(),
            version: version.to_owned(),
            resource: resource.to_owned(),
            kind: kind.to_owned(),
            namespaced,
            verbs: ["get", "list", "watch"].map(str::to_owned).to_vec(),
        }
    }

    /// The resource to read.
    #[must_use]
    pub fn as_ref(&self) -> ResourceRef {
        ResourceRef::new(&self.group, &self.version, &self.resource, &self.kind)
    }

    /// Whether it can be listed.
    #[must_use]
    pub fn listable(&self) -> bool {
        self.verbs.iter().any(|v| v == "list")
    }
}

/// Which resource to read: group, version, plural and kind.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceRef {
    /// The API group; empty for the core group.
    pub group: String,
    /// The version.
    pub version: String,
    /// The plural resource name.
    pub resource: String,
    /// The kind.
    pub kind: String,
}

impl ResourceRef {
    /// A resource reference.
    #[must_use]
    pub fn new(group: &str, version: &str, resource: &str, kind: &str) -> Self {
        Self {
            group: group.to_owned(),
            version: version.to_owned(),
            resource: resource.to_owned(),
            kind: kind.to_owned(),
        }
    }

    /// The resource of the k8s-openapi type `K`.
    #[must_use]
    pub fn of<K: k8s_openapi::Resource>() -> Self {
        Self::new(K::GROUP, K::VERSION, K::URL_PATH_SEGMENT, K::KIND)
    }

    /// `group/version`, or `version` for the core group (the `apiVersion` of its objects).
    #[must_use]
    pub fn api_version(&self) -> String {
        if self.group.is_empty() {
            self.version.clone()
        } else {
            format!("{}/{}", self.group, self.version)
        }
    }

    /// `resource.group`, or `resource` for the core group, as RBAC and refusals name it.
    #[must_use]
    pub fn key(&self) -> String {
        if self.group.is_empty() {
            self.resource.clone()
        } else {
            format!("{}.{}", self.resource, self.group)
        }
    }

    /// Whether this is the core group's Secrets.
    #[must_use]
    pub fn is_secret(&self) -> bool {
        self.group.is_empty() && self.resource == "secrets"
    }

    pub(crate) fn api_resource(&self) -> ApiResource {
        ApiResource::from_gvk_with_plural(
            &GroupVersionKind::gvk(&self.group, &self.version, &self.kind),
            &self.resource,
        )
    }
}

/// Hides a Secret's values in an object read through the generic path (see the module
/// documentation). Other kinds are left as they are.
pub(crate) fn hide_secret_values(resource: &ResourceRef, object: &mut DynamicObject) {
    if !resource.is_secret() {
        return;
    }
    hide_last_applied(&mut object.metadata);
    for field in ["data", "stringData"] {
        if let Some(map) = object
            .data
            .get_mut(field)
            .and_then(serde_json::Value::as_object_mut)
        {
            for value in map.values_mut() {
                *value = serde_json::Value::String(String::new());
            }
        }
    }
}

/// Replaces the last-applied annotation's value, which may hold a Secret's values.
pub(crate) fn hide_last_applied(metadata: &mut ObjectMeta) {
    if let Some(value) = metadata
        .annotations
        .as_mut()
        .and_then(|a| a.get_mut(LAST_APPLIED))
    {
        HIDDEN.clone_into(value);
    }
}

/// Hides what a metadata-only list of `resource` must not show.
pub(crate) fn hide_metadata(resource: &ResourceRef, metadata: &mut ObjectMeta) {
    if resource.is_secret() {
        hide_last_applied(metadata);
    }
}

/// Every resource the cluster serves, from discovery: the aggregated form (two requests, served
/// since Kubernetes 1.30 [aggregated-discovery]) and, when that fails, one request per group.
///
/// [aggregated-discovery]: https://kubernetes.io/docs/concepts/overview/kubernetes-api/#aggregated-discovery
pub(crate) async fn served_resources(
    client: &kube::Client,
) -> Result<Vec<ServedResource>, ClusterError> {
    let discovery = match Discovery::new(client.clone()).run_aggregated().await {
        Ok(discovery) => discovery,
        Err(_) => Discovery::new(client.clone()).run().await?,
    };
    let mut served: Vec<ServedResource> = discovery
        .groups()
        .flat_map(kube::discovery::ApiGroup::recommended_resources)
        .map(|(resource, capabilities)| ServedResource {
            group: resource.group,
            version: resource.version,
            resource: resource.plural,
            kind: resource.kind,
            namespaced: capabilities.scope == Scope::Namespaced,
            verbs: capabilities.operations,
        })
        .collect();
    served.sort();
    Ok(served)
}

fn api(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
) -> Api<DynamicObject> {
    let ar = resource.api_resource();
    namespace.map_or_else(
        || Api::all_with(client.clone(), &ar),
        |namespace| Api::namespaced_with(client.clone(), namespace, &ar),
    )
}

/// `Ok(None)` for a `404` (the resource is not served after all), the error otherwise.
fn absent<T>(error: kube::Error) -> Result<Option<T>, ClusterError> {
    match ClusterError::from(error) {
        error if error.is_not_found() => Ok(None),
        other => Err(other),
    }
}

pub(crate) async fn list_objects(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
) -> Result<Option<Vec<DynamicObject>>, ClusterError> {
    if serves(client, &resource.api_version()).await? == Some(false) {
        return Ok(None);
    }
    match api(client, resource, namespace)
        .list(&ListParams::default())
        .await
    {
        Ok(list) => {
            let mut items = list.items;
            for object in &mut items {
                hide_secret_values(resource, object);
            }
            Ok(Some(items))
        }
        Err(error) => absent(error),
    }
}

/// Every object of a built-in kind, without asking discovery first (see
/// [`Cluster::list_kind`](crate::Cluster::list_kind)).
pub(crate) async fn list_kind(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
) -> Result<Vec<DynamicObject>, ClusterError> {
    let mut items = api(client, resource, namespace)
        .list(&ListParams::default())
        .await?
        .items;
    for object in &mut items {
        hide_secret_values(resource, object);
    }
    Ok(items)
}

pub(crate) async fn list_metadata(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
) -> Result<Option<Vec<ObjectMeta>>, ClusterError> {
    if serves(client, &resource.api_version()).await? == Some(false) {
        return Ok(None);
    }
    match api(client, resource, namespace)
        .list_metadata(&ListParams::default())
        .await
    {
        Ok(list) => Ok(Some(
            list.items
                .into_iter()
                .map(|item| {
                    let mut metadata = item.metadata;
                    hide_metadata(resource, &mut metadata);
                    metadata
                })
                .collect(),
        )),
        Err(error) => absent(error),
    }
}

/// [`list_metadata`] for a resource discovery has just named: no discovery check, and served
/// from the watch cache when it can be (see the module documentation).
pub(crate) async fn list_served_metadata(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: &str,
) -> Result<Option<Vec<ObjectMeta>>, ClusterError> {
    match api(client, resource, Some(namespace))
        .list_metadata(&ListParams::default().match_any())
        .await
    {
        Ok(list) => Ok(Some(
            list.items
                .into_iter()
                .map(|item| {
                    let mut metadata = item.metadata;
                    hide_metadata(resource, &mut metadata);
                    metadata
                })
                .collect(),
        )),
        Err(error) => absent(error),
    }
}

/// Whether the person may list a kind, as a `SelfSubjectRulesReview` hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// A rule allows listing it.
    Allowed,
    /// No rule allows it, and the review is complete.
    Denied,
    /// No rule allows it, but the review is incomplete (or could not be made): only a list
    /// tells.
    Unknown,
}

/// One rule of a `SelfSubjectRulesReview`, as far as listing needs it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessRule {
    /// Verbs (`list`, `*`).
    pub verbs: Vec<String>,
    /// API groups (`""` for the core group, `*`).
    pub groups: Vec<String>,
    /// Resources (`deployments`, `*`).
    pub resources: Vec<String>,
    /// Names the rule is limited to; a limited rule does not allow listing.
    pub names: Vec<String>,
}

/// What a person may do in a namespace, from one `SelfSubjectRulesReview`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessRules {
    /// The rules.
    pub rules: Vec<AccessRule>,
    /// Another authorizer may allow more than the rules say.
    pub incomplete: bool,
}

impl AccessRules {
    /// Rules that allow everything (for a test cluster without permissions).
    #[must_use]
    pub fn everything() -> Self {
        Self {
            rules: vec![AccessRule {
                verbs: vec!["*".to_owned()],
                groups: vec!["*".to_owned()],
                resources: vec!["*".to_owned()],
                names: Vec::new(),
            }],
            incomplete: false,
        }
    }

    /// Whether these rules let the person list `resource`.
    #[must_use]
    pub fn list(&self, resource: &ResourceRef) -> Access {
        let has = |values: &[String], wanted: &str| values.iter().any(|v| v == "*" || v == wanted);
        let allowed = self.rules.iter().any(|rule| {
            rule.names.is_empty()
                && has(&rule.verbs, "list")
                && has(&rule.groups, &resource.group)
                && has(&rule.resources, &resource.resource)
        });
        self.answer(allowed)
    }

    /// Whether these rules let the person `verb` the object `name` of `resource`: a rule limited
    /// to names (`resourceNames`) allows it when it names `name`.
    #[must_use]
    pub fn allows(&self, verb: &str, resource: &ResourceRef, name: &str) -> Access {
        let has = |values: &[String], wanted: &str| values.iter().any(|v| v == "*" || v == wanted);
        let allowed = self.rules.iter().any(|rule| {
            (rule.names.is_empty() || rule.names.iter().any(|n| n == name))
                && has(&rule.verbs, verb)
                && has(&rule.groups, &resource.group)
                && has(&rule.resources, &resource.resource)
        });
        self.answer(allowed)
    }

    const fn answer(&self, allowed: bool) -> Access {
        if allowed {
            Access::Allowed
        } else if self.incomplete {
            Access::Unknown
        } else {
            Access::Denied
        }
    }
}

pub(crate) async fn review_access(
    client: &kube::Client,
    request: &zeabur_core::problem::Denied,
) -> Result<bool, ClusterError> {
    use k8s_openapi::api::authorization::v1::{
        ResourceAttributes, SelfSubjectAccessReview, SelfSubjectAccessReviewSpec,
    };
    // A subresource is part of `resource` as RBAC writes it (`pods/log`); the review takes it
    // apart.
    let (resource, subresource) = match request.resource.split_once('/') {
        Some((resource, subresource)) => (resource, Some(subresource.to_owned())),
        None => (request.resource.as_str(), None),
    };
    let review = SelfSubjectAccessReview {
        spec: SelfSubjectAccessReviewSpec {
            resource_attributes: Some(ResourceAttributes {
                verb: Some(request.verb.clone()),
                group: Some(request.group.clone()),
                resource: Some(resource.to_owned()),
                subresource,
                name: request.name.clone(),
                namespace: request.namespace.clone(),
                ..ResourceAttributes::default()
            }),
            ..SelfSubjectAccessReviewSpec::default()
        },
        ..SelfSubjectAccessReview::default()
    };
    let answer = Api::<SelfSubjectAccessReview>::all(client.clone())
        .create(&kube::api::PostParams::default(), &review)
        .await?;
    Ok(answer.status.is_some_and(|status| status.allowed))
}

pub(crate) async fn access_rules(
    client: &kube::Client,
    namespace: &str,
) -> Result<AccessRules, ClusterError> {
    use k8s_openapi::api::authorization::v1::{SelfSubjectRulesReview, SelfSubjectRulesReviewSpec};
    let review = SelfSubjectRulesReview {
        spec: SelfSubjectRulesReviewSpec {
            namespace: Some(namespace.to_owned()),
        },
        ..SelfSubjectRulesReview::default()
    };
    let answer = Api::<SelfSubjectRulesReview>::all(client.clone())
        .create(&kube::api::PostParams::default(), &review)
        .await?;
    let Some(status) = answer.status else {
        return Ok(AccessRules {
            rules: Vec::new(),
            incomplete: true,
        });
    };
    Ok(AccessRules {
        incomplete: status.incomplete,
        rules: status
            .resource_rules
            .into_iter()
            .map(|rule| AccessRule {
                verbs: rule.verbs,
                groups: rule.api_groups.unwrap_or_default(),
                resources: rule.resources.unwrap_or_default(),
                names: rule.resource_names.unwrap_or_default(),
            })
            .collect(),
    })
}

pub(crate) async fn get_object(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
    name: &str,
) -> Result<Option<DynamicObject>, ClusterError> {
    if serves(client, &resource.api_version()).await? == Some(false) {
        return Ok(None);
    }
    let mut object = api(client, resource, namespace).get(name).await?;
    hide_secret_values(resource, &mut object);
    Ok(Some(object))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_read_generically_keeps_its_keys_and_loses_its_values() {
        let secret = ResourceRef::new("", "v1", "secrets", "Secret");
        let mut object: DynamicObject = serde_json::from_value(serde_json::json!({
            "apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": "db", "namespace": "shop", "annotations": {
                LAST_APPLIED: "{\"data\":{\"password\":\"aHVudGVyMg==\"}}",
                "team": "data"
            }},
            "data": {"password": "aHVudGVyMg=="},
            "stringData": {"token": "s3cr3t"}
        }))
        .unwrap();
        hide_secret_values(&secret, &mut object);
        let text = serde_json::to_string(&object).unwrap();
        assert!(!text.contains("aHVudGVyMg==") && !text.contains("s3cr3t"));
        assert_eq!(object.data["data"]["password"], "");
        assert_eq!(object.data["stringData"]["token"], "");
        let annotations = object.metadata.annotations.unwrap();
        assert_eq!(annotations[LAST_APPLIED], HIDDEN);
        assert_eq!(annotations["team"], "data");
    }

    #[test]
    fn access_rules_hint_which_kinds_may_be_listed() {
        let deployments = ResourceRef::new("apps", "v1", "deployments", "Deployment");
        let secrets = ResourceRef::new("", "v1", "secrets", "Secret");
        let rules = AccessRules {
            rules: vec![
                AccessRule {
                    verbs: vec!["get".into(), "list".into()],
                    groups: vec!["apps".into()],
                    resources: vec!["*".into()],
                    names: Vec::new(),
                },
                // A rule limited to names never allows a list.
                AccessRule {
                    verbs: vec!["*".into()],
                    groups: vec![String::new()],
                    resources: vec!["secrets".into()],
                    names: vec!["one".into()],
                },
            ],
            incomplete: false,
        };
        assert_eq!(rules.list(&deployments), Access::Allowed);
        assert_eq!(rules.list(&secrets), Access::Denied);
        // A rule limited to names allows a verb on the objects it names.
        assert_eq!(rules.allows("patch", &secrets, "one"), Access::Allowed);
        assert_eq!(rules.allows("patch", &secrets, "two"), Access::Denied);
        assert_eq!(rules.allows("patch", &deployments, "web"), Access::Denied);
        assert_eq!(rules.allows("get", &deployments, "web"), Access::Allowed);
        let partial = AccessRules {
            incomplete: true,
            ..rules
        };
        assert_eq!(partial.list(&secrets), Access::Unknown);
        assert_eq!(AccessRules::everything().list(&secrets), Access::Allowed);
    }

    #[test]
    fn other_kinds_are_left_alone() {
        let config = ResourceRef::new("", "v1", "configmaps", "ConfigMap");
        let mut object: DynamicObject = serde_json::from_value(serde_json::json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": {"name": "app", "annotations": {LAST_APPLIED: "{}"}},
            "data": {"mode": "fast"}
        }))
        .unwrap();
        hide_secret_values(&config, &mut object);
        assert_eq!(object.data["data"]["mode"], "fast");
        assert_eq!(object.metadata.annotations.unwrap()[LAST_APPLIED], "{}");
    }
}
