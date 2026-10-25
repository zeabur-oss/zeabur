//! What a template's pods use when idle, from Zeabur Catalog's
//! `measurements/templates/<id>.json`, which its nightly end-to-end run writes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::bundle;

/// The environment Zeabur Catalog measures templates in, without its Kubernetes version.
const TEMPLATE_ENVIRONMENT: &str = "single-node-amd64-";

/// What a template's pods used once idle, on a fresh cluster after its smoke checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdleUse {
    /// The pods' working set in bytes.
    pub memory_bytes: u64,
    /// The pods' CPU in millicores.
    pub cpu_millicores: u64,
    /// Where it was measured: `<profile>-<architecture>-<Kubernetes version>`.
    pub environment: String,
    /// The day it was measured (`2026-10-25`).
    pub measured_at: String,
}

#[derive(Deserialize)]
struct File {
    environments: BTreeMap<String, Environment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Environment {
    #[serde(rename = "idleMemoryMiB")]
    idle_memory_mib: f64,
    #[serde(rename = "idleCPUm")]
    idle_cpu_m: f64,
    measured_at: String,
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

/// The newest single-node amd64 environment of a measurement file, else its newest environment.
fn idle_from(text: &str) -> Option<IdleUse> {
    let file: File = serde_json::from_str(text).ok()?;
    let (environment, measured) = file
        .environments
        .iter()
        .rev()
        .find(|(key, _)| key.starts_with(TEMPLATE_ENVIRONMENT))
        .or_else(|| file.environments.iter().next_back())?;
    Some(IdleUse {
        memory_bytes: whole(measured.idle_memory_mib * 1024.0 * 1024.0),
        cpu_millicores: whole(measured.idle_cpu_m),
        environment: environment.clone(),
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

    #[test]
    fn takes_the_newest_single_node_amd64_measurement() {
        let text = r#"{"template": "ghost", "environments": {
            "multi-node-amd64-v1.37.0": {"idleMemoryMiB": 900, "idleCPUm": 9, "measuredAt": "2026-10-25"},
            "single-node-amd64-v1.36.4": {"idleMemoryMiB": 300, "idleCPUm": 3, "measuredAt": "2026-10-11"},
            "single-node-amd64-v1.37.0": {"idleMemoryMiB": 310, "idleCPUm": 4, "measuredAt": "2026-10-25"},
            "single-node-arm64-v1.37.0": {"idleMemoryMiB": 280, "idleCPUm": 4, "measuredAt": "2026-10-25"}
        }}"#;
        assert_eq!(
            idle_from(text),
            Some(IdleUse {
                memory_bytes: 310 * 1024 * 1024,
                cpu_millicores: 4,
                environment: "single-node-amd64-v1.37.0".to_owned(),
                measured_at: "2026-10-25".to_owned(),
            })
        );
    }

    #[test]
    fn falls_back_to_another_environment() {
        let text = r#"{"environments": {
            "multi-node-arm64-v1.36.4": {"idleMemoryMiB": 12.4, "idleCPUm": 0, "measuredAt": "2026-10-11"}
        }}"#;
        let idle = idle_from(text).unwrap();
        assert_eq!(idle.environment, "multi-node-arm64-v1.36.4");
        assert_eq!(idle.memory_bytes, 13_002_342);
    }

    #[test]
    fn nothing_without_a_readable_measurement() {
        assert_eq!(idle_from(r#"{"environments": {}}"#), None);
        assert_eq!(idle_from("{"), None);
    }
}
