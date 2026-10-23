//! Node shells: the plan that opens one (namespace, pod, Event), the list, the terminal that only
//! its opener enters and that deletes the pod when it closes, the plan that removes a leftover,
//! and the refusals: someone who is not a cluster administrator, a binding that lets others in,
//! a plan the cluster moved past.

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::time::Duration;

    use axum::Router;
    use axum::http::StatusCode;
    use futures_util::{SinkExt as _, StreamExt as _};
    use serde_json::{Value, json};
    use tokio::net::TcpStream;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, Identity, ServedResource, WriteBody};

    use crate::common::{app, send};

    type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

    const NAMESPACE: &str = "zeabur-node-shell";
    const PERSON: &str = "zeabur:alice@example.com";

    fn cluster() -> MemoryCluster {
        MemoryCluster::new(State {
            identity: Identity {
                username: PERSON.to_owned(),
                groups: vec!["zeabur:admins".to_owned()],
            },
            nodes: vec![
                serde_json::from_value(json!({"metadata": {"name": "vps-1", "uid": "node-1"}}))
                    .unwrap(),
            ],
            ..State::default()
        })
    }

    async fn plan(app: &Router, request: &Value) -> (StatusCode, Value) {
        send(
            app,
            "POST",
            "/api/v1/cluster/node-shells/plan",
            Some(request),
        )
        .await
    }

    async fn apply(app: &Router, planned: &Value) -> (StatusCode, Value) {
        send(
            app,
            "POST",
            "/api/v1/cluster/node-shells/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await
    }

    async fn opened(app: &Router) -> (Value, Value) {
        let (status, planned) = plan(app, &json!({"op": "open", "node": "vps-1"})).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let (status, applied) = apply(app, &planned).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        (planned, applied)
    }

    fn pod_of(cluster: &MemoryCluster, name: &str) -> Option<Value> {
        cluster
            .inspect(|s| {
                s.objects.get(NAMESPACE).and_then(|o| {
                    o.pods
                        .iter()
                        .find(|p| p.metadata.name.as_deref() == Some(name))
                        .map(|p| serde_json::to_value(p).unwrap())
                })
            })
            .flatten()
    }

    fn start(cluster: &MemoryCluster, name: &str) {
        cluster.update(|s| {
            let pods = &mut s.objects.get_mut(NAMESPACE).unwrap().pods;
            let pod = pods
                .iter_mut()
                .find(|p| p.metadata.name.as_deref() == Some(name))
                .unwrap();
            pod.status = Some(
                serde_json::from_value(json!({"phase": "Running",
                    "startTime": "2026-10-23T05:00:03Z",
                    "containerStatuses": [{"name": "shell", "image": "busybox", "imageID": "",
                        "ready": true, "restartCount": 0, "state": {"running": {}}}]}))
                .unwrap(),
            );
        });
    }

    #[tokio::test]
    async fn opening_a_shell_creates_the_namespace_the_pod_and_an_event_about_the_node() {
        let cluster = cluster();
        let app = app(&cluster);
        let (planned, applied) = opened(&app).await;
        let kinds: Vec<(&str, &str)> = planned["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| (w["action"].as_str().unwrap(), w["kind"].as_str().unwrap()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("create", "Namespace"),
                ("create", "Pod"),
                ("create", "Event")
            ]
        );
        assert_eq!(planned["deadlineSeconds"], 1800);
        assert_eq!(planned["firstSessionSeconds"], 120);
        assert_eq!(planned["person"], PERSON);
        assert_eq!(planned["exposed"], json!([]));
        assert!(
            planned["kubectl"]
                .as_str()
                .unwrap()
                .starts_with("kubectl debug node/vps-1 ")
        );
        let pod = applied["pod"].as_str().unwrap();
        assert_eq!(pod, planned["pod"]);
        assert!(pod.starts_with("zeabur-node-shell-vps-1-"));

        let namespace = cluster
            .inspect(|s| {
                s.namespaces
                    .iter()
                    .find(|n| n.metadata.name.as_deref() == Some(NAMESPACE))
                    .cloned()
            })
            .flatten()
            .unwrap();
        assert_eq!(
            namespace.metadata.labels.unwrap()["pod-security.kubernetes.io/enforce"],
            "privileged"
        );
        let body = pod_of(&cluster, pod).unwrap();
        assert_eq!(body["spec"]["nodeName"], "vps-1");
        assert_eq!(body["spec"]["hostPID"], true);
        let event = cluster
            .inspect(|s| {
                s.dynamic
                    .iter()
                    .find(|o| o.metadata.name.as_deref() == Some(pod))
                    .map(|o| serde_json::to_value(o).unwrap())
            })
            .flatten()
            .unwrap();
        assert_eq!(event["regarding"]["name"], "vps-1");
        assert_eq!(event["regarding"]["uid"], "node-1");
        assert_eq!(event["reason"], "NodeShellOpened");

        let (status, listed) = send(&app, "GET", "/api/v1/cluster/node-shells", None).await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        assert_eq!(listed["shells"][0]["name"], pod);
        assert_eq!(listed["shells"][0]["phase"], "starting");
        assert_eq!(listed["shells"][0]["mine"], true);

        let (_, again) = plan(&app, &json!({"op": "open", "node": "vps-1"})).await;
        let kinds: Vec<&str> = again["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["Pod", "Event"]);
    }

    #[tokio::test]
    async fn someone_who_is_not_a_cluster_administrator_is_refused_plainly() {
        let cluster = cluster();
        cluster.update(|s| s.cluster_wide_denied = true);
        let app = app(&cluster);
        let (status, refused) = plan(&app, &json!({"op": "open", "node": "vps-1"})).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
        assert_eq!(
            refused["denied"],
            json!({"verb": "*", "group": "*", "resource": "*"})
        );
        assert!(
            refused["detail"]
                .as_str()
                .unwrap()
                .starts_with("Only cluster administrators can use a shell on a node")
        );
        let (status, _) = send(&app, "GET", "/api/v1/cluster/node-shells", None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(cluster.inspect(|s| s.writes.is_empty()).unwrap());
    }

    #[tokio::test]
    async fn a_missing_node_is_not_found() {
        let app = app(&cluster());
        let (status, _) = plan(&app, &json!({"op": "open", "node": "vps-9"})).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
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

    #[tokio::test]
    async fn a_binding_that_lets_others_create_or_enter_pods_there_blocks_the_plan() {
        let cluster = cluster();
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
            s.dynamic = vec![
                rbac(
                    "ClusterRole",
                    None,
                    &json!({"metadata": {"name": "edit"}, "rules": [
                    {"apiGroups": [""], "resources": ["pods", "pods/exec"],
                     "verbs": ["create"]}]}),
                ),
                rbac(
                    "ClusterRoleBinding",
                    None,
                    &json!({"metadata": {"name": "ops-edit"},
                    "roleRef": {"kind": "ClusterRole", "name": "edit"},
                    "subjects": [{"kind": "Group", "name": "ops"}]}),
                ),
            ];
        });
        let app = app(&cluster);
        let (status, planned) = plan(&app, &json!({"op": "open", "node": "vps-1"})).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert_eq!(planned.get("token"), None);
        assert_eq!(
            planned["exposed"],
            json!([{"kind": "ClusterRoleBinding", "name": "ops-edit",
                    "role": "ClusterRole/edit",
                    "rights": ["create pods", "create pods/exec"],
                    "subjects": [{"kind": "Group", "name": "ops"}]}])
        );

        cluster.update(|s| {
            s.dynamic
                .retain(|o| o.metadata.name.as_deref() != Some("ops-edit"));
        });
        let (_, planned) = plan(&app, &json!({"op": "open", "node": "vps-1"})).await;
        assert!(planned["token"].is_string(), "{planned}");
        cluster.update(|s| {
            s.dynamic.push(rbac(
                "ClusterRoleBinding",
                None,
                &json!({"metadata": {"name": "late"},
                "roleRef": {"kind": "ClusterRole", "name": "edit"},
                "subjects": [{"kind": "User", "name": "zeabur:bob"}]}),
            ));
        });
        let (status, refused) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert!(cluster.inspect(|s| s.writes.is_empty()).unwrap());
    }

    #[tokio::test]
    async fn a_plan_the_cluster_moved_past_is_refused() {
        let cluster = cluster();
        let app = app(&cluster);
        let (_, planned) = plan(&app, &json!({"op": "open", "node": "vps-1"})).await;
        cluster.update(|s| {
            s.namespaces.push(
                serde_json::from_value(json!({"metadata": {"name": NAMESPACE,
                    "resourceVersion": "1"}}))
                .unwrap(),
            );
        });
        let (status, _) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_leftover_shell_is_removed_by_a_plan_of_its_own() {
        let cluster = cluster();
        let app = app(&cluster);
        let (_, applied) = opened(&app).await;
        let pod = applied["pod"].as_str().unwrap();
        let (status, planned) = plan(&app, &json!({"op": "remove", "pod": pod})).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert_eq!(planned["writes"][0]["action"], "delete");
        assert_eq!(planned["node"], "vps-1");
        let (status, _) = apply(&app, &planned).await;
        assert_eq!(status, StatusCode::OK);
        assert!(pod_of(&cluster, pod).is_none());
        let (_, listed) = send(&app, "GET", "/api/v1/cluster/node-shells", None).await;
        assert_eq!(listed["shells"], json!([]));

        let (status, _) = plan(&app, &json!({"op": "remove", "pod": "web-1"})).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

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

    async fn next(socket: &mut Socket) -> Message {
        tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("no message in time")
            .expect("the socket ended")
            .unwrap()
    }

    async fn frame(socket: &mut Socket) -> Value {
        loop {
            if let Message::Text(text) = next(socket).await {
                return serde_json::from_str(&text).unwrap();
            }
        }
    }

    async fn output_until(socket: &mut Socket, expected: &str) {
        let mut seen = String::new();
        while !seen.contains(expected) {
            if let Message::Binary(bytes) = next(socket).await {
                seen.push_str(&String::from_utf8_lossy(&bytes));
            }
        }
    }

    #[tokio::test]
    async fn the_shell_opens_for_its_opener_and_its_pod_is_deleted_when_it_closes() {
        let cluster = cluster();
        let router = app(&cluster);
        let (_, applied) = opened(&router).await;
        let pod = applied["pod"].as_str().unwrap().to_owned();
        start(&cluster, &pod);
        let address = serve(router).await;
        let path = format!("/api/v1/cluster/node-shells/{pod}/terminal");

        let mut socket = connect(address, &path).await;
        assert_eq!(
            frame(&mut socket).await,
            json!({"type": "ready", "container": "shell"})
        );
        output_until(&mut socket, "root@vps-1:~# ").await;
        socket
            .send(Message::binary(b"exit\r".to_vec()))
            .await
            .unwrap();
        assert_eq!(frame(&mut socket).await, json!({"type": "exit", "code": 0}));
        let mut deleted = None;
        for _ in 0..50 {
            deleted = cluster
                .inspect(|s| {
                    s.writes
                        .iter()
                        .find(|w| matches!(w.body, WriteBody::Delete(_)))
                        .cloned()
                })
                .flatten();
            if deleted.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let deleted = deleted.expect("the pod was not deleted");
        assert_eq!(deleted.target.name, pod);
        let WriteBody::Delete(preconditions) = deleted.body else {
            unreachable!()
        };
        assert_eq!(preconditions.grace_period_seconds, Some(0));
        assert!(pod_of(&cluster, &pod).is_none());
    }

    #[tokio::test]
    async fn only_whoever_opened_the_shell_may_enter_it_and_only_as_an_administrator() {
        let cluster = cluster();
        let router = app(&cluster);
        let (_, applied) = opened(&router).await;
        let pod = applied["pod"].as_str().unwrap().to_owned();
        start(&cluster, &pod);
        let address = serve(router).await;
        let path = format!("/api/v1/cluster/node-shells/{pod}/terminal");

        cluster.update(|s| s.identity.username = "zeabur:bob@example.com".to_owned());
        let mut socket = connect(address, &path).await;
        let error = frame(&mut socket).await;
        assert_eq!(error["type"], "error");
        assert_eq!(error["problem"]["status"], 404);

        cluster.update(|s| {
            s.identity.username = PERSON.to_owned();
            s.refused_rights.insert("create pods/exec".to_owned());
        });
        let mut socket = connect(address, &path).await;
        let error = frame(&mut socket).await;
        assert_eq!(error["problem"]["status"], 403);
        assert_eq!(error["problem"]["denied"]["verb"], "create");
        assert_eq!(error["problem"]["denied"]["resource"], "pods/exec");

        cluster.update(|s| {
            s.refused_rights.clear();
            s.cluster_wide_denied = true;
        });
        let mut socket = connect(address, &path).await;
        let error = frame(&mut socket).await;
        assert_eq!(error["problem"]["status"], 403);
        assert_eq!(error["problem"]["denied"]["resource"], "*");
        assert!(pod_of(&cluster, &pod).is_some());

        cluster.update(|s| s.cluster_wide_denied = false);
        let mut socket = connect(
            address,
            &format!("/api/v1/projects/{NAMESPACE}/pods/{pod}/exec?container=shell"),
        )
        .await;
        let error = frame(&mut socket).await;
        assert_eq!(error["problem"]["status"], 403);
        assert_eq!(error["problem"].get("denied"), None);
    }
}
