//! The whole cluster's backups as Velero keeps them, and the objects Zeabur writes to set them up.
//!
//! Where they go is a `BackupStorageLocation` [velero-bsl], when they run a `Schedule`
//! [velero-schedule], and how each went a `Backup` [velero-backup].
//!
//! # What Zeabur writes
//!
//! - The location `default` in Velero's namespace, through the AWS plugin, which speaks to any
//!   S3-compatible store (`s3Url`, path-style addressing) [velero-aws]. Its keys go in the Secret
//!   [`CREDENTIALS`] as an AWS credentials file under [`CREDENTIALS_KEY`], named by the location's
//!   `spec.credential` [velero-bsl]; a location that names no Secret keeps Velero's own.
//! - The schedule [`SCHEDULE`]: its cron, the namespaces it covers (`*` for all), its `ttl`, and
//!   file-system backup of every volume (`defaultVolumesToFsBackup`) [velero-fsb], since local-path
//!   volumes have no snapshots. That setting follows the node agent ([`NODE_AGENT`]): a cluster
//!   without it gets `false`, because Velero fails a backup that asks for file-system backup
//!   without the agent.
//! - A backup now: a `Backup` with the schedule's template and its `velero.io/schedule-name` label,
//!   named `<schedule>-<UTC time>`, as `velero backup create --from-schedule` makes one
//!   [velero-cli].
//! - A restore: a `Restore` of one namespace of a backup, named `<backup>-<UTC time>` as `velero
//!   restore create --from-backup` names one [velero-restore]. Velero skips an object that is
//!   already there [velero-restore], so Zeabur restores into a new project and leaves the original
//!   as it is:
//!   - while the namespace exists, into a new namespace through `namespaceMapping`
//!     [velero-restore]. Zeabur creates that namespace first as a project, so Velero keeps its
//!     labels. A resource modifier ([`paused_cron_jobs`]) sets `spec.suspend` on every CronJob, so
//!     the copy runs no scheduled job twice [velero-modifiers];
//!   - when the namespace no longer exists, under its own name.
//!
//!   Both leave out Ingresses and HTTPRoutes ([`HELD_BACK`]), so no restore claims a hostname
//!   before the person checks the project. The domains come back in a last step: a second
//!   `Restore` of only the routes ([`domains_restore`]), or a move from the project that serves
//!   them ([`moved_route`]).
//! - A download: a `DownloadRequest` for a backup's contents or its list of objects
//!   [velero-download]. Velero answers with a signed URL to the location.
//!
//! # Sizes
//!
//! A `Backup` does not say how many bytes it holds. The bytes its file-system backups copied are
//! the sum of its `PodVolumeBackup`s' `status.progress.bytesDone`, by their
//! `velero.io/backup-name` label [velero-fsb]; objects add little to that.
//!
//! [velero-bsl]: https://velero.io/docs/v1.17/api-types/backupstoragelocation/
//! [velero-schedule]: https://velero.io/docs/v1.17/api-types/schedule/
//! [velero-backup]: https://velero.io/docs/v1.17/api-types/backup/
//! [velero-fsb]: https://velero.io/docs/v1.17/file-system-backup/
//! [velero-aws]: https://github.com/vmware-tanzu/velero-plugin-for-aws/blob/v1.13.0/backupstoragelocation.md
//! [velero-cli]: https://velero.io/docs/v1.17/backup-reference/
//! [velero-restore]: https://velero.io/docs/v1.17/restore-reference/
//! [velero-download]: https://velero.io/docs/v1.17/api-types/downloadrequest/
//! [velero-modifiers]: https://velero.io/docs/v1.17/restore-resource-modifiers/

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{ConfigMap, Secret, SecretKeySelector, TypedLocalObjectReference};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use utoipa::ToSchema;
use zeabur_kube::DynamicObject;

use crate::backups::Outcome;
use crate::{Read, strings, text};

/// `velero.io/v1` `BackupStorageLocation`, cluster-wide.
pub const LOCATIONS: Read = Read {
    group: "velero.io",
    version: "v1",
    resource: "backupstoragelocations",
    kind: "BackupStorageLocation",
};

/// `velero.io/v1` `PodVolumeBackup`, cluster-wide.
pub const POD_VOLUME_BACKUPS: Read = Read {
    group: "velero.io",
    version: "v1",
    resource: "podvolumebackups",
    kind: "PodVolumeBackup",
};

/// `velero.io/v1` `Restore`, cluster-wide.
pub const RESTORES: Read = Read {
    group: "velero.io",
    version: "v1",
    resource: "restores",
    kind: "Restore",
};

/// Velero's namespace when none of its objects says otherwise, as its chart installs it.
pub const DEFAULT_NAMESPACE: &str = "velero";
/// The location Zeabur sets up, the one Velero's chart and CLI call `default`.
pub const LOCATION: &str = "default";
/// The schedule Zeabur sets up.
pub const SCHEDULE: &str = "zeabur";
/// The Secret holding the location's keys.
pub const CREDENTIALS: &str = "zeabur-backup-credentials";
/// Its key.
pub const CREDENTIALS_KEY: &str = "cloud";
/// The Secret holding the SSE-C key of a location Zeabur creates, as the catalog names it.
pub const BACKUP_KEY: &str = "velero-backup-key";
/// Its key. The AWS plugin needs a value of exactly 32 bytes.
pub const BACKUP_KEY_KEY: &str = "key";
/// The label Velero puts on a schedule's backups.
pub const SCHEDULE_LABEL: &str = "velero.io/schedule-name";
const BACKUP_LABEL: &str = "velero.io/backup-name";
const API_VERSION: &str = "velero.io/v1";

/// Whether Velero can reach a location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = VeleroLocationPhase)]
pub enum LocationPhase {
    /// Velero's last check reached it.
    Available,
    /// Velero's last check failed.
    Unavailable,
    /// Not checked yet.
    Unknown,
}

