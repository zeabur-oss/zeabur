//! Velero: backups of namespaces' Kubernetes objects, and of their volumes when snapshots or
//! file-system backup are set up, from its `Schedule` and `Backup` objects [velero-schedule]
//! [velero-backup].
//!
//! Velero's objects live in its own namespace (usually `velero`), so they are read cluster-wide
//! and kept when they cover the project's namespace: `includedNamespaces` empty or `*` means every
//! namespace, and `excludedNamespaces` takes some out [velero-backup].
//!
//! # Volumes
//!
//! A Velero backup covers the namespace's objects. Whether it also copies a claim's data depends
//! on volume snapshots (a snapshot location and a CSI driver or provider plugin) or on file-system
//! backup [velero-fsb]. Zeabur counts the project's claims as covered only when the schedule asks
//! for file-system backup of every volume (`defaultVolumesToFsBackup: true`); otherwise the
//! schedule covers the namespace and says that volume data depends on snapshots.
//!
//! # How a run went
//!
//! `status.phase` [velero-backup]: `Completed`; `PartiallyFailed`; `Failed` or
//! `FailedValidation`; the in-progress phases. A backup made by a schedule carries the label
//! `velero.io/schedule-name` [velero-labels].
//!
//! [velero-schedule]: https://velero.io/docs/v1.17/api-types/schedule/
//! [velero-backup]: https://velero.io/docs/v1.17/api-types/backup/
//! [velero-fsb]: https://velero.io/docs/v1.17/file-system-backup/
//! [velero-labels]: https://github.com/vmware-tanzu/velero/blob/v1.17.0/pkg/apis/velero/v1/labels_annotations.go

use zeabur_kube::DynamicObject;

use crate::backups::{Findings, Outcome, Run, Schedule, Target, TargetKind, newest};
use crate::{Module, Read, strings, text};

/// The module.
pub static MODULE: Module = Module {
    id: "velero",
    capability: "volume-backup",
    reads: &[SCHEDULES, BACKUPS],
    http: &[],
};

/// `velero.io/v1` `Schedule`, cluster-wide.
pub const SCHEDULES: Read = Read {
    group: "velero.io",
    version: "v1",
    resource: "schedules",
    kind: "Schedule",
};

/// `velero.io/v1` `Backup`, cluster-wide.
pub const BACKUPS: Read = Read {
    group: "velero.io",
    version: "v1",
    resource: "backups",
    kind: "Backup",
};

/// The note on a schedule whose volume data depends on snapshots.
pub const VOLUMES_NEED_SNAPSHOTS: &str = "backups.veleroVolumesNeedSnapshots";

/// Whether a backup spec at `pointer` covers `namespace`.
fn covers(object: &DynamicObject, pointer: &str, namespace: &str) -> bool {
    let included = strings(object, &format!("{pointer}/includedNamespaces"));
    let excluded = strings(object, &format!("{pointer}/excludedNamespaces"));
    let in_scope = included.is_empty() || included.iter().any(|n| n == "*" || n == namespace);
    in_scope && !excluded.iter().any(|n| n == namespace)
}

fn targets(
    object: &DynamicObject,
    pointer: &str,
    namespace: &str,
    volumes: &[String],
) -> Vec<Target> {
    let mut out = vec![Target {
        kind: TargetKind::Namespace,
        name: namespace.to_owned(),
    }];
    let fs_backup = object
        .data
        .pointer(&format!("{pointer}/defaultVolumesToFsBackup"))
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    if fs_backup {
        out.extend(volumes.iter().map(|v| Target {
            kind: TargetKind::Volume,
            name: v.clone(),
        }));
    }
    out
}

