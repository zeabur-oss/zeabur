//! A container's files and volumes: browse, download, preview, upload, make a folder, rename and
//! delete, through `pods/exec` as the person (see `zeabur_kube::files`).
//!
//! The commands run in the container itself. An image without `tar`, `find` or `stat` (a
//! distroless image) is read through Zeabur's debug container for it instead (the
//! `debugContainer` change), which mounts the container's volumes at the same paths; the image's
//! own files are not reachable that way, and a request for them says so. `via` names that debug
//! container.
//!
//! # What is never shown or changed
//!
//! - The files of a Secret volume, a projected volume with a Secret or a service account token, or
//!   a Secrets Store CSI volume: a listing names them without sizes, and neither a download nor a
//!   preview reads them, also through a link. Their values are read in the Secret's editor after
//!   its reveal, which this does not bypass.
//! - `/proc`, `/sys` and `/dev`: the kernel's, not files.
//! - A ConfigMap, Secret, projected or downward API volume, or one mounted read-only, takes no
//!   writes; the ConfigMap's or Secret's editor changes them.
//! - A mount point, `/` and a folder holding a mount point cannot be renamed or deleted: `rm -r`
//!   would follow into the volume.
//!
//! A folder is not downloaded whole: an archive of it could reach into a Secret volume mounted
//! below it.
//!
//! The limits are [`MAX_ENTRIES`], [`MAX_TRANSFER`] and [`MAX_PREVIEW`]; the answer to
//! `GET …/files` carries them so the interface says the same.

#![expect(
    unused_qualifications,
    reason = "utoipa's `path` macro expands `params(…)` to a qualified path"
)]

use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, RawQuery};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use zeabur_core::message::Message;
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::Cluster;
use zeabur_kube::files::{self, EntryKind, Failure, FileError, FilePath, Target};
use zeabur_plan::terminal::{self, ContainerState};

use crate::error::{ApiError, read_problem};
use crate::handlers::Person;
use crate::pods::exec::pick_container;
use crate::projects::writes::terminal::read_pod;

/// The most entries a listing returns; a larger folder says how many it has.
pub const MAX_ENTRIES: usize = 1000;
/// The largest file downloaded or uploaded, in bytes.
pub const MAX_TRANSFER: u64 = 100_000_000;
/// The largest file shown as text, in bytes.
pub const MAX_PREVIEW: u64 = 1_000_000;

/// Paths that are the kernel's.
const KERNEL: [&str; 3] = ["/proc", "/sys", "/dev"];

/// What a mount is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FileMountKind {
    /// A `PersistentVolumeClaim`.
    Claim,
    /// A ConfigMap.
    ConfigMap,
    /// A Secret, or anything else that carries credentials.
    Secret,
    /// Scratch space: an `emptyDir` or a generic ephemeral volume.
    EmptyDir,
    /// The image's own files, at `/`.
    Image,
    /// Another kind of volume (`hostPath`, NFS, downward API, …).
    Other,
}

/// When a mount's files go away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FileLost {
    /// When the replica is replaced.
    Replaced,
    /// When the container restarts.
    Restart,
}

/// A place in the container's file system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileMount {
    /// Where it is mounted.
    pub path: String,
    /// What it is.
    pub kind: FileMountKind,
    /// The volume's name in the pod; absent for the image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    /// The claim, ConfigMap or Secret behind it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// Writes are refused.
    pub read_only: bool,
    /// Only part of the volume is mounted (`subPath`).
    pub part: bool,
    /// When its files go away; absent when they stay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lost: Option<FileLost>,
}

/// How the files are reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FileReaderKind {
    /// Through the container itself.
    Container,
    /// Through Zeabur's debug container (`via`), which mounts the same volumes; the image's own
    /// files are out of reach.
    DebugContainer,
    /// Not at all: the image has no `tar`, and no debug container mounts its volumes yet.
    None,
}

/// How the files are reached, and what is missing when they are not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileReader {
    /// How.
    pub kind: FileReaderKind,
    /// The debug container to pass as `via`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// For `none`: whether the person may add a debug container (`patch` on
    /// `pods/ephemeralcontainers`); absent when the cluster would not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_add_debug_container: Option<bool>,
}

/// The limits of this server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileLimits {
    /// The most entries a listing returns.
    pub entries: usize,
    /// The largest download or upload, in bytes.
    pub transfer: u64,
    /// The largest text preview, in bytes.
    pub preview: u64,
}

/// A container's mounts and how its files are reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Files {
    /// The pod.
    pub pod: String,
    /// The container.
    pub container: String,
    /// Its mounts, by path, the image's last.
    pub mounts: Vec<FileMount>,
    /// How the files are reached.
    pub reader: FileReader,
    /// The limits.
    pub limits: FileLimits,
}

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum FileEntryType {
    /// A file.
    File,
    /// A folder.
    Directory,
    /// A link whose target does not exist.
    BrokenLink,
    /// A device, socket or pipe.
    Other,
}

