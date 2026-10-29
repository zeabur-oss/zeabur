//! `KubeSolo` for a personal install: its files, ports and kubeconfig.
//!
//! `KubeSolo` is one process with `SQLite`, a bridge CNI and no scheduler. It enforces no
//! `NetworkPolicy`, so a personal install isolates nothing. It reads `/etc/kubesolo/config.yaml`
//! (`kubesolo --print-config` shows every key) and keeps its state in `/var/lib/kubesolo`.

use serde_json::{Value, json};

use crate::PlanError;
use crate::cluster::HostFile;
use crate::pins::HelmController;

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
/// The kubelet's certificate, whose subject names the node.
pub const KUBELET_CERT: &str = "/var/lib/kubesolo/pki/kubelet/kubelet.crt";
/// The cluster-admin kubeconfig `KubeSolo` writes.
pub const ADMIN_KUBECONFIG: &str = "/var/lib/kubesolo/pki/admin/admin.kubeconfig";
/// The script the unit runs after `KubeSolo` starts; see [`host_files`].
pub const WEBHOOK_SCRIPT: &str = "/usr/local/libexec/zeabur-kubesolo-webhook";
/// The installer's downloads and the kubectl it uses.
pub const STATE_DIR: &str = "/var/lib/zeabur-installer";
/// The pinned kubectl.
pub const KUBECTL: &str = "/var/lib/zeabur-installer/bin/kubectl";
/// The kubeconfig the installer writes for the person's own computer.
pub const PERSONAL_KUBECONFIG: &str = "/etc/zeabur-installer/kubeconfig.yaml";
/// The directory containerd reads registry settings from, one directory per registry host.
pub const REGISTRY_DIR: &str = "/var/lib/kubesolo/containerd/registry";
/// zot's CA on the host: containerd reads it on each pull.
pub const ZOT_CA_FILE: &str = "/var/lib/kubesolo/containerd/registry/zot.zot.svc:5000/ca.crt";
/// The `hosts.toml` that sends `zot.zot.svc:5000` to zot's address.
pub const ZOT_HOSTS_FILE: &str =
    "/var/lib/kubesolo/containerd/registry/zot.zot.svc:5000/hosts.toml";
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

/// The containerd of `KubeSolo` cannot resolve `zot.zot.svc`, so its images come from zot's fixed
/// `ClusterIP`, which zot's certificate names.
#[must_use]
pub fn zot_hosts_file(cluster_ip: &str) -> HostFile {
    HostFile {
        path: ZOT_HOSTS_FILE.to_owned(),
        mode: 0o644,
        content: format!(
            "# Written by zeabur-installer; `zeabur-installer doctor` compares it.\n\
             server = \"https://{cluster_ip}:5000\"\n\n\
             [host.\"https://{cluster_ip}:5000\"]\n  capabilities = [\"pull\", \"resolve\"]\n  \
             ca = \"{ZOT_CA_FILE}\"\n"
        ),
    }
}

/// What the configuration file sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The node's name, so a later change of the host name does not register a second node.
    pub node_name: String,
    /// The address the node registers with and the kubeconfig names; `None` lets `KubeSolo`
    /// choose, preferring a private address.
    pub node_ip: Option<String>,
    /// Extra API server certificate names.
    pub tls_san: Vec<String>,
}

const HEADER: &str = "# Written by zeabur-installer; `zeabur-installer doctor` compares it.\n";

