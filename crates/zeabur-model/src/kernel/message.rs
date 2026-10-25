//! Sentences a client words in its own language.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A sentence a client words in its own language: a stable key and the values it names.
///
/// Zeabur never sends prose for diagnoses, gaps and the plain-language parts of a problem, so
/// every interface translates them the same way and a key's meaning is part of the contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// The sentence, such as `diagnosis.crashLoop.cause.exitCode`.
    pub key: String,
    /// The values it names, such as the container and the exit code.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

impl Message {
    /// A sentence without values.
    #[must_use]
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            params: BTreeMap::new(),
        }
    }

    /// Adds the value `name`.
    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<String>) -> Self {
        self.params.insert(name.to_owned(), value.into());
        self
    }
}
