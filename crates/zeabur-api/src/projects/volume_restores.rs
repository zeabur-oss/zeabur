//! Restoring a volume from its K8up backups:
//!
//! - `GET /projects/{p}/volumes/{claim}/restores`: the snapshots of the claim, its restores, and
//!   the parts of a `Restore` the interface puts in a plan (the backup's backend and Pod security,
//!   and the S3 method that exports a snapshot).
//! - `GET /projects/{p}/volumes/{claim}/restores/{restore}/download`: the file an export wrote,
//!   streamed.
//!
//! An export is in the bucket of the claim's backups ([`zeabur_modules::k8up`]). Zeabur reads the
//! bucket's keys from the backup's Secret as the person, lists the files the export can have
//! written and fetches the one of its snapshot, with requests it signs for a short time
//! ([`crate::cluster::object_store`]). No URL or key reaches the page. Requests go only to the
//! endpoint of the claim's backups.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::Response;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, DynamicObject};
use zeabur_modules::backups::Outcome;
use zeabur_modules::k8up::{self, RestoreView, SnapshotView};

use super::backups::{ProviderState, read_all};
use crate::cluster::object_store::{Keys, authority, fetched, presigned_get};
use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::{AppState, Person};
use crate::views::UnreadableView;

/// The most of a listing read.
const MAX_LISTING: usize = 1024 * 1024;

/// What a claim can be restored from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VolumeRestores {
    /// Whether K8up's objects could be read.
    pub state: ProviderState,
    /// The schedule of the claim's backups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// The snapshots of the claim, newest first. Only these times can be restored.
    pub snapshots: Vec<SnapshotView>,
    /// The restores of the claim's backups, newest first.
    pub restores: Vec<RestoreView>,
    /// A `Restore` with the backup's backend and Pod security, for a snapshot and a method.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<Value>,
    /// The `restoreMethod` that exports a snapshot to the bucket of the backups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export: Option<Value>,
    /// What could not be read.
    pub unreadable: Vec<UnreadableView>,
}

async fn k8up_objects(
    cluster: &dyn Cluster,
    project: &str,
    unreadable: &mut Vec<UnreadableView>,
) -> Result<Result<[Vec<DynamicObject>; 3], ProviderState>, ApiError> {
    read_all(
        cluster,
        [&k8up::SCHEDULES, &k8up::SNAPSHOTS, &k8up::RESTORES],
        Some(project),
        unreadable,
    )
    .await
    .map_err(|e| ApiError::from(e.into_problem(&format!("volume restores of {project}"))))
}

