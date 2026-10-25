//! Zeabur as a confidential OAuth 2.0 client of Zeabur Identity [rfc6749].
//!
//! - **Configuration from the issuer.** The endpoints come from
//!   `<issuer>/.well-known/openid-configuration` [oidc-discovery] (§4), read once and kept. The
//!   document's `issuer` must equal the configured one (§4.3), every endpoint must be `https` (see
//!   [`crate::url`]), and when it lists PKCE methods, S256 must be one.
//! - **Authorization code with PKCE S256 and `state`** ([`crate::flow`]); the redirect URI is one
//!   fixed URL, which Zeabur Identity compares exactly.
//! - **The issuer of the answer is checked** [rfc9207]: when the document says the issuer sends
//!   `iss` with its answers, an answer without it, or with another, is refused; this is the mix-up
//!   defence of [rfc9700] §4.4.2.1.
//! - **Client authentication** is `client_secret_basic`: the identifier and the secret,
//!   form-urlencoded, then Base64, in `Authorization` ([rfc6749] §2.3.1).
//! - **Tokens are opaque here.** No ID token is verified: who the person is comes from
//!   kube-apiserver, which validates the access token itself, through a `SelfSubjectReview`. The
//!   access token's lifetime comes from `expires_in` ([rfc6749] §5.1); an answer without it is
//!   refused rather than guessed.
//! - **One claim of the ID token is read**: `can_elevate`, which Zeabur Identity sets when an
//!   `elevate` request would succeed. Its signature is not checked: the token comes straight from
//!   the token endpoint over TLS ([oidc-core] §3.1.3.7, item 6), and the claim only decides whether
//!   admin rights are offered. It grants nothing; kube-apiserver checks the elevated token. Without
//!   the claim, or without an ID token, admin rights are not offered.
//! - **Refresh** sends the refresh token and keeps the one the answer returns; Zeabur Identity
//!   rotates it on every use and revokes the whole family when a used one comes back ([rfc9700]
//!   §4.14.2), so refreshes of one session never overlap ([`crate::session`]).
//! - **Revocation** [rfc7009] of the refresh token on sign-out, when the issuer has an endpoint for
//!   it. Access tokens cannot be revoked (kube-apiserver verifies them offline); they expire within
//!   minutes.
//!
//! # References
//!
//! - [rfc6749] D. Hardt, Ed., "The OAuth 2.0 Authorization Framework," RFC 6749, IETF, Oct. 2012. <https://www.rfc-editor.org/rfc/rfc6749>
//! - [rfc7009] T. Lodderstedt, Ed., S. Dronia and M. Scurtescu, "OAuth 2.0 Token Revocation,"
//!   RFC 7009, IETF, Aug. 2013. <https://www.rfc-editor.org/rfc/rfc7009>
//! - [rfc9207] K. Meyer zu Selhausen and D. Fett, "OAuth 2.0 Authorization Server Issuer
//!   Identification," RFC 9207, IETF, Mar. 2022. <https://www.rfc-editor.org/rfc/rfc9207>
//! - [rfc9700] T. Lodderstedt, J. Bradley, A. Labunets and D. Fett, "Best Current Practice for
//!   OAuth 2.0 Security," RFC 9700, IETF, Jan. 2025. <https://www.rfc-editor.org/rfc/rfc9700>
//! - [oidc-core] N. Sakimura, J. Bradley, M. Jones, B. de Medeiros and C. Mortimore, "OpenID
//!   Connect Core 1.0," OpenID Foundation. <https://openid.net/specs/openid-connect-core-1_0.html>
//! - [oidc-discovery] N. Sakimura, J. Bradley, M. Jones and E. Jay, "OpenID Connect Discovery 1.0,"
//!   OpenID Foundation. <https://openid.net/specs/openid-connect-discovery-1_0.html>
//!
//! [rfc6749]: https://www.rfc-editor.org/rfc/rfc6749
//! [rfc7009]: https://www.rfc-editor.org/rfc/rfc7009
//! [rfc9207]: https://www.rfc-editor.org/rfc/rfc9207
//! [rfc9700]: https://www.rfc-editor.org/rfc/rfc9700
//! [oidc-core]: https://openid.net/specs/openid-connect-core-1_0.html#TokenResponseValidation
//! [oidc-discovery]: https://openid.net/specs/openid-connect-discovery-1_0.html

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use bytes::Bytes;
use http::{HeaderValue, Method, Request, StatusCode, header};
use http_body_util::Full;
use secrecy::zeroize::Zeroizing;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use tokio::sync::OnceCell;
use zeabur_core::secret::join;

