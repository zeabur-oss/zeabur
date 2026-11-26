//! Every save is reported once, with the answer the person got.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::measure::Report;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects};
    use zeabur_telemetry::Submitted;

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

    fn scale(replicas: u32) -> Value {
        json!({"changes": [{"op": "scale",
            "workload": {"kind": "Deployment", "name": "web"}, "replicas": replicas}]})
    }

    async fn plan(app: &Router, changes: &Value) -> Value {
        let (status, planned) =
            send(app, "POST", "/api/v1/projects/shop/plan", Some(changes)).await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        planned
    }

    async fn apply(app: &Router, planned: &Value) -> StatusCode {
        let token = json!({"token": planned["token"]});
        send(app, "POST", "/api/v1/projects/shop/apply", Some(&token))
            .await
            .0
    }

    fn results(reports: &Reports) -> Vec<Submitted> {
        reports.lock().unwrap().iter().map(|r| r.result).collect()
    }

    #[tokio::test]
    async fn a_save_of_several_changes_to_several_objects_is_one_report() {
        let cluster = cluster();
        let (app, reports) = api(&cluster);
        let changes = json!({"changes": [
            {"op": "scale", "workload": {"kind": "Deployment", "name": "web"}, "replicas": 2},
            {"op": "env", "workload": {"kind": "Deployment", "name": "web"},
             "changes": [{"op": "set", "name": "MODE", "value": "maintenance"}]},
            {"op": "create", "object": {"apiVersion": "v1", "kind": "ConfigMap",
             "metadata": {"name": "settings"}}},
        ]});
        let planned = plan(&app, &changes).await;
        assert_eq!(apply(&app, &planned).await, StatusCode::OK);
        assert_eq!(results(&reports), [Submitted::Accepted]);
        assert_eq!(reports.lock().unwrap()[0].user, "alice");
    }

    #[tokio::test]
    async fn a_refused_write_is_reported_with_its_status() {
        let cluster = cluster();
        let (app, reports) = api(&cluster);
        cluster.update(|s| {
            s.forbidden_writes.insert("Deployment/web".to_owned());
        });
        let planned = plan(&app, &scale(2)).await;
        assert_eq!(apply(&app, &planned).await, StatusCode::FORBIDDEN);
        assert_eq!(results(&reports), [Submitted::Forbidden]);
    }

    #[tokio::test]
    async fn a_plan_made_before_someone_else_changed_the_deployment_is_a_conflict() {
        let cluster = cluster();
        let (app, reports) = api(&cluster);
        let planned = plan(&app, &scale(2)).await;
        cluster.update(|state| {
            let web = &mut state.objects.get_mut("shop").unwrap().deployments[0];
            web.metadata.generation = Some(2);
        });
        assert_eq!(apply(&app, &planned).await, StatusCode::CONFLICT);
        assert_eq!(results(&reports), [Submitted::Conflict]);
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_is_read_after_the_save() {
        let cluster = cluster();
        let (app, _) = api(&cluster);
        let planned = plan(&app, &scale(2)).await;
        assert_eq!(apply(&app, &planned).await, StatusCode::OK);
        let reads = || cluster.inspect(|state| state.object_reads).unwrap();
        let after_apply = reads();
        tokio::time::sleep(Duration::from_mins(5)).await;
        assert_eq!(reads(), after_apply);
    }
}
