//! A bucket's files: browse, download, preview, upload, make a folder, rename and delete, through
//! the store's S3 gateway (see [`crate::cluster::s3`]).
//!
//! Zeabur reads the store's access key from its Secret as the person for each request, so a
//! person who may not read the Secret cannot browse. A change also needs the right to update the
//! bucket's `Bucket` object, which kube-apiserver answers for the person: the gateway's key may do
//! everything in the bucket, so the person's own right decides. No answer carries the key.
//!
//! S3 has no folders. A folder is a key prefix that ends in `/` [s3-folders]; a listing groups the
//! keys under it (`list`), "make folder" writes the empty object `<prefix>/`, and deleting a
//! folder deletes every key under the prefix, a thousand at a time [s3-delete]. S3 cannot rename
//! either; a rename of one object copies it and deletes the old key [s3-copy]. Folders are not
//! renamed.
//!
//! A large upload goes in parts [s3-multipart]: the page starts it, sends each part of at most
//! [`MAX_PART`] bytes, and completes or aborts it. Each part streams through without being held.
//!
//! [s3-folders]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-folders.html
//! [s3-delete]: https://docs.aws.amazon.com/AmazonS3/latest/API/API_DeleteObjects.html
//! [s3-copy]: https://docs.aws.amazon.com/AmazonS3/latest/API/API_CopyObject.html
//! [s3-multipart]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpuoverview.html

#![expect(
    unused_qualifications,
    reason = "utoipa's `path` macro expands `params(…)` to a qualified path"
)]

use std::fmt::Write as _;

use axum::body::Body;
use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use base64::Engine as _;
use http_body_util::BodyExt as _;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use zeabur_core::problem::{Denied, Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::Cluster;
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_modules::seaweedfs;

use crate::cluster::object_store::md5;
use crate::cluster::s3::{self, Keys, S3Request};
use crate::error::{ApiError, get_read, read_problem};
use crate::extract::{Json, Path};
use crate::handlers::{AppState, Person};
use crate::pods::files::disposition;
use crate::projects::backups::read_all;

/// The most keys a listing returns at once; the rest follow with `after`.
pub const MAX_KEYS: usize = 1000;
/// The largest part of an upload, and the largest object uploaded whole, in bytes.
pub const MAX_PART: u64 = 64 * 1024 * 1024;
/// The largest object shown as text or as an image, in bytes.
pub const MAX_PREVIEW: u64 = 1_000_000;
/// The most objects counted before a folder is deleted.
pub const MAX_COUNT: u64 = 10_000;
/// The most of an answer of the gateway's read to say what went wrong.
const MAX_ANSWER: usize = 1024 * 1024;
/// Image types shown in the page; any other object is downloaded as bytes, so the gateway's
/// `Content-Type` cannot make the page run what a bucket holds. SVG can carry scripts and is not
/// one.
const IMAGES: [&str; 5] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/avif",
];

/// One level of a bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BucketListing {
    /// The prefix listed: empty for the bucket's top, else ending in `/`.
    pub prefix: String,
    /// The folders in it, as full prefixes ending in `/`.
    pub folders: Vec<String>,
    /// The objects in it, by key. The folder's own empty object is left out.
    pub objects: Vec<BucketObject>,
    /// Pass as `after` for the next page; absent on the last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// The limits the server keeps.
    pub limits: BucketLimits,
}

/// One object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BucketObject {
    /// Its key.
    pub key: String,
    /// Its size in bytes.
    pub size: u64,
    /// When it was last written, as the gateway says it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

/// The limits of a bucket's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BucketLimits {
    /// The most keys a page lists.
    pub keys: usize,
    /// The largest part of an upload, and the largest object uploaded whole.
    pub part: u64,
    /// The largest object previewed.
    pub preview: u64,
    /// The most objects counted before a folder is deleted.
    pub count: u64,
}

const LIMITS: BucketLimits = BucketLimits {
    keys: MAX_KEYS,
    part: MAX_PART,
    preview: MAX_PREVIEW,
    count: MAX_COUNT,
};

/// An object's preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BucketPreview {
    /// Its key.
    pub key: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its `Content-Type`, as it was uploaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Its text; absent when it is not UTF-8 text or larger than `limits.preview`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// An image the page may show from `…/object?inline=true`.
    pub image: bool,
    /// Larger than `limits.preview`.
    pub too_large: bool,
}

/// What is under a prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BucketCount {
    /// The objects, up to `limits.count`.
    pub objects: u64,
    /// Their bytes.
    pub bytes: u64,
    /// There are more than `limits.count`.
    pub more: bool,
}

