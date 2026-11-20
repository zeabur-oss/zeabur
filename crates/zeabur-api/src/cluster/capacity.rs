//! CPU and memory: Kubernetes quantities, and what each pod requests, summed per node the way
//! `kubectl describe node` sums it.
//!
//! # A pod's requests
//!
//! Kubernetes' `resourcehelper.PodRequests` [k8s-resource-helpers], which `kubectl describe node`
//! uses [kubectl-describe], and the sidecar rules [k8s-sidecars]:
//!
//! - the regular containers and the sidecars (init containers with `restartPolicy: Always`) run
//!   together, so their requests add up;
//! - an ordinary init container runs alone, beside the sidecars started before it; the pod needs
//!   the largest of those moments;
//! - the pod needs the larger of the two, plus the `RuntimeClass`'s `overhead`;
//! - pod-level `spec.resources` replace that sum for the resources they name.
//!
//! Limits are summed by the same rule. A container without a limit adds nothing, so a node's
//! limit total can understate what its pods may use, as in `kubectl describe node`. Resources
//! resized in place (`status.containerStatuses[].resources`) are not read: the spec's values are
//! used.
//!
//! # Allocatable, not capacity
//!
//! The denominator is the node's `allocatable`: "the amount of compute resources that are
//! available for pods. The scheduler does not over-subscribe 'Allocatable'" [k8s-reserve].
//! Capacity includes what is reserved for the system and would make a full node look roomy.
//!
//! [k8s-resource-helpers]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/component-helpers/resource/helpers.go
//! [kubectl-describe]: https://github.com/kubernetes/kubectl/blob/master/pkg/describe/describe.go
//! [k8s-sidecars]: https://kubernetes.io/docs/concepts/workloads/pods/sidecar-containers/
//! [k8s-reserve]: https://kubernetes.io/docs/tasks/administer-cluster/reserve-compute-resources/

use std::collections::BTreeMap;

pub(crate) use zeabur_core::quantity::{bytes, millicores};
use zeabur_kube::k8s_openapi::api::core::v1::{Container, Node, PodSpec};
use zeabur_kube::k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use zeabur_kube::{ClusterPod, ContainerResources};

/// CPU and memory together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Amount {
    /// Millicores.
    pub(crate) cpu: u64,
    /// Bytes.
    pub(crate) memory: u64,
}

impl Amount {
    pub(crate) fn of(map: &BTreeMap<String, String>) -> Self {
        Self {
            cpu: map.get("cpu").and_then(|q| millicores(q)).unwrap_or(0),
            memory: map.get("memory").and_then(|q| bytes(q)).unwrap_or(0),
        }
    }

    const fn plus(self, other: Self) -> Self {
        Self {
            cpu: self.cpu.saturating_add(other.cpu),
            memory: self.memory.saturating_add(other.memory),
        }
    }

    fn max(self, other: Self) -> Self {
        Self {
            cpu: self.cpu.max(other.cpu),
            memory: self.memory.max(other.memory),
        }
    }

    pub(crate) const fn add(&mut self, other: Self) {
        *self = self.plus(other);
    }
}

impl std::iter::Sum for Amount {
    fn sum<I: Iterator<Item = Self>>(amounts: I) -> Self {
        amounts.fold(Self::default(), Self::plus)
    }
}

/// What a pod requests and limits, by the rules in the module documentation.
pub(crate) fn pod_amounts(pod: &ClusterPod) -> (Amount, Amount) {
    let sum = |pick: fn(&ContainerResources) -> &BTreeMap<String, String>| {
        let regular = pod
            .containers
            .iter()
            .chain(pod.init_containers.iter().filter(|c| c.sidecar))
            .fold(Amount::default(), |acc, c| acc.plus(Amount::of(pick(c))));
        let mut sidecars = Amount::default();
        let mut init_peak = Amount::default();
        for container in &pod.init_containers {
            let own = Amount::of(pick(container));
            if container.sidecar {
                sidecars = sidecars.plus(own);
                init_peak = init_peak.max(sidecars);
            } else {
                init_peak = init_peak.max(own.plus(sidecars));
            }
        }
        let mut total = regular.max(init_peak);
        if let Some(pod_level) = &pod.pod_resources {
            let map = pick(pod_level);
            if let Some(cpu) = map.get("cpu").and_then(|q| millicores(q)) {
                total.cpu = cpu;
            }
            if let Some(memory) = map.get("memory").and_then(|q| bytes(q)) {
                total.memory = memory;
            }
        }
        total
    };
    let overhead = Amount::of(&pod.overhead);
    let requests = sum(|c| &c.requests).plus(overhead);
    // Overhead adds to a limit only where the pod has one, as `PodLimits` does.
    let mut limits = sum(|c| &c.limits);
    if limits.cpu > 0 {
        limits.cpu = limits.cpu.saturating_add(overhead.cpu);
    }
    if limits.memory > 0 {
        limits.memory = limits.memory.saturating_add(overhead.memory);
    }
    (requests, limits)
}

