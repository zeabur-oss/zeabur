//! The files of the in-memory cluster's pods, and the commands of [`crate::files`] run on them:
//! only the argument vectors that module builds are understood; any other command is not found.

use std::collections::BTreeMap;

use crate::files::{self, FilePath};
use crate::port::ExitStatus;

/// One node of a pod's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryNode {
    /// A regular file.
    File(Vec<u8>),
    /// A folder.
    Directory,
    /// A symbolic link to an absolute or relative path.
    Link(String),
}

/// A pod's files, by absolute path; `/` is always a folder. Every container of the pod sees
/// them, as it would see the pod's volumes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryFiles {
    /// The nodes, each path's parent a folder.
    pub nodes: BTreeMap<String, MemoryNode>,
    /// Paths whose writes are refused as on a read-only volume.
    pub read_only: Vec<String>,
}

const MODIFIED: i64 = 1_760_000_000;

impl MemoryFiles {
    /// Adds a file, and the folders above it.
    pub fn file(&mut self, path: &str, contents: impl Into<Vec<u8>>) -> &mut Self {
        self.parents(path);
        self.nodes
            .insert(path.to_owned(), MemoryNode::File(contents.into()));
        self
    }

    /// Adds a folder, and the folders above it.
    pub fn directory(&mut self, path: &str) -> &mut Self {
        self.parents(path);
        self.nodes.insert(path.to_owned(), MemoryNode::Directory);
        self
    }

    /// Adds a link to `target`, and the folders above it.
    pub fn link(&mut self, path: &str, target: &str) -> &mut Self {
        self.parents(path);
        self.nodes
            .insert(path.to_owned(), MemoryNode::Link(target.to_owned()));
        self
    }

    fn parents(&mut self, path: &str) {
        let mut at = FilePath::parse(path).and_then(|p| p.parent());
        while let Some(folder) = at {
            if !folder.is_root() {
                self.nodes
                    .entry(folder.as_str().to_owned())
                    .or_insert(MemoryNode::Directory);
            }
            at = folder.parent();
        }
    }

    fn node(&self, path: &str) -> Option<&MemoryNode> {
        if path == "/" {
            Some(&MemoryNode::Directory)
        } else {
            self.nodes.get(path)
        }
    }

    /// `path` with every link resolved.
    fn resolve(&self, path: &str) -> Option<String> {
        let mut done = String::new();
        let mut parts: Vec<String> = path
            .split('/')
            .filter(|p| !p.is_empty())
            .rev()
            .map(str::to_owned)
            .collect();
        let mut hops = 0;
        while let Some(part) = parts.pop() {
            match part.as_str() {
                "." => continue,
                ".." => {
                    let cut = done.rfind('/').unwrap_or(0);
                    done.truncate(cut);
                    continue;
                }
                _ => {}
            }
            let next = format!("{done}/{part}");
            match self.node(&next)? {
                MemoryNode::Link(target) => {
                    hops += 1;
                    if hops > 40 {
                        return None;
                    }
                    if target.starts_with('/') {
                        done.clear();
                    }
                    parts.extend(
                        target
                            .split('/')
                            .filter(|p| !p.is_empty())
                            .rev()
                            .map(str::to_owned),
                    );
                }
                MemoryNode::File(_) | MemoryNode::Directory => done = next,
            }
        }
        Some(if done.is_empty() {
            "/".to_owned()
        } else {
            done
        })
    }

    fn stat(&self, path: &str, follow: bool) -> Option<(u32, usize)> {
        let path = if follow {
            self.resolve(path)?
        } else {
            path.to_owned()
        };
        Some(match self.node(&path)? {
            MemoryNode::File(data) => (0o100_644, data.len()),
            MemoryNode::Directory => (0o040_755, 4096),
            MemoryNode::Link(target) => (0o120_777, target.len()),
        })
    }

    fn children(&self, folder: &str) -> Vec<String> {
        let prefix = if folder == "/" {
            "/".to_owned()
        } else {
            format!("{folder}/")
        };
        self.nodes
            .keys()
            .filter_map(|path| path.strip_prefix(&prefix))
            .filter(|rest| !rest.is_empty() && !rest.contains('/'))
            .map(str::to_owned)
            .collect()
    }

    fn writable(&self, path: &str) -> bool {
        !self.read_only.iter().any(|ro| {
            FilePath::parse(path)
                .zip(FilePath::parse(ro))
                .is_some_and(|(p, ro)| p.is_within(&ro))
        })
    }