/// What `claim` can be restored from: the snapshots K8up lists for it, its restores, and the
/// parts of a `Restore`, read as the person.
#[utoipa::path(get, path = "/api/v1/projects/{project}/volumes/{claim}/restores", tag = "projects",
    operation_id = "getVolumeRestores",
    params(("project" = String, Path, description = "The project's namespace"),
           ("claim" = String, Path, description = "The claim")),
    responses((status = 200, body = VolumeRestores), (status = 403, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    Person(cluster): Person,
    Path((project, claim)): Path<(String, String)>,
) -> Result<Json<VolumeRestores>, ApiError> {
    let mut unreadable = Vec::new();
    let found = k8up_objects(cluster.as_ref(), &project, &mut unreadable).await?;
    let mut out = VolumeRestores {
        state: ProviderState::Installed,
        schedule: None,
        snapshots: Vec::new(),
        restores: Vec::new(),
        template: None,
        export: None,
        unreadable,
    };
    match found {
        Ok([schedules, snapshots, restores]) => {
            out.restores = k8up::restores(&restores, &claim);
            if let Some(schedule) = k8up::schedule_of(&schedules, &claim) {
                out.schedule.clone_from(&schedule.metadata.name);
                out.snapshots = k8up::snapshots(&snapshots, schedule, &claim);
                out.template = Some(k8up::restore_template(schedule, &claim));
                out.export = k8up::export_method(schedule);
            }
        }
        Err(state) => out.state = state,
    }
    Ok(Json(out))
}

fn unavailable(detail: String) -> ApiError {
    ApiError::from(Problem::new(Kind::Unavailable, detail))
}

fn invalid(detail: String) -> ApiError {
    ApiError::from(Problem::new(Kind::Invalid, detail))
}

/// One value of a Secret, by a `{name, key}` reference, read as the person.
async fn secret_value(
    cluster: &dyn Cluster,
    project: &str,
    reference: Option<&Value>,
) -> Result<String, ApiError> {
    let name = reference
        .and_then(|r| r["name"].as_str())
        .unwrap_or_default();
    let key = reference
        .and_then(|r| r["key"].as_str())
        .unwrap_or_default();
    if name.is_empty() || key.is_empty() {
        return Err(invalid(
            "The export names no Secret with the bucket's keys.".to_owned(),
        ));
    }
    let values = cluster.reveal_secret(project, name).await.map_err(|e| {
        read_problem(
            e,
            &format!("Secret {name} in {project}"),
            get_read("", "secrets", Some(project), name),
        )
    })?;
    values
        .get(key)
        .and_then(|v| String::from_utf8(v.clone()).ok())
        .ok_or_else(|| invalid(format!("Secret {name} has no key {key:?}.")))
}

/// K8up's schedules, snapshots and restores in `project`, or why they cannot be read.
async fn readable_k8up(
    cluster: &dyn Cluster,
    project: &str,
) -> Result<[Vec<DynamicObject>; 3], ApiError> {
    let mut unreadable = Vec::new();
    k8up_objects(cluster, project, &mut unreadable)
        .await?
        .map_err(|_state| {
            if unreadable.is_empty() {
                unavailable("K8up is not installed.".to_owned())
            } else {
                ApiError::from(Problem::new(
                    Kind::Forbidden,
                    format!("K8up's objects in {project} could not be read."),
                ))
            }
        })
}

/// The export `restore` of `claim`, once it succeeded: its view and its `restoreMethod.s3`.
fn finished_export<'a>(
    restores: &'a [DynamicObject],
    claim: &str,
    restore: &str,
) -> Result<(RestoreView, &'a Value), ApiError> {
    let object = restores
        .iter()
        .find(|r| r.metadata.name.as_deref() == Some(restore))
        .filter(|r| {
            r.metadata
                .labels
                .as_ref()
                .and_then(|l| l.get(k8up::VOLUME_LABEL))
                .map(String::as_str)
                == Some(claim)
        })
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::NotFound,
                format!("{claim} has no restore {restore:?}."),
            ))
        })?;
    let view = k8up::restores(std::slice::from_ref(object), claim)
        .pop()
        .ok_or_else(|| invalid(format!("Restore {restore} is not of {claim}.")))?;
    if view.outcome != Outcome::Succeeded {
        return Err(invalid(format!(
            "Restore {restore} has not finished, so there is nothing to download."
        )));
    }
    let s3 = object
        .data
        .pointer("/spec/restoreMethod/s3")
        .ok_or_else(|| {
            invalid(format!(
                "Restore {restore} restores into a volume, not a file."
            ))
        })?;
    Ok((view, s3))
}

/// When the snapshot `id` (or a prefix of it, as a `Restore` names it) was taken.
fn snapshot_date(snapshots: &[DynamicObject], id: &str) -> Option<String> {
    snapshots.iter().find_map(|s| {
        let text = |p: &str| s.data.pointer(p).and_then(Value::as_str);
        let full = text("/spec/id")?;
        (full.starts_with(id) || id.starts_with(full))
            .then(|| text("/spec/date").map(str::to_owned))?
    })
}

/// The files of a `ListObjectsV2` answer: each key and when it was written.
fn listed(xml: &str) -> Vec<(String, String)> {
    let field = |part: &str, tag: &str| {
        let open = format!("<{tag}>");
        let start = part.find(&open)? + open.len();
        let end = part.get(start..)?.find(&format!("</{tag}>"))? + start;
        Some(
            part.get(start..end)?
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&"),
        )
    };
    xml.split("<Contents>")
        .skip(1)
        .filter_map(|part| {
            Some((
                field(part, "Key")?,
                field(part, "LastModified").unwrap_or_default(),
            ))
        })
        .collect()
}

/// The file an export of the snapshot taken at `date` wrote: the one named `<prefix><time>.tar.gz`
/// for that time, else the newest.
fn exported(mut files: Vec<(String, String)>, prefix: &str, date: Option<&str>) -> Option<String> {
    let at = date.and_then(|d| d.parse::<Timestamp>().ok());
    let time = |key: &str| {
        key.strip_prefix(prefix)?
            .strip_suffix(".tar.gz")?
            .parse::<Timestamp>()
            .ok()
    };
    if let Some(found) = files
        .iter()
        .find(|(key, _)| at.is_some() && time(key) == at)
    {
        return Some(found.0.clone());
    }
    files.sort_by(|a, b| b.1.cmp(&a.1));
    files.into_iter().next().map(|(key, _)| key)
}

