//! What a project is missing, with the guidance each finding rests on.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::attribution::WorkloadRef;
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
    /// The workload the fix changes, or whose page asks for what the fix needs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workload: Option<WorkloadRef>,
    /// The values the plan sets; absent when a person must choose them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<FixValues>,
}

impl Fix {
    /// A fix of `kind` and `id` without parameters, workload or values.
    #[must_use]
    pub fn new(kind: FixKind, id: &str) -> Self {
        Self {
            kind,
            id: id.to_owned(),
            params: BTreeMap::new(),
            workload: None,
            values: None,
        }
    }

    /// Adds the parameter `name`.
    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<String>) -> Self {
        self.params.insert(name.to_owned(), value.into());
        self
    }
}

/// The values a fix's plan sets in one container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "set", rename_all = "camelCase")]
pub enum FixValues {
    /// CPU and memory requests and limits: what the container sets, and starting values for
    /// what it does not.
    #[serde(rename_all = "camelCase")]
    Resources {
        /// The container.
        container: String,
        /// Requests, by resource (`cpu`, `memory`).
        requests: BTreeMap<String, String>,
        /// Limits, the same way.
        limits: BTreeMap<String, String>,
    },
    /// A TCP readiness check on the port the Service sends traffic to.
    #[serde(rename_all = "camelCase")]
    TcpReadiness {
        /// The container.
        container: String,
        /// The port.
        port: i32,
        /// Seconds between checks.
        period_seconds: i32,
    },
    /// A new disk mounted at a directory.
    #[serde(rename_all = "camelCase")]
    Volume {
        /// The container.
        container: String,
        /// Where the container sees the disk.
        mount_path: String,
        /// The disk's size, a Kubernetes quantity.
        size: String,
    },
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
