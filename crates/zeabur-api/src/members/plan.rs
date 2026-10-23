//! Members' changes as plans: inviting someone, changing what a member may do and removing one
//! are planned first and applied by a token, as a project's changes are (see
//! `projects::writes`).
//!
//! The plan lists every object the change writes, in the order it writes them: the `User`, the
//! `ClusterRoleBinding` and `RoleBinding`s of [`super::access`], and the member's invitations,
//! credentials and grants when they go. The token binds the person, the change, and the
//! `resourceVersion` or `uid` of every object read (or that it was absent); apply computes the
//! writes again from the cluster as it is, refuses when any of those moved, dry-runs every write,
//! then writes. An invitation's link is made only then, so its token is never in a plan: the plan
//! says that a link will be made and how long it will work.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ObjectTarget, Preconditions, ResourceRef, WriteBody, WriteRequest};

use super::access::{self, Binding, ProjectGrant, RBAC_GROUP, Step};
use super::directory::{IdentityKind, Object, UserSpec, request, setup_links};
use super::handlers::{directory, issuer_of, you};
use super::writes::{
    INVITE_SECONDS, SetupLinkView, admin_patch, bad, is_username, issue, new_user, refuse,
    user_object,
};
use super::{ADMIN_GROUP, USER_PREFIX};
use crate::error::{ApiError, read_problem};
use crate::handlers::{AppState, Everyday, Person};
use crate::plan_token::{self as token, LIFETIME_SECONDS, PlanKey, TokenError};
use crate::projects::writes::token_problem;
use crate::settings::IdentitySettings;

/// Sealed members' plans start with this, so a project's token never opens as one, nor the
/// other way round.
const PREFIX: &str = "m1.";

/// A change to the members, as a person asks for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum MemberChange {
    /// Invite someone: a one-time link that adds a passkey to their account, the member made
    /// first when new. An existing member keeps their name and access.
    #[serde(rename_all = "camelCase")]
    Invite {
        /// Login name: lower-case letters, digits, `-` and `.`, at most 63 characters.
        username: String,
        /// Name shown in the interface, for a new member.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        /// Whether a new member may take on admin rights.
        #[serde(default)]
        admin: bool,
        /// A new member's roles in projects, when they are not an admin.
        #[serde(default)]
        projects: Vec<ProjectGrant>,
    },
    /// Change what a member may do: admin rights, or roles in chosen projects (a project left out
    /// loses theirs). Nobody takes their own admin rights away.
    #[serde(rename_all = "camelCase")]
    Access {
        /// The member.
        username: String,
        /// May take on admin rights, which cover every project.
        admin: bool,
        /// Roles in projects; empty for an admin.
        #[serde(default)]
        projects: Vec<ProjectGrant>,
    },
    /// Remove a member: the role bindings Zeabur made for them, their links, passkeys, linked
    /// accounts and sessions, then the member. Nobody removes themselves.
    #[serde(rename_all = "camelCase")]
    Remove {
        /// The member.
        username: String,
    },
}

/// How a planned write changes its object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MemberWriteAction {
    /// A new object.
    Create,
    /// A JSON Patch that tests the `uid` and `resourceVersion` it was planned against.
    Patch,
    /// A `DELETE` that holds only while the object has the `uid` it was planned against.
    Delete,
}

/// One object a member plan writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberWrite {
    /// `apiVersion`.
    pub api_version: String,
    /// The kind.
    pub kind: String,
    /// Its namespace; absent for a cluster-scoped object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Its name; for the invitation, the prefix its random name starts with.
    pub name: String,
    /// The write.
    pub action: MemberWriteAction,
    /// The object as it is, YAML; absent when it is created or not shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_yaml: Option<String>,
    /// The object as it will be, YAML; absent when it is deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_yaml: Option<String>,
}

/// A member plan and the token that applies it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberPlan {
    /// Send this to `apply` to carry out exactly this plan.
    pub token: String,
    /// When the token stops working, RFC 3339.
    pub expires_at: String,
    /// The writes, in order.
    pub writes: Vec<MemberWrite>,
    /// For an invitation: how long its link will work once made, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_seconds: Option<i64>,
    /// For an invitation: whether it makes a new member.
    pub new_member: bool,
}

