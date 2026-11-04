//! Reading the configuration and the catalog.

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::Path;

use zeabur_installer_host::{download, facts};
use zeabur_installer_plan::catalog::Index;
use zeabur_installer_plan::config::{Config, Document, JoinConfig, PersonalConfig};
use zeabur_installer_plan::rename_inputs::rename_input_keys;
use zeabur_installer_plan::{PlanError, pins};

use crate::{InstallError, out};

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

/// Writes the catalog's new input names into the configuration file at `path`, or says which keys
/// to rename when that cannot be done safely. `apply` and `gui` call it once, before reading the
/// file; every read uses the new names either way. A file that does not read as a configuration
/// is left alone, for the command to report.
pub(crate) fn rename_inputs(path: &Path) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    if Document::parse(&text).is_err() {
        return;
    }
    let rewrite = rename_input_keys(&text);
    let shown = path.display();
    let mut left = rewrite.left;
    if let Some(new_text) = &rewrite.text {
        match write_renamed(path, new_text) {
            Ok(()) => {
                for r in &rewrite.done {
                    out::err(&format!(
                        "{shown}: renamed input {} to {} ({})",
                        r.old, r.new, r.component
                    ));
                }
            }
            Err(e) => {
                out::err(&format!(
                    "{shown}: could not write the new input names: {e}"
                ));
                left.extend(rewrite.done);
            }
        }
    }
    for r in left {
        out::err(&format!(
            "{shown}: {r}; this run uses the new name, rename it in the file"
        ));
    }
}

/// Replaces the file behind `path` with `text` through a new temporary file in its directory,
/// keeping its mode and owner. When the owner cannot be kept, the file is left as it is.
fn write_renamed(path: &Path, text: &str) -> Result<(), String> {
    let target = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let meta = fs::metadata(&target).map_err(|e| e.to_string())?;
    let mode = meta.permissions().mode() & 0o7777;
    if mode & 0o222 == 0 {
        return Err("the file is read-only".to_owned());
    }
    let dir = target.parent().unwrap_or_else(|| Path::new("/"));
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    std::os::unix::fs::fchown(temp.as_file(), Some(meta.uid()), Some(meta.gid()))
        .map_err(|e| format!("its owner cannot be kept: {e}"))?;
    temp.as_file()
        .set_permissions(fs::Permissions::from_mode(mode))
        .map_err(|e| e.to_string())?;
    temp.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(&target).map_err(|e| e.error.to_string())?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;

    use super::{document, rename_inputs};

    const BASE: &str = "# my server\naccess: {mode: domain, identity: login.example.com, zeabur: zeabur.example.com}\n";

    fn write(dir: &Path, text: &str, mode: u32) -> std::path::PathBuf {
        let path = dir.join("zeabur.yaml");
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    #[test]
    fn block_style_is_renamed_and_comments_stay() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!(
            "{BASE}inputs:\n  # backups\n  volume-backup/velero:\n    bucket: b # where\n    s3-url: https://s3.example.com # s3-url\n    note: s3-url\n"
        );
        let path = write(dir.path(), &text, 0o600);
        rename_inputs(&path);
        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(after, text.replace("    s3-url: https", "    s3Url: https"));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn flow_style_is_renamed_in_both_sections_and_comments_stay() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!(
            "{BASE}inputs:\n  volume-backup/velero: {{s3-url: x, bucket: b}} # keep\nsecretFiles:\n  intrusion-prevention/crowdsec: {{\"bouncer-key\": /etc/key}}\n"
        );
        let path = write(dir.path(), &text, 0o640);
        rename_inputs(&path);
        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(
            after,
            text.replace("{s3-url:", "{s3Url:")
                .replace("\"bouncer-key\"", "\"bouncerKey\"")
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn a_second_read_finds_nothing_to_rename() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!("{BASE}inputs:\n  volume-backup/velero:\n    s3-url: x\n");
        let path = write(dir.path(), &text, 0o600);
        rename_inputs(&path);
        let first = fs::read_to_string(&path).unwrap();
        let again = zeabur_installer_plan::rename_inputs::rename_input_keys(&first);
        assert!(again.done.is_empty() && again.left.is_empty() && again.text.is_none());
        rename_inputs(&path);
        assert_eq!(fs::read_to_string(&path).unwrap(), first);
    }

    #[test]
    fn reading_the_file_leaves_it_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!("{BASE}inputs:\n  volume-backup/velero:\n    s3-url: x\n");
        let path = write(dir.path(), &text, 0o600);
        let zeabur_installer_plan::config::Document::Install(config) = document(&path).unwrap()
        else {
            panic!("not an install configuration");
        };
        assert_eq!(config.inputs["volume-backup/velero"]["s3Url"], "x");
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn the_rewrite_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!("{BASE}inputs:\n  volume-backup/velero:\n    s3-url: x\n");
        let path = write(dir.path(), &text, 0o600);
        rename_inputs(&path);
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["zeabur.yaml"]);
    }

    #[test]
    fn a_value_equal_to_an_old_name_is_not_changed() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!(
            "{BASE}inputs:\n  volume-backup/velero:\n    bucket: s3-url\n    s3-url: s3-url\n"
        );
        let path = write(dir.path(), &text, 0o600);
        rename_inputs(&path);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            text.replace("    s3-url: s3-url", "    s3Url: s3-url")
        );
    }

    #[test]
    fn the_same_name_in_another_component_is_not_changed() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!(
            "{BASE}inputs:\n  other/thing:\n    s3-url: a\n  volume-backup/velero:\n    s3-url: b\n"
        );
        let path = write(dir.path(), &text, 0o600);
        rename_inputs(&path);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            text.replace("    s3-url: b", "    s3Url: b")
        );
    }

    #[test]
    fn an_unsafe_case_leaves_the_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        for body in [
            "inputs:\n  volume-backup/velero:\n    s3-url: a\n    s3Url: b\n",
            "inputs:\n  volume-backup/velero: {s3-url: a, s3Url: b}\n",
            "inputs:\n  volume-backup/velero: {s3-url: a,\n    bucket: b}\n",
            "inputs: {volume-backup/velero: {s3-url: a}}\n",
            "inputs:\n  other/x: &v {s3-url: a}\n  volume-backup/velero: *v\n",
        ] {
            let text = format!("{BASE}{body}");
            let path = write(dir.path(), &text, 0o600);
            rename_inputs(&path);
            assert_eq!(fs::read_to_string(&path).unwrap(), text, "{body}");
        }
    }

    #[test]
    fn a_read_only_file_is_not_written_and_the_run_goes_on() {
        let dir = tempfile::tempdir().unwrap();
        let text = format!("{BASE}inputs:\n  volume-backup/velero:\n    s3-url: x\n");
        let path = write(dir.path(), &text, 0o400);
        rename_inputs(&path);
        let doc = document(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        let zeabur_installer_plan::config::Document::Install(config) = doc else {
            panic!("not an install configuration");
        };
        assert_eq!(config.inputs["volume-backup/velero"]["s3Url"], "x");
    }
}
