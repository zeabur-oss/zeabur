//! Changing Zeabur Identity's people: setup links and removals.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::crypto::{random, sha256};
use zeabur_core::message::Message;
use zeabur_core::problem::{Kind, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ObjectTarget, Preconditions, WriteBody, WriteRequest};

use super::directory::{Directory, IdentityKind, Object, UserSpec, request};
use super::{ADMIN_GROUP, API_VERSION, EXPIRES_AT_ANNOTATION, EXPIRY_BUCKET_LABEL, USER_LABEL};
use crate::error::{ApiError, read_problem};

/// How long a link made for a new member lasts, as `zeabur-identityctl invite`'s default.
pub(crate) const INVITE_SECONDS: i64 = 7 * 24 * 3600;
/// How long a reset link lasts: long enough to reach someone, short enough to matter little if
/// it leaks.
pub(crate) const RESET_SECONDS: i64 = 24 * 3600;

/// Someone to invite.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InviteRequest {
    /// Login name: lower-case letters, digits, `-` and `.`, at most 63 characters. An existing
    /// member gets a link that adds a passkey; their name and role stay.
    pub username: String,
    /// Name shown in the interface, for a new member.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Whether a new member may take on admin rights (groups and elevated groups `admins`).
    #[serde(default)]
    pub admin: bool,
}

/// A member's setup link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetupLinkRequest {
    /// Also remove their passkeys, linked accounts, sessions and other links first, so only
    /// whoever opens this link gets in.
    #[serde(default)]
    pub reset: bool,
}

/// A one-time setup link, shown once: only its hash is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupLinkView {
    /// The link; whoever opens it first adds a passkey to the account.
    pub url: String,
    /// Whose account it adds a passkey to.
    pub username: String,
    /// When it stops working, RFC 3339.
    pub expires_at: String,
    /// The `Invitation` object's name, to revoke it.
    pub name: String,
    /// Whether the member was created now.
    pub created_member: bool,
    /// Whether their passkeys, sessions and other links were removed first.
    pub reset: bool,
}

/// Whether `name` can be a Zeabur Identity user name: a lower-case DNS subdomain of at most 63
/// characters, as its `User` schema requires.
#[must_use]
pub(crate) fn is_username(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
}

pub(crate) fn bad(detail: impl Into<String>) -> ApiError {
    ApiError::from(Problem::new(Kind::BadRequest, detail))
}

/// A change Zeabur refuses itself: the summary is the sentence `key` and the fix `key.fix`, so the
/// interface words both.
pub(crate) fn refuse(detail: &str, fix: &str, key: &str) -> ApiError {
    let mut problem = Problem::new(Kind::Conflict, detail);
    problem.summary = Message::new(key);
    problem.fix_for_occasion(fix, Message::new(&format!("{key}.fix")));
    ApiError::from(problem)
}

pub(crate) fn target(kind: IdentityKind, namespace: &str, name: &str) -> ObjectTarget {
    ObjectTarget {
        api_version: API_VERSION.to_owned(),
        kind: kind.kind().to_owned(),
        plural: kind.plural().to_owned(),
        namespace: namespace.to_owned(),
        name: name.to_owned(),
    }
}

async fn send(
    cluster: &dyn Cluster,
    kind: IdentityKind,
    namespace: &str,
    name: &str,
    body: WriteBody,
) -> Result<(), ApiError> {
    let verb = match &body {
        WriteBody::Create(_) => "create",
        WriteBody::Delete(_) => "delete",
        WriteBody::JsonPatch(_) | WriteBody::EphemeralContainers(_) => "patch",
    };
    let write = WriteRequest {
        target: target(kind, namespace, name),
        body,
        dry_run: false,
    };
    cluster.write(&write).await.map(|_| ()).map_err(|error| {
        let what = format!("{} {name} in {namespace}", kind.kind());
        let named = (verb != "create").then_some(name);
        read_problem(error, &what, request(verb, kind, namespace, named))
    })
}

/// Deletes one object that is still the one read (its `uid`); one already gone is fine.
pub(crate) async fn delete<S: Sync>(
    cluster: &dyn Cluster,
    kind: IdentityKind,
    namespace: &str,
    object: &Object<S>,
) -> Result<(), ApiError> {
    let body = WriteBody::Delete(Preconditions {
        uid: object.metadata.uid.clone(),
        resource_version: None,
    });
    match send(cluster, kind, namespace, object.name(), body).await {
        Err(error) if error.0.kind == Kind::NotFound => Ok(()),
        other => other,
    }
}

