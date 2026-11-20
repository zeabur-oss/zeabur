//! Reading any resource the cluster serves, as the person.
//!
//! Discovery, metadata-only lists, whole lists and single objects, for kinds Zeabur has no typed
//! binding for (custom resources, and the built-in kinds when a page shows an object as it is).
//!
//! # Not installed is not forbidden
//!
//! kube-apiserver authorizes a request before it looks the resource up, so reading a resource
//! whose API is not installed answers `403`, not `404`, to someone without the permission
//! [authz]. Every read here goes first, and only a refusal (and, for a single object, a `404`)
//! asks discovery, which every authenticated person may read through the default
//! `system:discovery` role [discovery-rbac]: a group version that is not served is reported as
//! not installed (`Ok(None)`), and only a served one is refused. A read that succeeds costs one
//! round trip.
//!
//! # Secret values stay here
//!
//! Only [`Cluster::reveal_secret`](crate::Cluster::reveal_secret) returns a Secret's values. A
//! Secret read through this path is decoded from the response bytes by a decoder that skips
//! every `data` and `stringData` value (the keys stay, each with an empty string), and its
//! `kubectl.kubernetes.io/last-applied-configuration` annotation, which `kubectl apply` fills with
//! the whole object including the values [kubectl-apply], replaced by a mark. Metadata-only lists
//! of Secrets get the same treatment of the annotation.
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
//! [`Cluster::access_rules`](crate::Cluster::access_rules) asks one `SelfSubjectRulesReview` of a
//! namespace [ssrr], which says only whether the person may write there at all. It may be
//! incomplete when another authorizer (a webhook) decides too. A right that must be known before
//! a click is one `SelfSubjectAccessReview`; the rest is decided by the dry run.
//!
//! [authz]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/
//! [rv-semantics]: https://kubernetes.io/docs/reference/using-api/api-concepts/#semantics-for-get-and-list
//! [ssrr]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access
//! [discovery-rbac]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#discovery-roles
//! [kubectl-apply]: https://kubernetes.io/docs/tasks/manage-kubernetes-objects/declarative-config/#how-apply-calculates-differences-and-merges-changes

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, DynamicObject, GetParams, ListParams};
use kube::core::{ApiResource, GroupVersionKind, Request, Resource as _};
use kube::discovery::{Discovery, Scope};

use crate::port::{ClusterError, RawList};
use crate::secrets;
use crate::wide::serves;

/// The annotation `kubectl apply` writes the last applied object into.
pub const LAST_APPLIED: &str = "kubectl.kubernetes.io/last-applied-configuration";

/// What replaces a hidden value.
pub const HIDDEN: &str = "(hidden)";

/// A resource kube-apiserver serves, as discovery describes it, at the version [`served_resources`]
/// picks for it.
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
/// A resource is served when any version of its group serves it, not only the group's preferred
/// version: an operator can move some kinds of a group to a new version and keep the others at
/// the old one. Each resource is listed once, at the version [`one_version`] picks.
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
        .flat_map(|group| {
            let every = group
                .versions()
                .flat_map(|version| group.versioned_resources(version))
                .map(|(resource, capabilities)| ServedResource {
                    group: resource.group,
                    version: resource.version,
                    resource: resource.plural,
                    kind: resource.kind,
                    namespaced: capabilities.scope == Scope::Namespaced,
                    verbs: capabilities.operations,
                })
                .collect();
            one_version(group.preferred_version_or_latest(), every)
        })
        .collect();
    served.sort();
    Ok(served)
}

