//! Host files written before K3s first starts, and the Calico manifest it runs.
//!
//! K3s hardened as its CIS guide says (<https://docs.k3s.io/security/hardening-guide>), to match
//! what RKE2's `cis` profile does by itself: Secrets encrypted at rest, Pod Security `restricted`
//! by default outside `kube-system`, `NetworkPolicies` that guard the system namespaces, an audit
//! log, `NodeRestriction` and `EventRateLimit`, and the kubelet and service-account flags. The
//! default level applies only to a namespace without its own Pod Security labels, as on RKE2.
//!
//! Calico replaces flannel and K3s's own policy controller, so `NetworkPolicies` behave as on
//! RKE2's Canal: the same Calico release enforces them.

use serde_json::{Value, json};

use crate::PlanError;
use crate::cluster::{
    AUDIT_POLICY, Distribution, EMPTY_AUTHN, HEADER, HostFile, KUBELET_ARGS, Options, yaml_list,
};
use crate::pins::CALICO;
use crate::plan::MANAGER;

/// The binary; `install.sh` expects it here.
pub const BINARY: &str = "/usr/local/bin/k3s";
/// The audit log's directory. Only root may read it, so K3s does not make it itself.
pub const AUDIT_LOG_DIR: &str = "/var/lib/rancher/k3s/server/logs";
/// Manifests K3s applies on every start and keeps applied.
pub const MANIFESTS_DIR: &str = "/var/lib/rancher/k3s/server/manifests";
/// The pod network: K3s's default cluster CIDR, which Calico's pool must match.
pub const POD_CIDR: &str = "10.42.0.0/16";

const K3S: Distribution = Distribution::K3s;

/// The files to write, in the order to write them.
#[must_use]
pub fn host_files(options: &Options) -> Vec<HostFile> {
    let layout = K3S.layout();
    let dir = layout.config_dir;
    let mut files = vec![
        K3S.sysctl(),
        HostFile {
            path: format!("{dir}/audit-policy.yaml"),
            mode: 0o600,
            content: AUDIT_POLICY.to_owned(),
        },
        HostFile {
            path: format!("{dir}/admission.yaml"),
            mode: 0o600,
            content: ADMISSION.to_owned(),
        },
        HostFile {
            path: format!("{dir}/config.yaml"),
            mode: 0o600,
            content: config(options),
        },
        HostFile {
            path: format!("{MANIFESTS_DIR}/zeabur-cis.yaml"),
            mode: 0o600,
            content: system_objects(),
        },
    ];
    if options.identity {
        files.push(HostFile {
            path: format!("{dir}/config.yaml.d/50-zeabur-identity.yaml"),
            mode: 0o600,
            content: format!(
                "{HEADER}kube-apiserver-arg+:\n  - authentication-config={}\n",
                layout.authn_file
            ),
        });
        files.push(HostFile {
            path: layout.authn_file.to_owned(),
            mode: 0o600,
            content: EMPTY_AUTHN.to_owned(),
        });
    }
    files
}

/// The files of a worker: the kernel parameters and the kubelet settings of the server.
#[must_use]
pub fn agent_files() -> Vec<HostFile> {
    vec![
        K3S.sysctl(),
        HostFile {
            path: format!("{}/config.yaml", K3S.layout().config_dir),
            mode: 0o600,
            content: format!("{HEADER}protect-kernel-defaults: true\n{KUBELET_ARGS}"),
        },
    ]
}

fn config(options: &Options) -> String {
    let dir = K3S.layout().config_dir;
    let mut out = format!(
        "{HEADER}protect-kernel-defaults: true\n\
         secrets-encryption: true\n\
         write-kubeconfig-mode: \"0600\"\n\
         # Calico, applied by the installer, takes the place of flannel and of K3s's policy controller.\n\
         flannel-backend: none\n\
         disable-network-policy: true\n\
         kube-apiserver-arg:\n  \
           - enable-admission-plugins=NodeRestriction,EventRateLimit\n  \
           - admission-control-config-file={dir}/admission.yaml\n  \
           - audit-policy-file={dir}/audit-policy.yaml\n  \
           - audit-log-path={AUDIT_LOG_DIR}/audit.log\n  \
           - audit-log-maxage=30\n  \
           - audit-log-maxbackup=10\n  \
           - audit-log-maxsize=100\n  \
           - service-account-extend-token-expiration=false\n\
         kube-controller-manager-arg:\n  \
           - terminated-pod-gc-threshold=1000\n"
    );
    out.push_str(KUBELET_ARGS);
    // Traefik and local-path come from the Zeabur catalog and the installer; ServiceLB stays and
    // gives Traefik ports 80 and 443.
    let mut disable = vec!["traefik".to_owned(), "local-storage".to_owned()];
    disable.extend(options.disable.iter().cloned());
    yaml_list(&mut out, "disable", &disable);
    yaml_list(&mut out, "tls-san", &options.tls_san);
    out
}

