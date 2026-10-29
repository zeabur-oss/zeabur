//! The plan of a node joining an existing cluster: K3s or RKE2 only, as an agent (worker) or, on
//! RKE2, a server, with what the joining VM measured about its distance to the server.
//!
//! Every node of a cluster belongs at one provider in one region. etcd waits for a majority on
//! every write, the kubelet and the API server talk constantly, and volumes are local, so a node
//! farther away slows the whole cluster or drops out of it. `plan` measures the round trip and
//! always says the rule.

use serde::Serialize;

use crate::cluster::{self, Distribution, HostFile};
use crate::config::{JoinConfig, Role};
use crate::plan::{ClusterPlan, Facts, Warning, cluster_plan, warning};
use crate::storage::{self, NodePrep};
use crate::{PlanError, node_name};

/// Connections timed to the server's join port.
pub const RTT_SAMPLES: usize = 5;
/// Above this median round trip, in microseconds, the server is farther than one region allows:
/// between availability zones of one region round trips are about 0.5 to 2 ms.
pub const RTT_WARN_US: u64 = 10_000;
/// Above this median round trip, in microseconds, the cluster is at risk: etcd's heartbeat is
/// 100 ms by default and every write waits for a majority round trip.
pub const RTT_CRITICAL_US: u64 = 50_000;

/// TCP ports on the server this node must reach. The first one is the join port; a node cannot
/// join without it.
#[must_use]
pub fn server_ports(distribution: Distribution, role: Role) -> Vec<(u16, &'static str)> {
    let mut ports = match distribution {
        Distribution::K3s => vec![(6443, "Kubernetes API and K3s supervisor")],
        Distribution::Rke2 => vec![(9345, "RKE2 supervisor"), (6443, "Kubernetes API")],
    };
    ports.push((10250, "kubelet"));
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
    /// TCP connect times to the server's join port, in microseconds; empty when it did not answer.
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
    /// The node's name in Kubernetes, written into the distribution's configuration.
    pub node_name: String,
    /// `worker` or `server`.
    pub role: Role,
    /// The URL it joins through.
    pub server: String,
    /// The systemd unit, such as `k3s-agent` or `rke2-server`.
    pub unit: &'static str,
    /// The release; it must be the cluster's.
    pub cluster: ClusterPlan,
    /// Files written before the node starts. The token file is not among them.
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
    /// Ports on this VM that `apply` checks are free before the node starts.
    pub local_ports: Vec<Port>,
    /// Things to read before applying.
    pub warnings: Vec<Warning>,
    /// Things `apply` refuses to go on with.
    pub blockers: Vec<Warning>,
}

fn rtt_warning(rtt: Rtt, port: u16) -> Option<Warning> {
    let median = ms(rtt.median_us);
    match latency(rtt.median_us) {
        Latency::Fine => None,
        Latency::High => Some(warning(
            "join.rtt.high",
            &[&median, &ms(RTT_WARN_US)],
            format!(
                "The server is {median} ms away (median of {} TCP connects to port {port}); \
                 within a region it is usually under {} ms.",
                rtt.samples,
                ms(RTT_WARN_US)
            ),
        )),
        Latency::Critical => Some(warning(
            "join.rtt.critical",
            &[&median, &ms(RTT_CRITICAL_US)],
            format!(
                "The server is {median} ms away (median of {} TCP connects to port {port}). \
                 Above {} ms the cluster will be slow and unstable.",
                rtt.samples,
                ms(RTT_CRITICAL_US)
            ),
        )),
    }
}

