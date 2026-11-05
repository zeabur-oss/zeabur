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
    sources(&document)?;
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

/// Checks that each `sources` path is there: a file, or for `zeaburChart` a directory too.
fn sources(document: &Document) -> Result<(), InstallError> {
    let mut files: Vec<(String, &str)> = Vec::new();
    let mut chart = None;
    let archives = match document {
        Document::Install(config) => {
            let s = &config.sources;
            for (key, path) in [("catalog", &s.catalog), ("identityctl", &s.identityctl)] {
                if let Some(path) = path {
                    files.push((key.to_owned(), path));
                }
            }
            chart = s.zeabur_chart.as_deref();
            &s.image_archives
        }
        Document::Join(config) => &config.sources.image_archives,
        Document::Personal(config) => {
            if let Some(path) = &config.sources.catalog {
                files.push(("catalog".to_owned(), path));
            }
            return check_files(&files);
        }
    };
    for (i, path) in archives.iter().enumerate() {
        files.push((format!("imageArchives[{i}]"), path));
    }
    if let Some(path) = chart {
        fs::metadata(path)
            .map_err(|e| InstallError::Message(format!("sources.zeaburChart: {path}: {e}")))?;
    }
    check_files(&files)
}

fn check_files(files: &[(String, &str)]) -> Result<(), InstallError> {
    for (key, path) in files {
        let meta = fs::metadata(path)
            .map_err(|e| InstallError::Message(format!("sources.{key}: {path}: {e}")))?;
        if !meta.is_file() {
            return Err(InstallError::Message(format!(
                "sources.{key}: {path} is not a file"
            )));
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_error(dir: &Path, sources: &str) -> String {
        let path = dir.join("config.yaml");
        let yaml = format!(
            "access:\n  mode: local\n  identity: identity.example.com\n  zeabur: zeabur.example.com\nsources:\n{sources}"
        );
        fs::write(&path, yaml).unwrap();
        match document(&path) {
            Ok(_) => String::new(),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn a_missing_or_wrong_kind_of_source_names_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, "").unwrap();
        let file = file.display().to_string();
        let folder = dir.path().display().to_string();
        let missing = dir.path().join("missing").display().to_string();
        for key in ["catalog", "identityctl", "imageArchives"] {
            let value = |path: &str| {
                if key == "imageArchives" {
                    format!("  {key}: [{path}]\n")
                } else {
                    format!("  {key}: {path}\n")
                }
            };
            let shown = if key == "imageArchives" {
                "imageArchives[0]"
            } else {
                key
            };
            let error = plan_error(dir.path(), &value(&missing));
            assert!(
                error.starts_with(&format!("sources.{shown}: {missing}:")),
                "{error}"
            );
            let error = plan_error(dir.path(), &value(&folder));
            assert_eq!(error, format!("sources.{shown}: {folder} is not a file"));
            assert_eq!(plan_error(dir.path(), &value(&file)), "");
        }
        let error = plan_error(dir.path(), &format!("  zeaburChart: {missing}\n"));
        assert!(
            error.starts_with(&format!("sources.zeaburChart: {missing}:")),
            "{error}"
        );
        assert_eq!(
            plan_error(dir.path(), &format!("  zeaburChart: {folder}\n")),
            ""
        );
        assert_eq!(
            plan_error(dir.path(), &format!("  zeaburChart: {file}\n")),
            ""
        );
    }
}
