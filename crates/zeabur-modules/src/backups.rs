//! The backups capability: which of a project's volumes, databases and namespace a backup
//! schedule covers, and how the latest backups went, whoever takes them.
//!
//! Each provider ([`crate::k8up`], [`crate::velero`], [`crate::cloudnative_pg`]) turns its own
//! objects into the shared model below; this module combines them per target. The model is
//! read-only: what is shared across providers is the list, the schedule and the outcome. How a
//! backup is restored differs too much between them (file-level restic snapshots, whole-namespace
//! resource backups, PostgreSQL base backups with WAL) to share, and stays with each provider.
//!
//! Outcomes are reported as each provider states them; Zeabur does not test that a backup can be
//! restored.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_model::{Citation, Fix, FixKind, Gap, Message, Origin, Severity};

/// What a backup covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = BackupTargetKind)]
pub enum TargetKind {
    /// A PersistentVolumeClaim's files.
    Volume,
    /// A PostgreSQL (or other database) cluster an operator runs.
    Database,
    /// The namespace's Kubernetes objects.
    Namespace,
}

/// One thing a backup covers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = BackupTarget)]
pub struct Target {
    /// What it is.
    pub kind: TargetKind,
    /// Its name: the claim, the database cluster or the namespace.
    pub name: String,
}

/// How a backup run went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = BackupOutcome)]
pub enum Outcome {
    /// It finished and the provider reports success.
    Succeeded,
    /// It finished with some items failed.
    PartiallyFailed,
    /// It failed.
    Failed,
    /// It is running or waiting to run.
    Running,
    /// The provider's object says nothing Zeabur can read.
    Unknown,
}

/// One backup run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = BackupRun)]
pub struct Run {
    /// The provider's object name.
    pub name: String,
    /// The provider (module id).
    pub provider: String,
    /// What it covered.
    pub targets: Vec<Target>,
    /// How it went.
    pub outcome: Outcome,
    /// When it started, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    /// When it finished, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<String>,
    /// The provider's message, for a failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// What a restore of this backup names: CloudNativePG's backup ID, Percona's destination, or
    /// the time MOCO and mariadb-operator restore to. Absent when it cannot be restored yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_from: Option<String>,
}

impl Run {
    /// The time it is sorted by: finished, else started.
    #[must_use]
    pub fn time(&self) -> Option<&str> {
        self.finished.as_deref().or(self.started.as_deref())
    }
}

/// One backup schedule.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = BackupSchedule)]
pub struct Schedule {
    /// The provider's object, `<namespace>/<name>`.
    pub name: String,
    /// The provider (module id).
    pub provider: String,
    /// The cron expression, as the provider writes it (CloudNativePG's has a seconds field).
    pub cron: String,
    /// The schedule is paused or suspended.
    pub paused: bool,
    /// What it covers in the project.
    pub targets: Vec<Target>,
    /// What else to know about the coverage (an i18n key), such as `velero.volumesNeedSnapshots`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Where it writes, such as `s3:https://s3.example.com/backups/shop`, when the provider says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    /// How many backups it keeps, by period (`last`, `hourly`, `daily`, `weekly`, `monthly`,
    /// `yearly`), when the provider counts them.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub retention: BTreeMap<String, i64>,
    /// The object that runs it once, without `metadata`: for K8up, a `Backup` with the schedule's
    /// backup spec and backend.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub run_now: Option<Value>,
}

/// What one provider found for a project.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = BackupProviderFindings)]
pub struct Findings {
    /// Its schedules that cover something of the project.
    pub schedules: Vec<Schedule>,
    /// Its runs that covered something of the project, newest first.
    pub runs: Vec<Run>,
}

/// One target's coverage across providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = BackupCoverage)]
pub struct Coverage {
    /// The target.
    pub target: Target,
    /// The schedules covering it (`<provider>:<namespace>/<name>`), active ones only.
    pub scheduled_by: Vec<String>,
    /// Its latest run, by any provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run: Option<Run>,
}

