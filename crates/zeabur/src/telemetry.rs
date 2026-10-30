//! The experience program (docs/reference/telemetry.mdx): one event the first time each person
//! uses Zeabur's web interface in a UTC day.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use zeabur_core::crypto;
use zeabur_telemetry::{Day, Event, InstallationId, Mode, Sender, Switch, ZeaburActive};

/// The people seen in the current UTC day, kept as hashes in memory only. A restart forgets
/// them.
#[derive(Debug)]
struct Daily {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    /// Random for this process and never sent, so a hash says nothing outside it.
    key: [u8; 32],
    day: Day,
    seen: HashSet<[u8; 32]>,
}

impl Daily {
    fn new(key: [u8; 32]) -> Self {
        Self {
            state: Mutex::new(State {
                key,
                day: Day::from_unix(0),
                seen: HashSet::new(),
            }),
        }
    }

    /// The day of `now` (Unix seconds) when it is the first time `person` is seen in it.
    fn first(&self, person: &str, now: i64) -> Option<Day> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let day = Day::from_unix(now);
        if day != state.day {
            state.day = day;
            state.seen.clear();
        }
        let mut input = state.key.to_vec();
        input.extend_from_slice(person.as_bytes());
        let hash = crypto::sha256(&input);
        state.seen.insert(hash).then_some(day)
    }
}

/// The program as the server runs it.
pub(crate) struct Telemetry {
    on: Option<(Daily, Arc<Sender>, InstallationId)>,
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
        let sender = Sender::new(switch, show);
        let on = id.filter(|_| sender.active()).and_then(|id| {
            Some((
                Daily::new(crypto::random::<32>().ok()?),
                Arc::new(sender),
                id,
            ))
        });
        Self { on }
    }

    #[cfg(test)]
    pub(crate) const fn off() -> Self {
        Self { on: None }
    }

    /// Notes that `person` used Zeabur at `now` (Unix seconds), and sends an event in the
    /// background the first time in a day.
    pub(crate) fn saw(&self, person: &str, now: i64) {
        let Some((daily, sender, id)) = &self.on else {
            return;
        };
        let Some(day) = daily.first(person, now) else {
            return;
        };
        let event = Event::ZeaburActive(ZeaburActive {
            zeabur_version: env!("CARGO_PKG_VERSION"),
            day,
            mode: Mode::InCluster,
        });
        let (sender, id) = (Arc::clone(sender), id.clone());
        drop(tokio::task::spawn_blocking(move || {
            sender.send(&id, &event);
        }));
    }
}

impl std::fmt::Debug for Telemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Telemetry")
            .field("on", &self.on.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_800_000_000;

    fn daily() -> Daily {
        Daily::new([7; 32])
    }

    #[test]
    fn a_person_is_reported_once_a_day() {
        let daily = daily();
        assert_eq!(daily.first("alice", NOW), Some(Day::from_unix(NOW)));
        assert_eq!(daily.first("alice", NOW + 5), None);
        assert_eq!(daily.first("bob", NOW + 10), Some(Day::from_unix(NOW)));
    }

    #[test]
    fn a_new_utc_day_starts_again() {
        let daily = daily();
        let midnight = NOW - NOW.rem_euclid(DAY) + DAY;
        assert!(daily.first("alice", midnight - 1).is_some());
        assert_eq!(
            daily.first("alice", midnight),
            Some(Day::from_unix(midnight))
        );
    }

    #[test]
    fn nothing_is_counted_when_the_program_is_off() {
        let id = "8f14e45f-ceea-467a-9575-0123456789ab";
        assert!(Telemetry::new(false, id).on.is_none());
        assert!(Telemetry::new(true, "not-a-uuid").on.is_none());
    }
}
