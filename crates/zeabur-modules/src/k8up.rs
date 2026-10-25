//! K8up: file-level backups of a namespace's PersistentVolumeClaims with restic, from its
//! `Schedule` and `Backup` objects [k8up-api].
//!
//! # Which claims a backup covers
//!
//! K8up backs up the claims of the namespace the `Schedule` or `Backup` lives in [k8up-rwo]:
//!
//! - only a claim annotated `k8up.io/backup: "true"`, as the catalogue installs K8up with
//!   `skipWithoutAnnotation: true` [k8up-annotations]. The operator's setting is in its own
//!   namespace, which a project's members cannot read;
//! - with `labelSelectors`, only claims matching one of them [k8up-api].
//!
//! # How a run went
//!
//! From the `Backup`'s conditions [k8up-status]: `Completed` with reason `Succeeded` or `Failed`;
//! `Progressing` while it runs. The finish time is the `Completed` condition's transition time.
//!
//! # Restoring a volume
//!
//! K8up lists the restic snapshots of each repository it backs up to as `Snapshot` objects in
//! the namespace: the restic ID, the time, the paths and the repository [k8up-api]. A claim's
//! files are at `/data/<claim>` [k8up-rwo], and only snapshots taken with the namespace as the
//! restic host are listed [k8up-snapshots].
//!
//! A `Restore` runs one snapshot with the backup's backend [k8up-restore], into a claim
//! (`restoreMethod.folder.claimName`). The snapshot's path is trimmed, so the files land at the
//! claim's root. restic keeps files the snapshot does not have unless the `Restore` sets
//! `delete: true`, which passes `--delete` [restic-restore]. K8up 2.16 does not check restic's exit
//! status here, so a restore that failed can still report `Completed` [k8up-folder-restore]; the
//! files must be checked.
//!
//! [k8up-api]: https://docs.k8up.io/k8up/2.13/references/api-reference.html
//! [k8up-snapshots]: https://github.com/k8up-io/k8up/blob/v2.16.1/restic/kubernetes/snapshots.go
//! [k8up-restore]: https://docs.k8up.io/k8up/2.16/how-tos/restore.html
//! [restic-restore]: https://restic.readthedocs.io/en/stable/050_restore.html
//! [k8up-folder-restore]: https://github.com/k8up-io/k8up/blob/v2.16.1/restic/cli/restore.go#L196
//! [k8up-rwo]: https://docs.k8up.io/k8up/2.13/explanations/rwo.html
//! [k8up-annotations]: https://docs.k8up.io/k8up/2.13/references/annotations.html
//! [k8up-status]: https://docs.k8up.io/k8up/2.13/references/status.html

use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use serde_json::{Value, json};
use zeabur_adapters::selector::matches;
use zeabur_kube::DynamicObject;

use crate::backups::{Findings, Outcome, Run, Schedule, Target, TargetKind, condition, newest};
use crate::{Module, Read, text};

/// The module.
pub static MODULE: Module = Module {
    id: "k8up",
    capability: "volume-backup",
    reads: &[SCHEDULES, BACKUPS],
    http: &[],
};

/// `k8up.io/v1` `Schedule`, in the project's namespace.
pub const SCHEDULES: Read = Read {
    group: "k8up.io",
    version: "v1",
    resource: "schedules",
    kind: "Schedule",
};

/// `k8up.io/v1` `Backup`, in the project's namespace.
pub const BACKUPS: Read = Read {
    group: "k8up.io",
    version: "v1",
    resource: "backups",
    kind: "Backup",
};

/// `k8up.io/v1` `Snapshot`, in the project's namespace.
pub const SNAPSHOTS: Read = Read {
    group: "k8up.io",
    version: "v1",
    resource: "snapshots",
    kind: "Snapshot",
};

/// `k8up.io/v1` `Restore`, in the project's namespace.
pub const RESTORES: Read = Read {
    group: "k8up.io",
    version: "v1",
    resource: "restores",
    kind: "Restore",
};

/// The label a volume's `Schedule` selects its claim by, and that Zeabur puts on each `Restore`
/// of the claim's backups.
pub const VOLUME_LABEL: &str = "zeabur.com/backup-volume";

/// The annotation that selects a claim (`"true"`).
pub const BACKUP_ANNOTATION: &str = "k8up.io/backup";

