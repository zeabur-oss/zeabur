//! The joins [`relations`] makes through indexes (selector to pod labels, claim template to PVC
//! name, Ingress backend to Service) give what scanning every pair gives, checked on generated
//! namespaces whose names and labels collide often.

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use proptest::prelude::*;
    use serde_json::{Value, json};
    use zeabur_graph::{Snapshot, Workload, ingress_backends, relations, workloads};
    use zeabur_model::{ObjectRef, Origin, Relation, RelationKind};

    const NAMES: [&str; 5] = ["db", "db-replica", "web", "api", "x"];

    fn labels() -> impl Strategy<Value = BTreeMap<String, String>> {
        prop::collection::btree_map(
            prop_oneof!["app", "tier", "role"],
            prop_oneof!["a", "b", "c"],
            0..3,
        )
    }

    /// A Deployment or a StatefulSet (with claim templates), named from [`NAMES`].
    fn workload() -> impl Strategy<Value = Value> {
        (
            any::<bool>(),
            prop::sample::select(&NAMES[..]),
            labels(),
            prop::sample::subsequence(vec!["data", "logs"], 0..=2),
        )
            .prop_map(|(stateful, name, labels, templates)| {
                let template = json!({"metadata": {"labels": labels}, "spec": {"containers": [{"name": "main"}]}});
                if stateful {
                    let claims: Vec<Value> = templates
                        .iter()
                        .map(|t| json!({"metadata": {"name": t}}))
                        .collect();
                    json!({"apiVersion": "apps/v1", "kind": "StatefulSet",
                           "metadata": {"name": name},
                           "spec": {"selector": {}, "serviceName": name, "template": template,
                                    "volumeClaimTemplates": claims}})
                } else {
                    json!({"apiVersion": "apps/v1", "kind": "Deployment",
                           "metadata": {"name": name},
                           "spec": {"selector": {}, "template": template}})
                }
            })
    }

    fn service() -> impl Strategy<Value = Value> {
        (prop::sample::select(&NAMES[..]), labels()).prop_map(|(name, selector)| {
            json!({"apiVersion": "v1", "kind": "Service", "metadata": {"name": name},
                   "spec": {"selector": selector}})
        })
    }

    /// A PVC named like a claim template's, or nearly: another set, no ordinal, not a number.
    fn claim() -> impl Strategy<Value = Value> {
        (
            prop_oneof!["data", "logs", "data-db"],
            prop::sample::select(&NAMES[..]),
            prop_oneof!["0", "1", "12", "", "x", "0-1"],
        )
            .prop_map(|(template, set, ordinal)| {
                json!({"apiVersion": "v1", "kind": "PersistentVolumeClaim",
                       "metadata": {"name": format!("{template}-{set}-{ordinal}")}})
            })
    }

    fn ingress() -> impl Strategy<Value = Value> {
        (
            "[a-z]{1,4}",
            prop::option::of(prop::sample::select(&NAMES[..])),
            prop::collection::vec(prop::sample::select(&NAMES[..]), 0..3),
        )
            .prop_map(|(name, default, backends)| {
                let paths: Vec<Value> = backends
                    .iter()
                    .map(|b| json!({"pathType": "Prefix", "backend": {"service": {"name": b}}}))
                    .collect();
                let mut spec = json!({"rules": [{"http": {"paths": paths}}]});
                if let Some(default) = default {
                    spec["defaultBackend"] = json!({"service": {"name": default}});
                }
                json!({"apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
                       "metadata": {"name": name}, "spec": spec})
            })
    }

    fn namespace() -> impl Strategy<Value = Snapshot> {
        (
            prop::collection::vec(workload(), 0..6),
            prop::collection::vec(service(), 0..5),
            prop::collection::vec(claim(), 0..8),
            prop::collection::vec(ingress(), 0..4),
        )
            .prop_map(|(workloads, services, claims, ingresses)| {
                let mut snapshot = Snapshot {
                    namespace: "generated".to_owned(),
                    ..Snapshot::default()
                };
                let objects = workloads
                    .into_iter()
                    .chain(services)
                    .chain(claims)
                    .chain(ingresses);
                for object in objects {
                    assert!(snapshot.insert(object).unwrap());
                }
                snapshot
            })
    }

    fn pod_labels(workload: Workload<'_>) -> Option<&BTreeMap<String, String>> {
        workload
            .pod_template()
            .and_then(|template| template.metadata.as_ref())
            .and_then(|meta| meta.labels.as_ref())
    }

    /// The three joins by scanning every pair, as the README states them.
    fn scanned(snapshot: &Snapshot) -> BTreeSet<Relation> {
        let all = workloads(snapshot);
        let mut out = BTreeSet::new();
        let mut selected = Vec::new();
        for service in &snapshot.services {
            let name = service.metadata.name.as_deref().unwrap();
            let selector = service.spec.as_ref().and_then(|s| s.selector.as_ref());
            let Some(selector) = selector.filter(|s| !s.is_empty()) else {
                continue;
            };
            for workload in &all {
                let labels = pod_labels(*workload);
                if selector
                    .iter()
                    .all(|(k, v)| labels.and_then(|l| l.get(k)) == Some(v))
                {
                    selected.push((name, workload.reference()));
                    out.insert(Relation {
                        workload: workload.reference(),
                        object: ObjectRef::new("Service", name),
                        kind: RelationKind::Selector,
                        origin: Origin::field("Service", name, "spec.selector".to_owned()),
                    });
                }
            }
        }
        for workload in &all {
            let Workload::StatefulSet(set) = workload else {
                continue;
            };
            let templates = set
                .spec
                .iter()
                .flat_map(|s| s.volume_claim_templates.iter().flatten());
            for template in templates {
                let template = template.metadata.name.as_deref().unwrap();
                for claim in &snapshot.persistent_volume_claims {
                    let claim = claim.metadata.name.as_deref().unwrap();
                    let ordinal = claim.strip_prefix(&format!("{template}-{}-", workload.name()));
                    if ordinal
                        .is_some_and(|o| !o.is_empty() && o.bytes().all(|b| b.is_ascii_digit()))
                    {
                        out.insert(Relation {
                            workload: workload.reference(),
                            object: ObjectRef::new("PersistentVolumeClaim", claim),
                            kind: RelationKind::ClaimTemplate,
                            origin: Origin::field(
                                "StatefulSet",
                                workload.name(),
                                format!("spec.volumeClaimTemplates[name={template}]"),
                            ),
                        });
                    }
                }
            }
        }
        for ingress in &snapshot.ingresses {
            let name = ingress.metadata.name.as_deref().unwrap();
            for (service, path) in ingress_backends(ingress) {
                for (_, workload) in selected.iter().filter(|(s, _)| *s == service) {
                    out.insert(Relation {
                        workload: workload.clone(),
                        object: ObjectRef::new("Ingress", name),
                        kind: RelationKind::Route,
                        origin: Origin::field("Ingress", name, path.clone()),
                    });
                }
            }
        }
        out
    }

    proptest! {
        #[test]
        fn indexed_joins_find_what_scanning_every_pair_finds(snapshot in namespace()) {
            let found = relations(&snapshot);
            prop_assert!(found.windows(2).all(|pair| pair[0] < pair[1]), "sorted, without duplicates");
            let joined: BTreeSet<Relation> = found
                .into_iter()
                .filter(|r| matches!(r.kind, RelationKind::Selector | RelationKind::ClaimTemplate | RelationKind::Route))
                .collect();
            prop_assert_eq!(joined, scanned(&snapshot));
        }
    }
}
