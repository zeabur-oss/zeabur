//! The experience program (docs/reference/telemetry.mdx): an install's start, end and failure.

use std::cell::Cell;
use std::fs;
use std::path::Path;

use zeabur_installer_host::{HostError, files};
use zeabur_installer_plan::PlanError;
use zeabur_installer_plan::cluster::Distribution;
use zeabur_installer_plan::config::Config;
use zeabur_telemetry::{Event, Failure, Install, InstallationId, Sender, Switch};

use crate::{InstallError, out};

/// Where the installation ID is kept, next to the configuration file.
const ID_FILE: &str = "installation-id";

/// Reports one `apply`. Does nothing unless the program is on.
pub(crate) struct Reporter {
    on: Option<(Sender, InstallationId)>,
    install: Install,
    step: Cell<zeabur_telemetry::Step>,
}

impl Reporter {
    /// Sends the start of an install. The ID is made at the first one and kept.
    pub(crate) fn start(joined: bool, config_path: &Path, install: Install) -> Self {
        Self::start_with(Switch::from_env(joined), config_path, install, |switch| {
            Sender::new(
                switch,
                std::sync::Arc::new(|body| out::err(&format!("telemetry: {body}"))),
            )
        })
    }

    fn start_with(
        switch: Switch,
        config_path: &Path,
        install: Install,
        sender: impl FnOnce(Switch) -> Sender,
    ) -> Self {
        let on = (switch != Switch::Off)
            .then(|| installation_id(config_path))
            .flatten()
            .map(|id| (sender(switch), id));
        let reporter = Self {
            on,
            install,
            step: Cell::new(zeabur_telemetry::Step::Plan),
        };
        reporter.send(&Event::InstallStarted(reporter.install.clone()));
        reporter
    }

    /// The ID, when the program is on: Zeabur's chart gets it too.
    pub(crate) fn id(&self) -> Option<&InstallationId> {
        self.on.as_ref().map(|(_, id)| id)
    }

    /// Notes that the install reached `step`.
    pub(crate) fn at(&self, step: zeabur_telemetry::Step) {
        self.step.set(step);
    }

    /// Sends how the install ended and hands `result` back.
    pub(crate) fn end<T>(&self, result: Result<T, InstallError>) -> Result<T, InstallError> {
        let install = self.install.clone();
        self.send(&match &result {
            Ok(_) => Event::InstallFinished(install),
            Err(error) => Event::InstallFailed(Failure {
                install,
                step: self.step.get(),
                error: code(error),
            }),
        });
        result
    }

    fn send(&self, event: &Event) {
        if let Some((sender, id)) = &self.on {
            sender.send(id, event);
        }
    }
}

/// The error's kind, from a fixed list.
const fn code(error: &InstallError) -> zeabur_telemetry::ErrorCode {
    use zeabur_telemetry::ErrorCode;
    match error {
        InstallError::Plan(PlanError::Config(_)) => ErrorCode::Config,
        InstallError::Plan(_) => ErrorCode::Plan,
        InstallError::Host(HostError::Io { .. }) => ErrorCode::Io,
        InstallError::Host(HostError::Command { .. }) => ErrorCode::Command,
        InstallError::Host(HostError::Download { .. }) => ErrorCode::Download,
        InstallError::Host(HostError::Digest { .. }) => ErrorCode::Digest,
        InstallError::Host(HostError::Parse { .. }) => ErrorCode::Parse,
        InstallError::Host(_) | InstallError::Message(_) => ErrorCode::Other,
    }
}

/// The ID kept next to the configuration file, made first if there is none. `None` when it can
/// be neither read nor written.
fn installation_id(config_path: &Path) -> Option<InstallationId> {
    let path = config_path.parent()?.join(ID_FILE);
    if let Some(id) = fs::read_to_string(&path)
        .ok()
        .and_then(|text| InstallationId::parse(&text))
    {
        return Some(id);
    }
    let bytes: [u8; 16] = files::random_bytes(16).ok()?.try_into().ok()?;
    let id = InstallationId::from_random(bytes);
    files::write(&path, 0o600, format!("{}\n", id.as_str()).as_bytes()).ok()?;
    Some(id)
}

/// What a team install is.
pub(crate) fn team(config: &Config) -> Install {
    Install {
        installer_version: env!("CARGO_PKG_VERSION"),
        profile: if config.profile == "multi-node" {
            zeabur_telemetry::Profile::MultiNode
        } else {
            zeabur_telemetry::Profile::SingleNode
        },
        distribution: match config.distribution {
            Distribution::K3s => zeabur_telemetry::Distribution::K3s,
            Distribution::Rke2 => zeabur_telemetry::Distribution::Rke2,
        },
    }
}

/// What a personal install is.
pub(crate) const fn personal() -> Install {
    Install {
        installer_version: env!("CARGO_PKG_VERSION"),
        profile: zeabur_telemetry::Profile::Personal,
        distribution: zeabur_telemetry::Distribution::KubeSolo,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn install() -> Install {
        personal()
    }

    fn collecting(shown: &Arc<Mutex<Vec<String>>>) -> impl FnOnce(Switch) -> Sender {
        let shown = Arc::clone(shown);
        move |switch| {
            Sender::to(
                switch,
                "http://127.0.0.1:9/",
                "k",
                Arc::new(move |body| shown.lock().unwrap().push(body.to_owned())),
            )
        }
    }

    fn events(shown: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        shown
            .lock()
            .unwrap()
            .iter()
            .map(|body| {
                let body: serde_json::Value = serde_json::from_str(body).unwrap();
                body["batch"][0]["event"].as_str().unwrap().to_owned()
            })
            .collect()
    }

    #[test]
    fn an_install_reports_its_start_and_how_it_ended() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.yaml");
        let shown = Arc::default();
        let report = Reporter::start_with(Switch::Debug, &config, install(), collecting(&shown));
        report.at(zeabur_telemetry::Step::Cluster);
        let ended = report.end::<()>(Err(InstallError::Message("secret detail".to_owned())));
        assert!(ended.is_err());
        assert_eq!(events(&shown), ["install_started", "install_failed"]);
        let last = shown.lock().unwrap().pop().unwrap();
        assert!(last.contains("\"step\":\"cluster\"") && last.contains("\"error\":\"other\""));
        assert!(!last.contains("secret detail"));

        let shown = Arc::default();
        let report = Reporter::start_with(Switch::Debug, &config, install(), collecting(&shown));
        report.end(Ok(())).unwrap();
        assert_eq!(events(&shown), ["install_started", "install_finished"]);
    }

    #[test]
    fn the_id_is_made_once_and_kept_private() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.yaml");
        let first = installation_id(&config).unwrap();
        assert_eq!(installation_id(&config), Some(first));
        let mode = fs::metadata(dir.path().join(ID_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn off_reports_nothing_and_makes_no_id() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.yaml");
        let shown = Arc::default();
        let report = Reporter::start_with(Switch::Off, &config, install(), collecting(&shown));
        report.end(Ok(())).unwrap();
        assert_eq!(events(&shown), Vec::<String>::new());
        assert!(report.id().is_none());
        assert!(!dir.path().join(ID_FILE).exists());
    }
}
