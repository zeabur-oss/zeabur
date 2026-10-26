//! A terminal with `kubectl` and `helm` that act as the person, in a short-lived tools pod.
//!
//! A plan creates the pod (the `clusterTools` change for a project, see `zeabur_plan::terminal`;
//! `cluster::tools` for the whole cluster); these endpoints use it.
//!
//! | Method and path | What it does |
//! |---|---|
//! | `GET /api/v1/projects/{p}/tools` | the person's tools pods and how they are doing; deletes those that have stopped or whose terminals closed |
//! | `GET /api/v1/projects/{p}/tools/{name}/terminal` | a terminal in it, over a WebSocket, as `pods/exec` (see `pods::exec`) |
//! | `DELETE /api/v1/projects/{p}/tools/{name}` | deletes it |
//!
//! `cluster::tools` serves the same for an administrator's cluster tools pods (`Scope`).
//!
//! # Where tools pods run
//!
//! Every tools pod, a project's or the cluster's, goes in one namespace, the chart's
//! `terminal.namespace` (`zeabur-terminals`). The chart lets Zeabur's own service account create,
//! list, read and delete pods there and run commands in them, and nothing more; members get no
//! rights there. Before Zeabur plans, lists, deletes or opens a tools pod, it asks kube-apiserver
//! as the person (`SelfSubjectAccessReview`) whether they may `create` `pods/exec` in the
//! project, and for the cluster's whether they may do everything everywhere. Only then does it
//! use its service account. It tells the person's pods from others' by their labels and the
//! person's own user name.
//!
//! On a person's own computer (`zeaburctl ui`), Zeabur has no token of the person's to hand over
//! and no service account, so there is no tools terminal: the person runs `kubectl` there.
//!
//! # The credential
//!
//! The pod's spec holds no credential. Each time a terminal opens, Zeabur writes a kubeconfig
//! with the person's current token into the pod's
//! memory-backed `~/.kube`, through standard input of a command run in it with `pods/exec`
//! (`Cluster::run`), so it never appears in an object, an environment variable, a command line
//! or a log. It names kube-apiserver's in-cluster address, the CA from the pod's
//! `kube-root-ca.crt`, and the project as the namespace (`default` in a cluster tools pod). The
//! token is the one the session uses (minutes long; see `zeabur_auth`), and kube-apiserver accepts
//! it until it expires: Zeabur cannot revoke it. The next terminal writes a fresh token.
//! `kubectl` and `helm` in the pod are wrappers that refuse to run once the token has expired, with
//! a message saying to reopen the terminal.
//!
//! # Lifetime
//!
//! When the last terminal of a pod closes, Zeabur deletes the kubeconfig at once, and the pod
//! after a grace period (a minute by default, for a reconnect). If the server restarted
//! meanwhile, the pod still stops at its `activeDeadlineSeconds`, and the next status request
//! deletes it. A pod outlives the project it was opened from until then.
//!
//! # Who else can use the token
//!
//! Whoever may run commands in pods of the terminal namespace: Zeabur's service account and
//! cluster administrators; and a node's administrator, who can read its memory. They could act as
//! the person until the token expires. A plan for a cluster tools pod has no token while a
//! binding lets anyone else into the namespace (see `cluster::tools`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::ws::WebSocket;
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::Response;
use secrecy::ExposeSecret as _;
use secrecy::zeroize::Zeroizing;
use serde::Serialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ExecRequest, ExitStatus, WriteRequest};
use zeabur_plan::terminal::{self as plans, TOOLS_CONTAINER, TOOLS_PROJECT_ANNOTATION, ToolsScope};

use crate::access::Handoff;
use crate::error::{ApiError, read_problem, review_refusal};
use crate::handlers::{AppState, Person};
use crate::pods::exec::{self, ServerFrame, Terminal};
use crate::projects::writes::terminal::{list_pods, local_mode, read_pod};
use crate::settings::TerminalSettings;

