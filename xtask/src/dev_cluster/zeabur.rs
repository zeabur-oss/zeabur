//! Zeabur, from zeabur-installer's HelmChart, with what only this cluster sets.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::identity::identityctl;
use super::images::split_tag;
use super::{DevCluster, IDENTITY_HOST, Profile, ZEABUR_HOST, cluster, snapshot};
use crate::output;
use crate::process::{random_hex, wait_for};

/// What Zeabur is installed from.
#[derive(Debug)]
pub(crate) struct Install<'a> {
    pub(crate) profile: Profile,
    pub(crate) identity_image: &'a str,
    pub(crate) zeabur_image: &'a str,
    /// Zeabur Proxy's image, when it was built here; otherwise Zeabur Proxy stays off.
    pub(crate) proxy_image: Option<&'a str>,
    pub(crate) chart: &'a Path,
}

pub(crate) fn install(dev: &DevCluster, install: &Install<'_>) -> Result<(), String> {
    // The domain policy, as zeabur-installer writes it with Zeabur. It stays when Zeabur is
    // removed.
    dev.kubectl()
        .args(["apply", "--server-side", "-f"])
        .arg(
            dev.root
                .join("crates/zeabur-installer/manifests/system-hosts.yaml"),
        )
        .quiet()?;
    // The projects the browser opens come first: their namespaces stay after Zeabur's removal.
    dev.kubectl()
        .arg("apply")
        .arg("-f")
        .arg(dev.root.join("web/e2e/manifests/talker.yaml"))
        .arg("-f")
        .arg(dev.root.join("web/e2e/manifests/second-project.yaml"))
        .quiet()?;
    let installed = dev
        .kubectl()
        .args([
            "get",
            "helmchart.helm.cattle.io",
            "-n",
            "kube-system",
            "zeabur",
        ])
        .attempt();
    if !installed {
        dev.write("before.txt", &snapshot::take(dev)?)?;
    }

    dev.apply_created("zeabur", &["namespace", "zeabur"])?;
    dev.platform("zeabur")?;
    secrets(dev, install.profile)?;

    let install_yaml = dev.write(
        "install.yaml",
        &install_config(
            install.identity_image,
            &dev.identityctl(),
            install.zeabur_image,
            install.chart,
        ),
    )?;
    dev.cmd("cargo")
        .current_dir(&dev.root)
        .args(["build", "-q", "--locked", "-p", "zeabur-installer"])
        .run()?;
    let manifests = dev
        .cmd(dev.root.join("target/debug/zeabur-installer"))
        .arg("--config")
        .arg(&install_yaml)
        .args(["manifests", "--cluster-name", cluster::NAME])
        .read()?;
    dev.kubectl()
        .args(["apply", "--server-side", "--force-conflicts", "-f", "-"])
        .input(manifests)
        .quiet()?;
    let values = chart_values(install.profile == Profile::Full, install.proxy_image);
    dev.apply(helm_chart_config(&values).to_string())?;

    let image = install.zeabur_image;
    let waited = wait_for(
        &format!("Zeabur to run {image} over HTTPS"),
        Duration::from_secs(600),
        || running(dev, image),
    );
    if let Err(e) = waited {
        dev.kubectl()
            .args([
                "-n",
                "kube-system",
                "logs",
                "job/helm-install-zeabur",
                "--tail=50",
            ])
            .report();
        dev.kubectl()
            .args(["-n", "zeabur", "get", "pods,events"])
            .report();
        return Err(e);
    }
    dev.write("profile", &format!("{}\n", install.profile.name()))?;
    summary(dev)
}

