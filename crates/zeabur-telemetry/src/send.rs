//! Whether to send, and the sending.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use bytes::Bytes;
use http::header::{CONTENT_TYPE, RETRY_AFTER};
use http::{Request, StatusCode};
use http_body_util::Full;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use zeabur_http::{Https, Reach};

use crate::event::{Event, InstallationId};
use crate::local::LocalId;

/// `PostHog`'s batch endpoint, through our own domain.
pub const ENDPOINT: &str = "https://oss-nori.zeabur.com/batch/";
/// The `PostHog` project key, `ZEABUR_TELEMETRY_KEY` at build time: only release builds have it
/// (docs/internal/releasing.md). Empty: nothing is sent.
pub const PROJECT_KEY: &str = match option_env!("ZEABUR_TELEMETRY_KEY") {
    Some(key) => key,
    None => "",
};

/// How long one request may take.
const TIMEOUT: Duration = if cfg!(test) {
    Duration::from_millis(500)
} else {
    Duration::from_secs(5)
};

/// The wait before the first retry; it doubles for each one after.
const BACKOFF: Duration = if cfg!(test) {
    Duration::from_millis(10)
} else {
    Duration::from_secs(1)
};

/// The longest wait before a retry, `Retry-After` included.
const MAX_WAIT: Duration = if cfg!(test) {
    Duration::from_millis(50)
} else {
    Duration::from_secs(30)
};

/// Retries after the first try of a request.
const RETRIES: u32 = 3;

/// Events waiting to be sent; more are dropped.
const CAPACITY: usize = 256;

/// The most events in one request.
const BATCH: usize = 50;

/// Where the body goes in debug mode.
pub type Show = Arc<dyn Fn(&str) + Send + Sync>;

/// Whether `DO_NOT_TRACK` in the environment turns off tracking: Zeabur's experience program and
/// its update checks.
#[must_use]
pub fn do_not_track() -> bool {
    do_not_track_in(std::env::var("DO_NOT_TRACK").ok().as_deref())
}

/// Whether a `DO_NOT_TRACK` of `value` turns off tracking: set to anything but empty or `0`.
#[must_use]
pub fn do_not_track_in(value: Option<&str>) -> bool {
    value.is_some_and(|v| !matches!(v.trim(), "" | "0"))
}

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
        if do_not_track_in(do_not_track) || !enabled {
            return Self::Off;
        }
        match telemetry.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("off") => Self::Off,
            Some("debug") => Self::Debug,
            _ => Self::On,
        }
    }
}

/// The ID events are sent under.
#[derive(Debug)]
pub enum Id {
    /// A server's installation ID.
    Installation(InstallationId),
    /// A local run's ID, read or made only when a request is about to be sent.
    Local(LocalId),
}

/// Sends events in the background, in batches, retrying failures that may pass.
///
/// [`send`](Self::send) queues an event and returns at once. The first event starts a worker on
/// the current Tokio runtime. [`close`](Self::close) waits for the queue to empty.
pub struct Sender {
    switch: Switch,
    endpoint: String,
    key: &'static str,
    id: Arc<Id>,
    show: Show,
    queue: Mutex<Queue>,
}

/// The worker and its queue.
#[derive(Default)]
struct Queue {
    events: Option<mpsc::Sender<Event>>,
    worker: Option<JoinHandle<()>>,
    closed: bool,
}

impl std::fmt::Debug for Sender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender")
            .field("switch", &self.switch)
            .finish_non_exhaustive()
    }
}

impl Sender {
    /// A sender to `PostHog` under `id`. `show` gets the body in debug mode.
    #[must_use]
    pub fn new(switch: Switch, id: Id, show: Show) -> Self {
        Self::to(switch, ENDPOINT, PROJECT_KEY, id, show)
    }

    /// A sender to another endpoint and key, for tests.
    #[must_use]
    pub fn to(switch: Switch, endpoint: &str, key: &'static str, id: Id, show: Show) -> Self {
        Self {
            switch,
            endpoint: endpoint.to_owned(),
            key,
            id: Arc::new(id),
            show,
            queue: Mutex::new(Queue::default()),
        }
    }