/// The token of a member plan to apply.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct MemberApplyRequest {
    /// The token `plan` returned.
    pub token: String,
}

/// What an applied member plan made.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberApplied {
    /// For an invitation: the link, shown once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<SetupLinkView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    person: String,
    groups: Vec<String>,
    change: MemberChange,
    versions: BTreeMap<String, String>,
    expires_at: i64,
}

/// One write, before it is sent.
#[derive(Debug, Clone)]
enum Op {
    Create {
        target: ObjectTarget,
        body: Value,
    },
    Patch {
        target: ObjectTarget,
        operations: Value,
        before: Value,
        after: Value,
    },
    Delete {
        target: ObjectTarget,
        uid: Option<String>,
        /// Shown in the plan; `None` for objects whose contents are not shown (credentials).
        before: Option<Value>,
    },
    /// The invitation, made last with a fresh token for the member as stored by then.
    Invite,
}

/// The writes a change needs and what they were computed against.
#[derive(Debug, Clone, Default)]
struct Computed {
    ops: Vec<Op>,
    versions: BTreeMap<String, String>,
    link_seconds: Option<i64>,
    new_member: bool,
}

fn key(target: &ObjectTarget) -> String {
    format!("{}/{}/{}", target.kind, target.namespace, target.name)
}

/// The roles asked for, checked against the request and what the chart installed.
pub(crate) fn wanted_projects(
    admin: bool,
    projects: &[ProjectGrant],
    offered: bool,
) -> Result<Vec<ProjectGrant>, ApiError> {
    let projects = access::check(projects).map_err(bad)?;
    if admin && !projects.is_empty() {
        return Err(bad(
            "An admin's rights cover every project: leave the projects out.",
        ));
    }
    if !offered && !projects.is_empty() {
        return Err(refuse(
            "Zeabur was installed without the member roles, so it cannot give roles in projects.",
            "Install the chart with memberRoles.enabled=true, or make the member an admin.",
            "members.noProjectRoles",
        ));
    }
    Ok(projects)
}

fn no_member(username: &str) -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        format!("There is no member {username}."),
    ))
}

impl Computed {
    fn create(&mut self, target: ObjectTarget, body: Value) {
        self.versions.entry(key(&target)).or_default();
        self.ops.push(Op::Create { target, body });
    }

    fn delete(&mut self, target: ObjectTarget, uid: Option<String>, before: Option<Value>) {
        self.versions
            .insert(key(&target), uid.clone().unwrap_or_default());
        self.ops.push(Op::Delete {
            target,
            uid,
            before,
        });
    }

    /// The baseline binding, created when it is not there yet.
    fn baseline(&mut self, username: &str, held: Option<&Value>) {
        if held.is_none() {
            self.create(
                access::target("ClusterRoleBinding", "", &access::binding_name(username)),
                access::baseline_binding(username),
            );
        }
    }

    /// The project bindings that take `current` to `wanted`, deletions first.
    fn bindings(&mut self, username: &str, current: &[Binding], wanted: &[ProjectGrant]) {
        let name = access::binding_name(username);
        for step in access::steps(username, current, wanted) {
            match step {
                Step::Delete(binding) => {
                    let before = binding
                        .role
                        .map(|role| access::role_binding(&binding.project, username, role));
                    self.delete(
                        access::target("RoleBinding", &binding.project, &name),
                        binding.uid,
                        before,
                    );
                }
                Step::Create(grant) => self.create(
                    access::target("RoleBinding", &grant.project, &name),
                    access::role_binding(&grant.project, username, grant.role),
                ),
            }
        }
    }