/// One entry of a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// Its name.
    pub name: String,
    /// What it is, or what its link points to.
    #[serde(rename = "type")]
    pub kind: FileEntryType,
    /// It is a symbolic link.
    pub link: bool,
    /// Its size in bytes; absent for a folder, and in a Secret's volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// When it was last modified, in seconds since 1970.
    pub modified: i64,
}

/// A folder's entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileList {
    /// The folder.
    pub path: String,
    /// Its first entries by name, folders first, at most `limits.entries`.
    pub items: Vec<FileEntry>,
    /// How many entries it has (that match `filter`).
    pub total_size: usize,
    /// It has more names than Zeabur reads; `totalSize` counts those read.
    pub more: bool,
}

/// A file shown as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    /// The file, links resolved.
    pub path: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its text; absent when it is not text or larger than `limits.preview`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Larger than `limits.preview`.
    pub too_large: bool,
}

/// The parameters that name a container's files.
#[expect(
    dead_code,
    reason = "describes the parameters in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, Default, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct FileParams {
    /// The container; by default the pod's default container.
    container: Option<String>,
    /// Zeabur's debug container to read through (`reader.via`).
    via: Option<String>,
    /// The absolute path.
    path: Option<String>,
    /// Only names that contain this, ignoring case (a listing).
    filter: Option<String>,
    /// Replace a file of the same name (an upload).
    replace: Option<bool>,
}

/// A folder to make.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileNewFolder {
    /// The container.
    #[serde(default)]
    pub container: Option<String>,
    /// Zeabur's debug container to write through.
    #[serde(default)]
    pub via: Option<String>,
    /// The folder's absolute path.
    pub path: String,
}

/// A rename within a folder.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileRename {
    /// The container.
    #[serde(default)]
    pub container: Option<String>,
    /// Zeabur's debug container to write through.
    #[serde(default)]
    pub via: Option<String>,
    /// The entry's absolute path.
    pub path: String,
    /// Its new name, in the same folder.
    pub name: String,
}

#[derive(Debug, Default)]
struct Query {
    container: Option<String>,
    via: Option<String>,
    path: Option<String>,
    filter: Option<String>,
    replace: bool,
}

impl Query {
    fn parse(query: Option<&str>) -> Self {
        let mut parsed = Self::default();
        for (name, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
            let value = (!value.is_empty()).then(|| value.into_owned());
            match name.as_ref() {
                "container" => parsed.container = value,
                "via" => parsed.via = value,
                "path" => parsed.path = value,
                "filter" => parsed.filter = value,
                "replace" => parsed.replace = value.as_deref() == Some("true"),
                _ => {}
            }
        }
        parsed
    }
}

fn refuse(kind: Kind, detail: &str, key: &str, params: &[(&str, &str)]) -> ApiError {
    let mut problem = Problem::new(kind, detail);
    let mut summary = Message::new(&format!("files.{key}"));
    let mut fix = Message::new(&format!("files.{key}.fix"));
    for (name, value) in params {
        summary = summary.with(name, *value);
        fix = fix.with(name, *value);
    }
    problem.summary = summary;
    problem.fix_for_occasion(String::new(), fix);
    ApiError::from(problem)
}

fn object_name(volume: &Value, kind: &str, field: &str) -> Option<String> {
    volume.get(kind)?.get(field)?.as_str().map(str::to_owned)
}

