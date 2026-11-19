//! The objects of a new image service as YAML: `POST /projects/{p}/image-service/yaml`.
//!
//! The "Docker image" form hands them to the YAML editor, for a person who wants to change what
//! the form cannot. It reads nothing from the cluster and writes nothing.

use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_plan::{ImageServiceChange, image_service_objects};

use crate::error::ApiError;
use crate::extract::{Json, Path};
use crate::handlers::{AppState, Person};
use crate::projects::writes::plan_problem;

/// The objects as YAML documents, in the order a plan creates them.
#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceYaml {
    /// One document per object, separated by `---`.
    pub yaml: String,
}

#[utoipa::path(post, path = "/api/v1/projects/{project}/image-service/yaml", tag = "projects",
    operation_id = "previewImageService",
    params(("project" = crate::rules::ProjectName, Path, description = "The project's namespace")),
    request_body = ImageServiceChange,
    responses((status = 200, body = ServiceYaml),
              (status = 400, body = Problem, content_type = PROBLEM),
              (status = 422, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn yaml(
    State(state): State<AppState>,
    Person(_): Person,
    Path(_project): Path<String>,
    Json(change): Json<ImageServiceChange>,
) -> Result<Json<ServiceYaml>, ApiError> {
    let issuer = state.settings.domains.cluster_issuer.as_deref();
    let objects = image_service_objects(&change, issuer).map_err(plan_problem)?;
    let documents = objects
        .iter()
        .map(serde_saphyr::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            ApiError::from(Problem::new(
                Kind::Internal,
                format!("Zeabur could not write the service's objects as YAML: {e}"),
            ))
        })?;
    Ok(Json(ServiceYaml {
        yaml: documents.join("---\n"),
    }))
}
