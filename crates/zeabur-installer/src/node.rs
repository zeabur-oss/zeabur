//! `node add` and `node sync`, run on a server of the cluster.
//!
//! `node add` prints what the new VM needs: the server's address and a join token, as a
//! configuration to paste. A worker gets a bootstrap token that expires; an RKE2 server gets the
//! cluster's server token, which a bootstrap token cannot replace. K3s takes workers only.

use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use zeabur_installer_host::facts;
use zeabur_installer_host::kubectl::Kubectl;
use zeabur_installer_host::token::{self, BootstrapToken};
use zeabur_installer_plan::cluster::Distribution;
use zeabur_installer_plan::config::{Document, Role};
use zeabur_installer_plan::plan::{self, MANAGER, RegistryPlan};
use zeabur_installer_plan::{PlanError, join};

use crate::apply::STATE_CONFIGMAP;
use crate::{InstallError, load, out};

/// A CRD Longhorn installs; the cluster runs Longhorn when it exists.
const LONGHORN_CRD: &str = "volumes.longhorn.io";

/// The name of every baseline policy the installer writes.
const BASELINE: &str = "zeabur-installer-baseline";

/// What `node add` needs from the command line.
#[derive(Debug, Clone)]
pub(crate) struct AddOptions {
    pub(crate) role: Role,
    pub(crate) address: Option<String>,
    /// How long a worker's token joins nodes.
    pub(crate) ttl: Duration,
}

/// The distribution of this server: the one whose admin kubeconfig is here.
pub(crate) fn server_distribution() -> Result<Distribution, InstallError> {
    Distribution::ALL
        .into_iter()
        .find(|d| Path::new(d.layout().admin_kubeconfig).exists())
        .ok_or_else(|| {
            InstallError::Message(
                "no K3s or RKE2 admin kubeconfig here: run this on a server of the cluster"
                    .to_owned(),
            )
        })
}

pub(crate) fn add(config_path: &Path, options: &AddOptions) -> Result<(), InstallError> {
    let distribution = server_distribution()?;
    if options.role == Role::Server && !distribution.takes_servers() {
        return Err(PlanError::SecondServer.into());
    }
    let layout = distribution.layout();
    let kubectl = Kubectl::admin(distribution);
    let name = facts::read()?.own_node_name();
    let node = kubectl.get("node", &name, None)?.ok_or_else(|| {
        InstallError::Message(format!("this server's node {name} is not in the cluster"))
    })?;
    let address = match &options.address {
        Some(a) => a.clone(),
        None => internal_ip(&node).ok_or_else(|| {
            InstallError::Message(format!(
                "node {name} has no InternalIP; name the address with --address"
            ))
        })?,
    };
    let host = if address.contains(':') {
        format!("[{address}]")
    } else {
        address
    };
    let server = format!("https://{host}:{}", distribution.join_port());
    let server_token = fs::read_to_string(layout.server_token_file).ok();

    let (token, expiry) = match options.role {
        Role::Worker => {
            let hash = ca_hash(&kubectl, server_token.as_deref(), layout.server_token_file)?;
            let t = BootstrapToken::generate()?;
            let expires = token::expires(options.ttl);
            let secret = t.secret_object(distribution, "zeabur-installer node add", &expires);
            kubectl.run(&["create", "-f", "-"], Some(secret.to_string().as_bytes()))?;
            (t.joining(&hash), Some((expires, t.id)))
        }
        Role::Server => {
            let token = server_token.ok_or_else(|| {
                InstallError::Message(format!(
                    "{} cannot be read; run as root",
                    layout.server_token_file
                ))
            })?;
            (token.trim().to_owned(), None)
        }
    };

    let single_node = match load::document(config_path) {
        Ok(Document::Install(config)) if config.profile == "single-node" => {
            Some(config_path.display().to_string())
        }
        Ok(_) | Err(_) => None,
    };
    let longhorn = kubectl
        .get("customresourcedefinition", LONGHORN_CRD, None)?
        .is_some();
    let registry = registry_mirror(&kubectl, layout.registry_ca_file)?;
    let node_names: Vec<String> = kubectl
        .run(
            &[
                "get",
                "nodes",
                "--output=jsonpath={.items[*].metadata.name}",
            ],
            None,
        )?
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let text = instructions(&Instructions {
        distribution,
        server: &server,
        role: options.role,
        token: &token,
        expiry: expiry.as_ref().map(|(e, id)| (e.as_str(), id.as_str())),
        single_node,
        longhorn,
        registry: registry.as_ref(),
        node_names: &node_names,
    });
    out::line(text.trim_end());
    Ok(())
}

