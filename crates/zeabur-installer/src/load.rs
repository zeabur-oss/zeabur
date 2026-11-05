//! Reading the configuration and the catalog.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use zeabur_installer_host::{download, facts};
use zeabur_installer_plan::catalog::Index;
use zeabur_installer_plan::config::{Config, Document, JoinConfig, PersonalConfig, Sources};
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

/// Checks the paths `plan` and `apply` take from `sources`, so a wrong one stops `plan` before
/// the host changes: each is a file, and `zeaburChart` a file or a directory.
pub(crate) fn sources(sources: &Sources) -> Result<(), InstallError> {
    if let Some(path) = &sources.identityctl {
        file("identityctl", path)?;
    }
    if let Some(path) = &sources.zeabur_chart {
        fs::metadata(path)
            .map_err(|e| InstallError::Message(format!("sources.zeaburChart: {path}: {e}")))?;
    }
    image_archives(&sources.image_archives)
}

/// Checks that each of `sources.imageArchives` is a file.
pub(crate) fn image_archives(paths: &[String]) -> Result<(), InstallError> {
    for (i, path) in paths.iter().enumerate() {
        file(&format!("imageArchives[{i}]"), path)?;
    }
    Ok(())
}

fn file(key: &str, path: &str) -> Result<(), InstallError> {
    let meta = fs::metadata(path)
        .map_err(|e| InstallError::Message(format!("sources.{key}: {path}: {e}")))?;
    if !meta.is_file() {
        return Err(InstallError::Message(format!(
            "sources.{key}: {path} is not a file"
        )));
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
        (Some(path), _) => fs::read(path)
            .map_err(|e| InstallError::Message(format!("sources.catalog: {path}: {e}")))?,
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

    fn error(result: Result<impl Sized, InstallError>) -> String {
        match result {
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
        let at = |key: &str, path: &str| {
            let mut sources = Sources::default();
            match key {
                "identityctl" => sources.identityctl = Some(path.to_owned()),
                "zeaburChart" => sources.zeabur_chart = Some(path.to_owned()),
                _ => sources.image_archives = vec![path.to_owned()],
            }
            sources
        };

        for key in ["identityctl", "imageArchives[0]"] {
            let missing_error = error(sources(&at(key, &missing)));
            assert!(
                missing_error.starts_with(&format!("sources.{key}: {missing}:")),
                "{missing_error}"
            );
            assert_eq!(
                error(sources(&at(key, &folder))),
                format!("sources.{key}: {folder} is not a file")
            );
            assert_eq!(error(sources(&at(key, &file))), "");
        }

        let chart_error = error(sources(&at("zeaburChart", &missing)));
        assert!(
            chart_error.starts_with(&format!("sources.zeaburChart: {missing}:")),
            "{chart_error}"
        );
        assert_eq!(error(sources(&at("zeaburChart", &folder))), "");
        assert_eq!(error(sources(&at("zeaburChart", &file))), "");

        let catalog_error = error(catalog(Some(&missing), &folder));
        assert!(
            catalog_error.starts_with(&format!("sources.catalog: {missing}:")),
            "{catalog_error}"
        );
    }

    #[test]
    fn a_missing_source_still_tells_what_kind_of_node_this_is() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            "join:\n  server: https://10.0.0.5:6443\n  tokenFile: /root/token\n\
             sources: {imageArchives: [/gone.tar]}\n",
        )
        .unwrap();
        assert!(join(&path).is_some());
    }
}
