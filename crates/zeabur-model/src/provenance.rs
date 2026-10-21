//! Which tool manages an object.

use serde::{Deserialize, Serialize};
pub use zeabur_kernel::tool::Tool;

/// Which tool manages an object, if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    /// The managing tool, when its labels or annotations are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
    /// The `app.kubernetes.io/managed-by` label, verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_by: Option<String>,
    /// A direct edit is likely to be reverted, so a client warns before making one.
    pub warn_before_edit: bool,
}
