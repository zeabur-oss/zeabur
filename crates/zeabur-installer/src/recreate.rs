//! Charts whose upgrade changes a StatefulSet field Kubernetes won't update
//! ([`zeabur_installer_plan::statefulset`]): what `plan` says about them, and how `apply` gets
//! the release through.

use serde_json::Value;
use zeabur_installer_host::helm;
use zeabur_installer_host::kubectl::Kubectl;
use zeabur_installer_plan::plan::{Plan, ReleasePlan, Warning};
use zeabur_installer_plan::statefulset::{
    self, FailedUpgrade, FixedChange, failed_upgrade, fixed_changes,
};

use crate::{InstallError, out};

const RELEASE_NAME: &str = "meta.helm.sh/release-name";
const RELEASE_NAMESPACE: &str = "meta.helm.sh/release-namespace";

/// A release whose newest revision failed.
pub(crate) struct Stuck {
    failed: FailedUpgrade,
    /// The StatefulSets that made it fail; empty when it failed for another reason.
    changes: Vec<FixedChange>,
    description: String,
}

/// The release's failed upgrade, when its newest revision failed.
pub(crate) fn stuck(
    kubectl: &Kubectl,
    name: &str,
    namespace: &str,
) -> Result<Option<Stuck>, InstallError> {
    let Some(failed) = failed_upgrade(&helm::history(kubectl, namespace, name)?) else {
        return Ok(None);
    };
    let newest = helm::release(kubectl, namespace, name, failed.newest)?;
    let description = newest
        .as_ref()
        .map_or_else(String::new, |r| r.description.clone());
    let deployed = match failed.deployed {
        Some(version) => helm::release(kubectl, namespace, name, version)?,
        None => None,
    };
    let changes = match (deployed, newest) {
        (Some(deployed), Some(newest)) => {
            fixed_changes(namespace, &deployed.manifest, &newest.manifest)?
        }
        _ => Vec::new(),
    };
    Ok(Some(Stuck {
        failed,
        changes,
        description,
    }))
}

/// Gets a release stuck on fixed StatefulSet fields through: deletes those StatefulSets, keeping
/// their Pods and claims, and the failed revisions, so helm-controller upgrades from the deployed
/// one again; then restarts the waiting install Job's Pod.
///
/// # Errors
///
/// When it failed for another reason, with what to do.
pub(crate) fn unstick(
    kubectl: &Kubectl,
    name: &str,
    namespace: &str,
    stuck: &Stuck,
) -> Result<(), InstallError> {
    if stuck.changes.is_empty() {
        return Err(failed_again(name, namespace, stuck));
    }
    for change in &stuck.changes {
        out::line(&format!(
            "    StatefulSet {}/{}: {} cannot change; recreating it, its Pods and claims stay",
            change.namespace,
            change.name,
            change.fields.join(", ")
        ));
        kubectl.run(
            &[
                "delete",
                "statefulset",
                &change.name,
                "--namespace",
                &change.namespace,
                "--cascade=orphan",
                "--ignore-not-found",
            ],
            None,
        )?;
    }
    for version in &stuck.failed.failed {
        kubectl.run(
            &[
                "delete",
                "secret",
                &statefulset::revision_secret(name, *version),
                "--namespace",
                namespace,
                "--ignore-not-found",
            ],
            None,
        )?;
    }
    let job = format!("--selector=job-name=helm-install-{name}");
    kubectl.run(
        &[
            "delete",
            "pods",
            "--namespace=kube-system",
            &job,
            "--ignore-not-found",
            "--wait=false",
        ],
        None,
    )?;
    Ok(())
}

/// The release failed and `apply` cannot get it through: what to do.
pub(crate) fn failed_again(name: &str, namespace: &str, stuck: &Stuck) -> InstallError {
    InstallError::Message(format!(
        "the last upgrade of {name} failed: {}\nhelm-controller does not retry a failed release. \
         Once the cause is fixed, run `{}` and apply again.",
        stuck.description,
        statefulset::retry_command(name, namespace, &stuck.failed)
    ))
}

/// What `apply` does about the StatefulSets of the installed charts that change or are stuck.
pub(crate) fn warnings(kubectl: &Kubectl, plan: &Plan) -> Result<Vec<Warning>, InstallError> {
    let Some(changes) = &plan.changes else {
        return Ok(Vec::new());
    };
    let failed = helm::with_failed_revisions(kubectl)?;
    let mut warnings = Vec::new();
    let mut owned: Option<Vec<(String, String, String)>> = None;
    for (component, release) in plan
        .components
        .iter()
        .flat_map(|c| c.releases.iter().map(move |r| (c, r)))
    {
        let ReleasePlan::HelmChart { name, object, .. } = release else {
            continue;
        };
        let Some(namespace) = object
            .pointer("/spec/targetNamespace")
            .and_then(Value::as_str)
        else {
            continue;
        };
        let has_failed = failed.iter().any(|(n, r)| n == namespace && r == name);
        if has_failed && let Some(stuck) = stuck(kubectl, name, namespace)? {
            warnings.push(if stuck.changes.is_empty() {
                statefulset::stuck(name, namespace, &stuck.failed, &stuck.description)
            } else {
                statefulset::recreates(name, &stuck.changes)
            });
            continue;
        }
        if !changes
            .charts
            .iter()
            .any(|c| c.component == component.id && c.release == *name)
        {
            continue;
        }
        let owned = match &mut owned {
            Some(owned) => owned,
            None => owned.insert(helm_stateful_sets(kubectl)?),
        };
        let sets: Vec<String> = owned
            .iter()
            .filter(|(release, release_namespace, _)| {
                release == name && release_namespace == namespace
            })
            .map(|(.., set)| set.clone())
            .collect();
        if !sets.is_empty() {
            warnings.push(statefulset::may_recreate(name, &sets));
        }
    }
    Ok(warnings)
}

