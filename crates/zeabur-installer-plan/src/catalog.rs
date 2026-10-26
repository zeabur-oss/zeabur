//! The parts of a Zeabur catalog release (`index.json`) the installer reads.
//!
//! The catalog is data, not an installer: it says which upstream components fill which capability,
//! their charts and values per profile, the inputs to ask for and the order to install them in.
//! Unknown fields are ignored, because the index only changes `indexVersion` on incompatible
//! changes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::PlanError;

/// The only `indexVersion` this installer understands.
pub const INDEX_VERSION: u64 = 2;

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
    /// Distributions it works on; empty means any.
    #[serde(default)]
    pub distributions: Vec<String>,
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
    /// Checks that it works after installing.
    #[serde(default)]
    pub smoke: Vec<Smoke>,
    /// The components to install first, then this one, per profile.
    #[serde(default)]
    pub install_order: BTreeMap<String, Vec<String>>,
    /// Idle use measured by the catalog's CI, keyed `<profile>-<arch>-v<kubernetes>`.
    #[serde(default)]
    pub measured: BTreeMap<String, Measured>,
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
    /// Never installed on this profile.
    Unsupported,
}

/// One Helm chart, one set of manifests or one downloaded manifest file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// Unique within the component.
    pub name: String,
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
    /// `random`: `generate_bytes` random bytes in `generate_encoding`.
    pub generate: Option<String>,
    /// 64 when absent.
    pub generate_bytes: Option<usize>,
    /// Hexadecimal when absent.
    #[serde(default)]
    pub generate_encoding: Encoding,
    /// Installing fails without it.
    #[serde(default)]
    pub required: bool,
    /// Used when nothing is given.
    pub default: Option<Value>,
}

/// How generated bytes are written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    /// Two hexadecimal digits per byte.
    #[default]
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
    /// `<capability>/<upstream>`.
    pub target: String,
    /// The release in the target.
    pub release: String,
    /// Values as YAML.
    pub values: String,
}

/// The security notes of a component.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Security {
    /// One sentence.
    pub summary: String,
    /// Runs privileged pods or host access.
    #[serde(default)]
    pub privileged: bool,
    /// The Pod Security level its namespaces need when it cannot run under `restricted`.
    #[serde(default)]
    pub pod_security: Option<String>,
    /// Hosts it calls.
    #[serde(default)]
    pub egress: Vec<String>,
}

/// One smoke check.
#[derive(Debug, Clone, Deserialize)]
pub struct Smoke {
    /// Its name.
    pub name: String,
    /// Objects the catalog's CI creates for the test; the installer skips such tests.
    pub manifest: Option<String>,
    /// `kubectl wait` arguments.
    #[serde(default)]
    pub wait: Vec<String>,
}

/// Idle use measured in CI.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measured {
    /// Working set at idle.
    #[serde(rename = "idleMemoryMiB")]
    pub idle_memory_mib: u64,
}

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
}

impl Component {
    /// How it is offered on a profile.
    #[must_use]
    pub fn default_for(&self, profile: &str) -> Default {
        self.defaults
            .get(profile)
            .copied()
            .unwrap_or(Default::Unsupported)
    }

    /// Whether it runs on a distribution.
    #[must_use]
    pub fn supports(&self, distribution: &str) -> bool {
        self.distributions.is_empty() || self.distributions.iter().any(|d| d == distribution)
    }

    /// The idle memory measured for a profile and architecture, on the newest Kubernetes measured.
    #[must_use]
    pub fn idle_memory_mib(&self, profile: &str, arch: &str) -> Option<u64> {
        let prefix = format!("{profile}-{arch}-v");
        self.measured
            .iter()
            .rev()
            .find(|(key, _)| key.starts_with(&prefix))
            .map(|(_, m)| m.idle_memory_mib)
    }
}
