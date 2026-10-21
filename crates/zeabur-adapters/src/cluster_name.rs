//! What a cluster is called, from what its provider writes on its nodes or in a kubeconfig.
//!
//! Kubernetes gives a cluster no name of its own. Providers do: they name the nodes after the
//! cluster, label them with it, or name the kubeconfig's cluster entry after it.

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

use crate::label;

/// Who runs the cluster, as far as its nodes or its kubeconfig entry tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Provider {
    /// Amazon EKS.
    Eks,
    /// Google Kubernetes Engine.
    Gke,
    /// Azure Kubernetes Service.
    Aks,
    /// k3d (k3s in Docker).
    K3d,
    /// kind (Kubernetes in Docker).
    Kind,
    /// minikube.
    Minikube,
    /// Docker Desktop's Kubernetes.
    DockerDesktop,
    /// `OrbStack`'s Kubernetes.
    OrbStack,
    /// k3s.
    K3s,
    /// k0s.
    K0s,
    /// RKE2.
    Rke2,
    /// A distribution not named here: the cluster goes by its first control plane node.
    Node,
}

/// A name read from the cluster's nodes, and who wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    /// The cluster's name.
    pub name: String,
    /// Where it comes from.
    pub provider: Provider,
}

/// eksctl's label with the cluster's name.
const EKSCTL_CLUSTER: &str = "alpha.eksctl.io/cluster-name";
/// The node group label of EKS managed node groups.
const EKS_NODEGROUP: &str = "eks.amazonaws.com/nodegroup";
/// AKS' node resource group, `MC_<resource group>_<cluster>_<region>` by default.
const AKS_CLUSTER: &str = "kubernetes.azure.com/cluster";
/// GKE's node pool label.
const GKE_NODEPOOL: &str = "cloud.google.com/gke-nodepool";
/// minikube's profile, which is the cluster's name.
const MINIKUBE_NAME: &str = "minikube.k8s.io/name";
/// k3s and RKE2 set it to `k3s` or `rke2` on nodes without a cloud provider.
const INSTANCE_TYPE: &str = "node.kubernetes.io/instance-type";
const K0S_ROLE: &str = "node.k0sproject.io/role";

/// The cluster's name from its nodes' metadata.
///
/// A provider's label or naming scheme, else the first control plane node's name. `None` when
/// the nodes name nothing usable (only addresses, or no control plane among them, as on managed
/// clusters without a naming label).
#[must_use]
pub fn detect(nodes: &[ObjectMeta]) -> Option<Detected> {
    let mut nodes: Vec<&ObjectMeta> = nodes.iter().collect();
    nodes.sort_by(|a, b| {
        a.creation_timestamp
            .as_ref()
            .map(|t| t.0)
            .cmp(&b.creation_timestamp.as_ref().map(|t| t.0))
            .then_with(|| a.name.cmp(&b.name))
    });
    let found = |provider, name: Option<String>| {
        name.filter(|n| !n.is_empty())
            .map(|name| Detected { name, provider })
    };
    for node in &nodes {
        let labels = node.labels.as_ref();
        let name = node.name.as_deref().unwrap_or_default();
        if let Some(cluster) = label(labels, EKSCTL_CLUSTER) {
            return found(Provider::Eks, Some(cluster.to_owned()));
        }
        if let Some(group) = label(labels, AKS_CLUSTER) {
            return found(Provider::Aks, aks_cluster(group));
        }
        if let Some(pool) = label(labels, GKE_NODEPOOL) {
            return found(Provider::Gke, gke_cluster(name, pool));
        }
        if let Some(profile) = label(labels, MINIKUBE_NAME) {
            return found(Provider::Minikube, Some(profile.to_owned()));
        }
        if let Some(cluster) = k3d_cluster(name) {
            return found(Provider::K3d, Some(cluster));
        }
        match name {
            "docker-desktop" => return found(Provider::DockerDesktop, Some(name.to_owned())),
            "orbstack" => return found(Provider::OrbStack, Some(name.to_owned())),
            _ => {}
        }
        if label(labels, INSTANCE_TYPE).is_none()
            && label(labels, K0S_ROLE).is_none()
            && let Some(cluster) = kind_cluster(name)
        {
            return found(Provider::Kind, Some(cluster));
        }
    }
    if nodes
        .iter()
        .any(|node| label(node.labels.as_ref(), EKS_NODEGROUP).is_some())
    {
        return None;
    }
    let first = nodes.iter().find(|node| is_control_plane(node))?;
    let labels = first.labels.as_ref();
    let provider = match label(labels, INSTANCE_TYPE) {
        Some("k3s") => Provider::K3s,
        Some("rke2") => Provider::Rke2,
        _ if label(labels, K0S_ROLE).is_some() => Provider::K0s,
        _ => Provider::Node,
    };
    let name = first.name.as_deref().filter(|n| !is_address(n))?;
    found(provider, Some(name.to_owned()))
}

