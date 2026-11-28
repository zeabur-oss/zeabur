//! Whether Keel [keel] moves a workload to new images, read from its annotations, and which
//! policies its images' tags allow.
//!
//! [keel]: https://keel.sh/docs/#policies

use serde::{Deserialize, Serialize};

/// The workload annotation that has Keel update it, and to which tags.
pub const KEEL_POLICY: &str = "keel.sh/policy";
/// The annotation that has Keel look for new tags itself instead of being told of them.
pub const KEEL_TRIGGER: &str = "keel.sh/trigger";
/// When Keel looks, as a cron schedule with seconds or `@hourly`.
pub const KEEL_POLL_SCHEDULE: &str = "keel.sh/pollSchedule";
/// With the `force` policy, update only when the same tag points to a new image.
pub const KEEL_MATCH_TAG: &str = "keel.sh/match-tag";
/// The policy that takes any new tag; Zeabur sets it for built images.
pub const KEEL_FORCE: &str = "force";
/// The trigger value for polling.
pub const KEEL_POLL: &str = "poll";
/// The schedule when no window is set.
pub const KEEL_HOURLY: &str = "@hourly";

/// The annotations Zeabur writes and removes for automatic updates.
pub const KEEL_ANNOTATIONS: [&str; 4] = [
    KEEL_POLICY,
    KEEL_TRIGGER,
    KEEL_POLL_SCHEDULE,
    KEEL_MATCH_TAG,
];

/// Which new versions of an image Keel moves a workload to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum ImagePolicy {
    /// `1.2.3` to `1.2.4`, not `1.3.0`.
    Patch,
    /// `1.2.3` to `1.3.0`, not `2.0.0`.
    Minor,
    /// Any newer version.
    Major,
    /// A new image pushed under the same tag, such as `latest`.
    SameTag,
}

impl ImagePolicy {
    /// The `keel.sh/policy` value, and whether `keel.sh/match-tag` is set.
    #[must_use]
    pub const fn annotation(self) -> (&'static str, bool) {
        match self {
            Self::Patch => ("patch", false),
            Self::Minor => ("minor", false),
            Self::Major => ("major", false),
            Self::SameTag => (KEEL_FORCE, true),
        }
    }

    /// The policies `images`, the tags of a workload's containers, allow: a version policy only
    /// when Keel reads every tag as a version, `SameTag` unless an image is pinned by digest.
    #[must_use]
    pub fn allowed<'a>(images: impl IntoIterator<Item = &'a str>) -> Vec<Self> {
        let images: Vec<&str> = images.into_iter().collect();
        if images.is_empty() || images.iter().any(|i| i.contains('@')) {
            return Vec::new();
        }
        if images.iter().all(|i| image_tag(i).is_some_and(is_version)) {
            vec![Self::Patch, Self::Minor, Self::Major, Self::SameTag]
        } else {
            vec![Self::SameTag]
        }
    }
}

/// The tag of an image reference; `None` when it names none (Docker reads that as `latest`).
fn image_tag(image: &str) -> Option<&str> {
    let last = image.rsplit('/').next().unwrap_or(image);
    last.split_once(':').map(|(_, tag)| tag)
}

/// Whether Keel's semver policies read `tag` as a version: `latest`, which moves to the newest
/// version tag, or one to three numbers with an optional leading `v`, where `8` reads as `8.0.0`.
/// See `shouldUpdate` in Keel's `internal/policy/semver.go`.
fn is_version(tag: &str) -> bool {
    if tag == "latest" {
        return true;
    }
    let tag = tag.strip_prefix('v').unwrap_or(tag);
    let parts: Vec<&str> = tag.split('.').collect();
    (1..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// A day of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum Weekday {
    /// Monday.
    Monday,
    /// Tuesday.
    Tuesday,
    /// Wednesday.
    Wednesday,
    /// Thursday.
    Thursday,
    /// Friday.
    Friday,
    /// Saturday.
    Saturday,
    /// Sunday.
    Sunday,
}

impl Weekday {
    const ALL: [Self; 7] = [
        Self::Sunday,
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
    ];

    /// The cron day of the week, Sunday 0.
    fn cron(self) -> usize {
        Self::ALL
            .iter()
            .position(|d| *d == self)
            .unwrap_or_default()
    }
}

/// When Keel looks for a new image: once a day or once a week, at the start of an hour, in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct UpdateWindow {
    /// The day; every day when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day: Option<Weekday>,
    /// The hour, 0 to 23, UTC.
    pub hour: u8,
}

