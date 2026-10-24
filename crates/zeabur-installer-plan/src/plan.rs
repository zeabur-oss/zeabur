//! The install plan: everything `apply` will do, computed before the host is touched.
//!
//! The same configuration, catalog and host facts give the same plan, byte for byte, so `plan`
//! can be reviewed, diffed and tested. Secret values are never part of it: a plan names the
//! Secret key and where its value comes from.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Value, json};

use crate::catalog::{Component, Index, SecretRef};
use crate::config::{AccessMode, Config};
use crate::pins::{self, RKE2};
use crate::rke2::{self, HostFile};
use crate::select::{self, Why};
use crate::storage::{self, NodePrep, Storage};
use crate::{PlanError, values};

/// The label and field manager on everything the installer creates.
pub const MANAGER: &str = "zeabur-installer";
/// The label the baseline policies let traffic in from. `app.kubernetes.io/managed-by` cannot be
/// it: a product that labels its own namespace, as Zeabur Identity does, takes that one over.
pub const BASELINE_LABEL: &str = "installer.zeabur.com/baseline";
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
/// The `ClusterIssuer` of `domain` access, shipped by the catalog's cert-manager component.
pub const ACME_ISSUER: &str = "letsencrypt";
/// RKE2's own memory at idle on one node, measured on the test VM (`kubectl top` plus containerd
/// and the rke2 process), rounded up.
const RKE2_IDLE_MIB: u64 = 1300;

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
    /// Memory in MiB.
    pub memory_mib: u64,
    /// The addresses the node registers with (RKE2's `node-ip`); empty leaves the choice to RKE2.
    pub node_ips: Vec<String>,
}

/// The whole plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Facts it was computed from.
    pub facts: Facts,
    /// The RKE2 release.
    pub rke2: Rke2Plan,
    /// Files written before RKE2 starts.
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
    /// The storage classes, and which one is the default.
    pub storage: Storage,
    /// What every node needs before the components' pods run on it.
    pub node_prep: Option<NodePrep>,
    /// How the two web services get certificates.
    pub access: AccessMode,
    /// Zeabur Identity, when installed.
    pub identity: Option<IdentityPlan>,
    /// Zeabur, when installed.
    pub zeabur: Option<ZeaburPlan>,
    /// Idle memory: RKE2 plus the measured components.
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

/// The RKE2 release and its downloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rke2Plan {
    /// Release tag.
    pub version: String,
    /// Whether the digests come from this installer (`true`) or the release's own list.
    pub pinned: bool,
    /// File name, URL and pinned SHA-256 (empty when unpinned).
    pub downloads: Vec<Download>,
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

/// A namespace and its Pod Security level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Namespace {
    /// Name.
    pub name: String,
    /// Why it is `privileged` instead of RKE2's default `restricted`.
    pub privileged: Option<String>,
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
    /// 64 random bytes as 128 hexadecimal digits, made once.
    Generate,
    /// A file, mode 0600, read at apply time.
    File(String),
}

/// One release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ReleasePlan {
    /// A `HelmChart` object RKE2's helm-controller installs and keeps installed.
    HelmChart {
        /// Release name.
        name: String,
        /// The object.
        object: Value,
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
    /// The `zeabur-identityctl` binary.
    pub identityctl: String,
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
    /// The chart: a local directory or `.tgz`, or an OCI reference.
    pub chart: String,
    /// Helm values, except the generated Secrets' names, which are fixed.
    pub values: Value,
}

