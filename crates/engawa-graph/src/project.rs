//! The whole namespace as a project.

use std::collections::BTreeMap;

use engawa_adapters::provenance::{detect_tool, managed_by};
use engawa_model::{
    Attribution, Container, ContainerResources, Endpoint, NamespaceAttribution, Origin,
    ProbeHandler, ProbeSummary, Probes, Project, Provenance, Quantities, Resource, ResourceKind,
    Route, Service, WorkloadRef,
};
use k8s_openapi::api::core::v1::{Probe, ResourceRequirements, Service as KubeService};
use k8s_openapi::api::networking::v1::Ingress;
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

use crate::attribution::{attribution_of, ingress_backends, relations_of};
use crate::connections::{connections, data_store};
use crate::env::{env, env_from_in, key_count};
use crate::mounts::mounts_in;
use crate::network::{ports, route_rules};
use crate::snapshot::{ByName, Snapshot, name_of};
use crate::workload::{Workload, containers, summarise, workloads};

/// Which tool manages an object, from its labels and annotations.
///
/// Any tool found means a direct edit is likely to be reverted on its next sync or upgrade, so
/// a client warns before making one.
#[must_use]
pub fn provenance(meta: &ObjectMeta) -> Provenance {
    let tool = detect_tool(meta);
    Provenance {
        tool,
        managed_by: managed_by(meta).map(str::to_owned),
        warn_before_edit: tool.is_some(),
    }
}

/// One namespace as a project.
///
/// A service per workload with its containers, Services and Ingresses; every ConfigMap,
/// Secret, PVC, Service and Ingress with the workloads it belongs to; and every reference
/// between them.
#[must_use]
pub fn project(snapshot: &Snapshot) -> Project {
    let all = workloads(snapshot);
    let relations = relations_of(snapshot, &all);
    let attribution = attribution_of(snapshot, &all, &relations);
    let index = Index::new(snapshot, &attribution);
    let services: Vec<Service> = all
        .into_iter()
        .map(|workload| service(snapshot, &attribution, &index, workload))
        .collect();
    Project {
        namespace: snapshot.namespace.clone(),
        connections: connections(&snapshot.namespace, &services),
        services,
        resources: resources(snapshot, &attribution),
        relations,
    }
}

/// What every workload's service looks up, built once for all of them: the objects by name, and
/// the Services and Ingresses each workload is an owner of, in the snapshot's order.
///
/// An endpoint or route does not repeat which workloads own its object: that list is the
/// object's in `resources`, once. Copied into every owner's endpoints, it would grow with the
/// cube of the workloads when every Service selects every workload.
struct Index<'a> {
    objects: ByName<'a>,
    endpoints: BTreeMap<&'a WorkloadRef, Vec<&'a KubeService>>,
    routes: BTreeMap<&'a WorkloadRef, Vec<&'a Ingress>>,
}

impl<'a> Index<'a> {
    fn new(snapshot: &'a Snapshot, attribution: &'a NamespaceAttribution) -> Self {
        let mut endpoints: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for object in &snapshot.services {
            if let Some(owners) = attribution.services.get(name_of(&object.metadata)) {
                for owner in &owners.owners {
                    endpoints.entry(owner).or_default().push(object);
                }
            }
        }
        let mut routes: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for object in &snapshot.ingresses {
            if let Some(owners) = attribution.ingresses.get(name_of(&object.metadata)) {
                for owner in &owners.owners {
                    routes.entry(owner).or_default().push(object);
                }
            }
        }
        Self {
            objects: ByName::new(snapshot),
            endpoints,
            routes,
        }
    }
}

fn service(
    snapshot: &Snapshot,
    attribution: &NamespaceAttribution,
    index: &Index<'_>,
    workload: Workload<'_>,
) -> Service {
    // Ephemeral containers belong to running pods, not to the workload's template.
    let projected = workload
        .pod_spec()
        .into_iter()
        .flat_map(containers)
        .filter(|view| view.list != "ephemeralContainers")
        .filter_map(|view| {
            let name = view.name;
            Some(Container {
                name: name.to_owned(),
                image: view.image.map(str::to_owned),
                init: view.is_init(),
                env: env(workload, name)?,
                imported_env: env_from_in(snapshot, &index.objects, workload, name)?,
                mounts: mounts_in(&index.objects, attribution, workload, name)?,
                resources: view.resources.and_then(container_resources),
                probes: probes(view.probes),
                origin: workload.origin(view.path(workload)),
            })
        })
        .collect();
    let me = workload.reference();
    let endpoints = index
        .endpoints
        .get(&me)
        .into_iter()
        .flatten()
        .map(|&object| {
            let name = name_of(&object.metadata);
            Endpoint {
                name: name.to_owned(),
                type_: object.spec.as_ref().and_then(|spec| spec.type_.clone()),
                cluster_ip: object
                    .spec
                    .as_ref()
                    .and_then(|spec| spec.cluster_ip.clone()),
                ports: ports(object),
                provenance: provenance(&object.metadata),
                origin: Origin::object("Service", name),
            }
        })
        .collect();
    let routes = index
        .routes
        .get(&me)
        .into_iter()
        .flatten()
        .map(|&object| {
            let name = name_of(&object.metadata);
            let mut backends: Vec<String> = ingress_backends(object)
                .into_iter()
                .map(|(service, _path)| service.to_owned())
                .collect();
            backends.sort();
            backends.dedup();
            let mut tls_hosts: Vec<String> = object
                .spec
                .iter()
                .flat_map(|spec| spec.tls.iter().flatten())
                .flat_map(|tls| tls.hosts.iter().flatten().cloned())
                .collect();
            tls_hosts.sort();
            tls_hosts.dedup();
            Route {
                name: name.to_owned(),
                hostnames: object
                    .spec
                    .iter()
                    .flat_map(|spec| spec.rules.iter().flatten())
                    .filter_map(|rule| rule.host.clone())
                    .collect(),
                backends,
                class: ingress_class(object),
                rules: route_rules(object),
                tls_hosts,
                provenance: provenance(&object.metadata),
                origin: Origin::object("Ingress", name),
            }
        })
        .collect();
    let summary = summarise(snapshot, workload);
    let pods = crate::pods::pods_of(snapshot, workload);
    let mut service = Service {
        id: engawa_model::id::service(&snapshot.namespace, me.kind, &me.name),
        status: crate::health::status(&summary, &pods, snapshot.now),
        workload: summary,
        pods,
        containers: projected,
        endpoints,
        routes,
        data_store: None,
        diagnoses: Vec::new(),
    };
    service.data_store = data_store(&service);
    service
}

