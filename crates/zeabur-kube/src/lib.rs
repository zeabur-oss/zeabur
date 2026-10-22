#![doc = include_str!("../README.md")]
//! # Identity
//!
//! A [`UserClient`] is built from two separate things: a [`ClusterEndpoint`] (where
//! kube-apiserver is and how to trust its certificate) and the person's credential. The endpoint
//! type has no field for a credential, and [`ClusterEndpoint::in_cluster`] reads only the
//! address and the CA bundle of the pod's service account, never its token. So the only identity
//! a request from this crate can carry is the one the person brought, and kube-apiserver
//! authorises it with that person's own RBAC rights [k8s-authn] [k8s-authz]. Zeabur does not
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

mod adapter;
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
    ClusterPods, ContainerResources, ExecRequest, ExecSession, ExitStatus, FIELD_MANAGER,
    GatewayRef, Identity, LogLines, LogOptions, NamespaceObjects, ObjectTarget, Preconditions,
    Resize, SecretSummary, ServicePort, TerminalSize, Unreadable, WriteBody, WriteRequest, Written,
};
use secrecy::SecretString;

/// Where kube-apiserver is and how to verify it. Holds no credential.
#[derive(Clone)]
pub struct ClusterEndpoint {
    url: http::Uri,
    root_certs: Option<Vec<Vec<u8>>>,
    root_cert_file: Option<PathBuf>,
    tls_server_name: Option<String>,
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
    pub const fn new(url: http::Uri, root_certs: Option<Vec<Vec<u8>>>) -> Self {
        Self {
            url,
            root_certs,
            root_cert_file: None,
            tls_server_name: None,
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
}

/// A bearer token a person presented (in the cluster, an access token issued by Zeabur Identity).
#[derive(Clone)]
pub struct UserToken(SecretString);

impl UserToken {
    /// Wraps a token. It is never logged or printed.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self(SecretString::from(token.into()))
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
        let mut config = endpoint.config();
        config.auth_info.token = Some(token.0.clone());
        Self::from_kube_config(config, None)
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

    fn from_kube_config(config: kube::Config, name: Option<String>) -> Result<Self, ConnectError> {
        client(config, POOL_IDLE).map(|client| Self { client, name })
    }

    /// The underlying kube-rs client, bound to this person.
    #[must_use]
    pub const fn kube(&self) -> &kube::Client {
        &self.client
    }
}

/// How long a connection to kube-apiserver stays open unused.
const POOL_IDLE: std::time::Duration = std::time::Duration::from_secs(30);

/// kube-rs's default stack (`kube::Client::try_from`) less its tracing and write timeout, with
/// a pool that closes idle connections. The default pool has no timer, so it keeps every
/// connection a burst of requests opened for as long as the client lives: in the cluster, a
/// whole session.
fn client(config: kube::Config, idle: std::time::Duration) -> Result<kube::Client, ConnectError> {
    use kube::client::ConfigExt as _;
    use kube::client::retry::RetryPolicy;
    let refused = |e: kube::Error| ConnectError::Client(e.to_string());
    let mut http = hyper_util::client::legacy::connect::HttpConnector::new();
    http.enforce_http(false);
    http.set_connect_timeout(config.connect_timeout);
    let https = config
        .rustls_https_connector_with_connector(http)
        .map_err(refused)?;
    let pool = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .pool_timer(hyper_util::rt::TokioTimer::new())
        .pool_idle_timeout(idle)
        .build(https);
    let service = tower::ServiceBuilder::new()
        .layer(config.base_uri_layer())
        .option_layer(
            config
                .default_retry
                .then(|| tower::retry::RetryLayer::new(RetryPolicy::server_retry())),
        )
        .option_layer(config.auth_layer().map_err(refused)?)
        .layer(config.extra_headers_layer().map_err(refused)?)
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let client = UserClient::with_token(&endpoint, &UserToken::new("t")).unwrap();
        assert_eq!(Cluster::kubeconfig_name(&client), None);
    }

    #[tokio::test]
    async fn an_idle_connection_is_closed_and_the_token_is_sent() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = socket.read(&mut buffer).await.unwrap();
                request.extend_from_slice(&buffer[..n]);
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}")
                .await
                .unwrap();
            // Kept alive until the client's pool lets it go.
            let closed = socket.read(&mut buffer).await.unwrap() == 0;
            (String::from_utf8(request).unwrap(), closed)
        });
        let mut config = kube::Config::new(format!("http://127.0.0.1:{port}").parse().unwrap());
        config.auth_info.token = Some(SecretString::from("alice-token"));
        let client = client(config, std::time::Duration::from_millis(100)).unwrap();
        let body = client
            .request_text(http::Request::get("/version").body(Vec::new()).unwrap())
            .await
            .unwrap();
        assert_eq!(body, "{}");
        let (request, closed) = tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .expect("the idle connection was not closed")
            .unwrap();
        assert!(closed);
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer alice-token")
        );
        drop(client);
    }

    #[test]
    fn tokens_do_not_leak_through_debug() {
        let token = UserToken::new("eyJhbGciOi.secret");
        assert!(!format!("{token:?}").contains("secret"));
    }
}
