//! What a service's overview says first: one tone and one headline.

use serde::{Deserialize, Serialize};

use crate::{Health, StatusCause, StatusWord};

/// A service's verdict, from its status, its diagnoses and its pods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceVerdict {
    /// The colour of the verdict.
    pub tone: Health,
    /// What it says.
    pub headline: Headline,
}

/// What a verdict says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "as", rename_all = "camelCase")]
pub enum Headline {
    /// It runs and nothing is wrong.
    Running,
    /// A rollout is under way.
    Deploying,
    /// It is scaled to zero.
    Stopped,
    /// Something is wrong.
    Problem {
        /// What is wrong.
        trouble: Trouble,
    },
    /// The word of a task or of an idle service: completed, scheduled, paused.
    Status {
        /// The status word.
        word: StatusWord,
    },
}

/// What is wrong, in a few words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "as", rename_all = "camelCase")]
pub enum Trouble {
    /// The word for the reason of a failing pod, or for a diagnosis.
    Reason {
        /// The word.
        word: StatusCause,
    },
    /// The status word, when no reason or diagnosis has a word.
    Status {
        /// The status word.
        word: StatusWord,
    },
    /// A health check keeps failing.
    HealthCheck,
    /// A `ResourceQuota` keeps its pods from being created.
    Quota,
}
