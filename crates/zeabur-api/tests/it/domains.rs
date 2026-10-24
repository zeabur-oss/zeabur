//! The domains new URLs are suggested under: the install's, a list chosen on the cluster page,
//! and whether a wildcard record sends each to the cluster.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::Settings;
    use zeabur_api::access::SingleCluster;
    use zeabur_api::resolve::Fixed;
    use zeabur_api::settings::DomainSettings;
    use zeabur_kube::k8s_openapi::api::networking::v1::Ingress;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects};

    use crate::common::send;

    fn cluster() -> MemoryCluster {
        let ingress: Ingress = serde_json::from_value(json!({
            "metadata": {"name": "zeabur", "namespace": "zeabur"},
            "spec": {"rules": [{"host": "zeabur.example.com"}]},
            "status": {"loadBalancer": {"ingress": [{"ip": "203.0.113.7"}]}}
        }))
        .unwrap();
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec![],
            },
            objects: BTreeMap::from([(
                "zeabur".to_owned(),
                NamespaceObjects {
                    namespace: "zeabur".to_owned(),
                    ingresses: vec![ingress],
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        })
    }

    fn app(cluster: &MemoryCluster) -> Router {
        let settings = Settings {
            namespace: Some("zeabur".to_owned()),
            domains: DomainSettings {
                suffixes: vec!["apps.example.com".to_owned()],
                cluster_issuer: Some("letsencrypt".to_owned()),
            },
            ..Settings::default()
        };
        let answers = BTreeMap::from([
            (
                "*.apps.example.com".to_owned(),
                vec!["203.0.113.7".to_owned()],
            ),
            (
                "*.shop.example.com".to_owned(),
                vec!["198.51.100.1".to_owned()],
            ),
            (
                "*.198-51-100-9.sslip.io".to_owned(),
                vec!["198.51.100.9".to_owned()],
            ),
        ]);
        zeabur_api::router_with_settings_and_resolver(
            Arc::new(SingleCluster(Arc::new(cluster.clone()))),
            settings,
            Arc::new(Fixed(answers)),
        )
    }

    async fn call(cluster: &MemoryCluster, method: &str, uri: &str, body: Option<&Value>) -> Value {
        let (status, body) = send(&app(cluster), method, uri, body).await;
        assert_eq!(status, StatusCode::OK, "{method} {uri}: {body}");
        body
    }

    fn states(setting: &Value) -> Vec<(String, String)> {
        setting["suffixes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                (
                    s["name"].as_str().unwrap().to_owned(),
                    s["dns"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn each_suffix_says_whether_its_wildcard_points_at_the_ingress() {
        let cluster = cluster();
        let setting = call(&cluster, "GET", "/api/v1/settings/domains", None).await;
        assert_eq!(setting["source"], "default");
        assert_eq!(setting["target"], json!(["203.0.113.7"]));
        assert_eq!(
            states(&setting),
            [("apps.example.com".to_owned(), "pointsHere".to_owned())]
        );
        let probe = setting["suffixes"][0]["probe"].as_str().unwrap();
        assert!(probe.starts_with("zeabur-check-") && probe.ends_with(".apps.example.com"));

        let setting = call(
            &cluster,
            "PUT",
            "/api/v1/settings/domains",
            Some(&json!({"suffixes": ["*.Shop.example.com", "new.example.com", "198-51-100-9.sslip.io"]})),
        )
        .await;
        assert_eq!(setting["source"], "setting");
        assert_eq!(
            states(&setting),
            [
                ("shop.example.com".to_owned(), "elsewhere".to_owned()),
                ("new.example.com".to_owned(), "missing".to_owned()),
                ("198-51-100-9.sslip.io".to_owned(), "elsewhere".to_owned()),
            ]
        );
        assert_eq!(setting["suffixes"][2]["sslip"], "198.51.100.9");
        let about = call(&cluster, "GET", "/api/v1/cluster", None).await;
        assert_eq!(about["domains"]["suffixes"][0], "shop.example.com");

        let setting = call(
            &cluster,
            "PUT",
            "/api/v1/settings/domains",
            Some(&json!({"suffixes": null})),
        )
        .await;
        assert_eq!(setting["source"], "default");
        let (status, _) = send(
            &app(&cluster),
            "PUT",
            "/api/v1/settings/domains",
            Some(&json!({"suffixes": ["localhost"]})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