/// Where backups go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = VeleroLocation)]
pub struct Location {
    /// Its name.
    pub name: String,
    /// Velero's namespace.
    pub namespace: String,
    /// Backups that name no location go here.
    pub default: bool,
    /// The object store plugin (`aws`).
    pub provider: String,
    /// The bucket.
    pub bucket: String,
    /// The folder in the bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// The region.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// The S3-compatible endpoint (`s3Url`); absent for AWS itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// The Secret holding its keys; absent when it uses Velero's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    /// Whether Velero can reach it.
    pub phase: LocationPhase,
    /// When Velero last checked, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_validated: Option<String>,
    /// Velero's message about the last check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// When backups run and what they take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = VeleroSchedule)]
pub struct ScheduleView {
    /// Its name.
    pub name: String,
    /// Velero's namespace.
    pub namespace: String,
    /// The cron expression, in the Velero server's clock (UTC unless set otherwise).
    pub cron: String,
    /// No backups run while paused.
    pub paused: bool,
    /// How long each backup is kept, as Velero writes it (`168h0m0s`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
    /// The same in whole days, when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_days: Option<u32>,
    /// The namespaces it covers; empty for every namespace.
    pub namespaces: Vec<String>,
    /// The namespaces it leaves out.
    pub excluded: Vec<String>,
    /// It copies the files on every volume (file-system backup).
    pub fs_backup: bool,
    /// When it last made a backup, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_backup: Option<String>,
    /// Why Velero refused it (`status.validationErrors`).
    pub validation_errors: Vec<String>,
    /// It is the schedule Zeabur sets up ([`SCHEDULE`]).
    pub zeabur: bool,
}

/// Why a backup did not finish well, in the words the interface explains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = VeleroFailure)]
pub enum Failure {
    /// The location could not be reached or does not exist.
    LocationUnavailable,
    /// Velero refused the backup's settings before it started.
    Invalid,
    /// It finished, but some objects or volumes were not copied.
    ItemsFailed,
    /// The Velero server restarted while it ran.
    Interrupted,
    /// Anything else; `reason` says what Velero said.
    Other,
}

/// One backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = VeleroBackup)]
pub struct BackupView {
    /// Its name.
    pub name: String,
    /// The schedule that made it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// The namespaces it covers; empty for every namespace.
    pub namespaces: Vec<String>,
    /// Velero's phase.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// How it went.
    pub outcome: Outcome,
    /// When it started, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    /// When it finished, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<String>,
    /// When Velero deletes it, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    /// Objects copied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<u64>,
    /// Objects it set out to copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_items: Option<u64>,
    /// Errors Velero counted.
    pub errors: u64,
    /// Warnings Velero counted.
    pub warnings: u64,
    /// Bytes its file-system backups copied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// Why it did not finish well.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
    /// Velero's own words for it (`failureReason`, `validationErrors`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl BackupView {
    /// The time it is sorted by: started, else finished.
    #[must_use]
    pub fn time(&self) -> Option<&str> {
        self.started.as_deref().or(self.finished.as_deref())
    }
}

/// One restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = VeleroRestore)]
pub struct RestoreView {
    /// Its name.
    pub name: String,
    /// The backup it restores.
    pub backup: String,
    /// The namespaces it restores; empty for every namespace of the backup.
    pub namespaces: Vec<String>,
    /// The new namespace it restores into, when it maps one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub into: Option<String>,
    /// Velero's phase.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// How it went.
    pub outcome: Outcome,
    /// When it started, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    /// When it finished, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<String>,
    /// Errors Velero counted.
    pub errors: u64,
    /// Warnings Velero counted, such as objects that were already there.
    pub warnings: u64,
    /// Velero's own words when it failed (`failureReason`, `validationErrors`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The only kinds it restores (`includedResources`), as `resource.group`; empty for all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,
}

fn namespace(object: &DynamicObject) -> String {
    object.metadata.namespace.clone().unwrap_or_default()
}

fn name(object: &DynamicObject) -> String {
    object.metadata.name.clone().unwrap_or_default()
}

fn number(object: &DynamicObject, pointer: &str) -> Option<u64> {
    object.data.pointer(pointer).and_then(Value::as_u64)
}

fn flag(object: &DynamicObject, pointer: &str) -> bool {
    object.data.pointer(pointer).and_then(Value::as_bool) == Some(true)
}

/// A location as the interface shows it.
#[must_use]
pub fn location(object: &DynamicObject) -> Location {
    let phase = match text(object, "/status/phase").as_deref() {
        Some("Available") => LocationPhase::Available,
        Some("Unavailable") => LocationPhase::Unavailable,
        _ => LocationPhase::Unknown,
    };
    Location {
        name: name(object),
        namespace: namespace(object),
        default: flag(object, "/spec/default"),
        provider: text(object, "/spec/provider").unwrap_or_default(),
        bucket: text(object, "/spec/objectStorage/bucket").unwrap_or_default(),
        prefix: text(object, "/spec/objectStorage/prefix"),
        region: text(object, "/spec/config/region"),
        endpoint: text(object, "/spec/config/s3Url"),
        credential: text(object, "/spec/credential/name"),
        phase,
        last_validated: text(object, "/status/lastValidationTime"),
        message: text(object, "/status/message"),
    }
}

/// `168h0m0s` as 7; `None` for anything but whole days.
#[must_use]
pub fn keep_days(ttl: &str) -> Option<u32> {
    let hours: u32 = ttl
        .strip_suffix("h0m0s")
        .or_else(|| ttl.strip_suffix('h'))?
        .parse()
        .ok()?;
    (hours > 0 && hours.is_multiple_of(24)).then_some(hours / 24)
}

/// `days` as Velero writes a `ttl`.
#[must_use]
pub fn ttl(days: u32) -> String {
    format!("{}h0m0s", u64::from(days) * 24)
}

/// The namespaces a backup spec at `pointer` covers; empty for every namespace.
fn covered(object: &DynamicObject, pointer: &str) -> Vec<String> {
    let included = strings(object, &format!("{pointer}/includedNamespaces"));
    if included.iter().any(|n| n == "*") {
        Vec::new()
    } else {
        included
    }
}

/// A schedule as the interface shows it.
#[must_use]
pub fn schedule(object: &DynamicObject) -> ScheduleView {
    let ttl = text(object, "/spec/template/ttl");
    ScheduleView {
        name: name(object),
        namespace: namespace(object),
        cron: text(object, "/spec/schedule").unwrap_or_default(),
        paused: flag(object, "/spec/paused"),
        keep_days: ttl.as_deref().and_then(keep_days),
        ttl,
        namespaces: covered(object, "/spec/template"),
        excluded: strings(object, "/spec/template/excludedNamespaces"),
        fs_backup: flag(object, "/spec/template/defaultVolumesToFsBackup"),
        last_backup: text(object, "/status/lastBackup"),
        validation_errors: strings(object, "/status/validationErrors"),
        zeabur: object.metadata.name.as_deref() == Some(SCHEDULE),
    }
}

/// The bytes each backup's file-system backups copied, by backup name.
#[must_use]
pub fn volume_bytes(pod_volume_backups: &[DynamicObject]) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for pvb in pod_volume_backups {
        let Some(backup) = pvb
            .metadata
            .labels
            .as_ref()
            .and_then(|l| l.get(BACKUP_LABEL))
        else {
            continue;
        };
        let bytes = number(pvb, "/status/progress/bytesDone").unwrap_or(0);
        let total: &mut u64 = out.entry(backup.clone()).or_default();
        *total = total.saturating_add(bytes);
    }
    out
}

/// What went wrong, from the phase and Velero's words.
fn failure(phase: Option<&str>, reason: Option<&str>, errors: u64) -> Option<Failure> {
    let said = reason.unwrap_or_default().to_lowercase();
    let location = said.contains("backup storage location")
        || said.contains("backupstoragelocation")
        || said.contains("unavailable");
    match phase {
        Some("FailedValidation" | "Failed") if location => Some(Failure::LocationUnavailable),
        Some("FailedValidation") => Some(Failure::Invalid),
        Some("Failed") if said.contains("server starting") => Some(Failure::Interrupted),
        Some("Failed") => Some(Failure::Other),
        Some(
            "PartiallyFailed"
            | "WaitingForPluginOperationsPartiallyFailed"
            | "FinalizingPartiallyFailed",
        ) => Some(Failure::ItemsFailed),
        Some("Completed") if errors > 0 => Some(Failure::ItemsFailed),
        _ => None,
    }
}

