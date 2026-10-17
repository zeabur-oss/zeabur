//! system-upgrade-controller: which Kubernetes version the cluster's nodes are being upgraded to,
//! from its `Plan` objects [suc-plan].
//!
//! A Plan names a channel (a URL the controller resolves to the newest version) or a fixed
//! version; the controller writes what that resolved to in `status.latestVersion` and the nodes it
//! is upgrading now in `status.applying`. Engawa reports those; it makes no outbound request of its
//! own to learn about newer versions.
//!
//! [suc-plan]: https://github.com/rancher/system-upgrade-controller/blob/v0.20.2/pkg/apis/upgrade.cattle.io/v1/types.go

use engawa_kube::DynamicObject;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{Module, Read, text};

/// The module.
pub static MODULE: Module = Module {
    id: "system-upgrade-controller",
    capability: "node-updates",
    reads: &[PLANS],
    http: &[],
};

/// `upgrade.cattle.io/v1` `Plan`, in every namespace.
pub const PLANS: Read = Read {
    group: "upgrade.cattle.io",
    version: "v1",
    resource: "plans",
    kind: "Plan",
};

/// A Plan, reduced to what an upgrade overview needs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SystemUpgradeControllerPlan)]
pub struct Plan {
    /// Its namespace.
    pub namespace: String,
    /// Its name.
    pub name: String,
    /// `spec.channel`: the URL the controller resolves to the newest version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// `spec.version`: a fixed target version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `status.latestVersion`: what the channel (or `version`) resolved to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<String>,
    /// `status.applying`: the nodes being upgraded now.
    pub applying: Vec<String>,
}

/// What system-upgrade-controller adds to the cluster's upgrades.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SystemUpgradeControllerUpgrades)]
pub struct Upgrades {
    /// The Plans, by namespace and name.
    pub plans: Vec<Plan>,
}

/// Each Plan's fields from its object, sorted by namespace and name.
#[must_use]
pub fn plans(objects: &[DynamicObject]) -> Vec<Plan> {
    let mut plans: Vec<Plan> = objects
        .iter()
        .map(|object| Plan {
            namespace: object.metadata.namespace.clone().unwrap_or_default(),
            name: object.metadata.name.clone().unwrap_or_default(),
            channel: text(object, "/spec/channel"),
            version: text(object, "/spec/version"),
            latest_version: text(object, "/status/latestVersion"),
            applying: object
                .data
                .pointer("/status/applying")
                .and_then(serde_json::Value::as_array)
                .map_or_else(Vec::new, |nodes| {
                    nodes
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .collect()
                }),
        })
        .collect();
    plans.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
    plans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_gives_its_channel_latest_version_and_nodes_in_progress() {
        let object: DynamicObject = serde_json::from_value(serde_json::json!({
            "apiVersion": "upgrade.cattle.io/v1", "kind": "Plan",
            "metadata": {"name": "server-plan", "namespace": "system-upgrade"},
            "spec": {"channel": "https://update.k3s.io/v1-release/channels/stable"},
            "status": {"latestVersion": "v1.36.5-k3s1", "applying": ["node-1"]}
        }))
        .unwrap();
        let plans = plans(&[object]);
        assert_eq!(plans[0].latest_version.as_deref(), Some("v1.36.5-k3s1"));
        assert_eq!(plans[0].applying, ["node-1"]);
        assert!(plans[0].version.is_none());
    }
}