/// What a delete removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BucketDeleted {
    /// The objects deleted.
    pub objects: u64,
}

/// A folder to make.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BucketNewFolder {
    /// Its prefix, ending in `/`.
    pub prefix: String,
}

/// A rename of one object.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BucketMove {
    /// The object's key.
    pub from: String,
    /// Its new key; no object may have it.
    pub to: String,
}

/// A started upload in parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BucketUpload {
    /// The upload's ID, for its parts.
    pub upload: String,
}

/// One part sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BucketPart {
    /// Its number, from 1.
    pub number: u16,
    /// The gateway's tag of it.
    pub etag: String,
}

/// The parts of an upload, to complete it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BucketParts {
    /// Every part, by number.
    pub parts: Vec<BucketPart>,
}

/// The parameters that name a bucket's keys.
#[expect(
    dead_code,
    reason = "describes the parameters in the OpenAPI document; parsed by hand"
)]
#[derive(Debug, Default, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct BucketParams {
    /// An object's key.
    key: Option<String>,
    /// A prefix, empty or ending in `/`.
    prefix: Option<String>,
    /// A listing's `next`.
    after: Option<String>,
    /// Shows an image in the page instead of downloading it.
    inline: Option<bool>,
}

#[derive(Debug, Default)]
struct Query {
    key: Option<String>,
    prefix: String,
    after: Option<String>,
    inline: bool,
}

impl Query {
    fn parse(query: Option<&str>) -> Self {
        let mut parsed = Self::default();
        for (name, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
            match name.as_ref() {
                "key" if !value.is_empty() => parsed.key = Some(value.into_owned()),
                "prefix" => parsed.prefix = value.into_owned(),
                "after" if !value.is_empty() => parsed.after = Some(value.into_owned()),
                "inline" => parsed.inline = value == "true",
                _ => {}
            }
        }
        parsed
    }

    fn key(&self) -> Result<&str, ApiError> {
        self.key
            .as_deref()
            .filter(|key| key_valid(key))
            .ok_or_else(|| bad("`key` must name an object: not empty, not ending in `/`."))
    }

    fn prefix(&self) -> Result<&str, ApiError> {
        prefix_valid(&self.prefix)
            .then_some(self.prefix.as_str())
            .ok_or_else(|| bad("`prefix` must be empty or end in `/`."))
    }
}

/// An object's key: S3 takes up to 1024 bytes of UTF-8 [s3-keys]; Zeabur also refuses control
/// characters, a key ending in `/` (a folder) and a key with an empty or `.`/`..` segment.
///
/// [s3-keys]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-keys.html
fn key_valid(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 1024
        && !key.ends_with('/')
        && !key.chars().any(char::is_control)
        && key
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn prefix_valid(prefix: &str) -> bool {
    prefix.is_empty() || prefix.strip_suffix('/').is_some_and(key_valid)
}

fn bad(detail: &str) -> ApiError {
    ApiError::from(Problem::new(Kind::BadRequest, detail))
}

/// A bucket of a store, and how to reach it.
struct Place<'a> {
    state: &'a AppState,
    endpoint: String,
    keys: Keys,
    bucket: String,
    now: Timestamp,
}

impl Place<'_> {
    async fn send(&self, request: S3Request<'_>) -> Result<Response<Body>, ApiError> {
        self.state
            .gateways
            .send(&self.endpoint, &self.keys, self.now, request)
            .await
            .map_err(|e| {
                ApiError::from(Problem::new(
                    Kind::Unavailable,
                    format!("The store's S3 gateway could not be reached: {e}"),
                ))
            })
    }

    /// `request`'s answer when it succeeds, else a problem saying what `what` was.
    async fn ok(&self, request: S3Request<'_>, what: &str) -> Result<Response<Body>, ApiError> {
        let answer = self.send(request).await?;
        if answer.status().is_success() {
            Ok(answer)
        } else {
            Err(refused(answer, &self.bucket, what).await)
        }
    }

    /// `request`'s answer as text, also refused when S3 puts an `<Error>` in a `200`, as
    /// `CopyObject` and `CompleteMultipartUpload` may [s3-copy].
    ///
    /// [s3-copy]: https://docs.aws.amazon.com/AmazonS3/latest/API/API_CopyObject.html
    async fn text(&self, request: S3Request<'_>, what: &str) -> Result<String, ApiError> {
        let answer = self.ok(request, what).await?;
        let text = answer_text(answer, what).await?;
        if text.contains("<Error>") {
            return Err(s3_problem(StatusCode::OK, &text, &self.bucket, what));
        }
        Ok(text)
    }

    fn request<'k>(&'k self, method: Method, key: Option<&'k str>) -> S3Request<'k> {
        S3Request::new(method, &self.bucket, key)
    }
}

