//! Zeabur from its chart, with the values of this cluster.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::api::networking::v1::{
    HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
    IngressServiceBackend, IngressSpec, IngressTLS, ServiceBackendPort,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
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
    /// The full profile's database updater.
    pub(crate) database_updater_image: Option<&'a str>,
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
    secrets(dev)?;
    dev.apply_object(&identity_ingress())?;

    let values = dev.write(
        "values.json",
        &values(
            install.zeabur_image,
            install.proxy_image,
            install.database_updater_image,
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

/// Fetches the chart's dependency, Zeabur Identity's chart, when `chart` needs it.
pub(crate) fn dependencies(dev: &DevCluster, chart: &Path) -> Result<(), String> {
    if !needs_dependency_build(chart) {
        return Ok(());
    }
    dev.cmd("helm")
        .args(["dependency", "build"])
        .arg(chart)
        .quiet()
        .map_err(|e| format!("fetching Zeabur Identity's chart: {e}"))
}

/// Whether `helm dependency build` must fetch Zeabur Identity's chart: only for an unpacked chart
/// without `charts/`. A packaged chart carries its dependencies, and Helm refuses to update it.
fn needs_dependency_build(chart: &Path) -> bool {
    chart.is_dir() && !chart.join("charts").is_dir()
}

/// Zeabur's OIDC client and its secret, the plan key, the certificates and the CAs it trusts.
fn secrets(dev: &DevCluster) -> Result<(), String> {
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
        dev.apply_object(&secret("zeabur-oidc", "client-secret", client_secret))?;
    }
    if !has(&["-n", "zeabur", "get", "secret", "zeabur-plan-key"]) {
        dev.apply_object(&secret("zeabur-plan-key", "plan-key", &random_hex(32)?))?;
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
    Ok(())
}

fn secret(name: &str, key: &str, value: &str) -> Secret {
    Secret {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some("zeabur".to_owned()),
            ..ObjectMeta::default()
        },
        type_: Some("Opaque".to_owned()),
        string_data: Some(BTreeMap::from([(key.to_owned(), value.to_owned())])),
        ..Secret::default()
    }
}

/// The `ClusterIssuer` of this cluster's CA, which the full profile installs.
const ISSUER: &str = "zeabur-local-ca";

/// Zeabur Identity's Ingress at `IDENTITY_HOST`.
fn identity_ingress() -> Ingress {
    Ingress {
        metadata: ObjectMeta {
            name: Some("zeabur-identity".to_owned()),
            namespace: Some("zeabur-identity".to_owned()),
            annotations: Some(BTreeMap::from([(
                "cert-manager.io/cluster-issuer".to_owned(),
                ISSUER.to_owned(),
            )])),
            ..ObjectMeta::default()
        },
        spec: Some(IngressSpec {
            ingress_class_name: Some("traefik".to_owned()),
            tls: Some(vec![IngressTLS {
                hosts: Some(vec![IDENTITY_HOST.to_owned()]),
                secret_name: Some("zeabur-identity-tls".to_owned()),
            }]),
            rules: Some(vec![IngressRule {
                host: Some(IDENTITY_HOST.to_owned()),
                http: Some(HTTPIngressRuleValue {
                    paths: vec![HTTPIngressPath {
                        path: Some("/".to_owned()),
                        path_type: "Prefix".to_owned(),
                        backend: IngressBackend {
                            service: Some(IngressServiceBackend {
                                name: "zeabur-identity".to_owned(),
                                port: Some(ServiceBackendPort {
                                    name: Some("http".to_owned()),
                                    number: None,
                                }),
                            }),
                            resource: None,
                        },
                    }],
                }),
            }]),
            ..IngressSpec::default()
        }),
        ..Ingress::default()
    }
}

/// The chart's values for this cluster, with Zeabur Proxy when its image is built here. Zeabur
/// Identity is at xtask/pins/zeabur-identity, and the cluster's components are installed already.
fn values(image: &str, proxy_image: Option<&str>, database_updater_image: Option<&str>) -> Value {
    let (repository, tag) = split_tag(image);
    // The templates prefer a digest: an empty one clears the one a released chart pins.
    let digest = image.split_once('@').map_or("", |(_, digest)| digest);
    let mut values = json!({
        "image": {"repository": repository, "tag": tag.unwrap_or("latest"), "digest": digest,
                  "pullPolicy": "IfNotPresent"},
        "global": {"domain": "localhost"},
        "identity": {"enabled": false},
        "components": {"install": []},
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
            "clusterIssuer": ISSUER,
            "tls": {"secretName": "zeabur-tls"},
        },
    });
    let proxy = proxy_image.map(|image| {
        let (repository, tag) = split_tag(image);
        json!({
            "enabled": true,
            "image": {"repository": repository, "tag": tag.unwrap_or("latest")},
        })
    });
    if let (Some(map), Some(proxy)) = (values.as_object_mut(), proxy) {
        map.insert("proxy".to_owned(), proxy);
    }
    if let (Some(map), Some(image)) = (values.as_object_mut(), database_updater_image) {
        let (repository, tag) = split_tag(image);
        map.insert(
            "databaseUpdater".to_owned(),
            json!({
                "enabled": true,
                "image": {"repository": repository, "tag": tag.unwrap_or("latest"), "digest": ""},
                "httpRegistries": ["zeabur-dev-registry:5000"],
            }),
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

    use super::{needs_dependency_build, values};

    #[test]
    fn only_an_unpacked_chart_without_its_dependencies_gets_them_built() {
        let dir = tempfile::tempdir().unwrap();
        let unpacked = dir.path().join("zeabur");
        std::fs::create_dir(&unpacked).unwrap();
        assert!(needs_dependency_build(&unpacked));

        std::fs::create_dir(unpacked.join("charts")).unwrap();
        assert!(!needs_dependency_build(&unpacked));

        let packaged = dir.path().join("zeabur-0.3.0.tgz");
        std::fs::write(&packaged, b"").unwrap();
        assert!(!needs_dependency_build(&packaged));
    }

    #[test]
    fn the_default_profile_sets_neither_builds_nor_the_proxy() {
        let values = values("zeabur:dev-0123456789ab", None, None);
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
    fn a_digest_reference_keeps_its_digest() {
        let values = values("ghcr.io/zeabur-oss/zeabur@sha256:0123", None, None);
        assert_eq!(values["image"]["repository"], "ghcr.io/zeabur-oss/zeabur");
        assert_eq!(values["image"]["digest"], "sha256:0123");
    }

    #[test]
    fn a_proxy_built_here_is_enabled_with_its_image() {
        let values = values(
            "zeabur:dev-1",
            Some("docker.io/library/zeabur-proxy:dev-0123456789ab"),
            None,
        );
        assert_eq!(
            values["proxy"],
            json!({
                "enabled": true,
                "image": {"repository": "docker.io/library/zeabur-proxy", "tag": "dev-0123456789ab"},
            })
        );
    }

    #[test]
    fn full_profile_uses_the_database_updater_built_here() {
        let values = values("zeabur:dev-1", None, Some("example.test/updater:dev-2"));
        assert_eq!(
            values["databaseUpdater"],
            json!({
                "enabled": true,
                "image": {"repository": "example.test/updater", "tag": "dev-2", "digest": ""},
                "httpRegistries": ["zeabur-dev-registry:5000"],
            })
        );
    }
}
