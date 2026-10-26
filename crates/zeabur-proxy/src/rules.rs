//! The order every request goes through, enforced by types.
//!
//! A backend parses the request into a fixed query, kube-apiserver grants it ([`Grant`]), and
//! only the pair, an [`Authorized`] query, can be built into the request the backend gets.
//! Nothing here performs I/O.

use http::Method;
use k8s_openapi::jiff::Timestamp;

use crate::access::Grant;
use crate::namespace::Namespace;
use crate::reject::Reject;

/// A parsed query and the one namespace it reads.
pub trait Scoped {
    /// The namespace the query reads, the one the access review asks about.
    fn namespace(&self) -> &Namespace;
}

/// A backend the proxy serves.
pub trait Backend {
    /// The path prefix it is served under, such as `/logs/`.
    const PREFIX: &'static str;
    /// The resource in the API group `proxy.zeabur.com` a person needs `get` on.
    const RESOURCE: &'static str;
    /// A request, parsed.
    type Query: Scoped + Send;
    /// The bounds the backend's requests are held to.
    type Limits: Send + Sync;

    /// Parses a request: `path` is the whole path, `query` the raw query string.
    ///
    /// # Errors
    ///
    /// When anything in the request is not on the backend's lists.
    fn parse(method: &Method, path: &str, query: Option<&str>) -> Result<Self::Query, Reject>;

    /// The request the backend gets, made only from the parsed query and the grant.
    ///
    /// # Errors
    ///
    /// When the query falls outside what the grant and the limits allow.
    fn build(
        query: &Authorized<Self::Query>,
        limits: &Self::Limits,
        now: Timestamp,
    ) -> Result<Upstream, Reject>;
}

/// A query kube-apiserver allowed: the query and the grant for the same namespace and resource.
#[derive(Debug)]
pub struct Authorized<Q> {
    query: Q,
    grant: Grant,
}

impl<Q: Scoped> Authorized<Q> {
    /// Pairs `query` with `grant`.
    ///
    /// # Errors
    ///
    /// [`Reject::Forbidden`] when the grant is for another namespace or resource.
    pub fn new<B: Backend<Query = Q>>(query: Q, grant: Grant) -> Result<Self, Reject> {
        if grant.namespace() == query.namespace() && grant.resource() == B::RESOURCE {
            Ok(Self { query, grant })
        } else {
            Err(Reject::Forbidden)
        }
    }

    /// The query.
    pub const fn query(&self) -> &Q {
        &self.query
    }

    /// The grant.
    pub const fn grant(&self) -> &Grant {
        &self.grant
    }
}

/// The request a backend gets: a fixed path and the parameters the backend module wrote. No
/// header of the original request and no part of its query string is carried over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    path: String,
    params: Vec<(&'static str, String)>,
}

impl Upstream {
    pub(crate) fn new(path: impl Into<String>, params: Vec<(&'static str, String)>) -> Self {
        Self {
            path: path.into(),
            params,
        }
    }

    /// The backend's path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The parameters, in order.
    #[must_use]
    pub fn params(&self) -> &[(&'static str, String)] {
        &self.params
    }

    /// The path and the encoded query string.
    #[must_use]
    pub fn path_and_query(&self) -> String {
        let mut query = form_urlencoded::Serializer::new(String::new());
        for (key, value) in &self.params {
            query.append_pair(key, value);
        }
        format!("{}?{}", self.path, query.finish())
    }
}
