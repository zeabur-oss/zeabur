//! Zeabur Catalog from its API, kept for every request and cached in the cluster.
//!
//! The API's files are static and say how long they stay fresh (`Cache-Control: max-age=600`).
//! Zeabur reads `catalog.json` at most that often, with the last answer's `ETag`, and then only
//! the component files whose `sourceDigest` changed (see [`zeabur_catalog::api`]). A component
//! file of another digest means the release changed between two reads: Zeabur reads once more,
//! and otherwise keeps the snapshot it has.
//!
//! Zeabur's own service account keeps the snapshot, gzipped, in the ConfigMap `zeabur-catalog`
//! of Zeabur's namespace, which the chart creates; it is read at start, so a restart without
//! network still has the catalog, and replicas share it. On a person's computer there is no such
//! account, and the snapshot lives in memory only.
//!
//! The files of manifest releases are downloaded when a plan needs them, checked against the
//! catalog's `urlDigests`, and kept in memory by digest, as a file of one digest never changes.
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /api/v1/cluster/catalog` | the catalog release Zeabur reads, when it was read, and why the last read failed |

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use futures_util::future::BoxFuture;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_catalog::api::{self, ApiError as CatalogError, MAX_FILE, Snapshot};
use zeabur_catalog::engine::{self, Download, Manifests};
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::ConfigMap;
use zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::k8s_openapi::{ByteString, Resource as _};
use zeabur_kube::{Cluster, ObjectTarget, ResourceRef, WriteBody, WriteRequest};

use crate::error::ApiError;
use crate::extract::Json;
use crate::handlers::{AppState, Person};

/// The ConfigMap the snapshot is cached in.
pub const CACHE: &str = "zeabur-catalog";
/// Its key.
const KEY: &str = "snapshot.json.gz";
/// How long an answer stays fresh: the API's `max-age`.
const FRESH: Duration = Duration::from_mins(10);
/// How long to wait after a failed read.
const RETRY: Duration = Duration::from_mins(1);
/// How long a failed first read waits before the next one: short, as there is nothing to serve.
const RETRY_FIRST: Duration = Duration::from_secs(5);
/// The largest manifest file Zeabur downloads; the Gateway API's is over 1 MiB.
const MAX_MANIFEST: usize = 16 << 20;
/// How long downloading one manifest file may take, redirects included.
const DOWNLOAD: Duration = Duration::from_mins(1);

/// One answer of the API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// `200`, with the body and its `ETag`.
    Body {
        /// The body.
        bytes: Vec<u8>,
        /// The `ETag`.
        etag: Option<String>,
    },
    /// `304`: the file named by the `ETag` sent is current.
    NotModified,
}

/// Reads a file of the API.
pub trait Fetch: Send + Sync {
    /// `GET url`, with `If-None-Match: etag` when there is one.
    fn get(&self, url: String, etag: Option<String>) -> BoxFuture<'_, Result<Fetched, String>>;

    /// `GET url` of a manifest file: its body.
    fn download(&self, url: String) -> BoxFuture<'_, Result<Vec<u8>, String>> {
        Box::pin(async move {
            match self.get(url.clone(), None).await? {
                Fetched::Body { bytes, .. } => Ok(bytes),
                Fetched::NotModified => Err(format!("{url} answered 304")),
            }
        })
    }
}

/// [`Fetch`] over HTTPS.
#[derive(Debug)]
pub struct Https(pub zeabur_http::Https);

impl Fetch for Https {
    fn get(&self, url: String, etag: Option<String>) -> BoxFuture<'_, Result<Fetched, String>> {
        Box::pin(async move {
            let mut request =
                axum::http::Request::get(&url).header(axum::http::header::USER_AGENT, "zeabur");
            if let Some(etag) = etag {
                request = request.header(axum::http::header::IF_NONE_MATCH, etag);
            }
            let request = request
                .body(http_body_util::Full::default())
                .map_err(|e| e.to_string())?;
            let (status, headers, bytes) = self.0.exchange_with_headers(request, MAX_FILE).await?;
            match status {
                200 => Ok(Fetched::Body {
                    bytes,
                    etag: headers
                        .get(axum::http::header::ETAG)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_owned),
                }),
                304 => Ok(Fetched::NotModified),
                _ => Err(format!("{url} answered {status}")),
            }
        })
    }

    fn download(&self, url: String) -> BoxFuture<'_, Result<Vec<u8>, String>> {
        Box::pin(async move { self.0.download(&url, MAX_MANIFEST, DOWNLOAD).await })
    }
}

/// The catalog, kept for every request.
pub struct Catalog {
    url: String,
    fetch: Option<Arc<dyn Fetch>>,
    /// Zeabur's own service account and its namespace, which hold the cache.
    cache: Option<(Arc<dyn Cluster>, String)>,
    kept: tokio::sync::Mutex<Kept>,
    /// The manifest files downloaded, by their hex SHA-256.
    files: std::sync::Mutex<BTreeMap<String, Arc<str>>>,
}

impl std::fmt::Debug for Catalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalog")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct Kept {
    snapshot: Option<Arc<Snapshot>>,
    etag: Option<String>,
    next: Option<Instant>,
    error: Option<String>,
    cache_error: Option<String>,
    cache_read: bool,
}

/// The catalog as Zeabur has it now.
#[derive(Debug, Clone)]
pub struct Current {
    /// The snapshot, when one was ever read.
    pub snapshot: Option<Arc<Snapshot>>,
    /// Why the last read failed.
    pub error: Option<String>,
    /// Why the last snapshot read is not kept in the cluster, only in memory.
    pub cache_error: Option<String>,
}

impl Catalog {
    /// A catalog read from `url` with `fetch` (`None`: it cannot be read, for `why`), cached by
    /// `cache`.
    #[must_use]
    pub fn new(
        url: String,
        fetch: Result<Arc<dyn Fetch>, String>,
        cache: Option<(Arc<dyn Cluster>, String)>,
    ) -> Self {
        let (fetch, error) = match fetch {
            Ok(fetch) => (Some(fetch), None),
            Err(why) => (None, Some(why)),
        };
        Self {
            url,
            fetch,
            cache,
            kept: tokio::sync::Mutex::new(Kept {
                error,
                ..Kept::default()
            }),
            files: std::sync::Mutex::default(),
        }
    }

    /// The files of `downloads`, each downloaded once and used only when its SHA-256 is the
    /// catalog's.
    ///
    /// # Errors
    ///
    /// `503` when a file cannot be downloaded; `422` when its digest differs. Both name the URL.
    pub async fn manifests(&self, downloads: Vec<Download>) -> Result<Manifests, ApiError> {
        let mut manifests = Manifests::new();
        for download in downloads {
            let kept = self
                .files
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&download.sha256)
                .cloned();
            let text = if let Some(text) = kept {
                text
            } else {
                let unavailable = |why: String| {
                    Problem::new(
                        Kind::Unavailable,
                        format!("Zeabur cannot download {}: {why}", download.url),
                    )
                };
                let fetch = self
                    .fetch
                    .as_ref()
                    .ok_or_else(|| unavailable("it has no HTTPS client".to_owned()))?;
                let bytes = fetch
                    .download(download.url.clone())
                    .await
                    .map_err(unavailable)?;
                let text: Arc<str> = engine::verify(&download, bytes)
                    .map_err(|why| Problem::new(Kind::Invalid, why))?
                    .into();
                self.files
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(download.sha256.clone(), Arc::clone(&text));
                text
            };
            manifests.insert(download.url, text);
        }
        Ok(manifests)
    }

    /// The API's base URL, ending in `/`.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The snapshot, read again when the last read is older than the API's `max-age`, or a
    /// minute after a failed read (5 seconds while there is no snapshot). One read goes at a
    /// time, and the others wait for it.
    pub async fn current(&self, now: Instant, fetched_at: Timestamp) -> Current {
        let mut kept = self.kept.lock().await;
        if !kept.cache_read {
            kept.cache_read = true;
            if let Some((cluster, namespace)) = &self.cache {
                match read_cache(cluster.as_ref(), namespace).await {
                    Ok(snapshot) => kept.snapshot = snapshot.map(Arc::new),
                    Err(e) => tracing::warn!(error = %e, "the cached Zeabur Catalog is not used"),
                }
            }
        }
        if let Some(fetch) = &self.fetch
            && kept.next.is_none_or(|next| now >= next)
        {
            let mut answer = self.refresh(fetch.as_ref(), &kept, fetched_at).await;
            if matches!(answer, Err(Refusal::Catalog(CatalogError::Mismatch(_)))) {
                answer = self.refresh(fetch.as_ref(), &kept, fetched_at).await;
            }
            match answer {
                Ok(None) => {
                    kept.next = Some(now + FRESH);
                    kept.error = None;
                }
                Ok(Some((snapshot, etag))) => {
                    if let Some((cluster, namespace)) = &self.cache {
                        let written = write_cache(cluster.as_ref(), namespace, &snapshot).await;
                        if let Err(e) = &written {
                            tracing::warn!(error = %e, "the Zeabur Catalog was not cached");
                        }
                        kept.cache_error = written.err();
                    }
                    kept.snapshot = Some(Arc::new(snapshot));
                    kept.etag = etag;
                    kept.next = Some(now + FRESH);
                    kept.error = None;
                }
                Err(e) => {
                    let wait = if kept.snapshot.is_some() {
                        RETRY
                    } else {
                        RETRY_FIRST
                    };
                    kept.next = Some(now + wait);
                    kept.error = Some(e.to_string());
                }
            }
        }
        Current {
            snapshot: kept.snapshot.clone(),
            error: kept.error.clone(),
            cache_error: kept.cache_error.clone(),
        }
    }

    /// A new snapshot and the `ETag` of its `catalog.json`; `None` when the one kept is current.
    async fn refresh(
        &self,
        fetch: &dyn Fetch,
        kept: &Kept,
        fetched_at: Timestamp,
    ) -> Result<Option<(Snapshot, Option<String>)>, Refusal> {
        let etag = kept.snapshot.as_ref().and_then(|_| kept.etag.clone());
        let url = format!("{}catalog.json", self.url);
        let (bytes, etag) = match fetch.get(url, etag).await.map_err(Refusal::Fetch)? {
            Fetched::NotModified => return Ok(None),
            Fetched::Body { bytes, etag } => (bytes, etag),
        };
        let index = api::index(&bytes)?;
        let previous = kept.snapshot.as_deref();
        // A file an older build could not read is asked for again; this build may read it.
        if previous.is_some_and(|p| p.index == index && p.problems.is_empty()) {
            return Ok(None);
        }
        let stale = previous.map_or_else(
            || index.components.iter().map(|c| c.id.clone()).collect(),
            |p| p.stale(&index),
        );
        let mut fetched: BTreeMap<String, api::Decoded> = BTreeMap::new();
        for id in stale {
            let url = format!("{}components/{id}.json", self.url);
            let Fetched::Body { bytes, .. } = fetch.get(url, None).await.map_err(Refusal::Fetch)?
            else {
                return Err(Refusal::Fetch(format!("components/{id}.json answered 304")));
            };
            let file = api::component(&index, &id, &bytes)?;
            fetched.insert(id, file);
        }
        let snapshot = Snapshot::assemble(index, fetched, previous, fetched_at)?;
        for (id, problem) in &snapshot.problems {
            tracing::warn!(component = %id, error = %problem, "a Zeabur Catalog file does not read");
        }
        Ok(Some((snapshot, etag)))
    }
}

/// Why a read failed.
#[derive(Debug, thiserror::Error)]
enum Refusal {
    #[error("{0}")]
    Fetch(String),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

async fn read_cache(cluster: &dyn Cluster, namespace: &str) -> Result<Option<Snapshot>, String> {
    let resource = ResourceRef::of::<ConfigMap>();
    let Some(object) =
        zeabur_kube::ClusterExt::find_object(cluster, &resource, Some(namespace), CACHE)
            .await
            .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let map: ConfigMap = object.try_parse().map_err(|e| e.to_string())?;
    let Some(bytes) = map.binary_data.and_then(|mut d| d.remove(KEY)) else {
        return Ok(None);
    };
    Snapshot::decode(&bytes.0)
        .map(Some)
        .map_err(|e| e.to_string())
}

async fn write_cache(
    cluster: &dyn Cluster,
    namespace: &str,
    snapshot: &Snapshot,
) -> Result<(), String> {
    let bytes = snapshot.encode().map_err(|e| e.to_string())?;
    let map = ConfigMap {
        metadata: ObjectMeta {
            name: Some(CACHE.to_owned()),
            namespace: Some(namespace.to_owned()),
            ..ObjectMeta::default()
        },
        binary_data: Some(BTreeMap::from([(KEY.to_owned(), ByteString(bytes))])),
        ..ConfigMap::default()
    };
    let body = serde_json::to_value(&map).map_err(|e| e.to_string())?;
    let request = WriteRequest {
        target: ObjectTarget {
            api_version: ConfigMap::API_VERSION.to_owned(),
            kind: ConfigMap::KIND.to_owned(),
            plural: ConfigMap::URL_PATH_SEGMENT.to_owned(),
            namespace: namespace.to_owned(),
            name: CACHE.to_owned(),
        },
        body: WriteBody::Apply(body),
        dry_run: false,
    };
    cluster
        .write(&request)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The catalog release Zeabur reads.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogView {
    /// Where Zeabur reads it.
    pub url: String,
    /// The release, `0.6.1`; absent when Zeabur has never read one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The release's commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// When it was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<Timestamp>,
    /// Why the last read failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Why the release read is kept in memory only, not in the ConfigMap `zeabur-catalog`, so
    /// a restart reads the catalog again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_error: Option<String>,
}

/// The catalog release Zeabur reads, when it was read, and why the last read failed.
#[utoipa::path(get, path = "/api/v1/cluster/catalog", tag = "cluster", operation_id = "getCatalog",
    responses((status = 200, body = CatalogView),
              (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(_): Person,
) -> Result<Json<CatalogView>, ApiError> {
    let current = current(&state).await;
    let release = current.snapshot.as_ref().map(|s| &s.index.catalog);
    Ok(Json(CatalogView {
        url: state.catalog.url().to_owned(),
        version: release.map(|r| r.version.clone()),
        commit: release.map(|r| r.commit.clone()),
        fetched_at: current.snapshot.as_ref().map(|s| s.fetched_at),
        error: current.error,
        cache_error: current.cache_error,
    }))
}

/// The catalog now, read again when it is stale.
pub(crate) async fn current(state: &AppState) -> Current {
    let now = (state.clock)();
    let fetched_at = Timestamp::from_second(now.as_second()).unwrap_or(now);
    state.catalog.current(Instant::now(), fetched_at).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn second(n: i64) -> Timestamp {
        Timestamp::from_second(n).unwrap()
    }

    const INDEX: &[u8] = include_bytes!("../../../zeabur-catalog/tests/api/catalog.json");
    const CERT_MANAGER: &[u8] =
        include_bytes!("../../../zeabur-catalog/tests/api/cert-manager.json");

    /// The API with cert-manager only; counts the reads.
    struct Fake {
        index: Mutex<Vec<u8>>,
        reads: Mutex<Vec<String>>,
        down: Mutex<bool>,
    }

    impl Fake {
        fn new() -> Arc<Self> {
            let mut index: serde_json::Value = serde_json::from_slice(INDEX).unwrap();
            index["components"]
                .as_array_mut()
                .unwrap()
                .retain(|c| c["id"] == "cert-manager");
            Arc::new(Self {
                index: Mutex::new(serde_json::to_vec(&index).unwrap()),
                reads: Mutex::new(Vec::new()),
                down: Mutex::new(false),
            })
        }
    }

    impl Fetch for Fake {
        fn get(&self, url: String, etag: Option<String>) -> BoxFuture<'_, Result<Fetched, String>> {
            Box::pin(async move {
                self.reads.lock().unwrap().push(url.clone());
                if *self.down.lock().unwrap() {
                    return Err("connection refused".to_owned());
                }
                if url.ends_with("catalog.json") {
                    if etag.as_deref() == Some("\"a\"") {
                        return Ok(Fetched::NotModified);
                    }
                    return Ok(Fetched::Body {
                        bytes: self.index.lock().unwrap().clone(),
                        etag: Some("\"a\"".to_owned()),
                    });
                }
                Ok(Fetched::Body {
                    bytes: CERT_MANAGER.to_vec(),
                    etag: None,
                })
            })
        }
    }

    fn catalog(fake: &Arc<Fake>, cache: Option<Arc<dyn Cluster>>) -> Catalog {
        let fetch: Arc<dyn Fetch> = Arc::<Fake>::clone(fake);
        Catalog::new(
            "https://catalog.test/v1/".to_owned(),
            Ok(fetch),
            cache.map(|c| (c, "zeabur".to_owned())),
        )
    }

    #[tokio::test]
    async fn a_fresh_snapshot_is_not_read_again_and_a_stale_one_asks_with_its_etag() {
        let fake = Fake::new();
        let catalog = catalog(&fake, None);
        let start = Instant::now();
        let first = catalog.current(start, second(0)).await;
        assert_eq!(first.snapshot.unwrap().index.catalog.version, "0.6.1");
        assert_eq!(fake.reads.lock().unwrap().len(), 2);

        catalog
            .current(start + Duration::from_secs(5), second(1))
            .await;
        assert_eq!(fake.reads.lock().unwrap().len(), 2);

        let later = catalog.current(start + FRESH, second(2)).await;
        assert_eq!(later.snapshot.unwrap().fetched_at, second(0));
        assert_eq!(fake.reads.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn without_network_the_snapshot_kept_is_used_and_the_error_named() {
        let fake = Fake::new();
        let kept = catalog(&fake, None);
        let start = Instant::now();
        kept.current(start, second(0)).await;
        *fake.down.lock().unwrap() = true;
        let now = kept.current(start + FRESH, second(1)).await;
        assert!(now.snapshot.is_some());
        assert_eq!(now.error.as_deref(), Some("connection refused"));

        let fresh = Fake::new();
        *fresh.down.lock().unwrap() = true;
        let never = catalog(&fresh, None).current(start, second(0)).await;
        assert!(never.snapshot.is_none());
        assert!(never.error.is_some());
    }

    #[tokio::test]
    async fn a_failed_first_read_is_tried_again_after_a_few_seconds() {
        let fake = Fake::new();
        *fake.down.lock().unwrap() = true;
        let catalog = catalog(&fake, None);
        let start = Instant::now();
        catalog.current(start, second(0)).await;
        catalog
            .current(start + Duration::from_secs(1), second(1))
            .await;
        assert_eq!(fake.reads.lock().unwrap().len(), 1);

        *fake.down.lock().unwrap() = false;
        let now = catalog.current(start + RETRY_FIRST, second(2)).await;
        assert!(now.snapshot.is_some());
        assert!(now.error.is_none());
    }

    #[tokio::test]
    async fn a_newer_release_with_fields_it_does_not_know_is_used() {
        let fake = Fake::new();
        let mut index: serde_json::Value =
            serde_json::from_slice(&fake.index.lock().unwrap()).unwrap();
        index["catalog"]["version"] = "0.6.0".into();
        index["catalog"]["added"] = true.into();
        index["added"] = serde_json::json!({"a": 1});
        *fake.index.lock().unwrap() = serde_json::to_vec(&index).unwrap();
        let now = catalog(&fake, None)
            .current(Instant::now(), second(0))
            .await;
        assert!(now.error.is_none());
        assert_eq!(now.snapshot.unwrap().index.catalog.version, "0.6.0");
    }

    #[tokio::test]
    async fn the_cluster_cache_outlives_a_restart() {
        let cluster: Arc<dyn Cluster> = Arc::new(zeabur_kube::memory::MemoryCluster::new(
            zeabur_kube::memory::State::default(),
        ));
        let fake = Fake::new();
        catalog(&fake, Some(Arc::clone(&cluster)))
            .current(Instant::now(), second(0))
            .await;

        let offline = Fake::new();
        *offline.down.lock().unwrap() = true;
        let now = catalog(&offline, Some(cluster))
            .current(Instant::now(), second(1))
            .await;
        assert_eq!(now.snapshot.unwrap().fetched_at, second(0));
        assert!(now.error.is_some());
    }

    #[tokio::test]
    async fn a_cached_file_that_did_not_read_is_read_again_after_a_restart() {
        let cluster: Arc<dyn Cluster> = Arc::new(zeabur_kube::memory::MemoryCluster::new(
            zeabur_kube::memory::State::default(),
        ));
        let fake = Fake::new();
        let listed = api::index(&fake.index.lock().unwrap()).unwrap();
        let unread = BTreeMap::from([("cert-manager".to_owned(), Err("too new".to_owned()))]);
        let old = Snapshot::assemble(listed, unread, None, second(0)).unwrap();
        write_cache(cluster.as_ref(), "zeabur", &old).await.unwrap();

        let now = catalog(&fake, Some(cluster))
            .current(Instant::now(), second(1))
            .await;
        let snapshot = now.snapshot.unwrap();
        assert!(snapshot.problems.is_empty());
        assert!(snapshot.components.contains_key("cert-manager"));
    }

    #[tokio::test]
    async fn a_cache_that_cannot_be_written_is_named() {
        let mut state = zeabur_kube::memory::State::default();
        state.forbidden_writes.insert(format!("ConfigMap/{CACHE}"));
        let cluster: Arc<dyn Cluster> = Arc::new(zeabur_kube::memory::MemoryCluster::new(state));
        let now = catalog(&Fake::new(), Some(cluster))
            .current(Instant::now(), second(0))
            .await;
        assert!(now.snapshot.is_some());
        assert!(now.cache_error.unwrap().contains(CACHE));
    }
}
