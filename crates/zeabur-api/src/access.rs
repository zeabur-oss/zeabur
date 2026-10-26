//! How a handler gets a [`Cluster`] for the person making the request.
//!
//! The port returns boxed futures, so it is a trait object: handlers are not generic, and one
//! router serves local mode (one person, their kubeconfig), the in-cluster server (a client per
//! request, from the person's token) and tests (an in-memory cluster).
//!
//! There is one way to the cluster for a request, [`ClusterAccess::for_request`]. An admin
//! account acts with its full rights directly.

use std::sync::Arc;

use axum::http::request::Parts;
use zeabur_core::problem::Problem;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, UserToken};

/// A person's bearer token for kube-apiserver, as the session holds it now.
#[derive(Clone)]
pub struct Handoff {
    /// The token.
    pub token: UserToken,
    /// When it expires.
    pub expires_at: Timestamp,
}

impl std::fmt::Debug for Handoff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handoff")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// Gives each request the cluster connection of the person who made it.
pub trait ClusterAccess: Send + Sync + 'static {
    /// The connection for this request, or the problem that prevents one (not signed in, …).
    /// Every read, write and terminal goes through it.
    ///
    /// # Errors
    ///
    /// When the request carries no usable identity.
    fn for_request(&self, parts: &Parts) -> Result<Arc<dyn Cluster>, Box<Problem>>;

    /// The token of [`ClusterAccess::for_request`], to hand to a tools pod (see the `tools`
    /// module) or to Zeabur Proxy (see `services::logs`); `None` where Zeabur holds no token of
    /// its own to hand over (local mode, where the kubeconfig's credential may be anything, and
    /// tests).
    fn handoff(&self, _parts: &Parts) -> Option<Handoff> {
        None
    }

    /// Turns `true` when the person of [`ClusterAccess::for_request`] signs out; their open
    /// terminals close then. `None` where nobody signs out.
    fn signed_out(&self, _parts: &Parts) -> Option<tokio::sync::watch::Receiver<bool>> {
        None
    }

    /// Whether people sign in to this server (and can sign out), as opposed to local mode.
    fn signs_in(&self) -> bool {
        false
    }

    /// The origin people open this server at (`https://zeabur.example.com`), when it is
    /// configured; a WebSocket handshake must come from a page of it. `None` (local mode): the
    /// handshake's `Origin` is compared with its `Host`.
    fn public_origin(&self) -> Option<&str> {
        None
    }
}

/// One connection for every request: local mode, where the person is whoever started the
/// process (and the local guard already checked the request came from them), and tests.
#[derive(Clone)]
pub struct SingleCluster(pub Arc<dyn Cluster>);

impl std::fmt::Debug for SingleCluster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleCluster").finish_non_exhaustive()
    }
}

impl ClusterAccess for SingleCluster {
    fn for_request(&self, _parts: &Parts) -> Result<Arc<dyn Cluster>, Box<Problem>> {
        Ok(Arc::clone(&self.0))
    }
}
