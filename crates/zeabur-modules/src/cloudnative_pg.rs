//! CloudNativePG: PostgreSQL base backups the operator takes for each `Cluster`, from its
//! `ScheduledBackup` and `Backup` objects [cnpg-api].
//!
//! A `ScheduledBackup` names its cluster (`spec.cluster.name`) and a six-field cron expression
//! whose first field is seconds, unlike a Kubernetes CronJob's [cnpg-api]; `spec.suspend` pauses
//! it. A `Backup` names its cluster too, and reports `status.phase` [cnpg-phases] with
//! `startedAt`, `stoppedAt` and `error`.
//!
//! CloudNativePG's pods belong to its `Cluster`, not to a Deployment or StatefulSet, so a database
//! is a target of its own here, named after the `Cluster`.
//!
//! [cnpg-api]: https://cloudnative-pg.io/docs/devel/cloudnative-pg.v1
//! [cnpg-phases]: https://github.com/cloudnative-pg/cloudnative-pg/blob/main/api/v1/backup_types.go

use zeabur_kube::DynamicObject;

use crate::backups::{Findings, Outcome, Run, Schedule, Target, TargetKind, newest};
use crate::{Module, Read, text};

/// The module.
pub static MODULE: Module = Module {
    id: "cloudnative-pg",
    capability: "postgres",
    reads: &[CLUSTERS, SCHEDULED_BACKUPS, BACKUPS],
    http: &[],
};

/// `postgresql.cnpg.io/v1` `Cluster`, in the project's namespace.
pub const CLUSTERS: Read = Read {
    group: "postgresql.cnpg.io",
    version: "v1",
    resource: "clusters",
    kind: "Cluster",
};

/// `postgresql.cnpg.io/v1` `ScheduledBackup`, in the project's namespace.
pub const SCHEDULED_BACKUPS: Read = Read {
    group: "postgresql.cnpg.io",
    version: "v1",
    resource: "scheduledbackups",
    kind: "ScheduledBackup",
};

/// `postgresql.cnpg.io/v1` `Backup`, in the project's namespace.
pub const BACKUPS: Read = Read {
    group: "postgresql.cnpg.io",
    version: "v1",
    resource: "backups",
    kind: "Backup",
};

fn database(object: &DynamicObject) -> Option<Target> {
    text(object, "/spec/cluster/name").map(|name| Target {
        kind: TargetKind::Database,
        name,
    })
}

/// The databases (one per `Cluster`), sorted by name.
#[must_use]
pub fn databases(clusters: &[DynamicObject]) -> Vec<Target> {
    let mut out: Vec<Target> = clusters
        .iter()
        .filter_map(|c| c.metadata.name.clone())
        .map(|name| Target {
            kind: TargetKind::Database,
            name,
        })
        .collect();
    out.sort();
    out
}

/// What CloudNativePG's objects of one namespace say about its databases. Runs are the newest
/// ten.
#[must_use]
pub fn findings(
    namespace: &str,
    scheduled: &[DynamicObject],
    backups: &[DynamicObject],
) -> Findings {
    let mut out = Findings::default();
    for schedule in scheduled {
        let (Some(cron), Some(target)) = (text(schedule, "/spec/schedule"), database(schedule))
        else {
            continue;
        };
        out.schedules.push(Schedule {
            name: format!(
                "{namespace}/{}",
                schedule.metadata.name.as_deref().unwrap_or_default()
            ),
            provider: MODULE.id.to_owned(),
            cron,
            paused: schedule
                .data
                .pointer("/spec/suspend")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            targets: vec![target],
            note: None,
        });
    }
    out.schedules.sort_by(|a, b| a.name.cmp(&b.name));
    for backup in backups {
        let Some(target) = database(backup) else {
            continue;
        };
        let outcome = match text(backup, "/status/phase").as_deref() {
            Some("completed") => Outcome::Succeeded,
            Some("failed" | "walArchivingFailing" | "invalid backup definition") => Outcome::Failed,
            Some("pending" | "started" | "running" | "finalizing") => Outcome::Running,
            _ => Outcome::Unknown,
        };
        out.runs.push(Run {
            name: backup.metadata.name.clone().unwrap_or_default(),
            provider: MODULE.id.to_owned(),
            targets: vec![target],
            outcome,
            started: text(backup, "/status/startedAt"),
            finished: text(backup, "/status/stoppedAt"),
            message: text(backup, "/status/error"),
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
    fn a_suspended_schedule_and_a_completed_backup() {
        let schedule = object(serde_json::json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "ScheduledBackup",
            "metadata": {"name": "daily", "namespace": "shop"},
            "spec": {"schedule": "0 0 2 * * *", "cluster": {"name": "pg"}, "suspend": true}
        }));
        let backup = object(serde_json::json!({
            "apiVersion": "postgresql.cnpg.io/v1", "kind": "Backup",
            "metadata": {"name": "daily-1", "namespace": "shop"},
            "spec": {"cluster": {"name": "pg"}},
            "status": {"phase": "completed", "startedAt": "2026-10-14T02:00:00Z", "stoppedAt": "2026-10-14T02:05:00Z"}
        }));
        let found = findings("shop", &[schedule], &[backup]);
        assert!(found.schedules[0].paused);
        assert_eq!(found.schedules[0].targets[0].name, "pg");
        assert_eq!(found.runs[0].outcome, Outcome::Succeeded);
    }
}
