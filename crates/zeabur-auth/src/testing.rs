//! An in-process issuer for tests.
//!
//! It serves discovery, the token endpoint (authorization code with PKCE S256 and
//! `client_secret_basic`, rotating refresh tokens that revoke their family on reuse, as
//! Zeabur Identity does) and revocation. A passkey sign-in is [`FakeIssuer::approve`].
//!
//! Access tokens read `<user>:<n>`, so a test's [`crate::session::Bind`] can tell who holds
//! them.

#![expect(
    clippy::unwrap_used,
    clippy::missing_panics_doc,
    clippy::significant_drop_tightening,
    reason = "a test helper: a failure fails the test"
)]

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::json;
use zeabur_core::crypto::sha256;

use crate::url::BaseUrl;

struct Code {
    user: String,
    challenge: String,
    redirect_uri: String,
}

/// The issuer's own records.
#[derive(Default)]
struct Books {
    codes: HashMap<String, Code>,
    /// Current refresh token of each family: the family and its user.
    current: HashMap<String, (usize, String)>,
    /// Rotated refresh tokens and their family.
    rotated: HashMap<String, usize>,
    revoked_families: HashSet<usize>,
    next: usize,
}

#[derive(Default)]
struct Inner {
    books: Books,
    recorded: Recorded,
}

/// What the issuer saw and how it behaves; tests read and change it.
#[derive(Debug, Default)]
pub struct Recorded {
    /// How many refreshes were answered with tokens.
    pub refreshes: usize,
    /// Refresh tokens revoked through the revocation endpoint.
    pub revocations: Vec<String>,
    /// Lifetime of everyday access tokens, in seconds.
    pub expires_in: i64,
    /// `refresh_expires_in` of each answer with a refresh token: how long, in seconds, the
    /// refresh token lasts unused. Zeabur Identity sends it; `None` leaves it out.
    pub refresh_expires_in: Option<i64>,
    /// Whether the token endpoint fails with `503`.
    pub down: bool,
}

/// The issuer.
pub struct FakeIssuer {
    url: BaseUrl,
    state: Arc<Mutex<Inner>>,
}

impl std::fmt::Debug for FakeIssuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeIssuer")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

/// The client the fake issuer knows.
pub const CLIENT_ID: &str = "zeabur";
/// Its secret; with characters that `client_secret_basic` must form-urlencode.
pub const CLIENT_SECRET: &str = "s3cret:with/odd+chars";

#[derive(Clone)]
struct Shared {
    url: String,
    state: Arc<Mutex<Inner>>,
}

impl FakeIssuer {
    /// Listens on a loopback port.
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(Inner {
            books: Books::default(),
            recorded: Recorded {
                expires_in: 600,
                refresh_expires_in: Some(3600),
                ..Recorded::default()
            },
        }));
        let shared = Shared {
            url: url.clone(),
            state: Arc::clone(&state),
        };
        let app = Router::new()
            .route("/.well-known/openid-configuration", get(metadata))
            .route("/token", post(token))
            .route("/revoke", post(revoke))
            .with_state(shared);
        tokio::spawn(async move { axum::serve(listener, app).await });
        Self {
            url: BaseUrl::parse(&url).unwrap(),
            state,
        }
    }

    /// The issuer URL.
    #[must_use]
    pub fn url(&self) -> BaseUrl {
        self.url.clone()
    }

    /// Reads or changes what the issuer recorded.
    pub fn with<T>(&self, f: impl FnOnce(&mut Recorded) -> T) -> T {
        f(&mut self.state.lock().unwrap().recorded)
    }

    /// `user` signs in at `authorize_url` with a passkey: the query string the browser brings
    /// back to the client's redirect URI (`code`, `state`, `iss`; or `error`).
    #[must_use]
    pub fn approve(&self, authorize_url: &str, user: &str) -> String {
        let query = authorize_url.split_once('?').map_or("", |(_, q)| q);
        let params: HashMap<String, String> = form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        let state = params.get("state").cloned().unwrap_or_default();
        let mut answer = form_urlencoded::Serializer::new(String::new());
        let mut inner = self.state.lock().unwrap();
        let books = &mut inner.books;
        books.next += 1;
        let code = format!("code-{}", books.next);
        books.codes.insert(
            code.clone(),
            Code {
                user: user.to_owned(),
                challenge: params.get("code_challenge").cloned().unwrap_or_default(),
                redirect_uri: params.get("redirect_uri").cloned().unwrap_or_default(),
            },
        );
        answer
            .append_pair("code", &code)
            .append_pair("state", &state)
            .append_pair("iss", self.url.as_str());
        answer.finish()
    }
}

