//! Reading Zeabur Identity's people: the objects, and the views made from them.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{Denied, Kind, Problem};
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterError, DynamicObject, ResourceRef};
use zeabur_model::Origin;

use super::access::ProjectGrant;
use super::{API_VERSION, DEPLOYMENT, GROUP, USER_PREFIX, VERSION};
use crate::error::{ApiError, list_read, read_problem};
use crate::projects::namespace::Permission;
use crate::settings::IdentitySettings;
use crate::views::{UndecodedView, UnreadableView};

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
    pub(crate) account_type: AccountType,
}

/// A `User`'s `spec.accountType`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub(crate) enum AccountType {
    #[default]
    Regular,
    Admin,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CredentialSpec {
    pub(crate) user: String,
    #[serde(flatten)]
    pub(crate) kind: CredentialKind,
}

/// What a `Credential` signs in with: the field that is set names the kind, as in Zeabur
/// Identity's `CredentialKind`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CredentialKind {
    Passkey(PasskeySpec),
    External(ExternalSpec),
    Password(PasswordSpec),
}

impl CredentialKind {
    pub(crate) const fn is_passkey(&self) -> bool {
        matches!(self, Self::Passkey(_))
    }
}

/// An account at an upstream provider.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExternalSpec {
    pub(crate) issuer: String,
}

impl ExternalSpec {
    /// `GitHub` or `Google` for their issuers, else the issuer.
    fn provider(&self) -> String {
        match self.issuer.as_str() {
            "https://github.com" => "GitHub".to_owned(),
            "https://accounts.google.com" => "Google".to_owned(),
            issuer => issuer.to_owned(),
        }
    }
}

/// A password. Its hash and second-factor secret stay unread.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PasswordSpec {
    #[serde(default)]
    pub(crate) totp: Option<serde::de::IgnoredAny>,
}

/// What Zeabur Identity changes after enrolment.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CredentialStatus {
    #[serde(default)]
    pub(crate) label: Option<String>,
    /// Unix seconds.
    #[serde(default)]
    pub(crate) last_used_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
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

/// An object with its typed `spec` and, for kinds that need it, `status`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Object<S, T = serde::de::IgnoredAny> {
    pub(crate) metadata: ObjectMeta,
    pub(crate) spec: S,
    #[serde(default)]
    pub(crate) status: T,
}

/// A `Credential`.
pub(crate) type CredentialObject = Object<CredentialSpec, CredentialStatus>;

impl<S, T> Object<S, T> {
    pub(crate) fn name(&self) -> &str {
        self.metadata.name.as_deref().unwrap_or_default()
    }

    pub(crate) fn created(&self) -> Option<Timestamp> {
        self.metadata.creation_timestamp.as_ref().map(|t| t.0)
    }
}

/// What Zeabur Identity holds about its people, as far as the person may read it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Directory {
    pub(crate) users: Vec<Object<UserSpec>>,
    pub(crate) credentials: Vec<CredentialObject>,
    pub(crate) invitations: Vec<Object<InvitationSpec>>,
    pub(crate) grants: Vec<Object<GrantSpec>>,
    pub(crate) unreadable: Vec<UnreadableView>,
    /// Objects of a shape Zeabur does not know.
    pub(crate) undecoded: Vec<UndecodedView>,
    /// Each member's projects, by user name; `None` when the bindings were not read.
    pub(crate) projects: Option<std::collections::BTreeMap<String, Vec<ProjectGrant>>>,
}

/// The objects of a kind that decode, and those that do not with why. One object of a shape
/// Zeabur does not know must not hide everyone else, nor be left out unsaid.
pub(crate) struct Decoded<O> {
    pub(crate) objects: Vec<O>,
    pub(crate) undecoded: Vec<UndecodedView>,
}

fn decode<O: for<'de> Deserialize<'de>>(
    objects: Vec<DynamicObject>,
    namespace: &str,
    kind: IdentityKind,
) -> Decoded<O> {
    let mut decoded = Decoded {
        objects: Vec::new(),
        undecoded: Vec::new(),
    };
    for object in objects {
        let name = object.metadata.name.clone().unwrap_or_default();
        match serde_json::to_value(object).and_then(serde_json::from_value) {
            Ok(object) => decoded.objects.push(object),
            Err(error) => decoded.undecoded.push(UndecodedView {
                origin: Origin::complete(API_VERSION, kind.kind(), Some(namespace), &name),
                error: error.to_string(),
            }),
        }
    }
    decoded
}

