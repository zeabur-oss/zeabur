//! How full disks are, from the metrics store (see `crate::settings`), read as the person through
//! the service proxy like usage history: each node's filesystem from node-exporter
//! (`node_filesystem_{size,free,avail}_bytes`), each claimed volume from the kubelet
//! (`kubelet_volume_stats_{capacity,used,available}_bytes`, by `namespace` and
//! `persistentvolumeclaim`) [kubelet-volume-stats]. Both are in what the VictoriaMetrics and
//! Prometheus k8s stacks scrape ([vmks-values], [kps-values]).
//!
//! The kubelet evicts pods when its filesystem has less than 10% free (`nodefs.available`)
//! [eviction], so a disk is near full well before it is full.
//!
//! [kubelet-volume-stats]: https://github.com/kubernetes/kubernetes/blob/master/pkg/kubelet/metrics/collectors/volume_stats.go
//! [vmks-values]: https://github.com/VictoriaMetrics/helm-charts/blob/master/charts/victoria-metrics-k8s-stack/values.yaml
//! [kps-values]: https://github.com/prometheus-community/helm-charts/blob/main/charts/kube-prometheus-stack/values.yaml
//! [eviction]: https://kubernetes.io/docs/concepts/scheduling-eviction/node-pressure-eviction/

use std::collections::BTreeMap;

use zeabur_kube::Cluster;
use zeabur_kube::k8s_openapi::api::core::v1::Node;
use zeabur_modules::prometheus_api::{self, PrometheusSample, QUERY, StoreError};

use crate::settings::{Proxied, Settings, StoreState, metrics_store, proxy_get, proxy_refusal};
use crate::views::{DiskState, DiskUsage, UnreadableView};

/// The mount points that can hold the kubelet's root directory, deepest first.
const KUBELET_MOUNTS: [&str; 4] = ["/var/lib/kubelet", "/var/lib", "/var", "/"];

/// node-exporter's filesystems at [`KUBELET_MOUNTS`].
pub(crate) const NODE_FILESYSTEMS: &str = "{__name__=~\"node_filesystem_(size|free|avail)_bytes\",\
     mountpoint=~\"/var/lib/kubelet|/var/lib|/var|/\"}";

/// The kubelet's volume metrics for the claims in `namespace`, a Kubernetes name.
pub(crate) fn volume_query(namespace: &str) -> String {
    format!(
        "{{__name__=~\"kubelet_volume_stats_(capacity|used|available)_bytes\",namespace=\"{namespace}\"}}"
    )
}

/// What the store answered: its series, or why there are none.
pub(crate) struct Asked {
    pub answer: Result<Vec<PrometheusSample>, DiskState>,
    /// The settings ConfigMap or the store, when the person may not read it.
    pub unreadable: Vec<UnreadableView>,
}

/// Disks by name, and whether they could be read.
pub(crate) struct Disks {
    pub state: DiskState,
    pub disks: BTreeMap<String, DiskUsage>,
}

impl Asked {
    /// The disks `pick` finds in the series; `NotCollected` when it finds none.
    pub(crate) fn disks(
        &self,
        pick: impl FnOnce(&[PrometheusSample]) -> BTreeMap<String, DiskUsage>,
    ) -> Disks {
        match &self.answer {
            Ok(samples) => {
                let disks = pick(samples);
                Disks {
                    state: if disks.is_empty() {
                        DiskState::NotCollected
                    } else {
                        DiskState::Available
                    },
                    disks,
                }
            }
            Err(state) => Disks {
                state: *state,
                disks: BTreeMap::new(),
            },
        }
    }
}

/// The instant query `promql` of the metrics store, as the person.
pub(crate) async fn ask(cluster: &dyn Cluster, settings: &Settings, promql: &str) -> Asked {
    let store = metrics_store(cluster, settings).await;
    let mut unreadable: Vec<UnreadableView> = store.unreadable.into_iter().collect();
    let encoded = form_urlencoded::Serializer::new(String::new())
        .append_pair("query", promql)
        .finish();
    let answer = match proxy_get(cluster, &store.service, &format!("{QUERY}?{encoded}")).await {
        Proxied::Answer(answer) => {
            prometheus_api::samples(&answer).map_err(|StoreError { .. }| DiskState::Unavailable)
        }
        Proxied::Failed { state, status, .. } => Err(match state {
            StoreState::NotConfigured => DiskState::NotConfigured,
            StoreState::Unreadable => {
                unreadable.extend(status.map(|status| proxy_refusal(&store.service, status)));
                DiskState::Unreadable
            }
            StoreState::Unavailable | StoreState::Available => DiskState::Unavailable,
        }),
    };
    Asked { answer, unreadable }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "byte counts, finite and clamped at 0; a fraction of a byte is nothing"
)]
const fn bytes(value: f64) -> u64 {
    value.max(0.0).round() as u64
}

