//! The experience program for a local run (docs/reference/telemetry.mdx): `zeaburctl ui` sends
//! `cluster_connected` unless the person left, and `zeaburctl telemetry reset` deletes the
//! random ID the run keeps.

use std::path::Path;
use std::sync::Arc;

use clap::Subcommand;
use zeabur_kube::ClusterError;
use zeabur_telemetry::{
    ClusterConnected, Connection, Event, LocalId, Mode, Sender, Show, Switch, standard_dir,
};

use crate::output;

/// `zeaburctl telemetry`.
#[derive(Debug, clap::Args)]
pub(crate) struct TelemetryArgs {
    #[command(subcommand)]
    command: TelemetryCommand,
}

#[derive(Debug, Subcommand)]
enum TelemetryCommand {
    /// Delete the random ID that zeaburctl ui keeps for the experience program.
    ///
    /// The next event it sends gets a new ID.
    Reset,
}

pub(crate) fn run(args: &TelemetryArgs) -> std::process::ExitCode {
    match args.command {
        TelemetryCommand::Reset => reset(),
    }
}

fn reset() -> std::process::ExitCode {
    let Some(directory) = standard_dir() else {
        output::error("cannot find the configuration directory of this computer");
        return std::process::ExitCode::FAILURE;
    };
    let had = LocalId::new(&directory, random).peek().is_some();
    match LocalId::reset(&directory) {
        Ok(()) => {
            output::info(if had {
                "Deleted your experience program ID. zeaburctl ui makes a new one the next time it is about to send an event."
            } else {
                "There is no experience program ID to delete."
            });
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            output::error(&format!("cannot delete the ID: {error}"));
            std::process::ExitCode::FAILURE
        }
    }
}

fn random() -> Option<[u8; 16]> {
    zeabur_core::crypto::random().ok()
}

/// The file in which the person can leave the program with `telemetry: false`.
const CONFIG: &str = "config.yaml";

#[derive(serde::Deserialize)]
struct Config {
    #[serde(default = "joined")]
    telemetry: bool,
}

const fn joined() -> bool {
    true
}

/// Whether `config.yaml` in `directory` leaves the program in. Only a missing file means the
/// default; a file that can't be read or parsed leaves it, so an opt-out is never lost.
fn joined_in(directory: &Path) -> bool {
    let path = directory.join(CONFIG);
    let unusable = |error: &dyn std::fmt::Display| {
        output::error(&format!(
            "cannot read {}: {error}. zeaburctl ui sends nothing until it is fixed.",
            path.display()
        ));
        false
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return true,
        Err(error) => return unusable(&error),
    };
    if text.trim().is_empty() {
        return true;
    }
    match serde_saphyr::from_str::<Config>(&text) {
        Ok(config) => config.telemetry,
        Err(error) => unusable(&error),
    }
}

/// What `zeaburctl ui` sends. It is on by default; `DO_NOT_TRACK`, `ZEABUR_TELEMETRY=off` or
/// `telemetry: false` in `config.yaml` turns it off.
pub(crate) struct Local {
    on: Option<(Sender, LocalId)>,
}

impl Local {
    /// The program as this environment and `config.yaml` set it. When it is on, says so and
    /// how to turn it off.
    pub(crate) fn from_env() -> Self {
        let Some(directory) = standard_dir() else {
            return Self { on: None };
        };
        let switch = Switch::from_env(joined_in(&directory));
        if switch == Switch::Off {
            return Self { on: None };
        }
        output::info(&format!(
            "zeaburctl ui sends its version and whether it could connect to the cluster, to help \
             improve Zeabur. To turn it off, set DO_NOT_TRACK=1 or put `telemetry: false` in {}.\n\
             What is sent: https://oss.zeabur.com/docs/zeabur/reference/telemetry/",
            directory.join(CONFIG).display()
        ));
        let show: Show = Arc::new(|body| output::info(&format!("telemetry: {body}")));
        Self {
            on: Some((Sender::new(switch, show), LocalId::new(&directory, random))),
        }
    }

    /// Sends how connecting to the cluster ended: one try, with a short timeout.
    pub(crate) async fn connected(&self, result: Connection) {
        let Some((sender, id)) = &self.on else {
            return;
        };
        let event = Event::ClusterConnected(ClusterConnected {
            zeabur_version: env!("CARGO_PKG_VERSION"),
            mode: Mode::Local,
            result,
        });
        sender.send_local(id, &event).await;
    }
}

/// How the cluster's answer to the first request ended the connection.
pub(crate) const fn outcome(error: &ClusterError) -> Connection {
    match (error, error.code()) {
        (ClusterError::Transport(_), _) => Connection::Unreachable,
        (_, Some(401)) => Connection::CredentialRefused,
        (_, Some(403)) => Connection::NoReadRight,
        _ => Connection::Other,
    }
}

#[cfg(test)]
mod tests {
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
            joined_in(directory.path())
        };
        assert!(with(None));
        assert!(with(Some("")));
        assert!(with(Some("telemetry: true\n")));
        assert!(with(Some("other: 1\n")));
        assert!(!with(Some("telemetry: false\n")));
        assert!(!with(Some("telemetry: [")));
        std::fs::write(directory.path().join(CONFIG), [0xFF, 0xFE]).unwrap();
        assert!(!joined_in(directory.path()));
    }

    #[test]
    fn a_connection_ends_by_what_the_cluster_said() {
        assert_eq!(
            outcome(&ClusterError::Transport("refused".to_owned())),
            Connection::Unreachable
        );
        assert_eq!(outcome(&status(401)), Connection::CredentialRefused);
        assert_eq!(outcome(&status(403)), Connection::NoReadRight);
        assert_eq!(outcome(&status(500)), Connection::Other);
        assert_eq!(
            outcome(&ClusterError::Decode("x".to_owned())),
            Connection::Other
        );
    }
}
