//! Releases, storage and Secret edits through plan and apply, against an in-memory cluster.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, from_value, json};
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, WriteBody};

    use crate::common::{app, planned_and_applied, send};

    fn owner(kind: &str, name: &str) -> Value {
        json!([{"apiVersion": "apps/v1", "kind": kind, "name": name, "uid": "u", "controller": true}])
    }

    fn cluster() -> MemoryCluster {
        let namespace: Namespace = from_value(json!({"metadata": {"name": "shop",
            "labels": {"zeabur.com/project": "true"}}}))
        .unwrap();
        let web = json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "41",
                         "annotations": {"deployment.kubernetes.io/revision": "2"}},
            "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "web"}},
                "template": {"metadata": {"labels": {"app": "web"}},
                    "spec": {"containers": [{"name": "app", "image": "example/web:2",
                        "ports": [{"containerPort": 8080}]}]}}}
        });
        let rs = |n: u32, image: &str| {
            from_value(json!({
                "metadata": {"name": format!("web-{n}"), "namespace": "shop",
                             "ownerReferences": owner("Deployment", "web"),
                             "annotations": {"deployment.kubernetes.io/revision": n.to_string()}},
                "spec": {"selector": {"matchLabels": {"app": "web"}},
                         "template": {"metadata": {"labels": {"app": "web", "pod-template-hash": "h"}},
                                      "spec": {"containers": [{"name": "app", "image": image}]}}},
                "status": {"replicas": 1, "readyReplicas": 1}
            }))
            .unwrap()
        };
        let claim = json!({
            "apiVersion": "v1", "kind": "PersistentVolumeClaim",
            "metadata": {"name": "uploads", "namespace": "shop", "resourceVersion": "9"},
            "spec": {"accessModes": ["ReadWriteOnce"], "storageClassName": "local-path",
                     "resources": {"requests": {"storage": "1Gi"}}}
        });
        let class = json!({"metadata": {"name": "local-path",
            "annotations": {"storageclass.kubernetes.io/is-default-class": "true"}},
            "provisioner": "rancher.io/local-path", "allowVolumeExpansion": true});
        let secret = json!({"apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": "web-secret", "namespace": "shop", "resourceVersion": "13"}, "type": "Opaque"});
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec!["developers".to_owned()],
            },
            namespaces: vec![namespace],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    deployments: vec![from_value(web).unwrap()],
                    persistent_volume_claims: vec![from_value(claim).unwrap()],
                    secrets: vec![zeabur_kube::SecretSummary {
                        secret: from_value(secret).unwrap(),
                        key_count: 2,
                    }],
                    ..NamespaceObjects::default()
                },
            )]),
            secret_values: BTreeMap::from([(
                "shop/web-secret".to_owned(),
                BTreeMap::from([
                    ("password".to_owned(), b"hunter2".to_vec()),
                    ("user".to_owned(), b"admin".to_vec()),
                ]),
            )]),
            replica_sets: BTreeMap::from([(
                "shop".to_owned(),
                vec![rs(1, "example/web:1"), rs(2, "example/web:2")],
            )]),
            storage_classes: Some(vec![from_value(class).unwrap()]),
            ..State::default()
        })
    }

    /// Plans `changes`, applies the plan, and returns the plan and the writes made.
    async fn plan_and_apply(cluster: &MemoryCluster, changes: Value) -> (Value, Vec<WriteBody>) {
        let planned = planned_and_applied(&app(cluster), "shop", changes).await;
        let writes = cluster
            .inspect(|s| s.writes.iter().map(|w| w.body.clone()).collect())
            .unwrap();
        (planned, writes)
    }

    #[tokio::test]
    async fn releases_list_the_replica_sets_newest_first() {
        let (status, body) = send(
            &app(&cluster()),
            "GET",
            "/api/v1/projects/shop/services/deployment/web/releases",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(
            (items[0]["revision"].clone(), items[0]["current"].clone()),
            (json!(2), json!(true))
        );
        assert_eq!(items[1]["images"], json!(["example/web:1"]));
        assert!(
            items[1]["template"]["metadata"]["labels"]
                .get("pod-template-hash")
                .is_none()
        );
        let (status, _) = send(
            &app(&cluster()),
            "GET",
            "/api/v1/projects/shop/services/deployment/none/releases",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_claim_grows_when_its_class_allows_it() {
        let cluster = cluster();
        let (_, writes) = plan_and_apply(
            &cluster,
            json!([{"op": "expandVolume", "claim": "uploads", "size": "5Gi"}]),
        )
        .await;
        let [WriteBody::JsonPatch(operations)] = writes.as_slice() else {
            panic!("one patch: {writes:?}");
        };
        assert_eq!(
            operations,
            &json!([
                {"op": "test", "path": "/metadata/resourceVersion", "value": "9"},
                {"op": "replace", "path": "/spec/resources/requests/storage", "value": "5Gi"}
            ])
        );
    }

    #[tokio::test]
    async fn a_new_volume_creates_its_claim_before_mounting_it() {
        let cluster = cluster();
        let (_, writes) = plan_and_apply(
            &cluster,
            json!([{"op": "addVolume", "workload": {"kind": "Deployment", "name": "web"},
                    "volume": {"name": "cache", "size": "2Gi", "mountPath": "/cache"}}]),
        )
        .await;
        assert!(
            matches!(&writes[0], WriteBody::Create(body) if body["kind"] == "PersistentVolumeClaim")
        );
        assert!(matches!(&writes[1], WriteBody::JsonPatch(_)));
    }

    #[tokio::test]
    async fn a_secret_edit_never_writes_the_mask() {
        let cluster = cluster();
        let yaml = "apiVersion: v1\nkind: Secret\nmetadata: {name: web-secret, labels: {tier: data}}\ntype: Opaque\ndata:\n  password: (hidden)\n  user: \u{2022}\u{2022}\u{2022}\u{2022}\n  legacy: \u{ab}redacted\u{bb}\nstringData:\n  token: plain-typed-value\n  note: (hidden)\n";
        let (planned, writes) = plan_and_apply(
            &cluster,
            json!([{"op": "edit", "resource": {"group": "", "version": "v1", "resource": "secrets"},
                    "name": "web-secret", "yaml": yaml}]),
        )
        .await;
        let [WriteBody::JsonPatch(operations)] = writes.as_slice() else {
            panic!("one patch: {writes:?}");
        };
        let text = operations.to_string();
        assert!(
            !text.contains("redacted") && !text.contains("(hidden)") && !text.contains('\u{2022}'),
            "{text}"
        );
        assert!(
            !text.contains("/data/password") && !text.contains("/data/user"),
            "{text}"
        );
        assert!(text.contains("/data/token"), "{text}");
        // The label changes with the keys, in the same patch.
        assert!(text.contains("/metadata/labels"), "{text}");
        // The token is random bytes in base64: leave it out of the search for the value.
        let mut shown = planned;
        if let Some(fields) = shown.as_object_mut() {
            fields.remove("token");
        }
        assert!(
            !shown.to_string().contains("plain-typed-value"),
            "the plan shows no Secret value"
        );
    }

    /// A plan the planner refuses because the object cannot take the change answers `422` with a
    /// problem document (RFC 9457); the web client tells its refusals apart by that status.
    #[tokio::test]
    async fn a_refused_plan_is_a_422_problem_and_writes_nothing() {
        let cluster = cluster();
        let (status, problem) = send(
            &app(&cluster),
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({"changes": [{"op": "edit", "resource": {"group": "", "version": "v1", "resource": "pods"},
                "name": "web-1", "yaml": "apiVersion: v1\nkind: Pod\nmetadata: {name: web-1}\n"}]})),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
        assert_eq!(problem["status"], 422, "{problem}");
        assert_eq!(problem["kind"], "invalid", "{problem}");
        assert!(
            problem["type"].is_string() && problem["detail"].is_string(),
            "{problem}"
        );
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }
}
