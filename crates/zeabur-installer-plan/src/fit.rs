//! Whether each node's Pods still fit in its memory after an upgrade: the server keeps the memory
//! this plan reserves from Pods, and the added and changed components request what the catalog
//! measured.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::catalog::{Component, Index, KIND};
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
    /// What the Pods scheduled on it request, by namespace.
    pub requests_mib: BTreeMap<String, u64>,
}

/// Reads [`NodeMemory`] from `kubectl get nodes -o json` and `kubectl get pods -A -o json`.
/// Pods that finished or are not scheduled request nothing.
#[must_use]
pub fn nodes(nodes: &Value, pods: &Value) -> Vec<NodeMemory> {
    let mut bytes: BTreeMap<&str, BTreeMap<&str, u128>> = BTreeMap::new();
    for pod in items(pods) {
        let phase = pod.pointer("/status/phase").and_then(Value::as_str);
        if matches!(phase, Some("Succeeded" | "Failed")) {
            continue;
        }
        let Some(node) = pod.pointer("/spec/nodeName").and_then(Value::as_str) else {
            continue;
        };
        let namespace = pod
            .pointer("/metadata/namespace")
            .and_then(Value::as_str)
            .unwrap_or_default();
        *bytes.entry(node).or_default().entry(namespace).or_default() += pod_request(pod);
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
        let mut requests_mib = BTreeMap::new();
        for (namespace, b) in bytes.get(name).into_iter().flatten() {
            requests_mib.insert((*namespace).to_owned(), mib_up(*b));
        }
        out.push(NodeMemory {
            name: name.to_owned(),
            capacity_mib: memory("capacity"),
            allocatable_mib: memory("allocatable"),
            requests_mib,
        });
    }
    out
}

fn items(list: &Value) -> &[Value] {
    list.get("items")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// What the scheduler counts for a Pod: its containers together or its largest init container,
/// whichever is more, plus its overhead.
fn pod_request(pod: &Value) -> u128 {
    let request = |container: &Value| {
        container
            .pointer("/resources/requests/memory")
            .and_then(Value::as_str)
            .and_then(quantity)
            .unwrap_or(0)
    };
    let mut containers = 0;
    for c in pod
        .pointer("/spec/containers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        containers += request(c);
    }
    let mut init = 0;
    for c in pod
        .pointer("/spec/initContainers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        init = init.max(request(c));
    }
    let overhead = pod
        .pointer("/spec/overhead/memory")
        .and_then(Value::as_str)
        .and_then(quantity)
        .unwrap_or(0);
    containers.max(init) + overhead
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

/// Components that share a namespace, and what their Pods request together after the apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Group {
    /// Their namespaces.
    pub namespaces: BTreeSet<String>,
    /// What the catalog measured their Pods request.
    pub requests_mib: u64,
}

/// A node whose Pods, after the apply, would request more memory than it lets Pods use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Crowded {
    pub node: String,
    pub requested_mib: u64,
    pub allocatable_mib: u64,
}

/// The nodes that run out. `server` is this server's node and the memory its kubelet keeps from
/// Pods once the plan's reservation applies. A group's Pods take the place of the Pods now in
/// its namespaces, on the node that has most of them, or on the node with the most room when it
/// has none yet.
pub(crate) fn crowded(nodes: &[NodeMemory], groups: &[Group], server: (&str, u64)) -> Vec<Crowded> {
    let (own, kept_mib) = server;
    let mut allocatable = Vec::new();
    let mut requested = Vec::new();
    for node in nodes {
        let mut mib = node.allocatable_mib;
        if node.name == own {
            mib = mib.min(node.capacity_mib.saturating_sub(kept_mib));
        }
        allocatable.push(mib);
        requested.push(node.requests_mib.values().sum::<u64>());
    }
    let mut extra = vec![0_u64; nodes.len()];
    for group in groups {
        let mut now = 0;
        let mut most: Option<(usize, u64)> = None;
        for (i, node) in nodes.iter().enumerate() {
            let mut here = 0;
            for namespace in &group.namespaces {
                here += node.requests_mib.get(namespace).copied().unwrap_or(0);
            }
            now += here;
            if here > 0 && most.is_none_or(|(_, m)| here > m) {
                most = Some((i, here));
            }
        }
        let more = group.requests_mib.saturating_sub(now);
        if more == 0 {
            continue;
        }
        let target = match most {
            Some((i, _)) => Some(i),
            None => roomiest(&allocatable, &requested, &extra),
        };
        if let Some(slot) = target.and_then(|i| extra.get_mut(i)) {
            *slot += more;
        }
    }
    let mut out = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let total = requested.get(i).copied().unwrap_or(0) + extra.get(i).copied().unwrap_or(0);
        let allocatable_mib = allocatable.get(i).copied().unwrap_or(0);
        if total > allocatable_mib {
            out.push(Crowded {
                node: node.name.clone(),
                requested_mib: total,
                allocatable_mib,
            });
        }
    }
    out
}

