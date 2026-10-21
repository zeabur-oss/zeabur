//! `/api/v1/members`: everyone who can sign in, invitations, setup links and removals.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use zeabur_core::message::Message;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{Cluster, Preconditions, WriteBody, WriteRequest};

use super::USER_PREFIX;
use super::directory::{
    CredentialSpec, Directory, IdentityKind, MembersView, Object, SetupLinks, can_manage, request,
    setup_links,
};
use super::writes::{
    INVITE_SECONDS, InviteRequest, RESET_SECONDS, SetupLinkRequest, SetupLinkView, bad, clear,
    create_user, delete, is_username, issue, refuse, remove_member, target, user_object,
};
use crate::error::{ApiError, read_problem};
use crate::handlers::{AppState, Person};

/// The problem when Zeabur Identity is not installed.
pub(crate) fn not_installed(namespace: &str) -> ApiError {
    let mut problem = Problem::new(
        Kind::NotFound,
        format!("Zeabur Identity is not installed in this cluster (no users in {namespace})."),
    );
    problem.summary = Message::new("members.notInstalled");
    problem.fix_for_occasion(
        "Install Zeabur Identity, or start Zeabur with the namespace it runs in.",
        Message::new("members.notInstalled.fix"),
    );
    ApiError::from(problem)
}

/// The issuer setup links start with, or the problem that says why there is none.
pub(crate) fn issuer_of(links: &SetupLinks) -> Result<String, ApiError> {
    links.issuer.clone().ok_or_else(|| {
        refuse(
            "Zeabur does not know Zeabur Identity's address, so it cannot make a link.",
            "Start Zeabur with --issuer-url, or let it read the zeabur-identity Deployment.",
            "members.noIssuer",
        )
    })
}

/// Who the person is to Kubernetes; empty when the cluster will not say.
pub(crate) async fn you(cluster: &dyn Cluster) -> String {
    cluster.whoami().await.map_or_default(|i| i.username)
}

/// The directory, or the problem when Zeabur Identity is not installed.
pub(crate) async fn directory(
    cluster: &dyn Cluster,
    namespace: &str,
) -> Result<Directory, ApiError> {
    Directory::load(cluster, namespace)
        .await?
        .ok_or_else(|| not_installed(namespace))
}

fn no_member(username: &str) -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        format!("There is no member {username}."),
    ))
}

/// Everyone who can sign in, and the setup links waiting to be used.
#[utoipa::path(get, path = "/api/v1/members", tag = "members", operation_id = "listMembers",
    responses((status = 200, body = MembersView),
              (status = 401, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn list(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<MembersView>, ApiError> {
    let identity = &state.settings.identity;
    let namespace = identity.namespace.as_str();
    let (directory, you, links, can) = futures_util::join!(
        Directory::load(cluster.as_ref(), namespace),
        you(cluster.as_ref()),
        setup_links(cluster.as_ref(), identity),
        can_manage(cluster.as_ref(), namespace),
    );
    let Some(directory) = directory? else {
        return Ok(Json(MembersView::not_installed(links, can)));
    };
    Ok(Json(directory.view(
        namespace,
        Some(&you),
        (state.clock)(),
        links,
        can,
    )))
}

/// Invites someone: a one-time link that adds a passkey to their account, made a member if new.
#[utoipa::path(post, path = "/api/v1/members/invitations", tag = "members",
    operation_id = "inviteMember", request_body = InviteRequest,
    responses((status = 201, body = SetupLinkView),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn invite(
    State(state): State<AppState>,
    Person(cluster): Person,
    Json(invite): Json<InviteRequest>,
) -> Result<(StatusCode, Json<SetupLinkView>), ApiError> {
    let username = invite.username.trim().to_owned();
    if !is_username(&username) {
        return Err(bad(format!(
            "{username:?} cannot be a user name: use lower-case letters, digits, - and ., at most \
             63 characters."
        )));
    }
    let invite = InviteRequest { username, ..invite };
    let identity = &state.settings.identity;
    let namespace = identity.namespace.as_str();
    let issuer = issuer_of(&setup_links(cluster.as_ref(), identity).await)?;
    let (user, created_member) =
        match user_object(cluster.as_ref(), namespace, &invite.username).await? {
            Some(user) => (user, false),
            None => (
                create_user(cluster.as_ref(), namespace, &invite).await?,
                true,
            ),
        };
    let (url, name, expires_at) = issue(
        cluster.as_ref(),
        namespace,
        &issuer,
        &user,
        (state.clock)(),
        INVITE_SECONDS,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(SetupLinkView::new(
            url,
            invite.username,
            expires_at,
            name,
            created_member,
            false,
        )),
    ))
}

/// Revokes a setup link that has not been used. One already used or expired is gone already.
#[utoipa::path(delete, path = "/api/v1/members/invitations/{name}", tag = "members",
    operation_id = "revokeInvitation",
    params(("name" = String, Path, description = "The `Invitation` object's name")),
    responses((status = 204),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn revoke_invitation(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let namespace = state.settings.identity.namespace.as_str();
    let write = WriteRequest {
        target: target(IdentityKind::Invitation, namespace, &name),
        body: WriteBody::Delete(Preconditions::default()),
        dry_run: false,
    };
    match cluster.write(&write).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(error) if error.code() == Some(404) => Ok(StatusCode::NO_CONTENT),
        Err(error) => Err(read_problem(
            error,
            &format!("Invitation {name}"),
            request("delete", IdentityKind::Invitation, namespace, Some(&name)),
        )),
    }
}

/// A new setup link for a member: to add a passkey, or (`reset`) to start over after a lost one.
#[utoipa::path(post, path = "/api/v1/members/{username}/setup-links", tag = "members",
    operation_id = "createSetupLink", request_body = SetupLinkRequest,
    params(("username" = String, Path, description = "The member's user name")),
    responses((status = 201, body = SetupLinkView),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn setup_link(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(username): Path<String>,
    Json(link): Json<SetupLinkRequest>,
) -> Result<(StatusCode, Json<SetupLinkView>), ApiError> {
    let identity = &state.settings.identity;
    let namespace = identity.namespace.as_str();
    let issuer = issuer_of(&setup_links(cluster.as_ref(), identity).await)?;
    let (directory, me) = futures_util::join!(
        directory(cluster.as_ref(), namespace),
        you(cluster.as_ref())
    );
    let directory = directory?;
    let Some(user) = user_object(cluster.as_ref(), namespace, &username).await? else {
        return Err(no_member(&username));
    };
    if link.reset {
        if me == format!("{USER_PREFIX}{username}") {
            return Err(refuse(
                "You cannot reset your own account: it would remove the passkey you are using.",
                "Add a passkey from your account instead, or ask another admin.",
                "members.resetSelf",
            ));
        }
        clear(cluster.as_ref(), namespace, &directory, &username).await?;
    }
    let (url, name, expires_at) = issue(
        cluster.as_ref(),
        namespace,
        &issuer,
        &user,
        (state.clock)(),
        RESET_SECONDS,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(SetupLinkView::new(
            url, username, expires_at, name, false, link.reset,
        )),
    ))
}

/// The rule for removing a credential of `username` as `you`: never your own last passkey.
pub(crate) fn check_removal(
    directory: &Directory,
    credential: &Object<CredentialSpec>,
    you: &str,
) -> Result<(), ApiError> {
    let owner = &credential.spec.user;
    let own = you == format!("{USER_PREFIX}{owner}");
    if own && credential.spec.kind == "Passkey" && directory.passkeys_of(owner) <= 1 {
        return Err(refuse(
            "This is your only passkey. Admin rights need a passkey, so removing it would lock you \
             out.",
            "Add another passkey first, then remove this one.",
            "account.lastPasskey",
        ));
    }
    Ok(())
}

/// Finds `username`'s credential `id`.
pub(crate) fn credential<'a>(
    directory: &'a Directory,
    username: &str,
    id: &str,
) -> Result<&'a Object<CredentialSpec>, ApiError> {
    directory
        .credentials
        .iter()
        .find(|c| c.name() == id && c.spec.user == username)
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::NotFound,
                format!("{username} has no passkey or linked account {id}."),
            ))
        })
}