/// A claim of the project, as K8up's selection reads it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Volume {
    /// Its name.
    pub name: String,
    /// Its labels.
    pub labels: BTreeMap<String, String>,
    /// Its annotations.
    pub annotations: BTreeMap<String, String>,
}

/// The claims a backup spec at `pointer` covers.
fn covered(object: &DynamicObject, pointer: &str, volumes: &[Volume]) -> Vec<Target> {
    let selectors: Vec<LabelSelector> = object
        .data
        .pointer(&format!("{pointer}/labelSelectors"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    volumes
        .iter()
        .filter(|v| v.annotations.get(BACKUP_ANNOTATION).map(String::as_str) == Some("true"))
        .filter(|v| selectors.is_empty() || selectors.iter().any(|s| matches(s, &v.labels)))
        .map(|v| Target {
            kind: TargetKind::Volume,
            name: v.name.clone(),
        })
        .collect()
}

/// The S3 repository a `Schedule` writes to, as restic names it: `s3:<endpoint>/<bucket>`.
fn destination(schedule: &DynamicObject) -> Option<String> {
    let backend = backend(schedule)?;
    let endpoint = backend.pointer("/s3/endpoint")?.as_str()?;
    let bucket = backend.pointer("/s3/bucket")?.as_str()?;
    Some(format!("s3:{}/{bucket}", endpoint.trim_end_matches('/')))
}

/// The backend a `Schedule`'s backups use: the backup's own, else the schedule's.
fn backend(schedule: &DynamicObject) -> Option<&Value> {
    schedule
        .data
        .pointer("/spec/backup/backend")
        .or_else(|| schedule.data.pointer("/spec/backend"))
}

/// The prune's `keepLast`, `keepDaily`, … as `last`, `daily`, ….
fn retention(schedule: &DynamicObject) -> BTreeMap<String, i64> {
    let Some(Value::Object(retention)) = schedule.data.pointer("/spec/prune/retention") else {
        return BTreeMap::new();
    };
    retention
        .iter()
        .filter_map(|(key, value)| {
            let period = key.strip_prefix("keep")?;
            let count = value.as_i64()?;
            let mut chars = period.chars();
            let first = chars.next()?.to_ascii_lowercase();
            Some((format!("{first}{}", chars.as_str()), count))
        })
        .collect()
}

/// A `Backup` that runs a `Schedule`'s backup once: its backup spec without the timing, with the
/// schedule-wide backend, Pod security context and Pod config when the backup has none.
fn run_now(schedule: &DynamicObject) -> Value {
    let mut spec = match schedule.data.pointer("/spec/backup") {
        Some(Value::Object(backup)) => backup.clone(),
        _ => serde_json::Map::new(),
    };
    spec.remove("schedule");
    spec.remove("concurrentRunsAllowed");
    for field in ["backend", "podSecurityContext", "podConfigRef"] {
        if let Some(value) = schedule.data.pointer(&format!("/spec/{field}"))
            && !spec.contains_key(field)
        {
            spec.insert(field.to_owned(), value.clone());
        }
    }
    json!({"apiVersion": "k8up.io/v1", "kind": "Backup", "spec": spec})
}

/// What K8up's objects of one namespace say about its claims. Runs are the newest ten.
#[must_use]
pub fn findings(
    namespace: &str,
    schedules: &[DynamicObject],
    backups: &[DynamicObject],
    volumes: &[Volume],
) -> Findings {
    let mut out = Findings::default();
    for schedule in schedules {
        let Some(cron) = text(schedule, "/spec/backup/schedule") else {
            continue;
        };
        out.schedules.push(Schedule {
            name: format!(
                "{namespace}/{}",
                schedule.metadata.name.as_deref().unwrap_or_default()
            ),
            provider: MODULE.id.to_owned(),
            cron,
            paused: false,
            targets: covered(schedule, "/spec/backup", volumes),
            note: None,
            destination: destination(schedule),
            retention: retention(schedule),
            run_now: Some(run_now(schedule)),
        });
    }
    out.schedules.sort_by(|a, b| a.name.cmp(&b.name));
    for backup in backups {
        let completed = condition(backup, "Completed");
        let outcome = match completed.as_ref().and_then(|c| c.1.as_deref()) {
            Some("Succeeded") => Outcome::Succeeded,
            Some("Failed") => Outcome::Failed,
            _ if condition(backup, "Progressing").is_some() => Outcome::Running,
            _ if completed.is_none() && condition(backup, "Ready").is_some() => Outcome::Running,
            _ => Outcome::Unknown,
        };
        out.runs.push(Run {
            name: backup.metadata.name.clone().unwrap_or_default(),
            provider: MODULE.id.to_owned(),
            targets: covered(backup, "/spec", volumes),
            outcome,
            started: backup
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.to_string()),
            finished: completed.as_ref().and_then(|c| c.0.clone()),
            message: completed
                .filter(|_| outcome == Outcome::Failed)
                .and_then(|c| c.2),
            restore_from: None,
        });
    }
    newest(&mut out.runs, 10);
    out
}

/// A snapshot of a claim that can be restored.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = VolumeSnapshot)]
pub struct SnapshotView {
    /// restic's ID.
    pub id: String,
    /// When it was taken, RFC 3339.
    pub date: String,
}

/// A `Restore` of a claim's backups.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = VolumeRestore)]
pub struct RestoreView {
    /// Its name.
    pub name: String,
    /// The snapshot.
    pub snapshot: String,
    /// The claim it restores into; absent for a restore to anywhere else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub into: Option<String>,
    /// It deletes what the snapshot does not have.
    pub delete: bool,
    /// How it went, as K8up reports it.
    pub outcome: Outcome,
    /// When it was created, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    /// When it finished, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<String>,
    /// Why it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The replicas to start the workload again with, which Zeabur notes on a restore into the
    /// claim itself ([`REPLICAS`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicas: Option<i32>,
}

/// The annotation Zeabur puts on a restore into the claim itself: the replicas the workload had.
pub const REPLICAS: &str = "zeabur.com/replicas";

/// The `Schedule` made for `claim` alone, else one whose backups select the claim's label.
#[must_use]
pub fn schedule_of<'a>(schedules: &'a [DynamicObject], claim: &str) -> Option<&'a DynamicObject> {
    let own = format!("{claim}-backup");
    schedules
        .iter()
        .find(|s| s.metadata.name.as_deref() == Some(own.as_str()))
        .or_else(|| {
            schedules.iter().find(|s| {
                s.data
                    .pointer("/spec/backup/labelSelectors")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|selector| {
                        selector
                            .get("matchLabels")
                            .and_then(|l| l.get(VOLUME_LABEL))
                            .and_then(Value::as_str)
                            == Some(claim)
                    })
            })
        })
}

