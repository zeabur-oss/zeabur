//! The Kubernetes distribution of a team install, K3s or RKE2, and the host files both share.
//!
//! RKE2's server is K3s's, so both read the same keys from `/etc/rancher/<name>/config.yaml`, then
//! `config.yaml.d/*.yaml` in name order, and both run helm-controller. K3s is one process with
//! `SQLite`: it takes workers but no second server. RKE2 runs the control plane as static pods
//! with etcd, and three servers keep it up when one fails. K3s gets Calico from the installer;
//! RKE2 brings Canal, Calico's policy engine on flannel.

use serde::{Deserialize, Serialize};

use crate::pins::{K3S, RKE2};

/// A team install's Kubernetes distribution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Distribution {
    /// K3s, hardened like RKE2's `cis` profile, with Calico. One server and any number of
    /// workers.
    #[default]
    K3s,
    /// RKE2 with its `cis` profile and Canal. Servers and workers.
    Rke2,
}

/// Where a distribution keeps its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Its configuration directory.
    pub config_dir: &'static str,
    /// The directory holding Zeabur Identity's authentication configuration.
    ///
    /// RKE2's kube-apiserver mounts the directory rather than the file: a file mount is pinned to
    /// one inode, so the atomic replace that installs a new configuration would never reach the
    /// pod. K3s's kube-apiserver runs on the host and reads the file itself.
    pub authn_dir: &'static str,
    /// The authentication configuration kube-apiserver re-reads about once a minute.
    pub authn_file: &'static str,
    /// The cluster-admin kubeconfig, kept for emergencies.
    pub admin_kubeconfig: &'static str,
    /// The `kubectl` that matches the server.
    pub kubectl: &'static str,
    /// Archives the distribution imports into containerd on every start.
    pub images_dir: &'static str,
    /// The join token of a joining node, mode 0600.
    pub join_token_file: &'static str,
    /// A server's own token; it joins servers and workers.
    pub server_token_file: &'static str,
    /// The kubelet's credentials on any node; a node may read its own Node object with them.
    pub kubelet_kubeconfig: &'static str,
    /// The kubelet's client certificate, whose subject names the node.
    pub kubelet_cert: &'static str,
    /// The registry mirrors and their CAs, read when the distribution starts.
    pub registries_file: &'static str,
    /// The CA of zot's certificate. containerd reads it on each pull, so the first server writes
    /// it once cert-manager has issued it, without a restart; a joining node writes it before
    /// starting.
    pub zot_ca_file: &'static str,
    /// The kernel parameters `protect-kernel-defaults` requires.
    pub sysctl_file: &'static str,
    /// The server's script that stops the distribution and removes it with its data.
    pub uninstall_server: &'static str,
    /// The worker's script.
    pub uninstall_agent: &'static str,
}

const K3S_LAYOUT: Layout = Layout {
    config_dir: "/etc/rancher/k3s",
    authn_dir: "/etc/rancher/k3s/zeabur-identity",
    authn_file: "/etc/rancher/k3s/zeabur-identity/authn.yaml",
    admin_kubeconfig: "/etc/rancher/k3s/k3s.yaml",
    kubectl: "/var/lib/rancher/k3s/data/current/bin/kubectl",
    images_dir: "/var/lib/rancher/k3s/agent/images",
    join_token_file: "/etc/rancher/k3s/zeabur-join-token",
    server_token_file: "/var/lib/rancher/k3s/server/token",
    kubelet_kubeconfig: "/var/lib/rancher/k3s/agent/kubelet.kubeconfig",
    kubelet_cert: "/var/lib/rancher/k3s/agent/client-kubelet.crt",
    registries_file: "/etc/rancher/k3s/registries.yaml",
    zot_ca_file: "/etc/rancher/k3s/zot-ca.crt",
    sysctl_file: "/etc/sysctl.d/60-k3s-cis.conf",
    uninstall_server: "/usr/local/bin/k3s-uninstall.sh",
    uninstall_agent: "/usr/local/bin/k3s-agent-uninstall.sh",
};