/// A backup as the interface shows it; `bytes` from [`volume_bytes`].
#[must_use]
pub fn backup(object: &DynamicObject, bytes: &BTreeMap<String, u64>) -> BackupView {
    let phase = text(object, "/status/phase");
    let outcome = match phase.as_deref() {
        Some("Completed") => Outcome::Succeeded,
        Some("PartiallyFailed") => Outcome::PartiallyFailed,
        Some("Failed" | "FailedValidation") => Outcome::Failed,
        Some(
            "New"
            | "InProgress"
            | "WaitingForPluginOperations"
            | "WaitingForPluginOperationsPartiallyFailed"
            | "Finalizing"
            | "FinalizingPartiallyFailed",
        ) => Outcome::Running,
        _ => Outcome::Unknown,
    };
    let validation = strings(object, "/status/validationErrors");
    let reason = text(object, "/status/failureReason")
        .or_else(|| (!validation.is_empty()).then(|| validation.join("; ")));
    let errors = number(object, "/status/errors").unwrap_or(0);
    let name = name(object);
    BackupView {
        schedule: object
            .metadata
            .labels
            .as_ref()
            .and_then(|l| l.get(SCHEDULE_LABEL))
            .cloned(),
        namespaces: covered(object, "/spec"),
        failure: failure(phase.as_deref(), reason.as_deref(), errors),
        phase,
        outcome,
        started: text(object, "/status/startTimestamp"),
        finished: text(object, "/status/completionTimestamp"),
        expires: text(object, "/status/expiration"),
        items: number(object, "/status/progress/itemsBackedUp"),
        total_items: number(object, "/status/progress/totalItems"),
        errors,
        warnings: number(object, "/status/warnings").unwrap_or(0),
        bytes: bytes.get(&name).copied(),
        reason,
        name,
    }
}

/// A restore as the interface shows it; one without a phase is new, not yet taken up.
#[must_use]
pub fn restore(object: &DynamicObject) -> RestoreView {
    let phase = text(object, "/status/phase");
    let outcome = match phase.as_deref() {
        Some("Completed") => Outcome::Succeeded,
        Some("PartiallyFailed") => Outcome::PartiallyFailed,
        Some("Failed" | "FailedValidation") => Outcome::Failed,
        Some(
            "New"
            | "InProgress"
            | "WaitingForPluginOperations"
            | "WaitingForPluginOperationsPartiallyFailed"
            | "Finalizing"
            | "FinalizingPartiallyFailed",
        )
        | None => Outcome::Running,
        _ => Outcome::Unknown,
    };
    let validation = strings(object, "/status/validationErrors");
    RestoreView {
        name: name(object),
        backup: text(object, "/spec/backupName").unwrap_or_default(),
        into: object
            .data
            .pointer("/spec/namespaceMapping")
            .and_then(Value::as_object)
            .and_then(|m| m.values().find_map(Value::as_str))
            .map(str::to_owned),
        namespaces: covered(object, "/spec"),
        outcome,
        started: text(object, "/status/startTimestamp"),
        finished: text(object, "/status/completionTimestamp"),
        errors: number(object, "/status/errors").unwrap_or(0),
        warnings: number(object, "/status/warnings").unwrap_or(0),
        reason: text(object, "/status/failureReason")
            .or_else(|| (!validation.is_empty()).then(|| validation.join("; "))),
        resources: strings(object, "/spec/includedResources")
            .into_iter()
            .filter(|r| r != "*")
            .collect(),
        phase,
    }
}

/// Velero's namespace: where its locations or schedules are, else [`DEFAULT_NAMESPACE`].
#[must_use]
pub fn velero_namespace(locations: &[DynamicObject], schedules: &[DynamicObject]) -> String {
    locations
        .iter()
        .chain(schedules)
        .find_map(|o| o.metadata.namespace.clone())
        .unwrap_or_else(|| DEFAULT_NAMESPACE.to_owned())
}

/// Where backups should go, as a person sets it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = VeleroLocationRequest)]
pub struct LocationRequest {
    /// The bucket.
    pub bucket: String,
    /// A folder in the bucket; the bucket's top when empty.
    #[serde(default)]
    pub prefix: String,
    /// The region; `us-east-1` when empty, which most S3-compatible stores accept.
    #[serde(default)]
    pub region: String,
    /// The endpoint, `https://…`; empty for AWS S3 itself.
    #[serde(default)]
    pub endpoint: String,
}

/// When backups run and what they take, as a person sets it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = VeleroScheduleRequest)]
pub struct ScheduleRequest {
    /// Five cron fields, in the Velero server's clock.
    pub cron: String,
    /// How many days each backup is kept, 1 to 365.
    pub keep_days: u32,
    /// The namespaces to back up; empty for every namespace.
    #[serde(default)]
    pub namespaces: Vec<String>,
}

const DEFAULT_REGION: &str = "us-east-1";

fn bucket_ok(bucket: &str) -> bool {
    (3..=63).contains(&bucket.len())
        && bucket
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        && !bucket.starts_with(['.', '-'])
        && !bucket.ends_with(['.', '-'])
}

/// Why a location request cannot be written, in a sentence; `None` when it can.
#[must_use]
pub fn location_problem(request: &LocationRequest) -> Option<String> {
    if !bucket_ok(request.bucket.trim()) {
        return Some(
            "Enter a bucket name of 3 to 63 lowercase letters, digits, dots and hyphens.".into(),
        );
    }
    let endpoint = request.endpoint.trim();
    let scheme = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"));
    if !endpoint.is_empty() && scheme.is_none_or(|rest| rest.is_empty() || rest.contains(' ')) {
        return Some("Enter the endpoint as an http:// or https:// address.".into());
    }
    if request.prefix.contains("..") || request.prefix.chars().any(char::is_whitespace) {
        return Some("Enter a folder without spaces or \"..\".".into());
    }
    None
}

fn cron_ok(cron: &str) -> bool {
    let fields: Vec<&str> = cron.split_whitespace().collect();
    fields.len() == 5
        && fields.iter().all(|f| {
            f.bytes()
                .all(|b| b.is_ascii_digit() || b",-*/".contains(&b))
        })
}

fn namespace_ok(name: &str) -> bool {
    (1..=63).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

/// Why a schedule request cannot be written, in a sentence; `None` when it can.
#[must_use]
pub fn schedule_problem(request: &ScheduleRequest) -> Option<String> {
    if !cron_ok(&request.cron) {
        return Some("Enter the schedule as five cron fields, such as 0 3 * * *.".into());
    }
    if !(1..=365).contains(&request.keep_days) {
        return Some("Keep backups from 1 to 365 days.".into());
    }
    if let Some(bad) = request.namespaces.iter().find(|n| !namespace_ok(n)) {
        return Some(format!("{bad:?} is not a namespace name."));
    }
    None
}

/// A `velero.io/v1` object with the spec `S`. Top-level fields Zeabur does not set (`status`)
/// stay as read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VeleroObject<S> {
    /// `velero.io/v1`.
    #[serde(default)]
    pub api_version: String,
    /// The kind.
    #[serde(default)]
    pub kind: String,
    /// Its metadata.
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// Its spec.
    pub spec: S,
    /// The other top-level fields.
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl<S> VeleroObject<S> {
    fn new(kind: &str, namespace: &str, name: &str, spec: S) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            kind: kind.to_owned(),
            metadata: ObjectMeta {
                name: Some(name.to_owned()),
                namespace: Some(namespace.to_owned()),
                ..ObjectMeta::default()
            },
            spec,
            rest: Map::new(),
        }
    }
}

