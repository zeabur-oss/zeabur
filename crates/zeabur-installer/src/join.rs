//! `plan`, `apply`, `doctor` and `uninstall` on a node that joins an existing cluster.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use serde_json::Value;
use zeabur_installer_host::kubectl::{self, Kubectl};
use zeabur_installer_host::preflight::{self, Encryption, Status};
use zeabur_installer_host::{facts, files, network, systemd};
use zeabur_installer_plan::config::{self, JoinConfig, Role};
use zeabur_installer_plan::join::{
    self, JoinFacts, JoinPlan, Latency, RTT_SAMPLES, SUPERVISOR_PORT,
};
use zeabur_installer_plan::rke2::{
    ADMIN_KUBECONFIG, AUTHN_CONFIGMAP, AUTHN_FILE, BIN_DIR, JOIN_TOKEN_FILE, KUBELET_KUBECONFIG,
};

use crate::apply::{self, Rke2Install};
use crate::{InstallError, node, out, render};

/// The join token, from the configuration or its file, checked against the role.
fn token(config: &JoinConfig) -> Result<String, InstallError> {
    let token = match (&config.join.token, &config.join.token_file) {
        (Some(token), _) => token.clone(),
        (None, Some(file)) => String::from_utf8(files::read_secret(Path::new(file))?)
            .map_err(|_e| InstallError::Message(format!("{file}: not text")))?,
        (None, None) => {
            return Err(InstallError::Message(
                "join: no token or tokenFile".to_owned(),
            ));
        }
    };
    let token = token.trim().to_owned();
    config::check_token(&token, config.join.role)?;
    Ok(token)
}

/// Reads the host and the way to the server.
fn read_facts(config: &JoinConfig, token: &str) -> Result<JoinFacts, InstallError> {
    let (host, port) = config::server_address(&config.join.server)?;
    let rtt_us = network::connect_times(&host, port, RTT_SAMPLES);
    let ports = join::server_ports(config.join.role)
        .into_iter()
        .map(|(p, _)| {
            let open = if p == SUPERVISOR_PORT {
                !rtt_us.is_empty()
            } else {
                network::open(&host, p)
            };
            (p, open)
        })
        .collect();
    Ok(JoinFacts {
        facts: facts::read()?,
        rtt_us,
        ports,
        token_pinned: config::token_pins_ca(token),
    })
}

fn make_plan(config: &JoinConfig) -> Result<(JoinPlan, String), InstallError> {
    let token = token(config)?;
    let facts = read_facts(config, &token)?;
    Ok((join::plan(config, &facts)?, token))
}

pub(crate) fn show_plan(config: &JoinConfig, json: bool) -> Result<(), InstallError> {
    let (plan, _) = make_plan(config)?;
    if json {
        let text = serde_json::to_string_pretty(&plan)
            .map_err(|e| InstallError::Message(e.to_string()))?;
        out::line(&text);
    } else {
        out::line(render::join_plan(&plan).trim_end());
        if apply::show_checks(
            plan.unit,
            config.host.require_encrypted_disk,
            plan.node_prep.as_ref(),
        ) {
            out::line("apply stops until what failed is fixed.");
        }
    }
    Ok(())
}

/// The node's name: RKE2 uses the host name, in lower case.
fn node_name(hostname: &str) -> String {
    hostname.to_ascii_lowercase()
}

pub(crate) fn apply(config: &JoinConfig) -> Result<(), InstallError> {
    let (plan, token) = make_plan(config)?;
    out::line(render::join_plan(&plan).trim_end());
    if let Some(blocker) = plan.blockers.first() {
        return Err(InstallError::Message(blocker.text.clone()));
    }
    apply::check_host(
        plan.unit,
        config.host.require_encrypted_disk,
        plan.node_prep.as_ref(),
    )?;
    apply::prepare_node(plan.node_prep.as_ref())?;

    let changed = files::write(
        Path::new(JOIN_TOKEN_FILE),
        0o600,
        format!("{token}\n").as_bytes(),
    )?;
    let started = apply::install_rke2(&Rke2Install {
        rke2: &plan.rke2,
        files: &plan.host_files,
        image_archives: &config.sources.image_archives,
        unit: plan.unit,
        server: plan.role == Role::Server,
        changed,
    })?;

    let name = node_name(&plan.facts.hostname);
    out::step(&format!("Waiting for node {name} to be Ready"));
    let own = Kubectl::new(format!("{BIN_DIR}/kubectl"), KUBELET_KUBECONFIG);
    kubectl::poll(
        Duration::from_mins(10),
        &format!("node {name} to be Ready"),
        || node_ready(&own, &name, started).unwrap_or(false),
    )?;

    if plan.role == Role::Server {
        let admin = Kubectl::rke2();
        admin.wait_ready(Duration::from_mins(5))?;
        sync_authn(&admin)?;
        node::report_sync(&admin)?;
    }

    out::step("Done");
    out::line(&format!(
        "{name} joined the cluster as a {}. On a server, see it with:\n  \
         {BIN_DIR}/kubectl --kubeconfig {ADMIN_KUBECONFIG} get nodes -o wide",
        plan.role.as_str()
    ));
    if plan.role == Role::Worker {
        out::line(
            "Then run `zeabur-installer node sync` there, so the namespaces' network policies let \
             this node's traffic in.",
        );
    }
    Ok(())
}

