//! Reading Zeabur Identity's people: the objects, and the views made from them.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::Denied;
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterError, DynamicObject, ResourceRef};
use zeabur_model::Origin;

use super::access::ProjectGrant;
use super::{
    ADMIN_GROUP, API_VERSION, DEPLOYMENT, GROUP, LABEL_ANNOTATION, LAST_USED_ANNOTATION,
    USER_PREFIX, VERSION,
};
use crate::error::{ApiError, list_read, read_problem};
use crate::projects::namespace::Permission;
use crate::settings::IdentitySettings;
use crate::views::UnreadableView;

/// One of Zeabur Identity's kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityKind {
    User,
    Credential,
    Invitation,
    Grant,
}

impl IdentityKind {
    pub(crate) const fn kind(self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Credential => "Credential",
            Self::Invitation => "Invitation",
            Self::Grant => "Grant",
        }
    }

    pub(crate) const fn plural(self) -> &'static str {
        match self {
            Self::User => "users",
            Self::Credential => "credentials",
            Self::Invitation => "invitations",
            Self::Grant => "grants",
        }
    }

    pub(crate) fn resource(self) -> ResourceRef {
        ResourceRef::new(GROUP, VERSION, self.plural(), self.kind())
    }

    /// `credentials.identity.zeabur.com`, as refusals name it.
    pub(crate) fn key(self) -> String {
        format!("{}.{GROUP}", self.plural())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserSpec {
    #[serde(default)]
    pub(crate) display_name: Option<String>,
    #[serde(default)]
    pub(crate) groups: Vec<String>,
    #[serde(default)]
    pub(crate) disabled: bool,
    #[serde(default)]
    pub(crate) elevated_groups: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CredentialSpec {
    pub(crate) user: String,
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) passkey: Option<PasskeySpec>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PasskeySpec {
    #[serde(default)]
    pub(crate) backup_eligible: bool,
    #[serde(default)]
    pub(crate) aaguid: Option<String>,
    #[serde(default)]
    pub(crate) label: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InvitationSpec {
    pub(crate) user: String,
    pub(crate) expires_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrantSpec {
    pub(crate) user: String,
    pub(crate) auth_time: i64,
}

/// An object with its typed `spec`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Object<S> {
    pub(crate) metadata: ObjectMeta,
    pub(crate) spec: S,
}

impl<S> Object<S> {
    pub(crate) fn name(&self) -> &str {
        self.metadata.name.as_deref().unwrap_or_default()
    }

    pub(crate) fn annotation(&self, key: &str) -> Option<&str> {
        self.metadata
            .annotations
            .as_ref()?
            .get(key)
            .map(String::as_str)
    }

    pub(crate) fn created(&self) -> Option<String> {
        self.metadata
            .creation_timestamp
            .as_ref()
            .map(|t| t.0.to_string())
    }
}

/// What Zeabur Identity holds about its people, as far as the person may read it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Directory {
    pub(crate) users: Vec<Object<UserSpec>>,
    pub(crate) credentials: Vec<Object<CredentialSpec>>,
    pub(crate) invitations: Vec<Object<InvitationSpec>>,
    pub(crate) grants: Vec<Object<GrantSpec>>,
    pub(crate) unreadable: Vec<UnreadableView>,
    /// Each member's projects, by user name; `None` when the bindings were not read.
    pub(crate) projects: Option<std::collections::BTreeMap<String, Vec<ProjectGrant>>>,
}

/// Objects that do not decode (made by hand, from another version) are left out: one bad object
/// must not hide everyone else.
fn decode<S: for<'de> Deserialize<'de>>(objects: Vec<DynamicObject>) -> Vec<Object<S>> {
    objects
        .into_iter()
        .filter_map(|object| serde_json::to_value(object).ok())
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect()
}

/// What reading one kind gave: its objects, nothing because the API is not installed, or a
/// refusal.
pub(crate) enum Read<S> {
    Objects(Vec<Object<S>>),
    NotInstalled,
}

pub(crate) async fn read<S: for<'de> Deserialize<'de>>(
    cluster: &dyn Cluster,
    namespace: &str,
    kind: IdentityKind,
) -> Result<Read<S>, ClusterError> {
    Ok(cluster
        .list_objects(&kind.resource(), Some(namespace))
        .await?
        .map_or(Read::NotInstalled, |objects| Read::Objects(decode(objects))))
}

/// The problem for a refused or failed read of `kind`.
pub(crate) fn read_error(error: ClusterError, namespace: &str, kind: IdentityKind) -> ApiError {
    read_problem(
        error,
        &format!("Zeabur Identity's {} in {namespace}", kind.plural()),
        list_read(GROUP, kind.plural(), Some(namespace)),
    )
}

/// The objects of a kind that only adds to the answer; a refusal is named in `unreadable`.
fn optional<S>(
    read: Result<Read<S>, ClusterError>,
    kind: IdentityKind,
    unreadable: &mut Vec<UnreadableView>,
) -> Vec<Object<S>> {
    match read {
        Ok(Read::Objects(objects)) => objects,
        Ok(Read::NotInstalled) => Vec::new(),
        Err(error) => {
            if let Ok(refused) = error.into_unreadable(&kind.key()) {
                unreadable.push(refused.into());
            }
            Vec::new()
        }
    }
}

impl Directory {
    /// Reads users and credentials, which every answer needs (a refusal is the answer), and
    /// invitations and grants, which only add to it (a refusal is named in `unreadable`).
    /// `None`: Zeabur Identity is not installed.
    pub(crate) async fn load(
        cluster: &dyn Cluster,
        namespace: &str,
    ) -> Result<Option<Self>, ApiError> {
        let (users, credentials, invitations, grants) = futures_util::join!(
            read::<UserSpec>(cluster, namespace, IdentityKind::User),
            read::<CredentialSpec>(cluster, namespace, IdentityKind::Credential),
            read::<InvitationSpec>(cluster, namespace, IdentityKind::Invitation),
            read::<GrantSpec>(cluster, namespace, IdentityKind::Grant),
        );
        let users = match users.map_err(|e| read_error(e, namespace, IdentityKind::User))? {
            Read::Objects(users) => users,
            Read::NotInstalled => return Ok(None),
        };
        let credentials =
            match credentials.map_err(|e| read_error(e, namespace, IdentityKind::Credential))? {
                Read::Objects(credentials) => credentials,
                Read::NotInstalled => Vec::new(),
            };
        let mut unreadable = Vec::new();
        let invitations = optional(invitations, IdentityKind::Invitation, &mut unreadable);
        let grants = optional(grants, IdentityKind::Grant, &mut unreadable);
        Ok(Some(Self {
            users,
            credentials,
            invitations,
            grants,
            unreadable,
            projects: None,
        }))
    }

    pub(crate) fn user(&self, username: &str) -> Option<&Object<UserSpec>> {
        self.users.iter().find(|u| u.name() == username)
    }

    /// The person's passkeys: what the last-passkey rule counts.
    pub(crate) fn passkeys_of(&self, username: &str) -> usize {
        self.credentials
            .iter()
            .filter(|c| c.spec.user == username && c.spec.kind == "Passkey")
            .count()
    }

    /// The member `user`, as `you` (the person's Kubernetes user name) sees them, with their
    /// projects when the bindings could be read.
    pub(crate) fn member(
        &self,
        user: &Object<UserSpec>,
        namespace: &str,
        you: Option<&str>,
        projects: Option<Vec<ProjectGrant>>,
    ) -> MemberView {
        let username = user.name().to_owned();
        let mine = |owner: &str| owner == username;
        let mut passkeys = Vec::new();
        let mut linked = Vec::new();
        for credential in self.credentials.iter().filter(|c| mine(&c.spec.user)) {
            let origin = Origin::complete(
                API_VERSION,
                "Credential",
                Some(namespace),
                credential.name(),
            );
            let last_used_at = credential
                .annotation(LAST_USED_ANNOTATION)
                .map(str::to_owned);
            if credential.spec.kind == "Passkey" {
                let passkey = credential.spec.passkey.clone().unwrap_or_default();
                let label = credential
                    .annotation(LABEL_ANNOTATION)
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .or_else(|| passkey.label.filter(|l| !l.trim().is_empty()));
                passkeys.push(PasskeyView {
                    id: credential.name().to_owned(),
                    label,
                    created_at: credential.created(),
                    last_used_at,
                    synced: passkey.backup_eligible,
                    authenticator: passkey.aaguid,
                    origin,
                });
            } else {
                linked.push(LinkedAccountView {
                    id: credential.name().to_owned(),
                    provider: credential.spec.kind.clone(),
                    created_at: credential.created(),
                    last_used_at,
                    origin,
                });
            }
        }
        passkeys.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        let grants: Vec<_> = self.grants.iter().filter(|g| mine(&g.spec.user)).collect();
        let last_sign_in_at = grants
            .iter()
            .filter_map(|g| Timestamp::from_second(g.spec.auth_time).ok())
            .map(|t| t.to_string())
            .chain(passkeys.iter().filter_map(|p| p.last_used_at.clone()))
            .chain(linked.iter().filter_map(|l| l.last_used_at.clone()))
            .filter_map(|t| t.parse::<Timestamp>().ok())
            .max()
            .map(|t| t.to_string());
        let spec = &user.spec;
        let admin = spec.elevated_groups.iter().any(|g| g == ADMIN_GROUP);
        MemberView {
            id: format!("members/{username}"),
            display_name: spec.display_name.clone().filter(|n| !n.trim().is_empty()),
            groups: spec.groups.clone(),
            role: if admin {
                MemberRole::Admin
            } else {
                MemberRole::Member
            },
            projects: projects.filter(|_| !admin),
            disabled: spec.disabled,
            passkeys,
            linked,
            last_sign_in_at,
            sessions: grants.len(),
            invitations: self
                .invitations
                .iter()
                .filter(|i| mine(&i.spec.user))
                .count(),
            you: you.is_some_and(|you| you == format!("{USER_PREFIX}{username}")),
            origin: Origin::complete(API_VERSION, "User", Some(namespace), &username),
            username,
        }
    }

    pub(crate) fn invitation(
        &self,
        invitation: &Object<InvitationSpec>,
        namespace: &str,
    ) -> InvitationView {
        let username = &invitation.spec.user;
        InvitationView {
            id: format!("members/invitations/{}", invitation.name()),
            name: invitation.name().to_owned(),
            username: username.clone(),
            expires_at: Timestamp::from_second(invitation.spec.expires_at)
                .map_or_default(|t| t.to_string()),
            created_at: invitation.created(),
            new_member: self.user(username).is_none()
                || !self.credentials.iter().any(|c| &c.spec.user == username),
            origin: Origin::complete(
                API_VERSION,
                "Invitation",
                Some(namespace),
                invitation.name(),
            ),
        }
    }

    /// Every member (sorted by name) and every invitation still valid at `now` (soonest to
    /// expire first).
    pub(crate) fn view(
        &self,
        namespace: &str,
        you: Option<&str>,
        now: Timestamp,
        setup_links: SetupLinks,
        can_manage: Permission,
    ) -> MembersView {
        let mut items: Vec<MemberView> = self
            .users
            .iter()
            .map(|user| {
                let projects = self
                    .projects
                    .as_ref()
                    .map(|grants| grants.get(user.name()).cloned().unwrap_or_default());
                self.member(user, namespace, you, projects)
            })
            .collect();
        items.sort_by(|a, b| a.username.cmp(&b.username));
        let mut invitations: Vec<InvitationView> = self
            .invitations
            .iter()
            .filter(|i| i.spec.expires_at > now.as_second())
            .map(|i| self.invitation(i, namespace))
            .collect();
        invitations.sort_by(|a, b| a.expires_at.cmp(&b.expires_at).then(a.name.cmp(&b.name)));
        MembersView {
            id: "members".to_owned(),
            installed: true,
            items,
            invitations,
            setup_links,
            can_manage,
            project_roles: false,
            unreadable: self.unreadable.clone(),
        }
    }
}

/// The Zeabur Identity user name of a Kubernetes user name: `zeabur:alice` is `alice`.
pub(crate) fn identity_username(kubernetes: &str) -> Option<&str> {
    kubernetes
        .strip_prefix(USER_PREFIX)
        .filter(|rest| !rest.is_empty() && !rest.contains(':'))
}

/// Whether setup links can be made and redeemed here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupLinks {
    /// The address links open at; `None` when neither the server's configuration nor the
    /// Identity Deployment (where it can be read) names it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// Whether Zeabur Identity redeems invitations (`--invitations`), from its Deployment;
    /// `None` when it could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted: Option<bool>,
}

/// The issuer and whether invitations are redeemed, from the Identity Deployment's container:
/// `ZEABUR_IDENTITY_ISSUER`, and `--invitations` or `ZEABUR_IDENTITY_INVITATIONS`.
pub(crate) fn from_deployment(deployment: &Value) -> (Option<String>, Option<bool>) {
    let containers = deployment
        .pointer("/spec/template/spec/containers")
        .and_then(Value::as_array);
    let Some(containers) = containers else {
        return (None, None);
    };
    let mut issuer = None;
    let mut accepted = false;
    for container in containers {
        let args = container.get("args").and_then(Value::as_array);
        if args
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|a| a == "--invitations" || a == "--invitations=true")
        {
            accepted = true;
        }
        for env in container
            .get("env")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let value = env.get("value").and_then(Value::as_str);
            match env.get("name").and_then(Value::as_str) {
                Some("ZEABUR_IDENTITY_ISSUER") => issuer = value.map(str::to_owned),
                Some("ZEABUR_IDENTITY_INVITATIONS") => {
                    accepted |= value.is_some_and(|v| v.eq_ignore_ascii_case("true"));
                }
                _ => {}
            }
        }
    }
    (issuer, Some(accepted))
}

