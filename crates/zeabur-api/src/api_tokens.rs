//! API tokens for scripts and CI: a project's ServiceAccounts labelled as tokens
//! ([`zeabur_plan::api_token`]), made, renewed and deleted as plans the way members' changes are
//! (see `members::plan`).
//!
//! Everything goes to kube-apiserver as the person, the `TokenRequest` included, so RBAC decides:
//! creating a binding needs the rights the role grants or `bind` on it [k8s-escalation], so a
//! person hands out only what they hold. The chart lets a project Developer write token
//! ServiceAccounts and their `RoleBinding`s, and its admission policy keeps those bindings to one
//! of the project roles for one ServiceAccount of the namespace. Before a plan, a
//! `SelfSubjectAccessReview` per right tells which roles the person can give and why not.
//!
//! The token is in the answer to apply only, once, and never in a plan, a log or a stored object.
//! The answer carries kube-apiserver's CA from the `kube-root-ca.crt` ConfigMap of the namespace
//! [k8s-root-ca]. Zeabur knows the API server only by its in-cluster address, so the person fills
//! in the address they reach it on.
//!
//! [k8s-escalation]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/#restrictions-on-role-binding-creation-or-update
//! [k8s-root-ca]: https://kubernetes.io/docs/reference/access-authn-authz/service-accounts-admin/#bound-service-account-token-volume

use std::collections::{BTreeMap, BTreeSet};

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::message::Message;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{
    Cluster, ClusterExt as _, IssuedToken, ObjectTarget, Preconditions, ResourceRef, WriteBody,
    WriteRequest,
};
use zeabur_plan::api_token::{self, Addressed, NewToken, Refusal, Scope, Step};

use crate::error::{ApiError, read_problem};
use crate::handlers::{AppState, Person};
use crate::members::access::ProjectRole;
use crate::members::plan::{MemberWrite, MemberWriteAction};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::projects::namespace::Permission;
use crate::projects::writes::token_problem;

/// Sealed token plans start with this, so no other plan's token opens as one.
const PREFIX: &str = "t1.";
const RBAC_GROUP: &str = "rbac.authorization.k8s.io";
const ROOT_CA: &str = "kube-root-ca.crt";

/// A change to a project's API tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum ApiTokenChange {
    /// Make a token: its ServiceAccount, the binding to `role` and the first token.
    #[serde(rename_all = "camelCase")]
    Create {
        /// The ServiceAccount's name: lower-case letters, digits and `-`, at most 63 characters.
        name: String,
        /// The project role it gets.
        role: ProjectRole,
        /// Whether the role applies in every namespace (`ClusterRoleBinding`), not only the
        /// project's.
        #[serde(default)]
        cluster: bool,
        /// How long the token should last; kube-apiserver may cut it.
        expiration_seconds: i64,
    },
    /// Issue another token for the same ServiceAccount; the ones issued before keep working until
    /// they expire.
    #[serde(rename_all = "camelCase")]
    Renew {
        /// The token's name.
        name: String,
        /// How long the new token should last.
        expiration_seconds: i64,
    },
    /// Delete the binding and the ServiceAccount, which revokes every token issued for it.
    #[serde(rename_all = "camelCase")]
    Delete {
        /// The token's name.
        name: String,
    },
}

/// One API token of a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiTokenView {
    /// The ServiceAccount's name.
    pub name: String,
    /// The role recorded when it was made; absent when it names another `ClusterRole`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ProjectRole>,
    /// Whether the role applies in every namespace.
    pub cluster: bool,
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

/// Why a role cannot be given to a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ChoiceRefusal {
    /// The person may not make tokens in the project.
    NoTokenRights,
    /// The person does not hold the role in the project, so cannot hand it out.
    NotHeld,
    /// Only admins make tokens that reach every namespace.
    NotAdmin,
}

