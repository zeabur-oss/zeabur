//! How to install a catalogue component yourself ([`engawa_recommend::install`]).
//!
//! engawa recommends and never installs: the answer is texts to copy (Helm commands, a k3s
//! `HelmChart`, Flux objects, an Argo CD `Application`), rendered from the catalogue embedded in
//! the binary. The only cluster read is the node count, which picks the default profile: more
//! than one node means `multi-node`. Someone who may not list nodes gets `single-node` and can
//! choose the other.

use axum::Json;
use axum::extract::{Path, RawQuery};
use engawa_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use engawa_kube::ClusterExt as _;
use engawa_kube::k8s_openapi::api::core::v1::Node;
use engawa_recommend::install::{InstallError, InstallGuide, PROFILES, guide};
use serde::Serialize;
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::handlers::Person;
use crate::query::ProfileParams;

/// How to install one component, and the profile that fits this cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallAnswer {
    /// The guide for the chosen profile.
    #[serde(flatten)]
    pub guide: InstallGuide,
    /// The profile that fits the cluster's node count (`single-node` when nodes could not be
    /// counted).
    pub default_profile: String,
}

/// How to install a catalogue component yourself: Helm commands, a k3s `HelmChart`, Flux objects
/// or an Argo CD `Application`, with the catalogue's pinned versions and values. engawa runs none
/// of them.
#[utoipa::path(get, path = "/api/v1/cluster/components/{capability}/{component}/install", tag = "cluster",
    params(("capability" = String, Path), ("component" = String, Path), ProfileParams),
    responses((status = 200, body = InstallAnswer), (status = 404, body = Problem, content_type = PROBLEM),
              (status = 400, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn guide_for(
    Person(cluster): Person,
    Path((capability, component)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Json<InstallAnswer>, ApiError> {
    let asked = form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .find(|(name, _)| name == "profile")
        .map(|(_, value)| value.into_owned());
    let default_profile = match cluster.list::<Node>(None).await {
        Ok(nodes) if nodes.len() > 1 => PROFILES[1],
        _ => PROFILES[0],
    };
    let profile = asked.as_deref().unwrap_or(default_profile);
    let guide = guide(&format!("{capability}/{component}"), profile).map_err(|e| {
        ApiError::from(match e {
            InstallError::UnknownComponent(_) => Problem::new(Kind::NotFound, e.to_string()),
            InstallError::UnknownProfile(_) => Problem::new(Kind::BadRequest, e.to_string()),
            InstallError::Catalogue(..) => Problem::new(Kind::Internal, e.to_string()),
        })
    })?;
    Ok(Json(InstallAnswer {
        guide,
        default_profile: default_profile.to_owned(),
    }))
}
