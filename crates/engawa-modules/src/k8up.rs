//! K8up: file-level backups of a namespace's PersistentVolumeClaims with restic, from its
//! `Schedule` and `Backup` objects [k8up-api].
//!
//! # Which claims a backup covers
//!
//! K8up backs up the claims of the namespace the `Schedule` or `Backup` lives in [k8up-rwo]:
//!
//! - a claim annotated `k8up.io/backup: "false"` is skipped; without the annotation it is backed
//!   up, unless the operator runs with `BACKUP_SKIP_WITHOUT_ANNOTATION=true`, which Engawa cannot
//!   see [k8up-annotations];
//! - with `labelSelectors`, only claims matching one of them [k8up-api].
//!
//! # How a run went
//!
//! From the `Backup`'s conditions [k8up-status]: `Completed` with reason `Succeeded` or `Failed`;
//! `Progressing` while it runs. The finish time is the `Completed` condition's transition time.
//!
//! [k8up-api]: https://docs.k8up.io/k8up/2.13/references/api-reference.html
//! [k8up-rwo]: https://docs.k8up.io/k8up/2.13/explanations/rwo.html
//! [k8up-annotations]: https://docs.k8up.io/k8up/2.13/references/annotations.html
//! [k8up-status]: https://docs.k8up.io/k8up/2.13/references/status.html

use std::collections::BTreeMap;

use engawa_adapters::selector::matches;
use engawa_kube::DynamicObject;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;

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

/// The annotation that excludes a claim (`"false"`).
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
        .filter(|v| v.annotations.get(BACKUP_ANNOTATION).map(String::as_str) != Some("false"))
        .filter(|v| selectors.is_empty() || selectors.iter().any(|s| matches(s, &v.labels)))
        .map(|v| Target {
            kind: TargetKind::Volume,
            name: v.name.clone(),
        })
        .collect()
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
