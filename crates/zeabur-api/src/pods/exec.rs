//! A terminal in a container, over a WebSocket, as the person.
//!
//! `GET /api/v1/projects/{project}/pods/{pod}/exec?container=…&command=…` upgrades to a
//! WebSocket [rfc6455] and runs the command in the container through `pods/exec` with the
//! person's credential, with standard input and a TTY, as `kubectl exec -it` does
//! [k8s-exec]. Opening it takes `create` on `pods/exec`, which Zeabur asks kube-apiserver about
//! first (a `SelfSubjectAccessReview`): the WebSocket handshake is a GET, which RBAC checks as
//! `get` [k8s-verbs], and only from Kubernetes 1.31 does kube-apiserver also require `create`, as
//! it always has for `kubectl exec` over SPDY [kep-4006]. Like every request, the terminal goes
//! with the person's credential
//! ([`ClusterAccess::for_request`](crate::access::ClusterAccess::for_request)).
//! `command` may repeat, one argument each, as in the Kubernetes API; it
//! defaults to `/bin/sh`. Images without a shell (distroless images [distroless]) end the session
//! at once with `command-not-found`, which the interface explains; a `debugContainer` plan adds
//! an ephemeral container with a shell to the pod, and `container` names it to open a terminal
//! there ([`super::debug`]).
//!
//! # Protocol
//!
//! Binary messages carry terminal bytes: from the browser, standard input; to it, what the
//! terminal shows (standard output and standard error, which the TTY merges). Text messages are
//! JSON control frames, tagged by `type` ([`ClientFrame`], [`ServerFrame`]):
//!
//! | Direction | Frame | Meaning |
//! |---|---|---|
//! | to the server | `{"type":"resize","cols":80,"rows":24}` | the terminal's new size |
//! | to the browser | `{"type":"ready","container":"web"}` | the process runs; bytes may flow |
//! | to the browser | `{"type":"exit","code":0}` | the process ended; `reason` `command-not-found` or `failed` (with `message`) instead of a code |
//! | to the browser | `{"type":"error","problem":{…}}` | the terminal could not be opened (a problem) |
//!
//! Unknown frames are ignored, so either side may add some. After `exit` or `error` the server
//! closes with `1000`. It also closes on its own with **`4001` "idle, reconnect"** after [`IDLE`]
//! without a byte either way. The kubelet has the same kind of limit
//! (`--streaming-connection-idle-timeout`, four hours by default [kubelet]); a browser tab is left
//! open more easily than a shell, so Zeabur's is shorter.
//!
//! Codes 4000–4999 are for private use [rfc6455] (§7.4.2).
//!
//! # Origin
//!
//! A WebSocket handshake is a `GET`, which the browser sends with the person's cookies from any
//! page, and the same-origin policy does not cover a WebSocket: the server must check the
//! `Origin` header itself [rfc6455] (§10.2). The upgrade is refused unless `Origin` names this
//! server and, when the browser sends it, `Sec-Fetch-Site` is `same-origin` [fetch-metadata].
//! "This server" is the configured public origin
//! ([`ClusterAccess::public_origin`](crate::access::ClusterAccess::public_origin)) in the
//! in-cluster server; locally, where no public URL is configured, it is scheme `http` or `https`
//! with the authority equal to `Host`. A request without `Origin` is
//! refused: browsers always send it on a WebSocket handshake [rfc6455] (§4.1).
//!
//! # Privacy
//!
//! Nothing a terminal carries is logged or kept: bytes are copied from one side to the other.
//!
//! # References
//!
//! - [rfc6455] I. Fette and A. Melnikov, "The WebSocket Protocol," RFC 6455, IETF, Dec. 2011. <https://www.rfc-editor.org/rfc/rfc6455>
//! - [k8s-exec] The Kubernetes Authors, "Get a Shell to a Running Container," Kubernetes
//!   Documentation. <https://kubernetes.io/docs/tasks/debug/debug-application/get-shell-running-container/>
//! - [distroless] Google, "Distroless container images." <https://github.com/GoogleContainerTools/distroless>
//! - [kubelet] The Kubernetes Authors, "kubelet," Kubernetes Documentation. <https://kubernetes.io/docs/reference/command-line-tools-reference/kubelet/>
//! - [fetch-metadata] W3C, "Fetch Metadata Request Headers," Working Draft. <https://www.w3.org/TR/fetch-metadata/>
//!
//! [rfc6455]: https://www.rfc-editor.org/rfc/rfc6455
//! [k8s-exec]: https://kubernetes.io/docs/tasks/debug/debug-application/get-shell-running-container/
//! [k8s-verbs]: https://kubernetes.io/docs/reference/access-authn-authz/authorization/#determine-the-request-verb
//! [kep-4006]: https://github.com/kubernetes/enhancements/tree/master/keps/sig-api-machinery/4006-transition-spdy-to-websockets
//! [distroless]: https://github.com/GoogleContainerTools/distroless
//! [kubelet]: https://kubernetes.io/docs/reference/command-line-tools-reference/kubelet/
//! [fetch-metadata]: https://www.w3.org/TR/fetch-metadata/

