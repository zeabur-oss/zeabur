//! The install plan: everything `apply` will do, computed before the host is touched.
//!
//! The same configuration, catalog and host facts give the same plan, byte for byte, so `plan`
//! can be reviewed, diffed and tested. Secret values are never part of it: a plan names the
//! Secret key and where its value comes from.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr};

use serde::Serialize;
use serde_json::{Value, json};

use crate::catalog::{Component, Encoding, Index, SecretRef};
use crate::cluster::{self, Distribution, HostFile};
use crate::config::{AccessMode, Config};
use crate::pins::{self, CALICO, K3S, K3s, RKE2};
use crate::remove::Removal;
use crate::select::{self, Selection, Why};
use crate::storage::{self, NodePrep, Storage};
use crate::{PlanError, node_name, values};

/// The label and field manager on everything the installer creates.
pub const MANAGER: &str = "zeabur-installer";
/// The label the baseline policies let traffic in from. `app.kubernetes.io/managed-by` cannot be
/// it: a product that labels its own namespace, as Zeabur Identity does, takes that one over.
pub const BASELINE_LABEL: &str = "zeabur.com/platform";
/// The field manager that applies the installer's labels on namespaces, and nothing else. Its own
/// manager keeps them when other manifests that hold the same namespace are applied.
pub const NAMESPACE_MANAGER: &str = "zeabur-installer-namespaces";
/// The annotation naming the catalog component an object belongs to.
pub const COMPONENT_ANNOTATION: &str = "installer.zeabur.com/component";
/// Zeabur Identity's namespace.
pub const IDENTITY_NAMESPACE: &str = "zeabur-identity";
/// Zeabur's namespace.
pub const ZEABUR_NAMESPACE: &str = "zeabur";
/// The `ClusterIssuer` of `local` access: a CA cert-manager makes on the server.
pub const LOCAL_ISSUER: &str = "zeabur-local-ca";
/// The catalog's Zeabur Builder, which brings zot: the in-cluster registry that nodes pull
/// through a mirror to its fixed address.
pub const BUILDER: &str = "builds/zeabur-builder";
/// The label Zeabur Builder puts on the Pods of a build's Job.
const BUILD_RUN_LABEL: &str = "builder.zeabur.com/buildrun";
/// The ConfigMap in Zeabur's namespace with zot's CA, which Zeabur trusts to read its images.
pub const REGISTRY_CA_CONFIGMAP: &str = "zeabur-registry-ca";
/// The catalog's log store, whose history Zeabur reads through Zeabur Proxy.
const VICTORIA_LOGS: &str = "logs/victoria-logs";
/// The catalog's metrics store, whose usage history Zeabur reads through Zeabur Proxy.
const VICTORIA_METRICS: &str = "metrics/victoria-metrics";
/// zot's input naming that address.
const ZOT_CLUSTER_IP_INPUT: &str = "cluster-ip";
/// The catalog's External Secrets Operator.
const EXTERNAL_SECRETS: &str = "external-secrets/external-secrets";
/// Its input naming the Kubernetes API server's addresses.
const KUBE_API_CIDR_INPUT: &str = "kube-api-cidr";
/// The `ClusterIssuer` of `domain` access, shipped by the catalog's cert-manager component.
pub const ACME_ISSUER: &str = "letsencrypt";
/// From this much memory up, an interface suggests RKE2 over K3s: an 8 GiB server, less what the
/// kernel and firmware keep. Below it, K3s leaves more of the host to the apps.
pub const RKE2_SUGGESTED_MIB: u64 = 7 * 1024;

/// Why components' pods need more than `restricted`: those the catalog does not mark privileged,
/// and those whose reason is worth more than the catalog's flag.
const PRIVILEGED: &[(&str, &str)] = &[
    (
        "metrics/victoria-metrics",
        "node-exporter reads the host's /proc and /sys on the host network",
    ),
    (
        storage::LONGHORN,
        "its manager, CSI plugin and instance managers mount /dev and the kubelet's directory \
         from the host and run iscsiadm and mount in the host's namespaces",
    ),
];

/// Facts about the host, read by `apply` and passed in so planning stays free of I/O.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Facts {
    /// `amd64` or `arm64`.
    pub arch: String,
    /// The host name; the default cluster name.
    pub hostname: String,
    /// The name an earlier install gave this node in Kubernetes; a re-run keeps it.
    pub node_name: Option<String>,
    /// Six hexadecimal digits from the machine id, for a node name when the host name will not do.
    pub host_hex: String,
    /// Memory in MiB.
    pub memory_mib: u64,
    /// The addresses the node registers with (`node-ip`); empty leaves the choice to K3s or RKE2.
    pub node_ips: Vec<String>,
    /// The prefix length of the first IPv4 node address on its interface.
    pub node_prefix: Option<u8>,
}

impl Facts {
    /// The name this host has as a node: [`Self::node_name`], else the host name in lower case,
    /// as K3s, RKE2 and `KubeSolo` name a node they are not told the name of.
    #[must_use]
    pub fn own_node_name(&self) -> String {
        self.node_name
            .clone()
            .unwrap_or_else(|| self.hostname.to_ascii_lowercase())
    }
}

/// The whole plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Facts it was computed from.
    pub facts: Facts,
    /// The node's name in Kubernetes, written into the distribution's configuration.
    pub node_name: String,
    /// The Kubernetes release.
    pub cluster: ClusterPlan,
    /// Calico's manifest, applied once K3s's API answers; `None` on RKE2, which brings Canal.
    pub calico: Option<Download>,
    /// Files written before K3s or RKE2 starts.
    #[serde(skip)]
    pub host_files: Vec<HostFile>,
    /// Their paths, for the JSON form.
    pub host_file_paths: Vec<String>,
    /// Namespaces created before anything installs into them.
    pub namespaces: Vec<Namespace>,
    /// Catalog components in install order.
    pub components: Vec<ComponentPlan>,
    /// Catalog components named but left out, with the reason.
    pub left_out: BTreeMap<String, String>,
    /// Installed components no longer planned; `apply` fills it from the cluster
    /// ([`crate::remove`]).
    pub removals: Vec<Removal>,
    /// The storage classes, and which one is the default.
    pub storage: Storage,
    /// What every node needs before the components' pods run on it.
    pub node_prep: Option<NodePrep>,
    /// zot's fixed `ClusterIP`, when Zeabur Builder brings zot: nodes pull its images through a
    /// mirror to it, checked against its CA.
    pub zot_cluster_ip: Option<String>,
    /// How the two web services get certificates.
    pub access: AccessMode,
    /// Zeabur Identity, when installed.
    pub identity: Option<IdentityPlan>,
    /// Zeabur, when installed.
    pub zeabur: Option<ZeaburPlan>,
    /// Idle memory: the distribution plus the measured components.
    pub memory: Memory,
    /// Things to read before applying.
    pub warnings: Vec<Warning>,
}

/// Something to read before applying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    /// A stable key, such as `memory.low`, for an interface that words it itself.
    pub code: &'static str,
    /// The values the text names, in order.
    pub values: Vec<String>,
    /// The warning in English.
    pub text: String,
}

pub(crate) fn warning(code: &'static str, values: &[&str], text: String) -> Warning {
    Warning {
        code,
        values: values.iter().map(|&v| v.to_owned()).collect(),
        text,
    }
}

/// The K3s or RKE2 release and its downloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterPlan {
    /// K3s or RKE2.
    pub distribution: Distribution,
    /// Release tag.
    pub version: String,
    /// Whether the digests come from this installer (`true`) or the release's own list.
    pub pinned: bool,
    /// The release's checksum list, read first when the release is not pinned.
    pub checksums: Option<Download>,
    /// K3s's binary or RKE2's tarball.
    pub release: Download,
    /// The image archives it imports on start.
    pub images: Vec<Download>,
    /// K3s's `install.sh`, always the pinned one: it writes the systemd unit and the uninstall
    /// script for any release.
    pub install_script: Option<Download>,
}

impl ClusterPlan {
    /// Every download, the checksum list first.
    pub fn downloads(&self) -> impl Iterator<Item = &Download> {
        self.checksums
            .iter()
            .chain(std::iter::once(&self.release))
            .chain(&self.images)
            .chain(&self.install_script)
    }
}

/// One download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    /// File name.
    pub file: String,
    /// URL.
    pub url: String,
    /// Expected SHA-256, when pinned.
    pub sha256: Option<String>,
}

/// Where `apply` takes a file from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Source {
    /// A path on this server, from `sources`.
    Local(String),
    /// A file of a pinned release, checked against its digest.
    Pinned(Download),
}

impl From<pins::Asset> for Download {
    fn from(asset: pins::Asset) -> Self {
        Self {
            file: asset.file,
            url: asset.url,
            sha256: Some(asset.sha256.to_owned()),
        }
    }
}

/// A namespace and its Pod Security level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Namespace {
    /// Name.
    pub name: String,
    /// Why it is `privileged` instead of the default `restricted`.
    pub privileged: Option<String>,
    /// Why it is `baseline` instead of `restricted`; ignored when it is `privileged`.
    pub baseline: Option<String>,
    /// Open to traffic from anywhere (the ingress namespace); otherwise only its own namespace,
    /// the ingress and the metrics collectors reach it.
    pub public: bool,
}

/// One catalog component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPlan {
    /// `<capability>/<upstream>`.
    pub id: String,
    /// Name for people.
    pub display_name: String,
    /// Why it is installed.
    pub why: &'static str,
    /// Secrets created before its releases.
    pub secrets: Vec<SecretPlan>,
    /// Releases in order.
    pub releases: Vec<ReleasePlan>,
    /// Components whose values it receives.
    pub contributed_by: Vec<String>,
    /// `kubectl wait` arguments that prove it works, one wait per entry. Only the catalog's tests
    /// that check the installed component itself; those creating test objects are for its CI.
    pub smoke: Vec<Vec<String>>,
    /// Measured idle memory.
    pub idle_memory_mib: Option<u64>,
    /// The catalog's one-line security summary.
    pub security: String,
}