/// The configuration file, the webhook script and the unit, in the order to write them.
///
/// `KubeSolo` v1.2.2's webhook sets a new Job's whole `nodeSelector` to the node, so
/// `kubernetes.io/os` goes away. helm-controller then finds its Job changed and replaces it again
/// and again, and no chart installs. The script takes Jobs out of the webhook's rules after each
/// start. The same webhook still gives each Pod the node.
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
    lines.extend([
        "kubernetes:".to_owned(),
        format!("  nodeName: {}", json!(options.node_name)),
    ]);
    if !options.tls_san.is_empty() {
        lines.extend([
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
            path: WEBHOOK_SCRIPT.to_owned(),
            mode: 0o755,
            content: WEBHOOK_SCRIPT_TEXT.to_owned(),
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
ExecStartPost=-/usr/local/libexec/zeabur-kubesolo-webhook
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

/// Removes `jobs` from the rules of `KubeSolo`'s webhook, and keeps watching for a minute: the
/// object from the last start exists before `KubeSolo` writes it again, so the script patches
/// whenever the rules name `jobs` and ends when they have not for a minute. It does not read the
/// journal. The `test` operation leaves another `KubeSolo` version's rules alone.
const WEBHOOK_SCRIPT_TEXT: &str = r#"#!/bin/sh
# Written by zeabur-installer. KubeSolo's webhook replaces a new Job's nodeSelector,
# and helm-controller then replaces its Job again and again. Pods still get the node.
kubectl="/var/lib/zeabur-installer/bin/kubectl --kubeconfig=/var/lib/kubesolo/pki/admin/admin.kubeconfig --context=kubernetes-admin@kubesolo"
patch='[{"op":"test","path":"/webhooks/0/rules/0/resources","value":["pods","persistentvolumeclaims","jobs"]},{"op":"replace","path":"/webhooks/0/rules/0/resources","value":["pods","persistentvolumeclaims"]}]'
i=0
quiet=0
while [ "$i" -lt 900 ]; do
  rules=$($kubectl get mutatingwebhookconfiguration webhook.kubesolo.io -o jsonpath='{.webhooks[0].rules[0].resources}' 2>/dev/null)
  case "$rules" in
    *jobs*) $kubectl patch mutatingwebhookconfiguration webhook.kubesolo.io --type=json -p "$patch" && quiet=0 ;;
    ?*) quiet=$((quiet + 1)); [ "$quiet" -ge 30 ] && exit 0 ;;
  esac
  i=$((i + 1))
  sleep 2
done
echo "KubeSolo did not write its webhook configuration in 30 minutes" >&2
exit 1
"#;

/// The helm-controller release manifest as the installer applies it: the controller's image
/// pinned by digest and the install Jobs' image set to [`HelmController::job_image`].
///
/// # Errors
///
/// When the manifest does not name an image the pins expect, which means they differ.
pub fn helm_controller_manifest(
    manifest: &str,
    pins: &HelmController,
) -> Result<String, PlanError> {
    let mut images: Vec<(&str, String)> = pins
        .images
        .iter()
        .map(|(reference, digest)| (*reference, format!("{reference}@{digest}")))
        .collect();
    images.push((pins.job_image.replaces, pins.job_image.reference.to_owned()));
    set_images(manifest, &images)
}

fn set_images(manifest: &str, images: &[(&str, String)]) -> Result<String, PlanError> {
    let mut out = manifest.to_owned();
    for (from, to) in images {
        let lines: Vec<String> = out
            .lines()
            .map(|line| match line.split_once(": ") {
                Some((key, value)) if value.trim() == *from => format!("{key}: {to}"),
                _ => line.to_owned(),
            })
            .collect();
        let set = lines.join("\n") + if out.ends_with('\n') { "\n" } else { "" };
        if set == out {
            return Err(PlanError::Values {
                what: "helm-controller manifest".to_owned(),
                reason: format!("it does not name the image {from}"),
            });
        }
        out = set;
    }
    Ok(out)
}

/// The install Job Pods of `pods` (a `kubectl get pods -o json` list) whose `helm` container ran
/// an image other than `digest`, as `namespace/name: image id`. A Pod that has not pulled its
/// image yet is not listed.
#[must_use]
pub fn job_image_mismatches(pods: &Value, digest: &str) -> Vec<String> {
    let Some(items) = pods.get("items").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .flat_map(|pod| {
            let name = format!(
                "{}/{}",
                pod.pointer("/metadata/namespace")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                pod.pointer("/metadata/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            );
            pod.pointer("/status/containerStatuses")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|c| c.get("name").and_then(Value::as_str) == Some("helm"))
                .filter_map(|c| c.get("imageID").and_then(Value::as_str))
                .filter(|id| !id.is_empty() && !id.ends_with(&format!("@{digest}")))
                .map(move |id| format!("{name}: {id}"))
                .collect::<Vec<_>>()
        })
        .collect()
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
    use crate::pins::JobImage;

    #[test]
    fn the_config_names_the_address_and_certificate_names() {
        let files = host_files(&Options {
            node_name: "solo".to_owned(),
            node_ip: Some("10.0.0.5".to_owned()),
            tls_san: vec!["solo.example.com".to_owned()],
        });
        let config = files.iter().find(|f| f.path == CONFIG_FILE).unwrap();
        assert_eq!(config.mode, 0o600);
        let value: Value = serde_saphyr::from_str(&config.content).unwrap();
        assert_eq!(value["kind"], "Config");
        assert_eq!(value["network"]["nodeIP"], "10.0.0.5");
        assert_eq!(value["kubernetes"]["nodeName"], "solo");
        assert_eq!(
            value["kubernetes"]["apiServer"]["extraSANs"],
            json!(["solo.example.com"])
        );
        let bare = host_files(&Options {
            node_name: "solo".to_owned(),
            node_ip: None,
            tls_san: Vec::new(),
        });
        let value: Value = serde_saphyr::from_str(&bare[0].content).unwrap();
        assert!(value.get("network").is_none());
        assert_eq!(value["kubernetes"], json!({"nodeName": "solo"}));
        let unit = files.iter().find(|f| f.path == UNIT_FILE).unwrap();
        assert!(
            unit.content
                .contains(&format!("ExecStart={BINARY} --config={CONFIG_FILE}"))
        );
    }

    #[test]
    fn the_controller_is_pinned_and_the_jobs_get_their_image() {
        let manifest = "        image: rancher/helm-controller:v0.17.9\n        - name: DEFAULT_JOB_IMAGE\n          value: ghcr.io/k3s-io/klipper-helm:v0.9.10\n";
        let pins = HelmController {
            version: "v0.17.9",
            manifest_sha256: "",
            images: &[("rancher/helm-controller:v0.17.9", "sha256:aa")],
            job_image: JobImage {
                replaces: "ghcr.io/k3s-io/klipper-helm:v0.9.10",
                reference: "rancher/klipper-helm:v0.13.3",
                digest: "sha256:bb",
            },
        };
        let set = helm_controller_manifest(manifest, &pins).unwrap();
        assert!(set.contains("image: rancher/helm-controller:v0.17.9@sha256:aa\n"));
        assert!(set.contains("value: rancher/klipper-helm:v0.13.3\n"));
        assert!(!set.contains("ghcr.io"));
        let other = HelmController {
            images: &[("rancher/other:v1", "sha256:cc")],
            ..pins
        };
        assert!(helm_controller_manifest(manifest, &other).is_err());
    }

    #[test]
    fn a_job_that_ran_another_image_is_named() {
        let pod = |name: &str, id: &str| {
            json!({
                "metadata": {"namespace": "kube-system", "name": name},
                "status": {"containerStatuses": [{"name": "helm", "imageID": id}]},
            })
        };
        let pods = json!({"items": [
            pod("helm-install-a", "ghcr.io/k3s-io/klipper-helm@sha256:aa"),
            pod("helm-install-b", "ghcr.io/k3s-io/klipper-helm@sha256:bb"),
            pod("helm-install-c", ""),
        ]});
        assert_eq!(
            job_image_mismatches(&pods, "sha256:aa"),
            vec!["kube-system/helm-install-b: ghcr.io/k3s-io/klipper-helm@sha256:bb"]
        );
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