/// Deletes `username`'s invitations, then credentials, then grants: invitations first, since
/// each could enrol a passkey into the account.
pub(crate) async fn clear(
    cluster: &dyn Cluster,
    namespace: &str,
    directory: &Directory,
    username: &str,
) -> Result<(), ApiError> {
    for invitation in directory
        .invitations
        .iter()
        .filter(|i| i.spec.user == username)
    {
        delete(cluster, IdentityKind::Invitation, namespace, invitation).await?;
    }
    for credential in directory
        .credentials
        .iter()
        .filter(|c| c.spec.user == username)
    {
        delete(cluster, IdentityKind::Credential, namespace, credential).await?;
    }
    for grant in directory.grants.iter().filter(|g| g.spec.user == username) {
        delete(cluster, IdentityKind::Grant, namespace, grant).await?;
    }
    Ok(())
}

/// The `User` for a new member.
pub(crate) fn new_user(namespace: &str, invite: &InviteRequest) -> Value {
    let groups: Vec<&str> = if invite.admin {
        vec![ADMIN_GROUP]
    } else {
        Vec::new()
    };
    let mut spec = json!({
        "username": invite.username,
        "groups": groups,
        "elevatedGroups": groups,
    });
    if let Some(name) = invite
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        && let Some(spec) = spec.as_object_mut()
    {
        spec.insert(
            "displayName".to_owned(),
            json!(name.chars().take(128).collect::<String>()),
        );
    }
    json!({
        "apiVersion": API_VERSION,
        "kind": "User",
        "metadata": {"name": invite.username, "namespace": namespace,
                     "labels": {"app.kubernetes.io/managed-by": "zeabur"}},
        "spec": spec,
    })
}

/// A fresh token, `<name>.<secret>`.
fn token(name: &str) -> Result<String, ApiError> {
    let secret = random::<32>().map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a random token.",
        ))
    })?;
    Ok(format!("{name}.{}", URL_SAFE_NO_PAD.encode(secret)))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| [b >> 4, b & 0xF])
        .map(|d| char::from(DIGITS.get(usize::from(d)).copied().unwrap_or(b'0')))
        .collect()
}

/// `sha256:<hex>` of a token, as Zeabur Identity compares it.
#[must_use]
pub(crate) fn token_hash(token: &str) -> String {
    format!("sha256:{}", hex(&sha256(token.as_bytes())))
}

/// The link that redeems `token`: the token in the fragment, which browsers send to no server.
#[must_use]
pub(crate) fn link(issuer: &str, token: &str) -> String {
    format!("{}/setup#token={token}", issuer.trim_end_matches('/'))
}

/// The `Invitation` object for `token`, owned by the `User` so it goes with them.
pub(crate) fn invitation(
    namespace: &str,
    name: &str,
    user: &Value,
    token: &str,
    expires_at: i64,
) -> Value {
    let username = user
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut metadata = json!({
        "name": name,
        "namespace": namespace,
        "labels": {
            "app.kubernetes.io/managed-by": "zeabur",
            USER_LABEL: username,
            EXPIRY_BUCKET_LABEL: (expires_at / 3600).to_string(),
        },
        "annotations": {EXPIRES_AT_ANNOTATION: expires_at.to_string()},
    });
    if let Some(uid) = user.pointer("/metadata/uid").and_then(Value::as_str)
        && let Some(metadata) = metadata.as_object_mut()
    {
        metadata.insert(
            "ownerReferences".to_owned(),
            json!([{"apiVersion": API_VERSION, "kind": "User", "name": username, "uid": uid}]),
        );
    }
    json!({
        "apiVersion": API_VERSION,
        "kind": "Invitation",
        "metadata": metadata,
        "spec": {"user": username, "tokenHash": token_hash(token), "expiresAt": expires_at},
    })
}

/// Makes an invitation for the `User` `user` (as read, with its `uid`) lasting `seconds` from
/// `now`, and returns its link.
pub(crate) async fn issue(
    cluster: &dyn Cluster,
    namespace: &str,
    issuer: &str,
    user: &Value,
    now: Timestamp,
    seconds: i64,
) -> Result<(String, String, i64), ApiError> {
    let suffix = random::<8>().map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "Zeabur could not make a random name.",
        ))
    })?;
    let name = format!("inv-{}", hex(&suffix));
    let token = token(&name)?;
    let expires_at = now.as_second().saturating_add(seconds);
    let object = invitation(namespace, &name, user, &token, expires_at);
    send(
        cluster,
        IdentityKind::Invitation,
        namespace,
        &name,
        WriteBody::Create(object),
    )
    .await?;
    Ok((link(issuer, &token), name, expires_at))
}

