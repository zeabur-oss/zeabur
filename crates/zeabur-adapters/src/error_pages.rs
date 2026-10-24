//! The page server a project runs for its error pages and maintenance page, and the annotation
//! that keeps the backends of a hostname in maintenance.
//!
//! The page server is what Zeabur Catalog's `error-pages` template deploys. Zeabur finds it by
//! [`PAGE_SERVER_LABEL`], not by name: its Service has the ports [`PAGES_PORT`] and
//! [`MAINTENANCE_PORT`], and its ConfigMap holds the pages by file name, such as `502.html`.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::Service;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::Value;

/// The label, set to `"true"`, on the Service and the ConfigMap of the page server.
pub const PAGE_SERVER_LABEL: &str = "zeabur.com/page-server";

/// The name of the Service port that serves each page by its file name, such as `/502.html`.
pub const PAGES_PORT: &str = "pages";

/// The name of the Service port that answers every request with `maintenance.html` and status
/// 503.
pub const MAINTENANCE_PORT: &str = "maintenance";

/// Annotation on an Ingress or `HTTPRoute`: a JSON object from each hostname in maintenance to
/// the backends it had.
///
/// On an Ingress the value is the `backend` of each path of the hostname's rules, in order. On an
/// `HTTPRoute` it is the `backendRefs` of each rule, in order, or `null` for a rule without them.
pub const MAINTENANCE_ANNOTATION: &str = "zeabur.com/maintenance-backend";

/// A page server's Service: its name and the numbers of its two ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageServer {
    /// The Service's name.
    pub name: String,
    /// The number of its [`PAGES_PORT`].
    pub pages: i32,
    /// The number of its [`MAINTENANCE_PORT`].
    pub maintenance: i32,
}

/// Whether the object carries [`PAGE_SERVER_LABEL`].
#[must_use]
pub fn is_page_server(meta: &ObjectMeta) -> bool {
    crate::label(meta.labels.as_ref(), PAGE_SERVER_LABEL) == Some("true")
}

/// The page server `service` is: `None` without the label or without both ports.
#[must_use]
pub fn page_server(service: &Service) -> Option<PageServer> {
    if !is_page_server(&service.metadata) {
        return None;
    }
    let port = |name: &str| {
        service
            .spec
            .as_ref()?
            .ports
            .iter()
            .flatten()
            .find(|p| p.name.as_deref() == Some(name))
            .map(|p| p.port)
    };
    Some(PageServer {
        name: service.metadata.name.clone()?,
        pages: port(PAGES_PORT)?,
        maintenance: port(MAINTENANCE_PORT)?,
    })
}

/// The hostnames in maintenance, each with the backends it had. Empty without the annotation;
/// `None` when its value is not a JSON object.
#[must_use]
pub fn maintenance(meta: &ObjectMeta) -> Option<BTreeMap<String, Value>> {
    let Some(text) = meta
        .annotations
        .as_ref()
        .and_then(|a| a.get(MAINTENANCE_ANNOTATION))
    else {
        return Some(BTreeMap::new());
    };
    let parsed: serde_json::Map<String, Value> = serde_json::from_str(text).ok()?;
    Some(parsed.into_iter().collect())
}

/// The names of the Services an Ingress hostname in maintenance sent its requests to.
#[must_use]
pub fn ingress_services(backends: &Value) -> Vec<String> {
    backends
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| b.pointer("/service/name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}