/// What Velero's objects say about `namespace` and its claims (`volumes`). Runs are the newest
/// ten.
#[must_use]
pub fn findings(
    namespace: &str,
    schedules: &[DynamicObject],
    backups: &[DynamicObject],
    volumes: &[String],
) -> Findings {
    let mut out = Findings::default();
    for schedule in schedules {
        if !covers(schedule, "/spec/template", namespace) {
            continue;
        }
        let Some(cron) = text(schedule, "/spec/schedule") else {
            continue;
        };
        let targets = targets(schedule, "/spec/template", namespace, volumes);
        let fs_backup = targets.iter().any(|t| t.kind == TargetKind::Volume);
        out.schedules.push(Schedule {
            name: format!(
                "{}/{}",
                schedule.metadata.namespace.as_deref().unwrap_or_default(),
                schedule.metadata.name.as_deref().unwrap_or_default()
            ),
            provider: MODULE.id.to_owned(),
            cron,
            paused: schedule
                .data
                .pointer("/spec/paused")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            targets,
            note: (!fs_backup && !volumes.is_empty()).then(|| VOLUMES_NEED_SNAPSHOTS.to_owned()),
        });
    }
    out.schedules.sort_by(|a, b| a.name.cmp(&b.name));
    for backup in backups.iter().filter(|b| covers(b, "/spec", namespace)) {
        let outcome = match text(backup, "/status/phase").as_deref() {
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
        let errors = backup
            .data
            .pointer("/status/errors")
            .and_then(serde_json::Value::as_u64)
            .filter(|n| *n > 0);
        out.runs.push(Run {
            name: backup.metadata.name.clone().unwrap_or_default(),
            provider: MODULE.id.to_owned(),
            targets: targets(backup, "/spec", namespace, volumes),
            outcome,
            started: text(backup, "/status/startTimestamp"),
            finished: text(backup, "/status/completionTimestamp"),
            message: errors.map(|n| format!("{n} errors")),
        });
    }
    newest(&mut out.runs, 10);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(value: serde_json::Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_schedule_of_every_namespace_covers_the_namespace_but_not_its_volumes() {
        let schedule = object(serde_json::json!({
            "apiVersion": "velero.io/v1", "kind": "Schedule",
            "metadata": {"name": "daily", "namespace": "velero"},
            "spec": {"schedule": "0 3 * * *", "template": {"includedNamespaces": ["*"]}}
        }));
        let found = findings("shop", &[schedule], &[], &["data".to_owned()]);
        let s = &found.schedules[0];
        assert_eq!(s.name, "velero/daily");
        assert_eq!(s.targets.len(), 1);
        assert_eq!(s.note.as_deref(), Some(VOLUMES_NEED_SNAPSHOTS));
    }

    #[test]
    fn excluded_namespaces_and_fs_backup_are_honoured() {
        let excluded = object(serde_json::json!({
            "apiVersion": "velero.io/v1", "kind": "Schedule",
            "metadata": {"name": "others", "namespace": "velero"},
            "spec": {"schedule": "0 3 * * *", "template": {"excludedNamespaces": ["shop"]}}
        }));
        let fs = object(serde_json::json!({
            "apiVersion": "velero.io/v1", "kind": "Schedule",
            "metadata": {"name": "shop-fs", "namespace": "velero"},
            "spec": {"schedule": "0 4 * * *", "template": {
                "includedNamespaces": ["shop"], "defaultVolumesToFsBackup": true}}
        }));
        let found = findings("shop", &[excluded, fs], &[], &["data".to_owned()]);
        assert_eq!(found.schedules.len(), 1);
        assert!(found.schedules[0].targets.iter().any(|t| t.name == "data"));
    }

    #[test]
    fn a_partially_failed_backup_reports_its_errors() {
        let backup = object(serde_json::json!({
            "apiVersion": "velero.io/v1", "kind": "Backup",
            "metadata": {"name": "daily-1", "namespace": "velero"},
            "spec": {"includedNamespaces": ["shop"]},
            "status": {"phase": "PartiallyFailed", "errors": 2,
                       "startTimestamp": "2026-10-14T03:00:00Z", "completionTimestamp": "2026-10-14T03:02:00Z"}
        }));
        let found = findings("shop", &[], &[backup], &[]);
        assert_eq!(found.runs[0].outcome, Outcome::PartiallyFailed);
        assert_eq!(found.runs[0].message.as_deref(), Some("2 errors"));
    }
}
