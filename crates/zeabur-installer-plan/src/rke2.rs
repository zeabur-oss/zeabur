//! Host files written before RKE2 first starts.
//!
//! RKE2's `cis` profile already does most of the CIS benchmark: Secrets are encrypted at rest,
//! Pod Security `restricted` is enforced outside `kube-system`, `NetworkPolicies` guard the system
//! namespaces and the service-account and kubelet flags are set
//! (<https://docs.rke2.io/security/hardening_guide>). What it leaves to the host is written here:
//! the kernel parameters it refuses to start without, an audit policy (RKE2's own logs nothing),
//! and the file kube-apiserver reads Zeabur Identity's issuer from, which must exist before the
//! first start because RKE2 mounts only files that exist.
//!
//! RKE2 reads `config.yaml`, then `config.yaml.d/*.yaml` in name order; a later file replaces a key
//! unless the key ends in `+`. Each key here is set in exactly one file, so a person's own
//! `config.yaml.d/90-*.yaml` overrides it cleanly.

/// RKE2's configuration directory.
pub const CONFIG_DIR: &str = "/etc/rancher/rke2";
/// The directory holding Zeabur Identity's authentication configuration.
///
/// kube-apiserver mounts the directory rather than the file: a file mount is pinned to one inode,
/// so the atomic replace that installs a new configuration would never reach the pod.
pub const AUTHN_DIR: &str = "/etc/rancher/rke2/zeabur-identity";
/// The authentication configuration kube-apiserver re-reads about once a minute.
pub const AUTHN_FILE: &str = "/etc/rancher/rke2/zeabur-identity/authn.yaml";
/// The `ConfigMap` in `kube-system` holding the authentication configuration, for servers that
/// join later.
pub const AUTHN_CONFIGMAP: &str = "zeabur-identity-authn";
/// The cluster-admin kubeconfig, kept for emergencies.
pub const ADMIN_KUBECONFIG: &str = "/etc/rancher/rke2/rke2.yaml";
/// kubectl, crictl and ctr.
pub const BIN_DIR: &str = "/var/lib/rancher/rke2/bin";
/// Archives RKE2 imports into containerd on every start.
pub const IMAGES_DIR: &str = "/var/lib/rancher/rke2/agent/images";
/// Where the tarball installs.
pub const PREFIX: &str = "/usr/local";
/// The join token of a joining node, mode 0600.
pub const JOIN_TOKEN_FILE: &str = "/etc/rancher/rke2/zeabur-join-token";
/// A server's own token; it joins servers and workers.
pub const SERVER_TOKEN_FILE: &str = "/var/lib/rancher/rke2/server/token";
/// The kubelet's credentials on any node; a node may read its own Node object with them.
pub const KUBELET_KUBECONFIG: &str = "/var/lib/rancher/rke2/agent/kubelet.kubeconfig";
/// The catalog's in-cluster registry, as Zeabur and Zeabur Builder name its images.
pub const ZOT_HOST: &str = "zot.zot.svc:5000";
/// The fixed `ClusterIP` of the catalog's zot (its `cluster-ip` input). Kubernetes keeps the low
/// end of the Service CIDR (RKE2's 10.43.0.0/16) for fixed addresses, as `CoreDNS`'s 10.43.0.10.
pub const ZOT_CLUSTER_IP: &str = "10.43.0.50";
/// The registry mirrors and their CAs, read when RKE2 starts.
pub const REGISTRIES_FILE: &str = "/etc/rancher/rke2/registries.yaml";
/// The CA of zot's certificate. containerd reads it on each pull, so the first server writes it
/// once cert-manager has issued it, without a restart; a joining node writes it before starting.
pub const ZOT_CA_FILE: &str = "/etc/rancher/rke2/zot-ca.crt";

/// The TCP ports a node listens on, and what for, that must be free before RKE2 starts.
///
/// Every node takes 80 and 443: `ServiceLB`, which RKE2 runs with `enable-servicelb`, gives
/// Traefik's `LoadBalancer` Service a pod on each node that holds them as host ports, so a request
/// to any node's address reaches the ingress. A worker runs no API server or supervisor.
#[must_use]
pub fn node_ports(server: bool) -> Vec<(u16, &'static str)> {
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
        ports.extend([(6443, "Kubernetes API"), (9345, "RKE2 supervisor")]);
    }
    ports.push((10250, "kubelet"));
    ports
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

/// RKE2 settings that depend on the chosen components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Extra API server certificate names.
    pub tls_san: Vec<String>,
    /// RKE2's packaged components left out, such as `rke2-metrics-server` when the catalog
    /// installs metrics-server.
    pub disable: Vec<String>,
    /// Zeabur Identity is installed, so kube-apiserver reads its issuer.
    pub identity: bool,
}

