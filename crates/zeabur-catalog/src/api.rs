//! Zeabur Catalog's API [catalog-api]: the typed files Zeabur reads, and a [`Snapshot`] of one
//! release made of them.
//!
//! The API is static JSON at [`DEFAULT_URL`]: `catalog.json` lists each component's `id` and
//! `sourceDigest`, and `components/<id>.json` holds one component. A snapshot is `catalog.json`
//! and each component file with the digest `catalog.json` lists, so it never mixes two releases.
//! Nothing here does I/O: `zeabur-api` fetches the files and keeps the snapshot.
//!
//! The path names the API's major version (`v1`). Before 1.0 a catalog minor release can change
//! the format [catalog-api], so a Zeabur release also names the series it reads ([`SERIES`]) and
//! does not use a snapshot of another.
//!
//! [catalog-api]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/releases.mdx

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read as _, Write as _};

use serde::{Deserialize, Serialize};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_model::PodSecurityLevel;
use zeabur_model::kernel::dns::is_dns_label;

/// Where the API is when no mirror is set.
pub const DEFAULT_URL: &str = "https://oss-catalog.zeabur.com/v1/";

/// The catalog series this build reads: `0.5.x`.
pub const SERIES: (u64, u64) = (0, 5);

/// The largest file Zeabur reads from the API.
pub const MAX_FILE: usize = 1 << 20;

/// `catalog.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    /// The release.
    pub catalog: ReleaseInfo,
    /// The distributions the catalog tests on, each with its pinned release.
    #[serde(default)]
    pub distributions: Vec<Distribution>,
    /// Each component's id and digest.
    pub components: Vec<Entry>,
}

/// The release a file belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseInfo {
    /// `0.5.3`.
    pub version: String,
    /// The commit it was built from.
    pub commit: String,
}

/// A distribution and its pinned release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Distribution {
    /// `k3s`, `rke2` or `kubesolo`.
    pub name: String,
    /// `v1.37.2+k3s1`.
    pub version: String,
}

/// One component in `catalog.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Its id.
    pub id: String,
    /// The digest of its file.
    pub source_digest: String,
}

/// `components/<id>.json`, with the fields Zeabur reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentFile {
    /// Its id.
    pub id: String,
    /// The digest of the component file.
    pub source_digest: String,
    /// The component file as written.
    pub declaration: Declaration,
    /// Each environment e2e ran it in.
    #[serde(default)]
    pub measurements: Vec<Measurement>,
}

/// A component file [component-format].
///
/// [component-format]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/component-format.mdx
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Declaration {
    /// Its name and summary.
    pub metadata: Metadata,
    /// `recommended`, `optional` or `unsupported`, by profile.
    #[serde(default)]
    pub defaults: BTreeMap<String, String>,
    /// The distributions it may be installed on.
    #[serde(default)]
    pub distributions: Vec<String>,
    /// The Kubernetes versions it supports, as a Helm chart's `kubeVersion` range.
    #[serde(rename = "kubeVersion")]
    pub kube_version: String,
    /// What it needs installed first.
    #[serde(default)]
    pub requires: Vec<Reference>,
    /// What it offers.
    #[serde(default)]
    pub provides: Provides,
    /// What it installs, in order.
    pub releases: Vec<Release>,
    /// How privileged it is.
    pub security: Security,
    /// Read-only checks of an installed component.
    #[serde(default)]
    pub health: Vec<Health>,
    /// What stays after it is removed.
    #[serde(default)]
    pub leftovers: Vec<String>,
    /// What finds it on a cluster.
    #[serde(default)]
    pub detection: Detection,
    /// CRDs of `detection.crds` whose objects it writes itself.
    #[serde(default, rename = "internalCRDs")]
    pub internal_crds: Vec<String>,
    /// Values asked for or generated; Zeabur does not install a component that has any yet.
    #[serde(default)]
    pub inputs: Vec<Named>,
    /// Values it adds to other components' releases; Zeabur does not install a component that
    /// has any yet.
    #[serde(default)]
    pub contributes: Vec<Contribution>,
}

/// The name and the one-sentence summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    /// `cert-manager`.
    pub name: String,
    /// One sentence, in English.
    #[serde(default)]
    pub summary: String,
}

/// A capability (any of its components) or a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    /// A capability, such as `ingress`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    /// A component id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
}

/// What a component offers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provides {
    /// The capability, such as `certificates`.
    #[serde(default)]
    pub capability: String,
}

/// The kind of a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReleaseType {
    /// A Helm chart.
    Helm,
    /// Manifests written in the file.
    Inline,
    /// Manifests at URLs.
    Manifest,
    /// A type this build does not know: the component is not offered.
    #[serde(other)]
    Unknown,
}

