//! The experience program for a local run (docs/reference/telemetry.mdx): `zeaburctl ui` sends
//! `cluster_connected` when the person joined, and `zeaburctl telemetry reset` deletes the
//! random ID the run keeps.

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

/// What `zeaburctl ui` sends. It does nothing unless the person joined with
/// `ZEABUR_TELEMETRY_ENABLED=true`, and `DO_NOT_TRACK` turns it off.
pub(crate) struct Local {
    on: Option<(Sender, LocalId)>,
    is_internal: bool,
}

impl Local {
    pub(crate) fn from_env() -> Self {
        let var = |name| std::env::var(name).ok();
        let is_true = |name| var(name).is_some_and(|v| matches!(v.trim(), "1" | "true"));
        let switch = Switch::from_env(is_true("ZEABUR_TELEMETRY_ENABLED"));
        let on = standard_dir()
            .filter(|_| switch != Switch::Off)
            .map(|directory| {
                let show: Show = Arc::new(|body| output::info(&format!("telemetry: {body}")));
                (Sender::new(switch, show), LocalId::new(&directory, random))
            });
        Self {
            on,
            is_internal: is_true("ZEABUR_TELEMETRY_INTERNAL"),
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
            is_internal: self.is_internal,
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
