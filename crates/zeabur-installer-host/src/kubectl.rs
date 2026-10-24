//! The cluster, through RKE2's own `kubectl` and admin kubeconfig.
//!
//! A subprocess rather than a client library: the installer applies a handful of objects and
//! waits on them, and `kubectl` is already on the host, matched to the server.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use zeabur_installer_plan::plan::MANAGER;
use zeabur_installer_plan::rke2::{ADMIN_KUBECONFIG, BIN_DIR};

use crate::{HostError, cmd};

/// `kubectl` with a kubeconfig.
#[derive(Debug, Clone)]
pub struct Kubectl {
    binary: PathBuf,
    kubeconfig: PathBuf,
}

impl Kubectl {
    /// RKE2's `kubectl` with its admin kubeconfig.
    #[must_use]
    pub fn rke2() -> Self {
        Self {
            binary: PathBuf::from(BIN_DIR).join("kubectl"),
            kubeconfig: PathBuf::from(ADMIN_KUBECONFIG),
        }
    }

    /// A `kubectl` binary with another kubeconfig, such as the kubelet's on an agent.
    #[must_use]
    pub fn new(binary: impl Into<PathBuf>, kubeconfig: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            kubeconfig: kubeconfig.into(),
        }
    }

    /// Runs `kubectl` with the kubeconfig.
    ///
    /// # Errors
    ///
    /// When it fails.
    pub fn run(&self, args: &[&str], stdin: Option<&[u8]>) -> Result<String, HostError> {
        let kubeconfig = format!("--kubeconfig={}", self.kubeconfig.display());
        let mut all = vec![kubeconfig.as_str()];
        all.extend_from_slice(args);
        cmd::run(&self.binary, &all, stdin)
    }

    /// Server-side applies YAML or JSON documents as the installer's field manager.
    ///
    /// # Errors
    ///
    /// When the server rejects them.
    pub fn apply(&self, documents: &[u8]) -> Result<String, HostError> {
        self.apply_as(MANAGER, documents)
    }

    /// Server-side applies documents as `manager`. Fields another manager set stay; only the ones
    /// `manager` set before and leaves out now are removed.
    ///
    /// # Errors
    ///
    /// When the server rejects them.
    pub fn apply_as(&self, manager: &str, documents: &[u8]) -> Result<String, HostError> {
        let manager = format!("--field-manager={manager}");
        self.run(
            &[
                "apply",
                "--server-side",
                "--force-conflicts",
                &manager,
                "-f",
                "-",
            ],
            Some(documents),
        )
    }

    /// Server-side applies documents into `namespace` when they name none.
    ///
    /// A chart's install job finishes before its pods serve, so objects checked by its webhook or
    /// of a kind it has only just defined are retried for a few minutes.
    ///
    /// # Errors
    ///
    /// When the server rejects them.
    pub fn apply_in(&self, namespace: &str, documents: &[u8]) -> Result<String, HostError> {
        let manager = format!("--field-manager={MANAGER}");
        let args = [
            "apply",
            "--server-side",
            "--force-conflicts",
            &manager,
            "--namespace",
            namespace,
            "-f",
            "-",
        ];
        let start = Instant::now();
        loop {
            match self.run(&args, Some(documents)) {
                Err(HostError::Command { reason, .. })
                    if not_serving_yet(&reason) && start.elapsed() < Duration::from_mins(5) =>
                {
                    thread::sleep(Duration::from_secs(5));
                }
                result => return result,
            }
        }
    }

    /// The kubeconfig, for other tools such as `zeabur-identityctl`.
    #[must_use]
    pub fn kubeconfig(&self) -> &std::path::Path {
        &self.kubeconfig
    }

    /// Server-side applies one object.
    ///
    /// # Errors
    ///
    /// When the server rejects it.
    pub fn apply_object(&self, object: &Value) -> Result<(), HostError> {
        self.apply(object.to_string().as_bytes()).map(drop)
    }

    /// Gets an object as JSON, or `None` when it does not exist.
    ///
    /// # Errors
    ///
    /// When the request fails for another reason.
    pub fn get(
        &self,
        kind: &str,
        name: &str,
        namespace: Option<&str>,
    ) -> Result<Option<Value>, HostError> {
        let mut args = vec!["get", kind, name, "--ignore-not-found", "--output=json"];
        if let Some(ns) = namespace {
            args.extend(["--namespace", ns]);
        }
        let out = self.run(&args, None)?;
        if out.trim().is_empty() {
            return Ok(None);
        }
        serde_json::from_str(&out)
            .map(Some)
            .map_err(|e| HostError::Parse {
                what: format!("kubectl get {kind} {name}"),
                reason: e.to_string(),
            })
    }

    /// `kubectl wait` with the given arguments.
    ///
    /// # Errors
    ///
    /// When the condition is not met in time.
    pub fn wait(&self, args: &[&str]) -> Result<(), HostError> {
        let mut all = vec!["wait"];
        all.extend_from_slice(args);
        self.run(&all, None).map(drop)
    }

    /// Waits for the API server to answer `/readyz`.
    ///
    /// # Errors
    ///
    /// When it does not within `timeout`.
    pub fn wait_ready(&self, timeout: Duration) -> Result<(), HostError> {
        poll(timeout, "the API server to become ready", || {
            self.run(&["get", "--raw=/readyz"], None).is_ok()
        })
    }
}