/// The file an export of `claim`'s backup wrote, as a `.tar.gz` file. Read as the person: the
/// `Restore`, the claim's backup schedule and the keys of its bucket. No URL or key reaches the
/// answer.
#[utoipa::path(get, path = "/api/v1/projects/{project}/volumes/{claim}/restores/{restore}/download", tag = "projects",
    operation_id = "downloadVolumeExport",
    params(("project" = String, Path, description = "The project's namespace"),
           ("claim" = String, Path, description = "The claim"),
           ("restore" = String, Path, description = "The export's `Restore`")),
    responses((status = 200, description = "The snapshot's files", content_type = "application/gzip"),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn download(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, claim, restore)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let cluster = cluster.as_ref();
    let [schedules, snapshots, restores] = readable_k8up(cluster, &project).await?;
    let (view, s3) = finished_export(&restores, &claim, &restore)?;
    let schedule = k8up::schedule_of(&schedules, &claim)
        .ok_or_else(|| invalid(format!("{claim} has no backup schedule.")))?;
    let backups = k8up::export_method(schedule)
        .ok_or_else(|| invalid(format!("The backups of {claim} go to no S3 bucket.")))?;
    let field = |method: Option<&Value>, name: &str| {
        method
            .and_then(|m| m.get(name))
            .and_then(Value::as_str)
            .map_or_else(String::new, |v| v.trim_end_matches('/').to_owned())
    };
    let (base, bucket) = (field(Some(s3), "endpoint"), field(Some(s3), "bucket"));
    let ours = backups.get("s3");
    if bucket.is_empty() || base != field(ours, "endpoint") || bucket != field(ours, "bucket") {
        return Err(invalid(format!(
            "Restore {restore} exports to another place than the bucket of the backups."
        )));
    }
    if authority(&base).is_none() {
        return Err(invalid("The export's endpoint is not a URL.".to_owned()));
    }
    let access = secret_value(cluster, &project, s3.get("accessKeyIDSecretRef")).await?;
    let secret = secret_value(cluster, &project, s3.get("secretAccessKeySecretRef")).await?;
    let keys = Keys {
        access: &access,
        secret: &secret,
        region: "",
    };
    let now = (state.clock)();
    let prefix = k8up::export_prefix(&project, &claim);
    let list = presigned_get(
        &base,
        &format!("/{bucket}"),
        &[("list-type", "2"), ("prefix", &prefix)],
        &keys,
        now,
    )
    .ok_or_else(|| invalid("The export's endpoint is not a URL.".to_owned()))?;
    let listing = fetched(
        &state.store,
        &list,
        Vec::new(),
        "The bucket",
        "when its files were listed",
    )
    .await?;
    let body = axum::body::to_bytes(listing, MAX_LISTING)
        .await
        .map_err(|e| unavailable(format!("The bucket's listing could not be read: {e}")))?;
    let at = snapshot_date(&snapshots, &view.snapshot);
    let key = exported(
        listed(&String::from_utf8_lossy(&body)),
        &prefix,
        at.as_deref(),
    )
    .ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::NotFound,
            format!("The bucket has no file of restore {restore}."),
        ))
    })?;
    let url = presigned_get(&base, &format!("/{bucket}/{key}"), &[], &keys, now)
        .ok_or_else(|| invalid("The export's endpoint is not a URL.".to_owned()))?;
    let file = fetched(
        &state.store,
        &url,
        Vec::new(),
        "The bucket",
        "for the exported file",
    )
    .await?;
    let mut response = Response::new(file);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/gzip"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        crate::pods::files::disposition(&key),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_export_of_a_snapshot_is_the_file_named_for_its_time_else_the_newest() {
        let xml = "<ListBucketResult><Contents><Key>backup-shop-data-2026-10-24T03:00:00Z.tar.gz</Key>\
            <LastModified>2026-10-25T09:00:00.000Z</LastModified></Contents>\
            <Contents><Key>backup-shop-data-2026-10-25T03:00:00+08:00.tar.gz</Key>\
            <LastModified>2026-10-25T08:00:00.000Z</LastModified></Contents></ListBucketResult>";
        let files = listed(xml);
        assert_eq!(
            exported(
                files.clone(),
                "backup-shop-data-",
                Some("2026-10-24T19:00:00Z")
            )
            .as_deref(),
            Some("backup-shop-data-2026-10-25T03:00:00+08:00.tar.gz")
        );
        assert_eq!(
            exported(files, "backup-shop-data-", None).as_deref(),
            Some("backup-shop-data-2026-10-24T03:00:00Z.tar.gz")
        );
        assert_eq!(
            exported(listed("<ListBucketResult/>"), "backup-shop-data-", None),
            None
        );
    }
}
