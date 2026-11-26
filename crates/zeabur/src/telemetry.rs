//! The experience program (docs/reference/telemetry.mdx): one event each time someone signs in
//! to Zeabur or saves changes, and one the first time each feature with its values is used in a
//! UTC day.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use zeabur_api::measure::Report;
use zeabur_core::crypto;
use zeabur_telemetry::{
    AppChangeSubmitted, Day, Event, Feature, FeatureUsed, Id, InstallationId, Mode, Sender,
    SignedIn, Switch,
};

/// `person`'s hash for the UTC day of `now` (Unix seconds) under `key`, which is derived from
/// the plan key and never sent. A restart keeps it; the next day changes it.
fn person(key: &[u8; 32], person: &str, now: i64) -> String {
    let mut input = Day::from_unix(now).since_epoch().to_be_bytes().to_vec();
    input.extend_from_slice(person.as_bytes());
    crypto::hex(&crypto::hmac_sha256(key, &input)[..16])
}

/// The features used in the current UTC day, in memory only. A restart forgets them.
#[derive(Debug, Default)]
struct Used {
    day: Option<Day>,
    features: HashSet<Feature>,
}

impl Used {
    /// The day of `now`, when it is the first time `feature` is used in that day.
    fn first(&mut self, feature: &Feature, now: i64) -> Option<Day> {
        let day = Day::from_unix(now);
        if self.day != Some(day) {
            self.day = Some(day);
            self.features.clear();
        }
        self.features.insert(*feature).then_some(day)
    }
}

/// The program as the server runs it.
pub(crate) struct Telemetry {
    on: Option<([u8; 32], Sender)>,
    used: Mutex<Used>,
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
        let on = id
            .map(|id| Sender::new(switch, Id::Installation(id), show))
            .filter(Sender::active)
            .map(|sender| (key, sender));
        Self {
            on,
            used: Mutex::default(),
        }
    }

    /// Whether anything is sent.
    pub(crate) const fn is_on(&self) -> bool {
        self.on.is_some()
    }

    #[cfg(test)]
    pub(crate) fn off() -> Self {
        Self {
            on: None,
            used: Mutex::default(),
        }
    }

    /// A program that hands each event's body to `show` instead of sending it.
    #[cfg(test)]
    pub(crate) fn debug(show: zeabur_telemetry::Show) -> Self {
        let id = Id::Installation(InstallationId::from_random([7; 16]));
        let sender = Sender::new(Switch::Debug, id, show);
        Self {
            on: Some(([7; 32], sender)),
            used: Mutex::default(),
        }
    }

    /// Waits up to `wait` for the queued events to be sent.
    pub(crate) async fn close(&self, wait: Duration) {
        if let Some((_, sender)) = &self.on {
            sender.close(wait).await;
        }
    }

    /// Sends in the background that `name` signed in at `now` (Unix seconds).
    pub(crate) fn signed_in(&self, name: &str, now: i64) {
        let Some((key, sender)) = &self.on else {
            return;
        };
        sender.send(Event::SignedIn(SignedIn {
            zeabur_version: env!("CARGO_PKG_VERSION"),
            person: person(key, name, now),
            mode: Mode::InCluster,
        }));
    }

    /// Sends `feature` in the background the first time it is used with these values in the
    /// UTC day of `now`. Called after the API call that used it succeeded.
    pub(crate) fn used(&self, feature: Feature, now: i64) {
        let Some((_, sender)) = &self.on else {
            return;
        };
        let first = self
            .used
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .first(&feature, now);
        let Some(day) = first else { return };
        let event = Event::FeatureUsed(FeatureUsed {
            zeabur_version: env!("CARGO_PKG_VERSION"),
            day,
            mode: Mode::InCluster,
            client: None,
            feature,
        });
        if !sender.send(event) {
            let mut used = self.used.lock().unwrap_or_else(PoisonError::into_inner);
            used.features.remove(&feature);
        }
    }

    /// Sends the answer to a save in the background.
    pub(crate) fn changed(&self, report: &Report) {
        let Some((key, sender)) = &self.on else {
            return;
        };
        sender.send(Event::AppChangeSubmitted(AppChangeSubmitted {
            zeabur_version: env!("CARGO_PKG_VERSION"),
            mode: Mode::InCluster,
            person: person(key, &report.user, report.at),
            result: report.result,
        }));
    }
}

impl std::fmt::Debug for Telemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Telemetry")
            .field("on", &self.on.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::Value;

    use super::*;

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_800_000_000;
    const KEY: [u8; 32] = [7; 32];

    #[test]
    fn a_person_keeps_one_hash_all_day_and_across_a_restart() {
        let alice = person(&KEY, "alice", NOW);
        assert_eq!(person(&KEY, "alice", NOW + 60), alice);
        assert_eq!(alice.len(), 32);
        assert_ne!(person(&KEY, "bob", NOW), alice);
        assert_ne!(person(&[8; 32], "alice", NOW), alice);
    }

    #[test]
    fn a_new_utc_day_gives_another_hash() {
        let midnight = NOW - NOW.rem_euclid(DAY) + DAY;
        assert_ne!(
            person(&KEY, "alice", midnight - 1),
            person(&KEY, "alice", midnight)
        );
    }

    /// The events shown, as `(event, person)`.
    type Shown = Arc<Mutex<Vec<(String, Option<String>)>>>;

    /// A program in debug mode and the events it shows.
    fn shown() -> (Telemetry, Shown) {
        let shown = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&shown);
        let show: zeabur_telemetry::Show = Arc::new(move |body| {
            let body: Value = serde_json::from_str(body).unwrap();
            let event = &body["batch"][0];
            log.lock().unwrap().push((
                event["event"].as_str().unwrap().to_owned(),
                event["properties"]["person"].as_str().map(str::to_owned),
            ));
        });
        (Telemetry::debug(show), shown)
    }

    #[test]
    fn a_feature_is_sent_once_a_day_and_names_nobody() {
        let (telemetry, shown) = shown();
        let tomorrow = NOW - NOW.rem_euclid(DAY) + DAY;
        telemetry.used(Feature::LogsViewed, NOW);
        telemetry.used(Feature::LogsViewed, NOW + 5);
        telemetry.used(Feature::TerminalOpened, NOW + 5);
        telemetry.used(Feature::LogsViewed, tomorrow);
        let used = ("feature_used".to_owned(), None);
        assert_eq!(*shown.lock().unwrap(), [used.clone(), used.clone(), used]);
    }

    #[test]
    fn each_sign_in_is_sent() {
        let (telemetry, shown) = shown();
        telemetry.signed_in("alice", NOW);
        telemetry.signed_in("alice", NOW + 60);
        let alice = Some(person(&KEY, "alice", NOW));
        assert_eq!(
            *shown.lock().unwrap(),
            [
                ("signed_in".to_owned(), alice.clone()),
                ("signed_in".to_owned(), alice),
            ]
        );
    }

    #[test]
    fn nothing_is_sent_when_the_program_is_off() {
        let id = "8f14e45f-ceea-467a-9575-0123456789ab";
        assert!(Telemetry::new(false, id, KEY).on.is_none());
        assert!(Telemetry::new(true, "not-a-uuid", KEY).on.is_none());
    }
}
