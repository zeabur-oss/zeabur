//! Files the app keeps: `settings.json` in Tauri's `app_config_dir` and pasted kubeconfigs in
//! `app_local_data_dir/kubeconfigs` (<https://docs.rs/tauri/latest/tauri/path/struct.PathResolver.html>).
//! Pasted kubeconfigs hold credentials, so they stay in the local folder, not the roaming one on
//! Windows, and on Unix only the owner can read them.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Writes `bytes` to a new file `dir/{slug}.yaml`, or `{slug}-2.yaml` and so on when that name is
/// taken; it never replaces a file. On Unix the folder is `0700`, also when it already existed
/// with a wider mode, and the file `0600`.
pub(crate) fn write_new_private(dir: &Path, slug: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut n = 1;
    loop {
        let name = if n == 1 {
            format!("{slug}.yaml")
        } else {
            format!("{slug}-{n}.yaml")
        };
        let path = dir.join(name);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        match options.open(&path) {
            Ok(mut file) => {
                file.write_all(bytes)?;
                return Ok(path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn each_paste_gets_its_own_owner_only_file() {
        let dir = std::env::temp_dir()
            .join(format!("zeabur-desktop-store-{}", std::process::id()))
            .join("kubeconfigs");
        let first = write_new_private(&dir, "team", b"first").unwrap();
        assert_eq!((mode(&dir), mode(&first)), (0o700, 0o600));

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let second = write_new_private(&dir, "team", b"second").unwrap();
        assert_ne!(first, second);
        assert_eq!((mode(&dir), mode(&second)), (0o700, 0o600));
        assert_eq!(std::fs::read(&first).unwrap(), b"first");
        assert_eq!(std::fs::read(&second).unwrap(), b"second");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
