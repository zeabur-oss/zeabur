//! A component's `kubeVersion`: a range of Kubernetes versions in the syntax of a Helm chart's
//! `kubeVersion` (Masterminds/semver).
//!
//! This reads the comparisons the catalog writes (`>=1.34.0-0`, `>=1.30.0-0 <1.38.0-0`, `||`
//! between alternatives). A pre-release version counts as its release, which is what the `-0`
//! of the catalog's ranges admits. `~`, `^`, wildcards and hyphen ranges do not read.

/// Whether `range` admits the server version `version` (`v1.36.5+k3s1`); `None` when either does
/// not read.
#[must_use]
pub fn admits(range: &str, version: &str) -> Option<bool> {
    let version = release(version)?;
    let mut any = false;
    for alternative in range.split("||") {
        any |= all(alternative, version)?;
    }
    Some(any)
}

/// The release of a version, without its `v`, pre-release and build metadata.
#[must_use]
pub fn release(version: &str) -> Option<(u64, u64, u64)> {
    let version = version.trim();
    let version = version.strip_prefix('v').unwrap_or(version);
    let version = version.split(['+', '-']).next()?;
    let mut parts = version.split('.');
    let mut next = || parts.next().map_or(Some(0), |p| p.parse().ok());
    let release = (next()?, next()?, next()?);
    parts.next().is_none().then_some(release)
}

fn all(alternative: &str, version: (u64, u64, u64)) -> Option<bool> {
    let mut tokens = alternative.split([',', ' ']).filter(|t| !t.is_empty());
    let mut seen = false;
    let mut admitted = true;
    while let Some(token) = tokens.next() {
        let split = token
            .find(|c: char| c.is_ascii_digit() || c == 'v')
            .unwrap_or(token.len());
        let (op, mut bound) = token.split_at(split);
        if bound.is_empty() {
            bound = tokens.next()?;
        }
        let bound = release(bound)?;
        admitted &= match op {
            ">=" => version >= bound,
            ">" => version > bound,
            "<=" => version <= bound,
            "<" => version < bound,
            "" | "=" => version == bound,
            "!=" => version != bound,
            _ => return None,
        };
        seen = true;
    }
    seen.then_some(admitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_catalogs_ranges() {
        assert_eq!(admits(">=1.34.0-0", "v1.36.5+k3s1"), Some(true));
        assert_eq!(admits(">=1.34.0-0", "v1.36.0-rc.1"), Some(true));
        assert_eq!(admits(">=1.34.0-0", "v1.33.9"), Some(false));
        assert_eq!(
            admits(">= 1.30.0-0, <1.36.0-0", "v1.36.0-rc.1"),
            Some(false)
        );
        assert_eq!(admits(">=1.30 <1.36", "v1.35.7"), Some(true));
        assert_eq!(
            admits("<1.30.0-0 || >=1.36.0-0", "v1.36.5+rke2r1"),
            Some(true)
        );
    }

    #[test]
    fn a_range_it_cannot_read_admits_nothing() {
        assert_eq!(admits("^1.30.0", "v1.36.5"), None);
        assert_eq!(admits("1.x", "v1.36.5"), None);
        assert_eq!(admits("", "v1.36.5"), None);
        assert_eq!(admits(">=1.30.0", "main"), None);
    }
}