fn roomiest(allocatable: &[u64], requested: &[u64], extra: &[u64]) -> Option<usize> {
    let mut best: Option<(usize, i128)> = None;
    for (i, a) in allocatable.iter().enumerate() {
        let used = requested.get(i).copied().unwrap_or(0) + extra.get(i).copied().unwrap_or(0);
        let room = i128::from(*a) - i128::from(used);
        if best.is_none_or(|(_, r)| room > r) {
            best = Some((i, room));
        }
    }
    best.map(|(i, _)| i)
}

/// The namespaces a component installs into, other than `kube-system`, which the distribution's
/// own Pods share.
fn namespaces(component: &Component) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for release in &component.releases {
        if release.namespace != "kube-system" {
            out.insert(release.namespace.clone());
        }
    }
    out
}

/// The added and changed components, each with every planned component it shares a namespace
/// with. A group with a component the catalog has no measurement for is left out.
fn groups(index: &Index, plan: &Plan, profile: &str) -> Vec<Group> {
    let Some(changes) = &plan.changes else {
        return Vec::new();
    };
    let distribution = plan.cluster.distribution.id();
    let arch = plan.facts.arch.as_str();
    let mut planned = Vec::new();
    for c in &plan.components {
        if let Some(component) = index.component(&c.id) {
            planned.push(component);
        }
    }
    let mut touched: Vec<&str> = changes.added.iter().map(String::as_str).collect();
    for chart in &changes.charts {
        touched.push(&chart.component);
    }
    let mut grouped = BTreeSet::new();
    let mut out = Vec::new();
    for id in touched {
        if grouped.contains(id) {
            continue;
        }
        let Some(first) = index.component(id) else {
            continue;
        };
        let mut members = vec![first];
        let mut spaces = namespaces(first);
        if spaces.is_empty() {
            continue;
        }
        loop {
            let mut grew = false;
            for c in &planned {
                if members.iter().any(|m| m.id == c.id) {
                    continue;
                }
                let theirs = namespaces(c);
                if theirs.is_disjoint(&spaces) {
                    continue;
                }
                spaces.extend(theirs);
                members.push(c);
                grew = true;
            }
            if !grew {
                break;
            }
        }
        let mut requests_mib = Some(0);
        for m in &members {
            grouped.insert(m.id.as_str());
            let request = m
                .request_memory_mib(distribution, profile, arch)
                .or_else(|| m.request_memory_mib(KIND, profile, arch));
            requests_mib = requests_mib.zip(request).map(|(a, b)| a + b);
        }
        if let Some(requests_mib) = requests_mib {
            out.push(Group {
                namespaces: spaces,
                requests_mib,
            });
        }
    }
    out
}

