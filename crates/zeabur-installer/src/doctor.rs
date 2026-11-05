//! `doctor`: checks the install, with the exit codes of the Nagios plugin convention, as
//! `zeaburctl doctor` uses them: 0 OK, 1 warning, 2 critical, 3 when the checks cannot run.

use std::process::ExitCode;
use std::time::Duration;

use serde_json::Value;
use zeabur_installer_host::kubectl::Kubectl;
use zeabur_installer_host::nodeprep::{self, Stage};
use zeabur_installer_host::preflight::{self, Encryption, Status};
use zeabur_installer_host::{facts, systemd};
use zeabur_installer_plan::catalog::Health;
use zeabur_installer_plan::cluster::Distribution;
use zeabur_installer_plan::plan::{IDENTITY_NAMESPACE, ZEABUR_NAMESPACE};
use zeabur_installer_plan::storage;

use crate::apply::STATE_CONFIGMAP;
use crate::out;

/// The exit code when the checks cannot run.
const UNKNOWN: u8 = 3;

/// Stops before the checks when they cannot read what they need.
pub(crate) fn cannot_run() -> Option<ExitCode> {
    if preflight::is_root() {
        return None;
    }
    out::line("UNKNOWN  run doctor as root (sudo): the checks read the cluster's files");
    Some(ExitCode::from(UNKNOWN))
}

/// Whether the network policies let in the traffic of every node.
fn policies(kubectl: &Kubectl) -> (Status, String) {
    match crate::node::policies_behind(kubectl) {
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
    }
}

pub(crate) fn run() -> ExitCode {
    if let Some(code) = cannot_run() {
        return code;
    }
    let mut results: Vec<(Status, String)> = Vec::new();
    let distribution = crate::node::server_distribution().unwrap_or_default();
    let unit = distribution.server_unit();
    if !systemd::is_active(unit) {
        out::line(&format!(
            "CRITICAL {unit} is not running: journalctl -u {unit}"
        ));
        return ExitCode::from(2);
    }
    let kubectl = Kubectl::admin(distribution);
    if kubectl.wait_ready(Duration::from_secs(30)).is_err() {
        out::line("CRITICAL the Kubernetes API does not answer /readyz");
        return ExitCode::from(2);
    }
    results.push((
        Status::Pass,
        format!("{unit} is running and the API is ready"),
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
                results.extend(longhorn_nodes(&kubectl));
            }
            if plan.get("zeabur").is_some_and(|z| !z.is_null()) {
                results.push(deployment(&kubectl, IDENTITY_NAMESPACE, "zeabur-identity"));
                for name in IDENTITY_CERTIFICATES {
                    let certificate = kubectl.get(
                        "certificates.cert-manager.io",
                        name,
                        Some(IDENTITY_NAMESPACE),
                    );
                    results.push(identity_certificate(
                        name,
                        certificate.ok().flatten().as_ref(),
                    ));
                }
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
        results.push(crate::join::node_addresses(&kubectl, &host.own_node_name()));
    }
    results.push(policies(&kubectl));
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
    let encryption_config = format!(
        "/var/lib/rancher/{}/server/cred/encryption-config.json",
        distribution.id()
    );
    results.push(secrets_encryption(
        distribution,
        &encryption_config,
        std::fs::read_to_string(&encryption_config).ok().as_deref(),
    ));

    report(&results)
}

/// Whether kube-apiserver writes Secrets to its datastore encrypted, from the encryption
/// configuration K3s or RKE2 writes at `path`: the first provider for `secrets` is the one it
/// writes with, and `identity` writes them as they are.
fn secrets_encryption(
    distribution: Distribution,
    path: &str,
    config: Option<&str>,
) -> (Status, String) {
    let Some(config) = config else {
        return (
            Status::Fail,
            format!("{path} is missing: Secrets are stored unencrypted"),
        );
    };
    let config: Value = serde_json::from_str(config).unwrap_or_default();
    let writer = config
        .get("resources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|r| {
            r.get("resources")
                .and_then(Value::as_array)
                .is_some_and(|names| names.iter().any(|n| n == "secrets"))
        })
        .and_then(|r| r.get("providers")?.get(0))
        .and_then(Value::as_object)
        .and_then(|p| p.keys().next().cloned());
    match writer.as_deref() {
        Some("identity") | None => (
            Status::Fail,
            format!(
                "Secrets are stored unencrypted: {} secrets-encrypt status",
                distribution.id()
            ),
        ),
        Some(provider) => (
            Status::Pass,
            format!("Secrets are stored encrypted ({provider})"),
        ),
    }
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

/// The component that keeps a Node object for every node it has met.
const LONGHORN: &str = "replicated-storage/longhorn";

/// The line for a component whose checks pass. It names only what was checked: its workloads are
/// reported under their namespace, so they may still be unavailable.
fn passed(id: &str, component: &Value, checks: &[Health]) -> String {
    let mut line = format!("{id}: charts installed");
    if !checks.is_empty() {
        line.push_str(", health checks pass");
    }
    if component.get("untested") == Some(&Value::Bool(true)) {
        line.push_str(" (untested)");
    }
    line
}

/// Each component's `HelmCharts` installed and the catalog's health checks in the install record
/// met, then the Deployments and `DaemonSets` of every component namespace available. Components
/// share namespaces, so a workload is reported once under its namespace, not under a component.
pub(crate) fn components(kubectl: &Kubectl, plan: &Value) -> Vec<(Status, String)> {
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
        let checks = component
            .get("health")
            .and_then(|h| serde_json::from_value::<Vec<Health>>(h.clone()).ok());
        if checks.is_none() {
            results.push((
                Status::Warn,
                format!(
                    "{id}: the install record has no health checks, because an older \
                     zeabur-installer wrote it. Run `zeabur-installer apply` to record them."
                ),
            ));
        }
        let failed = failed_checks(
            kubectl,
            id,
            checks.as_deref().unwrap_or_default(),
            &mut results,
        );
        if checks.is_some() && problems.is_empty() && failed.is_empty() {
            results.push((
                Status::Pass,
                passed(id, component, checks.as_deref().unwrap_or_default()),
            ));
        }
        if !problems.is_empty() {
            results.push((Status::Fail, format!("{id}: {}", problems.join("; "))));
        }
        results.extend(
            failed
                .into_iter()
                .map(|failure| (Status::Fail, format!("{id}: {failure}"))),
        );
    }
    for ns in namespaces(plan) {
        let args = [
            "get",
            "deployments,daemonsets",
            "--namespace",
            ns,
            "--output=json",
        ];
        match kubectl.run(&args, None) {
            Ok(out) => {
                let list = serde_json::from_str::<Value>(&out).unwrap_or_default();
                for workload in unavailable(&list) {
                    results.push((
                        Status::Fail,
                        format!("{ns}: {workload} (kubectl get pods -n {ns})"),
                    ));
                }
            }
            Err(e) => results.push((Status::Fail, format!("reading workloads in {ns}: {e}"))),
        }
    }
    results
}

