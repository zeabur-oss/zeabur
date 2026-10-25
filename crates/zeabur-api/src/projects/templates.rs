//! The `template` and `pageServer` changes: a template of Zeabur Catalog, rendered for the project
//! ([`zeabur_catalog::templates()`]) and opened into the `create` of each object.
//!
//! A `pageServer` change is the `error-pages` template with the project's HTML in its ConfigMap of
//! pages. Each object goes as one YAML document, as a person's own YAML does, so the plan checks
//! its kind with discovery and sends every write as a dry run first.

use serde_json::{Value, json};
use zeabur_catalog::{RenderError, RenderOptions};
use zeabur_core::problem::{Kind, Problem, ProblemAt};
use zeabur_plan::{Change, ErrorPageHtml, PlanRequest, TemplateChange};

use crate::error::ApiError;

/// The template a `pageServer` change deploys.
const PAGE_SERVER_TEMPLATE: &str = "error-pages";

/// `request` with each `template` and `pageServer` change opened into the `create` of each object
/// of its template; the request as it is when it has none.
///
/// # Errors
///
/// `422` when a template's domain or inputs fail the catalogue's checks, or the template needs a
/// `ClusterIssuer` and Zeabur has none; `404` for a template this build does not carry; `500`
/// when a template this build carries does not read.
pub(crate) fn open(
    namespace: &str,
    cluster_issuer: Option<&str>,
    request: PlanRequest,
) -> Result<PlanRequest, ApiError> {
    if !request
        .changes
        .iter()
        .any(|c| matches!(c, Change::PageServer(_) | Change::Template(_)))
    {
        return Ok(request);
    }
    let mut changes = Vec::with_capacity(request.changes.len());
    for (index, change) in request.changes.into_iter().enumerate() {
        let objects = if let Change::Template(template) = &change {
            render(namespace, cluster_issuer, template).map_err(|e| problem(&e, index, template))?
        } else if let Change::PageServer(pages) = &change {
            let template = TemplateChange {
                template: PAGE_SERVER_TEMPLATE.to_owned(),
                domain: None,
                inputs: std::collections::BTreeMap::new(),
            };
            let mut objects = render(namespace, cluster_issuer, &template)
                .map_err(|e| problem(&e, index, &template))?;
            for object in &mut objects {
                write_pages(object, pages);
            }
            objects
        } else {
            changes.push(change);
            continue;
        };
        for object in objects {
            let yaml = serde_saphyr::to_string(&object).map_err(|e| {
                ApiError::from(Problem::new(
                    Kind::Internal,
                    format!("Zeabur could not write a template's object as YAML: {e}"),
                ))
            })?;
            changes.push(Change::Create {
                object: Value::Null,
                yaml: Some(yaml),
            });
        }
    }
    Ok(PlanRequest { changes })
}

fn render(
    namespace: &str,
    cluster_issuer: Option<&str>,
    template: &TemplateChange,
) -> Result<Vec<Value>, RenderError> {
    zeabur_catalog::render(
        &template.template,
        &RenderOptions {
            namespace,
            domain: template.domain.as_deref(),
            cluster_issuer,
            inputs: &template.inputs,
        },
    )
}

fn problem(error: &RenderError, change: usize, template: &TemplateChange) -> ApiError {
    let kind = match error {
        RenderError::UnknownTemplate(_) => Kind::NotFound,
        RenderError::Broken { .. } => Kind::Internal,
        RenderError::Namespace(_)
        | RenderError::NoDomain
        | RenderError::Domain(_)
        | RenderError::NoClusterIssuer
        | RenderError::ClusterIssuer(_)
        | RenderError::Generated(_)
        | RenderError::Missing(_)
        | RenderError::Pattern(_)
        | RenderError::UnknownInput(_) => Kind::Invalid,
    };
    let field = match error {
        RenderError::NoDomain | RenderError::Domain(_) => Some("domain".to_owned()),
        RenderError::Generated(name)
        | RenderError::Missing(name)
        | RenderError::Pattern(name)
        | RenderError::UnknownInput(name) => Some(format!("inputs.{name}")),
        RenderError::UnknownTemplate(_)
        | RenderError::Namespace(_)
        | RenderError::NoClusterIssuer
        | RenderError::ClusterIssuer(_)
        | RenderError::Broken { .. } => None,
    };
    let detail = if matches!(error, RenderError::Broken { .. }) {
        format!("{error} ({})", template.template)
    } else {
        error.to_string()
    };
    ApiError::from(Problem {
        at: Some(ProblemAt {
            change,
            line: None,
            field,
        }),
        ..Problem::new(kind, detail)
    })
}

/// Writes `pages` into `object` when it is the page server's ConfigMap of pages.
fn write_pages(object: &mut Value, pages: &ErrorPageHtml) {
    let html = [
        ("502.html", &pages.bad_gateway),
        ("503.html", &pages.service_unavailable),
        ("504.html", &pages.gateway_timeout),
        ("maintenance.html", &pages.maintenance),
    ];
    if zeabur_model::adapters::error_pages::is_page_server(&meta(object))
        && object.get("kind").and_then(Value::as_str) == Some("ConfigMap")
        && let Some(data) = object.get_mut("data").and_then(Value::as_object_mut)
    {
        for (key, page) in html {
            if let Some(page) = page {
                data.insert(key.to_owned(), json!(page));
            }
        }
    }
}

fn meta(object: &Value) -> zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
    object
        .get("metadata")
        .and_then(|m| serde_json::from_value(m.clone()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn created(request: &PlanRequest) -> Vec<Value> {
        request
            .changes
            .iter()
            .map(|c| {
                let Change::Create {
                    yaml: Some(yaml), ..
                } = c
                else {
                    panic!("not a create from YAML: {c:?}");
                };
                serde_saphyr::from_str(yaml).unwrap()
            })
            .collect()
    }

    #[test]
    fn a_page_server_is_the_error_pages_template_with_the_project_html() {
        let request = PlanRequest {
            changes: vec![Change::PageServer(ErrorPageHtml {
                maintenance: Some("<p>Back soon</p>".to_owned()),
                ..ErrorPageHtml::default()
            })],
        };
        let objects = created(&open("shop", None, request).unwrap());
        let named = |kind: &str, name: &str| {
            objects.iter().find(|o| {
                o["kind"] == kind
                    && o.pointer("/metadata/name").and_then(Value::as_str) == Some(name)
            })
        };
        let pages = named("ConfigMap", "error-pages").unwrap();
        assert_eq!(pages["data"]["maintenance.html"], "<p>Back soon</p>");
        assert!(
            pages["data"]["502.html"]
                .as_str()
                .unwrap()
                .contains("Bad gateway")
        );
        let service = named("Service", "error-pages").unwrap();
        assert_eq!(service.pointer("/metadata/namespace"), Some(&json!("shop")));
        assert!(
            zeabur_model::adapters::error_pages::page_server(
                &serde_json::from_value(service.clone()).unwrap()
            )
            .is_some()
        );
    }

    #[test]
    fn a_wrong_input_points_at_its_field() {
        let request = PlanRequest {
            changes: vec![Change::Template(TemplateChange {
                template: "wordpress".to_owned(),
                domain: Some("blog.example.com".to_owned()),
                inputs: [("adminEmail".to_owned(), "nope".to_owned())].into(),
            })],
        };
        let problem = open("shop", Some("letsencrypt"), request).unwrap_err().0;
        assert_eq!(problem.kind, Kind::Invalid);
        assert_eq!(
            problem.at.and_then(|a| a.field).as_deref(),
            Some("inputs.adminEmail")
        );
    }
}
