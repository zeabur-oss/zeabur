#![doc = include_str!("../README.md")]
//! # Identity
//!
//! A [`UserClient`] is built from two separate things: a [`ClusterEndpoint`] (where
//! kube-apiserver is and how to trust its certificate) and the person's credential. The endpoint
//! type has no field for a credential, and [`ClusterEndpoint::in_cluster`] reads only the
//! address and the CA bundle of the pod's service account, never its token. So the only identity
//! a request from this crate can carry is the one the person brought, and kube-apiserver
//! authorises it with that person's own RBAC rights [k8s-authn] [k8s-authz]. The one exception
//! is [`UserClient::with_token_file`]: Zeabur's own service account, which the chart lets into
//! the terminal namespace only (see `zeabur_api::tools`). Zeabur does not
//! impersonate: impersonation would need `impersonate` rights on Zeabur's service account, which
//! would then be able to do anything any user can [k8s-impersonation].
//!
//! # References
//!
//! - [k8s-authn] The Kubernetes Authors, "Authenticating," Kubernetes Documentation. <https://kubernetes.io/docs/reference/access-authn-authz/authentication/>
//! - [k8s-authz] The Kubernetes Authors, "Using RBAC Authorization," Kubernetes Documentation. <https://kubernetes.io/docs/reference/access-authn-authz/rbac/>
//! - [k8s-impersonation] The Kubernetes Authors, "User impersonation," in "Authenticating,"
//!   Kubernetes Documentation.
//!   <https://kubernetes.io/docs/reference/access-authn-authz/authentication/#user-impersonation>
//!
//! [k8s-authn]: https://kubernetes.io/docs/reference/access-authn-authz/authentication/
//! [k8s-authz]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/
//! [k8s-impersonation]: https://kubernetes.io/docs/reference/access-authn-authz/authentication/#user-impersonation

use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

mod adapter;
pub mod files;
mod logs;
#[cfg(feature = "memory")]
pub mod memory;
mod names;
pub mod objects;
pub mod port;
mod wide;
mod write;

/// The Kubernetes object types, for the Kubernetes version Zeabur supports at minimum (1.34).
pub use k8s_openapi;
pub use kube::api::DynamicObject;
pub use logs::unavailable_log;
pub use objects::{Access, AccessRule, AccessRules, ResourceRef, ServedResource};
pub use port::{
    Answer, Cluster, ClusterError, ClusterEvents, ClusterExt, ClusterFacts, ClusterPod,
    ClusterPods, ContainerResources, ExecRequest, ExecSession, ExecStream, ExitStatus,
    FIELD_MANAGER, GatewayRef, Identity, LogLines, LogOptions, NamespaceObjects, ObjectTarget,
    Preconditions, Resize, SecretSummary, ServicePort, TerminalSize, Unreadable, WriteBody,
    WriteRequest, Written,
};
use secrecy::{ExposeSecret as _, SecretString};

/// Where kube-apiserver is and how to verify it. Holds no credential.
///
/// Its clones share one pool of connections, which every [`UserClient::with_token`] client of
/// the endpoint sends through, so the connections open at once follow the requests in flight,
/// not the number of signed-in people. A connection carries no identity: each request carries
/// its person's bearer token, and kube-apiserver authenticates every request on its own
/// [k8s-authn]. The pool's TLS has no client certificate, and its verifier re-reads the CA
/// bundle, so a rotated bundle is still picked up.
///
/// [k8s-authn]: https://kubernetes.io/docs/reference/access-authn-authz/authentication/
#[derive(Clone)]
pub struct ClusterEndpoint {
    url: http::Uri,
    root_certs: Option<Vec<Vec<u8>>>,
    root_cert_file: Option<PathBuf>,
    tls_server_name: Option<String>,
    pool: Arc<Mutex<Option<Pool>>>,
}

impl fmt::Debug for ClusterEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClusterEndpoint")
            .field("url", &self.url)
            .field("tls_server_name", &self.tls_server_name)
            .finish_non_exhaustive()
    }
}

impl ClusterEndpoint {
    /// An endpoint at `url`, trusting `root_certs` (DER), or the system roots when `None`.
    #[must_use]
    pub fn new(url: http::Uri, root_certs: Option<Vec<Vec<u8>>>) -> Self {
        Self {
            url,
            root_certs,
            root_cert_file: None,
            tls_server_name: None,
            pool: Arc::default(),
        }
    }