/// What reading one kind gave: its objects, nothing because the API is not installed, or a
/// refusal.
pub(crate) enum Read<O> {
    Objects(Decoded<O>),
    NotInstalled,
}

pub(crate) async fn read<O: for<'de> Deserialize<'de>>(
    cluster: &dyn Cluster,
    namespace: &str,
    kind: IdentityKind,
) -> Result<Read<O>, ClusterError> {
    Ok(cluster
        .list_objects(&kind.resource(), Some(namespace))
        .await?
        .map_or(Read::NotInstalled, |objects| {
            Read::Objects(decode(objects, namespace, kind))
        }))
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
fn optional<O>(
    read: Result<Read<O>, ClusterError>,
    kind: IdentityKind,
    unreadable: &mut Vec<UnreadableView>,
    undecoded: &mut Vec<UndecodedView>,
) -> Vec<O> {
    match read {
        Ok(Read::Objects(decoded)) => {
            undecoded.extend(decoded.undecoded);
            decoded.objects
        }
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
            read::<Object<UserSpec>>(cluster, namespace, IdentityKind::User),
            read::<CredentialObject>(cluster, namespace, IdentityKind::Credential),
            read::<Object<InvitationSpec>>(cluster, namespace, IdentityKind::Invitation),
            read::<Object<GrantSpec>>(cluster, namespace, IdentityKind::Grant),
        );
        let Read::Objects(users) =
            users.map_err(|e| read_error(e, namespace, IdentityKind::User))?
        else {
            return Ok(None);
        };
        let mut undecoded = users.undecoded;
        let credentials =
            match credentials.map_err(|e| read_error(e, namespace, IdentityKind::Credential))? {
                Read::Objects(credentials) => {
                    undecoded.extend(credentials.undecoded);
                    credentials.objects
                }
                Read::NotInstalled => Vec::new(),
            };
        let mut unreadable = Vec::new();
        let invitations = optional(
            invitations,
            IdentityKind::Invitation,
            &mut unreadable,
            &mut undecoded,
        );
        let grants = optional(grants, IdentityKind::Grant, &mut unreadable, &mut undecoded);
        Ok(Some(Self {
            users: users.objects,
            credentials,
            invitations,
            grants,
            unreadable,
            undecoded,
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
            .filter(|c| c.spec.user == username && c.spec.kind.is_passkey())
            .count()
    }

    /// The member `user`, as `you` (the person's Kubernetes user name) sees them, with their
    /// projects when the bindings could be read. `project_roles` says whether Zeabur Identity
    /// gives roles per project.
    pub(crate) fn member(
        &self,
        user: &Object<UserSpec>,
        namespace: &str,
        you: Option<&str>,
        projects: Option<Vec<ProjectGrant>>,
        project_roles: bool,
    ) -> MemberView {
        let username = user.name().to_owned();
        let is_you = you.is_some_and(|you| you == format!("{USER_PREFIX}{username}"));
        let mine = |owner: &str| owner == username;
        let mut passkeys = Vec::new();
        let mut linked = Vec::new();
        let mut password = None;
        for credential in self.credentials.iter().filter(|c| mine(&c.spec.user)) {
            let origin = Origin::complete(
                API_VERSION,
                "Credential",
                Some(namespace),
                credential.name(),
            );
            let last_used_at = credential
                .status
                .last_used_at
                .and_then(|secs| Timestamp::from_second(secs).ok());
            match &credential.spec.kind {
                CredentialKind::Passkey(passkey) => {
                    let label = credential
                        .status
                        .label
                        .as_deref()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(str::to_owned)
                        .or_else(|| passkey.label.clone().filter(|l| !l.trim().is_empty()));
                    passkeys.push(PasskeyView {
                        id: credential.name().to_owned(),
                        label,
                        created_at: credential.created(),
                        last_used_at,
                        synced: passkey.backup_eligible,
                        authenticator: passkey.aaguid.clone(),
                        origin,
                    });
                }
                CredentialKind::External(external) => {
                    linked.push(LinkedAccountView {
                        id: credential.name().to_owned(),
                        provider: external.provider(),
                        created_at: credential.created(),
                        last_used_at,
                        origin,
                    });
                }
                CredentialKind::Password(spec) => {
                    password = Some(PasswordView {
                        id: credential.name().to_owned(),
                        created_at: credential.created(),
                        last_used_at,
                        second_factor: spec.totp.is_some(),
                        origin,
                    });
                }
            }
        }
        passkeys.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        let grants: Vec<_> = self.grants.iter().filter(|g| mine(&g.spec.user)).collect();
        let last_sign_in_at = grants
            .iter()
            .filter_map(|g| Timestamp::from_second(g.spec.auth_time).ok())
            .chain(passkeys.iter().filter_map(|p| p.last_used_at))
            .chain(linked.iter().filter_map(|l| l.last_used_at))
            .chain(password.as_ref().and_then(|p| p.last_used_at))
            .max();
        let spec = &user.spec;
        let admin = spec.account_type == AccountType::Admin;
        MemberView {
            id: format!("members/{username}"),
            display_name: spec.display_name.clone().filter(|n| !n.trim().is_empty()),
            groups: spec.groups.clone(),
            role: if admin {
                MemberRole::Admin
            } else {
                MemberRole::Member
            },
            actions: MemberActions {
                can_remove: !is_you,
                can_change_access: !is_you && (!project_roles || admin || projects.is_some()),
            },
            projects: projects.filter(|_| !admin),
            disabled: spec.disabled,
            passkeys,
            linked,
            password,
            last_sign_in_at,
            sessions: grants.len(),
            invitations: self
                .invitations
                .iter()
                .filter(|i| mine(&i.spec.user))
                .count(),
            you: is_you,
            origin: Origin::complete(API_VERSION, "User", Some(namespace), &username),
            username,
        }
    }

    pub(crate) fn invitation(
        &self,
        invitation: &Object<InvitationSpec>,
        namespace: &str,
    ) -> Result<InvitationView, ApiError> {
        let username = &invitation.spec.user;
        let expires_at = Timestamp::from_second(invitation.spec.expires_at).map_err(|_range| {
            ApiError::from(Problem::new(
                Kind::Internal,
                format!(
                    "Zeabur Identity gave invitation {} an expiry out of range.",
                    invitation.name()
                ),
            ))
        })?;
        Ok(InvitationView {
            id: format!("members/invitations/{}", invitation.name()),
            name: invitation.name().to_owned(),
            username: username.clone(),
            expires_at,
            created_at: invitation.created(),
            new_member: self.user(username).is_none()
                || !self.credentials.iter().any(|c| &c.spec.user == username),
            origin: Origin::complete(
                API_VERSION,
                "Invitation",
                Some(namespace),
                invitation.name(),
            ),
        })
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
        project_roles: bool,
    ) -> Result<MembersView, ApiError> {
        let mut items: Vec<MemberView> = self
            .users
            .iter()
            .map(|user| {
                let projects = self
                    .projects
                    .as_ref()
                    .map(|grants| grants.get(user.name()).cloned().unwrap_or_default());
                self.member(user, namespace, you, projects, project_roles)
            })
            .collect();
        items.sort_by(|a, b| a.username.cmp(&b.username));
        let mut invitations = self
            .invitations
            .iter()
            .filter(|i| i.spec.expires_at > now.as_second())
            .map(|i| self.invitation(i, namespace))
            .collect::<Result<Vec<_>, _>>()?;
        invitations.sort_by(|a, b| a.expires_at.cmp(&b.expires_at).then(a.name.cmp(&b.name)));
        Ok(MembersView {
            id: "members".to_owned(),
            installed: true,
            items,
            invitations,
            setup_links,
            can_manage,
            project_roles,
            assignable_roles: super::access::ProjectRole::ALL.to_vec(),
            default_role: super::access::ProjectRole::DEFAULT,
            api_tokens: Vec::new(),
            can_manage_tokens: Permission::Unknown,
            unreadable: self.unreadable.clone(),
            undecoded: self.undecoded.clone(),
        })
    }
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
        .review_access(&request(
            "create",
            IdentityKind::Invitation,
            namespace,
            None,
        ))
        .await
        .into()
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
    /// When it was added.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Timestamp>,
    /// When it last signed in, where Zeabur Identity records it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<Timestamp>,
    /// Whether it may be synced to the owner's other devices (the authenticator's BE flag).
    pub synced: bool,
    /// The authenticator model (AAGUID), when it reported one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authenticator: Option<String>,
    /// The `Credential`.
    pub origin: Origin,
}

/// An account at an upstream provider, such as GitHub or Google, linked for everyday sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkedAccountView {
    /// The `Credential` object's name.
    pub id: String,
    /// `GitHub`, `Google`, or the upstream issuer of another provider.
    pub provider: String,
    /// When it was linked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Timestamp>,
    /// When it last signed in, where recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<Timestamp>,
    /// The `Credential`.
    pub origin: Origin,
}

/// A password, with or without a second factor. A member has at most one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PasswordView {
    /// The `Credential` object's name.
    pub id: String,
    /// When it was set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Timestamp>,
    /// When it last signed in, where recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<Timestamp>,
    /// Whether sign-in asks for an authenticator code after the password.
    pub second_factor: bool,
    /// The `Credential`.
    pub origin: Origin,
}

/// What a member may do in the cluster, as far as Zeabur Identity grants it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MemberRole {
    /// An admin account (`accountType: Admin`): full rights, passkey sign-in only.
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
    /// Their password, if they set one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<PasswordView>,
    /// The latest sign-in Zeabur Identity knows of: from live sessions and recorded last use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sign_in_at: Option<Timestamp>,
    /// Signed-in sessions (grants) that can still renew.
    pub sessions: usize,
    /// Pending setup links.
    pub invitations: usize,
    /// Whether this is the person asking.
    pub you: bool,
    /// What the person asking may do to this member.
    #[serde(flatten)]
    pub actions: MemberActions,
    /// The `User`.
    pub origin: Origin,
}

