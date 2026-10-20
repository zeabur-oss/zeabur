//! `GET /api/v1/session`: how the person is signed in to Zeabur itself, as opposed to who the
//! cluster says they are (`whoami`).
//!
//! The web interface reads it to know whether it may offer signing out (only where people sign
//! in) and whether the person holds administrator rights at the moment, and until when.

use std::future::Future;

use axum::Json;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};

use crate::access::Elevation;
use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::views::WhoAmI;

/// Whether administrator rights are offered and in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ElevationState {
    /// Not offered here.
    Unavailable,
    /// Offered; anything the cluster refuses can be retried as an administrator.
    Available,
    /// In use until `elevatedUntil`.
    Active,
}

/// How the person is signed in to Zeabur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    /// Whether people sign in to this server, and so can sign out. `false` on this machine
    /// (`zeaburctl ui`), which acts with the kubeconfig of whoever started it.
    pub signs_in: bool,
    /// When the credential in use expires, RFC 3339: the everyday one, which Zeabur renews
    /// before then while the session lasts, or while admin rights are in use theirs, which is
    /// `elevatedUntil` and is not renewed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// Whether administrator rights are offered and in use.
    pub elevation: ElevationState,
    /// When the administrator rights end, RFC 3339, while they are in use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevated_until: Option<String>,
}

/// The session of the request, read through [`crate::access::ClusterAccess`].
pub(crate) struct Current(SessionView);

impl<S> FromRequestParts<S> for Current
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let access = AppState::from_ref(state).access;
        let view = access.for_request(parts).map(|current| {
            let (elevation, elevated_until) = match current.elevation {
                Elevation::Unavailable => (ElevationState::Unavailable, None),
                Elevation::Available => (ElevationState::Available, None),
                Elevation::Active { until } => (ElevationState::Active, Some(until.to_string())),
            };
            SessionView {
                signs_in: access.signs_in(),
                expires_at: access.credential_expiry(parts).map(|t| t.to_string()),
                elevation,
                elevated_until,
            }
        });
        std::future::ready(view.map(Self).map_err(ApiError))
    }
}

/// How you are signed in to Zeabur: whether you can sign out, and admin rights in use.
#[utoipa::path(get, path = "/api/v1/session", tag = "identity", operation_id = "getSession",
    responses((status = 200, body = SessionView),
              (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(Current(view): Current) -> Json<SessionView> {
    Json(view)
}

/// Who the cluster says you are.
#[utoipa::path(get, path = "/api/v1/whoami", tag = "identity",
    responses((status = 200, body = WhoAmI), (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn whoami(Person(cluster): Person) -> Result<Json<WhoAmI>, ApiError> {
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ApiError::from(e.into_problem("your identity")))?;
    Ok(Json(WhoAmI {
        username: identity.username,
        groups: identity.groups,
    }))
}