    /// Runs `command` with `input` on standard input: its exit, standard output and error.
    #[expect(
        clippy::too_many_lines,
        reason = "one arm per command the files module builds, read side by side"
    )]
    pub(crate) async fn run(
        &mut self,
        command: &[String],
        input: &[u8],
    ) -> (ExitStatus, Vec<u8>, Vec<u8>) {
        let args: Vec<&str> = command.iter().map(String::as_str).collect();
        let fail = |message: String| (ExitStatus::Exited(1), Vec::new(), message.into_bytes());
        let ok = |out: Vec<u8>| (ExitStatus::Exited(0), out, Vec::new());
        match args.as_slice() {
            ["tar", "--help"] => ok(b"Usage: tar\n".to_vec()),
            ["find", start, "-mindepth", "1", "-maxdepth", "1", "-print0"] => {
                let Some(folder) = start.strip_suffix("/.") else {
                    return fail(format!("find: {start}: unexpected"));
                };
                let folder = if folder.is_empty() { "/" } else { folder };
                match self.resolve(folder) {
                    Some(real) if self.node(&real) == Some(&MemoryNode::Directory) => {
                        let mut out = Vec::new();
                        for name in self.children(&real) {
                            out.extend_from_slice(format!("{start}/{name}\0").as_bytes());
                        }
                        ok(out)
                    }
                    Some(_) => fail(format!("find: '{start}': Not a directory")),
                    None => fail(format!("find: '{start}': No such file or directory")),
                }
            }
            ["stat", rest @ ..] => {
                let (follow, rest) = match rest {
                    ["-L", rest @ ..] => (true, rest),
                    _ => (false, rest),
                };
                let ["-c", "%f %s %Y %n", "--", paths @ ..] = rest else {
                    return fail("stat: unexpected".to_owned());
                };
                let mut out = Vec::new();
                let mut err = Vec::new();
                for path in paths {
                    match self.stat(path, follow) {
                        Some((mode, size)) => out.extend_from_slice(
                            format!("{mode:x} {size} {MODIFIED} {path}\n").as_bytes(),
                        ),
                        None => err.extend_from_slice(
                            format!("stat: can't stat '{path}': No such file or directory\n")
                                .as_bytes(),
                        ),
                    }
                }
                let code = i32::from(!err.is_empty());
                (ExitStatus::Exited(code), out, err)
            }
            ["readlink", "-f", "--", path] => self.resolve(path).map_or_else(
                || (ExitStatus::Exited(1), Vec::new(), Vec::new()),
                |real| ok(format!("{real}\n").into_bytes()),
            ),
            ["tar", "-c", "-f", "-", "-C", parent, member] => {
                let Some(name) = member.strip_prefix("./") else {
                    return fail("tar: unexpected".to_owned());
                };
                let path = if *parent == "/" {
                    format!("/{name}")
                } else {
                    format!("{parent}/{name}")
                };
                let (flag, data) = match self.node(&path) {
                    Some(MemoryNode::File(data)) => (b'0', data.clone()),
                    Some(MemoryNode::Directory) => (b'5', Vec::new()),
                    Some(MemoryNode::Link(_)) => (b'2', Vec::new()),
                    None => {
                        return fail(format!("tar: {member}: No such file or directory"));
                    }
                };
                let size = data.len() as u64;
                let mut out = files::file_header(member, size, 0);
                if let Some(byte) = out
                    .len()
                    .checked_sub(512)
                    .and_then(|start| out.get_mut(start + 156))
                {
                    *byte = flag;
                }
                out.extend_from_slice(&data);
                out.extend(files::archive_end(size));
                ok(out)
            }
            ["tar", "-x", "-m", "-o", "-f", "-", "-C", folder] => {
                if !self.writable(folder) {
                    return fail(format!("tar: {folder}: Read-only file system"));
                }
                if self.node(folder) != Some(&MemoryNode::Directory) {
                    return fail(format!(
                        "tar: can't change directory to '{folder}': No such file or directory"
                    ));
                }
                match files::read_archive(&mut &*input).await {
                    Ok((name, data)) => {
                        let path = if *folder == "/" {
                            format!("/{name}")
                        } else {
                            format!("{folder}/{name}")
                        };
                        if self.node(&path) == Some(&MemoryNode::Directory) {
                            return fail(format!("tar: can't open '{name}': Is a directory"));
                        }
                        self.nodes.insert(path, MemoryNode::File(data));
                        ok(Vec::new())
                    }
                    Err(e) => fail(format!("tar: {e}")),
                }
            }
            ["mkdir", "--", path] => {
                if !self.writable(path) {
                    fail(format!(
                        "mkdir: can't create directory '{path}': Read-only file system"
                    ))
                } else if self.node(path).is_some() {
                    fail(format!(
                        "mkdir: can't create directory '{path}': File exists"
                    ))
                } else {
                    self.nodes.insert((*path).to_owned(), MemoryNode::Directory);
                    ok(Vec::new())
                }
            }
            ["mv", "--", from, to] => {
                if !self.writable(from) || !self.writable(to) {
                    return fail(format!("mv: can't rename '{from}': Read-only file system"));
                }
                let moved: Vec<(String, MemoryNode)> = self
                    .nodes
                    .iter()
                    .filter(|(path, _)| {
                        path.as_str() == *from
                            || path.strip_prefix(*from).is_some_and(|r| r.starts_with('/'))
                    })
                    .map(|(p, n)| (p.clone(), n.clone()))
                    .collect();
                if moved.is_empty() {
                    return fail(format!(
                        "mv: can't rename '{from}': No such file or directory"
                    ));
                }
                for (path, node) in moved {
                    self.nodes.remove(&path);
                    let rest = path.strip_prefix(*from).unwrap_or_default();
                    self.nodes.insert(format!("{to}{rest}"), node);
                }
                ok(Vec::new())
            }
            ["rm", "-r", "-f", "--", path] => {
                if !self.writable(path) {
                    return fail(format!("rm: can't remove '{path}': Read-only file system"));
                }
                self.nodes.retain(|p, _| {
                    p.as_str() != *path
                        && !p.strip_prefix(*path).is_some_and(|r| r.starts_with('/'))
                });
                ok(Vec::new())
            }
            _ => (ExitStatus::CommandNotFound, Vec::new(), Vec::new()),
        }
    }
}
