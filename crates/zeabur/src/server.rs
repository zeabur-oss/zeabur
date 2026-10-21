//! The server: sign-in through Zeabur Identity, the API acting with each person's token, and the
//! web interface.
//!
//! | Method and path | What it does |
//! |---|---|
//! | `GET /api/healthz` | `ok`; outside every check below, for the kubelet |
//! | `GET /auth/login?return=/path` | starts signing in; afterwards the browser goes to `return` |
//! | `GET /auth/callback` | where Zeabur Identity sends the browser back, for signing in and elevation |
//! | `POST /auth/logout` | ends the session and revokes its refresh token at Zeabur Identity |
//! | `GET /auth/elevate` | in a window of its own: asks Zeabur Identity for administrator rights |
//! | `DELETE /auth/elevation` | drops administrator rights |
//! | `/api/…` | the API (`zeabur_api`), as the person the session belongs to |
//! | anything else | the web interface |
//!
//! Every request but the health check passes [`zeabur_core::guard`] first: its `Host` must be
//! the public URL's, and a request that changes something must come from Zeabur's own page.
//!
//! **Elevation**, as `sudo`: while the session holds an elevated token, every request to the API
//! goes with it (reads, writes, the terminal, revealing a Secret), and a terminal ends when it
//! does. Only a plan's binding uses the base token, so plans survive elevating and the rights
//! ending (see `zeabur_api::writes`). Where elevation is turned on and not in use, anything the
//! cluster refuses comes back as `elevation-required` (see `zeabur_api`'s `error` module). The
//! interface then opens `/auth/elevate` in a window: Zeabur Identity asks for the passkey and
//! issues a token with the person's elevated groups, valid for minutes and never renewed. The
//! callback checks that it belongs to the same user as the session (a token for someone else is
//! refused), keeps it in the session, and its page tells the interface through a
//! `BroadcastChannel`, which fetches again what was refused.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use secrecy::SecretString;
use zeabur_api::ApiError;
use zeabur_api::access::{Access, ClusterAccess, Elevation, Handoff};
use zeabur_api::plan_token::PlanKey;
use zeabur_auth::client::{AuthError, OAuthClient};
use zeabur_auth::cookie;
use zeabur_auth::flow::{FlowKey, LIFETIME_SECONDS, Pending, Purpose, return_path};
use zeabur_auth::session::{Bind, Clock, Credentials, Lookup, Sessions};
use zeabur_auth::url::BaseUrl;
use zeabur_core::guard::{self, Origins, Refusal};
use zeabur_core::problem::{Kind, Problem};
use zeabur_kube::Cluster;
use zeabur_kube::k8s_openapi::jiff::Timestamp;

use crate::pages::{self, Elevated, SignInProblem};

/// A cluster connection bound to one person's token, and the token, which only a tools pod is
/// handed (`zeabur_api::tools`).
#[derive(Clone)]
pub(crate) struct Connection {
    pub(crate) cluster: Arc<dyn Cluster>,
    pub(crate) token: Arc<SecretString>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").finish_non_exhaustive()
    }
}

/// What the sign-in routes and the API share.
pub(crate) struct Auth {
    pub(crate) public: BaseUrl,
    pub(crate) oauth: Arc<OAuthClient>,
    pub(crate) sessions: Arc<Sessions<Connection>>,
    pub(crate) connect: Bind<Connection>,
    pub(crate) flow_key: FlowKey,
    pub(crate) elevation: bool,
    pub(crate) clock: Clock,
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Auth")
            .field("public", &self.public)
            .field("elevation", &self.elevation)
            .finish_non_exhaustive()
    }
}

/// The session a request to the API belongs to, found by [`session`].
#[derive(Clone)]
enum Signed {
    In(Credentials<Connection>),
    IssuerDown,
}

