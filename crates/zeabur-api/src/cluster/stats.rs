//! What the kubelet measures of disks: each node's root filesystem and each mounted volume, from
//! its stats summary (`/stats/summary`) [summary-api], read through kube-apiserver's node proxy.
//!
//! The summary is what metrics-server reads too; unlike metrics-server it carries filesystems,
//! and unlike a metrics store it needs nothing installed. Reading it takes `get` on
//! `nodes/proxy`, a right a cluster administrator has and few others do; without it the disks
//! are left out and the answer says so.
//!
//! The kubelet evicts pods when a node's filesystem has less than 10% free (`nodefs.available`)
//! [eviction], so a disk is near full well before it is full.
//!
//! [summary-api]: https://kubernetes.io/docs/reference/instrumentation/node-metrics/
//! [eviction]: https://kubernetes.io/docs/concepts/scheduling-eviction/node-pressure-eviction/

use std::collections::BTreeMap;
use std::sync::Arc;

use futures_util::StreamExt as _;
use serde_json::Value;
use zeabur_kube::{Cluster, ClusterError};

use crate::views::{DiskUsage, StatsState, UnreadableView};

/// The most summaries read at once.
const AT_ONCE: usize = 8;

/// The resource a refusal names.
pub(crate) const NODES_PROXY: &str = "nodes/proxy";

/// A volume a pod mounts from a claim, as the kubelet measured it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClaimDisk {
    pub namespace: String,
    pub claim: String,
    pub pod: String,
    pub disk: DiskUsage,
}

/// One node's summary, as far as disks go.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    /// The node's root filesystem (`nodefs`).
    pub node: Option<DiskUsage>,
    /// What the container runtime's images take.
    pub images_bytes: Option<u64>,
    pub claims: Vec<ClaimDisk>,
}

fn bytes(value: &Value, key: &str) -> Option<u64> {
    value.get(key)?.as_u64()
}

fn disk(value: &Value) -> Option<DiskUsage> {
    let capacity_bytes = bytes(value, "capacityBytes")?;
    let used_bytes = bytes(value, "usedBytes")?;
    let available_bytes =
        bytes(value, "availableBytes").unwrap_or_else(|| capacity_bytes.saturating_sub(used_bytes));
    (capacity_bytes > 0).then_some(DiskUsage {
        capacity_bytes,
        used_bytes,
        available_bytes,
    })
}

/// The disks in one `/stats/summary` answer.
pub(crate) fn summary(value: &Value) -> Summary {
    let node = value.get("node");
    let claims = value
        .get("pods")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|pod| {
            let name = pod
                .pointer("/podRef/name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            pod.get("volume")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(move |volume| {
                    let reference = volume.get("pvcRef")?;
                    Some(ClaimDisk {
                        namespace: reference.get("namespace")?.as_str()?.to_owned(),
                        claim: reference.get("name")?.as_str()?.to_owned(),
                        pod: name.clone(),
                        disk: disk(volume)?,
                    })
                })
        })
        .collect();
    Summary {
        node: node.and_then(|n| n.get("fs")).and_then(disk),
        images_bytes: node
            .and_then(|n| n.pointer("/runtime/imageFs"))
            .and_then(|fs| bytes(fs, "usedBytes")),
        claims,
    }
}

/// The summaries of `nodes`, by node, with whether they could be read: a refusal anywhere makes
/// the state `Unreadable` and is named in `unreadable`; no answer at all `Unavailable`. A node
/// that did not answer is left out.
pub(crate) async fn summaries(
    cluster: &Arc<dyn Cluster>,
    nodes: Vec<String>,
    unreadable: &mut Vec<UnreadableView>,
) -> (BTreeMap<String, Summary>, StatsState) {
    let answers: Vec<(String, Result<Value, ClusterError>)> = futures_util::stream::iter(nodes)
        .map(|node| {
            let cluster = Arc::clone(cluster);
            async move {
                let answer = cluster.node_proxy_get(&node, "/stats/summary").await;
                (node, answer)
            }
        })
        .buffer_unordered(AT_ONCE)
        .collect()
        .await;
    let asked = answers.len();
    let mut read = BTreeMap::new();
    let mut refused = None;
    for (node, answer) in answers {
        match answer {
            Ok(value) => {
                read.insert(node, summary(&value));
            }
            Err(error) => {
                if let Ok(refusal) = error.into_unreadable(NODES_PROXY) {
                    refused.get_or_insert(refusal);
                }
            }
        }
    }
    let state = match refused {
        Some(refusal) => {
            unreadable.push(refusal.into());
            StatsState::Unreadable
        }
        None if read.is_empty() && asked > 0 => StatsState::Unavailable,
        None => StatsState::Available,
    };
    (read, state)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_the_node_filesystem_images_and_claimed_volumes() {
        let answer = json!({
            "node": {
                "nodeName": "vps-1",
                "fs": {"capacityBytes": 40_000, "usedBytes": 30_000, "availableBytes": 8_000},
                "runtime": {"imageFs": {"capacityBytes": 40_000, "usedBytes": 12_000}}
            },
            "pods": [
                {"podRef": {"name": "db-0", "namespace": "shop"}, "volume": [
                    {"name": "data", "pvcRef": {"name": "data-db-0", "namespace": "shop"},
                     "capacityBytes": 100, "usedBytes": 90, "availableBytes": 10},
                    {"name": "kube-api-access", "capacityBytes": 100, "usedBytes": 1}
                ]},
                {"podRef": {"name": "web-1", "namespace": "shop"}}
            ]
        });
        let read = summary(&answer);
        assert_eq!(
            read.node,
            Some(DiskUsage {
                capacity_bytes: 40_000,
                used_bytes: 30_000,
                available_bytes: 8_000
            })
        );
        assert_eq!(read.images_bytes, Some(12_000));
        assert_eq!(read.claims.len(), 1);
        assert_eq!(read.claims[0].claim, "data-db-0");
        assert_eq!(read.claims[0].pod, "db-0");
        assert_eq!(read.claims[0].disk.used_bytes, 90);
    }

    #[test]
    fn an_answer_without_filesystems_has_no_disks() {
        let read = summary(&json!({"node": {"nodeName": "a"}}));
        assert_eq!(read, Summary::default());
        let read = summary(&json!({"node": {"fs": {"capacityBytes": 0, "usedBytes": 0}}}));
        assert_eq!(read.node, None);
    }
}