/// One release of a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// Its name in the file.
    pub name: String,
    /// The namespace it installs into.
    pub namespace: String,
    /// What it is.
    #[serde(rename = "type")]
    pub kind: ReleaseType,
    /// The Helm release name, when it is not `name`.
    #[serde(
        default,
        rename = "releaseName",
        skip_serializing_if = "Option::is_none"
    )]
    pub helm: Option<String>,
    /// The chart: `oci://…`, or a name in `repo`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart: Option<String>,
    /// The HTTP chart repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The chart version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The values, as YAML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<String>,
    /// Values merged over `values`, as YAML, by profile.
    #[serde(default)]
    pub profiles: BTreeMap<String, String>,
    /// The manifests of an inline release, as YAML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifests: Option<String>,
}

impl Release {
    /// The Helm release name.
    #[must_use]
    pub fn helm_name(&self) -> &str {
        self.helm.as_deref().unwrap_or(&self.name)
    }
}

/// How privileged a component is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Security {
    /// The Pod Security level of its namespaces.
    pub pod_security: PodSecurityLevel,
    /// What it may do, in one sentence.
    #[serde(default)]
    pub summary: String,
}

/// One health check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    /// Its name.
    pub name: String,
    /// The objects it reads.
    pub resource: HealthResource,
    /// The condition they reach.
    pub condition: String,
    /// How long to wait, such as `60s`.
    #[serde(default)]
    pub timeout: String,
    /// What to tell the person when it fails.
    #[serde(default)]
    pub advice: String,
}

/// The objects a health check reads: one `name`, a `selector`, or `all`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResource {
    /// `Deployment`, `APIService`, `Node.longhorn.io`, ….
    pub kind: String,
    /// The namespace, for a namespaced kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// One object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The objects with these labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<BTreeMap<String, String>>,
    /// Every object of the kind.
    #[serde(default)]
    pub all: bool,
}

/// What finds a component on a cluster, as far as removing it reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    /// Its CRDs.
    #[serde(default)]
    pub crds: Vec<String>,
    /// The CSI drivers of the volumes it provides.
    #[serde(default)]
    pub csi_drivers: Vec<String>,
}

/// An entry with a name, of a list Zeabur only counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Named {
    /// Its name.
    pub name: String,
}

/// Values a component adds to another's release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contribution {
    /// The other component.
    pub target: ContributionTarget,
}

/// The component values are added to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionTarget {
    /// Its id.
    pub component: String,
}

/// One e2e run of a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    /// `kind`, `k3s`, `rke2` or `kubesolo`.
    pub distribution: String,
    /// The distribution's release on the host.
    #[serde(default)]
    pub distribution_version: Option<String>,
    /// The platform.
    pub platform: Platform,
    /// `passed`, or anything else for a run that did not pass.
    pub result: String,
    /// The digest of the component file the run installed.
    pub source_digest: String,
    /// The day, `YYYY-MM-DD`.
    pub measured_at: String,
}

/// An OS and an architecture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Platform {
    /// `linux`.
    pub os: String,
    /// `amd64`.
    pub architecture: String,
}

impl ComponentFile {
    /// Whether `testedOn` lists `distribution` with `platform` (`linux/amd64`) [installers]: the
    /// newest passing run there, on the distribution's pinned release, has this file's digest
    /// and is no older than the newest failed run (a pass on the same day wins).
    ///
    /// [installers]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/installers.mdx#tested-on
    #[must_use]
    pub fn tested_on(&self, index: &Index, distribution: &str, platform: &str) -> bool {
        let Some(pinned) = index.distributions.iter().find(|d| d.name == distribution) else {
            return false;
        };
        let runs = self.measurements.iter().filter(|m| {
            m.distribution == distribution
                && m.distribution_version.as_deref() == Some(pinned.version.as_str())
                && format!("{}/{}", m.platform.os, m.platform.architecture) == platform
        });
        let (mut pass, mut fail): (Option<&Measurement>, Option<&str>) = (None, None);
        for run in runs {
            if run.result == "passed" {
                if pass.is_none_or(|p| run.measured_at > p.measured_at) {
                    pass = Some(run);
                }
            } else if fail.is_none_or(|f| run.measured_at.as_str() > f) {
                fail = Some(&run.measured_at);
            }
        }
        pass.is_some_and(|p| {
            p.source_digest == self.source_digest
                && fail.is_none_or(|f| p.measured_at.as_str() >= f)
        })
    }
}

