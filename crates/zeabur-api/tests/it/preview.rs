//! A plan's preview: the pods it replaces, whether new ones fit the quota and the nodes, the data
//! and the fields it removes.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::{Value, json};
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, Node, ResourceQuota};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, ServedResource};

    use crate::common::{app, send};

    fn cluster(quota_used: &str) -> MemoryCluster {
        let namespace: Namespace =
            serde_json::from_value(json!({"metadata": {"name": "shop"}})).unwrap();
        let web: Deployment = serde_json::from_value(json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "web", "namespace": "shop", "resourceVersion": "41"},
            "spec": {
                "replicas": 1,
                "selector": {"matchLabels": {"app": "web"}},
                "template": {
                    "metadata": {"labels": {"app": "web"}},
                    "spec": {
                        "containers": [{"name": "app", "image": "example/app:1",
                            "resources": {"requests": {"cpu": "200m", "memory": "128Mi"}},
                            "env": [{"name": "MODE", "value": "production"}],
                            "volumeMounts": [{"name": "cache", "mountPath": "/cache"}]}],
                        "volumes": [{"name": "cache", "emptyDir": {}}]
                    }
                }
            }
        }))
        .unwrap();
        let quota: ResourceQuota = serde_json::from_value(json!({
            "metadata": {"name": "compute", "namespace": "shop"},
            "status": {"hard": {"requests.cpu": "1"}, "used": {"requests.cpu": quota_used}}
        }))
        .unwrap();
        let node: Node = serde_json::from_value(json!({
            "metadata": {"name": "node-1"},
            "status": {"allocatable": {"cpu": "4", "memory": "8Gi", "pods": "110"},
                "conditions": [{"type": "Ready", "status": "True"}]}
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
            resource_quotas: BTreeMap::from([("shop".to_owned(), vec![quota])]),
            nodes: vec![node],
            ..State::default()
        })
    }

    async fn preview(cluster: &MemoryCluster, changes: Value) -> Value {
        let app = app(cluster);
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({ "changes": changes })),
        )
        .await;
        assert_eq!(status.as_u16(), 200, "{body}");
        body["preview"].clone()
    }

    fn workload() -> Value {
        json!({"kind": "Deployment", "name": "web"})
    }

    #[tokio::test]
    async fn a_scale_past_the_quota_is_blocked_by_name() {
        let preview = preview(
            &cluster("700m"),
            json!([{"op": "scale", "workload": workload(), "replicas": 3}]),
        )
        .await;
        assert_eq!(preview["rollouts"][0]["actions"], json!(["scale"]));
        assert_eq!(preview["rollouts"][0]["fromReplicas"], 1);
        assert_eq!(preview["fit"]["status"], "blocked", "{preview}");
        assert_eq!(preview["fit"]["pods"], 2);
        assert_eq!(
            preview["fit"]["blockers"],
            json!([{"kind": "quota", "name": "compute", "resource": "requests.cpu",
                "need": 400, "available": 300}])
        );
        assert_eq!(preview["dataLoss"], json!([]));
    }

    #[tokio::test]
    async fn a_roll_names_its_fields_and_the_empty_dir_it_loses() {
        let preview = preview(
            &cluster("0"),
            json!([{"op": "env", "workload": workload(),
                "changes": [{"op": "set", "name": "MODE", "value": "maintenance"}]}]),
        )
        .await;
        let rollout = &preview["rollouts"][0];
        assert_eq!(rollout["actions"], json!(["roll"]));
        assert_eq!(rollout["strategy"], "RollingUpdate");
        assert_eq!(rollout["downtime"], false);
        assert_eq!(
            rollout["fields"],
            json!(["spec.template.spec.containers[name=app].env[name=MODE].value"])
        );
        assert_eq!(preview["fit"]["status"], "fits", "{preview}");
        assert_eq!(
            preview["dataLoss"],
            json!([{"kind": "emptyDir", "workload": workload(), "volume": "cache",
                "mountPaths": ["/cache"]}])
        );
        assert_eq!(preview["removals"], json!([]));
    }

    #[tokio::test]
    async fn a_field_left_out_of_an_edit_is_listed_as_removed() {
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata: {name: web, namespace: shop}\nspec:\n  replicas: 1\n  strategy: {type: Recreate}\n  selector: {matchLabels: {app: web}}\n  template:\n    metadata: {labels: {app: web}}\n    spec:\n      containers:\n      - name: app\n        image: example/app:1\n        resources: {requests: {cpu: 200m, memory: 128Mi}}\n        volumeMounts: [{name: cache, mountPath: /cache}]\n      volumes: [{name: cache, emptyDir: {}}]\n";
        let preview = preview(
            &cluster("0"),
            json!([{"op": "edit",
                "resource": {"group": "apps", "version": "v1", "resource": "deployments"},
                "name": "web", "yaml": yaml}]),
        )
        .await;
        assert_eq!(preview["rollouts"][0]["strategy"], "Recreate");
        assert_eq!(preview["rollouts"][0]["downtime"], true);
        let removals = preview["removals"].as_array().unwrap();
        assert!(
            removals
                .iter()
                .any(|r| r["path"].as_str().unwrap().contains("env") && r["leftOutOfEdit"] == true),
            "{preview}"
        );
    }

    #[tokio::test]
    async fn maintenance_on_a_serving_domain_does_not_list_it_as_starting() {
        let cluster = cluster("0");
        cluster.update(|s| {
            s.served.push(ServedResource::readable(
                "traefik.io",
                "v1alpha1",
                "middlewares",
                "Middleware",
                true,
            ));
            let objects = s.objects.get_mut("shop").unwrap();
            objects.services.push(
                serde_json::from_value(json!({
                    "metadata": {"name": "error-pages", "namespace": "shop",
                        "labels": {"zeabur.com/page-server": "true"}},
                    "spec": {"ports": [{"name": "pages", "port": 8080},
                        {"name": "maintenance", "port": 8081}]}
                }))
                .unwrap(),
            );
            objects.ingresses.push(
                serde_json::from_value(json!({
                    "metadata": {"name": "web", "namespace": "shop", "uid": "u-1"},
                    "spec": {"ingressClassName": "traefik", "rules": [{"host": "shop.example.com",
                        "http": {"paths": [{"path": "/", "pathType": "Prefix",
                            "backend": {"service": {"name": "web", "port": {"number": 80}}}}]}}]}
                }))
                .unwrap(),
            );
        });
        let preview = preview(
            &cluster,
            json!([{"op": "maintenance", "route": {"kind": "Ingress", "name": "web"},
                "hostname": "shop.example.com", "enabled": true}]),
        )
        .await;
        assert_eq!(preview["domains"], json!([]), "{preview}");
    }
}