const RKE2_LAYOUT: Layout = Layout {
    config_dir: "/etc/rancher/rke2",
    authn_dir: "/etc/rancher/rke2/zeabur-identity",
    authn_file: "/etc/rancher/rke2/zeabur-identity/authn.yaml",
    admin_kubeconfig: "/etc/rancher/rke2/rke2.yaml",
    kubectl: "/var/lib/rancher/rke2/bin/kubectl",
    images_dir: "/var/lib/rancher/rke2/agent/images",
    join_token_file: "/etc/rancher/rke2/zeabur-join-token",
    server_token_file: "/var/lib/rancher/rke2/server/token",
    kubelet_kubeconfig: "/var/lib/rancher/rke2/agent/kubelet.kubeconfig",
    kubelet_cert: "/var/lib/rancher/rke2/agent/client-kubelet.crt",
    registries_file: "/etc/rancher/rke2/registries.yaml",
    zot_ca_file: "/etc/rancher/rke2/zot-ca.crt",
    sysctl_file: "/etc/sysctl.d/60-rke2-cis.conf",
    uninstall_server: "/usr/local/bin/rke2-uninstall.sh",
    uninstall_agent: "/usr/local/bin/rke2-uninstall.sh",
};

/// The OS at idle, which every team install has.
const OS_IDLE_MIB: u64 = 350;
/// Zeabur and Identity at idle.
const ZEABUR_IDLE_MIB: u64 = 60;
/// Traefik and cert-manager, what `minimal` installs, and room to spare.
const MINIMAL_AND_HEADROOM_MIB: u64 = 150 + 250;

impl Distribution {
    /// Every distribution, in the order an interface lists them.
    pub const ALL: [Self; 2] = [Self::K3s, Self::Rke2];

