//! Updates from the `latest.json` of the newest Zeabur release that this build's channel takes: a
//! release build takes releases, a prerelease build takes prereleases too. The app checks once on
//! start and again when the person asks; it installs only when the person asks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::utils::config::BundleType;
use tauri::{AppHandle, Emitter, Manager, State, Url};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{chrome, output};

/// The newest release's page, when no check has found a release.
const RELEASES: &str = "https://github.com/zeabur-oss/zeabur/releases/latest";

/// The newest releases, prereleases included; `/releases/latest` skips prereleases.
const RELEASES_API: &str = "https://api.github.com/repos/zeabur-oss/zeabur/releases?per_page=30";

/// The update manifest each release carries (scripts/desktop-release.ts).
const MANIFEST: &str = "latest.json";

/// A release as the GitHub REST API lists it.
#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: Url,
}

/// The manifest of the newest release newer than `current` that `current`'s channel takes, or
/// `None` when there is no such release.
fn newer_manifest(releases: &[Release], current: &Version) -> Option<Url> {
    let prerelease_channel = !current.pre.is_empty();
    releases
        .iter()
        .filter(|release| prerelease_channel || !release.prerelease)
        .filter_map(|release| {
            let version = Version::parse(release.tag_name.strip_prefix('v')?).ok()?;
            let manifest = release.assets.iter().find(|asset| asset.name == MANIFEST)?;
            (version > *current).then_some((version, &manifest.browser_download_url))
        })
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, url)| url.clone())
}

/// Why a check or an install did not finish. The details go to the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Failure {
    /// A build without the updater's public key.
    NotConfigured,
    /// GitHub, the network or the release did not answer as expected.
    Failed,
}

/// What went wrong, for the terminal.
#[derive(Debug, thiserror::Error)]
enum UpdateError {
    #[error("cannot list the releases: {0}")]
    Releases(#[from] reqwest::Error),
    #[error(transparent)]
    Updater(#[from] tauri_plugin_updater::Error),
    #[error("this copy of Zeabur cannot replace itself")]
    CannotReplace,
    #[error("no update to install")]
    NothingFound,
}

impl From<UpdateError> for Failure {
    fn from(err: UpdateError) -> Self {
        output::warn(&format!("update: {err}"));
        Self::Failed
    }
}

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
    install: Install,
}

fn available(update: &Update) -> Available {
    Available {
        version: update.version.clone(),
        current: update.current_version.clone(),
        install: install_kind(),
    }
}

/// The release the last check found.
#[derive(Default)]
pub(crate) struct Updates {
    found: Mutex<Option<Update>>,
    /// A check the menu asked for that the chooser has not taken up yet.
    requested: AtomicBool,
}

impl std::fmt::Debug for Updates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Updates").finish_non_exhaustive()
    }
}

impl Updates {
    /// Only the macOS menu asks for a check.
    #[cfg(any(target_os = "macos", test))]
    fn request(&self) {
        self.requested.store(true, Ordering::SeqCst);
    }

    fn take_request(&self) -> bool {
        self.requested.swap(false, Ordering::SeqCst)
    }

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
        .and_then(|updater| tauri_plugin_updater::Config::deserialize(updater).ok())
        .is_some_and(|config| !config.pubkey.is_empty())
}

async fn releases() -> Result<Vec<Release>, reqwest::Error> {
    reqwest::Client::builder()
        .user_agent(concat!("zeabur-desktop/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

async fn find(app: &AppHandle) -> Result<Option<Update>, UpdateError> {
    let releases = releases().await?;
    let Some(manifest) = newer_manifest(&releases, &app.package_info().version) else {
        return Ok(None);
    };
    app.updater_builder()
        .endpoints(vec![manifest])
        .and_then(tauri_plugin_updater::UpdaterBuilder::build)?
        .check()
        .await
        .map_err(UpdateError::from)
}

async fn check(app: &AppHandle) -> Result<Option<Available>, Failure> {
    if !configured(app) {
        return Err(Failure::NotConfigured);
    }
    let update = find(app).await?;
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
        if let Ok(Some(found)) = check(&app).await
            && let Err(err) = app.emit_to(chrome::CHOOSER, "update-available", found)
        {
            output::warn(&format!("cannot tell the chooser about the update: {err}"));
        }
    });
}

/// The menu's Check for Updates: the chooser checks and shows the result. The request waits in
/// `Updates` until the chooser takes it, since a chooser opened just now has no listener yet.
#[cfg(target_os = "macos")]
pub(crate) fn check_from_menu(app: &AppHandle) {
    app.state::<Updates>().request();
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

/// Whether the menu asked for a check since the chooser last asked.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri passes a command's arguments by value"
)]
pub(crate) fn take_check_request(updates: State<'_, Updates>) -> bool {
    updates.take_request()
}

