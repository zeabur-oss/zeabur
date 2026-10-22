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
    async fn inviting_a_new_name_makes_the_member_and_a_link_whose_token_is_not_stored() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(
            &cluster,
            "POST",
            "/api/v1/members/invitations",
            Some(&json!({"username": "cy", "displayName": "Cy", "admin": true})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["createdMember"], true);
        let url = body["url"].as_str().unwrap();
        let token = url
            .strip_prefix("https://id.example.com/setup#token=")
            .unwrap();
        let name = body["name"].as_str().unwrap();
        assert!(token.starts_with(&format!("{name}.")));
        assert_eq!(
            writes(&cluster),
            [
                "create User/cy".to_owned(),
                "create ClusterRoleBinding/zeabur-member-cy".to_owned(),
                format!("create Invitation/{name}")
            ]
        );
        let dynamic = cluster.inspect(|s| s.dynamic.clone()).unwrap();
        let invitation = dynamic
            .iter()
            .find(|o| o.metadata.name.as_deref() == Some(name))
            .unwrap();
        let stored = serde_json::to_string(invitation).unwrap();
        assert!(!stored.contains(token.split_once('.').unwrap().1));
        assert_eq!(invitation.data["spec"]["user"], "cy");
        let cy = dynamic
            .iter()
            .find(|o| o.metadata.name.as_deref() == Some("cy"))
            .unwrap();
        assert_eq!(cy.data["spec"]["elevatedGroups"], json!(["admins"]));
    }

    #[tokio::test]
    async fn inviting_an_existing_member_adds_a_link_and_a_bad_name_is_refused() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(
            &cluster,
            "POST",
            "/api/v1/members/invitations",
            Some(&json!({"username": "bob"})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["createdMember"], false);
        assert_eq!(writes(&cluster).len(), 1);
        let (status, _) = call(
            &cluster,
            "POST",
            "/api/v1/members/invitations",
            Some(&json!({"username": "Bob Smith"})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
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
    async fn removing_a_member_removes_everything_of_theirs_then_them_but_never_yourself() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(&cluster, "DELETE", "/api/v1/members/bob", None).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
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
        assert_eq!(bindings(&cluster, "bob").len(), 0);
        let (status, body) = call(&cluster, "DELETE", "/api/v1/members/ada", None).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        let (status, _) = call(&cluster, "DELETE", "/api/v1/members/nobody", None).await;
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

    #[tokio::test]
    async fn without_an_issuer_no_link_is_made() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = send(
            &app_with(&cluster, Settings::default()),
            "POST",
            "/api/v1/members/invitations",
            Some(&json!({"username": "cy"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["summary"]["key"], "members.noIssuer");
        assert_eq!(body["fixKey"]["key"], "members.noIssuer.fix");
        assert_eq!(writes(&cluster).len(), 0);
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

    #[tokio::test]
    async fn a_new_member_gets_the_baseline_and_a_binding_per_project_before_the_link() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(
            &cluster,
            "POST",
            "/api/v1/members/invitations",
            Some(&json!({"username": "cy", "projects": [
                {"project": "shop", "role": "developer"},
                {"project": "blog", "role": "viewer"}
            ]})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
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
        let shop = cluster
            .inspect(|s| s.dynamic.clone())
            .unwrap()
            .into_iter()
            .find(|o| {
                o.metadata.namespace.as_deref() == Some("shop")
                    && o.metadata.name.as_deref() == Some("zeabur-member-cy")
            })
            .unwrap();
        assert_eq!(
            shop.data["subjects"],
            json!([{"kind": "User", "apiGroup": "rbac.authorization.k8s.io", "name": "zeabur:cy"}])
        );
    }

    #[tokio::test]
    async fn an_admin_with_projects_or_a_project_twice_is_refused() {
        let cluster = signed_in("zeabur:ada");
        for body in [
            json!({"username": "cy", "admin": true,
                   "projects": [{"project": "shop", "role": "viewer"}]}),
            json!({"username": "cy", "projects": [
                {"project": "shop", "role": "viewer"}, {"project": "shop", "role": "developer"}]}),
            json!({"username": "cy", "projects": [{"project": "Shop", "role": "viewer"}]}),
        ] {
            let (status, answer) =
                call(&cluster, "POST", "/api/v1/members/invitations", Some(&body)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
        }
        assert_eq!(writes(&cluster).len(), 0);
    }

    #[tokio::test]
    async fn changing_a_role_replaces_its_binding_and_a_project_left_out_loses_it() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(
            &cluster,
            "PUT",
            "/api/v1/members/bob/access",
            Some(&json!({"admin": false, "projects": [
                {"project": "shop", "role": "developer"},
                {"project": "billing", "role": "viewer"}
            ]})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        assert_eq!(
            bindings(&cluster, "bob"),
            [
                "/zeabur-member",
                "billing/zeabur-project-viewer",
                "shop/zeabur-project-developer"
            ]
        );
        assert_eq!(
            writes(&cluster),
            [
                "delete RoleBinding/zeabur-member-bob",
                "delete RoleBinding/zeabur-member-bob",
                "create RoleBinding/zeabur-member-bob",
                "create RoleBinding/zeabur-member-bob",
            ]
        );
    }

    #[tokio::test]
    async fn an_admin_loses_their_project_bindings_and_nobody_demotes_themselves() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(
            &cluster,
            "PUT",
            "/api/v1/members/bob/access",
            Some(&json!({"admin": true})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        assert_eq!(bindings(&cluster, "bob"), ["/zeabur-member"]);
        assert_eq!(writes(&cluster).last().unwrap(), "patch User/bob");
        let bob = cluster
            .inspect(|s| s.dynamic.clone())
            .unwrap()
            .into_iter()
            .find(|o| o.metadata.name.as_deref() == Some("bob"))
            .unwrap();
        assert_eq!(bob.data["spec"]["elevatedGroups"], json!(["admins"]));
        let (status, body) = call(
            &cluster,
            "PUT",
            "/api/v1/members/ada/access",
            Some(&json!({"admin": false})),
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
        let (status, body) = send(
            &app_with(&cluster, off),
            "PUT",
            "/api/v1/members/bob/access",
            Some(&json!({"admin": false, "projects": [{"project": "shop", "role": "viewer"}]})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["summary"]["key"], "members.noProjectRoles");
        assert_eq!(writes(&cluster).len(), 0);
    }

    #[tokio::test]
    async fn a_refused_binding_read_stops_the_change_before_any_write() {
        let cluster = signed_in("zeabur:ada");
        cluster.update(|s| {
            s.forbidden_resources
                .insert("rolebindings.rbac.authorization.k8s.io".to_owned());
        });
        let (status, body) = call(
            &cluster,
            "PUT",
            "/api/v1/members/bob/access",
            Some(&json!({"admin": false, "projects": []})),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(body["denied"]["resource"], "rolebindings");
        assert_eq!(writes(&cluster).len(), 0);
    }
}
