//! The server: sign-in through Zeabur Identity, the API acting with each person's token, and the
//! web interface.
//!
//! | Method and path | What it does |
//! |---|---|
//! | `GET /api/healthz` | `ok`; outside every check below, for the kubelet |
//! | `GET /auth/login?return=/path` | starts signing in; afterwards the browser goes to `return` |
//! | `GET /auth/callback` | where Zeabur Identity sends the browser back after signing in |
//! | `POST /auth/logout` | ends the session and revokes its refresh token at Zeabur Identity |
//! | `/api/…` | the API (`zeabur_api`), as the person the session belongs to |
//! | anything else | the web interface; a page opened without a session goes to `/auth/login` first |
//!
//! Every request but the health check passes [`zeabur_core::guard`] first: its `Host` must be
//! the public URL's, and a request that changes something must come from Zeabur's own page.
//!
//! **Command line.** A request with no session cookie may carry `Authorization: Bearer <token>`,
//! the person's own token for kube-apiserver (`zeaburctl up`). It must be a JWT from Zeabur
//! Identity: Zeabur reads its `iss` and does not use another token. Zeabur acts with it as with
//! a session's, so kube-apiserver checks the signature and decides what it may do. It is not
//! kept, and a tools pod or Zeabur Proxy never gets it. The guard still applies, so such a client
//! sends `Origin` and the custom header.
//!
//! **Idle sessions.** A request with the header [`BACKGROUND`] does not count as activity (see
//! [`Activity`]): the interface sends it on requests it makes on its own after a minute without
//! input, so an open tab does not keep a session alive.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use secrecy::ExposeSecret as _;
use zeabur_api::ApiError;
use zeabur_api::access::{ClusterAccess, Handoff};
use zeabur_api::account::SignInPage;
use zeabur_api::plan_token::PlanKey;
use zeabur_api::services::logs::LogProxy;
use zeabur_auth::client::{AuthError, OAuthClient};
use zeabur_auth::cookie;
use zeabur_auth::flow::{FlowKey, LIFETIME_SECONDS, Pending, return_path};
use zeabur_auth::passkey::PasskeyEndpoints;
use zeabur_auth::session::{Activity, Bind, Clock, Credentials, Lookup, Sessions};
use zeabur_auth::url::BaseUrl;
use zeabur_core::guard::{self, Origins, Refusal};
use zeabur_core::problem::{Kind, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, UserToken};

use crate::pages::{self, SignInProblem};

/// The header the interface sends on a request it makes on its own while nobody uses the page.
pub(crate) const BACKGROUND: &str = "x-zeabur-background";

/// A cluster connection bound to one person's token, and the token, which only a tools pod is
/// handed (`zeabur_api::tools`).
#[derive(Clone)]
pub(crate) struct Connection {
    pub(crate) cluster: Arc<dyn Cluster>,
    pub(crate) token: UserToken,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").finish_non_exhaustive()
    }
}

/// What the sign-in routes and the API share.
pub(crate) struct Auth {
    pub(crate) public: BaseUrl,
    /// The Zeabur Identity issuer. A bearer token must name it as `iss`.
    pub(crate) issuer: BaseUrl,
    pub(crate) oauth: Arc<OAuthClient>,
    pub(crate) sessions: Arc<Sessions<Connection>>,
    pub(crate) connect: Bind<Connection>,
    pub(crate) flow_key: FlowKey,
    pub(crate) clock: Clock,
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Auth")
            .field("public", &self.public)
            .finish_non_exhaustive()
    }
}

/// The session a request to the API belongs to, found by [`session`].
#[derive(Clone)]
enum Signed {
    In(Credentials<Connection>),
    /// A bearer token the request presented itself.
    Token(Connection),
    IssuerDown,
}

/// The issuer's page for managing how people sign in, from its passkey endpoints.
pub(crate) struct IssuerPage(pub(crate) PasskeyEndpoints);

impl SignInPage for IssuerPage {
    fn find(&self) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>> {
        Box::pin(self.0.manage())
    }
}

/// The whole server.
pub(crate) fn app(
    auth: &Arc<Auth>,
    plan_key: PlanKey,
    settings: zeabur_api::Settings,
    sign_in: Arc<dyn SignInPage>,
    log_proxy: Option<Arc<dyn LogProxy>>,
) -> Router {
    let access = Arc::new(SessionAccess {
        origin: auth.public.origin(),
    });
    let api = zeabur_api::router_with_key(access, plan_key, settings, sign_in, log_proxy).layer(
        axum::middleware::from_fn_with_state(Arc::clone(auth), session),
    );
    let sign_in = Router::new()
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/logout", post(logout))
        .with_state(Arc::clone(auth));
    let guarded = api.merge(sign_in);
    #[cfg(feature = "ui")]
    let guarded = guarded.fallback_service(zeabur_ui::router().layer(
        axum::middleware::from_fn_with_state(Arc::clone(auth), sign_in_first),
    ));
    let guarded = guarded.layer(axum::middleware::from_fn_with_state(
        Arc::clone(auth),
        check,
    ));
    Router::new()
        .route("/api/healthz", get(|| async { "ok" }))
        .merge(guarded)
}

