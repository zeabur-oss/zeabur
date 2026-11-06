//! One line per request: who, which namespace, which backend and path, allowed or why not, how
//! long and how many bytes. Never the token, the query or the data.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

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
/// queries cannot hold the backend for everyone. A query waits for a place up to `wait`, as one
/// page asks for several series at once.
#[derive(Debug)]
pub(crate) struct Inflight {
    total: Arc<Semaphore>,
    per_person: usize,
    wait: Duration,
    people: Mutex<HashMap<String, Arc<Semaphore>>>,
}

impl Inflight {
    pub(crate) fn new(total: usize, per_person: usize, wait: Duration) -> Arc<Self> {
        Arc::new(Self {
            total: Arc::new(Semaphore::new(total)),
            per_person,
            wait,
            people: Mutex::default(),
        })
    }

    pub(crate) async fn enter(self: &Arc<Self>, person: &str) -> Result<Slot, Reject> {
        let own = {
            let mut people = self.people.lock().unwrap_or_else(PoisonError::into_inner);
            let own = people
                .entry(person.to_owned())
                .or_insert_with(|| Arc::new(Semaphore::new(self.per_person)));
            Arc::clone(own)
        };
        let places = async {
            let mine = Arc::clone(&own).acquire_owned().await.ok()?;
            let total = Arc::clone(&self.total).acquire_owned().await.ok()?;
            Some((mine, total))
        };
        let Ok(Some((mine, total))) = tokio::time::timeout(self.wait, places).await else {
            self.leave(person, own);
            return Err(Reject::Busy);
        };
        Ok(Slot {
            inflight: Arc::clone(self),
            person: person.to_owned(),
            own: Some(own),
            mine: Some(mine),
            _total: total,
        })
    }

    /// Forgets `person` once nobody holds or waits for one of their places. Every holder and
    /// waiter has a clone of `own`, taken and given back under the lock, so the count is exact
    /// here.
    fn leave(&self, person: &str, own: Arc<Semaphore>) {
        let mut people = self.people.lock().unwrap_or_else(PoisonError::into_inner);
        let last = Arc::strong_count(&own) == 2;
        drop(own);
        if last {
            people.remove(person);
        }
    }
}

/// A place among the queries in flight, given back when dropped.
#[derive(Debug)]
pub(crate) struct Slot {
    inflight: Arc<Inflight>,
    person: String,
    own: Option<Arc<Semaphore>>,
    mine: Option<OwnedSemaphorePermit>,
    _total: OwnedSemaphorePermit,
}

impl Drop for Slot {
    fn drop(&mut self) {
        // The permit holds a clone of the semaphore too, so it goes first.
        drop(self.mine.take());
        if let Some(own) = self.own.take() {
            self.inflight.leave(&self.person, own);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WAIT: Duration = Duration::from_millis(50);

    #[tokio::test]
    async fn a_person_waits_for_a_place_and_the_proxy_has_a_ceiling() {
        let inflight = Inflight::new(3, 2, WAIT);
        let a1 = inflight.enter("a").await.unwrap();
        let _a2 = inflight.enter("a").await.unwrap();
        assert_eq!(inflight.enter("a").await.err(), Some(Reject::Busy));
        let _b1 = inflight.enter("b").await.unwrap();
        assert_eq!(inflight.enter("c").await.err(), Some(Reject::Busy));
        drop(a1);
        assert!(inflight.enter("a").await.is_ok());
    }

    // The usage history asks for CPU, memory and restarts at once (zeabur-api's
    // `services::history::read`); the third waits for the first to end instead of failing.
    #[tokio::test]
    async fn a_third_query_gets_the_place_the_first_gives_back() {
        let inflight = Inflight::new(16, 2, Duration::from_secs(5));
        let first = inflight.enter("a").await.unwrap();
        let _second = inflight.enter("a").await.unwrap();
        let third = tokio::spawn({
            let inflight = Arc::clone(&inflight);
            async move { inflight.enter("a").await.map(drop) }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        drop(first);
        assert_eq!(third.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn a_person_is_forgotten_once_their_places_are_back() {
        let inflight = Inflight::new(16, 2, WAIT);
        let held = inflight.enter("a").await.unwrap();
        assert!(inflight.people.lock().unwrap().contains_key("a"));
        drop(held);
        assert!(inflight.people.lock().unwrap().is_empty());
    }

    #[test]
    fn a_person_is_forgotten_when_their_last_queries_end_together() {
        const AT_ONCE: usize = 8;
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let inflight = Inflight::new(AT_ONCE, AT_ONCE, WAIT);
        for _ in 0..500 {
            let start = Arc::new(std::sync::Barrier::new(AT_ONCE));
            let mut ends = Vec::new();
            for _ in 0..AT_ONCE {
                let slot = runtime.block_on(inflight.enter("a")).unwrap();
                let start = Arc::clone(&start);
                ends.push(std::thread::spawn(move || {
                    start.wait();
                    drop(slot);
                }));
            }
            for end in ends {
                end.join().unwrap();
            }
            assert!(inflight.people.lock().unwrap().is_empty());
        }
    }
}