/// Why a file or a snapshot cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    /// A file does not parse.
    #[error("Zeabur Catalog, {file}: {message}")]
    Parse {
        /// The file.
        file: String,
        /// Why.
        message: String,
    },
    /// The release is of a series this build does not read.
    #[error("Zeabur Catalog {version} needs a newer Zeabur; this one reads {}.{}.x", SERIES.0, SERIES.1)]
    Series {
        /// The release's version.
        version: String,
    },
    /// A component file is not the one `catalog.json` lists: the fetch spanned two releases.
    #[error("Zeabur Catalog, components/{0}.json is not the file catalog.json lists")]
    Mismatch(String),
    /// A component is listed with an id that is not a DNS label.
    #[error("Zeabur Catalog lists a component with the id {0:?}")]
    BadId(String),
    /// The cached snapshot does not decode.
    #[error("the cached Zeabur Catalog does not read: {0}")]
    Cache(String),
}

/// Reads `catalog.json`, refusing a release of another series.
///
/// # Errors
///
/// It does not parse, names another series, or lists an id that is not a DNS label.
pub fn index(bytes: &[u8]) -> Result<Index, ApiError> {
    let index: Index = serde_json::from_slice(bytes).map_err(|e| ApiError::Parse {
        file: "catalog.json".to_owned(),
        message: e.to_string(),
    })?;
    if series(&index.catalog.version) != Some(SERIES) {
        return Err(ApiError::Series {
            version: index.catalog.version,
        });
    }
    if let Some(bad) = index.components.iter().find(|c| !is_dns_label(&c.id)) {
        return Err(ApiError::BadId(bad.id.clone()));
    }
    Ok(index)
}

/// Reads `components/<id>.json` and checks it is the file `index` lists.
///
/// # Errors
///
/// It does not parse, or its id or digest is not the one listed.
pub fn component(index: &Index, id: &str, bytes: &[u8]) -> Result<ComponentFile, ApiError> {
    let file: ComponentFile = serde_json::from_slice(bytes).map_err(|e| ApiError::Parse {
        file: format!("components/{id}.json"),
        message: e.to_string(),
    })?;
    let listed = index
        .components
        .iter()
        .any(|c| c.id == id && c.id == file.id && c.source_digest == file.source_digest);
    if listed {
        Ok(file)
    } else {
        Err(ApiError::Mismatch(id.to_owned()))
    }
}

/// `major.minor` of a version such as `0.5.3`.
fn series(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.trim_start_matches('v').split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// One catalog release as Zeabur read it: `catalog.json` and every component file it lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// `catalog.json`.
    pub index: Index,
    /// The component files, by id.
    pub components: BTreeMap<String, ComponentFile>,
    /// When it was fetched.
    pub fetched_at: Timestamp,
}

impl Snapshot {
    /// The ids `index` lists whose digest differs from this snapshot's, which a refresh fetches.
    #[must_use]
    pub fn stale(&self, index: &Index) -> Vec<String> {
        index
            .components
            .iter()
            .filter(|c| {
                self.components
                    .get(&c.id)
                    .is_none_or(|f| f.source_digest != c.source_digest)
            })
            .map(|c| c.id.clone())
            .collect()
    }

    /// The snapshot `index` lists: each component from `fetched`, or from `previous` when its
    /// digest is unchanged.
    ///
    /// # Errors
    ///
    /// A component listed is in neither.
    pub fn assemble(
        index: Index,
        mut fetched: BTreeMap<String, ComponentFile>,
        previous: Option<&Self>,
        fetched_at: Timestamp,
    ) -> Result<Self, ApiError> {
        let mut components = BTreeMap::new();
        for entry in &index.components {
            let file = fetched.remove(&entry.id).or_else(|| {
                previous
                    .and_then(|p| p.components.get(&entry.id))
                    .filter(|f| f.source_digest == entry.source_digest)
                    .cloned()
            });
            let file = file.ok_or_else(|| ApiError::Mismatch(entry.id.clone()))?;
            components.insert(entry.id.clone(), file);
        }
        Ok(Self {
            index,
            components,
            fetched_at,
        })
    }

    /// The component that provides `capability` and an install would choose on `profile`
    /// [installers]: the first in id order that is `recommended`, else the first `optional`.
    ///
    /// [installers]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/installers.mdx#install-order
    #[must_use]
    pub fn provider(&self, capability: &str, profile: &str) -> Option<&ComponentFile> {
        let offering = |level: &str| {
            self.components.values().find(|c| {
                c.declaration.provides.capability == capability
                    && c.declaration.defaults.get(profile).map(String::as_str) == Some(level)
            })
        };
        offering("recommended").or_else(|| offering("optional"))
    }

    /// The ids of the components whose `requires` names `id` or its capability.
    #[must_use]
    pub fn needed_by(&self, id: &str) -> BTreeSet<&str> {
        let capability = self
            .components
            .get(id)
            .map(|c| c.declaration.provides.capability.as_str());
        self.components
            .values()
            .filter(|c| {
                c.declaration.requires.iter().any(|r| {
                    r.component.as_deref() == Some(id)
                        || (r.capability.is_some() && r.capability.as_deref() == capability)
                })
            })
            .map(|c| c.id.as_str())
            .collect()
    }

