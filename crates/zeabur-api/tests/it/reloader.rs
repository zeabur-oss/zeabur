//! Stakater Reloader: the cluster says whether it runs, a service's setting turns it on through a
//! plan, and a plan that changes a ConfigMap says Reloader restarts the services that follow it.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{ConfigMap, Namespace};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, NamespaceObjects, WriteBody};

    use crate::common::{app, planned_and_applied, send};

    fn reader(name: &str, annotations: &Value) -> Deployment {
        serde_json::from_value(json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": name, "namespace": "shop", "resourceVersion": "41",
                         "generation": 2, "annotations": annotations},
            "spec": {
                "selector": {"matchLabels": {"app": name}},
                "template": {
                    "metadata": {"labels": {"app": name}},
                    "spec": {"containers": [{"name": "app", "image": "example/app:1",
                        "envFrom": [{"configMapRef": {"name": "settings"}}]}]}
                }
            }
        }))
        .unwrap()
    }

    fn cluster() -> MemoryCluster {
        let namespace: Namespace = serde_json::from_value(json!({"metadata": {"name": "shop",
            "labels": {"zeabur.com/project": "true"}}}))
        .unwrap();
        let settings: ConfigMap = serde_json::from_value(json!({
            "metadata": {"name": "settings", "namespace": "shop", "resourceVersion": "7"},
            "data": {"MODE": "production"}
        }))
        .unwrap();
        MemoryCluster::new(State {
            namespaces: vec![namespace],
            objects: BTreeMap::from([(
                "shop".to_owned(),
                NamespaceObjects {
                    namespace: "shop".to_owned(),
                    deployments: vec![
                        reader("web", &json!({"reloader.stakater.com/auto": "true"})),
                        reader("worker", &json!({})),
                    ],
                    config_maps: vec![settings],
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        })
    }

    fn reloader(available: i64) -> DynamicObject {
        serde_json::from_value(json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "reloader-reloader", "namespace": "reloader",
                         "labels": {"app.kubernetes.io/name": "reloader"}},
            "status": {"availableReplicas": available}
        }))
        .unwrap()
    }

    async fn state(cluster: &MemoryCluster) -> Value {
        let (status, body) =
            send(&app(cluster), "GET", "/api/v1/cluster/config-reload", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    #[tokio::test]
    async fn the_cluster_says_whether_reloader_runs_and_never_refuses() {
        let cluster = cluster();
        assert_eq!(state(&cluster).await["reloader"], "notInstalled");
        cluster.update(|s| s.dynamic.push(reloader(0)));
        assert_eq!(state(&cluster).await["reloader"], "stopped");
        cluster.update(|s| s.dynamic = vec![reloader(1)]);
        let running = state(&cluster).await;
        assert_eq!(running["reloader"], "running");
        assert_eq!(running["deployments"][0]["namespace"], "reloader");
        cluster.update(|s| {
            s.forbidden_resources.insert("deployments.apps".to_owned());
        });
        assert_eq!(state(&cluster).await["reloader"], "unknown");
    }

    fn change_settings() -> Value {
        json!({"changes": [{"op": "data", "kind": "ConfigMap", "name": "settings",
            "changes": [{"op": "set", "key": "MODE", "value": "maintenance"}]}]})
    }

    async fn warnings(cluster: &MemoryCluster) -> Value {
        let (status, planned) = send(
            &app(cluster),
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&change_settings()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        planned["warnings"].clone()
    }

    #[tokio::test]
    async fn a_data_change_names_who_reloader_restarts_and_who_still_needs_a_restart() {
        let cluster = cluster();
        cluster.update(|s| s.dynamic.push(reloader(1)));
        let web = json!([{"kind": "Deployment", "name": "web"}]);
        let worker = json!([{"kind": "Deployment", "name": "worker"}]);
        assert_eq!(
            warnings(&cluster).await,
            json!([
                {"code": "reloaderRestarts", "workloads": web, "confirmed": true},
                {"code": "restartNeeded", "workloads": worker},
            ])
        );
        // Someone who may not see Reloader is told it restarts web, unconfirmed.
        cluster.update(|s| {
            s.forbidden_resources.insert("deployments.apps".to_owned());
        });
        assert_eq!(warnings(&cluster).await[0]["confirmed"], false);
        // Without Reloader the annotation restarts nothing.
        cluster.update(|s| {
            s.forbidden_resources.clear();
            s.dynamic.clear();
        });
        assert_eq!(
            warnings(&cluster).await,
            json!([{"code": "restartNeeded",
                "workloads": [{"kind": "Deployment", "name": "web"},
                              {"kind": "Deployment", "name": "worker"}]}])
        );
    }

    #[tokio::test]
    async fn the_setting_is_one_annotation_on_the_workload() {
        let cluster = cluster();
        let app = app(&cluster);
        planned_and_applied(
            &app,
            "shop",
            json!([{"op": "configReload",
                "workload": {"kind": "Deployment", "name": "worker"}, "enabled": true}]),
        )
        .await;
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let WriteBody::JsonPatch(operations) = &writes[0].body else {
            panic!("the setting is a JSON Patch");
        };
        assert_eq!(
            operations[1],
            json!({"op": "add", "path": "/metadata/annotations/reloader.stakater.com~1auto",
                   "value": "true"})
        );
        let (_, service) = send(
            &app,
            "GET",
            "/api/v1/projects/shop/services/deployment/web",
            None,
        )
        .await;
        assert_eq!(service["configReload"]["auto"], true);
    }
}