/// The namespaces the components' releases install into, each once. `kube-system` is left out:
/// K3s runs its own workloads there.
fn namespaces(plan: &Value) -> Vec<&str> {
    let mut found = Vec::new();
    for component in plan
        .get("components")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for release in component
            .get("releases")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let namespace = release
                .get("namespace")
                .or_else(|| release.pointer("/object/spec/targetNamespace"))
                .and_then(Value::as_str);
            if let Some(ns) = namespace
                && ns != "kube-system"
                && !found.contains(&ns)
            {
                found.push(ns);
            }
        }
    }
    found
}

/// The catalog's health checks of a component that fail. A Longhorn node whose Kubernetes node is
/// gone is a warning of its own, and the `nodes-ready` check then covers the other nodes only.
fn failed_checks(
    kubectl: &Kubectl,
    id: &str,
    checks: &[Health],
    results: &mut Vec<(Status, String)>,
) -> Vec<String> {
    let list = if id == LONGHORN {
        longhorn_node_list(kubectl).ok()
    } else {
        None
    };
    let not_ready = list.as_ref().map(not_ready_longhorn_nodes);
    if let Some(not_ready) = &not_ready
        && !not_ready.gone.is_empty()
    {
        results.push((Status::Warn, removed_longhorn_nodes_advice(&not_ready.gone)));
    }
    failed_health(checks, |check, timeout| match &not_ready {
        Some(not_ready) if check.name == "nodes-ready" && !not_ready.gone.is_empty() => {
            not_ready.other.is_empty()
        }
        _ => wait_health(check, timeout, |args| kubectl.wait(args)).is_ok(),
    })
}

