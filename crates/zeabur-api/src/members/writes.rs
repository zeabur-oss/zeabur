//! Changing Zeabur Identity's people: setup links and removals.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_core::crypto::{random, sha256};
use zeabur_core::message::Message;
use zeabur_core::problem::{Kind, Problem};
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use zeabur_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};
use zeabur_kube::{Cluster, ObjectTarget, Preconditions, WriteBody, WriteRequest};

use super::directory::{
    AccountType, Directory, IdentityKind, InvitationSpec, Object, Read, UserSpec, read, read_error,
    request,
};
use super::{API_VERSION, EXPIRES_AT_ANNOTATION, EXPIRY_BUCKET_LABEL, USER_LABEL};
use crate::error::{ApiError, read_problem, to_json};

/// How long a link made for a new member lasts, as `zeabur-identityctl invite`'s default.
pub(crate) const INVITE_SECONDS: i64 = 7 * 24 * 3600;
/// How long a reset link lasts: long enough to reach someone, short enough to matter little if
/// it leaks.
pub(crate) const RESET_SECONDS: i64 = 24 * 3600;

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
    /// When it stops working.
    pub expires_at: Timestamp,
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
    name.len() <= zeabur_model::kernel::dns::LABEL_MAX && zeabur_core::dns::is_dns_subdomain(name)
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
        WriteBody::JsonPatch(_)
        | WriteBody::MergePatch(_)
        | WriteBody::Apply(_)
        | WriteBody::EphemeralContainers(_) => "patch",
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
pub(crate) async fn delete<S: Sync, T: Sync>(
    cluster: &dyn Cluster,
    kind: IdentityKind,
    namespace: &str,
    object: &Object<S, T>,
) -> Result<(), ApiError> {
    let body = WriteBody::Delete(Preconditions {
        uid: object.metadata.uid.clone(),
        resource_version: None,
        grace_period_seconds: None,
        foreground: false,
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

/// A Zeabur Identity object Zeabur creates.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewObject<S> {
    api_version: &'static str,
    kind: &'static str,
    metadata: ObjectMeta,
    spec: S,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewUserSpec {
    username: String,
    groups: Vec<String>,
    account_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewInvitationSpec {
    user: String,
    token_hash: String,
    expires_at: i64,
}

fn managed() -> BTreeMap<String, String> {
    BTreeMap::from([(
        "app.kubernetes.io/managed-by".to_owned(),
        "zeabur".to_owned(),
    )])
}

/// The `User` for a new member.
pub(crate) fn new_user(
    namespace: &str,
    username: &str,
    display_name: Option<&str>,
    admin: bool,
) -> NewObject<NewUserSpec> {
    NewObject {
        api_version: API_VERSION,
        kind: "User",
        metadata: ObjectMeta {
            name: Some(username.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(managed()),
            ..ObjectMeta::default()
        },
        spec: NewUserSpec {
            username: username.to_owned(),
            groups: Vec::new(),
            account_type: account_type(admin),
            display_name: display_name
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(|n| n.chars().take(128).collect()),
        },
    }
}

const fn account_type(admin: bool) -> &'static str {
    if admin { "Admin" } else { "Regular" }
}

/// The JSON Patch that makes `user` an admin account or a regular one, against the object as
/// read; `None` when it already is what is asked.
pub(crate) fn admin_patch(user: &Object<UserSpec>, admin: bool) -> Option<Value> {
    if (user.spec.account_type == AccountType::Admin) == admin {
        return None;
    }
    Some(json!([
        {"op": "test", "path": "/metadata/uid", "value": user.metadata.uid},
        {"op": "test", "path": "/metadata/resourceVersion", "value": user.metadata.resource_version},
        {"op": "add", "path": "/spec/accountType", "value": account_type(admin)},
    ]))
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
    user: &ObjectMeta,
    token: &str,
    expires_at: i64,
) -> NewObject<NewInvitationSpec> {
    let username = user.name.clone().unwrap_or_default();
    let mut labels = managed();
    labels.insert(USER_LABEL.to_owned(), username.clone());
    labels.insert(
        EXPIRY_BUCKET_LABEL.to_owned(),
        (expires_at / 3600).to_string(),
    );
    NewObject {
        api_version: API_VERSION,
        kind: "Invitation",
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(labels),
            annotations: Some(BTreeMap::from([(
                EXPIRES_AT_ANNOTATION.to_owned(),
                expires_at.to_string(),
            )])),
            owner_references: user.uid.clone().map(|uid| {
                vec![OwnerReference {
                    api_version: API_VERSION.to_owned(),
                    kind: "User".to_owned(),
                    name: username.clone(),
                    uid,
                    ..OwnerReference::default()
                }]
            }),
            ..ObjectMeta::default()
        },
        spec: NewInvitationSpec {
            user: username,
            token_hash: token_hash(token),
            expires_at,
        },
    }
}

/// The name of `username`'s one invitation: a hash, since a token ends its name at the first dot
/// and a user name may hold dots. One name per member is what lets the server refuse a second
/// link made at the same moment.
fn invitation_name(username: &str) -> String {
    let digest: Vec<u8> = sha256(username.as_bytes()).into_iter().take(8).collect();
    format!("inv-{}", hex(&digest))
}

/// Makes the one invitation for the `User` `user` (as read, with its `uid`) lasting `seconds`
/// from `now`, and returns its link. The member's earlier invitations are deleted first, so an
/// older link stops working. Two links made at once cannot both survive: they share a name, so
/// the second creation is refused.
pub(crate) async fn issue(
    cluster: &dyn Cluster,
    namespace: &str,
    issuer: &str,
    user: &Value,
    now: Timestamp,
    seconds: i64,
) -> Result<(String, String, Timestamp), ApiError> {
    let user = user
        .get("metadata")
        .and_then(|m| ObjectMeta::deserialize(m).ok())
        .unwrap_or_default();
    let username = user.name.clone().unwrap_or_default();
    let earlier = read::<Object<InvitationSpec>>(cluster, namespace, IdentityKind::Invitation)
        .await
        .map_err(|e| read_error(e, namespace, IdentityKind::Invitation))?;
    if let Read::Objects(earlier) = earlier {
        for invitation in earlier.iter().filter(|i| i.spec.user == username) {
            delete(cluster, IdentityKind::Invitation, namespace, invitation).await?;
        }
    }
    let name = invitation_name(&username);
    let token = token(&name)?;
    let expires_at = now
        .checked_add(SignedDuration::from_secs(seconds))
        .map_err(|_overflow| {
            ApiError::from(Problem::new(
                Kind::Internal,
                "The link would expire past the last time Zeabur can represent.",
            ))
        })?;
    let object = to_json(&invitation(
        namespace,
        &name,
        &user,
        &token,
        expires_at.as_second(),
    ))?;
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
        let user = ObjectMeta {
            name: Some("ada".to_owned()),
            uid: Some("u-1".to_owned()),
            ..ObjectMeta::default()
        };
        let token = token("inv-1").unwrap();
        let (name, secret) = token.split_once('.').unwrap();
        assert_eq!(name, "inv-1");
        assert_eq!(secret.len(), 43);
        let object =
            serde_json::to_value(invitation("zeabur-identity", "inv-1", &user, &token, 7200))
                .unwrap();
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
    fn a_new_admin_is_an_admin_account_and_a_member_a_regular_one() {
        let admin = serde_json::to_value(new_user("ns", "cy", Some(" Cy "), true)).unwrap();
        assert_eq!(admin["spec"]["groups"], json!([]));
        assert_eq!(admin["spec"]["accountType"], "Admin");
        assert_eq!(admin["spec"]["displayName"], "Cy");
        let member = serde_json::to_value(new_user("ns", "cy", None, false)).unwrap();
        assert_eq!(member["spec"]["groups"], json!([]));
        assert_eq!(member["spec"]["accountType"], "Regular");
    }

    #[test]
    fn the_admin_role_changes_only_the_account_type() {
        let user: Object<UserSpec> = serde_json::from_value(json!({
            "metadata": {"name": "bob", "uid": "u", "resourceVersion": "7"},
            "spec": {"username": "bob", "groups": ["dev"]}
        }))
        .unwrap();
        let patch = admin_patch(&user, true).unwrap();
        assert_eq!(patch[1]["value"], "7");
        assert_eq!(patch[2]["path"], "/spec/accountType");
        assert_eq!(patch[2]["value"], "Admin");
        assert_eq!(admin_patch(&user, false), None);
    }
}
