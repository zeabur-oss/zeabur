//! The plan of a node joining an existing cluster: RKE2 only, as an agent (worker) or a server,
//! with what the joining VM measured about its distance to the server.
//!
//! Every node of a cluster belongs at one provider in one region. etcd waits for a majority on
//! every write, the kubelet and the API server talk constantly, and volumes are local, so a node
//! farther away slows the whole cluster or drops out of it. `plan` measures the round trip and
//! always says the rule.

use serde::Serialize;

use crate::PlanError;
use crate::config::{JoinConfig, Role};
use crate::plan::{Facts, Rke2Plan, Warning, rke2_plan, warning};
use crate::rke2::{self, HostFile};
use crate::storage::{self, NodePrep};

/// Connections timed to the server's port 9345.
pub const RTT_SAMPLES: usize = 5;
/// Above this median round trip, in microseconds, the server is farther than one region allows:
/// between availability zones of one region round trips are about 0.5 to 2 ms.
pub const RTT_WARN_US: u64 = 10_000;
/// Above this median round trip, in microseconds, the cluster is at risk: etcd's heartbeat is
/// 100 ms by default and every write waits for a majority round trip.
pub const RTT_CRITICAL_US: u64 = 50_000;

/// The supervisor port; a node cannot join without it.
pub const SUPERVISOR_PORT: u16 = 9345;

/// TCP ports on the server this node must reach.
#[must_use]
pub fn server_ports(role: Role) -> Vec<(u16, &'static str)> {
    let mut ports = vec![
        (SUPERVISOR_PORT, "RKE2 supervisor"),
        (6443, "Kubernetes API"),
        (10250, "kubelet"),
    ];
    if role == Role::Server {
        ports.extend([(2379, "etcd client"), (2380, "etcd peer")]);
    }
    ports
}

/// What the joining VM found out, read by the caller so planning stays free of I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinFacts {
    /// The host.
    pub facts: Facts,
    /// TCP connect times to the server's port 9345, in microseconds; empty when it did not answer.
    pub rtt_us: Vec<u64>,
    /// Each port of `server_ports` and whether a TCP connection to it succeeded.
    pub ports: Vec<(u16, bool)>,
    /// The token carries the cluster CA's hash, so the node checks whom it joins.
    pub token_pinned: bool,
}

/// One port and what was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    /// The port number.
    pub number: u16,
    /// `tcp` or `udp`.
    pub protocol: &'static str,
    /// What uses it.
    pub purpose: &'static str,
    /// Whether it answered; `None` when it cannot be checked from here (UDP).
    pub open: Option<bool>,
}

/// Round trips to the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rtt {
    /// Median, in microseconds.
    pub median_us: u64,
    /// Fastest.
    pub min_us: u64,
    /// Slowest.
    pub max_us: u64,
    /// How many connections were timed.
    pub samples: usize,
}

/// How far the server is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Latency {
    /// Within what one region gives.
    Fine,
    /// More than one region gives.
    High,
    /// The cluster will be slow and unstable.
    Critical,
}

/// The latency class of a median round trip.
#[must_use]
pub const fn latency(median_us: u64) -> Latency {
    if median_us > RTT_CRITICAL_US {
        Latency::Critical
    } else if median_us > RTT_WARN_US {
        Latency::High
    } else {
        Latency::Fine
    }
}

/// The median of the samples (the lower middle one for an even count).
#[must_use]
pub fn rtt(samples: &[u64]) -> Option<Rtt> {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let median = *sorted.get((sorted.len().checked_sub(1)?) / 2)?;
    Some(Rtt {
        median_us: median,
        min_us: *sorted.first()?,
        max_us: *sorted.last()?,
        samples: sorted.len(),
    })
}

/// Milliseconds with one decimal.
#[must_use]
pub fn ms(us: u64) -> String {
    let tenths = (us + 50) / 100;
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// The plan of a joining node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinPlan {
    /// Facts it was computed from.
    pub facts: Facts,
    /// `worker` or `server`.
    pub role: Role,
    /// The supervisor URL.
    pub server: String,
    /// The systemd unit: `rke2-agent` or `rke2-server`.
    pub unit: &'static str,
    /// The RKE2 release; it must be the cluster's.
    pub rke2: Rke2Plan,
    /// Files written before RKE2 starts. The token file is not among them.
    #[serde(skip)]
    pub host_files: Vec<HostFile>,
    /// Their paths, for the JSON form.
    pub host_file_paths: Vec<String>,
    /// What this node needs before the cluster's components run on it.
    pub node_prep: Option<NodePrep>,
    /// Round trips to the server.
    pub rtt: Option<Rtt>,
    /// Ports on the server, and UDP ports that cannot be checked.
    pub ports: Vec<Port>,
    /// Ports on this VM that `apply` checks are free before RKE2 starts.
    pub local_ports: Vec<Port>,
    /// Things to read before applying.
    pub warnings: Vec<Warning>,
    /// Things `apply` refuses to go on with.
    pub blockers: Vec<Warning>,
}

