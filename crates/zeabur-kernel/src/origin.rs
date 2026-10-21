//! Where a value comes from: the Kubernetes object and the field inside it.

use serde::{Deserialize, Serialize};

/// Where a value is *declared* or read from: the Kubernetes object and the field inside it.
///
/// Every value the API returns carries one, so a client can always show which object to open,
/// and a change plan knows which field it writes. It is the one provenance type of the API: the
/// projection's values, events, logs, ConfigMaps, Secrets and projects all use it.
///
/// Inside a project document `apiVersion` and `namespace` are left out: every object is in the
/// project's namespace, and the kind names one API group. Responses that stand alone (an event,
/// a log, a ConfigMap) carry both, so the reference is complete without its context; that is
/// Kubernetes' `ObjectReference` shape [k8s-api-conventions].
///
/// `field_path` names JSON fields with dots. A list whose entries Kubernetes merges by a key is
/// addressed by that key, `containers[name=app]`; an atomic list by index, `envFrom[0]`.
///
/// [k8s-api-conventions]: https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md#object-references
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    /// The object's `apiVersion`, when the response stands alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    /// The object's kind, such as `Deployment`.
    pub kind: String,
    /// The object's namespace, when the response stands alone and the object is namespaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// The object's name.
    pub name: String,
    /// The field inside the object, absent when the value is the object as a whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_path: Option<String>,
    /// The value read at that field, as evidence names it (`4` at a quota's
    /// `status.hard.requests.cpu`); absent where the origin only says where a value comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl Origin {
    /// The object `kind`/`name` of the projected namespace, as a whole.
    #[must_use]
    pub fn object(kind: &str, name: &str) -> Self {
        Self {
            api_version: None,
            kind: kind.to_owned(),
            namespace: None,
            name: name.to_owned(),
            field_path: None,
            value: None,
        }
    }

    /// The field `path` of the object `kind`/`name` of the projected namespace.
    #[must_use]
    pub fn field(kind: &str, name: &str, path: String) -> Self {
        Self {
            field_path: Some(path),
            ..Self::object(kind, name)
        }
    }

    /// This origin narrowed to the field `path`.
    #[must_use]
    pub fn with_field(self, path: String) -> Self {
        Self {
            field_path: Some(path),
            ..self
        }
    }

    /// This origin with the value read at it.
    #[must_use]
    pub fn with_value(self, value: impl Into<String>) -> Self {
        Self {
            value: Some(value.into()),
            ..self
        }
    }

    /// A complete reference to an object: `apiVersion`, `kind`, `namespace` (`None` for a
    /// cluster-scoped object) and `name`.
    #[must_use]
    pub fn complete(api_version: &str, kind: &str, namespace: Option<&str>, name: &str) -> Self {
        Self {
            api_version: Some(api_version.to_owned()),
            namespace: namespace.map(str::to_owned),
            ..Self::object(kind, name)
        }
    }
}
