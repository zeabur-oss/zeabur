//! Storage: which class is the default, and what Longhorn needs on every node.
//!
//! RKE2 ships no storage class, so the installer always adds local-path. When the plan includes
//! Longhorn, the catalog's values for the profile say whether its class `longhorn` is the default
//! (multi-node) or local-path stays it (single-node). Longhorn attaches volumes over iSCSI from
//! the node itself, so every node needs an initiator, `iscsid` and the `iscsi_tcp` module before
//! Longhorn's pods start there; `ReadWriteMany` volumes and NFS backups also need an NFS client.

use serde::Serialize;
use serde_json::Value;

/// Longhorn's component id in the catalog.
pub const LONGHORN: &str = "replicated-storage/longhorn";
/// Longhorn's storage class.
pub const LONGHORN_CLASS: &str = "longhorn";
/// The class the installer adds from rancher's local-path-provisioner.
pub const LOCAL_PATH_CLASS: &str = "local-path";
/// The annotation that makes a storage class the default.
pub const DEFAULT_CLASS_ANNOTATION: &str = "storageclass.kubernetes.io/is-default-class";
/// Loads `iscsi_tcp` at boot, before `iscsid` starts.
pub const MODULES_FILE: &str = "/etc/modules-load.d/zeabur-longhorn.conf";
/// Keeps multipathd off Longhorn's disks.
pub const MULTIPATH_FILE: &str = "/etc/multipath/conf.d/zeabur-longhorn.conf";
/// Longhorn's disks are iSCSI LUNs from tgt, which reports this vendor and product.
///
/// Blacklisting them alone leaves every other disk to multipathd, unlike the
/// `devnode "^sd[a-z0-9]+"` of Longhorn's knowledge base.
pub const MULTIPATH_BLACKLIST: &str = concat!(
    "# Written by zeabur-installer: Longhorn's iSCSI disks are not multipath devices.\n",
    "blacklist {\n",
    "    device {\n",
    "        vendor \"IET\"\n",
    "        product \"VIRTUAL-DISK\"\n",
    "    }\n",
    "}\n",
);

/// The storage classes of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Storage {
    /// The class a claim without `storageClassName` gets.
    pub default_class: String,
    /// The other classes the installer provides.
    pub other_classes: Vec<String>,
}

/// A package a node needs, by distribution family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    /// What it is for.
    pub purpose: &'static str,
    /// A program it installs; its presence means the package is there.
    pub binary: &'static str,
    /// Debian and Ubuntu.
    pub debian: &'static str,
    /// RHEL, Rocky, Alma, Oracle Linux, Fedora.
    pub rhel: &'static str,
    /// SLES and openSUSE.
    pub suse: &'static str,
}

/// What a node needs before a component's pods run on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePrep {
    /// The component that needs it.
    pub component: &'static str,
    /// Packages to install.
    pub packages: &'static [Package],
    /// Kernel modules to load now and at boot.
    pub modules: &'static [&'static str],
    /// systemd units to enable and start.
    pub services: &'static [&'static str],
}

/// Longhorn's node requirements (<https://longhorn.io/docs/1.12.1/deploy/install/>).
pub const LONGHORN_PREP: NodePrep = NodePrep {
    component: LONGHORN,
    packages: &[
        Package {
            purpose: "iSCSI initiator: volumes attach over iSCSI",
            binary: "iscsiadm",
            debian: "open-iscsi",
            rhel: "iscsi-initiator-utils",
            suse: "open-iscsi",
        },
        Package {
            purpose: "NFSv4 client: ReadWriteMany volumes and NFS backups",
            binary: "mount.nfs",
            debian: "nfs-common",
            rhel: "nfs-utils",
            suse: "nfs-client",
        },
    ],
    modules: &["iscsi_tcp"],
    services: &["iscsid"],
};

/// The node preparation a selection of components needs.
#[must_use]
pub fn node_prep<'a>(mut components: impl Iterator<Item = &'a str>) -> Option<NodePrep> {
    components.any(|c| c == LONGHORN).then_some(LONGHORN_PREP)
}

/// Whether Longhorn's merged chart values make its class the default; the chart's own default is
/// `true`.
#[must_use]
pub fn longhorn_is_default(values: &Value) -> bool {
    values
        .pointer("/persistence/defaultClass")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// The names of the default storage classes in `kubectl get storageclass -o json`.
#[must_use]
pub fn default_classes(classes: &Value) -> Vec<String> {
    let pointer = format!(
        "/metadata/annotations/{}",
        DEFAULT_CLASS_ANNOTATION.replace('/', "~1")
    );
    classes
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|c| c.pointer(&pointer).and_then(Value::as_str) == Some("true"))
        .filter_map(|c| c.pointer("/metadata/name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

/// Longhorn's checks of each node that failed, from `kubectl get nodes.longhorn.io -o json`:
/// `<node>: <condition> (<message>)`.
#[must_use]
pub fn longhorn_node_problems(nodes: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    for node in nodes
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = node
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        for condition in node
            .pointer("/status/conditions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if condition.get("status").and_then(Value::as_str) != Some("False") {
                continue;
            }
            let kind = condition
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let message = condition
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            problems.push(if message.is_empty() {
                format!("{name}: {kind}")
            } else {
                format!("{name}: {kind} ({message})")
            });
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn longhorn_needs_preparing_only_when_planned() {
        assert_eq!(node_prep(["ingress/traefik"].into_iter()), None);
        let prep = node_prep(["ingress/traefik", LONGHORN].into_iter()).unwrap();
        assert_eq!(prep.services, ["iscsid"]);
        assert_eq!(prep.modules, ["iscsi_tcp"]);
        assert_eq!(prep.packages[0].debian, "open-iscsi");
    }

    #[test]
    fn the_charts_default_is_a_default_class() {
        assert!(longhorn_is_default(&json!({})));
        assert!(longhorn_is_default(
            &json!({"persistence": {"defaultClass": true}})
        ));
        assert!(!longhorn_is_default(
            &json!({"persistence": {"defaultClass": false}})
        ));
    }

    #[test]
    fn default_classes_are_read_from_their_annotation() {
        let classes = json!({"items": [
            {"metadata": {"name": "local-path", "annotations": {DEFAULT_CLASS_ANNOTATION: "false"}}},
            {"metadata": {"name": "longhorn", "annotations": {DEFAULT_CLASS_ANNOTATION: "true"}}},
            {"metadata": {"name": "other"}},
        ]});
        assert_eq!(default_classes(&classes), ["longhorn"]);
        assert_eq!(default_classes(&json!({})), Vec::<String>::new());
    }

    #[test]
    fn failed_longhorn_conditions_name_the_node() {
        let nodes = json!({"items": [
            {"metadata": {"name": "vps-1"}, "status": {"conditions": [
                {"type": "Ready", "status": "True"},
                {"type": "RequiredPackages", "status": "False", "message": "Missing packages: [nfs-common] "},
            ]}},
            {"metadata": {"name": "vps-2"}, "status": {"conditions": [
                {"type": "Schedulable", "status": "False", "message": ""},
            ]}},
        ]});
        assert_eq!(
            longhorn_node_problems(&nodes),
            [
                "vps-1: RequiredPackages (Missing packages: [nfs-common])",
                "vps-2: Schedulable"
            ]
        );
    }
}
