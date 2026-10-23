//! The whole namespace as a project.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Probe, ResourceRequirements, Service as KubeService};
use k8s_openapi::api::networking::v1::Ingress;
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use zeabur_adapters::provenance::{detect_tool, managed_by};
use zeabur_adapters::{Conventions, LABEL_SHARED_VARIABLES, error_pages, traefik};
use zeabur_model::{
    Attribution, Container, ContainerResources, Endpoint, NamespaceAttribution, Origin,
    ProbeHandler, ProbeSummary, Probes, Project, Provenance, Quantities, Resource, ResourceKind,
    Route, RouteMiddleware, Service, WorkloadRef,
};

use crate::attribution::{attribution_of, ingress_backends, relations_of};
use crate::connections::{connections, data_store};
use crate::env::{env, env_from_in, key_count};
use crate::mounts::mounts_in;
use crate::network::{ports, route_rules};
use crate::snapshot::{ByName, Snapshot, name_of};
use crate::workload::{Workload, containers, summarise_with, workloads};

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
/// A service per workload with its containers; every Kubernetes Service and Ingress that reaches
/// a workload, once, which the services name; and every ConfigMap, Secret, PVC, Service and
/// Ingress with the workloads it belongs to.
///
/// The references between workloads and resources ([`crate::relations`]) decide who owns what,
/// but are not part of the project: the same edges are in the services' endpoint and route
/// names, their containers' variables and mounts, and each resource's owners.
#[must_use]
pub fn project(snapshot: &Snapshot) -> Project {
    project_with(snapshot, &Conventions::ZEABUR)
}

/// [`project`] under the labels and field manager of `conventions`.
#[must_use]
pub fn project_with(snapshot: &Snapshot, conventions: &Conventions) -> Project {
    let all = workloads(snapshot);
    let attribution = attribution_of(snapshot, &all, &relations_of(snapshot, &all));
    let index = Index::new(snapshot, &attribution);
    let services: Vec<Service> = all
        .into_iter()
        .map(|workload| service(snapshot, &attribution, &index, workload, conventions))
        .collect();
    let owned = |owners: Option<&Attribution>| owners.is_some_and(|a| !a.owners.is_empty());
    let endpoints = snapshot
        .services
        .iter()
        .filter(|o| owned(attribution.services.get(name_of(&o.metadata))))
        .map(endpoint);
    let routes = snapshot
        .ingresses
        .iter()
        .filter(|o| owned(attribution.ingresses.get(name_of(&o.metadata))))
        .map(route);
    Project {
        namespace: snapshot.namespace.clone(),
        connections: connections(&snapshot.namespace, &services),
        services,
        endpoints: by_name(endpoints, |e| &e.name),
        routes: by_name(routes, |r| &r.name),
        resources: resources(snapshot, &attribution),
    }
}

/// `items` sorted by name, the first of each name kept.
fn by_name<T>(items: impl Iterator<Item = T>, name: impl Fn(&T) -> &String) -> Vec<T> {
    let mut out: Vec<T> = items.collect();
    out.sort_by(|a, b| name(a).cmp(name(b)));
    out.dedup_by(|a, b| name(a) == name(b));
    out
}

/// What every workload's service looks up, built once for all of them: the objects by name, and
/// the Services and Ingresses each workload is an owner of.
///
/// A service names its Services and Ingresses; their details are in the project once. Copied
/// into every owner's service, they would grow with the square of the workloads when every
/// Service selects every workload.
struct Index<'a> {
    objects: ByName<'a>,
    endpoints: BTreeMap<&'a WorkloadRef, Vec<&'a str>>,
    routes: BTreeMap<&'a WorkloadRef, Vec<&'a str>>,
}

impl<'a> Index<'a> {
    fn new(snapshot: &'a Snapshot, attribution: &'a NamespaceAttribution) -> Self {
        let mut endpoints: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for object in &snapshot.services {
            let name = name_of(&object.metadata);
            if let Some(owners) = attribution.services.get(name) {
                for owner in &owners.owners {
                    endpoints.entry(owner).or_default().push(name);
                }
            }
        }
        let mut routes: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for object in &snapshot.ingresses {
            let name = name_of(&object.metadata);
            if let Some(owners) = attribution.ingresses.get(name) {
                for owner in &owners.owners {
                    routes.entry(owner).or_default().push(name);
                }
            }
        }
        Self {
            objects: ByName::new(snapshot),
            endpoints,
            routes,
        }
    }

