//! The events and their properties.

use serde::{Serialize, Serializer};
use serde_json::{Map, Value, json};

/// The random identifier of one installation. It comes from nothing about the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationId(String);

impl InstallationId {
    /// A version 4 UUID made from 16 random bytes: version 4 in the high bits of octet 6 and
    /// variant `10` in octet 8, as RFC 9562 §5.4 (<https://www.rfc-editor.org/rfc/rfc9562#section-5.4>)
    /// lays out. The bytes should come from a CSPRNG (§6.9).
    #[must_use]
    pub fn from_random(mut bytes: [u8; 16]) -> Self {
        bytes[6] = (bytes[6] & 0x0F) | 0x40;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        let mut text = String::with_capacity(36);
        for (i, byte) in bytes.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                text.push('-');
            }
            text.extend(char::from_digit(u32::from(byte >> 4), 16));
            text.extend(char::from_digit(u32::from(byte & 0x0F), 16));
        }
        Self(text)
    }

    /// Reads a UUID in the hex-and-dash form of RFC 9562 §4, in any case.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let groups: Vec<&str> = text.split('-').collect();
        let sizes = [8, 4, 4, 4, 12];
        let canonical = groups.len() == sizes.len()
            && groups.iter().zip(sizes).all(|(group, size)| {
                group.len() == size && group.bytes().all(|b| b.is_ascii_hexdigit())
            });
        canonical.then(|| Self(text.to_ascii_lowercase()))
    }

    /// The identifier as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A day in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Day(i64);

impl Day {
    /// The UTC day that contains `seconds` since the Unix epoch.
    #[must_use]
    pub const fn from_unix(seconds: i64) -> Self {
        Self(seconds.div_euclid(86_400))
    }
}

impl Serialize for Day {
    /// `YYYY-MM-DD`, by Howard Hinnant's `civil_from_days`.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let z = self.0 + 719_468;
        let era = z.div_euclid(146_097);
        let day_of_era = z.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        serializer.serialize_str(&format!("{year:04}-{month:02}-{day:02}"))
    }
}

/// How Zeabur runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// In the cluster it manages.
    InCluster,
}

/// What the installer was asked to install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    /// One server.
    SingleNode,
    /// Several nodes.
    MultiNode,
    /// One person's install.
    Personal,
}

/// The Kubernetes distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Distribution {
    /// K3s.
    K3s,
    /// RKE2.
    Rke2,
    /// `KubeSolo`.
    KubeSolo,
}

/// Where in an install it stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Step {
    /// Reading the configuration and planning.
    Plan,
    /// Checks of the host.
    Preflight,
    /// Installing what the node needs before components run.
    NodePrep,
    /// The Kubernetes distribution itself.
    Cluster,
    /// Namespaces, network policies and storage.
    ClusterSetup,
    /// Zeabur Catalog components.
    Components,
    /// Zeabur Identity.
    Identity,
    /// Zeabur.
    Zeabur,
    /// Recording what was installed.
    Record,
}

/// Why an install stopped, from a fixed list; never a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// The configuration is invalid.
    Config,
    /// The plan cannot be made.
    Plan,
    /// A file or directory.
    Io,
    /// A command failed.
    Command,
    /// A download failed.
    Download,
    /// A download did not match its digest.
    Digest,
    /// Output did not parse.
    Parse,
    /// Anything else.
    Other,
}

/// Someone used Zeabur for the first time in a UTC day. It names nobody.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ZeaburActive {
    /// The version of Zeabur.
    pub zeabur_version: &'static str,
    /// The UTC day.
    pub day: Day,
    /// How Zeabur runs.
    pub mode: Mode,
}

/// What an install is, on every installer event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Install {
    /// The version of the installer.
    pub installer_version: &'static str,
    /// What was asked for.
    pub profile: Profile,
    /// The distribution.
    pub distribution: Distribution,
}

/// An install that stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Failure {
    /// The install.
    #[serde(flatten)]
    pub install: Install,
    /// Where it stopped.
    pub step: Step,
    /// Why.
    pub error: ErrorCode,
}

/// Everything that can be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Someone used Zeabur for the first time in a day.
    ZeaburActive(ZeaburActive),
    /// An install began.
    InstallStarted(Install),
    /// An install ended well.
    InstallFinished(Install),
    /// An install stopped.
    InstallFailed(Failure),
}

impl Event {
    /// The event's name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ZeaburActive(_) => "zeabur_active",
            Self::InstallStarted(_) => "install_started",
            Self::InstallFinished(_) => "install_finished",
            Self::InstallFailed(_) => "install_failed",
        }
    }

    /// The event's own properties.
    fn properties(&self) -> Value {
        let value = match self {
            Self::ZeaburActive(active) => serde_json::to_value(active),
            Self::InstallStarted(install) | Self::InstallFinished(install) => {
                serde_json::to_value(install)
            }
            Self::InstallFailed(failure) => serde_json::to_value(failure),
        };
        value.unwrap_or_else(|_| json!({}))
    }

    /// The body of the request to `PostHog`'s batch endpoint. The identifier is the only
    /// identity, no person profile is made, and `PostHog` is told not to look up a location.
    #[must_use]
    pub fn body(&self, key: &str, id: &InstallationId) -> String {
        let mut properties = match self.properties() {
            Value::Object(map) => map,
            Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::Array(_) => Map::new(),
        };
        properties.insert("$process_person_profile".to_owned(), json!(false));
        properties.insert("$geoip_disable".to_owned(), json!(true));
        json!({
            "api_key": key,
            "batch": [{
                "event": self.name(),
                "distinct_id": id.as_str(),
                "properties": properties,
            }],
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serialized(day: Day) -> String {
        serde_json::to_value(day)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn a_day_reads_as_its_utc_date() {
        assert_eq!(serialized(Day::from_unix(0)), "1970-01-01");
        assert_eq!(serialized(Day::from_unix(1_782_864_000)), "2026-07-01");
        assert_eq!(serialized(Day::from_unix(1_782_863_999)), "2026-06-30");
        assert_eq!(serialized(Day::from_unix(951_782_400)), "2000-02-29");
        assert_eq!(serialized(Day::from_unix(-1)), "1969-12-31");
    }

    #[test]
    fn a_random_identifier_is_a_version_4_uuid_and_reads_back() {
        let id = InstallationId::from_random([0xFF; 16]);
        assert_eq!(id.as_str(), "ffffffff-ffff-4fff-bfff-ffffffffffff");
        assert_eq!(InstallationId::parse(id.as_str()), Some(id));
        assert_eq!(
            InstallationId::parse(" 0A1B2C3D-0000-4000-8000-000000000000\n")
                .unwrap()
                .as_str(),
            "0a1b2c3d-0000-4000-8000-000000000000"
        );
        for bad in [
            "",
            "abc",
            "0a1b2c3d-0000-4000-8000-00000000000",
            "g-g-g-g-g",
        ] {
            assert_eq!(InstallationId::parse(bad), None, "{bad}");
        }
    }
}
