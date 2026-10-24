//! The plan, written for people.

use zeabur_installer_plan::config::{AccessMode, Location, Role};
use zeabur_installer_plan::join::{self, JoinPlan};
use zeabur_installer_plan::plan::{Plan, ReleasePlan};
use zeabur_installer_plan::rke2::JOIN_TOKEN_FILE;
use zeabur_installer_plan::storage::NodePrep;

/// The plan as text.
pub(crate) fn plan(plan: &Plan) -> String {
    let mut s = String::new();
    let rke2 = &plan.rke2;
    let support = if rke2.pinned {
        "pinned, checked against this installer's digests"
    } else {
        "UNSUPPORTED"
    };
    push(
        &mut s,
        &format!(
            "RKE2 {} ({support}), CIS profile, on {} ({}, {} MiB)",
            rke2.version, plan.facts.hostname, plan.facts.arch, plan.facts.memory_mib
        ),
    );
    node_addresses(&mut s, &plan.facts.node_ips);
    push(&mut s, "");
    push(&mut s, "Components");
    for c in &plan.components {
        let memory = c
            .idle_memory_mib
            .map_or_else(|| "unmeasured".to_owned(), |m| format!("{m} MiB"));
        push(&mut s, &format!("  {:<48} {:<10} {memory}", c.id, c.why));
        for release in &c.releases {
            let what = match release {
                ReleasePlan::HelmChart { name, object } => format!(
                    "chart {name} {}",
                    object
                        .pointer("/spec/version")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                ),
                ReleasePlan::Manifests { name, .. } => format!("manifests {name}"),
                ReleasePlan::Urls { name, urls, .. } => {
                    format!("manifests {name} ({} URLs)", urls.len())
                }
            };
            push(&mut s, &format!("      {what}"));
        }
    }
    for (id, reason) in &plan.left_out {
        push(&mut s, &format!("  {id:<48} left out: {reason}"));
    }
    push(&mut s, "");
    let privileged: Vec<String> = plan
        .namespaces
        .iter()
        .filter_map(|n| {
            n.privileged
                .as_ref()
                .map(|why| format!("  {} ({why})", n.name))
        })
        .collect();
    if !privileged.is_empty() {
        push(
            &mut s,
            "Namespaces exempt from the restricted Pod Security level",
        );
        for line in &privileged {
            push(&mut s, line);
        }
        push(&mut s, "");
    }
    storage(&mut s, plan);
    if let (Some(identity), Some(zeabur)) = (&plan.identity, &plan.zeabur) {
        let certificates = match plan.access {
            AccessMode::Domain => "Let's Encrypt",
            AccessMode::Local => "a CA made on this server",
        };
        push(
            &mut s,
            &format!(
                "Zeabur           {} (certificates from {certificates})",
                zeabur.url
            ),
        );
        push(&mut s, &format!("Zeabur Identity  {}", identity.issuer));
        push(&mut s, "");
    }
    let m = &plan.memory;
    let unmeasured = if m.unmeasured > 0 {
        format!(", plus {} unmeasured", m.unmeasured)
    } else {
        String::new()
    };
    push(
        &mut s,
        &format!(
            "Memory at idle   about {} MiB of {} (RKE2 {}, components {}{unmeasured})",
            m.rke2_mib + m.components_mib,
            m.host_mib,
            m.rke2_mib,
            m.components_mib
        ),
    );
    for warning in &plan.warnings {
        push(&mut s, &format!("warning: {}", warning.text));
    }
    s
}

/// The plan of a joining node as text.
pub(crate) fn join_plan(plan: &JoinPlan) -> String {
    let mut s = String::new();
    let rke2 = &plan.rke2;
    let support = if rke2.pinned {
        "pinned, checked against this installer's digests"
    } else {
        "UNSUPPORTED"
    };
    let (what, runs) = match plan.role {
        Role::Worker => ("RKE2 agent", "a worker"),
        Role::Server => ("RKE2 server", "a server (control plane and etcd)"),
    };
    push(&mut s, &format!("Join {} as {runs}", plan.server));
    push(
        &mut s,
        &format!(
            "{what} {} ({support}), CIS profile, on {} ({}, {} MiB)",
            rke2.version, plan.facts.hostname, plan.facts.arch, plan.facts.memory_mib
        ),
    );
    push(
        &mut s,
        "Installs RKE2 only: Zeabur, Zeabur Identity and the catalog's components are already in \
         the cluster.",
    );
    node_addresses(&mut s, &plan.facts.node_ips);
    push(&mut s, "");
    push(&mut s, "Files");
    for path in &plan.host_file_paths {
        push(&mut s, &format!("  {path}"));
    }
    push(
        &mut s,
        &format!("  {JOIN_TOKEN_FILE} (the token, mode 0600)"),
    );
    push(&mut s, "");
    push(
        &mut s,
        &format!(
            "Location         this VM: {}; the server: {}",
            location(plan.location.as_ref()),
            location(plan.expect.as_ref())
        ),
    );
    match &plan.rtt {
        Some(rtt) => push(
            &mut s,
            &format!(
                "Distance         {} ms to the server (median of {} TCP connects; {} to {})",
                join::ms(rtt.median_us),
                rtt.samples,
                join::ms(rtt.min_us),
                join::ms(rtt.max_us)
            ),
        ),
        None => push(&mut s, "Distance         the server did not answer"),
    }
    if !plan.labels.is_empty() {
        let labels: Vec<String> = plan
            .labels
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        push(&mut s, &format!("Node labels      {}", labels.join(", ")));
    }
    if let Some(prep) = &plan.node_prep {
        node_prep(&mut s, prep);
    }
    ports(&mut s, plan);
    for blocker in &plan.blockers {
        push(&mut s, &format!("error: {}", blocker.text));
    }
    for warning in &plan.warnings {
        push(&mut s, &format!("warning: {}", warning.text));
    }
    s
}