/// Whether the node reports Ready, after `since` (seconds since the Unix epoch) when given: the
/// kubelet reports its status as soon as it starts, so a heartbeat from before a restart is the
/// old kubelet's. The kubelet's own credentials may read its Node object, so this works on an
/// agent, which has no admin kubeconfig.
pub(crate) fn node_ready(
    kubectl: &Kubectl,
    name: &str,
    since: Option<u64>,
) -> Result<bool, InstallError> {
    let node = kubectl.get("node", name, None)?;
    Ok(node.as_ref().is_some_and(|n| ready_since(n, since)))
}

fn ready_since(node: &Value, since: Option<u64>) -> bool {
    node.pointer("/status/conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|c| {
            c.get("type").and_then(Value::as_str) == Some("Ready")
                && c.get("status").and_then(Value::as_str) == Some("True")
                && since.is_none_or(|since| {
                    c.get("lastHeartbeatTime")
                        .and_then(Value::as_str)
                        .and_then(kubectl::unix_seconds)
                        .is_some_and(|beat| beat > since)
                })
        })
}

/// Copies Zeabur Identity's authentication configuration from the cluster to this server, so its
/// kube-apiserver trusts the same issuer as the others. Returns whether the file changed.
fn sync_authn(admin: &Kubectl) -> Result<bool, InstallError> {
    let Some(config) = authn_from_cluster(admin)? else {
        out::err(&format!(
            "    warning: kube-system/{AUTHN_CONFIGMAP} does not exist yet; run `zeabur-installer \
             apply` on the first server, then `zeabur-installer apply` here again"
        ));
        return Ok(false);
    };
    Ok(files::write(
        Path::new(AUTHN_FILE),
        0o600,
        config.as_bytes(),
    )?)
}

fn authn_from_cluster(admin: &Kubectl) -> Result<Option<String>, InstallError> {
    Ok(admin
        .get("configmap", AUTHN_CONFIGMAP, Some("kube-system"))?
        .and_then(|c| {
            c.pointer("/data/authn.yaml")
                .and_then(Value::as_str)
                .map(str::to_owned)
        }))
}

pub(crate) fn doctor(config: &JoinConfig) -> ExitCode {
    if let Some(code) = crate::doctor::cannot_run() {
        return code;
    }
    let mut results: Vec<(Status, String)> = Vec::new();
    let unit = match config.join.role {
        Role::Worker => "rke2-agent",
        Role::Server => "rke2-server",
    };
    if !systemd::is_active(unit) {
        out::line(&format!(
            "CRITICAL {unit} is not running: journalctl -u {unit}"
        ));
        return ExitCode::from(2);
    }
    results.push((Status::Pass, format!("{unit} is running")));

    let name = facts::read().map_or_else(|_| String::new(), |f| node_name(&f.hostname));
    let own = Kubectl::new(format!("{BIN_DIR}/kubectl"), KUBELET_KUBECONFIG);
    results.push(match node_ready(&own, &name, None) {
        Ok(true) => (Status::Pass, format!("node {name} is Ready")),
        Ok(false) => (
            Status::Fail,
            format!("node {name} is not Ready: on a server, kubectl describe node {name}"),
        ),
        Err(e) => (Status::Fail, format!("reading node {name}: {e}")),
    });

    results.push(node_addresses(&own, &name));
    results.push(distance(config));

    if config.join.role == Role::Server {
        let admin = Kubectl::rke2();
        let local = std::fs::read_to_string(AUTHN_FILE).ok();
        results.push(match authn_from_cluster(&admin) {
            Ok(Some(cluster)) if Some(&cluster) == local.as_ref() => (
                Status::Pass,
                "kube-apiserver trusts the same issuer as the first server".to_owned(),
            ),
            Ok(Some(_)) => (
                Status::Warn,
                format!(
                    "{AUTHN_FILE} differs from kube-system/{AUTHN_CONFIGMAP}: run zeabur-installer apply here"
                ),
            ),
            Ok(None) => (
                Status::Pass,
                "no Zeabur Identity configuration in the cluster".to_owned(),
            ),
            Err(e) => (Status::Warn, format!("reading {AUTHN_CONFIGMAP}: {e}")),
        });
    }

    results.push(match preflight::disk_encryption("/var/lib/rancher") {
        Encryption::Encrypted => (Status::Pass, "/var/lib/rancher is encrypted".to_owned()),
        Encryption::Plain => (
            Status::Warn,
            "/var/lib/rancher is not on an encrypted device".to_owned(),
        ),
        Encryption::Unknown => (
            Status::Warn,
            "could not tell whether /var/lib/rancher is encrypted".to_owned(),
        ),
    });
    crate::doctor::report(&results)
}