    /// Whether [`send`](Self::send) does anything.
    #[must_use]
    pub fn active(&self) -> bool {
        self.switch != Switch::Off
    }

    /// Queues `event`, or shows it in debug mode. Returns whether it was queued or shown: not
    /// when the switch is off, the build has no key, the queue is full or closed, or there is no
    /// Tokio runtime.
    pub fn send(&self, event: Event) -> bool {
        match self.switch {
            Switch::Off => false,
            Switch::Debug => {
                let id = match &*self.id {
                    Id::Installation(id) => Some(id.clone()),
                    Id::Local(local) => local.peek(),
                };
                let distinct_id = id.as_ref().map_or("local-id", InstallationId::as_str);
                (self.show)(&event.body_as(self.key, distinct_id));
                true
            }
            Switch::On if self.key.is_empty() => false,
            Switch::On => self.queue(event),
        }
    }

    fn queue(&self, event: Event) -> bool {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        if queue.closed {
            return false;
        }
        if queue.events.is_none() {
            let Ok(runtime) = tokio::runtime::Handle::try_current() else {
                tracing::debug!(event = "telemetry.failed", "no Tokio runtime");
                return false;
            };
            let (events, waiting) = mpsc::channel(CAPACITY);
            let post = Post {
                endpoint: self.endpoint.clone(),
                key: self.key,
                id: Arc::clone(&self.id),
            };
            queue.worker = Some(runtime.spawn(post.run(waiting)));
            queue.events = Some(events);
        }
        let queued = queue
            .events
            .as_ref()
            .is_some_and(|events| events.try_send(event).is_ok());
        drop(queue);
        if !queued {
            tracing::debug!(event = "telemetry.failed", "queue full; dropped");
        }
        queued
    }

    /// Takes no more events and waits up to `wait` for the queued ones to be sent. Events still
    /// waiting after that are lost.
    pub async fn close(&self, wait: Duration) {
        let worker = {
            let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
            queue.closed = true;
            queue.events = None;
            queue.worker.take()
        };
        if let Some(worker) = worker
            && tokio::time::timeout(wait, worker).await.is_err()
        {
            tracing::debug!(event = "telemetry.failed", "not all sent before exit");
        }
    }
}

/// How one request ended.
enum Answer {
    Sent,
    /// Not worth a retry: the endpoint refused the body or the key.
    Refused(String),
    /// May pass later; wait for `after` if the endpoint said so.
    Retry {
        after: Option<Duration>,
        error: String,
    },
}

/// What the worker posts with.
struct Post {
    endpoint: String,
    key: &'static str,
    id: Arc<Id>,
}

impl Post {
    /// Sends the queued events until the queue is closed and empty.
    async fn run(self, mut waiting: mpsc::Receiver<Event>) {
        let https = match zeabur_http::tls(None) {
            Ok(tls) => Https::new(Reach::Any, &tls),
            Err(error) => {
                tracing::debug!(event = "telemetry.failed", %error, "no TLS");
                return;
            }
        };
        let mut events = Vec::with_capacity(BATCH);
        while waiting.recv_many(&mut events, BATCH).await > 0 {
            self.deliver(&https, &events).await;
            events.clear();
        }
    }

    /// Posts `events` in one request, with retries.
    async fn deliver(&self, https: &Https, events: &[Event]) {
        let id = match &*self.id {
            Id::Installation(id) => id.clone(),
            Id::Local(local) => match local.get() {
                Ok(id) => id,
                Err(error) => {
                    tracing::debug!(event = "telemetry.failed", %error, "no local ID");
                    return;
                }
            },
        };
        let body = Event::batch(self.key, id.as_str(), events);
        let mut wait = BACKOFF;
        for attempt in 0..=RETRIES {
            match self.post(https, &body).await {
                Answer::Sent => return,
                Answer::Refused(error) => {
                    tracing::warn!(event = "telemetry.refused", %error, "not sent");
                    return;
                }
                Answer::Retry { error, .. } if attempt == RETRIES => {
                    tracing::debug!(event = "telemetry.failed", %error, "not sent");
                }
                Answer::Retry { after, .. } => {
                    tokio::time::sleep(after.unwrap_or(wait).min(MAX_WAIT)).await;
                    wait *= 2;
                }
            }
        }
    }

