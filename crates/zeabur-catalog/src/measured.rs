//! What a template's pods use when idle, from Zeabur Catalog's
//! `measurements/templates/<id>.json`, which its nightly end-to-end run writes. Only passed
//! runs count.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
    idle: Amounts,
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

#[derive(Default, Deserialize)]
struct Amounts {
    #[serde(default, rename = "memoryMiB")]
    memory_mib: f64,
    #[serde(default, rename = "cpuMillis")]
    cpu_millis: f64,
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
    Some(IdleUse {
        memory_bytes: whole(measured.idle.memory_mib * 1024.0 * 1024.0),
        cpu_millicores: whole(measured.idle.cpu_millis),
        environment: measured.name(),
        measured_at: measured.measured_at.clone(),
    })
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a measurement is a small non-negative number; `as` saturates"
)]
const fn whole(value: f64) -> u64 {
    value.round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(distribution: &str, arch: &str, memory: f64, result: &str, at: &str) -> String {
        format!(
            r#"{{"distribution": "{distribution}", "profile": "single-node",
                "platform": {{"os": "linux", "architecture": "{arch}"}},
                "kubernetesVersion": "v1.36.4", "idle": {{"memoryMiB": {memory}, "cpuMillis": 4}},
                "result": "{result}", "measuredAt": "{at}"}}"#
        )
    }

    #[test]
    fn takes_the_newest_passed_single_node_kind_amd64_run() {
        let text = format!(
            r#"{{"template": "ghost", "environments": [{}, {}, {}, {}, {}]}}"#,
            run("k3s", "amd64", 900.0, "passed", "2026-10-26"),
            run("kind", "amd64", 300.0, "passed", "2026-10-11"),
            run("kind", "amd64", 310.0, "passed", "2026-10-25"),
            run("kind", "amd64", 0.0, "failed", "2026-10-27"),
            run("kind", "arm64", 280.0, "passed", "2026-10-28"),
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
            run("k3s", "arm64", 12.4, "passed", "2026-10-11"),
            run("kind", "amd64", 0.0, "skipped", "2026-10-12"),
        );
        let idle = idle_from(&text).unwrap();
        assert_eq!(idle.environment, "k3s-single-node-arm64-v1.36.4");
        assert_eq!(idle.memory_bytes, 13_002_342);
    }

    #[test]
    fn a_run_without_a_result_did_not_pass_and_the_others_still_count() {
        let text = format!(
            r#"{{"environments": [{}, {}]}}"#,
            run("kind", "amd64", 300.0, "passed", "2026-10-11"),
            run("kind", "amd64", 900.0, "passed", "2026-10-12")
                .replace(r#""result": "passed", "#, ""),
        );
        let idle = idle_from(&text).unwrap();
        assert_eq!(idle.measured_at, "2026-10-11");
    }

    #[test]
    fn nothing_without_a_readable_measurement() {
        assert_eq!(idle_from(r#"{"environments": []}"#), None);
        assert_eq!(idle_from("{"), None);
    }
}