use crate::flow::{Pending, Purpose};
pub use crate::http::ExtraRoots;
use crate::http::Http;
use crate::url::BaseUrl;

/// Where the issuer sends the browser back: one path for signing in and for elevation.
pub const CALLBACK_PATH: &str = "/auth/callback";

/// How Zeabur is registered with the issuer.
pub struct ClientConfig {
    /// The issuer's URL.
    pub issuer: BaseUrl,
    /// Zeabur's public URL; the redirect URI is this followed by [`CALLBACK_PATH`].
    pub public_url: BaseUrl,
    /// The `client_id`.
    pub client_id: String,
    /// The client secret.
    pub client_secret: SecretString,
    /// Certificate authorities to trust for the issuer besides the platform's.
    pub issuer_ca: ExtraRoots,
}

impl fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientConfig")
            .field("issuer", &self.issuer)
            .field("public_url", &self.public_url)
            .field("client_id", &self.client_id)
            .field("issuer_ca", &self.issuer_ca)
            .finish_non_exhaustive()
    }
}

/// Why talking to the issuer failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    /// The issuer could not be reached or failed (transport, timeout, `5xx`).
    #[error("cannot reach the issuer: {0}")]
    Unreachable(String),
    /// The issuer refused with an OAuth error ([rfc6749] §5.2), such as `invalid_grant` for a
    /// refresh token that was revoked or expired.
    ///
    /// [rfc6749]: https://www.rfc-editor.org/rfc/rfc6749#section-5.2
    #[error("the issuer refused: {error} ({})", description.as_deref().unwrap_or("no description"))]
    Refused {
        /// The error code.
        error: String,
        /// The issuer's description, for logs.
        description: Option<String>,
    },
    /// The issuer answered something Zeabur cannot use.
    #[error("the issuer's answer is not usable: {0}")]
    Invalid(String),
}

/// Tokens from the token endpoint.
pub struct TokenSet {
    /// The access token, sent to kube-apiserver.
    pub access_token: SecretString,
    /// Its lifetime in seconds.
    pub expires_in: i64,
    /// The refresh token, when one was issued (never with elevated tokens).
    pub refresh_token: Option<SecretString>,
    /// Whether the ID token says an `elevate` request would succeed.
    pub can_elevate: bool,
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenSet")
            .field("expires_in", &self.expires_in)
            .field("refresh_token", &self.refresh_token.is_some())
            .field("can_elevate", &self.can_elevate)
            .finish_non_exhaustive()
    }
}

/// The endpoints from the issuer's metadata.
#[derive(Debug, Clone)]
struct Provider {
    authorization_endpoint: String,
    token_endpoint: String,
    revocation_endpoint: Option<String>,
    iss_parameter: bool,
}

#[derive(Deserialize)]
struct Metadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    #[serde(default)]
    revocation_endpoint: Option<String>,
    #[serde(default)]
    code_challenge_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    authorization_response_iss_parameter_supported: bool,
}

#[derive(Deserialize)]
struct TokenAnswer {
    access_token: SecretString,
    token_type: String,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    refresh_token: Option<SecretString>,
    #[serde(default)]
    id_token: Option<SecretString>,
}

#[derive(Deserialize)]
struct IdTokenClaims {
    #[serde(default)]
    can_elevate: bool,
}