/// Finds `bucket` of `store` in `project` and reads its key as the person; with `change`, also
/// asks kube-apiserver whether the person may update the `Bucket`.
async fn place<'a>(
    state: &'a AppState,
    cluster: &dyn Cluster,
    (project, store, bucket): (&str, &str, &str),
    change: bool,
) -> Result<Place<'a>, ApiError> {
    let failed = |e: zeabur_kube::ClusterError| {
        ApiError::from(e.into_problem(&format!("object storage of {project}")))
    };
    let mut unreadable = Vec::new();
    let read = read_all(
        cluster,
        [
            &seaweedfs::SEAWEEDS,
            &seaweedfs::BUCKETS,
            &seaweedfs::CREDENTIALS,
        ],
        Some(project),
        &mut unreadable,
    )
    .await
    .map_err(failed)?;
    let Ok([stores, buckets, credentials]) = read else {
        return Err(Problem::new(
            Kind::NotFound,
            format!("Project {project} has no object storage Zeabur can read."),
        )
        .into());
    };
    if let Some(refused) = unreadable.into_iter().next() {
        return Err(Problem::new(
            Kind::Forbidden,
            format!(
                "The cluster does not let you read {} in project {project}.",
                refused.resource
            ),
        )
        .into());
    }
    let not_found = || {
        ApiError::from(Problem::new(
            Kind::NotFound,
            format!("Store {store} has no bucket {bucket}."),
        ))
    };
    let found = stores
        .iter()
        .find(|s| s.metadata.name.as_deref() == Some(store))
        .map(|s| seaweedfs::object_storage(project, s, &buckets, &credentials, &[]))
        .ok_or_else(not_found)?;
    let object = found
        .buckets
        .iter()
        .find(|b| b.s3_name == bucket)
        .ok_or_else(not_found)?;
    let connection = found.connection.ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Conflict,
            format!("Store {store} has no access key yet, so Zeabur cannot open its buckets."),
        ))
    })?;
    if change {
        may_change(cluster, project, &object.name, bucket).await?;
    }
    let keys = read_keys(cluster, project, store, &connection).await?;
    Ok(Place {
        state,
        endpoint: state.gateways.endpoint(&connection.host, connection.port),
        keys,
        bucket: bucket.to_owned(),
        now: (state.clock)(),
    })
}

/// Refused unless kube-apiserver says the person may update the `Bucket` object `name`, which
/// holds S3 bucket `bucket`.
async fn may_change(
    cluster: &dyn Cluster,
    project: &str,
    name: &str,
    bucket: &str,
) -> Result<(), ApiError> {
    let right = Denied {
        verb: "update".to_owned(),
        group: seaweedfs::BUCKETS.group.to_owned(),
        resource: seaweedfs::BUCKETS.resource.to_owned(),
        name: Some(name.to_owned()),
        namespace: Some(project.to_owned()),
    };
    match cluster.review_access(&right).await {
        Ok(true) => Ok(()),
        Ok(false) => {
            let mut problem = Problem::new(
                Kind::Forbidden,
                format!("The cluster does not let you change bucket {bucket}."),
            );
            problem.deny(right);
            Err(problem.into())
        }
        Err(error) => Err(read_problem(
            error,
            &format!("whether you may change bucket {bucket}"),
            right,
        )),
    }
}

/// The access key of `store` from its Secret, read as the person.
async fn read_keys(
    cluster: &dyn Cluster,
    project: &str,
    store: &str,
    connection: &zeabur_modules::databases::Connection,
) -> Result<Keys, ApiError> {
    let values = cluster
        .reveal_secret(project, &connection.secret)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("the access key of store {store}"),
                get_read("", "secrets", Some(project), &connection.secret),
            )
        })?;
    let value = |key: &str| {
        values
            .get(key)
            .and_then(|v| String::from_utf8(v.clone()).ok())
            .filter(|v| !v.is_empty())
    };
    connection
        .username_key
        .as_deref()
        .and_then(value)
        .zip(value(&connection.password_key))
        .map(|(id, secret)| Keys { id, secret })
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::Conflict,
                format!(
                    "Secret {} has no access key for store {store} yet.",
                    connection.secret
                ),
            ))
        })
}

/// The text of `body`, up to [`MAX_ANSWER`] bytes; `None` when the body breaks off or is longer.
async fn read_text(body: Body) -> Option<String> {
    let limited = http_body_util::Limited::new(body, MAX_ANSWER);
    let collected = limited.collect().await.ok()?;
    Some(String::from_utf8_lossy(&collected.to_bytes()).into_owned())
}

