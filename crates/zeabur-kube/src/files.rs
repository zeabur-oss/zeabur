//! A container's files, read and written through `pods/exec` as the person.
//!
//! As `kubectl cp` does, `tar` packs a file on the way out and unpacks one on the way in
//! [kubectl-cp]. A folder is listed with `find` and `stat`, which GNU coreutils, findutils and
//! BusyBox all have.
//!
//! # No shell
//!
//! Names come from people and from the container itself, so none is ever put into a command
//! line that a shell parses. Every command is an argument vector that kube-apiserver passes to
//! the container runtime as it is (`command=` once per argument [k8s-exec]); a path is always
//! absolute, so it never starts with `-`, and an archive member is written `./name`. Names may
//! hold spaces, newlines, quotes or any other byte but `/` and NUL:
//!
//! - `find -print0` ends each name with NUL, the one byte a name cannot hold.
//! - `stat` prints its numbers before the path it was given, and the path is checked against the
//!   one asked for, so a name with a line break cannot pass for another entry.
//! - A file goes into the container inside a tar archive Zeabur writes itself, on standard input.
//!
//! [kubectl-cp]: https://kubernetes.io/docs/reference/kubectl/generated/kubectl_cp/
//! [k8s-exec]: https://kubernetes.io/docs/reference/kubernetes-api/workload-resources/pod-v1/#connect-get-exec-pod-v1-core

use std::pin::Pin;

use futures_util::{Stream, StreamExt as _};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};

use crate::port::{Cluster, ClusterError, ExecStream, ExitStatus};

/// The longest path accepted, as Linux's `PATH_MAX`.
const MAX_PATH: usize = 4096;
/// The longest name accepted, as Linux's `NAME_MAX`.
const MAX_NAME: usize = 255;
/// The most of a folder's names read before it counts as larger than can be listed.
const MAX_NAMES_OUTPUT: usize = 16 * 1024 * 1024;
/// The most bytes of paths one `stat` is given; kube-apiserver carries them in the URL.
const STAT_BATCH_BYTES: usize = 24 * 1024;
/// The most paths one `stat` is given.
const STAT_BATCH: usize = 200;
/// The `stat` calls of one listing at once.
const STAT_CONCURRENCY: usize = 4;
/// What is kept of standard error, for the message.
const MAX_ERROR: usize = 4096;
const BLOCK: usize = 512;

/// An absolute path in a container: no `.` or `..` component, no NUL, no empty component, no
/// trailing `/` but for the root itself.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FilePath(String);

impl FilePath {
    /// The root, `/`.
    #[must_use]
    pub fn root() -> Self {
        Self("/".to_owned())
    }

    /// `path`, when it is one (see [`FilePath`]); a trailing `/` and repeated `/` are dropped.
    #[must_use]
    pub fn parse(path: &str) -> Option<Self> {
        if !path.starts_with('/') || path.len() > MAX_PATH || path.contains('\0') {
            return None;
        }
        let mut parts = Vec::new();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            if !valid_name(part) {
                return None;
            }
            parts.push(part);
        }
        Some(Self(format!("/{}", parts.join("/"))))
    }

    /// The path as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is `/`.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0 == "/"
    }

    /// The folder it is in; `None` for `/`.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        let cut = self.0.rfind('/')?;
        if self.is_root() {
            return None;
        }
        Some(Self(
            self.0
                .get(..cut)
                .filter(|p| !p.is_empty())
                .unwrap_or("/")
                .to_owned(),
        ))
    }

    /// Its last component; `None` for `/`.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        let cut = self.0.rfind('/')?;
        self.0.get(cut + 1..).filter(|n| !n.is_empty())
    }

    /// `name` inside this folder, when `name` is one ([`valid_name`]).
    #[must_use]
    pub fn join(&self, name: &str) -> Option<Self> {
        if !valid_name(name) {
            return None;
        }
        let joined = if self.is_root() {
            format!("/{name}")
        } else {
            format!("{}/{name}", self.0)
        };
        (joined.len() <= MAX_PATH).then_some(Self(joined))
    }

    /// Whether this is `folder` or inside it.
    #[must_use]
    pub fn is_within(&self, folder: &Self) -> bool {
        folder.is_root()
            || self.0 == folder.0
            || self
                .0
                .strip_prefix(folder.0.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    }
}

impl std::fmt::Display for FilePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `name` can be one entry of a folder: not empty, not `.` or `..`, no `/` or NUL, at
/// most 255 bytes. Anything else goes, a leading `-`, spaces and line breaks included.
#[must_use]
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= MAX_NAME
        && !name.contains(['/', '\0'])
}

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A folder.
    Directory,
    /// A symbolic link.
    Link,
    /// A device, socket or pipe.
    Other,
}

impl EntryKind {
    /// From `st_mode`'s file type bits [inode].
    ///
    /// [inode]: https://man7.org/linux/man-pages/man7/inode.7.html
    #[must_use]
    pub const fn from_mode(mode: u32) -> Self {
        match mode & 0o170_000 {
            0o100_000 => Self::File,
            0o040_000 => Self::Directory,
            0o120_000 => Self::Link,
            _ => Self::Other,
        }
    }
}