/// The whole server.
pub(crate) fn app(auth: &Arc<Auth>, plan_key: PlanKey, settings: zeabur_api::Settings) -> Router {
    let access = Arc::new(SessionAccess {
        elevation: auth.elevation,
        origin: auth.public.origin(),
    });
    let api = zeabur_api::router_with_key(access, plan_key, settings).layer(
        axum::middleware::from_fn_with_state(Arc::clone(auth), session),
    );
    let sign_in = Router::new()
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/logout", post(logout))
        .route("/auth/elevate", get(elevate))
        .route("/auth/elevation", delete(drop_elevation))
        .route("/auth/elevated.js", get(|| async { pages::script() }))
        .with_state(Arc::clone(auth));
    let guarded = api.merge(sign_in);
    #[cfg(feature = "ui")]
    let guarded = guarded.fallback_service(zeabur_ui::router());
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

/// Finds the session of an API request (renewing its token when due) and hands it to
/// [`SessionAccess`] through the request's extensions.
async fn session(State(auth): State<Arc<Auth>>, mut request: Request, next: Next) -> Response {
    if let Some(id) = cookie::get(request.headers(), cookie::SESSION) {
        let signed = match auth.sessions.current(id).await {
            Lookup::Active(credentials) => Some(Signed::In(credentials)),
            Lookup::IssuerUnavailable => Some(Signed::IssuerDown),
            Lookup::SignedOut => None,
        };
        if let Some(signed) = signed {
            request.extensions_mut().insert(signed);
        }
    }
    next.run(request).await
}

/// The API's view of sessions: every request with the elevated token while it lasts, otherwise
/// with the base one.
#[derive(Debug)]
struct SessionAccess {
    elevation: bool,
    /// The public URL's origin, which a terminal's handshake must come from.
    origin: String,
}

fn signed_out() -> Box<Problem> {
    Box::new(Problem::new(
        Kind::SignedOut,
        "Your session ended, or you signed out. Sign in to use Zeabur.",
    ))
}

fn credentials(parts: &Parts) -> Result<&Credentials<Connection>, Box<Problem>> {
    match parts.extensions.get::<Signed>() {
        Some(Signed::In(credentials)) => Ok(credentials),
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

fn timestamp(seconds: i64) -> Timestamp {
    Timestamp::from_second(seconds).unwrap_or(Timestamp::UNIX_EPOCH)
}

impl ClusterAccess for SessionAccess {
    fn for_request(&self, parts: &Parts) -> Result<Access, Box<Problem>> {
        let credentials = credentials(parts)?;
        Ok(match (&credentials.elevated, self.elevation) {
            (Some(elevated), _) => Access {
                cluster: Arc::clone(&elevated.value.cluster),
                elevation: Elevation::Active {
                    until: timestamp(elevated.expires_at),
                },
            },
            (None, true) => Access {
                cluster: Arc::clone(&credentials.base.value.cluster),
                elevation: Elevation::Available,
            },
            (None, false) => Access {
                cluster: Arc::clone(&credentials.base.value.cluster),
                elevation: Elevation::Unavailable,
            },
        })
    }

    /// The base token, for a plan's binding: elevating adds groups, and a plan made with one set
    /// must still apply with the other.
    fn everyday(&self, parts: &Parts) -> Result<Arc<dyn Cluster>, Box<Problem>> {
        Ok(Arc::clone(&credentials(parts)?.base.value.cluster))
    }

    /// The expiry of the token [`ClusterAccess::for_request`] uses: the elevated one's while it
    /// is held, so a terminal ends with the admin rights.
    fn credential_expiry(&self, parts: &Parts) -> Option<Timestamp> {
        let credentials = credentials(parts).ok()?;
        let expires_at = credentials
            .elevated
            .as_ref()
            .map_or(credentials.base.expires_at, |elevated| elevated.expires_at);
        Some(timestamp(expires_at))
    }

    /// The token of [`ClusterAccess::for_request`]: the elevated one while it is held.
    fn handoff(&self, parts: &Parts) -> Option<Handoff> {
        let credentials = credentials(parts).ok()?;
        let (bound, admin) = credentials
            .elevated
            .as_ref()
            .map_or((&credentials.base, false), |elevated| (elevated, true));
        Some(Handoff {
            token: Arc::clone(&bound.value.token),
            expires_at: timestamp(bound.expires_at),
            admin,
        })
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

/// Sends the browser to Zeabur Identity for `purpose`, with the flow in `cookie_name`.
async fn start(auth: &Auth, headers: &HeaderMap, purpose: Purpose, cookie_name: &str) -> Response {
    let Ok(pending) = Pending::start(purpose, (auth.clock)()) else {
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
            &[cookie::set(cookie_name, &sealed, Some(LIFETIME_SECONDS))],
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
    start(
        &auth,
        &headers,
        Purpose::SignIn { return_to },
        cookie::SIGN_IN,
    )
    .await
}

/// `GET /auth/elevate`, in a window the interface opened.
async fn elevate(State(auth): State<Arc<Auth>>, headers: HeaderMap) -> Response {
    if !auth.elevation {
        return problem(Kind::NotFound, "This Zeabur does not offer admin rights.");
    }
    // Only Zeabur's own page opens this window; a navigation from another site could start an
    // elevation the person did not ask for.
    if headers
        .get("sec-fetch-site")
        .is_some_and(|site| site != "same-origin")
    {
        return problem(
            Kind::Forbidden,
            "Only the Zeabur page itself can ask for admin rights.",
        );
    }
    let signed_in = match cookie::get(&headers, cookie::SESSION) {
        Some(id) => matches!(auth.sessions.current(id).await, Lookup::Active(_)),
        None => false,
    };
    if !signed_in {
        return pages::elevated(&headers, &Elevated::SignedOut);
    }
    start(&auth, &headers, Purpose::Elevate, cookie::ELEVATE).await
}

/// `GET /auth/callback`: Zeabur Identity's answer, for a sign-in or an elevation.
async fn callback(State(auth): State<Arc<Auth>>, uri: Uri, headers: HeaderMap) -> Response {
    let query = parameters(&uri);
    let state = query.get("state").map_or("", String::as_str);
    let now = (auth.clock)();
    let flow = [cookie::SIGN_IN, cookie::ELEVATE]
        .into_iter()
        .find_map(|name| {
            cookie::get(&headers, name)
                .and_then(|sealed| Pending::open(&auth.flow_key, sealed, now))
                .filter(|pending| pending.matches(state))
                .map(|pending| (name, pending))
        });
    let Some((name, pending)) = flow else {
        return pages::sign_in_problem(&headers, SignInProblem::Expired);
    };
    let finished = cookie::clear(name);
    let mut response = match pending.purpose() {
        Purpose::SignIn { return_to } => {
            sign_in(&auth, &headers, &query, &pending, return_to).await
        }
        Purpose::Elevate => {
            let outcome = elevation(&auth, &headers, &query, &pending).await;
            pages::elevated(&headers, &outcome)
        }
    };
    set_cookies(response.headers_mut(), &[finished]);
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
    return_to: &str,
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
    let identity = match (auth.connect)(&tokens.access_token) {
        Ok(connection) => connection.cluster.whoami().await.map_err(|e| e.to_string()),
        Err(error) => Err(error),
    };
    let identity = match identity {
        Ok(identity) => identity,
        Err(error) => {
            tracing::warn!(event = "auth.sign_in", outcome = "failure", %error, "the cluster did not accept the token");
            return failed(SignInProblem::ClusterRefused);
        }
    };
    let username = identity.username;
    // A session is never carried over a sign-in: the previous one ends (session fixation,
    // OWASP Session Management Cheat Sheet, "Renew the Session ID After Any Privilege Level
    // Change").
    if let Some(old) = cookie::get(headers, cookie::SESSION) {
        auth.sessions.end_session(old);
    }
    match auth.sessions.start(username.clone(), tokens) {
        Ok(id) => {
            tracing::info!(event = "auth.sign_in", outcome = "success", user = %username, "signed in");
            redirect(return_to, &[cookie::set(cookie::SESSION, &id, None)])
        }
        Err(error) => {
            tracing::error!(event = "auth.sign_in", outcome = "failure", %error, "cannot start a session");
            problem(Kind::Internal, "Zeabur could not start a session.")
        }
    }
}

async fn elevation(
    auth: &Auth,
    headers: &HeaderMap,
    query: &HashMap<String, String>,
    pending: &Pending,
) -> Elevated {
    let Some(id) = cookie::get(headers, cookie::SESSION) else {
        return Elevated::SignedOut;
    };
    let Lookup::Active(session) = auth.sessions.current(id).await else {
        return Elevated::SignedOut;
    };
    let code = match code(auth, query).await {
        Ok(code) => code,
        Err(Ok(error)) if error == "invalid_scope" => return Elevated::NotAllowed,
        Err(Ok(error)) => {
            tracing::info!(event = "auth.elevate", outcome = "failure", %error, "elevation did not finish");
            return Elevated::Cancelled;
        }
        Err(Err(error)) => {
            tracing::warn!(event = "auth.elevate", outcome = "failure", %error, "elevation refused");
            return Elevated::Failed;
        }
    };
    let tokens = match auth.oauth.exchange(code, pending.verifier()).await {
        Ok(tokens) => tokens,
        Err(error) => {
            tracing::warn!(event = "auth.elevate", outcome = "failure", %error, "cannot redeem the code");
            return Elevated::Failed;
        }
    };
    // The elevated token must be the same person's: Zeabur Identity adds groups, never changes the
    // user. A passkey of someone else, used in this window, must not lend its rights to this
    // session; and the plan token's binding (the user name) stays true.
    let identity = match (auth.connect)(&tokens.access_token) {
        Ok(connection) => connection.cluster.whoami().await.map_err(|e| e.to_string()),
        Err(error) => Err(error),
    };
    match identity {
        Ok(identity) if identity.username == session.username => {}
        Ok(identity) => {
            tracing::warn!(
                event = "auth.elevate", outcome = "failure", user = %session.username,
                presented = %identity.username, "elevated token belongs to another user"
            );
            return Elevated::Failed;
        }
        Err(error) => {
            tracing::warn!(event = "auth.elevate", outcome = "failure", %error, "the cluster did not accept the elevated token");
            return Elevated::Failed;
        }
    }
    match auth.sessions.elevate(id, &tokens) {
        Ok(until) => {
            tracing::info!(event = "auth.elevate", outcome = "success", user = %session.username, "admin rights granted");
            Elevated::Done(timestamp(until).to_string())
        }
        Err(error) => {
            tracing::warn!(event = "auth.elevate", outcome = "failure", %error, "cannot keep the elevated token");
            Elevated::SignedOut
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

/// `DELETE /auth/elevation`.
async fn drop_elevation(State(auth): State<Arc<Auth>>, headers: HeaderMap) -> Response {
    if let Some(id) = cookie::get(&headers, cookie::SESSION) {
        auth.sessions.drop_elevation(id);
    }
    no_content(&[])
}

#[cfg(test)]
mod tests;
