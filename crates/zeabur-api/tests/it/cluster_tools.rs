//! A kubectl terminal for the whole cluster: the plan that starts its pod (and the namespace when
//! missing), reuse, the list, the terminal whose kubeconfig starts in `default`, and the refusals:
//! someone who is not a cluster administrator, a binding that lets others into the pod's
//! namespace, the person's own computer, a plan the cluster moved past. Project and cluster tools
//! pods stay apart.

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::Router;
    use axum::http::StatusCode;
    use axum::http::request::Parts;
    use futures_util::StreamExt as _;
    use secrecy::SecretString;
    use serde_json::{Value, json};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    use zeabur_api::Settings;
    use zeabur_api::access::{Access, ClusterAccess, Elevation, Handoff};
    use zeabur_core::problem::Problem;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Cluster, DynamicObject, Identity, ServedResource};

    use crate::common::{app, send};

    const NAMESPACE: &str = "zeabur-cluster-tools";
    const TOKEN: &str = "eyJhbGciOi.secret-token-of-alice.sig";
    const WAIT: Duration = Duration::from_secs(5);

    fn cluster() -> MemoryCluster {
        let namespace: Namespace =
            serde_json::from_value(json!({"metadata": {"name": "shop"}})).unwrap();
        MemoryCluster::new(State {
            identity: Identity {
                username: "zeabur:alice@example.com".to_owned(),
                groups: vec!["zeabur:admins".to_owned()],
            },
            namespaces: vec![namespace],
            ..State::default()
        })
    }

    /// The in-cluster server's view of one administrator, whose token is handed to tools pods.
    struct Signed(Arc<dyn Cluster>);

    impl ClusterAccess for Signed {
        fn for_request(&self, _parts: &Parts) -> Result<Access, Box<Problem>> {
            Ok(Access {
                cluster: Arc::clone(&self.0),
                elevation: Elevation::Unavailable,
            })
        }

        fn credential_expiry(&self, _parts: &Parts) -> Option<Timestamp> {
            Some(Timestamp::now() + SignedDuration::from_mins(10))
        }

        fn handoff(&self, _parts: &Parts) -> Option<Handoff> {
            Some(Handoff {
                token: Arc::new(SecretString::from(TOKEN.to_owned())),
                expires_at: Timestamp::now() + SignedDuration::from_mins(10),
                admin: true,
            })
        }
    }

    fn signed(cluster: &MemoryCluster) -> Router {
        let mut settings = Settings::default();
        settings.terminal.tools_grace = Duration::ZERO;
        settings.terminal.tools_namespace = Some("zeabur-tools".to_owned());
        zeabur_api::router_with_settings(Arc::new(Signed(Arc::new(cluster.clone()))), settings)
    }

    async fn plan(app: &Router) -> (StatusCode, Value) {
        send(app, "POST", "/api/v1/cluster/tools/plan", None).await
    }

    async fn apply(app: &Router, planned: &Value) -> (StatusCode, Value) {
        send(
            app,
            "POST",
            "/api/v1/cluster/tools/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await
    }

    fn stored(cluster: &MemoryCluster, namespace: &str) -> Vec<Value> {
        cluster
            .inspect(|s| {
                s.objects.get(namespace).map(|o| {
                    o.pods
                        .iter()
                        .map(|p| serde_json::to_value(p).unwrap())
                        .collect()
                })
            })
            .flatten()
            .unwrap_or_default()
    }

    fn namespace_of(cluster: &MemoryCluster, name: &str) -> Option<Value> {
        cluster
            .inspect(|s| {
                s.namespaces
                    .iter()
                    .find(|n| n.metadata.name.as_deref() == Some(name))
                    .map(|n| serde_json::to_value(n).unwrap())
            })
            .flatten()
    }

    #[tokio::test]
    async fn an_administrator_starts_a_cluster_tools_pod_in_its_own_namespace() {
        let cluster = cluster();
        let app = signed(&cluster);
        let (status, planned) = plan(&app).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert!(!planned.to_string().contains("secret-token"));
        assert_eq!(planned["namespace"], NAMESPACE);
        let kinds: Vec<&str> = planned["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["Namespace", "Pod"]);
        assert_eq!(planned["warnings"][0]["code"], "toolsPod");
        assert_eq!(planned["warnings"][0]["namespace"], NAMESPACE);
        assert_eq!(planned["warnings"][1]["code"], "toolsCredentialExposed");
        let name = planned["pod"].as_str().unwrap().to_owned();

        let (status, applied) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(applied, json!({"pod": name, "namespace": NAMESPACE}));
        let created = namespace_of(&cluster, NAMESPACE).unwrap();
        assert_eq!(
            created["metadata"]["labels"]["pod-security.kubernetes.io/enforce"],
            "restricted"
        );
        let [pod] = stored(&cluster, NAMESPACE).try_into().unwrap();
        assert_eq!(pod["metadata"]["name"], name.as_str());
        assert_eq!(pod["metadata"]["labels"]["zeabur.com/tools"], "cluster");
        assert!(pod["metadata"]["annotations"]["zeabur.com/project"].is_null());
        assert!(!pod.to_string().contains("secret-token"));

        let (status, again) = plan(&app).await;
        assert_eq!(status, StatusCode::OK, "{again}");
        assert_eq!(again["writes"], json!([]));
        assert_eq!(again["pod"], name.as_str());
        assert_eq!(again["warnings"][0]["code"], "toolsPodReused");

        let (_, listed) = send(&app, "GET", "/api/v1/cluster/tools", None).await;
        assert_eq!(listed["available"], true);
        assert_eq!(listed["namespace"], NAMESPACE);
        assert_eq!(listed["pods"][0]["name"], name.as_str());

        let (_, project) = send(&app, "GET", "/api/v1/projects/shop/tools", None).await;
        assert_eq!(project["pods"], json!([]));
    }

    #[tokio::test]
    async fn the_terminal_starts_kubectl_in_the_default_namespace() {
        let cluster = cluster();
        let app = signed(&cluster);
        let (_, planned) = plan(&app).await;
        let (status, applied) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let name = applied["pod"].as_str().unwrap().to_owned();
        cluster.update(|state| {
            let pod = &mut state.objects.get_mut(NAMESPACE).unwrap().pods[0];
            pod.status = Some(
                serde_json::from_value(json!({"phase": "Running",
                    "containerStatuses": [{"name": "tools", "image": "k8s", "imageID": "",
                    "ready": true, "restartCount": 0, "state": {"running": {}}}]}))
                .unwrap(),
            );
        });

        let address = serve(app).await;
        let mut socket = connect(address, &format!("/api/v1/cluster/tools/{name}/terminal")).await;
        assert_eq!(
            frame(&mut socket).await,
            json!({"type": "ready", "container": "tools"})
        );
        let runs = cluster.inspect(|s| s.runs.clone()).unwrap();
        let [setup] = runs.as_slice() else {
            panic!("one setup run: {runs:?}")
        };
        assert_eq!(setup.pod, format!("{NAMESPACE}/{name}"));
        assert!(!setup.command.iter().any(|a| a.contains("secret-token")));
        let config: Value = serde_json::from_slice(&setup.input).unwrap();
        assert_eq!(config["contexts"][0]["context"]["namespace"], "default");
    }

    #[tokio::test]
    async fn someone_who_is_not_a_cluster_administrator_is_refused() {
        let cluster = cluster();
        cluster.update(|s| s.cluster_wide_denied = true);
        let app = signed(&cluster);
        let (status, refused) = plan(&app).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
        assert!(
            refused["detail"]
                .as_str()
                .unwrap()
                .starts_with("Only cluster administrators can use"),
            "{refused}"
        );
        assert_eq!(
            refused["denied"],
            json!({"verb": "*", "group": "*", "resource": "*"})
        );
        let (status, _) = send(&app, "GET", "/api/v1/cluster/tools", None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(namespace_of(&cluster, NAMESPACE).is_none());
    }

    #[tokio::test]
    async fn on_a_persons_own_computer_there_is_no_cluster_tools_pod() {
        let app = app(&cluster());
        let (status, refused) = plan(&app).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
        assert_eq!(
            refused["fixKey"]["key"],
            "problem.badRequest.fix.toolsLocally"
        );
        let (_, tools) = send(&app, "GET", "/api/v1/cluster/tools", None).await;
        assert_eq!(tools["available"], false);
    }

    fn rbac(kind: &str, namespace: Option<&str>, body: &Value) -> DynamicObject {
        let mut object = body.clone();
        object["apiVersion"] = json!("rbac.authorization.k8s.io/v1");
        object["kind"] = json!(kind);
        if let Some(namespace) = namespace {
            object["metadata"]["namespace"] = json!(namespace);
        }
        serde_json::from_value(object).unwrap()
    }

    /// RBAC served, the namespace there, and a Role in it and in `shop` that lets in `pods/exec`.
    fn with_exec_roles(cluster: &MemoryCluster) {
        cluster.update(|s| {
            for (plural, kind, namespaced) in [
                ("clusterroles", "ClusterRole", false),
                ("clusterrolebindings", "ClusterRoleBinding", false),
                ("roles", "Role", true),
                ("rolebindings", "RoleBinding", true),
            ] {
                s.served.push(ServedResource::readable(
                    "rbac.authorization.k8s.io",
                    "v1",
                    plural,
                    kind,
                    namespaced,
                ));
            }
            s.namespaces.push(
                serde_json::from_value(json!({"metadata": {"name": NAMESPACE,
                    "resourceVersion": "1"}}))
                .unwrap(),
            );
            for namespace in [NAMESPACE, "shop"] {
                s.dynamic.push(rbac(
                    "Role",
                    Some(namespace),
                    &json!({"metadata": {"name": "exec"}, "rules": [
                        {"apiGroups": [""], "resources": ["pods/exec"], "verbs": ["create"]}]}),
                ));
                s.dynamic.push(rbac(
                    "RoleBinding",
                    Some(namespace),
                    &json!({"metadata": {"name": "ops-exec"},
                        "roleRef": {"kind": "Role", "name": "exec"},
                        "subjects": [{"kind": "Group", "name": "ops"}]}),
                ));
            }
        });
    }

    #[tokio::test]
    async fn a_binding_that_lets_others_into_its_namespace_blocks_the_plan() {
        let cluster = cluster();
        with_exec_roles(&cluster);
        let app = signed(&cluster);
        let (status, planned) = plan(&app).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert_eq!(planned.get("token"), None);
        assert_eq!(
            planned["exposed"],
            json!([{"kind": "RoleBinding", "name": "ops-exec", "role": "Role/exec",
                    "rights": ["create pods/exec"],
                    "subjects": [{"kind": "Group", "name": "ops"}]}])
        );

        cluster.update(|s| {
            s.dynamic.retain(|o| {
                !(o.metadata.name.as_deref() == Some("ops-exec")
                    && o.metadata.namespace.as_deref() == Some(NAMESPACE))
            });
        });
        let (_, planned) = plan(&app).await;
        assert_eq!(
            planned["exposed"],
            json!([]),
            "a binding in shop does not count"
        );
        assert!(planned["token"].is_string(), "{planned}");

        cluster.update(|s| {
            s.dynamic.push(rbac(
                "RoleBinding",
                Some(NAMESPACE),
                &json!({"metadata": {"name": "late"},
                    "roleRef": {"kind": "Role", "name": "exec"},
                    "subjects": [{"kind": "User", "name": "zeabur:bob"}]}),
            ));
        });
        let (status, refused) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert!(
            refused["detail"]
                .as_str()
                .unwrap()
                .contains("RoleBinding late let others in"),
            "{refused}"
        );
        assert_eq!(stored(&cluster, NAMESPACE), Vec::<Value>::new());
    }

    #[tokio::test]
    async fn a_plan_the_cluster_moved_past_is_refused() {
        let cluster = cluster();
        let app = signed(&cluster);
        let (_, stale) = plan(&app).await;
        let (_, planned) = plan(&app).await;
        let (status, _) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::OK);
        let (status, refused) = apply(&app, &stale).await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert_eq!(stored(&cluster, NAMESPACE).len(), 1);
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