/// What `stat` says of a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    /// What it is.
    pub kind: EntryKind,
    /// Its size in bytes.
    pub size: u64,
    /// When it was last modified, in seconds since 1970.
    pub modified: i64,
}

/// One entry of a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its name.
    pub name: String,
    /// What it is.
    pub kind: EntryKind,
    /// For a link, what it points to; `None` when that does not exist.
    pub target: Option<EntryKind>,
    /// Its size, or for a link its target's.
    pub size: u64,
    /// When it was last modified.
    pub modified: i64,
}

/// A folder's entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    /// The first entries by name, at most the limit asked for.
    pub entries: Vec<Entry>,
    /// How many entries the folder has (that match the filter).
    pub total: usize,
    /// The folder has more names than Zeabur reads; `total` counts those read.
    pub more: bool,
}

/// Why a command failed, from what it wrote to standard error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// No such file or folder.
    NotFound,
    /// The container's user may not.
    PermissionDenied,
    /// The file system is mounted read-only.
    ReadOnly,
    /// Something of that name is already there.
    Exists,
    /// A path that should be a folder is not.
    NotADirectory,
    /// A path that should be a file is a folder.
    IsADirectory,
    /// The volume is full.
    NoSpace,
    /// Anything else.
    Other,
}

impl Failure {
    /// The failure the messages of coreutils, findutils, BusyBox and the C library name.
    #[must_use]
    pub fn of(message: &str) -> Self {
        let has = |text: &str| message.contains(text);
        if has("No such file") || has("can't open") || has("cannot stat") {
            Self::NotFound
        } else if has("Permission denied") || has("Operation not permitted") {
            Self::PermissionDenied
        } else if has("Read-only file system") {
            Self::ReadOnly
        } else if has("File exists") {
            Self::Exists
        } else if has("Not a directory") {
            Self::NotADirectory
        } else if has("Is a directory") {
            Self::IsADirectory
        } else if has("No space left") || has("Disk quota exceeded") {
            Self::NoSpace
        } else {
            Self::Other
        }
    }
}

/// Why a file operation failed.
#[derive(Debug, thiserror::Error)]
pub enum FileError {
    /// kube-apiserver refused or could not open the stream.
    #[error(transparent)]
    Cluster(#[from] ClusterError),
    /// The image does not have the command (a distroless image has no `tar`).
    #[error("the container has no {0}")]
    CommandNotFound(String),
    /// The command failed; `message` is what it wrote to standard error.
    #[error("{message}")]
    Failed {
        /// Why, as far as the message tells.
        failure: Failure,
        /// Standard error, or the runtime's message.
        message: String,
    },
    /// A file was asked for, and the path is something else.
    #[error("not a file")]
    NotAFile(EntryKind),
    /// The file is larger than the limit.
    #[error("{size} bytes is more than {limit}")]
    TooLarge {
        /// Its size.
        size: u64,
        /// The limit.
        limit: u64,
    },
    /// The stream did not carry what the command writes.
    #[error("unexpected output: {0}")]
    Protocol(String),
}

impl FileError {
    fn failed(message: String) -> Self {
        Self::Failed {
            failure: Failure::of(&message),
            message,
        }
    }
}

/// The container a command runs in.
#[derive(Clone, Copy)]
pub struct Target<'a> {
    /// The person's cluster.
    pub cluster: &'a dyn Cluster,
    /// The pod's namespace.
    pub namespace: &'a str,
    /// The pod.
    pub pod: &'a str,
    /// The container.
    pub container: &'a str,
}

impl std::fmt::Debug for Target<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Target")
            .field("namespace", &self.namespace)
            .field("pod", &self.pod)
            .field("container", &self.container)
            .finish_non_exhaustive()
    }
}

// ── Commands ─────────────────────────────────────────────────────────────────────────────────

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| (*p).to_owned()).collect()
}

/// Whether the image has `tar`: any answer but "not found" says so.
#[must_use]
pub fn probe_command() -> Vec<String> {
    argv(&["tar", "--help"])
}

/// The names in `folder`, each ended by NUL. `folder/.` makes `find` follow a folder that is a
/// link (`/var/run` → `/run`).
#[must_use]
pub fn names_command(folder: &FilePath) -> Vec<String> {
    argv(&[
        "find",
        &dot(folder),
        "-mindepth",
        "1",
        "-maxdepth",
        "1",
        "-print0",
    ])
}

fn dot(folder: &FilePath) -> String {
    if folder.is_root() {
        "/.".to_owned()
    } else {
        format!("{folder}/.")
    }
}

/// The type bits, size and time of each path, one line each, the path last. `follow` reads what
/// a link points to instead of the link.
#[must_use]
pub fn stat_command(paths: &[FilePath], follow: bool) -> Vec<String> {
    let mut command = argv(&["stat"]);
    if follow {
        command.push("-L".to_owned());
    }
    command.extend(argv(&["-c", "%f %s %Y %n", "--"]));
    command.extend(paths.iter().map(|p| p.as_str().to_owned()));
    command
}

/// The path with every link resolved.
#[must_use]
pub fn resolve_command(path: &FilePath) -> Vec<String> {
    argv(&["readlink", "-f", "--", path.as_str()])
}