/// The fields of a `BackupStorageLocation` spec Zeabur sets [velero-bsl]; the others stay.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationSpec {
    /// The object store plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Backups that name no location go here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
    /// The bucket and the folder in it.
    #[serde(default)]
    pub object_storage: ObjectStorage,
    /// The plugin's settings.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, String>,
    /// The Secret key with the plugin's keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<SecretKeySelector>,
    /// The other fields.
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// A location's `objectStorage`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectStorage {
    /// The bucket.
    #[serde(default)]
    pub bucket: String,
    /// The folder in the bucket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// The other fields.
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// The fields of a `Schedule` spec Zeabur sets [velero-schedule]; the others stay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleSpec {
    /// The cron.
    #[serde(default)]
    pub schedule: String,
    /// Whether it makes no backups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused: Option<bool>,
    /// The spec of each backup.
    #[serde(default)]
    pub template: BackupSpec,
    /// The other fields.
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// The fields of a `Backup` spec Zeabur sets [velero-backup]; the others stay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSpec {
    /// The namespaces, `*` for all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub included_namespaces: Option<Vec<String>>,
    /// How long Velero keeps the backup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
    /// File-system backup of every volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_volumes_to_fs_backup: Option<bool>,
    /// The other fields.
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// The fields of a `Restore` spec Zeabur sets [velero-restore].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSpec {
    /// The backup.
    pub backup_name: String,
    /// The namespaces of the backup it restores.
    pub included_namespaces: Vec<String>,
    /// Each namespace of the backup and the one it goes into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace_mapping: Option<BTreeMap<String, String>>,
    /// The kinds it leaves out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excluded_resources: Option<Vec<String>>,
    /// The only kinds it restores.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub included_resources: Option<Vec<String>>,
    /// The resource modifier ConfigMap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_modifier: Option<TypedLocalObjectReference>,
}

/// A `DownloadRequest` spec [velero-download].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DownloadRequestSpec {
    /// What it asks for.
    pub target: DownloadTarget,
}

/// What a `DownloadRequest` asks for, of which backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DownloadTarget {
    /// What.
    pub kind: Download,
    /// The backup.
    pub name: String,
}

/// The location as it will be: `current` with the request's fields, or a new default location.
///
/// `credential` names the Secret with its keys, or keeps the one it names (none: Velero's own).
/// A new location has the store encrypt every object with the key in [`BACKUP_KEY`] (SSE-C).
/// A current location keeps its encryption: a key added later makes older objects unreadable
/// on stores that refuse SSE-C headers for objects written without them.
///
/// # Errors
///
/// When `current` is not a `BackupStorageLocation`.
pub fn location_object(
    namespace: &str,
    current: Option<&Value>,
    request: &LocationRequest,
    credential: bool,
) -> Result<VeleroObject<LocationSpec>, serde_json::Error> {
    let mut object = match current {
        Some(current) => VeleroObject::deserialize(current)?,
        None => VeleroObject::new(
            "BackupStorageLocation",
            namespace,
            LOCATION,
            LocationSpec {
                provider: Some("aws".to_owned()),
                default: Some(true),
                config: BTreeMap::from([(
                    "customerKeyEncryptionSecret".to_owned(),
                    format!("{BACKUP_KEY}/{BACKUP_KEY_KEY}"),
                )]),
                ..LocationSpec::default()
            },
        ),
    };
    let region = match request.region.trim() {
        "" => DEFAULT_REGION,
        region => region,
    };
    let prefix = request.prefix.trim().trim_matches('/');
    let spec = &mut object.spec;
    request
        .bucket
        .trim()
        .clone_into(&mut spec.object_storage.bucket);
    spec.object_storage.prefix = (!prefix.is_empty()).then(|| prefix.to_owned());
    spec.config.insert("region".to_owned(), region.to_owned());
    let endpoint = request.endpoint.trim().trim_end_matches('/');
    if endpoint.is_empty() {
        spec.config.remove("s3Url");
        spec.config.remove("s3ForcePathStyle");
    } else {
        spec.config.insert("s3Url".to_owned(), endpoint.to_owned());
        spec.config
            .insert("s3ForcePathStyle".to_owned(), "true".to_owned());
    }
    if credential {
        spec.credential = Some(SecretKeySelector {
            name: CREDENTIALS.to_owned(),
            key: CREDENTIALS_KEY.to_owned(),
            optional: None,
        });
    }
    Ok(object)
}

/// The keys as the AWS plugin reads them, an AWS shared credentials file.
#[must_use]
pub fn credentials_file(access_key_id: &str, secret_access_key: &str) -> String {
    format!(
        "[default]\naws_access_key_id={}\naws_secret_access_key={}\n",
        access_key_id.trim(),
        secret_access_key.trim()
    )
}

fn opaque_secret(namespace: &str, name: &str, key: &str, value: &str) -> Secret {
    Secret {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            ..ObjectMeta::default()
        },
        type_: Some("Opaque".to_owned()),
        string_data: Some(BTreeMap::from([(key.to_owned(), value.to_owned())])),
        ..Secret::default()
    }
}

/// The Secret with the SSE-C key `key` of a new location.
#[must_use]
pub fn backup_key_secret(namespace: &str, key: &str) -> Secret {
    opaque_secret(namespace, BACKUP_KEY, BACKUP_KEY_KEY, key)
}

/// The Secret with the keys under `string_data`, as Zeabur writes it with `stringData`.
#[must_use]
pub fn credentials_secret(namespace: &str, string_data: &str) -> Secret {
    opaque_secret(namespace, CREDENTIALS, CREDENTIALS_KEY, string_data)
}

/// The name of Velero's node agent, the DaemonSet that copies volume files.
pub const NODE_AGENT: &str = "node-agent";

/// The schedule as it will be: `current` with the request's fields, or Zeabur's new schedule.
/// Whatever else `current` sets (hooks, other filters, its location) stays.
///
/// `node_agent` is whether the node agent runs; `None` when that is not known, which keeps the
/// file-system backup setting of `current` and turns it off in a new schedule.
///
/// # Errors
///
/// When `current` is not a `Schedule`.
pub fn schedule_object(
    namespace: &str,
    current: Option<&Value>,
    request: &ScheduleRequest,
    node_agent: Option<bool>,
) -> Result<VeleroObject<ScheduleSpec>, serde_json::Error> {
    let mut object = match current {
        Some(current) => VeleroObject::deserialize(current)?,
        None => VeleroObject::new("Schedule", namespace, SCHEDULE, ScheduleSpec::default()),
    };
    object.spec.schedule = request
        .cron
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let namespaces = if request.namespaces.is_empty() {
        vec!["*".to_owned()]
    } else {
        request.namespaces.clone()
    };
    let template = &mut object.spec.template;
    template.included_namespaces = Some(namespaces);
    template.ttl = Some(ttl(request.keep_days));
    if let Some(files) = node_agent.or_else(|| current.is_none().then_some(false)) {
        template.default_volumes_to_fs_backup = Some(files);
    }
    Ok(object)
}

/// The schedule paused or resumed.
///
/// # Errors
///
/// When `current` is not a `Schedule`.
pub fn paused(
    current: &Value,
    paused: bool,
) -> Result<VeleroObject<ScheduleSpec>, serde_json::Error> {
    let mut object: VeleroObject<ScheduleSpec> = VeleroObject::deserialize(current)?;
    object.spec.paused = Some(paused);
    Ok(object)
}