const HEADER: &str = "# Written by zeabur-installer; `zeabur-installer doctor` compares it.\n\
                      # Put your own settings in config.yaml.d/90-local.yaml instead of editing this file.\n";

/// The files to write, in the order to write them.
#[must_use]
pub fn host_files(options: &Options) -> Vec<HostFile> {
    let mut files = vec![
        sysctl(),
        HostFile {
            path: format!("{CONFIG_DIR}/audit-policy.yaml"),
            mode: 0o600,
            content: AUDIT_POLICY.to_owned(),
        },
        HostFile {
            path: format!("{CONFIG_DIR}/config.yaml"),
            mode: 0o600,
            content: config(options),
        },
    ];
    if options.identity {
        files.push(HostFile {
            path: format!("{CONFIG_DIR}/config.yaml.d/50-zeabur-identity.yaml"),
            mode: 0o600,
            content: format!(
                "{HEADER}kube-apiserver-arg:\n  - authentication-config={AUTHN_FILE}\n\
                 kube-apiserver-extra-mount:\n  - {AUTHN_DIR}:{AUTHN_DIR}:ro\n"
            ),
        });
        files.push(HostFile {
            path: AUTHN_FILE.to_owned(),
            mode: 0o600,
            content: EMPTY_AUTHN.to_owned(),
        });
    }
    files
}

/// The kernel parameters of RKE2's rke2-cis-sysctl.conf; `protect-kernel-defaults` makes the
/// kubelet refuse to start when they differ. Every node needs them.
fn sysctl() -> HostFile {
    HostFile {
        path: "/etc/sysctl.d/60-rke2-cis.conf".to_owned(),
        mode: 0o644,
        content: "# Written by zeabur-installer: the kernel parameters RKE2's cis profile requires.\n\
                  vm.panic_on_oom=0\nvm.overcommit_memory=1\nkernel.panic=10\nkernel.panic_on_oops=1\n"
            .to_owned(),
    }
}

/// The files of a worker: the kernel parameters and an agent `config.yaml` with the server's
/// kubelet settings. The agent takes no server keys, so this is not the server's file.
#[must_use]
pub fn agent_files() -> Vec<HostFile> {
    vec![
        sysctl(),
        HostFile {
            path: format!("{CONFIG_DIR}/config.yaml"),
            mode: 0o600,
            content: format!("{HEADER}profile: cis\n{KUBELET_ARGS}"),
        },
    ]
}

/// The drop-in naming the server to join and the token file.
#[must_use]
pub fn join_file(server: &str) -> HostFile {
    HostFile {
        path: format!("{CONFIG_DIR}/config.yaml.d/10-join.yaml"),
        mode: 0o600,
        content: format!("{HEADER}server: {server}\ntoken-file: {JOIN_TOKEN_FILE}\n"),
    }
}

/// The node's container runtime cannot resolve `zot.zot.svc`, so its images come from zot's fixed
/// `ClusterIP`, which kube-proxy routes on every node and zot's certificate names.
#[must_use]
pub fn registries_file(cluster_ip: &str) -> HostFile {
    HostFile {
        path: REGISTRIES_FILE.to_owned(),
        mode: 0o600,
        content: format!(
            "# Written by zeabur-installer; `zeabur-installer doctor` compares it.\n\
             mirrors:\n  {ZOT_HOST}:\n    endpoint: [\"https://{cluster_ip}:5000\"]\n\
             configs:\n  \"{cluster_ip}:5000\":\n    tls:\n      ca_file: {ZOT_CA_FILE}\n"
        ),
    }
}

/// zot's CA on a joining node, copied from the server by `node add`.
#[must_use]
pub fn zot_ca_file(pem: &str) -> HostFile {
    HostFile {
        path: ZOT_CA_FILE.to_owned(),
        mode: 0o644,
        content: pem.to_owned(),
    }
}

/// The drop-in naming the addresses the node registers with, so RKE2 does not pick a temporary
/// IPv6 address. `None` when the host's addresses could not be read.
#[must_use]
pub fn node_ip_file(addresses: &[String]) -> Option<HostFile> {
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
        path: format!("{CONFIG_DIR}/config.yaml.d/20-node-ip.yaml"),
        mode: 0o600,
        content,
    })
}

const KUBELET_ARGS: &str = "kubelet-arg:\n  - pod-max-pids=4096\n";