/// Zeabur's OIDC client and its secret, the plan key, the certificates and the CAs it trusts.
fn secrets(dev: &DevCluster, profile: Profile) -> Result<(), String> {
    let has = |args: &[&str]| dev.kubectl().args(args).attempt();
    let client = has(&[
        "-n",
        "zeabur-identity",
        "get",
        "oauthclients.identity.zeabur.com",
        "zeabur",
    ]);
    if !client || !has(&["-n", "zeabur", "get", "secret", "zeabur-oidc"]) {
        let created = identityctl(dev)
            .args(["client", "create", "zeabur", "--type", "confidential"])
            .args(["--audience", "kubernetes", "--display-name", "Zeabur"])
            .arg(format!("--home-url=https://{ZEABUR_HOST}/"))
            .arg(format!(
                "--redirect-uri=https://{ZEABUR_HOST}/auth/callback"
            ))
            .output()?;
        let stderr = String::from_utf8_lossy(&created.stderr);
        if !created.status.success() {
            return Err(format!(
                "creating Zeabur's OIDC client ({}): {}",
                created.status,
                stderr.trim()
            ));
        }
        dev.apply(created.stdout)?;
        let client_secret = stderr
            .lines()
            .rfind(|line| !line.trim().is_empty())
            .ok_or("zeabur-identityctl client create printed no secret")?
            .trim();
        dev.apply(secret("zeabur-oidc", "client-secret", client_secret).to_string())?;
    }
    if !has(&["-n", "zeabur", "get", "secret", "zeabur-plan-key"]) {
        dev.apply(secret("zeabur-plan-key", "plan-key", &random_hex(32)?).to_string())?;
    }
    let tls = dev.tls();
    dev.apply_created(
        "zeabur",
        &[
            "secret",
            "tls",
            "zeabur-tls",
            &format!("--cert={}", tls.join("tls.crt").display()),
            &format!("--key={}", tls.join("tls.key").display()),
        ],
    )?;
    dev.apply_created(
        "zeabur",
        &[
            "configmap",
            "zeabur-identity-ca",
            &format!("--from-file=ca.crt={}", tls.join("ca.crt").display()),
        ],
    )?;
    // Zeabur reads zot over HTTPS against its CA, as zeabur-installer sets it up.
    if profile == Profile::Full {
        dev.apply_created(
            "zeabur",
            &[
                "configmap",
                "zeabur-registry-ca",
                &format!(
                    "--from-file=ca.crt={}",
                    dev.state.join("zot-ca.crt").display()
                ),
            ],
        )?;
    }
    Ok(())
}

fn secret(name: &str, key: &str, value: &str) -> Value {
    json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": {"name": name, "namespace": "zeabur"},
        "type": "Opaque",
        "stringData": {key: value},
    })
}

/// zeabur-installer's configuration for objects from images and a chart of this cluster.
fn install_config(identity_image: &str, identityctl: &Path, image: &str, chart: &Path) -> String {
    let quote = |s: &str| Value::from(s).to_string();
    format!(
        "access: {{mode: local, identity: {}, zeabur: {}}}\nsources:\n  identityImage: {}\n  identityctl: {}\n  zeaburImage: {}\n  zeaburChart: {}\n",
        quote(IDENTITY_HOST),
        quote(ZEABUR_HOST),
        quote(identity_image),
        quote(&identityctl.display().to_string()),
        quote(image),
        quote(&chart.display().to_string()),
    )
}

/// The values only this cluster sets: the registry's CA in the full profile, and Zeabur Proxy
/// when its image is built here.
fn chart_values(full: bool, proxy_image: Option<&str>) -> Value {
    let mut values = serde_json::Map::new();
    if full {
        values.insert(
            "builds".to_owned(),
            json!({"caBundle": {"configMap": "zeabur-registry-ca"}}),
        );
    }
    if let Some(image) = proxy_image {
        let (repository, tag) = split_tag(image);
        values.insert(
            "proxy".to_owned(),
            json!({
                "enabled": true,
                "image": {"repository": repository, "tag": tag.unwrap_or("latest")},
            }),
        );
    }
    Value::Object(values)
}

fn helm_chart_config(values: &Value) -> Value {
    json!({
        "apiVersion": "helm.cattle.io/v1",
        "kind": "HelmChartConfig",
        "metadata": {"name": "zeabur", "namespace": "kube-system"},
        "spec": {"valuesContent": values.to_string()},
    })
}

