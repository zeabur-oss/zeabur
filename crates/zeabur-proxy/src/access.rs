//! Whether the person may read: a `SelfSubjectAccessReview` and the namespace, both asked of
//! kube-apiserver with the person's own token.
//!
//! The proxy holds no credential of its own: its pod mounts no service account token, so it can
//! only ask as the person. A `SelfSubjectAccessReview` answers for whoever sends it, and every
//! authenticated user may create one (`system:basic-user`)
//! (<https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access>).
//! The resource is a virtual one in the group `proxy.zeabur.com`, granted with ordinary RBAC, as
//! OpenShift's LokiStack does with `loki.grafana.com`.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use k8s_openapi::api::authentication::v1::SelfSubjectReview;
use k8s_openapi::api::authorization::v1::{
    ResourceAttributes, SelfSubjectAccessReview, SelfSubjectAccessReviewSpec,
};
use k8s_openapi::api::core::v1::Namespace as KubeNamespace;
use k8s_openapi::jiff::Timestamp;
use kube::api::{Api, PostParams};
use secrecy::{ExposeSecret as _, SecretString};
use sha2::{Digest as _, Sha256};

use crate::namespace::Namespace;
use crate::reject::Reject;

/// The API group of the proxy's virtual resources.
pub const GROUP: &str = "proxy.zeabur.com";

/// The bearer token a request carried. Never logged.
#[derive(Clone)]
pub struct Token(SecretString);

impl Token {
    /// Wraps a token.
    #[must_use]
    pub fn new(token: &str) -> Self {
        Self(SecretString::from(token))
    }

    fn digest(&self, namespace: &Namespace, resource: &str) -> [u8; 32] {
        let mut hash = Sha256::new();
        for part in [self.0.expose_secret(), namespace.as_str(), resource] {
            hash.update(part.as_bytes());
            hash.update([0]);
        }
        hash.finalize().into()
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(…)")
    }
}

/// kube-apiserver's yes: this person may `get` this resource in this namespace, which was created
/// at this time. Only [`KubeReviewer`] makes one, after asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    user: String,
    namespace: Namespace,
    resource: &'static str,
    created: Timestamp,
}

impl Grant {
    /// Who asked, as kube-apiserver named them.
    #[must_use]
    pub fn user(&self) -> &str {
        &self.user
    }

    /// The namespace.
    #[must_use]
    pub const fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    /// The resource in [`GROUP`].
    #[must_use]
    pub const fn resource(&self) -> &'static str {
        self.resource
    }

    /// When the namespace was created.
    #[must_use]
    pub const fn created(&self) -> Timestamp {
        self.created
    }

    #[cfg(test)]
    pub(crate) fn for_tests(namespace: &str, resource: &'static str, created: Timestamp) -> Self {
        Self {
            user: "tester".to_owned(),
            namespace: Namespace::parse(namespace).unwrap(),
            resource,
            created,
        }
    }
}

/// What the access review said, before anything is read.
pub trait Reviewer: Send + Sync + 'static {
    /// Asks whether the holder of `token` may `get` `resource` in `namespace`.
    fn review(
        &self,
        token: &Token,
        namespace: &Namespace,
        resource: &'static str,
    ) -> impl Future<Output = Result<Grant, Reject>> + Send;
}

type Pool = hyper_util::client::legacy::Client<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    kube::client::Body,
>;

/// The access review against kube-apiserver, with a short cache.
#[derive(Clone)]
pub struct KubeReviewer {
    config: kube::Config,
    pool: Pool,
    timeout: Duration,
    cache: Arc<Cache>,
}

impl fmt::Debug for KubeReviewer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KubeReviewer")
            .field("url", &self.config.cluster_url)
            .finish_non_exhaustive()
    }
}

impl KubeReviewer {
    /// Asks the API server of the cluster this pod runs in, trusting its CA bundle. Whatever
    /// credential the in-cluster configuration names is dropped: each request carries the
    /// person's token alone.
    ///
    /// # Errors
    ///
    /// When the pod has no CA bundle or the environment lacks the API server's address.
    pub fn in_cluster(timeout: Duration, ttl: Duration) -> Result<Self, String> {
        let found = kube::Config::incluster().map_err(|e| e.to_string())?;
        let mut config = kube::Config::new(found.cluster_url);
        config.root_cert = found.root_cert;
        config.default_namespace = found.default_namespace;
        config.read_timeout = Some(timeout);
        Self::new(config, timeout, ttl)
    }