/// The storage classes, and what the nodes need for them.
fn storage(s: &mut String, plan: &Plan) {
    let mut classes = vec![format!("{} (default)", plan.storage.default_class)];
    classes.extend(plan.storage.other_classes.iter().cloned());
    push(s, &format!("Storage          {}", classes.join(", ")));
    if let Some(prep) = &plan.node_prep {
        node_prep(s, prep);
    }
    push(s, "");
}

/// What apply installs and starts on the node for a component, where it is missing.
fn node_prep(s: &mut String, prep: &NodePrep) {
    let packages: Vec<&str> = prep.packages.iter().map(|p| p.binary).collect();
    push(
        s,
        &format!(
            "Node preparation {} needs {}, the {} module and {} running; apply installs and \
             starts what is missing",
            prep.component,
            packages.join(", "),
            prep.modules.join(", "),
            prep.services.join(", ")
        ),
    );
}

/// The ports on the server this VM must reach, and the ones on this VM that must be free.
fn ports(s: &mut String, plan: &JoinPlan) {
    push(s, "Ports on the server");
    for port in &plan.ports {
        let state = match port.open {
            Some(true) => "open   ",
            Some(false) => "CLOSED ",
            None => "unknown",
        };
        push(
            s,
            &format!(
                "  {state} {} {:<5} {}",
                port.protocol.to_ascii_uppercase(),
                port.number,
                port.purpose
            ),
        );
    }
    push(
        s,
        "Ports on this VM, free before RKE2 starts (apply checks them)",
    );
    for port in &plan.local_ports {
        push(
            s,
            &format!(
                "  {} {:<5} {}",
                port.protocol.to_ascii_uppercase(),
                port.number,
                port.purpose
            ),
        );
    }
    push(s, "");
}

/// The addresses the node registers with, which the network policies let in.
fn node_addresses(s: &mut String, addresses: &[String]) {
    let text = if addresses.is_empty() {
        "chosen by RKE2: this host's addresses could not be read".to_owned()
    } else {
        format!(
            "{} (temporary IPv6 addresses left out)",
            addresses.join(", ")
        )
    };
    push(s, &format!("Node addresses   {text}"));
}

fn location(location: Option<&Location>) -> String {
    location.map_or_else(
        || "unknown".to_owned(),
        |l| {
            let zone = l
                .zone
                .as_ref()
                .map_or_else(String::new, |z| format!(" ({z})"));
            format!("{} {}{zone}", l.provider, l.region)
        },
    )
}

fn push(s: &mut String, line: &str) {
    s.push_str(line);
    s.push('\n');
}

#[cfg(test)]
mod tests {
    use zeabur_installer_plan::config::Document;
    use zeabur_installer_plan::join::JoinFacts;
    use zeabur_installer_plan::plan::Facts;

    use super::*;

    fn plan(yaml: &str, rtt_us: &[u64], here: Option<Location>) -> JoinPlan {
        let Document::Join(config) = Document::parse(yaml).unwrap() else {
            panic!("not a join");
        };
        let facts = JoinFacts {
            facts: Facts {
                arch: "amd64".to_owned(),
                hostname: "vps-2".to_owned(),
                memory_mib: 8192,
                node_ips: vec!["10.0.0.6".to_owned(), "fd00::6".to_owned()],
            },
            location: here,
            rtt_us: rtt_us.to_vec(),
            ports: vec![
                (9345, !rtt_us.is_empty()),
                (6443, true),
                (10250, true),
                (2379, true),
                (2380, false),
            ],
            token_pinned: true,
        };
        join::plan(&config, &facts).unwrap()
    }

    fn hetzner(region: &str) -> Location {
        Location {
            provider: "hetzner".to_owned(),
            region: region.to_owned(),
            zone: Some(format!("{region}-dc14")),
        }
    }

    #[test]
    fn a_worker_in_the_same_region() {
        let p = plan(
            include_str!("../../../docs/reference/installer-examples/join.yaml"),
            &[640, 610, 700, 900, 650],
            Some(hetzner("fsn1")),
        );
        let text = join_plan(&p).replace(&p.rke2.version, "<rke2>");
        assert_eq!(text, include_str!("../fixtures/join-worker.txt"));
    }

    #[test]
    fn a_server_in_another_region() {
        let p = plan(
            "join:\n  server: https://10.0.0.5:9345\n  role: server\n  tokenFile: /root/token\n  \
             expect: {provider: hetzner, region: fsn1}\n  serverConfig: {disable: [rke2-metrics-server], identity: true}\n",
            &[24_000, 23_500, 25_100, 24_200, 23_900],
            Some(hetzner("hel1")),
        );
        let text = join_plan(&p).replace(&p.rke2.version, "<rke2>");
        assert_eq!(text, include_str!("../fixtures/join-server.txt"));
    }

    #[test]
    fn an_unreachable_server_is_an_error() {
        let p = plan(
            include_str!("../../../docs/reference/installer-examples/join.yaml"),
            &[],
            None,
        );
        let text = join_plan(&p);
        assert!(text.contains("Distance         the server did not answer"));
        assert!(text.contains("error: Cannot connect to https://10.0.0.5:9345."));
    }
}
