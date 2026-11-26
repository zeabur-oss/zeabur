//! `zeaburctl telemetry`: shows the setting of the experience program for a local run
//! (`zeabur_local::telemetry`), leaves or joins it, and deletes the random ID the run keeps.

use std::path::Path;
use std::sync::Arc;

use clap::Subcommand;
use zeabur_local::telemetry::{CONFIG, Client, Local, Show, parse, random, read_config};
use zeabur_telemetry::{LocalId, PROJECT_KEY, Switch, standard_dir};

use crate::output;

/// `zeaburctl telemetry`.
#[derive(Debug, clap::Args)]
pub(crate) struct TelemetryArgs {
    #[command(subcommand)]
    command: TelemetryCommand,
}

#[derive(Debug, Subcommand)]
enum TelemetryCommand {
    /// Show whether zeaburctl ui sends events, and which setting decides it.
    Status,
    /// Leave the experience program: writes `telemetry: false` to config.yaml.
    ///
    /// The random ID stays; `zeaburctl telemetry reset` deletes it.
    Disable,
    /// Join the experience program again: writes `telemetry: true` to config.yaml.
    Enable,
    /// Delete the random ID that zeaburctl ui keeps for the experience program.
    ///
    /// The next event it sends gets a new ID.
    Reset,
}

pub(crate) fn run(args: &TelemetryArgs) -> std::process::ExitCode {
    let Some(directory) = standard_dir() else {
        output::error("cannot find the configuration directory of this computer");
        return std::process::ExitCode::FAILURE;
    };
    let done = match args.command {
        TelemetryCommand::Status => {
            status(&directory);
            Ok(())
        }
        TelemetryCommand::Disable => set(&directory, false),
        TelemetryCommand::Enable => set(&directory, true),
        TelemetryCommand::Reset => reset(&directory),
    };
    match done {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            output::error(&message);
            std::process::ExitCode::FAILURE
        }
    }
}

fn status(directory: &Path) {
    let config = directory.join(CONFIG);
    let var = |name| std::env::var(name).ok();
    let (do_not_track, telemetry) = (var("DO_NOT_TRACK"), var("ZEABUR_TELEMETRY"));
    let read = read_config(directory);
    let joined = matches!(read, Ok(None | Some(true)));
    let switch = Switch::resolve(joined, do_not_track.as_deref(), telemetry.as_deref());
    let state = match switch {
        Switch::Off => "off: zeaburctl ui sends nothing",
        Switch::Debug => "debug: zeaburctl ui prints each event and sends nothing",
        Switch::On if PROJECT_KEY.is_empty() => "on, but this build has no key and sends nothing",
        Switch::On => "on: zeaburctl ui sends its events",
    };
    let file = match &read {
        Ok(None) => "not set, so the program is joined".to_owned(),
        Ok(Some(joined)) => format!("telemetry: {joined}"),
        Err(error) => format!("cannot be read ({error}), so nothing is sent"),
    };
    let shown = |value: Option<String>| value.unwrap_or_else(|| "not set".to_owned());
    let id = if LocalId::new(directory, random).peek().is_some() {
        "kept; `zeaburctl telemetry reset` deletes it"
    } else {
        "none yet"
    };
    output::info(&format!(
        "Experience program: {state}\n\
         {}: {file}\n\
         DO_NOT_TRACK: {}\n\
         ZEABUR_TELEMETRY: {}\n\
         ID in {}: {id}",
        config.display(),
        shown(do_not_track),
        shown(telemetry),
        directory.join("local-id").display(),
    ));
}