/// What a volume of the pod's `spec.volumes` is, and the object behind it.
fn classify(volume: Option<&Value>) -> (FileMountKind, Option<String>, bool) {
    let Some(volume) = volume else {
        return (FileMountKind::Other, None, false);
    };
    if volume.get("persistentVolumeClaim").is_some() {
        let read_only = volume
            .pointer("/persistentVolumeClaim/readOnly")
            .and_then(Value::as_bool)
            == Some(true);
        (
            FileMountKind::Claim,
            object_name(volume, "persistentVolumeClaim", "claimName"),
            read_only,
        )
    } else if volume.get("secret").is_some() {
        (
            FileMountKind::Secret,
            object_name(volume, "secret", "secretName"),
            true,
        )
    } else if let Some(projected) = volume.get("projected") {
        let sources = projected
            .get("sources")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        let secret = sources.iter().find_map(|s| s.pointer("/secret/name"));
        let credentials = secret.is_some()
            || sources.iter().any(|s| {
                s.get("serviceAccountToken").is_some() || s.get("clusterTrustBundle").is_some()
            });
        if credentials {
            (
                FileMountKind::Secret,
                secret.and_then(Value::as_str).map(str::to_owned),
                true,
            )
        } else {
            let config = sources
                .iter()
                .find_map(|s| s.pointer("/configMap/name"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            (FileMountKind::ConfigMap, config, true)
        }
    } else if volume.get("configMap").is_some() {
        (
            FileMountKind::ConfigMap,
            object_name(volume, "configMap", "name"),
            true,
        )
    } else if volume
        .pointer("/csi/driver")
        .and_then(Value::as_str)
        .is_some_and(|d| d == "secrets-store.csi.k8s.io")
    {
        (FileMountKind::Secret, None, true)
    } else if volume.get("emptyDir").is_some() || volume.get("ephemeral").is_some() {
        (FileMountKind::EmptyDir, None, false)
    } else {
        (
            FileMountKind::Other,
            None,
            volume.get("downwardAPI").is_some(),
        )
    }
}

/// The mounts of `container` in `pod`, by path, then the image at `/`.
fn mounts_of(pod: &Value, container: &str) -> Vec<FileMount> {
    let volumes: Vec<&Value> = pod
        .pointer("/spec/volumes")
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |v| v.iter().collect());
    let spec = pod
        .pointer("/spec/containers")
        .and_then(Value::as_array)
        .and_then(|c| {
            c.iter()
                .find(|c| c.get("name").and_then(Value::as_str) == Some(container))
        });
    let mut mounts: Vec<FileMount> = spec
        .and_then(|c| c.get("volumeMounts"))
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|m| {
            let path = FilePath::parse(m.get("mountPath")?.as_str()?)?;
            let name = m.get("name")?.as_str()?;
            let volume = volumes
                .iter()
                .find(|v| v.get("name").and_then(Value::as_str) == Some(name))
                .copied();
            let (kind, object, read_only) = classify(volume);
            Some(FileMount {
                path: path.as_str().to_owned(),
                kind,
                volume: Some(name.to_owned()),
                object,
                read_only: read_only || m.get("readOnly").and_then(Value::as_bool) == Some(true),
                part: m.get("subPath").is_some() || m.get("subPathExpr").is_some(),
                lost: (kind == FileMountKind::EmptyDir).then_some(FileLost::Replaced),
            })
        })
        .filter(|m| !m.path.is_empty() && m.path != "/")
        .collect();
    mounts.sort_by(|a, b| a.path.cmp(&b.path));
    mounts.push(FileMount {
        path: "/".to_owned(),
        kind: FileMountKind::Image,
        volume: None,
        object: None,
        read_only: spec
            .and_then(|c| c.pointer("/securityContext/readOnlyRootFilesystem"))
            .and_then(Value::as_bool)
            == Some(true),
        part: false,
        lost: Some(FileLost::Restart),
    });
    mounts
}

/// The mount `path` is in: the deepest that holds it.
fn mount_of<'a>(mounts: &'a [FileMount], path: &FilePath) -> Option<&'a FileMount> {
    mounts
        .iter()
        .filter(|m| FilePath::parse(&m.path).is_some_and(|p| path.is_within(&p)))
        .max_by_key(|m| m.path.len())
}

fn is_kernel(path: &FilePath) -> bool {
    KERNEL
        .iter()
        .filter_map(|k| FilePath::parse(k))
        .any(|k| path.is_within(&k))
}

fn is_secret(mounts: &[FileMount], path: &FilePath) -> bool {
    match mount_of(mounts, path).map(|m| m.kind) {
        Some(FileMountKind::Secret) => true,
        Some(
            FileMountKind::Claim
            | FileMountKind::ConfigMap
            | FileMountKind::EmptyDir
            | FileMountKind::Image
            | FileMountKind::Other,
        )
        | None => false,
    }
}

/// A pod's container and how its files are reached, for one request.
struct Place {
    cluster: Arc<dyn Cluster>,
    project: String,
    pod: String,
    /// The container commands run in: the one asked for, or `via`.
    runs_in: String,
    /// The paths `via` mounts; `None` when commands run in the container itself.
    reachable: Option<Vec<String>>,
    mounts: Vec<FileMount>,
}

impl Place {
    async fn find(
        cluster: Arc<dyn Cluster>,
        project: &str,
        pod: &str,
        container: Option<String>,
        via: Option<String>,
    ) -> Result<Self, ApiError> {
        let object = read_pod(cluster.as_ref(), project, pod).await?;
        let container = terminal::target_container(&object, container.as_deref())
            .map_err(crate::projects::writes::plan_problem)?;
        let mounts = mounts_of(&object, &container);
        let (runs_in, reachable) = match via {
            None => (pick_container(&object, pod, Some(container))?, None),
            Some(via) => {
                let debug = terminal::debug_containers(&object)
                    .into_iter()
                    .find(|c| c.name == via && c.target.as_deref() == Some(container.as_str()))
                    .ok_or_else(|| {
                        Problem::new(
                            Kind::NotFound,
                            format!("Pod {pod} has no debug container {via} for {container}."),
                        )
                    })?;
                pick_container(&object, pod, Some(debug.name.clone()))?;
                (debug.name, Some(debug.mounts))
            }
        };
        Ok(Self {
            cluster,
            project: project.to_owned(),
            pod: pod.to_owned(),
            runs_in,
            reachable,
            mounts,
        })
    }

