//! The input of the projection.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    ConfigMap, PersistentVolumeClaim, Pod, Secret, Service as KubeService,
};
use k8s_openapi::api::networking::v1::Ingress;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::jiff::Timestamp;

/// The objects of one namespace, as read with a person's credential.
///
/// The projection is a pure function of this value. Whoever builds it decides what was
/// readable; the projection never asks for more.
///
/// **Secrets carry metadata and `type` only.** Their `data` and `stringData` are expected to be
/// absent, and the projection never reads them even when present: a Secret's values never
/// reach a projected view. A Secret's key names are not read either; how many keys it holds,
/// which an `envFrom` import shows, comes from [`Snapshot::secret_key_counts`].
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// The namespace every object belongs to.
    pub namespace: String,
    /// The Deployments.
    pub deployments: Vec<Deployment>,
    /// Every StatefulSet.
    pub stateful_sets: Vec<StatefulSet>,
    /// Every DaemonSet.
    pub daemon_sets: Vec<DaemonSet>,
    /// The Jobs, including those a CronJob created.
    pub jobs: Vec<Job>,
    /// Every CronJob.
    pub cron_jobs: Vec<CronJob>,
    /// The Pods. The projection does not read them yet; they are here for the views of a
    /// workload's pods.
    pub pods: Vec<Pod>,
    /// The Kubernetes Services.
    pub services: Vec<KubeService>,
    /// The ConfigMaps, in full.
    pub config_maps: Vec<ConfigMap>,
    /// The Secrets: metadata and `type` only.
    pub secrets: Vec<Secret>,
    /// How many keys each Secret holds, by name, where the reader could tell.
    pub secret_key_counts: BTreeMap<String, usize>,
    /// The persistent volume claims.
    pub persistent_volume_claims: Vec<PersistentVolumeClaim>,
    /// The Ingresses.
    pub ingresses: Vec<Ingress>,
    /// When the objects were read. It decides whether a container's last crash is recent (see
    /// [`crate::failing`]); without it only the reasons the kubelet reports count.
    pub now: Option<Timestamp>,
}

/// The name of an object, empty when it has none (kube-apiserver never returns such an
/// object).
pub(crate) fn name_of(meta: &ObjectMeta) -> &str {
    meta.name.as_deref().unwrap_or_default()
}

impl Snapshot {
    /// Adds `object` to the list of its `kind`, as a cluster reader would: a Secret without its
    /// values, keeping only how many keys it holds. `Ok(false)` for a kind a snapshot does not
    /// hold (an `Event`, an `HTTPRoute`), which is left to the caller.
    ///
    /// # Errors
    ///
    /// When `object` is not an object of its kind.
    pub fn insert(&mut self, object: serde_json::Value) -> Result<bool, serde_json::Error> {
        use serde_json::from_value;
        let kind = object
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        match kind {
            "Deployment" => self.deployments.push(from_value(object)?),
            "StatefulSet" => self.stateful_sets.push(from_value(object)?),
            "DaemonSet" => self.daemon_sets.push(from_value(object)?),
            "Job" => self.jobs.push(from_value(object)?),
            "CronJob" => self.cron_jobs.push(from_value(object)?),
            "Pod" => self.pods.push(from_value(object)?),
            "Service" => self.services.push(from_value(object)?),
            "ConfigMap" => self.config_maps.push(from_value(object)?),
            "PersistentVolumeClaim" => self.persistent_volume_claims.push(from_value(object)?),
            "Ingress" => self.ingresses.push(from_value(object)?),
            "Secret" => {
                let mut secret: Secret = from_value(object)?;
                let keys = secret.data.take().map_or(0, |data| data.len())
                    + secret.string_data.take().map_or(0, |data| data.len());
                if let Some(name) = secret.metadata.name.clone() {
                    self.secret_key_counts.insert(name, keys);
                }
                self.secrets.push(secret);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The ConfigMap called `name`.
    #[must_use]
    pub fn config_map(&self, name: &str) -> Option<&ConfigMap> {
        self.config_maps
            .iter()
            .find(|object| name_of(&object.metadata) == name)
    }

    /// The Secret called `name` (metadata and type only).
    #[must_use]
    pub fn secret(&self, name: &str) -> Option<&Secret> {
        self.secrets
            .iter()
            .find(|object| name_of(&object.metadata) == name)
    }

    /// The persistent volume claim called `name`.
    #[must_use]
    pub fn persistent_volume_claim(&self, name: &str) -> Option<&PersistentVolumeClaim> {
        self.persistent_volume_claims
            .iter()
            .find(|object| name_of(&object.metadata) == name)
    }
}

/// The ConfigMaps, Secrets and PVCs of a snapshot by name, for the lookups the projection makes
/// for every workload. Like [`Snapshot::config_map`] and the others, the first object of a name
/// wins.
#[derive(Debug)]
pub(crate) struct ByName<'a> {
    config_maps: BTreeMap<&'a str, &'a ConfigMap>,
    secrets: BTreeMap<&'a str, &'a Secret>,
    persistent_volume_claims: BTreeMap<&'a str, &'a PersistentVolumeClaim>,
}

impl<'a> ByName<'a> {
    pub(crate) fn new(snapshot: &'a Snapshot) -> Self {
        Self {
            config_maps: first_by_name(&snapshot.config_maps, |object| &object.metadata),
            secrets: first_by_name(&snapshot.secrets, |object| &object.metadata),
            persistent_volume_claims: first_by_name(&snapshot.persistent_volume_claims, |object| {
                &object.metadata
            }),
        }
    }

    pub(crate) fn config_map(&self, name: &str) -> Option<&'a ConfigMap> {
        self.config_maps.get(name).copied()
    }

    pub(crate) fn secret(&self, name: &str) -> Option<&'a Secret> {
        self.secrets.get(name).copied()
    }

    pub(crate) fn persistent_volume_claim(&self, name: &str) -> Option<&'a PersistentVolumeClaim> {
        self.persistent_volume_claims.get(name).copied()
    }
}

fn first_by_name<T>(objects: &[T], meta: impl Fn(&T) -> &ObjectMeta) -> BTreeMap<&str, &T> {
    let mut out = BTreeMap::new();
    for object in objects {
        out.entry(name_of(meta(object))).or_insert(object);
    }
    out
}
