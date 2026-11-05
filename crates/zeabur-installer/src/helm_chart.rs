//! Installing a `HelmChart` and telling from the release's history whether helm-controller
//! upgraded it.
//!
//! Charts use `failurePolicy: retry`, so the install Job upgrades over a failed release. In that
//! path its script exits 0 even when the upgrade fails, so the Job's own status says nothing:
//! the `status` label of the release's newest revision Secret does.

use std::time::Duration;

use serde_json::Value;
use zeabur_installer_host::kubectl::{self, Kubectl};

use crate::InstallError;

/// The newest revision of a Helm release, from the labels of its Secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Revision {
    pub version: u64,
    pub status: String,
}

/// What the release's newest revision says about the upgrade `apply` waits for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Waiting,
    Deployed,
    Failed,
}

/// Applies a `HelmChart` and waits until Helm deploys a revision newer than the one before the
/// apply. A chart that did not change and is deployed needs no new revision. A newer failed
/// revision stops `apply` with the end of the install Job's log.
pub(crate) fn install(kubectl: &Kubectl, name: &str, object: &Value) -> Result<(), InstallError> {
    let chart_namespace = object
        .pointer("/metadata/namespace")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let namespace = release_namespace(object);
    let before = newest(kubectl, namespace, name)?;
    let previous = kubectl.get("helmchart", name, Some(chart_namespace))?;
    kubectl.apply_object(object)?;
    let applied = kubectl.get("helmchart", name, Some(chart_namespace))?;
    let changed = spec(previous.as_ref()) != spec(applied.as_ref());
    if !upgrade_expected(before.as_ref(), changed) {
        return Ok(());
    }
    restart_finished_job(kubectl, name)?;

    let mut failed = false;
    kubectl::poll(
        Duration::from_mins(15),
        &format!("the {name} chart to install"),
        || {
            let Ok(now) = newest(kubectl, namespace, name) else {
                return false;
            };
            match judge(before.as_ref(), now.as_ref()) {
                Verdict::Waiting => false,
                Verdict::Deployed => true,
                Verdict::Failed => {
                    failed = true;
                    true
                }
            }
        },
    )?;
    if !failed {
        return Ok(());
    }
    let job = format!("job/helm-install-{name}");
    let logs = kubectl
        .run(
            &["logs", "--namespace=kube-system", &job, "--tail=30"],
            None,
        )
        .unwrap_or_default();
    Err(InstallError::Message(format!(
        "the {name} chart did not upgrade; once the cause is fixed, the next apply upgrades it \
         again over the failed release\nkubectl logs --namespace=kube-system {job}:\n{}",
        logs.trim_end()
    )))
}

/// Whether Helm has to write a new revision: the `HelmChart` changed, or the release is not
/// installed or not deployed.
pub(crate) fn upgrade_expected(before: Option<&Revision>, changed: bool) -> bool {
    let Some(revision) = before else {
        return true;
    };
    changed || revision.status != "deployed"
}

/// Judges an upgrade by the newest revision: only one newer than `before` decides it.
pub(crate) fn judge(before: Option<&Revision>, now: Option<&Revision>) -> Verdict {
    let Some(now) = now else {
        return Verdict::Waiting;
    };
    if before.is_some_and(|b| now.version <= b.version) {
        return Verdict::Waiting;
    }
    match now.status.as_str() {
        "deployed" => Verdict::Deployed,
        "failed" => Verdict::Failed,
        _ => Verdict::Waiting,
    }
}

/// Deletes the install Job when it has finished, so helm-controller creates it again from the
/// `HelmChart`. helm-controller starts no Job for a change outside its config hash.
fn restart_finished_job(kubectl: &Kubectl, name: &str) -> Result<(), InstallError> {
    let job_name = format!("helm-install-{name}");
    let Some(job) = kubectl.get("job", &job_name, Some("kube-system"))? else {
        return Ok(());
    };
    if !finished(&job) {
        return Ok(());
    }
    kubectl.run(
        &[
            "delete",
            "job",
            &job_name,
            "--namespace=kube-system",
            "--ignore-not-found",
            "--wait=false",
        ],
        None,
    )?;
    Ok(())
}

