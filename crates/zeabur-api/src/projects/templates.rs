//! The `template` and `pageServer` changes: a template of Zeabur Catalog, rendered for the project
//! ([`zeabur_catalog::Catalog`]) and opened into the `create` of each object.
//!
//! A `pageServer` change is the `error-pages` template with the project's HTML in its ConfigMap of
//! pages. Each object goes as one YAML document, as a person's own YAML does, so the plan checks
//! its kind with discovery and sends every write as a dry run first.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use zeabur_catalog::{Catalog, RenderError, RenderOptions};
use zeabur_core::problem::{Kind, Problem, ProblemAt};
use zeabur_plan::{Change, ErrorPageHtml, PlanRequest, created_object};

use crate::error::ApiError;

/// The template a `pageServer` change deploys.
pub(crate) const PAGE_SERVER_TEMPLATE: &str = "error-pages";

/// A request with its templates opened, and the digest of what they rendered.
#[derive(Debug)]
pub(crate) struct Opened {
    pub(crate) request: PlanRequest,
    /// SHA-256 of the rendered objects, in hex; `None` when the request has no template.
    pub(crate) digest: Option<String>,
}

/// `request` with each `template` and `pageServer` change opened into the `create` of each object
/// of its template; the request as it is when it has none.
///
/// # Errors
///
/// `422` when a template's domain or inputs fail the catalogue's checks, or the template needs a
/// `ClusterIssuer` and Zeabur has none; `404` for a template this build does not carry; `500`
/// when a template this build carries does not read.
pub(crate) fn open(
    catalog: &Catalog,
    namespace: &str,
    cluster_issuer: Option<&str>,
    request: PlanRequest,
) -> Result<Opened, ApiError> {
    if !request
        .changes
        .iter()
        .any(|c| matches!(c, Change::PageServer(_) | Change::Template(_)))
    {
        return Ok(Opened {
            request,
            digest: None,
        });
    }
    let mut changes = Vec::with_capacity(request.changes.len());
    let mut digest = Sha256::new();
    for (index, change) in request.changes.into_iter().enumerate() {
        let objects = if let Change::Template(template) = &change {
            let inputs = &template.inputs;
            render(
                catalog,
                namespace,
                cluster_issuer,
                &template.template,
                template.domain.as_deref(),
                inputs,
            )
            .map_err(|e| problem(&e, index, &template.template))?
        } else if let Change::PageServer(pages) = &change {
            let mut objects = render(
                catalog,
                namespace,
                cluster_issuer,
                PAGE_SERVER_TEMPLATE,
                None,
                &BTreeMap::new(),
            )
            .map_err(|e| problem(&e, index, PAGE_SERVER_TEMPLATE))?;
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
            digest.update(yaml.len().to_string());
            digest.update(":");
            digest.update(&yaml);
            changes.push(Change::Create {
                object: Value::Null,
                yaml: Some(yaml),
            });
        }
    }
    Ok(Opened {
        request: PlanRequest { changes },
        digest: Some(hex::encode(digest.finalize())),
    })
}

/// Checks each `template` change against the form it came from: `409` when the template's
/// revision is not the one the form was built from, `422` when the request does not create the
/// Secret name and key of one of its generated inputs.
pub(crate) fn check_forms(catalog: &Catalog, request: &PlanRequest) -> Result<(), ApiError> {
    let created = created_secrets(request);
    for (index, change) in request.changes.iter().enumerate() {
        let Change::Template(change) = change else {
            continue;
        };
        let template = catalog
            .template(&change.template)
            .map_err(|e| problem(&e, index, &change.template))?;
        if template.revision != change.revision {
            return Err(at(
                index,
                Problem::new(
                    Kind::Conflict,
                    "A template changed after the form was loaded. Reload and deploy it again.",
                ),
            ));
        }
        let missing = template.inputs.iter().find_map(|input| {
            let secret = input.secret.as_ref().filter(|_| input.generate.is_some())?;
            let key = (secret.name.clone(), secret.key.clone());
            (!created.contains(&key)).then_some((input, secret))
        });
        if let Some((input, secret)) = missing {
            return Err(at(
                index,
                Problem::new(
                    Kind::Invalid,
                    format!(
                        "The request does not create the Secret {} with the key {}, which \
                         holds the generated input {}.",
                        secret.name, secret.key, input.name
                    ),
                ),
            ));
        }
    }
    Ok(())
}

