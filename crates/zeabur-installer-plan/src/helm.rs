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

/// How often helm-controller's install Job runs again after a failure, instead of its default
/// of 1000, so a failure that repeats ends the Job in minutes.
pub const BACK_OFF_LIMIT: u32 = 5;

/// Helm holds a release in one of these while an operation runs, and refuses another until it
/// ends; one left by a killed Job stays.
#[must_use]
pub fn is_pending(status: &str) -> bool {
    status.starts_with("pending-")
}

/// The revision a rollback returns to: the newest deployed one, else the newest superseded one.
#[must_use]
pub fn last_good(history: &[Revision]) -> Option<u64> {
    let mut deployed = None;
    let mut superseded = None;
    for revision in history {
        let best = match revision.status.as_str() {
            "deployed" => &mut deployed,
            "superseded" => &mut superseded,
            _ => continue,
        };
        if best.is_none_or(|v| revision.version > v) {
            *best = Some(revision.version);
        }
    }
    deployed.or(superseded)
}

/// Whether Helm's error comes from the objects themselves, such as a field the API server
/// refuses to change or a value its schema rejects, so trying again fails the same way.
#[must_use]
pub fn is_deterministic(error: &str) -> bool {
    const SIGNS: [&str; 4] = [
        "is invalid",
        "field is immutable",
        "error validating",
        "unable to build kubernetes objects",
    ];
    let mut found = false;
    for sign in SIGNS {
        if error.contains(sign) {
            found = true;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(revisions: &[(u64, &str)]) -> Vec<Revision> {
        let mut history = Vec::new();
        for (version, status) in revisions {
            history.push(Revision {
                version: *version,
                status: (*status).to_owned(),
            });
        }
        history
    }

    #[test]
    fn a_rollback_returns_to_the_deployed_revision_under_a_failed_upgrade() {
        let failed = history(&[(1, "superseded"), (2, "deployed"), (3, "failed")]);
        assert_eq!(last_good(&failed), Some(2));
        let rolled_back = history(&[(1, "superseded"), (2, "superseded"), (3, "failed")]);
        assert_eq!(last_good(&rolled_back), Some(2));
        assert_eq!(last_good(&history(&[(1, "failed")])), None);
    }

    #[test]
    fn a_refused_field_is_deterministic_and_a_timeout_is_not() {
        let immutable = "Upgrade \"zot\" failed: cannot patch \"zot\" with kind StatefulSet: \
                         StatefulSet.apps \"zot\" is invalid: spec: Forbidden: updates to \
                         statefulset spec for fields other than 'replicas' are forbidden";
        assert!(is_deterministic(immutable));
        assert!(is_deterministic(
            "Upgrade \"x\" failed: error validating \"\": error validating data"
        ));
        assert!(!is_deterministic(
            "Upgrade \"zot\" failed: context deadline exceeded"
        ));
        assert!(!is_deterministic(
            "Internal error occurred: failed calling webhook \"webhook.cert-manager.io\""
        ));
    }
}