/// A Secret key and where its value comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretPlan {
    /// The key.
    pub secret: SecretRefPlan,
    /// `generate` (kept when it already exists) or a file path.
    pub source: SecretSource,
}

/// A Secret key, serialisable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecretRefPlan {
    /// Namespace.
    pub namespace: String,
    /// Name.
    pub name: String,
    /// Key.
    pub key: String,
}

/// Where a secret value comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretSource {
    /// Random bytes, made once.
    Generate {
        /// How many.
        bytes: usize,
        /// How they are written.
        encoding: Encoding,
    },
    /// A file, mode 0600, read at apply time.
    File(String),
}

/// One release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ReleasePlan {
    /// A `HelmChart` object the distribution's helm-controller installs and keeps installed.
    HelmChart {
        /// Release name.
        name: String,
        /// The object.
        object: Value,
        /// `sha256:<hex>` the chart must have; the installer then puts the checked chart in the
        /// object.
        digest: Option<String>,
    },
    /// Manifests applied with server-side apply.
    Manifests {
        /// Release name.
        name: String,
        /// Namespace for namespaced objects without one.
        namespace: String,
        /// YAML.
        yaml: String,
    },
    /// Manifests downloaded and applied with server-side apply.
    Urls {
        /// Release name.
        name: String,
        /// Namespace for namespaced objects without one.
        namespace: String,
        /// URLs.
        urls: Vec<String>,
        /// `sha256:<hex>` of each URL's file, in order; empty when the catalog has none.
        digests: Vec<String>,
        /// Skipped when one of these CRDs exists.
        skip_if_present: Vec<String>,
    },
}

/// Zeabur Identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityPlan {
    /// Issuer URL.
    pub issuer: String,
    /// Host name.
    pub host: String,
    /// Image reference.
    pub image: String,
    /// The `zeabur-identityctl` binary, or its release archive.
    pub identityctl: Source,
    /// The Ingress exposing it.
    pub ingress: Value,
    /// Whether it accepts invitation links.
    pub invitations: bool,
}

/// Zeabur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZeaburPlan {
    /// Public URL.
    pub url: String,
    /// The chart: a local directory or `.tgz`, or the released `.tgz`.
    pub chart: Source,
    /// Helm values, except the generated Secrets' names, which are fixed.
    pub values: Value,
}

/// Memory at idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    /// The Kubernetes distribution itself: K3s, RKE2 or `KubeSolo`, with the OS and Zeabur.
    pub cluster_mib: u64,
    /// The components with a measurement.
    pub components_mib: u64,
    /// Components without a measurement for this architecture.
    pub unmeasured: usize,
    /// The host's memory.
    pub host_mib: u64,
}

/// Computes the plan.
///
/// # Errors
///
/// When the selection fails, values do not parse or a required input is missing.
pub fn plan(index: &Index, config: &Config, facts: &Facts) -> Result<Plan, PlanError> {
    let distribution = config.distribution;
    if facts.memory_mib < distribution.min_memory_mib() {
        return Err(PlanError::TeamMemory {
            distribution,
            min: distribution.min_memory_mib(),
            have: facts.memory_mib,
        });
    }
    let node_name = node_name::choose(config.host.node_name.as_deref(), facts, &[])?;
    let selection = select::select(index, config)?;
    let profile = config.profile.as_str();
    let chosen: BTreeSet<&str> = selection
        .components
        .iter()
        .map(|(id, _)| id.as_str())
        .collect();
    let mut warnings = Vec::new();

    let cluster = cluster_plan(distribution, config.version(), &facts.arch, &mut warnings)?;
    let kubernetes = distribution.kubernetes();
    if !index
        .kubernetes
        .iter()
        .any(|v| v.starts_with(&format!("{kubernetes}.")))
    {
        let tested = index.kubernetes.join(", ");
        warnings.push(warning(
            "kubernetes.untested",
            &[kubernetes, &tested],
            format!("The catalog was not tested on Kubernetes {kubernetes}; it lists {tested}."),
        ));
    }

    let choices = Choices {
        inputs: &config.inputs,
        secret_files: &config.secret_files,
        facts,
    };
    let (components, mut namespaces) =
        components(index, &selection, &choices, profile, &facts.arch)?;

    if !chosen.contains("certificates/cert-manager") && config.zeabur {
        return Err(PlanError::NeedsDisabled {
            component: "zeabur".to_owned(),
            dependency: "certificates/cert-manager".to_owned(),
        });
    }

    let (identity, zeabur) = products(
        config,
        facts,
        pins::PRODUCTS.as_ref(),
        &chosen,
        &mut namespaces,
        &mut warnings,
    )?;

    let zot_cluster_ip = zot_cluster_ip(index, &config.inputs, &chosen);
    let host_files = server_files(
        config,
        &chosen,
        facts,
        &node_name,
        zot_cluster_ip.as_deref(),
    );

    let memory = memory(distribution, &components, facts.memory_mib, &mut warnings);

    Ok(Plan {
        storage: storage_plan(&components, profile, &mut warnings),
        facts: facts.clone(),
        node_name,
        cluster,
        calico: (distribution == Distribution::K3s).then(|| CALICO.manifest().into()),
        host_file_paths: host_files.iter().map(|f| f.path.clone()).collect(),
        host_files,
        namespaces: namespaces.into_values().collect(),
        components,
        left_out: selection.left_out,
        removals: Vec::new(),
        node_prep: storage::node_prep(chosen.iter().copied()),
        zot_cluster_ip,
        access: config.access.mode,
        identity,
        zeabur,
        memory,
        warnings,
    })
}

/// The storage classes: local-path always, Longhorn's when it is planned, and the default as
/// Longhorn's values for the profile say.
fn storage_plan(
    components: &[ComponentPlan],
    profile: &str,
    warnings: &mut Vec<Warning>,
) -> Storage {
    let longhorn_values = components
        .iter()
        .filter(|c| c.id == storage::LONGHORN)
        .flat_map(|c| &c.releases)
        .find_map(|r| match r {
            ReleasePlan::HelmChart { object, .. } => object
                .pointer("/spec/valuesContent")
                .and_then(Value::as_str)
                .and_then(|v| serde_json::from_str::<Value>(v).ok()),
            ReleasePlan::Manifests { .. } | ReleasePlan::Urls { .. } => None,
        });
    let Some(values) = longhorn_values else {
        return Storage {
            default_class: storage::LOCAL_PATH_CLASS.to_owned(),
            other_classes: Vec::new(),
        };
    };
    let replicas = values
        .pointer("/persistence/defaultClassReplicaCount")
        .map_or_else(
            || "3".to_owned(),
            |v| v.to_string().trim_matches('"').to_owned(),
        );
    let (default_class, other) = if storage::longhorn_is_default(&values) {
        warnings.push(warning(
            "storage.default.longhorn",
            &[storage::LONGHORN_CLASS, storage::LOCAL_PATH_CLASS],
            format!(
                "New volumes without a storage class go to {longhorn} (Longhorn). {local} stays, \
                 not the default: name it for databases that keep their own replicas, such as \
                 CloudNativePG. On a cluster where {local} was the default, apply makes it a plain \
                 class; volumes already on it stay there.",
                longhorn = storage::LONGHORN_CLASS,
                local = storage::LOCAL_PATH_CLASS,
            ),
        ));
        (storage::LONGHORN_CLASS, storage::LOCAL_PATH_CLASS)
    } else {
        (storage::LOCAL_PATH_CLASS, storage::LONGHORN_CLASS)
    };
    if profile == "multi-node" {
        warnings.push(warning(
            "storage.longhorn.replicas",
            &[&replicas],
            format!(
                "Longhorn keeps {replicas} copies of each volume, one per node. With fewer nodes, \
                 volumes run degraded with one copy per node and gain the rest as nodes join; \
                 with two nodes, losing one leaves a single copy until it is back."
            ),
        ));
    }
    Storage {
        default_class: default_class.to_owned(),
        other_classes: vec![other.to_owned()],
    }
}

/// The distribution's packaged components the catalog replaces.
fn packaged_to_disable(distribution: Distribution, chosen: &BTreeSet<&str>) -> Vec<String> {
    chosen
        .contains("metrics/metrics-server")
        .then(|| distribution.packaged_metrics_server().to_owned())
        .into_iter()
        .collect()
}

/// Zeabur Identity and Zeabur, and their namespaces.
fn products(
    config: &Config,
    facts: &Facts,
    pins: Option<&pins::Products>,
    chosen: &BTreeSet<&str>,
    namespaces: &mut BTreeMap<String, Namespace>,
    warnings: &mut Vec<Warning>,
) -> Result<(Option<IdentityPlan>, Option<ZeaburPlan>), PlanError> {
    if !config.zeabur {
        return Ok((None, None));
    }
    let tls_issuer = match config.access.mode {
        AccessMode::Domain => ACME_ISSUER,
        AccessMode::Local => LOCAL_ISSUER,
    };
    for name in [IDENTITY_NAMESPACE, ZEABUR_NAMESPACE] {
        namespaces.insert(
            name.to_owned(),
            Namespace {
                name: name.to_owned(),
                privileged: None,
                baseline: None,
                public: false,
            },
        );
    }
    let sources = &config.sources;
    let image = sources
        .identity_image
        .clone()
        .or_else(|| pins.map(|p| p.identity.image.to_owned()));
    let identityctl = sources.identityctl.clone().map(Source::Local).or_else(|| {
        pins.and_then(|p| p.identity.identityctl(&facts.arch))
            .map(|a| Source::Pinned(a.into()))
    });
    let chart = sources
        .zeabur_chart
        .clone()
        .map(Source::Local)
        .or_else(|| pins.map(|p| Source::Pinned(p.zeabur.chart().into())));
    let zeabur_image = sources
        .zeabur_image
        .clone()
        .or_else(|| pins.map(|p| p.zeabur.image.to_owned()));
    let missing: Vec<&'static str> = [
        ("identityImage", image.is_none()),
        ("identityctl", identityctl.is_none()),
        ("zeaburImage", zeabur_image.is_none()),
        ("zeaburChart", chart.is_none()),
    ]
    .into_iter()
    .filter_map(|(key, missing)| missing.then_some(key))
    .collect();
    let (Some(image), Some(identityctl), Some(chart), Some(zeabur_image)) =
        (image, identityctl, chart, zeabur_image)
    else {
        return Err(PlanError::Unpublished(missing));
    };
    let identity = IdentityPlan {
        issuer: config.issuer(),
        host: config.access.identity.clone(),
        image,
        identityctl,
        ingress: ingress(
            IDENTITY_NAMESPACE,
            "zeabur-identity",
            &config.access.identity,
            tls_issuer,
        ),
        invitations: config.access.invitations,
    };
    // The chart reads usage history from the catalog's VictoriaMetrics by default.
    if !chosen.contains(VICTORIA_METRICS) {
        warnings.push(warning(
            "metrics.missing",
            &[],
            "Without metrics/victoria-metrics, Zeabur shows no usage history.".to_owned(),
        ));
    }
    if config.access.mode == AccessMode::Local {
        warnings.push(warning(
            "local.dns",
            &[&config.access.identity, &config.access.zeabur],
            format!(
                "Local access: {} and {} must resolve to this server for browsers and for the \
                 server itself, through DNS (names such as <address>.sslip.io do) rather than a \
                 hosts file, which pods do not read.",
                config.access.identity, config.access.zeabur
            ),
        ));
    }
    let suffixes = suffixes(config, facts);
    warn_no_suffixes(config, facts, &suffixes, warnings);
    let mut values = zeabur_values(config, facts, tls_issuer, &zeabur_image, &suffixes);
    chosen_values(chosen, &mut values);
    Ok((
        Some(identity),
        Some(ZeaburPlan {
            url: config.zeabur_url(),
            chart,
            values,
        }),
    ))
}

