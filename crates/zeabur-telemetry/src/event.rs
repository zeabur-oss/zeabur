//! The events and their properties.

use serde::ser::SerializeMap as _;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value, json};

/// The random identifier of one installation. It comes from nothing about the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationId(String);

impl InstallationId {
    /// A version 4 UUID made from 16 random bytes (RFC 9562 §5.4), which should come from a CSPRNG.
    #[must_use]
    pub fn from_random(bytes: [u8; 16]) -> Self {
        Self(uuid_v4(bytes))
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

/// A version 4 UUID made from 16 random bytes: version 4 in the high bits of octet 6 and
/// variant `10` in octet 8, as RFC 9562 §5.4 (<https://www.rfc-editor.org/rfc/rfc9562#section-5.4>)
/// lays out. The bytes should come from a CSPRNG (§6.9).
fn uuid_v4(mut bytes: [u8; 16]) -> String {
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
    text
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

    /// Days since 1970-01-01.
    #[must_use]
    pub const fn since_epoch(self) -> i64 {
        self.0
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
    /// On the person's computer with their kubeconfig: `zeaburctl ui` or Zeabur Desktop.
    Local,
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
    /// Tells people apart within this installation and day, and nowhere else: 32 hexadecimal
    /// digits under a key that never leaves the installation.
    pub person: String,
    /// How Zeabur runs.
    pub mode: Mode,
}

/// How a service was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// From a Git repository.
    Git,
    /// From an uploaded folder.
    Upload,
    /// From a container image.
    Image,
    /// From a template of Zeabur Catalog, by its ID. A `&'static str`, so only an ID the
    /// embedded catalog carries can be named.
    Template(&'static str),
}

impl Source {
    const fn name(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Upload => "upload",
            Self::Image => "image",
            Self::Template(_) => "template",
        }
    }
}

/// The kind of a database or of object storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DatabaseKind {
    /// PostgreSQL.
    Postgresql,
    /// MongoDB.
    Mongodb,
    /// MySQL.
    Mysql,
    /// MariaDB.
    Mariadb,
    /// Dragonfly, which speaks the Redis protocol.
    Dragonfly,
    /// S3-compatible object storage.
    ObjectStorage,
}

/// Where a domain's name comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DomainKind {
    /// Under one of the suffixes Zeabur offers new services.
    Generated,
    /// Any other name.
    Custom,
}

/// A feature someone used, with the one enum value that tells uses of it apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    /// A service was created.
    ServiceCreated(Source),
    /// A database or object storage was created.
    DatabaseCreated(DatabaseKind),
    /// A domain was added.
    DomainAdded(DomainKind),
    /// A log stream was opened.
    LogsViewed,
    /// A terminal session was opened.
    TerminalOpened,
    /// Someone was invited.
    MemberInvited,
    /// Services were created from a Docker Compose file.
    Compose,
}

impl Feature {
    const fn name(self) -> &'static str {
        match self {
            Self::ServiceCreated(_) => "service_created",
            Self::DatabaseCreated(_) => "database_created",
            Self::DomainAdded(_) => "domain_added",
            Self::LogsViewed => "logs_viewed",
            Self::TerminalOpened => "terminal_opened",
            Self::MemberInvited => "member_invited",
            Self::Compose => "compose",
        }
    }
}

impl Serialize for Feature {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("feature", self.name())?;
        match self {
            Self::ServiceCreated(source) => {
                map.serialize_entry("source", source.name())?;
                if let Source::Template(id) = source {
                    map.serialize_entry("template", id)?;
                }
            }
            Self::DatabaseCreated(kind) => map.serialize_entry("kind", kind)?,
            Self::DomainAdded(kind) => map.serialize_entry("kind", kind)?,
            Self::LogsViewed | Self::TerminalOpened | Self::MemberInvited | Self::Compose => {}
        }
        map.end()
    }
}

/// A feature was used, for the first time that day with these values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureUsed {
    /// The version of Zeabur.
    pub zeabur_version: &'static str,
    /// The UTC day.
    pub day: Day,
    /// What was used.
    #[serde(flatten)]
    pub feature: Feature,
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

/// How connecting to a cluster with a kubeconfig ended, by the cluster's answer to the first
/// request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Connection {
    /// The cluster said who the credential signs in as.
    Connected,
    /// No answer: a network error or a timeout.
    Unreachable,
    /// 401: the credential was refused.
    CredentialRefused,
    /// 403: the credential cannot read what Zeabur needs.
    NoReadRight,
    /// Anything else.
    Other,
}

/// Someone connected to a cluster with a kubeconfig, or tried to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClusterConnected {
    /// The version of Zeabur.
    pub zeabur_version: &'static str,
    /// How Zeabur runs.
    pub mode: Mode,
    /// How it ended.
    pub result: Connection,
}

