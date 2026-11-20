//! `GET /api/v1/account`: who you are and what you may do, as the cluster sees it.
//!
//! It also says where to manage how you sign in. Nothing here reads the issuer's own objects, so
//! it works with any issuer kube-apiserver trusts and for everyone, admin or not.
//!
//! # Who you are
//!
//! One `SelfSubjectReview` [k8s-ssr]: the user name and groups kube-apiserver authenticated.
//!
//! # What you may do
//!
//! A cluster administrator (every verb on every resource, cluster-wide, from one
//! `SelfSubjectAccessReview`) is Admin in every project, and no project is reviewed. Anyone else
//! gets, per adopted project, the product's role kube-apiserver's reviews [k8s-ssrr] point to:
//!
//! | Role | Answer |
//! |---|---|
//! | Admin | an access review allows `create` on `rolebindings.rbac.authorization.k8s.io` |
//! | Developer | the rules review has a write verb (the project is not read-only) |
//! | Viewer | read-only, and an access review allows `list` on `pods` |
//!
//! Less than that is no role. A project whose reviews fail, or whose rules review is incomplete,
//! says so, without a role.
//!
//! # Signing in
//!
//! Zeabur holds no credentials. The page links to the issuer's own page for them: the URL the
//! server was started with (`--account-url`), else the `manage` URL the issuer publishes in
//! `/.well-known/passkey-endpoints` ([`SignInPage`]), else nothing.
//!
//! [k8s-ssr]: https://kubernetes.io/docs/reference/access-authn-authz/authentication/#self-subject-review
//! [k8s-ssrr]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::extract::State;
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_core::names::{LABEL_PROJECT, admin_only_namespace, is_system_namespace};
use zeabur_core::problem::{Denied, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{AccessRules, Cluster};
use zeabur_model::ProjectDetails;

use crate::error::ApiError;
use crate::extract::Json;
use crate::handlers::{AppState, Person};

/// How many projects are reviewed at the same time.
const CONCURRENCY: usize = 8;

/// Finds the issuer's page where people manage how they sign in.
pub trait SignInPage: Send + Sync {
    /// Its URL, when the issuer names one.
    fn find(&self) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>>;
}

/// A fixed answer: none where there is no issuer to ask, or one for tests.
#[derive(Debug, Clone, Default)]
pub struct Fixed(pub Option<String>);

impl SignInPage for Fixed {
    fn find(&self) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>> {
        let answer = self.0.clone();
        Box::pin(async move { answer })
    }
}

/// A product role in a project, from the person's rights there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum AccountRole {
    /// Rights short of Viewer, or none.
    None,
    /// Reads services, replicas and logs.
    Viewer,
    /// Also changes services, opens terminals and reads secrets.
    Developer,
    /// Also gives others access.
    Admin,
}

/// Your role in one adopted project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAccess {
    /// The Namespace's name.
    pub name: String,
    /// The name shown instead, when one was given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Your role; absent when your rights there could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<AccountRole>,
}

/// Who you are, what you may do, and where to manage how you sign in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    /// `account`.
    pub id: String,
    /// Your user name, as kube-apiserver authenticated you.
    pub username: String,
    /// Your groups, as kube-apiserver authenticated you.
    pub groups: Vec<String>,
    /// Whether you may do anything anywhere: Admin in every project.
    pub cluster_admin: bool,
    /// Your role in each adopted project, by name; absent when the projects could not be listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projects: Option<Vec<ProjectAccess>>,
    /// The issuer's page for your sign-in methods, when it names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manage_sign_in_url: Option<String>,
}

/// The person's role in `project`, whose read-only answer is `read_only` (see the module docs).
pub(crate) async fn role(
    cluster: &dyn Cluster,
    project: &str,
    read_only: Option<bool>,
) -> Option<AccountRole> {
    let review = |verb: &str, group: &str, resource: &str| Denied {
        verb: verb.to_owned(),
        group: group.to_owned(),
        resource: resource.to_owned(),
        name: None,
        namespace: Some(project.to_owned()),
    };
    let admin = review("create", "rbac.authorization.k8s.io", "rolebindings");
    let pods = review("list", "", "pods");
    let (admin, viewer) =
        futures_util::join!(cluster.review_access(&admin), cluster.review_access(&pods));
    if admin.ok()? {
        return Some(AccountRole::Admin);
    }
    if !read_only? {
        return Some(AccountRole::Developer);
    }
    Some(if viewer.ok()? {
        AccountRole::Viewer
    } else {
        AccountRole::None
    })
}

/// Whether the person may do every verb on every resource, cluster-wide.
pub(crate) async fn cluster_admin(cluster: &dyn Cluster) -> bool {
    let everything = Denied {
        verb: "*".to_owned(),
        group: "*".to_owned(),
        resource: "*".to_owned(),
        name: None,
        namespace: None,
    };
    cluster.review_access(&everything).await.unwrap_or(false)
}

/// The adopted projects the person may list, by name; `None` when they may not list them.
async fn adopted(cluster: &dyn Cluster) -> Option<Vec<(String, ProjectDetails)>> {
    let namespaces = cluster.namespaces().await.ok()?;
    let mut projects: Vec<_> = namespaces
        .into_iter()
        .filter(|n| {
            n.metadata
                .labels
                .as_ref()
                .is_some_and(|l| l.contains_key(LABEL_PROJECT))
        })
        .filter_map(|n| {
            let details = ProjectDetails::from_annotations(n.metadata.annotations.as_ref());
            n.metadata
                .name
                .filter(|name| !is_system_namespace(name) && admin_only_namespace(name).is_none())
                .map(|name| (name, details))
        })
        .collect();
    projects.sort_by(|a, b| a.0.cmp(&b.0));
    Some(projects)
}

/// Who you are, your role in each project, and where to manage how you sign in.
#[utoipa::path(get, path = "/api/v1/account", tag = "identity", operation_id = "getAccount",
    responses((status = 200, body = AccountView),
              (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<AccountView>, ApiError> {
    let (you, admin, projects, page) = futures_util::join!(
        cluster.whoami(),
        cluster_admin(cluster.as_ref()),
        adopted(cluster.as_ref()),
        async {
            match &state.settings.account_url {
                Some(url) => Some(url.clone()),
                None => state.sign_in.find().await,
            }
        },
    );
    let you = you.map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let projects = match projects {
        Some(projects) => Some(roles(&cluster, projects, admin).await),
        None => None,
    };
    Ok(Json(AccountView {
        id: "account".to_owned(),
        username: you.username,
        groups: you.groups,
        cluster_admin: admin,
        projects,
        manage_sign_in_url: page,
    }))
}

async fn roles(
    cluster: &Arc<dyn Cluster>,
    projects: Vec<(String, ProjectDetails)>,
    admin: bool,
) -> Vec<ProjectAccess> {
    futures_util::stream::iter(projects)
        .map(|(name, details)| {
            let cluster = Arc::clone(cluster);
            async move {
                let role = if admin {
                    Some(AccountRole::Admin)
                } else {
                    let rules = cluster.access_rules(&name).await.ok();
                    let read_only = rules.as_ref().and_then(AccessRules::read_only);
                    role(cluster.as_ref(), &name, read_only).await
                };
                ProjectAccess {
                    name,
                    display_name: details.display_name,
                    role,
                }
            }
        })
        .buffered(CONCURRENCY)
        .collect()
        .await
}
