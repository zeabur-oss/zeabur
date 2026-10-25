//! Templates of Zeabur Catalog against an in-memory cluster: the list says which components a
//! template needs and whether the cluster serves them, and a `template` change is planned as the
//! objects of the template in the project.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::settings::{DomainSettings, Settings};
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{Identity, NamespaceObjects, ServedResource, WriteBody};

    use crate::common::{app_with, send};

    fn cluster() -> MemoryCluster {
        let namespace: Namespace = serde_json::from_value(json!({"metadata": {"name": "shop",
            "labels": {"zeabur.com/project": "true"}}}))
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
                    ..NamespaceObjects::default()
                },
            )]),
            ..State::default()
        })
    }

    fn serve_cloudnative_pg(cluster: &MemoryCluster) {
        cluster.update(|s| {
            s.served.push(ServedResource {
                verbs: ["create", "get", "list"].map(str::to_owned).to_vec(),
                ..ServedResource::readable("postgresql.cnpg.io", "v1", "clusters", "Cluster", true)
            });
        });
    }

    fn settings() -> Settings {
        Settings {
            domains: DomainSettings {
                suffixes: Vec::new(),
                cluster_issuer: Some("letsencrypt".to_owned()),
            },
            ..Settings::default()
        }
    }

    /// What the browser sends for Miniflux: the Secret of its generated input, then the template.
    fn miniflux() -> Value {
        let password = "a".repeat(43);
        json!({"changes": [
            {"op": "create", "object": {"apiVersion": "v1", "kind": "Secret",
                "metadata": {"name": "miniflux-secrets"}, "type": "Opaque",
                "stringData": {"admin-password": password}}},
            {"op": "template", "template": "miniflux", "domain": "rss.example.com"},
        ]})
    }

    fn requirement<'a>(list: &'a Value, template: &str, component: &str) -> &'a Value {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == template)
            .and_then(|t| {
                t["requires"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["component"] == component)
            })
            .unwrap()
    }

    #[tokio::test]
    async fn a_template_is_planned_and_applied_as_its_objects_in_the_project() {
        let cluster = cluster();
        serve_cloudnative_pg(&cluster);
        let app = app_with(&cluster, settings());
        let (status, planned) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&miniflux()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{planned}");
        assert!(!planned.to_string().contains(&"a".repeat(43)), "{planned}");
        // The database and the domain start too, and the database's instance counts.
        let preview = &planned["preview"];
        assert_eq!(preview["databases"][0]["name"], "miniflux-db", "{preview}");
        assert_eq!(preview["domains"], json!(["rss.example.com"]), "{preview}");
        assert_eq!(preview["fit"]["pods"], 2, "{preview}");
        let (status, applied) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let created: Vec<Value> = cluster
            .inspect(|s| {
                s.writes
                    .iter()
                    .filter_map(|w| {
                        if let WriteBody::Create(body) = &w.body {
                            Some(body.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap();
        let named = |kind: &str, name: &str| {
            created
                .iter()
                .find(|o| o["kind"] == kind && o["metadata"]["name"] == name)
                .unwrap_or_else(|| panic!("no {kind} {name} in {created:?}"))
        };
        for (kind, name) in [
            ("Secret", "miniflux-secrets"),
            ("Deployment", "miniflux"),
            ("Service", "miniflux"),
            ("Ingress", "miniflux"),
            ("Cluster", "miniflux-db"),
        ] {
            assert_eq!(
                named(kind, name)["metadata"]["namespace"],
                "shop",
                "{kind} {name}"
            );
        }
        for object in created.iter().filter(|o| o["kind"] != "Secret") {
            assert_eq!(
                object["metadata"]["labels"]["app.kubernetes.io/instance"], "miniflux",
                "{object}"
            );
        }
        let ingress = named("Ingress", "miniflux");
        assert_eq!(ingress["spec"]["rules"][0]["host"], "rss.example.com");
        assert_eq!(
            ingress["metadata"]["annotations"]["cert-manager.io/cluster-issuer"],
            "letsencrypt"
        );
    }

    #[tokio::test]
    async fn deploying_a_template_again_names_every_object_that_exists() {
        let cluster = cluster();
        serve_cloudnative_pg(&cluster);
        let app = app_with(&cluster, settings());
        let (_, planned) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&miniflux()),
        )
        .await;
        let (status, _) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/apply",
            Some(&json!({"token": planned["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, problem) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&miniflux()),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{problem}");
        assert_eq!(problem["summary"]["key"], "problem.summary.already-exists");
        let objects = problem["summary"]["params"]["objects"].as_str().unwrap();
        for object in [
            "Secret miniflux-secrets",
            "Deployment miniflux",
            "Cluster miniflux-db",
        ] {
            assert!(objects.contains(object), "{problem}");
        }
    }

    #[tokio::test]
    async fn a_missing_component_is_reported_and_its_objects_are_not_planned() {
        let cluster = cluster();
        let app = app_with(&cluster, settings());
        let (status, list) = send(&app, "GET", "/api/v1/cluster/templates", None).await;
        assert_eq!(status, StatusCode::OK, "{list}");
        let cnpg = requirement(&list, "miniflux", "cloudnative-pg");
        assert_eq!(cnpg["state"], "missing");
        assert_eq!(cnpg["name"], "CloudNativePG");
        let (status, problem) = send(
            &app,
            "POST",
            "/api/v1/projects/shop/plan",
            Some(&miniflux()),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
        assert!(
            problem["detail"]
                .as_str()
                .unwrap()
                .contains("postgresql.cnpg.io"),
            "{problem}"
        );
        assert!(cluster.inspect(|s| s.writes.is_empty()).unwrap());

        serve_cloudnative_pg(&cluster);
        let (_, list) = send(&app, "GET", "/api/v1/cluster/templates", None).await;
        assert_eq!(
            requirement(&list, "miniflux", "cloudnative-pg")["state"],
            "installed"
        );
    }

    #[tokio::test]
    async fn a_template_input_that_fails_its_pattern_points_at_the_input() {
        let cluster = cluster();
        serve_cloudnative_pg(&cluster);
        let app = app_with(&cluster, settings());
        let request = json!({"changes": [{"op": "template", "template": "wordpress",
            "domain": "blog.example.com", "inputs": {"adminEmail": "nope"}}]});
        let (status, problem) =
            send(&app, "POST", "/api/v1/projects/shop/plan", Some(&request)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
        assert_eq!(problem["at"]["field"], "inputs.adminEmail", "{problem}");
    }
}