fn rtt_warning(rtt: Rtt) -> Option<Warning> {
    let median = ms(rtt.median_us);
    match latency(rtt.median_us) {
        Latency::Fine => None,
        Latency::High => Some(warning(
            "join.rtt.high",
            &[&median, &ms(RTT_WARN_US)],
            format!(
                "The server is {median} ms away (median of {} TCP connects to port \
                 {SUPERVISOR_PORT}); within a region it is usually under {} ms.",
                rtt.samples,
                ms(RTT_WARN_US)
            ),
        )),
        Latency::Critical => Some(warning(
            "join.rtt.critical",
            &[&median, &ms(RTT_CRITICAL_US)],
            format!(
                "The server is {median} ms away (median of {} TCP connects to port \
                 {SUPERVISOR_PORT}). Above {} ms the cluster will be slow and unstable.",
                rtt.samples,
                ms(RTT_CRITICAL_US)
            ),
        )),
    }
}

/// The server's ports with what was found, a warning for closed ones besides 9345 (a blocker of
/// its own), and the UDP port that cannot be checked.
fn ports(role: Role, found: &[(u16, bool)], host: &str, warnings: &mut Vec<Warning>) -> Vec<Port> {
    let mut ports: Vec<Port> = server_ports(role)
        .into_iter()
        .map(|(port, purpose)| Port {
            number: port,
            protocol: "tcp",
            purpose,
            open: found
                .iter()
                .find(|(p, _)| *p == port)
                .map(|(_, open)| *open),
        })
        .collect();
    let closed: Vec<String> = ports
        .iter()
        .filter(|p| p.number != SUPERVISOR_PORT && p.open == Some(false))
        .map(|p| p.number.to_string())
        .collect();
    if !closed.is_empty() {
        let list = closed.join(", ");
        warnings.push(warning(
            "join.port.closed",
            &[&list, host],
            format!(
                "TCP {list} on {host} cannot be reached from this VM; check the firewall or \
                 security group."
            ),
        ));
    }
    ports.push(Port {
        number: 8472,
        protocol: "udp",
        purpose: "Canal VXLAN",
        open: None,
    });
    ports
}

fn same_region() -> Warning {
    warning(
        "join.sameRegion",
        &[],
        "Put every node at the same hosting provider and in the same region. Different \
         availability zones in one region are fine. Between providers or regions, latency slows \
         etcd, the Kubernetes API and storage, and can make the cluster unstable."
            .to_owned(),
    )
}

/// What `node add` tells before a node joins: the same messages `plan` shows on the new VM, less
/// what only the VM can find out.
#[must_use]
pub fn advice(role: Role, longhorn: bool) -> Vec<Warning> {
    let mut all = vec![same_region()];
    all.extend(
        standing_warnings(role, true, longhorn)
            .into_iter()
            .filter(|w| w.code != "join.udp"),
    );
    all
}

/// What every join is told: the ports, UDP, high availability for servers, where volumes live,
/// and an unpinned token.
fn standing_warnings(role: Role, token_pinned: bool, longhorn: bool) -> Vec<Warning> {
    let mut warnings = Vec::new();
    warnings.push(warning(
        "join.ports",
        &[],
        "Allow these between the nodes: TCP 9345 and 6443 to the servers, TCP 10250 and 9099 and \
         UDP 8472 (Canal VXLAN) between all nodes, and TCP 2379 to 2381 between servers. Keep UDP \
         8472 closed to the internet."
            .to_owned(),
    ));
    warnings.push(warning(
        "join.udp",
        &[],
        "UDP 8472 cannot be checked from here. If it is blocked, pods on different nodes cannot \
         reach each other."
            .to_owned(),
    ));
    if role == Role::Server {
        warnings.push(warning(
            "join.servers.ha",
            &[],
            "The control plane survives losing a server only with three or more servers: etcd \
             needs a majority. With two, losing either one stops the cluster, so add a third."
                .to_owned(),
        ));
    }
    if longhorn {
        warnings.push(warning(
            "join.storage.longhorn",
            &[],
            "The cluster runs Longhorn: apply installs an iSCSI initiator and an NFS client here, \
             starts iscsid and loads iscsi_tcp. Longhorn then keeps replicas under \
             /var/lib/longhorn on this node's disk. Volumes on local-path still stay on the node \
             that created them."
                .to_owned(),
        ));
    } else {
        warnings.push(warning(
            "join.storage.local",
            &[],
            "Volumes on the default local-path storage class stay on the node that created them: \
             a pod using one runs only on that node, and the data goes with the node."
                .to_owned(),
        ));
    }
    if !token_pinned {
        warnings.push(warning(
            "join.token.unpinned",
            &[],
            "The token has no CA hash (K10<hash>::…), so this VM trusts whichever server answers \
             at that address. Use the token exactly as node add printed it."
                .to_owned(),
        ));
    }
    warnings
}

