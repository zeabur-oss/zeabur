//! Reading the configuration and the catalog.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use zeabur_installer_host::{download, facts};
use zeabur_installer_plan::catalog::Index;
use zeabur_installer_plan::config::{Config, Document, JoinConfig, PersonalConfig};
use zeabur_installer_plan::{PlanError, pins};

use crate::InstallError;

/// Reads a configuration file of either kind. A join configuration holding its token must not be
/// readable by others.
pub(crate) fn document(path: &Path) -> Result<Document, InstallError> {
    let document = Document::parse(&read(path)?)?;
    if let Document::Join(JoinConfig { join, .. }) = &document
        && join.token.is_some()
    {
        let mode = fs::metadata(path)
            .map_err(|e| InstallError::Message(format!("{}: {e}", path.display())))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(InstallError::Message(format!(
                "{} holds a join token and others can read it (mode {:o}); chmod 0600 {0}",
                path.display(),
                mode & 0o777
            )));
        }
    }
    Ok(document)
}

/// The join configuration at `path`, if that is what it holds.
pub(crate) fn join(path: &Path) -> Option<JoinConfig> {
    if !path.exists() {
        return None;
    }
    match document(path) {
        Ok(Document::Join(config)) => Some(config),
        Ok(Document::Install(_) | Document::Personal(_)) | Err(_) => None,
    }
}

/// The personal configuration at `path`, if that is what it holds.
pub(crate) fn personal(path: &Path) -> Option<PersonalConfig> {
    if !path.exists() {
        return None;
    }
    match document(path) {
        Ok(Document::Personal(config)) => Some(config),
        Ok(Document::Install(_) | Document::Join(_)) | Err(_) => None,
    }
}

fn read(path: &Path) -> Result<String, InstallError> {
    fs::read_to_string(path).map_err(|e| {
        InstallError::Message(format!(
            "{}: {e}; see https://oss.zeabur.com/docs/zeabur/reference/installer-configuration/",
            path.display()
        ))
    })
}

/// Reads the configuration file, or gives the defaults (`Config::defaults`) when there is none,
/// saying so.
pub(crate) fn config_or_defaults(path: &Path) -> Result<(Config, bool), InstallError> {
    if path.exists() {
        return Ok((config(path)?, false));
    }
    let config = Config::defaults(&facts::addresses()).ok_or_else(|| {
        InstallError::Message(format!(
            "{} does not exist and the server has no global address to make sslip.io names \
             from; see https://oss.zeabur.com/docs/zeabur/reference/installer-configuration/",
            path.display()
        ))
    })?;
    Ok((config, true))
}

/// Reads the configuration file.
pub(crate) fn config(path: &Path) -> Result<Config, InstallError> {
    match document(path)? {
        Document::Install(config) => Ok(config),
        Document::Join(_) => Err(InstallError::Message(format!(
            "{} joins this server to a cluster; only plan, apply, doctor and uninstall read it",
            path.display()
        ))),
        Document::Personal(_) => Err(InstallError::Message(format!(
            "{} is a personal install; only plan, apply, doctor and uninstall read it",
            path.display()
        ))),
    }
}

/// Reads the catalog index: `sources.catalog`, or the pinned release checked against its digest
/// and kept in `state_dir`.
pub(crate) fn catalog(source: Option<&str>, state_dir: &str) -> Result<Index, InstallError> {
    let bytes = match (source, pins::PRODUCTS) {
        (Some(path), _) => {
            fs::read(path).map_err(|e| InstallError::Message(format!("{path}: {e}")))?
        }
        (None, Some(products)) => {
            let index = products.catalog.index();
            let dest = Path::new(state_dir).join("index.json");
            download::fetch(&download::agent(), &index.url, &dest, Some(index.sha256))?;
            fs::read(&dest)
                .map_err(|e| InstallError::Message(format!("{}: {e}", dest.display())))?
        }
        (None, None) => return Err(PlanError::Unpublished(vec!["catalog"]).into()),
    };
    Ok(Index::parse(&bytes)?)
}