/// kube-apiserver's address inside the cluster, which its serving certificate names
/// [k8s-access-api].
///
/// [k8s-access-api]: https://kubernetes.io/docs/tasks/run-application/access-api-from-pod/#directly-accessing-the-rest-api
const IN_CLUSTER_SERVER: &str = "https://kubernetes.default.svc";

/// How long after its deadline a tools pod that still shows as running is deleted anyway.
const SWEEP_SLACK_SECONDS: i64 = 5 * 60;

/// What a tools pod is for, as its routes name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Scope {
    /// The project of `/api/v1/projects/{p}/tools`.
    Project(String),
    /// The whole cluster, for an administrator: `/api/v1/cluster/tools`.
    Cluster,
}

/// Why a cluster tools terminal is for cluster administrators only.
pub(crate) const CLUSTER_ADMIN: &str =
    "kubectl and helm in it act with your admin rights across the cluster.";

impl Scope {
    pub(crate) fn tools(&self) -> ToolsScope<'_> {
        match self {
            Self::Project(project) => ToolsScope::Project(project),
            Self::Cluster => ToolsScope::Cluster,
        }
    }

    /// Where `kubectl` acts, for the greeting.
    fn place(&self) -> String {
        match self {
            Self::Project(project) => format!("in project {project}"),
            Self::Cluster => format!(
                "across the cluster, starting in namespace {}",
                plans::CLUSTER_TOOLS_START
            ),
        }
    }

    /// Refused unless kube-apiserver says the person may run commands in the project's pods, or,
    /// for the cluster, do everything everywhere. `cluster` is the person's.
    pub(crate) async fn allowed(&self, cluster: &dyn Cluster) -> Result<(), ApiError> {
        match self {
            Self::Project(project) => {
                let right = exec_in(project);
                match cluster.review_access(&right).await {
                    Ok(true) => Ok(()),
                    Ok(false) => Err(exec::exec_refusal(review_refusal(
                        &format!("a kubectl terminal for project {project}"),
                        right,
                    ))),
                    Err(error) => Err(read_problem(
                        error,
                        "whether you may run commands in the project's pods",
                        right,
                    )),
                }
            }
            Self::Cluster => {
                crate::cluster::node_shells::require_admin(
                    cluster,
                    "a kubectl terminal for the cluster",
                    CLUSTER_ADMIN,
                )
                .await
            }
        }
    }
}

/// `create` on `pods/exec` in `namespace`, for any pod.
fn exec_in(namespace: &str) -> Denied {
    Denied {
        verb: "create".to_owned(),
        group: String::new(),
        resource: "pods/exec".to_owned(),
        name: None,
        namespace: Some(namespace.to_owned()),
    }
}

/// Zeabur's own service account in the terminal namespace, from `settings.token_file`.
pub(crate) fn connect(settings: &TerminalSettings) -> Option<Arc<dyn Cluster>> {
    let path = settings.token_file.as_deref()?;
    match zeabur_kube::ClusterEndpoint::in_cluster()
        .and_then(|endpoint| zeabur_kube::UserClient::with_token_file(&endpoint, path))
    {
        Ok(client) => Some(Arc::new(client)),
        Err(error) => {
            tracing::warn!(%error, "no tools terminals: Zeabur cannot use its service account");
            None
        }
    }
}

/// Zeabur's own service account in the terminal namespace.
pub(crate) fn zeabur(state: &AppState) -> Result<Arc<dyn Cluster>, ApiError> {
    state.terminals.clone().ok_or_else(local_mode)
}

/// The open terminals of each tools pod, by `namespace/name`.
#[derive(Debug, Default)]
pub struct Sessions {
    open: Mutex<HashMap<String, Open>>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Open {
    terminals: usize,
    /// Bumped each time the last terminal closes, so a deletion scheduled then is dropped once
    /// another terminal has opened and closed meanwhile.
    closings: u64,
    /// When the last terminal closed, while none is open.
    idle_since: Option<Timestamp>,
}

impl Sessions {
    fn opened(&self, key: &str) {
        if let Ok(mut open) = self.open.lock() {
            let entry = open.entry(key.to_owned()).or_default();
            entry.terminals += 1;
            entry.idle_since = None;
        }
    }