/// The chart values that follow from the chosen components.
fn chosen_values(chosen: &BTreeSet<&str>, values: &mut Value) {
    let Some(map) = values.as_object_mut() else {
        return;
    };
    if chosen.contains(BUILDER) {
        map.insert(
            "builds".to_owned(),
            json!({"caBundle": {"configMap": REGISTRY_CA_CONFIGMAP}}),
        );
    }
    let logs = chosen.contains(VICTORIA_LOGS);
    if logs || chosen.contains(VICTORIA_METRICS) {
        map.insert(
            "proxy".to_owned(),
            json!({"enabled": true, "logs": {"enabled": logs}}),
        );
    }
}

/// Zeabur Identity and Zeabur as `plan` gives them, for a cluster that has its components from
/// another source (`scripts/dev-cluster.sh`). `None` when the configuration says `zeabur: false`.
///
/// # Errors
///
/// When a source is missing.
pub fn products_only(
    config: &Config,
    facts: &Facts,
) -> Result<Option<(IdentityPlan, ZeaburPlan)>, PlanError> {
    let (identity, zeabur) = products(
        config,
        facts,
        pins::PRODUCTS.as_ref(),
        &BTreeSet::new(),
        &mut BTreeMap::new(),
        &mut Vec::new(),
    )?;
    Ok(identity.zip(zeabur))
}

/// Idle memory, and a warning when the host is short of it.
fn memory(
    distribution: Distribution,
    components: &[ComponentPlan],
    host_mib: u64,
    warnings: &mut Vec<Warning>,
) -> Memory {
    let memory = idle_memory(distribution.base_idle_mib(), components, host_mib);
    let idle = memory.cluster_mib + memory.components_mib;
    if idle > host_mib * 4 / 5 {
        let title = distribution.title();
        warnings.push(warning(
            "memory.tight",
            &[&idle.to_string(), &host_mib.to_string(), title],
            format!("At idle, the system, {title} and the components use about {idle} MiB of this host's {host_mib} MiB."),
        ));
    }
    memory
}

pub(crate) fn idle_memory(cluster_mib: u64, components: &[ComponentPlan], host_mib: u64) -> Memory {
    Memory {
        cluster_mib,
        components_mib: components.iter().filter_map(|c| c.idle_memory_mib).sum(),
        unmeasured: components
            .iter()
            .filter(|c| c.idle_memory_mib.is_none())
            .count(),
        host_mib,
    }
}

/// The release of `distribution` and its downloads: the pinned one, or `version` checked against
/// its own checksum list, with a warning.
pub(crate) fn cluster_plan(
    distribution: Distribution,
    version: Option<&str>,
    arch: &str,
    warnings: &mut Vec<Warning>,
) -> Result<ClusterPlan, PlanError> {
    let unknown_arch = || PlanError::Arch(arch.to_owned());
    let pinned_version = distribution.pinned_version();
    let (version, pinned) = match version {
        Some(v) if v != pinned_version => {
            let title = distribution.title();
            warnings.push(warning(
                "cluster.unsupported",
                &[title, v],
                format!(
                    "{title} {v} is not supported by this installer release: its downloads are \
                     checked only against the release's own checksum list, and the catalog may \
                     not have been tested on it."
                ),
            ));
            (v.to_owned(), false)
        }
        Some(_) | None => (pinned_version.to_owned(), true),
    };
    let url: fn(&str, &str) -> String = match distribution {
        Distribution::K3s => pins::k3s_url,
        Distribution::Rke2 => pins::rke2_url,
    };
    let download = |file: String, sha: &str| Download {
        url: url(&version, &file),
        sha256: pinned.then(|| sha.to_owned()),
        file,
    };
    let (release, images, install_script) = match distribution {
        Distribution::K3s => (
            download(
                K3s::binary_file(arch),
                K3S.binary.get(arch).ok_or_else(unknown_arch)?,
            ),
            vec![download(
                format!("k3s-airgap-images-{arch}.tar.zst"),
                K3S.images.get(arch).ok_or_else(unknown_arch)?,
            )],
            Some(Download {
                file: "install.sh".to_owned(),
                url: pins::k3s_install_script_url(K3S.version),
                sha256: Some(K3S.install_script.to_owned()),
            }),
        ),
        Distribution::Rke2 => {
            let files = RKE2.files(arch).ok_or_else(unknown_arch)?;
            (
                download(format!("rke2.linux-{arch}.tar.gz"), files.tarball),
                vec![
                    download(
                        format!("rke2-images-core.linux-{arch}.tar.zst"),
                        files.images_core,
                    ),
                    download(
                        format!("rke2-images-canal.linux-{arch}.tar.zst"),
                        files.images_canal,
                    ),
                ],
                None,
            )
        }
    };
    let checksums = (!pinned).then(|| {
        let file = format!("sha256sum-{arch}.txt");
        Download {
            url: url(&version, &file),
            sha256: None,
            file,
        }
    });
    Ok(ClusterPlan {
        distribution,
        version,
        pinned,
        checksums,
        release,
        images,
        install_script,
    })
}

/// What a configuration gives the components: input values and secret files.
pub(crate) struct Choices<'a> {
    pub(crate) inputs: &'a BTreeMap<String, BTreeMap<String, Value>>,
    pub(crate) secret_files: &'a BTreeMap<String, BTreeMap<String, String>>,
    pub(crate) facts: &'a Facts,
}

/// Whether the installer fills this input itself, so a front end need not ask for it.
#[must_use]
pub fn installer_fills(component: &str, input: &str) -> bool {
    matches!(
        (component, input),
        (BUILDER, ZOT_CLUSTER_IP_INPUT) | (EXTERNAL_SECRETS, KUBE_API_CIDR_INPUT)
    )
}

/// Inputs that the installer adds to a catalog component: (component, input, release, values
/// path). The catalog sets a default at the path.
const INSTALLER_INPUTS: &[(&str, &str, &str, &str)] = &[(
    "volume-backup/velero",
    "region",
    "velero",
    "configuration.backupStorageLocation.0.config.region",
)];

/// The value of an input the installer knows; a configured value wins over it.
fn known_input(component: &str, input: &str, facts: &Facts) -> Option<Value> {
    match (component, input) {
        (BUILDER, ZOT_CLUSTER_IP_INPUT) => Some(json!(cluster::ZOT_CLUSTER_IP)),
        (EXTERNAL_SECRETS, KUBE_API_CIDR_INPUT) => {
            let ip = facts
                .node_ips
                .iter()
                .find(|a| a.parse::<Ipv4Addr>().is_ok())?;
            Some(json!(ipv4_network(ip, facts.node_prefix)?))
        }
        _ => None,
    }
}

/// The IPv4 network of `address` as `network/prefix`; the address with `/32` without a prefix.
fn ipv4_network(address: &str, prefix: Option<u8>) -> Option<String> {
    let ip: Ipv4Addr = address.parse().ok()?;
    let prefix = prefix.filter(|p| (1..=32).contains(p)).unwrap_or(32);
    let mask = u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0);
    Some(format!("{}/{prefix}", Ipv4Addr::from(u32::from(ip) & mask)))
}

/// The chosen components and the namespaces their releases install into.
pub(crate) fn components(
    index: &Index,
    selection: &Selection,
    choices: &Choices<'_>,
    profile: &str,
    arch: &str,
) -> Result<(Vec<ComponentPlan>, BTreeMap<String, Namespace>), PlanError> {
    let chosen: BTreeSet<&str> = selection
        .components
        .iter()
        .map(|(id, _)| id.as_str())
        .collect();
    // Longhorn first, so the volumes of the components after it find its class.
    let mut ordered = selection.components.clone();
    ordered.sort_by_key(|(id, _)| id != storage::LONGHORN);
    let mut components = Vec::new();
    let mut namespaces: BTreeMap<String, Namespace> = BTreeMap::new();
    for (id, why) in &ordered {
        let component = index
            .component(id)
            .ok_or_else(|| PlanError::Unknown(id.clone()))?;
        let contributors: Vec<&Component> = index
            .components
            .iter()
            .filter(|c| {
                chosen.contains(c.id.as_str()) && c.contributes.iter().any(|x| x.target == *id)
            })
            .collect();
        components.push(component_plan(
            component,
            *why,
            &contributors,
            choices,
            profile,
            arch,
        )?);
        let privileged = privileged(component, choices);
        let baseline = baseline(component);
        for release in &component.releases {
            if release.namespace == "kube-system" {
                continue;
            }
            let entry = namespaces
                .entry(release.namespace.clone())
                .or_insert_with(|| Namespace {
                    name: release.namespace.clone(),
                    privileged: None,
                    baseline: None,
                    public: component.capability == "ingress",
                });
            if entry.privileged.is_none() {
                entry.privileged.clone_from(&privileged);
            }
            if entry.baseline.is_none() {
                entry.baseline.clone_from(&baseline);
            }
        }
    }
    Ok((components, namespaces))
}

