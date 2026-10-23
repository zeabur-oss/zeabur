//! The error-page server Zeabur runs in a project, and the annotation that keeps the backends of
//! a hostname in maintenance.
//!
//! The server is a Deployment, a Service and a ConfigMap, all named [`SERVER`].

use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::Value;

/// The name of the Deployment, Service and ConfigMap of the error-page server.
pub const SERVER: &str = "zeabur-error-pages";

/// The Service port that serves each page by its file name, such as `/502.html`.
pub const PAGES_PORT: i32 = 8080;

/// The Service port that answers every request with `maintenance.html` and status 503.
pub const MAINTENANCE_PORT: i32 = 8081;

/// Annotation on an Ingress or `HTTPRoute`: a JSON object from each hostname in maintenance to
/// the backends it had.
///
/// On an Ingress the value is the `backend` of each path of the hostname's rules, in order. On an
/// `HTTPRoute` it is the `backendRefs` of each rule, in order, or `null` for a rule without them.
pub const MAINTENANCE_ANNOTATION: &str = "zeabur.com/maintenance-backend";

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
