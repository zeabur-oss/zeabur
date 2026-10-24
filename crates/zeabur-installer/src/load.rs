//! Reading the configuration and the catalog.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use zeabur_installer_host::{download, facts};
use zeabur_installer_plan::catalog::Index;
use zeabur_installer_plan::config::{Config, Document, JoinConfig, Sources};
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
        Ok(Document::Install(_)) | Err(_) => None,
    }
}

fn read(path: &Path) -> Result<String, InstallError> {
    fs::read_to_string(path).map_err(|e| {
        InstallError::Message(format!(
            "{}: {e}; see docs/installer/configuration.md",
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
    let address = facts::addresses().into_iter().next().ok_or_else(|| {
        InstallError::Message(format!(
            "{} does not exist and the server has no global address to make sslip.io names \
             from; see docs/installer/configuration.md",
            path.display()
        ))
    })?;
    Ok((Config::defaults(&address), true))
}

/// Reads the configuration file.
pub(crate) fn config(path: &Path) -> Result<Config, InstallError> {
    match document(path)? {
        Document::Install(config) => Ok(config),
        Document::Join(_) => Err(InstallError::Message(format!(
            "{} joins this server to a cluster; only plan, apply, doctor and uninstall read it",
            path.display()
        ))),
    }
}

/// Reads the catalog index: `sources.catalog`, or the pinned release checked against its digest.
pub(crate) fn catalog(sources: &Sources) -> Result<Index, InstallError> {
    let bytes = match (&sources.catalog, pins::PRODUCTS) {
        (Some(path), _) => {
            fs::read(path).map_err(|e| InstallError::Message(format!("{path}: {e}")))?
        }
        (None, Some(products)) => {
            let index = products.catalog.index();
            let dest = Path::new(crate::apply::STATE_DIR).join("index.json");
            download::fetch(&download::agent(), &index.url, &dest, Some(index.sha256))?;
            fs::read(&dest)
                .map_err(|e| InstallError::Message(format!("{}: {e}", dest.display())))?
        }
        (None, None) => return Err(PlanError::Unpublished(vec!["catalog"]).into()),
    };
    Ok(Index::parse(&bytes)?)
}