/// The whole text of a successful `answer` to `what`. A body that breaks off or is too long is a
/// problem, never an empty answer: a `200` to `CopyObject` may still hold an `<Error>`, and an
/// empty listing would look like the last page.
async fn answer_text(answer: Response<Body>, what: &str) -> Result<String, ApiError> {
    read_text(answer.into_body()).await.ok_or_else(|| {
        ApiError::from(Problem::new(
            Kind::Unavailable,
            format!(
                "The S3 gateway's answer to {what} broke off or was longer than {MAX_ANSWER} bytes."
            ),
        ))
    })
}

/// The problem of a refused `answer` to `what` in `bucket`.
async fn refused(answer: Response<Body>, bucket: &str, what: &str) -> ApiError {
    let status = answer.status();
    let text = read_text(answer.into_body()).await.unwrap_or_default();
    s3_problem(status, &text, bucket, what)
}

/// The problem of S3's error `xml` with `status`: its `Code` and `Message` only, which name no
/// key [s3-errors].
///
/// [s3-errors]: https://docs.aws.amazon.com/AmazonS3/latest/API/ErrorResponses.html
fn s3_problem(status: StatusCode, xml: &str, bucket: &str, what: &str) -> ApiError {
    let code = s3::element(xml, "Code").unwrap_or_default();
    let message = s3::element(xml, "Message").unwrap_or_default();
    let (kind, detail) = match (status.as_u16(), code.as_str()) {
        (404, _) | (_, "NoSuchKey" | "NoSuchBucket" | "NoSuchUpload") => (
            Kind::NotFound,
            format!("Bucket {bucket} has no {what}: {code} {message}"),
        ),
        (403, _) | (_, "AccessDenied") => (
            Kind::Unavailable,
            format!(
                "The store's access key may not use {what} in bucket {bucket}: {code} {message}"
            ),
        ),
        (400 | 411 | 413, _) | (_, "EntityTooLarge" | "InvalidPart" | "InvalidPartOrder") => (
            Kind::BadRequest,
            format!("The S3 gateway refused {what}: {code} {message}"),
        ),
        _ => (
            Kind::Unavailable,
            format!("The S3 gateway answered {status} for {what}: {code} {message}"),
        ),
    };
    Problem::new(kind, detail.trim_end_matches([' ', ':']).to_owned()).into()
}

/// The `Content-Length` of a request, which an upload needs and which may not be over
/// [`MAX_PART`].
fn upload_length(headers: &HeaderMap) -> Result<u64, ApiError> {
    let size = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or_else(|| bad("An upload needs `Content-Length`."))?;
    if size > MAX_PART {
        return Err(bad(&format!(
            "The part is {size} bytes, more than the {MAX_PART} Zeabur sends at once. Send it in \
             parts."
        )));
    }
    Ok(size)
}

/// The `Content-Type` a request gives the object, if it gives a valid one.
fn content_type(headers: &HeaderMap) -> Option<HeaderValue> {
    headers
        .get(header::CONTENT_TYPE)
        .filter(|v| v.to_str().is_ok_and(|t| !t.is_empty() && t.len() <= 255))
        .cloned()
}

/// The last segment of `key`.
fn name_of(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
}

/// Lists one level of a bucket.
#[utoipa::path(get, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/objects",
    tag = "projects", operation_id = "listBucketObjects",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path), BucketParams),
    responses((status = 200, body = BucketListing),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn list(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket)): Path<(String, String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Json<BucketListing>, ApiError> {
    let query = Query::parse(query.as_deref());
    let prefix = query.prefix()?;
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), false).await?;
    let mut request = place
        .request(Method::GET, None)
        .query("list-type", "2")
        .query("delimiter", "/")
        .query("max-keys", MAX_KEYS.to_string())
        .query("prefix", prefix);
    if let Some(after) = &query.after {
        request = request.query("continuation-token", after.clone());
    }
    let xml = place.text(request, "the listing").await?;
    Ok(Json(listing(prefix, &xml)))
}

