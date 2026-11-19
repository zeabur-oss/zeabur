//! The full profile: the components Zeabur works with, Zeabur Builder among them, from Zeabur
//! Catalog as people install them.

use std::time::Duration;

use serde_json::json;

use super::{DevCluster, ZOT_CA, ZOT_IP, catalog, cluster};
use crate::process::wait_for;

/// Installs the full profile's components.
pub(crate) fn install(dev: &DevCluster) -> Result<(), String> {
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
    for (component, namespace) in [
        ("cloudnative-pg", "cnpg-system"),
        ("percona-mongodb", "psmdb-operator"),
        ("keel", "keel"),
    ] {
        catalog::install(dev, component, namespace, &[])?;
    }
    test_s3(dev)?;
    builder(dev)
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
        // weed shell answers on stderr, and can wait for the master for ever; `timeout` makes
        // that one failed try.
        let output = dev
            .kubectl()
            .args(["-n", "s3-test", "exec", "deploy/seaweedfs", "--", "sh", "-c"])
            .arg("echo s3.bucket.create -name backups | timeout 20 weed shell -master=localhost:9333 -filer=localhost:8888")
            .output()?;
        let said =
            String::from_utf8_lossy(&output.stdout) + String::from_utf8_lossy(&output.stderr);
        if said.contains("created bucket") || said.contains("already exists") {
            Ok(())
        } else {
            Err(said.trim().to_owned())
        }
    })
    .inspect_err(|_| dev.diagnose("s3-test"))
}

/// Zeabur Builder's component: zot, with the fixed ClusterIP the node's mirror points at, and
/// Zeabur Builder's chart. zot's CA goes where the mirror reads it; containerd reads it on each
/// pull.
fn builder(dev: &DevCluster) -> Result<(), String> {
    catalog::install(
        dev,
        "zeabur-builder",
        "zot",
        &[&format!("clusterIp={ZOT_IP}")],
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
        .quiet()
        .inspect_err(|_| {
            dev.diagnose("zot");
            dev.diagnose("cert-manager");
        })?;
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
        .quiet()?;
    dev.rollout(
        "builder-system",
        "deployment/zeabur-builder",
        Duration::from_secs(180),
    )?;
    dev.platform("builder-system")
}
