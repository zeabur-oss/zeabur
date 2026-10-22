//! Your account: who the cluster says you are, your role in each project from your rights there,
//! and the issuer's page for your sign-in methods. No Zeabur Identity object is read.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::Settings;
    use zeabur_api::access::SingleCluster;
    use zeabur_api::account::Fixed;
    use zeabur_api::plan_token::PlanKey;
    use zeabur_kube::k8s_openapi::api::core::v1::Namespace;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{AccessRule, AccessRules, Identity};

    use crate::common::{app, app_with, send};

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

    fn rules(verbs: &[&str], resources: &[&str]) -> AccessRules {
        let owned = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect();
        AccessRules {
            rules: vec![AccessRule {
                verbs: owned(verbs),
                groups: vec![String::new()],
                resources: owned(resources),
                names: Vec::new(),
            }],
            incomplete: true,
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
                (
                    "web".to_owned(),
                    Some(rules(&["get", "list"], &["pods", "pods/log"])),
                ),
                ("api".to_owned(), Some(rules(&["get"], &["configmaps"]))),
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
                {"name": "api", "role": "none"},
                {"name": "billing"},
                {"name": "web", "role": "viewer"},
            ])
        );
        assert!(body.get("manageSignInUrl").is_none());
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
            zeabur_api::router_with_key(
                Arc::new(SingleCluster(Arc::new(cluster.clone()))),
                PlanKey::from_bytes([7; 32]),
                settings,
                Arc::new(Fixed(url.map(str::to_owned))),
            )
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
                app_with(&cluster, configured),
                json!("https://id.example.com/me"),
            ),
        ] {
            let (_, body) = send(&app, "GET", "/api/v1/account", None).await;
            assert_eq!(body["manageSignInUrl"], expected, "{body}");
        }
    }
}