/// A project's targets in their documented order: its claims, its databases, then its
/// namespace.
#[must_use]
pub fn targets(namespace: &str, volumes: &[String], databases: &[Target]) -> Vec<Target> {
    let mut claims: Vec<Target> = volumes
        .iter()
        .map(|name| Target {
            kind: TargetKind::Volume,
            name: name.clone(),
        })
        .collect();
    claims.sort();
    claims
        .into_iter()
        .chain(databases.iter().cloned())
        .chain([Target {
            kind: TargetKind::Namespace,
            name: namespace.to_owned(),
        }])
        .collect()
}

/// Combines the providers' findings into one coverage row per target, in the order given.
#[must_use]
pub fn coverage(targets: &[Target], findings: &[Findings]) -> Vec<Coverage> {
    targets
        .iter()
        .map(|target| {
            let scheduled_by = findings
                .iter()
                .flat_map(|f| &f.schedules)
                .filter(|s| !s.paused && s.targets.contains(target))
                .map(|s| format!("{}:{}", s.provider, s.name))
                .collect();
            let last_run = findings
                .iter()
                .flat_map(|f| &f.runs)
                .filter(|r| r.targets.contains(target))
                .max_by(|a, b| a.time().cmp(&b.time()))
                .cloned();
            Coverage {
                target: target.clone(),
                scheduled_by,
                last_run,
            }
        })
        .collect()
}

/// The gaps: volumes and databases no active schedule covers (rules `volume-without-backup` and
/// `database-without-backup`).
#[must_use]
pub fn unscheduled(coverage: &[Coverage]) -> Vec<&Coverage> {
    coverage
        .iter()
        .filter(|c| c.target.kind != TargetKind::Namespace && c.scheduled_by.is_empty())
        .collect()
}

/// Which backup providers could be read in the cluster, for choosing a gap's fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Installed {
    /// K8up's API is served and readable.
    pub k8up: bool,
    /// Velero's API is served and readable.
    pub velero: bool,
}

/// The id of the rule for volumes no schedule covers.
pub const VOLUME_WITHOUT_BACKUP: &str = "volume-without-backup";
/// The id of the rule for databases no schedule covers.
pub const DATABASE_WITHOUT_BACKUP: &str = "database-without-backup";

/// The rules `volume-without-backup` and `database-without-backup`: claims and databases no
/// active schedule covers ([`unscheduled`]).
///
/// A database is a warning: it is the data a project cannot rebuild, and its operator backs it
/// up natively (CloudNativePG's `ScheduledBackup` [cnpg-backup]). A claim is advice: many hold
/// caches or data that can be rebuilt. A claim's fix is a K8up `Schedule` for the namespace when
/// K8up is installed [k8up-schedule], else a Velero `Schedule` with file-system backup of its
/// volumes when Velero is [velero-fs], else installing K8up.
///
/// [cnpg-backup]: https://cloudnative-pg.io/documentation/current/backup/
/// [k8up-schedule]: https://docs.k8up.io/k8up/references/schedule-specification.html
/// [velero-fs]: https://velero.io/docs/main/file-system-backup/
#[must_use]
pub fn gaps(namespace: &str, coverage: &[Coverage], installed: Installed) -> Vec<crate::Finding> {
    unscheduled(coverage)
        .into_iter()
        .map(|c| {
            let name = c.target.name.as_str();
            if c.target.kind == TargetKind::Database {
                crate::Finding {
                    gap: Gap {
                        rule: DATABASE_WITHOUT_BACKUP.to_owned(),
                        severity: Severity::Warning,
                        subject: Origin::object("Cluster", name),
                        service: None,
                        message: Message::new("gap.databaseWithoutBackup").with("database", name),
                        fix: snippet_fix("backups.scheduledBackup", "database", name),
                        citations: vec![cite(
                            "CloudNativePG: Backup",
                            "https://cloudnative-pg.io/documentation/current/backup/",
                        )],
                    },
                    object: Some(scheduled_backup(namespace, name)),
                }
            } else {
                let (fix, object) = volume_fix(namespace, name, installed);
                crate::Finding {
                    gap: Gap {
                        rule: VOLUME_WITHOUT_BACKUP.to_owned(),
                        severity: Severity::Advice,
                        subject: Origin::object("PersistentVolumeClaim", name),
                        service: None,
                        message: Message::new("gap.volumeWithoutBackup").with("claim", name),
                        fix,
                        citations: vec![
                            cite(
                                "K8up: Schedule",
                                "https://docs.k8up.io/k8up/references/schedule-specification.html",
                            ),
                            cite(
                                "Velero: File System Backup",
                                "https://velero.io/docs/main/file-system-backup/",
                            ),
                        ],
                    },
                    object,
                }
            }
        })
        .collect()
}