    fn target(&self) -> Target<'_> {
        Target {
            cluster: self.cluster.as_ref(),
            namespace: &self.project,
            pod: &self.pod,
            container: &self.runs_in,
        }
    }

    fn path(&self, path: Option<&str>) -> Result<FilePath, ApiError> {
        let path = path.and_then(FilePath::parse).ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::BadRequest,
                "`path` must be an absolute path without `.` or `..` parts.",
            ))
        })?;
        if is_kernel(&path) {
            return Err(refuse(
                Kind::Conflict,
                "/proc, /sys and /dev are the kernel's, not files; Zeabur does not show them.",
                "kernel",
                &[],
            ));
        }
        if let Some(reachable) = &self.reachable
            && !reachable
                .iter()
                .filter_map(|p| FilePath::parse(p))
                .any(|p| path.is_within(&p))
        {
            return Err(refuse(
                Kind::Conflict,
                &format!(
                    "{path} is in the image's own files, which the debug container does not \
                     reach; only the volumes are."
                ),
                "imageUnreachable",
                &[("path", path.as_str())],
            ));
        }
        Ok(path)
    }

    /// Refuses a read of a Secret's files, also through a link (`real`).
    fn readable(&self, path: &FilePath, real: &FilePath) -> Result<(), ApiError> {
        if is_kernel(real) {
            return Err(refuse(
                Kind::Conflict,
                "/proc, /sys and /dev are the kernel's, not files; Zeabur does not show them.",
                "kernel",
                &[],
            ));
        }
        for p in [path, real] {
            if is_secret(&self.mounts, p) {
                let object = mount_of(&self.mounts, p)
                    .and_then(|m| m.object.clone())
                    .unwrap_or_default();
                return Err(refuse(
                    Kind::Conflict,
                    "This file is in a Secret's volume; its value is read in the Secret's editor, \
                     after revealing it.",
                    "secret",
                    &[("secret", &object)],
                ));
            }
        }
        Ok(())
    }

    /// Refuses a write where the mount takes none.
    fn writable(&self, path: &FilePath) -> Result<(), ApiError> {
        let Some(mount) = mount_of(&self.mounts, path) else {
            return Ok(());
        };
        let editor = match mount.kind {
            FileMountKind::ConfigMap => Some("configMap"),
            FileMountKind::Secret => Some("secret"),
            FileMountKind::Claim
            | FileMountKind::EmptyDir
            | FileMountKind::Image
            | FileMountKind::Other => None,
        };
        if let Some(editor) = editor {
            let object = mount.object.clone().unwrap_or_default();
            return Err(refuse(
                Kind::Conflict,
                &format!(
                    "{} is a {editor} volume; its files change in the {editor}'s editor.",
                    mount.path
                ),
                &format!("{editor}ReadOnly"),
                &[("object", &object)],
            ));
        }
        if mount.read_only {
            return Err(refuse(
                Kind::Conflict,
                &format!("{} is mounted read-only.", mount.path),
                "readOnly",
                &[("path", &mount.path)],
            ));
        }
        Ok(())
    }

    /// Refuses renaming or deleting `/`, a mount point, or a folder that holds one.
    fn movable(&self, path: &FilePath) -> Result<(), ApiError> {
        let holds = self
            .mounts
            .iter()
            .filter_map(|m| FilePath::parse(&m.path))
            .any(|m| m.is_within(path));
        if path.is_root() || holds {
            return Err(refuse(
                Kind::Conflict,
                &format!("{path} is or holds a mount point; it stays where the pod mounts it."),
                "mountPoint",
                &[("path", path.as_str())],
            ));
        }
        self.writable(path)
    }

    fn problem(&self, error: FileError, path: &FilePath) -> ApiError {
        file_problem(error, &self.pod, &self.project, &self.runs_in, path)
    }
}

