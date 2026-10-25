//! What deleting a service takes with it: the objects the workload made and nothing else uses.
//!
//! - **Made by the service.** An object is the service's when it carries the label
//!   `app.kubernetes.io/name: <service>` that "Deploy an image" and "Deploy from Git" write on
//!   everything they create, or where its name is how Zeabur or Kubernetes made it for the service:
//!   a route `<service>-<host>` that "Add domain" created, the `<route>-tls` certificate Secret of
//!   a route that goes, a claim a StatefulSet's template made (`<template>-<service>-<n>`).
//! - **Used by nothing else.** A ConfigMap, Secret or claim another workload reads or mounts, a
//!   Kubernetes Service that also selects another workload's pods, and a route that also leads to
//!   one, stay. So does a route to a Service that stays.
//! - **Not in the plan.** What an `ownerReference` ties to the workload goes with it through the
//!   garbage collector, and the deletion lists it already. Objects Helm, Argo CD or Flux manage,
//!   and objects another controller owns (a database's Secrets), stay: deleting them is theirs. A
//!   Zeabur Builder `Build` and its Git credentials are deleted by their own changes.
//! - Every object the service uses that stays is named with the reason.

use std::collections::BTreeSet;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::Value;
use zeabur_graph::{Snapshot, ingress_backends, is_machinery_secret, relations, workloads};
use zeabur_model::adapters::provenance::{Tool, detect_tool};
use zeabur_model::adapters::{controller_of, label};
use zeabur_model::{HttpRoute, RelationKind, WorkloadKind, WorkloadRef};

use crate::change::{KeptReason, ObjectName, Warning};
use crate::checks::ResourceRef;
use crate::rules::route_name;

/// The label "Deploy an image" and "Deploy from Git" put on every object they create.
pub const SERVICE_LABEL: &str = "app.kubernetes.io/name";

/// The label of the Secrets a Zeabur Builder `Build` fetches with.
const GIT_CREDENTIALS_LABEL: &str = "builder.zeabur.com/git-credentials";

/// The ConfigMap kube-apiserver publishes into every namespace.
const ROOT_CA_CONFIG_MAP: &str = "kube-root-ca.crt";

/// What deleting a workload deletes besides it, and what it uses that stays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Owned {
    /// The objects to delete, routes first and claims last.
    pub deleted: Vec<(ResourceRef, String)>,
    /// A [`Warning::Stays`] for each object it uses or made that stays.
    pub stays: Vec<Warning>,
}

fn resource(group: &str, version: &str, plural: &str) -> ResourceRef {
    ResourceRef {
        group: group.to_owned(),
        version: version.to_owned(),
        resource: plural.to_owned(),
    }
}

/// The workload `resource` `name` names, when `resource` is a workload kind.
#[must_use]
pub fn workload_of(resource: &ResourceRef, name: &str) -> Option<WorkloadRef> {
    let kind = match (resource.group.as_str(), resource.resource.as_str()) {
        ("apps", "deployments") => WorkloadKind::Deployment,
        ("apps", "statefulsets") => WorkloadKind::StatefulSet,
        ("apps", "daemonsets") => WorkloadKind::DaemonSet,
        ("batch", "cronjobs") => WorkloadKind::CronJob,
        ("batch", "jobs") => WorkloadKind::Job,
        _ => return None,
    };
    Some(WorkloadRef {
        kind,
        name: name.to_owned(),
    })
}