/// A role a token can be given, and whether this person can give it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleChoice {
    /// The role.
    pub role: ProjectRole,
    /// In every namespace.
    pub cluster: bool,
    /// What `SelfSubjectAccessReview` answered for the rights it needs.
    pub permission: Permission,
    /// Why not, when denied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<ChoiceRefusal>,
}

/// A project's API tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiTokens {
    /// The tokens, by name.
    pub items: Vec<ApiTokenView>,
    /// The roles a new token can be given.
    pub choices: Vec<RoleChoice>,
    /// Whether the person may renew a token.
    pub can_renew: Permission,
    /// Whether the person may delete a token.
    pub can_delete: Permission,
}

/// An API token plan and the token that applies it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiTokenPlan {
    /// Send this to `apply` to carry out exactly this plan.
    pub token: String,
    /// When the plan token stops working, RFC 3339.
    pub expires_at: String,
    /// The writes, in order. The `TokenRequest` is a creation of kind `TokenRequest`.
    pub writes: Vec<MemberWrite>,
}

/// The token of an API token plan to apply.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ApiTokenApply {
    /// The token `plan` returned.
    pub token: String,
}

/// A token kube-apiserver issued, shown once.
#[derive(Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssuedTokenView {
    /// The ServiceAccount's name.
    pub name: String,
    /// Its namespace.
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

/// What an applied plan made.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiTokenApplied {
    /// For a new or renewed token: the token, shown once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued: Option<IssuedTokenView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    person: String,
    groups: Vec<String>,
    project: String,
    change: ApiTokenChange,
    versions: BTreeMap<String, String>,
    expires_at: i64,
}

/// The writes a change needs and what they were computed against.
#[derive(Debug, Clone, Default)]
struct Computed {
    steps: Vec<Step>,
    versions: BTreeMap<String, String>,
    /// The ServiceAccount as read or as it will be made, shown before the expiry is recorded.
    account: Option<Value>,
}

fn right(verb: &str, group: &str, resource: &str, namespace: Option<&str>) -> Denied {
    Denied {
        verb: verb.to_owned(),
        group: group.to_owned(),
        resource: resource.to_owned(),
        name: None,
        namespace: namespace.map(str::to_owned),
    }
}

async fn permission(cluster: &dyn Cluster, rights: &[Denied]) -> Permission {
    let mut all = Permission::Allowed;
    for wanted in rights {
        let one = match cluster.review_access(wanted).await {
            Ok(true) => Permission::Allowed,
            Ok(false) => Permission::Denied,
            Err(_) => Permission::Unknown,
        };
        all = all.and(one);
    }
    all
}

fn bad(detail: impl Into<String>) -> ApiError {
    ApiError::from(Problem::new(Kind::BadRequest, detail))
}

fn refuse(kind: Kind, detail: &str, key: &str) -> ApiError {
    let mut problem = Problem::new(kind, detail);
    problem.summary = Message::new(key);
    ApiError::from(problem)
}

fn check_project(project: &str) -> Result<(), ApiError> {
    if zeabur_plan::checks::is_dns_label(project) {
        Ok(())
    } else {
        Err(bad(format!("{project:?} is not a project's name.")))
    }
}

fn service_accounts() -> ResourceRef {
    ResourceRef::new("", "v1", "serviceaccounts", "ServiceAccount")
}

/// A deletion of one object of a token, as a member's plan writes it.
pub(crate) struct Revoked {
    pub(crate) target: ObjectTarget,
    pub(crate) uid: Option<String>,
    pub(crate) before: Value,
}