    fn patch_admin(
        &mut self,
        namespace: &str,
        user: &Value,
        typed: &Object<UserSpec>,
        admin: bool,
    ) {
        let Some(operations) = admin_patch(typed, admin) else {
            return;
        };
        let target = super::writes::target(IdentityKind::User, namespace, typed.name());
        let mut after = user.clone();
        for op in operations.as_array().into_iter().flatten() {
            if op["op"] == "add"
                && let Some(field) = op["path"].as_str().and_then(|p| p.strip_prefix("/spec/"))
                && let Some(spec) = after.get_mut("spec").and_then(Value::as_object_mut)
            {
                spec.insert(field.to_owned(), op["value"].clone());
            }
        }
        self.versions.insert(
            key(&target),
            typed.metadata.resource_version.clone().unwrap_or_default(),
        );
        self.ops.push(Op::Patch {
            target,
            operations,
            before: user.clone(),
            after,
        });
    }
}

/// The member's baseline `ClusterRoleBinding`, if Zeabur made one.
async fn baseline_of(cluster: &dyn Cluster, username: &str) -> Result<Option<Value>, ApiError> {
    let resource = ResourceRef::new(
        RBAC_GROUP,
        "v1",
        "clusterrolebindings",
        "ClusterRoleBinding",
    );
    let name = access::binding_name(username);
    match cluster.get_object(&resource, None, &name).await {
        Ok(found) => Ok(found.and_then(|o| serde_json::to_value(o).ok())),
        Err(error) if error.code() == Some(404) => Ok(None),
        Err(error) => Err(read_problem(
            error,
            &format!("ClusterRoleBinding {name}"),
            Denied {
                verb: "get".to_owned(),
                group: RBAC_GROUP.to_owned(),
                resource: "clusterrolebindings".to_owned(),
                name: Some(name.clone()),
                namespace: None,
            },
        )),
    }
}

fn typed_user(user: &Value) -> Result<Object<UserSpec>, ApiError> {
    serde_json::from_value(user.clone()).map_err(|e| {
        ApiError::from(Problem::new(
            Kind::Internal,
            format!("The member's User could not be read: {e}."),
        ))
    })
}

fn is_admin(user: &Object<UserSpec>) -> bool {
    user.spec.elevated_groups.iter().any(|g| g == ADMIN_GROUP)
}

/// Reads what `change` needs and works out its writes; writes nothing.
async fn compute(
    state: &AppState,
    cluster: &dyn Cluster,
    change: &MemberChange,
) -> Result<Computed, ApiError> {
    let identity = &state.settings.identity;
    match change {
        MemberChange::Invite {
            username,
            display_name,
            admin,
            projects,
        } => {
            let projects = wanted_projects(*admin, projects, identity.project_roles)?;
            invite(
                identity,
                cluster,
                username.trim(),
                display_name.as_deref(),
                *admin,
                &projects,
            )
            .await
        }
        MemberChange::Access {
            username,
            admin,
            projects,
        } => {
            let projects = wanted_projects(*admin, projects, identity.project_roles)?;
            access_change(identity, cluster, username, *admin, &projects).await
        }
        MemberChange::Remove { username } => remove(identity, cluster, username).await,
    }
}

async fn invite(
    identity: &IdentitySettings,
    cluster: &dyn Cluster,
    username: &str,
    display_name: Option<&str>,
    admin: bool,
    projects: &[ProjectGrant],
) -> Result<Computed, ApiError> {
    if !is_username(username) {
        return Err(bad(format!(
            "{username:?} cannot be a user name: use lower-case letters, digits, - and ., at \
             most 63 characters."
        )));
    }
    issuer_of(&setup_links(cluster, identity).await)?;
    let namespace = identity.namespace.as_str();
    let mut computed = Computed::default();
    let user_target = super::writes::target(IdentityKind::User, namespace, username);
    if let Some(user) = user_object(cluster, namespace, username).await? {
        let version = user
            .pointer("/metadata/resourceVersion")
            .and_then(Value::as_str)
            .unwrap_or_default();
        computed
            .versions
            .insert(key(&user_target), version.to_owned());
    } else {
        computed.new_member = true;
        computed.create(
            user_target,
            new_user(namespace, username, display_name, admin),
        );
        if identity.project_roles {
            let current = access::bindings_for(cluster, username).await?;
            let held = baseline_of(cluster, username).await?;
            computed.baseline(username, held.as_ref());
            computed.bindings(username, &current, projects);
        }
    }
    computed.ops.push(Op::Invite);
    computed.link_seconds = Some(INVITE_SECONDS);
    Ok(computed)
}