/// The cluster's name from the kubeconfig's name for it.
///
/// Without what tools add around it: `k3d-` and `kind-` prefixes, EKS' ARN
/// (`arn:aws:eks:<region>:<account>:cluster/<name>`), eksctl's `<name>.<region>.eksctl.io` and
/// GKE's `gke_<project>_<location>_<name>`.
#[must_use]
pub fn from_kubeconfig(name: &str) -> (String, Option<Provider>) {
    if name.starts_with("arn:aws:eks:")
        && let Some((_, cluster)) = name.rsplit_once(":cluster/")
        && !cluster.is_empty()
    {
        return (cluster.to_owned(), Some(Provider::Eks));
    }
    if let Some(rest) = name.strip_suffix(".eksctl.io")
        && let Some((cluster, _region)) = rest.rsplit_once('.')
        && !cluster.is_empty()
    {
        return (cluster.to_owned(), Some(Provider::Eks));
    }
    if let Some(rest) = name.strip_prefix("gke_") {
        let parts: Vec<&str> = rest.splitn(3, '_').collect();
        if let [_project, _location, cluster] = parts.as_slice()
            && !cluster.is_empty()
        {
            return ((*cluster).to_owned(), Some(Provider::Gke));
        }
    }
    for (prefix, provider) in [("k3d-", Provider::K3d), ("kind-", Provider::Kind)] {
        if let Some(cluster) = name.strip_prefix(prefix).filter(|c| !c.is_empty()) {
            return (cluster.to_owned(), Some(provider));
        }
    }
    let provider = match name {
        "minikube" => Some(Provider::Minikube),
        "docker-desktop" => Some(Provider::DockerDesktop),
        "orbstack" => Some(Provider::OrbStack),
        _ => None,
    };
    (name.to_owned(), provider)
}

/// Whether `host` is an address rather than a name: an IPv4 or IPv6 literal, or a name made of
/// one (`ip-10-0-1-2.ec2.internal`).
#[must_use]
pub fn is_address(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.contains(':') || host.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    let first = host.split('.').next().unwrap_or(host);
    first.strip_prefix("ip-").is_some_and(|rest| {
        let parts: Vec<&str> = rest.split('-').collect();
        parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok())
    })
}

fn is_control_plane(node: &ObjectMeta) -> bool {
    node.labels.as_ref().is_some_and(|labels| {
        labels.contains_key("node-role.kubernetes.io/control-plane")
            || labels.contains_key("node-role.kubernetes.io/master")
    }) || label(node.labels.as_ref(), K0S_ROLE) == Some("control-plane")
}

/// k3d names its nodes `k3d-<cluster>-server-<n>` and `k3d-<cluster>-agent-<n>`.
fn k3d_cluster(node: &str) -> Option<String> {
    let rest = node.strip_prefix("k3d-")?;
    let (head, index) = rest.rsplit_once('-')?;
    index.parse::<u32>().ok()?;
    let cluster = head
        .strip_suffix("-server")
        .or_else(|| head.strip_suffix("-agent"))?;
    (!cluster.is_empty()).then(|| cluster.to_owned())
}

/// kind names its nodes `<cluster>-control-plane[N]` and `<cluster>-worker[N]`.
fn kind_cluster(node: &str) -> Option<String> {
    let base = node.trim_end_matches(|c: char| c.is_ascii_digit());
    let cluster = base
        .strip_suffix("-control-plane")
        .or_else(|| base.strip_suffix("-worker"))?;
    (!cluster.is_empty()).then(|| cluster.to_owned())
}

