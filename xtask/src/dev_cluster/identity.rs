//! Zeabur Identity, as zeabur-installer installs it, and kube-apiserver trusting it.

use std::time::Duration;

use super::{DevCluster, IDENTITY_HOST, cluster};
use crate::process::{Cmd, wait_for};

const RELOADS: &str = "apiserver_authentication_config_controller_automatic_reloads_total";

/// `zeabur-identityctl` of this cluster.
pub(crate) fn identityctl(dev: &DevCluster) -> Cmd {
    dev.cmd(dev.identityctl())
        .env("ZEABUR_IDENTITY_ISSUER", format!("https://{IDENTITY_HOST}"))
}

pub(crate) fn install(dev: &DevCluster, image: &str) -> Result<(), String> {
    let context = |e: String| format!("installing Zeabur Identity: {e}");
    let crds = identityctl(dev).arg("crds").read().map_err(context)?;
    dev.kubectl()
        .args(["apply", "--server-side", "-f", "-"])
        .input(crds)
        .quiet()
        .map_err(context)?;
    let manifests = identityctl(dev)
        .args(["manifests", "--image", image, "--serve-arg=--invitations"])
        .read()
        .map_err(context)?;
    dev.kubectl()
        .args(["apply", "--server-side", "--force-conflicts", "-f", "-"])
        .input(manifests)
        .quiet()
        .map_err(context)?;
    dev.rollout(
        "zeabur-identity",
        "deployment/zeabur-identity",
        Duration::from_secs(300),
    )?;
    dev.platform("zeabur-identity")?;
    let tls = dev.tls();
    dev.apply_created(
        "zeabur-identity",
        &[
            "secret",
            "tls",
            "zeabur-identity-tls",
            &format!("--cert={}", tls.join("tls.crt").display()),
            &format!("--key={}", tls.join("tls.key").display()),
        ],
    )?;
    trust(dev)
}

/// Writes Zeabur Identity's `AuthenticationConfiguration` where kube-apiserver re-reads it (about
/// once a minute), and waits until it has.
fn trust(dev: &DevCluster) -> Result<(), String> {
    let wanted = identityctl(dev)
        .args(["kube-config", "--distro", "k3s"])
        .probe()?;
    let wanted = wanted.trim_end();
    let path = format!("{}/authn.yaml", cluster::AUTHN_DIR);
    let current = dev
        .cmd("docker")
        .args(["exec", cluster::NODE, "cat", &path])
        .probe()
        .unwrap_or_default();
    if current.trim_end() == wanted {
        return Ok(());
    }
    let before = reloads(dev)?;
    dev.cmd("docker")
        .args(["exec", "-i", cluster::NODE, "sh", "-c"])
        .arg(format!(
            "cat >{dir}/.authn.yaml.tmp && mv {dir}/.authn.yaml.tmp {path}",
            dir = cluster::AUTHN_DIR
        ))
        .input(format!("{wanted}\n"))
        .quiet()?;
    wait_for(
        "kube-apiserver to load Zeabur Identity's configuration",
        Duration::from_secs(300),
        || {
            let now = reloads(dev)?;
            if now > before {
                Ok(())
            } else {
                Err(format!(
                    "{now} successful reloads, {before} before the write"
                ))
            }
        },
    )
}

fn reloads(dev: &DevCluster) -> Result<f64, String> {
    let metrics = dev.kubectl().args(["get", "--raw", "/metrics"]).probe()?;
    Ok(successful_reloads(&metrics))
}

/// The successful automatic reloads of the `AuthenticationConfiguration` in kube-apiserver's
/// `/metrics`.
fn successful_reloads(metrics: &str) -> f64 {
    metrics
        .lines()
        .filter(|line| {
            line.strip_prefix(RELOADS)
                .is_some_and(|rest| rest.starts_with(['{', ' ']))
                && line.contains("status=\"success\"")
        })
        .filter_map(|line| line.split_whitespace().last()?.parse::<f64>().ok())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::successful_reloads;

    #[test]
    fn only_successful_reloads_count() {
        let metrics = r#"# HELP apiserver_authentication_config_controller_automatic_reloads_total [ALPHA] Total number of automatic reloads.
# TYPE apiserver_authentication_config_controller_automatic_reloads_total counter
apiserver_authentication_config_controller_automatic_reloads_total{apiserver_id_hash="sha256:1",status="failure"} 4
apiserver_authentication_config_controller_automatic_reloads_total{apiserver_id_hash="sha256:1",status="success"} 2
apiserver_authentication_config_controller_automatic_reloads_total{apiserver_id_hash="sha256:2",status="success"} 1
apiserver_authentication_config_controller_automatic_reloads_total_other{status="success"} 9
apiserver_authentication_config_controller_last_config_info{apiserver_id_hash="sha256:1",status="success"} 1
"#;
        assert!((successful_reloads(metrics) - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn no_reloads_yet_is_zero() {
        assert!(successful_reloads("go_goroutines 12\n").abs() < f64::EPSILON);
    }
}