/// Where links lead and whether they work, from the settings and the Identity Deployment.
pub(crate) async fn setup_links(cluster: &dyn Cluster, settings: &IdentitySettings) -> SetupLinks {
    let deployments = ResourceRef::new("apps", "v1", "deployments", "Deployment");
    let found = cluster
        .get_object(&deployments, Some(&settings.namespace), DEPLOYMENT)
        .await
        .ok()
        .flatten()
        .and_then(|d| serde_json::to_value(d).ok());
    let (issuer, accepted) = found.as_ref().map_or((None, None), from_deployment);
    SetupLinks {
        issuer: settings
            .issuer
            .clone()
            .or(issuer)
            .map(|i| i.trim_end_matches('/').to_owned()),
        accepted,
    }
}

/// Whether the person may create invitations: the right every change here starts with.
pub(crate) async fn can_manage(cluster: &dyn Cluster, namespace: &str) -> Permission {
    cluster
        .access_rules(namespace)
        .await
        .map_or(Permission::Unknown, |rules| {
            rules
                .allows("create", &IdentityKind::Invitation.resource(), "")
                .into()
        })
}

/// A request Zeabur makes to `kind`, as a refusal names it.
pub(crate) fn request(
    verb: &str,
    kind: IdentityKind,
    namespace: &str,
    name: Option<&str>,
) -> Denied {
    Denied {
        verb: verb.to_owned(),
        group: GROUP.to_owned(),
        resource: kind.plural().to_owned(),
        name: name.map(str::to_owned),
        namespace: Some(namespace.to_owned()),
    }
}

