//! A request kube-apiserver refused.

use serde::{Deserialize, Serialize};

/// The request kube-apiserver refused with `403`, as Zeabur made it.
///
/// These are the attributes its authorizers decide on [k8s-authz]. kube-apiserver's `Status` names
/// the object (`details.kind`, `details.group`, `details.name`) but not the verb [api-conventions],
/// and its message is not an interface, so these come from the request, not from the answer.
///
/// [k8s-authz]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#review-your-request-attributes
/// [api-conventions]: https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md#response-status-kind
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Denied {
    /// The request verb (`create`, `patch`, …).
    pub verb: String,
    /// The API group; empty for the core group.
    pub group: String,
    /// The resource (plural, `deployments`).
    pub resource: String,
    /// The object's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The object's namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

impl Denied {
    /// A request for `verb` on `resource` in `group`, with no object and no namespace.
    #[must_use]
    pub fn new(verb: &str, group: &str, resource: &str) -> Self {
        Self {
            verb: verb.to_owned(),
            group: group.to_owned(),
            resource: resource.to_owned(),
            name: None,
            namespace: None,
        }
    }

    /// The same request for the object `name`.
    #[must_use]
    pub fn named(mut self, name: &str) -> Self {
        self.name = Some(name.to_owned());
        self
    }

    /// The same request in `namespace`; an empty one is cluster scope.
    #[must_use]
    pub fn in_namespace(mut self, namespace: &str) -> Self {
        self.namespace = (!namespace.is_empty()).then(|| namespace.to_owned());
        self
    }
}
