//! API tokens: machine accounts managed like members, each a ServiceAccount
//! ([`zeabur_plan::api_token`]) with the bindings of [`super::access`]. Admins change them
//! through member plans ([`super::plan`]).
//!
//! The token is only in the answer to apply, once, with kube-apiserver's CA from the namespace's
//! `kube-root-ca.crt` [k8s-root-ca].
//!
//! [k8s-root-ca]: https://kubernetes.io/docs/reference/access-authn-authz/service-accounts-admin/#bound-service-account-token-volume

use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::{ConfigMap, ServiceAccount};
use zeabur_kube::{Cluster, ClusterError, DynamicObject, ResourceRef};
use zeabur_plan::api_token::{self, NAMESPACE, Refusal};

use super::access::{Binding, Holder, ProjectGrant, grants_of};
use crate::error::{ApiError, read_problem};
use crate::projects::namespace::Permission;

const ROOT_CA: &str = "kube-root-ca.crt";

/// One API token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiTokenView {
    /// Its name, which is its ServiceAccount's.
    pub name: String,
    /// Its roles in projects.
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
    /// kube-apiserver's CA bundle, PEM; absent when it could not be read.
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

/// Whether the person may create ServiceAccounts in the token namespace.
pub(crate) async fn can_manage(cluster: &dyn Cluster) -> Permission {
    let request = Denied {
        verb: "create".to_owned(),
        group: String::new(),
        resource: "serviceaccounts".to_owned(),
        name: None,
        namespace: Some(NAMESPACE.to_owned()),
    };
    match cluster.review_access(&request).await {
        Ok(true) => Permission::Allowed,
        Ok(false) => Permission::Denied,
        Err(_) => Permission::Unknown,
    }
}

/// The refusal key of the token read, as `unreadable` names it.
pub(crate) fn key() -> String {
    "serviceaccounts".to_owned()
}

/// Every API token, by name, with its access from `bindings`. An absent namespace has none.
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
    let mut items: Vec<ApiTokenView> = accounts
        .into_iter()
        .filter_map(|account| api_token::read(&account.try_parse::<ServiceAccount>().ok()?))
        .map(|token| ApiTokenView {
            projects: grants_of(bindings, &Holder::token(&token.name)),
            name: token.name,
            created_by: token.created_by,
            created_at: token.created_at,
            expires_at: token.expires_at,
        })
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

fn no_token(name: &str) -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        format!("There is no API token {name}."),
    ))
}

/// The ServiceAccount of the token `name`; `None` when there is none.
pub(crate) async fn account(
    cluster: &dyn Cluster,
    name: &str,
) -> Result<Option<ServiceAccount>, ApiError> {
    match cluster
        .get_object(&service_accounts(), Some(NAMESPACE), name)
        .await
    {
        Ok(found) => Ok(found.and_then(|o| o.try_parse().ok())),
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
pub(crate) async fn existing(
    cluster: &dyn Cluster,
    name: &str,
) -> Result<ServiceAccount, ApiError> {
    let account = account(cluster, name)
        .await?
        .ok_or_else(|| no_token(name))?;
    api_token::read(&account).ok_or_else(|| no_token(name))?;
    Ok(account)
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
    let found: DynamicObject = cluster
        .get_object(&ResourceRef::of::<ConfigMap>(), Some(NAMESPACE), ROOT_CA)
        .await
        .ok()??;
    found.try_parse::<ConfigMap>().ok()?.data?.remove("ca.crt")
}
