//! The parts of a Zeabur catalog release (`index.json`) the installer reads.
//!
//! The catalog is data, not an installer: it says which upstream components fill which capability,
//! their charts and values per profile, the inputs to ask for and the order to install them in.
//! Unknown fields are ignored, because the index only changes `indexVersion` on incompatible
//! changes, and a value it does not know is read as the catalog's `docs/reference/releases.mdx`
//! says.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{PlanError, kube_version};

/// The only `indexVersion` this installer understands.
pub const INDEX_VERSION: u64 = 3;

/// A parsed `index.json`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    /// Incompatible changes bump it.
    #[expect(clippy::struct_field_names, reason = "the catalog's field name")]
    pub index_version: u64,
    /// `single-node`, `multi-node`.
    pub profiles: Vec<String>,
    /// Kubernetes versions the catalog was tested on.
    #[serde(default)]
    pub kubernetes: Vec<String>,
    /// Every component.
    pub components: Vec<Component>,
}

/// One upstream component filling one capability.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// `<capability>/<upstream>`.
    pub id: String,
    /// The capability it fills.
    pub capability: String,
    /// Its name for people.
    pub metadata: Metadata,
    /// `recommended`, `optional` or `unsupported`, per profile.
    pub defaults: BTreeMap<String, Default>,
    /// Distributions it works on. A distribution this installer does not know matches nothing.
    pub distributions: Vec<String>,
    /// The Kubernetes versions it supports, as a Helm chart's `kubeVersion` (`>=1.34.0-0`).
    pub kube_version: String,
    /// Where the catalog's CI installed it and passed.
    pub tested_on: Vec<TestedOn>,
    /// Releases installed in this order.
    pub releases: Vec<Release>,
    /// Values asked for or generated before installing.
    #[serde(default)]
    pub inputs: Vec<Input>,
    /// Values merged into another component's release.
    #[serde(default)]
    pub contributes: Vec<Contribution>,
    /// What the component can reach and change.
    pub security: Security,
    /// Checks that it works once installed, which `apply` waits for and `doctor` runs.
    #[serde(default)]
    pub health: Vec<Health>,
    /// The components to install first, then this one, per profile.
    #[serde(default)]
    pub install_order: BTreeMap<String, Vec<String>>,
    /// What the catalog's CI measured in each environment where its test passed.
    #[serde(default)]
    pub measured: Vec<Measured>,
    /// It keeps data of its own.
    #[serde(default)]
    pub persists_data: bool,
    /// What `helm uninstall` leaves: `customresourcedefinitions`, `persistentvolumeclaims` or
    /// `<resource>/<name>`.
    #[serde(default)]
    pub leftovers: Vec<String>,
    /// How to find it in a cluster.
    #[serde(default)]
    pub detection: Detection,
    /// What it serves other components and the installer.
    #[serde(default)]
    pub provides: Provides,
    /// CRDs of [`Detection::crds`] whose objects it writes itself, so they are no one's data.
    #[serde(default, rename = "internalCRDs")]
    pub internal_crds: Vec<String>,
    /// What it needs beyond its measured idle use.
    #[serde(default)]
    pub resources: Resources,
    /// `NetworkPolicy` manifests that let its clients in, for installers that isolate
    /// namespaces.
    pub network_policies: Option<String>,
}

/// What a component serves.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Provides {
    /// An OCI registry inside the cluster that nodes pull from.
    pub registry: Option<Registry>,
}

/// What a component needs beyond its measured idle use.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Resources {
    /// Each kind of work it does, such as a build.
    #[serde(default)]
    pub work: Vec<Work>,
}

/// One kind of work a component does.
#[derive(Debug, Clone, Deserialize)]
pub struct Work {
    /// Unique in the component.
    pub name: String,
    /// What each pod that does one unit of the work requests.
    #[serde(default)]
    pub requests: Amounts,
    /// What the component uses on top of its idle use while it does one unit of the work.
    #[serde(default, rename = "use")]
    pub uses: Amounts,
}

/// Memory and CPU. The catalog leaves out an amount that is zero or not given.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct Amounts {
    /// Memory in MiB.
    #[serde(rename = "memoryMiB")]
    pub memory_mib: Option<u64>,
    /// CPU in millicores.
    #[serde(rename = "cpuMillis")]
    pub cpu_millis: Option<u64>,
}

impl Amounts {
    /// The memory, 0 when the catalog leaves it out.
    #[must_use]
    pub fn memory(self) -> u64 {
        self.memory_mib.unwrap_or(0)
    }
}