/// A tar archive of `path` alone on standard output, as `kubectl cp` reads a file
/// (`tar cf - …`); links are archived as links, not followed.
#[must_use]
pub fn pack_command(path: &FilePath) -> Option<Vec<String>> {
    let parent = path.parent()?;
    let name = path.name()?;
    Some(argv(&[
        "tar",
        "-c",
        "-f",
        "-",
        "-C",
        parent.as_str(),
        &format!("./{name}"),
    ]))
}

/// Unpacks the archive on standard input into `folder`, as `kubectl cp` writes (`tar -xmf -`),
/// owned by the container's user rather than the archive's (`-o`).
#[must_use]
pub fn unpack_command(folder: &FilePath) -> Vec<String> {
    argv(&["tar", "-x", "-m", "-o", "-f", "-", "-C", folder.as_str()])
}

/// Makes the folder `path`.
#[must_use]
pub fn make_dir_command(path: &FilePath) -> Vec<String> {
    argv(&["mkdir", "--", path.as_str()])
}

/// Renames `from` to `to`.
#[must_use]
pub fn rename_command(from: &FilePath, to: &FilePath) -> Vec<String> {
    argv(&["mv", "--", from.as_str(), to.as_str()])
}

/// Deletes `path`, and everything in it when it is a folder.
#[must_use]
pub fn remove_command(path: &FilePath) -> Vec<String> {
    argv(&["rm", "-r", "-f", "--", path.as_str()])
}

// ── Parsing ──────────────────────────────────────────────────────────────────────────────────

/// The names `find -print0` printed for `folder` ([`names_command`]); anything not under it is
/// left out.
#[must_use]
pub fn parse_names(output: &[u8], folder: &FilePath) -> Vec<String> {
    let prefix = format!("{}/", dot(folder));
    output
        .split(|b| *b == 0)
        .filter_map(|item| {
            let item = String::from_utf8_lossy(item);
            item.strip_prefix(prefix.as_str())
                .filter(|name| valid_name(name))
                .map(str::to_owned)
        })
        .collect()
}

/// What `stat` printed for `paths` ([`stat_command`]), in their order.
///
/// `None` for a path it printed nothing for (gone since it was listed). Each line must end with
/// the path asked for,
/// so a line break inside a name cannot shift the others.
#[must_use]
pub fn parse_stats(output: &[u8], paths: &[FilePath]) -> Vec<Option<Stat>> {
    let mut rest = output;
    paths
        .iter()
        .map(|path| {
            let parsed = parse_stat_line(rest, path.as_str().as_bytes());
            if let Some((stat, after)) = parsed {
                rest = after;
                Some(stat)
            } else {
                None
            }
        })
        .collect()
}

fn parse_stat_line<'a>(input: &'a [u8], path: &[u8]) -> Option<(Stat, &'a [u8])> {
    let (mode, input) = token(input)?;
    let (size, input) = token(input)?;
    let (modified, input) = token(input)?;
    let input = input.strip_prefix(path)?.strip_prefix(b"\n")?;
    Some((
        Stat {
            kind: EntryKind::from_mode(u32::from_str_radix(mode, 16).ok()?),
            size: size.parse().ok()?,
            modified: modified.parse().ok()?,
        },
        input,
    ))
}

fn token(input: &[u8]) -> Option<(&str, &[u8])> {
    let end = input.iter().position(|b| *b == b' ')?;
    let (word, rest) = input.split_at(end);
    let word = std::str::from_utf8(word).ok()?;
    let valid = !word.is_empty() && word.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
    valid.then_some((word, rest.get(1..)?))
}

