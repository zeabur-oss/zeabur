//! `POST /projects/{p}/builds/{b}/source`: pushes an uploaded folder to the registry for a
//! `Build` that builds one ([`super::oci_push`]), and answers the digest to name in the `Build`.
//!
//! Zeabur acts as the person at every step: it reads the `Build` and the upload Secret the
//! controller wrote with the person's own rights, so whoever may not read that Secret cannot
//! upload. The body goes to the registry as it arrives; Zeabur keeps none of it, and the token
//! goes into no answer and no log. Naming the digest in the `Build` is a plan like any other
//! write (`archiveDigest`).

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, header};
use secrecy::SecretString;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{Cluster, ClusterExt as _, ResourceRef};
use zeabur_model::Origin;

use super::oci_push::{MAX_LAYER, PushError, Target};
use crate::error::{ApiError, get_read, read_problem};
use crate::handlers::{AppState, Person};

/// The folder as pushed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadedSource {
    /// The `Build`.
    pub origin: Origin,
    /// The artifact's manifest digest, for the `archiveDigest` change.
    pub digest: String,
    /// The tag it was pushed under.
    pub tag: String,
    /// The compressed size.
    pub size: u64,
}

/// Pushes the request's body, a gzip tar of the folder, as the `Build`'s next source.
///
/// `Content-Length` is the size, at most 256 MiB. `request` is the value of the `uploadToken`
/// change the controller has answered; the upload goes to the registry the token names.
#[utoipa::path(post, path = "/api/v1/projects/{project}/builds/{build}/source", tag = "projects",
    operation_id = "uploadBuildSource",
    params(("project" = String, Path), ("build" = String, Path),
           ("request" = String, Query, description = "The upload token request the Build has answered")),
    request_body(content = Vec<u8>, content_type = "application/gzip"),
    responses((status = 200, body = UploadedSource),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 404, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM),
              (status = 503, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn upload(
    Person(cluster): Person,
    State(state): State<AppState>,
    Path((project, build)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<UploadedSource>, ApiError> {
    let length = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|size| *size > 0)
        .ok_or_else(|| bad("An upload needs `Content-Length`."))?;
    if length > MAX_LAYER {
        return Err(bad(&format!(
            "The folder is {length} bytes packed, more than the {MAX_LAYER} a build takes."
        )));
    }
    let request = query
        .as_deref()
        .and_then(|q| {
            form_urlencoded::parse(q.as_bytes())
                .find_map(|(k, v)| (k == "request").then(|| v.into_owned()))
        })
        .filter(|r| !r.is_empty())
        .ok_or_else(|| bad("Name the upload `request` the Build has answered."))?;

    let target = issued_target(cluster.as_ref(), &state, &project, &build, &request).await?;
    let now = (state.clock)();
    let tag = format!("upload-{}", now.as_second());
    let pushed = state
        .pusher
        .push(&target, &tag, body, length)
        .await
        .map_err(|e| push_problem(&e))?;
    Ok(Json(UploadedSource {
        origin: Origin::complete(
            "builder.zeabur.com/v1alpha1",
            "Build",
            Some(&project),
            &build,
        ),
        digest: pushed.digest,
        tag: pushed.tag,
        size: length,
    }))
}

/// Where and with what the registry takes the folder of `build`: read as the person, from the
/// `Build` and the upload Secret the controller wrote for `request`.
async fn issued_target(
    cluster: &dyn Cluster,
    state: &AppState,
    project: &str,
    build: &str,
    request: &str,
) -> Result<Target, ApiError> {
    let builds = ResourceRef::new("builder.zeabur.com", "v1alpha1", "builds", "Build");
    let object = cluster
        .find_object(&builds, Some(project), build)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("build {build}"),
                get_read("builder.zeabur.com", "builds", Some(project), build),
            )
        })?
        .ok_or_else(|| {
            ApiError::from(Problem::new(
                Kind::NotFound,
                format!("There is no Build named {build}."),
            ))
        })?;
    let data = &object.data;
    if data.pointer("/spec/source/archive").is_none() {
        return Err(bad(&format!(
            "Build {build} does not build an uploaded folder."
        )));
    }
    let answered = data
        .pointer("/status/uploadToken/request")
        .and_then(Value::as_str);
    let expires = data
        .pointer("/status/uploadToken/expiresAt")
        .and_then(Value::as_str)
        .and_then(|t| t.parse::<Timestamp>().ok());
    let now = (state.clock)();
    if answered != Some(request) || expires.is_none_or(|expires| expires <= now) {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            "The upload token is not ready or has expired. Ask for a new one.",
        )));
    }
    let secret_name = data
        .pointer("/status/uploadToken/secretName")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("The Build names no upload Secret."))?;
    let values = cluster
        .reveal_secret(project, secret_name)
        .await
        .map_err(|e| {
            read_problem(
                e,
                &format!("secret {secret_name}"),
                get_read("", "secrets", Some(project), secret_name),
            )
        })?;
    let token = values
        .get("token")
        .and_then(|t| String::from_utf8(t.clone()).ok())
        .map(SecretString::from)
        .ok_or_else(|| bad("The upload Secret holds no token."))?;
    let host = values
        .get(".dockerconfigjson")
        .and_then(|c| serde_json::from_slice::<Value>(c).ok())
        .and_then(|c| c.get("auths")?.as_object()?.keys().next().cloned());
    let address = state
        .settings
        .upload_address
        .clone()
        .or(host)
        .ok_or_else(|| bad("The upload Secret names no registry."))?;

    Ok(Target {
        address,
        repository: format!("{project}/{build}/source"),
        token,
    })
}

fn bad(detail: &str) -> ApiError {
    ApiError::from(Problem::new(Kind::BadRequest, detail))
}

fn push_problem(error: &PushError) -> ApiError {
    ApiError::from(match error {
        PushError::Refused => Problem::new(
            Kind::Conflict,
            "The registry did not accept the upload token. Ask for a new one.",
        ),
        PushError::Length => Problem::new(
            Kind::BadRequest,
            "The upload ended before `Content-Length` bytes arrived, or went on past it.",
        ),
        PushError::Unreachable(why) => Problem::new(
            Kind::Unavailable,
            format!("Zeabur cannot reach the registry: {why}."),
        ),
        PushError::Unexpected(why) => Problem::new(
            Kind::Unavailable,
            format!("The registry did not take the folder: {why}."),
        ),
    })
}
