//! A debug container and a tools pod against an in-memory cluster: the plans and what they say,
//! the writes, reuse, refusals in plain words, the terminal in each, and the tools pod's
//! credential reaching it only through a command's standard input; deleting the project takes
//! the person's tools pods of it along. Tools pods run in the terminal namespace, which only
//! Zeabur's own service account uses; the person cannot see it.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::Router;
    use axum::http::StatusCode;
    use axum::http::request::Parts;
    use futures_util::{SinkExt as _, StreamExt as _};
    use secrecy::SecretString;
    use serde_json::{Value, json};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    use zeabur_api::Settings;
    use zeabur_api::access::{ClusterAccess, Handoff, SingleCluster};
    use zeabur_core::problem::Problem;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Pod};
    use zeabur_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Cluster, Identity, NamespaceObjects, WriteBody};

    use crate::common::send;

    const TOKEN: &str = "eyJhbGciOi.secret-token-of-alice.sig";
    const WAIT: Duration = Duration::from_secs(5);
    const TERMINALS: &str = "zeabur-terminals";

    fn pod(value: Value) -> Pod {
        serde_json::from_value(value).unwrap()
    }

    /// A pod whose image has no shell.
    fn distroless() -> Pod {
        pod(json!({
            "metadata": {"name": "api-1", "namespace": "shop", "uid": "uid-api-1",
                         "resourceVersion": "7"},
            "spec": {
                "securityContext": {"runAsNonRoot": true, "runAsUser": 65532},
                "containers": [{"name": "api", "image": "gcr.io/distroless/static:nonroot"}]
            },
            "status": {"phase": "Running", "containerStatuses": [
                {"name": "api", "image": "x", "imageID": "", "ready": true, "restartCount": 0,
                 "state": {"running": {}}}
            ]}
        }))
    }

    fn namespace(name: &str) -> Namespace {
        serde_json::from_value(json!({"metadata": {"name": name}})).unwrap()
    }

    fn cluster() -> MemoryCluster {
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice@example.com".to_owned(),
                groups: vec!["developers".to_owned()],
            },
            namespaces: vec![namespace("shop")],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    pods: vec![distroless()],
                    ..NamespaceObjects::default()
                },
            )]),
            forbidden_namespaces: [TERMINALS.to_owned()].into(),
            ..State::default()
        })
    }

    /// Zeabur's own service account, which sees the terminal namespace only.
    fn zeabur() -> MemoryCluster {
        MemoryCluster::new(State {
            identity: Identity {
                username: "system:serviceaccount:zeabur:zeabur".to_owned(),
                groups: Vec::new(),
            },
            namespaces: vec![namespace(TERMINALS)],
            objects: BTreeMap::from([(
                TERMINALS.to_owned(),
                NamespaceObjects {
                    namespace: TERMINALS.to_owned(),
                    ..NamespaceObjects::default()
                },
            )]),
            forbidden_namespaces: ["shop".to_owned()].into(),
            ..State::default()
        })
    }

    /// The in-cluster server's view of one person: their token, expiring in ten minutes, handed
    /// to a tools pod.
    struct Signed {
        cluster: Arc<dyn Cluster>,
    }

    impl ClusterAccess for Signed {
        fn for_request(&self, _parts: &Parts) -> Result<Arc<dyn Cluster>, Box<Problem>> {
            Ok(Arc::clone(&self.cluster))
        }

        fn handoff(&self, _parts: &Parts) -> Option<Handoff> {
            Some(Handoff {
                token: zeabur_kube::UserToken::new(SecretString::from(TOKEN)).unwrap(),
                expires_at: Timestamp::now() + SignedDuration::from_mins(10),
            })
        }
    }

    fn settings() -> Settings {
        let mut settings = Settings::default();
        settings.terminal.tools_grace = Duration::ZERO;
        settings
    }

    fn local(cluster: &MemoryCluster) -> Router {
        zeabur_api::router_with_settings(
            Arc::new(SingleCluster(Arc::new(cluster.clone()))),
            settings(),
        )
    }

    fn signed(cluster: &MemoryCluster, zeabur: &MemoryCluster) -> Router {
        zeabur_api::router_with_terminals(
            Arc::new(Signed {
                cluster: Arc::new(cluster.clone()),
            }),
            settings(),
            Arc::new(zeabur.clone()),
        )
    }

    async fn plan(app: &Router, change: Value) -> (StatusCode, Value) {
        send(
            app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({"changes": [change]})),
        )
        .await
    }

    async fn apply(app: &Router, planned: &Value) -> (StatusCode, Value) {
        send(
            app,
            "POST",
            "/api/v1/projects/shop/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await
    }

    fn debug() -> Value {
        json!({"op": "debugContainer", "pod": "api-1"})
    }

    fn stored_pod(cluster: &MemoryCluster, namespace: &str, name: &str) -> Option<Value> {
        cluster
            .inspect(|state| {
                state.objects.get(namespace).and_then(|objects| {
                    objects
                        .pods
                        .iter()
                        .find(|p| p.metadata.name.as_deref() == Some(name))
                        .map(|p| serde_json::to_value(p).unwrap())
                })
            })
            .flatten()
    }

    fn tools_pod(name: &str, person: &str, project: &str, phase: &str) -> Pod {
        let key = zeabur_plan::terminal::person_key(person);
        pod(json!({
            "metadata": {"name": name, "namespace": TERMINALS, "uid": format!("uid-{name}"),
                "resourceVersion": "3", "creationTimestamp": "2026-10-21T00:00:00Z",
                "labels": {"zeabur.com/tools": "true", "zeabur.com/tools-person": key},
                "annotations": {"zeabur.com/tools-project": project}},
            "spec": {"activeDeadlineSeconds": 1_000_000_000,
                     "containers": [{"name": "tools", "image": "k8s"}]},
            "status": {"phase": phase}
        }))
    }

    fn set_pod_status(cluster: &MemoryCluster, namespace: &str, name: &str, status: Value) {
        cluster.update(|state| {
            let objects = state.objects.get_mut(namespace).unwrap();
            let pod = objects
                .pods
                .iter_mut()
                .find(|p| p.metadata.name.as_deref() == Some(name))
                .unwrap();
            pod.status = Some(serde_json::from_value(status).unwrap());
        });
    }

    #[tokio::test]
    async fn a_debug_container_is_planned_applied_and_its_status_read() {
        let cluster = cluster();
        let app = local(&cluster);
        let (status, planned) = plan(&app, debug()).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let write = &planned["objects"][0]["write"];
        assert_eq!(write["type"], "ephemeralContainers");
        assert_eq!(write["operations"][0]["path"], "/metadata/uid");
        let warning = &planned["warnings"][0];
        assert_eq!(warning["code"], "debugContainerStays");
        assert_eq!(warning["target"], "api");
        let name = warning["container"].as_str().unwrap().to_owned();
        assert!(name.starts_with("zeabur-debug-"));

        let (status, applied) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let stored = stored_pod(&cluster, "shop", "api-1").unwrap();
        let added = &stored["spec"]["ephemeralContainers"][0];
        assert_eq!(added["name"], name.as_str());
        assert_eq!(added["targetContainerName"], "api");
        assert_eq!(added["image"], zeabur_api::settings::DEBUG_IMAGE);
        assert_eq!(added["securityContext"]["runAsUser"], 65532);
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        assert!(matches!(
            writes.as_slice(),
            [w] if matches!(w.body, WriteBody::EphemeralContainers(_))
        ));

        let (status, debug) =
            send(&app, "GET", "/api/v1/projects/shop/pods/api-1/debug", None).await;
        assert_eq!(status, StatusCode::OK, "{debug}");
        assert_eq!(debug["target"], "api");
        assert_eq!(debug["container"]["name"], name.as_str());
        assert_eq!(debug["container"]["state"], "unknown");

        set_pod_status(
            &cluster,
            "shop",
            "api-1",
            json!({"phase": "Running", "ephemeralContainerStatuses": [
                {"name": name, "image": "busybox", "imageID": "", "ready": false,
                 "restartCount": 0, "state": {"waiting": {"reason": "ImagePullBackOff",
                 "message": "Back-off pulling image \"busybox\""}}}
            ]}),
        );
        let (_, debug) = send(&app, "GET", "/api/v1/projects/shop/pods/api-1/debug", None).await;
        assert_eq!(debug["container"]["state"], "waiting");
        assert_eq!(
            debug["problem"]["fixKey"]["key"],
            "problem.conflict.fix.terminalImage"
        );
        assert_eq!(
            debug["problem"]["fixKey"]["params"]["setting"],
            "terminal.debugImage"
        );
    }

    #[tokio::test]
    async fn a_running_debug_container_is_used_again_and_a_terminal_opens_in_it() {
        let cluster = cluster();
        cluster.update(|state| {
            let pod = &mut state.objects.get_mut("shop").unwrap().pods[0];
            let mut value = serde_json::to_value(&*pod).unwrap();
            value["spec"]["ephemeralContainers"] = json!([{"name": "zeabur-debug-abcdef",
                "image": "busybox", "targetContainerName": "api", "stdin": true, "tty": true}]);
            value["status"]["ephemeralContainerStatuses"] = json!([{"name": "zeabur-debug-abcdef",
                "image": "busybox", "imageID": "", "ready": false, "restartCount": 0,
                "state": {"running": {}}}]);
            *pod = serde_json::from_value(value).unwrap();
            state.without_shell.insert("shop/api-1".to_owned());
        });
        let app = local(&cluster);
        let (status, planned) = plan(&app, debug()).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert_eq!(planned["objects"], json!([]));
        assert_eq!(planned["warnings"][0]["code"], "debugContainerReused");
        assert_eq!(planned["warnings"][0]["container"], "zeabur-debug-abcdef");

        let address = serve(app).await;
        let mut socket = connect(
            address,
            "/api/v1/projects/shop/pods/api-1/exec?container=zeabur-debug-abcdef",
        )
        .await;
        assert_eq!(
            frame(&mut socket).await,
            json!({"type": "ready", "container": "zeabur-debug-abcdef"})
        );
        socket
            .send(Message::binary(b"exit 3\r".to_vec()))
            .await
            .unwrap();
        assert_eq!(frame(&mut socket).await, json!({"type": "exit", "code": 3}));

        let mut socket = connect(address, "/api/v1/projects/shop/pods/api-1/exec").await;
        assert_eq!(frame(&mut socket).await["type"], "ready");
        assert_eq!(
            frame(&mut socket).await,
            json!({"type": "exit", "reason": "command-not-found"})
        );
    }

    #[tokio::test]
    async fn a_refused_debug_container_names_the_right_it_needs() {
        let cluster = cluster();
        cluster.update(|state| {
            state.forbidden_writes.insert("Pod/api-1".to_owned());
        });
        let app = local(&cluster);
        let (_, planned) = plan(&app, debug()).await;
        let missing = planned["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["code"] == "rightsMissing")
            .unwrap();
        assert_eq!(missing["rights"][0]["verb"], "patch");
        assert_eq!(missing["rights"][0]["resource"], "pods/ephemeralcontainers");
        let (status, refused) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
        assert_eq!(
            refused["denied"],
            json!({"verb": "patch", "group": "", "resource": "pods/ephemeralcontainers",
                   "name": "api-1", "namespace": "shop"})
        );
    }

    #[tokio::test]
    async fn a_cluster_without_ephemeral_containers_says_so() {
        let cluster = cluster();
        cluster.update(|state| state.without_ephemeral_containers = true);
        let app = local(&cluster);
        let (_, planned) = plan(&app, debug()).await;
        let (status, refused) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
        assert_eq!(
            refused["fixKey"]["key"],
            "problem.invalid.fix.ephemeralContainers"
        );
    }

    #[tokio::test]
    async fn a_terminal_change_is_planned_alone() {
        let app = local(&cluster());
        let (status, refused) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({"changes": [debug(), {"op": "clusterTools"}]})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    }

    #[tokio::test]
    async fn on_a_persons_own_computer_there_is_no_tools_pod() {
        let app = local(&cluster());
        let (status, refused) = plan(&app, json!({"op": "clusterTools"})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
        assert_eq!(
            refused["fixKey"]["key"],
            "problem.badRequest.fix.toolsLocally"
        );
        let (_, tools) = send(&app, "GET", "/api/v1/projects/shop/tools", None).await;
        assert_eq!(tools["available"], false);
    }

    #[tokio::test]
    async fn the_token_reaches_the_tools_pod_only_through_standard_input() {
        let cluster = cluster();
        let zeabur = zeabur();
        let app = signed(&cluster, &zeabur);
        let (status, planned) = plan(&app, json!({"op": "clusterTools"})).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert!(!planned.to_string().contains("secret-token"));
        let tools = &planned["warnings"][0];
        assert_eq!(tools["code"], "toolsPod");
        assert_eq!(tools["namespace"], TERMINALS);
        assert_eq!(tools["person"], "alice@example.com");
        assert_eq!(tools["deadlineSeconds"], 1800);
        assert!(tools["credentialExpiresAt"].is_string());
        let name = tools["pod"].as_str().unwrap().to_owned();

        let (status, applied) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let created = stored_pod(&zeabur, TERMINALS, &name).unwrap();
        assert_eq!(
            created["metadata"]["annotations"]["zeabur.com/tools-project"],
            "shop"
        );
        assert_eq!(created["spec"]["automountServiceAccountToken"], false);
        assert!(!created.to_string().contains("secret-token"));

        let (_, again) = plan(&app, json!({"op": "clusterTools"})).await;
        assert_eq!(again["objects"], json!([]));
        assert_eq!(again["warnings"][0]["code"], "toolsPodReused");

        let (_, listed) = send(&app, "GET", "/api/v1/projects/shop/tools", None).await;
        assert_eq!(listed["available"], true);
        assert_eq!(listed["pods"][0]["name"], name.as_str());
        assert_eq!(listed["pods"][0]["ready"], false);

        set_pod_status(
            &zeabur,
            TERMINALS,
            &name,
            json!({"phase": "Running", "startTime": "2026-10-21T00:00:00Z",
                   "containerStatuses": [{"name": "tools", "image": "k8s", "imageID": "",
                   "ready": true, "restartCount": 0, "state": {"running": {}}}]}),
        );
        let address = serve(app.clone()).await;
        let mut socket = connect(
            address,
            &format!("/api/v1/projects/shop/tools/{name}/terminal"),
        )
        .await;
        assert_eq!(
            frame(&mut socket).await,
            json!({"type": "ready", "container": "tools"})
        );
        let runs = zeabur.inspect(|s| s.runs.clone()).unwrap();
        let [setup] = runs.as_slice() else {
            panic!("one setup run: {runs:?}")
        };
        assert_eq!(setup.pod, format!("{TERMINALS}/{name}"));
        assert_eq!(setup.container, "tools");
        assert!(String::from_utf8_lossy(&setup.input).contains(TOKEN));
        assert!(!setup.command.iter().any(|a| a.contains("secret-token")));
        let config: Value = serde_json::from_slice(&setup.input).unwrap();
        assert_eq!(config["contexts"][0]["context"]["namespace"], "shop");
        let writes = zeabur.inspect(|s| format!("{:?}", s.writes)).unwrap();
        assert!(!writes.contains("secret-token"));
        assert_eq!(
            cluster.inspect(|s| s.writes.len()),
            Some(0),
            "nothing as the person"
        );

        socket
            .send(Message::binary(b"exit\r".to_vec()))
            .await
            .unwrap();
        let deleted = async {
            while stored_pod(&zeabur, TERMINALS, &name).is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        tokio::time::timeout(WAIT, deleted)
            .await
            .expect("the pod is deleted after its last terminal");
        let runs = zeabur.inspect(|s| s.runs.clone()).unwrap();
        let forget = runs.last().unwrap();
        assert!(
            forget.command.iter().any(|a| a.contains("rm -f")),
            "{runs:?}"
        );
        assert_eq!(forget.input, b"");
    }

    #[tokio::test]
    async fn without_the_right_to_run_commands_in_the_project_there_is_no_tools_pod() {
        let cluster = cluster();
        cluster.update(|s| {
            s.refused_rights.insert("create pods/exec".to_owned());
        });
        let zeabur = zeabur();
        let app = signed(&cluster, &zeabur);
        let (status, refused) = plan(&app, json!({"op": "clusterTools"})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
        assert_eq!(refused["denied"]["resource"], "pods/exec", "{refused}");
        assert_eq!(refused["denied"]["namespace"], "shop", "{refused}");
        let (status, _) = send(&app, "GET", "/api/v1/projects/shop/tools", None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(zeabur.inspect(|s| s.writes.len()), Some(0));
    }

    #[tokio::test]
    async fn only_the_persons_own_tools_pods_of_the_project_are_listed_deleted_and_swept() {
        let cluster = cluster();
        let zeabur = zeabur();
        zeabur.update(|state| {
            let pods = &mut state.objects.get_mut(TERMINALS).unwrap().pods;
            let alice = "alice@example.com";
            pods.push(tools_pod("zeabur-tools-mine", alice, "shop", "Running"));
            pods.push(tools_pod("zeabur-tools-old", alice, "shop", "Failed"));
            pods.push(tools_pod("zeabur-tools-blog", alice, "blog", "Running"));
            pods.push(tools_pod(
                "zeabur-tools-bobs",
                "bob@example.com",
                "shop",
                "Running",
            ));
            let mut going = tools_pod("zeabur-tools-going", alice, "shop", "Running");
            going.metadata.deletion_timestamp =
                serde_json::from_value(json!("2026-10-21T00:10:00Z")).ok();
            pods.push(going);
            pods.push(tools_pod("zeabur-tools-kept", alice, "shop", "Failed"));
            state
                .forbidden_writes
                .insert("Pod/zeabur-tools-kept".to_owned());
        });
        let app = signed(&cluster, &zeabur);
        let (_, listed) = send(&app, "GET", "/api/v1/projects/shop/tools", None).await;
        assert_eq!(listed["namespace"], TERMINALS);
        assert_eq!(listed["deleted"], json!(["zeabur-tools-old"]));
        let names: Vec<&str> = listed["pods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["zeabur-tools-mine", "zeabur-tools-kept"]);
        assert!(stored_pod(&zeabur, TERMINALS, "zeabur-tools-old").is_none());

        for other in ["zeabur-tools-bobs", "zeabur-tools-blog"] {
            let (status, _) = send(
                &app,
                "DELETE",
                &format!("/api/v1/projects/shop/tools/{other}"),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{other}");
            assert!(stored_pod(&zeabur, TERMINALS, other).is_some());
        }

        set_pod_status(
            &zeabur,
            TERMINALS,
            "zeabur-tools-bobs",
            json!({"phase": "Running", "containerStatuses": [{"name": "tools", "image": "k8s",
                   "imageID": "", "ready": true, "restartCount": 0, "state": {"running": {}}}]}),
        );
        let address = serve(app.clone()).await;
        let mut socket = connect(
            address,
            "/api/v1/projects/shop/tools/zeabur-tools-bobs/terminal",
        )
        .await;
        let refused = frame(&mut socket).await;
        assert_eq!(refused["type"], "error", "{refused}");
        assert_eq!(
            zeabur.inspect(|s| s.runs.len()),
            Some(0),
            "nothing ran in Bob's pod"
        );

        let (status, _) = send(
            &app,
            "DELETE",
            "/api/v1/projects/shop/tools/zeabur-tools-mine",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(stored_pod(&zeabur, TERMINALS, "zeabur-tools-mine").is_none());
    }

    #[tokio::test]
    async fn deleting_the_project_deletes_the_persons_tools_pods_of_it_and_no_others() {
        let cluster = cluster();
        cluster.update(|state| {
            state.namespaces = vec![
                serde_json::from_value(json!({"metadata": {"name": "shop",
                    "labels": {"zeabur.com/project": "true"}}}))
                .unwrap(),
            ];
        });
        let zeabur = zeabur();
        zeabur.update(|state| {
            let pods = &mut state.objects.get_mut(TERMINALS).unwrap().pods;
            let alice = "alice@example.com";
            pods.push(tools_pod("zeabur-tools-mine", alice, "shop", "Running"));
            pods.push(tools_pod("zeabur-tools-blog", alice, "blog", "Running"));
            pods.push(tools_pod(
                "zeabur-tools-bobs",
                "bob@example.com",
                "shop",
                "Running",
            ));
        });
        let app = signed(&cluster, &zeabur);
        let (status, planned) = plan(&app, json!({"op": "deleteProject"})).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let (status, applied) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::OK, "{applied}");

        assert!(stored_pod(&zeabur, TERMINALS, "zeabur-tools-mine").is_none());
        for other in ["zeabur-tools-blog", "zeabur-tools-bobs"] {
            assert!(stored_pod(&zeabur, TERMINALS, other).is_some(), "{other}");
        }
    }

    type Socket = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    async fn serve(router: Router) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        address
    }

    async fn connect(address: SocketAddr, path: &str) -> Socket {
        let mut request = format!("ws://{address}{path}")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("origin", format!("http://{address}").parse().unwrap());
        tokio_tungstenite::connect_async(request).await.unwrap().0
    }

    async fn frame(socket: &mut Socket) -> Value {
        loop {
            let message = tokio::time::timeout(WAIT, socket.next())
                .await
                .expect("no message in time")
                .expect("the socket ended")
                .unwrap();
            if let Message::Text(text) = &message {
                return serde_json::from_str(text).unwrap();
            }
        }
    }
}