/// Pod Security `restricted` for every namespace without its own labels, `kube-system` exempt, as
/// RKE2's `cis` profile writes it; and a cluster-wide limit on Events, which a flood of them from
/// one workload would otherwise turn into load on the datastore.
const ADMISSION: &str = "# Written by zeabur-installer.
apiVersion: apiserver.config.k8s.io/v1
kind: AdmissionConfiguration
plugins:
  - name: PodSecurity
    configuration:
      apiVersion: pod-security.admission.config.k8s.io/v1
      kind: PodSecurityConfiguration
      defaults:
        enforce: restricted
        enforce-version: latest
        audit: restricted
        audit-version: latest
        warn: restricted
        warn-version: latest
      exemptions:
        usernames: []
        runtimeClasses: []
        namespaces: [kube-system]
  - name: EventRateLimit
    configuration:
      apiVersion: eventratelimit.admission.k8s.io/v1alpha1
      kind: Configuration
      limits:
        - type: Server
          qps: 5000
          burst: 20000
";

/// What RKE2's `cis` profile creates in the system namespaces and K3s does not: a policy that
/// lets in only the namespace's own pods, `CoreDNS` reachable from every pod, and default service
/// accounts that mount no token. K3s applies the file on every start, as RKE2 re-creates its own.
fn system_objects() -> String {
    let mut objects: Vec<Value> = ["kube-system", "kube-public", "default"]
        .into_iter()
        .map(|namespace| {
            policy(
                "default-network-policy",
                namespace,
                &json!({}),
                &json!([{"from": [{"podSelector": {}}]}]),
            )
        })
        .collect();
    objects.push(policy(
        "default-network-dns-policy",
        "kube-system",
        &json!({"matchLabels": {"k8s-app": "kube-dns"}}),
        &json!([{"ports": [
            {"port": 53, "protocol": "TCP"},
            {"port": 53, "protocol": "UDP"},
        ]}]),
    ));
    objects.extend(
        ["kube-system", "kube-public", "default", "kube-node-lease"]
            .into_iter()
            .map(|namespace| {
                json!({
                    "apiVersion": "v1",
                    "kind": "ServiceAccount",
                    "metadata": {"name": "default", "namespace": namespace},
                    "automountServiceAccountToken": false,
                })
            }),
    );
    let mut out = String::from("# Written by zeabur-installer; K3s applies it on every start.\n");
    for object in objects {
        out.push_str("---\n");
        out.push_str(&serde_json::to_string(&object).unwrap_or_default());
        out.push('\n');
    }
    out
}

fn policy(name: &str, namespace: &str, pods: &Value, ingress: &Value) -> Value {
    json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "NetworkPolicy",
        "metadata": {
            "name": name,
            "namespace": namespace,
            "labels": {"app.kubernetes.io/managed-by": MANAGER},
        },
        "spec": {"podSelector": pods, "policyTypes": ["Ingress"], "ingress": ingress},
    })
}

