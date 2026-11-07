//! What every handler shares: the API's state and the person's connections.

use std::any::Any;
use std::future::Future;
use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::response::{IntoResponse as _, Response};
use tower_http::catch_panic::CatchPanicLayer;
use zeabur_core::problem::{self, Kind, Problem};
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
    pub zeabur_proxy: Option<Arc<dyn crate::services::proxy::ZeaburProxy>>,
    /// Reads images' configurations from their registries (see [`crate::images`]).
    pub images: Arc<crate::images::Inspector>,
    /// The client of GitHub's API; `None` when TLS could not be set up.
    pub github: Option<crate::images::https::Https>,
    /// Zeabur's releases as GitHub listed them last (see [`crate::cluster::update`]).
    pub releases: Arc<crate::cluster::update::Releases>,
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

/// Answers a handler's panic as an `internal` problem. The panic's message goes to the log, not
/// to the person: it can name Zeabur's internals.
pub(crate) fn catch_panic() -> CatchPanicLayer<fn(Box<dyn Any + Send>) -> Response> {
    CatchPanicLayer::custom(panicked)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "the signature CatchPanicLayer calls"
)]
fn panicked(payload: Box<dyn Any + Send>) -> Response {
    let message = if let Some(text) = payload.downcast_ref::<&str>() {
        text
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.as_str()
    } else {
        "(not text)"
    };
    tracing::error!(panic = message, "a request handler panicked");
    ApiError::from(Problem::new(
        Kind::Internal,
        "Zeabur hit a bug while answering this request.",
    ))
    .into_response()
}

/// A known API path asked with a method it does not take: an `about:blank` problem, as HTTP's
/// own `405` says all there is (RFC 9457, section 4.2.1). axum adds the `Allow` header.
pub(crate) async fn method_not_allowed(method: axum::http::Method) -> Response {
    let status = axum::http::StatusCode::METHOD_NOT_ALLOWED;
    let body = serde_json::json!({
        "type": "about:blank",
        "title": status.canonical_reason(),
        "status": status.as_u16(),
        "detail": format!("This API endpoint does not accept {method} requests."),
    });
    let content_type = [(axum::http::header::CONTENT_TYPE, problem::MEDIA_TYPE)];
    axum::response::IntoResponse::into_response((status, content_type, body.to_string()))
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use axum::routing::get;
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;

    use super::*;

    #[tokio::test]
    async fn a_panic_is_answered_as_a_problem_without_its_message() {
        async fn boom() {
            panic!("secret internals");
        }
        let app = Router::new().route("/", get(boom)).layer(catch_panic());
        let request = Request::builder().uri("/").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("/problems/internal"), "{body}");
        assert!(!body.contains("secret internals"), "{body}");
    }
}