/// One entry per resource of a group, as kubectl picks it [server-preferred]: the group's
/// `preferred` version when it serves the resource, else the first version in `every` that does.
/// `every` holds the group's resources at each of its versions, in kube-rs's order of Kubernetes
/// version priority (`v1` before `v1beta1`) [version-priority].
///
/// [server-preferred]: https://github.com/kubernetes/client-go/blob/master/discovery/discovery_client.go
/// [version-priority]: https://kubernetes.io/docs/tasks/extend-kubernetes/custom-resources/custom-resource-definition-versioning/#version-priority
fn one_version(preferred: &str, every: Vec<ServedResource>) -> Vec<ServedResource> {
    let (mut chosen, others): (Vec<_>, Vec<_>) =
        every.into_iter().partition(|r| r.version == preferred);
    for other in others {
        if !chosen.iter().any(|r| r.resource == other.resource) {
            chosen.push(other);
        }
    }
    chosen
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

/// [`absent`], and `Ok(None)` for a refusal when discovery says the API is not installed.
async fn absent_or_not_installed<T>(
    client: &kube::Client,
    resource: &ResourceRef,
    error: kube::Error,
) -> Result<Option<T>, ClusterError> {
    match ClusterError::from(error) {
        error if error.is_not_found() => Ok(None),
        error if error.is_refused() => not_installed(client, resource, error).await,
        other => Err(other),
    }
}

/// `Ok(None)` when discovery says the API of `resource` is not installed, `error` otherwise.
async fn not_installed<T>(
    client: &kube::Client,
    resource: &ResourceRef,
    error: ClusterError,
) -> Result<Option<T>, ClusterError> {
    if serves(client, &resource.api_version()).await? == Some(false) {
        Ok(None)
    } else {
        Err(error)
    }
}

/// The LIST of `resource` as kube-apiserver sent it.
async fn request_list(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
) -> Result<RawList, kube::Error> {
    let path = DynamicObject::url_path(&resource.api_resource(), namespace);
    let request = Request::new(path)
        .list(&ListParams::default())
        .map_err(kube::Error::BuildRequest)?;
    let body = client.request_text(request).await?.into_bytes();
    Ok(RawList::new(resource, body))
}

pub(crate) async fn list_objects(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
) -> Result<Option<Vec<DynamicObject>>, ClusterError> {
    match request_list(client, resource, namespace).await {
        Ok(list) => Ok(Some(list.objects()?)),
        Err(error) => absent_or_not_installed(client, resource, error).await,
    }
}

/// Every object of a built-in kind, without asking discovery first (see
/// [`Cluster::list_raw`](crate::Cluster::list_raw)).
pub(crate) async fn list_raw(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
) -> Result<RawList, ClusterError> {
    Ok(request_list(client, resource, namespace).await?)
}

/// The metadata of the objects of a built-in kind that carry `key=value` (see
/// [`Cluster::list_labelled`](crate::Cluster::list_labelled)).
pub(crate) async fn list_labelled(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
    selector: &str,
) -> Result<Vec<ObjectMeta>, ClusterError> {
    let list = api(client, resource, namespace)
        .list_metadata(&ListParams::default().labels(selector))
        .await?;
    Ok(list
        .items
        .into_iter()
        .map(|item| {
            let mut metadata = item.metadata;
            hide_metadata(resource, &mut metadata);
            metadata
        })
        .collect())
}

pub(crate) async fn list_metadata(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
) -> Result<Option<Vec<ObjectMeta>>, ClusterError> {
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
        Err(error) => absent_or_not_installed(client, resource, error).await,
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

/// One rule of a `SelfSubjectRulesReview`, as far as [`AccessRules::read_only`] needs it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessRule {
    /// Verbs (`list`, `*`).
    pub verbs: Vec<String>,
    /// API groups (`""` for the core group, `*`).
    pub groups: Vec<String>,
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
            }],
            incomplete: false,
        }
    }

    /// Whether the person may only read in the namespace: no rule has a write verb.
    /// `None` when the review is incomplete and no rule writes, since another authorizer may
    /// allow a write. Creating the self reviews every signed-in person may make
    /// (`system:basic-user`) is not a write.
    #[must_use]
    pub fn read_only(&self) -> Option<bool> {
        self.grants(&WRITE_VERBS).map(|writes| !writes)
    }

    /// Whether the person may list some resource in the namespace. `None` when the review is
    /// incomplete and no rule lists.
    #[must_use]
    pub fn lists_any(&self) -> Option<bool> {
        self.grants(&["*", "list"])
    }

    /// Whether a rule outside the self-review groups has one of `verbs`; `None` when none does
    /// but the review is incomplete.
    fn grants(&self, verbs: &[&str]) -> Option<bool> {
        let granted = self.rules.iter().any(|rule| {
            rule.verbs.iter().any(|v| verbs.contains(&v.as_str()))
                && rule
                    .groups
                    .iter()
                    .any(|g| !SELF_REVIEW_GROUPS.contains(&g.as_str()))
        });
        (granted || !self.incomplete).then_some(granted)
    }
}

/// Verbs that change objects ([verbs]).
///
/// [verbs]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#determine-the-request-verb
const WRITE_VERBS: [&str; 6] = [
    "*",
    "create",
    "update",
    "patch",
    "delete",
    "deletecollection",
];

