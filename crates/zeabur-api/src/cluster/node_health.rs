//! What is wrong with a node, in the order it hurts.
//!
//! Down first, then a pressure the kubelet acts on, memory or disk nearly full, then what only
//! limits new pods (cordoned, reservations). Memory fill is measured use against the machine's
//! capacity and disk fill against the filesystem, when metrics exist; reservations (requests) only
//! say whether new pods fit.

use serde::Serialize;
use utoipa::ToSchema;
use zeabur_model::Health;

use crate::views::{DiskUsage, NodeView};

/// The shares at which a fill turns amber and red.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Threshold {
    /// Amber from this share.
    pub warn: f64,
    /// Red from this share.
    pub fail: f64,
}

impl Threshold {
    /// How a fill of `share` shows; `ok` when it is unknown.
    #[must_use]
    pub fn health(self, share: Option<f64>) -> Health {
        match share {
            Some(s) if s >= self.fail => Health::Fail,
            Some(s) if s >= self.warn => Health::Warn,
            _ => Health::Ok,
        }
    }
}

/// The thresholds a node is judged by.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeThresholds {
    /// The share of allocatable reserved above which new pods may not fit.
    pub tight: f64,
    /// Memory in use: the kernel kills processes as it runs out.
    pub memory: Threshold,
    /// The kubelet's filesystem: the kubelet starts evicting pods below 10% free
    /// (`nodefs.available<10%`).
    pub disk: Threshold,
}

/// The thresholds Zeabur judges nodes by.
pub const THRESHOLDS: NodeThresholds = NodeThresholds {
    tight: 0.9,
    memory: Threshold {
        warn: 0.85,
        fail: 0.95,
    },
    disk: Threshold {
        warn: 0.85,
        fail: 0.9,
    },
};

/// A volume: the app's writes fail when it is full.
pub const VOLUME: Threshold = Threshold {
    warn: 0.85,
    fail: 0.95,
};

/// What is wrong with a node, in the order it hurts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum NodeIssueKind {
    /// Its `Ready` condition is not `True`.
    NotReady,
    /// `MemoryPressure`: the kubelet evicts pods.
    MemoryPressure,
    /// `DiskPressure`: the kubelet evicts pods.
    DiskPressure,
    /// `NetworkUnavailable`.
    NetworkUnavailable,
    /// Memory in use is near the machine's capacity.
    MemoryFull,
    /// The kubelet's filesystem is nearly full.
    DiskFull,
    /// `PIDPressure`.
    PidPressure,
    /// Cordoned, with replicas that stay until someone moves them.
    Cordoned,
    /// Cordoned, with replicas leaving.
    Draining,
    /// Cordoned, with nothing left to move.
    Maintenance,
    /// Pods reserve nearly all its memory.
    ReservedMemory,
    /// Pods reserve nearly all its CPU.
    ReservedCpu,
}

impl NodeIssueKind {
    /// It takes services down, or is about to; the rest only limit what new pods can do.
    #[must_use]
    pub const fn is_problem(self) -> bool {
        !matches!(
            self,
            Self::Cordoned
                | Self::Draining
                | Self::Maintenance
                | Self::ReservedMemory
                | Self::ReservedCpu
        )
    }
}

/// One thing wrong with a node.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeIssue {
    /// What.
    pub kind: NodeIssueKind,
    /// How it shows.
    pub health: Health,
    /// The share it is about: memory or disk in use, or reserved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<f64>,
}

/// One issue on a node, in a group.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeIssueAt {
    /// The node.
    pub node: String,
    /// How it shows there.
    pub health: Health,
    /// The share it is about there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<f64>,
}

/// One kind of issue over every node that has it, so twelve nodes over-reserved on memory read
/// as one line.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeIssueGroup {
    /// What.
    pub kind: NodeIssueKind,
    /// The worst health of its nodes.
    pub health: Health,
    /// It takes services down, or is about to.
    pub problem: bool,
    /// Each node with it, worst first.
    pub issues: Vec<NodeIssueAt>,
}

