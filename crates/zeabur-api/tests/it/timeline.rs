//! A service's timeline against an in-memory cluster: releases with the plan that reverts the
//! running one, field managers' last writes, warnings folded by reason, out-of-memory kills.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, from_value, json};
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects};

    use crate::common::{app, send};

    fn rs(n: u32, created: &str) -> Value {
        json!({
            "metadata": {"name": format!("web-{n}abcd"), "namespace": "shop",
                "creationTimestamp": created,
                "ownerReferences": [{"apiVersion": "apps/v1", "kind": "Deployment", "name": "web",
                    "uid": "u", "controller": true}],
                "annotations": {"deployment.kubernetes.io/revision": n.to_string()}},
            "spec": {"selector": {"matchLabels": {"app": "web"}},
                "template": {"metadata": {"labels": {"app": "web"}},
                    "spec": {"containers": [{"name": "app", "image": format!("example/web:{n}")}]}}},
            "status": {"replicas": 1, "readyReplicas": 1}
        })
    }

    fn cluster() -> MemoryCluster {
        let namespace: Namespace = from_value(json!({"metadata": {"name": "shop"}})).unwrap();
        let web = json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "web", "namespace": "shop",
                "annotations": {"deployment.kubernetes.io/revision": "2"},
                "managedFields": [
                    {"manager": "zeabur", "operation": "Apply", "time": "2026-10-02T09:00:00Z",
                        "fieldsV1": {"f:spec": {"f:template": {"f:spec": {}}}}},
                    {"manager": "kube-controller-manager", "operation": "Update",
                        "subresource": "status", "time": "2026-10-02T09:01:00Z",
                        "fieldsV1": {"f:status": {}}}]},
            "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "web"}},
                "template": {"metadata": {"labels": {"app": "web"}},
                    "spec": {"containers": [{"name": "app", "image": "example/web:2"}]}}}
        });
        let pod = json!({
            "apiVersion": "v1", "kind": "Pod",
            "metadata": {"name": "web-2abcd-x8k2p", "namespace": "shop", "labels": {"app": "web"}},
            "spec": {"containers": [{"name": "app", "image": "example/web:2"}]},
            "status": {"phase": "Running", "containerStatuses": [{"name": "app", "ready": false,
                "restartCount": 3, "image": "example/web:2", "imageID": "",
                "lastState": {"terminated": {"exitCode": 137, "reason": "OOMKilled",
                    "finishedAt": "2026-10-02T10:00:00Z"}}}]}
        });
        let event = |name: &str, last: &str, count: i32| {
            from_value(json!({
                "metadata": {"name": name, "namespace": "shop"},
                "eventTime": null,
                "type": "Warning", "reason": "BackOff",
                "note": "Back-off restarting failed container app",
                "regarding": {"kind": "Pod", "name": "web-1abcd-q2w3e"},
                "deprecatedCount": count, "deprecatedLastTimestamp": last
            }))
            .unwrap()
        };
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
                    pods: vec![from_value(pod).unwrap()],
                    ..NamespaceObjects::default()
                },
            )]),
            replica_sets: BTreeMap::from([(
                "shop".to_owned(),
                vec![
                    from_value(rs(1, "2026-10-01T08:00:00Z")).unwrap(),
                    from_value(rs(2, "2026-10-02T09:00:00Z")).unwrap(),
                ],
            )]),
            events: BTreeMap::from([(
                "shop".to_owned(),
                vec![
                    event("a", "2026-10-01T12:00:00Z", 4),
                    event("b", "2026-10-01T12:30:00Z", 2),
                ],
            )]),
            ..State::default()
        })
    }

    #[tokio::test]
    async fn a_timeline_lists_releases_writes_warnings_and_kills_newest_first() {
        let app = app(&cluster());
        let (status, body) = send(
            &app,
            "GET",
            "/api/v1/projects/shop/services/deployment/web/timeline",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let kinds: Vec<&str> = body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            ["termination", "release", "write", "warning", "release"],
            "{body}"
        );
        let items = &body["items"];
        assert_eq!(items[0]["message"]["key"], "timeline.termination.oomKilled");
        assert_eq!(items[1]["release"]["current"], true);
        assert_eq!(
            items[1]["revert"],
            json!({"changes": [{"op": "rollback",
                "workload": {"kind": "Deployment", "name": "web"}, "revision": 1}],
                "restartAfter": false})
        );
        assert_eq!(items[2]["writer"]["tool"], "zeabur");
        assert_eq!(items[2]["fields"], json!(["spec.template"]));
        assert_eq!(items[3]["message"]["key"], "timeline.warning.crashLoop");
        assert_eq!(items[3]["count"], 6);
        assert_eq!(items[3]["at"], "2026-10-01T12:30:00Z");
        assert!(items[4].get("revert").is_none());

        let (status, project) = send(&app, "GET", "/api/v1/projects/shop/timeline", None).await;
        assert_eq!(status, StatusCode::OK, "{project}");
        assert_eq!(project["items"], body["items"]);
    }
}
