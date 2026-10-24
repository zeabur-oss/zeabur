//! Writing files atomically with a mode.

use std::fs::{self, OpenOptions, Permissions};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::Path;

use crate::HostError;

/// Writes `content` to `path` with `mode` through a temporary file and a rename, so a reader sees
/// the old file or the new one. Returns whether anything changed.
///
/// # Errors
///
/// When a directory cannot be made or the file cannot be written.
pub fn write(path: &Path, mode: u32, content: &[u8]) -> Result<bool, HostError> {
    if let Ok(meta) = fs::metadata(path)
        && meta.permissions().mode() & 0o7777 == mode
        && fs::read(path).is_ok_and(|old| old == content)
    {
        return Ok(false);
    }
    let dir = path.parent().unwrap_or_else(|| Path::new("/"));
    fs::create_dir_all(dir).map_err(HostError::io(dir))?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let temp = dir.join(format!(".{name}.zeabur-installer"));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(&temp)
        .map_err(HostError::io(&temp))?;
    // `mode` above is masked by the umask; set it exactly.
    file.set_permissions(Permissions::from_mode(mode))
        .map_err(HostError::io(&temp))?;
    file.write_all(content).map_err(HostError::io(&temp))?;
    file.sync_all().map_err(HostError::io(&temp))?;
    fs::rename(&temp, path).map_err(HostError::io(path))?;
    Ok(true)
}

/// Reads a secret file, refusing one others can read.
///
/// # Errors
///
/// When it cannot be read or its mode lets the group or others read it.
pub fn read_secret(path: &Path) -> Result<Vec<u8>, HostError> {
    let mut file = fs::File::open(path).map_err(HostError::io(path))?;
    let mode = file
        .metadata()
        .map_err(HostError::io(path))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(HostError::Io {
            path: path.to_owned(),
            source: std::io::Error::other(format!(
                "mode {:o} lets others read it; chmod 0600",
                mode & 0o777
            )),
        });
    }
    let mut bytes = Vec::new();
    std::io::copy(&mut file, &mut bytes).map_err(HostError::io(path))?;
    Ok(bytes)
}

/// 64 random bytes as 128 hexadecimal digits.
///
/// # Errors
///
/// When `/dev/urandom` cannot be read.
pub fn random_hex() -> Result<String, HostError> {
    let mut bytes = [0_u8; 64];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(HostError::io("/dev/urandom"))?;
    Ok(hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_once_with_the_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b.yaml");
        assert!(write(&path, 0o600, b"x").unwrap());
        assert!(!write(&path, 0o600, b"x").unwrap());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(write(&path, 0o644, b"x").unwrap());
        assert!(read_secret(&path).is_err());
        assert!(write(&path, 0o600, b"y").unwrap());
        assert_eq!(read_secret(&path).unwrap(), b"y");
        assert_eq!(random_hex().unwrap().len(), 128);
    }
}
