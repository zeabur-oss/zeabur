//! Zeabur Catalog's API [catalog-api]: a [`Snapshot`] of one release, made of the files that
//! `zeabur-catalog-format` types.
//!
//! The API is static JSON at [`DEFAULT_URL`]: `catalog.json` lists each component's `id` and
//! `sourceDigest`, and `components/<id>.json` holds one component. A snapshot is `catalog.json`
//! and each component file with the digest `catalog.json` lists, so it never mixes two releases.
//! Nothing here does I/O: `zeabur-api` fetches the files and keeps the snapshot.
//!
//! The path names the API's major version (`v1`). Every catalog release on `v1` only adds to it
//! [catalog-api], so Zeabur reads any of them and leaves out the fields it does not know. A
//! component file that does not decode is kept as a problem with its id ([`Snapshot::problems`]),
//! not left out.
//!
//! [catalog-api]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/releases.mdx

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read as _, Write as _};

use serde::{Deserialize, Serialize};
use zeabur_catalog_format::api::Entry;
pub use zeabur_catalog_format::api::{ComponentFile, Index};
use zeabur_catalog_format::common::{Mode, Profile, Reference};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_model::kernel::dns::is_dns_label;

/// Where the API is when no mirror is set.
pub const DEFAULT_URL: &str = "https://oss-catalog.zeabur.com/v1/";

/// The largest file Zeabur reads from the API.
pub const MAX_FILE: usize = 1 << 20;

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

/// Reads `catalog.json`.
///
/// # Errors
///
/// It does not parse, or lists an id that is not a DNS label.
pub fn index(bytes: &[u8]) -> Result<Index, ApiError> {
    let index: Index = serde_json::from_slice(bytes).map_err(|e| ApiError::Parse {
        file: "catalog.json".to_owned(),
        message: e.to_string(),
    })?;
    if let Some(bad) = index.components.iter().find(|c| !is_dns_label(&c.id)) {
        return Err(ApiError::BadId(bad.id.clone()));
    }
    Ok(index)
}

/// A component file as listed: decoded, or why it does not decode.
pub type Decoded = Result<ComponentFile, String>;

/// Reads `components/<id>.json` and checks it is the file `index` lists. A file that is the one
/// listed but does not decode with the catalog's types is `Ok(Err(why))`.
///
/// # Errors
///
/// It is no JSON object with an `id` and a `sourceDigest`, or its id or digest is not the one
/// listed.
pub fn component(index: &Index, id: &str, bytes: &[u8]) -> Result<Decoded, ApiError> {
    let entry: Entry = serde_json::from_slice(bytes).map_err(|e| ApiError::Parse {
        file: format!("components/{id}.json"),
        message: e.to_string(),
    })?;
    let listed = index
        .components
        .iter()
        .any(|c| c.id == id && c.id == entry.id && c.source_digest == entry.source_digest);
    if !listed {
        return Err(ApiError::Mismatch(id.to_owned()));
    }
    Ok(serde_json::from_slice(bytes).map_err(|e| e.to_string()))
}

/// The profile named `name` (`single-node`).
#[must_use]
pub fn profile(name: &str) -> Option<Profile> {
    Profile::ALL.into_iter().find(|p| p.as_str() == name)
}

/// One catalog release as Zeabur read it: `catalog.json` and every component file it lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// `catalog.json`.
    pub index: Index,
    /// The component files that decode, by id.
    pub components: BTreeMap<String, ComponentFile>,
    /// Why each listed component file that does not decode does not, by id.
    #[serde(default)]
    pub problems: BTreeMap<String, String>,
    /// When it was fetched.
    pub fetched_at: Timestamp,
}

impl Snapshot {
    /// The ids `index` lists whose digest differs from this snapshot's, or whose file did not
    /// decode, which a refresh fetches.
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
        mut fetched: BTreeMap<String, Decoded>,
        previous: Option<&Self>,
        fetched_at: Timestamp,
    ) -> Result<Self, ApiError> {
        let mut components = BTreeMap::new();
        let mut problems = BTreeMap::new();
        for entry in &index.components {
            let file = fetched.remove(&entry.id).or_else(|| {
                previous
                    .and_then(|p| p.components.get(&entry.id))
                    .filter(|f| f.source_digest == entry.source_digest)
                    .cloned()
                    .map(Ok)
            });
            match file.ok_or_else(|| ApiError::Mismatch(entry.id.clone()))? {
                Ok(file) => {
                    components.insert(entry.id.clone(), file);
                }
                Err(why) => {
                    problems.insert(entry.id.clone(), why);
                }
            }
        }
        Ok(Self {
            index,
            components,
            problems,
            fetched_at,
        })
    }

    /// The component that provides `capability` and an install would choose on `profile`
    /// [installers]: the first in id order that is `recommended`, else the first `optional`.
    ///
    /// [installers]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/installers.mdx#install-order
    #[must_use]
    pub fn provider(&self, capability: &str, profile: Profile) -> Option<&ComponentFile> {
        let offering = |mode: Mode| {
            self.components.values().find(|c| {
                c.declaration.capability() == Some(capability)
                    && c.declaration.mode(profile) == mode
            })
        };
        offering(Mode::Recommended).or_else(|| offering(Mode::Optional))
    }

    /// The ids of the components whose `requires` names `id` or its capability.
    #[must_use]
    pub fn needed_by(&self, id: &str) -> BTreeSet<&str> {
        let capability = self
            .components
            .get(id)
            .and_then(|c| c.declaration.capability());
        self.components
            .values()
            .filter(|c| {
                c.declaration.requires.iter().any(|r| match r {
                    Reference::Component(component) => component == id,
                    Reference::Capability(name) => Some(name.as_str()) == capability,
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

    /// Reads [`Self::encode`]'s bytes.
    ///
    /// # Errors
    ///
    /// They do not decode.
    pub fn decode(bytes: &[u8]) -> Result<Self, ApiError> {
        let mut json = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(16 * MAX_FILE as u64)
            .read_to_end(&mut json)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
        serde_json::from_slice(&json).map_err(|e| ApiError::Cache(e.to_string()))
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
    fn a_component_file_of_another_release_is_a_mismatch() {
        let mut listed = only_cert_manager();
        assert!(matches!(
            component(&listed, "cert-manager", CERT_MANAGER),
            Ok(Ok(_))
        ));
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
    fn a_listed_file_that_does_not_decode_is_a_problem_with_its_id() {
        let listed = only_cert_manager();
        let mut file: serde_json::Value = serde_json::from_slice(CERT_MANAGER).unwrap();
        file["declaration"]["releases"] = "not a list".into();
        let bytes = serde_json::to_vec(&file).unwrap();
        let read = component(&listed, "cert-manager", &bytes).unwrap();
        let fetched = BTreeMap::from([("cert-manager".to_owned(), read)]);
        let snapshot = Snapshot::assemble(listed.clone(), fetched, None, second(0)).unwrap();
        assert!(snapshot.components.is_empty());
        assert!(snapshot.problems["cert-manager"].contains("expected a sequence"));
        // A refresh of the same release asks for it again.
        assert_eq!(snapshot.stale(&listed), ["cert-manager"]);
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