fn problem(kind: Kind, detail: &str) -> Response {
    let mut response = ApiError::from(Problem::new(kind, detail)).into_response();
    pages::secure(response.headers_mut());
    response
}

/// [`zeabur_core::guard`] on every request, for the public URL.
async fn check(State(auth): State<Arc<Auth>>, request: Request, next: Next) -> Response {
    let origins = Origins::new([auth.public.origin()]);
    match guard::check(&origins, request.method(), request.headers()) {
        Ok(()) => next.run(request).await,
        Err(Refusal::Host) => problem(
            Kind::Forbidden,
            "This address does not belong to this Zeabur. Open Zeabur at its own address.",
        ),
        Err(Refusal::CrossSite | Refusal::NotFromZeabur) => problem(
            Kind::Forbidden,
            "Zeabur accepts changes only from its own page.",
        ),
    }
}

/// Sends a browser that opens a page without a session straight to `/auth/login`, so it does
/// not load the interface only for its script to leave for Zeabur Identity. Anything but a
/// top-level navigation (scripts, files, the API) is served as before; the interface still sends
/// the browser to sign in when the API answers `signed-out`.
#[cfg(feature = "ui")]
async fn sign_in_first(State(auth): State<Arc<Auth>>, request: Request, next: Next) -> Response {
    if request.method() != axum::http::Method::GET
        || !navigation(request.headers())
        || !zeabur_ui::is_page(request.uri().path())
    {
        return next.run(request).await;
    }
    let signed_in = match cookie::get(request.headers(), cookie::SESSION) {
        Some(id) => !matches!(
            auth.sessions.current(id, Activity::Person).await,
            Lookup::SignedOut
        ),
        None => false,
    };
    if signed_in {
        return next.run(request).await;
    }
    let back = return_path(
        request
            .uri()
            .path_and_query()
            .map(axum::http::uri::PathAndQuery::as_str),
    );
    if back == "/" {
        redirect("/auth/login", &[])
    } else {
        let back: String = form_urlencoded::byte_serialize(back.as_bytes()).collect();
        redirect(&format!("/auth/login?return={back}"), &[])
    }
}

/// Whether the browser is loading a document to show, by Fetch Metadata or, from a browser
/// without it, by `Accept`.
#[cfg(feature = "ui")]
fn navigation(headers: &HeaderMap) -> bool {
    match (headers.get("sec-fetch-mode"), headers.get("sec-fetch-dest")) {
        (Some(mode), dest) => mode == "navigate" && dest.is_none_or(|dest| dest == "document"),
        (None, Some(dest)) => dest == "document",
        (None, None) => headers
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|accept| accept.contains("text/html")),
    }
}

/// Finds the session of an API request (renewing its token when due) and hands it to
/// [`SessionAccess`] through the request's extensions.
async fn session(State(auth): State<Arc<Auth>>, mut request: Request, next: Next) -> Response {
    let signed = if let Some(id) = cookie::get(request.headers(), cookie::SESSION) {
        let activity = if request.headers().contains_key(BACKGROUND) {
            Activity::Background
        } else {
            Activity::Person
        };
        match auth.sessions.current(id, activity).await {
            Lookup::Active(credentials) => Some(Signed::In(credentials)),
            Lookup::IssuerUnavailable => Some(Signed::IssuerDown),
            Lookup::SignedOut => None,
        }
    } else {
        bearer(request.headers())
            .filter(|token| zeabur_kube::issued_by(token.expose_secret(), auth.issuer.as_str()))
            .and_then(|token| (auth.connect)(token).ok().map(Signed::Token))
    };
    if let Some(signed) = signed {
        request.extensions_mut().insert(signed);
    }
    next.run(request).await
}

/// The token of `Authorization: Bearer <token>`.
fn bearer(headers: &HeaderMap) -> Option<secrecy::SecretString> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty())
        .then(|| secrecy::SecretString::from(token.trim().to_owned()))
}

/// The API's view of sessions: every request with the session's token.
#[derive(Debug)]
struct SessionAccess {
    /// The public URL's origin, which a terminal's handshake must come from.
    origin: String,
}