async fn access_change(
    identity: &IdentitySettings,
    cluster: &dyn Cluster,
    username: &str,
    admin: bool,
    projects: &[ProjectGrant],
) -> Result<Computed, ApiError> {
    let namespace = identity.namespace.as_str();
    let me = you(cluster).await;
    let Some(user) = user_object(cluster, namespace, username).await? else {
        return Err(no_member(username));
    };
    let typed = typed_user(&user)?;
    if is_admin(&typed) && !admin && me == format!("{USER_PREFIX}{username}") {
        return Err(refuse(
            "You cannot take away your own admin rights.",
            "Ask another admin to change your access.",
            "members.demoteSelf",
        ));
    }
    let mut computed = Computed::default();
    // What is taken away goes first, so a failure half-way never leaves more than was asked.
    if !admin {
        computed.patch_admin(namespace, &user, &typed, false);
    }
    if identity.project_roles {
        let current = access::bindings_for(cluster, username).await?;
        let held = baseline_of(cluster, username).await?;
        computed.baseline(username, held.as_ref());
        computed.bindings(username, &current, projects);
    }
    if admin {
        computed.patch_admin(namespace, &user, &typed, true);
    }
    Ok(computed)
}

async fn remove(
    identity: &IdentitySettings,
    cluster: &dyn Cluster,
    username: &str,
) -> Result<Computed, ApiError> {
    let namespace = identity.namespace.as_str();
    let me = you(cluster).await;
    if me == format!("{USER_PREFIX}{username}") {
        return Err(refuse(
            "You cannot remove yourself.",
            "Ask another admin to remove you.",
            "members.removeSelf",
        ));
    }
    let directory = directory(cluster, namespace).await?;
    let Some(user) = directory.user(username) else {
        return Err(no_member(username));
    };
    let mut computed = Computed::default();
    let current = access::bindings_for(cluster, username).await?;
    computed.bindings(username, &current, &[]);
    if let Some(held) = baseline_of(cluster, username).await? {
        let uid = held
            .pointer("/metadata/uid")
            .and_then(Value::as_str)
            .map(str::to_owned);
        computed.delete(
            access::target("ClusterRoleBinding", "", &access::binding_name(username)),
            uid,
            Some(held),
        );
    }
    let invitations = directory
        .invitations
        .iter()
        .filter(|i| i.spec.user == username)
        .map(|i| (IdentityKind::Invitation, &i.metadata));
    let credentials = directory
        .credentials
        .iter()
        .filter(|c| c.spec.user == username)
        .map(|c| (IdentityKind::Credential, &c.metadata));
    let grants = directory
        .grants
        .iter()
        .filter(|g| g.spec.user == username)
        .map(|g| (IdentityKind::Grant, &g.metadata));
    for (kind, metadata) in invitations.chain(credentials).chain(grants) {
        let name = metadata.name.as_deref().unwrap_or_default();
        computed.delete(
            super::writes::target(kind, namespace, name),
            metadata.uid.clone(),
            None,
        );
    }
    let before = user_object(cluster, namespace, username).await?;
    computed.delete(
        super::writes::target(IdentityKind::User, namespace, user.name()),
        user.metadata.uid.clone(),
        before,
    );
    Ok(computed)
}

fn yaml(object: &Value) -> Option<String> {
    zeabur_graph::manifest::to_yaml(&zeabur_graph::manifest::clean(object)).ok()
}

fn shown(target: &ObjectTarget, action: MemberWriteAction) -> MemberWrite {
    MemberWrite {
        api_version: target.api_version.clone(),
        kind: target.kind.clone(),
        namespace: (!target.namespace.is_empty()).then(|| target.namespace.clone()),
        name: target.name.clone(),
        action,
        before_yaml: None,
        after_yaml: None,
    }
}

