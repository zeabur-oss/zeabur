//! `GET /api/v1/cluster/backups/{backup}/download`: a Velero backup's contents, streamed as a
//! file.
//!
//! Restore plans read the backup through the same path: its list of objects
//! (`resource_list`), and the routes and Namespaces in its archive (`routes`, `namespaces`).
//!
//! As the person, Zeabur creates a `DownloadRequest` for the backup's contents or its list and
//! waits until Velero signs a URL for it [velero-download]. That URL expires after ten minutes,
//! and Velero deletes the request after it expires. Zeabur fetches it from the location's
//! endpoint, with the location's customer key when it encrypts with one ([`super::object_store`]).
//! The contents are Velero's archive of the objects of the backup, as `velero backup download`
//! saves it; the files of volumes are in the location's Kopia repository, not in it. The list is
//! what `velero backup describe --details` prints, gzipped JSON [velero-describe].
//!
//! [velero-download]: https://velero.io/docs/v1.17/api-types/downloadrequest/
//! [velero-describe]: https://github.com/vmware-tanzu/velero/blob/v1.17.0/pkg/cmd/util/downloadrequest/downloadrequest.go

use std::collections::BTreeMap;
use std::io::Read as _;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::{HeaderName, HeaderValue, header};
use axum::response::Response;
use serde_json::Value;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{Cluster, ClusterExt as _, ObjectTarget, WriteBody, WriteRequest};
use zeabur_modules::backups::Outcome;
use zeabur_modules::velero::cluster::Download;
use zeabur_modules::velero::{self, cluster as v};

use super::backups::{GROUP, chosen_location, not_installed, objects, target};
use super::object_store::{authority, customer_key_headers, fetched};
use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::{AppState, Person};

/// How long Velero may take to sign the URL.
const SIGNING: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(500);
/// The most bytes read of a backup's list of objects, unpacked.
const LIST_BYTES: usize = 16 << 20;
/// The most bytes read of a backup's archive, packed and unpacked.
const ARCHIVE_BYTES: usize = 512 << 20;

fn bad_gateway(detail: String) -> ApiError {
    ApiError::from(Problem::new(Kind::Unavailable, detail))
}

/// The hosts the location's URLs may name: its endpoint and the public URL it signs with.
fn location_hosts(location: &Value) -> Vec<String> {
    let mut hosts: Vec<String> = ["/spec/config/s3Url", "/spec/config/publicUrl"]
        .iter()
        .filter_map(|p| location.pointer(p).and_then(Value::as_str))
        .filter_map(authority)
        .collect();
    hosts.dedup();
    hosts
}

/// The URL Velero signs for the `DownloadRequest` `name`, once it has.
async fn signed_url(
    cluster: &dyn Cluster,
    request: &ObjectTarget,
    namespace: &str,
    name: &str,
) -> Result<String, ApiError> {
    let deadline = tokio::time::Instant::now() + SIGNING;
    loop {
        let signed = cluster
            .find_object(&request.resource(), Some(namespace), name)
            .await
            .ok()
            .flatten()
            .and_then(|o| v::download_url(&o));
        if let Some((url, _)) = signed {
            return Ok(url);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(bad_gateway(
                "Velero did not prepare the download in time; check that its server runs."
                    .to_owned(),
            ));
        }
        tokio::time::sleep(POLL).await;
    }
}

/// The headers that read objects the location encrypts with a customer key, read as the person;
/// none when it does not.
async fn customer_key(
    cluster: &dyn Cluster,
    namespace: &str,
    location: &Value,
) -> Result<Vec<(HeaderName, HeaderValue)>, ApiError> {
    let Some((secret, key)) = location
        .pointer("/spec/config/customerKeyEncryptionSecret")
        .and_then(Value::as_str)
        .and_then(|s| s.split_once('/'))
    else {
        return Ok(Vec::new());
    };
    let values = cluster
        .reveal_secret(namespace, secret)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("Secret {secret} in {namespace}"),
                get_read("", "secrets", Some(namespace), secret),
            )
        })?;
    let value = values.get(key).ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::NotFound,
            format!("Secret {secret} has no key {key:?}, so the backup cannot be read."),
        ))
    })?;
    Ok(customer_key_headers(value))
}