/// The tokens `created_by` made in `project` that go when their role there changes: every one,
/// or with `viewer_stays` all but their Viewer tokens. A token acts for its maker, so it keeps no
/// more than they do. Tokens for every namespace stay: only admins make them. Each token's binding
/// goes before its ServiceAccount.
pub(crate) async fn revoked(
    cluster: &dyn Cluster,
    project: &str,
    created_by: &str,
    viewer_stays: bool,
) -> Result<Vec<Revoked>, ApiError> {
    let accounts = cluster
        .list_kind(&service_accounts(), Some(project))
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("the ServiceAccounts of {project}"),
                right("list", "", "serviceaccounts", Some(project)),
            )
        })?;
    let viewer = ProjectRole::Viewer.cluster_role();
    let mut revoked = Vec::new();
    for account in accounts {
        let Ok(object) = serde_json::to_value(&account) else {
            continue;
        };
        let Some(token) = api_token::read(&object) else {
            continue;
        };
        if token.scope != Scope::Project || token.created_by.as_deref() != Some(created_by) {
            continue;
        }
        if viewer_stays && token.role.as_deref() == Some(viewer) {
            continue;
        }
        let binding = read_object(
            cluster,
            &Addressed::binding(project, &token.name, Scope::Project),
        )
        .await?;
        for step in api_token::delete(&object, binding.as_ref()) {
            if let Step::Delete {
                target,
                uid,
                before,
            } = step
            {
                revoked.push(Revoked {
                    target: object_target(&target),
                    uid,
                    before,
                });
            }
        }
    }
    Ok(revoked)
}

fn view(token: api_token::Token) -> ApiTokenView {
    ApiTokenView {
        role: token.role.as_deref().and_then(ProjectRole::of_cluster_role),
        cluster: token.scope == Scope::Cluster,
        name: token.name,
        created_by: token.created_by,
        created_at: token.created_at,
        expires_at: token.expires_at,
    }
}

/// The roles a new token can be given in `project`, each with the rights it needs.
async fn choices(cluster: &dyn Cluster, project: &str) -> Vec<RoleChoice> {
    let ns = Some(project);
    let tokens = permission(
        cluster,
        &[
            right("create", "", "serviceaccounts", ns),
            right("patch", "", "serviceaccounts", ns),
            right("create", "", "serviceaccounts/token", ns),
        ],
    )
    .await;
    let binds = permission(cluster, &[right("create", RBAC_GROUP, "rolebindings", ns)]).await;
    let project_tokens = tokens.and(binds);
    // Only a Developer reads Secrets: whoever cannot does not hold the role to hand it out.
    let developer = permission(cluster, &[right("get", "", "secrets", ns)]).await;
    let admin = permission(
        cluster,
        &[right("create", RBAC_GROUP, "clusterrolebindings", None)],
    )
    .await;
    let choice = |role, cluster, permission: Permission, refusal| RoleChoice {
        role,
        cluster,
        permission,
        refusal: (permission == Permission::Denied).then_some(refusal),
    };
    let mut out = Vec::new();
    for role in [ProjectRole::Developer, ProjectRole::Viewer] {
        let held = if role == ProjectRole::Developer {
            developer
        } else {
            Permission::Allowed
        };
        let refusal = if project_tokens == Permission::Denied {
            ChoiceRefusal::NoTokenRights
        } else {
            ChoiceRefusal::NotHeld
        };
        out.push(choice(role, false, project_tokens.and(held), refusal));
    }
    for role in [ProjectRole::Developer, ProjectRole::Viewer] {
        out.push(choice(
            role,
            true,
            tokens.and(admin),
            ChoiceRefusal::NotAdmin,
        ));
    }
    out
}