/// What `node add` prints.
#[derive(Debug)]
pub(crate) struct Instructions<'a> {
    pub(crate) distribution: Distribution,
    pub(crate) server: &'a str,
    pub(crate) role: Role,
    pub(crate) token: &'a str,
    /// A bootstrap token's expiry and id.
    pub(crate) expiry: Option<(&'a str, &'a str)>,
    /// The configuration file, when it still says `profile: single-node`.
    pub(crate) single_node: Option<String>,
    /// The cluster runs Longhorn, so the new node needs preparing for it.
    pub(crate) longhorn: bool,
    /// The in-cluster registry's mirror, when the cluster runs one.
    pub(crate) registry: Option<&'a Mirror>,
    /// The cluster's nodes, whose names the new node does not take.
    pub(crate) node_names: &'a [String],
}

/// The in-cluster registry as a joining node reaches it.
#[derive(Debug)]
pub(crate) struct Mirror {
    /// `<service>.<namespace>.svc:<port>`.
    pub(crate) host: String,
    /// Its Service's fixed `ClusterIP`.
    pub(crate) cluster_ip: String,
    /// The CA of its certificate, PEM.
    pub(crate) ca: String,
}

/// The text `node add` prints.
pub(crate) fn instructions(i: &Instructions<'_>) -> String {
    let mut text = String::new();
    let role = i.role.as_str();
    push(
        &mut text,
        &format!("Add a {role} to this cluster through {}.", i.server),
    );
    push(&mut text, "");
    push(&mut text, "Before you start");
    for advice in join::advice(i.distribution, i.role, i.longhorn) {
        push(&mut text, &format!("  - {}", advice.text));
    }
    if let Some(path) = &i.single_node {
        push(
            &mut text,
            &format!(
                "  - {path} says profile: single-node. Set profile: multi-node and run apply here \
                 to use the catalog's values for several nodes."
            ),
        );
    }
    push(&mut text, "");
    push(
        &mut text,
        "On the new VM, as root, with the same zeabur-installer copied to it:",
    );
    push(&mut text, "");
    text.push_str(&commands(
        i.distribution,
        i.server,
        i.role,
        i.token,
        i.longhorn,
        i.registry,
        i.node_names,
    ));
    push(&mut text, "");
    if let Some((expires, _)) = i.expiry {
        push(
            &mut text,
            &format!(
                "The token joins workers until {expires}. Revoke it as soon as the node is \
                 Ready; the node keeps working without it."
            ),
        );
    } else {
        push(
            &mut text,
            "This is the cluster's server token: it does not expire and joins servers and \
             workers. Keep it out of chats and tickets.",
        );
    }
    if i.role == Role::Worker {
        push(&mut text, "");
        push(
            &mut text,
            "After the worker joins, run zeabur-installer node sync on a server. Until then, the \
             network policies refuse the new node's traffic.",
        );
    }
    push(&mut text, "");
    let layout = i.distribution.layout();
    let kubectl = format!(
        "{} --kubeconfig {}",
        layout.kubectl, layout.admin_kubeconfig
    );
    push(&mut text, "When the node is Ready, here:");
    push(&mut text, &format!("  {kubectl} get nodes -o wide"));
    if i.role == Role::Worker {
        push(
            &mut text,
            "  zeabur-installer node sync   # lets the new node's traffic through the network \
             policies",
        );
    }
    if let Some((_, id)) = i.expiry {
        push(
            &mut text,
            &format!(
                "  {kubectl} --namespace kube-system delete secret bootstrap-token-{id}   \
                 # revokes the token"
            ),
        );
    }
    text
}

