//! The experience program (docs/reference/telemetry.mdx): counts the people who used the API
//! today, and about once an hour reports the count.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use zeabur_auth::session::Clock;
use zeabur_core::crypto;
use zeabur_telemetry::{Day, Event, InstallationId, Mode, Sender, Switch, ZeaburDay};

/// Until the first report, then between reports.
const FIRST: Duration = Duration::from_mins(1);
const EVERY: Duration = Duration::from_hours(1);

/// The people who used the API in the current UTC day, kept as hashes in memory only. A
/// restart forgets them.
#[derive(Debug)]
pub(crate) struct Daily {
    state: Option<Mutex<State>>,
}

#[derive(Debug)]
struct State {
    /// Random for this process and never sent, so a hash says nothing outside it.
    key: [u8; 32],
    day: Day,
    seen: HashSet<[u8; 32]>,
}

impl Daily {
    /// Counts when `on`; otherwise [`record`](Self::record) does nothing.
    pub(crate) fn new(on: bool) -> Self {
        let state = on
            .then(|| crypto::random::<32>().ok())
            .flatten()
            .map(|key| {
                Mutex::new(State {
                    key,
                    day: Day::from_unix(0),
                    seen: HashSet::new(),
                })
            });
        Self { state }
    }

    /// Notes that `person` used the API at `now` (Unix seconds).
    pub(crate) fn record(&self, person: &str, now: i64) {
        let Some(state) = &self.state else { return };
        let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
        state.roll(now);
        let mut input = state.key.to_vec();
        input.extend_from_slice(person.as_bytes());
        state.seen.insert(crypto::sha256(&input));
    }

    /// The count for the day of `now`, or `None` when nobody used the API that day.
    pub(crate) fn today(&self, now: i64) -> Option<ZeaburDay> {
        let mut state = self
            .state
            .as_ref()?
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.roll(now);
        let people = u32::try_from(state.seen.len()).unwrap_or(u32::MAX);
        (people > 0).then_some(ZeaburDay {
            zeabur_version: env!("CARGO_PKG_VERSION"),
            day: state.day,
            people,
            mode: Mode::InCluster,
        })
    }
}

impl State {
    /// Starts a new set when `now` is in another day.
    fn roll(&mut self, now: i64) {
        let day = Day::from_unix(now);
        if day != self.day {
            self.day = day;
            self.seen.clear();
        }
    }
}

/// The program as the server runs it.
pub(crate) struct Telemetry {
    pub(crate) daily: Arc<Daily>,
    sender: Arc<Sender>,
    id: Option<InstallationId>,
}

impl Telemetry {
    /// Off when not joined, when `DO_NOT_TRACK` is set, or when there is no valid ID.
    pub(crate) fn new(enabled: bool, installation_id: &str) -> Self {
        let id = InstallationId::parse(installation_id);
        if enabled && id.is_none() && !installation_id.is_empty() {
            tracing::warn!(
                event = "telemetry.id",
                "--installation-id is not a UUID; the experience program is off"
            );
        }
        let switch = if id.is_some() {
            Switch::from_env(enabled)
        } else {
            Switch::Off
        };
        let show: zeabur_telemetry::Show =
            Arc::new(|body| tracing::info!(event = "telemetry.debug", %body, "would send"));
        Self {
            daily: Arc::new(Daily::new(switch != Switch::Off)),
            sender: Arc::new(Sender::new(switch, show)),
            id,
        }
    }

    /// Reports the day's count in the background, if the program is on.
    pub(crate) fn spawn(self, clock: Clock) {
        let Some(id) = self.id.filter(|_| self.sender.active()) else {
            return;
        };
        tokio::spawn(async move {
            let mut every = tokio::time::interval_at(tokio::time::Instant::now() + FIRST, EVERY);
            loop {
                every.tick().await;
                let Some(day) = self.daily.today(clock()) else {
                    continue;
                };
                let (sender, id) = (Arc::clone(&self.sender), id.clone());
                drop(
                    tokio::task::spawn_blocking(move || sender.send(&id, &Event::ZeaburDay(day)))
                        .await,
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_800_000_000;

    #[test]
    fn one_person_counts_once_however_often() {
        let daily = Daily::new(true);
        for offset in 0..5 {
            daily.record("alice", NOW + offset);
        }
        daily.record("bob", NOW + 10);
        assert_eq!(daily.today(NOW + 20).unwrap().people, 2);
    }

    #[test]
    fn a_new_utc_day_starts_again() {
        let daily = Daily::new(true);
        let midnight = NOW - NOW.rem_euclid(DAY) + DAY;
        daily.record("alice", midnight - 1);
        assert_eq!(daily.today(midnight - 1).unwrap().people, 1);
        assert!(daily.today(midnight).is_none());
        daily.record("alice", midnight);
        daily.record("bob", midnight + 5);
        assert_eq!(daily.today(midnight + 6).unwrap().people, 2);
    }

    #[test]
    fn nothing_is_counted_when_the_program_is_off() {
        let daily = Daily::new(false);
        daily.record("alice", NOW);
        assert!(daily.today(NOW).is_none());
    }
}