    /// Asks the API server `config` names, with none of its credentials.
    ///
    /// # Errors
    ///
    /// When the TLS client cannot be built.
    pub fn new(mut config: kube::Config, timeout: Duration, ttl: Duration) -> Result<Self, String> {
        use kube::client::ConfigExt as _;
        config.auth_info = kube::config::AuthInfo::default();
        config.headers.clear();
        let mut http = hyper_util::client::legacy::connect::HttpConnector::new();
        http.enforce_http(false);
        http.set_connect_timeout(Some(timeout));
        let https = config
            .rustls_https_connector_with_connector(http)
            .map_err(|e| e.to_string())?;
        let pool =
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .pool_timer(hyper_util::rt::TokioTimer::new())
                .pool_idle_timeout(Duration::from_secs(30))
                .build(https);
        Ok(Self {
            config,
            pool,
            timeout,
            cache: Arc::new(Cache::new(ttl)),
        })
    }

    /// A kube-rs client over the shared pool that sends `token` and nothing else.
    fn client(&self, token: &Token) -> Result<kube::Client, Reject> {
        use kube::client::ConfigExt as _;
        let mut config = self.config.clone();
        config.auth_info.token = Some(token.0.clone());
        let auth = config.auth_layer().map_err(|_failed| Reject::Unavailable)?;
        let service = tower::ServiceBuilder::new()
            .layer(config.base_uri_layer())
            .option_layer(auth)
            .map_err(tower::BoxError::from)
            .service(self.pool.clone());
        Ok(kube::Client::new(service, config.default_namespace))
    }

    async fn ask(
        &self,
        token: &Token,
        namespace: &Namespace,
        resource: &'static str,
    ) -> Result<Grant, Reject> {
        let client = self.client(token)?;
        let review = SelfSubjectAccessReview {
            spec: SelfSubjectAccessReviewSpec {
                resource_attributes: Some(ResourceAttributes {
                    group: Some(GROUP.to_owned()),
                    resource: Some(resource.to_owned()),
                    verb: Some("get".to_owned()),
                    namespace: Some(namespace.as_str().to_owned()),
                    ..ResourceAttributes::default()
                }),
                ..SelfSubjectAccessReviewSpec::default()
            },
            ..SelfSubjectAccessReview::default()
        };
        let reviews: Api<SelfSubjectAccessReview> = Api::all(client.clone());
        let selves: Api<SelfSubjectReview> = Api::all(client.clone());
        let post = PostParams::default();
        let me = SelfSubjectReview::default();
        let (answer, who) = tokio::join!(reviews.create(&post, &review), selves.create(&post, &me));
        let allowed = answer
            .map_err(review_error)?
            .status
            .is_some_and(|status| status.allowed && status.denied != Some(true));
        if !allowed {
            return Err(Reject::Forbidden);
        }
        let user = who
            .ok()
            .and_then(|who| who.status?.user_info?.username)
            .unwrap_or_default();
        let namespaces: Api<KubeNamespace> = Api::all(client);
        let created = namespaces
            .get(namespace.as_str())
            .await
            .map_err(read_error)?
            .metadata
            .creation_timestamp
            .ok_or(Reject::Forbidden)?
            .0;
        Ok(Grant {
            user,
            namespace: namespace.clone(),
            resource,
            created,
        })
    }
}

/// A refused token is the person's to fix; anything else from kube-apiserver means it could not
/// decide, and nothing is let through.
fn review_error(error: kube::Error) -> Reject {
    match error {
        kube::Error::Api(status) if status.code == 401 => Reject::Unauthenticated,
        kube::Error::Api(status) if status.code == 403 => Reject::Forbidden,
        kube::Error::Api(_)
        | kube::Error::HyperError(_)
        | kube::Error::Service(_)
        | kube::Error::ProxyProtocolUnsupported { .. }
        | kube::Error::ProxyProtocolDisabled { .. }
        | kube::Error::FromUtf8(_)
        | kube::Error::LinesCodecMaxLineLengthExceeded
        | kube::Error::ReadEvents(_)
        | kube::Error::HttpError(_)
        | kube::Error::SerdeError(_)
        | kube::Error::BuildRequest(_)
        | kube::Error::InferConfig(_)
        | kube::Error::InferKubeconfig(_)
        | kube::Error::Discovery(_)
        | kube::Error::RustlsTls(_)
        | kube::Error::TlsRequired
        | kube::Error::UpgradeConnection(_)
        | kube::Error::Auth(_) => Reject::Unavailable,
    }
}