fn component_plan(
    component: &Component,
    why: Why,
    contributors: &[&Component],
    choices: &Choices<'_>,
    profile: &str,
    arch: &str,
) -> Result<ComponentPlan, PlanError> {
    let id = component.id.as_str();
    let (secrets, paths) = inputs(component, choices)?;

    let mut releases = Vec::new();
    for release in &component.releases {
        let name = release.release_name().to_owned();
        if let Some(chart) = &release.chart {
            let mut vals = values::parse(
                release.values.as_deref().unwrap_or(""),
                &format!("{id} {name}"),
            )?;
            if let Some(over) = release.profiles.as_ref().and_then(|p| p.get(profile)) {
                values::merge(
                    &mut vals,
                    values::parse(over, &format!("{id} {name} {profile}"))?,
                );
            }
            for contributor in contributors {
                for c in contributor
                    .contributes
                    .iter()
                    .filter(|c| c.target == id && c.release == release.name)
                {
                    values::merge(
                        &mut vals,
                        values::parse(&c.values, &format!("{} → {id}", contributor.id))?,
                    );
                }
            }
            for (_, path, value) in paths.iter().filter(|(r, ..)| *r == release.name) {
                values::set_path(&mut vals, path, value.clone())?;
            }
            let mut spec = json!({
                "chart": chart,
                "version": release.version,
                "targetNamespace": release.namespace,
                "createNamespace": false,
                "valuesContent": values::canonical(&vals),
                "failurePolicy": "abort",
                "timeout": "10m0s",
            });
            if let (Some(repo), Some(map)) = (&release.repo, spec.as_object_mut()) {
                map.insert("repo".to_owned(), json!(repo));
            }
            releases.push(ReleasePlan::HelmChart {
                object: helm_chart(&name, id, &spec),
                name,
                digest: release.chart_digest.clone(),
            });
        } else if let Some(yaml) = &release.manifests {
            releases.push(ReleasePlan::Manifests {
                name,
                namespace: release.namespace.clone(),
                yaml: yaml.clone(),
            });
        } else {
            let version = release.version.clone().unwrap_or_default();
            #[expect(
                clippy::literal_string_with_formatting_args,
                reason = "the catalog's placeholder"
            )]
            let placeholder = "{version}";
            releases.push(ReleasePlan::Urls {
                name,
                namespace: release.namespace.clone(),
                urls: release
                    .urls
                    .iter()
                    .map(|u| u.replace(placeholder, &version))
                    .collect(),
                digests: release.url_digests.clone(),
                skip_if_present: release.skip_if_present.clone(),
            });
        }
    }

    Ok(ComponentPlan {
        id: id.to_owned(),
        display_name: component.metadata.name.clone(),
        why: match why {
            Why::Preset => "preset",
            Why::Enabled => "enabled",
            Why::Dependency => "dependency",
        },
        secrets,
        releases,
        contributed_by: contributors.iter().map(|c| c.id.clone()).collect(),
        smoke: component
            .smoke
            .iter()
            .filter(|s| s.manifest.is_none())
            .flat_map(|s| {
                s.wait
                    .iter()
                    .map(|w| w.split_whitespace().map(str::to_owned).collect())
            })
            .collect(),
        idle_memory_mib: component.idle_memory_mib(profile, arch),
        security: component.security.summary.clone(),
    })
}

/// The component's Secrets, and the values of its path inputs as (release, path, value).
type Inputs = (Vec<SecretPlan>, Vec<(String, String, Value)>);

fn inputs(component: &Component, choices: &Choices<'_>) -> Result<Inputs, PlanError> {
    let id = component.id.as_str();
    let first_chart = component
        .releases
        .iter()
        .find(|r| r.chart.is_some())
        .map(|r| r.name.clone());
    let given = choices.inputs.get(id);
    let files = choices.secret_files.get(id);
    let mut secrets = Vec::new();
    let mut paths: Vec<(String, String, Value)> = Vec::new();
    for input in &component.inputs {
        if let Some(secret) = &input.secret {
            let source = match files.and_then(|f| f.get(&input.name)) {
                Some(path) => SecretSource::File(path.clone()),
                None if input.generate.as_deref() == Some("random") => SecretSource::Generate {
                    bytes: input.generate_bytes.unwrap_or(64),
                    encoding: input.generate_encoding,
                },
                None => {
                    return Err(PlanError::MissingInput {
                        component: id.to_owned(),
                        input: input.name.clone(),
                        hint: format!("secretFiles.{id}.{}: <file, mode 0600>", input.name),
                    });
                }
            };
            secrets.push(SecretPlan {
                secret: secret_ref(secret),
                source,
            });
        } else if let Some(path) = &input.path {
            let value = given
                .and_then(|g| g.get(&input.name))
                .cloned()
                .or_else(|| input.default.clone())
                .or_else(|| known_input(id, &input.name, choices.facts));
            let release = input
                .release
                .clone()
                .or_else(|| first_chart.clone())
                .unwrap_or_default();
            match value {
                Some(value) => paths.push((release, path.clone(), value)),
                None if input.required => {
                    return Err(PlanError::MissingInput {
                        component: id.to_owned(),
                        input: input.name.clone(),
                        hint: format!(
                            "inputs.{id}.{}: <value>  # {}",
                            input.name, input.description
                        ),
                    });
                }
                None => {}
            }
        }
    }
    for (_, name, release, path) in INSTALLER_INPUTS.iter().filter(|(c, ..)| *c == id) {
        if let Some(value) = given.and_then(|g| g.get(*name)) {
            paths.push(((*release).to_owned(), (*path).to_owned(), value.clone()));
        }
    }
    Ok((secrets, paths))
}

/// Why a component's namespaces need the `privileged` Pod Security level, if they do.
fn privileged(component: &Component, choices: &Choices<'_>) -> Option<String> {
    let id = &component.id;
    if id == "volume-backup/velero" {
        // Only its node agent needs the level, and the `file-backup` input installs it.
        let agent = choices
            .inputs
            .get(id)
            .and_then(|given| given.get("file-backup"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return agent.then(|| format!("{id}: its node agent mounts host paths and runs as root"));
    }
    PRIVILEGED
        .iter()
        .find(|(c, _)| c == id)
        .map(|(c, reason)| format!("{c}: {reason}"))
        .or_else(|| {
            component
                .security
                .privileged
                .then(|| format!("{id} is privileged"))
        })
}

/// Why a component's namespaces need the `baseline` Pod Security level, if they do.
fn baseline(component: &Component) -> Option<String> {
    (component.security.level.as_deref() == Some("baseline")).then(|| {
        format!(
            "{}: its pods set no seccomp profile or dropped capabilities",
            component.id
        )
    })
}

/// The first server's files.
fn server_files(
    config: &Config,
    chosen: &BTreeSet<&str>,
    facts: &Facts,
    node_name: &str,
    zot_cluster_ip: Option<&str>,
) -> Vec<HostFile> {
    let distribution = config.distribution;
    let mut files = distribution.server_files(&cluster::Options {
        tls_san: config.host.tls_san.clone(),
        disable: packaged_to_disable(distribution, chosen),
        identity: config.zeabur,
    });
    files.push(distribution.node_name_file(node_name));
    files.extend(distribution.node_ip_file(&facts.node_ips));
    files.extend(zot_cluster_ip.map(|ip| distribution.registries_file(ip)));
    files
}

/// zot's fixed `ClusterIP`: the configured `cluster-ip` input, else [`cluster::ZOT_CLUSTER_IP`].
/// `None` when Zeabur Builder is not chosen, or its catalog release takes no such input.
pub(crate) fn zot_cluster_ip(
    index: &Index,
    inputs: &BTreeMap<String, BTreeMap<String, Value>>,
    chosen: &BTreeSet<&str>,
) -> Option<String> {
    let component = index
        .component(BUILDER)
        .filter(|_| chosen.contains(BUILDER))?;
    let input = component
        .inputs
        .iter()
        .find(|i| i.name == ZOT_CLUSTER_IP_INPUT)?;
    let given = inputs
        .get(BUILDER)
        .and_then(|g| g.get(&input.name))
        .and_then(Value::as_str);
    Some(given.unwrap_or(cluster::ZOT_CLUSTER_IP).to_owned())
}

/// A `HelmChart` in `kube-system`, labelled as the installer's.
fn helm_chart(name: &str, component: &str, spec: &Value) -> Value {
    json!({
        "apiVersion": "helm.cattle.io/v1",
        "kind": "HelmChart",
        "metadata": {
            "name": name,
            "namespace": "kube-system",
            "labels": {"app.kubernetes.io/managed-by": MANAGER},
            "annotations": {COMPONENT_ANNOTATION: component},
        },
        "spec": spec,
    })
}

fn secret_ref(secret: &SecretRef) -> SecretRefPlan {
    SecretRefPlan {
        namespace: secret.namespace.clone(),
        name: secret.name.clone(),
        key: secret.key.clone(),
    }
}

/// An Ingress on Traefik with a certificate from the `ClusterIssuer`.
fn ingress(namespace: &str, service: &str, host: &str, issuer: &str) -> Value {
    json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "Ingress",
        "metadata": {
            "name": service,
            "namespace": namespace,
            "labels": {"app.kubernetes.io/managed-by": MANAGER},
            "annotations": {"cert-manager.io/cluster-issuer": issuer},
        },
        "spec": {
            "ingressClassName": "traefik",
            "tls": [{"hosts": [host], "secretName": format!("{service}-tls")}],
            "rules": [{
                "host": host,
                "http": {"paths": [{
                    "path": "/",
                    "pathType": "Prefix",
                    "backend": {"service": {"name": service, "port": {"name": "http"}}},
                }]},
            }],
        },
    })
}