    /// The cluster this pod runs in: the address from the environment and the service account's
    /// CA bundle (re-read when it rotates). The service account's token is not read.
    ///
    /// # Errors
    ///
    /// When the pod has no service account mount or the environment lacks the API address.
    pub fn in_cluster() -> Result<Self, ConnectError> {
        Ok(Self::from_config(
            kube::Config::incluster().map_err(|e| ConnectError::Endpoint(e.to_string()))?,
        ))
    }

    /// The address and trust settings of a kube-rs configuration, without its credentials.
    #[must_use]
    pub fn from_config(config: kube::Config) -> Self {
        Self {
            url: config.cluster_url,
            root_certs: config.root_cert,
            root_cert_file: config.root_cert_file,
            tls_server_name: config.tls_server_name,
            pool: Arc::default(),
        }
    }

    /// The API server's URL.
    #[must_use]
    pub const fn url(&self) -> &http::Uri {
        &self.url
    }

    fn config(&self) -> kube::Config {
        let mut config = kube::Config::new(self.url.clone());
        config.root_cert.clone_from(&self.root_certs);
        config.root_cert_file.clone_from(&self.root_cert_file);
        config.tls_server_name.clone_from(&self.tls_server_name);
        config
    }

    /// The endpoint's pool, made on first use. It is made from [`Self::config`], which has no
    /// credential, so its connections present no client certificate.
    fn pool(&self) -> Result<Pool, ConnectError> {
        let mut slot = self.pool.lock().unwrap_or_else(PoisonError::into_inner);
        let pool = match slot.as_ref() {
            Some(pool) => pool.clone(),
            None => slot.insert(pool(&self.config(), POOL_IDLE)?).clone(),
        };
        drop(slot);
        Ok(pool)
    }
}

/// A bearer token a person presented (in the cluster, an access token issued by Zeabur Identity).
///
/// It holds the token and one `Authorization` header value made from it. Clones share both, and
/// every request that carries the token sends that header value.
#[derive(Clone)]
pub struct UserToken(Arc<Bearer>);

struct Bearer {
    token: SecretString,
    authorization: http::HeaderValue,
}

impl UserToken {
    /// Wraps a token. It is never logged or printed.
    ///
    /// # Errors
    ///
    /// When the token cannot be part of an HTTP header.
    pub fn new(token: SecretString) -> Result<Self, ConnectError> {
        let value = zeabur_core::secret::join(&["Bearer ", token.expose_secret()]);
        // The conversion moves the buffer into the header value. It does not copy it.
        let mut authorization =
            http::HeaderValue::try_from(value).map_err(|_invalid| ConnectError::Token)?;
        authorization.set_sensitive(true);
        Ok(Self(Arc::new(Bearer {
            token,
            authorization,
        })))
    }

    /// The bearer token the kubeconfig signs `context` (the current one when `None`) in with,
    /// as the kubeconfig's own token, token file or credential plugin gives it now.
    ///
    /// # Errors
    ///
    /// When the kubeconfig cannot be used, or signs in without a bearer token (a client
    /// certificate, a password).
    pub async fn from_kubeconfig(context: Option<&str>) -> Result<Self, ConnectError> {
        let options = kube::config::KubeConfigOptions {
            context: context.map(str::to_owned),
            ..kube::config::KubeConfigOptions::default()
        };
        let config = kube::Config::from_kubeconfig(&options)
            .await
            .map_err(|e| ConnectError::Kubeconfig(e.to_string()))?;
        Self::signed_in_by(config).await
    }

