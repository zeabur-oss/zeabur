//! The chart's exact updater is read as the person; an unreadable updater is never absent.

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde::Deserialize;
    use serde_json::json;
    use zeabur_core::problem::{Kind, Problem};
    use zeabur_kube::NamespaceObjects;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::ConfigMap;
    use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
    use zeabur_kube::memory::{MemoryCluster, State};

    use crate::common::{app, send};

    #[derive(Debug, Deserialize)]
    struct Answer {
        status: String,
        message: String,
        problem: Option<Problem>,
    }

    async fn state(cluster: &MemoryCluster) -> Answer {
        let (status, body) = send(
            &app(cluster),
            "GET",
            "/api/v1/cluster/database-updater",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        serde_json::from_value(body).unwrap()
    }

    fn deployed() -> MemoryCluster {
        let map: ConfigMap = serde_json::from_value(json!({
            "metadata": {"name": "zeabur-info", "namespace": "kube-public"},
            "data": {"databaseUpdaterNamespace": "platform", "databaseUpdaterDeployment": "database-updater"}
        })).unwrap();
        let deployment: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "database-updater", "namespace": "platform", "generation": 2},
            "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "database-updater"}},
                "template": {"spec": {"containers": [{"name": "updater", "image": "example/updater:1"}]}}},
            "status": {"observedGeneration": 2, "replicas": 1, "updatedReplicas": 1,
                "readyReplicas": 1, "availableReplicas": 1,
                "conditions": [{"type": "Available", "status": "True"}]}
        })).unwrap();
        MemoryCluster::new(State {
            objects: [
                (
                    "kube-public".to_owned(),
                    NamespaceObjects {
                        namespace: "kube-public".to_owned(),
                        config_maps: vec![map],
                        ..NamespaceObjects::default()
                    },
                ),
                (
                    "platform".to_owned(),
                    NamespaceObjects {
                        namespace: "platform".to_owned(),
                        deployments: vec![deployment],
                        ..NamespaceObjects::default()
                    },
                ),
            ]
            .into(),
            ..State::default()
        })
    }

    #[tokio::test]
    async fn a_locator_or_deployment_that_is_absent_is_missing() {
        let cluster = MemoryCluster::new(State::default());
        assert_eq!(state(&cluster).await.status, "missing");
        let cluster = deployed();
        cluster.update(|s| s.objects.get_mut("platform").unwrap().deployments.clear());
        assert_eq!(state(&cluster).await.status, "missing");
        cluster.update(|s| s.objects.get_mut("kube-public").unwrap().config_maps[0].data = None);
        assert_eq!(state(&cluster).await.status, "missing");
    }

    #[tokio::test]
    async fn only_the_named_updater_counts_without_cluster_wide_permission() {
        let cluster = deployed();
        cluster.update(|s| {
            s.forbidden_resources.extend([
                "deployments.apps".to_owned(),
                "customresourcedefinitions.apiextensions.k8s.io".to_owned(),
            ]);
        });
        let ready = state(&cluster).await;
        assert_eq!(ready.status, "ready");
        assert!(ready.problem.is_none());
        cluster.update(|s| {
            s.objects.get_mut("platform").unwrap().deployments[0]
                .metadata
                .name = Some("another-updater".to_owned());
        });
        assert_eq!(state(&cluster).await.status, "missing");
    }

    #[tokio::test]
    async fn rollout_pause_and_deletion_are_not_ready() {
        for change in ["generation", "starting", "paused", "deleting"] {
            let cluster = deployed();
            cluster.update(|s| {
                let deployment = &mut s.objects.get_mut("platform").unwrap().deployments[0];
                match change {
                    "generation" => deployment.metadata.generation = Some(3),
                    "starting" => deployment.status.as_mut().unwrap().available_replicas = Some(0),
                    "paused" => {
                        deployment.spec.as_mut().unwrap().replicas = Some(0);
                        let status = deployment.status.as_mut().unwrap();
                        status.replicas = Some(0);
                        status.updated_replicas = Some(0);
                        status.ready_replicas = Some(0);
                        status.available_replicas = Some(0);
                    }
                    "deleting" => {
                        deployment.metadata.deletion_timestamp =
                            Some(Time("2026-11-27T12:00:00Z".parse().unwrap()));
                    }
                    _ => unreachable!(),
                }
            });
            assert_eq!(state(&cluster).await.status, "notReady", "{change}");
        }
    }

    #[tokio::test]
    async fn refusing_the_public_locator_or_named_deployment_is_unknown() {
        for namespace in ["kube-public", "platform"] {
            let cluster = deployed();
            cluster.update(|s| {
                s.forbidden_namespaces.insert(namespace.to_owned());
            });
            let answer = state(&cluster).await;
            assert_eq!(answer.status, "unknown");
            assert_eq!(answer.problem.unwrap().kind, Kind::Forbidden);
            assert!(answer.message.contains("access"));
        }
    }

    #[tokio::test]
    async fn cluster_failure_and_incomplete_configuration_are_not_reported_as_missing() {
        let cluster = deployed();
        cluster.update(|s| s.busy = Some(5));
        let answer = state(&cluster).await;
        assert_eq!(answer.status, "unknown");
        assert_eq!(answer.problem.unwrap().kind, Kind::Unavailable);
        cluster.update(|s| {
            s.busy = None;
            s.objects.get_mut("kube-public").unwrap().config_maps[0]
                .data
                .as_mut()
                .unwrap()
                .remove("databaseUpdaterDeployment");
        });
        assert_eq!(state(&cluster).await.status, "unknown");
    }
}
