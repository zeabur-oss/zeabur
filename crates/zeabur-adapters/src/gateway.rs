//! The annotation that keeps the rules of a Gateway API `HTTPRoute` while it redirects.

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::Value;

/// Annotation on an `HTTPRoute` that redirects: its `spec.rules` from before, as JSON.
///
/// Gateway API refuses a `RequestRedirect` filter next to `backendRefs`, so the redirect replaces
/// the rules, and stopping it writes these back.
pub const REDIRECT_RULES_ANNOTATION: &str = "zeabur.com/redirect-rules";

/// The rules kept while the route redirects. `None` without the annotation, or when its value
/// is not a JSON array.
#[must_use]
pub fn redirect_rules(meta: &ObjectMeta) -> Option<Vec<Value>> {
    let text = meta.annotations.as_ref()?.get(REDIRECT_RULES_ANNOTATION)?;
    serde_json::from_str(text).ok()
}