/// GKE names nodes `gke-<cluster>-<pool>-<hash>-<suffix>`.
fn gke_cluster(node: &str, pool: &str) -> Option<String> {
    let rest = node.strip_prefix("gke-")?;
    let (cluster, _) = rest.rsplit_once(&format!("-{pool}-"))?;
    (!cluster.is_empty()).then(|| cluster.to_owned())
}

/// AKS' default node resource group is `MC_<resource group>_<cluster>_<region>`; the cluster is
/// the part before the region. A custom node resource group names no cluster.
fn aks_cluster(group: &str) -> Option<String> {
    let rest = group
        .strip_prefix("MC_")
        .or_else(|| group.strip_prefix("mc_"))?;
    let (rest, _region) = rest.rsplit_once('_')?;
    let (_group, cluster) = rest.rsplit_once('_')?;
    (!cluster.is_empty()).then(|| cluster.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn node(name: &str, labels: &serde_json::Value) -> ObjectMeta {
        serde_json::from_value(json!({"name": name, "labels": labels})).unwrap()
    }

    fn created(name: &str, at: &str, labels: &serde_json::Value) -> ObjectMeta {
        serde_json::from_value(json!({"name": name, "labels": labels, "creationTimestamp": at}))
            .unwrap()
    }

    fn detected(nodes: &[ObjectMeta]) -> Option<(String, Provider)> {
        detect(nodes).map(|d| (d.name, d.provider))
    }

    const CONTROL_PLANE: &str = "node-role.kubernetes.io/control-plane";

    #[test]
    fn eks_by_eksctls_label_and_without_one_no_name() {
        let labels = json!({EKSCTL_CLUSTER: "shop-prod", EKS_NODEGROUP: "ng-1"});
        assert_eq!(
            detected(&[node("ip-192-168-1-10.ec2.internal", &labels)]),
            Some(("shop-prod".to_owned(), Provider::Eks))
        );
        let managed = json!({EKS_NODEGROUP: "ng-1"});
        assert_eq!(
            detected(&[node("ip-192-168-1-10.ec2.internal", &managed)]),
            None
        );
    }

    #[test]
    fn gke_by_the_node_name_and_its_pool() {
        let labels = json!({GKE_NODEPOOL: "default-pool"});
        assert_eq!(
            detected(&[node("gke-shop-eu-default-pool-3f2a9c1d-x7k2", &labels)]),
            Some(("shop-eu".to_owned(), Provider::Gke))
        );
        assert_eq!(
            detected(&[node("gke-shop-eu-default-p-3f2a9c1d-x7k2", &labels)]),
            None
        );
    }

    #[test]
    fn aks_by_its_node_resource_group() {
        let labels = json!({AKS_CLUSTER: "MC_shop-rg_shop-aks_westeurope"});
        assert_eq!(
            detected(&[node("aks-nodepool1-12345678-vmss000000", &labels)]),
            Some(("shop-aks".to_owned(), Provider::Aks))
        );
        let custom = json!({AKS_CLUSTER: "shop-nodes"});
        assert_eq!(
            detected(&[node("aks-nodepool1-12345678-vmss000000", &custom)]),
            None
        );
    }

    #[test]
    fn k3d_by_its_server_and_agent_names() {
        let k3s = json!({INSTANCE_TYPE: "k3s", CONTROL_PLANE: "true"});
        assert_eq!(
            detected(&[node("k3d-zeabur-l2-server-0", &k3s)]),
            Some(("zeabur-l2".to_owned(), Provider::K3d))
        );
        assert_eq!(
            detected(&[node("k3d-dev-agent-1", &json!({INSTANCE_TYPE: "k3s"}))]),
            Some(("dev".to_owned(), Provider::K3d))
        );
    }

    #[test]
    fn kind_by_its_control_plane_and_worker_names() {
        let cp = json!({CONTROL_PLANE: ""});
        assert_eq!(
            detected(&[node("kind-control-plane", &cp)]),
            Some(("kind".to_owned(), Provider::Kind))
        );
        assert_eq!(
            detected(&[node("staging-worker2", &json!({}))]),
            Some(("staging".to_owned(), Provider::Kind))
        );
    }

    #[test]
    fn minikube_docker_desktop_and_orbstack() {
        let minikube = json!({MINIKUBE_NAME: "minikube", CONTROL_PLANE: ""});
        assert_eq!(
            detected(&[node("minikube", &minikube)]),
            Some(("minikube".to_owned(), Provider::Minikube))
        );
        let cp = json!({CONTROL_PLANE: ""});
        assert_eq!(
            detected(&[node("docker-desktop", &cp)]),
            Some(("docker-desktop".to_owned(), Provider::DockerDesktop))
        );
        assert_eq!(
            detected(&[node("orbstack", &cp)]),
            Some(("orbstack".to_owned(), Provider::OrbStack))
        );
    }

    #[test]
    fn k3s_rke2_and_k0s_by_the_first_control_plane() {
        let k3s = json!({INSTANCE_TYPE: "k3s", CONTROL_PLANE: "true"});
        let nodes = [
            created("vps-2", "2026-02-01T00:00:00Z", &k3s),
            created("vps-1", "2026-01-01T00:00:00Z", &k3s),
            created(
                "worker",
                "2025-01-01T00:00:00Z",
                &json!({INSTANCE_TYPE: "k3s"}),
            ),
        ];
        assert_eq!(detected(&nodes), Some(("vps-1".to_owned(), Provider::K3s)));
        let rke2 = json!({INSTANCE_TYPE: "rke2", CONTROL_PLANE: "true"});
        assert_eq!(
            detected(&[node("cp-a", &rke2)]),
            Some(("cp-a".to_owned(), Provider::Rke2))
        );
        let k0s = json!({K0S_ROLE: "control-plane"});
        assert_eq!(
            detected(&[node("k0s-main", &k0s)]),
            Some(("k0s-main".to_owned(), Provider::K0s))
        );
        let kubeadm = json!({"node-role.kubernetes.io/master": ""});
        assert_eq!(
            detected(&[node("cp1", &kubeadm)]),
            Some(("cp1".to_owned(), Provider::Node))
        );
    }

    #[test]
    fn an_address_or_no_control_plane_names_nothing() {
        let cp = json!({CONTROL_PLANE: ""});
        assert_eq!(detected(&[node("10.0.0.5", &cp)]), None);
        assert_eq!(detected(&[node("ip-10-0-0-5.ec2.internal", &cp)]), None);
        assert_eq!(detected(&[node("worker-a", &json!({}))]), None);
        assert_eq!(detected(&[]), None);
    }

    #[test]
    fn kubeconfig_names_lose_what_tools_add() {
        let cases = [
            ("k3d-zeabur-l2", "zeabur-l2", Some(Provider::K3d)),
            ("kind-dev", "dev", Some(Provider::Kind)),
            (
                "arn:aws:eks:eu-west-1:123456789012:cluster/shop-prod",
                "shop-prod",
                Some(Provider::Eks),
            ),
            (
                "shop-prod.eu-west-1.eksctl.io",
                "shop-prod",
                Some(Provider::Eks),
            ),
            (
                "gke_acme-123_europe-west1-b_shop-eu",
                "shop-eu",
                Some(Provider::Gke),
            ),
            ("minikube", "minikube", Some(Provider::Minikube)),
            (
                "docker-desktop",
                "docker-desktop",
                Some(Provider::DockerDesktop),
            ),
            ("orbstack", "orbstack", Some(Provider::OrbStack)),
            ("prod-eu", "prod-eu", None),
            ("k3d-", "k3d-", None),
        ];
        for (kubeconfig, name, provider) in cases {
            assert_eq!(
                from_kubeconfig(kubeconfig),
                (name.to_owned(), provider),
                "{kubeconfig}"
            );
        }
    }

    #[test]
    fn addresses_are_told_from_names() {
        for address in [
            "10.43.0.1",
            "[fd00::1]",
            "fd00::1",
            "ip-10-0-1-2.ec2.internal",
        ] {
            assert!(is_address(address), "{address}");
        }
        for name in ["zeabur.localhost", "vps-1", "ip-range", "k3d-dev-server-0"] {
            assert!(!is_address(name), "{name}");
        }
    }
}
