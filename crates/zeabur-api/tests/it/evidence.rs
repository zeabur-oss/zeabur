//! A service's evidence bundle against an in-memory cluster: problems, the way to it, events, the
//! previous run's log and the objects, with no Secret and no variable value.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{from_value, json};
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, SecretSummary};

    use crate::common::{app, send};

    fn cluster() -> MemoryCluster {
        let namespace: Namespace = from_value(json!({"metadata": {"name": "shop"}})).unwrap();
        let web = json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "web", "namespace": "shop"},
            "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "web"}},
                "template": {"metadata": {"labels": {"app": "web"}},
                    "spec": {"containers": [{"name": "app", "image": "example/web:2",
                        "ports": [{"containerPort": 8080}],
                        "env": [{"name": "PASSWORD", "value": "hunter2"},
                            {"name": "TOKEN", "valueFrom":
                                {"secretKeyRef": {"name": "web-secret", "key": "token"}}}]}]}}},
            "status": {"replicas": 1, "readyReplicas": 0}
        });
        let service = json!({
            "apiVersion": "v1", "kind": "Service",
            "metadata": {"name": "web", "namespace": "shop"},
            "spec": {"selector": {"app": "web"}, "ports": [{"port": 80, "targetPort": 8080}]}
        });
        let pod = json!({
            "apiVersion": "v1", "kind": "Pod",
            "metadata": {"name": "web-1abcd-aaaaa", "namespace": "shop", "labels": {"app": "web"}},
            "spec": {"containers": [{"name": "app", "image": "example/web:2"}]},
            "status": {"phase": "Running", "containerStatuses": [{"name": "app", "ready": false,
                "restartCount": 4, "image": "example/web:2", "imageID": "",
                "state": {"waiting": {"reason": "CrashLoopBackOff", "message": "back-off"}},
                "lastState": {"terminated": {"exitCode": 1, "reason": "Error",
                    "finishedAt": "2026-10-02T08:05:00Z"}}}]}
        });
        let event = from_value(json!({
            "metadata": {"name": "e", "namespace": "shop"}, "eventTime": null,
            "type": "Warning", "reason": "BackOff",
            "note": "Back-off restarting failed container app",
            "regarding": {"kind": "Pod", "name": "web-1abcd-aaaaa"},
            "deprecatedCount": 7, "deprecatedLastTimestamp": "2026-10-02T08:06:00Z"
        }))
        .unwrap();
        let secret = json!({"apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": "web-secret", "namespace": "shop"}, "type": "Opaque"});
        MemoryCluster::new(State {
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
                    services: vec![from_value(service).unwrap()],
                    pods: vec![from_value(pod).unwrap()],
                    secrets: vec![SecretSummary {
                        secret: from_value(secret).unwrap(),
                        key_count: 1,
                    }],
                    ..NamespaceObjects::default()
                },
            )]),
            secret_values: BTreeMap::from([(
                "shop/web-secret".to_owned(),
                BTreeMap::from([("token".to_owned(), b"s3cr3t-token".to_vec())]),
            )]),
            events: BTreeMap::from([("shop".to_owned(), vec![event])]),
            logs: BTreeMap::from([("shop/web-1abcd-aaaaa".to_owned(), String::new())]),
            previous_logs: BTreeMap::from([(
                "shop/web-1abcd-aaaaa".to_owned(),
                "starting\npanic: cannot open /data/db: permission denied\n".to_owned(),
            )]),
            ..State::default()
        })
    }

    #[tokio::test]
    async fn the_bundle_holds_problems_events_logs_and_objects_but_no_secret() {
        let app = app(&cluster());
        let (status, body) = send(
            &app,
            "GET",
            "/api/v1/projects/shop/services/deployment/web/evidence",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let text = body["markdown"].as_str().unwrap();
        for wanted in [
            "# Deployment `web` in project `shop`",
            "**crashLoop** in container `app`",
            "## The way to it",
            "web.shop.svc.cluster.local",
            "BackOff on Pod/web-1abcd-aaaaa ×7",
            "panic: cannot open /data/db: permission denied",
            "### Deployment `web`",
            "### Service `web`",
            "secretKeyRef",
        ] {
            assert!(text.contains(wanted), "{wanted:?} missing from:\n{text}");
        }
        for unwanted in ["hunter2", "s3cr3t-token", "managedFields"] {
            assert!(!text.contains(unwanted), "{unwanted:?} in:\n{text}");
        }
    }
}