/// What a pod with `spec` requests and limits, by the same rules.
pub(crate) fn spec_amounts(spec: &PodSpec) -> (Amount, Amount) {
    let resources = |container: &Container| {
        let map = |pick: Option<&BTreeMap<String, Quantity>>| {
            pick.into_iter()
                .flatten()
                .map(|(k, v)| (k.clone(), v.0.clone()))
                .collect()
        };
        let r = container.resources.as_ref();
        ContainerResources {
            requests: map(r.and_then(|r| r.requests.as_ref())),
            limits: map(r.and_then(|r| r.limits.as_ref())),
            sidecar: container.restart_policy.as_deref() == Some("Always"),
        }
    };
    let pod = ClusterPod {
        containers: spec.containers.iter().map(resources).collect(),
        init_containers: spec
            .init_containers
            .iter()
            .flatten()
            .map(resources)
            .collect(),
        overhead: spec
            .overhead
            .iter()
            .flatten()
            .map(|(k, v)| (k.clone(), v.0.clone()))
            .collect(),
        ..ClusterPod::default()
    };
    pod_amounts(&pod)
}

/// What a ready, schedulable node has left for new pods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct NodeRoom {
    /// `allocatable` less what its pods request.
    pub(crate) free: Amount,
    /// Pod slots left.
    pub(crate) pods: u64,
}

/// Whether a node takes new pods: it is not cordoned and its `Ready` condition is `True`.
fn schedulable(node: &Node) -> bool {
    let cordoned = node
        .spec
        .as_ref()
        .and_then(|s| s.unschedulable)
        .unwrap_or(false);
    let ready = node
        .status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .is_some_and(|c| c.iter().any(|c| c.type_ == "Ready" && c.status == "True"));
    !cordoned && ready
}

/// The room on each ready, schedulable node: allocatable less its pods' requests. Host use
/// outside pods is not subtracted, so a fit here means the pods may fit.
pub(crate) fn node_room(nodes: &[Node], pods: &[ClusterPod]) -> Vec<NodeRoom> {
    let mut used: BTreeMap<&str, (Amount, u64)> = BTreeMap::new();
    for pod in pods {
        if matches!(pod.phase.as_deref(), Some("Succeeded" | "Failed")) {
            continue;
        }
        if let Some(node) = pod.node_name.as_deref() {
            let entry = used.entry(node).or_default();
            entry.0.add(pod_amounts(pod).0);
            entry.1 += 1;
        }
    }
    nodes
        .iter()
        .filter(|node| schedulable(node))
        .map(|node| {
            let allocatable = node.status.as_ref().and_then(|s| s.allocatable.as_ref());
            let quantity = |key: &str| allocatable.and_then(|a| a.get(key)).map(|q| q.0.as_str());
            let (taken, count) = used
                .get(node.metadata.name.as_deref().unwrap_or_default())
                .copied()
                .unwrap_or_default();
            NodeRoom {
                free: Amount {
                    cpu: quantity("cpu")
                        .and_then(millicores)
                        .unwrap_or(0)
                        .saturating_sub(taken.cpu),
                    memory: quantity("memory")
                        .and_then(bytes)
                        .unwrap_or(0)
                        .saturating_sub(taken.memory),
                },
                pods: quantity("pods")
                    .and_then(|p| p.parse::<u64>().ok())
                    .unwrap_or(110)
                    .saturating_sub(count),
            }
        })
        .collect()
}