/// The random identifier of one change, shared by its two events. It means nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeId(String);

impl ChangeId {
    /// A version 4 UUID made from 16 random bytes (RFC 9562 §5.4), which should come from a CSPRNG.
    #[must_use]
    pub fn from_random(bytes: [u8; 16]) -> Self {
        Self(uuid_v4(bytes))
    }
}

/// What a change does to one Deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// A new image, or a new Deployment.
    DeployImage,
    /// A new replica count.
    ScaleReplicas,
    /// A change of environment variables.
    ChangeEnv,
}

/// One change someone made, on both of its events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    /// The version of Zeabur.
    pub zeabur_version: &'static str,
    /// How Zeabur runs.
    pub mode: Mode,
    /// The person, as `zeabur_active` hashes them for the UTC day the change was submitted. Both
    /// events of a change carry the same hash, also when the rollout ends on the next day, so a
    /// change never links two days of one person.
    pub person: String,
    /// Ties the change's two events together.
    #[serde(rename = "change_id")]
    pub id: ChangeId,
    /// What it does.
    pub operation: Operation,
}

/// The answer Zeabur gave the person for a change, by HTTP status class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Submitted {
    /// The cluster accepted the write.
    Accepted,
    /// 403: the person may not write it.
    Forbidden,
    /// 422: the cluster or Zeabur refused the values.
    Invalid,
    /// 409: something else changed it first.
    Conflict,
    /// 502, 503 or 504: the cluster did not answer.
    Unreachable,
    /// Any other status.
    Other,
}

/// A change was sent to the cluster and answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppChangeSubmitted {
    /// The change.
    #[serde(flatten)]
    pub change: Change,
    /// The answer.
    pub result: Submitted,
}

/// How the rollout of an accepted change ended, in the terms `kubectl rollout status` uses
/// (<https://kubernetes.io/docs/concepts/workloads/controllers/deployment/#deployment-status>).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rollout {
    /// Every replica runs the change and is available.
    Available,
    /// The `Progressing` condition turned `False` with the reason `ProgressDeadlineExceeded`.
    ProgressDeadlineExceeded,
}

/// The rollout of an accepted change ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RolloutFinished {
    /// The change.
    #[serde(flatten)]
    pub change: Change,
    /// How it ended.
    pub result: Rollout,
}

/// Everything that can be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Someone used Zeabur for the first time in a day.
    ZeaburActive(ZeaburActive),
    /// A feature was used.
    FeatureUsed(FeatureUsed),
    /// An install began.
    InstallStarted(Install),
    /// An install ended well.
    InstallFinished(Install),
    /// An install stopped.
    InstallFailed(Failure),
    /// Someone connected to a cluster with a kubeconfig, or tried to.
    ClusterConnected(ClusterConnected),
    /// A change was sent to the cluster and answered.
    AppChangeSubmitted(AppChangeSubmitted),
    /// The rollout of an accepted change ended.
    RolloutFinished(RolloutFinished),
}

impl Event {
    /// The event's name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ZeaburActive(_) => "zeabur_active",
            Self::FeatureUsed(_) => "feature_used",
            Self::InstallStarted(_) => "install_started",
            Self::InstallFinished(_) => "install_finished",
            Self::InstallFailed(_) => "install_failed",
            Self::ClusterConnected(_) => "cluster_connected",
            Self::AppChangeSubmitted(_) => "app_change_submitted",
            Self::RolloutFinished(_) => "rollout_finished",
        }
    }

    /// The event's own properties.
    fn properties(&self) -> Value {
        let value = match self {
            Self::ZeaburActive(active) => serde_json::to_value(active),
            Self::FeatureUsed(used) => serde_json::to_value(used),
            Self::InstallStarted(install) | Self::InstallFinished(install) => {
                serde_json::to_value(install)
            }
            Self::InstallFailed(failure) => serde_json::to_value(failure),
            Self::ClusterConnected(connected) => serde_json::to_value(connected),
            Self::AppChangeSubmitted(submitted) => serde_json::to_value(submitted),
            Self::RolloutFinished(finished) => serde_json::to_value(finished),
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
    fn a_change_is_sent_as_flat_properties() {
        let change = Change {
            zeabur_version: "1.0.0",
            mode: Mode::InCluster,
            person: "ab".to_owned(),
            id: ChangeId::from_random([1; 16]),
            operation: Operation::ScaleReplicas,
        };
        let event = Event::RolloutFinished(RolloutFinished {
            change,
            result: Rollout::ProgressDeadlineExceeded,
        });
        let properties = event.properties();
        assert_eq!(properties["operation"], "scale_replicas");
        assert_eq!(
            properties["change_id"],
            "01010101-0101-4101-8101-010101010101"
        );
        assert_eq!(properties["result"], "progress_deadline_exceeded");
        assert!(properties.get("change").is_none());
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