/// The `can_elevate` claim of a compact JWS ID token; `false` when it cannot be read.
fn can_elevate(id_token: Option<&str>) -> bool {
    id_token
        .and_then(|token| token.split('.').nth(1))
        .and_then(|payload| URL_SAFE_NO_PAD.decode(payload).ok())
        .and_then(|json| serde_json::from_slice::<IdTokenClaims>(&json).ok())
        .is_some_and(|claims| claims.can_elevate)
}

#[derive(Deserialize)]
struct ErrorAnswer {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

/// The OAuth client.
pub struct OAuthClient {
    issuer: BaseUrl,
    client_id: String,
    /// The `Authorization` header of `client_secret_basic`. The client secret is not kept
    /// otherwise.
    basic: HeaderValue,
    redirect_uri: String,
    http: Http,
    provider: OnceCell<Provider>,
}

impl fmt::Debug for OAuthClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthClient")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .finish_non_exhaustive()
    }
}

impl OAuthClient {
    /// A client for `config`. The issuer is not contacted until it is needed.
    ///
    /// # Errors
    ///
    /// When TLS cannot be set up.
    pub fn new(config: ClientConfig) -> Result<Self, AuthError> {
        let http = Http::new(config.issuer_ca).map_err(|e| AuthError::Invalid(e.to_string()))?;
        Ok(Self {
            basic: basic(&config.client_id, &config.client_secret)?,
            redirect_uri: config.public_url.join(CALLBACK_PATH),
            issuer: config.issuer,
            client_id: config.client_id,
            http,
            provider: OnceCell::new(),
        })
    }

    /// The redirect URI to register with the issuer.
    #[must_use]
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Reads the issuer's metadata now, so a misconfiguration shows when the server starts.
    ///
    /// # Errors
    ///
    /// When the issuer cannot be reached or its metadata is not usable.
    pub async fn discover(&self) -> Result<(), AuthError> {
        self.provider().await.map(|_| ())
    }

    async fn provider(&self) -> Result<&Provider, AuthError> {
        self.provider
            .get_or_try_init(|| self.fetch_provider())
            .await
    }

    async fn fetch_provider(&self) -> Result<Provider, AuthError> {
        let issuer = &self.issuer;
        let url = issuer.join("/.well-known/openid-configuration");
        let request = Request::get(&url)
            .header(header::ACCEPT, "application/json")
            .body(Full::new(Bytes::new()))
            .map_err(|e| AuthError::Invalid(e.to_string()))?;
        let (status, body) = self.send(request).await?;
        if status != StatusCode::OK {
            return Err(unexpected(status, &url));
        }
        let metadata: Metadata =
            serde_json::from_slice(&body).map_err(|e| AuthError::Invalid(format!("{url}: {e}")))?;
        // OpenID Connect Discovery §4.3: the issuer in the document is the one asked.
        if metadata.issuer.trim_end_matches('/') != issuer.as_str() {
            return Err(AuthError::Invalid(format!(
                "{url} names the issuer {:?}, not {issuer}",
                metadata.issuer
            )));
        }
        if metadata
            .code_challenge_methods_supported
            .as_ref()
            .is_some_and(|methods| !methods.iter().any(|m| m == "S256"))
        {
            return Err(AuthError::Invalid(format!(
                "{url}: PKCE S256 is not supported"
            )));
        }
        let endpoint = |text: &str| {
            BaseUrl::parse(text)
                .map(|u| u.as_str().to_owned())
                .map_err(|e| AuthError::Invalid(e.to_string()))
        };
        Ok(Provider {
            authorization_endpoint: endpoint(&metadata.authorization_endpoint)?,
            token_endpoint: endpoint(&metadata.token_endpoint)?,
            revocation_endpoint: metadata
                .revocation_endpoint
                .as_deref()
                .map(endpoint)
                .transpose()?,
            iss_parameter: metadata.authorization_response_iss_parameter_supported,
        })
    }

