//! The features a successful apply used reach the experience program once the writes are done,
//! as enum values and catalog template IDs only, and not when the apply fails.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::settings::{DomainSettings, Settings};
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Service};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, ServedResource};
    use zeabur_telemetry::{DatabaseKind, DomainKind, Feature, Source};

    use crate::common::send;
    use crate::support::TestApi;

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
        let cluster = MemoryCluster::new(State {
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
        });
        cluster.update(|s| {
            for (group, version, resource, kind) in [
                ("postgresql.cnpg.io", "v1", "clusters", "Cluster"),
                ("builder.zeabur.com", "v1alpha1", "builds", "Build"),
            ] {
                s.served.push(ServedResource {
                    verbs: ["create", "get", "list"].map(str::to_owned).to_vec(),
                    ..ServedResource::readable(group, version, resource, kind, true)
                });
            }
        });
        cluster
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

    type Seen = Arc<Mutex<Vec<Feature>>>;

    fn api(cluster: &MemoryCluster) -> (Router, Seen) {
        let seen = Seen::default();
        let log = Arc::clone(&seen);
        let app = TestApi::new(cluster)
            .with_settings(settings())
            .with_used(Arc::new(move |feature| log.lock().unwrap().push(feature)))
            .router();
        (app, seen)
    }

    fn taken(seen: &Seen) -> Vec<Feature> {
        seen.lock().unwrap().clone()
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

    fn build(source: &Value) -> Value {
        json!({"op": "create", "object": {"apiVersion": "builder.zeabur.com/v1alpha1",
            "kind": "Build", "metadata": {"name": "site"},
            "spec": {"builderRef": "dockerfile", "source": source,
                     "output": {"image": "zot.zot.svc:5000/shop/site"}}}})
    }

    #[tokio::test]
    async fn an_image_service_is_sent_once_after_the_apply() {
        assert_eq!(
            used(json!([deployment()])).await,
            [Feature::ServiceCreated(Source::Image)]
        );
    }

    #[tokio::test]
    async fn a_service_built_from_git_is_not_also_an_image_service() {
        let git = json!({"git": {"url": "https://github.com/acme/site.git", "ref": "main"}});
        assert_eq!(
            used(json!([build(&git), deployment()])).await,
            [Feature::ServiceCreated(Source::Git)]
        );
    }

    #[tokio::test]
    async fn a_template_is_sent_by_its_catalog_id_and_its_domain_by_where_the_name_comes_from() {
        let features = used(json!([
            {"op": "create", "object": {"apiVersion": "v1", "kind": "Secret",
                "metadata": {"name": "miniflux-secrets"}, "type": "Opaque",
                "stringData": {"admin-password": "a".repeat(43)}}},
            {"op": "template", "template": "miniflux", "domain": "rss.example.com"},
        ]))
        .await;
        assert_eq!(
            features,
            [
                Feature::ServiceCreated(Source::Template("miniflux")),
                Feature::DomainAdded(DomainKind::Custom),
            ]
        );
    }

    #[tokio::test]
    async fn a_database_is_sent_by_its_kind() {
        let features = used(json!([{"op": "create", "object": {
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
            "metadata": {"name": "db"}, "spec": {"instances": 1}}}]))
        .await;
        assert_eq!(
            features,
            [Feature::DatabaseCreated(DatabaseKind::Postgresql)]
        );
    }

    #[tokio::test]
    async fn a_domain_under_an_offered_suffix_is_generated_and_any_other_is_custom() {
        let add = |hostname: &str| {
            json!({"op": "addDomain", "workload": {"kind": "Deployment", "name": "web"},
                   "hostname": hostname})
        };
        assert_eq!(
            used(json!([add("web-shop.apps.example.com")])).await,
            [Feature::DomainAdded(DomainKind::Generated)]
        );
        assert_eq!(
            used(json!([add("notapps.example.com")])).await,
            [Feature::DomainAdded(DomainKind::Custom)]
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
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_write_that_creates_no_service_sends_nothing() {
        let (app, seen) = api(&cluster());
        let planned = plan(
            &app,
            &json!([{"op": "scale", "workload": {"kind": "Deployment", "name": "web"},
                     "replicas": 2}]),
        )
        .await;
        assert_eq!(apply(&app, &planned).await, StatusCode::OK);
        assert!(seen.lock().unwrap().is_empty());
    }
}
