//! A project's storage against an in-memory cluster with k3s's local-path class and Longhorn:
//! the classes a new volume can use, and where each claim's data is.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, from_value, json};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, ServedResource};

    use crate::common::{app, send};

    fn class(name: &str, provisioner: &str, expands: bool, default: bool) -> Value {
        let annotations = if default {
            json!({"storageclass.kubernetes.io/is-default-class": "true"})
        } else {
            json!({})
        };
        json!({"metadata": {"name": name, "annotations": annotations},
            "provisioner": provisioner, "allowVolumeExpansion": expands,
            "reclaimPolicy": "Delete", "volumeBindingMode": "WaitForFirstConsumer"})
    }

    fn claim(name: &str, class: &str, volume: &str) -> Value {
        json!({"metadata": {"name": name, "namespace": "shop"},
            "spec": {"accessModes": ["ReadWriteOnce"], "storageClassName": class,
                "volumeName": volume, "resources": {"requests": {"storage": "5Gi"}}},
            "status": {"phase": "Bound", "capacity": {"storage": "5Gi"}}})
    }

    fn cluster() -> MemoryCluster {
        let mut state = State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec!["developers".to_owned()],
            },
            namespaces: vec![from_value(json!({"metadata": {"name": "shop"}})).unwrap()],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    persistent_volume_claims: vec![
                        from_value(claim("web-data", "local-path", "pvc-local")).unwrap(),
                        from_value(claim("files", "longhorn", "pvc-replicated")).unwrap(),
                    ],
                    ..NamespaceObjects::default()
                },
            )]),
            storage_classes: Some(vec![
                from_value(class("longhorn", "driver.longhorn.io", true, false)).unwrap(),
                from_value(class("local-path", "rancher.io/local-path", false, true)).unwrap(),
            ]),
            ..State::default()
        };
        state.served.push(ServedResource::readable(
            "",
            "v1",
            "persistentvolumes",
            "PersistentVolume",
            false,
        ));
        state.dynamic.extend([
            from_value(json!({"apiVersion": "v1", "kind": "PersistentVolume",
                "metadata": {"name": "pvc-local"},
                "spec": {"storageClassName": "local-path",
                    "nodeAffinity": {"required": {"nodeSelectorTerms": [{"matchExpressions": [
                        {"key": "kubernetes.io/hostname", "operator": "In",
                            "values": ["vps-3"]}]}]}}}}))
            .unwrap(),
            from_value(json!({"apiVersion": "v1", "kind": "PersistentVolume",
                "metadata": {"name": "pvc-replicated"},
                "spec": {"storageClassName": "longhorn", "csi": {"driver": "driver.longhorn.io",
                    "volumeHandle": "pvc-replicated"}}}))
            .unwrap(),
        ]);
        MemoryCluster::new(state)
    }

    async fn storage(cluster: &MemoryCluster) -> Value {
        let (status, body) =
            send(&app(cluster), "GET", "/api/v1/projects/shop/storage", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    #[tokio::test]
    async fn classes_come_default_first_with_what_kind_of_disk_each_gives() {
        let body = storage(&cluster()).await;
        let classes = body["classes"].as_array().unwrap();
        let summary: Vec<(&str, &str, bool, bool)> = classes
            .iter()
            .map(|c| {
                (
                    c["name"].as_str().unwrap(),
                    c["kind"].as_str().unwrap(),
                    c["default"].as_bool().unwrap(),
                    c["allowExpansion"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("local-path", "local", true, false),
                ("longhorn", "replicated", false, true)
            ]
        );
        assert_eq!(classes[0]["bindingMode"], "WaitForFirstConsumer");
        assert_eq!(classes[0]["origin"]["kind"], "StorageClass");
        assert_eq!(body["unreadable"], json!([]));
    }

    #[tokio::test]
    async fn a_local_claim_names_its_node_and_a_longhorn_claim_is_not_pinned() {
        let body = storage(&cluster()).await;
        let claims: BTreeMap<&str, &Value> = body["claims"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c["claim"].as_str().unwrap(), c))
            .collect();
        let local = claims["web-data"];
        assert_eq!(
            (&local["storageClass"], &local["pinned"], &local["node"]),
            (&json!("local-path"), &json!(true), &json!("vps-3"))
        );
        assert_eq!(local["capacity"], "5Gi");
        let replicated = claims["files"];
        assert_eq!(replicated["pinned"], false);
        assert!(replicated.get("node").is_none());
    }

    #[tokio::test]
    async fn refused_classes_and_volumes_are_named_and_the_claims_stay() {
        let cluster = cluster();
        cluster.update(|s| {
            s.storage_classes = None;
            s.forbidden_resources.insert("persistentvolumes".to_owned());
        });
        let body = storage(&cluster).await;
        assert_eq!(body["classes"], json!([]));
        let refused: Vec<&str> = body["unreadable"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["resource"].as_str().unwrap())
            .collect();
        assert_eq!(
            refused,
            ["storageclasses.storage.k8s.io", "persistentvolumes"]
        );
        let claims = body["claims"].as_array().unwrap();
        assert_eq!(claims.len(), 2);
        assert!(claims.iter().all(|c| c.get("pinned").is_none()));
        assert_eq!(claims[0]["storageClass"], "longhorn");
    }

    #[tokio::test]
    async fn a_restored_claim_replaces_the_old_one_and_the_deployment_recreates() {
        let cluster = cluster();
        cluster.update(|s| {
            let shop = s.objects.get_mut("shop").unwrap();
            shop.persistent_volume_claims
                .push(from_value(claim("web-data-restored", "local-path", "pvc-new")).unwrap());
            shop.deployments.push(
                from_value(json!({
                    "metadata": {"name": "web", "namespace": "shop", "generation": 2},
                    "spec": {"selector": {"matchLabels": {"app": "web"}},
                        "strategy": {"type": "RollingUpdate",
                                     "rollingUpdate": {"maxSurge": 1}},
                        "template": {"metadata": {"labels": {"app": "web"}},
                            "spec": {"containers": [{"name": "web", "image": "nginx",
                                "volumeMounts": [{"name": "data", "mountPath": "/data"}]}],
                            "volumes": [{"name": "data",
                                "persistentVolumeClaim": {"claimName": "web-data"}}]}}}
                }))
                .unwrap(),
            );
        });
        let app = app(&cluster);
        crate::common::planned_and_applied(
            &app,
            "shop",
            json!([{"op": "swapClaim", "workload": {"kind": "Deployment", "name": "web"},
                    "from": "web-data", "to": "web-data-restored"}]),
        )
        .await;
        let web = cluster
            .inspect(|s| s.objects["shop"].deployments[0].clone())
            .unwrap();
        let web = serde_json::to_value(web).unwrap();
        assert_eq!(
            web["spec"]["template"]["spec"]["volumes"][0]["persistentVolumeClaim"]["claimName"],
            "web-data-restored"
        );
        assert_eq!(web["spec"]["strategy"], json!({"type": "Recreate"}));
        let claims = cluster
            .inspect(|s| s.objects["shop"].persistent_volume_claims.len())
            .unwrap();
        assert_eq!(claims, 3, "the old claim stays");

        let (status, _) = crate::common::plan_and_apply(
            &app,
            "shop",
            json!([{"op": "swapClaim", "workload": {"kind": "Deployment", "name": "web"},
                    "from": "web-data-restored", "to": "missing"}]),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