fn push(text: &mut String, line: &str) {
    text.push_str(line);
    text.push('\n');
}

/// The shell commands that write the join configuration and run the installer.
pub(crate) fn commands(
    distribution: Distribution,
    server: &str,
    role: Role,
    token: &str,
    longhorn: bool,
    registry: Option<&Mirror>,
    node_names: &[String],
) -> String {
    let mut yaml = format!(
        "distribution: {}\njoin:\n  server: {server}\n  role: {}\n  token: {token}\n",
        distribution.id(),
        role.as_str()
    );
    if !node_names.is_empty() {
        for part in ["  nodeNames: [", &node_names.join(", "), "]\n"] {
            yaml.push_str(part);
        }
    }
    if longhorn {
        yaml.push_str("  longhorn: true\n");
    }
    if let Some(Mirror {
        host,
        cluster_ip,
        ca,
    }) = registry
    {
        for part in [
            "  registry:\n    host: ",
            host,
            "\n    clusterIp: ",
            cluster_ip,
            "\n    ca: |\n",
        ] {
            yaml.push_str(part);
        }
        for line in ca.lines() {
            for part in ["      ", line, "\n"] {
                yaml.push_str(part);
            }
        }
    }
    if role == Role::Server {
        let (disable, identity) = server_config(distribution);
        let disable = disable.join(", ");
        let identity = identity.to_string();
        for part in [
            "  serverConfig:
    disable: [",
            &disable,
            "]
    identity: ",
            &identity,
            "
",
        ] {
            yaml.push_str(part);
        }
    }
    format!(
        "install -d -m 0700 /etc/zeabur-installer\n\
         (umask 077; cat > /etc/zeabur-installer/config.yaml) <<'EOF'\n\
         {yaml}EOF\n\
         zeabur-installer plan    # what it installs, the distance to this server and warnings\n\
         zeabur-installer apply   # installs {} and waits until the node is Ready\n",
        distribution.title()
    )
}

/// What every server must share, read from this server's configuration.
fn server_config(distribution: Distribution) -> (Vec<String>, bool) {
    let dir = distribution.layout().config_dir;
    let disable = fs::read_to_string(format!("{dir}/config.yaml"))
        .ok()
        .and_then(|text| serde_saphyr::from_str::<Value>(&text).ok())
        .and_then(|v| v.get("disable").cloned())
        .and_then(|v| v.as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let identity = Path::new(&format!("{dir}/config.yaml.d/50-zeabur-identity.yaml")).exists();
    (disable, identity)
}

/// The registry `apply` recorded and the CA it copied to this server, when the cluster runs one
/// with a fixed `ClusterIP`.
fn registry_mirror(kubectl: &Kubectl, ca_file: &str) -> Result<Option<Mirror>, InstallError> {
    let Ok(ca) = fs::read_to_string(ca_file) else {
        return Ok(None);
    };
    let registry = kubectl
        .get("configmap", STATE_CONFIGMAP, Some("kube-system"))?
        .and_then(|state| {
            let plan: Value = serde_json::from_str(state.pointer("/data/plan")?.as_str()?).ok()?;
            serde_json::from_value::<RegistryPlan>(plan.get("registry")?.clone()).ok()
        });
    Ok(registry.and_then(|r| {
        Some(Mirror {
            host: r.host,
            cluster_ip: r.cluster_ip?,
            ca,
        })
    }))
}

fn internal_ip(node: &Value) -> Option<String> {
    node.pointer("/status/addresses")?
        .as_array()?
        .iter()
        .find(|a| a.get("type").and_then(Value::as_str) == Some("InternalIP"))?
        .get("address")?
        .as_str()
        .map(str::to_owned)
}

/// The cluster CA's hash, from `kube-root-ca.crt` as Zeabur reads it, checked against the hash in
/// this server's token when both are known.
fn ca_hash(
    kubectl: &Kubectl,
    server_token: Option<&str>,
    token_file: &str,
) -> Result<String, InstallError> {
    let from_token = server_token.and_then(token::hash_of_token);
    let pem = kubectl
        .get("configmap", "kube-root-ca.crt", Some("kube-system"))?
        .and_then(|c| {
            c.pointer("/data/ca.crt")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    let from_configmap = pem.as_deref().and_then(|p| token::ca_hash(p.as_bytes()));
    match (from_configmap, from_token) {
        (Some(a), Some(b)) if a != b => Err(InstallError::Message(format!(
            "the CA hash of kube-root-ca.crt ({a}) differs from the one in {token_file} ({b}); \
             please report this"
        ))),
        (Some(hash), _) => Ok(hash),
        (None, Some(hash)) => Ok(hash.to_owned()),
        (None, None) => Err(InstallError::Message(
            "could not work out the cluster CA's hash; run node add as root on a server".to_owned(),
        )),
    }
}

pub(crate) fn sync(_config_path: &Path) -> Result<(), InstallError> {
    report_sync(&Kubectl::admin(server_distribution()?))
}

/// Runs [`sync_policies`] under its own step and says how many policies changed.
pub(crate) fn report_sync(kubectl: &Kubectl) -> Result<(), InstallError> {
    out::step("Network policies");
    let changed = sync_policies(kubectl)?;
    out::line(&format!(
        "{changed} network {} updated for the current nodes",
        if changed == 1 { "policy" } else { "policies" }
    ));
    Ok(())
}

/// Rewrites every baseline policy the installer made with the addresses of the current nodes.
/// Returns how many changed.
fn sync_policies(kubectl: &Kubectl) -> Result<usize, InstallError> {
    let sources = crate::apply::node_addresses(kubectl)?;
    let mut changed = 0;
    for policy in baseline_policies(kubectl)? {
        let have = plan::policy_sources(&policy);
        if have.iter().eq(sources.iter()) {
            continue;
        }
        if let Some(namespace) = policy
            .pointer("/metadata/namespace")
            .and_then(Value::as_str)
        {
            kubectl.apply_object(&plan::network_policy(namespace, &sources))?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// The installer's baseline policies.
pub(crate) fn baseline_policies(kubectl: &Kubectl) -> Result<Vec<Value>, InstallError> {
    let selector = format!("--selector=app.kubernetes.io/managed-by={MANAGER}");
    let list = kubectl.run(
        &[
            "get",
            "networkpolicies",
            "--all-namespaces",
            &selector,
            "--output=json",
        ],
        None,
    )?;
    let list: Value = serde_json::from_str(&list)
        .map_err(|e| InstallError::Message(format!("reading network policies: {e}")))?;
    Ok(list
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|p| p.pointer("/metadata/name").and_then(Value::as_str) == Some(BASELINE))
        .cloned()
        .collect())
}

/// Nodes whose traffic the baseline policies do not let in yet.
pub(crate) fn policies_behind(kubectl: &Kubectl) -> Result<Vec<String>, InstallError> {
    let sources = crate::apply::node_addresses(kubectl)?;
    let mut missing = Vec::new();
    for policy in baseline_policies(kubectl)? {
        let have = plan::policy_sources(&policy);
        for s in &sources {
            if !have.contains(s) && !missing.contains(s) {
                missing.push(s.clone());
            }
        }
    }
    Ok(missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_node_add_prints() {
        let text = instructions(&Instructions {
            distribution: Distribution::K3s,
            server: "https://10.0.1.10:6443",
            role: Role::Worker,
            token: "K10aa::abcdef.0123456789abcdef",
            expiry: Some(("2026-10-23T09:00:00Z", "abcdef")),
            single_node: Some("/etc/zeabur-installer/config.yaml".to_owned()),
            longhorn: false,
            registry: None,
            node_names: &["vps-1".to_owned()],
        });
        assert_eq!(text, include_str!("../fixtures/node-add.txt"));
    }

    #[test]
    fn a_server_join_needs_no_node_sync() {
        let text = instructions(&Instructions {
            distribution: Distribution::Rke2,
            server: "https://10.0.1.10:9345",
            role: Role::Server,
            token: "K10aa::server:0123456789abcdef",
            expiry: None,
            single_node: None,
            longhorn: false,
            registry: None,
            node_names: &["vps-1".to_owned()],
        });
        assert!(!text.contains("node sync"));
    }

    #[test]
    fn the_commands_to_paste() {
        let text = commands(
            Distribution::Rke2,
            "https://10.0.0.5:9345",
            Role::Worker,
            "K10aa::abcdef.0123456789abcdef",
            false,
            None,
            &["vps-1".to_owned(), "vps-2".to_owned()],
        );
        assert_eq!(text, include_str!("../fixtures/node-add-worker.txt"));
        let yaml = text
            .split_once("<<'EOF'\n")
            .and_then(|(_, rest)| rest.split_once("EOF\n"))
            .map(|(yaml, _)| yaml)
            .unwrap();
        let hash = "a".repeat(64);
        let parsed = Document::parse(&yaml.replace("K10aa", &format!("K10{hash}"))).unwrap();
        let Document::Join(parsed) = parsed else {
            panic!("not a join: {yaml}");
        };
        assert_eq!(parsed.join.node_names, ["vps-1", "vps-2"]);
    }

    #[test]
    fn a_cluster_with_a_registry_gives_the_new_node_its_mirror() {
        let ca = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n";
        let text = commands(
            Distribution::K3s,
            "https://10.0.0.5:6443",
            Role::Worker,
            "K10aa::abcdef.0123456789abcdef",
            false,
            Some(&Mirror {
                host: "zot.zot.svc:5000".to_owned(),
                cluster_ip: "10.43.0.50".to_owned(),
                ca: ca.to_owned(),
            }),
            &[],
        );
        let yaml = text
            .split_once("<<'EOF'\n")
            .and_then(|(_, rest)| rest.split_once("EOF\n"))
            .map(|(yaml, _)| yaml)
            .unwrap();
        let hash = "a".repeat(64);
        let Ok(Document::Join(config)) =
            Document::parse(&yaml.replace("K10aa", &format!("K10{hash}")))
        else {
            panic!("not a join: {yaml}");
        };
        let registry = config.join.registry.unwrap();
        assert_eq!(registry.host, "zot.zot.svc:5000");
        assert_eq!(registry.cluster_ip.to_string(), "10.43.0.50");
        assert_eq!(registry.ca, ca);
    }

    #[test]
    fn a_cluster_with_longhorn_prepares_the_new_node() {
        let text = instructions(&Instructions {
            distribution: Distribution::K3s,
            server: "https://10.0.1.10:6443",
            role: Role::Worker,
            token: "K10aa::abcdef.0123456789abcdef",
            expiry: Some(("2026-10-23T09:00:00Z", "abcdef")),
            single_node: None,
            longhorn: true,
            registry: None,
            node_names: &[],
        });
        assert!(text.contains("The cluster runs Longhorn"), "{text}");
        assert!(!text.contains("default local-path storage class"), "{text}");
        let yaml = text
            .split_once("<<'EOF'\n")
            .and_then(|(_, rest)| rest.split_once("EOF\n"))
            .map(|(yaml, _)| yaml)
            .unwrap();
        let hash = "a".repeat(64);
        let Ok(Document::Join(config)) =
            Document::parse(&yaml.replace("K10aa", &format!("K10{hash}")))
        else {
            panic!("not a join: {yaml}");
        };
        assert!(config.join.longhorn);
    }
}