/// The name of a new `DownloadRequest`: the backup, the second and a random tail, so that two
/// downloads in one second do not collide.
fn request_name(
    backup: &str,
    now: &zeabur_kube::k8s_openapi::jiff::Timestamp,
) -> Result<String, ApiError> {
    let [a, b, c] = zeabur_core::crypto::random::<3>().map_err(|_random| {
        ApiError::from(Problem::new(
            Kind::Internal,
            "The system random number generator failed, so Zeabur could not name the download.",
        ))
    })?;
    let at = now.strftime("%Y%m%d%H%M%S").to_string();
    let tail = u32::from_be_bytes([0, a, b, c]);
    Ok(format!("{}-{tail:06x}", v::backup_name(backup, &at)))
}

/// `what` of `backup`, fetched as the person: Zeabur creates a `DownloadRequest` in Velero's
/// namespace and reads the location's customer key. No URL or key leaves this function.
pub(crate) async fn fetch(
    state: &AppState,
    cluster: &dyn Cluster,
    backup: &str,
    what: Download,
) -> Result<axum::body::Body, ApiError> {
    let super::backups::Found::Read(found) = objects(cluster, None).await? else {
        return Err(not_installed());
    };
    let namespace = found.namespace.as_str();
    let object = cluster
        .find_object(&velero::BACKUPS.resource(), Some(namespace), backup)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("backup {backup}"),
                get_read(GROUP, "backups", Some(namespace), backup),
            )
        })?
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::NotFound,
                format!("Velero has no backup {backup:?}."),
            ))
        })?;
    let view = v::backup(&object, &BTreeMap::new());
    if !matches!(view.outcome, Outcome::Succeeded | Outcome::PartiallyFailed) {
        return Err(ApiError::from(Problem::new(
            Kind::Invalid,
            format!("Backup {backup} has not finished, so it cannot be read."),
        )));
    }
    let location = object
        .data
        .pointer("/spec/storageLocation")
        .and_then(Value::as_str)
        .and_then(|name| {
            found
                .locations
                .iter()
                .find(|l| l.metadata.name.as_deref() == Some(name))
        })
        .or_else(|| chosen_location(&found.locations))
        .and_then(|l| serde_json::to_value(l).ok())
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::NotFound,
                format!("The location of backup {backup} could not be read."),
            ))
        })?;

    let name = request_name(backup, &(state.clock)())?;
    let request = target("DownloadRequest", namespace, &name);
    cluster
        .write(&WriteRequest {
            target: request.clone(),
            body: WriteBody::Create(v::download_request(namespace, &name, backup, what)),
            dry_run: false,
        })
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("DownloadRequest {name} in {namespace}"),
                super::backups::right("create", "downloadrequests", namespace, None),
            )
        })?;
    let url = signed_url(cluster, &request, namespace, &name).await?;
    let allowed = location_hosts(&location);
    let host = authority(&url).unwrap_or_default();
    // AWS S3 itself: no endpoint is set, and the URL names an amazonaws.com host.
    let aws = allowed.is_empty() && host.ends_with(".amazonaws.com");
    if !aws && !allowed.contains(&host) {
        return Err(bad_gateway(format!(
            "Velero signed a URL for {host}, which is not the location's endpoint."
        )));
    }
    let headers = customer_key(cluster, namespace, &location).await?;
    fetched(
        &state.store,
        &url,
        headers,
        "The backup location",
        "for the backup's contents",
    )
    .await
}

/// The first `limit` bytes of `body`; a problem when it holds more.
async fn collected(body: axum::body::Body, limit: usize) -> Result<bytes::Bytes, ApiError> {
    http_body_util::BodyExt::collect(http_body_util::Limited::new(body, limit))
        .await
        .map(http_body_util::Collected::to_bytes)
        .map_err(|e| bad_gateway(format!("The backup could not be read: {e}")))
}

