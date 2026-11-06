//! A project's API tokens: ServiceAccounts with a binding to a project role, and the tokens
//! kube-apiserver issues for them.

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, Identity, ServedResource, WriteBody};

    use crate::common::send;
    use crate::support::TestApi;

    const BASE: &str = "/api/v1/projects/shop/api-tokens";
    /// 2026-11-06T00:00:00Z.
    const NOW: i64 = 1_793_923_200;

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    fn signed_in(username: &str) -> MemoryCluster {
        let rbac = |plural: &str, kind: &str, namespaced: bool| {
            ServedResource::readable("rbac.authorization.k8s.io", "v1", plural, kind, namespaced)
        };
        MemoryCluster::new(State {
            identity: Identity {
                username: username.to_owned(),
                groups: vec![],
            },
            served: vec![
                ServedResource::readable("", "v1", "serviceaccounts", "ServiceAccount", true),
                ServedResource::readable("", "v1", "configmaps", "ConfigMap", true),
                rbac("rolebindings", "RoleBinding", true),
                rbac("clusterrolebindings", "ClusterRoleBinding", false),
            ],
            dynamic: vec![
                object(json!({
                    "apiVersion": "v1", "kind": "ConfigMap",
                    "metadata": {"name": "kube-root-ca.crt", "namespace": "shop", "uid": "uid-ca",
                                 "resourceVersion": "1"},
                    "data": {"ca.crt": "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n"},
                })),
                object(json!({
                    "apiVersion": "v1", "kind": "ServiceAccount",
                    "metadata": {"name": "default", "namespace": "shop", "uid": "uid-default",
                                 "resourceVersion": "1"},
                })),
            ],
            now: NOW,
            ..State::default()
        })
    }

    /// Plans `change` and applies the plan: the answer of the step that stopped, and the plan.
    async fn change(cluster: &MemoryCluster, change: Value) -> (StatusCode, Value, Value) {
        let app = TestApi::new(cluster).router();
        let (status, plan) = send(&app, "POST", &format!("{BASE}/plan"), Some(&change)).await;
        if status != StatusCode::OK {
            return (status, plan, Value::Null);
        }
        let (status, applied) = send(
            &app,
            "POST",
            &format!("{BASE}/apply"),
            Some(&json!({"token": plan["token"]})),
        )
        .await;
        (status, applied, plan)
    }

    async fn list(cluster: &MemoryCluster) -> Value {
        let (status, body) = send(&TestApi::new(cluster).router(), "GET", BASE, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    /// The writes made, as `verb Kind/name`.
    fn writes(cluster: &MemoryCluster) -> Vec<String> {
        let mut out = Vec::new();
        for write in cluster.inspect(|s| s.writes.clone()).unwrap() {
            let verb = match write.body {
                WriteBody::Create(_) => "create",
                WriteBody::Delete(_) => "delete",
                WriteBody::JsonPatch(_) | WriteBody::EphemeralContainers(_) => "patch",
            };
            out.push(format!(
                "{verb} {}/{}",
                write.target.kind, write.target.name
            ));
        }
        out
    }

    fn create(name: &str, role: &str, cluster: bool, seconds: i64) -> Value {
        json!({"op": "create", "name": name, "role": role, "cluster": cluster,
               "expirationSeconds": seconds})
    }

    fn item<'a>(list: &'a Value, name: &str) -> Option<&'a Value> {
        list["items"].as_array()?.iter().find(|t| t["name"] == name)
    }

    #[tokio::test]
    async fn a_new_token_is_a_bound_service_account_and_its_token_is_shown_once() {
        let cluster = signed_in("zeabur:ada");
        let (status, applied, plan) =
            change(&cluster, create("ci", "developer", false, 86_400)).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let shown: Vec<String> = plan["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                format!(
                    "{} {}",
                    w["action"].as_str().unwrap(),
                    w["kind"].as_str().unwrap()
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                "create ServiceAccount",
                "create RoleBinding",
                "create TokenRequest",
                "patch ServiceAccount"
            ]
        );
        assert!(!plan.to_string().contains("memory-token"), "{plan}");

        assert_eq!(
            writes(&cluster),
            [
                "create ServiceAccount/ci",
                "create RoleBinding/zeabur-api-token-ci",
                "patch ServiceAccount/ci"
            ]
        );
        let issued = &applied["issued"];
        assert!(
            issued["token"]
                .as_str()
                .unwrap()
                .starts_with("memory-token")
        );
        assert_eq!(issued["expiresAt"], "2026-11-07T00:00:00Z");
        assert!(issued["ca"].as_str().unwrap().contains("BEGIN CERTIFICATE"));
        assert_eq!(issued["recorded"], true);

        let listed = list(&cluster).await;
        let token = item(&listed, "ci").unwrap();
        assert_eq!(token["role"], "developer");
        assert_eq!(token["cluster"], false);
        assert_eq!(token["createdBy"], "zeabur:ada");
        assert_eq!(token["expiresAt"], "2026-11-07T00:00:00Z");
        assert!(token.get("token").is_none(), "{token}");
        assert!(item(&listed, "default").is_none(), "{listed}");
    }

    #[tokio::test]
    async fn kube_apiserver_cuts_the_expiry_and_the_answer_says_so() {
        let cluster = signed_in("zeabur:ada");
        cluster.update(|s| s.max_token_seconds = Some(3600));
        let (status, applied, _) =
            change(&cluster, create("ci", "viewer", false, 30 * 86_400)).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(applied["issued"]["expiresAt"], "2026-11-06T01:00:00Z");
        let listed = list(&cluster).await;
        assert_eq!(
            item(&listed, "ci").unwrap()["expiresAt"],
            "2026-11-06T01:00:00Z"
        );
    }

    #[tokio::test]
    async fn renewing_issues_another_token_for_the_same_account_and_records_its_expiry() {
        let cluster = signed_in("zeabur:ada");
        let (status, ..) = change(&cluster, create("ci", "viewer", false, 3600)).await;
        assert_eq!(status, StatusCode::OK);
        cluster.update(|s| s.now = NOW + 1800);
        let (status, applied, plan) = change(
            &cluster,
            json!({"op": "renew", "name": "ci", "expirationSeconds": 7200}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(plan["writes"][0]["kind"], "TokenRequest");
        assert_eq!(applied["issued"]["expiresAt"], "2026-11-06T02:30:00Z");
        let asked = cluster.inspect(|s| s.issued_tokens.clone()).unwrap();
        assert_eq!(asked.last().unwrap(), &("shop/ci".to_owned(), 7200));
        let listed = list(&cluster).await;
        assert_eq!(
            item(&listed, "ci").unwrap()["expiresAt"],
            "2026-11-06T02:30:00Z"
        );
    }

    #[tokio::test]
    async fn deleting_a_token_deletes_its_binding_then_its_account() {
        let cluster = signed_in("zeabur:ada");
        let (status, ..) = change(&cluster, create("ci", "viewer", false, 3600)).await;
        assert_eq!(status, StatusCode::OK);
        let (status, applied, _) = change(&cluster, json!({"op": "delete", "name": "ci"})).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert!(applied.get("issued").is_none(), "{applied}");
        let made = writes(&cluster);
        assert_eq!(
            made[made.len() - 2..],
            [
                "delete RoleBinding/zeabur-api-token-ci",
                "delete ServiceAccount/ci"
            ]
        );
        assert!(item(&list(&cluster).await, "ci").is_none());
    }

    #[tokio::test]
    async fn a_service_account_that_is_not_a_token_is_neither_renewed_nor_deleted() {
        let cluster = signed_in("zeabur:ada");
        for op in [
            json!({"op": "renew", "name": "default", "expirationSeconds": 3600}),
            json!({"op": "delete", "name": "default"}),
        ] {
            let (status, body, _) = change(&cluster, op).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        }
        assert_eq!(writes(&cluster), Vec::<String>::new());
    }

    #[tokio::test]
    async fn only_an_admin_makes_a_token_for_every_namespace() {
        let cluster = signed_in("zeabur:bob");
        cluster.update(|s| s.cluster_wide_denied = true);
        let listed = list(&cluster).await;
        for choice in listed["choices"].as_array().unwrap() {
            if choice["cluster"] == true {
                assert_eq!(choice["permission"], "denied", "{choice}");
                assert_eq!(choice["refusal"], "notAdmin", "{choice}");
            } else {
                assert_eq!(choice["permission"], "allowed", "{choice}");
            }
        }
        let (status, body, _) = change(&cluster, create("ci", "viewer", true, 3600)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(body["summary"]["key"], "apiTokens.clusterAdmins");
        assert_eq!(writes(&cluster), Vec::<String>::new());

        let admin = signed_in("zeabur:ada");
        let (status, applied, _) = change(&admin, create("ci", "viewer", true, 3600)).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert!(
            writes(&admin)
                .contains(&"create ClusterRoleBinding/zeabur-api-token:shop:ci".to_owned())
        );
        assert_eq!(item(&list(&admin).await, "ci").unwrap()["cluster"], true);
    }

    #[tokio::test]
    async fn a_role_the_person_does_not_hold_is_greyed_out_with_the_reason() {
        let cluster = signed_in("zeabur:bob");
        cluster.update(|s| {
            s.refused_rights.insert("get secrets".to_owned());
        });
        let listed = list(&cluster).await;
        let developer = listed["choices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["role"] == "developer" && c["cluster"] == false)
            .unwrap();
        assert_eq!(developer["permission"], "denied");
        assert_eq!(developer["refusal"], "notHeld");

        cluster.update(|s| {
            s.refused_rights
                .insert("create serviceaccounts/token".to_owned());
        });
        let listed = list(&cluster).await;
        for choice in listed["choices"].as_array().unwrap() {
            assert_eq!(choice["permission"], "denied", "{choice}");
        }
        assert_eq!(listed["canRenew"], "denied");
    }

    #[tokio::test]
    async fn a_taken_name_and_a_too_short_token_are_refused_before_any_write() {
        let cluster = signed_in("zeabur:ada");
        let (status, body, _) = change(&cluster, create("default", "viewer", false, 3600)).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["summary"]["key"], "apiTokens.taken");
        let (status, body, _) = change(&cluster, create("ci", "viewer", false, 60)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        let (status, body, _) = change(&cluster, create("Not_A_Name", "viewer", false, 3600)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(writes(&cluster), Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_plan_made_before_someone_else_took_the_name_is_not_applied() {
        let cluster = signed_in("zeabur:ada");
        let app = TestApi::new(&cluster).router();
        let (status, plan) = send(
            &app,
            "POST",
            &format!("{BASE}/plan"),
            Some(&create("ci", "viewer", false, 3600)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        cluster.update(|s| {
            s.dynamic.push(object(json!({
                "apiVersion": "v1", "kind": "ServiceAccount",
                "metadata": {"name": "ci", "namespace": "shop", "uid": "uid-ci",
                             "resourceVersion": "1"},
            })));
        });
        let (status, body) = send(
            &app,
            "POST",
            &format!("{BASE}/apply"),
            Some(&json!({"token": plan["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(writes(&cluster), Vec::<String>::new());
    }
}