impl Resources {
    /// What one unit of each kind of work needs on top of its idle use: what it uses, and no
    /// less than its pod asks the scheduler for. `None` when it does no work of its own.
    #[must_use]
    pub fn work_mib(&self) -> Option<u64> {
        (!self.work.is_empty()).then(|| {
            self.work
                .iter()
                .map(|w| w.uses.memory().max(w.requests.memory()))
                .sum()
        })
    }
}

/// A distribution and the node platforms where the catalog's CI passed on it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TestedOn {
    /// `k3s`, `rke2` or `kubesolo`.
    pub distribution: String,
    /// `<os>/<architecture>`, such as `linux/amd64`.
    pub platforms: Vec<String>,
}

/// A component by its id.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ComponentRef {
    /// `<capability>/<upstream>`.
    pub component: String,
}

/// An in-cluster registry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    /// `<service>.<namespace>.svc:<port>`, the name in its certificate and in image references.
    pub host: String,
    /// The Secret key with the CA its certificate chains to.
    pub ca: SecretRef,
}

impl Registry {
    /// The Service name and namespace in [`Registry::host`].
    #[must_use]
    pub fn service(&self) -> (&str, &str) {
        let name = self.host.split(':').next().unwrap_or_default();
        let mut parts = name.split('.');
        (
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
        )
    }
}

/// The part of a component's `detection` the installer reads.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    /// The CRDs it serves, as `<plural>.<group>`.
    #[serde(default)]
    pub crds: Vec<String>,
    /// The CSI drivers of the `PersistentVolume`s it provides.
    #[serde(default)]
    pub csi_drivers: Vec<String>,
}

/// The part of a component's `metadata` the installer reads.
#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    /// Its name for people.
    pub name: String,
}

/// How a component is offered on a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Default {
    /// Installed unless left out.
    Recommended,
    /// Installed when asked for.
    Optional,
    /// Never installed on this profile, and what an unknown value means.
    #[serde(other)]
    Unsupported,
}

/// How a release installs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseType {
    /// A Helm chart: `chart`, `version`, `chartDigest`, `repo` for an HTTP repository.
    Helm,
    /// Files downloaded from `urls`.
    Manifest,
    /// The objects in `manifests`.
    Inline,
    /// A type this installer does not know: the component is not offered.
    #[serde(other)]
    Unknown,
}

/// One Helm chart, one set of manifests or one downloaded manifest file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// Unique within the component.
    pub name: String,
    /// How it installs.
    #[serde(rename = "type")]
    pub kind: ReleaseType,
    /// The namespace it installs into.
    pub namespace: String,
    /// The Helm release name, when it differs from `name`.
    #[expect(clippy::struct_field_names, reason = "the catalog's field name")]
    pub release_name: Option<String>,
    /// Skip the release when one of these CRDs already exists.
    #[serde(default)]
    pub skip_if_present: Vec<String>,
    /// A chart reference: a name in `repo`, or an `oci://` reference.
    pub chart: Option<String>,
    /// A Helm repository URL.
    pub repo: Option<String>,
    /// Manifest URLs with a `{version}` placeholder.
    #[serde(default)]
    pub urls: Vec<String>,
    /// The chart or manifest version.
    pub version: Option<String>,
    /// `sha256:<hex>` of the chart: of the `.tgz` from `repo`, or of an `oci://` chart's manifest.
    pub chart_digest: Option<String>,
    /// `sha256:<hex>` of each file in `urls`, in the same order.
    #[serde(default)]
    pub url_digests: Vec<String>,
    /// Inline manifests.
    pub manifests: Option<String>,
    /// Values shared by every profile, as YAML.
    pub values: Option<String>,
    /// Values per profile, merged over `values`.
    pub profiles: Option<BTreeMap<String, String>>,
}

impl Release {
    /// The Helm release name.
    #[must_use]
    pub fn release_name(&self) -> &str {
        self.release_name.as_deref().unwrap_or(&self.name)
    }
}

/// A value the component needs before it installs.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Input {
    /// Unique within the component.
    pub name: String,
    /// What it is, for the person asked.
    pub description: String,
    /// The release whose values receive it; the first chart release when absent.
    pub release: Option<String>,
    /// A dotted path in the release's values; numeric segments index lists.
    pub path: Option<String>,
    /// Or a Secret key it is stored in.
    pub secret: Option<SecretRef>,
    /// How the installer makes the value when none is given.
    pub generate: Option<Generate>,
    /// Installing fails without it.
    #[serde(default)]
    pub required: bool,
    /// Used when nothing is given.
    pub default: Option<Value>,
}

