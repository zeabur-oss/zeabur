//! A change of one Deployment is reported when the cluster answers it, and an accepted one again
//! when its rollout ends.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::json;
    use zeabur_api::measure::{Fact, Report};
    use zeabur_kube::k8s_openapi::api::apps::v1::{Deployment, DeploymentStatus};
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects};
    use zeabur_telemetry::{Operation, Rollout, Submitted};

    use crate::common::send;
    use crate::support::TestApi;

    type Reports = Arc<Mutex<Vec<Report>>>;

    fn cluster() -> MemoryCluster {
        let namespace: Namespace = serde_json::from_value(json!({"metadata": {"name": "shop",
            "labels": {"zeabur.com/project": "true"}}}))
        .unwrap();
        let web: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "7", "generation": 1},
            "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "web"}},
                     "template": {"metadata": {"labels": {"app": "web"}},
                                  "spec": {"containers": [{"name": "web", "image": "web:1"}]}}}
        }))
        .unwrap();
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
                    deployments: vec![web],
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        })
    }

    fn api(cluster: &MemoryCluster) -> (Router, Reports) {
        let reports = Reports::default();
        let log = Arc::clone(&reports);
        let router = TestApi::new(cluster)
            .with_changes(Arc::new(move |report| log.lock().unwrap().push(report)))
            .router();
        (router, reports)
    }

    async fn scale(app: &Router, replicas: u32) -> StatusCode {
        let (status, planned) = send(
            app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({"changes": [{"op": "scale",
                "workload": {"kind": "Deployment", "name": "web"}, "replicas": replicas}]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        let (status, _) = send(
            app,
            "POST",
            "/api/v1/projects/shop/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        status
    }

    fn rolled_out(cluster: &MemoryCluster, replicas: i32) {
        cluster.update(|state| {
            let web = &mut state.objects.get_mut("shop").unwrap().deployments[0];
            web.status = Some(DeploymentStatus {
                observed_generation: web.metadata.generation,
                replicas: Some(replicas),
                updated_replicas: Some(replicas),
                available_replicas: Some(replicas),
                ..DeploymentStatus::default()
            });
        });
    }

    fn facts(reports: &Reports) -> Vec<Fact> {
        reports
            .lock()
            .unwrap()
            .iter()
            .map(|report| report.fact.clone())
            .collect()
    }

    async fn wait(seconds: u64) {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_scale_is_reported_when_accepted_and_when_its_replicas_are_available() {
        let cluster = cluster();
        let (app, reports) = api(&cluster);
        assert_eq!(scale(&app, 3).await, StatusCode::OK);
        assert_eq!(facts(&reports), [Fact::Submitted(Submitted::Accepted)]);

        wait(30).await;
        assert_eq!(facts(&reports).len(), 1, "the rollout goes on");

        rolled_out(&cluster, 3);
        wait(6).await;
        assert_eq!(
            facts(&reports),
            [
                Fact::Submitted(Submitted::Accepted),
                Fact::Finished(Rollout::Available)
            ]
        );
        let reports = reports.lock().unwrap();
        assert_eq!(reports[0].id, reports[1].id);
        assert_eq!(reports[1].operation, Operation::ScaleReplicas);
        assert_eq!(reports[1].user, "alice");
    }

    #[tokio::test(start_paused = true)]
    async fn a_refused_write_is_reported_with_its_status_and_not_followed() {
        let cluster = cluster();
        let (app, reports) = api(&cluster);
        cluster.update(|s| {
            s.forbidden_writes.insert("Deployment/web".to_owned());
        });
        assert_eq!(scale(&app, 2).await, StatusCode::FORBIDDEN);
        let reads = cluster.inspect(|state| state.object_reads).unwrap();
        wait(60).await;
        assert_eq!(facts(&reports), [Fact::Submitted(Submitted::Forbidden)]);
        assert_eq!(cluster.inspect(|state| state.object_reads).unwrap(), reads);
    }

    #[tokio::test(start_paused = true)]
    async fn a_newer_change_ends_the_wait_without_a_result() {
        let cluster = cluster();
        let (app, reports) = api(&cluster);
        assert_eq!(scale(&app, 2).await, StatusCode::OK);
        assert_eq!(scale(&app, 3).await, StatusCode::OK);
        rolled_out(&cluster, 3);
        wait(60).await;
        let finished: Vec<_> = reports
            .lock()
            .unwrap()
            .iter()
            .filter(|report| matches!(report.fact, Fact::Finished(_)))
            .map(|report| report.id.clone())
            .collect();
        assert_eq!(finished, [reports.lock().unwrap()[1].id.clone()]);
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_is_read_without_the_program() {
        let cluster = cluster();
        let app = TestApi::new(&cluster).router();
        assert_eq!(scale(&app, 2).await, StatusCode::OK);
        let reads = || cluster.inspect(|state| state.object_reads).unwrap();
        let after_apply = reads();
        wait(60).await;
        assert_eq!(reads(), after_apply);
    }
}