/// The snapshots of `claim` in the repository of `schedule`, newest first.
#[must_use]
pub fn snapshots(
    objects: &[DynamicObject],
    schedule: &DynamicObject,
    claim: &str,
) -> Vec<SnapshotView> {
    let Some(repository) = destination(schedule) else {
        return Vec::new();
    };
    let path = format!("/data/{claim}");
    let mut out: Vec<SnapshotView> = objects
        .iter()
        .filter(|o| text(o, "/spec/repository").as_deref() == Some(repository.as_str()))
        .filter(|o| {
            o.data
                .pointer("/spec/paths")
                .and_then(Value::as_array)
                .is_some_and(|p| p.iter().any(|p| p == path.as_str()))
        })
        .filter_map(|o| {
            Some(SnapshotView {
                id: text(o, "/spec/id")?,
                date: text(o, "/spec/date")?,
            })
        })
        .collect();
    out.sort_by(|a, b| b.date.cmp(&a.date));
    out
}

/// The `Restore`s of `claim`'s backups, by [`VOLUME_LABEL`], newest first.
#[must_use]
pub fn restores(objects: &[DynamicObject], claim: &str) -> Vec<RestoreView> {
    let mut out: Vec<RestoreView> = objects
        .iter()
        .filter(|o| {
            o.metadata
                .labels
                .as_ref()
                .and_then(|l| l.get(VOLUME_LABEL))
                .map(String::as_str)
                == Some(claim)
        })
        .map(|o| {
            let completed = condition(o, "Completed");
            let outcome = match completed.as_ref().and_then(|c| c.1.as_deref()) {
                Some("Succeeded") => Outcome::Succeeded,
                Some("Failed") => Outcome::Failed,
                _ => Outcome::Running,
            };
            RestoreView {
                name: o.metadata.name.clone().unwrap_or_default(),
                snapshot: text(o, "/spec/snapshot").unwrap_or_default(),
                into: text(o, "/spec/restoreMethod/folder/claimName"),
                delete: o.data.pointer("/spec/delete").and_then(Value::as_bool) == Some(true),
                outcome,
                started: o
                    .metadata
                    .creation_timestamp
                    .as_ref()
                    .map(|t| t.0.to_string()),
                finished: completed.as_ref().and_then(|c| c.0.clone()),
                message: completed
                    .filter(|_| outcome == Outcome::Failed)
                    .and_then(|c| c.2),
                replicas: o
                    .metadata
                    .annotations
                    .as_ref()
                    .and_then(|a| a.get(REPLICAS))
                    .and_then(|r| r.parse().ok()),
            }
        })
        .collect();
    out.sort_by(|a, b| b.started.cmp(&a.started));
    out
}

