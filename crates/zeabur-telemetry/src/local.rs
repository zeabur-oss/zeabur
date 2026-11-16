//! The random ID of a local run (`zeaburctl ui`, Zeabur Desktop), kept in a file.

use std::ffi::OsString;
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::event::InstallationId;

/// Counts the files that wait to be linked into place.
static TEMP: AtomicU32 = AtomicU32::new(0);
/// The file in the configuration directory.
const FILE: &str = "local-id";
/// The directory of Zeabur in the platform's configuration directory.
const DIRECTORY: &str = "zeabur";

/// 16 random bytes from the system CSPRNG, or `None` when it fails. The caller supplies it
/// because this crate has no random source of its own.
pub type Random = fn() -> Option<[u8; 16]>;

/// The platforms whose standard configuration directory is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    Linux,
    /// `~/Library/Application Support`.
    MacOs,
    /// `%APPDATA%`.
    Windows,
}

impl Platform {
    /// The platform this program was built for.
    pub const HERE: Self = if cfg!(target_os = "windows") {
        Self::Windows
    } else if cfg!(target_os = "macos") {
        Self::MacOs
    } else {
        Self::Linux
    };
}

/// The directory that holds the ID of a local run on `platform`, with `var` as the environment.
/// `None` when the environment names no home.
#[must_use]
pub fn standard_dir_on(
    platform: Platform,
    var: impl Fn(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    let set = |name: &str| {
        var(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let base = match platform {
        Platform::Linux => set("XDG_CONFIG_HOME")
            .filter(|path| path.is_absolute())
            .or_else(|| set("HOME").map(|home| home.join(".config"))),
        Platform::MacOs => set("HOME").map(|home| home.join("Library").join("Application Support")),
        Platform::Windows => set("APPDATA"),
    };
    base.map(|base| base.join(DIRECTORY))
}

/// The directory that holds the ID of a local run here.
#[must_use]
pub fn standard_dir() -> Option<PathBuf> {
    standard_dir_on(Platform::HERE, |name| std::env::var_os(name))
}

/// The ID of one local run: a random UUID in a file of mode `0600`. It is made by
/// [`get`](Self::get), when something is about to be sent, and by nothing else.
#[derive(Debug)]
pub struct LocalId {
    path: PathBuf,
    random: Random,
}

impl LocalId {
    /// The ID in `directory`, which need not exist yet.
    #[must_use]
    pub fn new(directory: &Path, random: Random) -> Self {
        Self {
            path: directory.join(FILE),
            random,
        }
    }

    /// The ID if it exists. Nothing is made.
    #[must_use]
    pub fn peek(&self) -> Option<InstallationId> {
        self.read().ok().flatten()
    }

    /// The ID; made and kept first when there is none. The file is read each time, so a
    /// [`reset`](Self::reset) by another process reaches a run that is already sending.
    ///
    /// # Errors
    ///
    /// When the file cannot be read or written, or there is no random number.
    pub fn get(&self) -> io::Result<InstallationId> {
        self.read()?.map_or_else(|| self.make(), Ok)
    }

    /// Deletes the file. No file is not an error.
    ///
    /// # Errors
    ///
    /// When the file exists and cannot be removed.
    pub fn reset(directory: &Path) -> io::Result<()> {
        match std::fs::remove_file(directory.join(FILE)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }

    /// The ID in the file; `None` when there is none or it is not an ID.
    fn read(&self) -> io::Result<Option<InstallationId>> {
        let mut text = String::new();
        match std::fs::File::open(&self.path) {
            Ok(file) => file.take(128).read_to_string(&mut text)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(InstallationId::parse(&text))
    }

    /// Writes the ID to a file of its own, then links it into place, so a file that exists is
    /// always complete and a valid one made by another run is never replaced.
    fn make(&self) -> io::Result<InstallationId> {
        let bytes = (self.random)().ok_or_else(|| io::Error::other("no random number"))?;
        let id = InstallationId::from_random(bytes);
        if let Some(directory) = self.path.parent() {
            create_private_dir(directory)?;
        }
        let temp = self.path.with_extension(format!(
            "{}.{}.tmp",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let linked = self.link(&temp, id);
        drop(std::fs::remove_file(&temp));
        linked
    }

    fn link(&self, temp: &Path, id: InstallationId) -> io::Result<InstallationId> {
        writeln!(private_file().open(temp)?, "{}", id.as_str())?;
        match std::fs::hard_link(temp, &self.path) {
            Ok(()) => Ok(id),
            // Another run made it first: take theirs. A file that is not an ID is replaced.
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => self
                .read()?
                .map_or_else(|| std::fs::rename(temp, &self.path).map(|()| id), Ok),
            Err(error) => Err(error),
        }
    }
}

fn create_private_dir(directory: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(directory)
}

/// A new file only the owner may read. Windows keeps a file in `%APPDATA%` to its owner already.
fn private_file() -> std::fs::OpenOptions {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[expect(clippy::unnecessary_wraps, reason = "the signature of `Random`")]
    fn random() -> Option<[u8; 16]> {
        Some([9; 16])
    }

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn the_directory_is_the_platforms_own() {
        let dir = |platform, pairs: &[(&str, &str)]| standard_dir_on(platform, env(pairs));
        assert_eq!(
            dir(
                Platform::Linux,
                &[("XDG_CONFIG_HOME", "/xdg"), ("HOME", "/home/a")]
            ),
            Some(PathBuf::from("/xdg/zeabur"))
        );
        assert_eq!(
            dir(
                Platform::Linux,
                &[("XDG_CONFIG_HOME", "relative"), ("HOME", "/home/a")]
            ),
            Some(PathBuf::from("/home/a/.config/zeabur"))
        );
        assert_eq!(
            dir(Platform::Linux, &[("HOME", "/home/a")]),
            Some(PathBuf::from("/home/a/.config/zeabur"))
        );
        assert_eq!(
            dir(
                Platform::MacOs,
                &[("HOME", "/Users/a"), ("XDG_CONFIG_HOME", "/xdg")]
            ),
            Some(PathBuf::from("/Users/a/Library/Application Support/zeabur"))
        );
        assert_eq!(
            dir(
                Platform::Windows,
                &[("APPDATA", "C:\\Users\\a\\AppData\\Roaming")]
            ),
            Some(PathBuf::from("C:\\Users\\a\\AppData\\Roaming").join("zeabur"))
        );
        assert_eq!(dir(Platform::Linux, &[]), None);
        assert_eq!(dir(Platform::Windows, &[("HOME", "/home/a")]), None);
    }

    #[test]
    fn nothing_is_made_until_get_and_the_id_stays() {
        let root = tempdir();
        let directory = root.join("zeabur");
        let local = LocalId::new(&directory, random);
        assert_eq!(local.peek(), None);
        assert!(!directory.exists());

        let id = local.get().unwrap();
        assert_eq!(id.as_str(), "09090909-0909-4909-8909-090909090909");
        assert_eq!(
            LocalId::new(&directory, || Some([1; 16])).get().unwrap(),
            id
        );
        assert_eq!(LocalId::new(&directory, random).peek(), Some(id));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_private() {
        use std::os::unix::fs::PermissionsExt as _;
        let root = tempdir();
        let directory = root.join("zeabur");
        LocalId::new(&directory, random).get().unwrap();
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&directory.join(FILE)), 0o600);
        assert_eq!(mode(&directory), 0o700);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reset_removes_it_and_the_next_id_is_new() {
        let root = tempdir();
        let first = LocalId::new(&root, random).get().unwrap();
        LocalId::reset(&root).unwrap();
        LocalId::reset(&root).unwrap();
        assert_eq!(LocalId::new(&root, random).peek(), None);
        let second = LocalId::new(&root, || Some([3; 16])).get().unwrap();
        assert_ne!(first, second);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_file_that_is_not_an_id_is_replaced() {
        let root = tempdir();
        std::fs::write(root.join(FILE), "not an id").unwrap();
        let local = LocalId::new(&root, random);
        assert_eq!(local.peek(), None);
        assert!(local.get().is_ok());
        assert!(LocalId::new(&root, random).peek().is_some());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_reset_reaches_a_run_that_already_has_the_id() {
        let root = tempdir();
        let running = LocalId::new(&root, random);
        let first = running.get().unwrap();
        LocalId::reset(&root).unwrap();
        assert_eq!(running.peek(), None);
        assert_ne!(first, LocalId::new(&root, || Some([5; 16])).get().unwrap());
        assert_ne!(running.get().unwrap(), first);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[expect(
        clippy::needless_collect,
        reason = "every thread starts before the first join"
    )]
    fn runs_that_start_together_keep_one_id() {
        let root = tempdir();
        let ids: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8_u8)
                .map(|n| {
                    let root = &root;
                    scope.spawn(move || {
                        let random: Random = if n % 2 == 0 {
                            || Some([1; 16])
                        } else {
                            || Some([2; 16])
                        };
                        LocalId::new(root, random).get().unwrap()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(ids.iter().all(|id| *id == ids[0]));
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_random_number_makes_no_file() {
        let root = tempdir();
        assert!(LocalId::new(&root, || None).get().is_err());
        assert!(!root.join(FILE).exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    pub(crate) fn tempdir() -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "zeabur-local-id-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
