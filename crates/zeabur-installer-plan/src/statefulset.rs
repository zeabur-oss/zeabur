//! StatefulSets a Helm upgrade cannot update in place, and Helm releases stuck on them.
//!
//! kube-apiserver refuses an update to a StatefulSet's spec "for fields other than 'replicas',
//! 'ordinals', 'template', 'updateStrategy', 'revisionHistoryLimit',
//! 'persistentVolumeClaimRetentionPolicy' and 'minReadySeconds'". A chart upgrade that changes
//! another one fails, and helm-controller's `failurePolicy: abort` then keeps the release failed.
//! `apply` deletes such a StatefulSet with `--cascade=orphan`, so its Pods and
//! PersistentVolumeClaims stay, and Helm creates it again (zeabur-oss/zeabur#269).

use serde_json::{Value, json};

use crate::PlanError;
use crate::plan::{Warning, warning};

/// The spec fields of a StatefulSet that an update may not change, as `plan` names them.
pub const FIXED_FIELDS: [&str; 4] = [
    "volumeClaimTemplates",
    "selector",
    "serviceName",
    "podManagementPolicy",
];

/// A StatefulSet whose fixed fields differ between two renderings of a chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedChange {
    /// Its namespace.
    pub namespace: String,
    /// Its name.
    pub name: String,
    /// The fields that differ, in [`FIXED_FIELDS`] order.
    pub fields: Vec<&'static str>,
}

/// The StatefulSets of both manifests whose fixed fields differ; `namespace` is the release's,
/// for objects that name none.
///
/// # Errors
///
/// When a manifest is not YAML.
pub fn fixed_changes(namespace: &str, from: &str, to: &str) -> Result<Vec<FixedChange>, PlanError> {
    let before = stateful_sets(namespace, from)?;
    let mut changes = Vec::new();
    for (key, after) in stateful_sets(namespace, to)? {
        let Some((_, before)) = before.iter().find(|(k, _)| *k == key) else {
            continue;
        };
        let fields: Vec<&'static str> = FIXED_FIELDS
            .into_iter()
            .filter(|field| fixed(before, field) != fixed(&after, field))
            .collect();
        if !fields.is_empty() {
            changes.push(FixedChange {
                namespace: key.0,
                name: key.1,
                fields,
            });
        }
    }
    Ok(changes)
}

/// A StatefulSet by namespace and name.
type Keyed = ((String, String), Value);

fn stateful_sets(namespace: &str, manifest: &str) -> Result<Vec<Keyed>, PlanError> {
    let documents: Vec<Value> =
        serde_saphyr::from_multiple(manifest).map_err(|e| PlanError::Values {
            what: "a Helm release's manifest".to_owned(),
            reason: e.to_string(),
        })?;
    Ok(documents
        .into_iter()
        .filter(|d| d["kind"] == "StatefulSet")
        .filter_map(|d| {
            let name = d.pointer("/metadata/name")?.as_str()?.to_owned();
            let namespace = d
                .pointer("/metadata/namespace")
                .and_then(Value::as_str)
                .unwrap_or(namespace)
                .to_owned();
            Some(((namespace, name), d))
        })
        .collect())
}

/// The field as kube-apiserver compares it, with its default when the manifest leaves it out.
fn fixed(stateful_set: &Value, field: &str) -> Value {
    let value = stateful_set
        .pointer(&format!("/spec/{field}"))
        .filter(|v| !v.is_null())
        .cloned();
    value.unwrap_or_else(|| match field {
        "volumeClaimTemplates" => json!([]),
        "serviceName" => json!(""),
        "podManagementPolicy" => json!("OrderedReady"),
        _ => Value::Null,
    })
}

/// One revision of a Helm release, as the labels of its Secret tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    /// The revision number.
    pub version: u64,
    /// Helm's status, such as `deployed` or `failed`.
    pub status: String,
}

/// A release whose newest revision failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedUpgrade {
    /// The revision that still runs, when one was ever deployed.
    pub deployed: Option<u64>,
    /// The newest revision.
    pub newest: u64,
    /// The failed revisions after the deployed one, oldest first; Helm reads the release's
    /// status from the newest.
    pub failed: Vec<u64>,
}

/// The release's failed upgrade, when its newest revision failed.
#[must_use]
pub fn failed_upgrade(history: &[Revision]) -> Option<FailedUpgrade> {
    let newest = history.iter().max_by_key(|r| r.version)?;
    if newest.status != "failed" {
        return None;
    }
    let deployed = history
        .iter()
        .filter(|r| r.status == "deployed")
        .map(|r| r.version)
        .max();
    let mut failed: Vec<u64> = history
        .iter()
        .filter(|r| r.status == "failed" && Some(r.version) > deployed)
        .map(|r| r.version)
        .collect();
    failed.sort_unstable();
    Some(FailedUpgrade {
        deployed,
        newest: newest.version,
        failed,
    })
}

/// The name of the Secret that holds a revision, as Helm's Secret driver names it.
#[must_use]
pub fn revision_secret(release: &str, version: u64) -> String {
    format!("sh.helm.release.v1.{release}.v{version}")
}

