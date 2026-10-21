//! `/api/v1/account`: your own account, passkeys and the link that adds one.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::Cluster;

use super::directory::{
    AccountStatus, AccountView, Directory, IdentityKind, can_manage, identity_username, setup_links,
};
use super::handlers::{check_removal, credential, issuer_of};
use super::writes::{
    OWN_SECONDS, PasskeyLabel, SetupLinkView, clean_label, delete, issue, rename, user_object,
};
use crate::error::ApiError;
use crate::handlers::{AppState, Person};

/// Your Zeabur Identity user name, or the refusal that says why there is none.
async fn own_username(cluster: &dyn Cluster) -> Result<(String, String), ApiError> {
    let you = super::handlers::you(cluster).await;
    let Some(username) = identity_username(&you).map(str::to_owned) else {
        return Err(ApiError::from(Problem::new(
            Kind::NotFound,
            format!(
                "You are signed in as {you}, which is not a Zeabur Identity account, so it has no \
                 passkeys here."
            ),
        )));
    };
    Ok((you, username))
}

/// Your account: who you are, your passkeys and linked accounts. Reading passkeys needs the
/// rights to read Zeabur Identity's credentials, which only admins have by default.
#[utoipa::path(get, path = "/api/v1/account", tag = "members", operation_id = "getAccount",
    responses((status = 200, body = AccountView),
              (status = 401, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<AccountView>, ApiError> {
    let identity = &state.settings.identity;
    let namespace = identity.namespace.as_str();
    let (you, links, can) = futures_util::join!(
        super::handlers::you(cluster.as_ref()),
        setup_links(cluster.as_ref(), identity),
        can_manage(cluster.as_ref(), namespace),
    );
    let answer = |status, member| AccountView {
        id: "account".to_owned(),
        kubernetes_username: you.clone(),
        status,
        member,
        setup_links: links.clone(),
        can_manage: can,
    };
    let Some(username) = identity_username(&you) else {
        return Ok(Json(answer(AccountStatus::Kubernetes, None)));
    };
    let Some(directory) = Directory::load(cluster.as_ref(), namespace).await? else {
        return Ok(Json(answer(AccountStatus::NotInstalled, None)));
    };
    Ok(Json(directory.user(username).map_or_else(
        || answer(AccountStatus::Missing, None),
        |user| {
            answer(
                AccountStatus::Identity,
                Some(directory.member(user, namespace, Some(&you))),
            )
        },
    )))
}

/// A one-time link that adds a passkey to your account, valid for an hour. Open it on the device
/// or in the password manager that should hold the passkey.
#[utoipa::path(post, path = "/api/v1/account/setup-links", tag = "members",
    operation_id = "createOwnSetupLink",
    responses((status = 201, body = SetupLinkView),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn setup_link(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<(StatusCode, Json<SetupLinkView>), ApiError> {
    let identity = &state.settings.identity;
    let namespace = identity.namespace.as_str();
    let (_, username) = own_username(cluster.as_ref()).await?;
    let issuer = issuer_of(&setup_links(cluster.as_ref(), identity).await)?;
    let Some(user) = user_object(cluster.as_ref(), namespace, &username).await? else {
        return Err(ApiError::from(Problem::new(
            Kind::NotFound,
            format!("Zeabur Identity has no account {username}."),
        )));
    };
    let (url, name, expires_at) = issue(
        cluster.as_ref(),
        namespace,
        &issuer,
        &user,
        (state.clock)(),
        OWN_SECONDS,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(SetupLinkView::new(
            url, username, expires_at, name, false, false,
        )),
    ))
}

/// Renames one of your passkeys; an empty name goes back to the one given when it was added.
#[utoipa::path(patch, path = "/api/v1/account/passkeys/{id}", tag = "members",
    operation_id = "renamePasskey", request_body = PasskeyLabel,
    params(("id" = String, Path, description = "The `Credential` object's name")),
    responses((status = 204),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn rename_passkey(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(id): Path<String>,
    Json(label): Json<PasskeyLabel>,
) -> Result<StatusCode, ApiError> {
    let namespace = state.settings.identity.namespace.as_str();
    let label = clean_label(label.label.as_deref())?;
    let (_, username) = own_username(cluster.as_ref()).await?;
    let directory = super::handlers::directory(cluster.as_ref(), namespace).await?;
    let found = credential(&directory, &username, &id)?;
    rename(cluster.as_ref(), namespace, found, label.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes one of your passkeys or linked accounts. Your last passkey is refused: admin rights
/// need one, and without it you could not sign in again.
#[utoipa::path(delete, path = "/api/v1/account/passkeys/{id}", tag = "members",
    operation_id = "removePasskey",
    params(("id" = String, Path, description = "The `Credential` object's name")),
    responses((status = 204),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn remove_passkey(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let namespace = state.settings.identity.namespace.as_str();
    let (you, username) = own_username(cluster.as_ref()).await?;
    let directory = super::handlers::directory(cluster.as_ref(), namespace).await?;
    let found = credential(&directory, &username, &id)?;
    check_removal(&directory, found, &you)?;
    delete(cluster.as_ref(), IdentityKind::Credential, namespace, found).await?;
    Ok(StatusCode::NO_CONTENT)
}
