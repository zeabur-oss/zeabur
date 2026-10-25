//! A debug container and a tools pod on a real cluster, as the person: the debug container is
//! added to a pod whose image has no shell and a terminal opens in it beside the target's
//! processes; the tools pod starts locked down with no token in its spec, in a terminal namespace
//! the person cannot read, and kubectl in it acts as the person with the kubeconfig written
//! through standard input; it is gone once its terminal closes. A second sandbox's person stands
//! in for Zeabur's own service account, with the rights the chart gives it.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::http::request::Parts;
use futures_util::{SinkExt as _, StreamExt as _};
use kube::api::Api;
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use zeabur_api::Settings;
use zeabur_api::access::{Access, ClusterAccess, Elevation, Handoff};
use zeabur_core::problem::{Denied, Problem};
use zeabur_kube::Cluster;
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};
use zeabur_test_support::sandbox::{PERSON, Sandbox, create};

use crate::common::send;

/// Long enough for a node to pull an image.
const WAIT: Duration = Duration::from_secs(180);

struct Signed {
    cluster: Arc<dyn Cluster>,
    token: String,
}

impl ClusterAccess for Signed {
    fn for_request(&self, _parts: &Parts) -> Result<Access, Box<Problem>> {
        Ok(Access {
            cluster: Arc::clone(&self.cluster),
            elevation: Elevation::Unavailable,
        })
    }

    fn credential_expiry(&self, _parts: &Parts) -> Option<Timestamp> {
        Some(Timestamp::now() + SignedDuration::from_mins(10))
    }

    fn handoff(&self, _parts: &Parts) -> Option<Handoff> {
        Some(Handoff {
            token: zeabur_kube::UserToken::new(SecretString::from(self.token.as_str())).unwrap(),
            expires_at: Timestamp::now() + SignedDuration::from_mins(10),
            admin: false,
        })
    }
}

fn app(sandbox: &Sandbox) -> Router {
    let mut settings = Settings::default();
    settings.terminal.tools_grace = Duration::ZERO;
    zeabur_api::router_with_settings(
        Arc::new(Signed {
            cluster: Arc::new(sandbox.user().clone()),
            token: sandbox.token().to_owned(),
        }),
        settings,
    )
}

fn with_terminals(sandbox: &Sandbox, terminals: &Sandbox) -> Router {
    let mut settings = Settings::default();
    settings.terminal.tools_grace = Duration::ZERO;
    terminals
        .namespace()
        .clone_into(&mut settings.terminal.namespace);
    zeabur_api::router_with_terminals(
        Arc::new(Signed {
            cluster: Arc::new(sandbox.user().clone()),
            token: sandbox.token().to_owned(),
        }),
        settings,
        Arc::new(terminals.user().clone()),
    )
}

/// The terminal namespace, whose person has the Role the chart gives Zeabur there.
async fn terminals() -> Sandbox {
    Sandbox::new(
        "zeabur-terminals",
        json!([
            {"apiGroups": [""], "resources": ["pods"],
             "verbs": ["create", "get", "list", "delete"]},
            {"apiGroups": [""], "resources": ["pods/exec"], "verbs": ["get", "create"]}
        ]),
    )
    .await
}

async fn sandbox() -> Sandbox {
    Sandbox::new(
        "zeabur-terminal",
        json!([
            {"apiGroups": [""], "resources": ["pods"],
             "verbs": ["create", "get", "list", "delete"]},
            {"apiGroups": [""], "resources": ["pods/exec"], "verbs": ["get", "create"]},
            {"apiGroups": [""], "resources": ["pods/ephemeralcontainers"],
             "verbs": ["get", "patch"]},
            {"apiGroups": [""], "resources": ["configmaps"], "verbs": ["get", "list"]}
        ]),
    )
    .await
}