impl UpdateWindow {
    /// The `keel.sh/pollSchedule` for `window`: `@hourly` without one, else a cron schedule with
    /// seconds first, as Keel reads it.
    #[must_use]
    pub fn schedule(window: Option<Self>) -> String {
        window.map_or_else(
            || KEEL_HOURLY.to_owned(),
            |w| {
                let day = w
                    .day
                    .map_or_else(|| "*".to_owned(), |d| d.cron().to_string());
                format!("0 0 {} * * {day}", w.hour)
            },
        )
    }

    /// The window [`Self::schedule`] wrote; `None` for `@hourly` and any other schedule.
    #[must_use]
    pub fn from_schedule(schedule: &str) -> Option<Self> {
        let fields: Vec<&str> = schedule.split_whitespace().collect();
        let [seconds, minutes, hour, "*", "*", day] = fields.as_slice() else {
            return None;
        };
        if *seconds != "0" || *minutes != "0" {
            return None;
        }
        let hour = hour.parse::<u8>().ok().filter(|h| *h < 24)?;
        let day = match *day {
            "*" => None,
            day => Some(*Weekday::ALL.get(day.parse::<usize>().ok()?)?),
        };
        Some(Self { day, hour })
    }
}

/// What Keel does with a workload now, by its annotations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateSetting {
    /// Keel looks for new image tags itself, as Zeabur's setting writes it.
    #[serde(rename_all = "camelCase")]
    Image {
        /// Which versions.
        policy: ImagePolicy,
        /// When; hourly when left out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window: Option<UpdateWindow>,
    },
    /// Keel moves it to each image it is told of (`force`, without polling): Zeabur's setting
    /// for a service built from source.
    NewBuilds,
    /// Annotations Zeabur does not write, shown as they are.
    #[serde(rename_all = "camelCase")]
    Other {
        /// `keel.sh/policy`.
        policy: String,
        /// `keel.sh/pollSchedule`, when set.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schedule: Option<String>,
    },
}

/// A long-running workload's automatic image updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ImageUpdates {
    /// What Keel does now; absent when it leaves the workload alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setting: Option<UpdateSetting>,
    /// The policies its images' tags allow (see [`ImagePolicy::allowed`]).
    pub allowed: Vec<ImagePolicy>,
}

impl ImageUpdates {
    /// What `annotations`, a workload's, ask Keel for, and what `images`, its containers', allow.
    #[must_use]
    pub fn read<'a>(
        annotations: impl IntoIterator<Item = (&'a String, &'a String)>,
        images: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let mut found: [Option<&str>; 4] = [None; 4];
        for (key, value) in annotations {
            if let Some(at) = KEEL_ANNOTATIONS.iter().position(|k| k == key)
                && let Some(slot) = found.get_mut(at)
            {
                *slot = Some(value.as_str());
            }
        }
        let [policy, trigger, schedule, match_tag] = found;
        Self {
            setting: policy.and_then(|p| setting(p, trigger, schedule, match_tag)),
            allowed: ImagePolicy::allowed(images),
        }
    }
}

