#![doc = include_str!("../README.md")]

pub mod catalog;
pub mod cluster;
pub mod config;
pub mod join;
pub mod k3s;
pub mod kubesolo;
pub mod personal;
pub mod pins;
pub mod plan;
pub mod remove;
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
        "a team install on {} needs a server with at least {min} MiB of memory; this one has {have} \
         MiB. Install for yourself instead: set `use: personal` (https://oss.zeabur.com/docs/zeabur/reference/installer-configuration/)",
        .distribution.title()
    )]
    TeamMemory {
        /// The distribution chosen.
        distribution: cluster::Distribution,
        /// What it needs.
        min: u64,
        /// What the host has.
        have: u64,
    },
    /// This build pins no release, and `sources` does not name these keys.
    #[error(
        "this installer is a development build and pins no release: set {} in the configuration \
         (https://oss.zeabur.com/docs/zeabur/reference/installer-configuration/)",
        .0.iter().map(|k| format!("sources.{k}")).collect::<Vec<_>>().join(", ")
    )]
    Unpublished(Vec<&'static str>),
    /// A bootstrap token was given to a node that joins as a server.
    #[error(
        "configuration: join token: a bootstrap token (<id>.<secret>) joins workers only; a server \
         needs the server token from `zeabur-installer node add --role server`"
    )]
    BootstrapTokenForServer,
    /// A second server was asked of K3s, which runs one.
    #[error(
        "configuration: join.role: server: K3s runs one server with SQLite and takes workers only. \
         For a control plane that survives losing a server, install RKE2 (`distribution: rke2`) \
         on three servers"
    )]
    SecondServer,
}