#![expect(
    unused_qualifications,
    reason = "utoipa's `path` macro expands `params(ExecParams)` to a qualified path"
)]

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRef, FromRequestParts, Path, RawQuery};
use axum::http::request::Parts;
use axum::http::{HeaderMap, header};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use utoipa::{IntoParams, ToSchema};
use zeabur_core::guard::{self, Origins};
use zeabur_core::problem::{Denied, Kind, Problem};
use zeabur_kube::{
    Cluster, ClusterExt as _, ExecRequest, ExecSession, ExitStatus, ResourceRef, TerminalSize,
};

use crate::error::{ApiError, get_read, read_problem, review_refusal};
use crate::handlers::AppState;

/// How long a terminal may stay without a byte either way before the server closes it.
pub const IDLE: Duration = Duration::from_mins(15);

/// The command run when the request names none.
const DEFAULT_COMMAND: &str = "/bin/sh";

/// The largest message the browser may send; a paste larger than this arrives in pieces.
pub(crate) const MAX_MESSAGE: usize = 64 * 1024;

/// Standard input waiting for the process: once this much is pending, the server stops reading
/// from the browser until the process takes it.
const MAX_PENDING_INPUT: usize = 256 * 1024;

/// How long the output may take to end after the process has exited.
const DRAIN: Duration = Duration::from_secs(2);

/// The close code after [`IDLE`].
pub const CLOSE_IDLE: u16 = 4001;

/// A control frame from the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClientFrame {
    /// The terminal's new size, in character cells.
    Resize {
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
    },
}

/// How a terminal's process ended, when it gave no exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ExitReason {
    /// The command is not in the image: a distroless image has no shell.
    CommandNotFound,
    /// The container runtime or the connection ended it; see `message`.
    Failed,
}

/// A control frame to the browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ServerFrame {
    /// The process runs; terminal bytes may flow.
    Ready {
        /// The container it runs in.
        container: String,
    },
    /// The process ended.
    Exit {
        /// Its exit code, when it exited on its own.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<i32>,
        /// Why it ended, when there is no code.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<ExitReason>,
        /// The container runtime's message, for `failed`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// The terminal could not be opened.
    Error {
        /// What went wrong.
        problem: Box<Problem>,
    },
}

impl From<ExitStatus> for ServerFrame {
    fn from(status: ExitStatus) -> Self {
        let (code, reason, message) = match status {
            ExitStatus::Exited(code) => (Some(code), None, None),
            ExitStatus::CommandNotFound => (None, Some(ExitReason::CommandNotFound), None),
            ExitStatus::Failed(message) => (None, Some(ExitReason::Failed), Some(message)),
        };
        Self::Exit {
            code,
            reason,
            message,
        }
    }
}

/// The parameters of a terminal (for the description; parsed by [`ExecQuery`]).
#[expect(
    dead_code,
    reason = "describes the parameters in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, Default, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ExecParams {
    /// The container; by default the one the pod's `kubectl.kubernetes.io/default-container`
    /// annotation names, or its first.
    container: Option<String>,
    /// The command, one argument per parameter (`command=ls&command=-l`); `/bin/sh` by default.
    command: Option<Vec<String>>,
}

