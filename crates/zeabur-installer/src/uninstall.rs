//! `uninstall`: removes Zeabur, and Zeabur Identity with it when asked. RKE2 and the catalog's
//! components stay; RKE2's own script removes everything. `--purge` then removes what the
//! installer itself left on the machine.

use std::fs;
use std::path::{Path, PathBuf};

use zeabur_installer_host::files;
use zeabur_installer_host::kubectl::Kubectl;
use zeabur_installer_plan::plan::{IDENTITY_NAMESPACE, ZEABUR_NAMESPACE};
use zeabur_installer_plan::rke2::{AUTHN_FILE, EMPTY_AUTHN, PREFIX};
use zeabur_installer_plan::storage::{MODULES_FILE, MULTIPATH_FILE};

use crate::apply::STATE_DIR;
use crate::{InstallError, Product, out};

/// Where local-path volumes live; never removed by the installer.
const VOLUMES_DIR: &str = "/var/lib/zeabur/storage";
/// Where Longhorn keeps its replicas on each node.
const LONGHORN_DIR: &str = "/var/lib/longhorn";

pub(crate) fn run(config: &Path, what: Product) -> Result<(), InstallError> {
    let kubectl = Kubectl::rke2();
    out::step("Removing Zeabur");
    kubectl.run(
        &[
            "delete",
            "--namespace=kube-system",
            "helmchart.helm.cattle.io/zeabur",
            "--ignore-not-found",
            "--wait",
        ],
        None,
    )?;
    kubectl.run(
        &[
            "delete",
            "namespace",
            ZEABUR_NAMESPACE,
            "--ignore-not-found",
            "--wait",
        ],
        None,
    )?;
    if matches!(what, Product::Identity) {
        out::step("Removing Zeabur Identity");
        // Stop trusting its tokens first.
        files::write(Path::new(AUTHN_FILE), 0o600, EMPTY_AUTHN.as_bytes())?;
        kubectl.run(
            &[
                "delete",
                "namespace",
                IDENTITY_NAMESPACE,
                "--ignore-not-found",
                "--wait",
            ],
            None,
        )?;
        out::line(
            "Zeabur Identity's custom resource definitions are kept; they hold no data once its namespace is gone.",
        );
    }
    out::line(&format!(
        "RKE2 and the catalog's components stay; {PREFIX}/bin/rke2-uninstall.sh removes them."
    ));
    let left = leftovers_text(&leftovers(config, None));
    if !left.is_empty() {
        out::line(&left);
    }
    Ok(())
}

/// RKE2 is still installed: `rke2-uninstall.sh` removes its binary.
pub(crate) fn rke2_installed() -> bool {
    Path::new(PREFIX).join("bin/rke2").exists()
}

/// What the installer keeps on the machine after `rke2-uninstall.sh`: its download cache and
/// local CA, the configuration (with a join token on a joined node), a token file it names, and
/// the files it wrote for Longhorn. Packages it installed for Longhorn stay.
pub(crate) fn leftovers(config: &Path, token_file: Option<&str>) -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from(STATE_DIR),
        config.to_owned(),
        PathBuf::from(MODULES_FILE),
        PathBuf::from(MULTIPATH_FILE),
    ];
    paths.extend(token_file.map(PathBuf::from));
    paths.retain(|p| p.exists());
    paths.dedup();
    paths
}

/// The lines naming what is left and how to remove it.
pub(crate) fn leftovers_text(paths: &[PathBuf]) -> String {
    let mut lines = Vec::new();
    if !paths.is_empty() {
        lines.push("After rke2-uninstall.sh, the installer's own files stay:".to_owned());
        for path in paths {
            lines.push(format!("  {}  {}", path.display(), size(path)));
        }
        lines.push(
            "Remove them with `zeabur-installer uninstall --purge` once RKE2 is uninstalled."
                .to_owned(),
        );
    }
    if Path::new(VOLUMES_DIR).exists() {
        lines.push(format!(
            "Volumes of the local-path storage class stay in {VOLUMES_DIR}; --purge keeps them."
        ));
    }
    if Path::new(LONGHORN_DIR).exists() {
        lines.push(format!(
            "Longhorn's replicas stay in {LONGHORN_DIR}; --purge keeps them."
        ));
    }
    lines.join("\n")
}

/// Removes `paths`, and the configuration's directory when that leaves it empty. Refuses while
/// RKE2 is installed: the configuration is how `apply`, `doctor` and `uninstall` know the node.
pub(crate) fn purge(paths: &[PathBuf], config: &Path) -> Result<(), InstallError> {
    if rke2_installed() {
        return Err(InstallError::Message(format!(
            "RKE2 is still installed ({PREFIX}/bin/rke2): run {PREFIX}/bin/rke2-uninstall.sh first"
        )));
    }
    out::step("Removing what the installer left");
    for path in paths {
        let removed = if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        removed.map_err(|e| InstallError::Message(format!("{}: {e}", path.display())))?;
        out::err(&format!("    removed {}", path.display()));
    }
    if let Some(dir) = config.parent()
        && fs::remove_dir(dir).is_ok()
    {
        out::err(&format!("    removed {}", dir.display()));
    }
    if paths.is_empty() {
        out::line("Nothing of the installer's is left.");
    }
    if Path::new(VOLUMES_DIR).exists() {
        out::line(&format!(
            "Volumes of the local-path storage class are kept in {VOLUMES_DIR}."
        ));
    }
    Ok(())
}

/// A path's size on disk, such as `1.2 GiB`.
fn size(path: &Path) -> String {
    fn bytes(path: &Path) -> u64 {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return 0;
        };
        if meta.is_dir() {
            fs::read_dir(path)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|e| bytes(&e.path()))
                .sum()
        } else {
            meta.len()
        }
    }
    human(bytes(path))
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut tenths = bytes * 10 / 1024;
    let mut unit = 0;
    while tenths >= 10 * 1024 && unit + 1 < UNITS.len() {
        tenths /= 1024;
        unit += 1;
    }
    let unit = UNITS.get(unit).unwrap_or(&"TiB");
    format!("{}.{} {unit}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KiB");
        assert_eq!(human(1_572_864_000), "1.4 GiB");
    }

    #[test]
    fn leftovers_name_existing_paths_and_the_purge() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.yaml");
        fs::write(&config, "join: {}\n").unwrap();
        let missing = dir.path().join("token");
        let paths = leftovers(&config, missing.to_str());
        assert!(paths.contains(&config));
        assert!(!paths.contains(&missing));
        let text = leftovers_text(std::slice::from_ref(&config));
        assert!(
            text.contains(&format!("{}  9 B", config.display())),
            "{text}"
        );
        assert!(text.contains("zeabur-installer uninstall --purge"));
    }
}
