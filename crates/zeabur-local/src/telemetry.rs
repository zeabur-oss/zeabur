//! The experience program for a local run (docs/reference/telemetry.mdx).
//!
//! `zeaburctl ui` and the desktop app send `cluster_connected` unless the person left.
//! `config.yaml` in the platform's configuration directory holds the person's choice.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use zeabur_api::used::Used;
use zeabur_kube::ClusterError;
pub use zeabur_telemetry::{Client, Connection, Show, Switch};
use zeabur_telemetry::{
    ClusterConnected, Day, Event, FeatureUsed, Id, LocalId, Mode, Sender, standard_dir,
};

use crate::ServiceError;

/// The file in which the person can leave the program with `telemetry: false`.
pub const CONFIG: &str = "config.yaml";

/// How long [`Local::close`] waits for the queued events.
const WAIT: Duration = Duration::from_secs(2);

#[derive(serde::Deserialize)]
struct Config {
    #[serde(default)]
    telemetry: Option<bool>,
}

/// What `config.yaml` in `directory` says: `None` when it is missing, empty or has no
/// `telemetry`, which means the program is joined.
///
/// # Errors
///
/// When the file exists and cannot be read or parsed.
pub fn read_config(directory: &Path) -> Result<Option<bool>, String> {
    let path = directory.join(CONFIG);
    let unusable =
        |error: &dyn std::fmt::Display| format!("cannot read {}: {error}", path.display());
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(unusable(&error)),
    };
    parse(&text).map_err(|error| unusable(&error))
}

/// The `telemetry` value of the text of a `config.yaml`.
///
/// # Errors
///
/// When the text is not such a file.
pub fn parse(text: &str) -> Result<Option<bool>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_saphyr::from_str::<Config>(text)
        .map(|config| config.telemetry)
        .map_err(|error| error.to_string())
}

/// A random ID for [`LocalId`].
#[must_use]
pub fn random() -> Option<[u8; 16]> {
    zeabur_core::crypto::random().ok()
}

/// What a local run sends. It is on by default; `DO_NOT_TRACK`, `ZEABUR_TELEMETRY=off` or
/// `telemetry: false` in `config.yaml` turns it off. Every event names the `client` that sends it.
#[derive(Debug, Default)]
pub struct Local {
    sender: Option<(Arc<Sender>, Client)>,
}

impl Local {
    /// The program of `client` as this environment and `config.yaml` set it. `show` gets each body
    /// in debug mode.
    ///
    /// # Errors
    ///
    /// When `config.yaml` cannot be read or parsed; nothing is sent until it is fixed, so an
    /// opt-out is never lost.
    pub fn from_env(client: Client, show: Show) -> Result<Self, String> {
        let Some(directory) = standard_dir() else {
            return Ok(Self::default());
        };
        let joined = read_config(&directory)?.unwrap_or(true);
        Ok(Self::with(
            Switch::from_env(joined),
            &directory,
            client,
            show,
        ))
    }

    /// The program of `client` with `switch`, keeping its ID in `directory`.
    #[must_use]
    pub fn with(switch: Switch, directory: &Path, client: Client, show: Show) -> Self {
        let id = Id::Local(LocalId::new(directory, random));
        let sender = Sender::new(switch, id, show);
        Self {
            sender: sender.active().then(|| (Arc::new(sender), client)),
        }
    }

    /// Whether anything is sent or shown.
    #[must_use]
    pub const fn is_on(&self) -> bool {
        self.sender.is_some()
    }

    /// Queues how connecting to the cluster ended. Call it inside the Tokio runtime.
    pub fn connected(&self, result: Connection) {
        if let Some((sender, client)) = &self.sender {
            sender.send(Event::ClusterConnected(ClusterConnected {
                zeabur_version: env!("CARGO_PKG_VERSION"),
                mode: Mode::Local,
                client: *client,
                result,
            }));
        }
    }

    /// What the local service calls after a call that used a feature succeeded; `None` when
    /// nothing is sent or shown.
    #[must_use]
    pub fn used(&self) -> Option<Used> {
        let (sender, client) = self.sender.as_ref()?;
        let (sender, client) = (Arc::clone(sender), *client);
        Some(Arc::new(move |feature| {
            let seconds = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0));
            sender.send(Event::FeatureUsed(FeatureUsed {
                zeabur_version: env!("CARGO_PKG_VERSION"),
                day: Day::from_unix(seconds),
                mode: Mode::Local,
                client: Some(client),
                feature,
            }));
        }))
    }

    /// Waits a moment for the queued events to be sent. Call it before the program exits.
    pub async fn close(&self) {
        if let Some((sender, _)) = &self.sender {
            sender.close(WAIT).await;
        }
    }
}