/// Memory at idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    /// RKE2 itself.
    pub rke2_mib: u64,
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
    let selection = select::select(index, config)?;
    let profile = config.profile.as_str();
    let chosen: BTreeSet<&str> = selection
        .components
        .iter()
        .map(|(id, _)| id.as_str())
        .collect();
    let mut warnings = Vec::new();

    let rke2 = rke2_plan(config.rke2_version.as_deref(), &facts.arch, &mut warnings)?;
    if !index
        .kubernetes
        .iter()
        .any(|v| v.starts_with(&format!("{}.", RKE2.kubernetes)))
    {
        let tested = index.kubernetes.join(", ");
        warnings.push(warning(
            "kubernetes.untested",
            &[RKE2.kubernetes, &tested],
            format!(
                "The catalog was not tested on Kubernetes {}; it lists {tested}.",
                RKE2.kubernetes
            ),
        ));
    }

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
            config,
            profile,
            &facts.arch,
        )?);
        let privileged = privileged(component);
        for release in &component.releases {
            if release.namespace == "kube-system" {
                continue;
            }
            let entry = namespaces
                .entry(release.namespace.clone())
                .or_insert_with(|| Namespace {
                    name: release.namespace.clone(),
                    privileged: None,
                    public: component.capability == "ingress",
                });
            if entry.privileged.is_none() {
                entry.privileged.clone_from(&privileged);
            }
        }
    }

    let tls_issuer = match config.access.mode {
        AccessMode::Domain => ACME_ISSUER,
        AccessMode::Local => LOCAL_ISSUER,
    };
    if !chosen.contains("certificates/cert-manager") && config.zeabur {
        return Err(PlanError::NeedsDisabled {
            component: "zeabur".to_owned(),
            dependency: "certificates/cert-manager".to_owned(),
        });
    }

    let (identity, zeabur) = products(
        config,
        facts,
        &chosen,
        tls_issuer,
        &mut namespaces,
        &mut warnings,
    )?;

    let host_files = server_files(config, &chosen, facts);

    let memory = memory(&components, facts.memory_mib, &mut warnings);

    Ok(Plan {
        storage: storage_plan(&components, profile, &mut warnings),
        facts: facts.clone(),
        rke2,
        host_file_paths: host_files.iter().map(|f| f.path.clone()).collect(),
        host_files,
        namespaces: namespaces.into_values().collect(),
        components,
        left_out: selection.left_out,
        node_prep: storage::node_prep(chosen.iter().copied()),
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

/// RKE2's packaged components the catalog replaces.
fn packaged_to_disable(chosen: &BTreeSet<&str>) -> Vec<String> {
    chosen
        .contains("metrics/metrics-server")
        .then(|| "rke2-metrics-server".to_owned())
        .into_iter()
        .collect()
}

/// Zeabur Identity and Zeabur, and their namespaces.
fn products(
    config: &Config,
    facts: &Facts,
    chosen: &BTreeSet<&str>,
    tls_issuer: &str,
    namespaces: &mut BTreeMap<String, Namespace>,
    warnings: &mut Vec<Warning>,
) -> Result<(Option<IdentityPlan>, Option<ZeaburPlan>), PlanError> {
    if !config.zeabur {
        return Ok((None, None));
    }
    for name in [IDENTITY_NAMESPACE, ZEABUR_NAMESPACE] {
        namespaces.insert(
            name.to_owned(),
            Namespace {
                name: name.to_owned(),
                privileged: None,
                public: false,
            },
        );
    }
    let image = config
        .sources
        .identity_image
        .clone()
        .ok_or(PlanError::Unpublished("identityImage"))?;
    let identityctl = config
        .sources
        .identityctl
        .clone()
        .ok_or(PlanError::Unpublished("identityctl"))?;
    let chart = config
        .sources
        .zeabur_chart
        .clone()
        .ok_or(PlanError::Unpublished("zeaburChart"))?;
    let zeabur_image = config
        .sources
        .zeabur_image
        .clone()
        .ok_or(PlanError::Unpublished("zeaburImage"))?;
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
    if !chosen.contains("metrics/victoria-metrics") {
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
    let values = zeabur_values(config, facts, tls_issuer, &zeabur_image);
    Ok((
        Some(identity),
        Some(ZeaburPlan {
            url: config.zeabur_url(),
            chart,
            values,
        }),
    ))
}

/// Idle memory, and a warning when the host is short of it.
fn memory(components: &[ComponentPlan], host_mib: u64, warnings: &mut Vec<Warning>) -> Memory {
    let unmeasured = components
        .iter()
        .filter(|c| c.idle_memory_mib.is_none())
        .count();
    let memory = Memory {
        rke2_mib: RKE2_IDLE_MIB,
        components_mib: components.iter().filter_map(|c| c.idle_memory_mib).sum(),
        unmeasured,
        host_mib,
    };
    if host_mib < 3800 {
        warnings.push(warning(
            "memory.low",
            &[&host_mib.to_string()],
            format!("RKE2 asks for at least 4096 MiB of memory; this host has {host_mib} MiB."),
        ));
    } else if memory.rke2_mib + memory.components_mib > host_mib * 3 / 4 {
        let idle = memory.rke2_mib + memory.components_mib;
        warnings.push(warning(
            "memory.tight",
            &[&idle.to_string(), &host_mib.to_string()],
            format!("At idle, RKE2 and the components use about {idle} MiB of this host's {host_mib} MiB."),
        ));
    }
    memory
}

pub(crate) fn rke2_plan(
    rke2_version: Option<&str>,
    arch: &str,
    warnings: &mut Vec<Warning>,
) -> Result<Rke2Plan, PlanError> {
    let files = RKE2
        .files(arch)
        .ok_or_else(|| PlanError::Arch(arch.to_owned()))?;
    let (version, pinned) = match rke2_version {
        Some(v) if v != RKE2.version => {
            warnings.push(warning(
                "rke2.unsupported",
                &[v],
                format!(
                    "RKE2 {v} is not supported by this installer release: its downloads are \
                     checked only against the release's own checksum list, and the catalog may \
                     not have been tested on it."
                ),
            ));
            (v.to_owned(), false)
        }
        Some(_) | None => (RKE2.version.to_owned(), true),
    };
    let named = [
        (format!("rke2.linux-{arch}.tar.gz"), files.tarball),
        (
            format!("rke2-images-core.linux-{arch}.tar.zst"),
            files.images_core,
        ),
        (
            format!("rke2-images-canal.linux-{arch}.tar.zst"),
            files.images_canal,
        ),
    ];
    let mut downloads: Vec<Download> = named
        .into_iter()
        .map(|(file, sha)| Download {
            url: pins::rke2_url(&version, &file),
            sha256: pinned.then(|| sha.to_owned()),
            file,
        })
        .collect();
    if !pinned {
        let file = format!("sha256sum-{arch}.txt");
        downloads.insert(
            0,
            Download {
                url: pins::rke2_url(&version, &file),
                sha256: None,
                file,
            },
        );
    }
    Ok(Rke2Plan {
        version,
        pinned,
        downloads,
    })
}

fn component_plan(
    component: &Component,
    why: Why,
    contributors: &[&Component],
    config: &Config,
    profile: &str,
    arch: &str,
) -> Result<ComponentPlan, PlanError> {
    let id = component.id.as_str();
    let (secrets, paths) = inputs(component, config)?;

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
                skip_if_present: release.skip_if_present.clone(),
            });
        }
    }

    Ok(ComponentPlan {
        id: id.to_owned(),
        display_name: component.display_name.clone(),
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

fn inputs(component: &Component, config: &Config) -> Result<Inputs, PlanError> {
    let id = component.id.as_str();
    let first_chart = component
        .releases
        .iter()
        .find(|r| r.chart.is_some())
        .map(|r| r.name.clone());
    let given = config.inputs.get(id);
    let files = config.secret_files.get(id);
    let mut secrets = Vec::new();
    let mut paths: Vec<(String, String, Value)> = Vec::new();
    for input in &component.inputs {
        if let Some(secret) = &input.secret {
            let source = match files.and_then(|f| f.get(&input.name)) {
                Some(path) => SecretSource::File(path.clone()),
                None if input.generate.as_deref() == Some("random") => SecretSource::Generate,
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
                .or_else(|| input.default.clone());
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
    Ok((secrets, paths))
}

/// Why a component's namespaces need the `privileged` Pod Security level, if they do.
fn privileged(component: &Component) -> Option<String> {
    let id = &component.id;
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

/// The first server's RKE2 files.
fn server_files(config: &Config, chosen: &BTreeSet<&str>, facts: &Facts) -> Vec<HostFile> {
    let mut files = rke2::host_files(&rke2::Options {
        tls_san: config.host.tls_san.clone(),
        disable: packaged_to_disable(chosen),
        identity: config.zeabur,
    });
    files.extend(rke2::node_ip_file(&facts.node_ips));
    files
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
    if namespace.privileged.is_some()
        && let Some(map) = labels.as_object_mut()
    {
        for mode in ["enforce", "audit", "warn"] {
            map.insert(
                format!("pod-security.kubernetes.io/{mode}"),
                json!("privileged"),
            );
        }
    }
    let mut metadata = json!({"name": namespace.name, "labels": labels});
    if let (Some(reason), Some(map)) = (&namespace.privileged, metadata.as_object_mut()) {
        map.insert(
            "annotations".to_owned(),
            json!({"installer.zeabur.com/privileged": reason}),
        );
    }
    json!({"apiVersion": "v1", "kind": "Namespace", "metadata": metadata})
}

/// The addresses node traffic comes from, read from `kubectl get nodes -o json`.
///
/// Each node's `InternalIP`, and the first address of each pod range, which a node's traffic to
/// pods on other nodes leaves from (Canal's VXLAN interface). Sorted and without repeats.
#[must_use]
pub fn node_sources(nodes: &Value) -> Vec<String> {
    let mut found = BTreeSet::new();
    for node in nodes
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for address in node
            .pointer("/status/addresses")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if address.get("type").and_then(Value::as_str) == Some("InternalIP")
                && let Some(a) = address.get("address").and_then(Value::as_str)
            {
                found.insert(a.to_owned());
            }
        }
        let cidrs = node
            .pointer("/spec/podCIDRs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str);
        for cidr in cidrs {
            if let Some((base, _)) = cidr.split_once('/') {
                found.insert(base.to_owned());
            }
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
/// [`BASELINE_LABEL`]; the ingress and the metrics collectors among them) and from the nodes
/// (kube-apiserver calling webhooks, kubelet probes).
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

/// The chart values of docs/install.md, with the Secrets the installer creates.
fn zeabur_values(config: &Config, facts: &Facts, issuer: &str, image: &str) -> Value {
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
        "domains": {"clusterIssuer": issuer},
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
            memory_mib: 8192,
            node_ips: vec!["10.0.0.5".to_owned()],
        }
    }

    fn ids(plan: &Plan) -> Vec<&str> {
        plan.components.iter().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn standard_follows_the_catalog_and_leaves_out_k3s_only_components() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let plan = plan(&index, &config(""), &facts()).unwrap();
        assert_eq!(
            ids(&plan),
            [
                "ingress/traefik",
                "certificates/cert-manager",
                "observability-operator/victoria-metrics-operator",
                "logs/victoria-logs",
                "metrics/metrics-server",
                "metrics/victoria-metrics",
            ]
        );
        assert_eq!(
            &plan.left_out["node-updates/system-upgrade-controller"],
            "runs on k3s only"
        );
        assert!(plan.rke2.pinned);
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
        assert_eq!(crowdsec.secrets.len(), 3);
        assert!(
            crowdsec
                .secrets
                .iter()
                .all(|s| s.source == SecretSource::Generate)
        );
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

    #[test]
    fn required_inputs_name_the_line_to_add() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let err = plan(&index, &config("enable: [dns]\n"), &facts())
            .unwrap_err()
            .to_string();
        assert!(err.contains("inputs.dns/external-dns.provider"), "{err}");
        let ok = plan(
            &index,
            &config(
                "enable: [dns]\ninputs:\n  dns/external-dns:\n    provider: cloudflare\n    \
                 domain-filters: [example.com]\n    owner-id: vps-1\n",
            ),
            &facts(),
        )
        .unwrap();
        let dns = ok
            .components
            .iter()
            .find(|c| c.id == "dns/external-dns")
            .unwrap();
        let ReleasePlan::HelmChart { object, .. } = &dns.releases[0] else {
            panic!()
        };
        assert!(
            object["spec"]["valuesContent"]
                .as_str()
                .unwrap()
                .contains(r#""domainFilters":["example.com"]"#)
        );
    }

    #[test]
    fn another_rke2_version_is_a_warning_not_an_error() {
        let index = Index::parse(INDEX.as_bytes()).unwrap();
        let p = plan(&index, &config("rke2Version: v1.37.1+rke2r1\n"), &facts()).unwrap();
        assert!(!p.rke2.pinned);
        assert!(p.rke2.downloads.iter().all(|d| d.sha256.is_none()));
        assert_eq!(p.rke2.downloads[0].file, "sha256sum-arm64.txt");
        assert!(p.warnings.iter().any(|w| w.code == "rke2.unsupported"));
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
    fn privileged_namespaces_and_policies() {
        let ns = Namespace {
            name: "observability".to_owned(),
            privileged: Some("x".to_owned()),
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
            [
                "10.0.0.5",
                "10.0.0.6",
                "10.42.0.0",
                "10.42.1.0",
                "fd00:42:0:1::"
            ]
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
    }
}
