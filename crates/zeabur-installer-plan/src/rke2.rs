//! Host files written before RKE2 first starts.
//!
//! RKE2's `cis` profile already does most of the CIS benchmark: Secrets are encrypted at rest,
//! Pod Security `restricted` is the default outside `kube-system`, `NetworkPolicies` guard the
//! system namespaces and the service-account and kubelet flags are set
//! (<https://docs.rke2.io/security/hardening_guide>). The default applies only to a namespace
//! without its own Pod Security labels: Zeabur labels a project `baseline`, and Zeabur's own
//! namespaces keep `restricted` or the exemption the plan gives them. What it leaves to the host is
//! written here: the kernel parameters it refuses to start without, an audit policy (RKE2's own
//! logs nothing), and the file kube-apiserver reads Zeabur Identity's issuer from, which must exist
//! before the first start because RKE2 mounts only files that exist.
//!
//! Each key here is set in exactly one file, so a person's own `config.yaml.d/90-*.yaml`
//! overrides it cleanly.

use crate::cluster::{
    AUDIT_POLICY, Distribution, EMPTY_AUTHN, HEADER, HostFile, KUBELET_ARGS, Options, yaml_list,
};

/// Where the tarball installs.
pub const PREFIX: &str = "/usr/local";

const RKE2: Distribution = Distribution::Rke2;

/// The files to write, in the order to write them.
#[must_use]
pub fn host_files(options: &Options) -> Vec<HostFile> {
    let layout = RKE2.layout();
    let dir = layout.config_dir;
    let mut files = vec![
        RKE2.sysctl(),
        HostFile {
            path: format!("{dir}/audit-policy.yaml"),
            mode: 0o600,
            content: AUDIT_POLICY.to_owned(),
        },
        HostFile {
            path: format!("{dir}/config.yaml"),
            mode: 0o600,
            content: config(options),
        },
    ];
    if options.identity {
        files.push(HostFile {
            path: format!("{dir}/config.yaml.d/50-zeabur-identity.yaml"),
            mode: 0o600,
            content: format!(
                "{HEADER}kube-apiserver-arg:\n  - authentication-config={file}\n\
                 kube-apiserver-extra-mount:\n  - {authn}:{authn}:ro\n",
                file = layout.authn_file,
                authn = layout.authn_dir,
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

/// The files of a worker: the kernel parameters and an agent `config.yaml` with the server's
/// kubelet settings. The agent takes no server keys, so this is not the server's file.
#[must_use]
pub fn agent_files() -> Vec<HostFile> {
    vec![
        RKE2.sysctl(),
        HostFile {
            path: format!("{}/config.yaml", RKE2.layout().config_dir),
            mode: 0o600,
            content: format!("{HEADER}profile: cis\n{KUBELET_ARGS}"),
        },
    ]
}

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
    yaml_list(&mut out, "disable", &options.disable);
    yaml_list(&mut out, "tls-san", &options.tls_san);
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
    fn kubelet_serves_only_the_allowed_ciphers_and_closes_idle_streams() {
        let options = Options {
            tls_san: vec![],
            disable: vec![],
            identity: false,
        };
        let server = host_files(&options);
        for content in [&server[2].content, &agent_files()[1].content] {
            let parsed: serde_json::Value = serde_saphyr::from_str(content).unwrap();
            let args: Vec<&str> = parsed["kubelet-arg"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|a| a.as_str())
                .collect();
            assert!(
                args.contains(&"streaming-connection-idle-timeout=5m"),
                "{args:?}"
            );
            let ciphers = args
                .iter()
                .find_map(|a| a.strip_prefix("tls-cipher-suites="))
                .unwrap();
            assert!(
                ciphers.split(',').all(|c| c.contains("_ECDHE_")),
                "{ciphers}"
            );
            assert!(ciphers.contains("TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256"));
        }
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
    fn agents_keep_the_kubelet_settings_without_server_keys() {
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
    }
}