/// A Kubernetes timestamp (`2026-10-23T08:00:30Z`, RFC 3339 in UTC) in seconds since the Unix
/// epoch.
#[must_use]
pub fn unix_seconds(text: &str) -> Option<u64> {
    let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (date.next()??, date.next()??, date.next()??);
    let time = time.split('.').next()?;
    let mut time = time.splitn(3, ':').map(|p| p.parse::<i64>().ok());
    let (hh, mm, ss) = (time.next()??, time.next()??, time.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from 1970-01-01 to the date in the proleptic Gregorian calendar (Howard Hinnant's
    // days_from_civil).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + hh * 3600 + mm * 60 + ss).ok()
}

/// Calls `ready` every few seconds until it returns `true`.
///
/// # Errors
///
/// When it does not within `timeout`.
pub fn poll(
    timeout: Duration,
    what: &str,
    mut ready: impl FnMut() -> bool,
) -> Result<(), HostError> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if ready() {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(3));
    }
    Err(HostError::Command {
        command: format!("waiting for {what}"),
        reason: format!("not done after {} s", timeout.as_secs()),
    })
}

fn not_serving_yet(reason: &str) -> bool {
    reason.contains("failed calling webhook")
        || reason.contains("no endpoints available for service")
        || reason.contains("ensure CRDs are installed first")
}

/// A Secret object, labelled as the installer's.
#[must_use]
pub fn secret(namespace: &str, name: &str, data: &BTreeMap<String, Vec<u8>>) -> Value {
    let data: BTreeMap<&str, String> = data
        .iter()
        .map(|(k, v)| (k.as_str(), STANDARD.encode(v)))
        .collect();
    json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {"name": name, "namespace": namespace, "labels": {"app.kubernetes.io/managed-by": MANAGER}},
        "type": "Opaque",
        "data": data,
    })
}

/// A key of a Secret object, decoded.
#[must_use]
pub fn secret_key(secret: &Value, key: &str) -> Option<Vec<u8>> {
    STANDARD
        .decode(secret.get("data")?.get(key)?.as_str()?)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps() {
        assert_eq!(unix_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(unix_seconds("2000-03-01T00:00:00Z"), Some(951_868_800));
        assert_eq!(unix_seconds("2026-10-23T08:00:30Z"), Some(1_792_742_430));
        assert_eq!(unix_seconds("2026-10-23T08:00:30.5Z"), Some(1_792_742_430));
        assert_eq!(unix_seconds("2026-10-23T08:00:30+08:00"), None);
        assert_eq!(unix_seconds("yesterday"), None);
    }

    #[test]
    fn secrets_round_trip() {
        let data = BTreeMap::from([("k".to_owned(), b"v".to_vec())]);
        let s = secret("ns", "n", &data);
        assert_eq!(s["data"]["k"], "dg==");
        assert_eq!(secret_key(&s, "k").unwrap(), b"v");
        assert!(secret_key(&s, "x").is_none());
    }

    #[test]
    fn webhooks_not_yet_serving_are_retried() {
        assert!(not_serving_yet(
            "exit status: 1\nError from server (InternalError): Internal error occurred: failed \
             calling webhook \"vlsingles.operator.victoriametrics.com\": failed to call webhook: \
             no endpoints available for service \"victoria-metrics-operator\""
        ));
        assert!(!not_serving_yet(
            "Error from server (Invalid): VLSingle.operator.victoriametrics.com \"logs\" is invalid"
        ));
    }
}
