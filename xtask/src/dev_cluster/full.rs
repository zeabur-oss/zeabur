//! The full profile: the components Zeabur works with, and Zeabur Builder from source.

use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::json;

use super::{DevCluster, ZOT_CA, ZOT_HOST, ZOT_IP, catalog, cluster, images};
use crate::process::wait_for;

const BUILDER_IMAGES: [&str; 3] = ["builder", "builder-job", "builder-railpack"];

/// Installs the full profile's components; `builder_src` is a working tree of Zeabur Builder,
/// or `None` for the pinned commit.
pub(crate) fn install(dev: &DevCluster, builder_src: Option<&Path>) -> Result<(), String> {
    // The ClusterIssuer local access names, signing with this cluster's CA.
    let tls = dev.tls();
    dev.apply_created(
        "cert-manager",
        &[
            "secret",
            "tls",
            "zeabur-local-ca",
            &format!("--cert={}", tls.join("ca.crt").display()),
            &format!("--key={}", tls.join("ca.key").display()),
        ],
    )?;
    let issuer = json!({
        "apiVersion": "cert-manager.io/v1",
        "kind": "ClusterIssuer",
        "metadata": {"name": "zeabur-local-ca"},
        "spec": {"ca": {"secretName": "zeabur-local-ca"}},
    });
    dev.apply(issuer.to_string())?;
    let stop_at = catalog::stop_at_from_env();
    for (component, namespace) in [
        ("databases/cloudnative-pg", "cnpg-system"),
        ("databases/percona-mongodb", "psmdb-operator"),
        ("image-updates/keel", "keel"),
    ] {
        catalog::install(dev, component, namespace, &[], stop_at.as_deref())?;
    }
    test_s3(dev)?;
    zot(dev)?;
    builder(dev, builder_src)
}

/// The test S3 the database backup test writes to, with its bucket.
fn test_s3(dev: &DevCluster) -> Result<(), String> {
    dev.kubectl()
        .arg("apply")
        .arg("-f")
        .arg(dev.root.join("web/e2e/manifests/s3.yaml"))
        .quiet()?;
    dev.rollout("s3-test", "deployment/seaweedfs", Duration::from_secs(180))?;
    wait_for("the test S3's bucket", Duration::from_secs(120), || {
        // weed shell answers on stderr.
        let output = dev
            .kubectl()
            .args(["-n", "s3-test", "exec", "deploy/seaweedfs", "--", "sh", "-c"])
            .arg("echo s3.bucket.create -name backups | weed shell -master=localhost:9333 -filer=localhost:8888")
            .output()?;
        let said =
            String::from_utf8_lossy(&output.stdout) + String::from_utf8_lossy(&output.stderr);
        if said.contains("created bucket") || said.contains("already exists") {
            Ok(())
        } else {
            Err(said.trim().to_owned())
        }
    })
}