/// A passkey: a way to sign in that lives on a device or in a password manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyView {
    /// The `Credential` object's name.
    pub id: String,
    /// The name its owner gave it, at enrolment or since.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// When it was added, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// When it last signed in, where Zeabur Identity records it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    /// Whether it may be synced to the owner's other devices (the authenticator's BE flag).
    pub synced: bool,
    /// The authenticator model (AAGUID), when it reported one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authenticator: Option<String>,
    /// The `Credential`.
    pub origin: Origin,
}

/// A GitHub or Google account linked for everyday sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkedAccountView {
    /// The `Credential` object's name.
    pub id: String,
    /// `GitHub` or `Google`.
    pub provider: String,
    /// When it was linked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// When it last signed in, where recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    /// The `Credential`.
    pub origin: Origin,
}

/// What a member may do in the cluster, as far as Zeabur Identity grants it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MemberRole {
    /// May take on admin rights (`elevatedGroups` holds `admins`).
    Admin,
    /// Signs in; their rights are whatever RBAC grants their name and groups.
    Member,
}

/// A person who can sign in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberView {
    /// `members/<username>`.
    pub id: String,
    /// Login name; Kubernetes sees `zeabur:<username>`.
    pub username: String,
    /// Name shown in the interface.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Groups; Kubernetes sees each as `zeabur:<group>`.
    pub groups: Vec<String>,
    /// Admin or member.
    pub role: MemberRole,
    /// A member's roles in projects, by project; `None` when Zeabur could not read them. An
    /// admin's are left out: their rights cover every project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projects: Option<Vec<ProjectGrant>>,
    /// A disabled member cannot sign in.
    pub disabled: bool,
    /// Passkeys, oldest first.
    pub passkeys: Vec<PasskeyView>,
    /// Linked GitHub or Google accounts.
    pub linked: Vec<LinkedAccountView>,
    /// The latest sign-in Zeabur Identity knows of: from live sessions and recorded last use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sign_in_at: Option<String>,
    /// Signed-in sessions (grants) that can still renew.
    pub sessions: usize,
    /// Pending setup links.
    pub invitations: usize,
    /// Whether this is the person asking.
    pub you: bool,
    /// The `User`.
    pub origin: Origin,
}