/// `part` of `whole`; unknown when either is.
#[expect(
    clippy::cast_precision_loss,
    reason = "a share needs far fewer digits than f64 keeps"
)]
pub(crate) fn share(part: Option<u64>, whole: u64) -> Option<f64> {
    part.filter(|_| whole > 0).map(|p| p as f64 / whole as f64)
}

/// The share of a filesystem that is no longer free to write.
pub(crate) fn disk_share(disk: Option<&DiskUsage>) -> Option<f64> {
    disk.and_then(|d| {
        share(
            Some(d.capacity_bytes.saturating_sub(d.available_bytes)),
            d.capacity_bytes,
        )
    })
}

/// Memory in use of the whole machine. The metrics API counts every process, the system's too,
/// so it is divided by the machine's capacity. Allocatable leaves out the system reserve, and on
/// a small server that makes a healthy node read above 100%.
fn memory_share(node: &NodeView) -> Option<f64> {
    share(
        node.usage.map(|u| u.memory_bytes),
        node.capacity.memory_bytes,
    )
}

const fn issue(kind: NodeIssueKind, health: Health, share: Option<f64>) -> NodeIssue {
    NodeIssue {
        kind,
        health,
        share,
    }
}

/// Worst health first, then the order it hurts, then the larger share.
fn by_issue(a: &NodeIssue, b: &NodeIssue) -> std::cmp::Ordering {
    (a.health, a.kind)
        .cmp(&(b.health, b.kind))
        .then_with(|| b.share.unwrap_or(0.0).total_cmp(&a.share.unwrap_or(0.0)))
}

/// Every issue of a node, worst first. A node that is down has only that one.
#[must_use]
pub fn issues(node: &NodeView) -> Vec<NodeIssue> {
    if node.ready != "True" {
        return vec![issue(NodeIssueKind::NotReady, Health::Fail, None)];
    }
    let mut out: Vec<NodeIssue> = node
        .problems
        .iter()
        .filter_map(|c| match c.r#type.as_str() {
            "MemoryPressure" => Some(issue(NodeIssueKind::MemoryPressure, Health::Fail, None)),
            "DiskPressure" => Some(issue(NodeIssueKind::DiskPressure, Health::Fail, None)),
            "PIDPressure" => Some(issue(NodeIssueKind::PidPressure, Health::Warn, None)),
            "NetworkUnavailable" => {
                Some(issue(NodeIssueKind::NetworkUnavailable, Health::Fail, None))
            }
            _ => None,
        })
        .collect();
    let has = |out: &[NodeIssue], kind| out.iter().any(|i| i.kind == kind);
    let memory = memory_share(node);
    let memory_health = THRESHOLDS.memory.health(memory);
    if memory_health != Health::Ok && !has(&out, NodeIssueKind::MemoryPressure) {
        out.push(issue(NodeIssueKind::MemoryFull, memory_health, memory));
    }
    let disk = disk_share(node.disk.as_ref());
    let disk_health = THRESHOLDS.disk.health(disk);
    if disk_health != Health::Ok && !has(&out, NodeIssueKind::DiskPressure) {
        out.push(issue(NodeIssueKind::DiskFull, disk_health, disk));
    }
    if node.cordoned {
        out.push(if node.leaving_pods.unwrap_or(0) > 0 {
            issue(NodeIssueKind::Draining, Health::Progress, None)
        } else if node.movable_pods == Some(0) {
            issue(NodeIssueKind::Maintenance, Health::Idle, None)
        } else {
            issue(NodeIssueKind::Cordoned, Health::Warn, None)
        });
    }
    if let Some(requests) = node.requests {
        let reserved = [
            (
                NodeIssueKind::ReservedMemory,
                share(Some(requests.memory_bytes), node.allocatable.memory_bytes),
            ),
            (
                NodeIssueKind::ReservedCpu,
                share(
                    Some(requests.cpu_millicores),
                    node.allocatable.cpu_millicores,
                ),
            ),
        ];
        out.extend(
            reserved
                .into_iter()
                .filter(|(_, s)| s.is_some_and(|s| s >= THRESHOLDS.tight))
                .map(|(kind, s)| issue(kind, Health::Warn, s)),
        );
    }
    out.sort_by(by_issue);
    out
}

