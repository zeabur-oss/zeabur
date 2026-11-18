//! What a template's pods use when idle, from Zeabur Catalog's
//! `measurements/templates/<id>.json`, which its nightly end-to-end run writes. Only passed
//! runs count. A run lists one pod of each workload; every template runs one pod per workload,
//! so the rows add up to the template.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_model::kernel::quantity;

use crate::bundle;

/// What a template's pods used once idle, on a fresh cluster after its smoke checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdleUse {
    /// The pods' working set in bytes.
    pub memory_bytes: u64,
    /// The pods' CPU in millicores.
    pub cpu_millicores: u64,
    /// Where it was measured: `<distribution>-<profile>-<architecture>-<Kubernetes version>`.
    pub environment: String,
    /// The day it was measured (`2026-10-25`).
    pub measured_at: String,
}

#[derive(Deserialize)]
struct File {
    environments: Vec<Environment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Environment {
    distribution: String,
    profile: String,
    platform: Platform,
    #[serde(default)]
    kubernetes_version: Option<String>,
    #[serde(default)]
    pods: Vec<Pod>,
    /// `passed`; any other value, or none, is a run that did not pass.
    #[serde(default)]
    result: String,
    measured_at: String,
}

#[derive(Deserialize)]
struct Platform {
    os: String,
    architecture: String,
}

/// One pod of a workload, as Kubernetes quantities (`22Mi`, `3m`).
#[derive(Deserialize)]
struct Pod {
    memory: String,
    cpu: String,
}

impl Environment {
    fn name(&self) -> String {
        let mut name = format!(
            "{}-{}-{}",
            self.distribution, self.profile, self.platform.architecture
        );
        if let Some(version) = &self.kubernetes_version {
            name.push('-');
            name.push_str(version);
        }
        name
    }
}

/// What the template `id` uses when idle, as the embedded catalogue measured it; `None` when it
/// has no measurement.
#[must_use]
pub fn template_idle(id: &str) -> Option<IdleUse> {
    bundle::TEMPLATE_MEASUREMENTS
        .iter()
        .find(|(name, _)| *name == id)
        .and_then(|(_, text)| idle_from(text))
}

/// The newest passed run of a measurement file on a single-node kind cluster on linux/amd64, else
/// its newest passed run.
fn idle_from(text: &str) -> Option<IdleUse> {
    let file: File = serde_json::from_str(text).ok()?;
    let passed = file.environments.iter().filter(|e| e.result == "passed");
    let preferred = |e: &&Environment| {
        e.distribution == "kind"
            && e.profile == "single-node"
            && e.platform.os == "linux"
            && e.platform.architecture == "amd64"
    };
    let measured = passed
        .clone()
        .filter(preferred)
        .max_by(|a, b| a.measured_at.cmp(&b.measured_at))
        .or_else(|| passed.max_by(|a, b| a.measured_at.cmp(&b.measured_at)))?;
    let mut memory_bytes = 0;
    let mut cpu_millicores = 0;
    for pod in &measured.pods {
        memory_bytes += quantity::bytes(&pod.memory)?;
        cpu_millicores += quantity::millicores(&pod.cpu)?;
    }
    Some(IdleUse {
        memory_bytes,
        cpu_millicores,
        environment: measured.name(),
        measured_at: measured.measured_at.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(distribution: &str, arch: &str, memory: &str, result: &str, at: &str) -> String {
        format!(
            r#"{{"distribution": "{distribution}", "profile": "single-node",
                "platform": {{"os": "linux", "architecture": "{arch}"}},
                "kubernetesVersion": "v1.36.4",
                "pods": [{{"workload": "app", "kind": "Deployment", "memory": "{memory}", "cpu": "3m"}},
                         {{"workload": "app-db", "kind": "Cluster", "memory": "10Mi", "cpu": "1m"}}],
                "result": "{result}", "measuredAt": "{at}"}}"#
        )
    }

    #[test]
    fn takes_the_newest_passed_single_node_kind_amd64_run() {
        let text = format!(
            r#"{{"template": "ghost", "environments": [{}, {}, {}, {}, {}]}}"#,
            run("k3s", "amd64", "900Mi", "passed", "2026-10-26"),
            run("kind", "amd64", "300Mi", "passed", "2026-10-11"),
            run("kind", "amd64", "300Mi", "passed", "2026-10-25"),
            run("kind", "amd64", "0", "failed", "2026-10-27"),
            run("kind", "arm64", "280Mi", "passed", "2026-10-28"),
        );
        assert_eq!(
            idle_from(&text),
            Some(IdleUse {
                memory_bytes: 310 * 1024 * 1024,
                cpu_millicores: 4,
                environment: "kind-single-node-amd64-v1.36.4".to_owned(),
                measured_at: "2026-10-25".to_owned(),
            })
        );
    }

    #[test]
    fn falls_back_to_another_passed_run() {
        let text = format!(
            r#"{{"environments": [{}, {}]}}"#,
            run("k3s", "arm64", "12Mi", "passed", "2026-10-11"),
            run("kind", "amd64", "0", "skipped", "2026-10-12"),
        );
        let idle = idle_from(&text).unwrap();
        assert_eq!(idle.environment, "k3s-single-node-arm64-v1.36.4");
        assert_eq!(idle.memory_bytes, 22 * 1024 * 1024);
    }

    #[test]
    fn a_run_without_a_result_did_not_pass_and_the_others_still_count() {
        let text = format!(
            r#"{{"environments": [{}, {}]}}"#,
            run("kind", "amd64", "300Mi", "passed", "2026-10-11"),
            run("kind", "amd64", "900Mi", "passed", "2026-10-12")
                .replace(r#""result": "passed", "#, ""),
        );
        let idle = idle_from(&text).unwrap();
        assert_eq!(idle.measured_at, "2026-10-11");
    }

    #[test]
    fn every_embedded_passed_run_reads() {
        for (id, text) in bundle::TEMPLATE_MEASUREMENTS {
            if text.contains(r#""result":"passed""#) || text.contains(r#""result": "passed""#) {
                let idle = idle_from(text);
                assert!(idle.is_some_and(|i| i.memory_bytes > 0), "{id}");
            }
        }
    }

    #[test]
    fn nothing_without_a_readable_measurement() {
        assert_eq!(idle_from(r#"{"environments": []}"#), None);
        assert_eq!(idle_from("{"), None);
        let unreadable = run("kind", "amd64", "lots", "passed", "2026-10-11");
        assert_eq!(
            idle_from(&format!(r#"{{"environments": [{unreadable}]}}"#)),
            None
        );
    }
}
