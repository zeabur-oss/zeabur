//! Whether the Pods on each node still fit in its memory once an upgrade reserves memory on the
//! server.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::plan::{Plan, Warning, warning};

/// One node's memory, as the API server reports it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeMemory {
    /// The node's name.
    pub name: String,
    /// `status.capacity.memory`.
    pub capacity_mib: u64,
    /// `status.allocatable.memory`: what Pods may request on it now.
    pub allocatable_mib: u64,
    /// What the Pods scheduled on it request.
    pub requested_mib: u64,
}

/// Reads [`NodeMemory`] from `kubectl get nodes -o json` and `kubectl get pods -A -o json`.
/// Pods that finished or are not scheduled request nothing.
#[must_use]
pub fn nodes(nodes: &Value, pods: &Value) -> Vec<NodeMemory> {
    let mut bytes: BTreeMap<&str, u128> = BTreeMap::new();
    for pod in items(pods) {
        let phase = pod.pointer("/status/phase").and_then(Value::as_str);
        if matches!(phase, Some("Succeeded" | "Failed")) {
            continue;
        }
        let Some(node) = pod.pointer("/spec/nodeName").and_then(Value::as_str) else {
            continue;
        };
        *bytes.entry(node).or_default() += pod_request(pod);
    }
    let mut out = Vec::new();
    for node in items(nodes) {
        let name = node
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let memory = |field: &str| {
            node.pointer(&format!("/status/{field}/memory"))
                .and_then(Value::as_str)
                .and_then(quantity)
                .map_or(0, mib_down)
        };
        out.push(NodeMemory {
            name: name.to_owned(),
            capacity_mib: memory("capacity"),
            allocatable_mib: memory("allocatable"),
            requested_mib: mib_up(bytes.get(name).copied().unwrap_or(0)),
        });
    }
    out
}