/// A terminal's query string.
#[derive(Debug, Default)]
struct ExecQuery {
    container: Option<String>,
    command: Vec<String>,
}

impl ExecQuery {
    fn parse(query: Option<&str>) -> Self {
        let mut parsed = Self::default();
        for (name, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
            match &*name {
                "container" if !value.is_empty() => parsed.container = Some(value.into_owned()),
                "command" => parsed.command.push(value.into_owned()),
                _ => {}
            }
        }
        if parsed.command.is_empty() {
            parsed.command.push(DEFAULT_COMMAND.to_owned());
        }
        parsed
    }
}

/// Whether the handshake comes from a page of this server, `public` when one is configured (see
/// the module documentation), with [`guard::websocket_allowed`].
fn same_origin(headers: &HeaderMap, public: Option<&str>) -> bool {
    let origins = public.map_or_else(
        || {
            headers
                .get(header::HOST)
                .and_then(|host| host.to_str().ok())
                .map(|host| Origins::new([format!("http://{host}"), format!("https://{host}")]))
        },
        |public| Some(Origins::new([public.to_owned()])),
    );
    origins.is_some_and(|origins| guard::websocket_allowed(&origins, headers))
}

/// The person opening a terminal: their cluster and how long a terminal may idle. Refuses a
/// handshake from another origin before anything else.
pub(crate) struct Terminal {
    pub(crate) cluster: Arc<dyn Cluster>,
    pub(crate) idle: Duration,
    /// The person's token, for a tools pod (see the `tools` module).
    pub(crate) handoff: Option<crate::access::Handoff>,
}

impl<S> FromRequestParts<S> for Terminal
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let state = AppState::from_ref(state);
        let terminal = if same_origin(&parts.headers, state.access.public_origin()) {
            state
                .access
                .for_request(parts)
                .map_err(ApiError)
                .map(|cluster| Self {
                    cluster,
                    idle: state.terminal_idle,
                    handoff: state.access.handoff(parts),
                })
        } else {
            Err(ApiError::from(Problem::new(
                Kind::Forbidden,
                "Only the Zeabur page itself can open a terminal.",
            )))
        };
        std::future::ready(terminal)
    }
}

/// A terminal in a container, over a WebSocket.
///
/// Binary messages carry terminal bytes both ways; text messages are JSON control frames,
/// `ClientFrame` to the server and `ServerFrame` to the browser. The handshake answers `101
/// Switching Protocols`; every later failure (`403` with `denied` naming `create` on
/// `pods/exec`; a pod or container that does not exist, a container that is not running) arrives
/// as an `error` frame, which a browser can read where it cannot read a refused handshake. After
/// `exit` or `error` the server closes with `1000`; it closes with `4001` after 15 minutes without
/// a byte either way. A handshake
/// from another origin is refused with `403`.
#[utoipa::path(get, path = "/api/v1/projects/{project}/pods/{pod}/exec", tag = "pods",
    params(("project" = String, Path), ("pod" = String, Path), ExecParams),
    responses((status = 101, description = "Switched to the terminal's WebSocket"),
              (status = 401, body = Problem, content_type = "application/problem+json"),
              (status = 403, body = Problem, content_type = "application/problem+json")))]