/// A namespace with its Pod Security level, labelled as the installer's.
#[must_use]
pub fn namespace_object(namespace: &Namespace) -> Value {
    let mut labels = json!({"app.kubernetes.io/managed-by": MANAGER, BASELINE_LABEL: "true"});
    let (level, reason) = match (&namespace.privileged, &namespace.baseline) {
        (Some(why), _) => (Some("privileged"), Some(why)),
        (None, Some(why)) => (Some("baseline"), Some(why)),
        (None, None) => (None, None),
    };
    if let (Some(level), Some(map)) = (level, labels.as_object_mut()) {
        for mode in ["enforce", "audit", "warn"] {
            map.insert(format!("pod-security.kubernetes.io/{mode}"), json!(level));
        }
    }
    let mut metadata = json!({"name": namespace.name, "labels": labels});
    if let (Some(level), Some(reason), Some(map)) = (level, reason, metadata.as_object_mut()) {
        map.insert(
            "annotations".to_owned(),
            json!({format!("installer.zeabur.com/{level}"): reason}),
        );
    }
    json!({"apiVersion": "v1", "kind": "Namespace", "metadata": metadata})
}

/// Calico's annotations on a Node naming the addresses of its VXLAN interface.
const CALICO_TUNNEL_ANNOTATIONS: [&str; 2] = [
    "projectcalico.org/IPv4VXLANTunnelAddr",
    "projectcalico.org/IPv6VXLANTunnelAddr",
];

/// The addresses node traffic comes from, read from `kubectl get nodes -o json`.
///
/// A node's traffic to pods on other nodes leaves from its VXLAN interface. Calico (K3s) names
/// that interface's address in an annotation; Canal (RKE2) gives it the first address of the
/// node's pod range. A node's traffic to its own pods passes without a rule. Sorted and without
/// repeats.
#[must_use]
pub fn node_sources(nodes: &Value) -> Vec<String> {
    let mut found = BTreeSet::new();
    for node in nodes
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let calico: Vec<String> = CALICO_TUNNEL_ANNOTATIONS
            .iter()
            .filter_map(|key| {
                node.pointer("/metadata/annotations")
                    .and_then(|a| a.get(*key))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect();
        if calico.is_empty() {
            found.extend(
                node.pointer("/spec/podCIDRs")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter_map(|cidr| cidr.split_once('/').map(|(base, _)| base.to_owned())),
            );
        } else {
            found.extend(calico);
        }
    }
    found.into_iter().collect()
}

/// The `ipBlock` sources of a baseline policy, as `network_policy` writes them.
#[must_use]
pub fn policy_sources(policy: &Value) -> BTreeSet<String> {
    policy
        .pointer("/spec/ingress/0/from")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| f.pointer("/ipBlock/cidr").and_then(Value::as_str))
        .filter_map(|c| c.split_once('/').map(|(a, _)| a.to_owned()))
        .collect()
}

/// The baseline `NetworkPolicy` of a namespace that is not public.
///
/// Traffic comes in from its own pods, from the installer's other namespaces (labelled
/// [`BASELINE_LABEL`]; the ingress and the metrics collectors among them) and from the other
/// nodes (kube-apiserver calling webhooks, containerd pulling from zot).
#[must_use]
pub fn network_policy(namespace: &str, node_addresses: &[String]) -> Value {
    let mut from = vec![
        json!({"podSelector": {}}),
        json!({"namespaceSelector": {"matchLabels": {BASELINE_LABEL: "true"}}}),
    ];
    from.extend(node_addresses.iter().map(|a| {
        let cidr = if a.contains(':') {
            format!("{a}/128")
        } else {
            format!("{a}/32")
        };
        json!({"ipBlock": {"cidr": cidr}})
    }));
    json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "NetworkPolicy",
        "metadata": {
            "name": "zeabur-installer-baseline",
            "namespace": namespace,
            "labels": {"app.kubernetes.io/managed-by": MANAGER},
        },
        "spec": {"podSelector": {}, "policyTypes": ["Ingress"], "ingress": [{"from": from}]},
    })
}

/// The `NetworkPolicy` that lets Zeabur Builder reach zot on its port.
///
/// The baseline policy of zot's namespace would refuse them: buildkitd pushes, the controller
/// deletes the repositories of deleted Builds, and a build's Job in a project namespace fetches its
/// archive source. Other pods in a project namespace cannot reach zot, so a compromised app cannot
/// pull the images of other projects. Only an admin can set the project label on a Namespace. A
/// person who can create Pods in a project namespace can set the build Job's label on a Pod.
#[must_use]
pub fn zot_builder_policy() -> Value {
    json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "NetworkPolicy",
        "metadata": {
            "name": "zeabur-installer-zot-builder",
            "namespace": "zot",
            "labels": {"app.kubernetes.io/managed-by": MANAGER},
        },
        "spec": {
            "podSelector": {"matchLabels": {"app.kubernetes.io/name": "zot"}},
            "policyTypes": ["Ingress"],
            "ingress": [{
                "from": [
                    {
                        "namespaceSelector": {"matchLabels": {"kubernetes.io/metadata.name": "builder-system"}},
                        "podSelector": {"matchLabels": {"app.kubernetes.io/name": "buildkitd"}},
                    },
                    {
                        "namespaceSelector": {"matchLabels": {"kubernetes.io/metadata.name": "builder-system"}},
                        "podSelector": {"matchLabels": {"app.kubernetes.io/name": "zeabur-builder"}},
                    },
                    {
                        "namespaceSelector": {"matchLabels": {"zeabur.com/project": "true"}},
                        "podSelector": {"matchExpressions": [{"key": BUILD_RUN_LABEL, "operator": "Exists"}]},
                    },
                ],
                "ports": [{"port": 5000, "protocol": "TCP"}],
            }],
        },
    })
}

/// The namespace where K3s and RKE2 run the `ServiceLB` pods.
pub const SERVICELB_NAMESPACE: &str = "kube-system";

/// The `NetworkPolicy` that lets outside traffic into the `ServiceLB` pods.
///
/// `kube-system/default-network-policy` lets in only pods of `kube-system`: RKE2's `cis` profile
/// adds it, and the installer adds it on K3s. Without this policy, a `ServiceLB` pod on a worker
/// gets no outside requests. Each distribution labels the pods with its own prefix.
#[must_use]
pub fn servicelb_policy(distribution: Distribution) -> Value {
    json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "NetworkPolicy",
        "metadata": {
            "name": "zeabur-installer-servicelb",
            "namespace": SERVICELB_NAMESPACE,
            "labels": {"app.kubernetes.io/managed-by": MANAGER},
        },
        "spec": {
            "podSelector": {"matchExpressions": [
                {"key": distribution.servicelb_label(), "operator": "Exists"},
            ]},
            "policyTypes": ["Ingress"],
            "ingress": [{}],
        },
    })
}

/// The objects of `local` access: a self-signed root, a CA certificate kept in cert-manager's
/// namespace, and the `ClusterIssuer` signing with it.
#[must_use]
pub fn local_ca_objects(cert_manager_namespace: &str, hostname: &str) -> Vec<Value> {
    let labels = json!({"app.kubernetes.io/managed-by": MANAGER});
    vec![
        json!({
            "apiVersion": "cert-manager.io/v1",
            "kind": "ClusterIssuer",
            "metadata": {"name": "zeabur-local-root", "labels": labels},
            "spec": {"selfSigned": {}},
        }),
        json!({
            "apiVersion": "cert-manager.io/v1",
            "kind": "Certificate",
            "metadata": {"name": LOCAL_ISSUER, "namespace": cert_manager_namespace, "labels": labels},
            "spec": {
                "isCA": true,
                "commonName": format!("Zeabur local CA ({hostname})"),
                "secretName": LOCAL_ISSUER,
                "duration": "87600h",
                "privateKey": {"algorithm": "ECDSA", "size": 256},
                "issuerRef": {"name": "zeabur-local-root", "kind": "ClusterIssuer", "group": "cert-manager.io"},
            },
        }),
        json!({
            "apiVersion": "cert-manager.io/v1",
            "kind": "ClusterIssuer",
            "metadata": {"name": LOCAL_ISSUER, "labels": labels},
            "spec": {"ca": {"secretName": LOCAL_ISSUER}},
        }),
    ]
}

/// When no app domain is configured and none could be chosen: what line to add.
fn warn_no_suffixes(
    config: &Config,
    facts: &Facts,
    suffixes: &[String],
    warnings: &mut Vec<Warning>,
) {
    if suffixes.is_empty() && config.access.suffixes.is_none() {
        let sslip = facts.node_ips.first().map(|a| sslip_suffix(a));
        warnings.push(match (config.access.mode, sslip) {
            (AccessMode::Local, Some(sslip)) => warning(
                "domains.sslip",
                &[&sslip],
                format!(
                    "This server has no public address, so new services get no suggested URL. \
                     Add `suffixes: [{sslip}]` under access for URLs only this network reaches, \
                     signed by the local CA."
                ),
            ),
            _ => warning(
                "domains.none",
                &[],
                "This server has no public address, so new services get no suggested URL. Add \
                 `suffixes: [apps.example.com]` under access, with a wildcard record \
                 `*.apps.example.com` pointing at the server."
                    .to_owned(),
            ),
        });
    }
}

/// `domains.suffixes`: the configured ones, else the node's sslip.io name when its address is
/// public. Let's Encrypt cannot reach a private address.
fn suffixes(config: &Config, facts: &Facts) -> Vec<String> {
    config.access.suffixes.clone().unwrap_or_else(|| {
        facts
            .node_ips
            .iter()
            .find(|a| is_public(a))
            .map_or_else(Vec::new, |a| vec![sslip_suffix(a)])
    })
}

/// The sslip.io domain that resolves to `address`: `203-0-113-7.sslip.io`, `2001-db8--1.sslip.io`.
#[must_use]
pub fn sslip_suffix(address: &str) -> String {
    format!("{}.sslip.io", address.replace(['.', ':'], "-"))
}