/// Removes a member's passkey or linked account. Sessions it started keep renewing until they
/// end or are signed out; your own last passkey is refused.
#[utoipa::path(delete, path = "/api/v1/members/{username}/credentials/{id}", tag = "members",
    operation_id = "removeMemberCredential",
    params(("username" = String, Path, description = "The member's user name"),
           ("id" = String, Path, description = "The `Credential` object's name")),
    responses((status = 204),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn remove_passkey(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((username, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let namespace = state.settings.identity.namespace.as_str();
    let (directory, me) = futures_util::join!(
        directory(cluster.as_ref(), namespace),
        you(cluster.as_ref())
    );
    let directory = directory?;
    let found = credential(&directory, &username, &id)?;
    check_removal(&directory, found, &me)?;
    delete(cluster.as_ref(), IdentityKind::Credential, namespace, found).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes a member: their links, passkeys, linked accounts and sessions, then the member.
/// Projects and everything in the cluster stay; RBAC bindings that name them stay too.
#[utoipa::path(delete, path = "/api/v1/members/{username}", tag = "members",
    operation_id = "removeMember",
    params(("username" = String, Path, description = "The member's user name")),
    responses((status = 204),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn remove(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(username): Path<String>,
) -> Result<StatusCode, ApiError> {
    let namespace = state.settings.identity.namespace.as_str();
    let (directory, me) = futures_util::join!(
        directory(cluster.as_ref(), namespace),
        you(cluster.as_ref())
    );
    let directory = directory?;
    if me == format!("{USER_PREFIX}{username}") {
        return Err(refuse(
            "You cannot remove yourself.",
            "Ask another admin to remove you.",
            "members.removeSelf",
        ));
    }
    let Some(user) = directory.user(&username) else {
        return Err(no_member(&username));
    };
    remove_member(cluster.as_ref(), namespace, &directory, user).await?;
    Ok(StatusCode::NO_CONTENT)
}

impl MembersView {
    fn not_installed(
        setup_links: SetupLinks,
        can_manage: crate::projects::namespace::Permission,
    ) -> Self {
        Self {
            id: "members".to_owned(),
            installed: false,
            items: Vec::new(),
            invitations: Vec::new(),
            setup_links,
            can_manage,
            unreadable: Vec::new(),
        }
    }
}

impl SetupLinkView {
    pub(crate) fn new(
        url: String,
        username: String,
        expires_at: i64,
        name: String,
        created_member: bool,
        reset: bool,
    ) -> Self {
        Self {
            url,
            username,
            expires_at: zeabur_kube::k8s_openapi::jiff::Timestamp::from_second(expires_at)
                .map_or_default(|t| t.to_string()),
            name,
            created_member,
            reset,
        }
    }
}