/// What the node list says about its nodes.
pub(crate) struct Judgement {
    /// [`groups`].
    pub(crate) attention: Vec<NodeIssueGroup>,
    /// [`worst_first`].
    pub(crate) worst_first: Vec<String>,
    /// [`worst`].
    pub(crate) worst: Option<String>,
}

/// Sets each node's `issues` and judges them together.
pub(crate) fn judge(nodes: &mut [NodeView]) -> Judgement {
    for node in nodes.iter_mut() {
        node.issues = issues(node);
    }
    let worst_first = worst_first(nodes);
    Judgement {
        attention: groups(nodes),
        worst: worst(nodes, &worst_first),
        worst_first,
    }
}

/// One group per kind of issue over `nodes`, worst first. Each node's `issues` must be set.
#[must_use]
pub fn groups(nodes: &[NodeView]) -> Vec<NodeIssueGroup> {
    let mut all: Vec<(&str, &NodeIssue)> = nodes
        .iter()
        .flat_map(|n| n.issues.iter().map(move |i| (n.name.as_str(), i)))
        .collect();
    all.sort_by(|a, b| a.1.kind.cmp(&b.1.kind).then_with(|| by_issue(a.1, b.1)));
    let mut groups: Vec<NodeIssueGroup> = all
        .chunk_by(|a, b| a.1.kind == b.1.kind)
        .filter_map(|chunk| {
            let (_, first) = chunk.first()?;
            Some(NodeIssueGroup {
                kind: first.kind,
                health: first.health,
                problem: first.kind.is_problem(),
                issues: chunk
                    .iter()
                    .map(|(node, i)| NodeIssueAt {
                        node: (*node).to_owned(),
                        health: i.health,
                        share: i.share,
                    })
                    .collect(),
            })
        })
        .collect();
    groups.sort_by_key(|g| (g.health, g.kind));
    groups
}

/// The names of `nodes`, worst first: by the worst issue, then how full, then the name. Each
/// node's `issues` must be set.
#[must_use]
pub fn worst_first(nodes: &[NodeView]) -> Vec<String> {
    let key = |n: &NodeView| {
        let worst = n.issues.first();
        (
            worst.map_or(Health::Ok, |i| i.health),
            worst.is_none(),
            worst.map(|i| i.kind),
        )
    };
    let fullest = |n: &NodeView| {
        memory_share(n)
            .unwrap_or(0.0)
            .max(disk_share(n.disk.as_ref()).unwrap_or(0.0))
    };
    let mut sorted: Vec<&NodeView> = nodes.iter().collect();
    sorted.sort_by(|a, b| {
        key(a)
            .cmp(&key(b))
            .then_with(|| fullest(b).total_cmp(&fullest(a)))
            .then_with(|| a.name.cmp(&b.name))
    });
    sorted.into_iter().map(|n| n.name.clone()).collect()
}

/// The node the totals would hide: the worst one, when there is more than one node and its
/// worst issue takes services down.
#[must_use]
pub fn worst(nodes: &[NodeView], worst_first: &[String]) -> Option<String> {
    if nodes.len() < 2 {
        return None;
    }
    let name = worst_first.first()?;
    nodes
        .iter()
        .find(|n| &n.name == name)?
        .issues
        .first()
        .filter(|i| i.kind.is_problem())
        .map(|_| name.clone())
}

#[cfg(test)]
mod tests {
    use zeabur_model::Origin;

    use super::*;
    use crate::views::{NodeCondition, Resources};

    const GIB: u64 = 1 << 30;