/// Computes the plan of a joining node.
///
/// # Errors
///
/// When the architecture has no RKE2 release.
pub fn plan(config: &JoinConfig, facts: &JoinFacts) -> Result<JoinPlan, PlanError> {
    let join = &config.join;
    let mut warnings = Vec::new();
    let mut blockers = Vec::new();
    let rke2 = rke2_plan(
        config.rke2_version.as_deref(),
        &facts.facts.arch,
        &mut warnings,
    )?;

    let (mut host_files, unit) = match join.role {
        Role::Worker => (rke2::agent_files(), "rke2-agent"),
        Role::Server => {
            let server_config = join.server_config.clone().unwrap_or_default();
            (
                rke2::host_files(&rke2::Options {
                    tls_san: config.host.tls_san.clone(),
                    disable: server_config.disable,
                    identity: server_config.identity,
                }),
                "rke2-server",
            )
        }
    };
    host_files.push(rke2::join_file(&join.server));
    host_files.extend(rke2::node_ip_file(&facts.facts.node_ips));
    if let Some(zot) = &join.zot {
        host_files.push(rke2::registries_file(&zot.cluster_ip.to_string()));
        host_files.push(rke2::zot_ca_file(&zot.ca));
    }

    warnings.push(same_region());

    let (host, _) = crate::config::server_address(&join.server)?;
    let rtt = rtt(&facts.rtt_us);
    match rtt {
        None => blockers.push(warning(
            "join.server.unreachable",
            &[&join.server],
            format!(
                "Cannot connect to {}. Check the address and that TCP {SUPERVISOR_PORT} is open \
                 from this VM.",
                join.server
            ),
        )),
        Some(rtt) => warnings.extend(rtt_warning(rtt)),
    }

    let ports = ports(join.role, &facts.ports, &host, &mut warnings);
    warnings.extend(standing_warnings(
        join.role,
        facts.token_pinned,
        join.longhorn,
    ));

    Ok(JoinPlan {
        facts: facts.facts.clone(),
        role: join.role,
        server: join.server.clone(),
        unit,
        rke2,
        host_file_paths: host_files.iter().map(|f| f.path.clone()).collect(),
        host_files,
        node_prep: join.longhorn.then_some(storage::LONGHORN_PREP),
        rtt,
        ports,
        local_ports: rke2::node_ports(join.role == Role::Server)
            .into_iter()
            .map(|(number, purpose)| Port {
                number,
                protocol: "tcp",
                purpose,
                open: None,
            })
            .collect(),
        warnings,
        blockers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Document, JoinConfig};

    fn config(extra: &str) -> JoinConfig {
        let yaml = format!(
            "join:\n  server: https://10.0.0.5:9345\n  tokenFile: /etc/zeabur-installer/join-token\n{extra}"
        );
        match Document::parse(&yaml).unwrap() {
            Document::Join(c) => c,
            Document::Install(_) | Document::Personal(_) => panic!("not a join"),
        }
    }

    fn facts(rtt_us: &[u64]) -> JoinFacts {
        JoinFacts {
            facts: Facts {
                arch: "amd64".to_owned(),
                hostname: "vps-2".to_owned(),
                memory_mib: 4096,
                node_ips: vec!["10.0.0.6".to_owned(), "fd00::6".to_owned()],
            },
            rtt_us: rtt_us.to_vec(),
            ports: vec![(9345, true), (6443, true), (10250, false)],
            token_pinned: true,
        }
    }

    fn codes(plan: &JoinPlan) -> Vec<&str> {
        plan.warnings.iter().map(|w| w.code).collect()
    }

    #[test]
    fn rtt_thresholds() {
        assert_eq!(latency(800), Latency::Fine);
        assert_eq!(latency(RTT_WARN_US), Latency::Fine);
        assert_eq!(latency(RTT_WARN_US + 1), Latency::High);
        assert_eq!(latency(RTT_CRITICAL_US), Latency::High);
        assert_eq!(latency(RTT_CRITICAL_US + 1), Latency::Critical);
        // One slow handshake does not move the median.
        let r = rtt(&[900, 700, 250_000, 800, 600]).unwrap();
        assert_eq!((r.median_us, r.min_us, r.max_us), (800, 600, 250_000));
        assert_eq!(rtt(&[]), None);
        assert_eq!(rtt(&[3, 1, 2, 4]).unwrap().median_us, 2);
        assert_eq!(ms(649), "0.6");
        assert_eq!(ms(12_340), "12.3");
    }

    #[test]
    fn a_node_joining_a_cluster_with_zot_pulls_from_it_through_the_mirror() {
        let ca = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n";
        let c = config(&format!(
            "  zot:\n    clusterIp: 10.43.0.50\n    ca: |\n      {}\n",
            ca.trim_end().replace('\n', "\n      ")
        ));
        let p = plan(&c, &facts(&[500; 5])).unwrap();
        assert!(p.host_files.contains(&rke2::registries_file("10.43.0.50")));
        assert!(p.host_files.contains(&rke2::zot_ca_file(ca)));
        let bad = "join:\n  server: https://10.0.0.5:9345\n  tokenFile: /etc/t\n  zot:\n    \
                   clusterIp: 10.43.0.50\n    ca: not a certificate\n";
        assert!(Document::parse(bad).is_err());
    }

    #[test]
    fn a_worker_near_the_server() {
        let p = plan(&config(""), &facts(&[600, 700, 650, 900, 640])).unwrap();
        assert_eq!(p.unit, "rke2-agent");
        assert_eq!(
            p.host_file_paths,
            [
                "/etc/sysctl.d/60-rke2-cis.conf",
                "/etc/rancher/rke2/config.yaml",
                "/etc/rancher/rke2/config.yaml.d/10-join.yaml",
                "/etc/rancher/rke2/config.yaml.d/20-node-ip.yaml",
            ]
        );
        assert_eq!(
            p.local_ports.iter().map(|p| p.number).collect::<Vec<_>>(),
            [80, 443, 10250]
        );
        assert_eq!(
            codes(&p),
            [
                "join.sameRegion",
                "join.port.closed",
                "join.ports",
                "join.udp",
                "join.storage.local"
            ]
        );
        assert_eq!(p.blockers.len(), 0);
        let closed = &p.warnings[1];
        assert_eq!(closed.values, ["10250", "10.0.0.5"]);
        let udp = p.ports.iter().find(|p| p.protocol == "udp").unwrap();
        assert_eq!(udp.open, None);
    }

    #[test]
    fn far_servers_are_warned_about() {
        let c = config("");
        let p = plan(&c, &facts(&[70_000; 5])).unwrap();
        assert!(codes(&p).contains(&"join.rtt.critical"));
        let critical = p
            .warnings
            .iter()
            .find(|w| w.code == "join.rtt.critical")
            .unwrap();
        assert_eq!(critical.values, ["70.0", "50.0"]);

        let mut f = facts(&[20_000; 5]);
        f.token_pinned = false;
        let p = plan(&c, &f).unwrap();
        assert!(codes(&p).contains(&"join.rtt.high"));
        assert!(codes(&p).contains(&"join.token.unpinned"));

        let p = plan(&c, &facts(&[])).unwrap();
        assert_eq!(p.blockers[0].code, "join.server.unreachable");
    }

    #[test]
    fn a_node_joining_a_longhorn_cluster_is_prepared_for_it() {
        let p = plan(&config("  longhorn: true\n"), &facts(&[500; 5])).unwrap();
        assert_eq!(p.node_prep, Some(storage::LONGHORN_PREP));
        assert!(codes(&p).contains(&"join.storage.longhorn"));
        assert!(!codes(&p).contains(&"join.storage.local"));
        let p = plan(&config(""), &facts(&[500; 5])).unwrap();
        assert_eq!(p.node_prep, None);
    }

    #[test]
    fn node_add_gives_the_same_advice() {
        let codes = |role| {
            advice(role, false)
                .iter()
                .map(|w| w.code)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            codes(Role::Worker),
            ["join.sameRegion", "join.ports", "join.storage.local"]
        );
        assert_eq!(
            codes(Role::Server),
            [
                "join.sameRegion",
                "join.ports",
                "join.servers.ha",
                "join.storage.local"
            ]
        );
    }

    #[test]
    fn a_server_gets_the_server_files_and_the_ha_warning() {
        let c = config(
            "  role: server\n  serverConfig: {disable: [rke2-metrics-server], identity: true}\n",
        );
        let p = plan(&c, &facts(&[500; 5])).unwrap();
        assert_eq!(p.unit, "rke2-server");
        assert!(codes(&p).contains(&"join.servers.ha"));
        let config = p
            .host_files
            .iter()
            .find(|f| f.path.ends_with("/config.yaml"))
            .unwrap();
        assert!(
            config
                .content
                .contains("disable:\n  - rke2-metrics-server\n")
        );
        assert!(
            p.host_file_paths
                .iter()
                .any(|f| f.ends_with("50-zeabur-identity.yaml"))
        );
        assert!(p.ports.iter().any(|port| port.number == 2380));
    }
}