    /// One terminal closed at `now`; the closing's number when it was the last.
    fn closed(&self, key: &str, now: Timestamp) -> Option<u64> {
        let mut open = self.open.lock().ok()?;
        let entry = open.get_mut(key)?;
        entry.terminals = entry.terminals.saturating_sub(1);
        let last = (entry.terminals == 0).then(|| {
            entry.closings += 1;
            entry.idle_since = Some(now);
            entry.closings
        });
        drop(open);
        last
    }

    /// Whether no terminal opened since `closing`.
    fn still_closed(&self, key: &str, closing: u64) -> bool {
        self.open.lock().is_ok_and(|open| {
            open.get(key)
                .is_some_and(|o| o.terminals == 0 && o.closings == closing)
        })
    }

    /// Whether the pod's last terminal closed more than `grace` before `now`, none open since.
    fn idle_past(&self, key: &str, grace: std::time::Duration, now: Timestamp) -> bool {
        let grace = i64::try_from(grace.as_secs()).unwrap_or(i64::MAX);
        self.open.lock().is_ok_and(|open| {
            open.get(key)
                .and_then(|o| o.idle_since.filter(|_| o.terminals == 0))
                .is_some_and(|since| now.as_second() >= since.as_second().saturating_add(grace))
        })
    }

    fn forget(&self, key: &str) {
        if let Ok(mut open) = self.open.lock() {
            open.remove(key);
        }
    }

    fn terminals(&self, key: &str) -> usize {
        self.open
            .lock()
            .ok()
            .and_then(|open| open.get(key).map(|o| o.terminals))
            .unwrap_or(0)
    }
}

/// A tools pod of the person's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolsPodStatus {
    /// Its name.
    pub name: String,
    /// Its namespace.
    pub namespace: String,
    /// The project it was opened from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Its image.
    pub image: String,
    /// The pod's phase: `Pending`, `Running`, `Succeeded`, `Failed`.
    pub phase: String,
    /// Whether a terminal can open in it now.
    pub ready: bool,
    /// When it started, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// When it stops whatever happens (`activeDeadlineSeconds` after it started), RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stops_at: Option<String>,
    /// How many terminals are open in it on this server.
    pub terminals: usize,
    /// Why it does not start, when it cannot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<Box<Problem>>,
}

/// The person's tools pods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolsStatus {
    /// The namespace these tools pods go in.
    pub namespace: String,
    /// Whether Zeabur can open a tools terminal here; `false` in local mode, which holds no
    /// token to hand over and no service account.
    pub available: bool,
    /// The person's tools pods there that have not stopped and are not being deleted, oldest
    /// first.
    pub pods: Vec<ToolsPodStatus>,
    /// The tools pods that had stopped or outlived their deadline and were deleted now.
    pub deleted: Vec<String>,
}

fn text<'a>(object: &'a Value, path: &str) -> Option<&'a str> {
    object.pointer(path).and_then(Value::as_str)
}

fn stops_at(pod: &Value) -> Option<Timestamp> {
    let started = text(pod, "/status/startTime")?.parse::<Timestamp>().ok()?;
    let deadline = pod.pointer("/spec/activeDeadlineSeconds")?.as_i64()?;
    Timestamp::from_second(started.as_second().saturating_add(deadline)).ok()
}

