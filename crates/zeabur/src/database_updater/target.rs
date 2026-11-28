//! The one database, image, placement and schedule a due check may update.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use zeabur_kube::DynamicObject;
use zeabur_kube::k8s_openapi::api::core::v1::{
    LocalObjectReference, Node, NodeAffinity, NodeSelectorRequirement, NodeSelectorTerm, Pod,
};
use zeabur_kube::k8s_openapi::jiff::civil::Time;
use zeabur_kube::k8s_openapi::jiff::tz::TimeZone;
use zeabur_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};
use zeabur_model::adapters::controller_of;
use zeabur_model::adapters::image_reference::valid_digest;
use zeabur_model::{ImagePolicy, database_image_updates, database_poll_schedule};

#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Placement {
    node_selector: BTreeMap<String, String>,
    node_affinity: Option<NodeAffinity>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Target {
    pub(super) uid: String,
    pub(super) namespace: String,
    pub(super) name: String,
    pub(super) image: String,
    pub(super) pull_secrets: Vec<String>,
    pub(super) schedule: String,
    placement: Placement,
}

impl Target {
    pub(super) fn read(cluster: &DynamicObject) -> Option<Self> {
        let meta = &cluster.metadata;
        let spec = cluster.data.get("spec")?;
        let choices = database_image_updates(meta, spec)?;
        if !choices.allowed.contains(&ImagePolicy::SameTag) {
            return None;
        }
        let annotations = meta.annotations.as_ref()?;
        let schedule = database_poll_schedule(meta)?;
        let secrets: Vec<LocalObjectReference> = spec
            .get("imagePullSecrets")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .ok()?
            .unwrap_or_default();
        let mut pull_secrets: Vec<_> = secrets.into_iter().map(|s| s.name).collect();
        let key = "keel.sh/imagePullSecret";
        if let Some(name) = meta
            .labels
            .as_ref()
            .and_then(|l| l.get(key))
            .or_else(|| annotations.get(key))
            && !name.is_empty()
            && !pull_secrets.contains(name)
        {
            pull_secrets.push(name.clone());
        }
        Some(Self {
            uid: meta.uid.clone().filter(|s| !s.is_empty())?,
            namespace: meta.namespace.clone()?,
            name: meta.name.clone()?,
            image: spec.get("imageName")?.as_str()?.to_owned(),
            pull_secrets,
            schedule: schedule.to_owned(),
            placement: spec
                .get("affinity")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .ok()?
                .unwrap_or_default(),
        })
    }

    pub(super) fn next(&self, now: Timestamp) -> Option<Timestamp> {
        if let Some(interval) = self.schedule.strip_prefix("@every ") {
            let duration = interval.parse::<SignedDuration>().ok()?;
            if !duration.is_positive() {
                return None;
            }
            // Keel intervals start on whole seconds and have a minimum period of one second.
            return Timestamp::from_second(now.as_second().checked_add(duration.as_secs().max(1))?)
                .ok();
        }
        calendar(&self.schedule, now)
    }

    pub(super) fn architectures(&self, nodes: &[Node]) -> Result<BTreeSet<String>, String> {
        let mut result = BTreeSet::new();
        for node in nodes {
            let labels = node.metadata.labels.as_ref();
            let label = |key: &str| labels.and_then(|l| l.get(key)).map(String::as_str);
            if !self
                .placement
                .node_selector
                .iter()
                .all(|(k, v)| label(k) == Some(v))
            {
                continue;
            }
            if let Some(required) = self.placement.node_affinity.as_ref().and_then(|a| {
                a.required_during_scheduling_ignored_during_execution
                    .as_ref()
            }) && !required
                .node_selector_terms
                .iter()
                .any(|t| matches_term(t, node))
            {
                continue;
            }
            let info = node.status.as_ref().and_then(|s| s.node_info.as_ref());
            let os = info
                .map(|n| n.operating_system.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| label("kubernetes.io/os"));
            match os {
                Some("linux") => {}
                Some(_) => continue,
                None => return Err("an eligible node has no operating system".to_owned()),
            }
            let arch = info
                .map(|n| n.architecture.as_str())
                .filter(|a| !a.is_empty())
                .or_else(|| label("kubernetes.io/arch"))
                .filter(|a| !a.is_empty())
                .ok_or("an eligible Linux node has no architecture")?;
            result.insert(arch.to_owned());
        }
        if result.is_empty() {
            return Err("no eligible Linux nodes for this database".to_owned());
        }
        Ok(result)
    }

    pub(super) fn running_ids<'a>(
        &self,
        pods: &'a [Pod],
        instances: usize,
    ) -> Result<Vec<&'a str>, String> {
        let mut ids = Vec::new();
        for pod in pods {
            if pod.metadata.deletion_timestamp.is_some()
                || controller_of(&pod.metadata).is_none_or(|owner| owner.uid != self.uid)
                || pod
                    .metadata
                    .labels
                    .as_ref()
                    .and_then(|l| l.get("cnpg.io/cluster"))
                    != Some(&self.name)
            {
                continue;
            }
            let id = pod
                .status
                .as_ref()
                .and_then(|s| s.container_statuses.as_ref())
                .into_iter()
                .flatten()
                .find(|c| c.name == "postgres" && c.ready)
                .map(|c| c.image_id.as_str());
            if let Some(id) = id {
                let id = id.split_once("://").map_or(id, |(_, id)| id);
                let id = id.rsplit_once('@').map_or(id, |(_, id)| id);
                if !valid_digest(id) {
                    return Err("a PostgreSQL instance has no usable image digest".to_owned());
                }
                ids.push(id);
            }
        }
        if ids.is_empty() || ids.len() != instances {
            return Err("cannot establish a running image for every instance".to_owned());
        }
        Ok(ids)
    }
}

