//! Which services appear to call which.
//!
//! Kubernetes records no "calls" relationship between workloads, so a connection is read from
//! two kinds of evidence in the projected services:
//!
//! - **A shared Secret or ConfigMap.** A service and a data store that read the same object usually
//!   share the store's address or credentials, and the service is the caller. Both references are
//!   fields of Kubernetes objects, so the connection is not inferred.
//! - **A variable's value naming a Service.** A literal value whose text names a Kubernetes Service
//!   in the namespace, by its short name or its cluster DNS name [k8s-dns] (`redis://redis:6379`, `http://api.shop.svc.cluster.local:8080`),
//!   points at the workloads that Service selects. Text is not a reference, so the connection is
//!   inferred.
//!
//! Neither says traffic flows; clients present connections as configuration.
//!
//! [k8s-dns]: https://kubernetes.io/docs/concepts/services-networking/dns-pod-service/#services

use std::collections::{BTreeMap, BTreeSet};

use zeabur_model::adapters::group::DATABASES_GROUP;
use zeabur_model::adapters::images::{DataStore, data_store_image};
use zeabur_model::{
    Connection, ConnectionEvidence, MountCategory, ObjectRef, Service, SourceKind, WorkloadKind,
    WorkloadRef,
};

/// The kind of data store a service is, if it is one.
///
/// It is one when it runs a data store image (the first container whose image is named like
/// one decides the kind), is in the `databases` group, or is a StatefulSet with persistent
/// storage.
#[must_use]
pub fn data_store(service: &Service) -> Option<DataStore> {
    let workload = &service.workload;
    let by_image = service
        .containers
        .iter()
        .filter_map(|container| container.image.as_deref())
        .find_map(data_store_image);
    let by_storage = workload.classification.group == DATABASES_GROUP
        || (workload.kind == WorkloadKind::StatefulSet
            && service.containers.iter().any(|container| {
                container
                    .mounts
                    .iter()
                    .any(|mount| mount.category == MountCategory::Persistent)
            }));
    by_image.or_else(|| by_storage.then_some(DataStore::Other))
}

/// The connections between `services` of `namespace`, sorted by source then target, each with
/// its evidence sorted.
#[must_use]
pub fn connections(namespace: &str, services: &[Service]) -> Vec<Connection> {
    let mut found: BTreeMap<(WorkloadRef, WorkloadRef), BTreeSet<ConnectionEvidence>> =
        BTreeMap::new();
    let mut add = |from: WorkloadRef, to: &WorkloadRef, evidence: ConnectionEvidence| {
        if from != *to {
            found
                .entry((from, to.clone()))
                .or_default()
                .insert(evidence);
        }
    };

    // Kubernetes Service name → the workloads it selects.
    let mut selected: BTreeMap<&str, Vec<WorkloadRef>> = BTreeMap::new();
    for service in services {
        for name in &service.endpoint_names {
            selected
                .entry(name.as_str())
                .or_default()
                .push(reference(service));
        }
    }

    for service in services {
        let me = reference(service);
        for container in &service.containers {
            for variable in &container.env {
                if variable.source.kind != SourceKind::Literal {
                    continue;
                }
                let Some(value) = variable.value.as_deref() else {
                    continue;
                };
                for (name, targets) in mentioned_services(value, namespace, &selected) {
                    for target in targets {
                        add(
                            me.clone(),
                            target,
                            ConnectionEvidence::EnvValue {
                                container: container.name.clone(),
                                variable: variable.name.clone(),
                                service: name.to_owned(),
                            },
                        );
                    }
                }
            }
        }
    }

    let stores: Vec<(WorkloadRef, BTreeSet<ObjectRef>)> = services
        .iter()
        .filter(|service| data_store(service).is_some())
        .map(|service| (reference(service), config_objects(service)))
        .collect();
    for service in services
        .iter()
        .filter(|service| data_store(service).is_none())
    {
        let mine = config_objects(service);
        for (store, theirs) in &stores {
            for object in mine.intersection(theirs) {
                add(
                    reference(service),
                    store,
                    ConnectionEvidence::SharedObject {
                        object: object.clone(),
                    },
                );
            }
        }
    }

    found
        .into_iter()
        .map(|((from, to), evidence)| Connection {
            from,
            to,
            inferred: evidence
                .iter()
                .all(|e| matches!(e, ConnectionEvidence::EnvValue { .. })),
            evidence: evidence.into_iter().collect(),
        })
        .collect()
}