fn signed_out() -> Box<Problem> {
    Box::new(Problem::new(
        Kind::SignedOut,
        "Your session ended, or you signed out. Sign in to use Zeabur.",
    ))
}

fn connection(parts: &Parts) -> Result<&Connection, Box<Problem>> {
    match parts.extensions.get::<Signed>() {
        Some(Signed::In(credentials)) => Ok(&credentials.base.value),
        Some(Signed::Token(connection)) => Ok(connection),
        Some(Signed::IssuerDown) => {
            let mut problem = Problem::new(
                Kind::Unavailable,
                "Your sign-in needs renewing, and Zeabur cannot reach the sign-in service.",
            );
            problem.fix_for_occasion(
                "Try again in a moment.",
                zeabur_core::message::Message::new("problem.unavailable.fix.tryAgainSoon"),
            );
            Err(Box::new(problem))
        }
        None => Err(signed_out()),
    }
}

/// The session's credentials; a bare token has none.
fn credentials(parts: &Parts) -> Option<&Credentials<Connection>> {
    match parts.extensions.get::<Signed>() {
        Some(Signed::In(credentials)) => Some(credentials),
        Some(Signed::Token(_) | Signed::IssuerDown) | None => None,
    }
}

fn timestamp(seconds: i64) -> Timestamp {
    Timestamp::from_second(seconds).unwrap_or(Timestamp::UNIX_EPOCH)
}

impl ClusterAccess for SessionAccess {
    fn for_request(&self, parts: &Parts) -> Result<Arc<dyn Cluster>, Box<Problem>> {
        Ok(Arc::clone(&connection(parts)?.cluster))
    }

    fn handoff(&self, parts: &Parts) -> Option<Handoff> {
        let base = &credentials(parts)?.base;
        Some(Handoff {
            token: base.value.token.clone(),
            expires_at: timestamp(base.expires_at),
        })
    }

    fn signed_out(&self, parts: &Parts) -> Option<tokio::sync::watch::Receiver<bool>> {
        Some(credentials(parts)?.signed_out.clone())
    }

    fn signs_in(&self) -> bool {
        true
    }

    fn public_origin(&self) -> Option<&str> {
        Some(&self.origin)
    }
}

fn redirect(location: &str, cookies: &[String]) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::SEE_OTHER;
    let headers = response.headers_mut();
    if let Ok(location) = HeaderValue::from_str(location) {
        headers.insert(header::LOCATION, location);
    }
    set_cookies(headers, cookies);
    pages::secure(headers);
    response
}

fn set_cookies(headers: &mut HeaderMap, cookies: &[String]) {
    for value in cookies {
        if let Ok(value) = HeaderValue::from_str(value) {
            headers.append(header::SET_COOKIE, value);
        }
    }
}

/// The problem detail when a sign-in cannot start for want of randomness.
const RANDOM_FAILED: &str =
    "The system random number generator failed, so Zeabur could not start a sign-in.";

/// Sends the browser to Zeabur Identity to sign in, back to `return_to` afterwards.
async fn start(auth: &Auth, headers: &HeaderMap, return_to: String) -> Response {
    let Ok(pending) = Pending::start(return_to, (auth.clock)()) else {
        return problem(Kind::Internal, RANDOM_FAILED);
    };
    let (Ok(sealed), url) = (
        pending.seal(&auth.flow_key),
        auth.oauth.authorization_url(&pending).await,
    ) else {
        return problem(Kind::Internal, RANDOM_FAILED);
    };
    match url {
        Ok(url) => redirect(
            &url,
            &[cookie::set(
                cookie::SIGN_IN,
                &sealed,
                Some(LIFETIME_SECONDS),
            )],
        ),
        Err(error) => {
            tracing::warn!(event = "auth.discovery", %error, "cannot start a sign-in");
            pages::sign_in_problem(headers, SignInProblem::IssuerUnreachable)
        }
    }
}

/// The query parameters of `uri`; of a repeated one, the first.
fn parameters(uri: &Uri) -> HashMap<String, String> {
    let mut parameters = HashMap::new();
    for (name, value) in form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()) {
        parameters
            .entry(name.into_owned())
            .or_insert_with(|| value.into_owned());
    }
    parameters
}

/// `GET /auth/login?return=/path`.
async fn login(State(auth): State<Arc<Auth>>, uri: Uri, headers: HeaderMap) -> Response {
    let query = parameters(&uri);
    let return_to = return_path(query.get("return").map(String::as_str));
    start(&auth, &headers, return_to).await
}