    /// Its name in the catalog's `distributions` lists and in configuration files.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::K3s => "k3s",
            Self::Rke2 => "rke2",
        }
    }

    /// Its name for people.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::K3s => "K3s",
            Self::Rke2 => "RKE2",
        }
    }

    /// Where it keeps its files.
    #[must_use]
    pub const fn layout(self) -> &'static Layout {
        match self {
            Self::K3s => &K3S_LAYOUT,
            Self::Rke2 => &RKE2_LAYOUT,
        }
    }

    /// The systemd unit of a server.
    #[must_use]
    pub const fn server_unit(self) -> &'static str {
        match self {
            Self::K3s => "k3s",
            Self::Rke2 => "rke2-server",
        }
    }

    /// The systemd unit of a worker.
    #[must_use]
    pub const fn agent_unit(self) -> &'static str {
        match self {
            Self::K3s => "k3s-agent",
            Self::Rke2 => "rke2-agent",
        }
    }

    /// The server port a node joins through: RKE2's supervisor, or K3s's, which shares the
    /// Kubernetes API's port.
    #[must_use]
    pub const fn join_port(self) -> u16 {
        match self {
            Self::K3s => 6443,
            Self::Rke2 => 9345,
        }
    }

    /// Whether another server can join, for a control plane that survives losing one.
    #[must_use]
    pub const fn takes_servers(self) -> bool {
        matches!(self, Self::Rke2)
    }

    /// The Kubernetes minor of the pinned release.
    #[must_use]
    pub const fn kubernetes(self) -> &'static str {
        match self {
            Self::K3s => K3S.kubernetes,
            Self::Rke2 => RKE2.kubernetes,
        }
    }

    /// The pinned release.
    #[must_use]
    pub const fn pinned_version(self) -> &'static str {
        match self {
            Self::K3s => K3S.version,
            Self::Rke2 => RKE2.version,
        }
    }

    /// The distribution and its control plane at idle, containerd, kubelet and the network
    /// included.
    ///
    /// RKE2: on a 4 GiB VM with the `standard` preset, `free` showed 2640 MiB used, about 500 of
    /// it the components. The rest is split by the process figures from that VM: kube-apiserver
    /// about 980, etcd 97, kube-controller-manager 92, canal 59, then kubelet, containerd and the
    /// rest of RKE2. K3s runs the control plane in one process on `SQLite`. On the same VM and
    /// preset, 10 minutes after a reboot, K3s with Calico used about 200 MiB less: `k3s server`
    /// and containerd 1030 against 1210 for RKE2's processes.
    #[must_use]
    pub const fn idle_mib(self) -> u64 {
        match self {
            Self::K3s => 1530,
            Self::Rke2 => 1730,
        }
    }

    /// What a team host uses at idle before any catalog component: the OS, the distribution, and
    /// Zeabur with Identity.
    #[must_use]
    pub const fn base_idle_mib(self) -> u64 {
        OS_IDLE_MIB + self.idle_mib() + ZEABUR_IDLE_MIB
    }

    /// The least memory a team install needs: the idle base, Traefik and cert-manager, and
    /// 250 MiB of headroom, rounded up to a hundred. A server with 4 GiB passes with either,
    /// also when a crash kernel reserve takes 512 MiB of it.
    #[must_use]
    pub const fn min_memory_mib(self) -> u64 {
        (self.base_idle_mib() + MINIMAL_AND_HEADROOM_MIB).div_ceil(100) * 100
    }

    /// The label `ServiceLB` puts on its pods.
    #[must_use]
    pub const fn servicelb_label(self) -> &'static str {
        match self {
            Self::K3s => "svccontroller.k3s.cattle.io/svcname",
            Self::Rke2 => "svccontroller.rke2.cattle.io/svcname",
        }
    }

    /// The group the supervisor lets join as an agent with a bootstrap token (k3s
    /// `NodeBootstrapTokenAuthGroup`).
    #[must_use]
    pub const fn bootstrap_group(self) -> &'static str {
        match self {
            Self::K3s => "system:bootstrappers:k3s:default-node-token",
            Self::Rke2 => "system:bootstrappers:rke2:default-node-token",
        }
    }

    /// The UDP port pods on different nodes reach each other through, and what uses it.
    #[must_use]
    pub const fn vxlan(self) -> (u16, &'static str) {
        match self {
            Self::K3s => (4789, "Calico VXLAN"),
            Self::Rke2 => (8472, "Canal VXLAN"),
        }
    }

    /// The packaged metrics-server, left out when the catalog installs its own.
    #[must_use]
    pub const fn packaged_metrics_server(self) -> &'static str {
        match self {
            Self::K3s => "metrics-server",
            Self::Rke2 => "rke2-metrics-server",
        }
    }

    /// The TCP ports a node listens on, and what for, that must be free before it starts.
    ///
    /// Every node takes 80 and 443: `ServiceLB` gives Traefik's `LoadBalancer` Service a pod on
    /// each node that holds them as host ports, so a request to any node's address reaches the
    /// ingress. A worker runs no API server or supervisor.
    #[must_use]
    pub fn node_ports(self, server: bool) -> Vec<(u16, &'static str)> {
        let mut ports = vec![
            (
                80,
                "HTTP ingress: ServiceLB holds it on every node, for when Traefik runs there",
            ),
            (
                443,
                "HTTPS ingress: ServiceLB holds it on every node, for when Traefik runs there",
            ),
        ];
        if server {
            ports.push((6443, "Kubernetes API"));
            if self == Self::Rke2 {
                ports.push((9345, "RKE2 supervisor"));
            }
        }
        ports.push((10250, "kubelet"));
        ports
    }

    /// The files of the first server, in the order to write them.
    #[must_use]
    pub fn server_files(self, options: &Options) -> Vec<HostFile> {
        match self {
            Self::K3s => crate::k3s::host_files(options),
            Self::Rke2 => crate::rke2::host_files(options),
        }
    }

    /// The files of a worker.
    #[must_use]
    pub fn agent_files(self) -> Vec<HostFile> {
        match self {
            Self::K3s => crate::k3s::agent_files(),
            Self::Rke2 => crate::rke2::agent_files(),
        }
    }

    /// The kernel parameters `protect-kernel-defaults` makes the kubelet check; it refuses to
    /// start when they differ. Every node needs them.
    #[must_use]
    pub fn sysctl(self) -> HostFile {
        HostFile {
            path: self.layout().sysctl_file.to_owned(),
            mode: 0o644,
            content: format!(
                "# Written by zeabur-installer: the kernel parameters {} requires.\n\
                 vm.panic_on_oom=0\nvm.overcommit_memory=1\nkernel.panic=10\nkernel.panic_on_oops=1\n",
                self.title()
            ),
        }
    }

    /// The drop-in naming the server to join and the token file.
    #[must_use]
    pub fn join_file(self, server: &str) -> HostFile {
        let layout = self.layout();
        HostFile {
            path: format!("{}/config.yaml.d/10-join.yaml", layout.config_dir),
            mode: 0o600,
            content: format!(
                "{HEADER}server: {server}\ntoken-file: {}\n",
                layout.join_token_file
            ),
        }
    }

    /// The node's container runtime cannot resolve `zot.zot.svc`, so its images come from zot's
    /// fixed `ClusterIP`, which kube-proxy routes on every node and zot's certificate names.
    #[must_use]
    pub fn registries_file(self, cluster_ip: &str) -> HostFile {
        let layout = self.layout();
        HostFile {
            path: layout.registries_file.to_owned(),
            mode: 0o600,
            content: format!(
                "# Written by zeabur-installer; `zeabur-installer doctor` compares it.\n\
                 mirrors:\n  {ZOT_HOST}:\n    endpoint: [\"https://{cluster_ip}:5000\"]\n\
                 configs:\n  \"{cluster_ip}:5000\":\n    tls:\n      ca_file: {}\n",
                layout.zot_ca_file
            ),
        }
    }

    /// zot's CA on a joining node, copied from the server by `node add`.
    #[must_use]
    pub fn zot_ca_file(self, pem: &str) -> HostFile {
        HostFile {
            path: self.layout().zot_ca_file.to_owned(),
            mode: 0o644,
            content: pem.to_owned(),
        }
    }

    /// The drop-in naming the node, so a later change of the host name does not register a
    /// second node.
    #[must_use]
    pub fn node_name_file(self, name: &str) -> HostFile {
        HostFile {
            path: self.node_name_path(),
            mode: 0o600,
            content: format!("{HEADER}node-name: {name}\n"),
        }
    }

    /// Where [`Self::node_name_file`] goes.
    #[must_use]
    pub fn node_name_path(self) -> String {
        format!(
            "{}/config.yaml.d/15-node-name.yaml",
            self.layout().config_dir
        )
    }

    /// The drop-in naming the addresses the node registers with, so it does not pick a temporary
    /// IPv6 address. `None` when the host's addresses could not be read.
    #[must_use]
    pub fn node_ip_file(self, addresses: &[String]) -> Option<HostFile> {
        if addresses.is_empty() {
            return None;
        }
        let mut content = format!("{HEADER}node-ip:\n");
        for address in addresses {
            for part in ["  - ", address, "\n"] {
                content.push_str(part);
            }
        }
        Some(HostFile {
            path: format!("{}/config.yaml.d/20-node-ip.yaml", self.layout().config_dir),
            mode: 0o600,
            content,
        })
    }
}

