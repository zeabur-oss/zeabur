//! A project's build limits: the memory and CPU one build may use, kept as annotations on its
//! namespace (`builder.zeabur.com/build-memory`, `…/build-cpu`) for Zeabur Builder to read.
//! Without an annotation, the cluster's default applies.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::kernel::names::{ANNOTATION_BUILD_CPU, ANNOTATION_BUILD_MEMORY};
use crate::kernel::quantity;

/// Whether `value` is a positive Kubernetes quantity, as Zeabur Builder accepts one.
#[must_use]
pub fn is_build_quantity(value: &str) -> bool {
    quantity::bytes(value).is_some_and(|n| n > 0)
}

/// A project's build limits, as its namespace's annotations give them. Each is absent when its
/// annotation is missing, blank or not a quantity (Builder ignores such a value).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProjectBuildLimits {
    /// The memory limit of one build, such as `2Gi`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_memory: Option<String>,
    /// The CPU limit of one build, such as `2` or `500m`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_cpu: Option<String>,
}

impl ProjectBuildLimits {
    /// The limits a namespace's annotations hold.
    #[must_use]
    pub fn from_annotations(annotations: Option<&BTreeMap<String, String>>) -> Self {
        let value = |key: &str| {
            annotations
                .and_then(|a| a.get(key))
                .map(|v| v.trim())
                .filter(|v| is_build_quantity(v))
                .map(str::to_owned)
        };
        Self {
            build_memory: value(ANNOTATION_BUILD_MEMORY),
            build_cpu: value(ANNOTATION_BUILD_CPU),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_valid_annotations_are_limits() {
        let annotations = BTreeMap::from([
            (ANNOTATION_BUILD_MEMORY.to_owned(), " 2Gi ".to_owned()),
            (ANNOTATION_BUILD_CPU.to_owned(), "lots".to_owned()),
        ]);
        assert_eq!(
            ProjectBuildLimits::from_annotations(Some(&annotations)),
            ProjectBuildLimits {
                build_memory: Some("2Gi".to_owned()),
                build_cpu: None,
            }
        );
        assert_eq!(
            ProjectBuildLimits::from_annotations(None),
            ProjectBuildLimits::default()
        );
    }

    #[test]
    fn quantities_are_kubernetes_quantities_above_zero() {
        for ok in ["2Gi", "512Mi", "1.5", "500m", "1e3", "2G"] {
            assert!(is_build_quantity(ok), "{ok}");
        }
        for bad in ["", "0", "-1", "2GB", "Gi", "two", "1 Gi", "0m"] {
            assert!(!is_build_quantity(bad), "{bad}");
        }
    }
}