    /// The names of the Services or Ingresses in `by_owner` that `owner` is an owner of, sorted,
    /// each once.
    fn names(by_owner: &BTreeMap<&WorkloadRef, Vec<&str>>, owner: &WorkloadRef) -> Vec<String> {
        let mut out: Vec<String> = by_owner
            .get(owner)
            .into_iter()
            .flatten()
            .map(|&name| name.to_owned())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// A Kubernetes Service as an endpoint, as [`project`] lists it. It reads the Service alone, so
/// one endpoint serves every workload the Service selects.
#[must_use]
pub fn endpoint(object: &KubeService) -> Endpoint {
    let name = name_of(&object.metadata);
    Endpoint {
        name: name.to_owned(),
        type_: object.spec.as_ref().and_then(|spec| spec.type_.clone()),
        cluster_ip: object
            .spec
            .as_ref()
            .and_then(|spec| spec.cluster_ip.clone()),
        ports: ports(object),
        external_addresses: object
            .status
            .iter()
            .filter_map(|status| status.load_balancer.as_ref())
            .flat_map(|lb| lb.ingress.iter().flatten())
            .filter_map(|i| i.ip.clone().or_else(|| i.hostname.clone()))
            .collect(),
        provenance: provenance(&object.metadata),
        origin: Origin::object("Service", name),
    }
}

/// An Ingress as a route, with every one of its rules. It reads the Ingress alone, so one route
/// serves every workload it reaches.
fn route(object: &Ingress) -> Route {
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
    let middlewares = traefik::middlewares(&object.metadata);
    let hostnames: Vec<String> = object
        .spec
        .iter()
        .flat_map(|spec| spec.rules.iter().flatten())
        .filter_map(|rule| rule.host.clone())
        .collect();
    let mut redirects: Vec<RouteMiddleware> = hostnames
        .iter()
        .map(|host| RouteMiddleware {
            host: Some(host.clone()),
            middleware: traefik::redirect_name(name, host),
        })
        .filter(|r| middlewares.contains(&r.middleware))
        .collect();
    redirects.sort_by(|a, b| a.host.cmp(&b.host));
    redirects.dedup();
    let basic_auth = Some(traefik::basic_auth_name(name))
        .filter(|m| middlewares.contains(m))
        .map(|middleware| RouteMiddleware {
            host: None,
            middleware,
        });
    let error_pages = Some(traefik::error_pages_name(name))
        .filter(|m| middlewares.contains(m))
        .map(|middleware| RouteMiddleware {
            host: None,
            middleware,
        });
    Route {
        name: name.to_owned(),
        hostnames,
        backends,
        class: ingress_class(object),
        rules: route_rules(object),
        tls_hosts,
        redirects,
        basic_auth,
        error_pages,
        maintenance: error_pages::maintenance(&object.metadata)
            .unwrap_or_default()
            .into_keys()
            .collect(),
        provenance: provenance(&object.metadata),
        origin: Origin::object("Ingress", name),
    }
}

fn service(
    snapshot: &Snapshot,
    attribution: &NamespaceAttribution,
    index: &Index<'_>,
    workload: Workload<'_>,
    conventions: &Conventions,
) -> Service {
    // Ephemeral containers belong to running pods, not to the workload's template.
    let mut projected: Vec<Container> = workload
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
                imported_env: env_from_in(
                    snapshot,
                    &index.objects,
                    workload,
                    name,
                    &conventions.field_manager,
                )?,
                mounts: mounts_in(&index.objects, attribution, workload, name)?,
                resources: view.resources.and_then(container_resources),
                probes: probes(view.probes),
                origin: workload.origin(view.path(workload)),
            })
        })
        .collect();
    crate::restart::mark(snapshot, &index.objects, workload, &mut projected);
    let me = workload.reference();
    let summary = summarise_with(snapshot, workload, conventions);
    let pods = crate::pods::pods_of(snapshot, workload);
    let mut service = Service {
        id: zeabur_model::id::service(&snapshot.namespace, me.kind, &me.name),
        status: crate::health::status(&summary, &pods, snapshot.now),
        workload: summary,
        pods,
        containers: projected,
        endpoint_names: Index::names(&index.endpoints, &me),
        route_names: Index::names(&index.routes, &me),
        data_store: None,
        config_reload: workload
            .meta()
            .annotations
            .as_ref()
            .and_then(zeabur_model::ConfigReload::from_annotations),
        pod_security: workload.pod_spec().map(crate::pod_security::level),
        run_as: workload.pod_spec().and_then(crate::pod_security::run_as),
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
            id: zeabur_model::id::resource(&snapshot.namespace, kind, name),
            kind,
            name: name.to_owned(),
            attribution: owners.clone(),
            provenance: provenance(meta),
            key_count: keys,
            shared_variables: meta
                .labels
                .as_ref()
                .and_then(|l| l.get(LABEL_SHARED_VARIABLES))
                .is_some_and(|v| v == "true"),
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