    fn node(name: &str) -> NodeView {
        NodeView {
            id: format!("cluster/nodes/{name}"),
            origin: Origin::complete("v1", "Node", None, name),
            name: name.to_owned(),
            roles: Vec::new(),
            ready: "True".to_owned(),
            problems: Vec::new(),
            cordoned: false,
            taints: Vec::new(),
            kubelet_version: String::new(),
            os_image: String::new(),
            kernel_version: String::new(),
            architecture: String::new(),
            container_runtime: String::new(),
            allocatable: Resources {
                cpu_millicores: 4000,
                memory_bytes: 8 * GIB,
                pods: None,
            },
            capacity: Resources {
                cpu_millicores: 4000,
                memory_bytes: 8 * GIB,
                pods: None,
            },
            requests: None,
            limits: None,
            movable_pods: None,
            leaving_pods: None,
            pinned_pods: None,
            usage: None,
            disk: None,
            created_at: None,
            issues: Vec::new(),
        }
    }

    fn judged(mut node: NodeView) -> NodeView {
        node.issues = issues(&node);
        node
    }

    const MIB: u64 = 1 << 20;

    fn small_server() -> NodeView {
        let mut n = node("a");
        n.capacity.memory_bytes = 3915 * MIB;
        n.allocatable.memory_bytes = 1835 * MIB;
        n
    }

    fn memory_used(mut node: NodeView, bytes: u64) -> NodeView {
        node.usage = Some(Resources {
            cpu_millicores: 0,
            memory_bytes: bytes,
            pods: None,
        });
        node
    }

    fn disk_free(mut node: NodeView, capacity: u64, available: u64) -> NodeView {
        node.disk = Some(DiskUsage {
            capacity_bytes: capacity,
            used_bytes: capacity - available,
            available_bytes: available,
        });
        node
    }

    fn kinds(node: &NodeView) -> Vec<(NodeIssueKind, Health)> {
        issues(node).iter().map(|i| (i.kind, i.health)).collect()
    }

    #[test]
    fn a_fill_turns_amber_then_red_at_its_thresholds() {
        let memory = THRESHOLDS.memory;
        assert_eq!(memory.health(None), Health::Ok);
        assert_eq!(memory.health(Some(0.84)), Health::Ok);
        assert_eq!(memory.health(Some(0.85)), Health::Warn);
        assert_eq!(memory.health(Some(0.95)), Health::Fail);
    }

    #[test]
    fn memory_nearly_full_leads_with_use_not_with_what_pods_reserve() {
        let mut used = memory_used(node("a"), 8 * GIB / 100 * 91);
        used.requests = Some(Resources {
            cpu_millicores: 100,
            memory_bytes: GIB,
            pods: Some(1),
        });
        let found = issues(&used);
        assert_eq!(
            found.iter().map(|i| (i.kind, i.health)).collect::<Vec<_>>(),
            [(NodeIssueKind::MemoryFull, Health::Warn)]
        );
        assert!(found[0].share.is_some_and(|s| (s - 0.91).abs() < 0.01));
    }

    #[test]
    fn a_small_server_is_not_full_because_the_system_reserve_is_left_out_of_allocatable() {
        let fresh = memory_used(small_server(), 2802 * MIB);
        assert_eq!(kinds(&fresh), []);
    }

    #[test]
    fn a_small_server_turns_red_when_the_machine_runs_out_of_memory() {
        let full = memory_used(small_server(), 3800 * MIB);
        assert_eq!(kinds(&full), [(NodeIssueKind::MemoryFull, Health::Fail)]);
    }

    #[test]
    fn a_node_that_is_down_has_only_that_issue() {
        let mut down = memory_used(node("a"), 8 * GIB);
        down.ready = "Unknown".to_owned();
        down.cordoned = true;
        assert_eq!(kinds(&down), [(NodeIssueKind::NotReady, Health::Fail)]);
    }