    async fn signed_in_by(config: kube::Config) -> Result<Self, ConnectError> {
        use kube::client::ConfigExt as _;
        let seen = Arc::new(Mutex::new(None::<http::HeaderValue>));
        let sink = Arc::clone(&seen);
        let capture = tower::service_fn(move |request: http::Request<kube::client::Body>| {
            let header = request.headers().get(http::header::AUTHORIZATION).cloned();
            *sink.lock().unwrap_or_else(PoisonError::into_inner) = header;
            std::future::ready(Ok::<_, std::convert::Infallible>(http::Response::new(
                kube::client::Body::empty(),
            )))
        });
        let service = tower::ServiceBuilder::new()
            .option_layer(config.auth_layer().map_err(|e| refused(&e))?)
            .map_err(tower::BoxError::from)
            .service(capture);
        let probe = kube::Client::new(service, config.default_namespace);
        // The answer is empty on purpose; only the header the auth layer adds matters.
        drop(probe.apiserver_version().await);
        let header = seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .and_then(|value| value.to_str().ok().map(str::to_owned));
        let token = header
            .as_deref()
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or_else(|| {
                ConnectError::Kubeconfig("it does not sign in with a bearer token".to_owned())
            })?;
        Self::new(SecretString::from(token.to_owned()))
    }

    /// The token.
    #[must_use]
    pub fn secret(&self) -> &SecretString {
        &self.0.token
    }

    /// `Bearer <token>`, marked sensitive.
    #[must_use]
    pub fn authorization(&self) -> &http::HeaderValue {
        &self.0.authorization
    }
}

impl fmt::Debug for UserToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UserToken(…)")
    }
}

/// A connection to kube-apiserver that acts as one person.
#[derive(Clone)]
pub struct UserClient {
    client: kube::Client,
    name: Option<String>,
}

impl fmt::Debug for UserClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserClient").finish_non_exhaustive()
    }
}

impl UserClient {
    /// Acts as the holder of `token`.
    ///
    /// # Errors
    ///
    /// When the TLS or HTTP client cannot be built.
    pub fn with_token(endpoint: &ClusterEndpoint, token: &UserToken) -> Result<Self, ConnectError> {
        let authorization = token.authorization().clone();
        Ok(Self {
            client: client(endpoint.config(), endpoint.pool()?, Some(authorization))?,
            name: None,
        })
    }

    /// Acts as the holder of the token in `path`, which is read again as it rotates. Only for
    /// Zeabur's own service account in the terminal namespace (see `zeabur_api::tools`).
    ///
    /// # Errors
    ///
    /// When the TLS or HTTP client cannot be built, or the file cannot be read.
    pub fn with_token_file(
        endpoint: &ClusterEndpoint,
        path: &std::path::Path,
    ) -> Result<Self, ConnectError> {
        let mut config = endpoint.config();
        config.auth_info.token_file = Some(path.to_string_lossy().into_owned());
        Ok(Self {
            client: client(config, endpoint.pool()?, None)?,
            name: None,
        })
    }

    /// Acts as the person at this machine, with a kubeconfig context (the current one when
    /// `context` is `None`). This is local mode: the credential is theirs, whatever it is.
    ///
    /// # Errors
    ///
    /// When the kubeconfig cannot be read or names no usable context.
    pub async fn from_kubeconfig(context: Option<&str>) -> Result<Self, ConnectError> {
        let options = kube::config::KubeConfigOptions {
            context: context.map(str::to_owned),
            ..kube::config::KubeConfigOptions::default()
        };
        let config = kube::Config::from_kubeconfig(&options)
            .await
            .map_err(|e| ConnectError::Kubeconfig(e.to_string()))?;
        Self::from_kube_config(config, kubeconfig_cluster(context))
    }

    /// A kubeconfig may name a client certificate, which binds a connection itself to the
    /// person, so this client keeps a pool of its own.
    fn from_kube_config(config: kube::Config, name: Option<String>) -> Result<Self, ConnectError> {
        let pool = pool(&config, POOL_IDLE)?;
        client(config, pool, None).map(|client| Self { client, name })
    }

    /// What Zeabur publishes about itself in the ConfigMap [`INFO_CONFIG_MAP`] in
    /// [`INFO_NAMESPACE`], which every signed-in person may read. Empty when the cluster has
    /// none.
    ///
    /// # Errors
    ///
    /// When kube-apiserver cannot be reached or refuses the read.
    pub async fn zeabur_info(&self) -> Result<ZeaburInfo, ConnectError> {
        let maps = kube::Api::<k8s_openapi::api::core::v1::ConfigMap>::namespaced(
            self.client.clone(),
            INFO_NAMESPACE,
        );
        let map = maps
            .get_opt(INFO_CONFIG_MAP)
            .await
            .map_err(|e| ConnectError::Read(e.to_string()))?;
        Ok(map.map_or_default(info_of))
    }