pub(crate) async fn exec(
    terminal: Terminal,
    Path((project, pod)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    upgrade: WebSocketUpgrade,
) -> Response {
    let query = ExecQuery::parse(query.as_deref());
    upgrade
        .max_message_size(MAX_MESSAGE)
        .on_upgrade(move |socket| run(socket, terminal, project, pod, query))
}

async fn run(
    mut socket: WebSocket,
    terminal: Terminal,
    project: String,
    pod: String,
    query: ExecQuery,
) {
    let opened = open(&*terminal.cluster, &project, &pod, query).await;
    let (container, session) = match opened {
        Ok(opened) => opened,
        Err(problem) => {
            let frame = ServerFrame::Error { problem: problem.0 };
            if send_frame(&mut socket, &frame).await {
                close(&mut socket, 1000, "").await;
            }
            return;
        }
    };
    if send_frame(&mut socket, &ServerFrame::Ready { container }).await {
        relay(socket, session, terminal.idle).await;
    }
}

/// Checks the pod and container as `kubectl exec` does (it reads the pod first), then starts the
/// command.
async fn open(
    cluster: &dyn Cluster,
    project: &str,
    pod: &str,
    query: ExecQuery,
) -> Result<(String, ExecSession), ApiError> {
    outside_node_shells(project)?;
    let pods = ResourceRef::new("", "v1", "pods", "Pod");
    let what = format!("pod {pod}");
    let object = cluster
        .find_object(&pods, Some(project), pod)
        .await
        .map_err(|e| read_problem(e, &what, get_read("", "pods", Some(project), pod)))?
        .ok_or_else(|| Problem::new(Kind::NotFound, format!("There is no pod named {pod}.")))?;
    let container = pick_container(&object.data, pod, query.container)?;
    let right = exec_right(project, pod);
    if matches!(cluster.review_access(&right).await, Ok(false)) {
        return Err(exec_refusal(review_refusal(
            &format!("a terminal in pod {pod}"),
            right,
        )));
    }
    let request = ExecRequest {
        container: container.clone(),
        command: query.command,
    };
    let session = cluster
        .exec(project, pod, &request)
        .await
        .map_err(|e| exec_refusal(read_problem(e, &format!("a terminal in pod {pod}"), right)))?;
    Ok((container, session))
}

/// Refused for the namespace of node shells: their terminal is `cluster::node_shells`', for
/// cluster administrators and whoever opened the shell only, and their files are the host's.
pub(crate) fn outside_node_shells(namespace: &str) -> Result<(), ApiError> {
    if namespace == zeabur_core::names::NODE_SHELL_NAMESPACE {
        return Err(Problem::new(
            Kind::Forbidden,
            "Node shells open from the node's page, and only for whoever opened them.",
        )
        .into());
    }
    Ok(())
}

/// `create` on `pods/exec`, which a terminal takes.
pub(crate) fn exec_right(namespace: &str, pod: &str) -> Denied {
    Denied {
        verb: "create".to_owned(),
        group: String::new(),
        resource: "pods/exec".to_owned(),
        name: Some(pod.to_owned()),
        namespace: Some(namespace.to_owned()),
    }
}

/// R8: the one right a terminal needs, which an administrator can grant in this project alone;
/// the web interface shows the Role and RoleBinding.
pub(crate) fn exec_refusal(mut refusal: ApiError) -> ApiError {
    if refusal.0.kind == Kind::Forbidden {
        refusal.0.fix_for_occasion(
            "Ask a cluster administrator for `get` and `create` on `pods/exec` in this \
             project; it grants only that, in this project only.",
            zeabur_core::message::Message::new("problem.forbidden.fix.sendGrant"),
        );
    }
    refusal
}

fn names<'a>(pod: &'a Value, path: &str) -> Vec<&'a str> {
    pod.pointer(path)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("name")?.as_str())
        .collect()
}

