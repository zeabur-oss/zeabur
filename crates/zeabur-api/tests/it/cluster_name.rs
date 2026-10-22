//! What the interface calls the cluster, in the order the API looks for a name.

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::Settings;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, WriteBody};

    use crate::common::{app_with, send};

    fn cluster() -> MemoryCluster {
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec![],
            },
            nodes: vec![
                serde_json::from_value(json!({"metadata": {"name": "k3d-zeabur-l2-server-0",
                    "labels": {"node.kubernetes.io/instance-type": "k3s",
                               "node-role.kubernetes.io/control-plane": "true"}}}))
                .unwrap(),
            ],
            facts: zeabur_kube::ClusterFacts {
                version: "v1.36.5+k3s1".to_owned(),
                ..zeabur_kube::ClusterFacts::default()
            },
            ..State::default()
        })
    }

    fn in_cluster(name: Option<&str>) -> Settings {
        Settings {
            namespace: Some("zeabur".to_owned()),
            cluster_name: name.map(str::to_owned),
            public_host: Some("zeabur.localhost".to_owned()),
            ..Settings::default()
        }
    }

    async fn call(
        cluster: &MemoryCluster,
        settings: Settings,
        method: &str,
        uri: &str,
        body: Option<&Value>,
    ) -> Value {
        let (status, body) = send(&app_with(cluster, settings), method, uri, body).await;
        assert_eq!(status, StatusCode::OK, "{method} {uri}: {body}");
        body
    }

    async fn named(cluster: &MemoryCluster, settings: Settings) -> (Value, Value, Value) {
        let about = call(cluster, settings, "GET", "/api/v1/cluster", None).await;
        (
            about["name"].clone(),
            about["nameSource"].clone(),
            about.get("nameProvider").cloned().unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn the_nodes_name_the_cluster_when_nothing_else_does_and_never_its_address() {
        let cluster = cluster();
        assert_eq!(
            named(&cluster, in_cluster(None)).await,
            (json!("zeabur-l2"), json!("nodes"), json!("k3d"))
        );
        cluster.update(|s| {
            s.forbidden_resources.insert("nodes".to_owned());
        });
        assert_eq!(
            named(&cluster, in_cluster(None)).await,
            (json!("zeabur.localhost"), json!("address"), Value::Null)
        );
    }

    #[tokio::test]
    async fn the_installs_name_comes_before_the_nodes_and_a_chosen_one_before_both() {
        let cluster = cluster();
        assert_eq!(
            named(&cluster, in_cluster(Some("shop-prod"))).await,
            (json!("shop-prod"), json!("install"), Value::Null)
        );

        let setting = call(
            &cluster,
            in_cluster(Some("shop-prod")),
            "PUT",
            "/api/v1/settings/cluster-name",
            Some(&json!({"name": "  Shop EU  "})),
        )
        .await;
        assert_eq!(setting["name"], "Shop EU");
        assert_eq!(setting["default"], "shop-prod");
        assert_eq!(setting["storedIn"]["name"], "zeabur-settings");
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        assert!(matches!(&writes[..], [w] if matches!(w.body, WriteBody::Create(_))));
        assert_eq!(
            named(&cluster, in_cluster(Some("shop-prod"))).await,
            (json!("Shop EU"), json!("setting"), Value::Null)
        );

        let setting = call(
            &cluster,
            in_cluster(Some("shop-prod")),
            "PUT",
            "/api/v1/settings/cluster-name",
            Some(&json!({"name": null})),
        )
        .await;
        assert!(setting.get("name").is_none());
        assert_eq!(
            named(&cluster, in_cluster(None)).await,
            (json!("zeabur-l2"), json!("nodes"), json!("k3d"))
        );
    }

    #[tokio::test]
    async fn a_name_is_one_short_line() {
        let cluster = cluster();
        for name in ["", "   ", "a\nb", &"x".repeat(64)] {
            let (status, problem) = send(
                &app_with(&cluster, in_cluster(None)),
                "PUT",
                "/api/v1/settings/cluster-name",
                Some(&json!({"name": name})),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{name:?}: {problem}");
        }
        let (status, _) = send(
            &app_with(&cluster, Settings::default()),
            "PUT",
            "/api/v1/settings/cluster-name",
            Some(&json!({"name": "dev"})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn local_mode_keeps_the_kubeconfigs_name_without_the_tools_prefix() {
        let cluster = cluster();
        cluster.update(|s| "k3d-dev".clone_into(&mut s.name));
        assert_eq!(
            named(&cluster, Settings::default()).await,
            (json!("dev"), json!("kubeconfig"), json!("k3d"))
        );
    }

    #[tokio::test]
    async fn the_cluster_says_how_new_public_urls_are_made() {
        let settings = Settings {
            domains: zeabur_api::settings::DomainSettings {
                suffix: Some("apps.example.com".to_owned()),
                cluster_issuer: Some("letsencrypt".to_owned()),
            },
            ..Settings::default()
        };
        let about = call(&cluster(), settings, "GET", "/api/v1/cluster", None).await;
        assert_eq!(
            about["domains"],
            json!({"suffix": "apps.example.com", "clusterIssuer": "letsencrypt"})
        );
    }
}