/// The namespace could not be read: one the person may not read, or that is gone, is refused.
fn read_error(error: kube::Error) -> Reject {
    match error {
        kube::Error::Api(status) if matches!(status.code, 403 | 404) => Reject::Forbidden,
        other @ (kube::Error::Api(_)
        | kube::Error::HyperError(_)
        | kube::Error::Service(_)
        | kube::Error::ProxyProtocolUnsupported { .. }
        | kube::Error::ProxyProtocolDisabled { .. }
        | kube::Error::FromUtf8(_)
        | kube::Error::LinesCodecMaxLineLengthExceeded
        | kube::Error::ReadEvents(_)
        | kube::Error::HttpError(_)
        | kube::Error::SerdeError(_)
        | kube::Error::BuildRequest(_)
        | kube::Error::InferConfig(_)
        | kube::Error::InferKubeconfig(_)
        | kube::Error::Discovery(_)
        | kube::Error::RustlsTls(_)
        | kube::Error::TlsRequired
        | kube::Error::UpgradeConnection(_)
        | kube::Error::Auth(_)) => review_error(other),
    }
}

impl Reviewer for KubeReviewer {
    async fn review(
        &self,
        token: &Token,
        namespace: &Namespace,
        resource: &'static str,
    ) -> Result<Grant, Reject> {
        let key = token.digest(namespace, resource);
        if let Some(answer) = self.cache.get(&key) {
            return answer;
        }
        let answer = tokio::time::timeout(self.timeout, self.ask(token, namespace, resource))
            .await
            .unwrap_or(Err(Reject::Unavailable));
        self.cache.put(key, &answer);
        answer
    }
}

/// Answers by a hash of the token, the namespace and the resource, for a few seconds: someone
/// removed from a project keeps reading for at most that long.
#[derive(Debug)]
struct Cache {
    ttl: Duration,
    entries: Mutex<HashMap<[u8; 32], Entry>>,
}

/// When it was asked, and the answer.
type Entry = (Instant, Result<Grant, Reject>);

impl Cache {
    /// Entries kept at most; past it the expired ones go, then all.
    const MAX: usize = 4096;

    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::default(),
        }
    }

    fn get(&self, key: &[u8; 32]) -> Option<Result<Grant, Reject>> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries
            .get(key)
            .filter(|(at, _)| at.elapsed() < self.ttl)
            .map(|(_, answer)| answer.clone())
    }

    /// Keeps a grant or a refusal; never a failure to ask, which is asked again.
    fn put(&self, key: [u8; 32], answer: &Result<Grant, Reject>) {
        let keep = match answer {
            Ok(_) | Err(Reject::Forbidden) => true,
            Err(
                Reject::NoToken
                | Reject::Unauthenticated
                | Reject::UnknownBackend
                | Reject::Method
                | Reject::Path
                | Reject::UnknownParameter
                | Reject::Repeated
                | Reject::MissingNamespace
                | Reject::InvalidNamespace
                | Reject::InvalidTime
                | Reject::InvalidLimit
                | Reject::InvalidQuery
                | Reject::BeforeNamespace
                | Reject::Busy
                | Reject::Unavailable
                | Reject::Backend,
            ) => false,
        };
        if self.ttl.is_zero() || !keep {
            return;
        }
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.len() >= Self::MAX {
            entries.retain(|_, (at, _)| at.elapsed() < self.ttl);
            if entries.len() >= Self::MAX {
                entries.clear();
            }
        }
        entries.insert(key, (Instant::now(), answer.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_keeps_grants_and_refusals_but_asks_again_after_a_failure() {
        let cache = Cache::new(Duration::from_secs(10));
        let token = Token::new("t");
        let a = Namespace::parse("a").unwrap();
        let b = Namespace::parse("b").unwrap();
        let grant = Grant::for_tests("a", "logs", Timestamp::UNIX_EPOCH);
        cache.put(token.digest(&a, "logs"), &Ok(grant.clone()));
        cache.put(token.digest(&b, "logs"), &Err(Reject::Unavailable));
        assert_eq!(cache.get(&token.digest(&a, "logs")), Some(Ok(grant)));
        assert_eq!(cache.get(&token.digest(&b, "logs")), None);
        assert_eq!(cache.get(&token.digest(&a, "metrics")), None);
        assert_eq!(cache.get(&Token::new("u").digest(&a, "logs")), None);
        cache.put(token.digest(&b, "logs"), &Err(Reject::Forbidden));
        assert_eq!(
            cache.get(&token.digest(&b, "logs")),
            Some(Err(Reject::Forbidden))
        );
    }

    #[test]
    fn nothing_is_kept_with_no_ttl() {
        let cache = Cache::new(Duration::ZERO);
        let token = Token::new("t");
        let a = Namespace::parse("a").unwrap();
        cache.put(token.digest(&a, "logs"), &Err(Reject::Forbidden));
        assert_eq!(cache.get(&token.digest(&a, "logs")), None);
    }

    #[test]
    fn a_token_is_never_printed() {
        assert_eq!(format!("{:?}", Token::new("secret")), "Token(…)");
    }
}