/// A file the installer owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFile {
    /// Absolute path.
    pub path: String,
    /// Unix mode.
    pub mode: u32,
    /// Exact content.
    pub content: String,
}

/// Server settings that depend on the chosen components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Extra API server certificate names.
    pub tls_san: Vec<String>,
    /// Packaged components left out, such as `rke2-metrics-server` when the catalog installs
    /// metrics-server.
    pub disable: Vec<String>,
    /// Zeabur Identity is installed, so kube-apiserver reads its issuer.
    pub identity: bool,
}

/// The `ConfigMap` in `kube-system` holding the authentication configuration, for servers that
/// join later.
pub const AUTHN_CONFIGMAP: &str = "zeabur-identity-authn";
/// The catalog's in-cluster registry, as Zeabur and Zeabur Builder name its images.
pub const ZOT_HOST: &str = "zot.zot.svc:5000";
/// The fixed `ClusterIP` of the catalog's zot (its `cluster-ip` input). Kubernetes keeps the low
/// end of the Service CIDR (10.43.0.0/16 on K3s and RKE2) for fixed addresses, as `CoreDNS`'s
/// 10.43.0.10.
pub const ZOT_CLUSTER_IP: &str = "10.43.0.50";

pub(crate) const HEADER: &str = "# Written by zeabur-installer; `zeabur-installer doctor` compares it.\n\
                      # Put your own settings in config.yaml.d/90-local.yaml instead of editing this file.\n";

/// Trusts no issuer yet. Zeabur Identity replaces it once it serves its discovery document, and
/// kube-apiserver picks the new file up without a restart.
///
/// K3s, and RKE2 with it, drops `--anonymous-auth=false` when an authentication configuration
/// file is set, so the file turns anonymous access off again, as
/// `zeabur-identityctl kube-config --distro k3s` writes it. kube-apiserver's probes in RKE2 send
/// a client certificate and do not need anonymous access.
pub const EMPTY_AUTHN: &str = "apiVersion: apiserver.config.k8s.io/v1\n\
                               kind: AuthenticationConfiguration\n\
                               anonymous:\n  enabled: false\n\
                               jwt: []\n";