/// The container asked for, or the pod's default; refused when the pod has no such container or
/// it is not running. An ephemeral container (a debug container) is one only when asked for by
/// name.
pub(crate) fn pick_container(
    pod: &Value,
    name: &str,
    asked: Option<String>,
) -> Result<String, ApiError> {
    let containers = names(pod, "/spec/containers");
    let ephemeral = names(pod, "/spec/ephemeralContainers");
    let default = pod
        .pointer("/metadata/annotations/kubectl.kubernetes.io~1default-container")
        .and_then(Value::as_str)
        .filter(|d| containers.contains(d))
        .or_else(|| containers.first().copied());
    let container = match asked {
        Some(asked)
            if containers.contains(&asked.as_str()) || ephemeral.contains(&asked.as_str()) =>
        {
            asked
        }
        Some(asked) => {
            return Err(Problem::new(
                Kind::NotFound,
                format!("Pod {name} has no container {asked}."),
            )
            .into());
        }
        None => default
            .ok_or_else(|| Problem::new(Kind::NotFound, format!("Pod {name} has no containers.")))?
            .to_owned(),
    };
    let state = [
        "/status/containerStatuses",
        "/status/ephemeralContainerStatuses",
    ]
    .into_iter()
    .filter_map(|path| pod.pointer(path).and_then(Value::as_array))
    .flatten()
    .find(|s| s.get("name").and_then(Value::as_str) == Some(container.as_str()))
    .and_then(|s| s.get("state"));
    if state.and_then(|s| s.get("running")).is_some() {
        return Ok(container);
    }
    let why = state
        .and_then(|s| {
            s.pointer("/waiting/reason")
                .or_else(|| s.pointer("/terminated/reason"))
        })
        .and_then(Value::as_str)
        .map_or_else(String::new, |reason| format!(" ({reason})"));
    let mut problem = Problem::new(
        Kind::Conflict,
        format!("Container {container} in pod {name} is not running{why}."),
    );
    problem.fix_for_occasion(
        "Wait until it runs; its logs and events say why it doesn't.",
        zeabur_core::message::Message::new("problem.conflict.fix.waitForContainer"),
    );
    Err(problem.into())
}

