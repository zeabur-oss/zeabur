//! A checkout of a pinned commit or release tag in a directory of its own, kept between runs for
//! its build cache.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{fmt, fs};

/// Variables that point git at another repository than the one given on its command line.
const REPOSITORY_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_SHALLOW_FILE",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
    "GIT_CEILING_DIRECTORIES",
    "GIT_DISCOVERY_ACROSS_FILESYSTEM",
];

/// What a pin file such as `ZEABUR_IDENTITY_PINNED` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pin {
    /// A full commit id in hex (SHA-1 or SHA-256).
    Commit(String),
    /// A release tag, `vX.Y.Z` or `vX.Y.Z-<pre-release>`.
    Tag(String),
}

impl Pin {
    /// Reads a pin, ignoring surrounding whitespace.
    pub(crate) fn parse(pin: &str) -> Result<Self, String> {
        let pin = pin.trim();
        if is_release_tag(pin) {
            Ok(Self::Tag(pin.to_owned()))
        } else if matches!(pin.len(), 40 | 64) && pin.bytes().all(|b| b.is_ascii_hexdigit()) {
            Ok(Self::Commit(pin.to_ascii_lowercase()))
        } else {
            Err(format!(
                "pin {pin:?} is neither a full commit id nor a release tag vX.Y.Z"
            ))
        }
    }

    /// What a shallow fetch asks for. A tag is fetched as the local tag it names: fetched bare, it
    /// would only set `FETCH_HEAD`.
    fn refspec(&self) -> String {
        match self {
            Self::Commit(id) => id.clone(),
            Self::Tag(tag) => format!("+refs/tags/{tag}:refs/tags/{tag}"),
        }
    }

    fn revision(&self) -> String {
        match self {
            Self::Commit(id) => format!("{id}^{{commit}}"),
            Self::Tag(tag) => format!("refs/tags/{tag}^{{commit}}"),
        }
    }
}

impl fmt::Display for Pin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Commit(name) | Self::Tag(name) => f.write_str(name),
        }
    }
}

// SemVer 2.0.0 §2 and §9: numbers without leading zeros; build metadata is refused, as in
// scripts/versions.ts.
fn is_release_tag(pin: &str) -> bool {
    let number = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|b| b.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))
    };
    let identifier = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && (!part.bytes().all(|b| b.is_ascii_digit()) || number(part))
    };
    pin.strip_prefix('v').is_some_and(|version| {
        let (core, pre) = match version.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (version, None),
        };
        let parts: Vec<&str> = core.split('.').collect();
        parts.len() == 3
            && parts.iter().all(|part| number(part))
            && pre.is_none_or(|pre| pre.split('.').all(identifier))
    })
}

/// Checks out `pin` from `url` in `dir`, detached, and removes untracked files except `target/`.
/// A directory that is not its own repository (a partial `.git` restored by a CI cache, or none
/// inside another checkout) is replaced, and git is always given the repository explicitly, so it
/// never acts on an enclosing one. A server that refuses to send a single commit gets a fetch of
/// its branches and tags.
pub(crate) fn checkout(dir: &Path, pin: &Pin, url: &str) -> Result<(), String> {
    if url.starts_with('-') {
        return Err(format!("{url:?} is not a Git URL"));
    }
    discard_unless_own_repository(dir)?;
    if !dir.join(".git").is_dir() {
        fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
        let mut init = git();
        init.args(["init", "-q"]).arg(dir);
        run(init)?;
    }
    let repo = Repository {
        dir: fs::canonicalize(dir).map_err(|e| format!("resolving {}: {e}", dir.display()))?,
    };
    if repo.resolve(pin).is_none() {
        let mut shallow = repo.command();
        shallow.args(["fetch", "-q", "--depth", "1", url, &pin.refspec()]);
        if !succeeds_quietly(shallow) {
            let mut full = repo.command();
            full.args([
                "fetch",
                "-q",
                url,
                "+refs/heads/*:refs/remotes/origin/*",
                "+refs/tags/*:refs/tags/*",
            ]);
            run(full)?;
        }
    }
    let commit = repo
        .resolve(pin)
        .ok_or_else(|| format!("{url} has no commit {pin}"))?;
    let mut switch = repo.command();
    switch.args([
        "-c",
        "advice.detachedHead=false",
        "checkout",
        "-q",
        "--force",
        "--detach",
        &commit,
    ]);
    run(switch)?;
    let mut clean = repo.command();
    clean.args(["clean", "-qfdx", "-e", "target"]);
    run(clean)
}