/// Whether Zeabur runs `image` and answers over HTTPS.
fn running(dev: &DevCluster, image: &str) -> Result<(), String> {
    let current = dev
        .kubectl()
        .args(["-n", "zeabur", "get", "deployment", "zeabur", "-o"])
        .arg("jsonpath={.spec.template.spec.containers[0].image}")
        .probe()?;
    if current != image {
        return Err(format!("the deployment runs {current:?}"));
    }
    dev.kubectl()
        .args([
            "-n",
            "zeabur",
            "rollout",
            "status",
            "deployment/zeabur",
            "--timeout=10s",
        ])
        .probe()?;
    dev.cmd("curl")
        .arg("-sSf")
        .arg("--cacert")
        .arg(dev.tls().join("ca.crt"))
        .args(["--resolve", &format!("{ZEABUR_HOST}:443:127.0.0.1")])
        .arg(format!("https://{ZEABUR_HOST}/api/healthz"))
        .probe()
        .map(drop)
}

fn summary(dev: &DevCluster) -> Result<(), String> {
    let identityctl = dev.identityctl();
    output::say("");
    output::say(&format!("Zeabur:  https://{ZEABUR_HOST}"));
    let enrolled = dev
        .kubectl()
        .args([
            "-n",
            "zeabur-identity",
            "get",
            "users.identity.zeabur.com",
            "admin",
        ])
        .attempt();
    if enrolled {
        output::say(&format!(
            "Admin:   enrolled; a new setup link: {} setup-link --username admin",
            identityctl.display()
        ));
    } else {
        identity_setup_link(dev)?;
    }
    output::say(&format!(
        "CA:      {} (trust it in the browser, or accept the certificate)",
        dev.tls().join("ca.crt").display()
    ));
    output::say(&format!(
        "kubectl: export KUBECONFIG={}",
        dev.kubeconfig().display()
    ));
    Ok(())
}

fn identity_setup_link(dev: &DevCluster) -> Result<(), String> {
    identityctl(dev)
        .args(["setup-link", "--username", "admin", "--valid-for", "24h"])
        .run()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::{Value, json};

    use super::{chart_values, helm_chart_config, install_config};

    #[test]
    fn the_default_profile_sets_no_values() {
        assert_eq!(chart_values(false, None), json!({}));
    }

    #[test]
    fn a_proxy_built_here_is_enabled_with_its_image() {
        let values = chart_values(
            true,
            Some("docker.io/library/zeabur-proxy:dev-0123456789ab"),
        );
        assert_eq!(
            values["proxy"],
            json!({
                "enabled": true,
                "image": {"repository": "docker.io/library/zeabur-proxy", "tag": "dev-0123456789ab"},
            })
        );
        assert_eq!(
            values["builds"]["caBundle"]["configMap"],
            "zeabur-registry-ca"
        );
    }

    #[test]
    fn the_helm_chart_config_carries_the_values_as_text() {
        let values = chart_values(false, Some("zeabur-proxy:dev-1"));
        let config = helm_chart_config(&values);
        assert_eq!(config["metadata"]["name"], "zeabur");
        assert_eq!(config["metadata"]["namespace"], "kube-system");
        let content: Value =
            serde_json::from_str(config["spec"]["valuesContent"].as_str().unwrap()).unwrap();
        assert_eq!(content, values);
    }

    #[test]
    fn install_yaml_names_the_local_sources() {
        let yaml = install_config(
            "docker.io/library/zeabur-identity:dev-0123456789ab",
            Path::new("/repo/target/dev-cluster/identityctl"),
            "ghcr.io/zeabur-oss/zeabur:0.1.1",
            Path::new("/charts/zeabur 0.1.1.tgz"),
        );
        let config: Value = serde_saphyr::from_str(&yaml).unwrap();
        assert_eq!(
            config,
            json!({
                "access": {"mode": "local", "identity": "identity.localhost", "zeabur": "zeabur.localhost"},
                "sources": {
                    "identityImage": "docker.io/library/zeabur-identity:dev-0123456789ab",
                    "identityctl": "/repo/target/dev-cluster/identityctl",
                    "zeaburImage": "ghcr.io/zeabur-oss/zeabur:0.1.1",
                    "zeaburChart": "/charts/zeabur 0.1.1.tgz",
                },
            })
        );
    }
}
