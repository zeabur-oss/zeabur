//! A successful apply hands what it used to the experience program once the writes are done, and
//! a failed one hands nothing. What each change counts as is in `used.rs`.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::settings::{DomainSettings, Settings};
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Service};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects};
    use zeabur_telemetry::{Feature, Source};

    use crate::common::send;
    use crate::support::{Seen, TestApi, recorder, taken};

    fn cluster() -> MemoryCluster {
        let namespace: Namespace = serde_json::from_value(json!({"metadata": {"name": "shop",
            "labels": {"zeabur.com/project": "true"}}}))
        .unwrap();
        let web: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "7"},
            "spec": {"selector": {"matchLabels": {"app": "web"}},
                     "template": {"metadata": {"labels": {"app": "web"}},
                                  "spec": {"containers": [{"name": "web", "image": "web:1"}]}}}
        }))
        .unwrap();
        let service: Service = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop"},
            "spec": {"selector": {"app": "web"}, "ports": [{"port": 8080}]}
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
                    services: vec![service],
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        })
    }

    fn settings() -> Settings {
        Settings {
            domains: DomainSettings {
                suffixes: vec!["apps.example.com".to_owned()],
                cluster_issuer: Some("letsencrypt".to_owned()),
            },
            ..Settings::default()
        }
    }

    fn api(cluster: &MemoryCluster) -> (Router, Seen) {
        let (used, seen) = recorder();
        let app = TestApi::new(cluster)
            .with_settings(settings())
            .with_used(used)
            .router();
        (app, seen)
    }

    async fn plan(app: &Router, changes: &Value) -> Value {
        let (status, planned) = send(
            app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({"changes": changes})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        planned
    }

    async fn apply(app: &Router, planned: &Value) -> StatusCode {
        send(
            app,
            "POST",
            "/api/v1/projects/shop/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await
        .0
    }

    async fn used(changes: Value) -> Vec<Feature> {
        let (app, seen) = api(&cluster());
        let planned = plan(&app, &changes).await;
        assert_eq!(apply(&app, &planned).await, StatusCode::OK);
        taken(&seen)
    }

    fn deployment() -> Value {
        json!({"op": "create", "object": {"apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "api"},
            "spec": {"selector": {"matchLabels": {"app": "api"}},
                     "template": {"metadata": {"labels": {"app": "api"}},
                                  "spec": {"containers": [{"name": "api", "image": "api:1"}]}}}}})
    }

    #[tokio::test]
    async fn a_successful_apply_is_sent_once() {
        assert_eq!(
            used(json!([deployment()])).await,
            [Feature::ServiceCreated(Source::Image)]
        );
    }

    #[tokio::test]
    async fn nothing_is_sent_when_the_apply_fails() {
        let cluster = cluster();
        let (app, seen) = api(&cluster);
        let planned = plan(&app, &json!([deployment()])).await;
        cluster.update(|s| {
            s.forbidden_writes.insert("Deployment/api".to_owned());
        });
        assert_ne!(apply(&app, &planned).await, StatusCode::OK);
        assert_eq!(taken(&seen), Vec::<Feature>::new());
    }

    #[tokio::test]
    async fn without_the_program_an_apply_reads_nothing_extra() {
        let reads = |used: Option<zeabur_api::used::Used>| async move {
            let cluster = cluster();
            let settings = Settings {
                namespace: Some("zeabur".to_owned()),
                ..settings()
            };
            let mut api = TestApi::new(&cluster).with_settings(settings);
            if let Some(used) = used {
                api = api.with_used(used);
            }
            let app = api.router();
            let add = json!([{"op": "addDomain",
                "workload": {"kind": "Deployment", "name": "web"},
                "hostname": "web.apps.example.com"}]);
            let planned = plan(&app, &add).await;
            let before = cluster.inspect(|s| s.object_reads).unwrap();
            assert_eq!(apply(&app, &planned).await, StatusCode::OK);
            cluster.inspect(|s| s.object_reads).unwrap() - before
        };
        let used: zeabur_api::used::Used = Arc::new(|_| {});
        assert!(reads(Some(used)).await > reads(None).await);
    }
}
