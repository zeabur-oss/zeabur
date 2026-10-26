//! `doctor`: checks the install, with the exit codes of the Nagios plugin convention, as
//! `zeaburctl doctor` uses them: 0 OK, 1 warning, 2 critical, 3 when the checks cannot run.

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

/// The exit code when the checks cannot run.
const UNKNOWN: u8 = 3;

/// Stops before the checks when they cannot read what they need.
pub(crate) fn cannot_run() -> Option<ExitCode> {
    if preflight::is_root() {
        return None;
    }
    out::line("UNKNOWN  run doctor as root (sudo): the checks read RKE2's files");
    Some(ExitCode::from(UNKNOWN))
}

pub(crate) fn run() -> ExitCode {
    if let Some(code) = cannot_run() {
        return code;
    }
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
    results.push(secrets_encryption(
        std::fs::read_to_string(ENCRYPTION_CONFIG).ok().as_deref(),
    ));

    report(&results)
}

/// The kube-apiserver encryption configuration RKE2 writes.
const ENCRYPTION_CONFIG: &str = "/var/lib/rancher/rke2/server/cred/encryption-config.json";

/// Whether kube-apiserver writes Secrets to etcd encrypted: the first provider for `secrets` is the
/// one it writes with, and `identity` writes them as they are.
fn secrets_encryption(config: Option<&str>) -> (Status, String) {
    let Some(config) = config else {
        return (
            Status::Fail,
            format!("{ENCRYPTION_CONFIG} is missing: Secrets in etcd are not encrypted"),
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
            "Secrets in etcd are not encrypted: rke2 secrets-encrypt status".to_owned(),
        ),
        Some(provider) => (
            Status::Pass,
            format!("Secrets in etcd are encrypted ({provider})"),
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

/// Each component's `HelmCharts` installed, its Deployments and `DaemonSets` available and its
/// smoke waits met.
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
        let mut namespaces: Vec<&str> = Vec::new();
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
                && !namespaces.contains(&ns)
            {
                namespaces.push(ns);
            }
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
        for ns in namespaces {
            match kubectl.run(
                &[
                    "get",
                    "deployments,daemonsets",
                    "--namespace",
                    ns,
                    "--output=json",
                ],
                None,
            ) {
                Ok(out) => {
                    let list = serde_json::from_str::<Value>(&out).unwrap_or_default();
                    problems.extend(
                        unavailable(&list)
                            .into_iter()
                            .map(|w| format!("{ns}: {w} (kubectl get pods -n {ns})")),
                    );
                }
                Err(e) => problems.push(format!("reading workloads in {ns}: {e}")),
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

/// The Deployments and `DaemonSets` in a list that have fewer Pods available than they want, as
/// text.
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
        if have < want {
            found.push(format!(
                "{}/{name} has {have} of {want} Pods available",
                kind.to_ascii_lowercase()
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
        assert_eq!(secrets_encryption(Some(rke2)).0, Status::Pass);
        let identity = rke2.replacen("aescbc", "identity", 1);
        assert_eq!(secrets_encryption(Some(&identity)).0, Status::Fail);
        assert_eq!(secrets_encryption(Some("{}")).0, Status::Fail);
        assert_eq!(secrets_encryption(None).0, Status::Fail);
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
                "daemonset/agent has 2 of 3 Pods available"
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
