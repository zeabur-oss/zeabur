//! Projection behaviours the language-neutral fixtures do not cover.

#[cfg(test)]
mod tests {
    use serde_json::{from_value, json};
    use zeabur_graph::{
        Snapshot, attribution, env_from, project, relations, with_http_routes, workloads,
    };
    use zeabur_model::{DataStore, Ownership, ReadOnlyReason, RelationKind, Tool};

    fn deployment(managed_fields: &serde_json::Value) -> serde_json::Value {
        json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": {"name": "web", "namespace": "demo", "managedFields": managed_fields},
            "spec": {
                "selector": {"matchLabels": {"app": "web"}},
                "template": {
                    "metadata": {"labels": {"app": "web"}},
                    "spec": {"containers": [{
                        "name": "app",
                        "envFrom": [{"configMapRef": {"name": "settings"}}]
                    }]}
                }
            }
        })
    }

    fn owner_of_env_from(manager: &str) -> serde_json::Value {
        json!([{
            "manager": manager,
            "operation": "Apply",
            "fieldsType": "FieldsV1",
            "fieldsV1": {"f:spec": {"f:template": {"f:spec": {"f:containers": {
                "k:{\"name\":\"app\"}": {".": {}, "f:envFrom": {}}
            }}}}}
        }])
    }

    fn snapshot(deployment: serde_json::Value, config_map_labels: &serde_json::Value) -> Snapshot {
        Snapshot {
            namespace: "demo".to_owned(),
            deployments: vec![from_value(deployment).unwrap()],
            config_maps: vec![
                from_value(json!({
                    "apiVersion": "v1",
                    "kind": "ConfigMap",
                    "metadata": {"name": "settings", "namespace": "demo", "labels": config_map_labels},
                    "data": {"A": "1"}
                }))
                .unwrap(),
            ],
            ..Snapshot::default()
        }
    }

    #[test]
    fn an_http_route_in_maintenance_or_redirecting_still_names_its_service() {
        let mut snapshot = snapshot(deployment(&json!([])), &json!({}));
        snapshot.services = vec![
            from_value(json!({
                "apiVersion": "v1",
                "kind": "Service",
                "metadata": {"name": "web", "namespace": "demo"},
                "spec": {"selector": {"app": "web"}, "ports": [{"port": 8080}]}
            }))
            .unwrap(),
        ];
        let page = json!([{"name": "zeabur-error-pages", "port": 8081}]);
        let routes = [
            json!({
                "metadata": {"name": "down", "namespace": "demo", "annotations": {
                    "zeabur.com/maintenance-backend":
                        r#"{"down.example.test":[[{"name":"web","port":8080}]]}"#
                }},
                "spec": {"hostnames": ["down.example.test"], "rules": [{"backendRefs": page}]}
            }),
            json!({
                "metadata": {"name": "moved", "namespace": "demo", "annotations": {
                    "zeabur.com/redirect-rules": r#"[{"backendRefs":[{"name":"web","port":8080}]}]"#
                }},
                "spec": {"hostnames": ["moved.example.test"], "rules": [{"filters": [{
                    "type": "RequestRedirect",
                    "requestRedirect": {"hostname": "example.test", "statusCode": 301}
                }]}]}
            }),
        ];
        let mut project = project(&snapshot);
        with_http_routes(&mut project, &routes);
        assert_eq!(project.services[0].route_names, ["down", "moved"]);
        let down = project.route("down").unwrap();
        assert_eq!(down.origin.kind, "HTTPRoute");
        assert_eq!(down.maintenance, ["down.example.test"]);
        assert_eq!(down.rules[0].service, "web");
        let moved = project.route("moved").unwrap();
        assert_eq!(moved.rules[0].service, "web");
        assert_eq!(moved.redirects[0].to.as_deref(), Some("example.test"));
        assert_eq!(moved.redirects[0].permanent, Some(true));
    }

    #[test]
    fn an_import_list_another_manager_owns_is_read_only() {
        let snapshot = snapshot(deployment(&owner_of_env_from("helm")), &json!({}));
        let imports = env_from(&snapshot, workloads(&snapshot)[0], "app").unwrap();
        assert!(imports[0].read_only);
        assert_eq!(
            imports[0].read_only_reason,
            Some(ReadOnlyReason::ListManagedBy {
                manager: "helm".to_owned()
            })
        );
    }

    #[test]
    fn an_import_list_zeabur_owns_is_editable() {
        let snapshot = snapshot(deployment(&owner_of_env_from("zeabur")), &json!({}));
        let imports = env_from(&snapshot, workloads(&snapshot)[0], "app").unwrap();
        assert!(!imports[0].read_only);
        assert_eq!(imports[0].key_count, Some(1));
    }

    #[test]
    fn an_import_of_a_gitops_managed_object_is_read_only() {
        let snapshot = snapshot(
            deployment(&json!([])),
            &json!({"kustomize.toolkit.fluxcd.io/name": "apps"}),
        );
        let imports = env_from(&snapshot, workloads(&snapshot)[0], "app").unwrap();
        assert_eq!(
            imports[0].read_only_reason,
            Some(ReadOnlyReason::SourceManagedBy { tool: Tool::Flux })
        );
    }

    #[test]
    fn an_ingress_belongs_to_the_workloads_its_services_select() {
        let mut snapshot = snapshot(deployment(&json!([])), &json!({}));
        snapshot.services.push(
            from_value(json!({
                "apiVersion": "v1",
                "kind": "Service",
                "metadata": {"name": "web", "namespace": "demo"},
                "spec": {"selector": {"app": "web"}, "ports": [{"port": 80}]}
            }))
            .unwrap(),
        );
        snapshot.ingresses.push(
            from_value(json!({
                "apiVersion": "networking.k8s.io/v1",
                "kind": "Ingress",
                "metadata": {"name": "public", "namespace": "demo"},
                "spec": {"rules": [{"host": "example.com", "http": {"paths": [{
                    "path": "/",
                    "pathType": "Prefix",
                    "backend": {"service": {"name": "web", "port": {"number": 80}}}
                }]}}]}
            }))
            .unwrap(),
        );
        let attribution = attribution(&snapshot);
        assert_eq!(attribution.ingresses["public"].ownership, Ownership::Owned);
        let route = relations(&snapshot)
            .into_iter()
            .find(|relation| relation.kind == RelationKind::Route)
            .unwrap();
        assert_eq!(
            route.origin.field_path.as_deref(),
            Some("spec.rules[0].http.paths[0].backend.service")
        );
    }

    #[test]
    fn a_service_says_what_kind_of_data_store_it_is() {
        let workload = |kind: &str, name: &str, images: &[&str]| {
            let containers: Vec<_> = images
                .iter()
                .enumerate()
                .map(|(index, image)| json!({"name": format!("c{index}"), "image": image}))
                .collect();
            json!({
                "apiVersion": "apps/v1",
                "kind": kind,
                "metadata": {"name": name, "namespace": "demo"},
                "spec": {
                    "selector": {"matchLabels": {"app": name}},
                    "template": {
                        "metadata": {"labels": {"app": name}},
                        "spec": {"containers": containers}
                    }
                }
            })
        };
        let snapshot = Snapshot {
            namespace: "demo".to_owned(),
            deployments: vec![
                from_value(workload("Deployment", "web", &["nginx:1.27"])).unwrap(),
                // A sidecar's image counts, not only the first container's.
                from_value(workload(
                    "Deployment",
                    "cache",
                    &["ghcr.io/acme/exporter:1", "valkey/valkey:8"],
                ))
                .unwrap(),
                from_value(workload(
                    "Deployment",
                    "ch",
                    &["clickhouse/clickhouse-server:25.8"],
                ))
                .unwrap(),
            ],
            stateful_sets: vec![
                from_value(workload("StatefulSet", "db", &["postgres:16"])).unwrap(),
            ],
            ..Snapshot::default()
        };
        let kinds: Vec<_> = project(&snapshot)
            .services
            .into_iter()
            .map(|service| (service.workload.name, service.data_store))
            .collect();
        assert_eq!(
            kinds,
            [
                ("cache".to_owned(), Some(DataStore::Redis)),
                ("ch".to_owned(), Some(DataStore::Search)),
                ("web".to_owned(), None),
                ("db".to_owned(), Some(DataStore::Postgres)),
            ]
        );
    }
}
