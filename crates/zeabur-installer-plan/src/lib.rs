#![doc = include_str!("../README.md")]

pub mod catalog;
pub mod config;
pub mod join;
pub mod kubesolo;
pub mod personal;
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
    #[error("there is no release for the {0} architecture; amd64 and arm64 are supported")]
    Arch(String),
    /// Too little memory for a team install.
    #[error(
        "a team install needs a server with at least 4 GiB of memory (RKE2 and Zeabur); this one \
         has {0} MiB. Install for yourself instead: set `use: personal` (docs/reference/installer-configuration.md)"
    )]
    TeamMemory(u64),
    /// This build pins no release, and `sources` does not name these keys.
    #[error(
        "this installer is a development build and pins no release: set {} in the configuration \
         (docs/reference/installer-configuration.md)",
        .0.iter().map(|k| format!("sources.{k}")).collect::<Vec<_>>().join(", ")
    )]
    Unpublished(Vec<&'static str>),
    /// A bootstrap token was given to a node that joins as a server.
    #[error(
        "configuration: join token: a bootstrap token (<id>.<secret>) joins workers only; a server \
         needs the server token from `zeabur-installer node add --role server`"
    )]
    BootstrapTokenForServer,
}