/// The objects of `workload` in `snapshot` (and `http_routes`) to delete with it.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one pass per kind, read top to bottom"
)]
pub fn owned(snapshot: &Snapshot, http_routes: &[Value], workload: &WorkloadRef) -> Owned {
    let mut out = Owned::default();
    let Some(this) = workloads(snapshot)
        .into_iter()
        .find(|w| w.kind() == workload.kind && w.name() == workload.name)
    else {
        return out;
    };
    let uid = this.meta().uid.clone();
    let name = workload.name.as_str();
    let relations = relations(snapshot);
    let users = |kind: &str, object: &str| -> BTreeSet<WorkloadRef> {
        relations
            .iter()
            .filter(|r| r.object.kind == kind && r.object.name == object)
            .map(|r| r.workload.clone())
            .collect()
    };
    let decide = |out: &mut Owned, kind: &str, meta: &ObjectMeta, made: bool| -> Option<bool> {
        let object = meta.name.as_deref().unwrap_or_default();
        let mut using = users(kind, object);
        let used = using.remove(workload);
        let labelled = label(meta.labels.as_ref(), SERVICE_LABEL) == Some(name);
        if !(used || labelled || made) {
            return None;
        }
        let tied = meta
            .owner_references
            .iter()
            .flatten()
            .any(|o| Some(&o.uid) == uid.as_ref());
        if tied {
            return None;
        }
        let at = Stay::new(kind, object);
        let warning = if !using.is_empty() {
            at.shared(using.into_iter().collect())
        } else if let Some(tool) = detect_tool(meta) {
            at.managed(tool)
        } else if let Some(controller) = controller_of(meta) {
            at.controlled(&controller.kind, &controller.name)
        } else if labelled || made {
            return Some(true);
        } else {
            at.because(KeptReason::NotMade)
        };
        out.stays.push(warning);
        Some(false)
    };

    let mut services: BTreeSet<&str> = BTreeSet::new();
    for service in &snapshot.services {
        if decide(&mut out, "Service", &service.metadata, false) == Some(true) {
            let object = service.metadata.name.as_deref().unwrap_or_default();
            services.insert(object);
        }
    }
    // The Services that stay but select this workload's pods: a route to one keeps working.
    let selecting: BTreeSet<&str> = relations
        .iter()
        .filter(|r| &r.workload == workload && r.kind == RelationKind::Selector)
        .map(|r| r.object.name.as_str())
        .collect();

    let mut routes: Vec<(ResourceRef, String)> = Vec::new();
    let mut certificates: BTreeSet<String> = BTreeSet::new();
    let mut kept_certificates: BTreeSet<String> = BTreeSet::new();
    for ingress in &snapshot.ingresses {
        let object = ingress.metadata.name.as_deref().unwrap_or_default();
        let backends: BTreeSet<&str> = ingress_backends(ingress)
            .into_iter()
            .map(|(service, _)| service)
            .collect();
        let hosts: Vec<&str> = ingress
            .spec
            .iter()
            .flat_map(|s| s.rules.iter().flatten())
            .filter_map(|r| r.host.as_deref())
            .collect();
        let made = object == name || hosts.iter().any(|h| route_name(name, h) == object);
        let leads_here = backends
            .iter()
            .any(|b| services.contains(b) || selecting.contains(b));
        let tls: Vec<String> = ingress
            .spec
            .iter()
            .flat_map(|s| s.tls.iter().flatten())
            .filter_map(|t| t.secret_name.clone())
            .collect();
        let gone = match decide(&mut out, "Ingress", &ingress.metadata, made && leads_here) {
            Some(true) if backends.iter().all(|b| services.contains(b)) => true,
            Some(true) => {
                out.stays
                    .push(Stay::new("Ingress", object).because(KeptReason::RoutesElsewhere));
                false
            }
            _ => false,
        };
        if gone {
            routes.push((
                resource("networking.k8s.io", "v1", "ingresses"),
                object.to_owned(),
            ));
            let own = format!("{object}-tls");
            certificates.extend(tls.into_iter().filter(|secret| *secret == own));
        } else {
            kept_certificates.extend(tls);
        }
    }
    for object in http_routes {
        let Ok(route) = serde_json::from_value::<HttpRoute>(object.clone()) else {
            continue;
        };
        let Some(meta) = object
            .get("metadata")
            .cloned()
            .and_then(|m| serde_json::from_value::<ObjectMeta>(m).ok())
        else {
            continue;
        };
        let route_object = meta.name.clone().unwrap_or_default();
        let backends: BTreeSet<&str> = route.local_services().collect();
        let made = route
            .spec
            .hostnames
            .iter()
            .any(|h| route_name(name, h) == route_object);
        let leads_here = backends
            .iter()
            .any(|b| services.contains(b) || selecting.contains(b));
        let others: BTreeSet<WorkloadRef> = relations
            .iter()
            .filter(|r| {
                r.kind == RelationKind::Selector
                    && backends.contains(r.object.name.as_str())
                    && &r.workload != workload
            })
            .map(|r| r.workload.clone())
            .collect();
        let labelled = label(meta.labels.as_ref(), SERVICE_LABEL) == Some(name);
        if !(leads_here || labelled) {
            continue;
        }
        let at = Stay::new("HTTPRoute", &route_object);
        if !others.is_empty() {
            out.stays.push(at.shared(others.into_iter().collect()));
        } else if let Some(tool) = detect_tool(&meta) {
            out.stays.push(at.managed(tool));
        } else if !(made || labelled) {
            out.stays.push(at.because(KeptReason::NotMade));
        } else if !backends.iter().all(|b| services.contains(b)) {
            out.stays.push(at.because(KeptReason::RoutesElsewhere));
        } else {
            routes.push((
                resource("gateway.networking.k8s.io", "v1", "httproutes"),
                route_object,
            ));
        }
    }
    out.deleted.extend(routes);
    out.deleted.extend(
        services
            .iter()
            .map(|s| (resource("", "v1", "services"), (*s).to_owned())),
    );

    for map in &snapshot.config_maps {
        if map.metadata.name.as_deref() == Some(ROOT_CA_CONFIG_MAP) {
            continue;
        }
        if decide(&mut out, "ConfigMap", &map.metadata, false) == Some(true) {
            let object = map.metadata.name.clone().unwrap_or_default();
            out.deleted.push((resource("", "v1", "configmaps"), object));
        }
    }
    for secret in &snapshot.secrets {
        let object = secret.metadata.name.as_deref().unwrap_or_default();
        let builds = label(secret.metadata.labels.as_ref(), GIT_CREDENTIALS_LABEL).is_some();
        if is_machinery_secret(secret) || builds {
            continue;
        }
        let certificate = certificates.contains(object) && !kept_certificates.contains(object);
        // cert-manager's own Secret goes with its Certificate, which goes with the Ingress.
        let issued = controller_of(&secret.metadata).is_some_and(|c| c.kind == "Certificate");
        if certificates.contains(object) && issued {
            continue;
        }
        if decide(&mut out, "Secret", &secret.metadata, certificate) == Some(true) {
            out.deleted
                .push((resource("", "v1", "secrets"), object.to_owned()));
        }
    }
    let templated: BTreeSet<&str> = relations
        .iter()
        .filter(|r| &r.workload == workload && r.kind == RelationKind::ClaimTemplate)
        .map(|r| r.object.name.as_str())
        .collect();
    for claim in &snapshot.persistent_volume_claims {
        let object = claim.metadata.name.as_deref().unwrap_or_default();
        let made = templated.contains(object);
        if decide(&mut out, "PersistentVolumeClaim", &claim.metadata, made) == Some(true) {
            out.deleted.push((
                resource("", "v1", "persistentvolumeclaims"),
                object.to_owned(),
            ));
        }
    }
    out
}

/// A [`Warning::Stays`] of one object.
struct Stay(ObjectName);

impl Stay {
    fn new(kind: &str, name: &str) -> Self {
        Self(ObjectName {
            kind: kind.to_owned(),
            name: name.to_owned(),
        })
    }

    fn warning(
        self,
        reason: KeptReason,
        workloads: Vec<WorkloadRef>,
        owner: Option<ObjectName>,
        tool: Option<Tool>,
    ) -> Warning {
        Warning::Stays {
            object: self.0,
            reason,
            workloads,
            owner,
            tool,
        }
    }

    fn because(self, reason: KeptReason) -> Warning {
        self.warning(reason, Vec::new(), None, None)
    }

    fn shared(self, workloads: Vec<WorkloadRef>) -> Warning {
        self.warning(KeptReason::Shared, workloads, None, None)
    }

    fn managed(self, tool: Tool) -> Warning {
        self.warning(KeptReason::Managed, Vec::new(), None, Some(tool))
    }

    fn controlled(self, kind: &str, name: &str) -> Warning {
        let owner = ObjectName {
            kind: kind.to_owned(),
            name: name.to_owned(),
        };
        self.warning(KeptReason::Controlled, Vec::new(), Some(owner), None)
    }
}