/// A project's API tokens and the roles a new one can be given. Read as the person.
#[utoipa::path(get, path = "/api/v1/projects/{project}/api-tokens", tag = "projects",
    operation_id = "listApiTokens",
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ApiTokens),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn list(
    Person(cluster): Person,
    Path(project): Path<String>,
) -> Result<Json<ApiTokens>, ApiError> {
    check_project(&project)?;
    let accounts = cluster
        .list_kind(&service_accounts(), Some(&project))
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("the ServiceAccounts of {project}"),
                right("list", "", "serviceaccounts", Some(&project)),
            )
        })?;
    let mut items = Vec::new();
    for account in accounts {
        let Ok(object) = serde_json::to_value(&account) else {
            continue;
        };
        if let Some(token) = api_token::read(&object) {
            items.push(view(token));
        }
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    let ns = Some(project.as_str());
    Ok(Json(ApiTokens {
        items,
        choices: choices(cluster.as_ref(), &project).await,
        can_renew: permission(
            cluster.as_ref(),
            &[
                right("create", "", "serviceaccounts/token", ns),
                right("patch", "", "serviceaccounts", ns),
            ],
        )
        .await,
        can_delete: permission(
            cluster.as_ref(),
            &[right("delete", "", "serviceaccounts", ns)],
        )
        .await,
    }))
}

fn key(target: &Addressed) -> String {
    format!("{}/{}/{}", target.kind, target.namespace, target.name)
}

fn object_target(target: &Addressed) -> ObjectTarget {
    ObjectTarget {
        api_version: target.api_version.clone(),
        kind: target.kind.clone(),
        plural: target.plural.clone(),
        namespace: target.namespace.clone(),
        name: target.name.clone(),
    }
}

fn what(target: &Addressed) -> String {
    if target.namespace.is_empty() {
        format!("{} {}", target.kind, target.name)
    } else {
        format!("{} {} in {}", target.kind, target.name, target.namespace)
    }
}

/// The object `target` names; `None` when it does not exist.
async fn read_object(cluster: &dyn Cluster, target: &Addressed) -> Result<Option<Value>, ApiError> {
    let resource = object_target(target).resource();
    let namespace = (!target.namespace.is_empty()).then_some(target.namespace.as_str());
    match cluster
        .find_object(&resource, namespace, &target.name)
        .await
    {
        Ok(found) => Ok(found.and_then(|o| serde_json::to_value(o).ok())),
        Err(error) => Err(read_problem(
            error,
            &what(target),
            Denied {
                verb: "get".to_owned(),
                group: resource.group.clone(),
                resource: target.plural.clone(),
                name: Some(target.name.clone()),
                namespace: namespace.map(str::to_owned),
            },
        )),
    }
}

fn refusal(refused: Refusal) -> ApiError {
    match refused {
        Refusal::Name => bad(
            "A token's name needs lower-case letters, digits and -, at most 63 characters, \
             starting and ending with a letter or digit.",
        ),
        Refusal::Seconds => bad(format!(
            "A token lasts from {} minutes to {} days.",
            api_token::MIN_SECONDS / 60,
            api_token::MAX_SECONDS / 86_400
        )),
    }
}

fn no_token(name: &str) -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        format!("There is no API token {name}."),
    ))
}

/// The token's ServiceAccount, which must exist and carry the token label.
async fn token_account(
    cluster: &dyn Cluster,
    project: &str,
    name: &str,
) -> Result<(Value, api_token::Token), ApiError> {
    let account = Addressed::service_account(project, name);
    let object = read_object(cluster, &account)
        .await?
        .ok_or_else(|| no_token(name))?;
    let token = api_token::read(&object).ok_or_else(|| no_token(name))?;
    Ok((object, token))
}