impl ServiceError {
    /// How connecting to the cluster ended, for `cluster_connected`.
    #[must_use]
    pub const fn connection(&self) -> Connection {
        match self {
            Self::Kubeconfig(_) => Connection::Other,
            Self::Identity(error) => outcome(error),
            Self::Listen(_) | Self::Random(_) | Self::Serve(_) => Connection::Connected,
        }
    }
}

/// How the cluster's answer to the first request ended the connection.
const fn outcome(error: &ClusterError) -> Connection {
    match (error, error.code()) {
        (ClusterError::Transport(_), _) => Connection::Unreachable,
        (_, Some(401)) => Connection::CredentialRefused,
        (_, Some(403)) => Connection::NoReadRight,
        _ => Connection::Other,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use zeabur_telemetry::Feature;

    use super::*;

    fn status(code: u16) -> ClusterError {
        let status = serde_json::json!({"code": code, "reason": "", "message": ""});
        ClusterError::Status(serde_json::from_value(status).unwrap())
    }

    #[test]
    fn config_yaml_leaves_the_program_only_when_it_says_so() {
        let directory = tempfile::tempdir().unwrap();
        let with = |text: Option<&str>| {
            if let Some(text) = text {
                std::fs::write(directory.path().join(CONFIG), text).unwrap();
            }
            read_config(directory.path()).map(|joined| joined.unwrap_or(true))
        };
        assert_eq!(with(None), Ok(true));
        assert_eq!(with(Some("")), Ok(true));
        assert_eq!(with(Some("telemetry: true\n")), Ok(true));
        assert_eq!(with(Some("other: 1\n")), Ok(true));
        assert_eq!(with(Some("telemetry: false\n")), Ok(false));
        assert!(with(Some("telemetry: [")).is_err());
        std::fs::write(directory.path().join(CONFIG), [0xFF, 0xFE]).unwrap();
        assert!(read_config(directory.path()).is_err());
    }

    #[test]
    fn a_connection_ends_by_what_the_cluster_said() {
        let identity = |error| ServiceError::Identity(error).connection();
        assert_eq!(
            identity(ClusterError::Transport("refused".to_owned())),
            Connection::Unreachable
        );
        assert_eq!(identity(status(401)), Connection::CredentialRefused);
        assert_eq!(identity(status(403)), Connection::NoReadRight);
        assert_eq!(identity(status(500)), Connection::Other);
        assert_eq!(
            identity(ClusterError::Decode("x".to_owned())),
            Connection::Other
        );
    }

    #[tokio::test]
    async fn off_sends_nothing_and_closes_at_once() {
        let directory = tempfile::tempdir().unwrap();
        let local = Local::with(
            Switch::Off,
            directory.path(),
            Client::Desktop,
            Arc::new(|_| {}),
        );
        assert!(!local.is_on());
        assert!(local.used().is_none());
        local.connected(Connection::Connected);
        local.close().await;
        assert_eq!(LocalId::new(directory.path(), random).peek(), None);
    }

    #[tokio::test]
    async fn debug_shows_cluster_connected_with_its_client_and_result() {
        let directory = tempfile::tempdir().unwrap();
        let shown = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&shown);
        let show: Show = Arc::new(move |body| log.lock().unwrap().push(body.to_owned()));
        let local = Local::with(Switch::Debug, directory.path(), Client::Zeaburctl, show);
        local.connected(Connection::NoReadRight);
        local.close().await;
        let body = shown.lock().unwrap().remove(0);
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["batch"][0]["event"], "cluster_connected");
        assert_eq!(body["batch"][0]["properties"]["mode"], "local");
        assert_eq!(body["batch"][0]["properties"]["client"], "zeaburctl");
        assert_eq!(body["batch"][0]["properties"]["result"], "no_read_right");
    }

    #[tokio::test]
    async fn a_feature_used_in_desktop_names_the_local_mode_and_desktop() {
        let directory = tempfile::tempdir().unwrap();
        let shown = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&shown);
        let show: Show = Arc::new(move |body| log.lock().unwrap().push(body.to_owned()));
        let local = Local::with(Switch::Debug, directory.path(), Client::Desktop, show);
        let used = local.used().unwrap();
        used(Feature::LogsViewed);
        used(Feature::LogsViewed);
        local.close().await;
        let bodies = shown.lock().unwrap().clone();
        let events: Vec<serde_json::Value> = bodies
            .iter()
            .flat_map(|body| {
                let body: serde_json::Value = serde_json::from_str(body).unwrap();
                body["batch"].as_array().unwrap().clone()
            })
            .collect();
        assert_eq!(events.len(), 2, "each use is sent");
        for event in events {
            assert_eq!(event["event"], "feature_used");
            assert_eq!(event["properties"]["feature"], "logs_viewed");
            assert_eq!(event["properties"]["mode"], "local");
            assert_eq!(event["properties"]["client"], "desktop");
        }
    }
}
