//! Members, kept in Zeabur Identity's custom resources.

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::Settings;
    use zeabur_api::members::access::{ProjectRole, baseline_binding, role_binding};
    use zeabur_api::settings::IdentitySettings;
    use zeabur_kube::memory::{MemoryCluster, State};
    use zeabur_kube::{DynamicObject, Identity, ServedResource, WriteBody};

    use crate::common::{app_with, send};

    const NS: &str = "zeabur-identity";

    fn object(kind: &str, name: &str, spec: Value, labels: Value) -> DynamicObject {
        let mut value = json!({
            "apiVersion": "identity.zeabur.com/v1alpha1",
            "kind": kind,
            "metadata": {"name": name, "namespace": NS, "uid": format!("uid-{name}"),
                         "resourceVersion": "1", "creationTimestamp": "2026-10-01T00:00:00Z"},
        });
        value["metadata"]["labels"] = labels;
        value["spec"] = spec;
        serde_json::from_value(value).unwrap()
    }

    fn user(name: &str, admin: bool) -> DynamicObject {
        let groups = if admin { json!(["admins"]) } else { json!([]) };
        object(
            "User",
            name,
            json!({"username": name, "groups": groups, "elevatedGroups": groups}),
            json!({}),
        )
    }

    fn passkey(name: &str, owner: &str) -> DynamicObject {
        object(
            "Credential",
            name,
            json!({"user": owner, "type": "Passkey",
                   "passkey": {"credentialId": name, "record": "{}"}}),
            json!({"identity.zeabur.com/user": owner}),
        )
    }

    fn signed_in(username: &str) -> MemoryCluster {
        let served = [
            "users:User",
            "credentials:Credential",
            "invitations:Invitation",
            "grants:Grant",
        ]
        .into_iter()
        .map(|r| {
            let (plural, kind) = r.split_once(':').unwrap();
            ServedResource::readable("identity.zeabur.com", "v1alpha1", plural, kind, true)
        })
        .chain([
            ServedResource::readable(
                "rbac.authorization.k8s.io",
                "v1",
                "rolebindings",
                "RoleBinding",
                true,
            ),
            ServedResource::readable(
                "rbac.authorization.k8s.io",
                "v1",
                "clusterrolebindings",
                "ClusterRoleBinding",
                false,
            ),
        ])
        .collect();
        let rbac = |mut value: Value| -> DynamicObject {
            // The memory cluster keeps a cluster-scoped object in namespace "".
            if value["metadata"].get("namespace").is_none() {
                value["metadata"]["namespace"] = "".into();
            }
            serde_json::from_value(value).unwrap()
        };
        MemoryCluster::new(State {
            identity: Identity {
                username: username.to_owned(),
                groups: vec![],
            },
            served,
            dynamic: vec![
                user("ada", true),
                user("bob", false),
                passkey("ada-1", "ada"),
                passkey("bob-1", "bob"),
                passkey("bob-2", "bob"),
                object(
                    "Invitation",
                    "inv-bob",
                    json!({"user": "bob", "tokenHash": "sha256:00", "expiresAt": 4_000_000_000_i64}),
                    json!({"identity.zeabur.com/user": "bob"}),
                ),
                object(
                    "Grant",
                    "grant-bob",
                    json!({"user": "bob", "client": "zeabur", "audience": "zeabur",
                           "authTime": 1_790_000_000, "expiresAt": 4_000_000_000_i64}),
                    json!({"identity.zeabur.com/user": "bob"}),
                ),
                rbac(role_binding("shop", "bob", ProjectRole::Viewer)),
                rbac(role_binding("blog", "bob", ProjectRole::Developer)),
                rbac(baseline_binding("bob")),
            ],
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

    async fn call(
        cluster: &MemoryCluster,
        method: &str,
        uri: &str,
        body: Option<&Value>,
    ) -> (StatusCode, Value) {
        send(&app_with(cluster, settings()), method, uri, body).await
    }

    /// Plans `change` and applies the plan on one server: the answer of the step that stopped,
    /// and the plan.
    async fn change_with(
        cluster: &MemoryCluster,
        settings: Settings,
        change: Value,
    ) -> (StatusCode, Value, Value) {
        let app = app_with(cluster, settings);
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

    async fn change(cluster: &MemoryCluster, change: Value) -> (StatusCode, Value, Value) {
        change_with(cluster, settings(), change).await
    }

    /// A plan's writes, as `action Kind/name`.
    fn planned(plan: &Value) -> Vec<String> {
        plan["writes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                format!(
                    "{} {}/{}",
                    w["action"].as_str().unwrap(),
                    w["kind"].as_str().unwrap(),
                    w["name"].as_str().unwrap()
                )
            })
            .collect()
    }

    /// The writes made, as `verb Kind/name`.
    fn writes(cluster: &MemoryCluster) -> Vec<String> {
        cluster
            .inspect(|s| s.writes.clone())
            .unwrap()
            .iter()
            .map(|w| {
                let verb = match w.body {
                    WriteBody::Create(_) => "create",
                    WriteBody::Delete(_) => "delete",
                    WriteBody::JsonPatch(_) | WriteBody::EphemeralContainers(_) => "patch",
                };
                format!("{verb} {}/{}", w.target.kind, w.target.name)
            })
            .collect()
    }

    #[tokio::test]
    async fn members_list_everyone_with_their_passkeys_and_pending_links() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(&cluster, "GET", "/api/v1/members", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["installed"], true);
        let names: Vec<_> = body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["username"].clone())
            .collect();
        assert_eq!(names, [json!("ada"), json!("bob")]);
        assert_eq!(body["items"][0]["role"], "admin");
        assert_eq!(body["items"][0]["you"], true);
        assert_eq!(body["items"][1]["passkeys"].as_array().unwrap().len(), 2);
        assert_eq!(body["items"][1]["sessions"], 1);
        assert_eq!(body["invitations"][0]["name"], "inv-bob");
        assert_eq!(body["setupLinks"]["issuer"], "https://id.example.com");
    }

    #[tokio::test]
    async fn without_zeabur_identity_the_list_says_it_is_not_installed() {
        let cluster = MemoryCluster::new(State::default());
        let (status, body) = call(&cluster, "GET", "/api/v1/members", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["installed"], false);
    }

    #[tokio::test]
    async fn a_person_who_may_not_read_users_is_refused_with_the_rule() {
        let cluster = signed_in("zeabur:bob");
        cluster.update(|s| {
            s.forbidden_in_namespaces
                .insert("users.identity.zeabur.com".to_owned());
        });
        let (status, body) = call(&cluster, "GET", "/api/v1/members", None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(body["denied"]["resource"], "users");
        assert_eq!(body["denied"]["group"], "identity.zeabur.com");
    }

    #[tokio::test]
    async fn a_reset_removes_links_then_passkeys_then_sessions_before_the_new_link() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(
            &cluster,
            "POST",
            "/api/v1/members/bob/setup-links",
            Some(&json!({"reset": true})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["reset"], true);
        let made = writes(&cluster);
        assert_eq!(
            made[..4],
            [
                "delete Invitation/inv-bob",
                "delete Credential/bob-1",
                "delete Credential/bob-2",
                "delete Grant/grant-bob"
            ]
        );
        assert!(made[4].starts_with("create Invitation/inv-"));
    }

    #[tokio::test]
    async fn a_link_without_reset_keeps_the_passkeys_and_nobody_resets_themselves() {
        let cluster = signed_in("zeabur:ada");
        let (status, _) = call(
            &cluster,
            "POST",
            "/api/v1/members/bob/setup-links",
            Some(&json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(writes(&cluster).len(), 1);
        let (status, body) = call(
            &cluster,
            "POST",
            "/api/v1/members/ada/setup-links",
            Some(&json!({"reset": true})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["summary"]["key"], "members.resetSelf");
        assert_eq!(body["fixKey"]["key"], "members.resetSelf.fix");
    }

    #[tokio::test]
    async fn your_last_passkey_is_refused_and_another_is_removed() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(
            &cluster,
            "DELETE",
            "/api/v1/members/ada/credentials/ada-1",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["summary"]["key"], "account.lastPasskey");
        assert_eq!(body["fixKey"]["key"], "account.lastPasskey.fix");
        assert_eq!(writes(&cluster).len(), 0);
        let (status, body) = call(
            &cluster,
            "DELETE",
            "/api/v1/members/bob/credentials/bob-1",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        assert_eq!(writes(&cluster), ["delete Credential/bob-1"]);
        let (status, _) = call(
            &cluster,
            "DELETE",
            "/api/v1/members/bob/credentials/ada-1",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn revoking_a_link_deletes_it_and_one_already_gone_is_fine() {
        let cluster = signed_in("zeabur:ada");
        let (status, _) = call(
            &cluster,
            "DELETE",
            "/api/v1/members/invitations/inv-bob",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = call(
            &cluster,
            "DELETE",
            "/api/v1/members/invitations/inv-bob",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(writes(&cluster), ["delete Invitation/inv-bob"]);
    }

    /// The bindings Zeabur holds for `username`, as `namespace/role`.
    fn bindings(cluster: &MemoryCluster, username: &str) -> Vec<String> {
        let mut found: Vec<String> = cluster
            .inspect(|s| s.dynamic.clone())
            .unwrap()
            .iter()
            .filter(|o| {
                o.metadata
                    .labels
                    .as_ref()
                    .and_then(|l| l.get("zeabur.com/member"))
                    .is_some_and(|u| u == username)
            })
            .map(|o| {
                format!(
                    "{}/{}",
                    o.metadata.namespace.as_deref().unwrap_or_default(),
                    o.data["roleRef"]["name"].as_str().unwrap()
                )
            })
            .collect();
        found.sort();
        found
    }

    #[tokio::test]
    async fn members_show_their_projects_or_name_the_bindings_that_could_not_be_read() {
        let cluster = signed_in("zeabur:ada");
        let (_, body) = call(&cluster, "GET", "/api/v1/members", None).await;
        assert_eq!(body["projectRoles"], true);
        assert_eq!(body["items"][0].get("projects"), None);
        assert_eq!(
            body["items"][1]["projects"],
            json!([{"project": "blog", "role": "developer"}, {"project": "shop", "role": "viewer"}])
        );
        cluster.update(|s| {
            s.forbidden_resources
                .insert("rolebindings.rbac.authorization.k8s.io".to_owned());
        });
        let (status, body) = call(&cluster, "GET", "/api/v1/members", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["items"][1].get("projects"), None);
        assert_eq!(
            body["unreadable"][0]["resource"],
            "rolebindings.rbac.authorization.k8s.io"
        );
    }

    fn dynamic(cluster: &MemoryCluster, namespace: &str, name: &str) -> DynamicObject {
        cluster
            .inspect(|s| s.dynamic.clone())
            .unwrap()
            .into_iter()
            .find(|o| {
                o.metadata.namespace.as_deref() == Some(namespace)
                    && o.metadata.name.as_deref() == Some(name)
            })
            .unwrap()
    }

    #[tokio::test]
    async fn an_invitation_is_planned_without_writing_and_its_link_is_made_only_at_apply() {
        let cluster = signed_in("zeabur:ada");
        let (status, plan) = call(
            &cluster,
            "POST",
            "/api/v1/members/plan",
            Some(&json!({"op": "invite", "username": "cy", "displayName": "Cy", "admin": true})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        assert!(plan["token"].as_str().unwrap().starts_with("m1."));
        assert_eq!(plan["newMember"], true);
        assert_eq!(plan["linkSeconds"], 7 * 24 * 3600);
        assert_eq!(
            planned(&plan),
            [
                "create User/cy",
                "create ClusterRoleBinding/zeabur-member-cy",
                "create Invitation/inv-"
            ]
        );
        assert!(
            plan["writes"][0]["afterYaml"]
                .as_str()
                .unwrap()
                .contains("- admins")
        );
        assert_eq!(writes(&cluster).len(), 0);

        let (status, applied, _) = change(
            &cluster,
            json!({"op": "invite", "username": "cy", "displayName": "Cy", "admin": true}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        let link = &applied["link"];
        assert_eq!(link["createdMember"], true);
        let url = link["url"].as_str().unwrap();
        let token = url
            .strip_prefix("https://id.example.com/setup#token=")
            .unwrap();
        let name = link["name"].as_str().unwrap();
        assert!(token.starts_with(&format!("{name}.")));
        assert_eq!(
            writes(&cluster),
            [
                "create User/cy".to_owned(),
                "create ClusterRoleBinding/zeabur-member-cy".to_owned(),
                format!("create Invitation/{name}")
            ]
        );
        let invitation = dynamic(&cluster, NS, name);
        let stored = serde_json::to_string(&invitation).unwrap();
        assert!(!stored.contains(token.split_once('.').unwrap().1));
        assert_eq!(invitation.data["spec"]["user"], "cy");
        assert_eq!(
            dynamic(&cluster, NS, "cy").data["spec"]["elevatedGroups"],
            json!(["admins"])
        );
    }

    #[tokio::test]
    async fn inviting_an_existing_member_adds_a_link_and_a_bad_name_is_refused() {
        let cluster = signed_in("zeabur:ada");
        let (status, applied, plan) =
            change(&cluster, json!({"op": "invite", "username": "bob"})).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(plan["newMember"], false);
        assert_eq!(planned(&plan), ["create Invitation/inv-"]);
        assert_eq!(applied["link"]["createdMember"], false);
        assert_eq!(writes(&cluster).len(), 1);
        let (status, ..) = change(&cluster, json!({"op": "invite", "username": "Bob Smith"})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn removing_a_member_removes_everything_of_theirs_then_them_but_never_yourself() {
        let cluster = signed_in("zeabur:ada");
        let (status, applied, plan) =
            change(&cluster, json!({"op": "remove", "username": "bob"})).await;
        assert_eq!(status, StatusCode::OK, "{applied}");
        assert_eq!(applied.get("link"), None);
        let made = writes(&cluster);
        assert_eq!(
            made[..3],
            [
                "delete RoleBinding/zeabur-member-bob",
                "delete RoleBinding/zeabur-member-bob",
                "delete ClusterRoleBinding/zeabur-member-bob"
            ]
        );
        assert_eq!(made.last().unwrap(), "delete User/bob");
        assert_eq!(made.len(), 8);
        assert_eq!(planned(&plan).len(), 8);
        assert_eq!(plan["writes"][0]["namespace"], "blog");
        assert_eq!(plan["writes"][2].get("namespace"), None);
        let credential = plan["writes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["kind"] == "Credential")
            .unwrap();
        assert_eq!(
            credential.get("beforeYaml"),
            None,
            "a passkey's record is not shown"
        );
        assert_eq!(bindings(&cluster, "bob").len(), 0);
        let (status, body, _) = change(&cluster, json!({"op": "remove", "username": "ada"})).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["summary"]["key"], "members.removeSelf");
        let (status, ..) = change(&cluster, json!({"op": "remove", "username": "nobody"})).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn without_an_issuer_no_link_is_planned() {
        let cluster = signed_in("zeabur:ada");
        let (status, body, _) = change_with(
            &cluster,
            Settings::default(),
            json!({"op": "invite", "username": "cy"}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["summary"]["key"], "members.noIssuer");
        assert_eq!(body["fixKey"]["key"], "members.noIssuer.fix");
        assert_eq!(writes(&cluster).len(), 0);
    }

    #[tokio::test]
    async fn a_new_member_gets_the_baseline_and_a_binding_per_project_before_the_link() {
        let cluster = signed_in("zeabur:ada");
        let (status, body, plan) = change(
            &cluster,
            json!({"op": "invite", "username": "cy", "projects": [
                {"project": "shop", "role": "developer"},
                {"project": "blog", "role": "viewer"}
            ]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            planned(&plan),
            [
                "create User/cy",
                "create ClusterRoleBinding/zeabur-member-cy",
                "create RoleBinding/zeabur-member-cy",
                "create RoleBinding/zeabur-member-cy",
                "create Invitation/inv-",
            ]
        );
        let made = writes(&cluster);
        assert_eq!(
            made[..4],
            [
                "create User/cy",
                "create ClusterRoleBinding/zeabur-member-cy",
                "create RoleBinding/zeabur-member-cy",
                "create RoleBinding/zeabur-member-cy",
            ]
        );
        assert!(made[4].starts_with("create Invitation/"));
        assert_eq!(
            bindings(&cluster, "cy"),
            [
                "/zeabur-member",
                "blog/zeabur-project-viewer",
                "shop/zeabur-project-developer"
            ]
        );
        assert_eq!(
            dynamic(&cluster, "shop", "zeabur-member-cy").data["subjects"],
            json!([{"kind": "User", "apiGroup": "rbac.authorization.k8s.io", "name": "zeabur:cy"}])
        );
    }

    #[tokio::test]
    async fn an_admin_with_projects_or_a_project_twice_is_refused() {
        let cluster = signed_in("zeabur:ada");
        for body in [
            json!({"op": "invite", "username": "cy", "admin": true,
                   "projects": [{"project": "shop", "role": "viewer"}]}),
            json!({"op": "invite", "username": "cy", "projects": [
                {"project": "shop", "role": "viewer"}, {"project": "shop", "role": "developer"}]}),
            json!({"op": "invite", "username": "cy",
                   "projects": [{"project": "Shop", "role": "viewer"}]}),
        ] {
            let (status, answer, _) = change(&cluster, body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
        }
        assert_eq!(writes(&cluster).len(), 0);
    }

    #[tokio::test]
    async fn changing_a_role_replaces_its_binding_and_a_project_left_out_loses_it() {
        let cluster = signed_in("zeabur:ada");
        let (status, body, plan) = change(
            &cluster,
            json!({"op": "access", "username": "bob", "admin": false, "projects": [
                {"project": "shop", "role": "developer"},
                {"project": "billing", "role": "viewer"}
            ]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            bindings(&cluster, "bob"),
            [
                "/zeabur-member",
                "billing/zeabur-project-viewer",
                "shop/zeabur-project-developer"
            ]
        );
        let expected = [
            "delete RoleBinding/zeabur-member-bob",
            "delete RoleBinding/zeabur-member-bob",
            "create RoleBinding/zeabur-member-bob",
            "create RoleBinding/zeabur-member-bob",
        ];
        assert_eq!(writes(&cluster), expected);
        assert_eq!(planned(&plan), expected);
        assert!(
            plan["writes"][0]["beforeYaml"]
                .as_str()
                .unwrap()
                .contains("zeabur-project-developer")
        );
    }

    #[tokio::test]
    async fn an_admin_loses_their_project_bindings_and_nobody_demotes_themselves() {
        let cluster = signed_in("zeabur:ada");
        let (status, body, plan) = change(
            &cluster,
            json!({"op": "access", "username": "bob", "admin": true}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(bindings(&cluster, "bob"), ["/zeabur-member"]);
        assert_eq!(writes(&cluster).last().unwrap(), "patch User/bob");
        assert_eq!(planned(&plan).last().unwrap(), "patch User/bob");
        assert_eq!(
            dynamic(&cluster, NS, "bob").data["spec"]["elevatedGroups"],
            json!(["admins"])
        );
        let (status, body, _) = change(
            &cluster,
            json!({"op": "access", "username": "ada", "admin": false}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["summary"]["key"], "members.demoteSelf");
    }

    #[tokio::test]
    async fn without_the_member_roles_projects_are_refused() {
        let cluster = signed_in("zeabur:ada");
        let off = Settings {
            identity: IdentitySettings {
                issuer: Some("https://id.example.com/".to_owned()),
                ..IdentitySettings::default()
            },
            ..Settings::default()
        };
        let (status, body, _) = change_with(
            &cluster,
            off,
            json!({"op": "access", "username": "bob", "admin": false,
                   "projects": [{"project": "shop", "role": "viewer"}]}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["summary"]["key"], "members.noProjectRoles");
        assert_eq!(writes(&cluster).len(), 0);
    }

    #[tokio::test]
    async fn a_refused_binding_read_stops_the_plan() {
        let cluster = signed_in("zeabur:ada");
        cluster.update(|s| {
            s.forbidden_resources
                .insert("rolebindings.rbac.authorization.k8s.io".to_owned());
        });
        let (status, body, _) = change(
            &cluster,
            json!({"op": "access", "username": "bob", "admin": false, "projects": []}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(body["denied"]["resource"], "rolebindings");
        assert_eq!(writes(&cluster).len(), 0);
    }

    #[tokio::test]
    async fn a_plan_is_refused_once_the_member_changed_and_a_project_token_never_applies() {
        let cluster = signed_in("zeabur:ada");
        let app = app_with(&cluster, settings());
        let (status, plan) = send(
            &app,
            "POST",
            "/api/v1/members/plan",
            Some(&json!({"op": "access", "username": "bob", "admin": true})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{plan}");
        cluster.update(|s| {
            if let Some(bob) = s
                .dynamic
                .iter_mut()
                .find(|o| o.metadata.name.as_deref() == Some("bob"))
            {
                bob.metadata.resource_version = Some("2".to_owned());
            }
        });
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/members/apply",
            Some(&json!({"token": plan["token"]})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(writes(&cluster).len(), 0);
        let sealed = plan["token"].as_str().unwrap().strip_prefix("m1.").unwrap();
        let project_token = format!("v2.{sealed}");
        let (status, _) = send(
            &app,
            "POST",
            "/api/v1/members/apply",
            Some(&json!({"token": project_token})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
