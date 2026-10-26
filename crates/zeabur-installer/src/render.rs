//! The plan, written for people.

use zeabur_installer_plan::config::{AccessMode, Role};
use zeabur_installer_plan::join::{self, JoinPlan};
use zeabur_installer_plan::personal::PersonalPlan;
use zeabur_installer_plan::plan::{ComponentPlan, Plan, ReleasePlan};
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
    components(&mut s, &plan.components, &plan.left_out);
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
            m.cluster_mib + m.components_mib,
            m.host_mib,
            m.cluster_mib,
            m.components_mib
        ),
    );
    for warning in &plan.warnings {
        push(&mut s, &format!("warning: {}", warning.text));
    }
    s
}

fn components(
    s: &mut String,
    components: &[ComponentPlan],
    left_out: &std::collections::BTreeMap<String, String>,
) {
    push(s, "Components");
    for c in components {
        let memory = c
            .idle_memory_mib
            .map_or_else(|| "unmeasured".to_owned(), |m| format!("{m} MiB"));
        push(s, &format!("  {:<48} {:<10} {memory}", c.id, c.why));
        for release in &c.releases {
            let what = match release {
                ReleasePlan::HelmChart { name, object, .. } => format!(
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
            push(s, &format!("      {what}"));
        }
    }
    for (id, reason) in left_out {
        push(s, &format!("  {id:<48} left out: {reason}"));
    }
}

/// The plan of a personal install as text.
pub(crate) fn personal_plan(plan: &PersonalPlan) -> String {
    let mut s = String::new();
    push(
        &mut s,
        &format!(
            "KubeSolo {} (Kubernetes {}, pinned), for one person, on {} ({}, {} MiB)",
            plan.kubesolo.version,
            plan.kubesolo.kubernetes,
            plan.facts.hostname,
            plan.facts.arch,
            plan.facts.memory_mib
        ),
    );
    push(
        &mut s,
        &format!(
            "helm-controller  {} (its image pinned by digest; the Jobs' image checked by digest after they run)",
            plan.helm_controller.version
        ),
    );
    let address = plan.facts.node_ips.first().map_or_else(
        || "chosen by KubeSolo: this host's addresses could not be read".to_owned(),
        |ip| format!("{ip}; the kubeconfig names it"),
    );
    push(&mut s, &format!("Node address     {address}"));
    push(&mut s, "");
    components(&mut s, &plan.components, &plan.left_out);
    push(&mut s, "");
    push(
        &mut s,
        &format!(
            "Storage          {} (KubeSolo's local-path), the default",
            plan.storage.default_class
        ),
    );
    push(
        &mut s,
        "Zeabur           none on the server: run `zeaburctl ui` on your computer",
    );
    push(&mut s, "");
    let m = &plan.memory;
    let unmeasured = if m.unmeasured > 0 {
        format!(", plus {} unmeasured", m.unmeasured)
    } else {
        String::new()
    };
    push(
        &mut s,
        &format!(
            "Memory at idle   about {} MiB of {} (KubeSolo {}, components {}{unmeasured})",
            m.cluster_mib + m.components_mib,
            m.host_mib,
            m.cluster_mib,
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

    fn plan(yaml: &str, rtt_us: &[u64]) -> JoinPlan {
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

    #[test]
    fn a_worker_near_the_server() {
        let p = plan(
            include_str!("../../../docs/reference/installer-examples/join.yaml"),
            &[640, 610, 700, 900, 650],
        );
        let text = join_plan(&p).replace(&p.rke2.version, "<rke2>");
        assert_eq!(text, include_str!("../fixtures/join-worker.txt"));
    }

    #[test]
    fn a_server_far_from_the_server() {
        let p = plan(
            "join:\n  server: https://10.0.0.5:9345\n  role: server\n  tokenFile: /root/token\n  \
             serverConfig: {disable: [rke2-metrics-server], identity: true}\n",
            &[24_000, 23_500, 25_100, 24_200, 23_900],
        );
        let text = join_plan(&p).replace(&p.rke2.version, "<rke2>");
        assert_eq!(text, include_str!("../fixtures/join-server.txt"));
    }

    #[test]
    fn an_unreachable_server_is_an_error() {
        let p = plan(
            include_str!("../../../docs/reference/installer-examples/join.yaml"),
            &[],
        );
        let text = join_plan(&p);
        assert!(text.contains("Distance         the server did not answer"));
        assert!(text.contains("error: Cannot connect to https://10.0.0.5:9345."));
    }
}