fn pod_status(pod: &Value, sessions: &Sessions) -> ToolsPodStatus {
    let name = text(pod, "/metadata/name").unwrap_or_default().to_owned();
    let namespace = text(pod, "/metadata/namespace")
        .unwrap_or_default()
        .to_owned();
    let image = text(pod, "/spec/containers/0/image")
        .unwrap_or_default()
        .to_owned();
    let state = pod
        .pointer("/status/containerStatuses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|s| text(s, "/name") == Some(TOOLS_CONTAINER))
        .and_then(|s| s.get("state"));
    let ready = state.and_then(|s| s.get("running")).is_some()
        && pod.pointer("/metadata/deletionTimestamp").is_none();
    let problem = state.and_then(|s| {
        crate::pods::debug::start_problem(
            &image,
            TOOLS_CONTAINER,
            text(s, "/waiting/reason")?,
            text(s, "/waiting/message"),
            "terminal.toolsImage",
        )
    });
    ToolsPodStatus {
        terminals: sessions.terminals(&format!("{namespace}/{name}")),
        project: text(
            pod,
            &format!(
                "/metadata/annotations/{}",
                TOOLS_PROJECT_ANNOTATION.replace('/', "~1")
            ),
        )
        .map(str::to_owned),
        phase: text(pod, "/status/phase").unwrap_or("Pending").to_owned(),
        started_at: text(pod, "/status/startTime").map(str::to_owned),
        stops_at: stops_at(pod).map(|t| t.to_string()),
        name,
        namespace,
        image,
        ready,
        problem,
    }
}

fn delete_request(namespace: &str, pod: &Value) -> WriteRequest {
    let name = text(pod, "/metadata/name").unwrap_or_default();
    crate::projects::writes::write_request(
        namespace,
        &zeabur_plan::Target::of::<Pod>(name),
        &zeabur_plan::Write::Delete {
            uid: text(pod, "/metadata/uid").map(str::to_owned),
            resource_version: text(pod, "/metadata/resourceVersion")
                .unwrap_or_default()
                .to_owned(),
        },
    )
}

/// Whether a tools pod is done with: stopped, or past its deadline with slack, as when the
/// kubelet that should stop it is gone.
fn finished(pod: &Value, now: Timestamp) -> bool {
    let stopped = matches!(text(pod, "/status/phase"), Some("Succeeded" | "Failed"));
    let overdue = text(pod, "/metadata/creationTimestamp")
        .and_then(|t| t.parse::<Timestamp>().ok())
        .zip(
            pod.pointer("/spec/activeDeadlineSeconds")
                .and_then(Value::as_i64),
        )
        .is_some_and(|(created, deadline)| {
            now.as_second()
                > created
                    .as_second()
                    .saturating_add(deadline)
                    .saturating_add(SWEEP_SLACK_SECONDS)
        });
    stopped || overdue
}

/// The person's tools pods in the project's tools namespace, deleting those that are done with.
#[utoipa::path(get, path = "/api/v1/projects/{project}/tools", tag = "pods",
    operation_id = "listToolsPods",
    params(("project" = String, Path)),
    responses((status = 200, body = ToolsStatus),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn status(
    State(state): State<AppState>,
    Person(cluster): Person,
    crate::handlers::Credential(handoff): crate::handlers::Credential,
    Path(project): Path<String>,
) -> Result<Json<ToolsStatus>, ApiError> {
    status_in(
        &state,
        cluster.as_ref(),
        handoff.is_some(),
        &Scope::Project(project),
    )
    .await
    .map(Json)
}

/// The person's tools pods for `scope`, deleting those that are done with. `cluster` is the
/// person's.
pub(crate) async fn status_in(
    state: &AppState,
    cluster: &dyn Cluster,
    available: bool,
    scope: &Scope,
) -> Result<ToolsStatus, ApiError> {
    let namespace = state.settings.terminal.namespace.clone();
    let mut pods = Vec::new();
    let mut deleted = Vec::new();
    if !available {
        return Ok(ToolsStatus {
            namespace,
            available,
            pods,
            deleted,
        });
    }
    scope.allowed(cluster).await?;
    let zeabur = zeabur(state)?;
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let key = plans::person_key(&identity.username);
    let now = (state.clock)();
    for pod in list_pods(zeabur.as_ref(), &namespace).await? {
        if !plans::is_tools_pod_of(&pod, &key, scope.tools()) {
            continue;
        }
        let name = text(&pod, "/metadata/name").unwrap_or_default().to_owned();
        let key = format!("{namespace}/{name}");
        if pod.pointer("/metadata/deletionTimestamp").is_some() {
            continue;
        }
        let closed = state
            .tools
            .idle_past(&key, state.settings.terminal.tools_grace, now);
        if (closed || finished(&pod, now))
            && state.tools.terminals(&key) == 0
            && zeabur
                .write(&delete_request(&namespace, &pod))
                .await
                .is_ok()
        {
            state.tools.forget(&key);
            deleted.push(name);
            continue;
        }
        pods.push(pod_status(&pod, &state.tools));
    }
    pods.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    Ok(ToolsStatus {
        namespace,
        available,
        pods,
        deleted,
    })
}

/// The person's tools pod `name`, read by Zeabur; `404` for anyone else's. `person` is the
/// person's cluster.
async fn own_pod(
    person: &dyn Cluster,
    zeabur: &dyn Cluster,
    namespace: &str,
    scope: &Scope,
    name: &str,
) -> Result<(Value, String), ApiError> {
    let identity = person
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    let pod = read_pod(zeabur, namespace, name).await?;
    if plans::is_tools_pod_of(&pod, &plans::person_key(&identity.username), scope.tools()) {
        Ok((pod, identity.username))
    } else {
        Err(ApiError::from(Problem::new(
            Kind::NotFound,
            format!("You have no tools pod named {name}."),
        )))
    }
}

/// Deletes a tools pod of the person's, as long as it is the one they read (`uid` and
/// `resourceVersion` preconditions); its terminals end with it.
#[utoipa::path(delete, path = "/api/v1/projects/{project}/tools/{name}", tag = "pods",
    operation_id = "deleteToolsPod",
    params(("project" = String, Path), ("name" = String, Path)),
    responses((status = 204, description = "Deleted"),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn delete(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, name)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    delete_in(&state, cluster.as_ref(), &Scope::Project(project), &name).await
}

/// Deletes the person's tools pod `name` for `scope`. `cluster` is the person's.
pub(crate) async fn delete_in(
    state: &AppState,
    cluster: &dyn Cluster,
    scope: &Scope,
    name: &str,
) -> Result<StatusCode, ApiError> {
    let zeabur = zeabur(state)?;
    scope.allowed(cluster).await?;
    let namespace = &state.settings.terminal.namespace;
    let (pod, _) = own_pod(cluster, zeabur.as_ref(), namespace, scope, name).await?;
    zeabur
        .write(&delete_request(namespace, &pod))
        .await
        .map_err(|error| ApiError::from(error.into_problem(&format!("pod {name}"))))?;
    Ok(StatusCode::NO_CONTENT)
}

/// The kubeconfig for the tools pod. It is cleared on drop.
fn kubeconfig(handoff: &Handoff, person: &str, start: &str) -> Zeroizing<Vec<u8>> {
    // Serialize the token from a borrowed `&str`. A `Value` would hold a copy of it.
    #[derive(Serialize)]
    #[serde(rename_all = "kebab-case")]
    struct Kubeconfig<'a> {
        api_version: &'static str,
        kind: &'static str,
        clusters: Value,
        users: [User<'a>; 1],
        contexts: Value,
        current_context: &'static str,
    }
    #[derive(Serialize)]
    struct User<'a> {
        name: &'a str,
        user: Token<'a>,
    }
    #[derive(Serialize)]
    struct Token<'a> {
        token: &'a str,
    }
    let config = |token| Kubeconfig {
        api_version: "v1",
        kind: "Config",
        clusters: json!([{"name": "zeabur", "cluster": {
            "server": IN_CLUSTER_SERVER,
            "certificate-authority": plans::TOOLS_CA,
        }}]),
        users: [User {
            name: person,
            user: Token { token },
        }],
        contexts: json!([{"name": "zeabur", "context": {
            "cluster": "zeabur",
            "user": person,
            "namespace": start,
        }}]),
        current_context: "zeabur",
    };
    let token = handoff.token.secret().expose_secret();
    // JSON writes one byte as at most six (`\u00XX`). With this capacity the buffer never moves,
    // so no copy of the token stays in freed memory.
    let rest = serde_json::to_vec(&config("")).map_or(0, |empty| empty.len());
    let mut out = Zeroizing::new(Vec::with_capacity(
        token.len().saturating_mul(6).saturating_add(rest),
    ));
    if serde_json::to_writer(&mut *out, &config(token)).is_err() {
        out.clear();
    }
    out
}