fn items(list: &Value) -> &[Value] {
    list.get("items")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// What the scheduler counts for a Pod, by the rules of `capacity.rs::pod_amounts` in
/// zeabur-api: its containers and sidecars together, or the peak while init containers run,
/// whichever is more; a Pod-level request instead when there is one; plus its overhead.
fn pod_request(pod: &Value) -> u128 {
    let memory = |at: Option<&Value>| {
        at.and_then(|r| r.pointer("/requests/memory"))
            .and_then(Value::as_str)
            .and_then(quantity)
            .unwrap_or(0)
    };
    let list = |path: &str| {
        pod.pointer(path)
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
    };
    let mut regular = 0;
    for c in list("/spec/containers") {
        regular += memory(c.get("resources"));
    }
    let mut sidecars = 0;
    let mut init_peak = 0;
    for c in list("/spec/initContainers") {
        let own = memory(c.get("resources"));
        if c.get("restartPolicy").and_then(Value::as_str) == Some("Always") {
            regular += own;
            sidecars += own;
            init_peak = init_peak.max(sidecars);
        } else {
            init_peak = init_peak.max(own + sidecars);
        }
    }
    let pod_level = pod
        .pointer("/spec/resources/requests/memory")
        .and_then(Value::as_str)
        .and_then(quantity);
    let total = pod_level.unwrap_or_else(|| regular.max(init_peak));
    let overhead = pod
        .pointer("/spec/overhead/memory")
        .and_then(Value::as_str)
        .and_then(quantity)
        .unwrap_or(0);
    total + overhead
}

/// A Kubernetes memory quantity (`8128452Ki`, `512Mi`, `1.5Gi`, `500M`, `1048576`) in bytes.
fn quantity(text: &str) -> Option<u128> {
    const SUFFIXES: [(&str, u128); 13] = [
        ("Ki", 1 << 10),
        ("Mi", 1 << 20),
        ("Gi", 1 << 30),
        ("Ti", 1 << 40),
        ("Pi", 1 << 50),
        ("Ei", 1 << 60),
        ("k", 1_000),
        ("M", 1_000_000),
        ("G", 1_000_000_000),
        ("T", 1_000_000_000_000),
        ("P", 1_000_000_000_000_000),
        ("E", 1_000_000_000_000_000_000),
        ("", 1),
    ];
    if let Some(milli) = text.strip_suffix('m') {
        return milli.parse::<u128>().ok().map(|m| m / 1000);
    }
    for (suffix, unit) in SUFFIXES {
        let Some(number) = text.strip_suffix(suffix) else {
            continue;
        };
        let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
        let whole: u128 = whole.parse().ok()?;
        let mut bytes = whole.checked_mul(unit)?;
        if !fraction.is_empty() {
            let digits: u128 = fraction.parse().ok()?;
            let scale = 10_u128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
            bytes = bytes.checked_add(digits.checked_mul(unit)? / scale)?;
        }
        return Some(bytes);
    }
    None
}

const MIB: u128 = 1 << 20;

fn mib_up(bytes: u128) -> u64 {
    u64::try_from(bytes.div_ceil(MIB)).unwrap_or(u64::MAX)
}

fn mib_down(bytes: u128) -> u64 {
    u64::try_from(bytes / MIB).unwrap_or(u64::MAX)
}

/// A warning for each node whose Pods request more than is left once the reservation applies.
///
/// The server keeps the smaller of its live allocatable memory
/// and its capacity less the reservation; other nodes keep their live allocatable memory, as this
/// apply doesn't change their kubelet. Nothing on a first install.
#[must_use]
pub fn warnings(plan: &Plan, nodes: &[NodeMemory]) -> Vec<Warning> {
    if plan.changes.is_none() {
        return Vec::new();
    }
    let distribution = plan.cluster.distribution;
    let title = distribution.title();
    let mut out = Vec::new();
    for node in nodes {
        let mut left = node.allocatable_mib;
        if node.name == plan.node_name {
            left = left.min(
                node.capacity_mib
                    .saturating_sub(distribution.kept_from_pods_mib()),
            );
        }
        if node.requested_mib <= left {
            continue;
        }
        let (name, requested, left) = (
            node.name.as_str(),
            node.requested_mib.to_string(),
            left.to_string(),
        );
        out.push(warning(
            "memory.nodeFull",
            &[name, &requested, &left, title],
            format!(
                "The Pods on node {name} already request {requested} MiB of memory, but only \
                 {left} MiB is left for Pods once the system and {title} have their share. Pods \
                 that don't fit stay Pending. To make room, delete a service on {name} or lower \
                 its replicas, move a service to another node, or add a node."
            ),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn requests_are_read_per_node() {
        let nodes = json!({"items": [{
            "metadata": {"name": "localhost"},
            "status": {"capacity": {"memory": "8126464Ki"}, "allocatable": {"memory": "5996544Ki"}},
        }]});
        let pod = |namespace: &str, phase: &str, node: Option<&str>, spec: Value| {
            let mut pod = json!({
                "metadata": {"namespace": namespace},
                "spec": spec,
                "status": {"phase": phase},
            });
            if let Some(node) = node {
                pod["spec"]["nodeName"] = json!(node);
            }
            pod
        };
        let pods = json!({"items": [
            pod("observability", "Running", Some("localhost"), json!({
                "containers": [
                    {"resources": {"requests": {"memory": "512Mi"}}},
                    {"resources": {"requests": {"memory": "0.5Gi"}}},
                ],
                "initContainers": [{"resources": {"requests": {"memory": "2Gi"}}}],
            })),
            pod("observability", "Running", Some("localhost"), json!({
                "containers": [{"resources": {"requests": {"memory": "1000M"}}}],
                "overhead": {"memory": "64Mi"},
            })),
            pod("apps", "Succeeded", Some("localhost"), json!({
                "containers": [{"resources": {"requests": {"memory": "1Gi"}}}],
            })),
            pod("apps", "Pending", None, json!({
                "containers": [{"resources": {"requests": {"memory": "1Gi"}}}],
            })),
        ]});
        assert_eq!(
            super::nodes(&nodes, &pods),
            [NodeMemory {
                name: "localhost".to_owned(),
                capacity_mib: 7936,
                allocatable_mib: 5856,
                // 2048 from the init container, then 954 (1000M rounded up) and 64.
                requested_mib: 2048 + 954 + 64,
            }]
        );
    }

    #[test]
    fn sidecars_run_with_the_containers_and_a_pod_level_request_wins() {
        let mib = |pod: Value| pod_request(&json!({"spec": pod})) >> 20;
        // A native sidecar keeps running next to the app.
        assert_eq!(
            mib(json!({
                "containers": [{"resources": {"requests": {"memory": "512Mi"}}}],
                "initContainers": [
                    {"restartPolicy": "Always", "resources": {"requests": {"memory": "512Mi"}}},
                ],
            })),
            1024
        );
        // An ordinary init container after a sidecar runs alongside it.
        assert_eq!(
            mib(json!({
                "containers": [{"resources": {"requests": {"memory": "128Mi"}}}],
                "initContainers": [
                    {"restartPolicy": "Always", "resources": {"requests": {"memory": "256Mi"}}},
                    {"resources": {"requests": {"memory": "1Gi"}}},
                ],
            })),
            1280
        );
        // A Pod-level request replaces the containers' total; overhead still adds.
        assert_eq!(
            mib(json!({
                "resources": {"requests": {"memory": "1Gi"}},
                "containers": [{}],
                "overhead": {"memory": "64Mi"},
            })),
            1088
        );
    }
}