/// Folders first, then by name without regard to case.
fn by_name(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

// ── Tar ──────────────────────────────────────────────────────────────────────────────────────

/// One header of a tar archive, as the POSIX ustar format lays it out [ustar], with the GNU
/// magic and the GNU long name entry (`L`) for a name longer than 99 bytes, which GNU tar and
/// BusyBox both read [gnu-tar].
///
/// [ustar]: https://pubs.opengroup.org/onlinepubs/9799919799/utilities/pax.html#tag_20_94_13_06
/// [gnu-tar]: https://www.gnu.org/software/tar/manual/html_node/Standard.html
fn header(name: &[u8], size: u64, kind: u8, modified: u64) -> [u8; BLOCK] {
    fn put_at(block: &mut [u8; BLOCK], at: usize, bytes: &[u8]) {
        if let Some(field) = block.get_mut(at..at + bytes.len()) {
            field.copy_from_slice(bytes);
        }
    }
    let mut block = [0_u8; BLOCK];
    put_at(&mut block, 0, name.get(..99).unwrap_or(name));
    put_at(&mut block, 100, b"0000644\0");
    put_at(&mut block, 108, b"0000000\0");
    put_at(&mut block, 116, b"0000000\0");
    put_at(&mut block, 124, format!("{size:011o}\0").as_bytes());
    put_at(&mut block, 136, format!("{modified:011o}\0").as_bytes());
    put_at(&mut block, 148, b"        ");
    put_at(&mut block, 156, &[kind]);
    put_at(&mut block, 257, b"ustar  \0");
    let sum: u32 = block.iter().map(|b| u32::from(*b)).sum();
    put_at(&mut block, 148, format!("{sum:06o}\0 ").as_bytes());
    block
}

fn padding(size: u64) -> usize {
    let rest = usize::try_from(size % BLOCK as u64).unwrap_or(0);
    if rest == 0 { 0 } else { BLOCK - rest }
}

/// The headers that come before a file of `name` and `size` in an archive.
#[must_use]
pub fn file_header(name: &str, size: u64, modified: u64) -> Vec<u8> {
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(3 * BLOCK);
    if bytes.len() > 99 {
        let long = u64::try_from(bytes.len() + 1).unwrap_or(u64::MAX);
        out.extend_from_slice(&header(b"././@LongLink", long, b'L', 0));
        out.extend_from_slice(bytes);
        out.push(0);
        out.resize(out.len() + padding(long), 0);
    }
    out.extend_from_slice(&header(bytes, size, b'0', modified));
    out
}

/// What follows a file of `size` bytes to the end of the archive: its padding, then two empty
/// blocks.
#[must_use]
pub fn archive_end(size: u64) -> Vec<u8> {
    vec![0; padding(size) + 2 * BLOCK]
}

/// The type and size of a header block's entry.
fn parse_header(block: &[u8; BLOCK]) -> Result<(u8, u64), FileError> {
    if block.iter().all(|b| *b == 0) {
        return Err(FileError::Protocol("the archive is empty".to_owned()));
    }
    let field = block.get(124..136).unwrap_or_default();
    let size = if field.first().is_some_and(|b| b & 0x80 != 0) {
        // GNU's base-256 for sizes of 8 GiB and more.
        field.iter().skip(1).fold(0_u64, |n, b| {
            n.saturating_mul(256).saturating_add(u64::from(*b))
        })
    } else {
        let text: String = field
            .iter()
            .map(|b| char::from(*b))
            .filter(char::is_ascii_digit)
            .collect();
        u64::from_str_radix(if text.is_empty() { "0" } else { &text }, 8)
            .map_err(|e| FileError::Protocol(format!("a size that is not a number ({e})")))?
    };
    Ok((block.get(156).copied().unwrap_or(0), size))
}

fn pax_size(records: &[u8]) -> Option<u64> {
    String::from_utf8_lossy(records).lines().find_map(|line| {
        let (_, record) = line.split_once(' ')?;
        record.strip_prefix("size=")?.parse().ok()
    })
}

async fn read_block(
    input: &mut (impl AsyncRead + Unpin + ?Sized),
) -> Result<[u8; BLOCK], FileError> {
    let mut block = [0_u8; BLOCK];
    input
        .read_exact(&mut block)
        .await
        .map_err(|e| FileError::Protocol(format!("the archive ended early ({e})")))?;
    Ok(block)
}

/// Reads the headers of the archive's first entry, skipping pax and GNU long name entries; its
/// type flag and size. `None` when the stream ends before a header.
async fn first_entry(
    input: &mut (impl AsyncRead + Unpin + ?Sized),
) -> Result<(u8, u64), FileError> {
    let mut size_override = None;
    loop {
        let block = read_block(input).await?;
        let (kind, size) = parse_header(&block)?;
        match kind {
            b'x' | b'g' | b'L' | b'K' => {
                let length = usize::try_from(size)
                    .ok()
                    .filter(|s| *s <= 1024 * 1024)
                    .ok_or_else(|| FileError::Protocol("a header too long".to_owned()))?;
                let mut data = vec![0_u8; length + padding(size)];
                input
                    .read_exact(&mut data)
                    .await
                    .map_err(|e| FileError::Protocol(format!("the archive ended early ({e})")))?;
                if kind == b'x' {
                    size_override = pax_size(data.get(..length).unwrap_or_default());
                }
            }
            _ => return Ok((kind, size_override.unwrap_or(size))),
        }
    }
}

const fn entry_kind(flag: u8) -> EntryKind {
    match flag {
        b'0' | 0 | b'7' => EntryKind::File,
        b'5' => EntryKind::Directory,
        b'2' | b'1' => EntryKind::Link,
        _ => EntryKind::Other,
    }
}

// ── Running ──────────────────────────────────────────────────────────────────────────────────

struct Output {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: String,
    /// Standard output was longer than asked for; the process was left.
    cut: bool,
}

async fn read_capped(
    reader: &mut (impl AsyncRead + Unpin + ?Sized),
    cap: usize,
) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => return (out, false),
            Ok(n) => {
                out.extend_from_slice(buffer.get(..n).unwrap_or_default());
                if out.len() > cap {
                    out.truncate(cap);
                    return (out, true);
                }
            }
        }
    }
}

fn error_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}

async fn open(target: Target<'_>, command: &[String]) -> Result<ExecStream, FileError> {
    Ok(target
        .cluster
        .stream(target.namespace, target.pod, target.container, command)
        .await?)
}

fn not_found(command: &[String]) -> FileError {
    FileError::CommandNotFound(command.first().cloned().unwrap_or_default())
}

async fn run(target: Target<'_>, command: &[String], cap: usize) -> Result<Output, FileError> {
    let ExecStream {
        input,
        mut output,
        mut error,
        exit,
    } = open(target, command).await?;
    drop(input);
    let ((stdout, cut), (stderr, _)) = tokio::join!(
        read_capped(&mut output, cap),
        read_capped(&mut error, MAX_ERROR)
    );
    if cut {
        return Ok(Output {
            status: ExitStatus::Exited(0),
            stdout,
            stderr: String::new(),
            cut,
        });
    }
    let status = exit.await;
    if status == ExitStatus::CommandNotFound {
        return Err(not_found(command));
    }
    Ok(Output {
        status,
        stdout,
        stderr: error_text(&stderr),
        cut,
    })
}