/// The problem for a file operation that failed.
fn file_problem(
    error: FileError,
    pod: &str,
    project: &str,
    container: &str,
    path: &FilePath,
) -> ApiError {
    let at = path.as_str();
    match error {
        FileError::Cluster(e) => {
            let mut refusal = read_problem(
                e,
                &format!("files in pod {pod}"),
                // As a terminal: the upgrade is a WebSocket handshake, a GET.
                Denied {
                    verb: "get".to_owned(),
                    group: String::new(),
                    resource: "pods/exec".to_owned(),
                    name: Some(pod.to_owned()),
                    namespace: Some(project.to_owned()),
                },
            );
            if refusal.0.kind == Kind::Forbidden {
                refusal.0.fix_for_occasion(
                    "Ask a cluster administrator for `get` and `create` on `pods/exec` in this \
                     project; it grants only that, in this project only.",
                    Message::new("problem.forbidden.fix.sendGrant"),
                );
            }
            refusal
        }
        FileError::CommandNotFound(command) => refuse(
            Kind::Conflict,
            &format!(
                "Container {container} has no {command}, which reading its files takes; a debug \
                 container can read its volumes."
            ),
            "noTools",
            &[("container", container), ("command", &command)],
        ),
        FileError::Failed { failure, message } => {
            let (kind, key) = match failure {
                Failure::NotFound => (Kind::NotFound, "notFound"),
                Failure::PermissionDenied => (Kind::Conflict, "permissionDenied"),
                Failure::ReadOnly => (Kind::Conflict, "readOnlyFileSystem"),
                Failure::Exists => (Kind::Conflict, "exists"),
                Failure::NotADirectory => (Kind::Conflict, "notADirectory"),
                Failure::IsADirectory => (Kind::Conflict, "isADirectory"),
                Failure::NoSpace => (Kind::Conflict, "noSpace"),
                Failure::Other => (Kind::Conflict, "failed"),
            };
            refuse(
                kind,
                &format!("{at}: {message}"),
                key,
                &[("path", at), ("message", &message)],
            )
        }
        FileError::NotAFile(_) => refuse(
            Kind::Conflict,
            &format!("{at} is not a file."),
            "notAFile",
            &[("path", at)],
        ),
        FileError::TooLarge { size, limit } => refuse(
            Kind::Conflict,
            &format!("{at} is {size} bytes, more than the {limit} Zeabur moves at once."),
            "tooLarge",
            &[
                ("path", at),
                ("size", &size.to_string()),
                ("limit", &limit.to_string()),
            ],
        ),
        FileError::Protocol(message) => ApiError::from(Problem::new(
            Kind::Internal,
            format!("The container answered something unexpected: {message}"),
        )),
    }
}

/// How the person's files are reached: the container, else a running debug container of
/// Zeabur's that mounts its volumes.
async fn reader(
    cluster: &dyn Cluster,
    pod: &Value,
    project: &str,
    name: &str,
    container: &str,
) -> Result<FileReader, ApiError> {
    let target = Target {
        cluster,
        namespace: project,
        pod: name,
        container,
    };
    match files::probe(target).await {
        Ok(()) => {
            return Ok(FileReader {
                kind: FileReaderKind::Container,
                via: None,
                can_add_debug_container: None,
            });
        }
        Err(FileError::CommandNotFound(_)) => {}
        Err(error) => {
            return Err(file_problem(
                error,
                name,
                project,
                container,
                &FilePath::root(),
            ));
        }
    }
    let shared: Vec<String> = mounts_of(pod, container)
        .into_iter()
        .filter(|m| m.kind != FileMountKind::Image && !m.part)
        .map(|m| m.path)
        .collect();
    let via = terminal::debug_containers(pod)
        .into_iter()
        .rev()
        .find(|c| {
            c.target.as_deref() == Some(container)
                && matches!(c.state, ContainerState::Running)
                && shared.iter().all(|p| c.mounts.contains(p))
        })
        .map(|c| c.name);
    if via.is_some() {
        return Ok(FileReader {
            kind: FileReaderKind::DebugContainer,
            via,
            can_add_debug_container: None,
        });
    }
    let right = Denied {
        verb: "patch".to_owned(),
        group: String::new(),
        resource: "pods/ephemeralcontainers".to_owned(),
        name: Some(name.to_owned()),
        namespace: Some(project.to_owned()),
    };
    Ok(FileReader {
        kind: FileReaderKind::None,
        via: None,
        can_add_debug_container: cluster.review_access(&right).await.ok(),
    })
}