fn version_of(object: &Value, field: &str) -> String {
    object
        .pointer(&format!("/metadata/{field}"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Reads what `change` needs and works out its writes; writes nothing.
async fn compute(
    cluster: &dyn Cluster,
    project: &str,
    person: &str,
    change: &ApiTokenChange,
) -> Result<Computed, ApiError> {
    let mut computed = Computed::default();
    match change {
        ApiTokenChange::Create {
            name,
            role,
            cluster: wide,
            expiration_seconds,
        } => {
            let scope = if *wide {
                Scope::Cluster
            } else {
                Scope::Project
            };
            if *wide
                && !cluster
                    .review_access(&right("create", RBAC_GROUP, "clusterrolebindings", None))
                    .await
                    .unwrap_or(false)
            {
                return Err(refuse(
                    Kind::Forbidden,
                    "Only admins can make a token that reaches every namespace.",
                    "apiTokens.clusterAdmins",
                ));
            }
            let new = NewToken {
                namespace: project,
                name,
                role: role.cluster_role(),
                scope,
                created_by: person,
            };
            computed.steps = api_token::create(&new, *expiration_seconds).map_err(refusal)?;
            for step in &computed.steps {
                if let Step::Create { target, .. } = step {
                    if read_object(cluster, target).await?.is_some() {
                        return Err(refuse(
                            Kind::Conflict,
                            &format!("{} already exists.", what(target)),
                            "apiTokens.taken",
                        ));
                    }
                    computed.versions.insert(key(target), String::new());
                }
            }
            computed.account = Some(api_token::service_account(&new));
        }
        ApiTokenChange::Renew {
            name,
            expiration_seconds,
        } => {
            let (object, _) = token_account(cluster, project, name).await?;
            computed.steps =
                api_token::renew(project, name, *expiration_seconds).map_err(refusal)?;
            computed.versions.insert(
                format!("ServiceAccount/{project}/{name}"),
                version_of(&object, "resourceVersion"),
            );
            computed.account = Some(object);
        }
        ApiTokenChange::Delete { name } => {
            let (object, token) = token_account(cluster, project, name).await?;
            let target = Addressed::binding(project, name, token.scope);
            let binding = read_object(cluster, &target).await?;
            computed.versions.insert(
                key(&target),
                binding
                    .as_ref()
                    .map_or_else(String::new, |b| version_of(b, "uid")),
            );
            computed.versions.insert(
                format!("ServiceAccount/{project}/{name}"),
                version_of(&object, "uid"),
            );
            computed.steps = api_token::delete(&object, binding.as_ref());
        }
    }
    Ok(computed)
}

fn yaml(object: &Value) -> Option<String> {
    zeabur_graph::manifest::to_yaml(&zeabur_graph::manifest::clean(object)).ok()
}

fn shown(target: &Addressed, kind: &str, action: MemberWriteAction) -> MemberWrite {
    MemberWrite {
        api_version: target.api_version.clone(),
        kind: kind.to_owned(),
        namespace: (!target.namespace.is_empty()).then(|| target.namespace.clone()),
        name: target.name.clone(),
        action,
        before_yaml: None,
        after_yaml: None,
    }
}

/// The writes as the plan shows them. The expiry is the one kube-apiserver will answer.
fn writes_of(computed: &Computed) -> Vec<MemberWrite> {
    let mut writes = Vec::new();
    for step in &computed.steps {
        writes.push(match step {
            Step::Create { target, body } => MemberWrite {
                after_yaml: yaml(body),
                ..shown(target, &target.kind, MemberWriteAction::Create)
            },
            Step::Token { target, seconds } => MemberWrite {
                api_version: "authentication.k8s.io/v1".to_owned(),
                after_yaml: yaml(&api_token::token_request(*seconds)),
                ..shown(target, "TokenRequest", MemberWriteAction::Create)
            },
            Step::RecordExpiry { target } => {
                let before = computed.account.clone().unwrap_or_default();
                let mut after = before.clone();
                if let Some(annotations) = after
                    .pointer_mut("/metadata/annotations")
                    .and_then(Value::as_object_mut)
                {
                    annotations.insert(
                        api_token::EXPIRES_AT_ANNOTATION.to_owned(),
                        json!("<as kube-apiserver answers>"),
                    );
                }
                MemberWrite {
                    before_yaml: yaml(&before),
                    after_yaml: yaml(&after),
                    ..shown(target, &target.kind, MemberWriteAction::Patch)
                }
            }
            Step::Delete { target, before, .. } => MemberWrite {
                before_yaml: yaml(before),
                ..shown(target, &target.kind, MemberWriteAction::Delete)
            },
        });
    }
    writes
}

fn plan_key(state: &AppState) -> Result<std::sync::Arc<token::PlanKey>, ApiError> {
    state.plan_key.clone().ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a token key when it started, so it cannot plan changes.",
        ))
    })
}

