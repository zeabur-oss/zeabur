//! `KubeSolo` for a personal install: its files, ports and kubeconfig.
//!
//! `KubeSolo` is one process with `SQLite`, a bridge CNI and no scheduler. It enforces no
//! `NetworkPolicy`, so a personal install isolates nothing. It reads `/etc/kubesolo/config.yaml`
//! (`kubesolo --print-config` shows every key) and keeps its state in `/var/lib/kubesolo`.

use serde_json::{Value, json};

use crate::PlanError;
use crate::rke2::HostFile;

/// The systemd unit.
pub const UNIT: &str = "kubesolo";
/// The binary from the release archive.
pub const BINARY: &str = "/usr/local/bin/kubesolo";
/// Its state: the database, certificates and images.
pub const DATA_DIR: &str = "/var/lib/kubesolo";
/// Its configuration file.
pub const CONFIG_FILE: &str = "/etc/kubesolo/config.yaml";
/// The unit file.
pub const UNIT_FILE: &str = "/etc/systemd/system/kubesolo.service";
/// The cluster-admin kubeconfig `KubeSolo` writes.
pub const ADMIN_KUBECONFIG: &str = "/var/lib/kubesolo/pki/admin/admin.kubeconfig";
/// The installer's downloads and the kubectl it uses.
pub const STATE_DIR: &str = "/var/lib/zeabur-installer";
/// The pinned kubectl.
pub const KUBECTL: &str = "/var/lib/zeabur-installer/bin/kubectl";
/// The kubeconfig the installer writes for the person's own computer.
pub const PERSONAL_KUBECONFIG: &str = "/etc/zeabur-installer/kubeconfig.yaml";
/// `KubeSolo`'s storage class, the default.
pub const LOCAL_PATH_CLASS: &str = "local-path";

/// The TCP ports `KubeSolo` listens on, and what for.
#[must_use]
pub fn ports() -> Vec<(u16, &'static str)> {
    vec![
        (
            80,
            "HTTP ingress: KubeSolo gives Traefik's LoadBalancer Service this host's address",
        ),
        (
            443,
            "HTTPS ingress: KubeSolo gives Traefik's LoadBalancer Service this host's address",
        ),
        (6443, "Kubernetes API"),
        (10250, "kubelet"),
        (10443, "KubeSolo's node-assignment webhook"),
    ]
}

/// What the configuration file sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The address the node registers with and the kubeconfig names; `None` lets `KubeSolo`
    /// choose, preferring a private address.
    pub node_ip: Option<String>,
    /// Extra API server certificate names.
    pub tls_san: Vec<String>,
}

const HEADER: &str = "# Written by zeabur-installer; `zeabur-installer doctor` compares it.\n";

/// The configuration file and the unit, in the order to write them.
#[must_use]
pub fn host_files(options: &Options) -> Vec<HostFile> {
    let mut lines = vec![
        HEADER.trim_end().to_owned(),
        "apiVersion: kubesolo.io/v1alpha1".to_owned(),
        "kind: Config".to_owned(),
    ];
    if let Some(ip) = &options.node_ip {
        lines.extend(["network:".to_owned(), format!("  nodeIP: {}", json!(ip))]);
    }
    if !options.tls_san.is_empty() {
        lines.extend([
            "kubernetes:".to_owned(),
            "  apiServer:".to_owned(),
            format!("    extraSANs: {}", json!(options.tls_san)),
        ]);
    }
    let config = lines.join("\n") + "\n";
    vec![
        HostFile {
            path: CONFIG_FILE.to_owned(),
            mode: 0o600,
            content: config,
        },
        HostFile {
            path: UNIT_FILE.to_owned(),
            mode: 0o644,
            content: UNIT_TEXT.to_owned(),
        },
    ]
}

/// The unit of `KubeSolo`'s own installer, without its environment file.
const UNIT_TEXT: &str = "# Written by zeabur-installer; `zeabur-installer doctor` compares it.
[Unit]
Description=KubeSolo
Documentation=https://github.com/portainer/kubesolo
Wants=network-online.target
After=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/kubesolo --config=/etc/kubesolo/config.yaml
KillMode=process
Delegate=yes
LimitNOFILE=65535
LimitNPROC=infinity
LimitCORE=infinity
TasksMax=infinity
TimeoutStartSec=0
Restart=always
RestartSec=3
OOMScoreAdjust=-500

[Install]
WantedBy=multi-user.target
";

/// Pins each image of `manifest` to its digest: `ref` becomes `ref@digest`.
///
/// # Errors
///
/// When an image is not in the manifest, which means the pins and the manifest differ.
pub fn pin_images(manifest: &str, images: &[(&str, &str)]) -> Result<String, PlanError> {
    let mut out = manifest.to_owned();
    for (reference, digest) in images {
        let lines: Vec<String> = out
            .lines()
            .map(|line| {
                let value = line.split_once(": ").map(|(_, v)| v.trim());
                if value == Some(reference) {
                    format!("{line}@{digest}")
                } else {
                    line.to_owned()
                }
            })
            .collect();
        let pinned = lines.join("\n") + if out.ends_with('\n') { "\n" } else { "" };
        if pinned == out {
            return Err(PlanError::Values {
                what: "helm-controller manifest".to_owned(),
                reason: format!("it does not name the image {reference}"),
            });
        }
        out = pinned;
    }
    Ok(out)
}