/// Whether a Job has a `Complete` or `Failed` condition that is true.
fn finished(job: &Value) -> bool {
    let conditions = job.pointer("/status/conditions").and_then(Value::as_array);
    for condition in conditions.into_iter().flatten() {
        let kind = condition.get("type").and_then(Value::as_str);
        let status = condition.get("status").and_then(Value::as_str);
        if matches!(kind, Some("Complete" | "Failed")) && status == Some("True") {
            return true;
        }
    }
    false
}

/// The release's newest revision, `None` when Helm has not installed it.
fn newest(
    kubectl: &Kubectl,
    namespace: &str,
    name: &str,
) -> Result<Option<Revision>, InstallError> {
    let selector = format!("--selector=owner=helm,name={name}");
    let out = kubectl.run(
        &[
            "get",
            "secrets",
            "--namespace",
            namespace,
            &selector,
            "--output=jsonpath={range .items[*]}{.metadata.labels.version} {.metadata.labels.status}{\"\\n\"}{end}",
        ],
        None,
    )?;
    let mut newest: Option<Revision> = None;
    for line in out.lines() {
        let Some((version, status)) = line.split_once(' ') else {
            continue;
        };
        let Ok(version) = version.parse() else {
            continue;
        };
        if newest.as_ref().is_none_or(|n| version > n.version) {
            newest = Some(Revision {
                version,
                status: status.to_owned(),
            });
        }
    }
    Ok(newest)
}

fn spec(chart: Option<&Value>) -> Option<&Value> {
    chart.and_then(|c| c.get("spec"))
}