/// The name of a backup made now from `schedule`, as `velero backup create --from-schedule`
/// names it: the schedule's name and the UTC time as `YYYYMMDDhhmmss`.
#[must_use]
pub fn backup_name(schedule: &str, compact_time: &str) -> String {
    format!("{schedule}-{compact_time}")
}

/// A backup now with `schedule`'s template, labelled as the schedule's. Without the node agent
/// (`node_agent` is `Some(false)`) it does not ask for file-system backup.
///
/// # Errors
///
/// When `schedule` is not a `Schedule`.
pub fn backup_now(
    schedule: &Value,
    name: &str,
    node_agent: Option<bool>,
) -> Result<VeleroObject<BackupSpec>, serde_json::Error> {
    let schedule: VeleroObject<ScheduleSpec> = VeleroObject::deserialize(schedule)?;
    let mut labels = schedule.metadata.labels.unwrap_or_default();
    labels.insert(
        SCHEDULE_LABEL.to_owned(),
        schedule.metadata.name.unwrap_or_default(),
    );
    let mut spec = schedule.spec.template;
    if node_agent == Some(false) && spec.default_volumes_to_fs_backup == Some(true) {
        spec.default_volumes_to_fs_backup = Some(false);
    }
    Ok(VeleroObject {
        api_version: API_VERSION.to_owned(),
        kind: "Backup".to_owned(),
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: schedule.metadata.namespace,
            labels: Some(labels),
            ..ObjectMeta::default()
        },
        spec,
        rest: Map::new(),
    })
}

/// What is wrong with restoring `namespace` from a backup of `covered` (empty for every
/// namespace); `None` when nothing is.
#[must_use]
pub fn restore_problem(covered: &[String], namespace: &str) -> Option<String> {
    if namespace.is_empty() {
        return Some("Choose the namespace to restore.".to_owned());
    }
    (!covered.is_empty() && !covered.iter().any(|n| n == namespace))
        .then(|| format!("The backup does not hold the namespace {namespace}."))
}

/// What is wrong with `into` as the name of a new namespace; `None` when nothing is.
#[must_use]
pub fn new_namespace_problem(source: &str, into: &str) -> Option<String> {
    if into == source {
        return Some(format!(
            "{source} still exists; choose another name for the new project."
        ));
    }
    if into.starts_with("kube-") {
        return Some(format!(
            "Names starting with kube- are kept for Kubernetes; {into} cannot be a project."
        ));
    }
    (!namespace_ok(into)).then(|| {
        format!(
            "{into:?} cannot name a namespace: use at most 63 lowercase letters, digits and \
             hyphens, starting and ending with a letter or digit."
        )
    })
}

/// The kinds that carry hostnames, as Velero names them (`resource.group`) [velero-restore].
pub const ROUTES: [&str; 2] = [
    "ingresses.networking.k8s.io",
    "httproutes.gateway.networking.k8s.io",
];

/// The kinds every project restore leaves out, so it claims no hostname.
///
/// These are the [`ROUTES`], and cert-manager's certificates and ACME challenges for those
/// hostnames. cert-manager makes the certificates again when a route comes back.
pub const HELD_BACK: [&str; 6] = [
    ROUTES[0],
    ROUTES[1],
    "certificates.cert-manager.io",
    "certificaterequests.cert-manager.io",
    "orders.acme.cert-manager.io",
    "challenges.acme.cert-manager.io",
];

fn restore_spec(backup: &str, source: &str, into: &str) -> RestoreSpec {
    RestoreSpec {
        backup_name: backup.to_owned(),
        included_namespaces: vec![source.to_owned()],
        namespace_mapping: (into != source)
            .then(|| BTreeMap::from([(source.to_owned(), into.to_owned())])),
        ..RestoreSpec::default()
    }
}

fn kinds(kinds: &[&str]) -> Vec<String> {
    kinds.iter().map(|k| (*k).to_owned()).collect()
}

/// A restore of `source` from `backup` without the [`HELD_BACK`] kinds, in Velero's namespace
/// `namespace`.
///
/// With `into`, the restore goes into that new namespace and applies the resource modifier of
/// the same name ([`paused_cron_jobs`]). Without it, the restore keeps the name `source`.
#[must_use]
pub fn restore_object(
    namespace: &str,
    name: &str,
    backup: &str,
    source: &str,
    into: Option<&str>,
) -> VeleroObject<RestoreSpec> {
    let mut spec = restore_spec(backup, source, into.unwrap_or(source));
    spec.excluded_resources = Some(kinds(&HELD_BACK));
    if into.is_some() {
        spec.resource_modifier = Some(TypedLocalObjectReference {
            api_group: None,
            kind: "ConfigMap".to_owned(),
            name: name.to_owned(),
        });
    }
    VeleroObject::new("Restore", namespace, name, spec)
}

/// The resource modifier `name` in Velero's namespace `namespace`: Velero sets `spec.suspend` on
/// each CronJob of `source` before it creates it [velero-modifiers].
///
/// The rules match the namespace in the backup, not the mapped one. Velero reads a JSON Patch
/// `value` of `"true"` as the boolean.
#[must_use]
pub fn paused_cron_jobs(namespace: &str, name: &str, source: &str) -> ConfigMap {
    let rules = format!(
        "version: v1\n\
         resourceModifierRules:\n\
         - conditions:\n    groupResource: cronjobs.batch\n    namespaces:\n    - {source}\n  \
         patches:\n  - operation: add\n    path: /spec/suspend\n    value: \"true\"\n"
    );
    ConfigMap {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            ..ObjectMeta::default()
        },
        data: Some(BTreeMap::from([("cronjobs.yaml".to_owned(), rules)])),
        ..ConfigMap::default()
    }
}

/// The JSON Patch that makes the `Restore` `restore` own an object, so the garbage collector
/// deletes the object with it; `None` when `restore` has no `uid` yet.
#[must_use]
pub fn owned_by(restore: &DynamicObject) -> Option<Value> {
    let uid = restore.metadata.uid.as_deref()?;
    let name = restore.metadata.name.as_deref()?;
    Some(
        json!([{"op": "add", "path": "/metadata/ownerReferences", "value": [{
            "apiVersion": API_VERSION, "kind": "Restore", "name": name, "uid": uid,
        }]}]),
    )
}

/// A restore of only the routes of `source` from `backup` into the namespace `into`, which may
/// be `source` itself [velero-restore]. Velero skips a route of the same name that is there.
#[must_use]
pub fn domains_restore(
    namespace: &str,
    name: &str,
    backup: &str,
    source: &str,
    into: &str,
) -> VeleroObject<RestoreSpec> {
    let mut spec = restore_spec(backup, source, into);
    spec.included_resources = Some(kinds(&ROUTES));
    VeleroObject::new("Restore", namespace, name, spec)
}

/// What a backup holds of one namespace, from Velero's list of the backup's objects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = VeleroBackupInventory)]
pub struct Inventory {
    /// The Services, whose in-cluster addresses name the namespace.
    pub services: Vec<String>,
    /// The Secrets.
    pub secrets: Vec<String>,
    /// The CronJobs.
    pub cron_jobs: Vec<String>,
    /// The routes, `Kind/name`.
    pub routes: Vec<String>,
}