/// The name and key of each Secret value `request` creates, from `data` or `stringData`, in an
/// object or in YAML.
fn created_secrets(request: &PlanRequest) -> BTreeSet<(String, String)> {
    request
        .changes
        .iter()
        .filter_map(|c| created_object(c)?.ok())
        .filter(|o| o.get("kind").and_then(Value::as_str) == Some("Secret"))
        .flat_map(|o| {
            let name = o
                .pointer("/metadata/name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            ["data", "stringData"]
                .into_iter()
                .filter_map(|field| o.get(field).and_then(Value::as_object))
                .flat_map(|data| data.keys().cloned())
                .map(|key| (name.clone(), key))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn at(change: usize, problem: Problem) -> ApiError {
    ApiError::from(Problem {
        at: Some(ProblemAt {
            change,
            line: None,
            field: None,
        }),
        ..problem
    })
}

/// [`open`], for a plan whose templates rendered `planned`.
///
/// # Errors
///
/// As [`open`], and `409` when the templates now render other objects.
pub(crate) fn open_as_planned(
    catalog: &Catalog,
    namespace: &str,
    cluster_issuer: Option<&str>,
    request: PlanRequest,
    planned: Option<&str>,
) -> Result<PlanRequest, ApiError> {
    let opened = open(catalog, namespace, cluster_issuer, request)?;
    if opened.digest.as_deref() != planned {
        return Err(ApiError::from(Problem::new(
            Kind::Conflict,
            "A template in this plan changed after the plan was made.",
        )));
    }
    Ok(opened.request)
}

fn render(
    catalog: &Catalog,
    namespace: &str,
    cluster_issuer: Option<&str>,
    id: &str,
    domain: Option<&str>,
    inputs: &BTreeMap<String, String>,
) -> Result<Vec<Value>, RenderError> {
    catalog.render(
        id,
        &RenderOptions {
            namespace,
            domain,
            cluster_issuer,
            inputs,
        },
    )
}

fn problem(error: &RenderError, change: usize, template: &str) -> ApiError {
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
        format!("{error} ({template})")
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
    use zeabur_plan::TemplateChange;

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
        let objects = created(
            &open(&Catalog::embedded(), "shop", None, request)
                .unwrap()
                .request,
        );
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
                revision: String::new(),
                domain: Some("blog.example.com".to_owned()),
                inputs: [("adminEmail".to_owned(), "nope".to_owned())].into(),
            })],
        };
        let problem = open(&Catalog::embedded(), "shop", Some("letsencrypt"), request)
            .unwrap_err()
            .0;
        assert_eq!(problem.kind, Kind::Invalid);
        assert_eq!(
            problem.at.and_then(|a| a.field).as_deref(),
            Some("inputs.adminEmail")
        );
    }

    fn miniflux(secret: &Value) -> PlanRequest {
        let catalog = Catalog::embedded();
        let revision = catalog.template("miniflux").unwrap().revision;
        serde_json::from_value(json!({"changes": [
            secret,
            {"op": "template", "template": "miniflux", "revision": revision,
             "domain": "rss.example.com"},
        ]}))
        .unwrap()
    }

    #[test]
    fn a_generated_secret_counts_in_data_or_string_data_as_an_object_or_yaml() {
        let cases = [
            json!({"op": "create", "object": {"apiVersion": "v1", "kind": "Secret",
                "metadata": {"name": "miniflux-secrets"},
                "stringData": {"admin-password": "a"}}}),
            json!({"op": "create", "object": {"apiVersion": "v1", "kind": "Secret",
                "metadata": {"name": "miniflux-secrets"}, "data": {"admin-password": "YQ=="}}}),
            json!({"op": "create", "yaml": "apiVersion: v1\nkind: Secret\nmetadata:\n  \
                name: miniflux-secrets\ndata:\n  admin-password: YQ==\n"}),
        ];
        for (position, secret) in cases.iter().enumerate() {
            let request = miniflux(secret);
            assert!(
                check_forms(&Catalog::embedded(), &request).is_ok(),
                "form {position}"
            );
        }
    }

    #[test]
    fn a_form_needs_the_current_revision_and_its_generated_secret() {
        let secret = json!({"op": "create", "object": {"apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": "miniflux-secrets"}, "stringData": {"admin-password": "a"}}});
        let mut stale = miniflux(&secret);
        if let Change::Template(template) = &mut stale.changes[1] {
            template.revision = "old".to_owned();
        }
        let renamed = miniflux(&json!({"op": "create", "object": {"apiVersion": "v1",
            "kind": "Secret", "metadata": {"name": "old-secrets"},
            "stringData": {"admin-password": "a"}}}));
        for (request, kind) in [(stale, Kind::Conflict), (renamed, Kind::Invalid)] {
            let problem = check_forms(&Catalog::embedded(), &request).unwrap_err().0;
            assert_eq!(problem.kind, kind, "{problem:?}");
            assert_eq!(problem.at.map(|a| a.change), Some(1));
        }
    }
}