/// Writes standard input to the kubeconfig, then the wrappers that refuse an expired token and
/// the greeting a login shell shows. Arguments: the token's expiry (seconds since the epoch), the
/// wrappers' message, the greeting.
const SETUP: &str = r#"set -eu
umask 077
mkdir -p "$HOME/.kube" "$HOME/bin" "$HOME/.cache/kube"
cat > "$HOME/.kube/config.new"
mv -f "$HOME/.kube/config.new" "$HOME/.kube/config"
printf '%s\n' "$1" > "$HOME/.kube/expires-at"
umask 022
for tool in kubectl helm; do
  real=$(PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin command -v "$tool" || true)
  [ -n "$real" ] || continue
  {
    printf '#!/bin/sh\n'
    printf 'if [ "$(date +%%s)" -ge "$(cat "$HOME/.kube/expires-at" 2>/dev/null || echo 0)" ]; then\n'
    printf '  echo %s >&2\n' "'$2'"
    printf '  exit 1\nfi\n'
    printf 'exec %s "$@"\n' "$real"
  } > "$HOME/bin/$tool"
  chmod 755 "$HOME/bin/$tool"
done
printf '%s\n' "$3" > "$HOME/.motd"
printf 'cat "$HOME/.motd"\n' > "$HOME/.profile"
"#;