/// The objects of `namespace` in a backup's list of objects (`BackupResourceList`): a map from
/// `group/version/Kind` (`v1/Kind` for the core group) to `namespace/name` [velero-describe].
///
/// [velero-describe]: https://github.com/vmware-tanzu/velero/blob/v1.17.0/pkg/backup/backed_up_items_map.go
#[must_use]
pub fn inventory(list: &BTreeMap<String, Vec<String>>, namespace: &str) -> Inventory {
    let mut out = Inventory::default();
    for (key, entries) in list {
        let parts: Vec<&str> = key.split('/').collect();
        let (group, kind) = match parts.as_slice() {
            [_, kind] => ("", *kind),
            [group, _, kind] => (*group, *kind),
            _ => continue,
        };
        let names = entries
            .iter()
            .filter_map(|e| e.split_once('/'))
            .filter(|(ns, _)| *ns == namespace)
            .map(|(_, name)| name);
        match (group, kind) {
            ("", "Service") => out.services.extend(names.map(str::to_owned)),
            ("", "Secret") => out.secrets.extend(
                names
                    .filter(|n| !n.starts_with("sh.helm.release.v1."))
                    .map(str::to_owned),
            ),
            ("batch", "CronJob") => out.cron_jobs.extend(names.map(str::to_owned)),
            ("networking.k8s.io", "Ingress") | ("gateway.networking.k8s.io", "HTTPRoute") => {
                out.routes.extend(names.map(|n| format!("{kind}/{n}")));
            }
            _ => {}
        }
    }
    for list in [
        &mut out.services,
        &mut out.secrets,
        &mut out.cron_jobs,
        &mut out.routes,
    ] {
        list.sort();
        list.dedup();
    }
    out
}

/// An Ingress or an HTTPRoute with its spec as read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// `networking.k8s.io/v1` or `gateway.networking.k8s.io/v1`.
    #[serde(default)]
    pub api_version: String,
    /// `Ingress` or `HTTPRoute`.
    #[serde(default)]
    pub kind: String,
    /// Its metadata.
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// Its spec, the same for the copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<Value>,
}

/// The copy of the route `route` that `into` gets when its hostnames move there: the same spec,
/// labels and annotations, without what the server or `kubectl` set.
///
/// # Errors
///
/// When `route` has no object metadata.
pub fn moved_route(route: &Value, into: &str) -> Result<Route, serde_json::Error> {
    let route = Route::deserialize(route)?;
    let annotations = route.metadata.annotations.and_then(|mut annotations| {
        annotations.remove("kubectl.kubernetes.io/last-applied-configuration");
        (!annotations.is_empty()).then_some(annotations)
    });
    Ok(Route {
        metadata: ObjectMeta {
            name: route.metadata.name,
            namespace: Some(into.to_owned()),
            labels: route.metadata.labels,
            annotations,
            ..ObjectMeta::default()
        },
        ..route
    })
}

#[derive(Deserialize)]
struct HostsView {
    #[serde(default)]
    spec: HostsSpec,
}

#[derive(Default, Deserialize)]
struct HostsSpec {
    #[serde(default)]
    rules: Vec<HostRule>,
    #[serde(default)]
    hostnames: Vec<String>,
}

#[derive(Deserialize)]
struct HostRule {
    host: Option<String>,
}

/// The hostnames of an Ingress (`spec.rules[].host`) or an HTTPRoute (`spec.hostnames`).
///
/// # Errors
///
/// When `route` is neither.
pub fn route_hosts(route: &Value) -> Result<Vec<String>, serde_json::Error> {
    let HostsView { spec } = HostsView::deserialize(route)?;
    let mut out: Vec<String> = spec
        .rules
        .into_iter()
        .filter_map(|r| r.host)
        .chain(spec.hostnames)
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

/// What a `DownloadRequest` asks for [velero-download].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Download {
    /// The archive of the backup's objects, `.tar.gz`.
    BackupContents,
    /// The list of the backup's objects, gzipped JSON.
    BackupResourceList,
}

/// A `DownloadRequest` for `what` of `backup` [velero-download].
#[must_use]
pub fn download_request(
    namespace: &str,
    name: &str,
    backup: &str,
    what: Download,
) -> VeleroObject<DownloadRequestSpec> {
    VeleroObject::new(
        "DownloadRequest",
        namespace,
        name,
        DownloadRequestSpec {
            target: DownloadTarget {
                kind: what,
                name: backup.to_owned(),
            },
        },
    )
}

