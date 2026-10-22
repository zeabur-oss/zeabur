//! How to install a catalogue component ([`zeabur_catalog::install`]).
//!
//! The answer is texts to copy (Helm commands, a k3s `HelmChart`, Flux objects, an Argo CD
//! `Application`), rendered from the catalogue embedded in the binary, and, for a component that
//! is only charts and asks for nothing, the k3s `HelmChart` objects Zeabur can create itself in
//! `kube-system` through a plan (a `create` change for each in project `kube-system`); k3s's Helm
//! controller then installs them. The cluster reads are the node count, which picks the default
//! profile (more than one node means `multi-node`; someone who may not list nodes gets
//! `single-node` and can choose the other), and the served resources, for whether there is a Helm
//! controller at all.

use axum::Json;
use axum::extract::{Path, RawQuery};
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_catalog::install::{
    HereBlocked, InstallError, InstallGuide, InstallHere, PROFILES, guide,
};
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Node;
use zeabur_kube::{Cluster, ClusterExt as _};

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
/// or an Argo CD `Application`, with the catalogue's pinned versions and values. Zeabur runs none
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
            InstallError::UnknownComponent(id) => Problem::new(
                Kind::NotFound,
                format!("Zeabur Catalog has no component {id}."),
            ),
            InstallError::UnknownProfile(profile) => Problem::new(
                Kind::BadRequest,
                format!("There is no profile {profile}; use single-node or multi-node."),
            ),
            InstallError::Catalogue(..) => Problem::new(Kind::Internal, e.to_string()),
        })
    })?;
    let mut guide = guide;
    if guide.here.blocked.is_none() && !serves_helm_charts(cluster.as_ref()).await {
        guide.here = InstallHere {
            objects: Vec::new(),
            blocked: Some(HereBlocked::NoHelmController),
        };
    }
    Ok(Json(InstallAnswer {
        guide,
        default_profile: default_profile.to_owned(),
    }))
}

/// Whether the cluster serves k3s's `HelmChart`; true when discovery fails, so the plan says why.
async fn serves_helm_charts(cluster: &dyn Cluster) -> bool {
    cluster.served_resources().await.map_or(true, |served| {
        served
            .iter()
            .any(|r| r.group == "helm.cattle.io" && r.resource == "helmcharts")
    })
}
