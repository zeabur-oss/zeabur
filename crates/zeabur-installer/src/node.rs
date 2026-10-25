//! `node add` and `node sync`, run on a server of the cluster.
//!
//! `node add` prints what the new VM needs: the server's address and a join token, as a
//! configuration to paste. A worker gets a bootstrap token that expires; a server gets the
//! cluster's server token, which a bootstrap token cannot replace.

use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use zeabur_installer_host::facts;
use zeabur_installer_host::kubectl::Kubectl;
use zeabur_installer_host::token::{self, BootstrapToken};
use zeabur_installer_plan::config::{Document, Role};
use zeabur_installer_plan::join;
use zeabur_installer_plan::plan::{self, MANAGER};
use zeabur_installer_plan::rke2::{
    ADMIN_KUBECONFIG, BIN_DIR, CONFIG_DIR, SERVER_TOKEN_FILE, ZOT_CA_FILE,
};

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

pub(crate) fn add(config_path: &Path, options: &AddOptions) -> Result<(), InstallError> {
    if !Path::new(ADMIN_KUBECONFIG).exists() {
        return Err(InstallError::Message(format!(
            "{ADMIN_KUBECONFIG} does not exist: run node add on a server of the cluster"
        )));
    }
    let kubectl = Kubectl::rke2();
    let name = facts::read()?.hostname.to_ascii_lowercase();
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
    let server = format!("https://{host}:{}", join::SUPERVISOR_PORT);
    let server_token = fs::read_to_string(SERVER_TOKEN_FILE).ok();

    let (token, expiry) = match options.role {
        Role::Worker => {
            let hash = ca_hash(&kubectl, server_token.as_deref())?;
            let t = BootstrapToken::generate()?;
            let expires = token::expires(options.ttl);
            let secret = t.secret_object("zeabur-installer node add", &expires);
            kubectl.run(&["create", "-f", "-"], Some(secret.to_string().as_bytes()))?;
            (t.joining(&hash), Some((expires, t.id)))
        }
        Role::Server => {
            let token = server_token.ok_or_else(|| {
                InstallError::Message(format!("{SERVER_TOKEN_FILE} cannot be read; run as root"))
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
    let zot = zot_mirror(&kubectl)?;
    let text = instructions(&Instructions {
        server: &server,
        role: options.role,
        token: &token,
        expiry: expiry.as_ref().map(|(e, id)| (e.as_str(), id.as_str())),
        single_node,
        longhorn,
        zot: zot.as_ref().map(|(ip, ca)| (ip.as_str(), ca.as_str())),
    });
    out::line(text.trim_end());
    Ok(())
}

/// What `node add` prints.
#[derive(Debug)]
pub(crate) struct Instructions<'a> {
    pub(crate) server: &'a str,
    pub(crate) role: Role,
    pub(crate) token: &'a str,
    /// A bootstrap token's expiry and id.
    pub(crate) expiry: Option<(&'a str, &'a str)>,
    /// The configuration file, when it still says `profile: single-node`.
    pub(crate) single_node: Option<String>,
    /// The cluster runs Longhorn, so the new node needs preparing for it.
    pub(crate) longhorn: bool,
    /// zot's `ClusterIP` and CA, when the cluster runs it.
    pub(crate) zot: Option<(&'a str, &'a str)>,
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
    for advice in join::advice(i.role, i.longhorn) {
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
    text.push_str(&commands(i.server, i.role, i.token, i.longhorn, i.zot));
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
    push(&mut text, "When the node is Ready, here:");
    push(
        &mut text,
        &format!("  {BIN_DIR}/kubectl --kubeconfig {ADMIN_KUBECONFIG} get nodes -o wide"),
    );
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
                "  {BIN_DIR}/kubectl --kubeconfig {ADMIN_KUBECONFIG} --namespace kube-system delete \
                 secret bootstrap-token-{id}   # revokes the token"
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
    server: &str,
    role: Role,
    token: &str,
    longhorn: bool,
    zot: Option<(&str, &str)>,
) -> String {
    let mut yaml = format!(
        "join:\n  server: {server}\n  role: {}\n  token: {token}\n",
        role.as_str()
    );
    if longhorn {
        yaml.push_str("  longhorn: true\n");
    }
    if let Some((cluster_ip, ca)) = zot {
        for part in ["  zot:\n    clusterIp: ", cluster_ip, "\n    ca: |\n"] {
            yaml.push_str(part);
        }
        for line in ca.lines() {
            for part in ["      ", line, "\n"] {
                yaml.push_str(part);
            }
        }
    }
    if role == Role::Server {
        let (disable, identity) = server_config();
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
         zeabur-installer apply   # installs RKE2 and waits until the node is Ready\n"
    )
}

/// What every server must share, read from this server's RKE2 configuration.
fn server_config() -> (Vec<String>, bool) {
    let disable = fs::read_to_string(format!("{CONFIG_DIR}/config.yaml"))
        .ok()
        .and_then(|text| serde_saphyr::from_str::<Value>(&text).ok())
        .and_then(|v| v.get("disable").cloned())
        .and_then(|v| v.as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let identity = Path::new(&format!(
        "{CONFIG_DIR}/config.yaml.d/50-zeabur-identity.yaml"
    ))
    .exists();
    (disable, identity)
}

/// zot's `ClusterIP` and the CA `apply` copied to this server, when the cluster runs it.
fn zot_mirror(kubectl: &Kubectl) -> Result<Option<(String, String)>, InstallError> {
    let Ok(ca) = fs::read_to_string(ZOT_CA_FILE) else {
        return Ok(None);
    };
    Ok(kubectl
        .get("service", "zot", Some("zot"))?
        .and_then(|s| s.pointer("/spec/clusterIP")?.as_str().map(str::to_owned))
        .filter(|ip| ip != "None")
        .map(|ip| (ip, ca)))
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
fn ca_hash(kubectl: &Kubectl, server_token: Option<&str>) -> Result<String, InstallError> {
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
            "the CA hash of kube-root-ca.crt ({a}) differs from the one in {SERVER_TOKEN_FILE} \
             ({b}); please report this"
        ))),
        (Some(hash), _) => Ok(hash),
        (None, Some(hash)) => Ok(hash.to_owned()),
        (None, None) => Err(InstallError::Message(
            "could not work out the cluster CA's hash; run node add as root on a server".to_owned(),
        )),
    }
}

pub(crate) fn sync(_config_path: &Path) -> Result<(), InstallError> {
    report_sync(&Kubectl::rke2())
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
            server: "https://10.0.1.10:9345",
            role: Role::Worker,
            token: "K10aa::abcdef.0123456789abcdef",
            expiry: Some(("2026-10-23T09:00:00Z", "abcdef")),
            single_node: Some("/etc/zeabur-installer/config.yaml".to_owned()),
            longhorn: false,
            zot: None,
        });
        assert_eq!(text, include_str!("../fixtures/node-add.txt"));
    }

    #[test]
    fn a_server_join_needs_no_node_sync() {
        let text = instructions(&Instructions {
            server: "https://10.0.1.10:9345",
            role: Role::Server,
            token: "K10aa::server:0123456789abcdef",
            expiry: None,
            single_node: None,
            longhorn: false,
            zot: None,
        });
        assert!(!text.contains("node sync"));
    }

    #[test]
    fn the_commands_to_paste() {
        let text = commands(
            "https://10.0.0.5:9345",
            Role::Worker,
            "K10aa::abcdef.0123456789abcdef",
            false,
            None,
        );
        assert_eq!(text, include_str!("../fixtures/node-add-worker.txt"));
        let yaml = text
            .split_once("<<'EOF'\n")
            .and_then(|(_, rest)| rest.split_once("EOF\n"))
            .map(|(yaml, _)| yaml)
            .unwrap();
        let hash = "a".repeat(64);
        let parsed = Document::parse(&yaml.replace("K10aa", &format!("K10{hash}"))).unwrap();
        assert!(matches!(parsed, Document::Join(_)));
    }

    #[test]
    fn a_cluster_with_zot_gives_the_new_node_its_mirror() {
        let ca = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n";
        let text = commands(
            "https://10.0.0.5:9345",
            Role::Worker,
            "K10aa::abcdef.0123456789abcdef",
            false,
            Some(("10.43.0.50", ca)),
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
        let zot = config.join.zot.unwrap();
        assert_eq!(zot.cluster_ip.to_string(), "10.43.0.50");
        assert_eq!(zot.ca, ca);
    }

    #[test]
    fn a_cluster_with_longhorn_prepares_the_new_node() {
        let text = instructions(&Instructions {
            server: "https://10.0.1.10:9345",
            role: Role::Worker,
            token: "K10aa::abcdef.0123456789abcdef",
            expiry: Some(("2026-10-23T09:00:00Z", "abcdef")),
            single_node: None,
            longhorn: true,
            zot: None,
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
