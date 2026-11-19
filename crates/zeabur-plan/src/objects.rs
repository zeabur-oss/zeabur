//! Custom resources Zeabur writes whole: Traefik `Middleware` and Gateway API `HTTPRoute`.
//!
//! k8s-openapi has no types for them, so each declares only the fields Zeabur writes.

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde::Serialize;

/// A Traefik `Middleware`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Middleware {
    pub api_version: String,
    pub kind: &'static str,
    pub metadata: ObjectMeta,
    pub spec: MiddlewareSpec,
}

/// The one section of a `Middleware` spec that Zeabur writes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum MiddlewareSpec {
    RedirectRegex {
        regex: String,
        replacement: String,
        permanent: bool,
    },
    BasicAuth {
        secret: String,
        remove_header: bool,
    },
    IpAllowList {
        source_range: Vec<String>,
    },
    Errors {
        status: Vec<String>,
        service: ErrorsService,
        query: String,
    },
}

/// The Service an `errors` middleware asks for the page.
#[derive(Debug, Serialize)]
pub(crate) struct ErrorsService {
    pub name: String,
    pub port: i32,
}

/// A Gateway API `HTTPRoute`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HttpRouteObject<S> {
    api_version: &'static str,
    kind: &'static str,
    pub metadata: ObjectMeta,
    pub spec: S,
}

impl<S> HttpRouteObject<S> {
    pub(crate) const fn new(metadata: ObjectMeta, spec: S) -> Self {
        Self {
            api_version: "gateway.networking.k8s.io/v1",
            kind: "HTTPRoute",
            metadata,
            spec,
        }
    }
}

/// The spec of a route for one hostname and one backend.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewHttpRouteSpec {
    pub parent_refs: Vec<ParentRef>,
    pub hostnames: Vec<String>,
    pub rules: Vec<NewHttpRouteRule>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ParentRef {
    pub name: String,
    pub namespace: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewHttpRouteRule {
    pub backend_refs: Vec<NewBackendRef>,
}

#[derive(Debug, Serialize)]
pub(crate) struct NewBackendRef {
    pub name: String,
    pub port: i32,
}
