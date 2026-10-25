#![doc = include_str!("../README.md")]

pub mod archive;
pub mod cmd;
pub mod download;
pub mod facts;
pub mod files;
pub mod kubectl;
pub mod network;
pub mod nodeprep;
pub mod preflight;
pub mod systemd;
pub mod token;

use std::path::PathBuf;

/// Why a host operation failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HostError {
    /// A file or directory.
    #[error("{path}: {source}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A command could not start or exited with a failure.
    #[error("{command}: {reason}")]
    Command {
        /// The command line.
        command: String,
        /// Its exit status and the end of its standard error.
        reason: String,
    },
    /// A download failed.
    #[error("downloading {url}: {reason}")]
    Download {
        /// The URL.
        url: String,
        /// Why.
        reason: String,
    },
    /// A download did not match its digest.
    #[error("{file}: SHA-256 is {actual}, expected {expected}; the download was removed")]
    Digest {
        /// The file name.
        file: String,
        /// Expected digest.
        expected: String,
        /// Actual digest.
        actual: String,
    },
    /// Output that did not parse.
    #[error("{what}: {reason}")]
    Parse {
        /// What was read.
        what: String,
        /// Why it did not parse.
        reason: String,
    },
}

impl HostError {
    pub(crate) fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Self {
        let path = path.into();
        move |source| Self::Io { path, source }
    }
}