/// The namespace helm-controller installs a `HelmChart`'s release into: `targetNamespace`, else
/// the `HelmChart`'s own.
fn release_namespace(object: &Value) -> &str {
    let target = object
        .pointer("/spec/targetNamespace")
        .and_then(Value::as_str);
    let own = object
        .pointer("/metadata/namespace")
        .and_then(Value::as_str);
    target.or(own).unwrap_or("default")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;

    use serde_json::json;

    use super::*;

    fn revision(version: u64, status: &str) -> Revision {
        Revision {
            version,
            status: status.to_owned(),
        }
    }

    #[test]
    fn a_failed_release_is_upgraded_again_even_when_its_chart_did_not_change() {
        assert!(upgrade_expected(Some(&revision(2, "failed")), false));
        assert!(upgrade_expected(
            Some(&revision(2, "pending-upgrade")),
            false
        ));
        assert!(upgrade_expected(None, false));
        assert!(upgrade_expected(Some(&revision(2, "deployed")), true));
        assert!(!upgrade_expected(Some(&revision(2, "deployed")), false));
    }

    #[test]
    fn only_a_revision_newer_than_the_one_before_decides() {
        let failed = revision(2, "failed");
        assert_eq!(judge(Some(&failed), Some(&failed)), Verdict::Waiting);
        assert_eq!(
            judge(Some(&failed), Some(&revision(3, "deployed"))),
            Verdict::Deployed
        );
        assert_eq!(
            judge(Some(&failed), Some(&revision(3, "failed"))),
            Verdict::Failed
        );
        assert_eq!(
            judge(Some(&failed), Some(&revision(3, "pending-upgrade"))),
            Verdict::Waiting
        );
        assert_eq!(
            judge(None, Some(&revision(1, "deployed"))),
            Verdict::Deployed
        );
        assert_eq!(judge(None, None), Verdict::Waiting);
    }

    /// A `kubectl` whose release history is `before` until something is applied and `after`
    /// from then on; the install Job is `job`, deletes go to `deleted`.
    const FAKE: &str = r#"#!/bin/sh
dir=$(dirname "${1#--kubeconfig=}")
shift
case "$*" in
"get secrets "*) if [ -f "$dir/applied" ]; then cat "$dir/after"; else cat "$dir/before"; fi ;;
"get helmchart "*) if [ -f "$dir/applied" ]; then cat "$dir/applied"; else cat "$dir/previous" 2>/dev/null || true; fi ;;
"get job "*) cat "$dir/job" 2>/dev/null || true ;;
"logs "*) echo 'Error: UPGRADE FAILED: StatefulSet.apps "zot" is invalid: spec: Forbidden' ;;
"delete "*) echo "$*" >> "$dir/deleted" ;;
*apply*) cat > "$dir/applied" ;;
esac
"#;

    fn fake(dir: &Path, before: &str, after: &str) -> Kubectl {
        fs::write(dir.join("before"), before).unwrap();
        fs::write(dir.join("after"), after).unwrap();
        let binary = dir.join("kubectl");
        fs::write(&binary, FAKE).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        Kubectl::new(&binary, dir.join("kubeconfig"))
    }

    fn zot(storage: &str) -> Value {
        json!({
            "apiVersion": "helm.cattle.io/v1",
            "kind": "HelmChart",
            "metadata": {"name": "zot", "namespace": "kube-system"},
            "spec": {"targetNamespace": "zot", "valuesContent": storage},
        })
    }

    const COMPLETE: &str = r#"{"status":{"conditions":[{"type":"Complete","status":"True"}]}}"#;

    #[test]
    fn a_failed_upgrade_stops_apply_with_its_cause() {
        let dir = tempfile::tempdir().unwrap();
        let kubectl = fake(dir.path(), "1 deployed\n", "1 deployed\n2 failed\n");
        let error = install(&kubectl, "zot", &zot("50Gi"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("the zot chart did not upgrade"), "{error}");
        assert!(error.contains("next apply upgrades it again"), "{error}");
        assert!(error.contains("UPGRADE FAILED"), "{error}");
    }

    #[test]
    fn a_failed_release_with_an_unchanged_chart_gets_its_install_job_again() {
        let dir = tempfile::tempdir().unwrap();
        let chart = zot("20Gi");
        fs::write(dir.path().join("previous"), chart.to_string()).unwrap();
        fs::write(dir.path().join("job"), COMPLETE).unwrap();
        let kubectl = fake(
            dir.path(),
            "1 deployed\n2 failed\n",
            "2 failed\n3 deployed\n",
        );
        install(&kubectl, "zot", &chart).unwrap();
        let deleted = fs::read_to_string(dir.path().join("deleted")).unwrap();
        assert!(
            deleted.starts_with("delete job helm-install-zot "),
            "{deleted}"
        );
    }

    #[test]
    fn a_changed_chart_restarts_a_finished_install_job_but_not_a_running_one() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("previous"), zot("20Gi").to_string()).unwrap();
        fs::write(dir.path().join("job"), COMPLETE).unwrap();
        let kubectl = fake(dir.path(), "1 deployed\n", "1 superseded\n2 deployed\n");
        install(&kubectl, "zot", &zot("50Gi")).unwrap();
        assert!(dir.path().join("deleted").exists());

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("previous"), zot("20Gi").to_string()).unwrap();
        fs::write(dir.path().join("job"), r#"{"status":{"active":1}}"#).unwrap();
        let kubectl = fake(dir.path(), "1 deployed\n", "1 superseded\n2 deployed\n");
        install(&kubectl, "zot", &zot("50Gi")).unwrap();
        assert!(!dir.path().join("deleted").exists());
    }

    #[test]
    fn an_unchanged_deployed_chart_needs_no_new_revision() {
        let dir = tempfile::tempdir().unwrap();
        let chart = zot("20Gi");
        fs::write(dir.path().join("previous"), chart.to_string()).unwrap();
        fs::write(dir.path().join("job"), COMPLETE).unwrap();
        let kubectl = fake(dir.path(), "1 deployed\n", "1 deployed\n");
        install(&kubectl, "zot", &chart).unwrap();
        assert!(!dir.path().join("deleted").exists());
    }
}