async fn metadata(State(shared): State<Shared>) -> Response {
    let url = &shared.url;
    axum::Json(json!({
        "issuer": url,
        "authorization_endpoint": format!("{url}/authorize"),
        "token_endpoint": format!("{url}/token"),
        "revocation_endpoint": format!("{url}/revoke"),
        "code_challenge_methods_supported": ["S256"],
        "authorization_response_iss_parameter_supported": true,
    }))
    .into_response()
}

/// A token answer for `user`'s access token number `n`, with `refresh`.
fn tokens(recorded: &Recorded, user: &str, n: usize, refresh: &str) -> Response {
    let mut answer = serde_json::Map::new();
    answer.insert("access_token".into(), json!(format!("{user}:{n}")));
    answer.insert("token_type".into(), json!("Bearer"));
    answer.insert("expires_in".into(), json!(recorded.expires_in));
    answer.insert("refresh_token".into(), json!(refresh));
    if let Some(seconds) = recorded.refresh_expires_in {
        answer.insert("refresh_expires_in".into(), json!(seconds));
    }
    axum::Json(answer).into_response()
}

fn oauth_error(status: StatusCode, error: &str) -> Response {
    (status, axum::Json(json!({"error": error}))).into_response()
}

fn authenticated(headers: &HeaderMap) -> bool {
    let expected = format!(
        "{}:{}",
        form_urlencoded::byte_serialize(CLIENT_ID.as_bytes()).collect::<String>(),
        form_urlencoded::byte_serialize(CLIENT_SECRET.as_bytes()).collect::<String>()
    );
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| STANDARD.decode(v).ok())
        .is_some_and(|decoded| decoded == expected.as_bytes())
}

async fn token(State(shared): State<Shared>, headers: HeaderMap, body: String) -> Response {
    if !authenticated(&headers) {
        return oauth_error(StatusCode::UNAUTHORIZED, "invalid_client");
    }
    let params: HashMap<String, String> = form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    let mut inner = shared.state.lock().unwrap();
    let Inner { books, recorded } = &mut *inner;
    if recorded.down {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let param = |name: &str| params.get(name).cloned().unwrap_or_default();
    match param("grant_type").as_str() {
        "authorization_code" => {
            let Some(code) = books.codes.remove(&param("code")) else {
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
            };
            let challenge = URL_SAFE_NO_PAD.encode(sha256(param("code_verifier").as_bytes()));
            if challenge != code.challenge || param("redirect_uri") != code.redirect_uri {
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
            }
            books.next += 1;
            let n = books.next;
            let refresh = format!("refresh-{n}");
            books
                .current
                .insert(refresh.clone(), (n, code.user.clone()));
            tokens(recorded, &code.user, n, &refresh)
        }
        "refresh_token" => {
            let presented = param("refresh_token");
            if let Some(family) = books.rotated.get(&presented).copied() {
                // Reuse of a rotated token: revoke the family, as Zeabur Identity does.
                books.revoked_families.insert(family);
                books.current.retain(|_, (f, _)| *f != family);
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
            }
            let Some((family, user)) = books.current.remove(&presented) else {
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_grant");
            };
            books.rotated.insert(presented, family);
            books.next += 1;
            recorded.refreshes += 1;
            let n = books.next;
            let refresh = format!("refresh-{n}");
            books
                .current
                .insert(refresh.clone(), (family, user.clone()));
            tokens(recorded, &user, n, &refresh)
        }
        _ => oauth_error(StatusCode::BAD_REQUEST, "unsupported_grant_type"),
    }
}

async fn revoke(State(shared): State<Shared>, headers: HeaderMap, body: String) -> Response {
    if !authenticated(&headers) {
        return oauth_error(StatusCode::UNAUTHORIZED, "invalid_client");
    }
    let token = form_urlencoded::parse(body.as_bytes())
        .find(|(name, _)| name == "token")
        .map_or_else(String::new, |(_, value)| value.into_owned());
    let mut inner = shared.state.lock().unwrap();
    let Inner { books, recorded } = &mut *inner;
    books.current.remove(&token);
    recorded.revocations.push(token);
    StatusCode::OK.into_response()
}