    #[test]
    fn a_pressure_replaces_the_fill_it_explains_and_issues_come_worst_first() {
        let mut pressed = memory_used(node("a"), 8 * GIB);
        pressed.problems = ["PIDPressure", "MemoryPressure"]
            .map(|t| NodeCondition {
                r#type: t.to_owned(),
                status: "True".to_owned(),
                reason: None,
                message: None,
            })
            .into();
        pressed.cordoned = true;
        assert_eq!(
            kinds(&pressed),
            [
                (NodeIssueKind::MemoryPressure, Health::Fail),
                (NodeIssueKind::PidPressure, Health::Warn),
                (NodeIssueKind::Cordoned, Health::Warn),
            ]
        );
    }

    #[test]
    fn the_disk_turns_red_where_the_kubelet_starts_evicting() {
        let nearly = disk_free(node("a"), 40, 5);
        assert_eq!(kinds(&nearly), [(NodeIssueKind::DiskFull, Health::Warn)]);
        let evicting = disk_free(node("a"), 40, 3);
        assert_eq!(kinds(&evicting), [(NodeIssueKind::DiskFull, Health::Fail)]);
    }

    #[test]
    fn a_cordoned_node_says_what_is_still_on_it() {
        let cordoned = |movable, leaving| {
            let mut n = node("a");
            n.cordoned = true;
            n.movable_pods = movable;
            n.leaving_pods = leaving;
            kinds(&n)
        };
        assert_eq!(
            cordoned(Some(3), Some(2)),
            [(NodeIssueKind::Draining, Health::Progress)]
        );
        assert_eq!(
            cordoned(Some(0), Some(0)),
            [(NodeIssueKind::Maintenance, Health::Idle)]
        );
        assert_eq!(
            cordoned(Some(3), Some(0)),
            [(NodeIssueKind::Cordoned, Health::Warn)]
        );
        assert_eq!(
            cordoned(None, None),
            [(NodeIssueKind::Cordoned, Health::Warn)]
        );
    }

    #[test]
    fn reservations_above_the_tight_share_only_limit_new_pods() {
        let mut tight = node("a");
        tight.requests = Some(Resources {
            cpu_millicores: 3900,
            memory_bytes: 4 * GIB,
            pods: Some(10),
        });
        let found = issues(&tight);
        assert_eq!(
            found.iter().map(|i| i.kind).collect::<Vec<_>>(),
            [NodeIssueKind::ReservedCpu]
        );
        assert!(found.iter().all(|i| !i.kind.is_problem()));
    }

    #[test]
    fn the_same_issue_on_many_nodes_is_one_group_worst_first() {
        let nodes = [
            judged(disk_free(node("a"), 40, 5)),
            judged(disk_free(node("b"), 40, 3)),
            judged({
                let mut n = node("c");
                n.cordoned = true;
                n
            }),
        ];
        let found = groups(&nodes);
        assert_eq!(
            found
                .iter()
                .map(|g| (g.kind, g.health, g.problem))
                .collect::<Vec<_>>(),
            [
                (NodeIssueKind::DiskFull, Health::Fail, true),
                (NodeIssueKind::Cordoned, Health::Warn, false),
            ]
        );
        let disk: Vec<&str> = found.first().map_or_else(Vec::new, |g| {
            g.issues.iter().map(|i| i.node.as_str()).collect()
        });
        assert_eq!(disk, ["b", "a"]);
    }

    #[test]
    fn the_worst_node_is_named_only_among_several_and_only_for_a_problem() {
        let a = judged(node("a"));
        let full = judged(disk_free(node("b"), 40, 3));
        let mut cordoned = node("c");
        cordoned.cordoned = true;
        let cordoned = judged(cordoned);

        let nodes = [a.clone(), full.clone(), cordoned.clone()];
        let order = worst_first(&nodes);
        assert_eq!(order, ["b", "c", "a"]);
        assert_eq!(worst(&nodes, &order).as_deref(), Some("b"));

        let quiet = [a, cordoned];
        assert_eq!(worst(&quiet, &worst_first(&quiet)), None);
        let alone = std::slice::from_ref(&full);
        assert_eq!(worst(alone, &worst_first(alone)), None);
    }
}