/// The host of a scrape target's `instance` (`10.0.0.5:9100`, `[fd00::5]:9100`, `vps-1`).
fn host(instance: &str) -> &str {
    if let Some(rest) = instance.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    match instance.split_once(':') {
        Some((host, port)) if !port.contains(':') => host,
        _ => instance,
    }
}

/// The node a node-exporter series is from: its `node` label where the scrape sets one, else
/// its `instance`, which is the node's address (node-exporter runs in the host's network in both
/// stacks) or, where the scrape names it so, the node's name.
fn node_of<'a>(sample: &PrometheusSample, nodes: &'a [Node]) -> Option<&'a str> {
    let named = sample.label("node");
    let host = host(sample.label("instance"));
    nodes.iter().find_map(|node| {
        let name = node.metadata.name.as_deref()?;
        let addressed = node
            .status
            .as_ref()
            .and_then(|s| s.addresses.as_ref())
            .is_some_and(|addresses| addresses.iter().any(|a| a.address == host));
        (name == named || (named.is_empty() && (name == host || addressed))).then_some(name)
    })
}

/// Each node's kubelet filesystem in node-exporter's series ([`NODE_FILESYSTEMS`]).
///
/// The kubelet's `nodefs` is the filesystem that holds its root directory, `/var/lib/kubelet`
/// unless `--root-dir` says otherwise ([eviction], "Eviction signals"; [kubelet]); of the
/// filesystems node-exporter reports, that is the one mounted deepest on that path, `/` when
/// nothing under it has a filesystem of its own.
///
/// [eviction]: https://kubernetes.io/docs/concepts/scheduling-eviction/node-pressure-eviction/#eviction-signals
/// [kubelet]: https://kubernetes.io/docs/reference/command-line-tools-reference/kubelet/
pub(crate) fn node_disks(
    samples: &[PrometheusSample],
    nodes: &[Node],
) -> BTreeMap<String, DiskUsage> {
    let mut found: BTreeMap<(&str, &str), [Option<f64>; 3]> = BTreeMap::new();
    for sample in samples {
        let Some(node) = node_of(sample, nodes) else {
            continue;
        };
        let slot = match sample.label("__name__") {
            "node_filesystem_size_bytes" => 0,
            "node_filesystem_free_bytes" => 1,
            "node_filesystem_avail_bytes" => 2,
            _ => continue,
        };
        let values = found.entry((node, sample.label("mountpoint"))).or_default();
        if let Some(value) = values.get_mut(slot) {
            *value = Some(value.map_or(sample.value, |v: f64| v.max(sample.value)));
        }
    }
    let mut disks = BTreeMap::new();
    for node in nodes.iter().filter_map(|n| n.metadata.name.as_deref()) {
        let disk = KUBELET_MOUNTS.iter().find_map(|mount| {
            let [Some(size), free, Some(avail)] = *found.get(&(node, *mount))? else {
                return None;
            };
            (size > 0.0).then(|| DiskUsage {
                capacity_bytes: bytes(size),
                used_bytes: bytes(size - free.unwrap_or(avail)),
                available_bytes: bytes(avail),
            })
        });
        if let Some(disk) = disk {
            disks.insert(node.to_owned(), disk);
        }
    }
    disks
}

