//! What a plan token binds. `zeabur-api` seals these claims into the token a person confirms
//! and opens them again on apply.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::change::PlanRequest;

/// How long a plan may be applied after it was made, in seconds.
pub const LIFETIME_SECONDS: i64 = 600;

/// What a token binds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
    /// The person's user name.
    pub person: String,
    /// The person's groups.
    pub groups: Vec<String>,
    /// The project (namespace).
    pub project: String,
    /// The request as made.
    pub request: PlanRequest,
    /// `resourceVersion` of each object the plan writes, by `kind/name`.
    pub versions: BTreeMap<String, String>,
    /// When the plan was made, RFC 3339.
    pub made_at: String,
    /// When it expires, in seconds since the Unix epoch.
    pub expires_at: i64,
}
