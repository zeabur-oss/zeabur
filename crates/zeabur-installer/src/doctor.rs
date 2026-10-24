//! `doctor`: checks the install, with exit codes monitoring plugins use (0 OK, 1 warning,
//! 2 critical).

use std::process::ExitCode;
use std::time::Duration;

use serde_json::Value;
use zeabur_installer_host::kubectl::Kubectl;
use zeabur_installer_host::nodeprep::{self, Stage};
use zeabur_installer_host::preflight::{self, Encryption, Status};
use zeabur_installer_host::{facts, systemd};
use zeabur_installer_plan::plan::{IDENTITY_NAMESPACE, ZEABUR_NAMESPACE};
use zeabur_installer_plan::storage;

use crate::apply::STATE_CONFIGMAP;
use crate::out;

pub(crate) fn run() -> ExitCode {
    let mut results: Vec<(Status, String)> = Vec::new();
    if !systemd::is_active("rke2-server") {
        out::line("CRITICAL rke2-server is not running: journalctl -u rke2-server");
        return ExitCode::from(2);
    }
    let kubectl = Kubectl::rke2();
    if kubectl.wait_ready(Duration::from_secs(30)).is_err() {
        out::line("CRITICAL the Kubernetes API does not answer /readyz");
        return ExitCode::from(2);
    }
    results.push((
        Status::Pass,
        "rke2-server is running and the API is ready".to_owned(),
    ));

    match kubectl.get("configmap", STATE_CONFIGMAP, Some("kube-system")) {
        Ok(Some(state)) => {
            let plan = state
                .pointer("/data/plan")
                .and_then(Value::as_str)
                .and_then(|p| serde_json::from_str::<Value>(p).ok())
                .unwrap_or_default();
            results.extend(components(&kubectl, &plan));
            results.push(default_class(&kubectl, &plan));
            if plan.get("nodePrep").is_some_and(|p| !p.is_null()) {
                results.extend(
                    nodeprep::check(&storage::LONGHORN_PREP, Stage::After)
                        .into_iter()
                        .filter(|c| c.status != Status::Pass)
                        .map(|c| (c.status, c.detail)),
                );
                results.push(longhorn_nodes(&kubectl));
            }
            if plan.get("zeabur").is_some_and(|z| !z.is_null()) {
                results.push(deployment(&kubectl, IDENTITY_NAMESPACE, "zeabur-identity"));
                results.push(deployment(&kubectl, ZEABUR_NAMESPACE, "zeabur"));
            }
        }
        Ok(None) => results.push((
            Status::Fail,
            "no record of an install: run zeabur-installer apply".to_owned(),
        )),
        Err(e) => results.push((Status::Fail, format!("reading the install record: {e}"))),
    }
    if let Ok(host) = facts::read() {
        results.push(crate::join::node_addresses(
            &kubectl,
            &host.hostname.to_ascii_lowercase(),
        ));
    }
    results.push(match crate::node::policies_behind(&kubectl) {
        Ok(missing) if missing.is_empty() => (
            Status::Pass,
            "the network policies let every node's traffic in".to_owned(),
        ),
        Ok(missing) => (
            Status::Warn,
            format!(
                "the network policies do not let {} in yet: zeabur-installer node sync",
                missing.join(", ")
            ),
        ),
        Err(e) => (Status::Warn, format!("reading the network policies: {e}")),
    });
    let encryption = match preflight::disk_encryption("/var/lib/rancher") {
        Encryption::Encrypted => (Status::Pass, "/var/lib/rancher is encrypted"),
        Encryption::Plain => (
            Status::Warn,
            "/var/lib/rancher is not on an encrypted device",
        ),
        Encryption::Unknown => (
            Status::Warn,
            "could not tell whether /var/lib/rancher is encrypted",
        ),
    };
    results.push((encryption.0, encryption.1.to_owned()));

    report(&results)
}

/// Prints the results and gives the exit code of the worst.
pub(crate) fn report(results: &[(Status, String)]) -> ExitCode {
    let worst = results
        .iter()
        .map(|(s, _)| *s)
        .max_by_key(|s| rank(*s))
        .unwrap_or(Status::Pass);
    for (status, text) in results {
        let label = match status {
            Status::Pass => "OK      ",
            Status::Warn => "WARNING ",
            Status::Fail => "CRITICAL",
        };
        out::line(&format!("{label} {text}"));
    }
    ExitCode::from(rank(worst))
}