/// The server's ports with what was found, a warning for closed ones besides the join port (a
/// blocker of its own), and the UDP port that cannot be checked.
fn ports(
    distribution: Distribution,
    role: Role,
    found: &[(u16, bool)],
    host: &str,
    warnings: &mut Vec<Warning>,
) -> Vec<Port> {
    let mut ports: Vec<Port> = server_ports(distribution, role)
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
        .filter(|p| p.number != distribution.join_port() && p.open == Some(false))
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
    let (number, purpose) = distribution.vxlan();
    ports.push(Port {
        number,
        protocol: "udp",
        purpose,
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
pub fn advice(distribution: Distribution, role: Role, longhorn: bool) -> Vec<Warning> {
    let mut all = vec![same_region()];
    all.extend(
        standing_warnings(distribution, role, true, longhorn)
            .into_iter()
            .filter(|w| w.code != "join.udp"),
    );
    all
}

/// What every join is told: the ports, UDP, high availability for servers, where volumes live,
/// and an unpinned token.
fn standing_warnings(
    distribution: Distribution,
    role: Role,
    token_pinned: bool,
    longhorn: bool,
) -> Vec<Warning> {
    let mut warnings = Vec::new();
    let (udp, network) = distribution.vxlan();
    let udp = udp.to_string();
    warnings.push(match distribution {
        Distribution::K3s => warning(
            "join.ports",
            &[&udp, network],
            format!(
                "Allow these between the nodes: TCP 6443 to the server, TCP 10250 and UDP {udp} \
                 ({network}) between all nodes. Keep UDP {udp} closed to the internet."
            ),
        ),
        Distribution::Rke2 => warning(
            "join.ports",
            &[&udp, network],
            format!(
                "Allow these between the nodes: TCP 9345 and 6443 to the servers, TCP 10250 and \
                 9099 and UDP {udp} ({network}) between all nodes, and TCP 2379 to 2381 between \
                 servers. Keep UDP {udp} closed to the internet."
            ),
        ),
    });
    warnings.push(warning(
        "join.udp",
        &[&udp],
        format!(
            "UDP {udp} cannot be checked from here. If it is blocked, pods on different nodes \
             cannot reach each other."
        ),
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
/// When the architecture has no release, K3s is asked for a second server, or the node name
/// cannot be used.
pub fn plan(config: &JoinConfig, facts: &JoinFacts) -> Result<JoinPlan, PlanError> {
    let join = &config.join;
    let node_name = node_name::choose(
        config.host.node_name.as_deref(),
        &facts.facts,
        &join.node_names,
    )?;
    let distribution = config.distribution;
    let mut warnings = Vec::new();
    let mut blockers = Vec::new();
    let version = match distribution {
        Distribution::K3s => config.k3s_version.as_deref(),
        Distribution::Rke2 => config.rke2_version.as_deref(),
    };
    let cluster = cluster_plan(distribution, version, &facts.facts.arch, &mut warnings)?;

    let (mut host_files, unit) = match join.role {
        Role::Worker => (distribution.agent_files(), distribution.agent_unit()),
        Role::Server if !distribution.takes_servers() => return Err(PlanError::SecondServer),
        Role::Server => {
            let server_config = join.server_config.clone().unwrap_or_default();
            (
                distribution.server_files(&cluster::Options {
                    tls_san: config.host.tls_san.clone(),
                    disable: server_config.disable,
                    identity: server_config.identity,
                }),
                distribution.server_unit(),
            )
        }
    };
    host_files.push(distribution.join_file(&join.server));
    host_files.push(distribution.node_name_file(&node_name));
    host_files.extend(distribution.node_ip_file(&facts.facts.node_ips));
    if let Some(registry) = &join.registry {
        host_files
            .push(distribution.registries_file(&registry.host, &registry.cluster_ip.to_string()));
        host_files.push(distribution.registry_ca_file(&registry.ca));
    }

    warnings.push(same_region());

    let (host, port) = crate::config::server_address(&join.server, distribution.join_port())?;
    let rtt = rtt(&facts.rtt_us);
    match rtt {
        None => blockers.push(warning(
            "join.server.unreachable",
            &[&join.server],
            format!(
                "Cannot connect to {}. Check the address and that TCP {port} is open from this VM.",
                join.server
            ),
        )),
        Some(rtt) => warnings.extend(rtt_warning(rtt, port)),
    }

    let ports = ports(distribution, join.role, &facts.ports, &host, &mut warnings);
    warnings.extend(standing_warnings(
        distribution,
        join.role,
        facts.token_pinned,
        join.longhorn,
    ));

    Ok(JoinPlan {
        facts: facts.facts.clone(),
        node_name,
        role: join.role,
        server: join.server.clone(),
        unit,
        cluster,
        host_file_paths: host_files.iter().map(|f| f.path.clone()).collect(),
        host_files,
        node_prep: join.longhorn.then_some(storage::LONGHORN_PREP),
        rtt,
        ports,
        local_ports: distribution
            .node_ports(join.role == Role::Server)
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
            "distribution: rke2\njoin:\n  server: https://10.0.0.5:9345\n  tokenFile: /etc/zeabur-installer/join-token\n{extra}"
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
                node_name: None,
                host_hex: "a1b2c3".to_owned(),
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
    fn a_node_joining_a_cluster_with_a_registry_pulls_from_it_through_the_mirror() {
        let ca = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n";
        let c = config(&format!(
            "  registry:\n    host: zot.zot.svc:5000\n    clusterIp: 10.43.0.50\n    ca: |\n      {}\n",
            ca.trim_end().replace('\n', "\n      ")
        ));
        let p = plan(&c, &facts(&[500; 5])).unwrap();
        let rke2 = Distribution::Rke2;
        let registries = rke2.registries_file("zot.zot.svc:5000", "10.43.0.50");
        assert!(registries.content.contains("https://10.43.0.50:5000"));
        assert!(p.host_files.contains(&registries));
        assert!(p.host_files.contains(&rke2.registry_ca_file(ca)));
        let bad = "join:\n  server: https://10.0.0.5:9345\n  tokenFile: /etc/t\n  registry:\n    \
                   host: zot.zot.svc:5000\n    clusterIp: 10.43.0.50\n    ca: not a certificate\n";
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
                "/etc/rancher/rke2/config.yaml.d/15-node-name.yaml",
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
    fn a_joining_node_is_named_once_and_apart_from_the_other_nodes() {
        let name_in = |p: &JoinPlan| {
            let file = p
                .host_files
                .iter()
                .find(|f| f.path.ends_with("15-node-name.yaml"))
                .unwrap();
            let value: serde_json::Value = serde_saphyr::from_str(&file.content).unwrap();
            value["node-name"].as_str().unwrap().to_owned()
        };
        let p = plan(&config(""), &facts(&[500; 5])).unwrap();
        assert_eq!(
            (p.node_name.as_str(), name_in(&p).as_str()),
            ("vps-2", "vps-2")
        );
        let taken = plan(&config("  nodeNames: [vps-1, vps-2]\n"), &facts(&[500; 5])).unwrap();
        assert_eq!(name_in(&taken), "node-a1b2c3");
        assert!(
            plan(
                &config("  nodeNames: [vps-1]\nhost: {nodeName: vps-1}\n"),
                &facts(&[500; 5])
            )
            .is_err()
        );
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
            advice(Distribution::Rke2, role, false)
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
