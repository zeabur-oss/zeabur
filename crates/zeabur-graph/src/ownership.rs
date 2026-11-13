//! Which object belongs to which: the controller owner references of a snapshot's objects,
//! walked by UID up to a root, and the root classified by `zeabur-model`'s database operators.
//!
//! An object's parent is its controller [k8s-owners]; a persistent volume claim without one is
//! the StatefulSet's whose claim template named it (`<template>-<set>-<ordinal>`), since a
//! StatefulSet adds owner references to its claims only under a `Delete` retention policy
//! [k8s-sts-retention].
//! A reference names its owner by API group, kind, name and UID in the dependent's namespace;
//! an object that differs in any of these, such as a replacement of the same name, is not that
//! owner. An owner that was not read, such as a database operator's custom object, is still a
//! node: its dependents name it, so it can be classified.
//!
//! [k8s-owners]: https://kubernetes.io/docs/concepts/overview/working-with-objects/owners-dependents/
//! [k8s-sts-retention]: https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/#persistentvolumeclaim-retention

use std::collections::{BTreeMap, BTreeSet};

use k8s_openapi::api::apps::v1::StatefulSet;
use k8s_openapi::api::core::v1::PersistentVolumeClaim;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use k8s_openapi::{Metadata, Resource};
use serde::Serialize;
use zeabur_model::adapters::operators::{DatabaseCluster, database_of};
use zeabur_model::adapters::{api_group, controller_of};

use crate::Snapshot;
use crate::attribution::created_by_claim_template;
use crate::snapshot::name_of;

/// An object's identity: two objects are the same only when all of it matches.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ObjectKey {
    /// Its namespace.
    pub namespace: String,
    /// Its API group, empty for the core group.
    pub group: String,
    /// Its kind.
    pub kind: String,
    /// Its name.
    pub name: String,
    /// Its UID.
    pub uid: String,
}

impl ObjectKey {
    /// The key of a typed object.
    #[must_use]
    pub fn of<K: Resource + Metadata<Ty = ObjectMeta>>(object: &K) -> Self {
        let meta = object.metadata();
        Self {
            namespace: meta.namespace.clone().unwrap_or_default(),
            group: K::GROUP.to_owned(),
            kind: K::KIND.to_owned(),
            name: name_of(meta).to_owned(),
            uid: meta.uid.clone().unwrap_or_default(),
        }
    }

    fn owner(namespace: &str, owner: &OwnerReference) -> Self {
        Self {
            namespace: namespace.to_owned(),
            group: api_group(&owner.api_version).to_owned(),
            kind: owner.kind.clone(),
            name: owner.name.clone(),
            uid: owner.uid.clone(),
        }
    }
}

/// The ownership graph of a snapshot.
#[derive(Debug, Clone, Default)]
pub struct OwnerGraph {
    /// Every object read and every owner named, with its parent.
    parents: BTreeMap<ObjectKey, Option<ObjectKey>>,
    read: BTreeSet<ObjectKey>,
}

impl OwnerGraph {
    /// The graph of the snapshot's objects.
    #[must_use]
    pub fn new(snapshot: &Snapshot) -> Self {
        let mut objects = Vec::new();
        add(&mut objects, &snapshot.deployments);
        add(&mut objects, &snapshot.stateful_sets);
        add(&mut objects, &snapshot.daemon_sets);
        add(&mut objects, &snapshot.jobs);
        add(&mut objects, &snapshot.cron_jobs);
        add(&mut objects, &snapshot.pods);
        add(&mut objects, &snapshot.services);
        add(&mut objects, &snapshot.config_maps);
        add(&mut objects, &snapshot.secrets);
        add(&mut objects, &snapshot.persistent_volume_claims);
        add(&mut objects, &snapshot.ingresses);
        let mut graph = Self::default();
        for (key, meta) in objects {
            let parent = match controller_of(meta) {
                Some(owner) => Some(ObjectKey::owner(&key.namespace, owner)),
                None if key.group.is_empty() && key.kind == PersistentVolumeClaim::KIND => {
                    claim_template_set(&snapshot.stateful_sets, &key)
                }
                None => None,
            };
            if let Some(parent) = &parent {
                graph.parents.entry(parent.clone()).or_insert(None);
            }
            graph.parents.insert(key.clone(), parent);
            graph.read.insert(key);
        }
        graph
    }

    /// Where `key`'s walk up its owners ends: the first node with no parent, or for an owner
    /// cycle the least node on the cycle. An unknown key is its own root.
    #[must_use]
    pub fn root<'a>(&'a self, key: &'a ObjectKey) -> &'a ObjectKey {
        let mut path = vec![key];
        let mut at = key;
        while let Some(Some(parent)) = self.parents.get(at) {
            if let Some(start) = path.iter().position(|seen| *seen == parent) {
                return path
                    .get(start..)
                    .and_then(|cycle| cycle.iter().min().copied())
                    .unwrap_or(parent);
            }
            path.push(parent);
            at = parent;
        }
        at
    }

    /// Every root, in key order.
    pub fn roots(&self) -> impl Iterator<Item = &ObjectKey> {
        self.parents.keys().filter(|key| self.root(key) == *key)
    }

    /// The nodes whose root is `root`, other than itself, in key order.
    pub fn members<'a>(&'a self, root: &'a ObjectKey) -> impl Iterator<Item = &'a ObjectKey> {
        self.parents
            .keys()
            .filter(move |key| *key != root && self.root(key) == root)
    }

    /// Whether `key` was read, rather than only named by a dependent.
    #[must_use]
    pub fn is_read(&self, key: &ObjectKey) -> bool {
        self.read.contains(key)
    }

    /// The database cluster `key` belongs to: its root, when a database operator's kind.
    #[must_use]
    pub fn database<'a>(&'a self, key: &'a ObjectKey) -> Option<DatabaseCluster<'a>> {
        let root = self.root(key);
        database_of(&root.group, &root.kind, &root.name)
    }
}

fn add<'a, K: Resource + Metadata<Ty = ObjectMeta>>(
    out: &mut Vec<(ObjectKey, &'a ObjectMeta)>,
    objects: &'a [K],
) {
    out.extend(
        objects
            .iter()
            .map(|object| (ObjectKey::of(object), object.metadata())),
    );
}

/// The StatefulSet of the claim's namespace whose claim template named it, when exactly one did.
fn claim_template_set(sets: &[StatefulSet], claim: &ObjectKey) -> Option<ObjectKey> {
    let mut named = sets
        .iter()
        .map(ObjectKey::of)
        .zip(sets)
        .filter(|(key, set)| {
            key.namespace == claim.namespace
                && set
                    .spec
                    .iter()
                    .flat_map(|spec| spec.volume_claim_templates.iter().flatten())
                    .any(|template| {
                        created_by_claim_template(
                            &claim.name,
                            name_of(&template.metadata),
                            &key.name,
                        )
                    })
        });
    match (named.next(), named.next()) {
        (Some((key, _)), None) => Some(key),
        _ => None,
    }
}