/// One level of a `ListObjectsV2` answer [s3-list].
///
/// [s3-list]: https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html
fn listing(prefix: &str, xml: &str) -> BucketListing {
    let folders = s3::raw_elements(xml, "CommonPrefixes")
        .into_iter()
        .filter_map(|p| s3::element(p, "Prefix"))
        .collect();
    let objects = s3::raw_elements(xml, "Contents")
        .into_iter()
        .filter_map(|c| {
            Some(BucketObject {
                key: s3::element(c, "Key")?,
                size: s3::element(c, "Size")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_default(),
                last_modified: s3::element(c, "LastModified"),
            })
        })
        .filter(|o| o.key != prefix)
        .collect();
    let truncated = s3::element(xml, "IsTruncated").as_deref() == Some("true");
    BucketListing {
        prefix: prefix.to_owned(),
        folders,
        objects,
        next: truncated
            .then(|| s3::element(xml, "NextContinuationToken"))
            .flatten(),
        limits: LIMITS,
    }
}

/// Counts what is under a prefix, up to `limits.count` objects.
#[utoipa::path(get, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/objects/count",
    tag = "projects", operation_id = "countBucketObjects",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path), BucketParams),
    responses((status = 200, body = BucketCount),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn count(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket)): Path<(String, String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Json<BucketCount>, ApiError> {
    let query = Query::parse(query.as_deref());
    let prefix = folder_prefix(&query)?;
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), false).await?;
    let mut count = BucketCount {
        objects: 0,
        bytes: 0,
        more: false,
    };
    let mut after: Option<String> = None;
    loop {
        let page = keys_under(&place, prefix, after.as_deref()).await?;
        for (_, size) in &page.keys {
            if count.objects == MAX_COUNT {
                count.more = true;
                return Ok(Json(count));
            }
            count.objects += 1;
            count.bytes += size;
        }
        match page.next {
            Some(next) => after = Some(next),
            None => return Ok(Json(count)),
        }
    }
}

/// A non-empty prefix ending in `/`: a folder.
fn folder_prefix(query: &Query) -> Result<&str, ApiError> {
    let prefix = query.prefix()?;
    if prefix.is_empty() {
        return Err(bad(
            "`prefix` must name a folder. To empty the bucket, delete the bucket.",
        ));
    }
    Ok(prefix)
}

/// One page of every key under a prefix, with sizes.
struct KeysPage {
    keys: Vec<(String, u64)>,
    next: Option<String>,
}

async fn keys_under(
    place: &Place<'_>,
    prefix: &str,
    after: Option<&str>,
) -> Result<KeysPage, ApiError> {
    let mut request = place
        .request(Method::GET, None)
        .query("list-type", "2")
        .query("max-keys", MAX_KEYS.to_string())
        .query("prefix", prefix);
    if let Some(after) = after {
        request = request.query("continuation-token", after);
    }
    let xml = place.text(request, "the listing").await?;
    let keys = s3::raw_elements(&xml, "Contents")
        .into_iter()
        .filter_map(|c| {
            Some((
                s3::element(c, "Key")?,
                s3::element(c, "Size")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_default(),
            ))
        })
        .collect();
    let truncated = s3::element(&xml, "IsTruncated").as_deref() == Some("true");
    Ok(KeysPage {
        keys,
        next: truncated
            .then(|| s3::element(&xml, "NextContinuationToken"))
            .flatten(),
    })
}

/// Downloads an object, or shows an image in the page with `inline`.
#[utoipa::path(get, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/object",
    tag = "projects", operation_id = "downloadBucketObject",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path), BucketParams),
    responses((status = 200, description = "The object's bytes", content_type = "application/octet-stream"),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn download(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket)): Path<(String, String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Response, ApiError> {
    let query = Query::parse(query.as_deref());
    let key = query.key()?;
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), false).await?;
    let answer = place
        .ok(
            place.request(Method::GET, Some(key)),
            &format!("object {key}"),
        )
        .await?;
    let image = answer
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|t| IMAGES.into_iter().find(|i| t.eq_ignore_ascii_case(i)));
    let length = answer.headers().get(header::CONTENT_LENGTH).cloned();
    let mut response = answer.into_body().into_response();
    let headers = response.headers_mut();
    if let Some(image) = image.filter(|_| query.inline) {
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(image));
        headers.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("inline"),
        );
    } else {
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        headers.insert(header::CONTENT_DISPOSITION, disposition(name_of(key)));
    }
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    if let Some(length) = length {
        headers.insert(header::CONTENT_LENGTH, length);
    }
    Ok(response)
}

