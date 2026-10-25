//! Packs a folder as the gzip tar Zeabur Builder builds (`docs/archive-source.md`): the
//! files `.gitignore` and `.dockerignore` leave in, and no `.git` folder.
//!
//! A `.gitignore` applies to its own folder and below, as Git reads it [gitignore]; a
//! `.dockerignore` only at the top, in the same pattern syntax. A path is left out when either
//! says so. A folder that is left out is not entered, so a pattern cannot bring back something
//! below it. A symbolic link stays when it points to a relative path inside the folder, as the
//! unpack rule requires; any other, and any file that is neither regular nor a folder, is left
//! out and counted.
//!
//! # References
//!
//! - [gitignore] The Git Project, "gitignore: Specifies intentionally untracked files to
//!   ignore," pattern format. <https://git-scm.com/docs/gitignore#_pattern_format>
//!
//! [gitignore]: https://git-scm.com/docs/gitignore#_pattern_format

use std::fs;
use std::path::{Component, Path};

use flate2::Compression;
use flate2::write::GzEncoder;
use regex::Regex;

/// Most bytes the packed folder may take, as Zeabur Builder downloads at most.
pub(crate) const MAX_PACKED: u64 = 256 * 1024 * 1024;
/// Most bytes the files may take together once unpacked.
pub(crate) const MAX_UNPACKED: u64 = 1024 * 1024 * 1024;

/// A folder, packed.
#[derive(Debug)]
pub(crate) struct Packed {
    /// The gzip tar.
    pub(crate) bytes: Vec<u8>,
    /// Files in it.
    pub(crate) files: usize,
    /// Entries left out because they were not regular files, folders or safe links.
    pub(crate) skipped: usize,
}

/// Why nothing was packed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PackError {
    /// The folder or a file in it cannot be read.
    Read(String),
    /// The files together are past the unpacked limit.
    Unpacked,
    /// The packed folder is past the limit; the largest top-level entries, to leave out.
    Packed(Vec<(String, u64)>),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(why) => write!(f, "cannot read the folder: {why}"),
            Self::Unpacked => write!(
                f,
                "the files add up to more than {} GiB; leave the large ones out in .gitignore",
                MAX_UNPACKED >> 30
            ),
            Self::Packed(largest) => {
                write!(
                    f,
                    "the folder packs to more than {} MiB; leave the large ones out in .gitignore",
                    MAX_PACKED >> 20
                )?;
                for (name, size) in largest {
                    write!(f, "\n  {name}  {} MiB", size >> 20)?;
                }
                Ok(())
            }
        }
    }
}

struct Rule {
    pattern: Regex,
    negated: bool,
    directories_only: bool,
}

/// The rules of one ignore file and the folder it sits in, relative to the root.
struct Rules {
    base: String,
    list: Vec<Rule>,
}

fn rules(base: &str, text: &str, docker: bool) -> Rules {
    Rules {
        base: base.to_owned(),
        list: text.lines().filter_map(|line| rule(line, docker)).collect(),
    }
}

fn rule(line: &str, docker: bool) -> Option<Rule> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let mut text = line.trim_end_matches(' ');
    if text.is_empty() || text.starts_with('#') {
        return None;
    }
    let negated = text.starts_with('!');
    if let Some(rest) = text.strip_prefix('!') {
        text = rest;
    }
    let directories_only = text.ends_with('/');
    let text = text.trim_end_matches('/');
    if text.is_empty() {
        return None;
    }
    let anchored = docker || text.contains('/');
    let body = translate(text.strip_prefix('/').unwrap_or(text));
    let source = if anchored {
        format!("^{body}$")
    } else {
        format!("^(?:(?s:.)*/)?{body}$")
    };
    Some(Rule {
        pattern: Regex::new(&source).ok()?,
        negated,
        directories_only,
    })
}

/// The regular expression for a gitignore glob: `*` and `?` stay inside a name, `**` crosses
/// folders.
fn translate(glob: &str) -> String {
    let chars: Vec<char> = glob.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        let before_slash = at == 0 || chars.get(at - 1) == Some(&'/');
        match c {
            '*' if chars.get(at + 1) == Some(&'*') && before_slash => match chars.get(at + 2) {
                Some('/') => {
                    out.push_str("(?:(?s:.)*/)?");
                    at += 3;
                }
                None => {
                    out.push_str("(?s:.)*");
                    at += 2;
                }
                Some(_) => {
                    out.push_str("[^/]*");
                    at += 2;
                }
            },
            '*' => {
                out.push_str("[^/]*");
                at += 1;
                while chars.get(at) == Some(&'*') {
                    at += 1;
                }
            }
            '?' => {
                out.push_str("[^/]");
                at += 1;
            }
            '\\' => {
                if let Some(next) = chars.get(at + 1) {
                    out.push_str(&regex::escape(&next.to_string()));
                    at += 2;
                } else {
                    at += 1;
                }
            }
            '[' => {
                if let Some((text, end)) = class(&chars, at) {
                    out.push_str(&text);
                    at = end;
                } else {
                    out.push_str("\\[");
                    at += 1;
                }
            }
            _ => {
                out.push_str(&regex::escape(&c.to_string()));
                at += 1;
            }
        }
    }
    out
}

