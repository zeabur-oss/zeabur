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

use std::collections::{BTreeMap, BTreeSet};

use k8s_openapi::api::apps::v1::StatefulSet;
use k8s_openapi::api::core::v1::PersistentVolumeClaim;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_model::adapters::{controller_of, created_by_claim_template};
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

/// The claims an operator manages, which get no volume backup advice.
///
/// These are a claim whose controller is an operator's object (CloudNativePG's `Cluster` owns its
/// claims), and a claim of the `volumeClaimTemplates` of a StatefulSet that an operator's object
/// controls (Percona, MOCO, mariadb-operator and Dragonfly run StatefulSets). A StatefulSet's claim
/// does not name the StatefulSet's owner, so it is found by its name `<template>-<set>-<ordinal>`.
///
/// A Pod or a StatefulSet as the controller is Kubernetes' own, not an operator's.
#[must_use]
pub fn operated_claims(
    claims: &[PersistentVolumeClaim],
    stateful_sets: &[StatefulSet],
) -> BTreeSet<String> {
    let mut operated = BTreeSet::new();
    for claim in claims {
        if let Some(name) = &claim.metadata.name
            && controller_of(&claim.metadata).is_some_and(|owner| !kubernetes_owner(owner))
        {
            operated.insert(name.clone());
        }
    }
    for set in stateful_sets {
        if controller_of(&set.metadata).is_none_or(kubernetes_owner) {
            continue;
        }
        let set_name = set.metadata.name.as_deref().unwrap_or_default();
        let templates = set
            .spec
            .iter()
            .flat_map(|spec| spec.volume_claim_templates.iter().flatten());
        for template in templates {
            let template_name = template.metadata.name.as_deref().unwrap_or_default();
            for claim in claims {
                if let Some(name) = &claim.metadata.name
                    && created_by_claim_template(name, template_name, set_name)
                {
                    operated.insert(name.clone());
                }
            }
        }
    }
    operated
}

fn kubernetes_owner(owner: &OwnerReference) -> bool {
    (owner.api_version == "v1" && owner.kind == "Pod")
        || (owner.api_version == "apps/v1" && owner.kind == "StatefulSet")
}

/// The rules `volume-without-backup` and `database-without-backup`: claims and databases no
/// active schedule covers ([`unscheduled`]).
///
/// A database is a warning: it is the data a project cannot rebuild, and its operator backs it
/// up natively (CloudNativePG's `ScheduledBackup` [cnpg-backup]). A claim is advice: many hold
/// caches or data that can be rebuilt. A claim in `operated` ([`operated_claims`]) has none: its
/// operator knows how to back up its data, and a copy of each claim alone can be inconsistent. Each
/// fix names a form that asks for the bucket and ends in a plan: the database's backups
/// (`backups.scheduledBackup`); for a claim, its volume's K8up backups when K8up is installed
/// (`backups.k8upSchedule`) [k8up-schedule], else the cluster's Velero schedule when Velero is
/// (`backups.veleroSchedule`) [velero-fs], else installing K8up.
///
/// [cnpg-backup]: https://cloudnative-pg.io/documentation/current/backup/
/// [k8up-schedule]: https://docs.k8up.io/k8up/references/schedule-specification.html
/// [velero-fs]: https://velero.io/docs/main/file-system-backup/
#[must_use]
pub fn gaps(
    coverage: &[Coverage],
    installed: Installed,
    operated: &BTreeSet<String>,
) -> Vec<crate::Finding> {
    unscheduled(coverage)
        .into_iter()
        .filter(|c| !(c.target.kind == TargetKind::Volume && operated.contains(&c.target.name)))
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
                        fix: form_fix("backups.scheduledBackup", "database", name),
                        citations: vec![cite(
                            "CloudNativePG: Backup",
                            "https://cloudnative-pg.io/documentation/current/backup/",
                        )],
                    },
                }
            } else {
                let fix = volume_fix(name, installed);
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

/// A fix that opens the form named `id`, which plans the change.
fn form_fix(id: &str, param: &str, value: &str) -> Fix {
    Fix {
        kind: FixKind::Plan,
        id: id.to_owned(),
        params: BTreeMap::from([(param.to_owned(), value.to_owned())]),
    }
}

/// A claim's fix: its volume's K8up backups, the cluster's Velero schedule, or installing K8up.
fn volume_fix(claim: &str, installed: Installed) -> Fix {
    if installed.k8up {
        form_fix("backups.k8upSchedule", "claim", claim)
    } else if installed.velero {
        form_fix("backups.veleroSchedule", "claim", claim)
    } else {
        Fix {
            kind: FixKind::Install,
            id: "volume-backup/k8up".to_owned(),
            params: BTreeMap::new(),
        }
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