/// Each claim's volume in the kubelet's series ([`volume_query`]), by claim. A volume mounted on
/// several nodes is measured by each kubelet; the fullest counts.
pub(crate) fn volume_disks(samples: &[PrometheusSample]) -> BTreeMap<String, DiskUsage> {
    let mut found: BTreeMap<&str, [Option<f64>; 3]> = BTreeMap::new();
    for sample in samples {
        let claim = sample.label("persistentvolumeclaim");
        let slot = match sample.label("__name__") {
            "kubelet_volume_stats_capacity_bytes" => 0,
            "kubelet_volume_stats_used_bytes" => 1,
            "kubelet_volume_stats_available_bytes" => 2,
            _ => continue,
        };
        if claim.is_empty() {
            continue;
        }
        let values = found.entry(claim).or_default();
        if let Some(value) = values.get_mut(slot) {
            *value = Some(match slot {
                // Fullest: the least available.
                2 => value.map_or(sample.value, |v: f64| v.min(sample.value)),
                _ => value.map_or(sample.value, |v: f64| v.max(sample.value)),
            });
        }
    }
    found
        .into_iter()
        .filter_map(|(claim, values)| {
            let [Some(capacity), used, Some(available)] = values else {
                return None;
            };
            (capacity > 0.0).then(|| {
                (
                    claim.to_owned(),
                    DiskUsage {
                        capacity_bytes: bytes(capacity),
                        used_bytes: bytes(used.unwrap_or(capacity - available)),
                        available_bytes: bytes(available),
                    },
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sample(name: &str, labels: &[(&str, &str)], value: f64) -> PrometheusSample {
        let mut labels: BTreeMap<String, String> = labels
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        labels.insert("__name__".to_owned(), name.to_owned());
        PrometheusSample { labels, value }
    }

    fn node(name: &str, address: &str) -> Node {
        serde_json::from_value(json!({"metadata": {"name": name},
            "status": {"addresses": [{"type": "InternalIP", "address": address}]}}))
        .unwrap()
    }

    fn filesystem(
        instance: &str,
        mount: &str,
        size: f64,
        free: f64,
        avail: f64,
    ) -> Vec<PrometheusSample> {
        let labels = [("instance", instance), ("mountpoint", mount)];
        vec![
            sample("node_filesystem_size_bytes", &labels, size),
            sample("node_filesystem_free_bytes", &labels, free),
            sample("node_filesystem_avail_bytes", &labels, avail),
        ]
    }

    #[test]
    fn a_node_disk_is_the_filesystem_that_holds_the_kubelet_directory() {
        let nodes = [
            node("vps-1", "10.0.0.1"),
            node("vps-2", "fd00::2"),
            node("vps-3", "10.0.0.3"),
        ];
        let samples: Vec<PrometheusSample> = [
            filesystem("10.0.0.1:9100", "/", 100.0, 40.0, 35.0),
            filesystem("10.0.0.1:9100", "/var", 500.0, 100.0, 75.0),
            filesystem("[fd00::2]:9100", "/", 200.0, 20.0, 10.0),
            // Named by the scrape rather than addressed.
            filesystem("vps-3", "/", 300.0, 30.0, 30.0),
            filesystem("10.9.9.9:9100", "/", 1.0, 1.0, 1.0),
        ]
        .concat();
        let disks = node_disks(&samples, &nodes);
        assert_eq!(
            disks["vps-1"],
            DiskUsage {
                capacity_bytes: 500,
                used_bytes: 400,
                available_bytes: 75
            }
        );
        assert_eq!(disks["vps-2"].used_bytes, 180);
        assert_eq!(disks["vps-3"].capacity_bytes, 300);
        assert_eq!(disks.len(), 3);
    }

    #[test]
    fn a_node_label_decides_over_the_instance() {
        let nodes = [node("vps-1", "10.0.0.1"), node("vps-2", "10.0.0.2")];
        let labels = [
            ("node", "vps-2"),
            ("instance", "10.0.0.1:9100"),
            ("mountpoint", "/"),
        ];
        let samples = [
            sample("node_filesystem_size_bytes", &labels, 10.0),
            sample("node_filesystem_avail_bytes", &labels, 4.0),
        ];
        let disks = node_disks(&samples, &nodes);
        assert_eq!(disks.keys().collect::<Vec<_>>(), ["vps-2"]);
        // Without free bytes, what is not available is taken.
        assert_eq!(disks["vps-2"].used_bytes, 6);
    }

    #[test]
    fn a_volume_measured_twice_counts_its_fullest() {
        let claim = |name: &str, value: f64, kubelet: &str| {
            sample(
                name,
                &[
                    ("persistentvolumeclaim", "data-db-0"),
                    ("instance", kubelet),
                ],
                value,
            )
        };
        let samples = [
            claim("kubelet_volume_stats_capacity_bytes", 10.0, "a"),
            claim("kubelet_volume_stats_used_bytes", 8.0, "a"),
            claim("kubelet_volume_stats_available_bytes", 2.0, "a"),
            claim("kubelet_volume_stats_available_bytes", 1.0, "b"),
            sample("kubelet_volume_stats_capacity_bytes", &[], 5.0),
        ];
        let disks = volume_disks(&samples);
        assert_eq!(
            disks["data-db-0"],
            DiskUsage {
                capacity_bytes: 10,
                used_bytes: 8,
                available_bytes: 1
            }
        );
        assert_eq!(disks.len(), 1);
    }

    #[test]
    fn instances_name_their_host() {
        assert_eq!(host("10.0.0.5:9100"), "10.0.0.5");
        assert_eq!(host("[fd00::5]:9100"), "fd00::5");
        assert_eq!(host("vps-1"), "vps-1");
        assert_eq!(host("fd00::5"), "fd00::5");
    }
}