/// Works out what a change to a project's API tokens writes, and a token to apply it; writes
/// nothing and issues no token.
#[utoipa::path(post, path = "/api/v1/projects/{project}/api-tokens/plan", tag = "projects",
    operation_id = "planApiTokenChange", request_body = ApiTokenChange,
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ApiTokenPlan),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(project): Path<String>,
    Json(change): Json<ApiTokenChange>,
) -> Result<Json<ApiTokenPlan>, ApiError> {
    check_project(&project)?;
    let sealing = plan_key(&state)?;
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let computed = compute(cluster.as_ref(), &project, &identity.username, &change).await?;
    let now = (state.clock)().as_second();
    let expires_at = now.saturating_add(LIFETIME_SECONDS);
    let writes = writes_of(&computed);
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        project,
        change,
        versions: computed.versions,
        expires_at,
    };
    let token = token::sign_as(&sealing, PREFIX, &claims).map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "The system random number generator failed, so Zeabur could not seal the plan.",
        ))
    })?;
    Ok(Json(ApiTokenPlan {
        token,
        expires_at: Timestamp::from_second(expires_at)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_string(),
        writes,
    }))
}

fn denied_of(target: &Addressed, verb: &str) -> Denied {
    let group = target
        .api_version
        .rsplit_once('/')
        .map_or("", |(group, _)| group);
    Denied {
        verb: verb.to_owned(),
        group: group.to_owned(),
        resource: target.plural.clone(),
        name: (verb != "create").then(|| target.name.clone()),
        namespace: (!target.namespace.is_empty()).then(|| target.namespace.clone()),
    }
}

/// Sends a creation or a deletion; a deletion of an object already gone is fine.
async fn send(cluster: &dyn Cluster, step: &Step, dry_run: bool) -> Result<(), ApiError> {
    let (target, body, verb) = match step {
        Step::Create { target, body } => (target, WriteBody::Create(body.clone()), "create"),
        Step::Delete { target, uid, .. } => (
            target,
            WriteBody::Delete(Preconditions {
                uid: uid.clone(),
                ..Preconditions::default()
            }),
            "delete",
        ),
        Step::Token { .. } | Step::RecordExpiry { .. } => return Ok(()),
    };
    let write = WriteRequest {
        target: object_target(target),
        body,
        dry_run,
    };
    match cluster.write(&write).await {
        Ok(_) => Ok(()),
        Err(error) if verb == "delete" && error.code() == Some(404) => Ok(()),
        Err(error) => Err(read_problem(error, &what(target), denied_of(target, verb))),
    }
}

