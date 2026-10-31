//! Helm's `IsCompatibleRange`: a chart version against a dependency's constraint, read as
//! Masterminds semver's `NewConstraint` and `Check` read them [semver].
//!
//! [semver]: https://github.com/Masterminds/semver/blob/v3.4.0/constraints.go

use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

const S: &str = r"[\t\n\f\r ]";
const OPS: &str = r"=||!=|>|<|>=|=>|<=|=<|~|~>|\^";
const CV: &str = r"v?([0-9|x|X|\*]+)(\.[0-9|x|X|\*]+)?(\.[0-9|x|X|\*]+)?(-([0-9A-Za-z\-]+(\.[0-9A-Za-z\-]+)*))?(\+([0-9A-Za-z\-]+(\.[0-9A-Za-z\-]+)*))?";

fn regex(pattern: &str) -> Option<Regex> {
    Regex::new(
        &pattern
            .replace("{S}", S)
            .replace("{OPS}", OPS)
            .replace("{CV}", CV),
    )
    .ok()
}

static CONSTRAINT: LazyLock<Option<Regex>> =
    LazyLock::new(|| regex(r"^{S}*({OPS}){S}*({CV}){S}*$"));
static RANGE: LazyLock<Option<Regex>> = LazyLock::new(|| regex(r"{S}*({CV}){S}+-{S}+({CV}){S}*"));
static FIND: LazyLock<Option<Regex>> = LazyLock::new(|| regex(r"({OPS}){S}*({CV})"));
static VALID: LazyLock<Option<Regex>> =
    LazyLock::new(|| regex(r"^({S}*({OPS}){S}*({CV}){S}*)((?:{S}+|,{S}*)({OPS}){S}*({CV}){S}*)*$"));
static LOOSE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"^v?([0-9]+)(\.[0-9]+)?(\.[0-9]+)?(-([0-9A-Za-z\-]+(\.[0-9A-Za-z\-]+)*))?(\+([0-9A-Za-z\-]+(\.[0-9A-Za-z\-]+)*))?$")
        .ok()
});

/// Whether `version` meets `constraint`; false when either does not parse, so a dependency
/// without a version matches no chart.
pub(super) fn compatible(constraint: &str, version: &str) -> bool {
    let (Some(version), Some(groups)) = (Version::parse(version), constraints(constraint)) else {
        return false;
    };
    groups
        .iter()
        .any(|(group, pre)| group.iter().all(|c| c.check(&version, *pre)))
}

struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    pre: String,
}

impl Version {
    /// `NewVersion`, which coerces `1` and `1.2` to `1.0.0` and `1.2.0`.
    fn parse(v: &str) -> Option<Self> {
        let m = LOOSE.as_ref()?.captures(v)?;
        let segment = |i| {
            m.get(i)
                .map_or(Some(0), |g| g.as_str().trim_start_matches('.').parse().ok())
        };
        let pre = m.get(5).map_or("", |g| g.as_str());
        if !pre.is_empty() && !valid_prerelease(pre) {
            return None;
        }
        Some(Self {
            major: segment(1)?,
            minor: segment(2)?,
            patch: segment(3)?,
            pre: pre.to_owned(),
        })
    }

    fn compare(&self, o: &Self) -> Ordering {
        let ordering = (self.major, self.minor, self.patch).cmp(&(o.major, o.minor, o.patch));
        match (ordering, self.pre.is_empty(), o.pre.is_empty()) {
            (Ordering::Equal, true, true) => Ordering::Equal,
            (Ordering::Equal, true, false) => Ordering::Greater,
            (Ordering::Equal, false, true) => Ordering::Less,
            (Ordering::Equal, false, false) => compare_prerelease(&self.pre, &o.pre),
            (ordering, ..) => ordering,
        }
    }

    fn lt(&self, o: &Self) -> bool {
        self.compare(o) == Ordering::Less
    }
}

fn valid_prerelease(pre: &str) -> bool {
    pre.split('.').all(|part| {
        let leading_zero = part.len() > 1 && part.starts_with('0');
        !part.is_empty() && (!leading_zero || part.bytes().any(|b| !b.is_ascii_digit()))
    })
}

fn compare_prerelease(v: &str, o: &str) -> Ordering {
    let (v, o): (Vec<_>, Vec<_>) = (v.split('.').collect(), o.split('.').collect());
    (0..v.len().max(o.len()))
        .map(|i| compare_pre_part(v.get(i).unwrap_or(&""), o.get(i).unwrap_or(&"")))
        .find(|d| d.is_ne())
        .unwrap_or(Ordering::Equal)
}

fn compare_pre_part(s: &str, o: &str) -> Ordering {
    if s == o {
        return Ordering::Equal;
    }
    if s.is_empty() {
        return Ordering::Less;
    }
    if o.is_empty() {
        return Ordering::Greater;
    }
    match (s.parse::<u64>(), o.parse::<u64>()) {
        (Err(_), Err(_)) => s.cmp(o),
        (Err(_), Ok(_)) => Ordering::Greater,
        (Ok(si), Ok(oi)) if si > oi => Ordering::Greater,
        (Ok(_), _) => Ordering::Less,
    }
}

struct Constraint {
    op: String,
    con: Version,
    dirty: bool,
    minor_dirty: bool,
    patch_dirty: bool,
}

