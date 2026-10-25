//! How to install a catalogue component ([`zeabur_catalog::install`]).
//!
//! The answer is texts to copy (Helm commands, a k3s `HelmChart`, Flux objects, an Argo CD
//! `Application`), rendered from the catalogue embedded in the binary. For a component that is
//! only charts and asks for nothing, each object form also lists the objects a plan creates in its
//! namespace (`kube-system`, `flux-system` or `argocd`); that form's controller then installs them.
//! The cluster reads are the node count, which picks the default profile (more than one node means
//! `multi-node`; someone who may not list nodes gets `single-node` and can choose the other), and
//! the served resources: an object form is left out when its controller is not served, and all
//! stay when discovery fails.

use axum::Json;
use axum::extract::{Path, RawQuery};
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_catalog::install::{InstallError, InstallFormat, InstallGuide, PROFILES, guide};
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::k8s_openapi::api::core::v1::Node;
use zeabur_kube::ClusterExt as _;

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

/// How to install a catalogue component: Helm commands, a k3s `HelmChart`, Flux objects or an
/// Argo CD `Application`, with the catalogue's pinned versions and values. An object form is listed
/// only when the cluster serves its controller, with the objects a plan creates.
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
    if let Ok(served) = cluster.served_resources().await {
        guide.formats.retain(|text| {
            controller(text.format).is_none_or(|(group, resource)| {
                served
                    .iter()
                    .any(|r| r.group == group && r.resource == resource)
            })
        });
    }
    Ok(Json(InstallAnswer {
        guide,
        default_profile: default_profile.to_owned(),
    }))
}

/// The resource a form's controller serves: its objects mean nothing in a cluster without it.
const fn controller(format: InstallFormat) -> Option<(&'static str, &'static str)> {
    match format {
        InstallFormat::Helm => None,
        InstallFormat::K3s => Some(("helm.cattle.io", "helmcharts")),
        InstallFormat::Flux => Some(("helm.toolkit.fluxcd.io", "helmreleases")),
        InstallFormat::ArgoCd => Some(("argoproj.io", "applications")),
    }
}