/// The catalog's health checks that fail, each with its advice and the wait that failed.
fn failed_health(checks: &[Health], wait: impl Fn(&Health, &str) -> bool) -> Vec<String> {
    checks
        .iter()
        .filter_map(|check| {
            let timeout = format!("--timeout={}", check.timeout);
            let args = check.args(&timeout);
            (!wait(check, &timeout)).then(|| {
                format!(
                    "{}: {} To check again: kubectl wait {}",
                    check.name,
                    check.advice,
                    args.join(" ")
                )
            })
        })
        .collect()
}

/// Runs a health check: waits for its objects to exist, then for its condition.
pub(crate) fn wait_health<E>(
    check: &Health,
    timeout: &str,
    mut wait: impl FnMut(&[&str]) -> Result<(), E>,
) -> Result<(), E> {
    wait(&check.create_args(timeout))?;
    wait(&check.args(timeout))
}

/// The Deployments and `DaemonSets` in a list that have fewer Pods available than they want, or
/// a Deployment scaled to 0, as text.
fn unavailable(list: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for item in list
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let kind = item.get("kind").and_then(Value::as_str).unwrap_or_default();
        let name = item
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let count = |path: &str| item.pointer(path).and_then(Value::as_u64);
        let (want, have) = if kind == "DaemonSet" {
            (
                count("/status/desiredNumberScheduled").unwrap_or(0),
                count("/status/numberAvailable").unwrap_or(0),
            )
        } else {
            (
                count("/spec/replicas").unwrap_or(1),
                count("/status/availableReplicas").unwrap_or(0),
            )
        };
        if kind != "DaemonSet" && want == 0 {
            found.push(format!("deployment/{name} is scaled to 0"));
        } else if have < want {
            found.push(format!(
                "{}/{name} has {have} of {want} Pods available",
                kind.to_ascii_lowercase()
            ));
        }
    }
    found
}

/// The nodes whose Longhorn checks of iSCSI, NFS or kernel modules fail, as Longhorn's Node
/// objects report them.
fn longhorn_nodes(kubectl: &Kubectl) -> Vec<(Status, String)> {
    match longhorn_node_list(kubectl) {
        Ok(list) => unprepared_nodes(&list)
            .into_iter()
            .map(|n| (Status::Fail, n))
            .collect(),
        Err(e) => vec![(Status::Warn, format!("reading Longhorn's nodes: {e}"))],
    }
}

/// Longhorn's Node objects that are not ready, sorted by why.
#[derive(Default)]
struct NotReadyNodes {
    /// The Kubernetes node is gone (reason `KubernetesNodeGone`).
    gone: Vec<String>,
    /// Any other reason.
    other: Vec<String>,
}

/// Reads each Longhorn node's Ready condition once.
fn not_ready_longhorn_nodes(list: &Value) -> NotReadyNodes {
    let mut found = NotReadyNodes::default();
    for node in list
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = node.pointer("/metadata/name").and_then(Value::as_str) else {
            continue;
        };
        let mut ready = false;
        let mut gone = false;
        for condition in node
            .pointer("/status/conditions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if condition.get("type").and_then(Value::as_str) == Some("Ready") {
                ready = condition.get("status").and_then(Value::as_str) == Some("True");
                gone =
                    condition.get("reason").and_then(Value::as_str) == Some("KubernetesNodeGone");
            }
        }
        if gone {
            found.gone.push(name.to_owned());
        } else if !ready {
            found.other.push(name.to_owned());
        }
    }
    found
}

fn removed_longhorn_nodes_advice(names: &[String]) -> String {
    format!(
        "Longhorn still lists nodes that Kubernetes has removed: {}. Deleting them fails while \
         they still hold replicas, so follow Add and maintain nodes → Remove a node first, then \
         run: kubectl -n longhorn-system delete nodes.longhorn.io {}",
        names.join(", "),
        names.join(" ")
    )
}

fn longhorn_node_list(kubectl: &Kubectl) -> Result<Value, String> {
    let out = kubectl
        .run(
            &[
                "get",
                "nodes.longhorn.io",
                "-n",
                "longhorn-system",
                "--output=json",
            ],
            None,
        )
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&out).map_err(|e| e.to_string())
}

