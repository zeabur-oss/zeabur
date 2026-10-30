//! Whether to send, and the sending.

use std::sync::Arc;
use std::time::Duration;

use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};

use crate::event::{Event, InstallationId};

/// `PostHog`'s batch endpoint.
pub const ENDPOINT: &str = "https://us.i.posthog.com/batch/";
/// The `PostHog` project key, `ZEABUR_TELEMETRY_KEY` at build time: only release builds have it
/// (docs/internal/releasing.md). Empty: nothing is sent.
pub const PROJECT_KEY: &str = match option_env!("ZEABUR_TELEMETRY_KEY") {
    Some(key) => key,
    None => "",
};

/// How long the whole request may take.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Where the body goes in debug mode.
pub type Show = Arc<dyn Fn(&str) + Send + Sync>;

/// What a [`Sender`] does with an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    /// Nothing.
    Off,
    /// Hands the body to the [`Show`] callback; sends nothing.
    Debug,
    /// Sends.
    On,
}

impl Switch {
    /// The switch for `enabled` (the program was joined) under `DO_NOT_TRACK` and
    /// `ZEABUR_TELEMETRY`.
    #[must_use]
    pub fn from_env(enabled: bool) -> Self {
        let var = |name| std::env::var(name).ok();
        Self::resolve(
            enabled,
            var("DO_NOT_TRACK").as_deref(),
            var("ZEABUR_TELEMETRY").as_deref(),
        )
    }

    /// `DO_NOT_TRACK` set to anything but empty or `0` turns everything off and wins over the
    /// rest. Otherwise nothing is done unless `enabled`; `ZEABUR_TELEMETRY` then says `off` or
    /// `debug`.
    #[must_use]
    pub fn resolve(enabled: bool, do_not_track: Option<&str>, telemetry: Option<&str>) -> Self {
        if do_not_track.is_some_and(|v| !matches!(v.trim(), "" | "0")) || !enabled {
            return Self::Off;
        }
        match telemetry.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("off") => Self::Off,
            Some("debug") => Self::Debug,
            _ => Self::On,
        }
    }
}

/// Sends events, once each and without waiting for an answer to matter.
pub struct Sender {
    switch: Switch,
    endpoint: String,
    key: &'static str,
    show: Show,
}

impl std::fmt::Debug for Sender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender")
            .field("switch", &self.switch)
            .finish_non_exhaustive()
    }
}

impl Sender {
    /// A sender to `PostHog`. `show` gets the body in debug mode.
    #[must_use]
    pub fn new(switch: Switch, show: Show) -> Self {
        Self::to(switch, ENDPOINT, PROJECT_KEY, show)
    }

    /// A sender to another endpoint and key, for tests.
    #[must_use]
    pub fn to(switch: Switch, endpoint: &str, key: &'static str, show: Show) -> Self {
        Self {
            switch,
            endpoint: endpoint.to_owned(),
            key,
            show,
        }
    }

    /// Whether [`send`](Self::send) does anything.
    #[must_use]
    pub fn active(&self) -> bool {
        self.switch != Switch::Off
    }

    /// Sends `event` once. Blocks for at most a few seconds; a failure is a debug log line.
    pub fn send(&self, id: &InstallationId, event: &Event) {
        match self.switch {
            Switch::Off => {}
            Switch::Debug => (self.show)(&event.body(self.key, id)),
            Switch::On if self.key.is_empty() => {}
            Switch::On => {
                if let Err(error) = self.post(&event.body(self.key, id)) {
                    tracing::debug!(event = "telemetry.failed", %error, "not sent");
                }
            }
        }
    }