/// A bracket expression starting at `start`, and where it ends; none when it is not closed.
fn class(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut at = start + 1;
    let mut out = String::from("[");
    if matches!(chars.get(at), Some('!' | '^')) {
        out.push('^');
        at += 1;
    }
    let first = at;
    while let Some(&c) = chars.get(at) {
        if c == ']' && at > first {
            out.push(']');
            return Some((out, at + 1));
        }
        if matches!(c, '\\' | '[' | '^' | '&' | '~') {
            out.push('\\');
        }
        out.push(c);
        at += 1;
    }
    None
}

/// What the rules of `sets`, shallowest first, decide for `path` (relative to the root): left
/// out or not. The last rule that matches wins.
fn excluded(sets: &[Rules], path: &str, is_dir: bool) -> bool {
    let mut excluded = false;
    for set in sets {
        let Some(relative) = relative_to(path, &set.base) else {
            continue;
        };
        for rule in &set.list {
            if (!rule.directories_only || is_dir) && rule.pattern.is_match(relative) {
                excluded = !rule.negated;
            }
        }
    }
    excluded
}

fn relative_to<'a>(path: &'a str, base: &str) -> Option<&'a str> {
    if base.is_empty() {
        Some(path)
    } else {
        path.strip_prefix(base)?.strip_prefix('/')
    }
}

struct Walk<'a> {
    root: &'a Path,
    docker: Rules,
    out: tar::Builder<GzEncoder<Vec<u8>>>,
    files: usize,
    skipped: usize,
    unpacked: u64,
    sizes: Vec<(String, u64)>,
}

/// Packs the folder `root`.
///
/// # Errors
///
/// When the folder cannot be read or is past the limits.
pub(crate) fn pack(root: &Path) -> Result<Packed, PackError> {
    let read = |e: std::io::Error| PackError::Read(e.to_string());
    let docker = fs::read_to_string(root.join(".dockerignore")).unwrap_or_default();
    let mut walk = Walk {
        root,
        docker: rules("", &docker, true),
        out: tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default())),
        files: 0,
        skipped: 0,
        unpacked: 0,
        sizes: Vec::new(),
    };
    walk.out.mode(tar::HeaderMode::Deterministic);
    walk.folder("", &mut Vec::new())?;
    let encoder = walk.out.into_inner().map_err(read)?;
    let bytes = encoder.finish().map_err(read)?;
    if bytes.len() as u64 > MAX_PACKED {
        let mut largest = walk.sizes;
        largest.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        largest.truncate(3);
        return Err(PackError::Packed(largest));
    }
    Ok(Packed {
        bytes,
        files: walk.files,
        skipped: walk.skipped,
    })
}

impl Walk<'_> {
    fn folder(&mut self, relative: &str, git: &mut Vec<Rules>) -> Result<(), PackError> {
        let read = |e: std::io::Error| PackError::Read(e.to_string());
        let here = self.root.join(relative);
        let mut names: Vec<String> = fs::read_dir(&here)
            .map_err(read)?
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect();
        names.sort();
        let own = fs::read_to_string(here.join(".gitignore")).ok();
        let pushed = own.is_some();
        if let Some(text) = own {
            git.push(rules(relative, &text, false));
        }
        for name in names {
            let path = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            if name == ".git" {
                continue;
            }
            let meta = fs::symlink_metadata(self.root.join(&path)).map_err(read)?;
            let is_dir = meta.is_dir();
            if excluded(git, &path, is_dir)
                || excluded(std::slice::from_ref(&self.docker), &path, is_dir)
            {
                continue;
            }
            let before = self.unpacked;
            if is_dir {
                self.out
                    .append_dir(&path, self.root.join(&path))
                    .map_err(read)?;
                self.folder(&path, git)?;
            } else if meta.is_file() {
                self.file(&path, meta.len())?;
            } else if meta.is_symlink() {
                self.link(&path)?;
            } else {
                self.skipped += 1;
            }
            if relative.is_empty() {
                self.sizes.push((path, self.unpacked - before));
            }
        }
        if pushed {
            git.pop();
        }
        Ok(())
    }

    fn file(&mut self, path: &str, size: u64) -> Result<(), PackError> {
        self.unpacked += size;
        if self.unpacked > MAX_UNPACKED {
            return Err(PackError::Unpacked);
        }
        let mut file =
            fs::File::open(self.root.join(path)).map_err(|e| PackError::Read(e.to_string()))?;
        self.out
            .append_file(path, &mut file)
            .map_err(|e| PackError::Read(e.to_string()))?;
        self.files += 1;
        Ok(())
    }

    fn link(&mut self, path: &str) -> Result<(), PackError> {
        let target =
            fs::read_link(self.root.join(path)).map_err(|e| PackError::Read(e.to_string()))?;
        if !stays_inside(path, &target) {
            self.skipped += 1;
            return Ok(());
        }
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        self.out
            .append_link(&mut header, path, &target)
            .map_err(|e| PackError::Read(e.to_string()))
    }
}