    /// The underlying kube-rs client, bound to this person.
    #[must_use]
    pub const fn kube(&self) -> &kube::Client {
        &self.client
    }
}

/// The data of the ConfigMap [`INFO_CONFIG_MAP`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZeaburInfo {
    /// Where Zeabur is.
    pub url: Option<String>,
    /// The Zeabur Identity issuer whose tokens Zeabur accepts.
    pub issuer: Option<String>,
}

fn info_of(map: k8s_openapi::api::core::v1::ConfigMap) -> ZeaburInfo {
    let mut data = map.data.unwrap_or_default();
    let mut take = |key: &str| {
        let value = data.remove(key)?;
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    };
    ZeaburInfo {
        url: take("url"),
        issuer: take("issuer"),
    }
}

/// Whether `token` is a JWT whose `iss` is `issuer`. The signature is not checked here;
/// kube-apiserver checks it.
#[must_use]
pub fn issued_by(token: &str, issuer: &str) -> bool {
    use base64::Engine as _;
    let mut parts = token.split('.');
    let (Some(_header), Some(payload), Some(_signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let Ok(payload) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else {
        return false;
    };
    serde_json::from_slice::<serde_json::Value>(&payload)
        .ok()
        .and_then(|claims| claims.get("iss")?.as_str().map(str::to_owned))
        .is_some_and(|iss| iss.trim_end_matches('/') == issuer.trim_end_matches('/'))
}

/// The namespace every signed-in person may read, as kubeadm's cluster-info lives in.
pub const INFO_NAMESPACE: &str = "kube-public";
/// The ConfigMap whose `url` is where Zeabur is and whose `issuer` signs its people in.
pub const INFO_CONFIG_MAP: &str = "zeabur-info";

/// How long a connection to kube-apiserver stays open unused.
const POOL_IDLE: std::time::Duration = std::time::Duration::from_secs(30);

type Pool = hyper_util::client::legacy::Client<
    hyper_timeout::TimeoutConnector<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    >,
    kube::client::Body,
>;

fn refused(error: &kube::Error) -> ConnectError {
    ConnectError::Client(error.to_string())
}

/// The connections of kube-rs's default client (`kube::Client::try_from`): TLS as `config`
/// says, its connect, read and write timeouts, in a pool that closes idle connections. The
/// default pool has no timer, so it keeps every connection a burst of requests opened for as
/// long as the pool lives.
fn pool(config: &kube::Config, idle: std::time::Duration) -> Result<Pool, ConnectError> {
    use kube::client::ConfigExt as _;
    let mut http = hyper_util::client::legacy::connect::HttpConnector::new();
    http.enforce_http(false);
    let https = config
        .rustls_https_connector_with_connector(http)
        .map_err(|e| refused(&e))?;
    let mut timed = hyper_timeout::TimeoutConnector::new(https);
    timed.set_connect_timeout(config.connect_timeout);
    timed.set_read_timeout(config.read_timeout);
    timed.set_write_timeout(config.write_timeout);
    Ok(
        hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            .pool_timer(hyper_util::rt::TokioTimer::new())
            .pool_idle_timeout(idle)
            .build(timed),
    )
}

/// The layers of kube-rs's default client (retry, auth, extra headers, tracing) over `pool`.
/// Each request carries `authorization` if it is set, else the credential of `config`.
fn client(
    config: kube::Config,
    pool: Pool,
    authorization: Option<http::HeaderValue>,
) -> Result<kube::Client, ConnectError> {
    use kube::client::ConfigExt as _;
    use kube::client::retry::RetryPolicy;
    use tower_http::classify::ServerErrorsFailureClass;
    let service = tower::ServiceBuilder::new()
        .layer(config.base_uri_layer())
        .option_layer(
            config
                .default_retry
                .then(|| tower::retry::RetryLayer::new(RetryPolicy::server_retry())),
        )
        .option_layer(config.auth_layer().map_err(|e| refused(&e))?)
        .option_layer(authorization.map(|value| {
            tower::util::MapRequestLayer::new(move |mut request: http::Request<_>| {
                request
                    .headers_mut()
                    .insert(http::header::AUTHORIZATION, value.clone());
                request
            })
        }))
        .layer(config.extra_headers_layer().map_err(|e| refused(&e))?)
        // kube-rs's request tracing, with its span and fields (OpenTelemetry's HTTP client
        // conventions).
        .layer(
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(|request: &http::Request<kube::client::Body>| {
                    tracing::debug_span!(
                        "HTTP",
                        http.method = %request.method(),
                        http.url = %request.uri(),
                        http.status_code = tracing::field::Empty,
                        otel.name = request.extensions().get::<&'static str>().unwrap_or(&"HTTP"),
                        otel.kind = "client",
                        otel.status_code = tracing::field::Empty,
                    )
                })
                .on_request(|_: &http::Request<kube::client::Body>, _: &tracing::Span| {
                    tracing::debug!("requesting");
                })
                .on_response(
                    |response: &http::Response<hyper::body::Incoming>,
                     _: std::time::Duration,
                     span: &tracing::Span| {
                        let status = response.status();
                        span.record("http.status_code", status.as_u16());
                        if status.is_client_error() || status.is_server_error() {
                            span.record("otel.status_code", "ERROR");
                        }
                    },
                )
                .on_body_chunk(())
                .on_eos(
                    |_: Option<&http::HeaderMap>, _: std::time::Duration, _: &tracing::Span| {
                        tracing::debug!("stream closed");
                    },
                )
                .on_failure(
                    |class: ServerErrorsFailureClass,
                     _: std::time::Duration,
                     span: &tracing::Span| {
                        span.record("otel.status_code", "ERROR");
                        match class {
                            ServerErrorsFailureClass::StatusCode(status) => {
                                span.record("http.status_code", status.as_u16());
                                tracing::error!("failed with status {status}");
                            }
                            ServerErrorsFailureClass::Error(error) => {
                                tracing::error!("failed with error {error}");
                            }
                        }
                    },
                ),
        )
        .map_err(tower::BoxError::from)
        .service(pool);
    Ok(kube::Client::new(service, config.default_namespace))
}

/// The kubeconfig's name for the cluster `context` (the current context when `None`) uses, as
/// `kubectl config get-contexts` shows it; `None` when the kubeconfig cannot be read for it.
fn kubeconfig_cluster(context: Option<&str>) -> Option<String> {
    cluster_of(kube::config::Kubeconfig::read().ok()?, context)
}

fn cluster_of(kubeconfig: kube::config::Kubeconfig, context: Option<&str>) -> Option<String> {
    let context = context.map(str::to_owned).or(kubeconfig.current_context)?;
    kubeconfig
        .contexts
        .into_iter()
        .find(|named| named.name == context)?
        .context
        .map(|c| c.cluster)
        .filter(|cluster| !cluster.is_empty())
}

/// Why a [`UserClient`] could not be built.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    /// The in-cluster address or CA bundle is missing.
    #[error("cannot find the cluster this pod runs in: {0}")]
    Endpoint(String),
    /// The kubeconfig could not be used.
    #[error("cannot use the kubeconfig: {0}")]
    Kubeconfig(String),
    /// The HTTP client could not be built.
    #[error("cannot build the Kubernetes client: {0}")]
    Client(String),
    /// A ConfigMap Zeabur publishes could not be read.
    #[error("cannot read {INFO_NAMESPACE}/{INFO_CONFIG_MAP}: {0}")]
    Read(String),
    /// The token has a character that an HTTP header cannot carry.
    #[error("the token is not a valid bearer token")]
    Token,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeabur_info_is_what_the_config_map_says() {
        let map = |data: serde_json::Value| {
            serde_json::from_value::<k8s_openapi::api::core::v1::ConfigMap>(
                serde_json::json!({"metadata": {"name": INFO_CONFIG_MAP}, "data": data}),
            )
            .unwrap()
        };
        assert_eq!(
            info_of(map(serde_json::json!({
                "url": " https://zeabur.example.com\n",
                "issuer": "https://identity.example.com",
            }))),
            ZeaburInfo {
                url: Some("https://zeabur.example.com".to_owned()),
                issuer: Some("https://identity.example.com".to_owned()),
            }
        );
        assert_eq!(
            info_of(map(serde_json::json!({"url": ""}))),
            ZeaburInfo::default()
        );
    }

    #[tokio::test]
    async fn a_kubeconfig_token_is_the_bearer_token_and_a_certificate_is_none() {
        let mut config = kube::Config::new(http::Uri::from_static("https://10.0.0.1:6443"));
        config.auth_info.token = Some(SecretString::from("person-token"));
        let token = UserToken::signed_in_by(config.clone()).await.unwrap();
        assert_eq!(token.secret().expose_secret(), "person-token");
        config.auth_info.token = None;
        config.auth_info.client_certificate = Some("/cert".to_owned());
        assert!(matches!(
            UserToken::signed_in_by(config).await,
            Err(ConnectError::Kubeconfig(_))
        ));
    }

    #[test]
    fn an_endpoint_drops_every_credential_of_the_config_it_came_from() {
        let mut config = kube::Config::new(http::Uri::from_static("https://10.0.0.1:6443"));
        config.auth_info.token = Some(SecretString::from("service-account-token"));
        config.auth_info.token_file = Some("/var/run/secrets/token".to_owned());
        config.auth_info.client_certificate = Some("/cert".to_owned());
        config.auth_info.username = Some("zeabur".to_owned());
        config.auth_info.impersonate = Some("someone".to_owned());

        let rebuilt = ClusterEndpoint::from_config(config).config();
        let auth = rebuilt.auth_info;
        assert!(auth.token.is_none() && auth.token_file.is_none());
        assert!(auth.client_certificate.is_none() && auth.client_key.is_none());
        assert!(auth.username.is_none() && auth.password.is_none());
        assert!(auth.impersonate.is_none() && auth.impersonate_groups.is_none());
        assert!(auth.exec.is_none() && auth.auth_provider.is_none());
        assert_eq!(rebuilt.headers.len(), 0);
    }

    #[test]
    fn the_cluster_is_named_as_the_kubeconfig_names_the_contexts_cluster() {
        let kubeconfig = kube::config::Kubeconfig::from_yaml(
            "apiVersion: v1
kind: Config
current-context: dev
contexts:
- name: dev
  context: {cluster: k3d-dev, user: alice}
- name: prod
  context: {cluster: prod-eu, user: alice}
",
        )
        .unwrap();
        assert_eq!(
            cluster_of(kubeconfig.clone(), None).as_deref(),
            Some("k3d-dev")
        );
        assert_eq!(
            cluster_of(kubeconfig.clone(), Some("prod")).as_deref(),
            Some("prod-eu")
        );
        assert_eq!(cluster_of(kubeconfig, Some("gone")), None);
    }

    #[tokio::test]
    async fn in_the_cluster_the_api_servers_address_names_nothing() {
        let endpoint = ClusterEndpoint::new(http::Uri::from_static("https://10.43.0.1:443"), None);
        let client = UserClient::with_token(&endpoint, &token("t")).unwrap();
        assert_eq!(Cluster::kubeconfig_name(&client), None);
    }

    /// One request off `socket`, `None` once the client has closed it.
    async fn request(socket: &mut tokio::net::TcpStream) -> Option<String> {
        use tokio::io::AsyncReadExt as _;
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = socket.read(&mut buffer).await.unwrap();
            if n == 0 {
                return None;
            }
            request.extend_from_slice(&buffer[..n]);
        }
        Some(String::from_utf8(request).unwrap().to_ascii_lowercase())
    }

    async fn answer(socket: &mut tokio::net::TcpStream) {
        use tokio::io::AsyncWriteExt as _;
        socket
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}")
            .await
            .unwrap();
    }

    async fn version(client: &UserClient) -> Result<String, kube::Error> {
        client
            .kube()
            .request_text(http::Request::get("/version").body(Vec::new()).unwrap())
            .await
    }

    async fn listen() -> (tokio::net::TcpListener, http::Uri) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        (listener, url.parse().unwrap())
    }

    fn own_pool(config: kube::Config, idle: std::time::Duration) -> UserClient {
        let pool = pool(&config, idle).unwrap();
        UserClient {
            client: client(config, pool, None).unwrap(),
            name: None,
        }
    }

    const PATIENCE: std::time::Duration = std::time::Duration::from_secs(5);

    #[tokio::test]
    async fn people_share_the_endpoints_connections_and_each_sends_their_own_token() {
        let (listener, url) = listen().await;
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut requests = Vec::new();
            while requests.len() < 2
                && let Some(request) = request(&mut socket).await
            {
                requests.push(request);
                answer(&mut socket).await;
            }
            let another =
                tokio::time::timeout(std::time::Duration::from_millis(200), listener.accept())
                    .await
                    .is_ok();
            (requests, another)
        });
        let endpoint = ClusterEndpoint::new(url, None);
        let alice = UserClient::with_token(&endpoint, &token("alice-token")).unwrap();
        let bob = UserClient::with_token(&endpoint.clone(), &token("bob-token")).unwrap();
        assert_eq!(version(&alice).await.unwrap(), "{}");
        assert_eq!(version(&bob).await.unwrap(), "{}");
        let (requests, another) = tokio::time::timeout(PATIENCE, server)
            .await
            .unwrap()
            .unwrap();
        assert!(!another, "Bob's request opened a connection of its own");
        let [first, second] = requests.as_slice() else {
            panic!("{requests:?}")
        };
        assert!(first.contains("authorization: bearer alice-token"));
        assert!(second.contains("authorization: bearer bob-token"));
        assert!(!second.contains("alice-token"));
    }

    /// Answers `429` and then `200` on one connection, and counts the requests.
    async fn busy_then_ok(listener: tokio::net::TcpListener) -> usize {
        use tokio::io::AsyncWriteExt as _;
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut seen = 0;
        while request(&mut socket).await.is_some() {
            seen += 1;
            let reply = if seen == 1 {
                let status = r#"{"kind":"Status","apiVersion":"v1","status":"Failure","code":429,"reason":"TooManyRequests"}"#;
                format!(
                    "HTTP/1.1 429 Too Many Requests\r\nretry-after: 0\r\ncontent-length: {}\r\n\r\n{status}",
                    status.len()
                )
            } else {
                "HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}".to_owned()
            };
            socket.write_all(reply.as_bytes()).await.unwrap();
        }
        seen
    }

    #[tokio::test]
    async fn other_requests_are_still_retried_when_busy() {
        let (listener, url) = listen().await;
        let server = tokio::spawn(busy_then_ok(listener));
        let client = own_pool(kube::Config::new(url), std::time::Duration::from_secs(60));
        assert_eq!(version(&client).await.unwrap(), "{}");
        drop(client);
        assert_eq!(
            tokio::time::timeout(PATIENCE, server)
                .await
                .unwrap()
                .unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn an_idle_connection_is_closed() {
        let (listener, url) = listen().await;
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            request(&mut socket).await.unwrap();
            answer(&mut socket).await;
            // Kept alive until the pool lets it go.
            request(&mut socket).await.is_none()
        });
        let client = own_pool(
            kube::Config::new(url),
            std::time::Duration::from_millis(100),
        );
        assert_eq!(version(&client).await.unwrap(), "{}");
        let closed = tokio::time::timeout(PATIENCE, server)
            .await
            .expect("the idle connection was not closed")
            .unwrap();
        assert!(closed);
    }

    #[tokio::test]
    async fn an_api_server_that_stops_answering_times_out() {
        let (listener, url) = listen().await;
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            request(&mut socket).await;
            // Never answers.
            std::future::pending::<()>().await;
        });
        let mut config = kube::Config::new(url);
        config.read_timeout = Some(std::time::Duration::from_millis(100));
        let client = own_pool(config, POOL_IDLE);
        let answered = tokio::time::timeout(PATIENCE, version(&client))
            .await
            .expect("the request did not time out");
        assert!(answered.is_err());
        server.abort();
    }

    fn token(token: &str) -> UserToken {
        UserToken::new(SecretString::from(token)).unwrap()
    }

    #[test]
    fn tokens_do_not_leak_through_debug() {
        let token = token("eyJhbGciOi.secret");
        assert!(!format!("{token:?}").contains("secret"));
        assert!(!format!("{:?}", token.authorization()).contains("secret"));
    }

    #[test]
    fn a_token_that_a_header_cannot_carry_is_refused() {
        assert!(matches!(
            UserToken::new(SecretString::from("a\nb")),
            Err(ConnectError::Token)
        ));
    }
}