/// An object as text, up to `limits.preview` bytes, or whether it is an image the page may show.
#[utoipa::path(get, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/object/preview",
    tag = "projects", operation_id = "previewBucketObject",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path), BucketParams),
    responses((status = 200, body = BucketPreview),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn preview(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket)): Path<(String, String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Json<BucketPreview>, ApiError> {
    let query = Query::parse(query.as_deref());
    let key = query.key()?;
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), false).await?;
    let what = format!("object {key}");
    let head = place
        .ok(place.request(Method::HEAD, Some(key)), &what)
        .await?;
    let size = head
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or_default();
    let content_type = head
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let too_large = size > MAX_PREVIEW;
    let image = !too_large
        && content_type
            .as_deref()
            .is_some_and(|t| IMAGES.iter().any(|i| t.eq_ignore_ascii_case(i)));
    let text = if too_large || image {
        None
    } else {
        let answer = place
            .ok(place.request(Method::GET, Some(key)), &what)
            .await?;
        let limit = usize::try_from(MAX_PREVIEW).unwrap_or(usize::MAX);
        let bytes = http_body_util::Limited::new(answer.into_body(), limit)
            .collect()
            .await
            .map_err(|e| {
                ApiError::from(Problem::new(
                    Kind::Unavailable,
                    format!("The S3 gateway stopped sending {what}: {e}"),
                ))
            })?
            .to_bytes();
        (!bytes.contains(&0))
            .then(|| String::from_utf8(bytes.to_vec()).ok())
            .flatten()
    };
    Ok(Json(BucketPreview {
        key: key.to_owned(),
        size,
        content_type,
        text,
        image,
        too_large,
    }))
}

/// Uploads an object of at most `limits.part` bytes whole, replacing one with its key.
#[utoipa::path(put, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/object",
    tag = "projects", operation_id = "uploadBucketObject",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path), BucketParams),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 204, description = "Written"),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn upload(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket)): Path<(String, String, String)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Body,
) -> Result<StatusCode, ApiError> {
    let query = Query::parse(query.as_deref());
    let key = query.key()?;
    let size = upload_length(&headers)?;
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), true).await?;
    let mut request = place
        .request(Method::PUT, Some(key))
        .header(header::CONTENT_LENGTH, HeaderValue::from(size))
        .body(body);
    if let Some(kind) = content_type(&headers) {
        request = request.header(header::CONTENT_TYPE, kind);
    }
    place.ok(request, &format!("the upload of {key}")).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Makes a folder: the empty object `<prefix>`.
#[utoipa::path(post, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/folders",
    tag = "projects", operation_id = "makeBucketFolder",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path)),
    request_body = BucketNewFolder,
    responses((status = 204, description = "Made"),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn make_folder(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket)): Path<(String, String, String)>,
    Json(request): Json<BucketNewFolder>,
) -> Result<StatusCode, ApiError> {
    if request.prefix.is_empty() || !prefix_valid(&request.prefix) {
        return Err(bad("`prefix` must name a folder and end in `/`."));
    }
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), true).await?;
    let put = place
        .request(Method::PUT, Some(&request.prefix))
        .header(header::CONTENT_LENGTH, HeaderValue::from(0_u64));
    place.ok(put, &format!("folder {}", request.prefix)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Renames one object: copies it to the new key, then deletes the old one.
#[utoipa::path(post, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/moves",
    tag = "projects", operation_id = "renameBucketObject",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path)),
    request_body = BucketMove,
    responses((status = 204, description = "Renamed"),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn rename(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket)): Path<(String, String, String)>,
    Json(request): Json<BucketMove>,
) -> Result<StatusCode, ApiError> {
    if !key_valid(&request.from) || !key_valid(&request.to) {
        return Err(bad("`from` and `to` must name objects, not folders."));
    }
    if request.from == request.to {
        return Err(bad("`to` is the object's own key."));
    }
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), true).await?;
    let taken = place
        .send(place.request(Method::HEAD, Some(&request.to)))
        .await?;
    if taken.status().is_success() {
        return Err(Problem::new(
            Kind::Conflict,
            format!("Bucket {bucket} already has an object {}.", request.to),
        )
        .into());
    }
    let source = format!(
        "/{}/{}",
        s3::encode(&bucket, false),
        s3::encode(&request.from, true)
    );
    let copy = place
        .request(Method::PUT, Some(&request.to))
        .header(
            HeaderName::from_static("x-amz-copy-source"),
            HeaderValue::from_str(&source).map_err(|_invalid| bad("`from` cannot be sent."))?,
        )
        .header(header::CONTENT_LENGTH, HeaderValue::from(0_u64));
    place
        .text(copy, &format!("object {}", request.from))
        .await?;
    place
        .ok(
            place.request(Method::DELETE, Some(&request.from)),
            &format!("object {}", request.from),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Deletes an object (`key`) or a folder and everything under it (`prefix`).
#[utoipa::path(delete, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/objects",
    tag = "projects", operation_id = "deleteBucketObjects",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path), BucketParams),
    responses((status = 200, body = BucketDeleted),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn delete(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket)): Path<(String, String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Json<BucketDeleted>, ApiError> {
    let query = Query::parse(query.as_deref());
    if query.key.is_some() {
        let key = query.key()?;
        let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), true).await?;
        place
            .ok(
                place.request(Method::DELETE, Some(key)),
                &format!("object {key}"),
            )
            .await?;
        return Ok(Json(BucketDeleted { objects: 1 }));
    }
    let prefix = folder_prefix(&query)?;
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), true).await?;
    let mut deleted = 0;
    let mut after: Option<String> = None;
    loop {
        let page = keys_under(&place, prefix, after.as_deref()).await?;
        if !page.keys.is_empty() {
            let keys: Vec<&str> = page.keys.iter().map(|(k, _)| k.as_str()).collect();
            deleted += delete_keys(&place, &keys).await?;
        }
        match page.next {
            Some(next) => after = Some(next),
            None => return Ok(Json(BucketDeleted { objects: deleted })),
        }
    }
}

