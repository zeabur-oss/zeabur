//! Your account: who the cluster says you are, your role in each project from your rights there,
//! and the issuer's page for your sign-in methods. No Zeabur Identity object is read.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::Settings;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{AccessRule, AccessRules, Identity};

    use crate::common::{app, send};
    use crate::support::TestApi;

    fn namespace(name: &str, adopted: bool) -> Namespace {
        let labels = if adopted {
            json!({"zeabur.com/project": "true"})
        } else {
            json!({})
        };
        serde_json::from_value(json!({"apiVersion": "v1", "kind": "Namespace",
            "metadata": {"name": name, "labels": labels}}))
        .unwrap()
    }

    fn rules(verbs: &[&str]) -> AccessRules {
        AccessRules {
            rules: vec![AccessRule {
                verbs: verbs.iter().map(|s| (*s).to_owned()).collect(),
                groups: vec![String::new()],
            }],
            incomplete: false,
        }
    }

    fn cluster(access_rules: BTreeMap<String, Option<AccessRules>>, admin: bool) -> MemoryCluster {
        MemoryCluster::new(State {
            identity: Identity {
                username: "zeabur:ada".to_owned(),
                groups: vec![
                    "zeabur:developers".to_owned(),
                    "system:authenticated".to_owned(),
                ],
            },
            namespaces: vec![
                namespace("web", true),
                namespace("api", true),
                namespace("legacy", false),
                namespace("kube-system", true),
                namespace("billing", true),
            ],
            access_rules,
            cluster_wide_denied: !admin,
            refused_rights: ["create rolebindings".to_owned()].into(),
            ..State::default()
        })
    }

    async fn account(cluster: &MemoryCluster) -> Value {
        let (status, body) = send(&app(cluster), "GET", "/api/v1/account", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    #[tokio::test]
    async fn you_are_who_the_cluster_says_with_a_role_per_adopted_project() {
        let cluster = cluster(
            BTreeMap::from([
                ("web".to_owned(), Some(rules(&["get", "list"]))),
                ("api".to_owned(), Some(rules(&["get", "patch"]))),
                ("billing".to_owned(), None),
            ]),
            false,
        );
        let body = account(&cluster).await;
        assert_eq!(body["username"], "zeabur:ada");
        assert_eq!(
            body["groups"],
            json!(["zeabur:developers", "system:authenticated"])
        );
        assert_eq!(body["clusterAdmin"], false);
        assert_eq!(
            body["projects"],
            json!([
                {"name": "api", "role": "developer"},
                {"name": "billing"},
                {"name": "web", "role": "viewer"},
            ])
        );
        assert!(body.get("manageSignInUrl").is_none());
    }

    /// Without pods to list, a read-only project gives no role; the right to create RoleBindings
    /// there gives Admin.
    #[tokio::test]
    async fn the_role_follows_the_access_reviews_of_the_project() {
        let cluster = cluster(
            BTreeMap::from([("web".to_owned(), Some(rules(&["get"])))]),
            false,
        );
        cluster.update(|s| {
            s.refused_rights.insert("list pods".to_owned());
        });
        let body = account(&cluster).await;
        assert_eq!(body["projects"][2], json!({"name": "web", "role": "none"}));
        cluster.update(|s| s.refused_rights.clear());
        let body = account(&cluster).await;
        assert_eq!(body["projects"][2], json!({"name": "web", "role": "admin"}));
    }

    #[tokio::test]
    async fn a_cluster_administrator_is_admin_everywhere() {
        let cluster = cluster(BTreeMap::from([("web".to_owned(), None)]), true);
        let body = account(&cluster).await;
        assert_eq!(body["clusterAdmin"], true);
        let roles: Vec<_> = body["projects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["role"].clone())
            .collect();
        assert_eq!(roles, [json!("admin"), json!("admin"), json!("admin")]);
    }

    #[tokio::test]
    async fn someone_who_may_read_nothing_still_learns_who_they_are() {
        let cluster = cluster(BTreeMap::new(), false);
        cluster.update(|s| s.forbid_everything = true);
        let body = account(&cluster).await;
        assert_eq!(body["username"], "zeabur:ada");
        assert!(body.get("projects").is_none(), "{body}");
    }

    #[tokio::test]
    async fn the_page_for_sign_in_methods_is_the_configured_one_else_the_issuers() {
        let cluster = cluster(BTreeMap::new(), false);
        let found = |url: Option<&str>, settings: Settings| {
            TestApi::new(&cluster)
                .with_settings(settings)
                .with_sign_in(url)
                .router()
        };
        let configured = Settings {
            account_url: Some("https://id.example.com/me".to_owned()),
            ..Settings::default()
        };
        for (app, expected) in [
            (
                found(Some("https://id.example.com/passkeys"), configured.clone()),
                json!("https://id.example.com/me"),
            ),
            (
                found(Some("https://id.example.com/passkeys"), Settings::default()),
                json!("https://id.example.com/passkeys"),
            ),
            (found(None, Settings::default()), Value::Null),
            (
                TestApi::new(&cluster).with_settings(configured).router(),
                json!("https://id.example.com/me"),
            ),
        ] {
            let (_, body) = send(&app, "GET", "/api/v1/account", None).await;
            assert_eq!(body["manageSignInUrl"], expected, "{body}");
        }
    }
}