/// Groups of the reviews a person makes about themselves.
const SELF_REVIEW_GROUPS: [&str; 2] = ["authorization.k8s.io", "authentication.k8s.io"];

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
    // kube-apiserver authorises `namespaces/{name}` in namespace `name`, so a RoleBinding there
    // counts: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/apiserver/pkg/endpoints/request/requestinfo.go
    let namespace = if request.group.is_empty() && resource == "namespaces" {
        request.namespace.clone().or_else(|| request.name.clone())
    } else {
        request.namespace.clone()
    };
    let review = SelfSubjectAccessReview {
        spec: SelfSubjectAccessReviewSpec {
            resource_attributes: Some(ResourceAttributes {
                verb: Some(request.verb.clone()),
                group: Some(request.group.clone()),
                resource: Some(resource.to_owned()),
                subresource,
                name: request.name.clone(),
                namespace,
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
    match request_object(client, resource, namespace, name).await {
        Ok(body) => Ok(Some(if resource.is_secret() {
            secrets::one(&body)?
        } else {
            serde_json::from_slice(&body).map_err(|e| ClusterError::Decode(e.to_string()))?
        })),
        Err(error) => match ClusterError::from(error) {
            error if error.is_refused() || error.is_not_found() => {
                not_installed(client, resource, error).await
            }
            other => Err(other),
        },
    }
}

/// The bytes kube-apiserver sent for one object.
async fn request_object(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
    name: &str,
) -> Result<Vec<u8>, kube::Error> {
    let path = DynamicObject::url_path(&resource.api_resource(), namespace);
    let request = Request::new(path)
        .get(name, &GetParams::default())
        .map_err(kube::Error::BuildRequest)?;
    Ok(client.request_text(request).await?.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn served(version: &str, resource: &str, kind: &str) -> ServedResource {
        ServedResource {
            group: "operator.victoriametrics.com".to_owned(),
            version: version.to_owned(),
            resource: resource.to_owned(),
            kind: kind.to_owned(),
            namespaced: true,
            verbs: vec!["get".to_owned(), "list".to_owned()],
        }
    }

    #[test]
    fn a_kind_outside_the_preferred_version_is_served_at_its_own_version() {
        let every = vec![
            served("v1", "vlogs", "VLogs"),
            served("v1beta1", "vmrules", "VMRule"),
            served("v1beta1", "vlogs", "VLogs"),
            served("v1alpha1", "vmrules", "VMRule"),
        ];
        assert_eq!(
            one_version("v1", every),
            [
                served("v1", "vlogs", "VLogs"),
                served("v1beta1", "vmrules", "VMRule"),
            ]
        );
    }

    #[test]
    fn the_preferred_version_wins_over_a_more_stable_one() {
        let every = vec![
            served("v1", "widgets", "Widget"),
            served("v1beta1", "widgets", "Widget"),
        ];
        assert_eq!(
            one_version("v1beta1", every),
            [served("v1beta1", "widgets", "Widget")]
        );
    }

    fn rule(verbs: &[&str], groups: &[&str]) -> AccessRule {
        AccessRule {
            verbs: verbs.iter().map(|v| (*v).to_owned()).collect(),
            groups: groups.iter().map(|g| (*g).to_owned()).collect(),
        }
    }

    #[test]
    fn a_namespace_without_a_write_verb_is_read_only() {
        let viewer = AccessRules {
            rules: vec![
                rule(&["get", "list", "watch"], &["*"]),
                rule(&["create"], &["authorization.k8s.io"]),
            ],
            incomplete: false,
        };
        assert_eq!(viewer.read_only(), Some(true));
        let incomplete = AccessRules {
            incomplete: true,
            ..viewer
        };
        assert_eq!(incomplete.read_only(), None);
        let mut editor = incomplete;
        editor.rules.push(rule(&["patch"], &["apps"]));
        assert_eq!(editor.read_only(), Some(false));
        assert_eq!(AccessRules::everything().read_only(), Some(false));
        assert_eq!(AccessRules::default().read_only(), Some(true));
    }

    #[test]
    fn listing_one_custom_kind_is_listing_something() {
        let reviewer = AccessRules {
            rules: vec![rule(&["create"], &["authorization.k8s.io"])],
            incomplete: false,
        };
        assert_eq!(reviewer.lists_any(), Some(false));
        let mut widgets = reviewer;
        widgets.rules.push(rule(&["list"], &["example.com"]));
        assert_eq!(widgets.lists_any(), Some(true));
    }
}
