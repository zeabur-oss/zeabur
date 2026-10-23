//! Node shells: a root shell on a node, for cluster administrators, as `kubectl debug node/<node>
//! -it --profile=sysadmin` and `nsenter` give one (see [`zeabur_plan::node_shell`]).
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /api/v1/cluster/node-shells` | the node shell pods, oldest first |
//! | `POST /api/v1/cluster/node-shells/plan` | what opening a shell on a node, or removing a shell pod, writes, and a token; writes nothing |
//! | `POST /api/v1/cluster/node-shells/apply` | carries out that plan by its token |
//! | `GET /api/v1/cluster/node-shells/{pod}/terminal` | the shell, over a WebSocket, as `pods/exec` (see `pods::exec`) |
//!
//! Everything is read and written as the person; Zeabur grants nothing. Every route asks
//! kube-apiserver first whether the person may do everything everywhere (a
//! `SelfSubjectAccessReview` of `*` on `*`): anyone else is refused, with admin rights offered
//! where they are not in use. Opening writes the namespace (created, or labelled to let privileged
//! pods in, when it has to be), the pod, and an Event about the node that names who opened it.
//! A plan is refused while an RBAC binding lets anyone but cluster administrators create pods in
//! the namespace or run commands in them ([`zeabur_plan::node_shell::exposures`]), and lists those
//! bindings.
//!
//! The terminal opens only for the person who opened the shell (the pod's person label), after a
//! review of `create` on `pods/exec`. When it closes, Zeabur deletes the pod at once (grace period
//! `0`); the pod also ends on its own once its last session is gone. Removing a pod is a plan of
//! its own, for shells left over.

use axum::Json;
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{
    Cluster, ClusterExt as _, DynamicObject, ExecRequest, ExecSession, Identity, ObjectTarget,
    Preconditions, ResourceRef, WriteBody, WriteRequest,
};
use zeabur_plan::node_shell::{self as shell, NAMESPACE, NodeShell, ShellState};

use crate::error::{ApiError, get_read, list_read, offer_elevation, read_problem, review_refusal};
use crate::handlers::{AppState, Everyday, Person};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::pods::exec::{self, ServerFrame, Terminal};
use crate::projects::writes::token_problem;

/// Sealed plans of this module start with this, so no other plan's token opens as one.
const PREFIX: &str = "s1.";
const RBAC_GROUP: &str = "rbac.authorization.k8s.io";

/// How a node shell pod is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ShellPhase {
    /// Not running yet: the image is pulled, or the container is created.
    Starting,
    /// Running: a terminal can open in it.
    Running,
    /// Stopped: its session ended, its time ran out, or it is being deleted.
    Ended,
}

/// A node shell pod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeShellView {
    /// The pod.
    pub name: String,
    /// The node it runs on.
    pub node: String,
    /// Who opened it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    /// Whether the person asking opened it, so may enter it.
    pub mine: bool,
    /// When it was created, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// When its deadline stops it, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stops_at: Option<String>,
    /// How it is doing.
    pub phase: ShellPhase,
    /// The kubelet's reason: `ErrImagePull` while starting, `DeadlineExceeded` or `Completed`
    /// when ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The kubelet's message while starting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// The node shell pods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct NodeShells {
    /// Oldest first.
    pub shells: Vec<NodeShellView>,
}

/// What to plan.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum NodeShellPlanRequest {
    /// Open a shell on `node`.
    Open {
        /// The node.
        node: String,
    },
    /// Remove the shell pod `pod`.
    Remove {
        /// The pod.
        pod: String,
    },
}

/// What a write does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ShellWriteAction {
    /// Creates the object.
    Create,
    /// Changes it.
    Patch,
    /// Deletes it.
    Delete,
}

/// One object a plan writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShellWrite {
    /// What it does.
    pub action: ShellWriteAction,
    /// The object's kind.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// Its namespace, for a namespaced object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// The object as it will be (as it is, for a deletion), YAML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaml: Option<String>,
}

/// Someone a binding lets into node shells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShellSubject {
    /// `User`, `Group` or `ServiceAccount`.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// A service account's namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

/// A binding that lets someone other than cluster administrators create pods in the node shell
/// namespace or run commands in them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShellExposure {
    /// `RoleBinding` or `ClusterRoleBinding`.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// The role it binds, `Role/name` or `ClusterRole/name`.
    pub role: String,
    /// What the role grants: `create pods`, `create pods/exec`, `get pods/exec`.
    pub rights: Vec<String>,
    /// Who it lets in.
    pub subjects: Vec<ShellSubject>,
}

/// A plan that opens a shell on a node or removes a shell pod.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeShellPlan {
    /// Send this to `apply` to carry out exactly this plan; absent while `exposed` lists
    /// bindings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// When the token stops working, RFC 3339.
    pub expires_at: String,
    /// `open` or `remove`.
    pub op: String,
    /// The node.
    pub node: String,
    /// The pod.
    pub pod: String,
    /// Its namespace.
    pub namespace: String,
    /// Who opens it (opened it, for a removal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    /// The image it runs.
    pub image: String,
    /// How long it runs at most, in seconds.
    pub deadline_seconds: i64,
    /// How long it waits for its first session, in seconds.
    pub first_session_seconds: u32,
    /// The writes, in order.
    pub writes: Vec<ShellWrite>,
    /// The `kubectl` command that opens the same shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kubectl: Option<String>,
    /// Bindings that let others into node shells; the plan cannot be applied while there is one.
    pub exposed: Vec<ShellExposure>,
}

/// The token of a plan to apply.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct NodeShellApply {
    /// The token `plan` returned.
    pub token: String,
}

/// What an apply did.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NodeShellApplied {
    /// The pod.
    pub pod: String,
    /// Its namespace.
    pub namespace: String,
    /// Its node.
    pub node: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    person: String,
    groups: Vec<String>,
    op: String,
    node: String,
    pod: String,
    made_at: String,
    image: String,
    /// The node's uid (open) or the pod's (remove).
    uid: Option<String>,
    /// The namespace's `resourceVersion` when it existed.
    namespace_version: Option<String>,
    expires_at: i64,
}

fn pods() -> ResourceRef {
    ResourceRef::new("", "v1", "pods", "Pod")
}

fn value(object: DynamicObject) -> Value {
    serde_json::to_value(object).unwrap_or(Value::Null)
}

fn text(object: &Value, path: &str) -> Option<String> {
    object
        .pointer(path)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn yaml(object: &Value) -> Option<String> {
    zeabur_graph::manifest::to_yaml(&zeabur_graph::manifest::clean(object)).ok()
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

/// Refused unless kube-apiserver says the person may do everything everywhere.
pub(crate) async fn require_admin(cluster: &dyn Cluster, what: &str) -> Result<(), ApiError> {
    if crate::account::cluster_admin(cluster).await {
        return Ok(());
    }
    let mut refusal = review_refusal(what, right("*", "*", "*", None));
    format!(
        "Only cluster administrators can use {what}: it is root on the node, outside Zeabur and \
         Kubernetes."
    )
    .clone_into(&mut refusal.0.detail);
    Err(refusal)
}

fn view(found: shell::ShellPod, person_key: &str, pods: &[Value]) -> NodeShellView {
    let mine = pods
        .iter()
        .find(|p| text(p, "/metadata/name").as_deref() == Some(found.name.as_str()))
        .and_then(|p| {
            text(
                p,
                &format!(
                    "/metadata/labels/{}",
                    shell::PERSON_LABEL.replace('/', "~1")
                ),
            )
        })
        .is_some_and(|key| key == person_key);
    let (phase, reason, message) = match found.state {
        ShellState::Starting { reason, message } => (ShellPhase::Starting, reason, message),
        ShellState::Running => (ShellPhase::Running, None, None),
        ShellState::Ended { reason } => (ShellPhase::Ended, reason, None),
    };
    NodeShellView {
        name: found.name,
        node: found.node,
        person: found.person,
        mine,
        created_at: found.created_at,
        stops_at: found.stops_at,
        phase,
        reason,
        message,
    }
}

async fn shell_pods(cluster: &dyn Cluster) -> Result<Vec<Value>, ApiError> {
    let listed = cluster
        .list_kind(&pods(), Some(NAMESPACE))
        .await
        .map_err(|e| read_problem(e, "the node shells", list_read("", "pods", Some(NAMESPACE))))?;
    Ok(listed.into_iter().map(value).collect())
}

/// The node shell pods, oldest first; cluster administrators only.
#[utoipa::path(get, path = "/api/v1/cluster/node-shells", tag = "cluster",
    operation_id = "nodeShells",
    responses((status = 200, body = NodeShells),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn list(
    Person(cluster): Person,
    Everyday(everyday): Everyday,
) -> Result<Json<NodeShells>, ApiError> {
    require_admin(cluster.as_ref(), "node shells").await?;
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let key = shell::person_key(&identity.username);
    let pods = shell_pods(cluster.as_ref()).await?;
    let shells = shell::shells(&pods)
        .into_iter()
        .map(|found| view(found, &key, &pods))
        .collect();
    Ok(Json(NodeShells { shells }))
}

/// What opening a shell writes, worked out against the cluster as it is.
struct Opening {
    node_uid: Option<String>,
    namespace: Option<Value>,
    exposed: Vec<shell::Exposure>,
}

async fn read_node(cluster: &dyn Cluster, node: &str) -> Result<Value, ApiError> {
    let nodes = ResourceRef::new("", "v1", "nodes", "Node");
    cluster
        .find_object(&nodes, None, node)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("node {node}"),
                get_read("", "nodes", None, node),
            )
        })?
        .map(value)
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::NotFound,
                format!("There is no node named {node}."),
            ))
        })
}

async fn rbac(
    cluster: &dyn Cluster,
    resource: &str,
    kind: &str,
    namespace: Option<&str>,
) -> Result<Vec<Value>, ApiError> {
    let reference = ResourceRef::new(RBAC_GROUP, "v1", resource, kind);
    let listed = cluster
        .list_objects(&reference, namespace)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("the {resource}"),
                list_read(RBAC_GROUP, resource, namespace),
            )
        })?
        .unwrap_or_default();
    Ok(listed
        .into_iter()
        .map(|o| {
            let mut data = value(o);
            if let Some(object) = data.as_object_mut() {
                object.insert("kind".to_owned(), Value::from(kind));
            }
            data
        })
        .collect())
}

/// The bindings that let others into node shells.
async fn exposed(
    cluster: &dyn Cluster,
    namespace_exists: bool,
) -> Result<Vec<shell::Exposure>, ApiError> {
    let mut bindings = rbac(cluster, "clusterrolebindings", "ClusterRoleBinding", None).await?;
    let mut roles = rbac(cluster, "clusterroles", "ClusterRole", None).await?;
    if namespace_exists {
        bindings.extend(rbac(cluster, "rolebindings", "RoleBinding", Some(NAMESPACE)).await?);
        roles.extend(rbac(cluster, "roles", "Role", Some(NAMESPACE)).await?);
    }
    Ok(shell::exposures(&bindings, &roles))
}

async fn opening(cluster: &dyn Cluster, node: &str) -> Result<Opening, ApiError> {
    let found = read_node(cluster, node).await?;
    let namespaces = ResourceRef::new("", "v1", "namespaces", "Namespace");
    let namespace = cluster
        .find_object(&namespaces, None, NAMESPACE)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("namespace {NAMESPACE}"),
                get_read("", "namespaces", None, NAMESPACE),
            )
        })?
        .map(value);
    let exposed = exposed(cluster, namespace.is_some()).await?;
    Ok(Opening {
        node_uid: text(&found, "/metadata/uid"),
        namespace,
        exposed,
    })
}

fn namespace_target() -> ObjectTarget {
    ObjectTarget {
        api_version: "v1".to_owned(),
        kind: "Namespace".to_owned(),
        plural: "namespaces".to_owned(),
        namespace: String::new(),
        name: NAMESPACE.to_owned(),
    }
}

fn pod_target(name: &str) -> ObjectTarget {
    ObjectTarget {
        api_version: "v1".to_owned(),
        kind: "Pod".to_owned(),
        plural: "pods".to_owned(),
        namespace: NAMESPACE.to_owned(),
        name: name.to_owned(),
    }
}

fn event_target(name: &str) -> ObjectTarget {
    ObjectTarget {
        api_version: "events.k8s.io/v1".to_owned(),
        kind: "Event".to_owned(),
        plural: "events".to_owned(),
        namespace: shell::EVENT_NAMESPACE.to_owned(),
        name: name.to_owned(),
    }
}

/// The namespace write, if one is needed, with the object as it will be.
fn namespace_write(namespace: Option<&Value>) -> Option<(WriteBody, ShellWriteAction, Value)> {
    namespace.map_or_else(
        || {
            let body = shell::namespace_body();
            Some((
                WriteBody::Create(body.clone()),
                ShellWriteAction::Create,
                body,
            ))
        },
        |current| {
            shell::namespace_patch(current).map(|(operations, after)| {
                (
                    WriteBody::JsonPatch(Value::Array(operations)),
                    ShellWriteAction::Patch,
                    after,
                )
            })
        },
    )
}

fn exposure_views(exposed: Vec<shell::Exposure>) -> Vec<ShellExposure> {
    exposed
        .into_iter()
        .map(|e| ShellExposure {
            kind: e.kind,
            name: e.name,
            role: e.role,
            rights: e.rights,
            subjects: e
                .subjects
                .into_iter()
                .map(|s| ShellSubject {
                    kind: s.kind,
                    name: s.name,
                    namespace: s.namespace,
                })
                .collect(),
        })
        .collect()
}

fn plan_key(state: &AppState) -> Result<std::sync::Arc<token::PlanKey>, ApiError> {
    state.plan_key.clone().ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a token key when it started, so it cannot plan changes.",
        ))
    })
}

fn seal(sealing: &token::PlanKey, claims: &Claims) -> Result<String, ApiError> {
    token::sign_as(sealing, PREFIX, claims).map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "The system random number generator failed, so Zeabur could not seal the plan.",
        ))
    })
}

async fn read_shell(cluster: &dyn Cluster, pod: &str) -> Result<Value, ApiError> {
    let found = cluster
        .find_object(&pods(), Some(NAMESPACE), pod)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("pod {pod}"),
                get_read("", "pods", Some(NAMESPACE), pod),
            )
        })?
        .map(value)
        .filter(shell::is_node_shell);
    found.ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::NotFound,
            format!("There is no node shell named {pod}."),
        ))
    })
}

/// What every plan is made with.
struct Planning<'a> {
    identity: &'a Identity,
    image: &'a str,
    made_at: &'a str,
    expires_at: i64,
}

async fn plan_open(
    cluster: &dyn Cluster,
    planning: &Planning<'_>,
    node: String,
) -> Result<(Claims, NodeShellPlan), ApiError> {
    let opening = opening(cluster, &node).await?;
    let request = NodeShell {
        node: &node,
        node_uid: opening.node_uid.as_deref(),
        person: &planning.identity.username,
        image: planning.image,
        made_at: planning.made_at,
    };
    let pod = shell::pod_name(&request);
    let mut writes = Vec::new();
    if let Some((_, action, after)) = namespace_write(opening.namespace.as_ref()) {
        writes.push(ShellWrite {
            action,
            kind: "Namespace".to_owned(),
            name: NAMESPACE.to_owned(),
            namespace: None,
            yaml: yaml(&after),
        });
    }
    writes.push(ShellWrite {
        action: ShellWriteAction::Create,
        kind: "Pod".to_owned(),
        name: pod.clone(),
        namespace: Some(NAMESPACE.to_owned()),
        yaml: yaml(&shell::pod_body(&request)),
    });
    writes.push(ShellWrite {
        action: ShellWriteAction::Create,
        kind: "Event".to_owned(),
        name: pod.clone(),
        namespace: Some(shell::EVENT_NAMESPACE.to_owned()),
        yaml: yaml(&shell::event_body(&request, planning.made_at)),
    });
    let claims = Claims {
        person: planning.identity.username.clone(),
        groups: planning.identity.groups.clone(),
        op: "open".to_owned(),
        node: node.clone(),
        pod: pod.clone(),
        made_at: planning.made_at.to_owned(),
        image: planning.image.to_owned(),
        uid: opening.node_uid.clone(),
        namespace_version: opening.namespace.as_ref().map(namespace_version),
        expires_at: planning.expires_at,
    };
    let plan = NodeShellPlan {
        token: None,
        expires_at: String::new(),
        op: claims.op.clone(),
        kubectl: Some(shell::kubectl_command(&node, planning.image)),
        node,
        pod,
        namespace: NAMESPACE.to_owned(),
        person: Some(planning.identity.username.clone()),
        image: planning.image.to_owned(),
        deadline_seconds: shell::DEADLINE_SECONDS,
        first_session_seconds: shell::FIRST_SESSION_SECONDS,
        writes,
        exposed: exposure_views(opening.exposed),
    };
    Ok((claims, plan))
}

async fn plan_remove(
    cluster: &dyn Cluster,
    planning: &Planning<'_>,
    pod: String,
) -> Result<(Claims, NodeShellPlan), ApiError> {
    let found = read_shell(cluster, &pod).await?;
    let node = text(&found, "/spec/nodeName").unwrap_or_default();
    let claims = Claims {
        person: planning.identity.username.clone(),
        groups: planning.identity.groups.clone(),
        op: "remove".to_owned(),
        node: node.clone(),
        pod: pod.clone(),
        made_at: planning.made_at.to_owned(),
        image: text(&found, "/spec/containers/0/image").unwrap_or_default(),
        uid: text(&found, "/metadata/uid"),
        namespace_version: None,
        expires_at: planning.expires_at,
    };
    let plan = NodeShellPlan {
        token: None,
        expires_at: String::new(),
        op: claims.op.clone(),
        node,
        writes: vec![ShellWrite {
            action: ShellWriteAction::Delete,
            kind: "Pod".to_owned(),
            name: pod.clone(),
            namespace: Some(NAMESPACE.to_owned()),
            yaml: yaml(&found),
        }],
        pod,
        namespace: NAMESPACE.to_owned(),
        person: found
            .pointer("/metadata/annotations")
            .and_then(|a| a.get(shell::PERSON_ANNOTATION))
            .and_then(Value::as_str)
            .map(str::to_owned),
        image: claims.image.clone(),
        deadline_seconds: shell::DEADLINE_SECONDS,
        first_session_seconds: shell::FIRST_SESSION_SECONDS,
        kubectl: None,
        exposed: Vec::new(),
    };
    Ok((claims, plan))
}

/// Works out what opening a shell on a node, or removing a shell pod, writes, and a token to
/// apply it; writes nothing. Refused (`403`, `denied` naming `*` on `*`) unless the person may do
/// everything everywhere; without a token while `exposed` lists bindings that let others into
/// node shells.
#[utoipa::path(post, path = "/api/v1/cluster/node-shells/plan", tag = "cluster",
    operation_id = "planNodeShell", request_body = NodeShellPlanRequest,
    responses((status = 200, body = NodeShellPlan),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(request): Json<NodeShellPlanRequest>,
) -> Result<Json<NodeShellPlan>, ApiError> {
    let sealing = plan_key(&state)?;
    require_admin(cluster.as_ref(), "a shell on a node").await?;
    let identity = everyday
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let now = (state.clock)();
    let expires_at = now.as_second().saturating_add(LIFETIME_SECONDS);
    let planning = Planning {
        identity: &identity,
        image: &state.settings.terminal.debug_image,
        made_at: &now.to_string(),
        expires_at,
    };
    let (claims, mut plan) = match request {
        NodeShellPlanRequest::Open { node } => plan_open(cluster.as_ref(), &planning, node).await?,
        NodeShellPlanRequest::Remove { pod } => {
            plan_remove(cluster.as_ref(), &planning, pod).await?
        }
    };
    plan.expires_at = Timestamp::from_second(expires_at)
        .unwrap_or(Timestamp::UNIX_EPOCH)
        .to_string();
    if plan.exposed.is_empty() {
        plan.token = Some(seal(&sealing, &claims)?);
    }
    Ok(Json(plan))
}

async fn send(
    cluster: &dyn Cluster,
    target: ObjectTarget,
    body: WriteBody,
    dry_run: bool,
    refused: Denied,
) -> Result<(), ApiError> {
    let what = format!("{} {}", target.kind, target.name);
    let write = WriteRequest {
        target,
        body,
        dry_run,
    };
    cluster
        .write(&write)
        .await
        .map(|_| ())
        .map_err(|error| read_problem(error, &what, refused))
}

fn moved(what: &str) -> ApiError {
    ApiError::from(Problem::new(
        Kind::Conflict,
        format!("{what} changed after the plan was made."),
    ))
}

const fn delete_now(uid: Option<String>) -> WriteBody {
    WriteBody::Delete(Preconditions {
        uid,
        resource_version: None,
        grace_period_seconds: Some(0),
    })
}

/// Carries out a plan by its token: checks it and the cluster again, dry-runs every write, then
/// writes.
#[utoipa::path(post, path = "/api/v1/cluster/node-shells/apply", tag = "cluster",
    operation_id = "applyNodeShell", request_body = NodeShellApply,
    responses((status = 200, body = NodeShellApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Everyday(everyday): Everyday,
    Json(request): Json<NodeShellApply>,
) -> Result<Json<NodeShellApplied>, ApiError> {
    let sealing = plan_key(&state)?;
    let claims: Claims = token::open_as(&sealing, PREFIX, &request.token).map_err(token_problem)?;
    let now = (state.clock)();
    if now.as_second() >= claims.expires_at {
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
    require_admin(cluster.as_ref(), "a shell on a node").await?;
    let cluster = cluster.as_ref();
    if claims.op == "remove" {
        return apply_remove(cluster, claims).await.map(Json);
    }
    let image = &state.settings.terminal.debug_image;
    apply_open(cluster, claims, image, now).await.map(Json)
}

async fn apply_remove(cluster: &dyn Cluster, claims: Claims) -> Result<NodeShellApplied, ApiError> {
    let found = read_shell(cluster, &claims.pod).await?;
    if text(&found, "/metadata/uid") != claims.uid {
        return Err(moved(&format!("Pod {}", claims.pod)));
    }
    let refused = Denied {
        name: Some(claims.pod.clone()),
        ..right("delete", "", "pods", Some(NAMESPACE))
    };
    for dry_run in [true, false] {
        send(
            cluster,
            pod_target(&claims.pod),
            delete_now(claims.uid.clone()),
            dry_run,
            refused.clone(),
        )
        .await?;
    }
    Ok(NodeShellApplied {
        pod: claims.pod,
        namespace: NAMESPACE.to_owned(),
        node: claims.node,
    })
}

/// Empty for a namespace without a version, so it still differs from no namespace.
fn namespace_version(namespace: &Value) -> String {
    text(namespace, "/metadata/resourceVersion").unwrap_or_default()
}

async fn apply_open(
    cluster: &dyn Cluster,
    claims: Claims,
    image: &str,
    now: Timestamp,
) -> Result<NodeShellApplied, ApiError> {
    let opening = opening(cluster, &claims.node).await?;
    if opening.node_uid != claims.uid {
        return Err(moved(&format!("Node {}", claims.node)));
    }
    if opening.namespace.as_ref().map(namespace_version) != claims.namespace_version {
        return Err(moved(&format!("Namespace {NAMESPACE}")));
    }
    if !opening.exposed.is_empty() {
        let names: Vec<String> = opening
            .exposed
            .iter()
            .map(|e| format!("{} {}", e.kind, e.name))
            .collect();
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            format!(
                "Since the plan was made, {} let others into node shells.",
                names.join(", ")
            ),
        )));
    }
    if claims.image != image {
        return Err(moved("The shell image"));
    }
    let request = NodeShell {
        node: &claims.node,
        node_uid: claims.uid.as_deref(),
        person: &claims.person,
        image: &claims.image,
        made_at: &claims.made_at,
    };
    if let Some((body, action, _)) = namespace_write(opening.namespace.as_ref()) {
        let verb = if action == ShellWriteAction::Create {
            "create"
        } else {
            "patch"
        };
        let refused = right(verb, "", "namespaces", None);
        send(
            cluster,
            namespace_target(),
            body.clone(),
            true,
            refused.clone(),
        )
        .await?;
        send(cluster, namespace_target(), body, false, refused).await?;
    }
    let pod = shell::pod_name(&request);
    let writes = [
        (
            pod_target(&pod),
            WriteBody::Create(shell::pod_body(&request)),
            right("create", "", "pods", Some(NAMESPACE)),
        ),
        (
            event_target(&pod),
            WriteBody::Create(shell::event_body(&request, &now.to_string())),
            right(
                "create",
                "events.k8s.io",
                "events",
                Some(shell::EVENT_NAMESPACE),
            ),
        ),
    ];
    for (target, body, refused) in &writes {
        send(cluster, target.clone(), body.clone(), true, refused.clone()).await?;
    }
    for (target, body, refused) in writes {
        send(cluster, target, body, false, refused).await?;
    }
    Ok(NodeShellApplied {
        pod,
        namespace: NAMESPACE.to_owned(),
        node: claims.node,
    })
}

/// The shell in a node shell pod the person opened, over a WebSocket.
///
/// The same protocol as `…/pods/{pod}/exec` (`ClientFrame`, `ServerFrame`, close codes `4000`
/// and `4001`); `ready` names the container `shell`. Failures arrive as an `error` frame: someone
/// who may not do everything everywhere, or may not `create` `pods/exec` in the namespace
/// (`403`, `elevation-required` where admin rights are offered and not in use), a pod that is not
/// a node shell the person opened (`404`), one not running (`409`). When the terminal closes,
/// Zeabur deletes the pod at once.
#[utoipa::path(get, path = "/api/v1/cluster/node-shells/{pod}/terminal", tag = "cluster",
    operation_id = "nodeShellTerminal",
    params(("pod" = String, Path)),
    responses((status = 101, description = "Switched to the terminal's WebSocket"),
              (status = 401, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn terminal(
    terminal: Terminal,
    Path(pod): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade
        .max_message_size(exec::MAX_MESSAGE)
        .on_upgrade(move |socket| run(socket, terminal, pod))
}

async fn run(mut socket: WebSocket, terminal: Terminal, pod: String) {
    let cluster = std::sync::Arc::clone(&terminal.cluster);
    match enter(cluster.as_ref(), &pod).await {
        Ok((uid, session)) => {
            let ready = ServerFrame::Ready {
                container: shell::CONTAINER.to_owned(),
            };
            if exec::send_frame(&mut socket, &ready).await {
                exec::relay(socket, session, terminal.expires_in, terminal.idle).await;
            }
            let ended = WriteRequest {
                target: pod_target(&pod),
                body: delete_now(uid),
                dry_run: false,
            };
            // The pod also ends on its own once no session is left.
            let _deleted = cluster.write(&ended).await;
        }
        Err(problem) => {
            let frame = ServerFrame::Error {
                problem: Box::new(offer_elevation(*problem.0, terminal.elevation)),
            };
            if exec::send_frame(&mut socket, &frame).await {
                exec::close(&mut socket, 1000, "").await;
            }
        }
    }
}

/// Checks that the person may enter `pod` and starts the shell; the pod's uid and the session.
async fn enter(
    cluster: &dyn Cluster,
    pod: &str,
) -> Result<(Option<String>, ExecSession), ApiError> {
    require_admin(cluster, "a shell on a node").await?;
    let right = exec::exec_right(NAMESPACE, pod);
    if matches!(cluster.review_access(&right).await, Ok(false)) {
        return Err(review_refusal(&format!("node shell {pod}"), right));
    }
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let found = read_shell(cluster, pod).await?;
    if !shell::is_opened_by(&found, &identity.username) {
        return Err(ApiError::from(Problem::new(
            Kind::NotFound,
            format!("You did not open node shell {pod}; only whoever opened it can enter it."),
        )));
    }
    let container = exec::pick_container(&found, pod, Some(shell::CONTAINER.to_owned()))?;
    let request = ExecRequest {
        container,
        command: shell::command(pod),
    };
    let session = cluster
        .exec(NAMESPACE, pod, &request)
        .await
        .map_err(|e| read_problem(e, &format!("node shell {pod}"), right))?;
    Ok((text(&found, "/metadata/uid"), session))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_request_names_its_operation() {
        let open: NodeShellPlanRequest =
            serde_json::from_value(serde_json::json!({"op": "open", "node": "vps-1"})).unwrap();
        assert_eq!(
            open,
            NodeShellPlanRequest::Open {
                node: "vps-1".to_owned()
            }
        );
        let remove: NodeShellPlanRequest =
            serde_json::from_value(serde_json::json!({"op": "remove", "pod": "p"})).unwrap();
        assert_eq!(
            remove,
            NodeShellPlanRequest::Remove {
                pod: "p".to_owned()
            }
        );
    }

    #[test]
    fn a_missing_namespace_is_created_and_one_without_the_label_is_patched() {
        let (_, action, after) = namespace_write(None).unwrap();
        assert_eq!(action, ShellWriteAction::Create);
        assert_eq!(
            after["metadata"]["labels"]["pod-security.kubernetes.io/enforce"],
            "privileged"
        );
        let bare = serde_json::json!({"metadata": {"name": NAMESPACE, "resourceVersion": "3"}});
        let (_, action, _) = namespace_write(Some(&bare)).unwrap();
        assert_eq!(action, ShellWriteAction::Patch);
        assert!(namespace_write(Some(&shell::namespace_body())).is_none());
    }
}