/// The admin kubeconfig as a file for the person's computer: only the certificate user, with the
/// cluster, user and context all named `name`. `server` replaces the API address when given.
///
/// # Errors
///
/// When the file has no context with a client certificate.
pub fn personal_kubeconfig(
    admin: &Value,
    name: &str,
    server: Option<&str>,
) -> Result<Value, PlanError> {
    let bad = |reason: &str| PlanError::Values {
        what: ADMIN_KUBECONFIG.to_owned(),
        reason: reason.to_owned(),
    };
    let named = |list: &str, key: &str| -> Option<Value> {
        admin
            .get(list)?
            .as_array()?
            .iter()
            .find(|e| e.get("name").and_then(Value::as_str) == Some(key))
            .cloned()
    };
    let contexts = admin
        .get("contexts")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("no contexts"))?;
    let (cluster, user) = contexts
        .iter()
        .find_map(|c| {
            let ctx = c.get("context")?;
            let cluster = named("clusters", ctx.get("cluster")?.as_str()?)?;
            let user = named("users", ctx.get("user")?.as_str()?)?;
            user.pointer("/user/client-certificate-data")?;
            Some((cluster, user))
        })
        .ok_or_else(|| bad("no context with a client certificate"))?;
    let mut cluster = cluster.get("cluster").cloned().unwrap_or_default();
    if let (Some(server), Some(map)) = (server, cluster.as_object_mut()) {
        map.insert("server".to_owned(), json!(server));
    }
    Ok(json!({
        "apiVersion": "v1",
        "kind": "Config",
        "clusters": [{"name": name, "cluster": cluster}],
        "users": [{"name": name, "user": user.get("user").cloned().unwrap_or_default()}],
        "contexts": [{"name": name, "context": {"cluster": name, "user": name}}],
        "current-context": name,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_names_the_address_and_certificate_names() {
        let files = host_files(&Options {
            node_ip: Some("10.0.0.5".to_owned()),
            tls_san: vec!["solo.example.com".to_owned()],
        });
        let config = files.iter().find(|f| f.path == CONFIG_FILE).unwrap();
        assert_eq!(config.mode, 0o600);
        let value: Value = serde_saphyr::from_str(&config.content).unwrap();
        assert_eq!(value["kind"], "Config");
        assert_eq!(value["network"]["nodeIP"], "10.0.0.5");
        assert_eq!(
            value["kubernetes"]["apiServer"]["extraSANs"],
            json!(["solo.example.com"])
        );
        let bare = host_files(&Options {
            node_ip: None,
            tls_san: Vec::new(),
        });
        let value: Value = serde_saphyr::from_str(&bare[0].content).unwrap();
        assert!(value.get("network").is_none());
        let unit = files.iter().find(|f| f.path == UNIT_FILE).unwrap();
        assert!(
            unit.content
                .contains(&format!("ExecStart={BINARY} --config={CONFIG_FILE}"))
        );
    }

    #[test]
    fn images_are_pinned_by_digest() {
        let manifest = "        image: rancher/helm-controller:v0.17.9\n        - name: DEFAULT_JOB_IMAGE\n          value: ghcr.io/k3s-io/klipper-helm:v0.9.10\n";
        let pinned = pin_images(
            manifest,
            &[
                ("rancher/helm-controller:v0.17.9", "sha256:aa"),
                ("ghcr.io/k3s-io/klipper-helm:v0.9.10", "sha256:bb"),
            ],
        )
        .unwrap();
        assert!(pinned.contains("image: rancher/helm-controller:v0.17.9@sha256:aa\n"));
        assert!(pinned.contains("value: ghcr.io/k3s-io/klipper-helm:v0.9.10@sha256:bb\n"));
        assert!(pin_images(manifest, &[("rancher/other:v1", "sha256:cc")]).is_err());
    }

    #[test]
    fn the_personal_kubeconfig_keeps_only_the_certificate_user() {
        let admin = json!({
            "clusters": [{"name": "kubesolo", "cluster": {"server": "https://10.0.0.5:6443", "certificate-authority-data": "Q0E="}}],
            "users": [
                {"name": "kubernetes-admin", "user": {"client-certificate-data": "Q0VSVA==", "client-key-data": "S0VZ"}},
                {"name": "admin-token", "user": {"tokenFile": "/nowhere"}},
            ],
            "contexts": [
                {"name": "admin-token@kubesolo", "context": {"cluster": "kubesolo", "user": "admin-token"}},
                {"name": "kubernetes-admin@kubesolo", "context": {"cluster": "kubesolo", "user": "kubernetes-admin"}},
            ],
            "current-context": "admin-token@kubesolo",
        });
        let config =
            personal_kubeconfig(&admin, "solo", Some("https://solo.example.com:6443")).unwrap();
        assert_eq!(config["current-context"], "solo");
        assert!(!config.to_string().contains("admin-token"));
        assert_eq!(config["users"][0]["user"]["client-key-data"], "S0VZ");
        assert_eq!(
            config["clusters"][0]["cluster"]["server"],
            "https://solo.example.com:6443"
        );
        assert_eq!(
            config["clusters"][0]["cluster"]["certificate-authority-data"],
            "Q0E="
        );
        assert!(personal_kubeconfig(&json!({"contexts": []}), "solo", None).is_err());
    }
}
