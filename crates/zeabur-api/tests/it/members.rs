//! Members and your account, kept in Zeabur Identity's custom resources.

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{Value, json};
    use zeabur_api::Settings;
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
                         "creationTimestamp": "2026-10-01T00:00:00Z"},
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
        .collect();
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
            ],
            ..State::default()
        })
    }

    fn settings() -> Settings {
        Settings {
            identity: IdentitySettings {
                issuer: Some("https://id.example.com/".to_owned()),
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
        assert_eq!(body["fixKey"]["key"], "members.fix.resetSelf");
    }

    #[tokio::test]
    async fn your_last_passkey_is_refused_and_another_is_removed() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(&cluster, "DELETE", "/api/v1/account/passkeys/ada-1", None).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["fixKey"]["key"], "account.fix.lastPasskey");
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
        let bob = signed_in("zeabur:bob");
        let (status, body) = call(&bob, "DELETE", "/api/v1/account/passkeys/bob-2", None).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        let (status, _) = call(&bob, "DELETE", "/api/v1/account/passkeys/ada-1", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn your_account_has_your_passkeys_and_renaming_one_keeps_its_name_in_an_annotation() {
        let cluster = signed_in("zeabur:bob");
        let (status, body) = call(&cluster, "GET", "/api/v1/account", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["status"], "identity");
        assert_eq!(body["member"]["username"], "bob");
        let (status, body) = call(
            &cluster,
            "PATCH",
            "/api/v1/account/passkeys/bob-1",
            Some(&json!({"label": " Phone "})),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        let (_, body) = call(&cluster, "GET", "/api/v1/account", None).await;
        assert_eq!(body["member"]["passkeys"][0]["label"], "Phone");
        let (status, _) = call(
            &cluster,
            "PATCH",
            "/api/v1/account/passkeys/bob-1",
            Some(&json!({"label": "x".repeat(65)})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn an_account_outside_zeabur_identity_says_so() {
        let cluster = signed_in("kubernetes-admin");
        let (status, body) = call(&cluster, "GET", "/api/v1/account", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["status"], "kubernetes");
        assert_eq!(body["kubernetesUsername"], "kubernetes-admin");
        let cluster = cluster_missing();
        let (_, body) = call(&cluster, "GET", "/api/v1/account", None).await;
        assert_eq!(body["status"], "missing");
    }

    fn cluster_missing() -> MemoryCluster {
        signed_in("zeabur:nobody")
    }

    #[tokio::test]
    async fn your_own_link_lasts_an_hour() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(&cluster, "POST", "/api/v1/account/setup-links", None).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["username"], "ada");
        assert!(
            body["url"]
                .as_str()
                .unwrap()
                .starts_with("https://id.example.com/setup#token=inv-")
        );
    }

    #[tokio::test]
    async fn removing_a_member_removes_everything_of_theirs_then_them_but_never_yourself() {
        let cluster = signed_in("zeabur:ada");
        let (status, body) = call(&cluster, "DELETE", "/api/v1/members/bob", None).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        assert_eq!(writes(&cluster).last().unwrap(), "delete User/bob");
        assert_eq!(writes(&cluster).len(), 5);
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
        assert_eq!(body["fixKey"]["key"], "members.fix.noIssuer");
        assert_eq!(writes(&cluster).len(), 0);
    }
}