/// Each StatefulSet Helm installed: its release, the release's namespace, and
/// `namespace/name`.
fn helm_stateful_sets(kubectl: &Kubectl) -> Result<Vec<(String, String, String)>, InstallError> {
    let text = kubectl.run(
        &["get", "statefulsets", "--all-namespaces", "--output=json"],
        None,
    )?;
    let list: Value = serde_json::from_str(&text)
        .map_err(|e| InstallError::Message(format!("kubectl get statefulsets: {e}")))?;
    Ok(list
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|set| {
            let meta = set.get("metadata")?;
            let text = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
            let annotations = meta.get("annotations");
            Some((
                text(annotations.and_then(|a| a.get(RELEASE_NAME)))?,
                text(annotations.and_then(|a| a.get(RELEASE_NAMESPACE)))?,
                format!(
                    "{}/{}",
                    text(meta.get("namespace"))?,
                    text(meta.get("name"))?
                ),
            ))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::fs;
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use serde_json::json;

    use super::*;

    /// A `kubectl` that logs its arguments and answers Helm's history and revisions from files.
    const FAKE: &str = r#"#!/bin/sh
dir=$(dirname "${1#--kubeconfig=}")
shift
echo "$*" >> "$dir/calls"
case "$1 $2" in
  "get secrets") cat "$dir/history" ;;
  "get secret") cat "$dir/$3" 2>/dev/null ;;
esac
"#;

    fn zot(storage: &str) -> String {
        format!(
            "apiVersion: apps/v1\nkind: StatefulSet\nmetadata:\n  name: zot\nspec:\n  \
             volumeClaimTemplates:\n    - metadata: {{name: zot-pvc}}\n      spec:\n        \
             resources:\n          requests: {{storage: {storage}}}\n"
        )
    }

    fn cluster(revisions: &[(u64, &str, String)]) -> (tempfile::TempDir, Kubectl) {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("kubectl");
        fs::write(&binary, FAKE).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let mut history = String::new();
        for (version, status, manifest) in revisions {
            writeln!(history, "{version} {status}").unwrap();
            let release = json!({
                "info": {"status": status, "description": format!("{status} revision")},
                "manifest": manifest,
            });
            let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
            gzip.write_all(release.to_string().as_bytes()).unwrap();
            let helm = STANDARD.encode(gzip.finish().unwrap());
            let secret = json!({"data": {"release": STANDARD.encode(helm)}});
            fs::write(
                dir.path()
                    .join(statefulset::revision_secret("zot", *version)),
                secret.to_string(),
            )
            .unwrap();
        }
        fs::write(dir.path().join("history"), history).unwrap();
        let kubectl = Kubectl::new(&binary, dir.path().join("kubeconfig"));
        (dir, kubectl)
    }

    fn deletes(dir: &Path) -> Vec<String> {
        fs::read_to_string(dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .filter(|l| l.starts_with("delete"))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn a_release_stuck_on_a_claim_size_gets_through() {
        let (dir, kubectl) = cluster(&[
            (1, "superseded", zot("20Gi")),
            (2, "deployed", zot("20Gi")),
            (3, "failed", zot("50Gi")),
        ]);
        let stuck = stuck(&kubectl, "zot", "zot").unwrap().unwrap();
        unstick(&kubectl, "zot", "zot", &stuck).unwrap();
        assert_eq!(
            deletes(dir.path()),
            [
                "delete statefulset zot --namespace zot --cascade=orphan --ignore-not-found",
                "delete secret sh.helm.release.v1.zot.v3 --namespace zot --ignore-not-found",
                "delete pods --namespace=kube-system --selector=job-name=helm-install-zot \
                 --ignore-not-found --wait=false",
            ]
        );
    }

    #[test]
    fn a_release_failed_for_another_reason_says_how_to_retry_and_deletes_nothing() {
        let (dir, kubectl) = cluster(&[(1, "deployed", zot("20Gi")), (2, "failed", zot("20Gi"))]);
        let stuck = stuck(&kubectl, "zot", "zot").unwrap().unwrap();
        let error = unstick(&kubectl, "zot", "zot", &stuck)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("failed revision"),
            "names Helm's description: {error}"
        );
        assert!(
            error.contains("kubectl --namespace zot delete secret sh.helm.release.v1.zot.v2"),
            "{error}"
        );
        assert_eq!(deletes(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn a_deployed_release_is_not_stuck() {
        let (_dir, kubectl) =
            cluster(&[(1, "superseded", zot("20Gi")), (2, "deployed", zot("50Gi"))]);
        assert!(stuck(&kubectl, "zot", "zot").unwrap().is_none());
    }
}