/// Deletes the kubeconfig.
const FORGET: &str = r#"rm -f "$HOME/.kube/config" "$HOME/.kube/expires-at""#;

/// What the wrappers say once the token has expired.
pub const EXPIRED: &str = "Your Zeabur credential in this terminal has expired, so kubectl and \
helm cannot act as you. Close the terminal and open it again from Zeabur.";

fn greeting(
    handoff: &Handoff,
    person: &str,
    place: &str,
    stops_at: Option<Timestamp>,
    grace_seconds: u64,
) -> String {
    let stops = stops_at.map_or_else(String::new, |t| format!(", and stops at {t} in any case"));
    format!(
        "kubectl and helm here act as {person}, {place}.\n\
         Your credential expires at {expires}; kubectl and helm then stop, and reopening the \
         terminal brings a fresh one.\n\
         This pod is deleted {grace_seconds} seconds after its last terminal closes{stops}.\n",
        expires = handoff.expires_at,
    )
}

fn script(command: &str, args: &[String]) -> Vec<String> {
    let mut all = vec![
        "sh".to_owned(),
        "-c".to_owned(),
        command.to_owned(),
        "sh".to_owned(),
    ];
    all.extend_from_slice(args);
    all
}

/// Opens a terminal in the tools pod: writes the person's current credential into it, then
/// starts a login shell. `zeabur` is Zeabur's own service account.
async fn open(
    terminal: &Terminal,
    zeabur: Option<&dyn Cluster>,
    settings: &TerminalSettings,
    scope: &Scope,
    name: &str,
) -> Result<(Value, zeabur_kube::ExecSession), ApiError> {
    let (Some(handoff), Some(cluster)) = (terminal.handoff.as_ref(), zeabur) else {
        return Err(local_mode());
    };
    scope.allowed(terminal.cluster.as_ref()).await?;
    let namespace = settings.namespace.as_str();
    let (pod, person) = own_pod(terminal.cluster.as_ref(), cluster, namespace, scope, name).await?;
    exec::pick_container(&pod, name, Some(TOOLS_CONTAINER.to_owned()))?;
    let exec_right = || exec::exec_right(namespace, name);
    let input = kubeconfig(handoff, &person, scope.tools().start());
    let args = [
        handoff.expires_at.as_second().to_string(),
        EXPIRED.to_owned(),
        greeting(
            handoff,
            &person,
            &scope.place(),
            stops_at(&pod),
            settings.tools_grace.as_secs(),
        ),
    ];
    let written = cluster
        .run(
            namespace,
            name,
            TOOLS_CONTAINER,
            &script(SETUP, &args),
            &input,
        )
        .await;
    drop(input);
    match written.map_err(|e| read_problem(e, &format!("tools pod {name}"), exec_right()))? {
        ExitStatus::Exited(0) => {}
        status @ (ExitStatus::Exited(_) | ExitStatus::CommandNotFound | ExitStatus::Failed(_)) => {
            return Err(ApiError::from(Problem::new(
                Kind::Internal,
                format!(
                    "Zeabur could not write your credential into tools pod {name}: the command \
                     ended with {status:?}."
                ),
            )));
        }
    }
    let session = cluster
        .exec(
            namespace,
            name,
            &ExecRequest {
                container: TOOLS_CONTAINER.to_owned(),
                command: vec!["/bin/sh".to_owned(), "-l".to_owned()],
            },
        )
        .await
        .map_err(|e| read_problem(e, &format!("a terminal in tools pod {name}"), exec_right()))?;
    Ok((pod, session))
}

