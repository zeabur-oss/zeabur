//! A database whose data is on the pod alone is a gap of the project, with the volume to add.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{from_value, json};
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects};

    use crate::common::{app, send};

    #[tokio::test]
    async fn a_database_without_a_volume_is_a_gap_with_its_data_directory() {
        let namespace: Namespace = from_value(json!({"metadata": {"name": "shop"}})).unwrap();
        let db = json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "db", "namespace": "shop"},
            "spec": {"replicas": 1, "selector": {"matchLabels": {"app": "db"}},
                "template": {"metadata": {"labels": {"app": "db"}},
                    "spec": {"containers": [{"name": "mysql", "image": "mysql:8.4"}]}}}
        });
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
                    deployments: vec![from_value(db).unwrap()],
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        });
        let (status, body) = send(&app(&cluster), "GET", "/api/v1/projects/shop/gaps", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let gap = body["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["rule"] == "ephemeral-data")
            .unwrap_or_else(|| panic!("no ephemeral-data gap in {body}"));
        assert_eq!(gap["severity"], "warning");
        assert_eq!(gap["message"]["key"], "gap.ephemeralData.none");
        assert_eq!(
            gap["fix"],
            json!({"kind": "plan", "id": "storage.addVolume", "params": {
                "service": "projects/shop/services/deployment/db", "container": "mysql",
                "mountPath": "/var/lib/mysql"}})
        );
    }
}