/// An address the internet can reach: not private, shared, loopback, link-local or for
/// documentation.
#[must_use]
pub fn is_public(address: &str) -> bool {
    match address.parse::<IpAddr>() {
        Ok(IpAddr::V4(a)) => {
            let [first, second, ..] = a.octets();
            !(a.is_private()
                || a.is_loopback()
                || a.is_link_local()
                || a.is_documentation()
                || a.is_unspecified()
                || a.is_broadcast()
                || (first == 100 && second & 0xC0 == 64))
        }
        Ok(IpAddr::V6(a)) => {
            let [first, second, ..] = a.segments();
            first & 0xE000 == 0x2000 && !(first == 0x2001 && second == 0x0DB8)
        }
        Err(_) => false,
    }
}

/// The chart values of docs/get-started/install.mdx, with the Secrets the installer creates.
fn zeabur_values(
    config: &Config,
    facts: &Facts,
    issuer: &str,
    image: &str,
    suffixes: &[String],
) -> Value {
    let (repository, digest) = image
        .split_once('@')
        .map_or((image, None), |(r, d)| (r, Some(d)));
    let (repository, tag) = match (digest, repository.rsplit_once(':')) {
        (None, Some((r, t))) if !t.contains('/') => (r, Some(t)),
        _ => (repository, None),
    };
    let mut image = json!({"repository": repository, "pullPolicy": "IfNotPresent"});
    if let Some(map) = image.as_object_mut() {
        if let Some(d) = digest {
            map.insert("digest".to_owned(), json!(d));
        }
        if let Some(t) = tag {
            map.insert("tag".to_owned(), json!(t));
        }
    }
    let mut issuer_values =
        json!({"url": config.issuer(), "clientSecret": {"existingSecret": "zeabur-oidc"}});
    if config.access.mode == AccessMode::Local
        && let Some(map) = issuer_values.as_object_mut()
    {
        map.insert(
            "caBundle".to_owned(),
            json!({"configMap": "zeabur-identity-ca"}),
        );
    }
    json!({
        "image": image,
        "clusterName": facts.hostname,
        "publicUrl": config.zeabur_url(),
        "issuer": issuer_values,
        "planKey": {"existingSecret": "zeabur-plan-key"},
        "domains": {"suffixes": suffixes, "clusterIssuer": issuer},
        "ingress": {
            "enabled": true,
            "className": "traefik",
            "annotations": {"cert-manager.io/cluster-issuer": issuer},
            "tls": {"secretName": "zeabur-tls"},
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = include_str!("../fixtures/index.json");

    fn config(extra: &str) -> Config {
        Config::parse(&format!(
            "access: {{mode: domain, identity: login.example.com, zeabur: zeabur.example.com}}\n\
             sources:\n  identityImage: ghcr.io/zeabur-oss/identity@sha256:aa\n  identityctl: /tmp/ctl\n  \
             zeaburImage: zeabur:dev\n  zeaburChart: /tmp/chart\n{extra}"
        ))
        .unwrap()
    }

    fn facts() -> Facts {
        Facts {
            arch: "arm64".to_owned(),
            hostname: "vps-1".to_owned(),
            node_name: None,
            host_hex: "a1b2c3".to_owned(),
            memory_mib: 8192,
            node_ips: vec!["10.0.0.5".to_owned()],
            node_prefix: Some(24),
        }
    }

    fn ids(plan: &Plan) -> Vec<&str> {
        plan.components.iter().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn standard_follows_the_catalog_and_leaves_out_components_of_another_distribution() {
        let mut index = Index::parse(INDEX.as_bytes()).unwrap();
        for component in &mut index.components {
            if component.id == "node-updates/system-upgrade-controller" {
                component.distributions = vec!["rke2".to_owned()];
            }
        }
        let plan = plan(&index, &config(""), &facts()).unwrap();
        assert_eq!(
            ids(&plan),
            [
                "ingress/traefik",
                "certificates/cert-manager",
                "builds/zeabur-builder",
                "databases/cloudnative-pg",
                "image-updates/keel",
                "observability-operator/victoria-metrics-operator",
                "logs/victoria-logs",
                "metrics/metrics-server",
                "metrics/victoria-metrics",
            ]
        );
        assert_eq!(
            &plan.left_out["node-updates/system-upgrade-controller"],
            "runs on rke2 only"
        );
        assert!(plan.cluster.pinned);
        assert_eq!(plan.cluster.distribution, Distribution::K3s);
        assert!(plan.calico.is_some());
        assert!(
            plan.host_file_paths
                .iter()
                .any(|p| p.ends_with("50-zeabur-identity.yaml"))
        );
        let observability = plan
            .namespaces
            .iter()
            .find(|n| n.name == "observability")
            .unwrap();
        assert!(observability.privileged.is_some());
        let traefik = plan
            .namespaces
            .iter()
            .find(|n| n.name == "traefik")
            .unwrap();
        assert!(traefik.public && traefik.privileged.is_none());
        assert_eq!(plan.storage.default_class, "local-path");
        assert_eq!(plan.storage.other_classes, Vec::<String>::new());
        assert_eq!(plan.node_prep, None);
    }

    #[test]
    fn the_server_is_named_in_its_k3s_configuration() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let named = |extra: &str, facts: &Facts| {
            let plan = plan(&index, &config(extra), facts)?;
            let file = plan
                .host_files
                .iter()
                .find(|f| f.path == "/etc/rancher/k3s/config.yaml.d/15-node-name.yaml")
                .unwrap();
            let value: Value = serde_saphyr::from_str(&file.content).unwrap();
            assert_eq!(value["node-name"], plan.node_name.as_str());
            Ok::<_, PlanError>(plan.node_name)
        };
        assert_eq!(named("", &facts()).unwrap(), "vps-1");
        assert_eq!(named("host: {nodeName: db-1}\n", &facts()).unwrap(), "db-1");
        assert_eq!(named("host: {nodeName: \"on\"}\n", &facts()).unwrap(), "on");
        assert_eq!(
            named("host: {nodeName: \"001\"}\n", &facts()).unwrap(),
            "001"
        );
        let mut cloud = facts();
        cloud.hostname = "ip-10-0-0-5".to_owned();
        assert_eq!(named("", &cloud).unwrap(), "node-a1b2c3");
        let mut renamed = facts();
        renamed.hostname = "10-0-0-5".to_owned();
        renamed.node_name = Some("vps-1".to_owned());
        assert_eq!(named("", &renamed).unwrap(), "vps-1");
        assert!(named("host: {nodeName: db-1}\n", &renamed).is_err());
    }

    #[test]
    fn velero_needs_the_privileged_level_only_for_its_node_agent() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let level = |extra: &str| {
            let plan = plan(
                &index,
                &config(&format!(
                    "enable: [volume-backup/velero]\ninputs:\n  volume-backup/velero:\n    \
                     bucket: b\n    s3-url: http://s3:9000\n{extra}\
                     secretFiles:\n  volume-backup/velero:\n    credentials: /tmp/c\n"
                )),
                &facts(),
            )
            .unwrap();
            plan.namespaces
                .iter()
                .find(|n| n.name == "velero")
                .map(|n| n.privileged.is_some())
        };
        assert_eq!(level(""), Some(false));
        assert_eq!(level("    file-backup: true\n"), Some(true));
    }

    #[test]
    fn velero_takes_the_buckets_region() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let values = |extra: &str| {
            let plan = plan(
                &index,
                &config(&format!(
                    "enable: [volume-backup/velero]\ninputs:\n  volume-backup/velero:\n    \
                     bucket: b\n    s3-url: https://r2.example.com\n{extra}\
                     secretFiles:\n  volume-backup/velero:\n    credentials: /tmp/c\n"
                )),
                &facts(),
            )
            .unwrap();
            let velero = plan
                .components
                .iter()
                .find(|c| c.id == "volume-backup/velero")
                .unwrap();
            let ReleasePlan::HelmChart { object, .. } = &velero.releases[0] else {
                panic!("velero is a chart")
            };
            let content = object["spec"]["valuesContent"].as_str().unwrap();
            serde_saphyr::from_str::<Value>(content).unwrap()["configuration"]
                ["backupStorageLocation"][0]["config"]["region"]
                .clone()
        };
        assert_eq!(values(""), json!("us-east-1"));
        assert_eq!(values("    region: auto\n"), json!("auto"));
    }

    #[test]
    fn k8up_namespace_gets_the_baseline_level() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let plan = plan(&index, &config("enable: [volume-backup/k8up]\n"), &facts()).unwrap();
        let k8up = plan
            .namespaces
            .iter()
            .find(|n| n.name == "k8up-system")
            .unwrap();
        assert!(k8up.baseline.is_some() && k8up.privileged.is_none());
    }

    #[test]
    fn multi_node_makes_longhorn_the_default_and_installs_it_first() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let plan = plan(&index, &config("profile: multi-node\n"), &facts()).unwrap();
        assert_eq!(ids(&plan)[0], storage::LONGHORN);
        assert_eq!(plan.storage.default_class, "longhorn");
        assert_eq!(plan.storage.other_classes, ["local-path"]);
        assert_eq!(plan.node_prep, Some(storage::LONGHORN_PREP));
        let longhorn = plan
            .namespaces
            .iter()
            .find(|n| n.name == "longhorn-system")
            .unwrap();
        assert!(longhorn.privileged.as_ref().unwrap().contains("iscsiadm"));
        let codes: Vec<&str> = plan.warnings.iter().map(|w| w.code).collect();
        assert!(codes.contains(&"storage.default.longhorn"));
        assert!(codes.contains(&"storage.longhorn.replicas"));
    }

    #[test]
    fn longhorn_on_one_node_leaves_local_path_the_default() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let plan = plan(&index, &config("enable: [replicated-storage]\n"), &facts()).unwrap();
        assert!(ids(&plan).contains(&storage::LONGHORN));
        assert_eq!(plan.storage.default_class, "local-path");
        assert_eq!(plan.storage.other_classes, ["longhorn"]);
        assert!(plan.node_prep.is_some());
        assert!(
            plan.warnings
                .iter()
                .all(|w| !w.code.starts_with("storage."))
        );
    }

    #[test]
    fn plans_are_deterministic() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let a =
            serde_json::to_string(&plan(&index, &config("preset: secure\n"), &facts()).unwrap())
                .unwrap();
        let b =
            serde_json::to_string(&plan(&index, &config("preset: secure\n"), &facts()).unwrap())
                .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn crowdsec_contributes_to_traefik_and_generates_its_secrets() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let plan = plan(&index, &config("preset: secure\n"), &facts()).unwrap();
        let traefik = plan
            .components
            .iter()
            .find(|c| c.id == "ingress/traefik")
            .unwrap();
        assert_eq!(traefik.contributed_by, ["intrusion-prevention/crowdsec"]);
        let ReleasePlan::HelmChart { object, .. } = &traefik.releases[1] else {
            panic!()
        };
        let values = object["spec"]["valuesContent"].as_str().unwrap();
        assert!(values.contains("crowdsec-bouncer@kubernetescrd"));
        let crowdsec = plan
            .components
            .iter()
            .find(|c| c.id == "intrusion-prevention/crowdsec")
            .unwrap();
        assert_ne!(crowdsec.secrets.len(), 0);
        assert!(crowdsec.secrets.iter().all(|s| matches!(
            s.source,
            SecretSource::Generate {
                bytes: 64,
                encoding: Encoding::Hex
            }
        )));
        assert!(
            ids(&plan).iter().position(|id| *id == "ingress/traefik")
                < ids(&plan)
                    .iter()
                    .position(|id| *id == "intrusion-prevention/crowdsec")
        );
    }

    #[test]
    fn minimal_and_disabling_a_dependency() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let p = plan(&index, &config("preset: minimal\n"), &facts()).unwrap();
        assert_eq!(
            ids(&p),
            [
                "ingress/traefik",
                "certificates/cert-manager",
                "metrics/metrics-server"
            ]
        );
        let err = plan(&index, &config("disable: [ingress]\n"), &facts()).unwrap_err();
        assert!(matches!(err, PlanError::NeedsDisabled { .. }), "{err}");
    }

    fn zot_values(plan: &Plan) -> String {
        let zot = plan.components.iter().find(|c| c.id == BUILDER).unwrap();
        zot.releases
            .iter()
            .find_map(|release| match release {
                ReleasePlan::HelmChart { object, .. } => {
                    object["spec"]["valuesContent"].as_str().map(str::to_owned)
                }
                ReleasePlan::Manifests { .. } | ReleasePlan::Urls { .. } => None,
            })
            .unwrap()
    }

    #[test]
    fn nodes_pull_from_builders_zot_through_its_fixed_address() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let without = plan(&index, &config("disable: [builds]\n"), &facts()).unwrap();
        assert_eq!(without.zot_cluster_ip, None);
        let k3s = Distribution::K3s;
        assert!(
            !without
                .host_file_paths
                .contains(&k3s.layout().registries_file.to_owned())
        );

        let with = plan(&index, &config(""), &facts()).unwrap();
        assert_eq!(
            with.zot_cluster_ip.as_deref(),
            Some(cluster::ZOT_CLUSTER_IP)
        );
        assert!(zot_values(&with).contains(r#""clusterIP":"10.43.0.50""#));
        let registries = with
            .host_files
            .iter()
            .find(|f| f.path == k3s.layout().registries_file)
            .unwrap();
        assert_eq!(registries, &k3s.registries_file(cluster::ZOT_CLUSTER_IP));

        let chosen = plan(
            &index,
            &config(
                "inputs:\n  builds/zeabur-builder:\n    \
                 cluster-ip: 10.43.7.7\n",
            ),
            &facts(),
        )
        .unwrap();
        assert_eq!(chosen.zot_cluster_ip.as_deref(), Some("10.43.7.7"));
        assert!(zot_values(&chosen).contains(r#""clusterIP":"10.43.7.7""#));
    }

    #[test]
    fn required_inputs_name_the_line_to_add() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let err = plan(
            &index,
            &config("enable: [volume-backup/velero]\n"),
            &facts(),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("inputs.volume-backup/velero.bucket"), "{err}");
        let ok = plan(
            &index,
            &config(
                "enable: [volume-backup/velero]\ninputs:\n  volume-backup/velero:\n    \
                 bucket: backups\n    s3-url: https://s3.example.com\nsecretFiles:\n  \
                 volume-backup/velero:\n    credentials: /root/velero-credentials\n",
            ),
            &facts(),
        )
        .unwrap();
        let velero = ok
            .components
            .iter()
            .find(|c| c.id == "volume-backup/velero")
            .unwrap();
        let ReleasePlan::HelmChart { object, .. } = &velero.releases[0] else {
            panic!()
        };
        assert!(
            object["spec"]["valuesContent"]
                .as_str()
                .unwrap()
                .contains(r#""s3Url":"https://s3.example.com""#)
        );
    }

    #[test]
    fn another_release_is_a_warning_not_an_error() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        for extra in [
            "distribution: rke2\nrke2Version: v1.37.1+rke2r1\n",
            "k3sVersion: v1.37.1+k3s1\n",
        ] {
            let p = plan(&index, &config(extra), &facts()).unwrap();
            assert!(!p.cluster.pinned);
            assert_eq!(
                p.cluster.downloads().next().unwrap().file,
                "sha256sum-arm64.txt"
            );
            assert!(
                p.cluster
                    .downloads()
                    .filter(|d| d.file != "install.sh")
                    .all(|d| d.sha256.is_none()),
                "{extra}"
            );
            assert!(p.warnings.iter().any(|w| w.code == "cluster.unsupported"));
        }
    }

    #[test]
    fn k3s_comes_with_its_binary_images_and_pinned_install_script() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let p = plan(&index, &config(""), &facts()).unwrap();
        let files: Vec<&str> = p.cluster.downloads().map(|d| d.file.as_str()).collect();
        assert_eq!(
            files,
            ["k3s-arm64", "k3s-airgap-images-arm64.tar.zst", "install.sh"]
        );
        assert!(p.cluster.downloads().all(|d| d.sha256.is_some()));
        assert!(
            p.cluster
                .release
                .url
                .starts_with("https://github.com/k3s-io/k3s/releases/download/v1.36.5%2Bk3s1/")
        );
        assert!(p.host_file_paths.iter().all(|f| !f.contains("rke2")));
        let rke2 = plan(&index, &config("distribution: rke2\n"), &facts()).unwrap();
        assert_eq!(rke2.calico, None);
        assert_eq!(rke2.cluster.install_script, None);
        assert!(rke2.host_file_paths.iter().all(|f| !f.contains("k3s")));
    }

    #[test]
    fn smoke_tests_that_make_objects_are_left_to_ci() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let p = plan(&index, &config(""), &facts()).unwrap();
        let smoke: Vec<&Vec<String>> = p.components.iter().flat_map(|c| &c.smoke).collect();
        assert_ne!(smoke.len(), 0);
        assert!(
            smoke
                .iter()
                .all(|w| !w.iter().any(|a| a == "catalog-smoke")),
            "{smoke:?}"
        );
    }

    #[test]
    fn baseline_namespace_is_labelled_baseline() {
        let ns = Namespace {
            name: "k8up-system".to_owned(),
            privileged: None,
            baseline: Some("x".to_owned()),
            public: false,
        };
        let object = namespace_object(&ns);
        for mode in ["enforce", "audit", "warn"] {
            assert_eq!(
                object["metadata"]["labels"][format!("pod-security.kubernetes.io/{mode}")],
                "baseline"
            );
        }
    }

    #[test]
    fn privileged_namespaces_and_policies() {
        let ns = Namespace {
            name: "observability".to_owned(),
            privileged: Some("x".to_owned()),
            baseline: Some("y".to_owned()),
            public: false,
        };
        let object = namespace_object(&ns);
        assert_eq!(
            object["metadata"]["labels"]["pod-security.kubernetes.io/enforce"],
            "privileged"
        );
        let policy = network_policy(
            "observability",
            &["10.0.0.5".to_owned(), "fd00::5".to_owned()],
        );
        assert_eq!(object["metadata"]["labels"][BASELINE_LABEL], "true");
        let from = &policy["spec"]["ingress"][0]["from"];
        assert_eq!(
            from[1]["namespaceSelector"]["matchLabels"],
            json!({BASELINE_LABEL: "true"})
        );
        assert_eq!(from[2]["ipBlock"]["cidr"], "10.0.0.5/32");
        assert_eq!(from[3]["ipBlock"]["cidr"], "fd00::5/128");
        assert_eq!(
            policy_sources(&policy).into_iter().collect::<Vec<_>>(),
            ["10.0.0.5", "fd00::5"]
        );
    }

    #[test]
    fn node_traffic_sources() {
        let nodes = json!({"items": [
            {"spec": {"podCIDRs": ["10.42.0.0/24"]},
             "status": {"addresses": [{"type": "InternalIP", "address": "10.0.0.5"},
                                      {"type": "Hostname", "address": "vps-1"}]}},
            {"spec": {"podCIDRs": ["10.42.1.0/24", "fd00:42:0:1::/64"]},
             "status": {"addresses": [{"type": "InternalIP", "address": "10.0.0.6"}]}},
        ]});
        assert_eq!(
            node_sources(&nodes),
            ["10.42.0.0", "10.42.1.0", "fd00:42:0:1::"]
        );
    }

    #[test]
    fn products_only_are_what_the_plan_installs() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let none = "disable: [logs/victoria-logs, metrics/victoria-metrics, builds]\n";
        let p = plan(&index, &config(none), &facts()).unwrap();
        assert_eq!(
            products_only(&config(none), &facts()).unwrap(),
            p.identity.zip(p.zeabur)
        );
    }

    #[test]
    fn zeabur_values_match_the_chart() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let p = plan(&index, &config(""), &facts()).unwrap();
        let values = &p.zeabur.unwrap().values;
        assert_eq!(
            values["image"],
            json!({"repository": "zeabur", "tag": "dev", "pullPolicy": "IfNotPresent"})
        );
        assert_eq!(values["issuer"]["url"], "https://login.example.com");
        assert_eq!(
            values["ingress"]["annotations"]["cert-manager.io/cluster-issuer"],
            "letsencrypt"
        );
        assert_eq!(values["domains"]["clusterIssuer"], "letsencrypt");
        assert_eq!(values["domains"]["suffixes"], json!([]));
    }

    #[test]
    fn zeabur_reads_history_through_its_proxy_when_a_store_is_installed() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let values = |disable: &str| {
            plan(&index, &config(disable), &facts())
                .unwrap()
                .zeabur
                .unwrap()
                .values
        };
        let both = values("");
        assert_eq!(both["proxy"]["enabled"], true);
        assert_eq!(both["proxy"]["logs"]["enabled"], true);
        let only_metrics = values("disable: [logs/victoria-logs]\n");
        assert_eq!(only_metrics["proxy"]["enabled"], true);
        assert_eq!(only_metrics["proxy"]["logs"]["enabled"], false);
        let only_logs = values("disable: [metrics/victoria-metrics]\n");
        assert_eq!(only_logs["proxy"]["enabled"], true);
        assert_eq!(only_logs["proxy"]["logs"]["enabled"], true);
        let neither = values("disable: [logs/victoria-logs, metrics/victoria-metrics]\n");
        assert!(neither.get("proxy").is_none());
    }

    #[test]
    fn zeabur_trusts_zots_ca_only_when_builder_is_installed() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let without = plan(&index, &config("disable: [builds]\n"), &facts()).unwrap();
        assert!(without.zeabur.unwrap().values.get("builds").is_none());
        let with = plan(&index, &config(""), &facts()).unwrap();
        assert_eq!(
            with.zeabur.unwrap().values["builds"]["caBundle"]["configMap"],
            REGISTRY_CA_CONFIGMAP
        );
    }

    #[test]
    fn servicelb_pods_alone_take_outside_traffic() {
        for (distribution, label) in [
            (Distribution::Rke2, "svccontroller.rke2.cattle.io/svcname"),
            (Distribution::K3s, "svccontroller.k3s.cattle.io/svcname"),
        ] {
            let policy = servicelb_policy(distribution);
            assert_eq!(policy["metadata"]["namespace"], "kube-system");
            assert_eq!(
                policy["spec"]["podSelector"],
                json!({"matchExpressions": [{"key": label, "operator": "Exists"}]}),
                "only ServiceLB pods"
            );
            assert_eq!(policy["spec"]["policyTypes"], json!(["Ingress"]));
            assert_eq!(policy["spec"]["ingress"], json!([{}]), "from any source");
        }
    }

    #[test]
    fn zot_admits_builder_and_build_jobs_on_its_port() {
        let policy = zot_builder_policy();
        let rule = &policy["spec"]["ingress"][0];
        let builder = json!({"kubernetes.io/metadata.name": "builder-system"});
        let from = rule["from"].as_array().unwrap();
        for pods in ["buildkitd", "zeabur-builder"] {
            assert!(
                from.contains(&json!({
                    "namespaceSelector": {"matchLabels": builder},
                    "podSelector": {"matchLabels": {"app.kubernetes.io/name": pods}},
                })),
                "{pods}"
            );
        }
        assert!(from.contains(&json!({
            "namespaceSelector": {"matchLabels": {"zeabur.com/project": "true"}},
            "podSelector": {"matchExpressions": [{"key": BUILD_RUN_LABEL, "operator": "Exists"}]},
        })));
        assert!(
            from.iter().all(|peer| peer.get("ipBlock").is_none()
                && peer.get("namespaceSelector").is_some()
                && peer.get("podSelector").is_some()),
            "every peer names its namespaces and its pods"
        );
        assert_eq!(rule["ports"], json!([{"port": 5000, "protocol": "TCP"}]));
    }

    #[test]
    fn suffixes_default_to_sslip_for_a_public_address() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let mut public = facts();
        public.node_ips = vec!["203.0.114.7".to_owned(), "2a01:4f8::1".to_owned()];
        let p = plan(&index, &config(""), &public).unwrap();
        assert_eq!(
            p.zeabur.unwrap().values["domains"]["suffixes"],
            json!(["203-0-114-7.sslip.io"])
        );
        assert!(p.warnings.iter().all(|w| !w.code.starts_with("domains.")));

        let p = plan(&index, &config(""), &facts()).unwrap();
        assert!(p.warnings.iter().any(|w| w.code == "domains.none"));

        let mut given = config("");
        given.access.suffixes = Some(vec!["apps.example.com".to_owned()]);
        let p = plan(&index, &given, &facts()).unwrap();
        assert_eq!(
            p.zeabur.unwrap().values["domains"]["suffixes"],
            json!(["apps.example.com"])
        );
    }

    #[test]
    fn public_addresses() {
        for a in ["203.0.114.7", "2a01:4f8::1"] {
            assert!(is_public(a), "{a}");
        }
        for a in [
            "10.0.0.5",
            "192.168.1.2",
            "100.64.0.1",
            "127.0.0.1",
            "fd00::5",
            "2001:db8::1",
            "x",
        ] {
            assert!(!is_public(a), "{a}");
        }
        assert_eq!(sslip_suffix("2a01:4f8::1"), "2a01-4f8--1.sslip.io");
    }

    #[test]
    fn products_come_from_sources_or_else_the_pinned_releases() {
        let pinned = pins::Products {
            catalog: pins::Catalog {
                version: "v0.1.0",
                index_sha256: "aa",
            },
            identity: pins::Identity {
                version: "v0.2.0",
                image: "ghcr.io/zeabur-oss/identity@sha256:bb",
                identityctl_amd64: "cc",
                identityctl_arm64: "dd",
            },
            zeabur: pins::Zeabur {
                version: "v0.3.0",
                image: "ghcr.io/zeabur-oss/zeabur@sha256:ee",
                chart_sha256: "ff",
            },
        };
        let only_access = Config::parse(
            "access: {mode: domain, identity: login.example.com, zeabur: zeabur.example.com}\n\
             sources: {zeaburImage: zeabur:dev}",
        )
        .unwrap();
        let products = |pins| {
            products(
                &only_access,
                &facts(),
                pins,
                &BTreeSet::new(),
                &mut BTreeMap::new(),
                &mut Vec::new(),
            )
        };

        let (identity, zeabur) = products(Some(&pinned)).unwrap();
        let (identity, zeabur) = (identity.unwrap(), zeabur.unwrap());
        assert_eq!(identity.image, "ghcr.io/zeabur-oss/identity@sha256:bb");
        assert_eq!(
            identity.identityctl,
            Source::Pinned(Download {
                file: "zeabur-identityctl-v0.2.0-linux-arm64.tar.gz".to_owned(),
                url: "https://github.com/zeabur-oss/identity/releases/download/v0.2.0/\
                      zeabur-identityctl-v0.2.0-linux-arm64.tar.gz"
                    .to_owned(),
                sha256: Some("dd".to_owned()),
            })
        );
        assert_eq!(
            zeabur.chart,
            Source::Pinned(Download {
                file: "zeabur-0.3.0.tgz".to_owned(),
                url:
                    "https://github.com/zeabur-oss/zeabur/releases/download/v0.3.0/zeabur-0.3.0.tgz"
                        .to_owned(),
                sha256: Some("ff".to_owned()),
            })
        );
        assert_eq!(
            zeabur.values["image"],
            json!({"repository": "zeabur", "tag": "dev", "pullPolicy": "IfNotPresent"})
        );

        let Err(PlanError::Unpublished(keys)) = products(None) else {
            panic!("a development build needs sources");
        };
        assert_eq!(keys, ["identityImage", "identityctl", "zeaburChart"]);
    }

    #[test]
    fn a_team_install_needs_the_least_memory() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        for (distribution, extra) in [
            (Distribution::K3s, ""),
            (Distribution::Rke2, "distribution: rke2\n"),
        ] {
            let mut small = facts();
            small.memory_mib = distribution.min_memory_mib();
            assert!(plan(&index, &config(extra), &small).is_ok());
            small.memory_mib -= 1;
            let Err(error) = plan(&index, &config(extra), &small) else {
                panic!("{} MiB is too little for a team install", small.memory_mib);
            };
            assert!(error.to_string().contains("use: personal"), "{error}");
            assert!(error.to_string().contains(distribution.title()), "{error}");
        }
    }

    #[test]
    fn the_standard_preset_fits_a_4_gib_server_without_a_warning() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let mut host = facts();
        host.memory_mib = 3379;
        for extra in ["", "distribution: rke2\n"] {
            let plan = plan(&index, &config(extra), &host).unwrap();
            let idle = plan.memory.cluster_mib + plan.memory.components_mib;
            assert!(idle < host.memory_mib * 4 / 5, "{idle} MiB");
            assert!(plan.warnings.iter().all(|w| w.code != "memory.tight"));
        }
    }

    #[test]
    fn node_sources_are_the_vxlan_addresses_of_calico_or_canal() {
        let nodes = json!({"items": [
            {
                "metadata": {"annotations": {
                    "projectcalico.org/IPv4VXLANTunnelAddr": "10.42.8.64",
                }},
                "spec": {"podCIDRs": ["10.42.0.0/24"]},
            },
            {"metadata": {}, "spec": {"podCIDRs": ["10.42.1.0/24", "fd00:42:1::/64"]}},
        ]});
        assert_eq!(
            node_sources(&nodes),
            ["10.42.1.0", "10.42.8.64", "fd00:42:1::"]
        );
    }

    #[test]
    fn ipv4_network_is_the_interface_subnet() {
        assert_eq!(
            ipv4_network("192.0.2.168", Some(24)).as_deref(),
            Some("192.0.2.0/24")
        );
        assert_eq!(
            ipv4_network("192.0.2.121", Some(20)).as_deref(),
            Some("192.0.0.0/20")
        );
        assert_eq!(
            ipv4_network("10.0.0.5", None).as_deref(),
            Some("10.0.0.5/32")
        );
        assert_eq!(ipv4_network("fd00::6", Some(64)), None);
    }
}
