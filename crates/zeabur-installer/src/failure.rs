//! What `apply` does when a Helm release does not upgrade: get a release left failed or pending
//! ready to upgrade, judge the upgrade by the release's history, roll back to the last good
//! revision, and say why it stopped.
//!
//! The host has no Helm, so `helm` runs in a Job from the image and service account of
//! helm-controller's install Job for the release.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use zeabur_installer_host::kubectl::{self, Kubectl};
use zeabur_installer_host::{archive, helm};
use zeabur_installer_plan::helm::{Revision, is_deterministic, is_pending, last_good, newest};
use zeabur_installer_plan::plan::MANAGER;
use zeabur_installer_plan::values;

use crate::{InstallError, out};

/// A release as helm-controller installs it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Release<'a> {
    /// The `HelmChart`'s name, which is the release's.
    pub name: &'a str,
    /// The release's namespace.
    pub namespace: &'a str,
}

impl Release<'_> {
    fn install_job(self) -> String {
        format!("helm-install-{}", self.name)
    }
}

/// Before an upgrade: a failed release is rolled back, so helm-controller upgrades it again
/// under `failurePolicy: abort`. A release left pending by an install Job that is gone needs no
/// help here: the install Job's next run marks it failed and upgrades it again. Returns whether
/// the install Job has to run again for the release to upgrade.
///
/// # Errors
///
/// When the release failed and has no revision to roll back to, with the cause.
pub(crate) fn prepare(kubectl: &Kubectl, release: Release<'_>) -> Result<bool, InstallError> {
    let history = helm::history(kubectl, release.namespace, release.name)?;
    let Some(top) = newest(&history) else {
        return Ok(false);
    };
    if is_pending(&top.status) {
        if installing(kubectl, release)? {
            return Ok(false);
        }
        out::err(&format!(
            "    {} was left {}; its install Job marks it failed and upgrades it again",
            release.name, top.status
        ));
        return Ok(true);
    }
    if top.status != "failed" {
        return Ok(false);
    }
    let reason = description(kubectl, release, top.version)?;
    if last_good(&history).is_none() {
        return Err(stop(kubectl, release, &reason, &Restored::Untouched, None));
    }
    out::err(&format!(
        "    the last upgrade of {} failed; rolling back before upgrading again",
        release.name
    ));
    rollback(kubectl, release)?;
    Ok(true)
}

/// What the release's history and install Job say about an upgrade `apply` waits for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Not done yet.
    Waiting,
    /// The newest revision is deployed and is the upgrade's.
    Deployed,
    /// The upgrade's revision failed, or an install Job created for it gave up.
    Failed,
}

/// Judges an upgrade by the release's history, so it does not matter when helm-controller
/// replaces the install Job. A revision newer than `since` decides it. When no upgrade is
/// `expected`, the revision `since` itself counts when it is deployed. `job` is the install Job;
/// one other than `stale` that gave up before it wrote a revision fails the upgrade too.
pub(crate) fn judge(
    history: &[Revision],
    since: Option<u64>,
    expected: bool,
    job: Option<&Value>,
    stale: Option<&Value>,
) -> Verdict {
    if let Some(top) = newest(history) {
        let newer = since.is_none_or(|s| top.version > s);
        if newer || !expected {
            match top.status.as_str() {
                "deployed" => return Verdict::Deployed,
                "failed" if newer => return Verdict::Failed,
                _ => {}
            }
        }
    }
    if let Some(job) = job
        && job.pointer("/metadata/uid") != stale
        && gave_up(job)
    {
        return Verdict::Failed;
    }
    Verdict::Waiting
}

