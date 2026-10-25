//! What every handler shares: the API's state and the person's connections.

use std::future::Future;
use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use zeabur_core::problem::{Kind, Problem};
use zeabur_kube::Cluster;

use crate::access::ClusterAccess;
use crate::error::ApiError;

/// Shared state of the API.
#[derive(Clone)]
pub struct AppState {
    /// How each request reaches the cluster as its person.
    pub access: Arc<dyn ClusterAccess>,
    /// The embedded Zeabur Catalog.
    pub catalogue: Arc<Vec<zeabur_catalog::Component>>,
    /// What a template uses when idle: [`zeabur_catalog::template_idle`], or made-up use in the
    /// demo.
    pub template_idle: fn(&str) -> Option<zeabur_catalog::IdleUse>,
    /// The key plan tokens are sealed with; `None` when none could be made.
    pub plan_key: Option<Arc<crate::plan_token::PlanKey>>,
    /// The time now: `Timestamp::now`, or a fixed time in tests, so answers that depend on the
    /// date (end of life, plan expiry) can be checked on one day.
    pub clock: fn() -> zeabur_kube::k8s_openapi::jiff::Timestamp,
    /// How long a terminal may idle before the server closes it ([`crate::pods::exec::IDLE`]).
    pub terminal_idle: std::time::Duration,
    /// Zeabur's settings as the server was started (see [`crate::settings`]).
    pub settings: Arc<crate::settings::Settings>,
    /// How domains are looked up for a route's trace ([`crate::resolve`]).
    pub resolver: Arc<dyn crate::resolve::Resolve>,
    /// The open terminals of tools pods (see [`crate::tools`]).
    pub tools: Arc<crate::tools::Sessions>,
    /// Where the issuer lets people manage how they sign in (see [`crate::account`]).
    pub sign_in: Arc<dyn crate::account::SignInPage>,
    /// Zeabur Proxy, for log history (see [`crate::services::logs`]); `None` without one.
    pub log_proxy: Option<Arc<dyn crate::services::logs::LogProxy>>,
    /// Reads images' configurations from their registries (see [`crate::images`]).
    pub images: Arc<crate::images::Inspector>,
    /// The client of GitHub's API; `None` when TLS could not be set up.
    pub github: Option<crate::images::https::Https>,
    /// Pushes uploaded folders to the registry (see [`crate::projects::source_upload`]).
    pub pusher: Arc<crate::projects::oci_push::Pusher>,
    /// Reads backups from their buckets (see [`crate::cluster::object_store`]).
    pub store: Arc<crate::cluster::object_store::Fetcher>,
    /// Zeabur's own service account, which may use pods in the terminal namespace only (see
    /// [`crate::tools`]); `None` where there is none (local mode).
    pub terminals: Option<Arc<dyn Cluster>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

/// The cluster connection of the person making the request (see [`ClusterAccess::for_request`]).
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
        std::future::ready(access.map(Self).map_err(ApiError))
    }
}

/// The person's token as the session holds it now, where Zeabur holds one (see
/// [`ClusterAccess::handoff`]); only for a tools pod and Zeabur Proxy.
pub(crate) struct Credential(pub(crate) Option<crate::access::Handoff>);

impl<S> FromRequestParts<S> for Credential
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let state = AppState::from_ref(state);
        std::future::ready(Ok(Self(state.access.handoff(parts))))
    }
}

/// An unknown API path.
pub(crate) async fn not_found() -> ApiError {
    ApiError::from(Problem::new(
        Kind::NotFound,
        "There is no such API endpoint.",
    ))
}
