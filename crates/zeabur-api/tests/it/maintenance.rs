//! Node maintenance: stopping and resuming scheduling, and maintenance's plan, apply and passes,
//! against an in-memory cluster.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::k8s_openapi::api::core::v1::{Node, Pod};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, Identity, NamespaceObjects, ServedResource};

    use crate::common::{app, send};

    fn node(name: &str) -> Node {
        serde_json::from_value(json!({
            "metadata": {"name": name, "uid": format!("uid-{name}"),
                         "labels": {"kubernetes.io/hostname": name}},
            "spec": {},
            "status": {"conditions": [{"type": "Ready", "status": "True"}],
                       "allocatable": {"cpu": "4", "memory": "8Gi", "pods": "110"}}
        }))
        .unwrap()
    }

    fn pod(name: &str, owner: Option<&str>) -> Pod {
        let owners = owner.map_or_else(
            || json!([]),
            |kind| {
                json!([{"apiVersion": "apps/v1", "kind": kind, "name": "web-5d8",
                          "uid": "o", "controller": true}])
            },
        );
        serde_json::from_value(json!({
            "metadata": {"name": name, "namespace": "shop", "uid": format!("uid-{name}"),
                         "labels": {"app": name}, "ownerReferences": owners},
            "spec": {"nodeName": "vps-3", "containers": [{"name": "main", "image": "x",
                "resources": {"requests": {"cpu": "250m", "memory": "256Mi"}}}]},
            "status": {"phase": "Running"}
        }))
        .unwrap()
    }

    fn cluster() -> MemoryCluster {
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec![],
            },
            namespaces: vec![
                serde_json::from_value(json!({"metadata": {"name": "shop"}})).unwrap(),
            ],
            nodes: vec![node("vps-1"), node("vps-3")],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    pods: vec![
                        pod("web", Some("ReplicaSet")),
                        pod("agent", Some("DaemonSet")),
                        pod("debug", None),
                    ],
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        })
    }

    fn unschedulable(cluster: &MemoryCluster) -> bool {
        cluster
            .inspect(|s| {
                s.nodes
                    .iter()
                    .find(|n| n.metadata.name.as_deref() == Some("vps-3"))
                    .and_then(|n| n.spec.as_ref()?.unschedulable)
            })
            .flatten()
            .unwrap_or(false)
    }

    async fn plan(app: &axum::Router, action: &str) -> Value {
        let (status, body) = send(
            app,
            "POST",
            "/api/v1/cluster/nodes/vps-3/maintenance/plan",
            Some(&json!({"action": action})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    #[tokio::test]
    async fn stopping_and_resuming_scheduling_patch_the_node_after_a_plan() {
        let cluster = cluster();
        let app = app(&cluster);
        let planned = plan(&app, "cordon").await;
        assert_eq!(planned["cordoned"], false);
        assert!(!unschedulable(&cluster), "a plan writes nothing");
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/cluster/nodes/vps-3/maintenance/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(unschedulable(&cluster));

        let planned = plan(&app, "uncordon").await;
        let (status, _) = send(
            &app,
            "POST",
            "/api/v1/cluster/nodes/vps-3/maintenance/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(!unschedulable(&cluster));
    }

    #[tokio::test]
    async fn maintenance_cordons_then_evicts_only_what_the_person_chose() {
        let cluster = cluster();
        let app = app(&cluster);
        let planned = plan(&app, "drain").await;
        let fates: Vec<(&str, &str)> = planned["pods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["name"].as_str().unwrap(), p["fate"].as_str().unwrap()))
            .collect();
        assert_eq!(
            fates,
            [("web", "moves"), ("debug", "deleted"), ("agent", "stays")]
        );
        assert_eq!(planned["capacity"]["status"], "fits");
        assert_eq!(planned["otherSchedulable"], 1);

        let (status, progress) = send(
            &app,
            "POST",
            "/api/v1/cluster/nodes/vps-3/maintenance/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{progress}");
        assert!(unschedulable(&cluster));
        assert_eq!(
            progress["pods"],
            json!([{"namespace": "shop", "name": "web", "outcome": "leaving", "zeabur": false}])
        );
        assert_eq!(progress["done"], false);

        cluster.update(|s| {
            if let Some(shop) = s.objects.get_mut("shop") {
                shop.pods
                    .retain(|p| p.metadata.name.as_deref() != Some("web"));
            }
        });
        let (status, progress) = send(
            &app,
            "POST",
            "/api/v1/cluster/nodes/vps-3/maintenance/evict",
            Some(&json!({"token": planned["token"], "includeDeleted": true})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{progress}");
        let outcomes: Vec<&str> = progress["pods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["outcome"].as_str().unwrap())
            .collect();
        assert_eq!(outcomes, ["gone", "leaving"]);
        let (_, nodes) = send(&app, "GET", "/api/v1/cluster/nodes", None).await;
        let vps3 = nodes["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["name"] == "vps-3")
            .unwrap();
        assert_eq!(
            (&vps3["movablePods"], &vps3["leavingPods"]),
            (&json!(1), &json!(1))
        );
    }

    async fn vps3_counts(app: &axum::Router) -> (Value, Value) {
        let (status, nodes) = send(app, "GET", "/api/v1/cluster/nodes", None).await;
        assert_eq!(status, StatusCode::OK, "{nodes}");
        let vps3 = nodes["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["name"] == "vps-3")
            .unwrap();
        (vps3["movablePods"].clone(), vps3["pinnedPods"].clone())
    }

    #[tokio::test]
    async fn a_pod_whose_volume_only_this_node_has_is_pinned_not_movable() {
        let cluster = cluster();
        cluster.update(|s| {
            let mut db = pod("db", Some("StatefulSet"));
            db.spec.as_mut().unwrap().volumes = Some(vec![
                serde_json::from_value(
                    json!({"name": "data", "persistentVolumeClaim": {"claimName": "data-db-0"}}),
                )
                .unwrap(),
            ]);
            s.objects.get_mut("shop").unwrap().pods.push(db);
            s.served.push(ServedResource::readable(
                "",
                "v1",
                "persistentvolumes",
                "PersistentVolume",
                false,
            ));
            s.dynamic.push(
                serde_json::from_value::<DynamicObject>(json!({
                    "apiVersion": "v1", "kind": "PersistentVolume",
                    "metadata": {"name": "pvc-1"},
                    "spec": {"claimRef": {"namespace": "shop", "name": "data-db-0"},
                             "nodeAffinity": {"required": {"nodeSelectorTerms": [
                                 {"matchExpressions": [{"key": "kubernetes.io/hostname",
                                     "operator": "In", "values": ["vps-3"]}]}]}}}
                }))
                .unwrap(),
            );
        });
        let app = app(&cluster);
        assert_eq!(vps3_counts(&app).await, (json!(2), json!(1)));

        cluster.update(|s| {
            s.forbidden_resources.insert("persistentvolumes".to_owned());
        });
        assert_eq!(
            vps3_counts(&app).await,
            (json!(3), json!(0)),
            "without PersistentVolumes every pod counts as movable"
        );
    }

    #[tokio::test]
    async fn passes_stop_once_the_node_takes_pods_again() {
        let cluster = cluster();
        let app = app(&cluster);
        let planned = plan(&app, "drain").await;
        let (status, _) = send(
            &app,
            "POST",
            "/api/v1/cluster/nodes/vps-3/maintenance/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        cluster.update(|s| {
            for node in &mut s.nodes {
                if let Some(spec) = node.spec.as_mut() {
                    spec.unschedulable = None;
                }
            }
        });
        let (status, _) = send(
            &app,
            "POST",
            "/api/v1/cluster/nodes/vps-3/maintenance/evict",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_person_who_may_not_patch_nodes_is_refused() {
        let cluster = cluster();
        cluster.update(|s| {
            s.forbidden_writes.insert("Node/vps-3".to_owned());
        });
        let app = app(&cluster);
        let planned = plan(&app, "cordon").await;
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/cluster/nodes/vps-3/maintenance/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(!unschedulable(&cluster));
    }
}
