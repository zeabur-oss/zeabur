//! The `pageServer` change: Zeabur Catalog's `error-pages` template, rendered for the project and
//! opened into the `create` of each object.
//!
//! The template denies all traffic to every pod of its namespace, as Zeabur Catalog asks of
//! every template. A project is a namespace that other services share, so that NetworkPolicy is
//! left out. The template's own NetworkPolicy still isolates the page server.

use serde_json::{Value, json};
use zeabur_core::problem::{Kind, Problem};
use zeabur_plan::{Change, ErrorPageHtml, PlanRequest};

use crate::error::ApiError;

/// `request` with each `pageServer` change opened into the `create` of each object of the
/// template; the request as it is when it has none.
///
/// # Errors
///
/// `500` when the template this build carries does not read.
pub(crate) fn open(namespace: &str, request: PlanRequest) -> Result<PlanRequest, ApiError> {
    if !request
        .changes
        .iter()
        .any(|c| matches!(c, Change::PageServer(_)))
    {
        return Ok(request);
    }
    let mut changes = Vec::with_capacity(request.changes.len());
    for change in request.changes {
        if let Change::PageServer(pages) = change {
            changes.extend(
                objects(namespace, &pages)?
                    .into_iter()
                    .map(|object| Change::Create { object, yaml: None }),
            );
        } else {
            changes.push(change);
        }
    }
    Ok(PlanRequest { changes })
}

/// The template's objects in `namespace`, with `pages` in its ConfigMap of pages and without the
/// NetworkPolicy that selects every pod.
fn objects(namespace: &str, pages: &ErrorPageHtml) -> Result<Vec<Value>, ApiError> {
    let text = zeabur_catalog::ERROR_PAGES_MANIFESTS.replace("{{ .Namespace }}", namespace);
    let broken = |detail: String| {
        ApiError::from(Problem::new(
            Kind::Internal,
            format!("The error-pages template this build carries does not read: {detail}"),
        ))
    };
    if text.contains("{{") {
        return Err(broken(
            "it uses a template field other than .Namespace".to_owned(),
        ));
    }
    let documents: Vec<Value> =
        serde_saphyr::from_multiple(&text).map_err(|e| broken(e.to_string()))?;
    let html = [
        ("502.html", &pages.bad_gateway),
        ("503.html", &pages.service_unavailable),
        ("504.html", &pages.gateway_timeout),
        ("maintenance.html", &pages.maintenance),
    ];
    Ok(documents
        .into_iter()
        .filter(|o| !o.is_null() && !every_pod_policy(o))
        .map(|mut object| {
            if zeabur_adapters::error_pages::is_page_server(&meta(&object))
                && object.get("kind").and_then(Value::as_str) == Some("ConfigMap")
                && let Some(data) = object.get_mut("data").and_then(Value::as_object_mut)
            {
                for (key, page) in html {
                    if let Some(page) = page {
                        data.insert(key.to_owned(), json!(page));
                    }
                }
            }
            object
        })
        .collect())
}

fn meta(object: &Value) -> zeabur_kube::k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
    object
        .get("metadata")
        .and_then(|m| serde_json::from_value(m.clone()).ok())
        .unwrap_or_default()
}

/// A NetworkPolicy whose `podSelector` is empty: it selects every pod of the namespace.
fn every_pod_policy(object: &Value) -> bool {
    object.get("kind").and_then(Value::as_str) == Some("NetworkPolicy")
        && object
            .pointer("/spec/podSelector")
            .and_then(Value::as_object)
            .is_none_or(serde_json::Map::is_empty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_into_the_template_without_the_namespace_wide_deny() {
        let request = PlanRequest {
            changes: vec![Change::PageServer(ErrorPageHtml {
                maintenance: Some("<p>Back soon</p>".to_owned()),
                ..ErrorPageHtml::default()
            })],
        };
        let opened = open("shop", request).unwrap();
        let objects: Vec<&Value> = opened
            .changes
            .iter()
            .map(|c| {
                let Change::Create { object, .. } = c else {
                    panic!("not a create: {c:?}");
                };
                object
            })
            .collect();
        let named = |kind: &str, name: &str| {
            objects.iter().copied().find(|o| {
                o["kind"] == kind
                    && o.pointer("/metadata/name").and_then(Value::as_str) == Some(name)
            })
        };
        assert!(named("NetworkPolicy", "default-deny").is_none());
        let own = named("NetworkPolicy", "error-pages").unwrap();
        assert_eq!(own["spec"]["policyTypes"], json!(["Ingress", "Egress"]));
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
        let server = zeabur_adapters::error_pages::page_server(
            &serde_json::from_value((*service).clone()).unwrap(),
        );
        assert!(server.is_some());
    }
}
