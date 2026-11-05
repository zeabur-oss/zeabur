//! Helm releases as Helm's Secret driver keeps them: one Secret per revision.

/// One revision of a Helm release, as the labels of its Secret tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    /// The revision number.
    pub version: u64,
    /// Helm's status, such as `deployed` or `failed`.
    pub status: String,
}

/// The newest revision of a release's history.
#[must_use]
pub fn newest(history: &[Revision]) -> Option<&Revision> {
    history.iter().max_by_key(|r| r.version)
}

/// The name of the Secret that holds a revision.
#[must_use]
pub fn revision_secret(release: &str, version: u64) -> String {
    format!("sh.helm.release.v1.{release}.v{version}")
}