fn cite(title: &str, url: &str) -> Citation {
    Citation {
        title: title.to_owned(),
        url: url.to_owned(),
    }
}

fn snippet_fix(id: &str, param: &str, value: &str) -> Fix {
    Fix {
        kind: FixKind::Snippet,
        id: id.to_owned(),
        params: BTreeMap::from([(param.to_owned(), value.to_owned())]),
    }
}

/// A CloudNativePG `ScheduledBackup` of `database`, daily at 03:00 (six cron fields, seconds
/// first), through the Barman Cloud plugin.
fn scheduled_backup(namespace: &str, database: &str) -> Value {
    json!({
        "apiVersion": "postgresql.cnpg.io/v1",
        "kind": "ScheduledBackup",
        "metadata": {"name": format!("{database}-daily"), "namespace": namespace},
        "spec": {
            "schedule": "0 0 3 * * *",
            "cluster": {"name": database},
            "method": "plugin",
            "pluginConfiguration": {"name": "barman-cloud.cloudnative-pg.io"},
        },
    })
}

/// A claim's fix: a K8up `Schedule` of the namespace, a Velero `Schedule` with file-system backup
/// of its volumes, or installing K8up. The repository and bucket are placeholders to fill in.
fn volume_fix(namespace: &str, claim: &str, installed: Installed) -> (Fix, Option<Value>) {
    if installed.k8up {
        let object = json!({
            "apiVersion": "k8up.io/v1",
            "kind": "Schedule",
            "metadata": {"name": "backups", "namespace": namespace},
            "spec": {
                "backend": {
                    "repoPasswordSecretRef": {"name": "backup-repo", "key": "password"},
                    "s3": {
                        "endpoint": "https://s3.example.test",
                        "bucket": format!("{namespace}-backups"),
                        "accessKeyIDSecretRef": {"name": "backup-s3", "key": "access-key-id"},
                        "secretAccessKeySecretRef": {"name": "backup-s3", "key": "secret-access-key"},
                    },
                },
                "backup": {"schedule": "@daily-random"},
                "prune": {
                    "schedule": "@weekly-random",
                    "retention": {"keepDaily": 7, "keepWeekly": 4},
                },
            },
        });
        (
            snippet_fix("backups.k8upSchedule", "claim", claim),
            Some(object),
        )
    } else if installed.velero {
        let object = json!({
            "apiVersion": "velero.io/v1",
            "kind": "Schedule",
            "metadata": {"name": format!("{namespace}-daily"), "namespace": "velero"},
            "spec": {
                "schedule": "0 3 * * *",
                "template": {
                    "includedNamespaces": [namespace],
                    "defaultVolumesToFsBackup": true,
                    "ttl": "168h0m0s",
                },
            },
        });
        (
            snippet_fix("backups.veleroSchedule", "claim", claim),
            Some(object),
        )
    } else {
        let fix = Fix {
            kind: FixKind::Install,
            id: "volume-backup/k8up".to_owned(),
            params: BTreeMap::new(),
        };
        (fix, None)
    }
}

/// Sorts runs newest first and keeps `limit` of them.
pub fn newest(runs: &mut Vec<Run>, limit: usize) {
    runs.sort_by(|a, b| b.time().cmp(&a.time()).then_with(|| a.name.cmp(&b.name)));
    runs.truncate(limit);
}

/// The `lastTransitionTime`, `reason` and `message` of the condition `kind` with status `True`.
pub(crate) fn condition(
    object: &zeabur_kube::DynamicObject,
    kind: &str,
) -> Option<(Option<String>, Option<String>, Option<String>)> {
    object
        .data
        .pointer("/status/conditions")?
        .as_array()?
        .iter()
        .find(|c| c["type"] == kind && c["status"] == "True")
        .map(|c| {
            let text = |key: &str| c[key].as_str().map(str::to_owned);
            (text("lastTransitionTime"), text("reason"), text("message"))
        })
}