/// A container's mounts and how its files are reached.
///
/// Probes the container for `tar`. Without it, `reader` names a running debug container of
/// Zeabur's that mounts the container's volumes, or says none does yet (`none`), and whether the
/// person may add one with the `debugContainer` change.
#[utoipa::path(get, path = "/api/v1/projects/{project}/pods/{pod}/files", tag = "pods",
    operation_id = "getFiles",
    params(("project" = String, Path), ("pod" = String, Path), FileParams),
    responses((status = 200, body = Files),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path((project, pod)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Json<Files>, ApiError> {
    let query = Query::parse(query.as_deref());
    let object = read_pod(cluster.as_ref(), &project, &pod).await?;
    let container = terminal::target_container(&object, query.container.as_deref())
        .map_err(crate::projects::writes::plan_problem)?;
    pick_container(&object, &pod, Some(container.clone()))?;
    let reader = reader(cluster.as_ref(), &object, &project, &pod, &container).await?;
    Ok(Json(Files {
        mounts: mounts_of(&object, &container),
        pod,
        container,
        reader,
        limits: FileLimits {
            entries: MAX_ENTRIES,
            transfer: MAX_TRANSFER,
            preview: MAX_PREVIEW,
        },
    }))
}

const KUBELET_ENTRY: &str = "..";

/// A folder's entries: the first [`MAX_ENTRIES`] by name, folders first.
///
/// In a Secret's volume the sizes are left out. The kubelet's own entries in a ConfigMap, Secret
/// or projected volume (`..data` and the timestamped folder it points to) are left out too: the
/// keys are what the volume holds.
#[utoipa::path(get, path = "/api/v1/projects/{project}/pods/{pod}/files/entries", tag = "pods",
    operation_id = "listFiles",
    params(("project" = String, Path), ("pod" = String, Path), FileParams),
    responses((status = 200, body = FileList),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn list(
    Person(cluster): Person,
    Path((project, pod)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Json<FileList>, ApiError> {
    let query = Query::parse(query.as_deref());
    let place = Place::find(cluster, &project, &pod, query.container, query.via).await?;
    let folder = place.path(query.path.as_deref())?;
    let listing = files::list(
        place.target(),
        &folder,
        query.filter.as_deref(),
        MAX_ENTRIES,
    )
    .await
    .map_err(|e| place.problem(e, &folder))?;
    let kubelets = mount_of(&place.mounts, &folder).is_some_and(|m| {
        matches!(m.kind, FileMountKind::ConfigMap | FileMountKind::Secret)
            && m.path == folder.as_str()
    });
    let hidden = |name: &str| kubelets && name.starts_with(KUBELET_ENTRY);
    let secret = is_secret(&place.mounts, &folder);
    let dropped = listing.entries.iter().filter(|e| hidden(&e.name)).count();
    let items = listing
        .entries
        .into_iter()
        .filter(|e| !hidden(&e.name))
        .map(|e| {
            let kind = match (e.kind, e.target) {
                (EntryKind::Link, None) => FileEntryType::BrokenLink,
                (EntryKind::Directory, _) | (EntryKind::Link, Some(EntryKind::Directory)) => {
                    FileEntryType::Directory
                }
                (EntryKind::File, _) | (EntryKind::Link, Some(EntryKind::File)) => {
                    FileEntryType::File
                }
                (EntryKind::Other, _)
                | (EntryKind::Link, Some(EntryKind::Other | EntryKind::Link)) => {
                    FileEntryType::Other
                }
            };
            FileEntry {
                size: (kind == FileEntryType::File && !secret).then_some(e.size),
                link: e.kind == EntryKind::Link,
                name: e.name,
                kind,
                modified: e.modified,
            }
        })
        .collect();
    Ok(Json(FileList {
        path: folder.as_str().to_owned(),
        items,
        total_size: listing.total.saturating_sub(dropped),
        more: listing.more,
    }))
}

/// Resolves `path` and checks it may be read: a file, outside a Secret's volume, also through
/// links.
async fn readable_file(place: &Place, path: &FilePath) -> Result<FilePath, ApiError> {
    place.readable(path, path)?;
    let real = files::resolve(place.target(), path)
        .await
        .map_err(|e| place.problem(e, path))?;
    place.readable(path, &real)?;
    Ok(real)
}

/// RFC 6266 `Content-Disposition` for `name`: an ASCII fallback and the UTF-8 name (RFC 8187).
fn disposition(name: &str) -> HeaderValue {
    let fallback: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let encoded: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    HeaderValue::from_str(&format!(
        "attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

/// A file's contents, streamed as the container reads it, up to [`MAX_TRANSFER`] bytes.
///
/// A file in a Secret's volume, also through a link, is refused; a folder is not downloaded.
#[utoipa::path(get, path = "/api/v1/projects/{project}/pods/{pod}/files/content", tag = "pods",
    operation_id = "downloadFile",
    params(("project" = String, Path), ("pod" = String, Path), FileParams),
    responses((status = 200, description = "The file's bytes", content_type = "application/octet-stream"),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn download(
    Person(cluster): Person,
    Path((project, pod)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Response, ApiError> {
    let query = Query::parse(query.as_deref());
    let place = Place::find(cluster, &project, &pod, query.container, query.via).await?;
    let path = place.path(query.path.as_deref())?;
    let real = readable_file(&place, &path).await?;
    let file = files::download(place.target(), &real, MAX_TRANSFER)
        .await
        .map_err(|e| place.problem(e, &path))?;
    let mut response = Body::from_stream(file.body).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(file.size));
    headers.insert(
        header::CONTENT_DISPOSITION,
        disposition(path.name().unwrap_or("file")),
    );
    Ok(response)
}

fn is_text(bytes: &[u8]) -> Option<String> {
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes.to_vec()).ok()
}

/// A file as text, up to [`MAX_PREVIEW`] bytes; `text` is absent when it is not UTF-8 text.
#[utoipa::path(get, path = "/api/v1/projects/{project}/pods/{pod}/files/preview", tag = "pods",
    operation_id = "previewFile",
    params(("project" = String, Path), ("pod" = String, Path), FileParams),
    responses((status = 200, body = FilePreview),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn preview(
    Person(cluster): Person,
    Path((project, pod)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Json<FilePreview>, ApiError> {
    let query = Query::parse(query.as_deref());
    let place = Place::find(cluster, &project, &pod, query.container, query.via).await?;
    let path = place.path(query.path.as_deref())?;
    let real = readable_file(&place, &path).await?;
    match files::read(place.target(), &real, MAX_PREVIEW).await {
        Ok(bytes) => Ok(Json(FilePreview {
            path: real.as_str().to_owned(),
            size: bytes.len() as u64,
            text: is_text(&bytes),
            too_large: false,
        })),
        Err(FileError::TooLarge { size, .. }) => Ok(Json(FilePreview {
            path: real.as_str().to_owned(),
            size,
            text: None,
            too_large: true,
        })),
        Err(e) => Err(place.problem(e, &path)),
    }
}

/// Writes the request's body as the file `path`, up to [`MAX_TRANSFER`] bytes.
///
/// The body is the file's bytes and `Content-Length` its size. A file of that name is replaced
/// only with `replace=true`; otherwise it is a `409` (`files.exists`).
#[utoipa::path(put, path = "/api/v1/projects/{project}/pods/{pod}/files/content", tag = "pods",
    operation_id = "uploadFile",
    params(("project" = String, Path), ("pod" = String, Path), FileParams),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 204, description = "Written"),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn upload(
    Person(cluster): Person,
    Path((project, pod)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Body,
) -> Result<StatusCode, ApiError> {
    let query = Query::parse(query.as_deref());
    let size = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::BadRequest,
                "An upload needs `Content-Length`.",
            ))
        })?;
    if size > MAX_TRANSFER {
        return Err(refuse(
            Kind::BadRequest,
            &format!(
                "The file is {size} bytes, more than the {MAX_TRANSFER} Zeabur moves at once."
            ),
            "tooLarge",
            &[
                ("size", &size.to_string()),
                ("limit", &MAX_TRANSFER.to_string()),
            ],
        ));
    }
    let place = Place::find(cluster, &project, &pod, query.container, query.via).await?;
    let path = place.path(query.path.as_deref())?;
    let (Some(folder), Some(name)) = (path.parent(), path.name()) else {
        return Err(Problem::new(Kind::BadRequest, "`path` must name a file.").into());
    };
    place.writable(&path)?;
    place.movable(&path)?;
    match files::stat(place.target(), &path)
        .await
        .map_err(|e| place.problem(e, &path))?
    {
        Some(stat) if stat.kind == EntryKind::File && query.replace => {}
        Some(stat) if stat.kind == EntryKind::File => {
            return Err(place.problem(
                FileError::Failed {
                    failure: Failure::Exists,
                    message: "File exists".to_owned(),
                },
                &path,
            ));
        }
        Some(stat) => return Err(place.problem(FileError::NotAFile(stat.kind), &path)),
        None => {}
    }
    let stream = body
        .into_data_stream()
        .map(|chunk| chunk.map(|b| b.to_vec()));
    files::upload(place.target(), &folder, name, size, Box::pin(stream))
        .await
        .map_err(|e| place.problem(e, &path))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Makes a folder.
#[utoipa::path(post, path = "/api/v1/projects/{project}/pods/{pod}/files/folders", tag = "pods",
    operation_id = "makeFolder",
    params(("project" = String, Path), ("pod" = String, Path)),
    request_body = FileNewFolder,
    responses((status = 204, description = "Made"),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn make_folder(
    Person(cluster): Person,
    Path((project, pod)): Path<(String, String)>,
    Json(request): Json<FileNewFolder>,
) -> Result<StatusCode, ApiError> {
    let place = Place::find(cluster, &project, &pod, request.container, request.via).await?;
    let path = place.path(Some(&request.path))?;
    if path.name().is_none_or(|n| !files::valid_name(n)) {
        return Err(Problem::new(Kind::BadRequest, "`path` must name a folder.").into());
    }
    place.writable(&path)?;
    files::make_dir(place.target(), &path)
        .await
        .map_err(|e| place.problem(e, &path))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Renames a file or folder within its folder; refused when the new name is taken.
#[utoipa::path(post, path = "/api/v1/projects/{project}/pods/{pod}/files/moves", tag = "pods",
    operation_id = "renameFile",
    params(("project" = String, Path), ("pod" = String, Path)),
    request_body = FileRename,
    responses((status = 204, description = "Renamed"),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn rename(
    Person(cluster): Person,
    Path((project, pod)): Path<(String, String)>,
    Json(request): Json<FileRename>,
) -> Result<StatusCode, ApiError> {
    let place = Place::find(cluster, &project, &pod, request.container, request.via).await?;
    let from = place.path(Some(&request.path))?;
    let to = from
        .parent()
        .and_then(|folder| folder.join(&request.name))
        .filter(|_| files::valid_name(&request.name))
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::BadRequest,
                "`name` must be a name: not empty, `.` or `..`, without `/`.",
            ))
        })?;
    place.movable(&from)?;
    place.writable(&to)?;
    files::rename(place.target(), &from, &to)
        .await
        .map_err(|e| place.problem(e, &to))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Deletes a file, or a folder with everything in it.
#[utoipa::path(delete, path = "/api/v1/projects/{project}/pods/{pod}/files/entries", tag = "pods",
    operation_id = "deleteFile",
    params(("project" = String, Path), ("pod" = String, Path), FileParams),
    responses((status = 204, description = "Deleted"),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn delete(
    Person(cluster): Person,
    Path((project, pod)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<StatusCode, ApiError> {
    let query = Query::parse(query.as_deref());
    let place = Place::find(cluster, &project, &pod, query.container, query.via).await?;
    let path = place.path(query.path.as_deref())?;
    place.movable(&path)?;
    if files::stat(place.target(), &path)
        .await
        .map_err(|e| place.problem(e, &path))?
        .is_none()
    {
        return Err(place.problem(
            FileError::Failed {
                failure: Failure::NotFound,
                message: "No such file or directory".to_owned(),
            },
            &path,
        ));
    }
    files::remove(place.target(), &path)
        .await
        .map_err(|e| place.problem(e, &path))?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn pod() -> Value {
        json!({
            "metadata": {"name": "web-1"},
            "spec": {
                "containers": [{"name": "web", "volumeMounts": [
                    {"name": "data", "mountPath": "/data"},
                    {"name": "config", "mountPath": "/config"},
                    {"name": "db", "mountPath": "/run/secrets/db"},
                    {"name": "token", "mountPath": "/var/run/secrets/kubernetes.io/serviceaccount", "readOnly": true},
                    {"name": "scratch", "mountPath": "/tmp"},
                    {"name": "data", "mountPath": "/logs", "subPath": "logs", "readOnly": true},
                    {"name": "store", "mountPath": "/mnt/secrets-store"}
                ]}],
                "volumes": [
                    {"name": "data", "persistentVolumeClaim": {"claimName": "uploads"}},
                    {"name": "config", "configMap": {"name": "web-config"}},
                    {"name": "db", "secret": {"secretName": "db-credentials"}},
                    {"name": "token", "projected": {"sources": [{"serviceAccountToken": {"path": "token"}}]}},
                    {"name": "scratch", "emptyDir": {}},
                    {"name": "store", "csi": {"driver": "secrets-store.csi.k8s.io"}}
                ]
            }
        })
    }

    fn path(p: &str) -> FilePath {
        FilePath::parse(p).unwrap()
    }

    #[test]
    fn mounts_say_what_holds_credentials_and_what_takes_writes() {
        let mounts = mounts_of(&pod(), "web");
        let kind = |p: &str| {
            let m = mounts.iter().find(|m| m.path == p).unwrap();
            (m.kind, m.read_only, m.object.clone())
        };
        assert_eq!(
            kind("/data"),
            (FileMountKind::Claim, false, Some("uploads".to_owned()))
        );
        assert_eq!(
            kind("/config"),
            (
                FileMountKind::ConfigMap,
                true,
                Some("web-config".to_owned())
            )
        );
        assert_eq!(
            kind("/run/secrets/db"),
            (
                FileMountKind::Secret,
                true,
                Some("db-credentials".to_owned())
            )
        );
        assert_eq!(
            kind("/var/run/secrets/kubernetes.io/serviceaccount").0,
            FileMountKind::Secret
        );
        assert_eq!(kind("/mnt/secrets-store").0, FileMountKind::Secret);
        assert_eq!(kind("/tmp"), (FileMountKind::EmptyDir, false, None));
        assert_eq!(
            kind("/logs"),
            (FileMountKind::Claim, true, Some("uploads".to_owned()))
        );
        assert_eq!(mounts.last().map(|m| m.kind), Some(FileMountKind::Image));
        assert!(is_secret(&mounts, &path("/run/secrets/db/..data/password")));
        assert!(is_secret(&mounts, &path("/run/secrets/db")));
        assert!(!is_secret(&mounts, &path("/run/secrets")));
        assert!(!is_secret(&mounts, &path("/data/secret")));
        assert!(is_kernel(&path("/proc/1/environ")));
        assert!(!is_kernel(&path("/process")));
    }

    #[test]
    fn a_download_is_named_in_ascii_and_in_utf8() {
        assert_eq!(
            disposition("報告 \"final\".txt"),
            "attachment; filename=\"__ _final_.txt\"; filename*=UTF-8''%E5%A0%B1%E5%91%8A%20%22final%22.txt"
        );
        assert_eq!(
            disposition("a\nb"),
            "attachment; filename=\"a_b\"; filename*=UTF-8''a%0Ab"
        );
    }
}