/// Deletes `keys`, at most a thousand, in one `DeleteObjects` [s3-delete], which needs
/// `Content-MD5`.
///
/// [s3-delete]: https://docs.aws.amazon.com/AmazonS3/latest/API/API_DeleteObjects.html
async fn delete_keys(place: &Place<'_>, keys: &[&str]) -> Result<u64, ApiError> {
    let mut xml = String::from("<Delete><Quiet>true</Quiet>");
    for key in keys {
        xml.push_str("<Object><Key>");
        xml.push_str(&s3::escape(key));
        xml.push_str("</Key></Object>");
    }
    xml.push_str("</Delete>");
    let checksum = base64::engine::general_purpose::STANDARD.encode(md5(xml.as_bytes()));
    let request = place
        .request(Method::POST, None)
        .query("delete", "")
        .header(
            HeaderName::from_static("content-md5"),
            HeaderValue::from_str(&checksum)
                .map_err(|_invalid| bad("The checksum cannot be sent."))?,
        )
        .header(header::CONTENT_LENGTH, HeaderValue::from(xml.len()))
        .body(Body::from(xml));
    let answer = place.ok(request, "the delete").await?;
    let text = answer_text(answer, "the delete").await?;
    let failed = s3::raw_elements(&text, "Error");
    if let Some(first) = failed.first() {
        return Err(Problem::new(
            Kind::Unavailable,
            format!(
                "The S3 gateway did not delete {} of {} objects; the first, {}: {} {}",
                failed.len(),
                keys.len(),
                s3::element(first, "Key").unwrap_or_default(),
                s3::element(first, "Code").unwrap_or_default(),
                s3::element(first, "Message").unwrap_or_default(),
            ),
        )
        .into());
    }
    Ok(u64::try_from(keys.len()).unwrap_or_default())
}

/// Starts an upload in parts.
#[utoipa::path(post, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/uploads",
    tag = "projects", operation_id = "startBucketUpload",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path), BucketParams),
    responses((status = 200, body = BucketUpload),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn start_upload(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket)): Path<(String, String, String)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Result<Json<BucketUpload>, ApiError> {
    let query = Query::parse(query.as_deref());
    let key = query.key()?;
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), true).await?;
    let mut request = place
        .request(Method::POST, Some(key))
        .query("uploads", "")
        .header(header::CONTENT_LENGTH, HeaderValue::from(0_u64));
    if let Some(kind) = content_type(&headers) {
        request = request.header(header::CONTENT_TYPE, kind);
    }
    let xml = place.text(request, &format!("the upload of {key}")).await?;
    let upload = s3::element(&xml, "UploadId")
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::Unavailable,
                format!("The S3 gateway started the upload of {key} without an ID."),
            ))
        })?;
    Ok(Json(BucketUpload { upload }))
}

/// Sends one part of an upload, of at most `limits.part` bytes.
#[utoipa::path(put, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/uploads/{upload}/parts/{part}",
    tag = "projects", operation_id = "sendBucketUploadPart",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path), ("upload" = String, Path), ("part" = u16, Path),
           BucketParams),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 200, body = BucketPart),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn upload_part(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket, upload, part)): Path<(String, String, String, String, u16)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<BucketPart>, ApiError> {
    let query = Query::parse(query.as_deref());
    let key = query.key()?;
    if !(1..=10_000).contains(&part) {
        return Err(bad("A part's number is from 1 to 10000."));
    }
    let size = upload_length(&headers)?;
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), true).await?;
    let request = place
        .request(Method::PUT, Some(key))
        .query("partNumber", part.to_string())
        .query("uploadId", upload)
        .header(header::CONTENT_LENGTH, HeaderValue::from(size))
        .body(body);
    let answer = place.ok(request, &format!("part {part} of {key}")).await?;
    let etag = answer
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::Unavailable,
                format!("The S3 gateway took part {part} of {key} without an ETag."),
            ))
        })?;
    Ok(Json(BucketPart { number: part, etag }))
}