/// What the person asking may do to a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemberActions {
    /// Remove them: never yourself.
    pub can_remove: bool,
    /// Change their access: never your own, and not when their project roles could not be read,
    /// since saving would replace roles nobody saw.
    pub can_change_access: bool,
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
    /// When it stops working.
    pub expires_at: Timestamp,
    /// When it was made.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Timestamp>,
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
    /// API tokens, by name: machine accounts with the access of a member.
    pub api_tokens: Vec<super::tokens::ApiTokenView>,
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
    /// Whether the person may create API tokens: `create serviceaccounts` in the token
    /// namespace.
    pub can_manage_tokens: Permission,
    /// Whether members can be given Developer or Viewer in chosen projects (the chart's member
    /// roles are installed).
    pub project_roles: bool,
    /// The roles a member can be given in a project, in the order to offer them.
    pub assignable_roles: Vec<super::access::ProjectRole>,
    /// The role a new member gets in a project unless another is chosen.
    pub default_role: super::access::ProjectRole,
    /// Kinds the person may not read (invitations, grants, role bindings), whose parts are left
    /// out.
    pub unreadable: Vec<UnreadableView>,
    /// Zeabur Identity's objects whose shape Zeabur does not know, which are left out of the
    /// members and invitations above.
    pub undecoded: Vec<UndecodedView>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn object<O: for<'de> Deserialize<'de>>(value: Value) -> O {
        serde_json::from_value(value).unwrap()
    }

    fn user(name: &str, admin: bool) -> Object<UserSpec> {
        object(json!({
            "metadata": {"name": name},
            "spec": {"username": name, "groups": [],
                     "accountType": if admin {"Admin"} else {"Regular"}}
        }))
    }

    fn passkey(name: &str, user: &str, created: &str, status: &Value) -> CredentialObject {
        object(json!({
            "metadata": {"name": name, "creationTimestamp": created},
            "status": status,
            "spec": {"user": user,
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
                    &json!({"label": "Phone", "lastUsedAt": 1_792_483_200}),
                ),
                object(json!({
                    "metadata": {"name": "c-3", "creationTimestamp": "2026-10-03T00:00:00Z"},
                    "spec": {"user": "bob",
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
            undecoded: Vec::new(),
            projects: None,
        }
    }

    #[test]
    fn a_member_has_their_passkeys_oldest_first_with_the_name_chosen_last() {
        let view = directory()
            .view(
                "zeabur-identity",
                Some("zeabur:ada"),
                Timestamp::from_second(1_000_000_000).unwrap(),
                SetupLinks {
                    issuer: None,
                    accepted: None,
                },
                Permission::Allowed,
                true,
            )
            .unwrap();
        let ada = &view.items[0];
        assert_eq!(ada.username, "ada");
        assert!(ada.you);
        assert_eq!(ada.role, MemberRole::Admin);
        let labels: Vec<_> = ada.passkeys.iter().map(|p| p.label.as_deref()).collect();
        assert_eq!(labels, [Some("Phone"), Some("MacBook")]);
        assert_eq!(ada.sessions, 1);
        // The recorded use is later than the grant's sign-in.
        assert_eq!(
            ada.last_sign_in_at,
            Some("2026-10-20T08:00:00Z".parse().unwrap())
        );
        let bob = &view.items[1];
        assert!(!bob.you);
        assert_eq!(bob.role, MemberRole::Member);
        assert_eq!(bob.passkeys.len(), 0);
        assert_eq!(bob.linked[0].provider, "GitHub");
        assert_eq!(bob.last_sign_in_at, None);
        assert_eq!(bob.invitations, 1);
        assert!(!ada.actions.can_remove && !ada.actions.can_change_access);
        // Bob's project roles were not read: saving his access would replace roles nobody saw.
        assert!(bob.actions.can_remove && !bob.actions.can_change_access);
    }

    #[test]
    fn expired_invitations_are_left_out_and_one_for_a_new_name_is_a_new_member() {
        let view = directory()
            .view(
                "zeabur-identity",
                None,
                Timestamp::from_second(1_000_000_000).unwrap(),
                SetupLinks {
                    issuer: None,
                    accepted: None,
                },
                Permission::Allowed,
                true,
            )
            .unwrap();
        assert_eq!(view.invitations.len(), 1);
        assert_eq!(view.invitations[0].username, "cy");
        assert!(view.invitations[0].new_member);
    }

    #[test]
    fn an_invitation_expiry_out_of_range_is_refused_not_dated() {
        let mut directory = directory();
        directory
            .invitations
            .push(object(json!({"metadata": {"name": "inv-far"},
            "spec": {"user": "dee", "tokenHash": "sha256:00", "expiresAt": i64::MAX}})));
        let refused = directory.view(
            "zeabur-identity",
            None,
            Timestamp::from_second(1_000_000_000).unwrap(),
            SetupLinks {
                issuer: None,
                accepted: None,
            },
            Permission::Allowed,
            true,
        );
        assert!(refused.is_err());
    }

    #[test]
    fn a_member_with_only_a_password_has_it_with_its_second_factor_and_last_use() {
        let mut directory = directory();
        directory.users.push(user("cy", false));
        directory.credentials.push(object(json!({
            "metadata": {"name": "password-cy", "creationTimestamp": "2026-10-04T00:00:00Z"},
            "status": {"lastUsedAt": 1_792_483_200},
            "spec": {"user": "cy", "password": {"hash": "$p$i=1$s$h",
                     "totp": {"secret": "sealed", "recovery": [], "enrolledStep": 1}}}
        })));
        let view = directory
            .view(
                "zeabur-identity",
                None,
                Timestamp::from_second(1_000_000_000).unwrap(),
                SetupLinks {
                    issuer: None,
                    accepted: None,
                },
                Permission::Allowed,
                true,
            )
            .unwrap();
        let cy = view.items.iter().find(|m| m.username == "cy").unwrap();
        let password = cy.password.as_ref().unwrap();
        assert!(password.second_factor);
        assert!(cy.passkeys.is_empty() && cy.linked.is_empty());
        assert_eq!(
            cy.last_sign_in_at,
            Some("2026-10-20T08:00:00Z".parse().unwrap())
        );
        // Ada and Bob have no password.
        assert!(
            view.items
                .iter()
                .filter(|m| m.username != "cy")
                .all(|m| m.password.is_none())
        );
    }

    #[test]
    fn a_password_without_a_second_factor_says_so() {
        let spec: CredentialSpec =
            object(json!({"user": "cy", "password": {"hash": "$p$i=1$s$h"}}));
        assert!(matches!(
            spec.kind,
            CredentialKind::Password(PasswordSpec { totp: None })
        ));
    }

    #[test]
    fn only_passkeys_count_towards_the_last_one() {
        let directory = directory();
        assert_eq!(directory.passkeys_of("ada"), 2);
        assert_eq!(directory.passkeys_of("bob"), 0);
        assert_eq!(directory.passkeys_of("nobody"), 0);
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

    #[test]
    fn a_credential_of_a_shape_zeabur_does_not_know_is_named_with_why() {
        let objects: Vec<DynamicObject> = object(json!([
            {"apiVersion": API_VERSION, "kind": "Credential",
             "metadata": {"name": "c-1"},
             "spec": {"user": "ada", "passkey": {"credentialId": "abc", "record": "{}"}}},
            {"apiVersion": API_VERSION, "kind": "Credential",
             "metadata": {"name": "c-2"},
             "spec": {"user": "ada", "securityKey": {"credentialId": "abc"}}},
        ]));
        let decoded =
            decode::<CredentialObject>(objects, "zeabur-identity", IdentityKind::Credential);
        let names: Vec<_> = decoded.objects.iter().map(Object::name).collect();
        assert_eq!(names, ["c-1"]);
        let [undecoded] = decoded.undecoded.as_slice() else {
            panic!("one undecoded object, got {:?}", decoded.undecoded);
        };
        assert_eq!(
            undecoded.origin,
            Origin::complete(API_VERSION, "Credential", Some("zeabur-identity"), "c-2")
        );
        assert!(
            undecoded.error.contains("CredentialKind"),
            "{}",
            undecoded.error
        );
    }
}
