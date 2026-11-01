//! Zeabur's own version, and how it was installed, for the interface's update check.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::Settings;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{NamespaceObjects, ServedResource};

    use crate::common::{app_with, send};

    fn in_cluster() -> Settings {
        Settings {
            namespace: Some("zeabur".to_owned()),
            deployment: Some("zeabur".to_owned()),
            update_check: true,
            ..Settings::default()
        }
    }

    fn deployed(
        labels: &[(&str, &str)],
        annotations: &[(&str, &str)],
        helm_chart: Option<&str>,
    ) -> MemoryCluster {
        let map = |pairs: &[(&str, &str)]| {
            Some(
                pairs
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                    .collect::<BTreeMap<_, _>>(),
            )
        };
        let deployment = Deployment {
            metadata: ObjectMeta {
                name: Some("zeabur".to_owned()),
                namespace: Some("zeabur".to_owned()),
                labels: map(labels),
                annotations: map(annotations),
                ..ObjectMeta::default()
            },
            ..Deployment::default()
        };
        let mut state = State {
            objects: [(
                "zeabur".to_owned(),
                NamespaceObjects {
                    namespace: "zeabur".to_owned(),
                    deployments: vec![deployment],
                    ..NamespaceObjects::default()
                },
            )]
            .into(),
            ..State::default()
        };
        state.served.push(ServedResource::readable(
            "helm.cattle.io",
            "v1",
            "helmcharts",
            "HelmChart",
            true,
        ));
        if let Some(manager) = helm_chart {
            state.dynamic.push(
                serde_json::from_value(json!({
                    "apiVersion": "helm.cattle.io/v1", "kind": "HelmChart",
                    "metadata": {"name": "zeabur", "namespace": "kube-system",
                        "labels": {"app.kubernetes.io/managed-by": manager}},
                }))
                .unwrap(),
            );
        }
        MemoryCluster::new(state)
    }

    async fn about(cluster: &MemoryCluster, settings: Settings) -> Value {
        let (status, body) = send(
            &app_with(cluster, settings),
            "GET",
            "/api/v1/cluster/zeabur",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    const HELM: (&str, &str) = ("meta.helm.sh/release-name", "zeabur");

    #[tokio::test]
    async fn the_version_and_the_switch_come_from_the_server() {
        let answer = about(&deployed(&[], &[], None), in_cluster()).await;
        assert_eq!(answer["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(answer["updateCheck"], true);
        let off = Settings {
            update_check: false,
            ..in_cluster()
        };
        assert_eq!(
            about(&deployed(&[], &[], None), off).await["updateCheck"],
            false
        );
    }

    #[tokio::test]
    async fn the_install_method_follows_the_deployments_manager() {
        let cases = [
            (
                deployed(&[], &[HELM], Some("zeabur-installer")),
                json!("installer"),
            ),
            (deployed(&[], &[HELM], Some("someone-else")), json!("helm")),
            (deployed(&[], &[HELM], None), json!("helm")),
            (
                deployed(&[("app.kubernetes.io/managed-by", "Helm")], &[], None),
                json!("helm"),
            ),
            (
                deployed(&[], &[HELM, ("argocd.argoproj.io/tracking-id", "x")], None),
                json!("argo-cd"),
            ),
            (
                deployed(&[("helm.toolkit.fluxcd.io/name", "zeabur")], &[HELM], None),
                json!("flux"),
            ),
            (deployed(&[], &[], None), Value::Null),
        ];
        for (cluster, expected) in cases {
            assert_eq!(about(&cluster, in_cluster()).await["installedBy"], expected);
        }
    }

    #[tokio::test]
    async fn on_this_machine_it_is_zeaburctl_and_refused_reads_say_nothing() {
        let cluster = deployed(&[], &[HELM], Some("zeabur-installer"));
        assert_eq!(
            about(&cluster, Settings::default()).await["installedBy"],
            "zeaburctl"
        );
        cluster.update(|s| {
            s.forbidden_namespaces.insert("zeabur".to_owned());
        });
        assert_eq!(
            about(&cluster, in_cluster()).await["installedBy"],
            Value::Null
        );
        let unnamed = Settings {
            deployment: None,
            ..in_cluster()
        };
        let cluster = deployed(&[], &[HELM], Some("zeabur-installer"));
        assert_eq!(about(&cluster, unnamed).await["installedBy"], Value::Null);
    }
}
