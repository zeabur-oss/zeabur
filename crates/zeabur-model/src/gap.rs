//! What a project is missing, with the guidance each finding rests on.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::diagnosis::Message;
use crate::source::Origin;

/// How much a gap matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", schema(as = GapSeverity))]
pub enum Severity {
    /// Likely to cause an outage or data loss.
    Warning,
    /// Worth doing; nothing breaks without it.
    Advice,
}

/// A public source a rule rests on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    /// The document's title.
    pub title: String,
    /// Where to read it.
    pub url: String,
}

/// How a gap is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum FixKind {
    /// A change Zeabur plans and applies (`id` names the plan template, or the form that asks for
    /// what the plan needs).
    Plan,
    /// A component to install (`id` names the catalogue component).
    Install,
}

/// The fix of a gap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    /// How it is closed.
    pub kind: FixKind,
    /// The plan template or component.
    pub id: String,
    /// What the template or component is filled in with.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

/// One finding of a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Gap {
    /// The rule's stable id, such as `resource-limits`.
    pub rule: String,
    /// How much it matters.
    pub severity: Severity,
    /// The object, or field of it, the finding is about.
    pub subject: Origin,
    /// The service it concerns, by id, when it concerns one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// What is missing, in one sentence.
    pub message: Message,
    /// How to close it.
    pub fix: Fix,
    /// The guidance the rule rests on.
    pub citations: Vec<Citation>,
}
