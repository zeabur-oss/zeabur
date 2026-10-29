//! The k3d cluster, its Traefik and its DNS.

use std::env;
use std::time::Duration;

use super::{DevCluster, IDENTITY_HOST, ZEABUR_HOST, images, registries};
use crate::process::wait_for;

pub(crate) const NAME: &str = "zeabur-dev";
pub(crate) const NODE: &str = "k3d-zeabur-dev-server-0";
pub(crate) const AUTHN_VOLUME: &str = "zeabur-dev-authn";
/// Where the node reads Zeabur Identity's `AuthenticationConfiguration`.
pub(crate) const AUTHN_DIR: &str = "/etc/rancher/k3s/zeabur-identity";
const K3S_IMAGE: &str = "rancher/k3s:v1.36.5-k3s1";
/// kube-apiserver starts with no issuer.
const NO_ISSUER: &str = "apiVersion: apiserver.config.k8s.io/v1
kind: AuthenticationConfiguration
anonymous:
  enabled: false
jwt: []
";

/// Creates the cluster unless it runs, and writes its kubeconfig.
pub(crate) fn create(dev: &DevCluster) -> Result<(), String> {
    if !dev.cmd("k3d").args(["cluster", "get", NAME]).attempt() {
        let mirrors =
            registries::parse_mirrors(&env::var("ZEABUR_REGISTRY_MIRRORS").unwrap_or_default())?;
        let registries = dev.write("registries.yaml", &registries::render(&mirrors))?;
        // kube-apiserver re-reads the file by path. It lives in a Docker volume, so this also
        // works where xtask runs in a container.
        dev.cmd("docker")
            .args(["volume", "rm", "-f", AUTHN_VOLUME])
            .quiet()?;
        dev.cmd("docker")
            .args(["volume", "create", AUTHN_VOLUME])
            .quiet()?;
        dev.cmd("docker")
            .args(["run", "--rm", "-i", "-v"])
            .arg(format!("{AUTHN_VOLUME}:/authn"))
            .args([images::BUSYBOX, "sh", "-c", "cat >/authn/authn.yaml"])
            .input(NO_ISSUER)
            .quiet()?;
        let k3s_image = env::var("K3S_IMAGE")
            .ok()
            .filter(|image| !image.is_empty())
            .unwrap_or_else(|| K3S_IMAGE.to_owned());
        // No k3d load balancer: port 443 maps straight onto the server node.
        dev.cmd("k3d")
            .args(["cluster", "create", NAME, "--image", &k3s_image, "--no-lb"])
            .args(["--wait", "--timeout", "300s"])
            .args([
                "--kubeconfig-update-default=false",
                "--kubeconfig-switch-context=false",
            ])
            .arg("--registry-config")
            .arg(registries)
            .arg("--volume")
            .arg(format!("{AUTHN_VOLUME}:{AUTHN_DIR}@server:0"))
            .arg("--k3s-arg")
            .arg(format!(
                "--kube-apiserver-arg=authentication-config={AUTHN_DIR}/authn.yaml@server:0"
            ))
            .args(["--port", "443:443@server:0:direct"])
            .run()
            .map_err(|e| format!("creating the cluster: {e}"))?;
    }
    let kubeconfig = dev.cmd("k3d").args(["kubeconfig", "get", NAME]).read()?;
    dev.write("kubeconfig", &kubeconfig).map(drop)
}

pub(crate) fn traefik(dev: &DevCluster) -> Result<(), String> {
    wait_for("Traefik to be installed", Duration::from_secs(300), || {
        dev.kubectl()
            .args(["-n", "kube-system", "get", "deployment/traefik"])
            .probe()
            .map(drop)
    })?;
    dev.rollout(
        "kube-system",
        "deployment/traefik",
        Duration::from_secs(300),
    )
}

/// Both host names resolve to Traefik in the cluster too, so Zeabur reaches Zeabur Identity at
/// its public URL (<https://docs.k3s.io/networking/networking-services#coredns>).
pub(crate) fn resolve_hosts(dev: &DevCluster) -> Result<(), String> {
    let rewrite = |host: &str| {
        format!("rewrite name exact {host} traefik.kube-system.svc.cluster.local answer auto\n")
    };
    let configmap = serde_json::json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": "coredns-custom", "namespace": "kube-system"},
        "data": {"zeabur-dev.override": rewrite(IDENTITY_HOST) + &rewrite(ZEABUR_HOST)},
    });
    dev.apply(configmap.to_string())?;
    dev.kubectl()
        .args([
            "-n",
            "kube-system",
            "rollout",
            "restart",
            "deployment/coredns",
        ])
        .quiet()?;
    dev.rollout(
        "kube-system",
        "deployment/coredns",
        Duration::from_secs(120),
    )?;
    wait_for(
        &format!("{IDENTITY_HOST} to resolve in the cluster"),
        Duration::from_secs(120),
        || resolves(dev),
    )?;
    dev.kubectl()
        .args(["delete", "pod", "dns-check", "--wait"])
        .quiet()
}

/// A pod per try, read through its log: `kubectl run --rm -i` may fail to attach to a pod that
/// has already ended and then leaves it behind.
fn resolves(dev: &DevCluster) -> Result<(), String> {
    dev.kubectl()
        .args(["delete", "pod", "dns-check", "--ignore-not-found", "--wait"])
        .probe()?;
    dev.kubectl()
        .args(["run", "dns-check", "--restart=Never"])
        .args(["--image", images::BUSYBOX, "--image-pull-policy=Never"])
        .args(["--", "nslookup", IDENTITY_HOST])
        .probe()?;
    dev.kubectl()
        .args(["wait", "pod/dns-check", "--timeout=60s"])
        .arg("--for=jsonpath={.status.phase}=Succeeded")
        .probe()?;
    let log = dev.kubectl().args(["logs", "dns-check"]).probe()?;
    if log
        .lines()
        .any(|line| line.starts_with("Name:") && line.contains(IDENTITY_HOST))
    {
        Ok(())
    } else {
        Err(format!("nslookup answered: {}", log.trim()))
    }
}