    /// Where to send the browser to start `pending`.
    ///
    /// # Errors
    ///
    /// When the issuer's metadata cannot be read.
    pub async fn authorization_url(&self, pending: &Pending) -> Result<String, AuthError> {
        let provider = self.provider().await?;
        let mut query = form_urlencoded::Serializer::new(String::new());
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("scope", pending.purpose().scope())
            .append_pair("state", pending.state())
            .append_pair("code_challenge", &pending.challenge())
            .append_pair("code_challenge_method", "S256");
        match pending.purpose() {
            // Ask for a fresh sign-in explicitly; Zeabur Identity has no single sign-on session, so
            // it always asks anyway.
            Purpose::Elevate => {
                query.append_pair("prompt", "login");
            }
            Purpose::SignIn { .. } => {}
        }
        let separator = if provider.authorization_endpoint.contains('?') {
            '&'
        } else {
            '?'
        };
        Ok(format!(
            "{}{separator}{}",
            provider.authorization_endpoint,
            query.finish()
        ))
    }

    /// Checks the `iss` parameter of an authorization answer [rfc9207] (§2.4).
    ///
    /// [rfc9207]: https://www.rfc-editor.org/rfc/rfc9207
    ///
    /// # Errors
    ///
    /// When `iss` names another issuer, or is missing where the issuer said it sends it.
    pub async fn check_issuer(&self, iss: Option<&str>) -> Result<(), AuthError> {
        let provider = self.provider().await?;
        match iss {
            Some(iss) if iss.trim_end_matches('/') == self.issuer.as_str() => Ok(()),
            Some(other) => Err(AuthError::Invalid(format!(
                "the answer comes from the issuer {other:?}, not {}",
                self.issuer
            ))),
            None if provider.iss_parameter => Err(AuthError::Invalid(
                "the answer does not name its issuer".to_owned(),
            )),
            None => Ok(()),
        }
    }

    /// Redeems `code` with the PKCE `verifier` of its sign-in.
    ///
    /// # Errors
    ///
    /// When the issuer refuses or cannot be reached.
    pub async fn exchange(&self, code: &str, verifier: &str) -> Result<TokenSet, AuthError> {
        self.token(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &self.redirect_uri),
            ("code_verifier", verifier),
        ])
        .await
    }

    /// New tokens for `refresh_token`.
    ///
    /// # Errors
    ///
    /// When the issuer refuses (the session is over) or cannot be reached.
    pub async fn refresh(&self, refresh_token: &SecretString) -> Result<TokenSet, AuthError> {
        self.token(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose_secret()),
        ])
        .await
    }

    /// Revokes `refresh_token` [rfc7009], when the issuer has a revocation endpoint.
    ///
    /// [rfc7009]: https://www.rfc-editor.org/rfc/rfc7009
    ///
    /// # Errors
    ///
    /// When the issuer refuses or cannot be reached.
    pub async fn revoke(&self, refresh_token: &SecretString) -> Result<(), AuthError> {
        let provider = self.provider().await?;
        let Some(endpoint) = provider.revocation_endpoint.clone() else {
            return Ok(());
        };
        let (status, body) = self
            .post(
                &endpoint,
                &[
                    ("token", refresh_token.expose_secret()),
                    ("token_type_hint", "refresh_token"),
                ],
            )
            .await?;
        if status == StatusCode::OK {
            Ok(())
        } else {
            Err(refusal(status, &body, &endpoint))
        }
    }

    async fn token(&self, form: &[(&str, &str)]) -> Result<TokenSet, AuthError> {
        let endpoint = self.provider().await?.token_endpoint.clone();
        let (status, body) = self.post(&endpoint, form).await?;
        if status != StatusCode::OK {
            return Err(refusal(status, &body, &endpoint));
        }
        let answer: TokenAnswer = serde_json::from_slice(&body)
            .map_err(|e| AuthError::Invalid(format!("{endpoint}: {e}")))?;
        // RFC 6749 §7.1: the client must understand the token type; kube-apiserver takes
        // bearer tokens. The type is case-insensitive (RFC 6749 §5.1).
        if !answer.token_type.eq_ignore_ascii_case("bearer") {
            return Err(AuthError::Invalid(format!(
                "{endpoint}: token type {:?} is not Bearer",
                answer.token_type
            )));
        }
        let expires_in = answer
            .expires_in
            .filter(|seconds| *seconds > 0)
            .ok_or_else(|| AuthError::Invalid(format!("{endpoint}: no expires_in")))?;
        Ok(TokenSet {
            access_token: answer.access_token,
            expires_in,
            refresh_token: answer.refresh_token,
            can_elevate: can_elevate(answer.id_token.as_ref().map(ExposeSecret::expose_secret)),
        })
    }

    async fn post(
        &self,
        endpoint: &str,
        form: &[(&str, &str)],
    ) -> Result<(StatusCode, Bytes), AuthError> {
        let body = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form)
            .finish();
        let request = Request::builder()
            .method(Method::POST)
            .uri(endpoint)
            .header(header::AUTHORIZATION, self.basic.clone())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::ACCEPT, "application/json")
            .body(Full::new(Bytes::from(body)))
            .map_err(|e| AuthError::Invalid(e.to_string()))?;
        self.send(request).await
    }

    async fn send(&self, request: Request<Full<Bytes>>) -> Result<(StatusCode, Bytes), AuthError> {
        self.http
            .send(request)
            .await
            .map_err(|e| AuthError::Unreachable(e.to_string()))
    }
}

