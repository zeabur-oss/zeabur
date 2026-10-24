//! Planning for `zeabur-installer`: from a configuration, a Zeabur catalog release and a few host
//! facts to the exact files, objects and steps `apply` carries out. Nothing here touches the host
//! or the network, so every decision is unit-tested and `plan` is deterministic.

pub mod catalog;
pub mod config;
pub mod join;
pub mod pins;
pub mod plan;
pub mod rke2;
pub mod select;
pub mod storage;
pub mod values;

/// Why a plan cannot be made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanError {
    /// The configuration file.
    #[error("configuration: {0}")]
    Config(String),
    /// `index.json` did not parse.
    #[error("catalog index: {0}")]
    CatalogJson(#[source] serde_json::Error),
    /// An `indexVersion` this installer does not read.
    #[error("catalog indexVersion {0} is not supported; this installer reads version {supported}", supported = catalog::INDEX_VERSION)]
    IndexVersion(u64),
    /// A capability or component name matches nothing in the catalog.
    #[error("`{0}` is not a capability or component in this catalog")]
    Unknown(String),
    /// A chosen component needs one that is disabled or cannot be installed.
    #[error("{component} needs {dependency}, which is disabled or not supported here")]
    NeedsDisabled {
        /// The component.
        component: String,
        /// What it needs.
        dependency: String,
    },
    /// A required input has no value.
    #[error("{component} needs the input `{input}`; add it to the configuration:\n  {hint}")]
    MissingInput {
        /// The component.
        component: String,
        /// The input.
        input: String,
        /// The configuration line to add.
        hint: String,
    },
    /// Values YAML did not parse or a path did not fit.
    #[error("values of {what}: {reason}")]
    Values {
        /// Where.
        what: String,
        /// Why.
        reason: String,
    },
    /// The host's CPU architecture.
    #[error("RKE2 has no release for the {0} architecture; amd64 and arm64 are supported")]
    Arch(String),
    /// A product has no published release yet, so its source must be named.
    #[error("no published release is pinned yet: set sources.{0} in the configuration")]
    Unpublished(&'static str),
}