/// Copies bytes both ways until the process ends, the browser leaves or the terminal idles.
/// Standard input is written as the process takes it, so a process that is busy writing output
/// never blocks the reading of more input, nor the other way round.
pub(crate) async fn relay(mut socket: WebSocket, session: ExecSession, idle: Duration) {
    let ExecSession {
        mut input,
        mut output,
        mut resize,
        mut exit,
    } = session;
    let idle_timer = tokio::time::sleep(idle);
    tokio::pin!(idle_timer);
    let mut pending = Vec::new();
    let mut buffer = vec![0_u8; 16 * 1024];
    let mut output_open = true;
    loop {
        let mut active = true;
        tokio::select! {
            message = socket.recv(), if pending.len() < MAX_PENDING_INPUT => match message {
                Some(Ok(Message::Binary(bytes))) => pending.extend_from_slice(&bytes),
                Some(Ok(Message::Text(text))) => {
                    if let Ok(ClientFrame::Resize { cols, rows }) = serde_json::from_str(&text) {
                        resize(TerminalSize { cols, rows });
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => active = false,
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
            },
            written = input.write(&pending), if !pending.is_empty() => match written {
                Ok(written) => drop(pending.drain(..written.min(pending.len()))),
                // The process closed its standard input; what is typed has nowhere to go.
                Err(_) => pending.clear(),
            },
            read = output.read(&mut buffer), if output_open => match read {
                Ok(0) | Err(_) => output_open = false,
                Ok(read) => {
                    let chunk = Bytes::copy_from_slice(buffer.get(..read).unwrap_or_default());
                    if socket.send(Message::Binary(chunk)).await.is_err() {
                        return;
                    }
                }
            },
            status = &mut exit => {
                if output_open && !drain(&mut socket, &mut output, &mut buffer).await {
                    return;
                }
                if send_frame(&mut socket, &ServerFrame::from(status)).await {
                    close(&mut socket, 1000, "").await;
                }
                return;
            },
            () = &mut idle_timer => {
                close(&mut socket, CLOSE_IDLE, "idle, reconnect").await;
                return;
            },
        }
        if active {
            idle_timer
                .as_mut()
                .reset(tokio::time::Instant::now() + idle);
        }
    }
}

/// Sends what is left of the output after the process exited, until it ends or [`DRAIN`] has
/// passed; `false` when the browser is gone.
async fn drain(
    socket: &mut WebSocket,
    output: &mut (impl tokio::io::AsyncRead + Unpin + ?Sized),
    buffer: &mut [u8],
) -> bool {
    let deadline = tokio::time::Instant::now() + DRAIN;
    loop {
        match tokio::time::timeout_at(deadline, output.read(buffer)).await {
            Ok(Ok(read)) if read > 0 => {
                let chunk = Bytes::copy_from_slice(buffer.get(..read).unwrap_or_default());
                if socket.send(Message::Binary(chunk)).await.is_err() {
                    return false;
                }
            }
            Ok(Ok(_) | Err(_)) | Err(_) => return true,
        }
    }
}

/// Sends a control frame; `false` when the browser is gone.
pub(crate) async fn send_frame(socket: &mut WebSocket, frame: &ServerFrame) -> bool {
    let Ok(text) = serde_json::to_string(frame) else {
        return false;
    };
    socket.send(Message::Text(text.into())).await.is_ok()
}

pub(crate) async fn close(socket: &mut WebSocket, code: u16, reason: &'static str) {
    let frame = CloseFrame {
        code,
        reason: reason.into(),
    };
    // The browser may already be gone; there is nobody left to tell.
    let _gone = socket.send(Message::Close(Some(frame))).await;
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        pairs
            .iter()
            .map(|(name, value)| {
                (
                    header::HeaderName::from_static(name),
                    HeaderValue::from_static(value),
                )
            })
            .collect()
    }

    #[test]
    fn only_a_page_of_this_server_may_open_a_terminal() {
        let host = ("host", "zeabur.example:8443");
        for (origin, site, allowed) in [
            ("https://zeabur.example:8443", None, true),
            ("http://zeabur.example:8443", Some("same-origin"), true),
            ("https://ZEABUR.example:8443", None, true),
            ("https://zeabur.example:8443", Some("same-site"), false),
            ("https://zeabur.example:8443", Some("cross-site"), false),
            ("https://attacker.example", None, false),
            ("https://zeabur.example", None, false),
            ("https://zeabur.example:8443.attacker.example", None, false),
            ("null", None, false),
            ("ws://zeabur.example:8443", None, false),
        ] {
            let mut pairs = vec![host, ("origin", origin)];
            pairs.extend(site.map(|s| ("sec-fetch-site", s)));
            assert_eq!(
                same_origin(&headers(&pairs), None),
                allowed,
                "{origin} {site:?}"
            );
        }
        assert!(!same_origin(&headers(&[host]), None));
        assert!(!same_origin(
            &headers(&[("origin", "https://zeabur.example:8443")]),
            None
        ));
    }

    /// With a public URL, `Origin` must be exactly its origin, whatever `Host` says: a proxy may
    /// rewrite `Host`, and a plain `http` page of the same host is another origin.
    #[test]
    fn with_a_public_url_only_its_pages_may_open_a_terminal() {
        let public = Some("https://zeabur.example.com");
        for (origin, host, site, allowed) in [
            (
                "https://zeabur.example.com",
                "zeabur.example.com",
                None,
                true,
            ),
            (
                "https://zeabur.example.com",
                "zeabur.zeabur.svc",
                Some("same-origin"),
                true,
            ),
            (
                "https://ZEABUR.example.com",
                "zeabur.example.com",
                None,
                true,
            ),
            (
                "http://zeabur.example.com",
                "zeabur.example.com",
                None,
                false,
            ),
            (
                "https://zeabur.example.com:8443",
                "zeabur.example.com:8443",
                None,
                false,
            ),
            ("https://attacker.example", "attacker.example", None, false),
            (
                "https://zeabur.example.com",
                "zeabur.example.com",
                Some("same-site"),
                false,
            ),
            ("null", "zeabur.example.com", None, false),
        ] {
            let mut pairs = vec![("host", host), ("origin", origin)];
            pairs.extend(site.map(|s| ("sec-fetch-site", s)));
            assert_eq!(
                same_origin(&headers(&pairs), public),
                allowed,
                "{origin} {host} {site:?}"
            );
        }
        assert!(!same_origin(
            &headers(&[("host", "zeabur.example.com")]),
            public
        ));
    }

    #[test]
    fn the_command_defaults_to_a_shell_and_keeps_its_arguments_in_order() {
        let query = ExecQuery::parse(Some("container=web"));
        assert_eq!(query.command, ["/bin/sh"]);
        let query = ExecQuery::parse(Some("command=ls&command=-l&command=%2Fetc%20x"));
        assert_eq!(query.command, ["ls", "-l", "/etc x"]);
        assert_eq!(query.container, None);
    }
}