// Keel's five-field form omits the weekday. A wildcard-derived day field (including */2)
// intersects with the other day field, while two explicitly restricted fields match either day.
fn calendar(schedule: &str, now: Timestamp) -> Option<Timestamp> {
    let pattern = match schedule {
        "@yearly" | "@annually" => "0 0 0 1 1 *",
        "@monthly" => "0 0 0 1 * *",
        "@weekly" => "0 0 0 * * 0",
        "@midnight" | "@daily" => "0 0 0 * * *",
        "@hourly" => "0 0 * * * *",
        other => other,
    };
    let mut fields = pattern.split_whitespace();
    let seconds = cron_field(fields.next()?, 0, 59, &[])?;
    let minutes = cron_field(fields.next()?, 0, 59, &[])?;
    let hours = cron_field(fields.next()?, 0, 23, &[])?;
    let dom = fields.next()?;
    let days = cron_field(dom, 1, 31, &[])?;
    let months = cron_field(
        fields.next()?,
        1,
        12,
        &[
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
        ],
    )?;
    let dow = fields.next().unwrap_or("*");
    let weekdays = cron_field(
        dow,
        0,
        6,
        &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"],
    )?;
    if fields.next().is_some() {
        return None;
    }
    let intersect = [dom, dow]
        .iter()
        .any(|day| day.split(',').any(|r| r.starts_with(['*', '?'])));
    let first = Timestamp::from_second(now.as_second().checked_add(1)?)
        .ok()?
        .to_zoned(TimeZone::UTC);
    let mut date = first.date();
    // The existing scheduler gives up after five calendar years, including impossible dates.
    while date.year() <= first.year() + 5 {
        let day = days.contains(&date.day());
        let weekday = weekdays.contains(&date.weekday().to_sunday_zero_offset());
        let matches_day = if intersect {
            day && weekday
        } else {
            day || weekday
        };
        if months.contains(&date.month()) && matches_day {
            let earliest = if date == first.date() {
                first.time()
            } else {
                Time::MIN
            };
            for &hour in hours.range(earliest.hour()..) {
                let minute_start = if hour == earliest.hour() {
                    earliest.minute()
                } else {
                    0
                };
                for &minute in minutes.range(minute_start..) {
                    let second_start = if (hour, minute) == (earliest.hour(), earliest.minute()) {
                        earliest.second()
                    } else {
                        0
                    };
                    if let Some(&second) = seconds.range(second_start..).next() {
                        return Some(
                            date.at(hour, minute, second, 0)
                                .to_zoned(TimeZone::UTC)
                                .ok()?
                                .timestamp(),
                        );
                    }
                }
            }
        }
        date = date.tomorrow().ok()?;
    }
    None
}

// A Keel field is a list of values or inclusive ranges with optional steps. N/step runs
// from N to the field's maximum. Sunday is zero and the weekday range ends at Saturday.
fn cron_field(field: &str, min: i8, max: i8, names: &[&str]) -> Option<BTreeSet<i8>> {
    let value = |s: &str| {
        let n = s.parse::<i8>().ok().or_else(|| {
            let index = names.iter().position(|name| s.eq_ignore_ascii_case(name))?;
            i8::try_from(index).ok()?.checked_add(min)
        })?;
        (min..=max).contains(&n).then_some(n)
    };
    let mut values = BTreeSet::new();
    for part in field.split(',').filter(|part| !part.is_empty()) {
        let (range, step) = part
            .split_once('/')
            .map_or((part, None), |(r, s)| (r, Some(s)));
        let (start, end) = if matches!(range, "*" | "?") {
            (min, max)
        } else if let Some((lo, hi)) = range.split_once('-') {
            (value(lo)?, value(hi)?)
        } else {
            let start = value(range)?;
            (start, if step.is_some() { max } else { start })
        };
        let step = step.unwrap_or("1").parse::<usize>().ok()?;
        if step == 0 || start > end {
            return None;
        }
        values.extend((start..=end).step_by(step));
    }
    (!values.is_empty()).then_some(values)
}

// Kubernetes ORs nodeSelectorTerms and ANDs the requirements in each nonempty term.
fn matches_term(term: &NodeSelectorTerm, node: &Node) -> bool {
    let labels = term.match_expressions.as_deref().unwrap_or_default();
    let fields = term.match_fields.as_deref().unwrap_or_default();
    !(labels.is_empty() && fields.is_empty())
        && labels.iter().all(|r| {
            matches_value(
                r,
                node.metadata
                    .labels
                    .as_ref()
                    .and_then(|l| l.get(&r.key))
                    .map(String::as_str),
            )
        })
        && fields
            .iter()
            .all(|r| r.key == "metadata.name" && matches_value(r, node.metadata.name.as_deref()))
}

fn matches_value(requirement: &NodeSelectorRequirement, actual: Option<&str>) -> bool {
    let values = requirement.values.as_deref().unwrap_or_default();
    let included = || actual.is_some_and(|v| values.iter().any(|wanted| wanted == v));
    match requirement.operator.as_str() {
        "In" => included(),
        "NotIn" => !included(),
        "Exists" => actual.is_some(),
        "DoesNotExist" => actual.is_none(),
        operator @ ("Gt" | "Lt") => {
            let Some((actual, bound)) = actual
                .and_then(|a| a.parse::<i64>().ok())
                .zip(values.first().and_then(|v| v.parse::<i64>().ok()))
            else {
                return false;
            };
            if operator == "Gt" {
                actual > bound
            } else {
                actual < bound
            }
        }
        _ => false,
    }
}