fn reference(service: &Service) -> WorkloadRef {
    WorkloadRef {
        kind: service.workload.kind,
        name: service.workload.name.clone(),
    }
}

/// The Secrets and ConfigMaps a service reads, through `env`, `envFrom` or volumes.
fn config_objects(service: &Service) -> BTreeSet<ObjectRef> {
    let mut out = BTreeSet::new();
    let mut add = |kind: SourceKind, name: Option<&str>| {
        let kind = match kind {
            SourceKind::Secret => "Secret",
            SourceKind::ConfigMap => "ConfigMap",
            SourceKind::Literal
            | SourceKind::PodField
            | SourceKind::ResourceField
            | SourceKind::File => return,
        };
        if let Some(name) = name {
            out.insert(ObjectRef::new(kind, name));
        }
    };
    for container in &service.containers {
        for variable in &container.env {
            add(variable.source.kind, variable.source.name.as_deref());
        }
        for imported in &container.imported_env {
            add(imported.source.kind, imported.source.name.as_deref());
        }
        for mount in &container.mounts {
            if let Some(object) = &mount.object {
                let kind = match object.kind.as_str() {
                    "Secret" => SourceKind::Secret,
                    "ConfigMap" => SourceKind::ConfigMap,
                    _ => continue,
                };
                add(kind, Some(&object.name));
            }
        }
    }
    out
}

/// Whether `value` names `name` as a host in `namespace`, the way a variable names a Service
/// ([`connections`]): `name` itself, or followed by `.<namespace>`, `.<namespace>.svc` or
/// `.<namespace>.svc.cluster.local`.
#[must_use]
pub fn names_host(value: &str, namespace: &str, name: &str) -> bool {
    !mentioned_services(value, namespace, &BTreeMap::from([(name, name)])).is_empty()
}

/// The names in `known` that `value` names as a host in `namespace`, as [`names_host`] decides
/// for one name, reading `value` once.
#[must_use]
pub fn named_hosts<'a, V>(
    value: &str,
    namespace: &str,
    known: &BTreeMap<&'a str, V>,
) -> BTreeSet<&'a str> {
    mentioned_services(value, namespace, known)
        .into_keys()
        .collect()
}

/// The names in `known` that `value` mentions as a host: a token of letters, digits, dots and
/// hyphens that is the name itself, or the name followed by `.<namespace>`,
/// `.<namespace>.svc` or `.<namespace>.svc.cluster.local`.
fn mentioned_services<'a, 'm, V>(
    value: &str,
    namespace: &str,
    known: &'m BTreeMap<&'a str, V>,
) -> BTreeMap<&'a str, &'m V> {
    let suffixes = [
        String::new(),
        format!(".{namespace}"),
        format!(".{namespace}.svc"),
        format!(".{namespace}.svc.cluster.local"),
    ];
    value
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'))
        .filter(|token| !token.is_empty())
        .filter_map(|token| {
            let (head, _) = token.split_once('.').unwrap_or((token, ""));
            let (name, targets) = known.get_key_value(head)?;
            suffixes
                .iter()
                .any(|suffix| token == format!("{head}{suffix}"))
                .then_some((*name, targets))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_names_are_found_in_urls_and_dns_names() {
        let known = BTreeMap::from([("redis", 1), ("api", 2), ("db", 3)]);
        let found = |value| -> Vec<&str> {
            mentioned_services(value, "shop", &known)
                .into_keys()
                .collect()
        };
        assert_eq!(found("redis://redis:6379/0"), ["redis"]);
        assert_eq!(found("http://api.shop.svc.cluster.local:8080"), ["api"]);
        assert_eq!(found("db"), ["db"]);
        assert_eq!(found("postgres://u@db.shop:5432/x"), ["db"]);
        // Another namespace, a longer host, or a word inside a sentence-like token are not
        // this namespace's Services.
        assert_eq!(
            found("http://api.other.svc.cluster.local"),
            Vec::<&str>::new()
        );
        assert_eq!(found("https://api.example.com"), Vec::<&str>::new());
        assert_eq!(found("redis-cache"), Vec::<&str>::new());
    }

    #[test]
    fn one_value_names_every_known_host_it_mentions() {
        let known = BTreeMap::from([("api", "api"), ("db", "db"), ("redis", "redis")]);
        let value = "http://api:8080,postgres://db.shop.svc:5432/x,other.example.com,cache";
        assert_eq!(
            named_hosts(value, "shop", &known)
                .into_iter()
                .collect::<Vec<_>>(),
            ["api", "db"]
        );
    }
}
