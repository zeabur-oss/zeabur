//! One line per request: who, which namespace, which backend and path, allowed or why not, how
//! long and how many bytes. Never the token, the query or the data.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

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
    slot: Option<Slot>,
}

impl Audit {
    pub(crate) fn start(path: &str) -> Self {
        Self {
            started: Instant::now(),
            path: path.chars().take(256).collect(),
            backend: "",
            user: String::new(),
            namespace: String::new(),
            slot: None,
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

    /// Holds the person's place among the queries in flight until the answer ends.
    pub(crate) fn hold(&mut self, slot: Slot) {
        self.slot = Some(slot);
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

/// Queries in flight: a few per person and a ceiling for the proxy, so one person's wide
/// queries cannot hold the backend for everyone.
#[derive(Debug)]
pub(crate) struct Inflight {
    total: Arc<Semaphore>,
    per_person: usize,
    people: Mutex<HashMap<String, usize>>,
}

impl Inflight {
    pub(crate) fn new(total: usize, per_person: usize) -> Arc<Self> {
        Arc::new(Self {
            total: Arc::new(Semaphore::new(total)),
            per_person,
            people: Mutex::default(),
        })
    }

    pub(crate) fn enter(self: &Arc<Self>, person: &str) -> Result<Slot, Reject> {
        let permit = Arc::clone(&self.total)
            .try_acquire_owned()
            .map_err(|_full| Reject::Busy)?;
        let mut people = self.people.lock().unwrap_or_else(PoisonError::into_inner);
        let count = people.entry(person.to_owned()).or_default();
        if *count >= self.per_person {
            return Err(Reject::Busy);
        }
        *count += 1;
        drop(people);
        Ok(Slot {
            inflight: Arc::clone(self),
            person: person.to_owned(),
            _permit: permit,
        })
    }
}

/// A place among the queries in flight, given back when dropped.
#[derive(Debug)]
pub(crate) struct Slot {
    inflight: Arc<Inflight>,
    person: String,
    _permit: OwnedSemaphorePermit,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut people = self
            .inflight
            .people
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = people.get_mut(&self.person) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                people.remove(&self.person);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_person_gets_a_few_places_and_the_proxy_a_ceiling() {
        let inflight = Inflight::new(3, 2);
        let a1 = inflight.enter("a").unwrap();
        let _a2 = inflight.enter("a").unwrap();
        assert_eq!(inflight.enter("a").err(), Some(Reject::Busy));
        let _b1 = inflight.enter("b").unwrap();
        assert_eq!(inflight.enter("c").err(), Some(Reject::Busy));
        drop(a1);
        assert!(inflight.enter("a").is_ok());
    }
}
