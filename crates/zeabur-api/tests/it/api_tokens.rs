//! API tokens: ServiceAccounts in their own namespace with a member's bindings, managed by admins
//! through member plans.

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::Settings;
    use zeabur_api::settings::IdentitySettings;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, Identity, ServedResource, WriteBody};

    use crate::common::send;
    use crate::support::TestApi;

    const TOKENS: &str = "zeabur-api-tokens";
    /// 2026-11-06T00:00:00Z.
    const NOW: i64 = 1_793_923_200;

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    fn signed_in(username: &str) -> MemoryCluster {
        let mut served = Vec::new();
        for (plural, kind) in [
            ("users", "User"),
            ("credentials", "Credential"),
            ("invitations", "Invitation"),
            ("grants", "Grant"),
        ] {
            served.push(ServedResource::readable(
                "identity.zeabur.com",
                "v1alpha1",
                plural,
                kind,
                true,
            ));
        }
        let rbac = |plural: &str, kind: &str, namespaced: bool| {
            ServedResource::readable("rbac.authorization.k8s.io", "v1", plural, kind, namespaced)
        };
        served.push(ServedResource::readable(
            "",
            "v1",
            "serviceaccounts",
            "ServiceAccount",
            true,
        ));
        served.push(ServedResource::readable(
            "",
            "v1",
            "configmaps",
            "ConfigMap",
            true,
        ));
        served.push(rbac("rolebindings", "RoleBinding", true));
        served.push(rbac("clusterrolebindings", "ClusterRoleBinding", false));
        MemoryCluster::new(State {
            identity: Identity {
                username: username.to_owned(),
                groups: vec![],
            },
            served,
            dynamic: vec![
                object(json!({
                    "apiVersion": "v1", "kind": "ConfigMap",
                    "metadata": {"name": "kube-root-ca.crt", "namespace": TOKENS, "uid": "uid-ca",
                                 "resourceVersion": "1"},
                    "data": {"ca.crt": "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n"},
                })),
                // Labelled with another value than its name: not a token.
                object(json!({
                    "apiVersion": "v1", "kind": "ServiceAccount",
                    "metadata": {"name": "default", "namespace": TOKENS, "uid": "uid-default",
                                 "resourceVersion": "1",
                                 "labels": {"zeabur.com/api-token": "true"}},
                })),
            ],
            now: NOW,
            ..State::default()
        })
    }

    fn settings() -> Settings {
        Settings {
            identity: IdentitySettings {
                issuer: Some("https://id.example.com/".to_owned()),
                project_roles: true,
                ..IdentitySettings::default()
            },
            ..Settings::default()
        }
    }

    /// Plans `change` and applies the plan: the answer of the step that stopped, and the plan.
    async fn change(cluster: &MemoryCluster, change: Value) -> (StatusCode, Value, Value) {
        let app = TestApi::new(cluster).with_settings(settings()).router();
        let (status, plan) = send(&app, "POST", "/api/v1/members/plan", Some(&change)).await;
        if status != StatusCode::OK {
            return (status, plan, Value::Null);
        }
        let (status, applied) = send(
            &app,
            "POST",
            "/api/v1/members/apply",
            Some(&json!({"token": plan["token"]})),
        )
        .await;
        (status, applied, plan)
    }

    async fn tokens(cluster: &MemoryCluster) -> Value {
        let app = TestApi::new(cluster).with_settings(settings()).router();
        let (status, body) = send(&app, "GET", "/api/v1/members", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["apiTokens"].clone()
    }

    fn item<'a>(list: &'a Value, name: &str) -> Option<&'a Value> {
        list.as_array()?.iter().find(|t| t["name"] == name)
    }

    /// The writes made, as `verb Kind/namespace/name`.
    fn writes(cluster: &MemoryCluster) -> Vec<String> {
        let mut out = Vec::new();
        for write in cluster.inspect(|s| s.writes.clone()).unwrap() {
            let verb = match write.body {
                WriteBody::Create(_) => "create",
                WriteBody::Delete(_) => "delete",
                WriteBody::JsonPatch(_)
                | WriteBody::MergePatch(_)
                | WriteBody::EphemeralContainers(_) => "patch",
            };
            out.push(format!(
                "{verb} {}/{}/{}",
                write.target.kind, write.target.namespace, write.target.name
            ));
        }
        out
    }

    /// The token's objects left in the cluster, as `Kind/namespace/name roleRef`.
    fn held(cluster: &MemoryCluster, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        for o in cluster.inspect(|s| s.dynamic.clone()).unwrap() {
            let labelled = o
                .metadata
                .labels
                .as_ref()
                .and_then(|l| l.get("zeabur.com/api-token"))
                .is_some_and(|v| v == name);
            if !labelled {
                continue;
            }
            let mut kind = String::new();
            if let Some(types) = &o.types {
                kind.clone_from(&types.kind);
            }
            let role = o.data["roleRef"]["name"].as_str().unwrap_or_default();
            out.push(format!(
                "{kind}/{}/{} {role}",
                o.metadata.namespace.clone().unwrap_or_default(),
                o.metadata.name.clone().unwrap_or_default()
            ));
        }
        out.sort();
        out
    }

    fn create(name: &str, projects: &Value, seconds: i64) -> Value {
        json!({"op": "createToken", "name": name, "projects": projects,
               "expirationSeconds": seconds})
    }

    #[tokio::test]
    async fn an_admin_makes_a_token_with_roles_in_projects_and_sees_it_once() {
        let cluster = signed_in("zeabur:ada");
        let projects = json!([{"project": "shop", "role": "developer"},
                              {"project": "blog", "role": "viewer"}]);
        let (status, applied, plan) = change(&cluster, create("ci", &projects, 86_400)).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert!(!plan.to_string().contains("memory-token"), "{plan}");
        let planned: Vec<&str> = plan["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["kind"].as_str().unwrap())
            .collect();
        assert_eq!(planned.first(), Some(&"ServiceAccount"));
        assert_eq!(
            planned[planned.len() - 2..],
            ["TokenRequest", "ServiceAccount"]
        );

        assert_eq!(
            held(&cluster, "ci"),
            [
                "ClusterRoleBinding//zeabur-api-token-ci zeabur-member",
                "RoleBinding/blog/zeabur-api-token-ci zeabur-project-viewer",
                "RoleBinding/shop/zeabur-api-token-ci zeabur-project-developer",
                "ServiceAccount/zeabur-api-tokens/ci ",
            ]
        );
        let binding = cluster
            .inspect(|s| s.dynamic.clone())
            .unwrap()
            .into_iter()
            .find(|o| o.metadata.namespace.as_deref() == Some("shop"))
            .unwrap();
        assert_eq!(
            binding.data["subjects"],
            json!([{"kind": "ServiceAccount", "name": "ci", "namespace": TOKENS}])
        );

        let issued = &applied["issued"];
        assert!(
            issued["token"]
                .as_str()
                .unwrap()
                .starts_with("memory-token")
        );
        assert_eq!(issued["namespace"], TOKENS);
        assert_eq!(issued["expiresAt"], "2026-11-07T00:00:00Z");
        assert!(issued["ca"].as_str().unwrap().contains("BEGIN CERTIFICATE"));
        assert_eq!(issued["recorded"], true);

        let listed = tokens(&cluster).await;
        let token = item(&listed, "ci").unwrap();
        assert_eq!(
            token["projects"],
            json!([{"project": "blog", "role": "viewer"},
                   {"project": "shop", "role": "developer"}])
        );
        assert_eq!(token["createdBy"], "zeabur:ada");
        assert_eq!(token["expiresAt"], "2026-11-07T00:00:00Z");
        assert!(token.get("token").is_none(), "{token}");
        assert!(item(&listed, "default").is_none(), "{listed}");
    }

    #[tokio::test]
    async fn changing_a_tokens_access_replaces_its_project_bindings() {
        let cluster = signed_in("zeabur:ada");
        let (status, ..) = change(
            &cluster,
            create("ci", &json!([{"project": "shop", "role": "viewer"}]), 3600),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, applied, _) = change(
            &cluster,
            json!({"op": "tokenAccess", "name": "ci",
                   "projects": [{"project": "blog", "role": "developer"}]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(
            held(&cluster, "ci"),
            [
                "ClusterRoleBinding//zeabur-api-token-ci zeabur-member",
                "RoleBinding/blog/zeabur-api-token-ci zeabur-project-developer",
                "ServiceAccount/zeabur-api-tokens/ci ",
            ]
        );
    }

    #[tokio::test]
    async fn a_token_cannot_be_made_without_the_member_roles() {
        let cluster = signed_in("zeabur:ada");
        let mut settings = settings();
        settings.identity.project_roles = false;
        let app = TestApi::new(&cluster).with_settings(settings).router();
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/members/plan",
            Some(&create("ci", &json!([]), 3600)),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["summary"]["key"], "apiTokens.noProjectRoles", "{body}");
        assert_eq!(writes(&cluster), Vec::<String>::new());
    }

    #[tokio::test]
    async fn tokens_are_listed_without_zeabur_identity() {
        let cluster = signed_in("zeabur:ada");
        let (status, ..) = change(
            &cluster,
            create("ci", &json!([{"project": "shop", "role": "viewer"}]), 3600),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        cluster.update(|s| s.served.retain(|r| r.group != "identity.zeabur.com"));
        let app = TestApi::new(&cluster).with_settings(settings()).router();
        let (status, body) = send(&app, "GET", "/api/v1/members", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["installed"], false);
        assert_eq!(body["canManageTokens"], "allowed");
        assert_eq!(
            item(&body["apiTokens"], "ci").unwrap()["projects"],
            json!([{"project": "shop", "role": "viewer"}])
        );
    }

    #[tokio::test]
    async fn deleting_a_token_deletes_its_bindings_then_its_service_account() {
        let cluster = signed_in("zeabur:ada");
        let (status, ..) = change(
            &cluster,
            create("ci", &json!([{"project": "shop", "role": "viewer"}]), 3600),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, applied, _) =
            change(&cluster, json!({"op": "deleteToken", "name": "ci"})).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let made = writes(&cluster);
        assert_eq!(
            made[made.len() - 3..],
            [
                "delete RoleBinding/shop/zeabur-api-token-ci",
                "delete ClusterRoleBinding//zeabur-api-token-ci",
                "delete ServiceAccount/zeabur-api-tokens/ci",
            ]
        );
        assert_eq!(held(&cluster, "ci"), Vec::<String>::new());
        assert!(item(&tokens(&cluster).await, "ci").is_none());
    }

    #[tokio::test]
    async fn renewing_issues_another_token_for_the_same_account_and_records_its_expiry() {
        let cluster = signed_in("zeabur:ada");
        let (status, ..) = change(&cluster, create("ci", &json!([]), 3600)).await;
        assert_eq!(status, StatusCode::OK);
        cluster.update(|s| s.now = NOW + 1800);
        let (status, applied, plan) = change(
            &cluster,
            json!({"op": "renewToken", "name": "ci", "expirationSeconds": 7200}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(plan["writes"][0]["kind"], "TokenRequest");
        assert_eq!(applied["issued"]["expiresAt"], "2026-11-06T02:30:00Z");
        let asked = cluster.inspect(|s| s.issued_tokens.clone()).unwrap();
        assert_eq!(asked.last().unwrap(), &(format!("{TOKENS}/ci"), 7200));
        assert_eq!(
            item(&tokens(&cluster).await, "ci").unwrap()["expiresAt"],
            "2026-11-06T02:30:00Z"
        );
    }

    #[tokio::test]
    async fn a_person_without_rights_in_the_token_namespace_is_refused_before_any_write() {
        let cluster = signed_in("zeabur:bob");
        cluster.update(|s| {
            s.forbidden_namespaces.insert(TOKENS.to_owned());
        });
        for op in [
            create("ci", &json!([]), 3600),
            json!({"op": "renewToken", "name": "ci", "expirationSeconds": 3600}),
            json!({"op": "deleteToken", "name": "ci"}),
        ] {
            let (status, body, _) = change(&cluster, op).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        }
        assert_eq!(writes(&cluster), Vec::<String>::new());
        let app = TestApi::new(&cluster).with_settings(settings()).router();
        let (status, body) = send(&app, "GET", "/api/v1/members", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["canManageTokens"], "denied", "{body}");
    }

    #[tokio::test]
    async fn bad_names_lifetimes_and_projects_are_refused_before_any_write() {
        let cluster = signed_in("zeabur:ada");
        let (status, ..) = change(&cluster, create("ci", &json!([]), 3600)).await;
        assert_eq!(status, StatusCode::OK);
        let before = writes(&cluster);
        let refused = [
            (create("ci", &json!([]), 3600), StatusCode::CONFLICT),
            (create("CI", &json!([]), 3600), StatusCode::BAD_REQUEST),
            (create("ci2", &json!([]), 60), StatusCode::BAD_REQUEST),
            (
                create(
                    "ci2",
                    &json!([{"project": TOKENS, "role": "developer"}]),
                    3600,
                ),
                StatusCode::BAD_REQUEST,
            ),
            (
                json!({"op": "deleteToken", "name": "default"}),
                StatusCode::NOT_FOUND,
            ),
        ];
        for (op, expected) in refused {
            let (status, body, _) = change(&cluster, op).await;
            assert_eq!(status, expected, "{body}");
        }
        assert_eq!(writes(&cluster), before);
    }

    #[tokio::test]
    async fn a_plan_made_before_someone_else_took_the_name_is_not_applied() {
        let cluster = signed_in("zeabur:ada");
        let app = TestApi::new(&cluster).with_settings(settings()).router();
        let (status, plan) = send(
            &app,
            "POST",
            "/api/v1/members/plan",
            Some(&create("ci", &json!([]), 3600)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        let (status, ..) = change(&cluster, create("ci", &json!([]), 3600)).await;
        assert_eq!(status, StatusCode::OK);
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/members/apply",
            Some(&json!({"token": plan["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
    }
}
