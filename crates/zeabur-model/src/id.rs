//! Resource names: the `id` of every top-level node the API returns.
//!
//! An `id` is a relative resource name in the shape of the API's paths, without the `/api/v1/`
//! prefix: `projects/shop/services/deployment/api`, `projects/shop/resources/configmap/api-env`,
//! `projects/shop/pods/api-5f6b7-k2m4p`, `projects/shop/events/api.17f3a`. It is the client's
//! cache key and link target, and the suffix of the matching API path, so it can be resolved
//! without knowing how Zeabur models the node [aip-122].
//!
//! - **Kinds are lower case** (`deployment`, `statefulset`, `configmap`): AIP-122 asks that
//!   "resource IDs should not use upper-case letters". The API accepts a kind in any case and
//!   answers with lower case; [`crate::WorkloadKind::parse`] and [`crate::ResourceKind::parse`] are
//!   the one place that comparison happens.
//! - **Kind and name are two segments**, not `kind.name`: Kubernetes names are DNS subdomains and
//!   may contain dots.
//! - **The kind is part of the name** because a Deployment and a StatefulSet may share a name.
//! - **`name` stays the Kubernetes name.** AIP-122 reserves `name` for the resource name; Zeabur
//!   keeps `name` as the Kubernetes field people know, and puts the path into `id`.
//!
//! Names are not escaped: every segment is a Kubernetes name (DNS label or subdomain), which
//! contains no `/`.
//!
//! [aip-122]: https://google.aip.dev/122

use crate::{ResourceKind, WorkloadKind};

/// `projects/<project>`.
#[must_use]
pub fn project(project: &str) -> String {
    format!("projects/{project}")
}

/// `projects/<project>/services/<kind>/<name>`.
#[must_use]
pub fn service(project: &str, kind: WorkloadKind, name: &str) -> String {
    format!("projects/{project}/services/{}/{name}", kind.segment())
}

/// `projects/<project>/resources/<kind>/<name>`.
#[must_use]
pub fn resource(project: &str, kind: ResourceKind, name: &str) -> String {
    format!("projects/{project}/resources/{}/{name}", kind.segment())
}

/// `projects/<project>/pods/<name>`.
#[must_use]
pub fn pod(project: &str, name: &str) -> String {
    format!("projects/{project}/pods/{name}")
}

/// `projects/<project>/events/<name>`.
#[must_use]
pub fn event(project: &str, name: &str) -> String {
    format!("projects/{project}/events/{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_lower_case_paths_and_kinds_parse_in_any_case() {
        assert_eq!(
            service("shop", WorkloadKind::StatefulSet, "db.primary"),
            "projects/shop/services/statefulset/db.primary"
        );
        assert_eq!(
            resource("shop", ResourceKind::PersistentVolumeClaim, "data"),
            "projects/shop/resources/persistentvolumeclaim/data"
        );
        for kind in WorkloadKind::ALL {
            assert_eq!(WorkloadKind::parse(kind.segment()), Some(kind));
            assert_eq!(WorkloadKind::parse(kind.as_str()), Some(kind));
            assert_eq!(kind.segment(), kind.as_str().to_ascii_lowercase());
        }
        for kind in ResourceKind::ALL {
            assert_eq!(ResourceKind::parse(kind.segment()), Some(kind));
            assert_eq!(
                ResourceKind::parse(&kind.as_str().to_ascii_uppercase()),
                Some(kind)
            );
            assert_eq!(kind.segment(), kind.as_str().to_ascii_lowercase());
        }
        assert_eq!(WorkloadKind::parse("ReplicaSet"), None);
        assert_eq!(ResourceKind::parse(""), None);
    }
}