/// The signed URL a `DownloadRequest` holds once Velero processed it, and when it expires.
#[must_use]
pub fn download_url(object: &DynamicObject) -> Option<(String, Option<String>)> {
    (text(object, "/status/phase").as_deref() == Some("Processed"))
        .then(|| text(object, "/status/downloadURL"))
        .flatten()
        .map(|url| (url, text(object, "/status/expiration")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    fn json_of(object: impl Serialize) -> Value {
        serde_json::to_value(object).unwrap()
    }

    #[test]
    fn a_location_reads_its_bucket_endpoint_keys_and_last_check() {
        let found = location(&object(json!({
            "apiVersion": API_VERSION, "kind": "BackupStorageLocation",
            "metadata": {"name": "default", "namespace": "velero"},
            "spec": {"provider": "aws", "default": true,
                     "objectStorage": {"bucket": "backups", "prefix": "prod"},
                     "config": {"region": "auto", "s3Url": "https://r2.example.com"},
                     "credential": {"name": CREDENTIALS, "key": "cloud"}},
            "status": {"phase": "Unavailable", "lastValidationTime": "2026-10-23T03:00:00Z",
                       "message": "BackupStorageLocation \"default\" is unavailable: AccessDenied"}
        })));
        assert_eq!(found.bucket, "backups");
        assert_eq!(found.prefix.as_deref(), Some("prod"));
        assert_eq!(found.endpoint.as_deref(), Some("https://r2.example.com"));
        assert_eq!(found.credential.as_deref(), Some(CREDENTIALS));
        assert_eq!(found.phase, LocationPhase::Unavailable);
        assert!(found.default);
    }

    #[test]
    fn a_new_location_speaks_s3_path_style_and_names_zeaburs_secrets() {
        let request = LocationRequest {
            bucket: "backups".into(),
            prefix: "/cluster/".into(),
            region: String::new(),
            endpoint: "https://minio.example.com/".into(),
        };
        let written = json_of(location_object("velero", None, &request, true).unwrap());
        assert_eq!(
            written["spec"],
            json!({"provider": "aws", "default": true,
                   "objectStorage": {"bucket": "backups", "prefix": "cluster"},
                   "config": {"region": "us-east-1", "s3Url": "https://minio.example.com",
                              "s3ForcePathStyle": "true",
                              "customerKeyEncryptionSecret": "velero-backup-key/key"},
                   "credential": {"name": CREDENTIALS, "key": "cloud"}})
        );
    }

    #[test]
    fn editing_a_location_keeps_what_it_does_not_set_and_its_keys_when_none_are_given() {
        let current = json!({
            "apiVersion": API_VERSION, "kind": "BackupStorageLocation",
            "metadata": {"name": "default", "namespace": "velero", "resourceVersion": "4"},
            "spec": {"provider": "aws", "default": true, "accessMode": "ReadWrite",
                     "objectStorage": {"bucket": "old", "prefix": "x"},
                     "config": {"region": "us-east-1", "s3Url": "http://minio:9000",
                                "s3ForcePathStyle": "true", "checksumAlgorithm": ""}}
        });
        let request = LocationRequest {
            bucket: "new".into(),
            prefix: String::new(),
            region: "eu-west-1".into(),
            endpoint: String::new(),
        };
        let written = json_of(location_object("velero", Some(&current), &request, false).unwrap());
        assert_eq!(written["spec"]["accessMode"], "ReadWrite");
        assert_eq!(written["spec"]["objectStorage"], json!({"bucket": "new"}));
        assert_eq!(
            written["spec"]["config"],
            json!({"region": "eu-west-1", "checksumAlgorithm": ""})
        );
        assert!(written["spec"].get("credential").is_none());
    }

    #[test]
    fn a_schedule_backs_up_every_namespace_or_the_chosen_ones_with_their_volumes() {
        let all = json_of(
            schedule_object(
                "velero",
                None,
                &ScheduleRequest {
                    cron: "0  3 * * *".into(),
                    keep_days: 7,
                    namespaces: vec![],
                },
                Some(true),
            )
            .unwrap(),
        );
        assert_eq!(all["spec"]["schedule"], "0 3 * * *");
        assert_eq!(
            all["spec"]["template"],
            json!({"includedNamespaces": ["*"], "ttl": "168h0m0s",
                   "defaultVolumesToFsBackup": true})
        );
        let view = schedule(&object(all));
        assert_eq!(view.namespaces, Vec::<String>::new());
        assert_eq!(view.keep_days, Some(7));
        assert!(view.fs_backup && view.zeabur);

        let current = json!({"apiVersion": API_VERSION, "kind": "Schedule",
            "metadata": {"name": "zeabur", "namespace": "velero"},
            "spec": {"schedule": "0 3 * * *", "paused": true,
                     "template": {"includedNamespaces": ["*"], "storageLocation": "default",
                                  "hooks": {}}}});
        let some = json_of(
            schedule_object(
                "velero",
                Some(&current),
                &ScheduleRequest {
                    cron: "0 1 * * *".into(),
                    keep_days: 30,
                    namespaces: vec!["shop".into(), "blog".into()],
                },
                None,
            )
            .unwrap(),
        );
        assert_eq!(some["spec"]["paused"], true);
        assert_eq!(some["spec"]["template"]["storageLocation"], "default");
        assert_eq!(
            some["spec"]["template"]["includedNamespaces"],
            json!(["shop", "blog"])
        );
        assert_eq!(some["spec"]["template"]["ttl"], "720h0m0s");
    }

    #[test]
    fn a_schedule_asks_for_file_backup_only_where_the_node_agent_runs() {
        let request = ScheduleRequest {
            cron: "0 3 * * *".into(),
            keep_days: 7,
            namespaces: vec![],
        };
        let files = |object: &Value| object["spec"]["template"]["defaultVolumesToFsBackup"].clone();
        assert_eq!(
            files(&json_of(
                schedule_object("velero", None, &request, Some(false)).unwrap()
            )),
            json!(false)
        );
        assert_eq!(
            files(&json_of(
                schedule_object("velero", None, &request, None).unwrap()
            )),
            json!(false)
        );
        let current = json_of(schedule_object("velero", None, &request, Some(true)).unwrap());
        assert_eq!(
            files(&json_of(
                schedule_object("velero", Some(&current), &request, Some(false)).unwrap()
            )),
            json!(false)
        );
        assert_eq!(
            files(&json_of(
                schedule_object("velero", Some(&current), &request, None).unwrap()
            )),
            json!(true)
        );
        let now = json_of(backup_now(&current, "zeabur-20261026000000", Some(false)).unwrap());
        assert_eq!(now["spec"]["defaultVolumesToFsBackup"], json!(false));
    }

    #[test]
    fn requests_that_velero_would_refuse_are_named() {
        let location = |bucket: &str, endpoint: &str| LocationRequest {
            bucket: bucket.into(),
            prefix: String::new(),
            region: String::new(),
            endpoint: endpoint.into(),
        };
        assert!(location_problem(&location("Backups", "")).is_some());
        assert!(location_problem(&location("backups", "minio:9000")).is_some());
        assert!(location_problem(&location("backups", "http://minio:9000")).is_none());
        let schedule = |cron: &str, keep_days: u32, namespaces: &[&str]| ScheduleRequest {
            cron: cron.into(),
            keep_days,
            namespaces: namespaces.iter().map(|n| (*n).to_owned()).collect(),
        };
        assert!(schedule_problem(&schedule("@daily", 7, &[])).is_some());
        assert!(schedule_problem(&schedule("0 3 * * *", 0, &[])).is_some());
        assert!(schedule_problem(&schedule("0 3 * * *", 7, &["Shop"])).is_some());
        assert!(schedule_problem(&schedule("0 */6 * * 1-5", 365, &["shop"])).is_none());
    }

    #[test]
    fn a_backup_now_copies_the_schedules_template_and_carries_its_label() {
        let current = json!({"apiVersion": API_VERSION, "kind": "Schedule",
            "metadata": {"name": "zeabur", "namespace": "velero", "labels": {"team": "ops"}},
            "spec": {"schedule": "0 3 * * *",
                     "template": {"includedNamespaces": ["*"], "ttl": "168h0m0s",
                                  "defaultVolumesToFsBackup": true}}});
        let name = backup_name("zeabur", "20261023101500");
        let made = json_of(backup_now(&current, &name, None).unwrap());
        assert_eq!(made["metadata"]["name"], "zeabur-20261023101500");
        assert_eq!(
            made["metadata"]["labels"],
            json!({"team": "ops", SCHEDULE_LABEL: "zeabur"})
        );
        assert_eq!(made["spec"], current["spec"]["template"]);
    }

    #[test]
    fn failures_are_told_apart_and_sizes_come_from_the_volume_backups() {
        let run = |phase: &str, status: Value| {
            let mut status = status;
            status["phase"] = json!(phase);
            object(json!({"apiVersion": API_VERSION, "kind": "Backup",
                "metadata": {"name": "zeabur-1", "namespace": "velero",
                             "labels": {SCHEDULE_LABEL: "zeabur"}},
                "spec": {"includedNamespaces": ["*"]}, "status": status}))
        };
        let pvbs = [
            object(json!({"apiVersion": API_VERSION, "kind": "PodVolumeBackup",
                "metadata": {"name": "a", "namespace": "velero", "labels": {BACKUP_LABEL: "zeabur-1"}},
                "status": {"progress": {"bytesDone": 1000, "totalBytes": 1000}}})),
            object(json!({"apiVersion": API_VERSION, "kind": "PodVolumeBackup",
                "metadata": {"name": "b", "namespace": "velero", "labels": {BACKUP_LABEL: "zeabur-1"}},
                "status": {"progress": {"bytesDone": 24}}})),
        ];
        let bytes = volume_bytes(&pvbs);
        let done = backup(
            &run(
                "Completed",
                json!({"progress": {"itemsBackedUp": 120, "totalItems": 120}}),
            ),
            &bytes,
        );
        assert_eq!(
            (done.outcome, done.failure, done.bytes, done.items),
            (Outcome::Succeeded, None, Some(1024), Some(120))
        );
        assert_eq!(done.schedule.as_deref(), Some("zeabur"));
        assert_eq!(done.namespaces, Vec::<String>::new());
        let unreachable = backup(
            &run(
                "FailedValidation",
                json!({"validationErrors": [
                    "an existing backup storage location wasn't specified at backup creation time and the default 'default' wasn't found"]}),
            ),
            &bytes,
        );
        assert_eq!(unreachable.failure, Some(Failure::LocationUnavailable));
        let restarted = backup(
            &run(
                "Failed",
                json!({"failureReason": "found a backup with status \"InProgress\" during the server starting, mark it as \"Failed\""}),
            ),
            &bytes,
        );
        assert_eq!(restarted.failure, Some(Failure::Interrupted));
        let partly = backup(&run("PartiallyFailed", json!({"errors": 2})), &bytes);
        assert_eq!(
            (partly.outcome, partly.failure, partly.errors),
            (Outcome::PartiallyFailed, Some(Failure::ItemsFailed), 2)
        );
    }

    #[test]
    fn a_restore_copies_into_a_new_namespace_without_routes_or_keeps_the_name_of_a_missing_one() {
        let copy = json_of(restore_object(
            "velero",
            "zeabur-1-2",
            "zeabur-1",
            "shop",
            Some("shop-2"),
        ));
        let mut read = object(copy.clone());
        read.data["status"] = json!({"phase": "PartiallyFailed", "warnings": 3, "errors": 1});
        let view = restore(&read);
        assert_eq!(view.backup, "zeabur-1");
        assert_eq!(view.namespaces, ["shop"]);
        assert_eq!(view.into.as_deref(), Some("shop-2"));
        assert_eq!(view.outcome, Outcome::PartiallyFailed);
        assert_eq!((view.errors, view.warnings), (1, 3));
        assert_eq!(copy["spec"]["excludedResources"], json!(HELD_BACK));
        assert_eq!(
            copy["spec"]["resourceModifier"],
            json!({"kind": "ConfigMap", "name": "zeabur-1-2"})
        );

        let same = json_of(restore_object(
            "velero",
            "zeabur-1-2",
            "zeabur-1",
            "shop",
            None,
        ));
        assert_eq!(
            same["spec"],
            json!({"backupName": "zeabur-1", "includedNamespaces": ["shop"],
                   "excludedResources": HELD_BACK})
        );
        assert_eq!(restore(&object(same)).into, None);

        assert!(restore_problem(&[], "shop").is_none());
        assert!(restore_problem(&["shop".to_owned()], "shop").is_none());
        assert!(restore_problem(&["shop".to_owned()], "").is_some());
        assert!(restore_problem(&["shop".to_owned()], "blog").is_some());
        assert!(new_namespace_problem("shop", "shop-2").is_none());
        for bad in ["shop", "Shop-2", "-x", ""] {
            assert!(new_namespace_problem("shop", bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn a_copy_pauses_the_cron_jobs_of_the_original_namespace_by_the_documented_rule_format() {
        let modifier = json_of(paused_cron_jobs("velero", "zeabur-1-2", "shop"));
        assert_eq!(modifier["metadata"]["name"], "zeabur-1-2");
        let rules: Value =
            serde_saphyr::from_str(modifier["data"]["cronjobs.yaml"].as_str().unwrap()).unwrap();
        assert_eq!(
            rules,
            json!({"version": "v1", "resourceModifierRules": [{
                "conditions": {"groupResource": "cronjobs.batch", "namespaces": ["shop"]},
                "patches": [{"operation": "add", "path": "/spec/suspend", "value": "true"}]
            }]})
        );
        let restore = object(json!({
            "apiVersion": API_VERSION, "kind": "Restore",
            "metadata": {"name": "zeabur-1-2", "namespace": "velero", "uid": "u-1"}
        }));
        assert_eq!(
            owned_by(&restore).unwrap()[0]["value"][0],
            json!({"apiVersion": API_VERSION, "kind": "Restore", "name": "zeabur-1-2", "uid": "u-1"})
        );
    }

    #[test]
    fn domains_come_back_as_a_restore_of_only_the_routes() {
        let back = json_of(domains_restore(
            "velero",
            "zeabur-1-3",
            "zeabur-1",
            "shop",
            "shop-2",
        ));
        assert_eq!(
            back["spec"],
            json!({"backupName": "zeabur-1", "includedNamespaces": ["shop"],
                   "namespaceMapping": {"shop": "shop-2"}, "includedResources": ROUTES})
        );
        let view = restore(&object(back));
        assert_eq!(view.resources, ROUTES);
        let home = json_of(domains_restore(
            "velero",
            "zeabur-1-3",
            "zeabur-1",
            "shop",
            "shop",
        ));
        assert_eq!(home["spec"].get("namespaceMapping"), None);
    }

    #[test]
    fn the_inventory_reads_one_namespace_of_the_backup_list() {
        let list: BTreeMap<String, Vec<String>> = serde_json::from_value(json!({
            "v1/Service": ["shop/api", "shop/postgres", "blog/web"],
            "v1/Secret": ["shop/db-app", "shop/sh.helm.release.v1.x.v1"],
            "batch/v1/CronJob": ["shop/nightly"],
            "networking.k8s.io/v1/Ingress": ["shop/web"],
            "gateway.networking.k8s.io/v1/HTTPRoute": ["blog/web"],
            "v1/Namespace": ["shop"],
            "apps/v1/Deployment": ["shop/api"]
        }))
        .unwrap();
        assert_eq!(
            inventory(&list, "shop"),
            Inventory {
                services: vec!["api".to_owned(), "postgres".to_owned()],
                secrets: vec!["db-app".to_owned()],
                cron_jobs: vec!["nightly".to_owned()],
                routes: vec!["Ingress/web".to_owned()],
            }
        );
    }

    #[test]
    fn a_moved_route_keeps_its_spec_and_hosts_and_drops_what_the_server_set() {
        let ingress = json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
            "metadata": {"name": "web", "namespace": "shop", "uid": "u", "resourceVersion": "9",
                         "labels": {"app": "web"},
                         "annotations": {"cert-manager.io/cluster-issuer": "le",
                                         "kubectl.kubernetes.io/last-applied-configuration": "{}"},
                         "managedFields": []},
            "spec": {"rules": [{"host": "shop.example.com"}, {"host": "www.example.com"}]},
            "status": {"loadBalancer": {}}
        });
        let moved = json_of(moved_route(&ingress, "shop-2").unwrap());
        assert_eq!(
            moved,
            json!({
                "apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
                "metadata": {"name": "web", "namespace": "shop-2", "labels": {"app": "web"},
                             "annotations": {"cert-manager.io/cluster-issuer": "le"}},
                "spec": {"rules": [{"host": "shop.example.com"}, {"host": "www.example.com"}]}
            })
        );
        assert_eq!(
            route_hosts(&ingress).unwrap(),
            ["shop.example.com", "www.example.com"]
        );
        let route = json!({"spec": {"hostnames": ["b.example.com", "a.example.com"]}});
        assert_eq!(
            route_hosts(&route).unwrap(),
            ["a.example.com", "b.example.com"]
        );
    }

    #[test]
    fn ttls_in_whole_days_read_as_days() {
        assert_eq!(keep_days("168h0m0s"), Some(7));
        assert_eq!(keep_days("720h"), Some(30));
        assert_eq!(keep_days("36h0m0s"), None);
        assert_eq!(ttl(7), "168h0m0s");
    }
}
