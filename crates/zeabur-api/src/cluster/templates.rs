//! The templates of Zeabur Catalog this build carries, with what each uses when idle
//! ([`zeabur_catalog::measured`]).

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};

use crate::error::ApiError;
use crate::handlers::{AppState, Person};

/// A template of Zeabur Catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateView {
    /// Its id (`error-pages`).
    pub id: String,
    /// What its pods use when idle; absent until Zeabur Catalog has measured it.
    pub idle: Option<zeabur_catalog::IdleUse>,
}

/// A template this build carries, and what its pods use when idle, so a deploy can say whether
/// the cluster has room for it. Readable by every signed-in person.
#[utoipa::path(get, path = "/api/v1/cluster/templates/{template}", tag = "cluster", operation_id = "getTemplate",
    params(("template" = String, Path, description = "The template's id (`error-pages`).")),
    responses((status = 200, body = TemplateView), (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(_): Person,
    Path(template): Path<String>,
) -> Result<Json<TemplateView>, ApiError> {
    if !zeabur_catalog::TEMPLATES.contains(&template.as_str()) {
        return Err(ApiError::from(Problem::new(
            Kind::NotFound,
            format!("This build of Zeabur does not carry the template {template}."),
        )));
    }
    Ok(Json(TemplateView {
        idle: (state.template_idle)(&template),
        id: template,
    }))
}
