//! `plan`, `apply`, `doctor` and `uninstall` on a node that joins an existing cluster.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use serde_json::Value;
use zeabur_installer_host::kubectl::{self, Kubectl};
use zeabur_installer_host::preflight::{self, Encryption, Status};
use zeabur_installer_host::{facts, files, network, systemd};
use zeabur_installer_plan::cluster::{AUTHN_CONFIGMAP, Distribution};
use zeabur_installer_plan::config::{self, JoinConfig, Role};
use zeabur_installer_plan::join::{self, JoinFacts, JoinPlan, Latency, RTT_SAMPLES};

use crate::apply::{self, ClusterInstall};
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
    let distribution = config.distribution;
    let (host, port) = config::server_address(&config.join.server, distribution.join_port())?;
    let rtt_us = network::connect_times(&host, port, RTT_SAMPLES);
    let ports = join::server_ports(distribution, config.join.role)
        .into_iter()
        .map(|(p, _)| {
            let open = if p == port {
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

/// What the plan installs, for the preflight checks.
const fn node(plan: &JoinPlan) -> preflight::Node {
    match plan.role {
        Role::Server => preflight::Node::Server(plan.cluster.distribution),
        Role::Worker => preflight::Node::Agent(plan.cluster.distribution),
    }
}

/// The node's own `kubectl`, with the kubelet's credentials, which an agent also has.
fn own_kubectl(distribution: Distribution) -> Kubectl {
    let layout = distribution.layout();
    Kubectl::new(layout.kubectl, layout.kubelet_kubeconfig)
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
            node(&plan),
            config.host.require_encrypted_disk,
            plan.node_prep.as_ref(),
        ) {
            out::line("apply stops until what failed is fixed.");
        }
    }
    Ok(())
}

pub(crate) fn apply(config: &JoinConfig) -> Result<(), InstallError> {
    let (plan, token) = make_plan(config)?;
    out::line(render::join_plan(&plan).trim_end());
    if let Some(blocker) = plan.blockers.first() {
        return Err(InstallError::Message(blocker.text.clone()));
    }
    apply::check_host(
        node(&plan),
        config.host.require_encrypted_disk,
        plan.node_prep.as_ref(),
    )?;
    apply::prepare_node(plan.node_prep.as_ref())?;

    let distribution = plan.cluster.distribution;
    let layout = distribution.layout();
    let changed = files::write(
        Path::new(layout.join_token_file),
        0o600,
        format!("{token}\n").as_bytes(),
    )?;
    let started = apply::install_cluster(&ClusterInstall {
        cluster: &plan.cluster,
        calico: None,
        files: &plan.host_files,
        image_archives: &config.sources.image_archives,
        server: plan.role == Role::Server,
        changed,
    })?;

    let name = plan.node_name.clone();
    out::step(&format!("Waiting for node {name} to be Ready"));
    let own = own_kubectl(distribution);
    kubectl::poll(
        Duration::from_mins(10),
        &format!("node {name} to be Ready"),
        || node_ready(&own, &name, started).unwrap_or(false),
    )?;

    if plan.role == Role::Server {
        let admin = Kubectl::admin(distribution);
        admin.wait_ready(Duration::from_mins(5))?;
        sync_authn(&admin, distribution)?;
        node::report_sync(&admin)?;
    }

    out::step("Done");
    out::line(&format!(
        "{name} joined the cluster as a {}. On a server, see it with:\n  \
         sudo {} --kubeconfig {} get nodes -o wide",
        plan.role.as_str(),
        layout.kubectl,
        layout.admin_kubeconfig
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
fn sync_authn(admin: &Kubectl, distribution: Distribution) -> Result<bool, InstallError> {
    let Some(config) = authn_from_cluster(admin)? else {
        out::err(&format!(
            "    warning: kube-system/{AUTHN_CONFIGMAP} does not exist yet; run `zeabur-installer \
             apply` on the first server, then `zeabur-installer apply` here again"
        ));
        return Ok(false);
    };
    Ok(files::write(
        Path::new(distribution.layout().authn_file),
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
    let distribution = config.distribution;
    let unit = match config.join.role {
        Role::Worker => distribution.agent_unit(),
        Role::Server => distribution.server_unit(),
    };
    if !systemd::is_active(unit) {
        out::line(&format!(
            "CRITICAL {unit} is not running: journalctl -u {unit}"
        ));
        return ExitCode::from(2);
    }
    results.push((Status::Pass, format!("{unit} is running")));

    let name = facts::read().map_or_else(|_| String::new(), |f| f.own_node_name());
    let own = own_kubectl(distribution);
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
        let admin = Kubectl::admin(distribution);
        let authn_file = distribution.layout().authn_file;
        let local = std::fs::read_to_string(authn_file).ok();
        results.push(match authn_from_cluster(&admin) {
            Ok(Some(cluster)) if Some(&cluster) == local.as_ref() => (
                Status::Pass,
                "kube-apiserver trusts the same issuer as the first server".to_owned(),
            ),
            Ok(Some(_)) => (
                Status::Warn,
                format!(
                    "{authn_file} differs from kube-system/{AUTHN_CONFIGMAP}: run zeabur-installer apply here"
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
                 apply here, which registers the node with a stable address (node-ip), \
                 then zeabur-installer node sync on a server",
                problems.join("; ")
            ),
        )
    }
}

/// The round trip to the server, now.
fn distance(config: &JoinConfig) -> (Status, String) {
    let Ok((host, port)) =
        config::server_address(&config.join.server, config.distribution.join_port())
    else {
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
/// `purge`, once K3s or RKE2 is gone, removes what the installer left.
pub(crate) fn uninstall(config: &JoinConfig, path: &Path, purge: bool) -> Result<(), InstallError> {
    let left = crate::uninstall::leftovers(path, config.join.token_file.as_deref());
    if purge {
        return crate::uninstall::purge(&left, path);
    }
    if crate::uninstall::installed().is_none() {
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
    let name = facts::read().map_or_else(|_| "<node>".to_owned(), |f| f.own_node_name());
    let distribution = config.distribution;
    let layout = distribution.layout();
    let kubectl = format!(
        "{} --kubeconfig {}",
        layout.kubectl, layout.admin_kubeconfig
    );
    let script = match config.join.role {
        Role::Worker => layout.uninstall_agent,
        Role::Server => layout.uninstall_server,
    };
    let mut text = format!(
        "this node joined a cluster as a {}; Zeabur and Zeabur Identity run on the servers. To \
         take this node out:\n  on a server:  {kubectl} drain {name} --ignore-daemonsets \
         --delete-emptydir-data\n                {kubectl} delete node {name}\n  here:         \
         {script}\n  then, on a server:  zeabur-installer node sync   \
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
            prefix: None,
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
        for (distribution, kubectl, script) in [
            (
                "rke2",
                "/var/lib/rancher/rke2/bin/kubectl --kubeconfig /etc/rancher/rke2/rke2.yaml",
                "/usr/local/bin/rke2-uninstall.sh",
            ),
            (
                "k3s",
                "/var/lib/rancher/k3s/data/current/bin/kubectl --kubeconfig /etc/rancher/k3s/k3s.yaml",
                "/usr/local/bin/k3s-agent-uninstall.sh",
            ),
        ] {
            let config = JoinConfig::parse(&format!(
                "distribution: {distribution}\njoin:\n  server: https://10.0.0.5:6443\n  \
                 token: K10{hash}::abcdef.0123456789abcdef\n"
            ))
            .unwrap();
            let text = take_out(&config).to_string();
            assert!(text.contains(&format!("{kubectl} drain ")), "{text}");
            assert!(text.contains(&format!("{kubectl} delete node ")), "{text}");
            assert!(text.contains(script), "{text}");
            assert!(text.contains("zeabur-installer node sync"));
        }
    }
}
