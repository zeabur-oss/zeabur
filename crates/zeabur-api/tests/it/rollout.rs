//! A rollout's progress against an in-memory cluster: a new pod in a crash loop makes it stuck,
//! with the plan that rolls back.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, from_value, json};
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects};

    use crate::common::{app, send};

    fn rs(n: u32, ready: i32) -> Value {
        json!({
            "metadata": {"name": format!("web-{n}abcd"), "namespace": "shop",
                "creationTimestamp": format!("2026-10-0{n}T08:00:00Z"),
                "ownerReferences": [{"apiVersion": "apps/v1", "kind": "Deployment", "name": "web",
                    "uid": "u", "controller": true}],
                "annotations": {"deployment.kubernetes.io/revision": n.to_string()}},
            "spec": {"selector": {"matchLabels": {"app": "web"}},
                "template": {"metadata": {"labels": {"app": "web"}},
                    "spec": {"containers": [{"name": "app", "image": format!("example/web:{n}")}]}}},
            "status": {"replicas": 1, "readyReplicas": ready}
        })
    }

    fn pod(release: &str, name: &str, waiting: Option<&str>) -> Value {
        let state = waiting.map_or_else(
            || json!({"running": {"startedAt": "2026-10-01T08:00:00Z"}}),
            |reason| json!({"waiting": {"reason": reason, "message": "back-off"}}),
        );
        json!({
            "apiVersion": "v1", "kind": "Pod",
            "metadata": {"name": name, "namespace": "shop", "labels": {"app": "web"},
                "ownerReferences": [{"apiVersion": "apps/v1", "kind": "ReplicaSet",
                    "name": release, "uid": "r", "controller": true}]},
            "spec": {"containers": [{"name": "app", "image": "example/web"}]},
            "status": {"phase": "Running", "containerStatuses": [{"name": "app",
                "ready": waiting.is_none(), "restartCount": 5, "image": "example/web",
                "imageID": "", "state": state,
                "lastState": {"terminated": {"exitCode": 1, "reason": "Error",
                    "finishedAt": "2026-10-02T08:05:00Z"}}}]}
        })
    }

    #[tokio::test]
    async fn a_new_pod_in_a_crash_loop_makes_the_rollout_stuck_with_a_rollback() {
        let namespace: Namespace = from_value(json!({"metadata": {"name": "shop"}})).unwrap();
        let web = json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "web", "namespace": "shop", "generation": 2,
                "annotations": {"deployment.kubernetes.io/revision": "2"}},
            "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "web"}},
                "template": {"metadata": {"labels": {"app": "web"}},
                    "spec": {"containers": [{"name": "app", "image": "example/web:2"}]}}},
            "status": {"observedGeneration": 2, "replicas": 2, "updatedReplicas": 1,
                "readyReplicas": 1, "availableReplicas": 1}
        });
        let cluster = MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: Vec::new(),
            },
            namespaces: vec![namespace],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    deployments: vec![from_value(web).unwrap()],
                    pods: vec![
                        from_value(pod("web-1abcd", "web-1abcd-aaaaa", None)).unwrap(),
                        from_value(pod(
                            "web-2abcd",
                            "web-2abcd-bbbbb",
                            Some("CrashLoopBackOff"),
                        ))
                        .unwrap(),
                    ],
                    ..NamespaceObjects::default()
                },
            )]),
            replica_sets: BTreeMap::from([(
                "shop".to_owned(),
                vec![from_value(rs(1, 1)).unwrap(), from_value(rs(2, 0)).unwrap()],
            )]),
            ..State::default()
        });
        let app = app(&cluster);
        let (status, body) = send(
            &app,
            "GET",
            "/api/v1/projects/shop/services/deployment/web/rollout",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["status"], "stuck", "{body}");
        assert_eq!(body["reason"], "crashLoop");
        assert_eq!(body["message"]["key"], "rollout.stuck.crashLoop");
        assert_eq!(
            [
                &body["updated"],
                &body["updatedReady"],
                &body["old"],
                &body["oldReady"]
            ],
            [&json!(1), &json!(0), &json!(1), &json!(1)]
        );
        assert_eq!(body["diagnoses"][0]["pods"], json!(["web-2abcd-bbbbb"]));
        assert_eq!(
            body["rollback"]["changes"][0],
            json!({"op": "rollback", "workload": {"kind": "Deployment", "name": "web"},
                "revision": 1})
        );

        let (status, _) = send(
            &app,
            "GET",
            "/api/v1/projects/shop/services/job/web/rollout",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
