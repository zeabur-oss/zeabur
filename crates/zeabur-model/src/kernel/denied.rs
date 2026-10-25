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
