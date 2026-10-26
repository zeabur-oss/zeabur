//! Deleting a project through the API, against an in-memory cluster: the plan says what goes and
//! what stays; the apply deletes the database objects before the Namespace, as the person; Zeabur's
//! own namespaces are refused; a missing right is told; a project being deleted says what holds
//! it.

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::k8s_openapi::api::apps::v1::Deployment;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, NamespaceObjects, ServedResource};

    use crate::common::{app, plan_and_apply, send};

    fn namespace(name: &str, extra: &Value) -> Namespace {
        let mut object = json!({"apiVersion": "v1", "kind": "Namespace",
            "metadata": {"name": name, "uid": format!("{name}-uid"), "resourceVersion": "5",
                "labels": {"zeabur.com/project": "true"}}});
        if let (Some(metadata), Some(extra)) = (
            object["metadata"].as_object_mut(),
            extra.get("metadata").and_then(Value::as_object),
        ) {
            metadata.extend(extra.clone());
        }
        if let Some(status) = extra.get("status") {
            object["status"] = status.clone();
        }
        serde_json::from_value(object).unwrap()
    }

    fn database(finalizers: &[&str]) -> DynamicObject {
        serde_json::from_value(
            json!({"apiVersion": "postgresql.cnpg.io/v1", "kind": "Cluster",
            "metadata": {"name": "db", "namespace": "shop", "uid": "db-uid",
                "resourceVersion": "9", "finalizers": finalizers},
            "spec": {"instances": 1}}),
        )
        .unwrap()
    }

    fn operator(available: i32) -> Deployment {
        serde_json::from_value(json!({"metadata": {"name": "cnpg-controller-manager",
                "namespace": "cnpg-system", "labels": {"app.kubernetes.io/name": "cloudnative-pg"}},
            "spec": {"selector": {}, "template": {"spec": {"containers": []}}},
            "status": {"availableReplicas": available}}))
        .unwrap()
    }

    fn cluster(shop: Namespace) -> MemoryCluster {
        let web: Deployment = serde_json::from_value(json!({
            "metadata": {"name": "web", "namespace": "shop", "uid": "web-uid", "resourceVersion": "3"},
            "spec": {"selector": {}, "template": {"spec": {"containers": [{"name": "web", "image": "nginx"}]}}}}))
        .unwrap();
        MemoryCluster::new(State {
            namespaces: vec![
                shop,
                namespace("zeabur-identity", &json!({})),
                namespace(
                    "traefik",
                    &json!({"metadata": {"labels": {
                    "zeabur.com/project": "true", "zeabur.com/platform": "true"}}}),
                ),
            ],
            objects: [
                (
                    "shop".to_owned(),
                    NamespaceObjects {
                        namespace: "shop".to_owned(),
                        deployments: vec![web],
                        ..NamespaceObjects::default()
                    },
                ),
                (
                    "cnpg-system".to_owned(),
                    NamespaceObjects {
                        namespace: "cnpg-system".to_owned(),
                        deployments: vec![operator(1)],
                        ..NamespaceObjects::default()
                    },
                ),
            ]
            .into(),
            served: vec![ServedResource::readable(
                "postgresql.cnpg.io",
                "v1",
                "clusters",
                "Cluster",
                true,
            )],
            dynamic: vec![database(&["cnpg.io/cleanup"])],
            ..State::default()
        })
    }

    fn deletes_project(plan: &Value) -> &Value {
        plan["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["code"] == "deletesProject")
            .unwrap_or_else(|| panic!("no deletesProject in {plan}"))
    }

    #[tokio::test]
    async fn a_project_with_a_database_goes_away_database_first() {
        let cluster = cluster(namespace("shop", &json!({})));
        let app = app(&cluster);
        let (status, plan) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({"changes": [{"op": "deleteProject"}]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        let gone = deletes_project(&plan);
        assert_eq!(
            gone["services"],
            json!([{"kind": "Deployment", "name": "web"}])
        );
        assert_eq!(
            gone["databases"],
            json!([{"kind": "Cluster", "name": "db", "operator": "ready"}])
        );

        let (status, applied) =
            plan_and_apply(&app, "shop", json!([{"op": "deleteProject"}])).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let deleted: Vec<(String, String)> = cluster
            .inspect(|s| {
                s.writes
                    .iter()
                    .filter(|w| matches!(w.body, zeabur_kube::WriteBody::Delete(_)))
                    .map(|w| (w.target.kind.clone(), w.target.name.clone()))
                    .collect()
            })
            .unwrap();
        assert_eq!(
            deleted,
            [
                ("Cluster".to_owned(), "db".to_owned()),
                ("Namespace".to_owned(), "shop".to_owned()),
            ]
        );
        let (_, list) = send(&app, "GET", "/api/v1/projects", None).await;
        assert!(
            !list["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["name"] == "shop"),
            "{list}"
        );
    }

    #[tokio::test]
    async fn a_database_whose_operator_does_not_run_is_named() {
        let cluster = cluster(namespace("shop", &json!({})));
        cluster.update(|s| {
            if let Some(objects) = s.objects.get_mut("cnpg-system") {
                objects.deployments = vec![operator(0)];
            }
        });
        let (status, plan) = send(
            &app(&cluster),
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({"changes": [{"op": "deleteProject"}]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert_eq!(
            deletes_project(&plan)["databases"][0]["operator"],
            "notReady"
        );
    }

    #[tokio::test]
    async fn zeabur_and_component_namespaces_are_refused() {
        let cluster = cluster(namespace("shop", &json!({})));
        let app = app(&cluster);
        for project in ["zeabur-identity", "traefik"] {
            let (status, problem) = send(
                &app,
                "POST",
                &format!("/api/v1/projects/{project}/plan"),
                Some(&json!({"changes": [{"op": "deleteProject"}]})),
            )
            .await;
            assert_ne!(status, StatusCode::OK, "{project}: {problem}");
            assert!(
                problem["type"]
                    .as_str()
                    .unwrap()
                    .ends_with("system-namespace"),
                "{project}: {problem}"
            );
        }
        assert!(
            cluster.inspect(|s| s.writes.is_empty()).unwrap(),
            "a refused plan writes nothing"
        );
    }

    #[tokio::test]
    async fn without_the_right_to_delete_the_namespace_the_plan_says_so_and_the_apply_is_refused() {
        let cluster = cluster(namespace("shop", &json!({})));
        cluster.update(|s| s.forbidden_writes = BTreeSet::from(["Namespace/shop".to_owned()]));
        let app = app(&cluster);
        let (status, plan) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&json!({"changes": [{"op": "deleteProject"}]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        let rights = plan["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["code"] == "rightsMissing")
            .unwrap_or_else(|| panic!("{plan}"));
        assert!(
            rights["rights"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["verb"] == "delete" && r["resource"] == "namespaces"),
            "{rights}"
        );
        let (status, problem) =
            plan_and_apply(&app, "shop", json!([{"op": "deleteProject"}])).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert!(cluster.inspect(|s| s.writes.is_empty()).unwrap());
    }

    #[tokio::test]
    async fn a_project_being_deleted_says_what_holds_it_and_is_not_deleted_twice() {
        let terminating = namespace(
            "shop",
            &json!({
                "metadata": {"deletionTimestamp": "2026-10-26T00:00:00Z"},
                "status": {"phase": "Terminating", "conditions": [
                    {"type": "NamespaceContentRemaining", "status": "True",
                     "message": "Some resources are remaining: clusters.postgresql.cnpg.io has 1 resource instances"},
                    {"type": "NamespaceFinalizersRemaining", "status": "True",
                     "message": "Some content in the namespace has finalizers remaining: cnpg.io/cleanup in 1 resource instances"}
                ]}
            }),
        );
        let cluster = cluster(terminating);
        let app = app(&cluster);
        let (status, ns) = send(&app, "GET", "/api/v1/projects/shop/namespace", None).await;
        assert_eq!(status, StatusCode::OK, "{ns}");
        assert_eq!(
            ns["deleting"]["held"],
            json!([{"group": "postgresql.cnpg.io", "resource": "clusters", "kind": "Cluster",
                "name": "db", "finalizers": ["cnpg.io/cleanup"]}]),
            "{ns}"
        );
        let (_, list) = send(&app, "GET", "/api/v1/projects", None).await;
        let shop = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "shop")
            .cloned()
            .unwrap();
        assert_eq!(shop["deleting"], true);
        let (status, problem) =
            plan_and_apply(&app, "shop", json!([{"op": "deleteProject"}])).await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
    }
}