/// How the installer makes an input's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "type")]
pub enum Generate {
    /// `bytes` random bytes in `encoding`.
    #[serde(rename = "random")]
    Random {
        /// 12 to 64.
        bytes: usize,
        /// How they are written.
        encoding: Encoding,
    },
    /// A free address in the Service CIDR.
    #[serde(rename = "clusterIP")]
    ClusterIp,
    /// A type this installer does not know: the input is not filled in.
    #[serde(other)]
    Unknown,
}

/// How generated bytes are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    /// Two hexadecimal digits per byte.
    Hex,
    /// URL-safe base64 without padding.
    Base64url,
}

/// A Secret key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct SecretRef {
    /// Secret name.
    pub name: String,
    /// Key in the Secret.
    pub key: String,
    /// Its namespace.
    pub namespace: String,
}

/// Values one component adds to another's release.
#[derive(Debug, Clone, Deserialize)]
pub struct Contribution {
    /// The release that receives them.
    pub target: Target,
    /// Values as YAML.
    pub values: String,
}

/// A release of another component.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Target {
    /// `<capability>/<upstream>`.
    pub component: String,
    /// The release in it.
    pub release: String,
}

/// The security notes of a component.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Security {
    /// One sentence.
    pub summary: String,
    /// The Pod Security level its namespaces need.
    #[serde(rename = "podSecurity")]
    pub level: PodSecurity,
    /// Hosts it calls.
    #[serde(default)]
    pub egress: Vec<String>,
}

/// A Pod Security Standards level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PodSecurity {
    /// The default.
    Restricted,
    /// No seccomp profile or dropped capabilities.
    Baseline,
    /// Privileged pods or host access.
    Privileged,
    /// A level this installer does not know: the component is not offered.
    #[serde(other)]
    Unknown,
}

/// One health check: a single `kubectl wait` that applies nothing.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Health {
    /// Its name.
    pub name: String,
    /// The arguments of the `kubectl wait`, one per item: a `--for=` condition and no `--timeout`.
    pub wait: Vec<String>,
    /// How long the wait may take, as `kubectl wait --timeout` reads it.
    pub timeout: String,
    /// What to do when it fails, in one English sentence.
    pub advice: String,
}

impl Health {
    /// The arguments of the `kubectl wait`, with `timeout` in place of the check's own.
    #[must_use]
    pub fn args<'a>(&'a self, timeout: &'a str) -> Vec<&'a str> {
        let mut args = Vec::with_capacity(self.wait.len() + 1);
        for arg in &self.wait {
            args.push(arg.as_str());
        }
        args.push(timeout);
        args
    }

    /// The arguments of a `kubectl wait` until the check's objects exist, with `timeout`.
    ///
    /// `kubectl wait --all` or `-l` fails at once while nothing matches yet.
    #[must_use]
    pub fn create_args<'a>(&'a self, timeout: &'a str) -> Vec<&'a str> {
        let mut args = Vec::with_capacity(self.wait.len() + 1);
        args.push("--for=create");
        for arg in &self.wait {
            if !arg.starts_with("--for=") {
                args.push(arg);
            }
        }
        args.push(timeout);
        args
    }
}

/// What the catalog's CI measured in one environment where the component's test passed.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measured {
    /// `kind`, `k3s`, `rke2` or `kubesolo`.
    pub distribution: String,
    /// The profile whose values the run installed; `single-node` on a host distribution.
    pub profile: String,
    /// The API server's platform.
    pub platform: Platform,
    /// The server's `gitVersion` without build metadata (`v1.36.4`); empty when the catalog
    /// leaves it out.
    #[serde(default)]
    pub kubernetes_version: String,
    /// The working set at idle.
    #[serde(default)]
    pub idle: Amounts,
    /// What its pods request.
    #[serde(default)]
    pub requests: Amounts,
    /// The day it was measured, `YYYY-MM-DD`.
    #[serde(rename = "measuredAt")]
    pub date: String,
}

/// An operating system and architecture.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Platform {
    /// `linux`.
    pub os: String,
    /// `amd64`, `arm64`.
    pub architecture: String,
}

/// The distribution of the catalog's measurements on kind, which run every profile.
pub const KIND: &str = "kind";

impl Index {
    /// Parses an `index.json`.
    ///
    /// # Errors
    ///
    /// When the JSON does not match, or the index version is not [`INDEX_VERSION`].
    pub fn parse(json: &[u8]) -> Result<Self, PlanError> {
        let index: Self = serde_json::from_slice(json).map_err(PlanError::CatalogJson)?;
        if index.index_version != INDEX_VERSION {
            return Err(PlanError::IndexVersion(index.index_version));
        }
        Ok(index)
    }

