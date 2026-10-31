//! The plan, written for people.

use zeabur_installer_plan::config::{AccessMode, Role};
use zeabur_installer_plan::join::{self, JoinPlan};
use zeabur_installer_plan::kubesolo;
use zeabur_installer_plan::personal::PersonalPlan;
use zeabur_installer_plan::plan::{ComponentPlan, Plan, ReleasePlan};
use zeabur_installer_plan::record::Changes;
use zeabur_installer_plan::remove::Removal;
use zeabur_installer_plan::storage::NodePrep;

/// The plan as text.
pub(crate) fn plan(plan: &Plan) -> String {
    let mut s = String::new();
    let cluster = &plan.cluster;
    push(
        &mut s,
        &format!(
            "{} {} ({}), CIS hardened, on {} ({}, {} MiB)",
            cluster.distribution.title(),
            cluster.version,
            support(cluster.pinned),
            plan.facts.hostname,
            plan.facts.arch,
            plan.facts.memory_mib
        ),
    );
    node_name(&mut s, &plan.node_name);
    node_addresses(&mut s, &plan.facts.node_ips);
    push(&mut s, "");
    s.push_str(&changes(
        plan.changes.as_ref(),
        cluster.distribution.server_unit(),
    ));
    components(&mut s, &plan.components, &plan.left_out);
    push(&mut s, "");
    s.push_str(&removals(&plan.removals));
    let exempt: Vec<String> = plan
        .namespaces
        .iter()
        .filter_map(|n| match (&n.privileged, &n.baseline) {
            (Some(why), _) => Some(format!("  {} ({why})", n.name)),
            (None, Some(why)) => Some(format!("  {} (baseline: {why})", n.name)),
            (None, None) => None,
        })
        .collect();
    if !exempt.is_empty() {
        push(
            &mut s,
            "Namespaces exempt from the restricted Pod Security level",
        );
        for line in &exempt {
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
            "Memory at idle   about {} MiB of {} ({} {}, components {}{unmeasured})",
            m.cluster_mib + m.components_mib,
            m.host_mib,
            cluster.distribution.title(),
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

/// What `apply` changes against the install record that `unit`'s cluster holds; empty on a first
/// install.
pub(crate) fn changes(changes: Option<&Changes>, unit: &str) -> String {
    let mut s = String::new();
    let Some(c) = changes else {
        return s;
    };
    push(
        &mut s,
        &format!(
            "Changes since the last apply (by zeabur-installer {})",
            c.installer
        ),
    );
    let before = s.len();
    if let Some(v) = &c.version {
        push(&mut s, &format!("  {unit:<48} {} -> {}", v.from, v.to));
    }
    for path in &c.host_files {
        push(&mut s, &format!("  {path:<48} changes"));
    }
    if c.restart {
        push(
            &mut s,
            &format!(
                "  restarts {unit}. The Kubernetes API stops answering for about 10 seconds. \
                 Running apps keep serving."
            ),
        );
    }
    for id in &c.added {
        push(&mut s, &format!("  {id:<48} added"));
    }
    for id in &c.removed {
        push(&mut s, &format!("  {id:<48} removed"));
    }
    for id in &c.kept {
        push(
            &mut s,
            &format!("  {id:<48} not removed, no longer updated"),
        );
    }
    for r in &c.replaced {
        push(&mut s, &format!("  {:<48} replaced by {}", r.from, r.to));
        push(&mut s, &format!("      {}", replaced_note(&r.from)));
    }
    for chart in &c.charts {
        let version = if chart.from == chart.to {
            chart.to.clone()
        } else {
            format!("{} -> {}", chart.from, chart.to)
        };
        let values = if chart.values { ", values changed" } else { "" };
        push(
            &mut s,
            &format!(
                "  {:<48} chart {} {version}{values}",
                chart.component, chart.release
            ),
        );
    }
    for id in &c.offered {
        push(
            &mut s,
            &format!("  {id:<48} new default, not installed. To add it: enable: [{id}]"),
        );
    }
    if s.len() == before {
        push(&mut s, "  nothing");
    }
    push(&mut s, "");
    s
}

/// What happens to the data of a component that its replacement takes over.
fn replaced_note(from: &str) -> &'static str {
    match from {
        "container-registry/zot" => {
            "Zeabur Builder takes over zot and deletes images under <project>/<name> paths that \
             no build made. To keep every image: disable: [builds/zeabur-builder]"
        }
        _ => "the new component takes over what the old one installed",
    }
}

/// The components `apply` removes and what stays of them; empty when there are none.
pub(crate) fn removals(removals: &[Removal]) -> String {
    let mut s = String::new();
    if removals.is_empty() {
        return s;
    }
    push(&mut s, "Removed (helm-controller uninstalls the charts)");
    for removal in removals {
        push(
            &mut s,
            &format!(
                "  {:<48} HelmChart {}",
                removal.id,
                removal.helm_charts.join(", ")
            ),
        );
        let mut stays: Vec<String> = removal
            .namespaces
            .iter()
            .map(|n| format!("namespace {n}"))
            .collect();
        stays.extend(removal.leftovers.iter().cloned());
        stays.extend(removal.manifests.iter().map(|m| format!("manifests {m}")));
        if !stays.is_empty() {
            push(&mut s, &format!("      stays: {}", stays.join(", ")));
        }
        if !removal.deleted_crds.is_empty() {
            push(
                &mut s,
                &format!(
                    "      deletes these CRDs and all their objects: {}",
                    removal.deleted_crds.join(", ")
                ),
            );
        }
        if let Some(why) = removal.refusal() {
            push(&mut s, &format!("      refused: {why}"));
        } else if removal.loses_data {
            push(
                &mut s,
                "      deletes the data it keeps, as allowDataLoss allows",
            );
        }
    }
    push(
        &mut s,
        "  What stays is deleted by hand (https://oss.zeabur.com/docs/zeabur/reference/installer/#remove-components).",
    );
    push(&mut s, "");
    s
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
    node_name(&mut s, &plan.node_name);
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
    s.push_str(&changes(plan.changes.as_ref(), kubesolo::UNIT));
    components(&mut s, &plan.components, &plan.left_out);
    push(&mut s, "");
    s.push_str(&removals(&plan.removals));
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
    let cluster = &plan.cluster;
    let title = cluster.distribution.title();
    let (what, runs) = match plan.role {
        Role::Worker => ("agent", "a worker"),
        Role::Server => ("server", "a server (control plane and etcd)"),
    };
    push(&mut s, &format!("Join {} as {runs}", plan.server));
    push(
        &mut s,
        &format!(
            "{title} {what} {} ({}), CIS hardened, on {} ({}, {} MiB)",
            cluster.version,
            support(cluster.pinned),
            plan.facts.hostname,
            plan.facts.arch,
            plan.facts.memory_mib
        ),
    );
    push(
        &mut s,
        &format!(
            "Installs {title} only: Zeabur, Zeabur Identity and the catalog's components are \
             already in the cluster."
        ),
    );
    node_name(&mut s, &plan.node_name);
    node_addresses(&mut s, &plan.facts.node_ips);
    push(&mut s, "");
    push(&mut s, "Files");
    for path in &plan.host_file_paths {
        push(&mut s, &format!("  {path}"));
    }
    push(
        &mut s,
        &format!(
            "  {} (the token, mode 0600)",
            cluster.distribution.layout().join_token_file
        ),
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
        "Ports on this VM, free before the node starts (apply checks them)",
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

const fn support(pinned: bool) -> &'static str {
    if pinned {
        "pinned, checked against this installer's digests"
    } else {
        "UNSUPPORTED"
    }
}

/// The addresses the node registers with, which the network policies let in.
fn node_name(s: &mut String, name: &str) {
    push(s, &format!("Node name        {name}"));
}

fn node_addresses(s: &mut String, addresses: &[String]) {
    let text = if addresses.is_empty() {
        "chosen by Kubernetes: this host's addresses could not be read".to_owned()
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
                node_name: None,
                host_hex: "a1b2c3".to_owned(),
                memory_mib: 8192,
                node_ips: vec!["10.0.0.6".to_owned(), "fd00::6".to_owned()],
                record: None,
                installer: None,
            },
            rtt_us: rtt_us.to_vec(),
            ports: vec![
                (9345, !rtt_us.is_empty()),
                (6443, !rtt_us.is_empty()),
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
        let text = join_plan(&p).replace(&p.cluster.version, "<version>");
        assert_eq!(text, include_str!("../fixtures/join-worker.txt"));
    }

    #[test]
    fn a_server_far_from_the_server() {
        let p = plan(
            "distribution: rke2\njoin:\n  server: https://10.0.0.5:9345\n  role: server\n  \
             tokenFile: /root/token\n  serverConfig: {disable: [rke2-metrics-server], identity: true}\n",
            &[24_000, 23_500, 25_100, 24_200, 23_900],
        );
        let text = join_plan(&p).replace(&p.cluster.version, "<version>");
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
        assert!(text.contains("error: Cannot connect to https://10.0.0.5:6443."));
    }

    #[test]
    fn an_upgrade_names_what_changes_and_how_to_add_a_new_default() {
        use zeabur_installer_plan::record::{ChartChange, VersionChange};
        let mut changes = Changes {
            installer: "0.1.1".to_owned(),
            added: vec!["databases/cloudnative-pg".to_owned()],
            removed: vec!["databases/old".to_owned()],
            kept: vec!["node-updates/system-upgrade-controller".to_owned()],
            replaced: vec![zeabur_installer_plan::record::Replaced {
                from: "container-registry/zot".to_owned(),
                to: "builds/zeabur-builder".to_owned(),
            }],
            charts: vec![ChartChange {
                component: "ingress/traefik".to_owned(),
                release: "traefik".to_owned(),
                from: "42.1.2".to_owned(),
                to: "42.1.3".to_owned(),
                values: false,
            }],
            offered: vec!["databases/other".to_owned()],
            version: None,
            host_files: Vec::new(),
            restart: false,
        };
        changes.set_host_files(vec!["/etc/rancher/k3s/registries.yaml".to_owned()]);
        let text = super::changes(Some(&changes), "k3s");
        assert!(text.starts_with("Changes since the last apply (by zeabur-installer 0.1.1)\n"));
        for line in [
            "/etc/rancher/k3s/registries.yaml",
            "restarts k3s",
            "databases/cloudnative-pg",
            "databases/old",
            "node-updates/system-upgrade-controller",
            "not removed, no longer updated",
            "container-registry/zot",
            "replaced by builds/zeabur-builder",
            "deletes images under <project>/<name>",
            "chart traefik 42.1.2 -> 42.1.3",
            "To add it: enable: [databases/other]",
        ] {
            assert!(text.contains(line), "{line} in\n{text}");
        }

        let same = Changes {
            added: Vec::new(),
            removed: Vec::new(),
            kept: Vec::new(),
            replaced: Vec::new(),
            charts: Vec::new(),
            offered: Vec::new(),
            version: Some(VersionChange {
                from: "v1.2.2".to_owned(),
                to: "v1.2.3".to_owned(),
            }),
            host_files: Vec::new(),
            restart: true,
            ..changes
        };
        let text = super::changes(Some(&same), "kubesolo");
        assert!(text.contains("v1.2.2 -> v1.2.3") && text.contains("restarts kubesolo"));
        assert_eq!(super::changes(None, "k3s"), "");
    }
}