/// `NewConstraint`: groups joined by `||`, each with whether it names a prerelease.
fn constraints(c: &str) -> Option<Vec<(Vec<Constraint>, bool)>> {
    let mut rewritten = c.to_owned();
    for m in RANGE.as_ref()?.captures_iter(c) {
        rewritten = rewritten.replacen(&m[0], &format!(">= {}, <= {} ", &m[1], &m[11]), 1);
    }
    let (valid, find) = (VALID.as_ref()?, FIND.as_ref()?);
    rewritten
        .split("||")
        .map(|v| {
            if !valid.is_match(v) {
                return None;
            }
            let group = find
                .find_iter(v)
                .map(|m| Constraint::parse(m.as_str()))
                .collect::<Option<Vec<_>>>()?;
            let pre = group.iter().any(|c| !c.con.pre.is_empty());
            Some((group, pre))
        })
        .collect()
}

fn is_x(s: &str) -> bool {
    matches!(s, "x" | "X" | "*")
}

impl Constraint {
    fn parse(c: &str) -> Option<Self> {
        let m = CONSTRAINT.as_ref()?.captures(c)?;
        let g = |i| m.get(i).map_or("", |g| g.as_str());
        let segment = |i| g(i).strip_prefix('.').unwrap_or_else(|| g(i));
        let (ver, dirty, minor_dirty, patch_dirty) = if is_x(g(3)) || g(3).is_empty() {
            (format!("0.0.0{}", g(6)), true, false, false)
        } else if is_x(segment(4)) || g(4).is_empty() {
            (format!("{}.0.0{}", g(3), g(6)), true, true, false)
        } else if is_x(segment(5)) || g(5).is_empty() {
            (format!("{}{}.0{}", g(3), g(4), g(6)), true, false, true)
        } else {
            (g(2).to_owned(), false, false, false)
        };
        Some(Self {
            op: g(1).to_owned(),
            con: Version::parse(&ver)?,
            dirty,
            minor_dirty,
            patch_dirty,
        })
    }

    fn check(&self, v: &Version, include_pre: bool) -> bool {
        if !v.pre.is_empty() && !include_pre {
            return false;
        }
        match self.op.as_str() {
            "" | "=" if self.dirty => self.tilde(v),
            "" | "=" => v.compare(&self.con).is_eq(),
            "!=" => self.not_equal(v),
            ">" => self.greater_than(v),
            "<" => v.lt(&self.con),
            ">=" | "=>" => !v.lt(&self.con),
            "<=" | "=<" => self.less_than_equal(v),
            "~" | "~>" => self.tilde(v),
            "^" => self.caret(v),
            _ => false,
        }
    }

    fn not_equal(&self, v: &Version) -> bool {
        let c = &self.con;
        if self.dirty {
            if c.major != v.major {
                return true;
            }
            if c.minor != v.minor && !self.minor_dirty {
                return true;
            } else if self.minor_dirty {
                return false;
            } else if c.patch != v.patch && !self.patch_dirty {
                return true;
            } else if self.patch_dirty {
                return (!v.pre.is_empty() || !c.pre.is_empty())
                    && compare_prerelease(&v.pre, &c.pre).is_ne();
            }
        }
        v.compare(c).is_ne()
    }

    fn greater_than(&self, v: &Version) -> bool {
        let c = &self.con;
        if !self.dirty {
            return v.compare(c).is_gt();
        }
        if v.major != c.major {
            return v.major > c.major;
        }
        if self.minor_dirty {
            return false;
        }
        if self.patch_dirty {
            return v.minor > c.minor;
        }
        v.compare(c).is_gt()
    }

    fn less_than_equal(&self, v: &Version) -> bool {
        let c = &self.con;
        if !self.dirty {
            return v.compare(c).is_le();
        }
        !(v.major > c.major || v.major == c.major && v.minor > c.minor && !self.minor_dirty)
    }

    fn tilde(&self, v: &Version) -> bool {
        let c = &self.con;
        if v.lt(c) {
            return false;
        }
        if c.major == 0 && c.minor == 0 && c.patch == 0 && !self.minor_dirty && !self.patch_dirty {
            return true;
        }
        v.major == c.major && (v.minor == c.minor || self.minor_dirty)
    }

    fn caret(&self, v: &Version) -> bool {
        let c = &self.con;
        if v.lt(c) {
            return false;
        }
        if c.major > 0 || self.minor_dirty {
            return v.major == c.major;
        }
        if v.major > 0 {
            return false;
        }
        if c.minor > 0 || self.patch_dirty {
            return v.minor == c.minor;
        }
        if v.minor > 0 {
            return false;
        }
        c.patch == v.patch
    }
}

#[cfg(test)]
mod tests {
    use super::compatible;

    #[test]
    fn constraints_read_as_masterminds_semver_reads_them() {
        for (constraint, version, expected) in [
            ("1.x", "1.4.2", true),
            ("1.x", "2.0.0", false),
            ("^1.2", "1.9.0", true),
            ("^0.2.3", "0.3.0", false),
            ("~1.2.3", "1.2.9", true),
            ("~1.2.3", "1.3.0", false),
            (">= 1.0, < 2.0", "1.5.0", true),
            (">=1.0 <2.0", "2.0.0", false),
            ("1.0 - 2.0", "2.0.0", true),
            ("<1.0 || >=3", "3.1.0", true),
            ("<1.0 || >=3", "2.0.0", false),
            ("*", "1.0.0-rc.1", false),
            (">=1.0.0-0", "1.0.0-rc.1", true),
            ("1.0.0", "v1", true),
            ("!=1.2.x", "1.2.5", false),
            ("", "1.0.0", false),
            ("1.0.0", "", false),
            ("abc", "1.0.0", false),
        ] {
            assert_eq!(
                compatible(constraint, version),
                expected,
                "{constraint:?} against {version:?}"
            );
        }
    }
}