/// The writes as the plan shows them. The invitation's name and token hash are made at apply.
fn writes_of(computed: &Computed, namespace: &str, username: &str, now: i64) -> Vec<MemberWrite> {
    computed
        .ops
        .iter()
        .map(|op| match op {
            Op::Create { target, body } => MemberWrite {
                after_yaml: yaml(body),
                ..shown(target, MemberWriteAction::Create)
            },
            Op::Patch {
                target,
                before,
                after,
                ..
            } => MemberWrite {
                before_yaml: yaml(before),
                after_yaml: yaml(after),
                ..shown(target, MemberWriteAction::Patch)
            },
            Op::Delete { target, before, .. } => MemberWrite {
                before_yaml: before.as_ref().and_then(yaml),
                ..shown(target, MemberWriteAction::Delete)
            },
            Op::Invite => {
                let target = super::writes::target(IdentityKind::Invitation, namespace, "inv-");
                let body = json!({
                    "apiVersion": target.api_version,
                    "kind": "Invitation",
                    "metadata": {"generateName": "inv-", "namespace": namespace},
                    "spec": {
                        "user": username,
                        "tokenHash": "sha256:…",
                        "expiresAt": now.saturating_add(computed.link_seconds.unwrap_or(INVITE_SECONDS)),
                    },
                });
                MemberWrite {
                    after_yaml: yaml(&body),
                    ..shown(&target, MemberWriteAction::Create)
                }
            }
        })
        .collect()
}

fn username_of(change: &MemberChange) -> &str {
    match change {
        MemberChange::Invite { username, .. }
        | MemberChange::Access { username, .. }
        | MemberChange::Remove { username } => username.trim(),
    }
}

fn plan_key(state: &AppState) -> Result<std::sync::Arc<PlanKey>, ApiError> {
    state.plan_key.clone().ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a token key when it started, so it cannot plan changes.",
        ))
    })
}

/// Works out the objects a change to the members writes, and a token to apply it; writes nothing.
#[utoipa::path(post, path = "/api/v1/members/plan", tag = "members",
    operation_id = "planMemberChange", request_body = MemberChange,
    responses((status = 200, body = MemberPlan),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(change): Json<MemberChange>,
) -> Result<Json<MemberPlan>, ApiError> {
    let sealing = plan_key(&state)?;
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let computed = compute(&state, cluster.as_ref(), &change).await?;
    let namespace = state.settings.identity.namespace.as_str();
    let now = (state.clock)().as_second();
    let writes = writes_of(&computed, namespace, username_of(&change), now);
    let expires_at = now.saturating_add(LIFETIME_SECONDS);
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
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
    Ok(Json(MemberPlan {
        token,
        expires_at: Timestamp::from_second(expires_at)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_string(),
        writes,
        link_seconds: computed.link_seconds,
        new_member: computed.new_member,
    }))
}

fn denied_of(target: &ObjectTarget, verb: &str) -> Denied {
    let group = target
        .api_version
        .rsplit_once('/')
        .map_or("", |(group, _)| group);
    if group == super::GROUP {
        let kind = match target.kind.as_str() {
            "User" => IdentityKind::User,
            "Credential" => IdentityKind::Credential,
            "Grant" => IdentityKind::Grant,
            _ => IdentityKind::Invitation,
        };
        let name = (verb != "create").then_some(target.name.as_str());
        return request(verb, kind, &target.namespace, name);
    }
    Denied {
        verb: verb.to_owned(),
        group: group.to_owned(),
        resource: target.plural.clone(),
        name: (verb != "create").then(|| target.name.clone()),
        namespace: (!target.namespace.is_empty()).then(|| target.namespace.clone()),
    }
}

const fn op_target(op: &Op) -> Option<&ObjectTarget> {
    match op {
        Op::Create { target, .. } | Op::Patch { target, .. } | Op::Delete { target, .. } => {
            Some(target)
        }
        Op::Invite => None,
    }
}