/// A container's requests and limits for CPU and memory, as written; `None` when it sets
/// neither.
fn container_resources(resources: &ResourceRequirements) -> Option<ContainerResources> {
    let pick = |map: Option<&BTreeMap<String, Quantity>>| {
        Quantities::new(
            map.and_then(|m| m.get("cpu")).map(|q| q.0.clone()),
            map.and_then(|m| m.get("memory")).map(|q| q.0.clone()),
        )
    };
    let out = ContainerResources {
        requests: pick(resources.requests.as_ref()),
        limits: pick(resources.limits.as_ref()),
    };
    (!out.requests.is_empty() || !out.limits.is_empty()).then_some(out)
}

/// An Ingress's class: `spec.ingressClassName`, else the deprecated `kubernetes.io/ingress.class`
/// annotation, which older manifests still use.
fn ingress_class(ingress: &Ingress) -> Option<String> {
    ingress
        .spec
        .as_ref()
        .and_then(|spec| spec.ingress_class_name.clone())
        .or_else(|| {
            ingress
                .metadata
                .annotations
                .as_ref()
                .and_then(|a| a.get("kubernetes.io/ingress.class"))
                .cloned()
        })
}

/// A container's liveness, readiness and startup probes; `None` when it sets none.
fn probes([liveness, readiness, startup]: [Option<&Probe>; 3]) -> Option<Probes> {
    let out = Probes {
        liveness: liveness.and_then(probe),
        readiness: readiness.and_then(probe),
        startup: startup.and_then(probe),
    };
    (!out.is_empty()).then_some(out)
}

/// What a probe checks; `None` for a probe with no handler, which Kubernetes rejects.
fn probe(probe: &Probe) -> Option<ProbeSummary> {
    let port = |port: &IntOrString| match port {
        IntOrString::Int(number) => number.to_string(),
        IntOrString::String(name) => name.clone(),
    };
    if let Some(http) = &probe.http_get {
        return Some(ProbeSummary {
            handler: ProbeHandler::Http,
            port: Some(port(&http.port)),
            path: http.path.clone(),
        });
    }
    if let Some(tcp) = &probe.tcp_socket {
        return Some(ProbeSummary {
            handler: ProbeHandler::Tcp,
            port: Some(port(&tcp.port)),
            path: None,
        });
    }
    if let Some(grpc) = &probe.grpc {
        return Some(ProbeSummary {
            handler: ProbeHandler::Grpc,
            port: Some(grpc.port.to_string()),
            path: None,
        });
    }
    probe.exec.as_ref().map(|_| ProbeSummary {
        handler: ProbeHandler::Exec,
        port: None,
        path: None,
    })
}

/// Every attributed resource, sorted by kind then name.
fn resources(snapshot: &Snapshot, attribution: &NamespaceAttribution) -> Vec<Resource> {
    let mut out = Vec::new();
    let mut push = |kind: ResourceKind, meta: &ObjectMeta, owners: &Attribution, keys| {
        let name = name_of(meta);
        out.push(Resource {
            id: engawa_model::id::resource(&snapshot.namespace, kind, name),
            kind,
            name: name.to_owned(),
            attribution: owners.clone(),
            provenance: provenance(meta),
            key_count: keys,
            origin: Origin::object(kind.as_str(), name),
        });
    };
    for object in &snapshot.config_maps {
        if let Some(owners) = attribution.config_maps.get(name_of(&object.metadata)) {
            let keys = key_count(object);
            push(
                ResourceKind::ConfigMap,
                &object.metadata,
                owners,
                Some(keys),
            );
        }
    }
    for object in &snapshot.secrets {
        let name = name_of(&object.metadata);
        if let Some(owners) = attribution.secrets.get(name) {
            let keys = snapshot.secret_key_counts.get(name).copied();
            push(ResourceKind::Secret, &object.metadata, owners, keys);
        }
    }
    for object in &snapshot.persistent_volume_claims {
        if let Some(owners) = attribution
            .persistent_volume_claims
            .get(name_of(&object.metadata))
        {
            push(
                ResourceKind::PersistentVolumeClaim,
                &object.metadata,
                owners,
                None,
            );
        }
    }
    for object in &snapshot.services {
        if let Some(owners) = attribution.services.get(name_of(&object.metadata)) {
            push(ResourceKind::Service, &object.metadata, owners, None);
        }
    }
    for object in &snapshot.ingresses {
        if let Some(owners) = attribution.ingresses.get(name_of(&object.metadata)) {
            push(ResourceKind::Ingress, &object.metadata, owners, None);
        }
    }
    out.sort_by(|a, b| (a.kind, &a.name).cmp(&(b.kind, &b.name)));
    out
}
