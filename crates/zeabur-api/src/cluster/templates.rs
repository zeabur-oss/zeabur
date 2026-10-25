//! The templates of Zeabur Catalog this build carries ([`zeabur_catalog::templates()`]).
//!
//! For each one: what it asks for, what it requests, whether the components it needs are there,
//! and what its pods use when idle ([`zeabur_catalog::measured`]).
//!
//! A component is *installed* when kube-apiserver serves one of the CustomResourceDefinitions the
//! catalogue detects it by, and *missing* when it serves none of them. Discovery is readable by
//! every signed-in person, so a person who cannot read the whole cluster gets the same answer as
//! an administrator. A component without such a rule, or a refused discovery, is *unknown*.

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_catalog::templates::{TemplateInput, TemplateService};
use zeabur_catalog::{Component, RenderError};
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::{Cluster, ClusterExt as _, ResourceRef, ServedResource};

use crate::cluster::capacity::Amount;
use crate::error::ApiError;
use crate::handlers::{AppState, Person};

/// A template of Zeabur Catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateView {
    /// Its id (`miniflux`).
    pub id: String,
    /// The application's name (`Miniflux`).
    pub name: String,
    /// The catalogue's description, in English, for operators.
    pub description: String,
    /// The licence of the application and its images (SPDX).
    pub license: String,
    /// The application's home page.
    pub homepage: String,
    /// The components it needs, and whether each one is there.
    pub requires: Vec<Requirement>,
    /// Its services.
    pub services: Vec<TemplateService>,
    /// What its services request in all: what the scheduler must find free.
    pub requests: Requests,
    /// What a person gives or the browser generates.
    pub inputs: Vec<TemplateInput>,
    /// It routes a domain: a deploy gives one.
    pub needs_domain: bool,
    /// It asks cert-manager for certificates through the cluster's issuer
    /// (`GET /api/v1/cluster`, `domains.clusterIssuer`).
    pub needs_cluster_issuer: bool,
    /// What its pods use when idle; absent until Zeabur Catalog has measured it.
    pub idle: Option<zeabur_catalog::IdleUse>,
}

/// CPU and memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Requests {
    /// Millicores.
    pub cpu_millicores: u64,
    /// Bytes.
    pub memory_bytes: u64,
}

/// A component a template needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    /// The capability (`databases`).
    pub capability: String,
    /// The component (`cloudnative-pg`); absent when any component of the capability does.
    pub component: Option<String>,
    /// The component's name (`CloudNativePG`), or the capability's when any component does.
    pub name: String,
    /// Whether it is there.
    pub state: RequirementState,
}

/// Whether a component a template needs is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RequirementState {
    /// kube-apiserver serves its API.
    Installed,
    /// kube-apiserver serves none of its API: install it first.
    Missing,
    /// Zeabur cannot tell.
    Unknown,
}

/// The templates this build carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateList {
    /// Every template, by id.
    pub items: Vec<TemplateView>,
    /// The address of kube-apiserver as the `kubernetes` Service's endpoints give it, as an
    /// IPv4 `/32`: what the catalogue's own tests give the `kubeApiCidr` input. Absent when the
    /// person may not read it or it is not IPv4.
    pub kube_api_cidr: Option<String>,
}

/// The templates this build carries, with what each one needs and whether the cluster has the
/// components for it, read as the person.
#[utoipa::path(get, path = "/api/v1/cluster/templates", tag = "cluster", operation_id = "listTemplates",
    responses((status = 200, body = TemplateList)))]