/// Completes an upload from its parts [s3-complete].
///
/// [s3-complete]: https://docs.aws.amazon.com/AmazonS3/latest/API/API_CompleteMultipartUpload.html
#[utoipa::path(post, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/uploads/{upload}/complete",
    tag = "projects", operation_id = "completeBucketUpload",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path), ("upload" = String, Path), BucketParams),
    request_body = BucketParts,
    responses((status = 204, description = "Written"),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn complete_upload(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket, upload)): Path<(String, String, String, String)>,
    RawQuery(query): RawQuery,
    Json(request): Json<BucketParts>,
) -> Result<StatusCode, ApiError> {
    let query = Query::parse(query.as_deref());
    let key = query.key()?;
    if request.parts.is_empty()
        || !request
            .parts
            .windows(2)
            .all(|w| w.first().map(|p| p.number) < w.get(1).map(|p| p.number))
    {
        return Err(bad("`parts` must list each part once, by number."));
    }
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), true).await?;
    let mut xml = String::from("<CompleteMultipartUpload>");
    for part in &request.parts {
        let _ignored = write!(
            xml,
            "<Part><PartNumber>{}</PartNumber><ETag>{}</ETag></Part>",
            part.number,
            s3::escape(&part.etag)
        );
    }
    xml.push_str("</CompleteMultipartUpload>");
    let complete = place
        .request(Method::POST, Some(key))
        .query("uploadId", upload)
        .header(header::CONTENT_LENGTH, HeaderValue::from(xml.len()))
        .body(Body::from(xml));
    place
        .text(complete, &format!("the upload of {key}"))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Aborts an upload and drops the parts sent.
#[utoipa::path(delete, path = "/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/uploads/{upload}",
    tag = "projects", operation_id = "abortBucketUpload",
    params(("project" = crate::rules::ProjectName, Path), ("store" = String, Path),
           ("bucket" = String, Path), ("upload" = String, Path), BucketParams),
    responses((status = 204, description = "Aborted"),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn abort_upload(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path((project, store, bucket, upload)): Path<(String, String, String, String)>,
    RawQuery(query): RawQuery,
) -> Result<StatusCode, ApiError> {
    let query = Query::parse(query.as_deref());
    let key = query.key()?;
    let place = place(&state, cluster.as_ref(), (&project, &store, &bucket), true).await?;
    let abort = place
        .request(Method::DELETE, Some(key))
        .query("uploadId", upload);
    place.ok(abort, &format!("the upload of {key}")).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_name_objects_and_prefixes_name_folders() {
        assert!(key_valid("a/b.txt"));
        for bad in ["", "a/", "a//b", "./a", "a/../b", "a\nb"] {
            assert!(!key_valid(bad), "{bad:?}");
        }
        assert!(key_valid(&"x".repeat(1024)));
        assert!(!key_valid(&"x".repeat(1025)));
        assert!(prefix_valid(""));
        assert!(prefix_valid("a/b/"));
        assert!(!prefix_valid("a"));
        assert!(!prefix_valid("/"));
    }

    #[test]
    fn a_listing_has_folders_and_objects_but_not_the_folder_itself() {
        let xml = "<ListBucketResult><Prefix>docs/</Prefix>\
                   <Contents><Key>docs/</Key><Size>0</Size></Contents>\
                   <Contents><Key>docs/a.txt</Key><Size>12</Size>\
                   <LastModified>2026-11-28T01:02:03.000Z</LastModified></Contents>\
                   <CommonPrefixes><Prefix>docs/img/</Prefix></CommonPrefixes>\
                   <IsTruncated>true</IsTruncated>\
                   <NextContinuationToken>abc</NextContinuationToken></ListBucketResult>";
        let listing = listing("docs/", xml);
        assert_eq!(listing.folders, ["docs/img/"]);
        assert_eq!(
            listing.objects,
            [BucketObject {
                key: "docs/a.txt".to_owned(),
                size: 12,
                last_modified: Some("2026-11-28T01:02:03.000Z".to_owned()),
            }]
        );
        assert_eq!(listing.next.as_deref(), Some("abc"));
    }
}
