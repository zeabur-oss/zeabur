//! How a handler gets a [`Cluster`] for the person making the request.
//!
//! The port returns boxed futures, so it is a trait object: handlers are not generic, and one
//! router serves local mode (one person, their kubeconfig), the in-cluster server (a client per
//! request, from the person's token) and tests (an in-memory cluster).
//!
//! There is one way to the cluster for a request, [`ClusterAccess::for_request`], which also says
//! whether an administrator's credential is in use or offered ([`Elevation`]). Elevation works
//! like `sudo`: while the person holds the admin credential, everything goes with it, reads
//! included. The everyday credential stays reachable only for a plan's binding
//! ([`ClusterAccess::everyday`]).

use std::sync::Arc;

use axum::http::request::Parts;
use engawa_core::problem::Problem;
use engawa_kube::Cluster;
use engawa_kube::k8s_openapi::jiff::Timestamp;

/// Whether the person can act as an administrator for a while (the in-cluster server with
/// elevation turned on; see the `engawa` binary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elevation {
    /// Not offered here: local mode, or a server without elevation.
    Unavailable,
    /// Offered, not in use: a request the cluster refuses may be retried as an administrator.
    Available,
    /// In use until this time: every request already carries the administrator's credential.
    Active {
        /// When the elevated credential expires.
        until: Timestamp,
    },
}

/// The connection a request goes through, and whether it is elevated.
#[derive(Clone)]
pub struct Access {
    /// The connection: the administrator's while the person holds one, as with `sudo`.
    pub cluster: Arc<dyn Cluster>,
    /// Whether an administrator's credential is in use or could be.
    pub elevation: Elevation,
}

impl std::fmt::Debug for Access {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Access")
            .field("elevation", &self.elevation)
            .finish_non_exhaustive()
    }
}

/// Gives each request the cluster connection of the person who made it.
pub trait ClusterAccess: Send + Sync + 'static {
    /// The connection for this request and its elevation state, or the problem that prevents one
    /// (not signed in, …). Every read, write and terminal goes through it: while the person holds
    /// an administrator's credential it is that one, otherwise their everyday one.
    ///
    /// # Errors
    ///
    /// When the request carries no usable identity.
    fn for_request(&self, parts: &Parts) -> Result<Access, Box<Problem>>;

    /// The connection of the person's everyday credential, also while they are elevated. Only
    /// for what must not change with elevation: a plan token is bound to the identity it was
    /// made with, and binding it to the everyday one lets a plan made before elevating apply
    /// after, and one made while elevated apply once the rights have ended (see the `writes`
    /// module).
    ///
    /// # Errors
    ///
    /// When the request carries no usable identity.
    fn everyday(&self, parts: &Parts) -> Result<Arc<dyn Cluster>, Box<Problem>> {
        self.for_request(parts).map(|access| access.cluster)
    }

    /// When the credential of [`ClusterAccess::for_request`] stops being valid, if it does: the
    /// administrator's while the person is elevated. kube-apiserver checks a token when a stream
    /// opens, not while it stays open, so a terminal is closed at this time by Engawa itself.
    fn credential_expiry(&self, _parts: &Parts) -> Option<Timestamp> {
        None
    }

    /// Whether people sign in to this server (and can sign out), as opposed to local mode.
    fn signs_in(&self) -> bool {
        false
    }

    /// The origin people open this server at (`https://engawa.example.com`), when it is
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
    fn for_request(&self, _parts: &Parts) -> Result<Access, Box<Problem>> {
        Ok(Access {
            cluster: Arc::clone(&self.0),
            elevation: Elevation::Unavailable,
        })
    }
}