/// `GET /auth/callback`: Zeabur Identity's answer to a sign-in.
async fn callback(State(auth): State<Arc<Auth>>, uri: Uri, headers: HeaderMap) -> Response {
    let query = parameters(&uri);
    let state = query.get("state").map_or("", String::as_str);
    let now = (auth.clock)();
    let Some(pending) = cookie::get(&headers, cookie::SIGN_IN)
        .and_then(|sealed| Pending::open(&auth.flow_key, sealed, now))
        .filter(|pending| pending.matches(state))
    else {
        return pages::sign_in_problem(&headers, SignInProblem::Expired);
    };
    let mut response = sign_in(&auth, &headers, &query, &pending).await;
    set_cookies(response.headers_mut(), &[cookie::clear(cookie::SIGN_IN)]);
    response
}

/// The code of a successful answer from the configured issuer, or why there is none.
async fn code<'a>(
    auth: &Auth,
    query: &'a HashMap<String, String>,
) -> Result<&'a str, Result<String, AuthError>> {
    if let Err(error) = auth
        .oauth
        .check_issuer(query.get("iss").map(String::as_str))
        .await
    {
        return Err(Err(error));
    }
    if let Some(error) = query.get("error") {
        return Err(Ok(error.clone()));
    }
    query
        .get("code")
        .map(String::as_str)
        .ok_or_else(|| Ok("no code".to_owned()))
}

async fn sign_in(
    auth: &Auth,
    headers: &HeaderMap,
    query: &HashMap<String, String>,
    pending: &Pending,
) -> Response {
    let failed = |problem| pages::sign_in_problem(headers, problem);
    let code = match code(auth, query).await {
        Ok(code) => code,
        Err(Ok(error)) => {
            tracing::info!(event = "auth.sign_in", outcome = "failure", %error, "sign-in did not finish");
            return failed(SignInProblem::Failed);
        }
        Err(Err(error)) => {
            tracing::warn!(event = "auth.sign_in", outcome = "failure", %error, "sign-in refused");
            return failed(SignInProblem::Failed);
        }
    };
    let tokens = match auth.oauth.exchange(code, pending.verifier()).await {
        Ok(tokens) => tokens,
        Err(error @ AuthError::Unreachable(_)) => {
            tracing::warn!(event = "auth.sign_in", outcome = "failure", %error, "cannot redeem the code");
            return failed(SignInProblem::IssuerUnreachable);
        }
        Err(error @ (AuthError::Refused { .. } | AuthError::Invalid(_))) => {
            tracing::warn!(event = "auth.sign_in", outcome = "failure", %error, "cannot redeem the code");
            return failed(SignInProblem::Failed);
        }
    };
    // Who the person is, as kube-apiserver sees the token: it validated the token itself.
    let refused = |error: &dyn std::fmt::Display| {
        tracing::warn!(event = "auth.sign_in", outcome = "failure", %error, "the cluster did not accept the token");
        failed(SignInProblem::ClusterRefused)
    };
    let connection = match (auth.connect)(tokens.access_token) {
        Ok(connection) => connection,
        Err(error) => return refused(&error),
    };
    let identity = match connection.cluster.whoami().await {
        Ok(identity) => identity,
        Err(error) => return refused(&error),
    };
    let username = identity.username;
    // A session is never carried over a sign-in: the previous one ends (session fixation,
    // OWASP Session Management Cheat Sheet, "Renew the Session ID After Any Privilege Level
    // Change").
    if let Some(old) = cookie::get(headers, cookie::SESSION) {
        auth.sessions.end_session(old);
    }
    match auth.sessions.start(
        username.clone(),
        connection,
        tokens.expires_in,
        tokens.refresh_token,
        tokens.refresh_expires_in,
    ) {
        Ok(id) => {
            tracing::info!(event = "auth.sign_in", outcome = "success", user = %username, "signed in");
            redirect(
                pending.return_to(),
                &[cookie::set(cookie::SESSION, &id, None)],
            )
        }
        Err(error) => {
            tracing::error!(event = "auth.sign_in", outcome = "failure", %error, "cannot start a session");
            problem(Kind::Internal, "Zeabur could not start a session.")
        }
    }
}

fn no_content(cookies: &[String]) -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    set_cookies(response.headers_mut(), cookies);
    pages::secure(response.headers_mut());
    response
}

/// `POST /auth/logout`.
async fn logout(State(auth): State<Arc<Auth>>, headers: HeaderMap) -> Response {
    if let Some(refresh) =
        cookie::get(&headers, cookie::SESSION).and_then(|id| auth.sessions.end_session(id))
    {
        tracing::info!(event = "auth.sign_out", "signed out");
        if let Err(error) = auth.oauth.revoke(&refresh).await {
            tracing::warn!(event = "auth.revoke", %error, "cannot revoke the refresh token");
        }
    }
    no_content(&[cookie::clear(cookie::SESSION)])
}

#[cfg(test)]
mod tests;