/// zot from Zeabur Builder's component, with the fixed ClusterIP the node's mirror points at.
/// zot's CA goes where the mirror reads it, as zeabur-installer copies it; containerd reads it on
/// each pull.
fn zot(dev: &DevCluster) -> Result<(), String> {
    catalog::install(
        dev,
        "builds/zeabur-builder",
        "zot",
        &[&format!("cluster-ip={ZOT_IP}")],
        Some("builder"),
    )?;
    dev.kubectl()
        .args([
            "-n",
            "zot",
            "wait",
            "certificate/zot-tls",
            "--for=condition=Ready",
        ])
        .arg("--timeout=120s")
        .quiet()?;
    dev.rollout("zot", "statefulset/zot", Duration::from_secs(180))?;
    let ca = dev
        .kubectl()
        .args(["-n", "zot", "get", "secret", "zot-tls", "-o"])
        .arg(r#"go-template={{index .data "ca.crt" | base64decode}}"#)
        .read()?;
    dev.write("zot-ca.crt", &ca)?;
    dev.cmd("docker")
        .args(["exec", "-i", cluster::NODE, "sh", "-c"])
        .arg(format!("cat >{ZOT_CA}.tmp && mv {ZOT_CA}.tmp {ZOT_CA}"))
        .input(ca)
        .quiet()
}

/// Zeabur Builder, built from source and installed from its chart.
fn builder(dev: &DevCluster, builder_src: Option<&Path>) -> Result<(), String> {
    let src = if let Some(src) = builder_src {
        src.to_path_buf()
    } else {
        let dir = dev.state.join("builder");
        images::pinned_checkout(
            dev,
            &dir,
            "ZEABUR_BUILDER_PINNED",
            "ZEABUR_BUILDER_GIT",
            "https://github.com/zeabur-oss/builder.git",
        )?;
        dir
    };
    let src = fs::canonicalize(&src)
        .map_err(|e| format!("Zeabur Builder's source {}: {e}", src.display()))?;
    dev.cmd("nix")
        .arg("develop")
        .arg(&src)
        .args([
            "--command",
            "./scripts/build-images.sh",
            "zeabur-builder",
            "dev",
        ])
        .current_dir(&src)
        .run()
        .map_err(|e| format!("building Zeabur Builder: {e}"))?;
    let built: Vec<String> = BUILDER_IMAGES
        .iter()
        .map(|name| format!("zeabur-builder/{name}:dev"))
        .collect();
    let ids = built
        .iter()
        .map(|image| images::image_id(dev, image))
        .collect::<Result<Vec<_>, _>>()?;
    let tag = images::joint_tag(&ids);
    let mut tagged = Vec::new();
    for (image, name) in built.iter().zip(BUILDER_IMAGES) {
        let image_tag = format!("zeabur-builder/{name}:{tag}");
        images::tag(dev, image, &image_tag)?;
        tagged.push(image_tag);
    }
    images::import(dev, &tagged)?;

    let chart = src.join("deploy/chart");
    // Helm installs the CRDs only on the first install.
    if dev
        .cmd("helm")
        .args(["-n", "builder-system", "status", "builder"])
        .attempt()
    {
        dev.kubectl()
            .args(["apply", "--server-side", "-f"])
            .arg(chart.join("crds/crds.yaml"))
            .quiet()?;
    }
    // zot takes pushes only with a token that Builder signs with the key of zot-token-signer.
    // buildkitd trusts zot's CA, copied from zot-tls at install time.
    let mut helm = dev
        .cmd("helm")
        .args(["upgrade", "--install", "builder"])
        .arg(&chart)
        .args(["-n", "builder-system", "--create-namespace"]);
    for (value, image) in [
        ("image", "builder"),
        ("jobImage", "builder-job"),
        ("railpackImage", "builder-railpack"),
    ] {
        helm = helm
            .arg("--set")
            .arg(format!("{value}.repository=zeabur-builder/{image}"))
            .arg("--set")
            .arg(format!("{value}.tag={tag}"));
    }
    for value in [
        format!("registry.host={ZOT_HOST}"),
        "registry.tokenKey.namespace=zot".to_owned(),
        "registry.tokenKey.secret=zot-token-signer".to_owned(),
        "registry.caFrom.namespace=zot".to_owned(),
        "registry.caFrom.secret=zot-tls".to_owned(),
        "registry.inCluster.namespace=zot".to_owned(),
        r"registry.inCluster.podLabels.app\.kubernetes\.io/name=zot".to_owned(),
    ] {
        helm = helm.arg("--set").arg(value);
    }
    helm.args([
        "--set-json",
        r#"extraArgs=["--buildkitd-host-users=false"]"#,
    ])
    .quiet()
    .map_err(|e| format!("installing Zeabur Builder: {e}"))?;
    dev.rollout(
        "builder-system",
        "deployment/zeabur-builder",
        Duration::from_secs(180),
    )?;
    dev.platform("builder-system")
}