/// Checks now; `Ok(None)` when this is the newest release.
#[tauri::command]
pub(crate) async fn check_update(app: AppHandle) -> Result<Option<Available>, Failure> {
    check(&app).await
}

/// Downloads the release the last check found, checks its signature, installs it and starts the
/// new version. On Windows the installer closes the app and starts it again. A failed install keeps
/// the release, so the button can try again.
#[tauri::command]
pub(crate) async fn install_update(app: AppHandle) -> Result<(), Failure> {
    if install_kind() != Install::InPlace {
        return Err(UpdateError::CannotReplace.into());
    }
    let update = app
        .state::<Updates>()
        .found()
        .clone()
        .ok_or(UpdateError::NothingFound)?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(UpdateError::from)?;
    app.restart()
}

/// The page of the release the last check found, with its notes, or the newest release's.
fn release_page(found: Option<&Update>) -> String {
    found
        .and_then(|update| Version::parse(&update.version).ok())
        .map_or_else(
            || RELEASES.to_owned(),
            |version| format!("https://github.com/zeabur-oss/zeabur/releases/tag/v{version}"),
        )
}

/// Opens the found release's page in the browser.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri passes a command's arguments by value"
)]
pub(crate) fn open_releases(updates: State<'_, Updates>) -> Result<(), String> {
    let page = release_page(updates.found().as_ref());
    crate::links::open_in_browser(&page).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use semver::Version;

    use super::{Asset, MANIFEST, Release, Updates, newer_manifest};

    fn release(tag: &str, prerelease: bool, manifest: bool) -> Release {
        let asset = |name: &str| Asset {
            name: name.to_owned(),
            browser_download_url: format!("https://example.com/{tag}/{name}")
                .parse()
                .unwrap_or_else(|err| unreachable!("{err}")),
        };
        let mut assets = vec![asset("SHA256SUMS.sigstore.json")];
        if manifest {
            assets.push(asset(MANIFEST));
        }
        Release {
            tag_name: tag.to_owned(),
            prerelease,
            assets,
        }
    }

    /// GitHub's list today, newest first: v0.2.0 is the newest release, without a manifest.
    fn listed() -> Vec<Release> {
        vec![
            release("v0.3.0-beta.2", true, true),
            release("v0.3.0-beta.1", true, true),
            release("v0.2.0", false, false),
            release("v0.2.0-beta.3", true, true),
        ]
    }

    fn manifest_for(releases: &[Release], current: &str) -> Option<String> {
        let current = Version::parse(current).unwrap_or_else(|err| unreachable!("{err}"));
        newer_manifest(releases, &current).map(|url| url.to_string())
    }

    #[test]
    fn a_prerelease_build_takes_the_newest_prerelease() {
        assert_eq!(
            manifest_for(&listed(), "0.3.0-beta.1").as_deref(),
            Some("https://example.com/v0.3.0-beta.2/latest.json"),
        );
    }

    #[test]
    fn a_prerelease_build_takes_the_release_it_leads_to() {
        let mut releases = listed();
        releases.insert(0, release("v0.3.0", false, true));
        assert_eq!(
            manifest_for(&releases, "0.3.0-beta.2").as_deref(),
            Some("https://example.com/v0.3.0/latest.json"),
        );
    }

    #[test]
    fn a_release_build_skips_prereleases() {
        assert_eq!(manifest_for(&listed(), "0.2.0"), None);
        let mut releases = listed();
        releases.insert(0, release("v0.3.0", false, true));
        assert_eq!(
            manifest_for(&releases, "0.2.0").as_deref(),
            Some("https://example.com/v0.3.0/latest.json"),
        );
    }

    #[test]
    fn the_newest_build_has_nothing_to_take() {
        assert_eq!(manifest_for(&listed(), "0.3.0-beta.2"), None);
    }

    #[test]
    fn a_release_without_a_manifest_is_skipped() {
        let releases = vec![
            release("v0.4.0-beta.1", true, false),
            release("v0.3.0", false, true),
        ];
        assert_eq!(
            manifest_for(&releases, "0.3.0-beta.1").as_deref(),
            Some("https://example.com/v0.3.0/latest.json"),
        );
    }

    #[test]
    fn a_menu_request_waits_until_the_chooser_takes_it() {
        let updates = Updates::default();
        assert!(!updates.take_request());
        updates.request();
        assert!(updates.take_request());
        assert!(!updates.take_request());
    }
}