/// [`run`], failing unless the command exits with `0`.
async fn succeed(target: Target<'_>, command: &[String], cap: usize) -> Result<Vec<u8>, FileError> {
    let output = run(target, command, cap).await?;
    match output.status {
        ExitStatus::Exited(0) => Ok(output.stdout),
        ExitStatus::Exited(_) => Err(FileError::failed(output.stderr)),
        ExitStatus::Failed(message) => Err(FileError::failed(if output.stderr.is_empty() {
            message
        } else {
            output.stderr
        })),
        ExitStatus::CommandNotFound => Err(not_found(command)),
    }
}

/// Whether the container has `tar`; [`FileError::CommandNotFound`] when it has not.
///
/// # Errors
///
/// When it has not, or the stream cannot be opened.
pub async fn probe(target: Target<'_>) -> Result<(), FileError> {
    run(target, &probe_command(), 0).await.map(drop)
}

/// The entries of `folder` whose name contains `filter` (ignoring case): the first `limit` by
/// name, sorted folders first, and how many there are.
///
/// # Errors
///
/// When a command is missing or fails (the folder does not exist, or the container's user may
/// not read it).
pub async fn list(
    target: Target<'_>,
    folder: &FilePath,
    filter: Option<&str>,
    limit: usize,
) -> Result<Listing, FileError> {
    let output = run(target, &names_command(folder), MAX_NAMES_OUTPUT).await?;
    if !output.cut && output.status != ExitStatus::Exited(0) && output.stdout.is_empty() {
        return Err(FileError::failed(match output.status {
            ExitStatus::Failed(message) if output.stderr.is_empty() => message,
            ExitStatus::Exited(_) | ExitStatus::CommandNotFound | ExitStatus::Failed(_) => {
                output.stderr
            }
        }));
    }
    let mut names = parse_names(&output.stdout, folder);
    if let Some(filter) = filter.map(str::to_lowercase).filter(|f| !f.is_empty()) {
        names.retain(|name| name.to_lowercase().contains(&filter));
    }
    names.sort_by(|a, b| by_name(a, b));
    let total = names.len();
    names.truncate(limit);
    let paths: Vec<FilePath> = names.iter().filter_map(|n| folder.join(n)).collect();
    let stats = stat_all(target, &paths, false).await?;
    let links: Vec<FilePath> = paths
        .iter()
        .zip(&stats)
        .filter(|(_, stat)| stat.is_some_and(|s| s.kind == EntryKind::Link))
        .map(|(path, _)| path.clone())
        .collect();
    let targets = stat_all(target, &links, true).await?;
    let mut followed = links.iter().zip(targets);
    let mut entries: Vec<Entry> = paths
        .iter()
        .zip(stats)
        .filter_map(|(path, stat)| {
            let stat = stat?;
            let pointed = (stat.kind == EntryKind::Link)
                .then(|| {
                    followed
                        .find(|(link, _)| *link == path)
                        .and_then(|(_, target)| target)
                })
                .flatten();
            Some(Entry {
                name: path.name()?.to_owned(),
                kind: stat.kind,
                target: pointed.map(|t| t.kind),
                size: pointed.map_or(stat.size, |t| t.size),
                modified: stat.modified,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        let folder =
            |e: &Entry| e.kind != EntryKind::Directory && e.target != Some(EntryKind::Directory);
        folder(a)
            .cmp(&folder(b))
            .then_with(|| by_name(&a.name, &b.name))
    });
    Ok(Listing {
        entries,
        total,
        more: output.cut,
    })
}

fn batches(paths: &[FilePath]) -> Vec<&[FilePath]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut bytes = 0;
    for (index, path) in paths.iter().enumerate() {
        if index > start && (index - start >= STAT_BATCH || bytes + path.0.len() > STAT_BATCH_BYTES)
        {
            out.extend(paths.get(start..index));
            start = index;
            bytes = 0;
        }
        bytes += path.0.len() + 1;
    }
    if start < paths.len() {
        out.extend(paths.get(start..));
    }
    out
}

async fn stat_all(
    target: Target<'_>,
    paths: &[FilePath],
    follow: bool,
) -> Result<Vec<Option<Stat>>, FileError> {
    let answers: Vec<Result<Vec<Option<Stat>>, FileError>> =
        futures_util::stream::iter(batches(paths))
            .map(|batch| async move {
                // `stat` exits 1 when a path has gone; the others are still printed.
                let output = run(target, &stat_command(batch, follow), usize::MAX).await?;
                Ok(parse_stats(&output.stdout, batch))
            })
            .buffered(STAT_CONCURRENCY)
            .collect()
            .await;
    let mut all = Vec::with_capacity(paths.len());
    for answer in answers {
        all.extend(answer?);
    }
    Ok(all)
}

/// What `path` is; `None` when it does not exist. Links are not followed.
///
/// # Errors
///
/// When `stat` is missing, or the stream cannot be opened.
pub async fn stat(target: Target<'_>, path: &FilePath) -> Result<Option<Stat>, FileError> {
    let paths = [path.clone()];
    Ok(stat_all(target, &paths, false)
        .await?
        .into_iter()
        .flatten()
        .next())
}

/// `path` with every link in it resolved.
///
/// # Errors
///
/// When the path does not exist or a command fails.
pub async fn resolve(target: Target<'_>, path: &FilePath) -> Result<FilePath, FileError> {
    let output = succeed(target, &resolve_command(path), MAX_PATH + 1).await?;
    let text = String::from_utf8_lossy(&output);
    let text = text.strip_suffix('\n').unwrap_or(&text);
    FilePath::parse(text).ok_or_else(|| FileError::Protocol(format!("readlink printed {text:?}")))
}

/// A file being read: its size, then its bytes as they arrive.
pub struct Download {
    /// Its size in bytes.
    pub size: u64,
    /// Its contents, `size` bytes in all; the stream to the container closes after the last.
    pub body: Pin<Box<dyn Stream<Item = Result<Vec<u8>, std::io::Error>> + Send>>,
}

impl std::fmt::Debug for Download {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Download")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// Starts reading the regular file `path` out of a tar archive of it.
///
/// Refused with [`FileError::TooLarge`] past `limit` bytes before any of it is read. Links are
/// archived as links, so resolve `path` first ([`resolve`]).
///
/// # Errors
///
/// When the file is missing or not a file, too large, or a command fails.
pub async fn download(
    target: Target<'_>,
    path: &FilePath,
    limit: u64,
) -> Result<Download, FileError> {
    let command = pack_command(path).ok_or(FileError::NotAFile(EntryKind::Directory))?;
    let ExecStream {
        input,
        mut output,
        mut error,
        exit,
    } = open(target, &command).await?;
    drop(input);
    let header = first_entry(&mut output).await;
    let (flag, size) = match header {
        Ok(found) => found,
        Err(protocol) => {
            // An archive that never started: the command failed or is missing.
            let (stderr, _) = read_capped(&mut error, MAX_ERROR).await;
            return Err(match exit.await {
                ExitStatus::CommandNotFound => not_found(&command),
                ExitStatus::Exited(0) => protocol,
                ExitStatus::Exited(_) | ExitStatus::Failed(_) => {
                    FileError::failed(error_text(&stderr))
                }
            });
        }
    };
    match entry_kind(flag) {
        EntryKind::File => {}
        kind @ (EntryKind::Directory | EntryKind::Link | EntryKind::Other) => {
            return Err(FileError::NotAFile(kind));
        }
    }
    if size > limit {
        return Err(FileError::TooLarge { size, limit });
    }
    tokio::spawn(async move {
        let mut sink = tokio::io::sink();
        // Standard error must be read for output to keep flowing; its words are not needed.
        let _copied = tokio::io::copy(&mut error, &mut sink).await;
    });
    let state = (output, size, Some(exit));
    let body = futures_util::stream::unfold(state, |(mut output, left, exit)| async move {
        if left == 0 {
            drop(exit);
            return None;
        }
        let mut buffer = vec![0_u8; usize::try_from(left.min(64 * 1024)).unwrap_or(64 * 1024)];
        match output.read(&mut buffer).await {
            Ok(0) => Some((
                Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "the file ended early",
                )),
                (output, 0, None),
            )),
            Ok(n) => {
                buffer.truncate(n);
                Some((Ok(buffer), (output, left - n as u64, exit)))
            }
            Err(e) => Some((Err(e), (output, 0, None))),
        }
    });
    Ok(Download {
        size,
        body: Box::pin(body),
    })
}