    /// Posts `body` and reads the answer's status, within [`TIMEOUT`]. The endpoint is fixed in
    /// the binary, so any address may be reached.
    async fn post(&self, https: &Https, body: &str) -> Answer {
        let retry = |error: String| Answer::Retry { after: None, error };
        let request = match Request::post(self.endpoint.as_str())
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(body.to_owned())))
        {
            Ok(request) => request,
            Err(error) => return Answer::Refused(error.to_string()),
        };
        let answer = match tokio::time::timeout(TIMEOUT, https.request(request)).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(error)) => return retry(error),
            Err(_elapsed) => return retry("no answer in time".to_owned()),
        };
        let status = answer.status();
        if status.is_success() {
            Answer::Sent
        } else if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            let after = answer
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse().ok())
                .map(Duration::from_secs);
            Answer::Retry {
                after,
                error: status.to_string(),
            }
        } else {
            Answer::Refused(status.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::sync::mpsc as std_mpsc;
    use std::time::Instant;

    use serde_json::Value;

    use super::*;
    use crate::event::{
        AppChangeSubmitted, Client, ClusterConnected, Connection, DatabaseKind, Day, DomainKind,
        Feature, FeatureUsed, Mode, SignedIn, Source, Submitted,
    };

    /// How the receiver answers one connection.
    #[derive(Clone, Copy)]
    enum Reply {
        /// This status, with this `Retry-After` when set.
        Status(u16, Option<&'static str>),
        /// Closes the connection without an answer.
        HangUp,
    }

    /// A receiver on a free port that records each request body.
    struct Receiver {
        url: String,
        bodies: std_mpsc::Receiver<String>,
    }

    /// A receiver that answers with `replies` in turn, then with 200.
    fn receiver_with(replies: Vec<Reply>) -> Receiver {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/batch/", listener.local_addr().unwrap());
        let (sent, bodies) = std_mpsc::channel();
        let mut replies = replies.into_iter();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let body = read_body(&mut stream);
                match replies.next().unwrap_or(Reply::Status(200, None)) {
                    Reply::HangUp => drop(stream),
                    Reply::Status(code, retry_after) => {
                        let retry = retry_after.map_or_default(|s| format!("retry-after: {s}\r\n"));
                        let head = format!(
                            "HTTP/1.1 {code} X\r\n{retry}content-length: 0\r\nconnection: close\r\n\r\n"
                        );
                        drop(stream.write_all(head.as_bytes()));
                    }
                }
                drop(sent.send(body));
            }
        });
        Receiver { url, bodies }
    }

    fn receiver() -> Receiver {
        receiver_with(Vec::new())
    }

    fn read_body(stream: &mut std::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
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
                    return body.to_owned();
                }
            } else if n == 0 {
                return String::new();
            }
        }
    }

    impl Receiver {
        /// What arrived so far, waiting a moment for more.
        fn requests(&self) -> Vec<String> {
            let mut all = Vec::new();
            while let Ok(body) = self.bodies.recv_timeout(Duration::from_millis(300)) {
                all.push(body);
            }
            all
        }
    }

    /// The names of the events in `body`.
    fn names(body: &str) -> Vec<String> {
        let body: Value = serde_json::from_str(body).unwrap();
        body["batch"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["event"].as_str().unwrap().to_owned())
            .collect()
    }

    fn id() -> InstallationId {
        InstallationId::from_random([7; 16])
    }

    fn installation() -> Id {
        Id::Installation(id())
    }

    fn used(feature: Feature) -> FeatureUsed {
        FeatureUsed {
            zeabur_version: "0.1.0",
            day: Day::from_unix(1_800_000_000),
            mode: Mode::InCluster,
            client: None,
            feature,
        }
    }

    fn events() -> Vec<Event> {
        vec![
            Event::SignedIn(SignedIn {
                zeabur_version: "0.1.0",
                person: "0123456789abcdef0123456789abcdef".to_owned(),
                mode: Mode::InCluster,
            }),
            Event::FeatureUsed(used(Feature::ServiceCreated(Source::Template("wordpress")))),
            Event::FeatureUsed(used(Feature::ServiceCreated(Source::Git))),
            Event::FeatureUsed(used(Feature::DatabaseCreated(DatabaseKind::ObjectStorage))),
            Event::FeatureUsed(used(Feature::DomainAdded(DomainKind::Custom))),
            Event::FeatureUsed(used(Feature::LogsViewed)),
            Event::FeatureUsed(FeatureUsed {
                mode: Mode::Local,
                client: Some(Client::Desktop),
                ..used(Feature::LogsViewed)
            }),
            Event::ClusterConnected(ClusterConnected {
                zeabur_version: "0.1.0",
                mode: Mode::Local,
                client: Client::Zeaburctl,
                result: Connection::NoReadRight,
            }),
            Event::AppChangeSubmitted(AppChangeSubmitted {
                zeabur_version: "0.1.0",
                mode: Mode::InCluster,
                person: "0123456789abcdef0123456789abcdef".to_owned(),
                result: Submitted::Conflict,
            }),
        ]
    }

    fn nobody() -> Show {
        Arc::new(|_| {})
    }

    fn on(url: &str) -> Sender {
        Sender::to(Switch::On, url, "phc_test", installation(), nobody())
    }

    const WAIT: Duration = Duration::from_secs(5);

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

    #[tokio::test]
    async fn off_makes_no_request() {
        let receiver = receiver();
        for switch in [
            Switch::resolve(true, Some("1"), None),
            Switch::resolve(true, None, Some("off")),
            Switch::resolve(false, None, None),
        ] {
            let sender = Sender::to(switch, &receiver.url, "phc_test", installation(), nobody());
            for event in events() {
                assert!(!sender.send(event));
            }
            sender.close(WAIT).await;
        }
        assert_eq!(receiver.requests(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn debug_shows_the_body_and_sends_nothing() {
        let receiver = receiver();
        let shown = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&shown);
        let show: Show = Arc::new(move |body| log.lock().unwrap().push(body.to_owned()));
        let sender = Sender::to(
            Switch::Debug,
            &receiver.url,
            "phc_test",
            installation(),
            show,
        );
        let event = events().remove(0);
        assert!(sender.send(event.clone()));
        sender.close(WAIT).await;
        assert_eq!(receiver.requests(), Vec::<String>::new());
        assert_eq!(*shown.lock().unwrap(), vec![event.body("phc_test", &id())]);
    }

    #[tokio::test]
    async fn no_key_sends_nothing() {
        let receiver = receiver();
        let sender = Sender::to(Switch::On, &receiver.url, "", installation(), nobody());
        assert!(!sender.send(events().remove(0)));
        sender.close(WAIT).await;
        assert_eq!(receiver.requests(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn events_queued_before_close_are_sent_under_the_id() {
        let receiver = receiver();
        let sender = on(&receiver.url);
        for event in events() {
            assert!(sender.send(event));
        }
        sender.close(WAIT).await;
        let requests = receiver.requests();
        let sent: Vec<String> = requests.iter().flat_map(|body| names(body)).collect();
        let queued: Vec<String> = events().iter().map(|e| e.name().to_owned()).collect();
        assert_eq!(sent, queued);
        let body: Value = serde_json::from_str(&requests[0]).unwrap();
        assert_eq!(body["api_key"], "phc_test");
        assert_eq!(body["batch"][0]["distinct_id"], id().as_str());
    }

    #[tokio::test]
    async fn a_closed_sender_takes_no_events() {
        let receiver = receiver();
        let sender = on(&receiver.url);
        sender.close(WAIT).await;
        assert!(!sender.send(events().remove(0)));
        assert_eq!(receiver.requests(), Vec::<String>::new());
    }

    #[test]
    fn without_a_runtime_an_event_is_not_queued() {
        let receiver = receiver();
        assert!(!on(&receiver.url).send(events().remove(0)));
    }

    #[tokio::test]
    async fn server_errors_rate_limits_and_hang_ups_are_retried() {
        let receiver = receiver_with(vec![
            Reply::Status(503, None),
            Reply::Status(429, Some("0")),
            Reply::HangUp,
        ]);
        let sender = on(&receiver.url);
        let event = events().remove(0);
        assert!(sender.send(event.clone()));
        sender.close(WAIT).await;
        let requests = receiver.requests();
        assert_eq!(requests, vec![event.body("phc_test", &id()); 4]);
    }

    #[tokio::test]
    async fn a_refused_request_is_not_retried() {
        let receiver = receiver_with(vec![Reply::Status(400, None)]);
        let sender = on(&receiver.url);
        assert!(sender.send(events().remove(0)));
        sender.close(WAIT).await;
        assert_eq!(receiver.requests().len(), 1);
    }

    #[tokio::test]
    async fn retries_end() {
        let receiver = receiver_with(vec![Reply::Status(500, None); 10]);
        let sender = on(&receiver.url);
        assert!(sender.send(events().remove(0)));
        sender.close(WAIT).await;
        assert_eq!(receiver.requests().len(), 1 + RETRIES as usize);
    }

    #[tokio::test]
    async fn a_receiver_that_is_down_holds_close_no_longer_than_its_wait() {
        let url = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}/batch/", listener.local_addr().unwrap())
        };
        let sender = on(&url);
        assert!(sender.send(events().remove(0)));
        let started = Instant::now();
        sender.close(Duration::from_millis(200)).await;
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn a_full_queue_drops_without_waiting() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/batch/", listener.local_addr().unwrap());
        let sender = on(&url);
        let started = Instant::now();
        let queued = (0..CAPACITY + 2 * BATCH)
            .filter(|_| sender.send(events().remove(0)))
            .count();
        assert!(queued < CAPACITY + 2 * BATCH);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
        sender.close(Duration::from_millis(10)).await;
        drop(listener);
    }

    #[tokio::test]
    async fn a_local_id_is_made_only_to_post() {
        let receiver = receiver();
        let directory = crate::local::tests::tempdir();
        let local = || Id::Local(LocalId::new(&directory, || Some([3; 16])));
        let event = events().remove(0);
        for (switch, key) in [
            (Switch::Off, "phc_test"),
            (Switch::Debug, "phc_test"),
            (Switch::On, ""),
        ] {
            let sender = Sender::to(switch, &receiver.url, key, local(), nobody());
            sender.send(event.clone());
            sender.close(WAIT).await;
        }
        let peek = || LocalId::new(&directory, || None).peek();
        assert_eq!(peek(), None);
        let sender = Sender::to(Switch::On, &receiver.url, "phc_test", local(), nobody());
        assert!(sender.send(event));
        sender.close(WAIT).await;
        let id = peek().unwrap();
        let requests = receiver.requests();
        let body: Value = serde_json::from_str(&requests[0]).unwrap();
        assert_eq!(body["batch"][0]["distinct_id"], id.as_str());
    }

    /// Everything an event may carry. A new property has to be added here and to the docs page.
    #[test]
    fn a_body_has_only_allowed_properties() {
        let allowed: &[&str] = &[
            "zeabur_version",
            "day",
            "person",
            "mode",
            "client",
            "feature",
            "source",
            "template",
            "kind",
            "result",
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
            if properties
                .get("source")
                .is_some_and(|source| source == "git")
            {
                assert!(!properties.contains_key("template"));
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