/// After the last terminal of `pod` closed: deletes the kubeconfig now, and the pod after the
/// grace period unless a terminal opened meanwhile.
fn after_close(
    sessions: Arc<Sessions>,
    cluster: Arc<dyn Cluster>,
    namespace: String,
    pod: Value,
    closing: u64,
    grace: std::time::Duration,
) {
    tokio::spawn(async move {
        let name = text(&pod, "/metadata/name").unwrap_or_default().to_owned();
        let key = format!("{namespace}/{name}");
        let _gone = cluster
            .run(
                &namespace,
                &name,
                TOOLS_CONTAINER,
                &script(FORGET, &[]),
                &[],
            )
            .await;
        tokio::time::sleep(grace).await;
        if !sessions.still_closed(&key, closing) {
            return;
        }
        let Ok(current) = read_pod(cluster.as_ref(), &namespace, &name).await else {
            return;
        };
        if cluster
            .write(&delete_request(&namespace, &current))
            .await
            .is_ok()
        {
            sessions.forget(&key);
        }
    });
}

/// A terminal in a tools pod of the person's, over a WebSocket.
///
/// The same protocol as `…/pods/{pod}/exec` (`ClientFrame`, `ServerFrame`, close code `4001`).
/// Before the shell starts, Zeabur writes the person's current credential into the
/// pod (see the module documentation); `ready` names the container `tools`. Failures arrive as an
/// `error` frame: a pod that is not the person's (`404`), one not running yet (`409`), local mode
/// (`400`), or `create` on `pods/exec` in the project refused (`403`). The pod is deleted a minute
/// (by default) after its last terminal closes.
#[utoipa::path(get, path = "/api/v1/projects/{project}/tools/{name}/terminal", tag = "pods",
    operation_id = "toolsTerminal",
    params(("project" = String, Path), ("name" = String, Path)),
    responses((status = 101, description = "Switched to the terminal's WebSocket"),
              (status = 401, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn terminal(
    State(state): State<AppState>,
    terminal: Terminal,
    Path((project, name)): Path<(String, String)>,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade_in(state, terminal, Scope::Project(project), name, upgrade)
}

/// The terminal in the person's tools pod `name` for `scope`.
pub(crate) fn upgrade_in(
    state: AppState,
    terminal: Terminal,
    scope: Scope,
    name: String,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade
        .max_message_size(exec::MAX_MESSAGE)
        .on_upgrade(move |socket| run(socket, state, terminal, scope, name))
}

async fn run(
    mut socket: WebSocket,
    state: AppState,
    terminal: Terminal,
    scope: Scope,
    name: String,
) {
    let settings = &state.settings.terminal;
    let namespace = settings.namespace.clone();
    let key = format!("{namespace}/{name}");
    state.tools.opened(&key);
    let zeabur = state.terminals.clone();
    match open(&terminal, zeabur.as_deref(), settings, &scope, &name).await {
        Ok((pod, session)) => {
            let ready = ServerFrame::Ready {
                container: TOOLS_CONTAINER.to_owned(),
            };
            if exec::send_frame(&mut socket, &ready).await {
                exec::relay(socket, session, terminal.idle, terminal.signed_out.clone()).await;
            }
            if let (Some(closing), Some(zeabur)) =
                (state.tools.closed(&key, (state.clock)()), zeabur)
            {
                after_close(
                    Arc::clone(&state.tools),
                    zeabur,
                    namespace,
                    pod,
                    closing,
                    settings.tools_grace,
                );
            }
        }
        Err(problem) => {
            let _last = state.tools.closed(&key, (state.clock)());
            let frame = ServerFrame::Error { problem: problem.0 };
            if exec::send_frame(&mut socket, &frame).await {
                exec::close(&mut socket, 1000, "").await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pod_is_deleted_only_when_no_terminal_opened_since_the_last_closed() {
        let sessions = Sessions::default();
        let now = Timestamp::UNIX_EPOCH;
        sessions.opened("ns/p");
        sessions.opened("ns/p");
        assert_eq!(sessions.closed("ns/p", now), None);
        let first = sessions.closed("ns/p", now).unwrap();
        sessions.opened("ns/p");
        let second = sessions.closed("ns/p", now).unwrap();
        assert!(!sessions.still_closed("ns/p", first));
        assert!(sessions.still_closed("ns/p", second));
        assert_eq!(sessions.terminals("ns/p"), 0);
    }

    #[test]
    fn a_pod_is_idle_past_the_grace_period_only_while_no_terminal_is_open() {
        let sessions = Sessions::default();
        let at = |s: i64| Timestamp::from_second(s).unwrap();
        let grace = std::time::Duration::from_secs(60);
        sessions.opened("ns/p");
        assert!(!sessions.idle_past("ns/p", grace, at(1000)));
        let _last = sessions.closed("ns/p", at(0));
        assert!(!sessions.idle_past("ns/p", grace, at(59)));
        assert!(sessions.idle_past("ns/p", grace, at(60)));
        sessions.opened("ns/p");
        assert!(!sessions.idle_past("ns/p", grace, at(120)));
        sessions.forget("ns/p");
        assert!(!sessions.idle_past("ns/p", grace, at(120)));
    }

    #[test]
    fn a_stopped_or_overdue_pod_is_done_with() {
        let now = "2026-10-21T01:00:00Z".parse::<Timestamp>().unwrap();
        let pod = |phase: &str, created: &str| {
            json!({"metadata": {"creationTimestamp": created},
                   "spec": {"activeDeadlineSeconds": 1800},
                   "status": {"phase": phase}})
        };
        assert!(!finished(&pod("Running", "2026-10-21T00:40:00Z"), now));
        assert!(finished(&pod("Failed", "2026-10-21T00:59:00Z"), now));
        assert!(finished(&pod("Running", "2026-10-21T00:20:00Z"), now));
    }

    #[test]
    fn the_kubeconfig_names_the_in_cluster_server_the_ca_and_the_project() {
        let handoff = Handoff {
            token: zeabur_kube::UserToken::new("t0ken".into()).unwrap(),
            expires_at: Timestamp::UNIX_EPOCH,
        };
        let config: Value = serde_json::from_slice(&kubeconfig(&handoff, "alice", "shop")).unwrap();
        assert_eq!(
            config["clusters"][0]["cluster"]["server"],
            IN_CLUSTER_SERVER
        );
        assert_eq!(
            config["clusters"][0]["cluster"]["certificate-authority"],
            plans::TOOLS_CA
        );
        assert_eq!(config["contexts"][0]["context"]["namespace"], "shop");
        assert_eq!(config["users"][0]["user"]["token"], "t0ken");
    }
}