const fn rank(status: Status) -> u8 {
    match status {
        Status::Pass => 0,
        Status::Warn => 1,
        Status::Fail => 2,
    }
}

/// Each component's `HelmCharts` installed and its smoke waits met.
fn components(kubectl: &Kubectl, plan: &Value) -> Vec<(Status, String)> {
    let mut results = Vec::new();
    for component in plan
        .get("components")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = component
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut problems = Vec::new();
        for release in component
            .get("releases")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if release.get("kind").and_then(Value::as_str) != Some("helmChart") {
                continue;
            }
            let name = release
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Ok(Some(job)) =
                kubectl.get("job", &format!("helm-install-{name}"), Some("kube-system"))
                && job
                    .pointer("/status/succeeded")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    == 0
            {
                problems.push(format!("chart {name} is not installed (kubectl logs -n kube-system job/helm-install-{name})"));
            }
        }
        for wait in component
            .get("smoke")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let mut args: Vec<&str> = wait
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            args.push("--timeout=10s");
            if kubectl.wait(&args).is_err() {
                problems.push(format!("kubectl wait {} failed", args.join(" ")));
            }
        }
        if problems.is_empty() {
            results.push((Status::Pass, id.to_owned()));
        } else {
            results.push((Status::Fail, format!("{id}: {}", problems.join("; "))));
        }
    }
    results
}

/// Whether the cluster's default storage class is the one the plan chose, and the only one.
fn default_class(kubectl: &Kubectl, plan: &Value) -> (Status, String) {
    let Some(planned) = plan
        .pointer("/storage/defaultClass")
        .and_then(Value::as_str)
    else {
        return (
            Status::Pass,
            "storage classes not in the install record".to_owned(),
        );
    };
    let classes = match kubectl
        .run(&["get", "storageclass", "--output=json"], None)
        .map_err(|e| e.to_string())
        .and_then(|out| serde_json::from_str::<Value>(&out).map_err(|e| e.to_string()))
    {
        Ok(classes) => classes,
        Err(e) => return (Status::Warn, format!("reading storage classes: {e}")),
    };
    match storage::default_classes(&classes).as_slice() {
        [only] if only == planned => (
            Status::Pass,
            format!("{planned} is the default storage class"),
        ),
        [] => (
            Status::Warn,
            format!("no default storage class; the plan makes it {planned}"),
        ),
        defaults => (
            Status::Warn,
            format!(
                "the default storage class is {}; the plan makes it {planned}",
                defaults.join(" and ")
            ),
        ),
    }
}

/// Longhorn's own checks of each node: packages, modules, disk space.
fn longhorn_nodes(kubectl: &Kubectl) -> (Status, String) {
    let nodes = kubectl
        .run(
            &[
                "get",
                "nodes.longhorn.io",
                "--namespace=longhorn-system",
                "--output=json",
            ],
            None,
        )
        .map_err(|e| e.to_string())
        .and_then(|out| serde_json::from_str::<Value>(&out).map_err(|e| e.to_string()));
    match nodes {
        Ok(nodes) => {
            let problems = storage::longhorn_node_problems(&nodes);
            if problems.is_empty() {
                (Status::Pass, "Longhorn finds every node ready".to_owned())
            } else {
                (
                    Status::Warn,
                    format!(
                        "Longhorn: {}. On a node that joined without it, add longhorn: true \
                         under join in its configuration and run zeabur-installer apply there",
                        problems.join("; ")
                    ),
                )
            }
        }
        Err(e) => (Status::Warn, format!("reading Longhorn's nodes: {e}")),
    }
}

fn deployment(kubectl: &Kubectl, namespace: &str, name: &str) -> (Status, String) {
    let target = format!("deployment/{name}");
    match kubectl.wait(&[
        "--namespace",
        namespace,
        &target,
        "--for=condition=Available",
        "--timeout=10s",
    ]) {
        Ok(()) => (Status::Pass, format!("{namespace}/{name} is available")),
        Err(_) => (
            Status::Fail,
            format!(
                "{namespace}/{name} is not available: kubectl describe -n {namespace} {target}"
            ),
        ),
    }
}