    fn post(&self, body: &str) -> Result<(), ureq::Error> {
        // Fails only when a provider is already installed, which is as good.
        drop(rustls::crypto::aws_lc_rs::default_provider().install_default());
        let agent: Agent = Agent::config_builder()
            .tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .max_redirects(0)
            .build()
            .into();
        agent
            .post(&self.endpoint)
            .header("content-type", "application/json")
            .send(body)
            .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::sync::{Mutex, mpsc};

    use serde_json::Value;

    use super::*;
    use crate::event::{
        Day, Distribution, ErrorCode, Failure, Install, Mode, Profile, Step, ZeaburDay,
    };

    /// A receiver on a free port that records each request body.
    struct Receiver {
        url: String,
        bodies: mpsc::Receiver<String>,
    }

    fn receiver() -> Receiver {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/batch/", listener.local_addr().unwrap());
        let (sent, bodies) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = Vec::new();
                let mut chunk = [0_u8; 4096];
                let body = loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    request.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&request).into_owned();
                    if let Some((head, body)) = text.split_once("\r\n\r\n") {
                        let length = head
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if body.len() >= length || n == 0 {
                            break body.to_owned();
                        }
                    } else if n == 0 {
                        break String::new();
                    }
                };
                drop(stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n"));
                drop(sent.send(body));
            }
        });
        Receiver { url, bodies }
    }

    impl Receiver {
        /// What arrived, waiting up to a second for it.
        fn requests(&self) -> Vec<String> {
            let mut all = Vec::new();
            while let Ok(body) = self.bodies.recv_timeout(Duration::from_millis(300)) {
                all.push(body);
            }
            all
        }
    }

    fn id() -> InstallationId {
        InstallationId::from_random([7; 16])
    }

    fn install() -> Install {
        Install {
            installer_version: "0.1.0",
            profile: Profile::SingleNode,
            distribution: Distribution::K3s,
        }
    }

    fn events() -> Vec<Event> {
        vec![
            Event::ZeaburDay(ZeaburDay {
                zeabur_version: "0.1.0",
                day: Day::from_unix(1_800_000_000),
                people: 3,
                mode: Mode::InCluster,
            }),
            Event::InstallStarted(install()),
            Event::InstallFinished(install()),
            Event::InstallFailed(Failure {
                install: install(),
                step: Step::Cluster,
                error: ErrorCode::Download,
            }),
        ]
    }

    fn nobody() -> Show {
        Arc::new(|_| {})
    }

    #[test]
    fn do_not_track_wins_over_everything() {
        for value in ["1", "true", "yes", " 1 "] {
            for telemetry in [None, Some("debug"), Some("on")] {
                assert_eq!(
                    Switch::resolve(true, Some(value), telemetry),
                    Switch::Off,
                    "{value} {telemetry:?}"
                );
            }
        }
        assert_eq!(Switch::resolve(true, Some("0"), None), Switch::On);
        assert_eq!(Switch::resolve(true, Some(""), None), Switch::On);
        assert_eq!(Switch::resolve(true, None, Some("DEBUG")), Switch::Debug);
        assert_eq!(Switch::resolve(true, None, Some("off")), Switch::Off);
        assert_eq!(Switch::resolve(false, None, Some("debug")), Switch::Off);
    }

    #[test]
    fn off_makes_no_request() {
        let receiver = receiver();
        for switch in [
            Switch::resolve(true, Some("1"), None),
            Switch::resolve(true, None, Some("off")),
            Switch::resolve(false, None, None),
        ] {
            let sender = Sender::to(switch, &receiver.url, "phc_test", nobody());
            for event in events() {
                sender.send(&id(), &event);
            }
        }
        assert_eq!(receiver.requests(), Vec::<String>::new());
    }

    #[test]
    fn debug_shows_the_body_and_sends_nothing() {
        let receiver = receiver();
        let shown = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&shown);
        let show: Show = Arc::new(move |body| log.lock().unwrap().push(body.to_owned()));
        let sender = Sender::to(Switch::Debug, &receiver.url, "phc_test", show);
        let event = &events()[0];
        sender.send(&id(), event);
        assert_eq!(receiver.requests(), Vec::<String>::new());
        assert_eq!(*shown.lock().unwrap(), vec![event.body("phc_test", &id())]);
    }

    #[test]
    fn on_sends_one_request_and_no_key_sends_none() {
        let receiver = receiver();
        Sender::to(Switch::On, &receiver.url, "", nobody()).send(&id(), &events()[0]);
        assert_eq!(receiver.requests(), Vec::<String>::new());
        Sender::to(Switch::On, &receiver.url, "phc_test", nobody()).send(&id(), &events()[0]);
        let requests = receiver.requests();
        assert_eq!(requests.len(), 1);
        let body: Value = serde_json::from_str(&requests[0]).unwrap();
        assert_eq!(body["api_key"], "phc_test");
        assert_eq!(body["batch"][0]["distinct_id"], id().as_str());
    }

    #[test]
    fn a_receiver_that_is_down_costs_nothing() {
        let url = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}/batch/", listener.local_addr().unwrap())
        };
        Sender::to(Switch::On, &url, "phc_test", nobody()).send(&id(), &events()[0]);
    }

    /// Everything an event may carry. A new property has to be added here and to the docs page.
    #[test]
    fn a_body_has_only_allowed_properties() {
        let allowed: &[&str] = &[
            "zeabur_version",
            "day",
            "people",
            "mode",
            "installer_version",
            "profile",
            "distribution",
            "step",
            "error",
            "$process_person_profile",
            "$geoip_disable",
        ];
        for event in events() {
            let body: Value = serde_json::from_str(&event.body("k", &id())).unwrap();
            assert_eq!(body["batch"][0]["event"], event.name());
            let properties = body["batch"][0]["properties"].as_object().unwrap();
            for (name, value) in properties {
                assert!(allowed.contains(&name.as_str()), "{name}");
                assert!(!value.is_object() && !value.is_array(), "{name}");
            }
            assert_eq!(properties["$process_person_profile"], false);
            assert_eq!(properties["$geoip_disable"], true);
            assert_eq!(body["batch"].as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn the_docs_page_names_every_event_and_property() {
        let page = include_str!("../../../docs/reference/telemetry.mdx");
        for event in events() {
            assert!(
                page.contains(&format!("`{}`", event.name())),
                "{}",
                event.name()
            );
            let body: Value = serde_json::from_str(&event.body("k", &id())).unwrap();
            for name in body["batch"][0]["properties"].as_object().unwrap().keys() {
                assert!(page.contains(&format!("`{name}`")), "{name}");
            }
        }
    }
}