/// Longhorn's node conditions that a node's preparation makes true.
const LONGHORN_PREP_CONDITIONS: [&str; 3] = [
    "RequiredPackages",
    "KernelModulesLoaded",
    "NFSClientInstalled",
];

/// The Longhorn Node objects in a list with a preparation condition that is not true, as text.
fn unprepared_nodes(list: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for node in list
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = node
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let failing: Vec<&str> = node
            .pointer("/status/conditions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|c| {
                c.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| LONGHORN_PREP_CONDITIONS.contains(&t))
                    && c.get("status").and_then(Value::as_str) == Some("False")
            })
            .map(|c| c.get("message").and_then(Value::as_str).unwrap_or_default())
            .collect();
        if !failing.is_empty() {
            found.push(format!(
                "node {name} is not prepared for Longhorn ({}): run zeabur-installer apply on it, \
                 then restart its Longhorn manager, which only checks the node again when it \
                 starts: kubectl -n longhorn-system delete pod -l app=longhorn-manager \
                 --field-selector spec.nodeName={name}",
                failing.join("; ")
            ));
        }
    }
    found
}

/// Whether the cluster's default storage class is the one the plan chose, and the only one.
pub(crate) fn default_class(kubectl: &Kubectl, plan: &Value) -> (Status, String) {
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

/// The root CA kube-apiserver trusts and the certificate Zeabur Identity's internal listener
/// serves. cert-manager renews both.
const IDENTITY_CERTIFICATES: [&str; 2] = [
    "zeabur-identity-internal-ca",
    "zeabur-identity-internal-tls",
];

/// Whether cert-manager reports the Certificate `Ready`. Without it, kube-apiserver stops
/// accepting new tokens once the certificate expires.
fn identity_certificate(name: &str, certificate: Option<&Value>) -> (Status, String) {
    let target = format!("{IDENTITY_NAMESPACE}/{name}");
    let Some(certificate) = certificate else {
        return (
            Status::Warn,
            format!("certificate {target} does not exist: run zeabur-installer apply"),
        );
    };
    let ready = certificate
        .pointer("/status/conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|c| c.get("type").and_then(Value::as_str) == Some("Ready"));
    let field = |key: &str| {
        ready
            .and_then(|c| c.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    if field("status") == "True" {
        let renewal = certificate
            .pointer("/status/renewalTime")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return (
            Status::Pass,
            format!("certificate {target} is ready; cert-manager renews it at {renewal}"),
        );
    }
    (
        Status::Fail,
        format!(
            "certificate {target} is not ready ({}): kubectl describe -n {IDENTITY_NAMESPACE} certificate {name}",
            field("message")
        ),
    )
}

pub(crate) fn deployment(kubectl: &Kubectl, namespace: &str, name: &str) -> (Status, String) {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn secrets_are_encrypted_only_when_the_first_provider_is_not_identity() {
        let rke2 = r#"{"kind":"EncryptionConfiguration","apiVersion":"apiserver.config.k8s.io/v1",
            "resources":[{"resources":["secrets"],
            "providers":[{"aescbc":{"keys":[{"name":"aescbckey","secret":"x"}]}},{"identity":{}}]}]}"#;
        assert_eq!(
            secrets_encryption(Distribution::Rke2, "", Some(rke2)).0,
            Status::Pass
        );
        let identity = rke2.replacen("aescbc", "identity", 1);
        assert_eq!(
            secrets_encryption(Distribution::K3s, "", Some(&identity)).0,
            Status::Fail
        );
        assert_eq!(
            secrets_encryption(Distribution::K3s, "", Some("{}")).0,
            Status::Fail
        );
        assert_eq!(
            secrets_encryption(Distribution::K3s, "", None).0,
            Status::Fail
        );
    }

    #[test]
    fn workloads_with_fewer_pods_available_than_wanted_are_listed() {
        let list = json!({"items": [
            {"kind": "Deployment", "metadata": {"name": "moco-controller"},
             "spec": {"replicas": 2}, "status": {}},
            {"kind": "Deployment", "metadata": {"name": "ready"},
             "spec": {"replicas": 1}, "status": {"availableReplicas": 1}},
            {"kind": "Deployment", "metadata": {"name": "off"},
             "spec": {"replicas": 0}, "status": {}},
            {"kind": "DaemonSet", "metadata": {"name": "agent"},
             "status": {"desiredNumberScheduled": 3, "numberAvailable": 2}},
        ]});
        assert_eq!(
            unavailable(&list),
            [
                "deployment/moco-controller has 0 of 2 Pods available",
                "deployment/off is scaled to 0",
                "daemonset/agent has 2 of 3 Pods available"
            ]
        );
    }

    #[test]
    fn a_passing_component_names_only_what_was_checked() {
        let check = Health {
            name: "nodes-ready".to_owned(),
            wait: Vec::new(),
            timeout: "60s".to_owned(),
            advice: String::new(),
        };
        assert_eq!(
            passed("metrics/victoria-metrics", &json!({}), &[]),
            "metrics/victoria-metrics: charts installed"
        );
        assert_eq!(
            passed(
                "replicated-storage/longhorn",
                &json!({"untested": true}),
                &[check]
            ),
            "replicated-storage/longhorn: charts installed, health checks pass (untested)"
        );
    }

    #[test]
    fn a_namespace_shared_by_components_has_its_workloads_read_once() {
        let plan = json!({"components": [
            {"id": "metrics/victoria-metrics", "releases": [
                {"kind": "helmChart", "name": "vm", "namespace": "observability"},
            ]},
            {"id": "logs/victoria-logs", "releases": [
                {"kind": "helmChart", "name": "vl", "namespace": "observability"},
                {"kind": "manifest", "object": {"spec": {"targetNamespace": "logs-extra"}}},
            ]},
            {"id": "ingress/traefik", "releases": [
                {"kind": "helmChart", "name": "traefik", "namespace": "kube-system"},
            ]},
        ]});
        assert_eq!(namespaces(&plan), ["observability", "logs-extra"]);
    }

    #[test]
    fn a_longhorn_node_without_its_packages_is_named_with_longhorns_message() {
        let node = |name: &str, conditions: Value| json!({"metadata": {"name": name}, "status": {"conditions": conditions}});
        let list = json!({"items": [
            node("vps-1", json!([
                {"type": "Ready", "status": "True"},
                {"type": "RequiredPackages", "status": "True"},
                {"type": "NFSClientInstalled", "status": "True"},
            ])),
            node("vps-2", json!([
                {"type": "Ready", "status": "True"},
                {"type": "RequiredPackages", "status": "False",
                 "message": "Missing packages: [nfs-common open-iscsi]"},
                {"type": "KernelModulesLoaded", "status": "False",
                 "message": "Kernel modules [iscsi_tcp] are not loaded"},
            ])),
            node("vps-3", json!([{"type": "Ready", "status": "False"}])),
        ]});
        assert_eq!(
            unprepared_nodes(&list),
            [
                "node vps-2 is not prepared for Longhorn (Missing packages: [nfs-common \
                 open-iscsi]; Kernel modules [iscsi_tcp] are not loaded): run zeabur-installer \
                 apply on it, then restart its Longhorn manager, which only checks the node \
                 again when it starts: kubectl -n longhorn-system delete pod -l \
                 app=longhorn-manager --field-selector spec.nodeName=vps-2"
            ]
        );
    }

    #[test]
    fn a_longhorn_node_that_kubernetes_removed_is_told_apart_from_one_not_ready() {
        let node = |name: &str, status: &str, reason: &str| {
            json!({"metadata": {"name": name}, "status": {"conditions": [
                {"type": "Ready", "status": status, "reason": reason},
            ]}})
        };
        let list = json!({"items": [
            node("vps-1", "True", ""),
            node("vps-2", "False", "KubernetesNodeGone"),
            node("vps-3", "False", "KubernetesNodeNotReady"),
        ]});
        let not_ready = not_ready_longhorn_nodes(&list);
        assert_eq!(not_ready.gone, ["vps-2"]);
        assert_eq!(not_ready.other, ["vps-3"]);
        let advice = removed_longhorn_nodes_advice(&not_ready.gone);
        assert!(advice.contains("nodes.longhorn.io vps-2"), "{advice}");
    }

    #[test]
    fn a_pod_health_check_leaves_out_pods_that_have_ended() {
        let check = Health {
            name: "ready".to_owned(),
            wait: ["--for=condition=Ready", "pod", "-l", "app=web", "-n", "web"]
                .map(str::to_owned)
                .to_vec(),
            timeout: "60s".to_owned(),
            advice: String::new(),
        };
        let mut calls: Vec<Vec<String>> = Vec::new();
        let result = wait_health(&check, "--timeout=60s", |args| {
            calls.push(args.iter().map(|&a| a.to_owned()).collect());
            Ok::<(), ()>(())
        });
        assert_eq!(result, Ok(()));
        for call in calls {
            assert!(
                call.contains(
                    &"--field-selector=status.phase!=Succeeded,status.phase!=Failed".to_owned()
                ),
                "{call:?}"
            );
        }
    }

    #[test]
    fn a_check_on_other_objects_has_no_pod_field_selector() {
        let check = Health {
            name: "ready".to_owned(),
            wait: ["--for=condition=Available", "deployment/web", "-n", "web"]
                .map(str::to_owned)
                .to_vec(),
            timeout: "60s".to_owned(),
            advice: String::new(),
        };
        for arg in check.args("--timeout=60s") {
            assert!(!arg.starts_with("--field-selector"), "{arg}");
        }
    }

    #[test]
    fn a_health_check_waits_for_its_objects_to_exist_before_its_condition() {
        let check = Health {
            name: "nodes-ready".to_owned(),
            wait: [
                "--for=condition=Ready",
                "nodes.longhorn.io",
                "--all",
                "-n",
                "longhorn-system",
            ]
            .map(str::to_owned)
            .to_vec(),
            timeout: "60s".to_owned(),
            advice: String::new(),
        };
        let mut calls: Vec<Vec<String>> = Vec::new();
        let result = wait_health(&check, "--timeout=60s", |args| {
            calls.push(args.iter().map(|&a| a.to_owned()).collect());
            Ok::<(), ()>(())
        });
        assert_eq!(result, Ok(()));
        assert_eq!(
            calls,
            [
                [
                    "--for=create",
                    "nodes.longhorn.io",
                    "--all",
                    "-n",
                    "longhorn-system",
                    "--timeout=60s"
                ],
                [
                    "--for=condition=Ready",
                    "nodes.longhorn.io",
                    "--all",
                    "-n",
                    "longhorn-system",
                    "--timeout=60s"
                ],
            ]
        );

        let mut conditions = false;
        let result = wait_health(&check, "--timeout=60s", |args| {
            conditions |= args[0] != "--for=create";
            Err(())
        });
        assert_eq!(result, Err(()));
        assert!(
            !conditions,
            "the condition is not waited for when no object appeared"
        );
    }

    #[test]
    fn a_failing_health_check_gives_the_catalogs_advice_and_its_wait() {
        let check = |name: &str, target: &str| Health {
            name: name.to_owned(),
            wait: ["--for=condition=Ready", target, "-n", "longhorn-system"]
                .map(str::to_owned)
                .to_vec(),
            timeout: "45s".to_owned(),
            advice: "Add longhorn: true under join and run zeabur-installer apply.".to_owned(),
        };
        let checks = [
            check("manager", "deployment/manager"),
            check("nodes-ready", "nodes.longhorn.io"),
        ];
        assert_eq!(failed_health(&checks, |_, _| true), Vec::<String>::new());
        let failed = failed_health(&checks, |check, _| check.name != "nodes-ready");
        assert_eq!(
            failed,
            [
                "nodes-ready: Add longhorn: true under join and run zeabur-installer apply. To check \
              again: kubectl wait --for=condition=Ready nodes.longhorn.io -n longhorn-system \
              --timeout=45s"
            ]
        );
    }

    #[test]
    fn identity_certificate_passes_only_when_cert_manager_reports_ready() {
        let certificate = |conditions: Value| json!({"status": {"conditions": conditions}});
        let cases = [
            (
                "ready",
                Some(certificate(json!([{"type": "Ready", "status": "True"}]))),
                Status::Pass,
            ),
            (
                "issuing failed",
                Some(certificate(
                    json!([{"type": "Ready", "status": "False", "message": "issuer not ready"}]),
                )),
                Status::Fail,
            ),
            ("no status yet", Some(json!({})), Status::Fail),
            ("not created by this release", None, Status::Warn),
        ];
        for (case, object, status) in cases {
            let (got, detail) =
                identity_certificate("zeabur-identity-internal-tls", object.as_ref());
            assert_eq!(got, status, "{case}: {detail}");
        }
    }
}