/// Whether the node's `InternalIP`s are stable addresses of this host. A temporary IPv6 address
/// changes within a day; the network policies, which let in the old one, then drop the node's
/// traffic.
pub(crate) fn node_addresses(kubectl: &Kubectl, name: &str) -> (Status, String) {
    let node = match kubectl.get("node", name, None) {
        Ok(Some(node)) => node,
        Ok(None) => return (Status::Warn, format!("node {name} is not registered")),
        Err(e) => return (Status::Warn, format!("reading node {name}: {e}")),
    };
    let internal: Vec<String> = node
        .pointer("/status/addresses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|a| a.get("type").and_then(Value::as_str) == Some("InternalIP"))
        .filter_map(|a| a.get("address").and_then(Value::as_str).map(str::to_owned))
        .collect();
    address_check(name, &internal, &facts::host_addresses())
}

fn address_check(name: &str, internal: &[String], host: &[facts::HostAddress]) -> (Status, String) {
    let mut problems = Vec::new();
    for address in internal {
        match host.iter().find(|h| &h.address == address) {
            None => problems.push(format!("{address} is no longer an address of this host")),
            Some(h) if h.temporary => problems.push(format!(
                "{address} is a temporary IPv6 address, which changes within a day"
            )),
            Some(h) if h.unusable => problems.push(format!("{address} is deprecated")),
            Some(_) => {}
        }
    }
    if problems.is_empty() {
        (
            Status::Pass,
            format!("node {name} has stable addresses: {}", internal.join(", ")),
        )
    } else {
        (
            Status::Warn,
            format!(
                "node {name}: {}; the network policies fall behind it. Run zeabur-installer \
                 apply here, which registers the node with a stable address (RKE2 node-ip), \
                 then zeabur-installer node sync on a server",
                problems.join("; ")
            ),
        )
    }
}

/// The round trip to the server, now.
fn distance(config: &JoinConfig) -> (Status, String) {
    let Ok((host, port)) = config::server_address(&config.join.server) else {
        return (Status::Fail, "join.server is not a valid URL".to_owned());
    };
    let Some(rtt) = join::rtt(&network::connect_times(&host, port, RTT_SAMPLES)) else {
        return (
            Status::Fail,
            format!("{} does not answer from this node", config.join.server),
        );
    };
    let text = format!(
        "the server is {} ms away (median of {})",
        join::ms(rtt.median_us),
        rtt.samples
    );
    match join::latency(rtt.median_us) {
        Latency::Fine => (Status::Pass, text),
        Latency::High => (
            Status::Warn,
            format!(
                "{text}; within a region it is usually under {} ms",
                join::ms(join::RTT_WARN_US)
            ),
        ),
        Latency::Critical => (
            Status::Fail,
            format!(
                "{text}; above {} ms the cluster is slow and unstable",
                join::ms(join::RTT_CRITICAL_US)
            ),
        ),
    }
}

/// A joined node has no Zeabur to remove: says how to take the node out and what stays, or with
/// `purge`, once RKE2 is gone, removes what the installer left.
pub(crate) fn uninstall(config: &JoinConfig, path: &Path, purge: bool) -> Result<(), InstallError> {
    let left = crate::uninstall::leftovers(path, config.join.token_file.as_deref());
    if purge {
        return crate::uninstall::purge(&left, path);
    }
    if !crate::uninstall::rke2_installed() {
        out::line(&crate::uninstall::leftovers_text(&left));
        return Ok(());
    }
    let mut text = take_out(config).to_string();
    let left = crate::uninstall::leftovers_text(&left);
    if !left.is_empty() {
        text.push('\n');
        text.push_str(&left);
    }
    Err(InstallError::Message(text))
}

/// How to take a joined node out of the cluster.
fn take_out(config: &JoinConfig) -> InstallError {
    let name = facts::read().map_or_else(|_| "<node>".to_owned(), |f| node_name(&f.hostname));
    let kubectl = format!("{BIN_DIR}/kubectl --kubeconfig {ADMIN_KUBECONFIG}");
    let mut text = format!(
        "this node joined a cluster as a {}; Zeabur and Zeabur Identity run on the servers. To \
         take this node out:\n  on a server:  {kubectl} drain {name} --ignore-daemonsets \
         --delete-emptydir-data\n                {kubectl} delete node {name}\n  here:         \
         /usr/local/bin/rke2-uninstall.sh\n  then, on a server:  zeabur-installer node sync   \
         # takes its addresses out of the network policies",
        config.join.role.as_str()
    );
    if config.join.role == Role::Server {
        text.push_str(
            "\nA server is also an etcd member: remove it only while the others keep a majority.",
        );
    }
    text.push_str("\nVolumes of the local-path storage class on this node go with it.");
    InstallError::Message(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_and_vanished_addresses_are_warned_about() {
        let host = |address: &str, temporary| facts::HostAddress {
            interface: "eth0".to_owned(),
            address: address.to_owned(),
            temporary,
            unusable: false,
            permanent: false,
        };
        let hosts = [
            host("10.0.0.6", false),
            host("fd00::a", true),
            host("fd00::5", false),
        ];
        let ok = address_check(
            "vps-2",
            &["10.0.0.6".to_owned(), "fd00::5".to_owned()],
            &hosts,
        );
        assert_eq!(ok.0, Status::Pass);
        let temporary = address_check("vps-2", &["fd00::a".to_owned()], &hosts);
        assert_eq!(temporary.0, Status::Warn);
        assert!(
            temporary.1.contains("fd00::a is a temporary IPv6 address"),
            "{}",
            temporary.1
        );
        let gone = address_check("vps-2", &["fd00::b".to_owned()], &hosts);
        assert!(
            gone.1
                .contains("fd00::b is no longer an address of this host")
        );
    }

    #[test]
    fn ready_only_after_the_restart() {
        let node = serde_json::json!({"status": {"conditions": [
            {"type": "MemoryPressure", "status": "False", "lastHeartbeatTime": "2026-10-23T08:00:30Z"},
            {"type": "Ready", "status": "True", "lastHeartbeatTime": "2026-10-23T08:00:30Z"},
        ]}});
        let at = kubectl::unix_seconds("2026-10-23T08:00:30Z").unwrap();
        assert!(ready_since(&node, None));
        assert!(ready_since(&node, Some(at - 1)));
        assert!(!ready_since(&node, Some(at)));
        let not_ready = serde_json::json!({"status": {"conditions": [
            {"type": "Ready", "status": "Unknown", "lastHeartbeatTime": "2026-10-23T08:00:30Z"},
        ]}});
        assert!(!ready_since(&not_ready, None));
    }

    #[test]
    fn uninstall_says_how_to_take_the_node_out_with_the_servers_kubectl() {
        let hash = "a".repeat(64);
        let config = JoinConfig::parse(&format!(
            "join:\n  server: https://10.0.0.5:9345\n  token: K10{hash}::abcdef.0123456789abcdef\n"
        ))
        .unwrap();
        let text = take_out(&config).to_string();
        assert!(text.contains(
            "/var/lib/rancher/rke2/bin/kubectl --kubeconfig /etc/rancher/rke2/rke2.yaml drain "
        ));
        assert!(text.contains(
            "/var/lib/rancher/rke2/bin/kubectl --kubeconfig /etc/rancher/rke2/rke2.yaml delete node "
        ));
        assert!(text.contains("/usr/local/bin/rke2-uninstall.sh"));
        assert!(text.contains("zeabur-installer node sync"));
    }
}