/// Calico's VXLAN manifest with K3s's pod network and the images pinned by digest.
///
/// IP forwarding in pods stays off, as with RKE2's Canal; `ServiceLB`'s pods turn it on with their
/// own sysctl.
///
/// # Errors
///
/// When the manifest does not have the expected text, which a checked download always has.
pub fn calico_manifest(manifest: &str) -> Result<String, PlanError> {
    let mut out = manifest.to_owned();
    let edits = [(
        "            # - name: CALICO_IPV4POOL_CIDR\n            #   value: \"192.168.0.0/16\"\n"
            .to_owned(),
        format!("            - name: CALICO_IPV4POOL_CIDR\n              value: \"{POD_CIDR}\"\n"),
    )];
    for (from, to) in edits
        .into_iter()
        .chain(CALICO.images.iter().map(|(tag, digest)| {
            (
                format!("image: {tag}\n"),
                format!("image: {tag}@{digest}\n"),
            )
        }))
    {
        if !out.contains(&from) {
            return Err(PlanError::Config(format!(
                "Calico {} manifest: `{}` not found",
                CALICO.version,
                from.trim()
            )));
        }
        out = out.replace(&from, &to);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(identity: bool) -> Options {
        Options {
            tls_san: vec!["k8s.example.com".to_owned()],
            disable: vec!["metrics-server".to_owned()],
            identity,
        }
    }

    fn yaml(content: &str) -> Value {
        serde_saphyr::from_str(content).unwrap()
    }

    #[test]
    fn the_server_is_hardened_as_the_k3s_cis_guide_says() {
        let files = host_files(&options(false));
        let config = files
            .iter()
            .find(|f| f.path == "/etc/rancher/k3s/config.yaml")
            .unwrap();
        let config = yaml(&config.content);
        assert_eq!(config["protect-kernel-defaults"], true);
        assert_eq!(config["secrets-encryption"], true);
        assert_eq!(config["disable-network-policy"], true);
        assert_eq!(config["flannel-backend"], "none");
        let apiserver = config["kube-apiserver-arg"].as_array().unwrap();
        for arg in [
            "enable-admission-plugins=NodeRestriction,EventRateLimit",
            "admission-control-config-file=/etc/rancher/k3s/admission.yaml",
            "audit-log-path=/var/lib/rancher/k3s/server/logs/audit.log",
            "service-account-extend-token-expiration=false",
        ] {
            assert!(apiserver.contains(&json!(arg)), "{arg}");
        }
        let disable = config["disable"].as_array().unwrap();
        for packaged in ["traefik", "local-storage", "metrics-server"] {
            assert!(disable.contains(&json!(packaged)), "{packaged}");
        }
        assert!(!disable.contains(&json!("servicelb")));
        assert_eq!(config["tls-san"], json!(["k8s.example.com"]));
    }

    #[test]
    fn pods_are_restricted_outside_kube_system() {
        let admission = yaml(ADMISSION);
        let psa = &admission["plugins"][0]["configuration"];
        assert_eq!(psa["defaults"]["enforce"], "restricted");
        assert_eq!(psa["exemptions"]["namespaces"], json!(["kube-system"]));
        assert_eq!(psa["exemptions"]["usernames"], json!([]));
    }

    #[test]
    fn identity_extends_the_api_server_flags_and_trusts_no_issuer_yet() {
        let files = host_files(&options(true));
        let drop_in = files
            .iter()
            .find(|f| f.path.ends_with("50-zeabur-identity.yaml"))
            .unwrap();
        let value = yaml(&drop_in.content);
        assert_eq!(
            value["kube-apiserver-arg+"],
            json!(["authentication-config=/etc/rancher/k3s/zeabur-identity/authn.yaml"])
        );
        assert!(
            files
                .iter()
                .any(|f| f.path == K3S.layout().authn_file && f.content == EMPTY_AUTHN)
        );
        assert!(
            !host_files(&options(false))
                .iter()
                .any(|f| f.path.contains("zeabur-identity"))
        );
    }

    #[test]
    fn system_namespaces_let_in_only_their_own_pods_and_dns() {
        let files = host_files(&options(false));
        let manifest = files
            .iter()
            .find(|f| f.path.starts_with(MANIFESTS_DIR))
            .unwrap();
        let objects: Vec<Value> = manifest
            .content
            .split("---\n")
            .skip(1)
            .map(|d| serde_json::from_str(d).unwrap())
            .collect();
        let policy = |namespace: &str, name: &str| {
            objects
                .iter()
                .find(|o| {
                    o["kind"] == "NetworkPolicy"
                        && o["metadata"]["namespace"] == namespace
                        && o["metadata"]["name"] == name
                })
                .cloned()
        };
        for namespace in ["kube-system", "kube-public", "default"] {
            let p = policy(namespace, "default-network-policy").unwrap();
            assert_eq!(
                p["spec"]["ingress"],
                json!([{"from": [{"podSelector": {}}]}])
            );
        }
        let dns = policy("kube-system", "default-network-dns-policy").unwrap();
        assert_eq!(
            dns["spec"]["podSelector"]["matchLabels"]["k8s-app"],
            "kube-dns"
        );
        assert!(dns["spec"]["ingress"][0].get("from").is_none());
        assert!(
            objects
                .iter()
                .filter(|o| o["kind"] == "ServiceAccount")
                .all(|o| o["automountServiceAccountToken"] == false)
        );
    }

    #[test]
    fn agents_get_the_kubelet_settings_and_no_server_keys() {
        let files = agent_files();
        assert_eq!(files[0], K3S.sysctl());
        let config = yaml(&files[1].content);
        assert_eq!(config["protect-kernel-defaults"], true);
        assert!(
            config["kubelet-arg"]
                .as_array()
                .unwrap()
                .contains(&json!("pod-max-pids=4096"))
        );
        assert!(config.get("secrets-encryption").is_none());
        assert!(config.get("kube-apiserver-arg").is_none());
    }

    #[test]
    fn calico_gets_the_pod_network_and_pinned_images_and_leaves_forwarding_off() {
        let images: Vec<String> = CALICO
            .images
            .iter()
            .map(|(tag, _)| format!("          image: {tag}\n"))
            .collect();
        let manifest = format!(
            "{}            # - name: CALICO_IPV4POOL_CIDR\n            #   value: \"192.168.0.0/16\"\n",
            images.concat()
        );
        let out = calico_manifest(&manifest).unwrap();
        assert!(out.contains("value: \"10.42.0.0/16\""));
        assert!(!out.contains("allow_ip_forwarding"));
        for (tag, digest) in CALICO.images {
            assert!(out.contains(&format!("image: {tag}@{digest}\n")));
        }
        assert!(calico_manifest("kind: DaemonSet\n").is_err());
    }
}