    /// The component with this id.
    #[must_use]
    pub fn component(&self, id: &str) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }

    /// The chosen component that serves the in-cluster registry.
    #[must_use]
    pub fn registry<'a>(
        &'a self,
        chosen: &BTreeSet<&str>,
    ) -> Option<(&'a Component, &'a Registry)> {
        self.components
            .iter()
            .filter(|c| chosen.contains(c.id.as_str()))
            .find_map(|c| Some((c, c.provides.registry.as_ref()?)))
    }
}

impl Component {
    /// How it is offered on a profile: `unsupported` everywhere when a release type or its Pod
    /// Security level is one this installer does not know.
    #[must_use]
    pub fn default_for(&self, profile: &str) -> Default {
        let known = self.security.level != PodSecurity::Unknown
            && self.releases.iter().all(|r| r.kind != ReleaseType::Unknown);
        self.defaults
            .get(profile)
            .copied()
            .filter(|_| known)
            .unwrap_or(Default::Unsupported)
    }

    /// Whether it runs on a distribution.
    #[must_use]
    pub fn supports(&self, distribution: &str) -> bool {
        self.distributions.iter().any(|d| d == distribution)
    }

    /// Whether it supports the Kubernetes of a server version (`v1.36.5+k3s1`). A range this
    /// installer cannot read supports nothing.
    #[must_use]
    pub fn admits(&self, kubernetes: &str) -> bool {
        kube_version::admits(&self.kube_version, kubernetes).unwrap_or(false)
    }

    /// Whether the catalog's CI passed with it on a distribution and node platform
    /// (`linux/amd64`).
    #[must_use]
    pub fn is_tested_on(&self, distribution: &str, platform: &str) -> bool {
        self.tested_on
            .iter()
            .any(|t| t.distribution == distribution && t.platforms.iter().any(|p| p == platform))
    }

    /// The input whose value the installer generates as a free Service address.
    #[must_use]
    pub fn cluster_ip_input(&self) -> Option<&Input> {
        self.inputs
            .iter()
            .find(|i| i.generate == Some(Generate::ClusterIp))
    }

    /// The CRDs whose objects mean someone uses it: [`Detection::crds`] less its internal ones.
    pub fn in_use_crds(&self) -> impl Iterator<Item = &String> {
        self.detection
            .crds
            .iter()
            .filter(|crd| !self.internal_crds.contains(crd))
    }

    /// The newest measurement on `distribution` with `profile` on a Linux node of `arch`: on
    /// [`KIND`] with either profile, or on a host distribution with `single-node`.
    #[must_use]
    pub fn measured(&self, distribution: &str, profile: &str, arch: &str) -> Option<&Measured> {
        self.measured
            .iter()
            .filter(|m| {
                m.distribution == distribution
                    && m.profile == profile
                    && m.platform.os == "linux"
                    && m.platform.architecture == arch
            })
            .max_by_key(|m| (kube_version::release(&m.kubernetes_version), &m.date))
    }

    /// The idle memory of [`Component::measured`].
    #[must_use]
    pub fn idle_memory_mib(&self, distribution: &str, profile: &str, arch: &str) -> Option<u64> {
        self.measured(distribution, profile, arch)
            .map(|m| m.idle.memory())
    }

    /// The memory its pods request in [`Component::measured`].
    #[must_use]
    pub fn request_memory_mib(&self, distribution: &str, profile: &str, arch: &str) -> Option<u64> {
        self.measured(distribution, profile, arch)
            .map(|m| m.requests.memory())
    }
}

/// The catalog this build pins, tested on every distribution it supports. The release the build
/// pins has no measurements yet.
#[cfg(test)]
pub(crate) fn pinned() -> Index {
    let mut index = Index::parse(include_bytes!(concat!(
        env!("ZEABUR_CATALOG_RELEASE"),
        "/index.json"
    )))
    .unwrap();
    for component in &mut index.components {
        if component.tested_on.is_empty() {
            component.tested_on = ["k3s", "rke2", "kubesolo"]
                .into_iter()
                .filter(|d| component.supports(d))
                .map(|d| TestedOn {
                    distribution: d.to_owned(),
                    platforms: vec!["linux/amd64".to_owned(), "linux/arm64".to_owned()],
                })
                .collect();
        }
    }
    index
}