/// A `Restore` with `schedule`'s backend and Pod security, labelled for `claim`, for the
/// snapshot and the method to be added.
#[must_use]
pub fn restore_template(schedule: &DynamicObject, claim: &str) -> Value {
    let mut spec = serde_json::Map::new();
    if let Some(backend) = backend(schedule) {
        spec.insert("backend".to_owned(), backend.clone());
    }
    for field in ["podSecurityContext", "podConfigRef"] {
        if let Some(value) = schedule
            .data
            .pointer(&format!("/spec/backup/{field}"))
            .or_else(|| schedule.data.pointer(&format!("/spec/{field}")))
        {
            spec.insert(field.to_owned(), value.clone());
        }
    }
    json!({
        "apiVersion": "k8up.io/v1", "kind": "Restore",
        "metadata": {"labels": {VOLUME_LABEL: claim}},
        "spec": spec,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_schedule_names_its_repository_retention_and_a_backup_that_runs_it_once() {
        let schedule = object(serde_json::json!({
            "apiVersion": "k8up.io/v1", "kind": "Schedule",
            "metadata": {"name": "uploads-backup", "namespace": "shop"},
            "spec": {
                "backend": {
                    "repoPasswordSecretRef": {"name": "uploads-backup", "key": "RESTIC_PASSWORD"},
                    "s3": {"endpoint": "https://s3.test/", "bucket": "b/shop/uploads"}
                },
                "podSecurityContext": {"runAsUser": 0},
                "backup": {
                    "schedule": "0 3 * * *",
                    "concurrentRunsAllowed": false,
                    "labelSelectors": [{"matchLabels": {"zeabur.com/backup-volume": "uploads"}}]
                },
                "prune": {"schedule": "0 4 * * *",
                          "retention": {"keepLast": 5, "keepDaily": 7, "tags": ["x"]}}
            }
        }));
        let found = findings("shop", &[schedule], &[], &[]);
        let schedule = &found.schedules[0];
        assert_eq!(
            schedule.destination.as_deref(),
            Some("s3:https://s3.test/b/shop/uploads")
        );
        assert_eq!(
            schedule.retention,
            BTreeMap::from([("daily".to_owned(), 7), ("last".to_owned(), 5)])
        );
        assert_eq!(
            schedule.run_now,
            Some(serde_json::json!({
                "apiVersion": "k8up.io/v1", "kind": "Backup",
                "spec": {
                    "labelSelectors": [{"matchLabels": {"zeabur.com/backup-volume": "uploads"}}],
                    "backend": {
                        "repoPasswordSecretRef": {"name": "uploads-backup", "key": "RESTIC_PASSWORD"},
                        "s3": {"endpoint": "https://s3.test/", "bucket": "b/shop/uploads"}
                    },
                    "podSecurityContext": {"runAsUser": 0}
                }
            }))
        );
    }

    #[test]
    fn a_failed_backup_keeps_its_message() {
        let backup = object(serde_json::json!({
            "apiVersion": "k8up.io/v1", "kind": "Backup",
            "metadata": {"name": "nightly-x", "namespace": "shop", "creationTimestamp": "2026-10-14T02:00:00Z"},
            "spec": {},
            "status": {"conditions": [
                {"type": "Completed", "status": "True", "reason": "Failed",
                 "message": "repository locked", "lastTransitionTime": "2026-10-14T02:01:00Z"}
            ]}
        }));
        let found = findings("shop", &[], &[backup], &[]);
        assert_eq!(found.runs[0].outcome, Outcome::Failed);
        assert_eq!(found.runs[0].message.as_deref(), Some("repository locked"));
        assert_eq!(
            found.runs[0].finished.as_deref(),
            Some("2026-10-14T02:01:00Z")
        );
    }
}
