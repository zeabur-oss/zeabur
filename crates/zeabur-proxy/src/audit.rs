//! One line per request: who, which namespace, which backend and path, allowed or why not, how
//! long and how many bytes. Never the token, the query or the data.

use std::time::Instant;

use crate::reject::Reject;

/// How a streamed answer ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Complete,
    TooLarge,
    TimedOut,
    BackendFailed,
    /// The client went away first.
    Abandoned,
}

impl Outcome {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::TooLarge => "too_large",
            Self::TimedOut => "timed_out",
            Self::BackendFailed => "backend_failed",
            Self::Abandoned => "abandoned",
        }
    }
}

/// A request's audit record, written once when it is decided or its answer ends.
#[derive(Debug)]
pub(crate) struct Audit {
    started: Instant,
    path: String,
    backend: &'static str,
    user: String,
    namespace: String,
}

impl Audit {
    pub(crate) fn start(path: &str) -> Self {
        Self {
            started: Instant::now(),
            path: path.chars().take(256).collect(),
            backend: "",
            user: String::new(),
            namespace: String::new(),
        }
    }

    pub(crate) const fn backend(&mut self, backend: &'static str) {
        self.backend = backend;
    }

    pub(crate) fn namespace(&mut self, namespace: &str) {
        namespace.clone_into(&mut self.namespace);
    }

    pub(crate) fn user(&mut self, user: &str) {
        user.clone_into(&mut self.user);
    }

    fn millis(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    pub(crate) fn refused(self, reject: Reject) {
        tracing::info!(
            target: "audit",
            user = ?self.user,
            namespace = ?self.namespace,
            backend = self.backend,
            path = ?self.path,
            decision = "deny",
            reason = reject.code(),
            status = reject.status().as_u16(),
            duration_ms = self.millis(),
            bytes = 0_u64,
        );
    }

    /// The backend refused the query itself.
    pub(crate) fn query_refused(self) {
        tracing::info!(
            target: "audit",
            user = ?self.user,
            namespace = ?self.namespace,
            backend = self.backend,
            path = ?self.path,
            decision = "allow",
            outcome = "query_refused",
            status = 400_u16,
            duration_ms = self.millis(),
            bytes = 0_u64,
        );
    }

    pub(crate) fn streamed(self, outcome: Outcome, bytes: u64) {
        tracing::info!(
            target: "audit",
            user = ?self.user,
            namespace = ?self.namespace,
            backend = self.backend,
            path = ?self.path,
            decision = "allow",
            outcome = outcome.as_str(),
            status = 200_u16,
            duration_ms = self.millis(),
            bytes,
        );
    }
}