/// Removes `dir` unless it is the top level of a repository of its own.
fn discard_unless_own_repository(dir: &Path) -> Result<(), String> {
    let remove_error = |e| format!("removing {}: {e}", dir.display());
    let Ok(metadata) = fs::symlink_metadata(dir) else {
        return Ok(());
    };
    if !metadata.is_dir() {
        return fs::remove_file(dir).map_err(remove_error);
    }
    let canonical =
        fs::canonicalize(dir).map_err(|e| format!("resolving {}: {e}", dir.display()))?;
    let mut toplevel = git();
    toplevel
        .arg("-C")
        .arg(&canonical)
        .args(["rev-parse", "--show-toplevel"]);
    if let Some(parent) = canonical.parent() {
        toplevel.env("GIT_CEILING_DIRECTORIES", parent);
    }
    let own = canonical.join(".git").is_dir()
        && output(toplevel).is_some_and(|top| Path::new(&top) == canonical);
    if own {
        Ok(())
    } else {
        fs::remove_dir_all(dir).map_err(remove_error)
    }
}

/// A repository whose `.git` is a directory inside its working tree.
#[derive(Debug)]
struct Repository {
    dir: PathBuf,
}

impl Repository {
    /// Git on this repository only, whatever encloses it or the environment says.
    fn command(&self) -> Command {
        let mut command = git();
        command
            .arg("--git-dir")
            .arg(self.dir.join(".git"))
            .arg("--work-tree")
            .arg(&self.dir)
            .current_dir(&self.dir);
        command
    }

    /// The commit `pin` names here, if this repository has it.
    fn resolve(&self, pin: &Pin) -> Option<String> {
        let mut verify = self.command();
        verify.args(["rev-parse", "-q", "--verify", &pin.revision()]);
        output(verify)
    }
}

fn git() -> Command {
    let mut command = Command::new("git");
    for name in REPOSITORY_ENV {
        command.env_remove(name);
    }
    command.stdin(Stdio::null());
    command
}

fn run(mut command: Command) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|e| format!("running {command:?}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command:?} failed ({status})"))
    }
}

