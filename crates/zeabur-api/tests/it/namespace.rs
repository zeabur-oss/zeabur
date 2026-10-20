//! Adopting a namespace, letting it go and a project's details through the API, against an
//! in-memory cluster: the write is one patch of the Namespace, cluster-scoped; the list shows the
//! result; a system namespace and a refused patch get typed problems.

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, WriteBody};

    use crate::common::{app, plan_and_apply, send};

    fn namespace(name: &str, labels: &Value) -> Namespace {
        serde_json::from_value(json!({"apiVersion": "v1", "kind": "Namespace",
            "metadata": {"name": name, "resourceVersion": "5", "labels": labels}}))
        .unwrap()
    }

    fn cluster() -> MemoryCluster {
        MemoryCluster::new(State {
            identity: Identity {
                username: "alice".to_owned(),
                groups: vec!["developers".to_owned()],
            },
            namespaces: vec![
                namespace("kube-system", &json!({})),
                namespace("shop", &json!({"team": "web"})),
            ],
            ..State::default()
        })
    }

    async fn summary(app: &Router, name: &str) -> Value {
        let (status, list) = send(app, "GET", "/api/v1/projects", None).await;
        assert_eq!(status, StatusCode::OK, "{list}");
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .cloned()
            .unwrap()
    }

    #[tokio::test]
    async fn adopting_then_letting_go_patches_only_the_label_of_the_namespace() {
        let cluster = cluster();
        let app = app(&cluster);
        assert_eq!(summary(&app, "shop").await["adopted"], false);

        let (status, applied) = plan_and_apply(&app, "shop", json!([{"op": "adopt"}])).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(applied["applied"][0]["target"]["kind"], "Namespace");
        let shop = summary(&app, "shop").await;
        assert_eq!(shop["adopted"], true);
        let writes = cluster.inspect(|s| s.writes.clone()).unwrap();
        assert_eq!(writes.len(), 1);
        // Cluster-scoped, and the patch adds the one label after pinning the resourceVersion.
        assert_eq!(writes[0].target.namespace, "");
        assert_eq!(
            writes[0].body,
            WriteBody::JsonPatch(json!([
                {"op": "test", "path": "/metadata/resourceVersion", "value": "5"},
                {"op": "add", "path": "/metadata/labels/zeabur.com~1project", "value": "true"}
            ]))
        );
        assert_eq!(cluster.inspect(|s| s.dry_runs), Some(1));

        let (status, applied) = plan_and_apply(
            &app,
            "shop",
            json!([{"op": "details", "displayName": "Web shop", "icon": "storefront"}]),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let shop = summary(&app, "shop").await;
        assert_eq!(
            (&shop["displayName"], &shop["icon"]),
            (&json!("Web shop"), &json!("storefront"))
        );

        let (status, applied) = plan_and_apply(&app, "shop", json!([{"op": "unadopt"}])).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let stored = cluster
            .inspect(|s| {
                s.namespaces
                    .iter()
                    .find(|n| n.metadata.name.as_deref() == Some("shop"))
                    .cloned()
            })
            .flatten()
            .unwrap();
        // Only the project label went; the other label and the details stay.
        assert_eq!(
            stored
                .metadata
                .labels
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
            [("team".to_owned(), "web".to_owned())]
        );
        assert_eq!(stored.metadata.annotations.unwrap().len(), 2);
        assert_eq!(summary(&app, "shop").await["adopted"], false);
    }

    #[tokio::test]
    async fn a_system_namespace_is_refused_with_its_own_type() {
        let cluster = cluster();
        let app = app(&cluster);
        assert_eq!(summary(&app, "kube-system").await["system"], true);
        let (status, problem) = plan_and_apply(&app, "kube-system", json!([{"op": "adopt"}])).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
        assert_eq!(problem["type"], "/problems/system-namespace");
        assert_eq!(problem["kind"], "system-namespace");
        assert_eq!(
            cluster.inspect(|s| (s.writes.len(), s.dry_runs)),
            Some((0, 0))
        );
    }

    #[tokio::test]
    async fn a_refused_patch_names_the_right_to_ask_for() {
        let cluster = cluster();
        cluster.update(|s| {
            s.forbidden_writes.insert("Namespace/shop".to_owned());
        });
        let app = app(&cluster);
        let (status, namespace) = send(&app, "GET", "/api/v1/projects/shop/namespace", None).await;
        assert_eq!(status, StatusCode::OK, "{namespace}");
        assert_eq!(namespace["canChange"], "denied");
        assert_eq!(namespace["adopted"], false);
        assert_eq!(namespace["system"], false);

        let (status, problem) = plan_and_apply(&app, "shop", json!([{"op": "adopt"}])).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(
            problem["denied"],
            json!({"verb": "patch", "group": "", "resource": "namespaces", "name": "shop"})
        );
        assert_eq!(cluster.inspect(|s| s.writes.len()), Some(0));
    }

    #[tokio::test]
    async fn a_refused_read_of_the_namespace_names_the_get() {
        let cluster = cluster();
        cluster.update(|s| {
            s.forbidden_resources.insert("namespaces".to_owned());
        });
        let app = app(&cluster);
        let denied = json!({"verb": "get", "group": "", "resource": "namespaces", "name": "shop"});
        let (status, problem) = send(&app, "GET", "/api/v1/projects/shop/namespace", None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["denied"], denied);

        let (status, problem) = plan_and_apply(&app, "shop", json!([{"op": "adopt"}])).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
        assert_eq!(problem["denied"], denied);
    }

    #[tokio::test]
    async fn the_namespace_says_whether_the_person_may_change_it() {
        let cluster = cluster();
        cluster.update(|s| {
            s.namespaces.push(
                serde_json::from_value(json!({"metadata": {"name": "blog",
                "labels": {"zeabur.com/project": "true"},
                "annotations": {"zeabur.com/display-name": "Blog",
                    "zeabur.com/icon": "not-an-icon"}}}))
                .unwrap(),
            );
        });
        let app = app(&cluster);
        let (status, namespace) = send(&app, "GET", "/api/v1/projects/blog/namespace", None).await;
        assert_eq!(status, StatusCode::OK, "{namespace}");
        assert_eq!(namespace["canChange"], "allowed");
        assert_eq!(namespace["adopted"], true);
        assert_eq!(namespace["displayName"], "Blog");
        // An icon outside the built-in set is no icon.
        assert!(namespace.get("icon").is_none());
        let (status, _) = send(&app, "GET", "/api/v1/projects/nowhere/namespace", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
