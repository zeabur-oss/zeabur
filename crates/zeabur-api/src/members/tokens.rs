//! API tokens: machine accounts managed like members. Each is a ServiceAccount in
//! [`api_token::NAMESPACE`] ([`zeabur_plan::api_token`]) with the bindings of [`super::access`]
//! for [`Holder::Token`]: Developer or Viewer in chosen projects and the baseline, or
//! `cluster-admin` for an admin token. Admins make, renew, change and delete them through member
//! plans ([`super::plan`]); nobody else has rights in the namespace.
//!
//! The token is in the answer to apply only, once, and never in a plan, a log or a stored object.
//! The answer carries kube-apiserver's CA from the `kube-root-ca.crt` ConfigMap of the namespace
//! [k8s-root-ca]. Zeabur knows the API server only by its in-cluster address, so the person fills
//! in the address they reach it on.
//!
//! [k8s-root-ca]: https://kubernetes.io/docs/reference/access-authn-authz/service-accounts-admin/#bound-service-account-token-volume

use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, Problem};
use zeabur_kube::{Cluster, ClusterError, ResourceRef};
use zeabur_plan::api_token::{self, NAMESPACE, Refusal};

use super::access::{ADMIN_ROLE, Binding, Holder, ProjectGrant, RBAC_GROUP, grants_of};
use crate::error::{ApiError, read_problem};

const ROOT_CA: &str = "kube-root-ca.crt";

/// One API token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiTokenView {
    /// Its name, which is its ServiceAccount's.
    pub name: String,
    /// Whether it is bound to `cluster-admin`, as an admin member is.
    pub admin: bool,
    /// Its roles in projects; empty for an admin token.
    pub projects: Vec<ProjectGrant>,
    /// The Kubernetes user name of whoever made it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// When it was made, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// When its latest token expires, RFC 3339, as kube-apiserver answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// A token kube-apiserver issued, shown once.
#[derive(Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssuedTokenView {
    /// The token's name.
    pub name: String,
    /// The namespace of its ServiceAccount.
    pub namespace: String,
    /// The bearer token.
    pub token: String,
    /// When it expires, RFC 3339, as kube-apiserver answered.
    pub expires_at: String,
    /// kube-apiserver's CA bundle, PEM, from `kube-root-ca.crt`; absent when it could not be
    /// read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca: Option<String>,
    /// Whether the expiry was recorded on the ServiceAccount for the token list.
    pub recorded: bool,
}

impl std::fmt::Debug for IssuedTokenView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedTokenView")
            .field("name", &self.name)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

pub(crate) fn service_accounts() -> ResourceRef {
    ResourceRef::new("", "v1", "serviceaccounts", "ServiceAccount")
}

fn cluster_role_bindings() -> ResourceRef {
    ResourceRef::new(
        RBAC_GROUP,
        "v1",
        "clusterrolebindings",
        "ClusterRoleBinding",
    )
}

/// The refusal key of the token read, as `unreadable` names it.
pub(crate) fn key() -> String {
    "serviceaccounts".to_owned()
}

/// Every API token, by name, with its access from `bindings` and the `ClusterRoleBinding`s. An
/// absent namespace has none.
pub(crate) async fn list(
    cluster: &dyn Cluster,
    bindings: &[Binding],
) -> Result<Vec<ApiTokenView>, ClusterError> {
    let accounts = match cluster
        .list_objects(&service_accounts(), Some(NAMESPACE))
        .await
    {
        Ok(found) => found.unwrap_or_default(),
        Err(error) if error.code() == Some(404) => Vec::new(),
        Err(error) => return Err(error),
    };
    let cluster_bindings = cluster
        .list_objects(&cluster_role_bindings(), None)
        .await?
        .unwrap_or_default();
    let mut admins = Vec::new();
    for binding in cluster_bindings {
        let Ok(object) = serde_json::to_value(&binding) else {
            continue;
        };
        if let Some(name) = admin_token(&object) {
            admins.push(name);
        }
    }
    let mut items = Vec::new();
    for account in accounts {
        let Ok(object) = serde_json::to_value(&account) else {
            continue;
        };
        let Some(token) = api_token::read(&object) else {
            continue;
        };
        let admin = admins.contains(&token.name);
        let projects = if admin {
            Vec::new()
        } else {
            grants_of(bindings, &Holder::token(&token.name))
        };
        items.push(ApiTokenView {
            admin,
            projects,
            name: token.name,
            created_by: token.created_by,
            created_at: token.created_at,
            expires_at: token.expires_at,
        });
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

/// The token a `ClusterRoleBinding` makes an admin: one Zeabur named and labelled for it, bound
/// to `cluster-admin`.
fn admin_token(binding: &Value) -> Option<String> {
    let name = binding
        .pointer(&format!(
            "/metadata/labels/{}",
            api_token::LABEL.replace('/', "~1")
        ))?
        .as_str()?;
    let holder = Holder::token(name);
    let named = binding.pointer("/metadata/name").and_then(Value::as_str)
        == Some(holder.binding_name().as_str());
    let role = binding.pointer("/roleRef/name").and_then(Value::as_str);
    (named && role == Some(ADMIN_ROLE)).then(|| name.to_owned())
}

fn no_token(name: &str) -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        format!("There is no API token {name}."),
    ))
}

/// The ServiceAccount of the token `name`; `None` when there is none.
pub(crate) async fn account(cluster: &dyn Cluster, name: &str) -> Result<Option<Value>, ApiError> {
    match cluster
        .get_object(&service_accounts(), Some(NAMESPACE), name)
        .await
    {
        Ok(found) => Ok(found.and_then(|o| serde_json::to_value(o).ok())),
        Err(error) if error.code() == Some(404) => Ok(None),
        Err(error) => Err(read_problem(
            error,
            &format!("ServiceAccount {name} in {NAMESPACE}"),
            Denied {
                verb: "get".to_owned(),
                group: String::new(),
                resource: "serviceaccounts".to_owned(),
                name: Some(name.to_owned()),
                namespace: Some(NAMESPACE.to_owned()),
            },
        )),
    }
}

/// The ServiceAccount of the token `name`, which must exist and be that token.
pub(crate) async fn existing(cluster: &dyn Cluster, name: &str) -> Result<Value, ApiError> {
    let object = account(cluster, name)
        .await?
        .ok_or_else(|| no_token(name))?;
    api_token::read(&object).ok_or_else(|| no_token(name))?;
    Ok(object)
}

pub(crate) fn refusal(refused: Refusal) -> ApiError {
    let detail = match refused {
        Refusal::Name => "A token's name needs lower-case letters, digits and -, at most 63 \
                          characters, starting and ending with a letter or digit."
            .to_owned(),
        Refusal::Seconds => format!(
            "A token lasts from {} minutes to {} days.",
            api_token::MIN_SECONDS / 60,
            api_token::MAX_SECONDS / 86_400
        ),
    };
    ApiError::from(Problem::new(Kind::BadRequest, detail))
}

/// kube-apiserver's CA bundle, from the token namespace's `kube-root-ca.crt`.
pub(crate) async fn root_ca(cluster: &dyn Cluster) -> Option<String> {
    let configmaps = ResourceRef::new("", "v1", "configmaps", "ConfigMap");
    let found = cluster
        .get_object(&configmaps, Some(NAMESPACE), ROOT_CA)
        .await
        .ok()??;
    found
        .data
        .pointer("/data/ca.crt")
        .and_then(Value::as_str)
        .map(str::to_owned)
}
