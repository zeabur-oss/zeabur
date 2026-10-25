//! A kubectl terminal for the whole cluster, for cluster administrators.
//!
//! It runs in a tools pod (see `zeabur_plan::terminal`) that is no project's; the Components page
//! offers it to run what Zeabur does not install itself.
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /api/v1/cluster/tools` | the person's cluster tools pods and how they are doing; deletes those that are done with |
//! | `POST /api/v1/cluster/tools/plan` | what starting one writes (the pod), and a token; writes nothing |
//! | `POST /api/v1/cluster/tools/apply` | carries out that plan by its token |
//! | `GET /api/v1/cluster/tools/{name}/terminal` | a terminal in it, over a WebSocket |
//! | `DELETE /api/v1/cluster/tools/{name}` | deletes it |
//!
//! Every route is refused unless kube-apiserver says the person may do everything everywhere. The
//! pod goes in the terminal namespace with the projects' tools pods, and Zeabur's own service
//! account creates it (see the `tools` module). `kubectl` starts in namespace `default`. As for
//! node shells, a plan has no token while a binding lets anyone but cluster administrators and
//! Zeabur's service account create pods in that namespace or run commands in its pods
//! (`exposed`), and its apply is refused when one has appeared since. Everything else (the
//! credential, the pod's lifetime) is the project tools pod's.

use axum::Json;
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ObjectTarget, WriteBody};
use zeabur_plan::terminal::ToolsScope;
use zeabur_plan::{Plan, Warning, Write};

use super::node_shells::{
    ShellExposure, ShellWrite, ShellWriteAction, exposed_except, exposure_views, moved, plan_key,
    require_admin, seal, send, yaml,
};
use crate::access::Handoff;
use crate::error::ApiError;
use crate::handlers::{AppState, Credential, Person};
use crate::plan_token::{self as token, LIFETIME_SECONDS, TokenError};
use crate::pods::exec::Terminal;
use crate::projects::writes::terminal::{Planner, list_pods, local_mode, tools_plan};
use crate::projects::writes::token_problem;
use crate::tools::{CLUSTER_ADMIN, Scope, ToolsStatus};

/// Sealed plans of this module start with this, so no other plan's token opens as one.
const PREFIX: &str = "t1.";
const WHAT: &str = "a kubectl terminal for the cluster";

/// A plan that starts the person's cluster tools pod, or uses the one that still runs.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterToolsPlan {
    /// Send this to `apply` to carry out exactly this plan; absent while `exposed` lists
    /// bindings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// When the token stops working, RFC 3339.
    pub expires_at: String,
    /// The namespace of the pod.
    pub namespace: String,
    /// The pod it starts, or the one it uses.
    pub pod: String,
    /// The writes, in order; none when it uses a running pod.
    pub writes: Vec<ShellWrite>,
    /// Who `kubectl` acts as and until when, or that a running pod is used.
    pub warnings: Vec<Warning>,
    /// Bindings that let others into the pod, and so to the credential in it; the plan cannot be
    /// applied while there is one.
    pub exposed: Vec<ShellExposure>,
}

/// The token of a plan to apply.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ClusterToolsApply {
    /// The token `plan` returned.
    pub token: String,
}

/// What an apply did.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ClusterToolsApplied {
    /// The pod a terminal opens in.
    pub pod: String,
    /// Its namespace.
    pub namespace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    person: String,
    groups: Vec<String>,
    made_at: String,
    pod: String,
    expires_at: i64,
}

/// What a plan writes, worked out against the cluster as it is.
struct Computed {
    namespace: String,
    plan: Plan,
    pod: String,
    exposed: Vec<zeabur_plan::node_shell::Exposure>,
}

impl Computed {
    /// Each write with what kube-apiserver would refuse it as; empty when a running pod is used.
    fn writes(&self) -> Vec<(ObjectTarget, Value, Denied)> {
        let mut writes = Vec::new();
        let Some(pod) = self.plan.objects.first() else {
            return writes;
        };
        if let Write::Create { body } = &pod.write {
            writes.push((
                ObjectTarget {
                    api_version: "v1".to_owned(),
                    kind: "Pod".to_owned(),
                    plural: "pods".to_owned(),
                    namespace: self.namespace.clone(),
                    name: self.pod.clone(),
                },
                body.clone(),
                right(&self.namespace),
            ));
        }
        writes
    }
}

fn right(namespace: &str) -> Denied {
    Denied {
        verb: "create".to_owned(),
        group: String::new(),
        resource: "pods".to_owned(),
        name: None,
        namespace: Some(namespace.to_owned()),
    }
}

/// `cluster` is the administrator's, which reads the bindings; `zeabur` is Zeabur's own service
/// account, which reads the pods.
async fn compute(
    state: &AppState,
    cluster: &dyn Cluster,
    zeabur: &dyn Cluster,
    handoff: &Handoff,
    person: &str,
    made_at: &str,
) -> Result<Computed, ApiError> {
    let settings = &state.settings.terminal;
    let namespace = settings.namespace.clone();
    let trusted = zeabur
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("Zeabur's own identity")))?;
    let exposed = exposed_except(cluster, &namespace, Some(&trusted.username)).await?;
    let pods = list_pods(zeabur, &namespace).await?;
    let planner = Planner {
        settings,
        person,
        handoff: Some(handoff),
        zeabur: Some(zeabur),
    };
    let plan = tools_plan(
        &planner,
        handoff,
        &namespace,
        ToolsScope::Cluster,
        &pods,
        made_at,
    );
    let pod = plan
        .objects
        .first()
        .map(|o| o.target.name.clone())
        .or_else(|| {
            plan.warnings.iter().find_map(|w| {
                if let Warning::ToolsPodReused { pod, .. } = w {
                    Some(pod.clone())
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();
    Ok(Computed {
        namespace,
        plan,
        pod,
        exposed,
    })
}

/// The person's cluster tools pods, deleting those that are done with; cluster administrators
/// only.
#[utoipa::path(get, path = "/api/v1/cluster/tools", tag = "cluster",
    operation_id = "clusterTools",
    responses((status = 200, body = ToolsStatus),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn status(
    State(state): State<AppState>,
    Person(cluster): Person,
    Credential(handoff): Credential,
) -> Result<Json<ToolsStatus>, ApiError> {
    crate::tools::status_in(&state, cluster.as_ref(), handoff.is_some(), &Scope::Cluster)
        .await
        .map(Json)
}

/// Works out what starting the person's cluster tools pod writes, and a token to apply it; writes
/// nothing. Refused (`403`, `denied` naming `*` on `*`) unless the person may do everything
/// everywhere; `400` on the person's own computer, where Zeabur holds no token to hand over;
/// without a token while `exposed` lists bindings that let others into the pod.
#[utoipa::path(post, path = "/api/v1/cluster/tools/plan", tag = "cluster",
    operation_id = "planClusterTools",
    responses((status = 200, body = ClusterToolsPlan),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn plan(
    State(state): State<AppState>,
    Person(cluster): Person,
    Credential(handoff): Credential,
) -> Result<Json<ClusterToolsPlan>, ApiError> {
    let sealing = plan_key(&state)?;
    require_admin(cluster.as_ref(), WHAT, CLUSTER_ADMIN).await?;
    let handoff = handoff.ok_or_else(local_mode)?;
    let zeabur = crate::tools::zeabur(&state)?;
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let now = (state.clock)();
    let made_at = Timestamp::from_second(now.as_second())
        .unwrap_or(Timestamp::UNIX_EPOCH)
        .to_string();
    let computed = compute(
        &state,
        cluster.as_ref(),
        zeabur.as_ref(),
        &handoff,
        &identity.username,
        &made_at,
    )
    .await?;
    let expires_at = now.as_second().saturating_add(LIFETIME_SECONDS);
    let claims = Claims {
        person: identity.username,
        groups: identity.groups,
        made_at,
        pod: computed.pod.clone(),
        expires_at,
    };
    let writes = computed
        .writes()
        .into_iter()
        .map(|(target, body, _)| ShellWrite {
            action: ShellWriteAction::Create,
            namespace: (!target.namespace.is_empty()).then(|| target.namespace.clone()),
            kind: target.kind,
            name: target.name,
            yaml: yaml(&body),
        })
        .collect();
    let token = if computed.exposed.is_empty() {
        Some(seal(&sealing, PREFIX, &claims)?)
    } else {
        None
    };
    Ok(Json(ClusterToolsPlan {
        token,
        expires_at: Timestamp::from_second(expires_at)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_string(),
        namespace: computed.namespace,
        pod: computed.pod,
        writes,
        warnings: computed.plan.warnings,
        exposed: exposure_views(computed.exposed),
    }))
}

/// Carries out a plan by its token: checks it and the cluster again, dry-runs each write, then
/// writes it.
#[utoipa::path(post, path = "/api/v1/cluster/tools/apply", tag = "cluster",
    operation_id = "applyClusterTools", request_body = ClusterToolsApply,
    responses((status = 200, body = ClusterToolsApplied),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn apply(
    State(state): State<AppState>,
    Person(cluster): Person,
    Credential(handoff): Credential,
    Json(request): Json<ClusterToolsApply>,
) -> Result<Json<ClusterToolsApplied>, ApiError> {
    let sealing = plan_key(&state)?;
    let claims: Claims = token::open_as(&sealing, PREFIX, &request.token).map_err(token_problem)?;
    if (state.clock)().as_second() >= claims.expires_at {
        return Err(token_problem(TokenError::Expired));
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
    require_admin(cluster.as_ref(), WHAT, CLUSTER_ADMIN).await?;
    let handoff = handoff.ok_or_else(local_mode)?;
    let zeabur = crate::tools::zeabur(&state)?;
    let computed = compute(
        &state,
        cluster.as_ref(),
        zeabur.as_ref(),
        &handoff,
        &claims.person,
        &claims.made_at,
    )
    .await?;
    if computed.pod != claims.pod {
        return Err(moved("Your cluster tools pod"));
    }
    if !computed.exposed.is_empty() {
        let names: Vec<String> = computed
            .exposed
            .iter()
            .map(|e| format!("{} {}", e.kind, e.name))
            .collect();
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            format!(
                "Since the plan was made, {} let others into cluster kubectl terminals.",
                names.join(", ")
            ),
        )));
    }
    for (target, body, refused) in computed.writes() {
        for dry_run in [true, false] {
            send(
                zeabur.as_ref(),
                target.clone(),
                WriteBody::Create(body.clone()),
                dry_run,
                refused.clone(),
            )
            .await?;
        }
    }
    Ok(Json(ClusterToolsApplied {
        pod: computed.pod,
        namespace: computed.namespace,
    }))
}

/// Deletes a cluster tools pod of the person's, as long as it is the one they read.
#[utoipa::path(delete, path = "/api/v1/cluster/tools/{name}", tag = "cluster",
    operation_id = "deleteClusterTools",
    params(("name" = String, Path)),
    responses((status = 204, description = "Deleted"),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn delete(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    crate::tools::delete_in(&state, cluster.as_ref(), &Scope::Cluster, &name).await
}

/// A terminal in a cluster tools pod of the person's, over a WebSocket: the project tools
/// terminal's protocol and credential (see the `tools` module), with `kubectl` starting in
/// namespace `default`. Failures arrive as an `error` frame, as there, and `403` for
/// anyone who may not do everything everywhere.
#[utoipa::path(get, path = "/api/v1/cluster/tools/{name}/terminal", tag = "cluster",
    operation_id = "clusterToolsTerminal",
    params(("name" = String, Path)),
    responses((status = 101, description = "Switched to the terminal's WebSocket"),
              (status = 401, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn terminal(
    State(state): State<AppState>,
    terminal: Terminal,
    Path(name): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Response {
    crate::tools::upgrade_in(state, terminal, Scope::Cluster, name, upgrade)
}
