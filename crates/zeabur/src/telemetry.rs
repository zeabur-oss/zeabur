//! The experience program (docs/reference/telemetry.mdx): one event the first time each person
//! uses Zeabur's web interface in a UTC day, and one the first time each feature with its values
//! is used.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use zeabur_core::crypto;
use zeabur_telemetry::{
    Day, Event, Feature, FeatureUsed, InstallationId, Mode, Sender, Switch, ZeaburActive,
};

/// The people seen in the current UTC day, in memory only. A restart forgets them, but the
/// hash sent for a person stays the same all day, so they are counted once.
#[derive(Debug)]
struct Daily {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    /// Derived from the plan key, so it survives restarts; never sent.
    key: [u8; 32],
    day: Day,
    seen: HashSet<[u8; 16]>,
    features: HashSet<Feature>,
}

impl Daily {
    fn new(key: [u8; 32]) -> Self {
        Self {
            state: Mutex::new(State {
                key,
                day: Day::from_unix(0),
                seen: HashSet::new(),
                features: HashSet::new(),
            }),
        }
    }

    /// The day of `now` (Unix seconds) and `person`'s hash for it, when it is the first time
    /// `person` is seen in that day.
    fn first(&self, person: &str, now: i64) -> Option<(Day, [u8; 16])> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let day = state.today(now);
        let mut input = day.since_epoch().to_be_bytes().to_vec();
        input.extend_from_slice(person.as_bytes());
        let mut hash = [0_u8; 16];
        hash.copy_from_slice(&crypto::hmac_sha256(&state.key, &input)[..16]);
        state.seen.insert(hash).then_some((day, hash))
    }

    /// The day of `now`, when it is the first time `feature` is used in that day.
    fn first_use(&self, feature: Feature, now: i64) -> Option<Day> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let day = state.today(now);
        state.features.insert(feature).then_some(day)
    }
}

impl State {
    /// The day of `now`; a new day forgets what was seen in the last.
    fn today(&mut self, now: i64) -> Day {
        let day = Day::from_unix(now);
        if day != self.day {
            self.day = day;
            self.seen.clear();
            self.features.clear();
        }
        day
    }
}

/// The program as the server runs it.
pub(crate) struct Telemetry {
    on: Option<(Daily, Arc<Sender>, InstallationId)>,
}

impl Telemetry {
    /// Off when not joined, when `DO_NOT_TRACK` is set, or when there is no valid ID.
    pub(crate) fn new(enabled: bool, installation_id: &str, key: [u8; 32]) -> Self {
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
        let on = id
            .filter(|_| sender.active())
            .map(|id| (Daily::new(key), Arc::new(sender), id));
        Self { on }
    }

    /// Whether anything is sent.
    pub(crate) const fn is_on(&self) -> bool {
        self.on.is_some()
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
        let Some((day, hash)) = daily.first(person, now) else {
            return;
        };
        let event = Event::ZeaburActive(ZeaburActive {
            zeabur_version: env!("CARGO_PKG_VERSION"),
            day,
            person: crypto::hex(&hash),
            mode: Mode::InCluster,
        });
        let (sender, id) = (Arc::clone(sender), id.clone());
        drop(tokio::task::spawn_blocking(move || {
            sender.send(&id, &event);
        }));
    }

    /// Sends `feature` in the background the first time in a UTC day that it is used, with
    /// these values. Called after the API call that used it succeeded.
    pub(crate) fn used(&self, feature: Feature, now: i64) {
        let Some((daily, sender, id)) = &self.on else {
            return;
        };
        let Some(day) = daily.first_use(feature, now) else {
            return;
        };
        let event = Event::FeatureUsed(FeatureUsed {
            zeabur_version: env!("CARGO_PKG_VERSION"),
            day,
            feature,
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
    use zeabur_telemetry::Source;

    use super::*;

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_800_000_000;

    fn daily() -> Daily {
        Daily::new([7; 32])
    }

    #[test]
    fn a_person_is_reported_once_a_day() {
        let daily = daily();
        let alice = daily.first("alice", NOW).unwrap();
        assert_eq!(alice.0, Day::from_unix(NOW));
        assert_eq!(daily.first("alice", NOW + 5), None);
        let bob = daily.first("bob", NOW + 10).unwrap();
        assert_ne!(alice.1, bob.1);
    }

    #[test]
    fn a_restart_sends_the_same_hash_that_day() {
        let before = daily().first("alice", NOW).unwrap();
        let after = daily().first("alice", NOW + 60).unwrap();
        assert_eq!(before, after);
        assert_ne!(Daily::new([8; 32]).first("alice", NOW).unwrap().1, before.1);
    }

    #[test]
    fn a_new_utc_day_starts_again_with_another_hash() {
        let daily = daily();
        let midnight = NOW - NOW.rem_euclid(DAY) + DAY;
        let late = daily.first("alice", midnight - 1).unwrap();
        let next = daily.first("alice", midnight).unwrap();
        assert_eq!(next.0, Day::from_unix(midnight));
        assert_ne!(late.1, next.1);
    }

    #[test]
    fn a_feature_is_reported_once_a_day_for_each_of_its_values() {
        let daily = daily();
        let wordpress = Feature::ServiceCreated(Source::Template("wordpress"));
        let ghost = Feature::ServiceCreated(Source::Template("ghost"));
        assert_eq!(daily.first_use(wordpress, NOW), Some(Day::from_unix(NOW)));
        assert_eq!(daily.first_use(wordpress, NOW + 5), None);
        assert!(daily.first_use(ghost, NOW + 6).is_some());
        let tomorrow = NOW - NOW.rem_euclid(DAY) + DAY;
        assert_eq!(
            daily.first_use(wordpress, tomorrow),
            Some(Day::from_unix(tomorrow))
        );
    }

    #[test]
    fn nothing_is_counted_when_the_program_is_off() {
        let id = "8f14e45f-ceea-467a-9575-0123456789ab";
        assert!(Telemetry::new(false, id, [7; 32]).on.is_none());
        assert!(Telemetry::new(true, "not-a-uuid", [7; 32]).on.is_none());
    }
}