fn named(changes: &[FixedChange]) -> String {
    changes
        .iter()
        .map(|c| format!("{}/{}", c.namespace, c.name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// For a planned chart change: the release's StatefulSets, which `apply` recreates when the
/// change turns out to touch a fixed field.
#[must_use]
pub fn may_recreate(release: &str, stateful_sets: &[String]) -> Warning {
    let list = stateful_sets.join(", ");
    warning(
        "statefulset.mayRecreate",
        &[release, &list],
        format!(
            "If the new settings of {release} change a field Kubernetes can't update on \
             StatefulSet {list} (volumeClaimTemplates, selector, serviceName or \
             podManagementPolicy), apply deletes the StatefulSet and Helm creates it again. Its \
             Pods and disks stay, and the disks keep their size."
        ),
    )
}

/// For a release whose last upgrade failed on fixed fields: what `apply` does.
#[must_use]
pub fn recreates(release: &str, changes: &[FixedChange]) -> Warning {
    let list = named(changes);
    let mut fields: Vec<&str> = changes.iter().flat_map(|c| c.fields.clone()).collect();
    fields.sort_unstable();
    fields.dedup();
    let fields = fields.join(", ");
    warning(
        "statefulset.recreate",
        &[release, &fields, &list],
        format!(
            "The last upgrade of {release} failed because it changes {fields} of StatefulSet \
             {list}, which Kubernetes can't update. apply deletes the StatefulSet and upgrades \
             again. Its Pods and disks stay, and the disks keep their size."
        ),
    )
}

/// What to run so helm-controller retries a release whose last upgrade failed for another reason.
#[must_use]
pub fn retry_command(release: &str, namespace: &str, failed: &FailedUpgrade) -> String {
    let secrets: Vec<String> = failed
        .failed
        .iter()
        .map(|v| revision_secret(release, *v))
        .collect();
    format!(
        "kubectl --namespace {namespace} delete secret {}",
        secrets.join(" ")
    )
}

/// For a release whose last upgrade failed for another reason: helm-controller won't retry it
/// until the failed revisions are gone.
#[must_use]
pub fn stuck(release: &str, namespace: &str, failed: &FailedUpgrade, why: &str) -> Warning {
    let command = retry_command(release, namespace, failed);
    warning(
        "release.failed",
        &[release, why, &command],
        format!(
            "The last upgrade of {release} failed ({why}). helm-controller doesn't retry a \
             failed release, so apply stops there. Once the cause is fixed, run {command} and \
             apply again."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zot(storage: &str, extra: &str) -> String {
        format!(
            "---\n# Source: zot/templates/service.yaml\napiVersion: v1\nkind: Service\nmetadata:\n  \
             name: zot\n---\n# Source: zot/templates/statefulset.yaml\napiVersion: apps/v1\nkind: \
             StatefulSet\nmetadata:\n  name: zot\nspec:\n  replicas: 1\n  serviceName: zot\n  \
             selector:\n    matchLabels: {{app.kubernetes.io/name: zot}}\n{extra}  \
             volumeClaimTemplates:\n    - metadata: {{name: zot-pvc}}\n      spec:\n        \
             accessModes: [ReadWriteOnce]\n        resources:\n          requests: {{storage: \
             {storage}}}\n"
        )
    }

    fn fields(from: &str, to: &str) -> Vec<Vec<&'static str>> {
        fixed_changes("zot", from, to)
            .unwrap()
            .into_iter()
            .map(|c| c.fields)
            .collect()
    }

    const NONE: Vec<Vec<&str>> = Vec::new();

    #[test]
    fn a_new_claim_size_is_a_fixed_change() {
        let changes = fixed_changes("zot", &zot("20Gi", ""), &zot("50Gi", "")).unwrap();
        assert_eq!(
            changes,
            vec![FixedChange {
                namespace: "zot".to_owned(),
                name: "zot".to_owned(),
                fields: vec!["volumeClaimTemplates"],
            }]
        );
    }

    #[test]
    fn fields_kubernetes_updates_are_not_fixed_changes() {
        let more = zot("20Gi", "  minReadySeconds: 5\n").replace("replicas: 1", "replicas: 2");
        assert_eq!(fields(&zot("20Gi", ""), &more), NONE);
    }

    #[test]
    fn a_default_written_out_is_no_change() {
        let explicit = zot("20Gi", "  podManagementPolicy: OrderedReady\n");
        assert_eq!(fields(&zot("20Gi", ""), &explicit), NONE);
        let parallel = zot("20Gi", "  podManagementPolicy: Parallel\n");
        assert_eq!(
            fields(&zot("20Gi", ""), &parallel),
            [["podManagementPolicy"]]
        );
    }

    #[test]
    fn a_stateful_set_only_one_side_has_is_no_fixed_change() {
        let service_only = "apiVersion: v1\nkind: Service\nmetadata:\n  name: zot\n";
        assert_eq!(fields(service_only, &zot("50Gi", "")), NONE);
        let elsewhere =
            zot("50Gi", "").replace("name: zot\nspec", "name: zot\n  namespace: other\nspec");
        assert_eq!(fields(&zot("20Gi", ""), &elsewhere), NONE);
    }

    fn history(revisions: &[(u64, &str)]) -> Vec<Revision> {
        revisions
            .iter()
            .map(|(version, status)| Revision {
                version: *version,
                status: (*status).to_owned(),
            })
            .collect()
    }

    #[test]
    fn a_failed_newest_revision_is_a_failed_upgrade_over_the_deployed_one() {
        let failed = failed_upgrade(&history(&[
            (1, "superseded"),
            (4, "failed"),
            (2, "deployed"),
            (3, "failed"),
        ]))
        .unwrap();
        assert_eq!(failed.deployed, Some(2));
        assert_eq!(failed.newest, 4);
        assert_eq!(
            retry_command("zot", "zot", &failed),
            "kubectl --namespace zot delete secret sh.helm.release.v1.zot.v3 \
             sh.helm.release.v1.zot.v4"
        );
    }

    #[test]
    fn a_deployed_newest_revision_is_not_stuck() {
        assert_eq!(
            failed_upgrade(&history(&[(1, "failed"), (2, "deployed")])),
            None
        );
        assert_eq!(failed_upgrade(&[]), None);
        assert_eq!(
            failed_upgrade(&history(&[(1, "failed")])).unwrap().deployed,
            None
        );
    }
}
