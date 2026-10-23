//! The Pod Security Standards' levels, as a namespace's labels name them and as a pod template
//! meets them [k8s-pss].
//!
//! [k8s-pss]: https://kubernetes.io/docs/concepts/security/pod-security-standards/

use serde::{Deserialize, Serialize};

/// A Pod Security Standards level, from the least to the most restrictive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum PodSecurityLevel {
    /// No restrictions.
    Privileged,
    /// Prevents known privilege escalations; a container may run as root.
    Baseline,
    /// Current hardening practice: non-root, no privilege escalation, every capability dropped
    /// but `NET_BIND_SERVICE`, a seccomp profile.
    Restricted,
}

impl PodSecurityLevel {
    /// The namespace label whose level Pod Security admission enforces: it refuses pods that
    /// break it [k8s-psa-labels].
    ///
    /// [k8s-psa-labels]: https://kubernetes.io/docs/concepts/security/pod-security-admission/#pod-security-admission-labels-for-namespaces
    pub const ENFORCE: &'static str = "pod-security.kubernetes.io/enforce";
    /// The namespace label whose level is recorded in the audit log.
    pub const AUDIT: &'static str = "pod-security.kubernetes.io/audit";
    /// The namespace label whose level is returned to the client as a warning.
    pub const WARN: &'static str = "pod-security.kubernetes.io/warn";

    /// The label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Privileged => "privileged",
            Self::Baseline => "baseline",
            Self::Restricted => "restricted",
        }
    }

    /// The level a label value names; `None` for any other value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "privileged" => Some(Self::Privileged),
            "baseline" => Some(Self::Baseline),
            "restricted" => Some(Self::Restricted),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PodSecurityLevel;

    #[test]
    fn levels_read_their_label_values_and_order_by_strictness() {
        for level in [
            PodSecurityLevel::Privileged,
            PodSecurityLevel::Baseline,
            PodSecurityLevel::Restricted,
        ] {
            assert_eq!(PodSecurityLevel::parse(level.as_str()), Some(level));
        }
        assert_eq!(PodSecurityLevel::parse("Restricted"), None);
        assert!(PodSecurityLevel::Baseline < PodSecurityLevel::Restricted);
    }
}