/// Reads `username`'s `User` as JSON, for its `uid`; `None` when there is none.
pub(crate) async fn user_object(
    cluster: &dyn Cluster,
    namespace: &str,
    username: &str,
) -> Result<Option<Value>, ApiError> {
    let resource = IdentityKind::User.resource();
    match cluster
        .get_object(&resource, Some(namespace), username)
        .await
    {
        Ok(found) => Ok(found.and_then(|o| serde_json::to_value(o).ok())),
        Err(error) if error.code() == Some(404) => Ok(None),
        Err(error) => Err(read_problem(
            error,
            &format!("User {username} in {namespace}"),
            request("get", IdentityKind::User, namespace, Some(username)),
        )),
    }
}

/// Creates the `User` for a new member and returns it as stored.
pub(crate) async fn create_user(
    cluster: &dyn Cluster,
    namespace: &str,
    invite: &InviteRequest,
) -> Result<Value, ApiError> {
    send(
        cluster,
        IdentityKind::User,
        namespace,
        &invite.username,
        WriteBody::Create(new_user(namespace, invite)),
    )
    .await?;
    user_object(cluster, namespace, &invite.username)
        .await?
        .ok_or_else(|| bad("The new member could not be read back."))
}

/// Deletes the member `user`: their invitations, credentials and grants, then the `User`.
pub(crate) async fn remove_member(
    cluster: &dyn Cluster,
    namespace: &str,
    directory: &Directory,
    user: &Object<UserSpec>,
) -> Result<(), ApiError> {
    clear(cluster, namespace, directory, user.name()).await?;
    delete(cluster, IdentityKind::User, namespace, user).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_names_follow_the_user_schema() {
        for good in ["ada", "ada.lovelace", "a-1", "a", &"a".repeat(63)] {
            assert!(is_username(good), "{good}");
        }
        for bad in [
            "",
            "Ada",
            "ada_l",
            "-ada",
            "ada-",
            "ada..l",
            ".ada",
            "zeabur:ada",
            &"a".repeat(64),
        ] {
            assert!(!is_username(bad), "{bad}");
        }
    }

    #[test]
    fn a_link_carries_its_token_in_the_fragment_and_only_the_hash_is_stored() {
        let user = json!({"metadata": {"name": "ada", "uid": "u-1"}});
        let token = token("inv-1").unwrap();
        let (name, secret) = token.split_once('.').unwrap();
        assert_eq!(name, "inv-1");
        assert_eq!(secret.len(), 43);
        let object = invitation("zeabur-identity", "inv-1", &user, &token, 7200);
        assert_eq!(object["spec"]["user"], "ada");
        assert_eq!(object["spec"]["expiresAt"], 7200);
        assert_eq!(object["metadata"]["labels"][USER_LABEL], "ada");
        assert_eq!(object["metadata"]["labels"][EXPIRY_BUCKET_LABEL], "2");
        assert_eq!(object["metadata"]["ownerReferences"][0]["uid"], "u-1");
        let hash = object["spec"]["tokenHash"].as_str().unwrap();
        assert_eq!(hash, token_hash(&token));
        assert!(!object.to_string().contains(secret));
        assert_eq!(
            link("https://id.example.com/", &token),
            format!("https://id.example.com/setup#token={token}")
        );
    }

    #[test]
    fn the_token_hash_is_identitys_sha256() {
        // printf 'abc' | sha256sum
        assert_eq!(
            token_hash("abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_new_admin_gets_the_admins_groups_and_a_member_none() {
        let invite = |admin| InviteRequest {
            username: "cy".to_owned(),
            display_name: Some(" Cy ".to_owned()),
            admin,
        };
        let admin = new_user("ns", &invite(true));
        assert_eq!(admin["spec"]["groups"], json!(["admins"]));
        assert_eq!(admin["spec"]["elevatedGroups"], json!(["admins"]));
        assert_eq!(admin["spec"]["displayName"], "Cy");
        let member = new_user("ns", &invite(false));
        assert_eq!(member["spec"]["groups"], json!([]));
        assert_eq!(member["spec"]["elevatedGroups"], json!([]));
    }
}
