//! A client of GitHub's REST API for the cluster's GitHub App (see
//! [`crate::cluster::github_app`]): the manifest conversion, the App's installations, and the
//! repositories an installation can read.
//!
//! The App signs in with a JWT that its private key signs with RS256 [gh-jwt]; an installation
//! with an installation access token that the JWT gets [gh-token]. Zeabur keeps neither: each
//! request reads the key from the App's Secret as the person, and asks again.
//!
//! [gh-jwt]: https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-json-web-token-jwt-for-a-github-app
//! [gh-token]: https://docs.github.com/en/rest/apps/apps#create-an-installation-access-token-for-an-app

use axum::http::{self, Request};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bytes::Bytes;
use http_body_util::Full;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::images::https::Https;

/// The most a GitHub answer may be.
const LIMIT: usize = 4 * 1024 * 1024;
/// The most pages of 100 repositories read for one installation.
const PAGES: u32 = 10;
/// The REST API version these requests follow [gh-versions].
///
/// [gh-versions]: https://docs.github.com/en/rest/about-the-rest-api/api-versions
const VERSION: &str = "2022-11-28";

/// The App's credentials, as GitHub's manifest conversion gives them [gh-manifest].
///
/// [gh-manifest]: https://docs.github.com/en/rest/apps/apps#create-a-github-app-from-a-manifest
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Conversion {
    pub id: u64,
    pub slug: String,
    pub client_id: String,
    pub pem: String,
    #[serde(default)]
    pub webhook_secret: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub owner: Option<Account>,
}

/// A user or organization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Account {
    pub login: String,
    #[serde(rename = "type", default)]
    pub kind: String,
}

/// One installation of the App [gh-installations].
///
/// [gh-installations]: https://docs.github.com/en/rest/apps/apps#list-installations-for-the-authenticated-app
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Installation {
    pub id: i64,
    pub account: Account,
    #[serde(default)]
    pub repository_selection: String,
    #[serde(default)]
    pub html_url: String,
}

/// A repository an installation can read [gh-repositories].
///
/// [gh-repositories]: https://docs.github.com/en/rest/apps/installations#list-repositories-accessible-to-the-app-installation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Repository {
    pub full_name: String,
    #[serde(default)]
    pub default_branch: String,
    #[serde(default)]
    pub private: bool,
}

/// GitHub's REST API at `api`.
pub(crate) struct GitHub<'a> {
    pub http: &'a Https,
    pub api: &'a str,
}

/// The JWT of the App `issuer` (its client ID, or its ID), valid from a minute before `now` for
/// nine minutes, as GitHub allows at most ten.
pub(crate) fn jwt(issuer: &str, pem: &str, now: i64) -> Result<String, String> {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = json!({"iat": now - 60, "exp": now + 9 * 60, "iss": issuer});
    let input = format!("{header}.{}", URL_SAFE_NO_PAD.encode(claims.to_string()));
    let signature = zeabur_core::crypto::sign_rs256(pem, input.as_bytes())
        .map_err(|e| format!("the App's private key: {e}"))?;
    Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature)))
}

impl GitHub<'_> {
    async fn call(
        &self,
        method: http::Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&Value>,
    ) -> Result<(u16, Value), String> {
        let url = format!("{}{path}", self.api.trim_end_matches('/'));
        let mut request = Request::builder()
            .method(method)
            .uri(&url)
            .header(http::header::USER_AGENT, "zeabur")
            .header(http::header::ACCEPT, "application/vnd.github+json")
            .header("x-github-api-version", VERSION);
        if let Some(bearer) = bearer {
            let mut value = http::HeaderValue::from_str(&format!("Bearer {bearer}"))
                .map_err(|e| e.to_string())?;
            value.set_sensitive(true);
            request = request.header(http::header::AUTHORIZATION, value);
        }
        if body.is_some() {
            request = request.header(http::header::CONTENT_TYPE, "application/json");
        }
        let body = body.map_or_else(Full::default, |b| Full::new(Bytes::from(b.to_string())));
        let request = request.body(body).map_err(|e| e.to_string())?;
        let (status, body) = self.http.exchange(request, LIMIT).await?;
        let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        Ok((status, value))
    }

    async fn expect<T: serde::de::DeserializeOwned>(
        &self,
        method: http::Method,
        path: &str,
        bearer: Option<&str>,
        body: Option<&Value>,
        want: u16,
    ) -> Result<T, String> {
        let (status, value) = self.call(method, path, bearer, body).await?;
        if status != want {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("no message");
            return Err(format!("GitHub answered {status}: {message}"));
        }
        serde_json::from_value(value).map_err(|e| format!("GitHub's answer is not expected: {e}"))
    }

    /// The App made from a manifest, by the `code` GitHub sent back; a code works once, for an
    /// hour.
    pub(crate) async fn convert(&self, code: &str) -> Result<Conversion, String> {
        if code.is_empty()
            || !code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err("the code GitHub sent back is not valid".to_owned());
        }
        self.expect(
            http::Method::POST,
            &format!("/app-manifests/{code}/conversions"),
            None,
            None,
            201,
        )
        .await
    }

    /// The App's installations, by its `jwt`.
    pub(crate) async fn installations(&self, jwt: &str) -> Result<Vec<Installation>, String> {
        self.expect(
            http::Method::GET,
            "/app/installations?per_page=100",
            Some(jwt),
            None,
            200,
        )
        .await
    }

    /// One installation of the App, by its `jwt`.
    pub(crate) async fn installation(&self, jwt: &str, id: i64) -> Result<Installation, String> {
        self.expect(
            http::Method::GET,
            &format!("/app/installations/{id}"),
            Some(jwt),
            None,
            200,
        )
        .await
    }

    /// The repositories installation `id` can read, with a token that may read their metadata
    /// only; at most [`PAGES`] pages of 100.
    pub(crate) async fn repositories(&self, jwt: &str, id: i64) -> Result<Vec<Repository>, String> {
        #[derive(Deserialize)]
        struct Token {
            token: String,
        }
        #[derive(Deserialize)]
        struct Page {
            repositories: Vec<Repository>,
        }
        let Token { token } = self
            .expect(
                http::Method::POST,
                &format!("/app/installations/{id}/access_tokens"),
                Some(jwt),
                Some(&json!({"permissions": {"metadata": "read"}})),
                201,
            )
            .await?;
        let mut out = Vec::new();
        for page in 1..=PAGES {
            let Page { repositories } = self
                .expect(
                    http::Method::GET,
                    &format!("/installation/repositories?per_page=100&page={page}"),
                    Some(&token),
                    None,
                    200,
                )
                .await?;
            let last = repositories.len() < 100;
            out.extend(repositories);
            if last {
                break;
            }
        }
        out.sort_by(|a, b| a.full_name.cmp(&b.full_name));
        Ok(out)
    }
}
