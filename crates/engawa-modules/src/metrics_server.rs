//! metrics-server: what each node and pod uses now, from the `metrics.k8s.io` API
//! [metrics-server].
//!
//! Usage is approximate, "meant only for autoscaling purposes" [metrics-server]; engawa shows it
//! beside requests, never instead of them. Any other provider of `metrics.k8s.io` (a Prometheus
//! adapter) serves the same API and is read the same way: upstream's "resource metrics pipeline"
//! [k8s-resource-metrics], the API both nodes and pods come from.
//!
//! Memory is the working set, the figure the kubelet acts on for eviction and that an
//! out-of-memory kill is measured against [ms-faq]. A pod's list leaves out pods metrics-server
//! has no sample for yet: one that started within its last two scrapes, or one not running
//! [ms-pod-storage]. The interface says "no sample yet" for those instead of a zero.
//!
//! [metrics-server]: https://github.com/kubernetes-sigs/metrics-server
//! [k8s-resource-metrics]: https://kubernetes.io/docs/tasks/debug/debug-cluster/resource-metrics-pipeline/
//! [ms-faq]: https://github.com/kubernetes-sigs/metrics-server/blob/master/FAQ.md
//! [ms-pod-storage]: https://github.com/kubernetes-sigs/metrics-server/blob/master/pkg/storage/pod.go

use engawa_kube::DynamicObject;

use crate::{Module, Read, text};

/// The module.
pub static MODULE: Module = Module {
    id: "metrics-server",
    capability: "metrics",
    reads: &[NODE_METRICS, POD_METRICS],
    http: &[],
};

/// `metrics.k8s.io/v1beta1` `NodeMetrics`, cluster-wide.
pub const NODE_METRICS: Read = Read {
    group: "metrics.k8s.io",
    version: "v1beta1",
    resource: "nodes",
    kind: "NodeMetrics",
};

/// `metrics.k8s.io/v1beta1` `PodMetrics`, read in one namespace.
pub const POD_METRICS: Read = Read {
    group: "metrics.k8s.io",
    version: "v1beta1",
    resource: "pods",
    kind: "PodMetrics",
};

/// What one node uses, as metrics-server last measured it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeUsage {
    /// The node.
    pub node: String,
    /// CPU, as a quantity (`480m`, `250000000n`).
    pub cpu: String,
    /// Memory (the working set), as a quantity (`1234Mi`).
    pub memory: String,
}

/// Each node's usage from its `NodeMetrics` object.
#[must_use]
pub fn node_usage(objects: &[DynamicObject]) -> Vec<NodeUsage> {
    objects
        .iter()
        .map(|object| NodeUsage {
            node: object.metadata.name.clone().unwrap_or_default(),
            cpu: text(object, "/usage/cpu").unwrap_or_default(),
            memory: text(object, "/usage/memory").unwrap_or_default(),
        })
        .collect()
}

/// What one container uses, as quantities.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerUsage {
    /// The container.
    pub name: String,
    /// CPU (`3m`, `1234567n`).
    pub cpu: String,
    /// Memory, the working set (`12345Ki`).
    pub memory: String,
}

/// What one pod uses, as metrics-server last measured it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PodUsage {
    /// The pod.
    pub pod: String,
    /// When the sample was taken (RFC 3339).
    pub timestamp: Option<String>,
    /// The window CPU was averaged over (`15s`, `1m0.5s`).
    pub window: Option<String>,
    /// Its containers.
    pub containers: Vec<ContainerUsage>,
}

/// Each pod's usage from its `PodMetrics` object, by name.
#[must_use]
pub fn pod_usage(objects: &[DynamicObject]) -> Vec<PodUsage> {
    let mut pods: Vec<PodUsage> = objects
        .iter()
        .map(|object| PodUsage {
            pod: object.metadata.name.clone().unwrap_or_default(),
            timestamp: text(object, "/timestamp"),
            window: text(object, "/window"),
            containers: object
                .data
                .pointer("/containers")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .map(|container| ContainerUsage {
                    name: string(container, "/name"),
                    cpu: string(container, "/usage/cpu"),
                    memory: string(container, "/usage/memory"),
                })
                .collect(),
        })
        .collect();
    pods.sort_by(|a, b| a.pod.cmp(&b.pod));
    pods
}

fn string(value: &serde_json::Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(value: serde_json::Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    /// A `PodMetrics` as metrics-server v0.8 serves it (nanocores, kibibytes), and one with a
    /// container left out of its answer.
    #[test]
    fn pod_metrics_give_each_containers_usage() {
        let pods = pod_usage(&[
            object(serde_json::json!({
                "apiVersion": "metrics.k8s.io/v1beta1", "kind": "PodMetrics",
                "metadata": {"name": "web-2", "namespace": "shop"},
                "timestamp": "2026-10-16T08:00:00Z", "window": "15.012s",
                "containers": [
                    {"name": "web", "usage": {"cpu": "1234567n", "memory": "20480Ki"}},
                    {"name": "proxy", "usage": {"cpu": "0", "memory": "4Mi"}}
                ]
            })),
            object(serde_json::json!({
                "apiVersion": "metrics.k8s.io/v1beta1", "kind": "PodMetrics",
                "metadata": {"name": "web-1", "namespace": "shop"},
                "timestamp": "2026-10-16T08:00:01Z", "window": "15s", "containers": []
            })),
        ]);
        assert_eq!(pods.len(), 2);
        assert_eq!(pods[0].pod, "web-1");
        assert_eq!(pods[0].containers, []);
        assert_eq!(pods[1].window.as_deref(), Some("15.012s"));
        assert_eq!(
            pods[1].containers[0],
            ContainerUsage {
                name: "web".to_owned(),
                cpu: "1234567n".to_owned(),
                memory: "20480Ki".to_owned(),
            }
        );
    }
}