/// Whether `pods` can each be placed on a node of `nodes` that still has its CPU, memory and a pod
/// slot. A pod never spans nodes, so room that is free only in total does not count. The search
/// tries the largest pod first on each node in turn and backs up when one is left over, so a fit
/// that a first-fit pass misses is found. It stops after `PLACE_STEPS` tries; a shortage it could
/// not prove counts as a fit.
pub(crate) fn place(pods: &[Amount], nodes: &[NodeRoom]) -> bool {
    let mut nodes = nodes.to_vec();
    let mut pods = pods.to_vec();
    pods.sort_by_key(|pod| std::cmp::Reverse((pod.memory, pod.cpu)));
    let mut steps = PLACE_STEPS;
    fits(&pods, &mut nodes, &mut steps)
}

const PLACE_STEPS: u32 = 10_000;

fn fits(pods: &[Amount], nodes: &mut [NodeRoom], steps: &mut u32) -> bool {
    let Some((pod, rest)) = pods.split_first() else {
        return true;
    };
    // A node with the same room as one already tried leads to the same answer.
    let mut tried: Vec<NodeRoom> = Vec::new();
    for i in 0..nodes.len() {
        if *steps == 0 {
            return true;
        }
        *steps -= 1;
        let Some(&node) = nodes.get(i) else { break };
        let room = node.pods > 0 && node.free.cpu >= pod.cpu && node.free.memory >= pod.memory;
        if !room || tried.contains(&node) {
            continue;
        }
        tried.push(node);
        let set = |nodes: &mut [NodeRoom], to: NodeRoom| {
            if let Some(slot) = nodes.get_mut(i) {
                *slot = to;
            }
        };
        let mut taken = node;
        taken.free.cpu -= pod.cpu;
        taken.free.memory -= pod.memory;
        taken.pods -= 1;
        set(nodes, taken);
        if fits(rest, nodes, steps) {
            return true;
        }
        set(nodes, node);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(requests: &[(&str, &str)], sidecar: bool) -> ContainerResources {
        ContainerResources {
            requests: requests
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            limits: BTreeMap::new(),
            sidecar,
        }
    }

    #[test]
    fn requests_follow_kubectl_for_init_containers_sidecars_and_overhead() {
        let pod = ClusterPod {
            containers: vec![container(&[("cpu", "100m"), ("memory", "128Mi")], false)],
            init_containers: vec![
                container(&[("cpu", "50m")], true),
                // Runs beside the sidecar: 50m + 500m = 550m > 150m regular.
                container(&[("cpu", "500m")], false),
            ],
            overhead: BTreeMap::from([("cpu".to_owned(), "10m".to_owned())]),
            ..ClusterPod::default()
        };
        let (requests, limits) = pod_amounts(&pod);
        assert_eq!(requests.cpu, 560);
        assert_eq!(requests.memory, 128 << 20);
        assert_eq!(limits, Amount::default());

        let pod_level = ClusterPod {
            pod_resources: Some(container(&[("memory", "1Gi")], false)),
            ..pod
        };
        assert_eq!(pod_amounts(&pod_level).0.memory, 1 << 30);
        assert_eq!(pod_amounts(&pod_level).0.cpu, 560);
    }

    const GIB: u64 = 1 << 30;

    fn node(memory: u64, pods: u64) -> NodeRoom {
        NodeRoom {
            free: Amount { cpu: 4000, memory },
            pods,
        }
    }

    fn pod(memory: u64) -> Amount {
        Amount { cpu: 100, memory }
    }

    #[test]
    fn a_pod_fits_only_on_one_node_not_across_several() {
        let nodes = [node(GIB, 110), node(GIB, 110), node(GIB, 110)];
        assert!(!place(&[pod(2 * GIB)], &nodes));
        assert!(place(&[pod(GIB), pod(GIB), pod(GIB)], &nodes));
    }

    #[test]
    fn the_largest_pod_is_placed_first() {
        let nodes = [node(GIB, 110), node(2 * GIB, 110)];
        assert!(place(&[pod(GIB), pod(2 * GIB)], &nodes));
    }

    #[test]
    fn a_fit_that_first_fit_misses_is_found() {
        let room = |cpu, memory| {
            let mut node = node(memory, 110);
            node.free.cpu = cpu;
            node
        };
        let nodes = [room(2000, 2 * GIB), room(1000, 2 * GIB)];
        let pods = [
            Amount {
                cpu: 1000,
                memory: 2 * GIB,
            },
            Amount {
                cpu: 2000,
                memory: GIB,
            },
        ];
        assert!(place(&pods, &nodes));
    }

    #[test]
    fn a_node_without_a_pod_slot_takes_no_pod() {
        assert!(!place(&[pod(GIB)], &[node(8 * GIB, 0)]));
    }
}
