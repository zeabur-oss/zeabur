//! The Zeabur server of a cluster, over HTTP with the person's own token.
//!
//! The server acts as the person who holds the token, so it may do what that person may and
//! nothing else. Every request says where it comes from (`Origin` and `Zeabur-Request`), as the
//! server asks of any change.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{HeaderValue, Method, Request, Uri, header};
use http_body_util::{BodyExt as _, Full, Limited};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use serde_json::{Value, json};
use zeabur_kube::UserToken;

/// The largest answer read.
const MAX_BODY: usize = 64 * 1024 * 1024;
/// How long a request that is not an upload may take.
const QUICK: Duration = Duration::from_secs(60);
/// How long an upload may take.
const UPLOAD: Duration = Duration::from_hours(1);

/// A Zeabur server and the token to call it with.
#[derive(Clone)]
pub(crate) struct Server {
    origin: String,
    token: UserToken,
    client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}

impl Server {
    /// The server at `address`, `https://host[:port]`. `http://` is only for this computer
    /// (`localhost`, `127.0.0.1`, `::1`), as the token must not cross a network in clear text.
    pub(crate) fn new(address: &str, token: UserToken) -> Result<Self, String> {
        let uri: Uri = address
            .trim()
            .parse()
            .map_err(|e| format!("--server {address:?}: {e}"))?;
        let scheme = uri.scheme_str().unwrap_or_default();
        let plain = matches!(scheme, "http" | "https")
            && uri.authority().is_some()
            && uri.path_and_query().is_none_or(|p| p.as_str() == "/");
        let authority = uri
            .authority()
            .map_or_else(String::new, ToString::to_string);
        if !plain {
            return Err(format!(
                "--server {address:?} is not an address like https://zeabur.example.com"
            ));
        }
        let local = matches!(
            uri.host().unwrap_or_default(),
            "localhost" | "127.0.0.1" | "[::1]"
        );
        if scheme == "http" && !local {
            return Err(format!(
                "--server {address:?} uses http://, which sends your token in clear text. Use https://; http:// works only for localhost."
            ));
        }
        Ok(Self {
            origin: format!("{scheme}://{authority}"),
            token,
            client: Client::builder(TokioExecutor::new()).build(connector()?),
        })
    }

    /// `GET` `path_and_query`: the JSON of a success, or the problem the server answered with.
    pub(crate) async fn get(&self, path_and_query: &str) -> Result<Value, Value> {
        self.send(Method::GET, path_and_query, None, Vec::new(), QUICK)
            .await
    }

    /// `POST` `body` as JSON.
    pub(crate) async fn post(&self, path_and_query: &str, body: &Value) -> Result<Value, Value> {
        let bytes = serde_json::to_vec(body)
            .map_err(|e| problem(&format!("The request is not JSON: {e}")))?;
        self.send(
            Method::POST,
            path_and_query,
            Some("application/json"),
            bytes,
            QUICK,
        )
        .await
    }

    /// `POST` `bytes` as `content_type`.
    pub(crate) async fn post_bytes(
        &self,
        path_and_query: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, Value> {
        self.send(
            Method::POST,
            path_and_query,
            Some(content_type),
            bytes,
            UPLOAD,
        )
        .await
    }

    async fn send(
        &self,
        method: Method,
        path_and_query: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
        deadline: Duration,
    ) -> Result<Value, Value> {
        let mut authorization = self.token.authorization().clone();
        authorization.set_sensitive(true);
        let mut request = Request::builder()
            .method(method)
            .uri(format!("{}{path_and_query}", self.origin))
            .header(header::AUTHORIZATION, authorization)
            .header(header::ORIGIN, &self.origin)
            .header(header::USER_AGENT, "zeaburctl")
            .header("zeabur-request", "1")
            .header(header::CONTENT_LENGTH, body.len());
        if let Some(content_type) = content_type {
            request = request.header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(content_type)
                    .map_err(|e| problem(&format!("{content_type}: {e}")))?,
            );
        }
        let request = request
            .body(Full::new(Bytes::from(body)))
            .map_err(|e| problem(&format!("{path_and_query} is not a request path: {e}")))?;
        let exchange = async {
            let response =
                self.client.request(request).await.map_err(|e| {
                    problem(&format!("Unable to reach {}: {}", self.origin, cause(&e)))
                })?;
            let status = response.status();
            let body = Limited::new(response.into_body(), MAX_BODY)
                .collect()
                .await
                .map_err(|e| problem(&format!("Unable to read the answer: {e}")))?
                .to_bytes();
            let value: Value = serde_json::from_slice(&body).unwrap_or_else(|_| {
                problem(&format!(
                    "{} answered {status}, which is not JSON.",
                    self.origin
                ))
            });
            if status.is_success() {
                Ok(value)
            } else {
                Err(value)
            }
        };
        tokio::time::timeout(deadline, exchange)
            .await
            .map_err(|_elapsed| problem(&format!("{} did not answer in time.", self.origin)))?
    }
}

/// A problem of `zeaburctl`'s own, in the server's shape.
fn problem(detail: &str) -> Value {
    json!({"title": "zeaburctl", "detail": detail})
}

/// The innermost cause, which names what failed (DNS, TLS, a refused connection).
fn cause(error: &(dyn std::error::Error + 'static)) -> String {
    let mut source = error;
    while let Some(next) = source.source() {
        source = next;
    }
    source.to_string()
}

/// HTTPS with the platform's roots, or plain HTTP.
pub(crate) fn connector() -> Result<HttpsConnector<HttpConnector>, String> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let verifier =
        rustls_platform_verifier::Verifier::new_with_extra_roots(Vec::new(), Arc::clone(&provider))
            .map_err(|e| e.to_string())?;
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    let mut http = HttpConnector::new();
    http.enforce_http(false);
    http.set_connect_timeout(Some(QUICK));
    Ok(hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config(tls)
        .https_or_http()
        .enable_http1()
        .wrap_connector(http))
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use super::*;

    fn token() -> UserToken {
        UserToken::new(SecretString::from("t".to_owned())).unwrap()
    }

    #[test]
    fn the_address_is_a_server_and_nothing_below_it() {
        for good in [
            "https://zeabur.example.com",
            "http://127.0.0.1:8080/",
            "http://localhost:8080",
            "http://[::1]:8080",
            " https://a.b:8443 ",
        ] {
            assert!(Server::new(good, token()).is_ok(), "{good}");
        }
        for bad in [
            "zeabur.example.com",
            "ftp://a.b",
            "https://a.b/projects",
            "http://zeabur.example.com",
            "http://10.0.0.5:8080",
            "http://localhost.example.com",
            "",
        ] {
            assert!(Server::new(bad, token()).is_err(), "{bad}");
        }
    }
}