fn succeeds_quietly(mut command: Command) -> bool {
    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The command's standard output, trimmed, if it succeeds.
fn output(mut command: Command) -> Option<String> {
    let output = command.stderr(Stdio::null()).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|out| out.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use tempfile::TempDir;

    use super::{Pin, checkout};

    const TAG: &str = "v0.1.1";

    /// Git in `dir`, with no configuration from this machine.
    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .args(["-c", "init.defaultBranch=main"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// Writes `file` and commits it; returns the commit.
    fn commit(dir: &Path, file: &str, content: &str) -> String {
        fs::write(dir.join(file), content).unwrap();
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-q", "-m", content]);
        git(dir, &["rev-parse", "HEAD"])
    }

    /// A served repository: a first commit, a release tag on the second, and a third on `main`.
    struct Upstream {
        _dir: TempDir,
        url: String,
        first: String,
        tagged: String,
    }

    fn upstream() -> Upstream {
        let dir = TempDir::new().unwrap();
        let path = fs::canonicalize(dir.path()).unwrap();
        git(&path, &["init", "-q"]);
        let first = commit(&path, "version", "1");
        let tagged = commit(&path, "version", "2");
        git(&path, &["tag", "-a", TAG, "-m", "release"]);
        commit(&path, "version", "3");
        Upstream {
            url: format!("file://{}", path.display()),
            _dir: dir,
            first,
            tagged,
        }
    }

    /// A repository with a commit, a modified tracked file and an untracked file.
    fn enclosing() -> (TempDir, PathBuf, String) {
        let dir = TempDir::new().unwrap();
        let path = fs::canonicalize(dir.path()).unwrap();
        git(&path, &["init", "-q"]);
        let head = commit(&path, "tracked", "committed");
        fs::write(path.join("tracked"), "modified").unwrap();
        fs::write(path.join("untracked"), "kept").unwrap();
        (dir, path, head)
    }

    fn assert_untouched(enclosing: &Path, head: &str) {
        assert_eq!(git(enclosing, &["rev-parse", "HEAD"]), head);
        assert_eq!(
            fs::read_to_string(enclosing.join("tracked")).unwrap(),
            "modified"
        );
        assert_eq!(
            fs::read_to_string(enclosing.join("untracked")).unwrap(),
            "kept"
        );
    }

    fn assert_checked_out(dir: &Path, commit: &str) {
        let dir = fs::canonicalize(dir).unwrap();
        assert_eq!(
            git(&dir, &["rev-parse", "--show-toplevel"]),
            dir.display().to_string()
        );
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), commit);
    }

    #[test]
    fn checks_out_a_commit_pin() {
        let upstream = upstream();
        let work = TempDir::new().unwrap();
        let dir = work.path().join("identity");
        checkout(&dir, &Pin::Commit(upstream.first.clone()), &upstream.url).unwrap();
        assert_checked_out(&dir, &upstream.first);
        assert_eq!(fs::read_to_string(dir.join("version")).unwrap(), "1");
    }

    // A tag fetched bare with --depth 1 only set FETCH_HEAD, and the checkout found no `v0.1.1`.
    #[test]
    fn checks_out_a_release_tag_pin_from_a_shallow_fetch() {
        let upstream = upstream();
        let work = TempDir::new().unwrap();
        let dir = work.path().join("identity");
        checkout(&dir, &Pin::parse(TAG).unwrap(), &upstream.url).unwrap();
        assert_checked_out(&dir, &upstream.tagged);
        assert!(dir.join(".git/shallow").is_file(), "not a shallow fetch");
    }

    // A CI cache of target/ restored a partial target/dev-cluster/identity/.git; git then found the
    // enclosing repository and force-checked-out and cleaned it.
    #[test]
    fn replaces_a_partial_git_directory_inside_another_repository() {
        let upstream = upstream();
        let (_enclosing, enclosing, head) = enclosing();
        let dir = enclosing.join("target/dev-cluster/identity");
        fs::create_dir_all(dir.join(".git/objects/pack")).unwrap();
        checkout(&dir, &Pin::parse(TAG).unwrap(), &upstream.url).unwrap();
        assert_checked_out(&dir, &upstream.tagged);
        assert_untouched(&enclosing, &head);
    }

    #[test]
    fn replaces_a_git_file_that_points_nowhere() {
        let upstream = upstream();
        let (_enclosing, enclosing, head) = enclosing();
        let dir = enclosing.join("target/dev-cluster/identity");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".git"), "gitdir: /nonexistent\n").unwrap();
        checkout(&dir, &Pin::Commit(upstream.first.clone()), &upstream.url).unwrap();
        assert_checked_out(&dir, &upstream.first);
        assert_untouched(&enclosing, &head);
    }

    #[test]
    fn replaces_a_plain_directory_inside_another_repository() {
        let upstream = upstream();
        let (_enclosing, enclosing, head) = enclosing();
        let dir = enclosing.join("target/dev-cluster/identity");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("stray"), "").unwrap();
        checkout(&dir, &Pin::parse(TAG).unwrap(), &upstream.url).unwrap();
        assert_checked_out(&dir, &upstream.tagged);
        assert!(!dir.join("stray").exists());
        assert_untouched(&enclosing, &head);
    }

    #[test]
    fn reuses_a_cached_checkout_without_fetching_and_keeps_target() {
        let upstream = upstream();
        let (_enclosing, enclosing, head) = enclosing();
        let dir = enclosing.join("target/dev-cluster/identity");
        let pin = Pin::Commit(upstream.first.clone());
        checkout(&dir, &pin, &upstream.url).unwrap();
        fs::create_dir_all(dir.join("target")).unwrap();
        fs::write(dir.join("target/cache"), "").unwrap();
        fs::write(dir.join("stray"), "").unwrap();
        fs::write(dir.join("version"), "edited").unwrap();
        checkout(&dir, &pin, "file:///nonexistent").unwrap();
        assert_checked_out(&dir, &upstream.first);
        assert!(dir.join("target/cache").exists());
        assert!(!dir.join("stray").exists());
        assert_eq!(fs::read_to_string(dir.join("version")).unwrap(), "1");
        assert_untouched(&enclosing, &head);
    }

    #[test]
    fn moves_a_cached_checkout_to_a_new_pin() {
        let upstream = upstream();
        let work = TempDir::new().unwrap();
        let dir = work.path().join("identity");
        checkout(&dir, &Pin::Commit(upstream.first.clone()), &upstream.url).unwrap();
        checkout(&dir, &Pin::parse(TAG).unwrap(), &upstream.url).unwrap();
        assert_checked_out(&dir, &upstream.tagged);
    }

    #[test]
    fn fails_on_a_pin_the_server_does_not_have() {
        let upstream = upstream();
        let work = TempDir::new().unwrap();
        let dir = work.path().join("identity");
        let error = checkout(&dir, &Pin::parse("v9.9.9").unwrap(), &upstream.url).unwrap_err();
        assert!(error.contains("v9.9.9"), "{error}");
    }

    #[test]
    fn reads_only_full_commit_ids_and_release_tags() {
        let id = "8AD652B60F8A54838D35BF27B5902E1883DE791E";
        assert_eq!(
            Pin::parse(&format!("{id}\n")),
            Ok(Pin::Commit(id.to_ascii_lowercase()))
        );
        assert_eq!(Pin::parse(" v0.1.1\n"), Ok(Pin::Tag("v0.1.1".to_owned())));
        for tag in ["v0.1.2-beta.0", "v1.0.0-rc.1", "v0.1.2-x-1.alpha"] {
            assert_eq!(Pin::parse(tag), Ok(Pin::Tag(tag.to_owned())));
        }
        for pin in [
            "main",
            "8ad652b",
            "v0.1",
            "v01.1.1",
            "v0.1.1-",
            "v0.1.1-beta..1",
            "v0.1.1-beta.01",
            "v0.1.1+build.1",
            "-v0.1.1",
            "--upload-pack=x",
            "",
        ] {
            assert!(Pin::parse(pin).is_err(), "{pin:?}");
        }
    }
}
