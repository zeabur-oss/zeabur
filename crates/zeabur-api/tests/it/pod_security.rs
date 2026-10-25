//! Pod Security through the API: a new project enforces `baseline`, a service shows the level
//! its pods meet, and switching it writes only the service, whatever the project enforces.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::json;
    use zeabur_kube::NamespaceObjects;
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};

    use crate::common::{app, planned_and_applied, send};

    const ENFORCE: &str = "pod-security.kubernetes.io/enforce";

    fn cluster(enforce: &str) -> MemoryCluster {
        let namespace: Namespace = serde_json::from_value(json!({"metadata": {"name": "shop",
            "resourceVersion": "5",
            "labels": {"zeabur.com/project": "true", ENFORCE: enforce}}}))
        .unwrap();
        let web: Deployment = serde_json::from_value(json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "41",
                         "generation": 2},
            "spec": {
                "selector": {"matchLabels": {"app": "web"}},
                "template": {
                    "metadata": {"labels": {"app": "web"}},
                    "spec": {
                        "securityContext": {"runAsNonRoot": true,
                                            "seccompProfile": {"type": "RuntimeDefault"}},
                        "containers": [{"name": "web", "image": "nginx",
                            "securityContext": {"allowPrivilegeEscalation": false,
                                                "capabilities": {"drop": ["ALL"]}}}]
                    }
                }
            }
        }))
        .unwrap();
        MemoryCluster::new(State {
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

    #[tokio::test]
    async fn a_new_project_enforces_baseline_and_says_so() {
        let cluster = MemoryCluster::new(State::default());
        let app = app(&cluster);
        planned_and_applied(&app, "my-app", json!([{"op": "createProject"}])).await;
        let (status, namespace) =
            send(&app, "GET", "/api/v1/projects/my-app/namespace", None).await;
        assert_eq!(status, StatusCode::OK, "{namespace}");
        assert_eq!(namespace["podSecurity"], "baseline", "{namespace}");
    }

    #[tokio::test]
    async fn switching_a_service_writes_only_the_service() {
        let cluster = cluster("restricted");
        let app = app(&cluster);
        let (status, project) = send(&app, "GET", "/api/v1/projects/shop", None).await;
        assert_eq!(status, StatusCode::OK, "{project}");
        assert_eq!(
            project["services"][0]["podSecurity"], "restricted",
            "{project}"
        );

        let web = json!({"kind": "Deployment", "name": "web"});
        planned_and_applied(
            &app,
            "shop",
            json!([{"op": "securityProfile", "workload": web, "profile": "baseline"}]),
        )
        .await;
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        let order: Vec<&str> = writes.iter().map(|w| w.target.kind.as_str()).collect();
        assert_eq!(order, ["Deployment"]);

        let (_, namespace) = send(&app, "GET", "/api/v1/projects/shop/namespace", None).await;
        assert_eq!(namespace["podSecurity"], "restricted", "{namespace}");
        let (_, project) = send(&app, "GET", "/api/v1/projects/shop", None).await;
        assert_eq!(
            project["services"][0]["podSecurity"], "baseline",
            "{project}"
        );
    }
}