fn set(directory: &Path, joined: bool) -> Result<(), String> {
    let path = directory.join(CONFIG);
    let failed =
        |error: &dyn std::fmt::Display| format!("cannot write {}: {error}", path.display());
    read_config(directory).map_err(|error| {
        format!("{error}. Fix or delete the file, then run this command again.")
    })?;
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(failed(&error)),
    };
    let changed = with_telemetry(&text, joined);
    if parse(&changed) != Ok(Some(joined)) {
        return Err(format!(
            "cannot change {} in its present form. Write `telemetry: {joined}` in it by hand.",
            path.display()
        ));
    }
    std::fs::create_dir_all(directory).map_err(|e| failed(&e))?;
    std::fs::write(&path, changed).map_err(|e| failed(&e))?;
    if !joined {
        output::info("You left the experience program. zeaburctl ui sends nothing.");
        if LocalId::new(directory, random).peek().is_some() {
            output::info("Your ID stays until you run `zeaburctl telemetry reset`.");
        }
    } else if Switch::from_env(true) == Switch::Off {
        output::info(
            "You joined the experience program, but DO_NOT_TRACK or ZEABUR_TELEMETRY=off in this \
             environment still turns it off.",
        );
    } else {
        output::info("You joined the experience program. zeaburctl ui sends its events again.");
    }
    Ok(())
}

/// `text` with its top-level `telemetry:` line set to `joined`, or with that line added. The
/// other lines, comments too, stay as they are.
fn with_telemetry(text: &str, joined: bool) -> String {
    let line = format!("telemetry: {joined}");
    let mut found = false;
    let mut lines = Vec::new();
    for current in text.lines() {
        let key = current
            .strip_prefix("telemetry")
            .is_some_and(|rest| rest.trim_start().starts_with(':'));
        if key && !found {
            found = true;
            lines.push(line.clone());
        } else {
            lines.push(current.to_owned());
        }
    }
    if !found {
        lines.push(line);
    }
    lines.join("\n") + "\n"
}

fn reset(directory: &Path) -> Result<(), String> {
    let had = LocalId::new(directory, random).peek().is_some();
    LocalId::reset(directory).map_err(|error| format!("cannot delete the ID: {error}"))?;
    output::info(if had {
        "Deleted your experience program ID. zeaburctl ui makes a new one the next time it is about to send an event."
    } else {
        "There is no experience program ID to delete."
    });
    Ok(())
}

/// The program for `zeaburctl ui`. When it is on, says so and how to turn it off.
pub(crate) fn local() -> Local {
    let show: Show = Arc::new(|body| output::info(&format!("telemetry: {body}")));
    let local = Local::from_env(Client::Zeaburctl, show).unwrap_or_else(|error| {
        output::error(&format!(
            "{error}. zeaburctl ui sends nothing until it is fixed."
        ));
        Local::default()
    });
    if local.is_on() {
        output::info(
            "zeaburctl ui sends its version, whether it could connect to the cluster and which \
             features you use, to help \
             improve Zeabur. `zeaburctl telemetry status` shows the setting; `zeaburctl telemetry \
             disable` or DO_NOT_TRACK=1 turns it off.\n\
             What is sent: https://oss.zeabur.com/docs/zeabur/reference/telemetry/",
        );
    }
    local
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_and_disable_change_only_the_telemetry_line() {
        let directory = tempfile::tempdir().unwrap();
        set(directory.path(), false).unwrap();
        assert_eq!(read_config(directory.path()), Ok(Some(false)));
        std::fs::write(
            directory.path().join(CONFIG),
            "# mine\nother: 1\ntelemetry: false\n",
        )
        .unwrap();
        set(directory.path(), true).unwrap();
        assert_eq!(
            std::fs::read_to_string(directory.path().join(CONFIG)).unwrap(),
            "# mine\nother: 1\ntelemetry: true\n"
        );
        std::fs::write(directory.path().join(CONFIG), "telemetry: [").unwrap();
        assert!(set(directory.path(), false).is_err());
        std::fs::write(directory.path().join(CONFIG), "{telemetry: true}\n").unwrap();
        assert!(set(directory.path(), false).is_err());
        assert_eq!(
            std::fs::read_to_string(directory.path().join(CONFIG)).unwrap(),
            "{telemetry: true}\n"
        );
    }
}