/// Metadata only, never bodies, so Secret values stay out of the log. Every request on Secrets
/// is kept, reads included, with who made it. Of the rest, only changes by people and workloads
/// are kept: reads, events, leases, token and access reviews, and requests from nodes and
/// control-plane components are frequent and routine. The first rule that matches decides. The
/// log keeps 30 days, 10 files of 100 MB, under `/var/lib/rancher/<name>/server/logs`.
pub(crate) const AUDIT_POLICY: &str = r#"# Written by zeabur-installer.
apiVersion: audit.k8s.io/v1
kind: Policy
omitStages: [RequestReceived]
rules:
  - level: Metadata
    resources:
      - group: ""
        resources: [secrets]
  - level: None
    nonResourceURLs: [/healthz*, /livez*, /readyz*, /version]
  - level: None
    userGroups: [system:nodes]
  - level: None
    users: [system:apiserver, system:kube-controller-manager, system:kube-scheduler, system:kube-proxy]
  - level: None
    verbs: [get, list, watch]
  - level: None
    resources:
      - group: ""
        resources: [events]
      - group: events.k8s.io
        resources: [events]
      - group: coordination.k8s.io
        resources: [leases]
      - group: authentication.k8s.io
        resources: [tokenreviews]
      - group: authorization.k8s.io
        resources: [subjectaccessreviews]
  - level: Metadata
"#;

/// The kubelet setting both distributions get: a fork bomb in one pod cannot take the node's
/// process table.
pub(crate) const KUBELET_ARGS: &str = "kubelet-arg:\n  - pod-max-pids=4096\n";

/// A YAML list under `key`, or nothing when it is empty.
pub(crate) fn yaml_list(out: &mut String, key: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    out.push_str(key);
    out.push_str(":\n");
    for item in items {
        out.push_str("  - ");
        out.push_str(item);
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn k3s_needs_less_memory_than_rke2_and_both_fit_4_gib_with_a_crash_kernel() {
        assert!(Distribution::K3s.min_memory_mib() < Distribution::Rke2.min_memory_mib());
        for d in Distribution::ALL {
            assert!(d.min_memory_mib() <= 3379, "{d:?}");
        }
    }

    #[test]
    fn the_audit_policy_keeps_every_secret_request_first_and_never_bodies() {
        let policy: serde_json::Value = serde_saphyr::from_str(AUDIT_POLICY).unwrap();
        let rules = policy["rules"].as_array().unwrap();
        assert_eq!(
            rules[0],
            serde_json::json!({"level": "Metadata",
                               "resources": [{"group": "", "resources": ["secrets"]}]})
        );
        assert!(
            rules
                .iter()
                .all(|r| ["None", "Metadata"].contains(&r["level"].as_str().unwrap()))
        );
    }

    #[test]
    fn join_files_name_the_server_and_the_node_addresses() {
        for d in Distribution::ALL {
            let join = d.join_file("https://10.0.0.5:6443");
            let value: serde_json::Value = serde_saphyr::from_str(&join.content).unwrap();
            assert_eq!(value["server"], "https://10.0.0.5:6443");
            assert_eq!(value["token-file"], d.layout().join_token_file);
            assert_eq!(join.mode, 0o600);
            assert!(join.path.starts_with(d.layout().config_dir));
            let ip = d
                .node_ip_file(&["10.0.0.6".to_owned(), "fd00::6".to_owned()])
                .unwrap();
            let value: serde_json::Value = serde_saphyr::from_str(&ip.content).unwrap();
            assert_eq!(value["node-ip"], serde_json::json!(["10.0.0.6", "fd00::6"]));
            assert_eq!(d.node_ip_file(&[]), None);
        }
    }

    #[test]
    fn only_rke2_listens_on_a_separate_supervisor_port() {
        let has = |d: Distribution, port| d.node_ports(true).iter().any(|(p, _)| *p == port);
        assert!(has(Distribution::Rke2, 9345));
        assert!(!has(Distribution::K3s, 9345));
        for d in Distribution::ALL {
            assert!(has(d, d.join_port()));
            assert!(!d.node_ports(false).iter().any(|(p, _)| *p == 6443));
        }
    }
}
