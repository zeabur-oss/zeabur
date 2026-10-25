//! The terminal against an in-memory cluster's echo shell, over a real WebSocket: bytes both
//! ways, resizes, the exit code, the server closing at credential expiry and after idling, the
//! problems a refusal or a missing pod or container become, and refusing other origins.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::Router;
    use futures_util::{SinkExt as _, StreamExt as _};
    use serde_json::{Value, json};
    use tokio::net::TcpStream;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::{Error as WsError, Message};
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
    use zeabur_api::access::SingleCluster;
    use zeabur_kube::k8s_openapi::api::core::v1::Pod;
    use zeabur_kube::k8s_openapi::jiff::Timestamp;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{NamespaceObjects, TerminalSize};

    type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

    /// How long any one expected message may take.
    const WAIT: Duration = Duration::from_secs(5);

    fn now() -> Timestamp {
        "2026-10-16T09:00:00Z".parse().unwrap()
    }

    fn cluster() -> MemoryCluster {
        let pod: Pod = serde_json::from_value(json!({
            "metadata": {"name": "web-1", "namespace": "shop"},
            "spec": {"containers": [
                {"name": "web", "image": "nginx:1.29"},
                {"name": "worker", "image": "worker:1"}
            ]},
            "status": {"phase": "Running", "containerStatuses": [
                {"name": "web", "image": "nginx:1.29", "imageID": "", "ready": true,
                 "restartCount": 0, "state": {"running": {}}},
                {"name": "worker", "image": "worker:1", "imageID": "", "ready": false,
                 "restartCount": 4, "state": {"waiting": {"reason": "CrashLoopBackOff"}}}
            ]}
        }))
        .unwrap();
        MemoryCluster::new(State {
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    pods: vec![pod],
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        })
    }

    async fn serve(router: Router) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        address
    }

    fn app(cluster: &MemoryCluster) -> Router {
        zeabur_api::router_at(Arc::new(SingleCluster(Arc::new(cluster.clone()))), now)
    }

    async fn connect_from(
        router: Router,
        query: &str,
        origin: Option<&str>,
    ) -> Result<Socket, WsError> {
        let address = serve(router).await;
        let mut request = format!("ws://{address}/api/v1/projects/shop/pods/web-1/exec{query}")
            .into_client_request()
            .unwrap();
        let origin = origin.map_or_else(|| format!("http://{address}"), str::to_owned);
        request
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
        tokio_tungstenite::connect_async(request)
            .await
            .map(|(socket, _)| socket)
    }

    async fn connect(router: Router, query: &str) -> Socket {
        connect_from(router, query, None).await.unwrap()
    }

    async fn next(socket: &mut Socket) -> Message {
        tokio::time::timeout(WAIT, socket.next())
            .await
            .expect("no message in time")
            .expect("the socket ended")
            .unwrap()
    }

    async fn frame(socket: &mut Socket) -> Value {
        loop {
            let message = next(socket).await;
            if let Message::Text(text) = &message {
                return serde_json::from_str(text).unwrap();
            }
            assert!(
                message.is_binary(),
                "expected a control frame, got {message:?}"
            );
        }
    }

    /// Reads terminal bytes until they contain `expected`; fails with what was seen.
    async fn output_until(socket: &mut Socket, expected: &str) {
        let mut seen = String::new();
        while !seen.contains(expected) {
            let message = next(socket).await;
            let Message::Binary(bytes) = &message else {
                panic!("expected {expected:?} after {seen:?}, got {message:?}");
            };
            seen.push_str(&String::from_utf8_lossy(bytes));
        }
    }

    async fn close_code(socket: &mut Socket) -> (CloseCode, String) {
        loop {
            if let Message::Close(Some(close)) = next(socket).await {
                return (close.code, close.reason.to_string());
            }
        }
    }

    #[tokio::test]
    async fn bytes_flow_both_ways_and_the_exit_code_comes_back() {
        let cluster = cluster();
        let mut socket = connect(app(&cluster), "?container=web").await;
        assert_eq!(
            frame(&mut socket).await,
            json!({"type": "ready", "container": "web"})
        );
        output_until(&mut socket, "$ ").await;

        socket
            .send(Message::binary(b"echo hello\r".to_vec()))
            .await
            .unwrap();
        output_until(&mut socket, "echo hello\r\n$ ").await;

        socket
            .send(Message::text(r#"{"type":"resize","cols":120,"rows":40}"#))
            .await
            .unwrap();
        let resized = TerminalSize {
            cols: 120,
            rows: 40,
        };
        let deadline = tokio::time::Instant::now() + WAIT;
        while cluster.inspect(|s| s.terminal_sizes.clone()) != Some(vec![resized]) {
            assert!(tokio::time::Instant::now() < deadline, "no resize arrived");
            tokio::task::yield_now().await;
        }

        socket
            .send(Message::binary(b"exit 3\r".to_vec()))
            .await
            .unwrap();
        assert_eq!(frame(&mut socket).await, json!({"type": "exit", "code": 3}));
        assert_eq!(close_code(&mut socket).await.0, CloseCode::Normal);
    }

    #[tokio::test]
    async fn the_default_container_runs_a_shell_by_default() {
        let mut socket = connect(app(&cluster()), "").await;
        assert_eq!(frame(&mut socket).await["container"], "web");
        output_until(&mut socket, "$ ").await;
    }

    #[tokio::test]
    async fn an_image_without_a_shell_says_so() {
        let cluster = cluster();
        cluster.update(|s| {
            s.without_shell.insert("shop/web-1".to_owned());
        });
        let mut socket = connect(app(&cluster), "").await;
        assert_eq!(frame(&mut socket).await["type"], "ready");
        assert_eq!(
            frame(&mut socket).await,
            json!({"type": "exit", "reason": "command-not-found"})
        );
    }

    #[tokio::test]
    async fn an_idle_terminal_is_closed() {
        let access = Arc::new(SingleCluster(Arc::new(cluster())));
        let router = zeabur_api::router_with_terminal_idle(access, now, Duration::from_millis(300));
        let mut socket = connect(router, "").await;
        assert_eq!(frame(&mut socket).await["type"], "ready");
        assert_eq!(close_code(&mut socket).await.0, CloseCode::from(4001));
    }

    /// A refusal names the permission.
    #[tokio::test]
    async fn a_refused_exec_names_the_permission_it_needs() {
        let cluster = cluster();
        cluster.update(|s| {
            s.exec_refused.insert("shop/web-1".to_owned());
        });
        let mut socket = connect(app(&cluster), "").await;
        let error = frame(&mut socket).await;
        assert_eq!(error["type"], "error");
        assert_eq!(error["problem"]["status"], 403);
        assert_eq!(error["problem"]["kind"], "forbidden");
        assert_eq!(error["problem"]["kubernetes"]["code"], 403);
        assert_eq!(
            error["problem"]["denied"],
            json!({"verb": "create", "group": "", "resource": "pods/exec",
                   "name": "web-1", "namespace": "shop"})
        );
        assert_eq!(close_code(&mut socket).await.0, CloseCode::Normal);
    }

    /// `get` on `pods/exec` is enough for kube-apiserver before 1.31 to open a WebSocket exec;
    /// Zeabur asks for `create` first, so the terminal never opens on `get` alone.
    #[tokio::test]
    async fn a_terminal_takes_create_on_pods_exec_not_just_get() {
        let cluster = cluster();
        cluster.update(|s| {
            s.refused_rights.insert("create pods/exec".to_owned());
        });
        let mut socket = connect(app(&cluster), "").await;
        let error = frame(&mut socket).await;
        assert_eq!(error["type"], "error");
        assert_eq!(error["problem"]["status"], 403);
        assert_eq!(
            error["problem"]["denied"],
            json!({"verb": "create", "group": "", "resource": "pods/exec",
                   "name": "web-1", "namespace": "shop"})
        );
        assert_eq!(close_code(&mut socket).await.0, CloseCode::Normal);
    }

    #[tokio::test]
    async fn a_missing_pod_or_container_or_one_that_is_not_running_is_a_problem() {
        for (query, status, fix) in [
            ("?container=db", 404, Value::Null),
            (
                "?container=worker",
                409,
                json!("problem.conflict.fix.waitForContainer"),
            ),
        ] {
            let mut socket = connect(app(&cluster()), query).await;
            let error = frame(&mut socket).await;
            assert_eq!(error["problem"]["status"], status, "{query}");
            assert_eq!(error["problem"]["fixKey"]["key"], fix, "{query}");
        }
        let empty = MemoryCluster::new(State::default());
        let mut socket = connect(app(&empty), "").await;
        assert_eq!(frame(&mut socket).await["problem"]["status"], 404);
    }

    #[tokio::test]
    async fn another_origin_cannot_open_a_terminal() {
        for origin in ["https://attacker.example", "null"] {
            let refused = connect_from(app(&cluster()), "", Some(origin)).await;
            let Err(WsError::Http(response)) = refused else {
                panic!("{origin}: the handshake was not refused");
            };
            assert_eq!(response.status(), 403, "{origin}");
        }
    }
}