/// A pending one-time setup link. Its token is not kept anywhere: it can be revoked, not shown
/// again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvitationView {
    /// `members/invitations/<name>`.
    pub id: String,
    /// The `Invitation` object's name.
    pub name: String,
    /// Who it enrols a passkey for.
    pub username: String,
    /// When it stops working, RFC 3339.
    pub expires_at: String,
    /// When it was made.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Whether it is for someone who cannot sign in yet (no credential).
    pub new_member: bool,
    /// The `Invitation`.
    pub origin: Origin,
}

/// Everyone who can sign in, and the links waiting to be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MembersView {
    /// `members`.
    pub id: String,
    /// Whether Zeabur Identity is installed; when not, everything else is empty.
    pub installed: bool,
    /// Members, by user name.
    pub items: Vec<MemberView>,
    /// Pending invitations, soonest to expire first.
    pub invitations: Vec<InvitationView>,
    /// Whether setup links can be made and redeemed.
    pub setup_links: SetupLinks,
    /// Whether the person may create invitations, as far as their rules tell.
    pub can_manage: Permission,
    /// Whether members can be given Developer or Viewer in chosen projects (the chart's member
    /// roles are installed).
    pub project_roles: bool,
    /// Kinds the person may not read (invitations, grants, role bindings), whose parts are left
    /// out.
    pub unreadable: Vec<UnreadableView>,
}