/// The `Authorization` header of `client_secret_basic`, marked sensitive: both parts
/// form-urlencoded before Base64 ([rfc6749] §2.3.1).
///
/// [rfc6749]: https://www.rfc-editor.org/rfc/rfc6749#section-2.3.1
fn basic(client_id: &str, secret: &SecretString) -> Result<HeaderValue, AuthError> {
    let encode = |s: &str| {
        let parts: Vec<&str> = form_urlencoded::byte_serialize(s.as_bytes()).collect();
        Zeroizing::new(join(&parts))
    };
    let pair = Zeroizing::new(join(&[
        &encode(client_id),
        ":",
        &encode(secret.expose_secret()),
    ]));
    let encoded = Zeroizing::new(STANDARD.encode(pair.as_bytes()));
    let mut value = HeaderValue::try_from(join(&["Basic ", &encoded]))
        .map_err(|e| AuthError::Invalid(e.to_string()))?;
    value.set_sensitive(true);
    Ok(value)
}

fn unexpected(status: StatusCode, url: &str) -> AuthError {
    if status.is_server_error() {
        AuthError::Unreachable(format!("{url} answered {status}"))
    } else {
        AuthError::Invalid(format!("{url} answered {status}"))
    }
}

/// An OAuth error answer ([rfc6749] §5.2), or, when the body is none, by its status.
///
/// [rfc6749]: https://www.rfc-editor.org/rfc/rfc6749#section-5.2
fn refusal(status: StatusCode, body: &[u8], url: &str) -> AuthError {
    match serde_json::from_slice::<ErrorAnswer>(body) {
        Ok(answer) if status.is_client_error() => AuthError::Refused {
            error: answer.error,
            description: answer.error_description,
        },
        Ok(_) | Err(_) => unexpected(status, url),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::testing::{CLIENT_ID, CLIENT_SECRET, FakeIssuer};

    fn client_for(issuer: &BaseUrl, secret: &str) -> OAuthClient {
        OAuthClient::new(ClientConfig {
            issuer: issuer.clone(),
            public_url: BaseUrl::parse_root("http://127.0.0.1:8080").unwrap(),
            client_id: CLIENT_ID.to_owned(),
            client_secret: SecretString::from(secret.to_owned()),
            issuer_ca: ExtraRoots::default(),
        })
        .unwrap()
    }

    fn query(text: &str) -> HashMap<String, String> {
        let query = text.split_once('?').map_or(text, |(_, q)| q);
        form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect()
    }

    #[tokio::test]
    async fn the_authorization_request_carries_pkce_state_and_the_exact_redirect_uri() {
        let issuer = FakeIssuer::start().await;
        let client = client_for(&issuer.url(), CLIENT_SECRET);
        let pending = Pending::start(
            Purpose::SignIn {
                return_to: "/".to_owned(),
            },
            0,
        )
        .unwrap();
        let url = client.authorization_url(&pending).await.unwrap();
        assert!(url.starts_with(&format!("{}/authorize?", issuer.url())));
        let params = query(&url);
        assert_eq!(params["response_type"], "code");
        assert_eq!(params["client_id"], "zeabur");
        assert_eq!(
            params["redirect_uri"],
            "http://127.0.0.1:8080/auth/callback"
        );
        assert_eq!(params["scope"], "openid");
        assert_eq!(params["state"], pending.state());
        assert_eq!(params["code_challenge"], pending.challenge());
        assert_eq!(params["code_challenge_method"], "S256");
        assert!(!params.contains_key("prompt"));
        assert!(!url.contains(pending.verifier()));

        let elevate = Pending::start(Purpose::Elevate, 0).unwrap();
        let params = query(&client.authorization_url(&elevate).await.unwrap());
        assert_eq!(params["scope"], "openid elevate");
        assert_eq!(params["prompt"], "login");
    }

    #[tokio::test]
    async fn a_code_is_redeemed_only_with_its_verifier_and_the_client_secret() {
        let issuer = FakeIssuer::start().await;
        let client = client_for(&issuer.url(), CLIENT_SECRET);
        let pending = Pending::start(Purpose::Elevate, 0).unwrap();
        let other = Pending::start(Purpose::Elevate, 0).unwrap();
        let authorize = client.authorization_url(&pending).await.unwrap();

        let answer = query(&issuer.approve(&authorize, "alice"));
        let refused = client.exchange(&answer["code"], other.verifier()).await;
        assert!(
            matches!(&refused, Err(AuthError::Refused { error, .. }) if error == "invalid_grant"),
            "{refused:?}"
        );

        let answer = query(&issuer.approve(&authorize, "alice"));
        let wrong_secret = client_for(&issuer.url(), "guess");
        let refused = wrong_secret
            .exchange(&answer["code"], pending.verifier())
            .await;
        assert!(
            matches!(&refused, Err(AuthError::Refused { error, .. }) if error == "invalid_client"),
            "{refused:?}"
        );

        let tokens = client
            .exchange(&answer["code"], pending.verifier())
            .await
            .unwrap();
        assert_eq!(tokens.access_token.expose_secret(), "alice:elevated:3");
        assert_eq!(tokens.expires_in, 300);
        // An elevated token comes without a refresh token.
        assert!(tokens.refresh_token.is_none());
        assert!(!format!("{tokens:?}").contains("alice"));
    }

    #[tokio::test]
    async fn the_answer_must_come_from_the_configured_issuer() {
        let issuer = FakeIssuer::start().await;
        let client = client_for(&issuer.url(), CLIENT_SECRET);
        let own = issuer.url().to_string();
        assert_eq!(client.check_issuer(Some(&own)).await, Ok(()));
        assert!(matches!(
            client.check_issuer(Some("https://attacker.example")).await,
            Err(AuthError::Invalid(_))
        ));
        // The issuer says it sends `iss`, so an answer without it is refused.
        assert!(matches!(
            client.check_issuer(None).await,
            Err(AuthError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn metadata_naming_another_issuer_is_refused() {
        let issuer = FakeIssuer::start().await;
        // The same server under another name: its metadata names 127.0.0.1, not localhost.
        let renamed = issuer.url().to_string().replace("127.0.0.1", "localhost");
        let client = client_for(&BaseUrl::parse(&renamed).unwrap(), CLIENT_SECRET);
        assert!(matches!(
            client.discover().await,
            Err(AuthError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn an_issuer_that_does_not_answer_is_unreachable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let client = client_for(&BaseUrl::parse(&url).unwrap(), CLIENT_SECRET);
        assert!(matches!(
            client.discover().await,
            Err(AuthError::Unreachable(_))
        ));
    }
}