/// Whether the link at `path` with a relative `target` points inside the folder.
fn stays_inside(path: &str, target: &Path) -> bool {
    let mut depth = Path::new(path).components().count().saturating_sub(1);
    for part in target.components() {
        match part {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::io::Read as _;

    use super::*;

    fn names(packed: &Packed) -> BTreeSet<String> {
        let mut text = Vec::new();
        flate2::read::GzDecoder::new(packed.bytes.as_slice())
            .read_to_end(&mut text)
            .unwrap();
        tar::Archive::new(text.as_slice())
            .entries()
            .unwrap()
            .map(|e| {
                e.unwrap()
                    .path()
                    .unwrap()
                    .to_string_lossy()
                    .trim_end_matches('/')
                    .to_owned()
            })
            .collect()
    }

    fn folder(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, text).unwrap();
        }
        dir
    }

    #[test]
    fn the_ignore_files_and_git_decide_what_is_packed() {
        let dir = folder(&[
            ("Dockerfile", "FROM scratch"),
            ("src/main.rs", ""),
            ("src/main.log", ""),
            ("src/keep.log", ""),
            ("src/.gitignore", "*.log\n!keep.log\n"),
            (".gitignore", "node_modules/\n/dist\n*.tmp\n"),
            (".dockerignore", "docs/**\n!docs/README.md\nsecret.txt\n"),
            ("node_modules/a/index.js", ""),
            ("dist", ""),
            ("lib/dist", ""),
            ("a.tmp", ""),
            ("docs/guide.md", ""),
            ("docs/README.md", ""),
            ("secret.txt", ""),
            ("lib/secret.txt", ""),
            (".git/config", ""),
        ]);
        let found = names(&pack(dir.path()).unwrap());
        let expected: BTreeSet<String> = [
            ".dockerignore",
            ".gitignore",
            "Dockerfile",
            "docs",
            "docs/README.md",
            "lib",
            "lib/dist",
            "lib/secret.txt",
            "src",
            "src/.gitignore",
            "src/keep.log",
            "src/main.rs",
        ]
        .map(str::to_owned)
        .into();
        assert_eq!(found, expected);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_stays_only_when_it_points_inside_the_folder() {
        let dir = folder(&[("a/real.txt", "x")]);
        std::os::unix::fs::symlink("real.txt", dir.path().join("a/inside")).unwrap();
        std::os::unix::fs::symlink("../a/real.txt", dir.path().join("a/up-and-in")).unwrap();
        std::os::unix::fs::symlink("../../etc/passwd", dir.path().join("a/out")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", dir.path().join("absolute")).unwrap();
        let packed = pack(dir.path()).unwrap();
        let found = names(&packed);
        assert!(
            found.contains("a/inside") && found.contains("a/up-and-in"),
            "{found:?}"
        );
        assert!(
            !found.contains("a/out") && !found.contains("absolute"),
            "{found:?}"
        );
        assert_eq!(packed.skipped, 2);
    }

    #[test]
    fn the_patterns_of_gitignore_mean_what_git_says() {
        let sets = [rules(
            "",
            "/a\nb/c\n**/d\ne/**\nf?.txt\n[g-h]*.md\n\\#x\n!/a\n#c\n",
            false,
        )];
        let skip = |path: &str, dir: bool| excluded(&sets, path, dir);
        assert!(!skip("a", false), "negated later");
        assert!(skip("b/c", false) && !skip("x/b/c", false));
        assert!(skip("d", false) && skip("x/y/d", false));
        assert!(skip("e/z", false) && !skip("e", true));
        assert!(skip("fa.txt", false) && !skip("fab.txt", false));
        assert!(skip("ga.md", false) && skip("h.md", false) && !skip("i.md", false));
        assert!(skip("#x", false));
    }

    #[test]
    fn a_folder_past_the_limit_names_what_to_leave_out() {
        let packed_error = PackError::Packed(vec![("node_modules".to_owned(), 400 << 20)]);
        assert!(packed_error.to_string().contains("node_modules  400 MiB"));
    }
}