/// Reads the regular file `path` whole, up to `limit` bytes.
///
/// # Errors
///
/// As [`download`].
pub async fn read(target: Target<'_>, path: &FilePath, limit: u64) -> Result<Vec<u8>, FileError> {
    let download = download(target, path, limit).await?;
    let mut out = Vec::with_capacity(usize::try_from(download.size).unwrap_or(0));
    let mut body = download.body;
    while let Some(chunk) = body.next().await {
        out.extend(chunk.map_err(|e| FileError::Protocol(e.to_string()))?);
    }
    Ok(out)
}

/// Writes `body`, `size` bytes, as the file `name` in `folder` (replacing one of that name),
/// inside a tar archive on standard input.
///
/// # Errors
///
/// When `body` is not `size` bytes or fails, or the command fails.
pub async fn upload<E: std::fmt::Display>(
    target: Target<'_>,
    folder: &FilePath,
    name: &str,
    size: u64,
    mut body: Pin<Box<dyn Stream<Item = Result<Vec<u8>, E>> + Send + '_>>,
) -> Result<(), FileError> {
    if !valid_name(name) {
        return Err(FileError::Protocol(format!("{name:?} is not a name")));
    }
    let command = unpack_command(folder);
    let ExecStream {
        mut input,
        mut output,
        mut error,
        exit,
    } = open(target, &command).await?;
    let modified = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let write = async {
        input
            .write_all(&file_header(name, size, modified))
            .await
            .map_err(|e| FileError::failed(e.to_string()))?;
        let mut sent = 0_u64;
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|e| FileError::Protocol(e.to_string()))?;
            sent += chunk.len() as u64;
            if sent > size {
                return Err(FileError::Protocol("more bytes than announced".to_owned()));
            }
            input
                .write_all(&chunk)
                .await
                .map_err(|e| FileError::failed(e.to_string()))?;
        }
        if sent != size {
            return Err(FileError::Protocol(format!(
                "{sent} bytes of the {size} announced"
            )));
        }
        input
            .write_all(&archive_end(size))
            .await
            .map_err(|e| FileError::failed(e.to_string()))?;
        input
            .shutdown()
            .await
            .map_err(|e| FileError::failed(e.to_string()))?;
        drop(input);
        Ok(())
    };
    let (written, _, (stderr, _)) = tokio::join!(
        write,
        read_capped(&mut output, MAX_ERROR),
        read_capped(&mut error, MAX_ERROR)
    );
    // A missing tar or a failed unpack says more than the broken pipe it leaves the writer.
    let status = exit.await;
    match status {
        ExitStatus::CommandNotFound => Err(not_found(&command)),
        ExitStatus::Exited(0) => written,
        ExitStatus::Exited(_) | ExitStatus::Failed(_) if !stderr.is_empty() => {
            Err(FileError::failed(error_text(&stderr)))
        }
        ExitStatus::Exited(_) => written.and_then(|()| Err(FileError::failed(String::new()))),
        ExitStatus::Failed(message) => written.and_then(|()| Err(FileError::failed(message))),
    }
}

