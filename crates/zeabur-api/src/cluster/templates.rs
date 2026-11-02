//! The templates of Zeabur Catalog this build carries ([`zeabur_catalog::templates()`]).
//!
//! For each one: what it asks for, what it requests, whether the components it needs are there,
//! and what its pods use when idle ([`zeabur_catalog::measured`]).
//!
//! Whether a component is there follows [the catalogue's rules for clients][template-clients].
//!
//! [template-clients]: https://github.com/zeabur-oss/catalog/blob/main/docs/reference/template-clients.mdx

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_catalog::templates::{TemplateInput, TemplateService};
use zeabur_catalog::{Component, RenderError};
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::ServedResource;

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
    /// What the application is, in one sentence, in English: the catalogue's `summary`, else
    /// its description.
    pub tagline: String,
    /// The tagline in other languages (`zh-TW`, `zh-CN`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tagline_locales: BTreeMap<String, String>,
    /// The licence of the application and its images (SPDX).
    pub license: String,
    /// The application's home page.
    pub homepage: String,
    /// The application's version (`2.2.0`).
    pub version: String,
    /// The labels of its objects, which the Secrets of generated values carry too.
    pub labels: BTreeMap<String, String>,
    /// The components it needs, and whether each one is there.
    pub requires: Vec<Requirement>,
    /// Its services.
    pub services: Vec<TemplateService>,
    /// What its services request in all: what the scheduler must find free.
    pub requests: Requests,
    /// What a person gives or the browser generates.
    pub inputs: Vec<TemplateInput>,
    /// What to know before deploying it, in one paragraph, in English; empty when it has none.
    pub summary: String,
    /// The summary in other languages (`zh-TW`, `zh-CN`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub summary_locales: BTreeMap<String, String>,
    /// What to know before deploying it, with how to contain each. In English, with the other
    /// languages in `locales`.
    pub risks: Vec<zeabur_catalog::templates::TemplateRisk>,
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
    Ok(Json(TemplateList { items }))
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
        tagline: template.tagline,
        tagline_locales: template.tagline_locales,
        license: template.license,
        homepage: template.homepage,
        version: template.version,
        labels: template.labels,
        services: template.services,
        inputs: template.inputs,
        summary: template.summary,
        summary_locales: template.summary_locales,
        risks: template.risks,
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use serde_json::{Value, json};
    use zeabur_catalog::Detection;

    use super::*;

    const CASES: &str = concat!(env!("ZEABUR_CATALOG"), "/conformance/template");

    fn component(case: &Value) -> Component {
        let id = case["id"].as_str().unwrap();
        let (capability, name) = id.split_once('/').unwrap();
        let crds = case["detection"]["crds"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|crd| crd.as_str().unwrap().to_owned())
            .collect();
        Component {
            id: name.to_owned(),
            capability: capability.to_owned(),
            display_name: id.to_owned(),
            homepage: String::new(),
            license: String::new(),
            description: String::new(),
            summary: BTreeMap::new(),
            defaults: BTreeMap::new(),
            distributions: Vec::new(),
            detection: Detection {
                crds,
                ..Detection::default()
            },
            tested_version: None,
        }
    }

    /// Runs each requires case of the catalogue's `conformance/template/`.
    #[test]
    fn finds_what_a_template_requires_as_the_catalogue_cases_want() {
        let mut ran = false;
        for entry in fs::read_dir(CASES).unwrap() {
            let path = entry.unwrap().path();
            let case: Value = serde_saphyr::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            let Some(requires) = case["requires"].as_array() else {
                continue;
            };
            ran = true;
            let catalogue: Vec<Component> = case["components"]
                .as_array()
                .unwrap()
                .iter()
                .map(component)
                .collect();
            let served: Option<Vec<ServedResource>> = case.get("served").map(|served| {
                served
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| {
                        let resource = r["resource"].as_str().unwrap();
                        ServedResource::readable(
                            r["group"].as_str().unwrap(),
                            "v1",
                            resource,
                            resource,
                            true,
                        )
                    })
                    .collect()
            });
            let got: serde_json::Map<String, Value> = requires
                .iter()
                .map(|name| {
                    let name = name.as_str().unwrap();
                    let state = requirement(&catalogue, name, served.as_deref()).state;
                    (name.to_owned(), json!(state))
                })
                .collect();
            assert_eq!(Value::Object(got), case["want"], "{}", path.display());
        }
        assert!(ran, "no requires cases in {CASES}");
    }

    #[test]
    fn names_the_component_a_template_requires() {
        let catalogue = zeabur_catalog::catalogue().unwrap();
        assert_eq!(
            requirement(&catalogue, "databases/cloudnative-pg", None).name,
            "CloudNativePG"
        );
    }
}