/// Trusts no issuer yet. Zeabur Identity replaces it once it serves its discovery document, and
/// kube-apiserver picks the new file up without a restart.
///
/// RKE2's server is k3s's, which drops `--anonymous-auth=false` when an authentication
/// configuration file is set, so the file turns anonymous access off again, as
/// `zeabur-identityctl kube-config --distro k3s` writes it. kube-apiserver's probes in RKE2 send
/// a client certificate and do not need anonymous access.
pub const EMPTY_AUTHN: &str = "apiVersion: apiserver.config.k8s.io/v1\n\
                               kind: AuthenticationConfiguration\n\
                               anonymous:\n  enabled: false\n\
                               jwt: []\n";

/// Metadata only, never bodies, so Secret values stay out of the log. Every request on Secrets
/// is kept, reads included, with who made it. Of the rest, only changes by people and workloads
/// are kept: reads, events, leases, token and access reviews, and requests from nodes and
/// control-plane components are frequent and routine. The first rule that matches decides. RKE2
/// keeps 30 days, 10 files of 100 MB, in /var/lib/rancher/rke2/server/logs/audit.log.
const AUDIT_POLICY: &str = r#"# Written by zeabur-installer.
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

fn config(options: &Options) -> String {
    let mut out = String::from(HEADER);
    out.push_str(
        "profile: cis\n\
         write-kubeconfig-mode: \"0600\"\n\
         # Traefik comes from the Zeabur catalog, and ServiceLB gives it ports 80 and 443.\n\
         ingress-controller: none\n\
         enable-servicelb: true\n\
         audit-policy-file: /etc/rancher/rke2/audit-policy.yaml\n",
    );
    out.push_str(KUBELET_ARGS);
    if !options.disable.is_empty() {
        out.push_str("disable:\n");
        for name in &options.disable {
            out.push_str("  - ");
            out.push_str(name);
            out.push('\n');
        }
    }
    if !options.tls_san.is_empty() {
        out.push_str("tls-san:\n");
        for san in &options.tls_san {
            out.push_str("  - ");
            out.push_str(san);
            out.push('\n');
        }
    }
    out
}

/// The `etcd` user the cis profile runs etcd as. It must be a local user in /etc/passwd.
pub const ETCD_USER: &[&str] = &[
    "useradd",
    "-r",
    "-c",
    "etcd user",
    "-s",
    "/sbin/nologin",
    "-M",
    "etcd",
    "-U",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_adds_the_mount_and_the_empty_file() {
        let files = host_files(&Options {
            tls_san: vec![],
            disable: vec!["rke2-metrics-server".to_owned()],
            identity: true,
        });
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/etc/sysctl.d/60-rke2-cis.conf",
                "/etc/rancher/rke2/audit-policy.yaml",
                "/etc/rancher/rke2/config.yaml",
                "/etc/rancher/rke2/config.yaml.d/50-zeabur-identity.yaml",
                "/etc/rancher/rke2/zeabur-identity/authn.yaml",
            ]
        );
        assert!(
            files[2]
                .content
                .contains("disable:\n  - rke2-metrics-server\n")
        );
        assert!(
            files[3]
                .content
                .contains("/etc/rancher/rke2/zeabur-identity:/etc/rancher/rke2/zeabur-identity:ro")
        );
    }

    #[test]
    fn config_parses_as_yaml() {
        let options = Options {
            tls_san: vec!["k8s.example.com".to_owned()],
            disable: vec![],
            identity: false,
        };
        let files = host_files(&options);
        for file in files.iter().filter(|f| {
            std::path::Path::new(&f.path)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("yaml"))
        }) {
            let value: serde_json::Value = serde_saphyr::from_str(&file.content).unwrap();
            assert!(value.is_object(), "{}", file.path);
        }
        assert!(files[2].content.contains("tls-san:\n  - k8s.example.com\n"));
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
    fn join_files_parse_and_keep_the_kubelet_settings() {
        let server = host_files(&Options {
            tls_san: vec![],
            disable: vec![],
            identity: false,
        });
        let agent = agent_files();
        assert_eq!(agent[0], server[0]);
        assert!(agent[1].content.contains("profile: cis\n"));
        assert!(agent[1].content.contains("pod-max-pids=4096"));
        assert!(!agent[1].content.contains("audit-policy-file"));
        let join = join_file("https://10.0.0.5:9345");
        let value: serde_json::Value = serde_saphyr::from_str(&join.content).unwrap();
        assert_eq!(value["server"], "https://10.0.0.5:9345");
        assert_eq!(value["token-file"], JOIN_TOKEN_FILE);
        assert_eq!(join.mode, 0o600);
        let ip = node_ip_file(&["10.0.0.6".to_owned(), "fd00::6".to_owned()]).unwrap();
        let value: serde_json::Value = serde_saphyr::from_str(&ip.content).unwrap();
        assert_eq!(value["node-ip"], serde_json::json!(["10.0.0.6", "fd00::6"]));
        assert_eq!(node_ip_file(&[]), None);
    }
}
