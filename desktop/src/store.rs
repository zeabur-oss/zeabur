//! Files the app keeps: `settings.json` in Tauri's `app_config_dir` and pasted kubeconfigs in
//! `app_local_data_dir/kubeconfigs` (<https://docs.rs/tauri/latest/tauri/path/struct.PathResolver.html>).
//! Pasted kubeconfigs hold credentials, so they stay in the local folder, not the roaming one on
//! Windows, and on Unix only the owner can read them.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Writes `bytes` to `dir/name`. On Unix the folder is `0700` and the file `0600`, also when they
/// already existed with wider modes.
pub fn write_private(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    let path = dir.join(name);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        if path.exists() {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    options.open(&path)?.write_all(bytes)?;
    Ok(path)
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn pasted_files_are_readable_by_the_owner_only() {
        let dir = std::env::temp_dir()
            .join(format!("zeabur-desktop-store-{}", std::process::id()))
            .join("kubeconfigs");
        let path = write_private(&dir, "team.yaml", b"first").unwrap();
        assert_eq!((mode(&dir), mode(&path)), (0o700, 0o600));

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&dir, "team.yaml", b"second").unwrap();
        assert_eq!((mode(&dir), mode(&path)), (0o700, 0o600));
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
    }
}