pub(crate) async fn list(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<TemplateList>, ApiError> {
    let templates = zeabur_catalog::templates().map_err(|e| render_problem(&e))?;
    let served = cluster.served_resources().await.ok();
    let items = templates
        .into_iter()
        .map(|t| view(&state, t, served.as_deref()))
        .collect();
    Ok(Json(TemplateList {
        items,
        kube_api_cidr: kube_api_cidr(cluster.as_ref()).await,
    }))
}

/// A template this build carries, what it needs, and what its pods use when idle, so a deploy can
/// say whether the cluster has room for it. Readable by every signed-in person.
#[utoipa::path(get, path = "/api/v1/cluster/templates/{template}", tag = "cluster", operation_id = "getTemplate",
    params(("template" = String, Path, description = "The template's id (`error-pages`).")),
    responses((status = 200, body = TemplateView), (status = 404, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get(
    State(state): State<AppState>,
    Person(cluster): Person,
    Path(template): Path<String>,
) -> Result<Json<TemplateView>, ApiError> {
    let template = zeabur_catalog::template(&template).map_err(|e| render_problem(&e))?;
    let served = cluster.served_resources().await.ok();
    Ok(Json(view(&state, template, served.as_deref())))
}

fn render_problem(error: &RenderError) -> ApiError {
    let kind = if matches!(error, RenderError::UnknownTemplate(_)) {
        Kind::NotFound
    } else {
        Kind::Internal
    };
    ApiError::from(Problem::new(kind, error.to_string()))
}

fn view(
    state: &AppState,
    template: zeabur_catalog::Template,
    served: Option<&[ServedResource]>,
) -> TemplateView {
    let mut requests = Amount::default();
    for service in &template.services {
        requests.add(Amount::of(&service.requests));
    }
    TemplateView {
        requires: template
            .requires
            .iter()
            .map(|r| requirement(&state.catalogue, r, served))
            .collect(),
        requests: Requests {
            cpu_millicores: requests.cpu,
            memory_bytes: requests.memory,
        },
        idle: (state.template_idle)(&template.id),
        id: template.id,
        name: template.name,
        description: template.description,
        license: template.license,
        homepage: template.homepage,
        services: template.services,
        inputs: template.inputs,
        needs_domain: template.needs_domain,
        needs_cluster_issuer: template.needs_cluster_issuer,
    }
}

/// `wanted` (`databases/cloudnative-pg`, or `databases`), and whether kube-apiserver serves the
/// CustomResourceDefinitions its components are detected by.
fn requirement(
    catalogue: &[Component],
    wanted: &str,
    served: Option<&[ServedResource]>,
) -> Requirement {
    let (capability, component) = match wanted.split_once('/') {
        Some((capability, component)) => (capability, Some(component)),
        None => (wanted, None),
    };
    let components: Vec<&Component> = catalogue
        .iter()
        .filter(|c| c.capability == capability && component.is_none_or(|id| c.id == id))
        .collect();
    let crds: Vec<&String> = components.iter().flat_map(|c| &c.detection.crds).collect();
    let state = match served {
        _ if crds.is_empty() => RequirementState::Unknown,
        None => RequirementState::Unknown,
        Some(served) if crds.iter().any(|crd| is_served(served, crd)) => {
            RequirementState::Installed
        }
        Some(_) => RequirementState::Missing,
    };
    Requirement {
        capability: capability.to_owned(),
        component: component.map(str::to_owned),
        name: match components.as_slice() {
            [one] => one.display_name.clone(),
            _ => capability.to_owned(),
        },
        state,
    }
}

/// Whether discovery names the resource of the CustomResourceDefinition `crd`, whose name is
/// `<plural>.<group>`.
fn is_served(served: &[ServedResource], crd: &str) -> bool {
    served
        .iter()
        .any(|r| crd.split_once('.') == Some((r.resource.as_str(), r.group.as_str())))
}

/// The first IPv4 address of the `kubernetes` Service's `EndpointSlice` in `default`, as a `/32`.
/// kube-apiserver writes that slice under the Service's name.
async fn kube_api_cidr(cluster: &dyn Cluster) -> Option<String> {
    let slices = ResourceRef::new("discovery.k8s.io", "v1", "endpointslices", "EndpointSlice");
    let slice = cluster
        .find_object(&slices, Some("default"), "kubernetes")
        .await
        .ok()
        .flatten()?;
    let slice = serde_json::to_value(slice).ok()?;
    if slice.get("addressType").and_then(serde_json::Value::as_str) != Some("IPv4") {
        return None;
    }
    let address = slice
        .pointer("/endpoints/0/addresses/0")
        .and_then(serde_json::Value::as_str)?
        .parse::<std::net::Ipv4Addr>()
        .ok()?;
    Some(format!("{address}/32"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_component_is_there_when_one_of_its_crds_is_served() {
        let catalogue = zeabur_catalog::catalogue().unwrap();
        let cnpg =
            ServedResource::readable("postgresql.cnpg.io", "v1", "clusters", "Cluster", true);
        let state = |wanted: &str, served: Option<&[ServedResource]>| {
            requirement(&catalogue, wanted, served).state
        };
        let with = [cnpg];
        assert_eq!(
            state("databases/cloudnative-pg", Some(&with)),
            RequirementState::Installed
        );
        assert_eq!(
            state("databases/percona-mongodb", Some(&with)),
            RequirementState::Missing
        );
        assert_eq!(state("databases", Some(&with)), RequirementState::Installed);
        assert_eq!(
            state("databases/cloudnative-pg", None),
            RequirementState::Unknown
        );
        assert_eq!(
            requirement(&catalogue, "databases/cloudnative-pg", Some(&with)).name,
            "CloudNativePG"
        );
    }
}