/// Makes the folder `path`.
///
/// # Errors
///
/// When something of that name exists or the command fails.
pub async fn make_dir(target: Target<'_>, path: &FilePath) -> Result<(), FileError> {
    succeed(target, &make_dir_command(path), 0).await.map(drop)
}

/// Renames `from` to `to`, refused when `to` exists (`mv` would replace a file, or move into a
/// folder).
///
/// # Errors
///
/// When `to` exists, `from` does not, or the command fails.
pub async fn rename(target: Target<'_>, from: &FilePath, to: &FilePath) -> Result<(), FileError> {
    if stat(target, to).await?.is_some() {
        return Err(FileError::Failed {
            failure: Failure::Exists,
            message: format!("{to} exists"),
        });
    }
    succeed(target, &rename_command(from, to), 0)
        .await
        .map(drop)
}

/// Deletes `path`, a folder with everything in it.
///
/// # Errors
///
/// When the command fails.
pub async fn remove(target: Target<'_>, path: &FilePath) -> Result<(), FileError> {
    succeed(target, &remove_command(path), 0).await.map(drop)
}

/// Reads one tar archive of a single file from `input`: its name and contents. For the in-memory
/// cluster and tests.
///
/// # Errors
///
/// When the archive is not one file.
pub async fn read_archive(
    input: &mut (impl AsyncRead + Unpin + ?Sized),
) -> Result<(String, Vec<u8>), FileError> {
    let mut name = None;
    loop {
        let block = read_block(input).await?;
        let (kind, size) = parse_header(&block)?;
        let length =
            usize::try_from(size).map_err(|e| FileError::Protocol(format!("too large ({e})")))?;
        let mut data = vec![0_u8; length + padding(size)];
        input
            .read_exact(&mut data)
            .await
            .map_err(|e| FileError::Protocol(format!("the archive ended early ({e})")))?;
        data.truncate(length);
        if kind == b'L' {
            let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
            name = Some(String::from_utf8_lossy(data.get(..end).unwrap_or_default()).into_owned());
            continue;
        }
        let short = block.get(..100).unwrap_or_default();
        let end = short.iter().position(|b| *b == 0).unwrap_or(short.len());
        let name = name.unwrap_or_else(|| {
            String::from_utf8_lossy(short.get(..end).unwrap_or_default()).into_owned()
        });
        return Ok((name, data));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(p: &str) -> FilePath {
        FilePath::parse(p).unwrap()
    }

    const ODD: [&str; 8] = [
        "-rf",
        "--help",
        "with space",
        "line\nbreak",
        "quote'\"`$(x)",
        "日本語.txt",
        "tab\tand;semicolon|pipe&",
        "*?[glob]",
    ];

    #[test]
    fn paths_are_absolute_and_never_climb() {
        assert_eq!(path("/data//uploads/").as_str(), "/data/uploads");
        assert_eq!(path("/").as_str(), "/");
        for bad in ["", "data", "/data/../etc", "/./data", "/a\0b", "../x"] {
            assert_eq!(FilePath::parse(bad), None, "{bad:?}");
        }
        assert_eq!(path("/data/a").parent(), Some(path("/data")));
        assert_eq!(path("/data").parent(), Some(FilePath::root()));
        assert_eq!(FilePath::root().parent(), None);
        assert_eq!(path("/data/a b").name(), Some("a b"));
        assert!(path("/data/x").is_within(&path("/data")));
        assert!(!path("/database").is_within(&path("/data")));
        assert!(path("/anything").is_within(&FilePath::root()));
        assert_eq!(path("/data").join(".."), None);
        assert_eq!(path("/data").join("a/b"), None);
    }

    /// Every odd name stays one argument of its own, after the options; none can become an
    /// option (`-rf`) or be split, quoted or expanded, since there is no shell.
    #[test]
    fn odd_names_stay_single_arguments() {
        let folder = path("/data");
        for name in ODD {
            assert!(valid_name(name), "{name:?}");
            let file = folder.join(name).unwrap();
            for command in [
                stat_command(std::slice::from_ref(&file), false),
                resolve_command(&file),
                pack_command(&file).unwrap(),
                make_dir_command(&file),
                rename_command(&file, &folder.join("new").unwrap()),
                remove_command(&file),
            ] {
                let arg = command.iter().find(|a| a.ends_with(name)).unwrap();
                assert!(arg.starts_with('/') || arg.starts_with("./"), "{command:?}");
                assert_ne!(command.first().map(String::as_str), Some("sh"));
                assert!(
                    !command
                        .iter()
                        .any(|a| a == "-c" && command.first().unwrap() == "sh")
                );
            }
            assert_eq!(
                pack_command(&file).unwrap(),
                ["tar", "-c", "-f", "-", "-C", "/data", &format!("./{name}")]
            );
            assert_eq!(
                remove_command(&file),
                ["rm", "-r", "-f", "--", file.as_str()]
            );
        }
    }

    #[test]
    fn find_names_are_read_up_to_nul_whatever_they_hold() {
        let folder = path("/data");
        let mut output = Vec::new();
        for name in ODD {
            output.extend_from_slice(format!("/data/./{name}\0").as_bytes());
        }
        output.extend_from_slice(b"/elsewhere/x\0");
        assert_eq!(parse_names(&output, &folder), ODD);
        assert_eq!(
            names_command(&FilePath::root()).get(1).map(String::as_str),
            Some("/.")
        );
        assert_eq!(
            parse_names(b"/./etc\0/./bin\0", &FilePath::root()),
            ["etc", "bin"]
        );
    }

    /// A name holding a line break that looks like another entry cannot shift or invent one.
    #[test]
    fn stat_lines_must_end_with_the_path_asked_for() {
        let spoof = "a\n81a4 9 0 b";
        let paths = [
            path("/data").join(spoof).unwrap(),
            path("/data/b"),
            path("/data/gone"),
            path("/data/c"),
        ];
        let output =
            format!("81a4 3 100 /data/{spoof}\n41ed 4096 200 /data/b\na1ff 7 300 /data/c\n");
        let stats = parse_stats(output.as_bytes(), &paths);
        assert_eq!(
            stats,
            [
                Some(Stat {
                    kind: EntryKind::File,
                    size: 3,
                    modified: 100
                }),
                Some(Stat {
                    kind: EntryKind::Directory,
                    size: 4096,
                    modified: 200
                }),
                None,
                Some(Stat {
                    kind: EntryKind::Link,
                    size: 7,
                    modified: 300
                }),
            ]
        );
    }

    #[test]
    fn stat_batches_keep_every_path_once_and_in_order() {
        let paths: Vec<FilePath> = (0..450).map(|i| path(&format!("/d/{i:0200}"))).collect();
        let split = batches(&paths);
        assert!(split.iter().all(|b| b.len() <= STAT_BATCH));
        assert!(
            split
                .iter()
                .all(|b| b.iter().map(|p| p.0.len() + 1).sum::<usize>() <= STAT_BATCH_BYTES + 300)
        );
        assert_eq!(split.concat(), paths);
        assert_eq!(batches(&[]), Vec::<&[FilePath]>::new());
    }

    #[tokio::test]
    async fn an_archive_written_reads_back_with_long_and_odd_names() {
        let long = "x".repeat(200);
        for name in ODD.iter().copied().chain([long.as_str()]) {
            let data = b"hello\0world".repeat(100);
            let mut archive = file_header(name, data.len() as u64, 0);
            archive.extend_from_slice(&data);
            archive.extend(archive_end(data.len() as u64));
            assert_eq!(archive.len() % BLOCK, 0);
            let (read, contents) = read_archive(&mut archive.as_slice()).await.unwrap();
            assert_eq!(read, name);
            assert_eq!(contents, data);
            let (kind, size) = first_entry(&mut archive.as_slice()).await.unwrap();
            assert_eq!((kind, size), (b'0', data.len() as u64));
        }
    }

    #[test]
    fn header_checksums_as_tar_computes_them() {
        let block = header(b"a.txt", 5, b'0', 0);
        let stored = u32::from_str_radix(
            std::str::from_utf8(block.get(148..154).unwrap()).unwrap(),
            8,
        )
        .unwrap();
        let mut blank = block;
        blank
            .get_mut(148..156)
            .unwrap()
            .copy_from_slice(b"        ");
        assert_eq!(stored, blank.iter().map(|b| u32::from(*b)).sum::<u32>());
        assert_eq!(parse_header(&block).unwrap(), (b'0', 5));
    }

    #[test]
    fn failures_are_told_by_the_words_of_coreutils_and_busybox() {
        for (message, failure) in [
            ("find: '/x/.': No such file or directory", Failure::NotFound),
            (
                "tar: ./a: Cannot open: Permission denied",
                Failure::PermissionDenied,
            ),
            (
                "mkdir: can't create directory '/c/x': Read-only file system",
                Failure::ReadOnly,
            ),
            (
                "mkdir: cannot create directory '/d/x': File exists",
                Failure::Exists,
            ),
            ("weird", Failure::Other),
        ] {
            assert_eq!(Failure::of(message), failure, "{message}");
        }
    }
}
