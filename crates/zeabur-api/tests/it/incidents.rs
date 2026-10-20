//! Cluster incidents against an in-memory cluster: a component with no ready replica and a node
//! that is not ready, each told once with the services, domains and projects it reaches.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Node, Pod, Service};
    use zeabur_kube::k8s_openapi::api::networking::v1::Ingress;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, NamespaceObjects};

    use crate::common::{app, send};

    fn from<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    fn shop() -> NamespaceObjects {
        NamespaceObjects {
            namespace: "shop".to_owned(),
            deployments: vec![from(json!({
                "metadata": {"name": "web", "namespace": "shop"},
                "spec": {"selector": {"matchLabels": {"app": "web"}},
                    "template": {"metadata": {"labels": {"app": "web"}},
                        "spec": {"containers": [{"name": "web", "image": "nginx:1.29",
                            "ports": [{"containerPort": 80}]}]}}}
            }))],
            pods: vec![from::<Pod>(json!({
                "metadata": {"name": "web-1", "namespace": "shop", "labels": {"app": "web"}},
                "spec": {"nodeName": "node-b", "containers": [{"name": "web", "image": "nginx:1.29"}]},
                "status": {"phase": "Running"}
            }))],
            services: vec![from::<Service>(json!({
                "metadata": {"name": "web", "namespace": "shop"},
                "spec": {"selector": {"app": "web"}, "ports": [{"port": 80, "targetPort": 80}]}
            }))],
            ingresses: vec![from::<Ingress>(json!({
                "metadata": {"name": "web", "namespace": "shop",
                    "annotations": {"cert-manager.io/cluster-issuer": "letsencrypt"}},
                "spec": {"ingressClassName": "traefik",
                    "tls": [{"hosts": ["shop.example.test"], "secretName": "web-tls"}],
                    "rules": [{"host": "shop.example.test", "http": {"paths": [{"path": "/",
                        "pathType": "Prefix",
                        "backend": {"service": {"name": "web", "port": {"number": 80}}}}]}}]}
            }))],
            ..NamespaceObjects::default()
        }
    }

    fn system() -> NamespaceObjects {
        let deployment = |name: &str, labels: Value, ready: i32| -> Deployment {
            from(json!({
                "metadata": {"name": name, "namespace": "kube-system", "labels": labels},
                "spec": {"replicas": 1, "selector": {}, "template": {}},
                "status": {"replicas": 1, "readyReplicas": ready,
                    "conditions": [{"type": "Available", "status": if ready == 0 { "False" } else { "True" },
                        "lastTransitionTime": "2026-10-20T08:00:00Z"}]}
            }))
        };
        NamespaceObjects {
            namespace: "kube-system".to_owned(),
            deployments: vec![
                deployment("traefik", json!({"app.kubernetes.io/name": "traefik"}), 0),
                deployment("coredns", json!({"k8s-app": "kube-dns"}), 1),
            ],
            ..NamespaceObjects::default()
        }
    }

    fn node(name: &str, ready: &str) -> Node {
        from(json!({
            "metadata": {"name": name},
            "status": {"conditions": [{"type": "Ready", "status": ready,
                "reason": if ready == "True" { "KubeletReady" } else { "NodeStatusUnknown" },
                "lastTransitionTime": "2026-10-20T09:00:00Z"}]}
        }))
    }

    fn cluster() -> MemoryCluster {
        let class: DynamicObject = from(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "IngressClass",
            "metadata": {"name": "traefik"},
            "spec": {"controller": "traefik.io/ingress-controller"}
        }));
        MemoryCluster::new(State {
            namespaces: vec![
                from::<Namespace>(json!({"metadata": {"name": "shop",
                    "labels": {"zeabur.com/project": "true"}}})),
                from::<Namespace>(json!({"metadata": {"name": "kube-system"}})),
            ],
            objects: BTreeMap::from([
                ("shop".to_owned(), shop()),
                ("kube-system".to_owned(), system()),
            ]),
            nodes: vec![node("node-a", "True"), node("node-b", "Unknown")],
            dynamic: vec![class],
            ..State::default()
        })
    }

    #[tokio::test]
    async fn a_down_ingress_controller_and_a_lost_node_are_each_told_once() {
        let cluster = cluster();
        let (status, list) = send(
            &app(&cluster),
            "GET",
            "/api/v1/projects/shop/incidents",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{list}");
        let ids: Vec<&str> = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            [
                "incidents/component/ingress-controller",
                "incidents/node/node-b"
            ]
        );

        let controller = &list["items"][0];
        assert_eq!(controller["kind"], "componentDown");
        assert_eq!(
            controller["fault"],
            json!({"origin": "component", "component": "ingress-controller", "fixer": "componentOwner"})
        );
        assert_eq!(controller["domains"], json!(["shop.example.test"]));
        assert_eq!(controller["projects"], json!(["shop"]));
        assert_eq!(
            controller["services"][0]["id"],
            "projects/shop/services/deployment/web"
        );
        assert_eq!(controller["since"], "2026-10-20T08:00:00Z");
        assert_eq!(
            controller["cause"]["key"],
            "incident.componentDown.cause.ingressController"
        );
        assert_eq!(controller["evidence"][0]["kind"], "Deployment");
        assert_eq!(controller["evidence"][0]["namespace"], "kube-system");

        let node = &list["items"][1];
        assert_eq!(
            node["fault"],
            json!({"origin": "cluster", "component": "node/node-b", "fixer": "clusterAdmin"})
        );
        assert_eq!(node["services"][0]["name"], "web");
        assert_eq!(node["cause"]["params"]["status"], "Unknown");
    }

    #[tokio::test]
    async fn the_cluster_answer_names_domains_and_projects_but_no_services() {
        let cluster = cluster();
        let (status, list) = send(&app(&cluster), "GET", "/api/v1/cluster/incidents", None).await;
        assert_eq!(status, StatusCode::OK, "{list}");
        assert_eq!(list["items"].as_array().unwrap().len(), 2);
        assert_eq!(list["items"][0]["domains"], json!(["shop.example.test"]));
        assert_eq!(list["items"][0]["services"], json!([]));
        assert_eq!(list["items"][1]["projects"], json!(["shop"]));
    }

    #[tokio::test]
    async fn a_refused_list_is_named_and_hides_only_what_it_would_have_shown() {
        let cluster = cluster();
        cluster.update(|s| {
            s.forbidden_resources.insert("nodes".to_owned());
        });
        let (status, list) = send(
            &app(&cluster),
            "GET",
            "/api/v1/projects/shop/incidents",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{list}");
        assert_eq!(list["items"].as_array().unwrap().len(), 1);
        assert_eq!(list["unreadable"][0]["resource"], "nodes");
    }

    #[tokio::test]
    async fn a_healthy_cluster_has_no_incident() {
        let cluster = cluster();
        cluster.update(|s| {
            s.nodes = vec![node("node-a", "True"), node("node-b", "True")];
            if let Some(system) = s.objects.get_mut("kube-system") {
                for deployment in &mut system.deployments {
                    if let Some(status) = &mut deployment.status {
                        status.ready_replicas = Some(1);
                    }
                }
            }
        });
        let (status, list) = send(&app(&cluster), "GET", "/api/v1/cluster/incidents", None).await;
        assert_eq!(status, StatusCode::OK, "{list}");
        assert_eq!(list["items"], json!([]));
        assert_eq!(list["unreadable"], json!([]));
    }
}