async fn plan_and_apply(app: &Router, project: &str, change: Value) -> Value {
    let (status, planned) = send(
        app,
        "POST",
        &format!("/api/v1/projects/{project}/plan"),
        Some(&json!({"changes": [change]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{planned}");
    let (status, applied) = send(
        app,
        "POST",
        &format!("/api/v1/projects/{project}/apply"),
        Some(&json!({"token": planned["token"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    planned
}

/// Asks `uri` until `done` holds of the answer.
async fn until(app: &Router, uri: &str, done: impl Fn(&Value) -> bool) -> Value {
    let poll = async {
        loop {
            let (_, answer) = send(app, "GET", uri, None).await;
            if done(&answer) {
                return answer;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };
    tokio::time::timeout(WAIT, poll)
        .await
        .unwrap_or_else(|_| panic!("{uri} never got there"))
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn serve(router: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    address
}

/// Opens the terminal at `path`, types `input`, and returns the first frame and everything the
/// terminal showed until the process exited.
async fn session(router: Router, path: &str, input: &str) -> (Value, String) {
    let address = serve(router).await;
    let mut request = format!("ws://{address}{path}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("origin", format!("http://{address}").parse().unwrap());
    let mut socket: Socket = tokio_tungstenite::connect_async(request).await.unwrap().0;
    let mut first = None;
    let mut shown = Vec::new();
    let read = async {
        while let Some(message) = socket.next().await {
            match message.unwrap() {
                Message::Text(text) => {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    if frame["type"] == "ready" {
                        socket
                            .send(Message::binary(input.as_bytes().to_vec()))
                            .await
                            .unwrap();
                    }
                    if first.is_none() {
                        first = Some(frame.clone());
                    }
                    if frame["type"] != "ready" {
                        break;
                    }
                }
                Message::Binary(bytes) => shown.extend_from_slice(&bytes),
                Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => {}
            }
        }
    };
    tokio::time::timeout(WAIT, read)
        .await
        .expect("the terminal ended in time");
    (first.unwrap(), String::from_utf8_lossy(&shown).into_owned())
}

#[tokio::test]
#[ignore = "needs a disposable cluster"]
async fn a_debug_container_opens_a_shell_beside_a_container_without_one() {
    let sandbox = sandbox().await;
    let ns = sandbox.namespace().to_owned();
    let pods = Api::<Pod>::namespaced(sandbox.admin().clone(), &ns);
    create(
        &pods,
        json!({"metadata": {"name": "paused"},
               "spec": {"securityContext": {"runAsNonRoot": true, "runAsUser": 65535},
                        "containers": [{"name": "pause", "image": "registry.k8s.io/pause:3.10"}]}}),
    )
    .await;
    let running = async {
        while pods
            .get("paused")
            .await
            .unwrap()
            .status
            .and_then(|s| s.phase)
            .as_deref()
            != Some("Running")
        {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };
    tokio::time::timeout(WAIT, running)
        .await
        .expect("the pod runs");
    let app = app(&sandbox);
    let status_uri = format!("/api/v1/projects/{ns}/pods/paused/debug");

    let planned = plan_and_apply(&app, &ns, json!({"op": "debugContainer", "pod": "paused"})).await;
    let stays = &planned["warnings"][0];
    assert_eq!(stays["code"], "debugContainerStays", "{planned}");
    let name = stays["container"].as_str().unwrap().to_owned();
    let status = until(&app, &status_uri, |s| s["container"]["state"] == "running").await;
    assert_eq!(status["container"]["name"], name.as_str());

    let stored = serde_json::to_value(pods.get("paused").await.unwrap()).unwrap();
    let added = &stored["spec"]["ephemeralContainers"][0];
    assert_eq!(added["targetContainerName"], "pause");
    assert_eq!(added["securityContext"]["runAsUser"], 65535);
    assert_eq!(
        added["securityContext"]["capabilities"]["drop"],
        json!(["ALL"])
    );

    let (ready, shown) = session(
        app.clone(),
        &format!("/api/v1/projects/{ns}/pods/paused/exec?container={name}"),
        "echo debug-$((40+2)); id -u; ps; exit\r",
    )
    .await;
    assert_eq!(ready, json!({"type": "ready", "container": name}));
    assert!(shown.contains("debug-42"), "{shown}");
    assert!(shown.contains("65535"), "{shown}");
    assert!(
        shown.contains("/pause"),
        "the target's processes are in sight: {shown}"
    );

    let again = plan_and_apply(&app, &ns, json!({"op": "debugContainer", "pod": "paused"})).await;
    assert_eq!(again["objects"], json!([]));
    assert_eq!(again["warnings"][0]["code"], "debugContainerReused");
}

#[tokio::test]
#[ignore = "needs a disposable cluster"]
async fn kubectl_in_a_tools_pod_acts_as_the_person_and_the_pod_goes_after() {
    let sandbox = sandbox().await;
    let terminals = terminals().await;
    let ns = sandbox.namespace().to_owned();
    let app = with_terminals(&sandbox, &terminals);
    let planned = plan_and_apply(&app, &ns, json!({"op": "clusterTools"})).await;
    assert!(!planned.to_string().contains(sandbox.token()));
    let name = planned["warnings"][0]["pod"].as_str().unwrap().to_owned();

    let pods = terminals.admin_api::<Pod>();
    let theirs = Api::<Pod>::namespaced(sandbox.user().kube().clone(), terminals.namespace());
    assert!(
        theirs
            .list(&kube::api::ListParams::default())
            .await
            .is_err(),
        "the person cannot list tools pods"
    );
    let exec = Denied {
        verb: "create".to_owned(),
        group: String::new(),
        resource: "pods/exec".to_owned(),
        name: Some(name.clone()),
        namespace: Some(terminals.namespace().to_owned()),
    };
    assert!(
        !sandbox.user().review_access(&exec).await.unwrap(),
        "the person cannot run commands in it past Zeabur"
    );
    let stored = serde_json::to_value(pods.get(&name).await.unwrap()).unwrap();
    assert!(!stored.to_string().contains(sandbox.token()));
    assert_eq!(stored["spec"]["automountServiceAccountToken"], false);
    assert_eq!(
        stored["spec"]["containers"][0]["securityContext"]["readOnlyRootFilesystem"],
        true
    );

    let tools_uri = format!("/api/v1/projects/{ns}/tools");
    until(&app, &tools_uri, |t| t["pods"][0]["ready"] == true).await;
    let (ready, shown) = session(
        app.clone(),
        &format!("/api/v1/projects/{ns}/tools/{name}/terminal"),
        "kubectl auth whoami -o jsonpath='{.status.userInfo.username}'; echo; \
         kubectl get configmaps -o name; kubectl get secrets; \
         ls /var/run/secrets/kubernetes.io 2>&1; exit\r",
    )
    .await;
    assert_eq!(ready, json!({"type": "ready", "container": "tools"}));
    assert!(
        shown.contains(&format!("system:serviceaccount:{ns}:{PERSON}")),
        "{shown}"
    );
    assert!(shown.contains("configmap/kube-root-ca.crt"), "{shown}");
    assert!(
        shown.contains("forbidden"),
        "only the person's rights: {shown}"
    );
    assert!(
        shown.contains("No such file"),
        "no service account token: {shown}"
    );
    assert!(!shown.contains(sandbox.token()));

    let gone = async {
        while pods.get_opt(&name).await.unwrap().is_some() {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };
    tokio::time::timeout(WAIT, gone)
        .await
        .expect("the tools pod is deleted after its terminal closes");
}