/// Whether the person is a Zeabur Identity user Zeabur can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AccountStatus {
    /// Signed in through Zeabur Identity; `member` is their account.
    Identity,
    /// Signed in as a `zeabur:` user that has no `User` object.
    Missing,
    /// Signed in with another credential (a kubeconfig's), which Zeabur Identity does not hold.
    Kubernetes,
    /// Zeabur Identity is not installed in the cluster.
    NotInstalled,
}

/// The person's own account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    /// `account`.
    pub id: String,
    /// Who the cluster says the person is.
    pub kubernetes_username: String,
    /// Whether Zeabur Identity holds the account.
    pub status: AccountStatus,
    /// The account, when it does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<MemberView>,
    /// Whether setup links can be made and redeemed.
    pub setup_links: SetupLinks,
    /// Whether the person may create invitations (and so add a passkey), as far as their rules
    /// tell.
    pub can_manage: Permission,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn object<S: for<'de> Deserialize<'de>>(value: Value) -> Object<S> {
        serde_json::from_value(value).unwrap()
    }

    fn user(name: &str, admin: bool) -> Object<UserSpec> {
        object(json!({
            "metadata": {"name": name},
            "spec": {"username": name, "groups": if admin {json!(["admins"])} else {json!([])},
                     "elevatedGroups": if admin {json!(["admins"])} else {json!([])}}
        }))
    }

    fn passkey(
        name: &str,
        user: &str,
        created: &str,
        annotations: &Value,
    ) -> Object<CredentialSpec> {
        object(json!({
            "metadata": {"name": name, "creationTimestamp": created, "annotations": annotations},
            "spec": {"user": user, "type": "Passkey",
                     "passkey": {"credentialId": "abc", "record": "{}", "label": "MacBook"}}
        }))
    }

    fn directory() -> Directory {
        Directory {
            users: vec![user("ada", true), user("bob", false)],
            credentials: vec![
                passkey("c-2", "ada", "2026-10-02T00:00:00Z", &json!({})),
                passkey(
                    "c-1",
                    "ada",
                    "2026-10-01T00:00:00Z",
                    &json!({LABEL_ANNOTATION: "Phone", LAST_USED_ANNOTATION: "2026-10-20T08:00:00Z"}),
                ),
                object(json!({
                    "metadata": {"name": "c-3", "creationTimestamp": "2026-10-03T00:00:00Z"},
                    "spec": {"user": "bob", "type": "GitHub",
                             "external": {"issuer": "https://github.com", "subject": "1"}}
                })),
            ],
            invitations: vec![
                object(json!({"metadata": {"name": "inv-old"},
                    "spec": {"user": "bob", "tokenHash": "sha256:00", "expiresAt": 100}})),
                object(json!({"metadata": {"name": "inv-new"},
                    "spec": {"user": "cy", "tokenHash": "sha256:00", "expiresAt": 2_000_000_000}})),
            ],
            grants: vec![object(json!({"metadata": {"name": "g-1"},
                "spec": {"user": "ada", "authTime": 1_760_947_200, "expiresAt": 2_000_000_000}}))],
            unreadable: Vec::new(),
            projects: None,
        }
    }

    #[test]
    fn a_member_has_their_passkeys_oldest_first_with_the_name_chosen_last() {
        let view = directory().view(
            "zeabur-identity",
            Some("zeabur:ada"),
            Timestamp::from_second(1_000_000_000).unwrap(),
            SetupLinks {
                issuer: None,
                accepted: None,
            },
            Permission::Allowed,
        );
        let ada = &view.items[0];
        assert_eq!(ada.username, "ada");
        assert!(ada.you);
        assert_eq!(ada.role, MemberRole::Admin);
        let labels: Vec<_> = ada.passkeys.iter().map(|p| p.label.as_deref()).collect();
        assert_eq!(labels, [Some("Phone"), Some("MacBook")]);
        assert_eq!(ada.sessions, 1);
        // The recorded use is later than the grant's sign-in.
        assert_eq!(ada.last_sign_in_at.as_deref(), Some("2026-10-20T08:00:00Z"));
        let bob = &view.items[1];
        assert!(!bob.you);
        assert_eq!(bob.role, MemberRole::Member);
        assert_eq!(bob.passkeys.len(), 0);
        assert_eq!(bob.linked[0].provider, "GitHub");
        assert_eq!(bob.last_sign_in_at, None);
        assert_eq!(bob.invitations, 1);
    }

    #[test]
    fn expired_invitations_are_left_out_and_one_for_a_new_name_is_a_new_member() {
        let view = directory().view(
            "zeabur-identity",
            None,
            Timestamp::from_second(1_000_000_000).unwrap(),
            SetupLinks {
                issuer: None,
                accepted: None,
            },
            Permission::Allowed,
        );
        assert_eq!(view.invitations.len(), 1);
        assert_eq!(view.invitations[0].username, "cy");
        assert!(view.invitations[0].new_member);
    }

    #[test]
    fn only_passkeys_count_towards_the_last_one() {
        let directory = directory();
        assert_eq!(directory.passkeys_of("ada"), 2);
        assert_eq!(directory.passkeys_of("bob"), 0);
        assert_eq!(directory.passkeys_of("nobody"), 0);
    }

    #[test]
    fn only_a_zeabur_user_name_is_an_identity_user() {
        assert_eq!(identity_username("zeabur:ada"), Some("ada"));
        assert_eq!(identity_username("zeabur:"), None);
        assert_eq!(identity_username("zeabur:elevated:ada"), None);
        assert_eq!(identity_username("kubernetes-admin"), None);
        assert_eq!(identity_username("oidc:ada"), None);
    }

    #[test]
    fn the_deployment_names_the_issuer_and_whether_it_redeems_invitations() {
        let deployment = |args: Value, env: Value| json!({"spec": {"template": {"spec": {"containers": [{"args": args, "env": env}]}}}});
        let issuer = json!([{"name": "ZEABUR_IDENTITY_ISSUER", "value": "https://id.example.com"}]);
        assert_eq!(
            from_deployment(&deployment(json!(["--invitations"]), issuer.clone())),
            (Some("https://id.example.com".to_owned()), Some(true))
        );
        assert_eq!(
            from_deployment(&deployment(json!([]), issuer)),
            (Some("https://id.example.com".to_owned()), Some(false))
        );
        assert_eq!(
            from_deployment(&deployment(
                Value::Null,
                json!([{"name": "ZEABUR_IDENTITY_INVITATIONS", "value": "true"}])
            )),
            (None, Some(true))
        );
        assert_eq!(from_deployment(&json!({})), (None, None));
    }
}