pub(crate) fn setting(
    policy: &str,
    trigger: Option<&str>,
    schedule: Option<&str>,
    match_tag: Option<&str>,
) -> Option<UpdateSetting> {
    let policy = policy.trim();
    if policy.is_empty() || policy == "never" {
        return None;
    }
    let same_tag = match_tag.is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));
    let other = || UpdateSetting::Other {
        policy: policy.to_owned(),
        schedule: schedule.map(str::to_owned),
    };
    if trigger != Some(KEEL_POLL) {
        return Some(if policy == KEEL_FORCE && !same_tag && schedule.is_none() {
            UpdateSetting::NewBuilds
        } else {
            other()
        });
    }
    let known = [
        ImagePolicy::Patch,
        ImagePolicy::Minor,
        ImagePolicy::Major,
        ImagePolicy::SameTag,
    ]
    .into_iter()
    .find(|p| p.annotation() == (policy, same_tag));
    let schedule = schedule.map(str::trim);
    let window = schedule.and_then(UpdateWindow::from_schedule);
    Some(match known {
        Some(policy) if window.is_some() || schedule == Some(KEEL_HOURLY) => {
            UpdateSetting::Image { policy, window }
        }
        _ => other(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn read(pairs: &[(&str, &str)], images: &[&str]) -> ImageUpdates {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        ImageUpdates::read(&map, images.iter().copied())
    }

    #[test]
    fn version_policies_need_a_version_tag() {
        let all = vec![
            ImagePolicy::Patch,
            ImagePolicy::Minor,
            ImagePolicy::Major,
            ImagePolicy::SameTag,
        ];
        assert_eq!(ImagePolicy::allowed(["redis:7.2.4"]), all);
        assert_eq!(ImagePolicy::allowed(["ghcr.io/acme/app:v1.0.12"]), all);
        assert_eq!(ImagePolicy::allowed(["nginx:1.27"]), all);
        assert_eq!(ImagePolicy::allowed(["postgres:16"]), all);
        assert_eq!(ImagePolicy::allowed(["app:latest"]), all);
        assert_eq!(
            ImagePolicy::allowed(["localhost:5000/app:1.2.3"]),
            all,
            "a registry port is not a tag"
        );
        for image in ["redis:7.2.4-alpine", "redis:alpine", "app:1.2.3.4", "app"] {
            assert_eq!(
                ImagePolicy::allowed([image]),
                [ImagePolicy::SameTag],
                "{image}"
            );
        }
    }

    #[test]
    fn every_container_must_allow_a_policy() {
        assert_eq!(
            ImagePolicy::allowed(["redis:7.2.4", "exporter:main"]),
            [ImagePolicy::SameTag]
        );
    }

    #[test]
    fn a_digest_pinned_image_allows_nothing() {
        assert_eq!(ImagePolicy::allowed(["redis:7.2.4@sha256:abc"]), []);
    }

    #[test]
    fn a_window_reads_back_from_its_schedule() {
        assert_eq!(UpdateWindow::schedule(None), "@hourly");
        for window in [
            UpdateWindow {
                day: Some(Weekday::Sunday),
                hour: 3,
            },
            UpdateWindow {
                day: Some(Weekday::Saturday),
                hour: 23,
            },
            UpdateWindow { day: None, hour: 0 },
        ] {
            let schedule = UpdateWindow::schedule(Some(window));
            assert_eq!(
                UpdateWindow::from_schedule(&schedule),
                Some(window),
                "{schedule}"
            );
        }
        assert_eq!(
            UpdateWindow::schedule(Some(UpdateWindow {
                day: Some(Weekday::Sunday),
                hour: 3
            })),
            "0 0 3 * * 0"
        );
        assert_eq!(UpdateWindow::from_schedule("@hourly"), None);
        assert_eq!(UpdateWindow::from_schedule("@every 5m"), None);
        assert_eq!(UpdateWindow::from_schedule("0 30 3 * * 0"), None);
    }

    #[test]
    fn annotations_read_as_the_setting_that_wrote_them() {
        let image = read(
            &[
                ("keel.sh/policy", "patch"),
                ("keel.sh/trigger", "poll"),
                ("keel.sh/pollSchedule", "0 0 3 * * 0"),
            ],
            &["redis:7.2.4"],
        );
        assert_eq!(
            image.setting,
            Some(UpdateSetting::Image {
                policy: ImagePolicy::Patch,
                window: Some(UpdateWindow {
                    day: Some(Weekday::Sunday),
                    hour: 3
                }),
            })
        );
        let same_tag = read(
            &[
                ("keel.sh/policy", "force"),
                ("keel.sh/match-tag", "true"),
                ("keel.sh/trigger", "poll"),
                ("keel.sh/pollSchedule", "@hourly"),
            ],
            &["app:latest"],
        );
        assert_eq!(
            same_tag.setting,
            Some(UpdateSetting::Image {
                policy: ImagePolicy::SameTag,
                window: None
            })
        );
        let built = read(&[("keel.sh/policy", "force")], &["r/app:main-1"]);
        assert_eq!(built.setting, Some(UpdateSetting::NewBuilds));
    }

    #[test]
    fn foreign_and_absent_annotations() {
        assert_eq!(read(&[], &["redis:7.2.4"]).setting, None);
        assert_eq!(read(&[("keel.sh/policy", "never")], &[]).setting, None);
        assert_eq!(
            read(
                &[
                    ("keel.sh/policy", "glob:build-*"),
                    ("keel.sh/trigger", "poll"),
                    ("keel.sh/pollSchedule", "@every 10m"),
                ],
                &[]
            )
            .setting,
            Some(UpdateSetting::Other {
                policy: "glob:build-*".to_owned(),
                schedule: Some("@every 10m".to_owned()),
            })
        );
    }
}
