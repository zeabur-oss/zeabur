//! Docker Compose import: `POST /projects/{p}/compose` reads a Compose file as the services it
//! makes ([`zeabur_plan::compose`]). It reads nothing from the cluster and writes nothing; the
//! web interface plans the objects as the "Docker image" and database forms do.

use axum::Json;
use axum::extract::{Path, State};
use zeabur_core::problem::MEDIA_TYPE as PROBLEM;
use zeabur_plan::compose::{ComposeRequest, Conversion, convert};

use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::projects::writes::plan_problem;

#[utoipa::path(post, path = "/api/v1/projects/{project}/compose", tag = "projects",
    operation_id = "convertCompose",
    params(("project" = String, Path, description = "The project's namespace")),
    request_body = ComposeRequest,
    responses((status = 200, body = Conversion),
              (status = 400, body = zeabur_core::problem::Problem, content_type = PROBLEM)))]
pub(crate) async fn convert_compose(
    State(state): State<AppState>,
    Person(_): Person,
    Path(_project): Path<String>,
    Json(request): Json<ComposeRequest>,
) -> Result<Json<Conversion>, ApiError> {
    convert(&request, &state.settings.terminal.debug_image)
        .map(Json)
        .map_err(|mut error| {
            if let Some(line) = error.line {
                error.detail = format!("Line {line}: {}", error.detail);
            }
            plan_problem(error)
        })
}
