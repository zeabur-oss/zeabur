//! Zeabur from its chart, with the values of this cluster.

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
    pub(crate) zeabur_image: &'a str,
    /// Zeabur Proxy's image, when it was built here; otherwise Zeabur Proxy stays off.
    pub(crate) proxy_image: Option<&'a str>,
    pub(crate) chart: &'a Path,
}

pub(crate) fn install(dev: &DevCluster, install: &Install<'_>) -> Result<(), String> {
    // The projects the browser opens come first: their namespaces stay after Zeabur's removal.
    dev.kubectl()
        .arg("apply")
        .arg("-f")
        .arg(dev.root.join("web/e2e/manifests/talker.yaml"))
        .arg("-f")
        .arg(dev.root.join("web/e2e/manifests/second-project.yaml"))
        .quiet()?;
    let installed = dev
        .cmd("helm")
        .args(["-n", "zeabur", "status", "zeabur"])
        .attempt();
    if !installed {
        dev.write("before.txt", &snapshot::take(dev)?)?;
    }

    dev.apply_created("zeabur", &["namespace", "zeabur"])?;
    dev.platform("zeabur")?;
    secrets(dev, install.profile)?;
    dev.apply(identity_ingress().to_string())?;

    let values = dev.write(
        "values.json",
        &values(
            install.zeabur_image,
            install.profile == Profile::Full,
            install.proxy_image,
        )
        .to_string(),
    )?;
    dev.cmd("helm")
        .args(["upgrade", "--install", "zeabur"])
        .arg(install.chart)
        .args(["-n", "zeabur", "-f"])
        .arg(&values)
        .quiet()
        .map_err(|e| format!("installing Zeabur: {e}"))?;

    let image = install.zeabur_image;
    let waited = wait_for(
        &format!("Zeabur to run {image} over HTTPS"),
        Duration::from_secs(600),
        || running(dev, image),
    );
    if let Err(e) = waited {
        dev.diagnose("zeabur");
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
    // Zeabur reads zot over HTTPS against its CA.
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

/// The `ClusterIssuer` of this cluster's CA, which the full profile installs.
const ISSUER: &str = "zeabur-local-ca";

/// Zeabur Identity's Ingress at `IDENTITY_HOST`.
fn identity_ingress() -> Value {
    json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "Ingress",
        "metadata": {
            "name": "zeabur-identity",
            "namespace": "zeabur-identity",
            "annotations": {"cert-manager.io/cluster-issuer": ISSUER},
        },
        "spec": {
            "ingressClassName": "traefik",
            "tls": [{"hosts": [IDENTITY_HOST], "secretName": "zeabur-identity-tls"}],
            "rules": [{
                "host": IDENTITY_HOST,
                "http": {"paths": [{
                    "path": "/",
                    "pathType": "Prefix",
                    "backend": {"service": {"name": "zeabur-identity", "port": {"name": "http"}}},
                }]},
            }],
        },
    })
}

/// The chart's values for this cluster (docs/install/existing-cluster.mdx), with the registry's
/// CA in the full profile, and Zeabur Proxy when its image is built here.
fn values(image: &str, full: bool, proxy_image: Option<&str>) -> Value {
    let (repository, tag) = split_tag(image);
    let mut values = json!({
        // An empty digest clears the one a released chart pins, which the templates prefer.
        "image": {"repository": repository, "tag": tag.unwrap_or("latest"), "digest": "",
                  "pullPolicy": "IfNotPresent"},
        "clusterName": cluster::NAME,
        "publicUrl": format!("https://{ZEABUR_HOST}"),
        "issuer": {
            "url": format!("https://{IDENTITY_HOST}"),
            "clientSecret": {"existingSecret": "zeabur-oidc"},
            "caBundle": {"configMap": "zeabur-identity-ca"},
        },
        "planKey": {"existingSecret": "zeabur-plan-key"},
        "domains": {"suffixes": [], "clusterIssuer": ISSUER},
        "ingress": {
            "enabled": true,
            "className": "traefik",
            "annotations": {"cert-manager.io/cluster-issuer": ISSUER},
            "tls": {"secretName": "zeabur-tls"},
        },
    });
    let builds = full.then(|| json!({"caBundle": {"configMap": "zeabur-registry-ca"}}));
    let proxy = proxy_image.map(|image| {
        let (repository, tag) = split_tag(image);
        json!({
            "enabled": true,
            "image": {"repository": repository, "tag": tag.unwrap_or("latest")},
        })
    });
    if let Some(map) = values.as_object_mut() {
        map.extend(
            [("builds", builds), ("proxy", proxy)]
                .into_iter()
                .filter_map(|(key, value)| Some((key.to_owned(), value?))),
        );
    }
    values
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
    use serde_json::json;

    use super::values;

    #[test]
    fn the_default_profile_sets_neither_builds_nor_the_proxy() {
        let values = values("zeabur:dev-0123456789ab", false, None);
        assert_eq!(
            values["image"],
            json!({"repository": "zeabur", "tag": "dev-0123456789ab", "digest": "",
                   "pullPolicy": "IfNotPresent"})
        );
        assert_eq!(values["issuer"]["url"], "https://identity.localhost");
        assert!(values.get("builds").is_none());
        assert!(values.get("proxy").is_none());
    }

    #[test]
    fn a_proxy_built_here_is_enabled_with_its_image() {
        let values = values(
            "zeabur:dev-1",
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
}