async fn root_ca(cluster: &dyn Cluster, namespace: &str) -> Option<String> {
    let configmaps = ResourceRef::new("", "v1", "configmaps", "ConfigMap");
    let found = cluster
        .find_object(&configmaps, Some(namespace), ROOT_CA)
        .await
        .ok()??;
    found
        .data
        .pointer("/data/ca.crt")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Carries out an API token plan by its token: checks it, dry-runs every creation and deletion,
/// then writes, requests the token and records its expiry. The token is in the answer only.
#[utoipa::path(post, path = "/api/v1/projects/{project}/api-tokens/apply", tag = "projects",
    operation_id = "applyApiTokenChange", request_body = ApiTokenApply,
    params(("project" = String, Path, description = "The project's namespace")),
    responses((status = 200, body = ApiTokenApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(project): Path<String>,
    Json(request): Json<ApiTokenApply>,
) -> Result<Json<ApiTokenApplied>, ApiError> {
    let sealing = plan_key(&state)?;
    let claims: Claims = token::open_as(&sealing, PREFIX, &request.token).map_err(token_problem)?;
    if (state.clock)().as_second() >= claims.expires_at {
        return Err(token_problem(TokenError::Expired));
    }
    if claims.project != project {
        return Err(bad("This plan is for another project."));
    }
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    if (identity.username.as_str(), identity.groups.as_slice())
        != (claims.person.as_str(), claims.groups.as_slice())
    {
        return Err(ApiError::from(Problem::new(
            Kind::Forbidden,
            "Someone else made this plan, or you made it with other group memberships.",
        )));
    }
    let computed = compute(
        cluster.as_ref(),
        &project,
        &identity.username,
        &claims.change,
    )
    .await?;
    if computed.versions != claims.versions {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            "Someone changed this API token after the plan was made.",
        )));
    }
    let mut tried = BTreeSet::new();
    for step in &computed.steps {
        if let Step::Create { target, .. } | Step::Delete { target, .. } = step
            && tried.insert(key(target))
        {
            send(cluster.as_ref(), step, true).await?;
        }
    }
    let mut issued: Option<(Addressed, IssuedToken)> = None;
    let mut recorded = false;
    for step in &computed.steps {
        match step {
            Step::Token { target, seconds } => {
                let token = cluster
                    .request_token(&target.namespace, &target.name, *seconds)
                    .await
                    .map_err(|e| {
                        read_problem(
                            e,
                            &format!("a token for {}", what(target)),
                            Denied {
                                verb: "create".to_owned(),
                                group: String::new(),
                                resource: "serviceaccounts/token".to_owned(),
                                name: Some(target.name.clone()),
                                namespace: Some(target.namespace.clone()),
                            },
                        )
                    })?;
                issued = Some((target.clone(), token));
            }
            Step::RecordExpiry { target } => {
                let Some((_, token)) = &issued else { continue };
                let write = WriteRequest {
                    target: object_target(target),
                    body: WriteBody::JsonPatch(json!(api_token::expiry_patch(&token.expires_at))),
                    dry_run: false,
                };
                // The token is issued by now; not recording its expiry must not hide it.
                recorded = cluster.write(&write).await.is_ok();
            }
            Step::Create { .. } | Step::Delete { .. } => {
                send(cluster.as_ref(), step, false).await?;
            }
        }
    }
    let issued = match issued {
        Some((target, token)) => Some(IssuedTokenView {
            ca: root_ca(cluster.as_ref(), &target.namespace).await,
            name: target.name,
            namespace: target.namespace,
            token: token.token,
            expires_at: token.expires_at,
            recorded,
        }),
        None => None,
    };
    Ok(Json(ApiTokenApplied { issued }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_read_as_the_interface_sends_them() {
        let change: ApiTokenChange = serde_json::from_value(json!({
            "op": "create", "name": "ci", "role": "viewer", "expirationSeconds": 3600
        }))
        .unwrap();
        assert_eq!(
            change,
            ApiTokenChange::Create {
                name: "ci".to_owned(),
                role: ProjectRole::Viewer,
                cluster: false,
                expiration_seconds: 3600,
            }
        );
    }

    #[test]
    fn the_token_request_is_shown_in_the_plan_and_no_token_is() {
        let new = NewToken {
            namespace: "shop",
            name: "ci",
            role: "zeabur-project-viewer",
            scope: Scope::Project,
            created_by: "zeabur:ada",
        };
        let computed = Computed {
            steps: api_token::create(&new, 3600).unwrap(),
            versions: BTreeMap::new(),
            account: Some(api_token::service_account(&new)),
        };
        let writes = writes_of(&computed);
        let kinds: Vec<&str> = writes.iter().map(|w| w.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "ServiceAccount",
                "RoleBinding",
                "TokenRequest",
                "ServiceAccount"
            ]
        );
        assert!(
            writes[2]
                .after_yaml
                .as_deref()
                .unwrap()
                .contains("expirationSeconds: 3600")
        );
        assert_eq!(writes[3].action, MemberWriteAction::Patch);
    }
}
