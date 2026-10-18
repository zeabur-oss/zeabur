//! What every handler shares: the API's state and the person's connections.

use std::future::Future;
use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use engawa_core::problem::{Kind, Problem};
use engawa_kube::Cluster;

use crate::access::ClusterAccess;
use crate::error::ApiError;

/// Shared state of the API.
#[derive(Clone)]
pub struct AppState {
    /// How each request reaches the cluster as its person.
    pub access: Arc<dyn ClusterAccess>,
    /// The embedded recommendations catalogue.
    pub catalogue: Arc<Vec<engawa_recommend::Component>>,
    /// The key plan tokens are sealed with; `None` when none could be made.
    pub plan_key: Option<Arc<engawa_plan::token::PlanKey>>,
    /// The time now: `Timestamp::now`, or a fixed time in tests, so answers that depend on the
    /// date (end of life, plan expiry) can be checked on one day.
    pub clock: fn() -> engawa_kube::k8s_openapi::jiff::Timestamp,
    /// How long a terminal may idle before the server closes it ([`crate::pods::exec::IDLE`]).
    pub terminal_idle: std::time::Duration,
    /// engawa's settings as the server was started (see [`crate::settings`]).
    pub settings: Arc<crate::settings::Settings>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

/// The cluster connection of the person making the request: the administrator's while they hold
/// one (see [`ClusterAccess::for_request`]).
pub(crate) struct Person(pub(crate) Arc<dyn Cluster>);

impl<S> FromRequestParts<S> for Person
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let state = AppState::from_ref(state);
        let access = state.access.for_request(parts);
        std::future::ready(access.map(|access| Self(access.cluster)).map_err(ApiError))
    }
}

/// The person's everyday connection, also while they are elevated; only for binding a plan to
/// its maker (see [`ClusterAccess::everyday`]).
pub(crate) struct Everyday(pub(crate) Arc<dyn Cluster>);

impl<S> FromRequestParts<S> for Everyday
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let state = AppState::from_ref(state);
        std::future::ready(state.access.everyday(parts).map(Self).map_err(ApiError))
    }
}

/// An unknown API path.
pub(crate) async fn not_found() -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        "There is no such API endpoint.",
    ))
}
