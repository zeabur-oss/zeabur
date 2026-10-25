//! Where a projected value comes from.

use serde::{Deserialize, Serialize};

use crate::is_false;
pub use crate::kernel::origin::Origin;

/// What an environment variable's value is read from.
///
/// This is the *value's* source, which is what a client needs to show or change it: a literal is
/// edited in place, a Secret or ConfigMap key is edited in that object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum SourceKind {
    /// `value`, written in the workload itself.
    Literal,
    /// `valueFrom.secretKeyRef` or an `envFrom` `secretRef`. The value is never projected.
    Secret,
    /// `valueFrom.configMapKeyRef` or an `envFrom` `configMapRef`.
    ConfigMap,
    /// `valueFrom.fieldRef`: a field of the pod (the downward API).
    PodField,
    /// `valueFrom.resourceFieldRef`: a container's resource request or limit.
    ResourceField,
    /// `valueFrom.fileKeyRef`: a key of an env file in one of the pod's volumes.
    File,
}

/// The source of a value: its kind and, for references, the object and key it reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", schema(as = ValueSource))]
pub struct Source {
    /// What the value is read from.
    pub kind: SourceKind,
    /// The referenced Secret or ConfigMap; the container for a `ResourceField`; the volume for
    /// a `File`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The key within that object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The pod field (`PodField`), resource (`ResourceField`) or file path (`File`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_path: Option<String>,
    /// The reference is marked optional: the container starts without it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub optional: bool,
}

impl Source {
    /// A source of `kind` with no reference.
    #[must_use]
    pub const fn new(kind: SourceKind) -> Self {
        Self {
            kind,
            name: None,
            key: None,
            field_path: None,
            optional: false,
        }
    }
}

/// A Kubernetes object in the projected namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    /// The object's kind, such as `ConfigMap`.
    pub kind: String,
    /// The object's name.
    pub name: String,
}

impl ObjectRef {
    /// The object `kind`/`name`.
    #[must_use]
    pub fn new(kind: &str, name: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            name: name.to_owned(),
        }
    }
}