/// Sends one write; a deletion of an object already gone is fine.
async fn send(cluster: &dyn Cluster, op: &Op, dry_run: bool) -> Result<(), ApiError> {
    let (target, body, verb) = match op {
        Op::Create { target, body } => (target, WriteBody::Create(body.clone()), "create"),
        Op::Patch {
            target, operations, ..
        } => (target, WriteBody::JsonPatch(operations.clone()), "patch"),
        Op::Delete { target, uid, .. } => (
            target,
            WriteBody::Delete(Preconditions {
                uid: uid.clone(),
                resource_version: None,
                grace_period_seconds: None,
            }),
            "delete",
        ),
        Op::Invite => return Ok(()),
    };
    let write = WriteRequest {
        target: target.clone(),
        body,
        dry_run,
    };
    match cluster.write(&write).await {
        Ok(_) => Ok(()),
        Err(error) if verb == "delete" && error.code() == Some(404) => Ok(()),
        Err(error) => {
            let what = if target.namespace.is_empty() {
                format!("{} {}", target.kind, target.name)
            } else {
                format!("{} {} in {}", target.kind, target.name, target.namespace)
            };
            Err(read_problem(error, &what, denied_of(target, verb)))
        }
    }
}

/// Carries out a member plan by its token: checks it, dry-runs every write, then writes, and
/// makes the invitation's link last.
#[utoipa::path(post, path = "/api/v1/members/apply", tag = "members",
    operation_id = "applyMemberChange", request_body = MemberApplyRequest,
    responses((status = 200, body = MemberApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(request): Json<MemberApplyRequest>,
) -> Result<Json<MemberApplied>, ApiError> {
    let sealing = plan_key(&state)?;
    let claims: Claims = token::open_as(&sealing, PREFIX, &request.token).map_err(token_problem)?;
    if (state.clock)().as_second() >= claims.expires_at {
        return Err(token_problem(TokenError::Expired));
    }
    let identity = everyday
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
    let computed = compute(&state, cluster.as_ref(), &claims.change).await?;
    if computed.versions != claims.versions {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            "Someone changed this member after the plan was made.",
        )));
    }
    // A binding whose role changes is deleted and created again; its creation cannot be tried
    // before the deletion is real, so only each object's first write is tried.
    let mut tried = std::collections::BTreeSet::new();
    for op in &computed.ops {
        if op_target(op).is_none_or(|target| tried.insert(key(target))) {
            send(cluster.as_ref(), op, true).await?;
        }
    }
    let namespace = state.settings.identity.namespace.as_str();
    let mut link = None;
    for op in &computed.ops {
        if !matches!(op, Op::Invite) {
            send(cluster.as_ref(), op, false).await?;
            continue;
        }
        let username = username_of(&claims.change);
        let issuer = issuer_of(&setup_links(cluster.as_ref(), &state.settings.identity).await)?;
        let user = user_object(cluster.as_ref(), namespace, username)
            .await?
            .ok_or_else(|| bad("The new member could not be read back."))?;
        let seconds = computed.link_seconds.unwrap_or(INVITE_SECONDS);
        let (url, name, expires_at) = issue(
            cluster.as_ref(),
            namespace,
            &issuer,
            &user,
            (state.clock)(),
            seconds,
        )
        .await?;
        link = Some(SetupLinkView::new(
            url,
            username.to_owned(),
            expires_at,
            name,
            computed.new_member,
            false,
        ));
    }
    Ok(Json(MemberApplied { link }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::members::access::{BASELINE_ROLE, ProjectRole};

    fn binding(project: &str, role: ProjectRole) -> Binding {
        Binding {
            username: "bob".to_owned(),
            project: project.to_owned(),
            role: Some(role),
            uid: Some(format!("uid-{project}")),
        }
    }

    fn grant(project: &str, role: ProjectRole) -> ProjectGrant {
        ProjectGrant {
            project: project.to_owned(),
            role,
        }
    }

    fn listed(computed: &Computed) -> Vec<String> {
        computed
            .ops
            .iter()
            .map(|op| match op {
                Op::Create { target, .. } => format!("create {}", key(target)),
                Op::Patch { target, .. } => format!("patch {}", key(target)),
                Op::Delete { target, .. } => format!("delete {}", key(target)),
                Op::Invite => "invite".to_owned(),
            })
            .collect()
    }

    #[test]
    fn a_role_change_deletes_before_it_creates_and_pins_what_it_deletes() {
        let mut computed = Computed::default();
        computed.baseline("bob", None);
        computed.bindings(
            "bob",
            &[
                binding("shop", ProjectRole::Viewer),
                binding("blog", ProjectRole::Developer),
            ],
            &[
                grant("shop", ProjectRole::Developer),
                grant("blog", ProjectRole::Developer),
            ],
        );
        assert_eq!(
            listed(&computed),
            [
                "create ClusterRoleBinding//zeabur-member-bob",
                "delete RoleBinding/shop/zeabur-member-bob",
                "create RoleBinding/shop/zeabur-member-bob",
            ]
        );
        assert_eq!(
            computed.versions["RoleBinding/shop/zeabur-member-bob"], "uid-shop",
            "a binding replaced is pinned by the uid of the one deleted"
        );
        let writes = writes_of(&computed, "zeabur-identity", "bob", 0);
        assert_eq!(writes[1].action, MemberWriteAction::Delete);
        assert!(
            writes[1]
                .before_yaml
                .as_deref()
                .unwrap()
                .contains("zeabur-project-viewer")
        );
        assert!(
            writes[2]
                .after_yaml
                .as_deref()
                .unwrap()
                .contains("zeabur-project-developer")
        );
        assert_eq!(writes[0].namespace, None);
    }

    #[test]
    fn a_held_baseline_is_not_created_again() {
        let mut computed = Computed::default();
        computed.baseline("bob", Some(&json!({})));
        assert!(computed.ops.is_empty());
    }

    #[test]
    fn making_an_admin_patches_only_the_groups_and_shows_both_sides() {
        let user = json!({
            "apiVersion": "identity.zeabur.com/v1alpha1", "kind": "User",
            "metadata": {"name": "bob", "namespace": "ns", "uid": "u", "resourceVersion": "7"},
            "spec": {"username": "bob", "groups": [], "elevatedGroups": []}
        });
        let typed = typed_user(&user).unwrap();
        let mut computed = Computed::default();
        computed.patch_admin("ns", &user, &typed, true);
        computed.patch_admin("ns", &user, &typed, false);
        assert_eq!(listed(&computed), ["patch User/ns/bob"]);
        assert_eq!(computed.versions["User/ns/bob"], "7");
        let writes = writes_of(&computed, "ns", "bob", 0);
        let after = writes[0].after_yaml.as_deref().unwrap();
        assert!(after.contains("admins"), "{after}");
        assert!(!writes[0].before_yaml.as_deref().unwrap().contains("admins"));
    }

    #[test]
    fn an_invitation_shows_no_token_and_a_plan_token_opens_only_as_a_members_plan() {
        let mut computed = Computed {
            link_seconds: Some(INVITE_SECONDS),
            ..Computed::default()
        };
        computed.ops.push(Op::Invite);
        let writes = writes_of(&computed, "zeabur-identity", "cy", 0);
        assert_eq!(writes[0].kind, "Invitation");
        assert!(
            writes[0]
                .after_yaml
                .as_deref()
                .unwrap()
                .contains("sha256:…")
        );
        let key = PlanKey::random().unwrap();
        let claims = Claims {
            person: "zeabur:ada".to_owned(),
            groups: Vec::new(),
            change: MemberChange::Remove {
                username: "bob".to_owned(),
            },
            versions: BTreeMap::new(),
            expires_at: 10,
        };
        let sealed = token::sign_as(&key, PREFIX, &claims).unwrap();
        assert_eq!(
            token::open_as::<Claims>(&key, PREFIX, &sealed).unwrap(),
            claims
        );
        assert!(token::verify(&key, &sealed, 0).is_err());
    }

    #[test]
    fn changes_read_as_the_interface_sends_them() {
        let change: MemberChange = serde_json::from_value(json!({
            "op": "access", "username": "bob", "admin": false,
            "projects": [{"project": "shop", "role": "viewer"}]
        }))
        .unwrap();
        assert_eq!(
            change,
            MemberChange::Access {
                username: "bob".to_owned(),
                admin: false,
                projects: vec![grant("shop", ProjectRole::Viewer)],
            }
        );
        assert_eq!(BASELINE_ROLE, "zeabur-member");
    }
}