/// Whether a Job has stopped trying without succeeding.
pub(crate) fn gave_up(job: &Value) -> bool {
    let count = |key: &str| {
        job.pointer(&format!("/status/{key}"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    count("succeeded") == 0 && count("failed") > 0 && count("active") == 0
}

/// After a failed upgrade: the first one whose error may pass is rolled back and its install Job
/// started again. Returns the newest revision and the install Job's uid from before the restart,
/// or, when it does not try again, Helm's reason.
pub(crate) fn retry_once(
    kubectl: &Kubectl,
    release: Release<'_>,
    retried: &mut bool,
) -> Result<(Option<u64>, Option<Value>), String> {
    let history =
        helm::history(kubectl, release.namespace, release.name).map_err(|e| e.to_string())?;
    let mut reason = String::new();
    let mut failed = false;
    if let Some(top) = newest(&history)
        && top.status == "failed"
    {
        failed = true;
        reason = description(kubectl, release, top.version).unwrap_or_default();
    }
    if *retried || is_deterministic(&reason) {
        return Err(reason);
    }
    *retried = true;
    if reason.is_empty() {
        out::err(&format!(
            "    {} did not upgrade; trying once more",
            release.name
        ));
    } else {
        out::err(&format!(
            "    {} did not upgrade ({reason}); trying once more",
            release.name
        ));
    }
    if failed && let Err(error) = rollback(kubectl, release) {
        return Err(format!("{reason}; rollback before the retry: {error}"));
    }
    let stale = install_uid(kubectl, release).map_err(|e| e.to_string())?;
    if let Err(error) = restart_install(kubectl, release) {
        return Err(format!("{reason}; restarting the install Job: {error}"));
    }
    let history =
        helm::history(kubectl, release.namespace, release.name).map_err(|e| e.to_string())?;
    Ok((newest(&history).map(|r| r.version), stale))
}

/// Rolls back to the last good revision, from which the install Job's next try upgrades.
/// Returns that revision.
///
/// # Errors
///
/// When there is none or `helm rollback` fails.
pub(crate) fn rollback(kubectl: &Kubectl, release: Release<'_>) -> Result<u64, InstallError> {
    let history = helm::history(kubectl, release.namespace, release.name)?;
    let Some(good) = last_good(&history) else {
        return Err(InstallError::Message(format!(
            "{} has no deployed revision to roll back to",
            release.name
        )));
    };
    let revision = good.to_string();
    run_helm(
        kubectl,
        release,
        &[
            "rollback",
            release.name,
            &revision,
            "--namespace",
            release.namespace,
        ],
    )?;
    Ok(good)
}

/// What [`give_up`] did with the `HelmChart`.
#[derive(Debug, PartialEq, Eq)]
enum Restored {
    /// Put back with the chart and values of the revision the release runs.
    Matching,
    /// Left as it is, for this reason.
    Left(String),
    /// Nothing to put back.
    Untouched,
}

/// Gives up on an upgrade: rolls the release back when its newest revision failed, and puts
/// back a `HelmChart` with the chart and values of the revision the release now runs, so
/// helm-controller has no failing upgrade to try again. `previous` is the `HelmChart` from before
/// the apply and `applied` the one it applied; the chart comes from whichever holds the
/// revision's version. Returns the error that says why, for `apply` to stop with.
pub(crate) fn give_up(
    kubectl: &Kubectl,
    release: Release<'_>,
    previous: Option<&Value>,
    applied: &Value,
    reason: &str,
) -> InstallError {
    let Some(previous) = previous else {
        return stop(kubectl, release, reason, &Restored::Untouched, None);
    };
    let history = helm::history(kubectl, release.namespace, release.name).unwrap_or_default();
    let mut kept = None;
    if newest(&history).is_some_and(|r| r.status == "failed") {
        match rollback(kubectl, release) {
            Ok(revision) => kept = Some(revision),
            Err(error) => out::err(&format!("    rollback failed: {error}")),
        }
    }
    let Some(version) = kept.or_else(|| last_good(&history)) else {
        let left = Restored::Left("the release has no deployed revision".to_owned());
        return stop(kubectl, release, reason, &left, kept);
    };
    let restored = restore(kubectl, release, &[previous, applied], version);
    stop(kubectl, release, reason, &restored, kept)
}

/// Applies a `HelmChart` that matches revision `version`, built from the first of `charts`
/// that holds its chart.
fn restore(kubectl: &Kubectl, release: Release<'_>, charts: &[&Value], version: u64) -> Restored {
    let revision = match helm::release(kubectl, release.namespace, release.name, version) {
        Ok(Some(revision)) => revision,
        Ok(None) => return Restored::Left(format!("revision {version} is gone")),
        Err(error) => return Restored::Left(format!("revision {version}: {error}")),
    };
    let Some(chart) = matching_chart(charts, &revision) else {
        return Restored::Left(format!(
            "neither the previous nor the new HelmChart holds chart version {} of revision {version}",
            revision.chart_version
        ));
    };
    match kubectl.apply_object(&chart) {
        Ok(()) => Restored::Matching,
        Err(error) => Restored::Left(format!("applying it failed: {error}")),
    }
}

/// The first of `charts` whose chart has the revision's version, with the revision's values.
fn matching_chart(charts: &[&Value], revision: &helm::Release) -> Option<Value> {
    for chart in charts {
        if chart_version(chart).as_deref() != Some(revision.chart_version.as_str()) {
            continue;
        }
        let mut restored = restorable(chart);
        let config = if revision.config.is_null() {
            json!({})
        } else {
            revision.config.clone()
        };
        if let Some(spec) = restored.get_mut("spec").and_then(Value::as_object_mut) {
            spec.insert(
                "valuesContent".to_owned(),
                json!(values::canonical(&config)),
            );
        }
        return Some(restored);
    }
    None
}

/// The version of the chart a `HelmChart` installs: from `chartContent`, else `version`.
fn chart_version(chart: &Value) -> Option<String> {
    if let Some(content) = chart.pointer("/spec/chartContent").and_then(Value::as_str) {
        let archive = STANDARD.decode(content).ok()?;
        return archive::chart_version(&archive).ok();
    }
    chart
        .pointer("/spec/version")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The error `apply` stops with: Helm's reason, the `HelmChart`'s conditions and the end of the
/// install Job's log.
fn stop(
    kubectl: &Kubectl,
    release: Release<'_>,
    reason: &str,
    restored: &Restored,
    kept: Option<u64>,
) -> InstallError {
    let chart = kubectl
        .get("helmchart", release.name, Some("kube-system"))
        .ok()
        .flatten();
    let log = kubectl
        .run(
            &[
                "logs",
                "--namespace=kube-system",
                &format!("job/{}", release.install_job()),
                "--tail=20",
            ],
            None,
        )
        .unwrap_or_default();
    InstallError::Message(cause(release, reason, chart.as_ref(), &log, kept, restored))
}

/// [`stop`]'s text.
fn cause(
    release: Release<'_>,
    reason: &str,
    chart: Option<&Value>,
    log: &str,
    kept: Option<u64>,
    restored: &Restored,
) -> String {
    let name = release.name;
    let mut lines = Vec::new();
    if reason.is_empty() {
        lines.push(format!("the {name} chart did not upgrade"));
    } else {
        lines.push(format!("the {name} chart did not upgrade: {reason}"));
    }
    match kept {
        Some(revision) => lines.push(format!(
            "rolled back to revision {revision}, so the previous version keeps running"
        )),
        None => lines.push("nothing was rolled back".to_owned()),
    }
    if let Restored::Left(why) = restored {
        lines.push(format!(
            "the HelmChart kube-system/{name} still holds the upgrade that failed, since no \
             HelmChart matching the running revision could be built: {why}"
        ));
    }
    let conditions = chart
        .and_then(|c| c.pointer("/status/conditions"))
        .and_then(Value::as_array);
    for condition in conditions.into_iter().flatten() {
        let field = |key: &str| condition.get(key).and_then(Value::as_str).unwrap_or("");
        let mut line = format!(
            "HelmChart kube-system/{name}: {} {}",
            field("type"),
            field("status")
        );
        for key in ["reason", "message"] {
            if !field(key).is_empty() {
                line = format!("{line}, {}", field(key));
            }
        }
        lines.push(line);
    }
    if !log.trim().is_empty() {
        lines.push(format!(
            "kubectl logs --namespace=kube-system job/{}:\n{}",
            release.install_job(),
            log.trim_end()
        ));
    }
    lines.join("\n")
}

/// Helm's description of a revision, which names the error of a failed one.
pub(crate) fn description(
    kubectl: &Kubectl,
    release: Release<'_>,
    version: u64,
) -> Result<String, InstallError> {
    let revision = helm::release(kubectl, release.namespace, release.name, version)?;
    Ok(revision.map_or_default(|r| r.description))
}

/// Whether a container of the install Job runs now, holding the release legitimately.
pub(crate) fn installing(kubectl: &Kubectl, release: Release<'_>) -> Result<bool, InstallError> {
    let selector = format!("--selector=job-name={}", release.install_job());
    let pods = kubectl.run(
        &[
            "get",
            "pods",
            "--namespace=kube-system",
            &selector,
            "--output=jsonpath={.items[*].status.containerStatuses[*].state.running.startedAt}",
        ],
        None,
    )?;
    Ok(!pods.trim().is_empty())
}

/// Deletes the install Job, so helm-controller creates it again from the `HelmChart` and runs it
/// now, with a fresh back-off. Only for a `HelmChart` helm-controller has no change to act on,
/// since it replaces the Job of a changed one itself.
pub(crate) fn restart_install(kubectl: &Kubectl, release: Release<'_>) -> Result<(), InstallError> {
    kubectl.run(
        &[
            "delete",
            "job",
            &release.install_job(),
            "--namespace=kube-system",
            "--ignore-not-found",
            "--wait=false",
        ],
        None,
    )?;
    Ok(())
}

/// The install Job's uid, if it exists.
pub(crate) fn install_uid(
    kubectl: &Kubectl,
    release: Release<'_>,
) -> Result<Option<Value>, InstallError> {
    let job = kubectl.get("job", &release.install_job(), Some("kube-system"))?;
    Ok(job.and_then(|j| j.pointer("/metadata/uid").cloned()))
}

/// Runs `helm` with `args` in a Job like the release's install Job and returns its output.
fn run_helm(
    kubectl: &Kubectl,
    release: Release<'_>,
    args: &[&str],
) -> Result<String, InstallError> {
    // helm-controller creates the install Job again within seconds of a restart.
    let mut install = None;
    let found = kubectl::poll(Duration::from_mins(1), &release.install_job(), || {
        install = kubectl
            .get("job", &release.install_job(), Some("kube-system"))
            .ok()
            .flatten();
        install.is_some()
    });
    let Some(install) = install.filter(|_| found.is_ok()) else {
        return Err(InstallError::Message(format!(
            "kube-system/{} is gone, so helm cannot run for {}",
            release.install_job(),
            release.name
        )));
    };
    let job = helm_job(&install, release.name, args);
    let name = format!("zeabur-helm-{}", release.name);
    kubectl.run(
        &[
            "delete",
            "job",
            &name,
            "--namespace=kube-system",
            "--ignore-not-found",
            "--wait=true",
        ],
        None,
    )?;
    kubectl.apply_object(&job)?;
    let mut done = None;
    let what = format!("helm {}", args.join(" "));
    kubectl::poll(Duration::from_mins(5), &what, || {
        let Ok(Some(job)) = kubectl.get("job", &name, Some("kube-system")) else {
            return false;
        };
        let count = |key: &str| {
            job.pointer(&format!("/status/{key}"))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        if count("succeeded") > 0 {
            done = Some(true);
        } else if count("failed") > 0 {
            done = Some(false);
        }
        done.is_some()
    })?;
    let log = kubectl
        .run(
            &["logs", "--namespace=kube-system", &format!("job/{name}")],
            None,
        )
        .unwrap_or_default();
    if done == Some(true) {
        Ok(log)
    } else {
        Err(InstallError::Message(format!(
            "helm {} for {} failed:\n{}",
            args.join(" "),
            release.name,
            log.trim_end()
        )))
    }
}

/// A Job that runs `helm args` once, with the image, service account and volumes of the
/// release's install Job.
fn helm_job(install: &Value, release: &str, args: &[&str]) -> Value {
    let mut pod = install
        .pointer("/spec/template/spec")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if let Some(spec) = pod.as_object_mut() {
        spec.insert("restartPolicy".to_owned(), json!("Never"));
    }
    if let Some(container) = pod
        .pointer_mut("/containers/0")
        .and_then(Value::as_object_mut)
    {
        container.insert("command".to_owned(), json!(["helm"]));
        container.insert("args".to_owned(), json!(args));
    }
    json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": format!("zeabur-helm-{release}"),
            "namespace": "kube-system",
            "labels": {"app.kubernetes.io/managed-by": MANAGER},
        },
        "spec": {
            "backoffLimit": 0,
            "ttlSecondsAfterFinished": 600,
            "template": {
                "metadata": {"labels": {"app.kubernetes.io/managed-by": MANAGER}},
                "spec": pod,
            },
        },
    })
}

/// The parts of a `HelmChart` read from the cluster that applying it again sets.
fn restorable(chart: &Value) -> Value {
    let mut metadata = serde_json::Map::new();
    for key in ["name", "namespace", "labels", "annotations"] {
        if let Some(value) = chart.pointer(&format!("/metadata/{key}")) {
            metadata.insert(key.to_owned(), value.clone());
        }
    }
    json!({
        "apiVersion": chart.get("apiVersion"),
        "kind": chart.get("kind"),
        "metadata": metadata,
        "spec": chart.get("spec"),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use zeabur_installer_host::archive::pack_chart;

    use super::*;

    const ZOT: Release<'static> = Release {
        name: "zot",
        namespace: "zot",
    };

    #[test]
    fn helm_runs_once_as_the_install_job_would() {
        let install = json!({"spec": {"template": {"spec": {
            "serviceAccountName": "helm-zot",
            "restartPolicy": "OnFailure",
            "containers": [{"name": "helm", "image": "rancher/klipper-helm:v0.9.4", "args": ["install"]}],
        }}}});
        let job = helm_job(
            &install,
            "zot",
            &["rollback", "zot", "2", "--namespace", "zot"],
        );
        let pod = &job["spec"]["template"]["spec"];
        assert_eq!(pod["serviceAccountName"], "helm-zot");
        assert_eq!(pod["restartPolicy"], "Never");
        assert_eq!(pod["containers"][0]["image"], "rancher/klipper-helm:v0.9.4");
        assert_eq!(pod["containers"][0]["command"], json!(["helm"]));
        assert_eq!(
            pod["containers"][0]["args"],
            json!(["rollback", "zot", "2", "--namespace", "zot"])
        );
        assert_eq!(job["spec"]["backoffLimit"], 0);
    }

    #[test]
    fn the_cause_names_helms_reason_the_conditions_and_the_log() {
        let chart = json!({"status": {"conditions": [{
            "type": "Failed",
            "status": "True",
            "reason": "BackoffLimitExceeded",
            "message": "Job has reached the specified backoff limit",
        }]}});
        let text = cause(
            ZOT,
            "Upgrade \"zot\" failed: StatefulSet.apps \"zot\" is invalid",
            Some(&chart),
            "Upgrading zot\nError: UPGRADE FAILED\n",
            Some(2),
            &Restored::Matching,
        );
        assert!(text.contains("is invalid"), "{text}");
        assert!(text.contains("rolled back to revision 2"), "{text}");
        assert!(text.contains("Failed True, BackoffLimitExceeded"), "{text}");
        assert!(text.contains("Error: UPGRADE FAILED"), "{text}");
        assert!(!text.contains("not done after"), "{text}");
    }

    #[test]
    fn a_restored_chart_drops_what_the_cluster_added() {
        let live = json!({
            "apiVersion": "helm.cattle.io/v1",
            "kind": "HelmChart",
            "metadata": {"name": "zot", "namespace": "kube-system", "resourceVersion": "9",
                         "uid": "u", "labels": {"a": "b"}},
            "spec": {"version": "1.0.0"},
            "status": {"jobName": "helm-install-zot"},
        });
        let restored = restorable(&live);
        assert_eq!(restored["spec"]["version"], "1.0.0");
        assert_eq!(restored.pointer("/metadata/resourceVersion"), None);
        assert_eq!(restored.get("status"), None);
    }

    fn history(revisions: &[(u64, &str)]) -> Vec<Revision> {
        let mut history = Vec::new();
        for (version, status) in revisions {
            history.push(Revision {
                version: *version,
                status: (*status).to_owned(),
            });
        }
        history
    }

    #[test]
    fn a_finished_or_missing_old_job_is_not_success_while_the_upgrade_has_no_revision() {
        let before = history(&[(1, "superseded"), (2, "failed"), (3, "deployed")]);
        let old = json!({"metadata": {"uid": "old"}, "status": {"succeeded": 1}});
        let stale = json!("old");
        let verdict = judge(&before, Some(3), true, Some(&old), Some(&stale));
        assert_eq!(verdict, Verdict::Waiting);
        assert_eq!(
            judge(&before, Some(3), true, None, Some(&stale)),
            Verdict::Waiting
        );
    }

    #[test]
    fn a_failed_revision_after_the_apply_fails_it_whatever_the_job_shows() {
        let after = history(&[
            (1, "superseded"),
            (2, "failed"),
            (3, "superseded"),
            (4, "failed"),
        ]);
        let old = json!({"metadata": {"uid": "old"}, "status": {"succeeded": 1}});
        let stale = json!("old");
        assert_eq!(
            judge(&after, Some(3), true, Some(&old), Some(&stale)),
            Verdict::Failed
        );
        assert_eq!(
            judge(&after, Some(3), true, None, Some(&stale)),
            Verdict::Failed
        );
    }

    #[test]
    fn a_deployed_revision_after_the_apply_is_success() {
        let after = history(&[(3, "superseded"), (4, "deployed")]);
        assert_eq!(judge(&after, Some(3), true, None, None), Verdict::Deployed);
        assert_eq!(
            judge(&history(&[(1, "deployed")]), None, true, None, None),
            Verdict::Deployed
        );
    }

    #[test]
    fn an_unchanged_chart_is_done_at_the_revision_it_had() {
        let same = history(&[(1, "superseded"), (2, "deployed")]);
        assert_eq!(judge(&same, Some(2), false, None, None), Verdict::Deployed);
        let running = history(&[(2, "deployed"), (3, "pending-upgrade")]);
        assert_eq!(
            judge(&running, Some(3), false, None, None),
            Verdict::Waiting
        );
    }

    #[test]
    fn a_new_job_that_gave_up_before_a_revision_fails_and_the_old_one_does_not() {
        let before = history(&[(1, "deployed")]);
        let failed =
            |uid: &str| json!({"metadata": {"uid": uid}, "status": {"failed": 5, "active": 0}});
        let stale = json!("old");
        assert_eq!(
            judge(&before, Some(1), true, Some(&failed("new")), Some(&stale)),
            Verdict::Failed
        );
        assert_eq!(
            judge(&before, Some(1), true, Some(&failed("old")), Some(&stale)),
            Verdict::Waiting
        );
        let retrying = json!({"metadata": {"uid": "new"}, "status": {"failed": 2, "active": 1}});
        assert_eq!(
            judge(&before, Some(1), true, Some(&retrying), Some(&stale)),
            Verdict::Waiting
        );
    }

    /// A `kubectl` for a release `zot` with the history in `history`: revision 1 ran chart 0.1.60
    /// with 20Gi, revision 2 failed, helm Jobs succeed, no install Pod runs, and each applied
    /// object is kept in `applied`.
    const FAKE: &str = r#"#!/bin/sh
dir=$(dirname "${1#--kubeconfig=}")
shift
case "$*" in
"get secrets --namespace zot --selector=owner=helm,name=zot"*) cat "$dir/history" ;;
"get secret sh.helm.release.v1.zot.v1 "*) cat "$dir/revision.json" ;;
"get secret sh.helm.release.v1.zot.v2 "*) cat "$dir/failed.json" ;;
"get job helm-install-zot "*) echo '{"metadata":{"uid":"i"},"spec":{"template":{"spec":{"containers":[{"name":"helm"}]}}}}' ;;
"get job zeabur-helm-zot "*) echo '{"status":{"succeeded":1}}' ;;
apply*) cat >> "$dir/applied"; echo >> "$dir/applied" ;;
esac
"#;

    fn secret(release: &Value) -> Value {
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(release.to_string().as_bytes()).unwrap();
        let helm = STANDARD.encode(gzip.finish().unwrap());
        json!({"data": {"release": STANDARD.encode(helm)}})
    }

    fn fake(dir: &Path, history: &str) -> Kubectl {
        fs::write(dir.join("history"), history).unwrap();
        let binary = dir.join("kubectl");
        fs::write(&binary, FAKE).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let deployed = json!({
            "info": {"status": "deployed"},
            "chart": {"metadata": {"name": "zot", "version": "0.1.60"}},
            "config": {"pvc": {"storage": "20Gi"}},
        });
        fs::write(dir.join("revision.json"), secret(&deployed).to_string()).unwrap();
        let failed = json!({"info": {"status": "failed", "description": "spec: Forbidden"}});
        fs::write(dir.join("failed.json"), secret(&failed).to_string()).unwrap();
        Kubectl::new(&binary, dir.join("kubeconfig"))
    }

    fn helm_chart(version: &str, storage: &str) -> Value {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Chart.yaml"),
            format!("name: zot\nversion: {version}\n"),
        )
        .unwrap();
        let archive = pack_chart(dir.path(), "zot").unwrap();
        json!({
            "apiVersion": "helm.cattle.io/v1",
            "kind": "HelmChart",
            "metadata": {"name": "zot", "namespace": "kube-system", "uid": "u"},
            "spec": {
                "chartContent": STANDARD.encode(archive),
                "targetNamespace": "zot",
                "valuesContent": json!({"pvc": {"storage": storage}}).to_string(),
            },
        })
    }

    fn applied(dir: &Path, kind: &str) -> Vec<Value> {
        let text = fs::read_to_string(dir.join("applied")).unwrap_or_default();
        let mut found = Vec::new();
        for line in text.lines() {
            let object: Value = serde_json::from_str(line).unwrap();
            if object["kind"] == kind {
                found.push(object);
            }
        }
        found
    }

    #[test]
    fn giving_up_puts_back_a_chart_with_the_deployed_revisions_values() {
        let dir = tempfile::tempdir().unwrap();
        let kubectl = fake(dir.path(), "1 deployed\n2 failed\n");
        let previous = helm_chart("0.1.60", "50Gi");
        let new = helm_chart("0.1.61", "50Gi");
        let error = give_up(&kubectl, ZOT, Some(&previous), &new, "spec: Forbidden");

        let jobs = applied(dir.path(), "Job");
        let args = &jobs[0]["spec"]["template"]["spec"]["containers"][0]["args"];
        assert_eq!(args, &json!(["rollback", "zot", "1", "--namespace", "zot"]));
        let charts = applied(dir.path(), "HelmChart");
        let restored = charts.last().unwrap();
        let values: Value =
            serde_json::from_str(restored["spec"]["valuesContent"].as_str().unwrap()).unwrap();
        assert_eq!(values, json!({"pvc": {"storage": "20Gi"}}));
        assert_eq!(
            restored["spec"]["chartContent"],
            previous["spec"]["chartContent"]
        );
        let text = error.to_string();
        assert!(text.contains("rolled back to revision 1"), "{text}");
        assert!(!text.contains("still holds"), "{text}");
    }

    #[test]
    fn giving_up_leaves_the_chart_alone_when_none_matches_the_revision() {
        let dir = tempfile::tempdir().unwrap();
        let kubectl = fake(dir.path(), "1 deployed\n2 failed\n");
        let previous = helm_chart("0.1.59", "50Gi");
        let new = helm_chart("0.1.61", "50Gi");
        let error = give_up(&kubectl, ZOT, Some(&previous), &new, "spec: Forbidden");

        assert_eq!(applied(dir.path(), "HelmChart"), Vec::<Value>::new());
        let text = error.to_string();
        assert!(
            text.contains("still holds the upgrade that failed"),
            "{text}"
        );
        assert!(text.contains("0.1.60"), "{text}");
    }

    #[test]
    fn a_release_left_pending_is_said_in_one_line_with_the_progress() {
        let dir = tempfile::tempdir().unwrap();
        let kubectl = fake(dir.path(), "1 deployed\n2 pending-upgrade\n");
        let mut rerun = None;
        let lines = out::capture(|| rerun = Some(prepare(&kubectl, ZOT).unwrap()));
        assert_eq!(rerun, Some(true));
        assert_eq!(
            lines,
            [(
                true,
                "    zot was left pending-upgrade; its install Job marks it failed and upgrades it again"
                    .to_owned()
            )]
        );
    }

    #[test]
    fn a_retry_is_said_in_one_line_with_the_progress() {
        let dir = tempfile::tempdir().unwrap();
        let kubectl = fake(dir.path(), "1 deployed\n2 failed\n");
        let mut retried = false;
        let mut result = None;
        let lines = out::capture(|| result = Some(retry_once(&kubectl, ZOT, &mut retried)));
        assert_eq!(result, Some(Ok((Some(2), Some(json!("i"))))));
        assert_eq!(
            lines,
            [(
                true,
                "    zot did not upgrade (spec: Forbidden); trying once more".to_owned()
            )]
        );
        let calls = applied(dir.path(), "Job");
        let args = &calls[0]["spec"]["template"]["spec"]["containers"][0]["args"];
        assert_eq!(args, &json!(["rollback", "zot", "1", "--namespace", "zot"]));
    }
}
