//! The deployment tools whose objects Zeabur does not edit.
//!
//! One type for every layer: the adapters detect a tool, the model shows it, and a refused
//! write names it in its problem.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A deployment tool that would revert a direct edit on its next sync or upgrade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum Tool {
    /// Helm.
    Helm,
    /// Argo CD.
    #[serde(rename = "Argo CD")]
    ArgoCd,
    /// Flux.
    Flux,
}
