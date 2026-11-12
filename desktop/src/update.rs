//! Updates from the `latest.json` of the newest Zeabur release (`plugins.updater` in
//! `tauri.conf.json`). The app checks once on start and again from the menu; it installs only when
//! the person asks.

use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use tauri::utils::config::BundleType;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{chrome, output};

/// The newest release's page, for the copies the updater cannot replace.
const RELEASES: &str = "https://github.com/zeabur-oss/zeabur/releases/latest";

/// How this copy of the app takes a new version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Install {
    /// The updater replaces the app: the macOS app, an installed Windows app, an `AppImage`.
    InPlace,
    /// A `.deb` or `.rpm`, which the package manager updates.
    Package,
    /// The portable Windows `.exe` or a build from source: the person downloads the new version.
    Download,
}

/// The bundler marks each package's binary with the package's type.
fn install_kind() -> Install {
    if cfg!(debug_assertions) {
        return Install::Download;
    }
    match tauri::utils::platform::bundle_type() {
        Some(
            BundleType::App
            | BundleType::Dmg
            | BundleType::AppImage
            | BundleType::Nsis
            | BundleType::Msi,
        ) => Install::InPlace,
        Some(BundleType::Deb | BundleType::Rpm) => Install::Package,
        None => Install::Download,
    }
}

/// A newer release, as the chooser shows it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Available {
    version: String,
    current: String,
    /// The release notes, Markdown.
    notes: Option<String>,
    install: Install,
}

fn available(update: &Update) -> Available {
    Available {
        version: update.version.clone(),
        current: update.current_version.clone(),
        notes: update.body.clone(),
        install: install_kind(),
    }
}

/// The release the last check found.
#[derive(Default)]
pub(crate) struct Updates {
    found: Mutex<Option<Update>>,
}

impl std::fmt::Debug for Updates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Updates").finish_non_exhaustive()
    }
}

impl Updates {
    fn found(&self) -> std::sync::MutexGuard<'_, Option<Update>> {
        self.found.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A build without the updater's public key (a local build) never checks.
fn configured(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|key| !key.is_empty())
}

async fn check(app: &AppHandle) -> Result<Option<Available>, String> {
    if !configured(app) {
        return Err("not-configured".into());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let found = update.as_ref().map(available);
    *app.state::<Updates>().found() = update;
    Ok(found)
}

/// The quiet check on start: a newer release goes to the chooser as `update-available`, and a
/// failure only to the terminal.
pub(crate) fn check_on_start(app: &AppHandle) {
    if !configured(app) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match check(&app).await {
            Ok(Some(found)) => {
                if let Err(err) = app.emit_to(chrome::CHOOSER, "update-available", found) {
                    output::warn(&format!("cannot tell the chooser about the update: {err}"));
                }
            }
            Ok(None) => {}
            Err(err) => output::warn(&format!("cannot check for updates: {err}")),
        }
    });
}

/// The menu's Check for Updates: the chooser checks and shows the result.
#[cfg(target_os = "macos")]
pub(crate) fn check_from_menu(app: &AppHandle) {
    if let Err(err) =
        chrome::show_chooser(app).and_then(|()| app.emit_to(chrome::CHOOSER, "check-updates", ()))
    {
        output::warn(&format!("cannot check for updates: {err}"));
    }
}

/// The release the quiet check found, for a chooser that opened after it.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri passes a command's arguments by value"
)]
pub(crate) fn found_update(updates: State<'_, Updates>) -> Option<Available> {
    updates.found().as_ref().map(available)
}

/// Checks now; `Ok(None)` when this is the newest release.
#[tauri::command]
pub(crate) async fn check_update(app: AppHandle) -> Result<Option<Available>, String> {
    check(&app).await
}

/// Downloads the release the last check found, checks its signature, installs it and starts the
/// new version. On Windows the installer closes the app and starts it again.
#[tauri::command]
pub(crate) async fn install_update(app: AppHandle) -> Result<(), String> {
    if install_kind() != Install::InPlace {
        return Err("this copy of Zeabur cannot replace itself".into());
    }
    let update = app
        .state::<Updates>()
        .found()
        .take()
        .ok_or("no update to install")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart()
}

/// Opens the newest release's page in the browser.
#[tauri::command]
pub(crate) fn open_releases() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(RELEASES)
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}
