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
    use axum::http::request::Parts;
    use futures_util::{SinkExt as _, StreamExt as _};
    use serde_json::{Value, json};
    use tokio::net::TcpStream;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::{Error as WsError, Message};
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
    use zeabur_api::access::{Access, ClusterAccess, Elevation, SingleCluster};
    use zeabur_core::problem::Problem;
    use zeabur_kube::k8s_openapi::api::core::v1::Pod;
    use zeabur_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Cluster, NamespaceObjects, TerminalSize};

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

    /// One person whose credential expires a second after [`now`].
    struct Expiring(Arc<dyn Cluster>);

    impl ClusterAccess for Expiring {
        fn for_request(&self, _parts: &Parts) -> Result<Access, Box<Problem>> {
            Ok(Access {
                cluster: Arc::clone(&self.0),
                elevation: Elevation::Unavailable,
            })
        }

        fn credential_expiry(&self, _parts: &Parts) -> Option<Timestamp> {
            Some(now() + SignedDuration::from_secs(1))
        }
    }

    /// One person whose admin rights are offered and not in use (`admin` is `None`), or held
    /// until a second after [`now`].
    struct Elevating {
        everyday: Arc<dyn Cluster>,
        admin: Option<Arc<dyn Cluster>>,
    }

    impl ClusterAccess for Elevating {
        fn for_request(&self, _parts: &Parts) -> Result<Access, Box<Problem>> {
            let everyday = || Access {
                cluster: Arc::clone(&self.everyday),
                elevation: Elevation::Available,
            };
            Ok(self.admin.as_ref().map_or_else(everyday, |admin| Access {
                cluster: Arc::clone(admin),
                elevation: Elevation::Active {
                    until: now() + SignedDuration::from_secs(1),
                },
            }))
        }

        fn credential_expiry(&self, _parts: &Parts) -> Option<Timestamp> {
            self.admin
                .as_ref()
                .map(|_| now() + SignedDuration::from_secs(1))
        }
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
    async fn the_terminal_closes_when_the_credential_expires() {
        let cluster: Arc<dyn Cluster> = Arc::new(cluster());
        let router = zeabur_api::router_at(Arc::new(Expiring(cluster)), now);
        let mut socket = connect(router, "").await;
        assert_eq!(frame(&mut socket).await["type"], "ready");
        let (code, reason) = close_code(&mut socket).await;
        assert_eq!(code, CloseCode::from(4000));
        assert_eq!(reason, "session expired, reconnect");
    }

    /// While the person is elevated the terminal runs with the admin credential (the everyday
    /// one is refused here) and ends when that one expires.
    #[tokio::test]
    async fn an_elevated_terminal_uses_the_admin_credential_until_it_expires() {
        let everyday = cluster();
        everyday.update(|s| {
            s.exec_refused.insert("shop/web-1".to_owned());
        });
        let access = Elevating {
            everyday: Arc::new(everyday),
            admin: Some(Arc::new(cluster())),
        };
        let mut socket = connect(zeabur_api::router_at(Arc::new(access), now), "").await;
        assert_eq!(frame(&mut socket).await["type"], "ready");
        let (code, _) = close_code(&mut socket).await;
        assert_eq!(code, CloseCode::from(4000));
    }

    #[tokio::test]
    async fn an_idle_terminal_is_closed() {
        let access = Arc::new(SingleCluster(Arc::new(cluster())));
        let router = zeabur_api::router_with_terminal_idle(access, now, Duration::from_millis(300));
        let mut socket = connect(router, "").await;
        assert_eq!(frame(&mut socket).await["type"], "ready");
        assert_eq!(close_code(&mut socket).await.0, CloseCode::from(4001));
    }

    /// A refusal names the permission; it offers admin rights only where they are offered and
    /// not in use (with them in use, retrying as admin would not help).
    #[tokio::test]
    async fn a_refused_exec_names_the_permission_it_needs() {
        let cluster = cluster();
        cluster.update(|s| {
            s.exec_refused.insert("shop/web-1".to_owned());
        });
        let refused: Arc<dyn Cluster> = Arc::new(cluster.clone());
        for (router, kind) in [
            (app(&cluster), "forbidden"),
            (
                zeabur_api::router_at(
                    Arc::new(Elevating {
                        everyday: Arc::clone(&refused),
                        admin: None,
                    }),
                    now,
                ),
                "elevation-required",
            ),
            (
                zeabur_api::router_at(
                    Arc::new(Elevating {
                        everyday: Arc::clone(&refused),
                        admin: Some(Arc::clone(&refused)),
                    }),
                    now,
                ),
                "forbidden",
            ),
        ] {
            let mut socket = connect(router, "").await;
            let error = frame(&mut socket).await;
            assert_eq!(error["type"], "error");
            assert_eq!(error["problem"]["status"], 403);
            assert_eq!(error["problem"]["kind"], kind);
            assert_eq!(error["problem"]["kubernetes"]["code"], 403);
            assert_eq!(
                error["problem"]["denied"],
                json!({"verb": "get", "group": "", "resource": "pods/exec",
                       "name": "web-1", "namespace": "shop"})
            );
            assert_eq!(close_code(&mut socket).await.0, CloseCode::Normal);
        }
    }

    #[tokio::test]
    async fn a_missing_pod_or_container_or_one_that_is_not_running_is_a_problem() {
        for (query, status, detail) in [
            ("?container=db", 404, "Pod web-1 has no container db."),
            (
                "?container=worker",
                409,
                "Container worker in pod web-1 is not running (CrashLoopBackOff).",
            ),
        ] {
            let mut socket = connect(app(&cluster()), query).await;
            let error = frame(&mut socket).await;
            assert_eq!(error["problem"]["status"], status, "{query}");
            assert_eq!(error["problem"]["detail"], detail, "{query}");
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