/// The objects of `backup` as Velero listed them: `group/version/Kind` to `namespace/name`.
pub(crate) async fn resource_list(
    state: &AppState,
    cluster: &dyn Cluster,
    backup: &str,
) -> Result<BTreeMap<String, Vec<String>>, ApiError> {
    let body = fetch(state, cluster, backup, Download::BackupResourceList).await?;
    let packed = collected(body, LIST_BYTES).await?;
    tokio::task::spawn_blocking(move || {
        let mut text = Vec::new();
        flate2::read::GzDecoder::new(packed.as_ref())
            .take(LIST_BYTES as u64)
            .read_to_end(&mut text)
            .map_err(|e| e.to_string())?;
        serde_json::from_slice(&text).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .flatten()
    .map_err(|e| {
        bad_gateway(format!(
            "Velero's list of backup {backup} could not be read: {e}"
        ))
    })
}

/// The routes of `namespace` in `backup`'s archive, each once.
pub(crate) async fn routes(
    state: &AppState,
    cluster: &dyn Cluster,
    backup: &str,
    namespace: &str,
) -> Result<Vec<Value>, ApiError> {
    let folder = format!("namespaces/{namespace}/");
    archived(state, cluster, backup, move |resource, rest| {
        if !v::ROUTES.contains(&resource) {
            return None;
        }
        let name = rest.strip_prefix(&folder)?.strip_suffix(".json")?;
        Some(format!("{resource}/{name}"))
    })
    .await
}

/// The Namespace objects in `backup`'s archive whose names are in `names`, each once.
pub(crate) async fn namespaces(
    state: &AppState,
    cluster: &dyn Cluster,
    backup: &str,
    names: Vec<String>,
) -> Result<Vec<Value>, ApiError> {
    archived(state, cluster, backup, move |resource, rest| {
        if resource != "namespaces" {
            return None;
        }
        let name = rest.strip_prefix("cluster/")?.strip_suffix(".json")?;
        names.iter().any(|n| n == name).then(|| name.to_owned())
    })
    .await
}

/// The objects of `backup`'s archive that `pick` keys, each once by its key.
///
/// Velero writes an object at `resources/<resource.group>/namespaces/<namespace>/<name>.json`,
/// or at `resources/<resource.group>/cluster/<name>.json` for a cluster-scoped one, and again
/// under the folder of its preferred version [velero-layout]. `pick` gets `<resource.group>` and
/// the rest of the path without that folder.
///
/// [velero-layout]: https://velero.io/docs/v1.17/output-file-format/
async fn archived(
    state: &AppState,
    cluster: &dyn Cluster,
    backup: &str,
    pick: impl Fn(&str, &str) -> Option<String> + Send + 'static,
) -> Result<Vec<Value>, ApiError> {
    let body = fetch(state, cluster, backup, Download::BackupContents).await?;
    let packed = collected(body, ARCHIVE_BYTES).await?;
    tokio::task::spawn_blocking(move || objects_in(&packed, &pick))
        .await
        .map_err(|e| e.to_string())
        .flatten()
        .map_err(|e| {
            bad_gateway(format!(
                "The archive of backup {backup} could not be read: {e}"
            ))
        })
}

fn objects_in(
    packed: &[u8],
    pick: &impl Fn(&str, &str) -> Option<String>,
) -> Result<Vec<Value>, String> {
    let unpacked = flate2::read::GzDecoder::new(packed).take(ARCHIVE_BYTES as u64);
    let mut archive = tar::Archive::new(unpacked);
    let mut found: BTreeMap<String, Value> = BTreeMap::new();
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry
            .path()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        let Some((resource, rest)) = path
            .strip_prefix("resources/")
            .and_then(|r| r.split_once('/'))
        else {
            continue;
        };
        let rest = match rest.split_once('/') {
            Some((folder, inner)) if folder.ends_with("-preferredversion") => inner,
            _ => rest,
        };
        let Some(key) = pick(resource, rest) else {
            continue;
        };
        let mut text = Vec::new();
        entry.read_to_end(&mut text).map_err(|e| e.to_string())?;
        let object: Value = serde_json::from_slice(&text).map_err(|e| e.to_string())?;
        found.insert(key, object);
    }
    Ok(found.into_values().collect())
}

/// A Velero backup's contents, as a `.tar.gz` file. Read and written as the person: creating a
/// `DownloadRequest` in Velero's namespace and reading the location's customer key. No URL or
/// key reaches the answer.
#[utoipa::path(get, path = "/api/v1/cluster/backups/{backup}/download", tag = "cluster",
    operation_id = "downloadClusterBackup",
    params(("backup" = String, Path, description = "The backup")),
    responses((status = 200, description = "Velero's archive of the backup's objects", content_type = "application/gzip"),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn download(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(backup): Path<String>,
) -> Result<Response, ApiError> {
    let contents = fetch(&state, cluster.as_ref(), &backup, Download::BackupContents).await?;
    let mut response = Response::new(contents);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/gzip"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        crate::pods::files::disposition(&format!("{backup}.tar.gz")),
    );
    Ok(response)
}
