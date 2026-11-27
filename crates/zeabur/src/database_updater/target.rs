//! The one database, image, placement and schedule a due check may update.

use std::collections::{BTreeMap, BTreeSet};

use croner::parser::{CronParser, Seconds};
use serde::Deserialize;
use zeabur_kube::DynamicObject;
use zeabur_kube::k8s_openapi::api::core::v1::{
    LocalObjectReference, Node, NodeAffinity, NodeSelectorRequirement, NodeSelectorTerm, Pod,
};
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
            return (duration >= SignedDuration::from_secs(1))
                .then(|| now.checked_add(duration).ok())
                .flatten();
        }
        let cron = CronParser::builder()
            .seconds(Seconds::Required)
            .build()
            .parse(&self.schedule)
            .ok()?;
        cron.find_next_occurrence(&now.to_zoned(TimeZone::UTC), false)
            .ok()
            .map(|t| t.timestamp())
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
                || !controller_of(&pod.metadata).is_some_and(|owner| owner.uid == self.uid)
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