/// A warning for each node whose Pods, once this upgrade has run, would request more memory than
/// the node lets Pods use. Nothing on a first install.
#[must_use]
pub fn warnings(index: &Index, plan: &Plan, profile: &str, nodes: &[NodeMemory]) -> Vec<Warning> {
    if plan.changes.is_none() {
        return Vec::new();
    }
    let distribution = plan.cluster.distribution;
    let server = (plan.node_name.as_str(), distribution.kept_from_pods_mib());
    let title = distribution.title();
    let groups = groups(index, plan, profile);
    let mut out = Vec::new();
    for c in crowded(nodes, &groups, server) {
        let (node, requested, allocatable) = (
            c.node,
            c.requested_mib.to_string(),
            c.allocatable_mib.to_string(),
        );
        out.push(warning(
            "memory.nodeFull",
            &[&node, &requested, &allocatable, title],
            format!(
                "After this apply, the Pods on node {node} ask for about {requested} MiB of \
                 memory, but only {allocatable} MiB is left for Pods once the system and {title} \
                 have their share. Pods that don't fit stay Pending. To make room, delete a \
                 service on {node} or lower its replicas, move a service to another node, or add \
                 a node."
            ),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn node(name: &str, allocatable_mib: u64, requests: &[(&str, u64)]) -> NodeMemory {
        NodeMemory {
            name: name.to_owned(),
            capacity_mib: 7936,
            allocatable_mib,
            requests_mib: requests
                .iter()
                .map(|(n, m)| ((*n).to_owned(), *m))
                .collect(),
        }
    }

    fn group(namespaces: &[&str], requests_mib: u64) -> Group {
        Group {
            namespaces: namespaces.iter().map(|n| (*n).to_owned()).collect(),
            requests_mib,
        }
    }

    /// What K3s's kubelet keeps from Pods on a server: 350, 1530 and 200.
    const K3S: u64 = crate::cluster::Distribution::K3s.kept_from_pods_mib();

    #[test]
    fn the_reservation_alone_can_fill_a_node() {
        // v0.1 left the whole server to Pods; the new reservation leaves 5856 MiB.
        let nodes = [node("localhost", 7736, &[("apps", 6000)])];
        let crowded = crowded(&nodes, &[], ("localhost", K3S));
        assert_eq!(
            crowded,
            [Crowded {
                node: "localhost".to_owned(),
                requested_mib: 6000,
                allocatable_mib: 5856,
            }]
        );
    }

    /// The names of the nodes that run out on this server.
    fn full(nodes: &[NodeMemory], groups: &[Group]) -> Vec<String> {
        let mut names = Vec::new();
        for c in crowded(nodes, groups, ("localhost", K3S)) {
            names.push(c.node);
        }
        names
    }

    #[test]
    fn a_component_that_grows_on_a_nearly_full_node_does_not_fit() {
        // Pods request 97% of what is left; vmsingle grows from a single-node to a multi-node size.
        let nodes = [
            node("localhost", 5856, &[("apps", 5300), ("observability", 380)]),
            node("node-d5a2f6", 3600, &[("apps", 500)]),
        ];
        let crowded = crowded(
            &nodes,
            &[group(&["observability"], 971)],
            ("localhost", K3S),
        );
        assert_eq!(
            crowded,
            [Crowded {
                node: "localhost".to_owned(),
                requested_mib: 5300 + 971,
                allocatable_mib: 5856,
            }]
        );
    }

    #[test]
    fn a_component_that_keeps_its_size_fits_where_it_runs() {
        let nodes = [node(
            "localhost",
            5856,
            &[("apps", 5300), ("observability", 500)],
        )];
        assert_eq!(
            full(&nodes, &[group(&["observability"], 500)]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_new_component_goes_to_the_node_with_the_most_room() {
        let room = [
            node("localhost", 5856, &[("apps", 5700)]),
            node("node-d5a2f6", 3600, &[("apps", 500)]),
        ];
        assert_eq!(full(&room, &[group(&["keda"], 600)]), Vec::<String>::new());

        let crowded = [
            node("localhost", 5856, &[("apps", 5700)]),
            node("node-d5a2f6", 3600, &[("apps", 3400)]),
        ];
        assert_eq!(full(&crowded, &[group(&["keda"], 600)]), ["node-d5a2f6"]);
    }

    #[test]
    fn a_worker_keeps_what_it_allows_now() {
        // Only the server's kubelet takes this plan's reservation.
        let worker = NodeMemory {
            capacity_mib: 3900,
            ..node("node-d5a2f6", 3700, &[("apps", 3600)])
        };
        assert_eq!(full(&[worker], &[]), Vec::<String>::new());
    }

    #[test]
    fn requests_are_read_per_node_and_namespace() {
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
                requests_mib: [("observability".to_owned(), 2048 + 954 + 64)].into(),
            }]
        );
    }
}