    /// The snapshot as gzipped JSON, to keep in a ConfigMap.
    ///
    /// # Errors
    ///
    /// Never in practice: writing to memory does not fail.
    pub fn encode(&self) -> Result<Vec<u8>, ApiError> {
        let json = serde_json::to_vec(self).map_err(|e| ApiError::Cache(e.to_string()))?;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&json)
            .and_then(|()| gz.finish())
            .map_err(|e| ApiError::Cache(e.to_string()))
    }

    /// Reads [`Self::encode`]'s bytes, refusing a snapshot of another series.
    ///
    /// # Errors
    ///
    /// They do not decode, or the snapshot is of another series.
    pub fn decode(bytes: &[u8]) -> Result<Self, ApiError> {
        let mut json = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(16 * MAX_FILE as u64)
            .read_to_end(&mut json)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
        let snapshot: Self =
            serde_json::from_slice(&json).map_err(|e| ApiError::Cache(e.to_string()))?;
        if series(&snapshot.index.catalog.version) != Some(SERIES) {
            return Err(ApiError::Series {
                version: snapshot.index.catalog.version,
            });
        }
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn second(n: i64) -> Timestamp {
        Timestamp::from_second(n).unwrap()
    }

    const INDEX: &[u8] = include_bytes!("../tests/api/catalog.json");
    const CERT_MANAGER: &[u8] = include_bytes!("../tests/api/cert-manager.json");

    fn only_cert_manager() -> Index {
        let mut index = index(INDEX).unwrap();
        index.components.retain(|c| c.id == "cert-manager");
        index
    }

    #[test]
    fn a_release_of_another_series_is_refused() {
        let text = String::from_utf8(INDEX.to_vec())
            .unwrap()
            .replacen("\"0.5.3\"", "\"0.6.0\"", 1);
        assert!(
            matches!(index(text.as_bytes()), Err(ApiError::Series { version }) if version == "0.6.0")
        );
    }

    #[test]
    fn a_component_file_of_another_release_is_a_mismatch() {
        let mut listed = only_cert_manager();
        assert!(component(&listed, "cert-manager", CERT_MANAGER).is_ok());
        listed.components[0].source_digest = "sha256:0".to_owned();
        assert_eq!(
            component(&listed, "cert-manager", CERT_MANAGER),
            Err(ApiError::Mismatch("cert-manager".to_owned()))
        );
        // A file served under another id.
        assert!(component(&only_cert_manager(), "traefik", CERT_MANAGER).is_err());
    }

    #[test]
    fn a_refresh_fetches_only_changed_files_and_keeps_the_rest() {
        let listed = only_cert_manager();
        let file = component(&listed, "cert-manager", CERT_MANAGER).unwrap();
        let fetched = BTreeMap::from([("cert-manager".to_owned(), file)]);
        let first = Snapshot::assemble(listed.clone(), fetched, None, second(0)).unwrap();
        assert_eq!(first.stale(&listed), Vec::<String>::new());

        let again = Snapshot::assemble(listed.clone(), BTreeMap::new(), Some(&first), second(1));
        assert_eq!(again.unwrap().components, first.components);

        let mut changed = listed;
        changed.components[0].source_digest = "sha256:1".to_owned();
        assert_eq!(first.stale(&changed), ["cert-manager"]);
        assert_eq!(
            Snapshot::assemble(changed, BTreeMap::new(), Some(&first), second(2)),
            Err(ApiError::Mismatch("cert-manager".to_owned()))
        );
    }

    #[test]
    fn a_snapshot_survives_the_cache() {
        let listed = only_cert_manager();
        let file = component(&listed, "cert-manager", CERT_MANAGER).unwrap();
        let fetched = BTreeMap::from([("cert-manager".to_owned(), file)]);
        let snapshot = Snapshot::assemble(listed, fetched, None, second(0)).unwrap();
        let bytes = snapshot.encode().unwrap();
        assert_eq!(Snapshot::decode(&bytes).unwrap(), snapshot);
        assert!(matches!(
            Snapshot::decode(b"not gzip"),
            Err(ApiError::Cache(_))
        ));
    }

    #[test]
    fn an_id_that_is_not_a_dns_label_is_refused() {
        let text = String::from_utf8(INDEX.to_vec()).unwrap().replacen(
            "\"cert-manager\"",
            "\"../etc\"",
            1,
        );
        assert!(matches!(index(text.as_bytes()), Err(ApiError::BadId(_))));
    }
}
