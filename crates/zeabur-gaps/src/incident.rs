//! Cluster incidents: one cause on the cluster's side, told once with everything it reaches.
//!
//! When the ingress controller has no ready replica, every public domain it serves stops
//! answering; when a node stops reporting, every pod on it is lost. Shown service by service,
//! that is many unrelated problems, each of which reads as the service's own. An incident names
//! the cause, its fault ([`Fault`]: a component, fixed by its owner, or the cluster, fixed by an
//! administrator) and its blast radius: the services, domains and projects it reaches.
//!
//! **Components.** A component is recognised by the labels its chart or manifest gives its
//! workload, the way Zeabur Catalog detects an installation:
//!
//! | Component | Workload labels |
//! |---|---|
//! | `ingress-controller` (Traefik) | `app.kubernetes.io/name: traefik` |
//! | `ingress-controller` (ingress-nginx) | `app.kubernetes.io/name: ingress-nginx`, `app.kubernetes.io/component: controller` |
//! | `cert-manager` | `app.kubernetes.io/name: cert-manager` or `webhook`, with `app.kubernetes.io/component: controller` or `webhook` and `app.kubernetes.io/instance` naming cert-manager |
//! | `cluster-dns` | `k8s-app: kube-dns` (`CoreDNS` as kubeadm and k3s deploy it; not verified against their manifests here) |
//! | `metrics-server` | `app.kubernetes.io/name: metrics-server` or `k8s-app: metrics-server` |
//!
//! A component is **down** when it wants replicas and none is ready (`readyReplicas`, or a
//! DaemonSet's `numberReady`, is 0) [k8s-deployment-status]. Fewer ready than wanted is not an
//! incident: the rest still serve.
//!
//! **Nodes.** A node whose `Ready` condition is `False` or `Unknown` runs nothing new, and the
//! node lifecycle controller evicts its pods after the toleration of `not-ready` or
//! `unreachable` runs out [k8s-node-condition] [k8s-taint-based-evictions].
//!
//! **Blast radius.** What each cause reaches, as far as the reader could see:
//!
//! - an ingress controller: the Ingresses of its class (`spec.ingressClassName` against the
//!   IngressClass's `spec.controller`; one without a class counts for every controller), their
//!   hosts and the services they route to;
//! - cert-manager: the Ingresses asking it for a certificate (`cert-manager.io/issuer` or
//!   `cert-manager.io/cluster-issuer`) [cert-manager-ingress], whose certificates are neither
//!   issued nor renewed;
//! - cluster DNS: the services that call another by name;
//! - metrics-server: no service; usage figures and autoscaling stop;
//! - a node: the services with a pod on it.
//!
//! **Symptoms.** A service's diagnosis is linked to an incident ([`link`]) only where the cause
//! explains it: a diagnosis about pods on a node that is not ready, a pod the scheduler cannot
//! place because the nodes carry the `not-ready` or `unreachable` taint, or a route that is not
//! accepted while its controller is down. Its fault becomes the incident's, so the service's
//! problem reads as the cluster's.
//!
//! [k8s-deployment-status]: https://kubernetes.io/docs/concepts/workloads/controllers/deployment/#deployment-status
//! [k8s-node-condition]: https://kubernetes.io/docs/reference/node/node-status/#condition
//! [k8s-taint-based-evictions]: https://kubernetes.io/docs/concepts/scheduling-eviction/taint-and-toleration/#taint-based-evictions
//! [cert-manager-ingress]: https://cert-manager.io/docs/usage/ingress/

use std::collections::{BTreeMap, BTreeSet};

use k8s_openapi::api::apps::v1::{DaemonSet, Deployment};
use k8s_openapi::api::core::v1::Node;
use k8s_openapi::api::networking::v1::Ingress;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use zeabur_graph::Snapshot;
use zeabur_model::{
    AffectedService, DiagnosisKind, Fault, FaultOrigin, Fixer, Incident, IncidentKind, Message,
    Origin, Project, Service, WorkloadKind,
};

use crate::IngressClassFact;

/// What a component does for the services around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentKind {
    /// Brings public traffic to the services an Ingress routes to.
    IngressController,
    /// Issues and renews the certificates Ingresses ask it for.
    CertManager,
    /// Answers the cluster's DNS: how services find each other by name.
    ClusterDns,
    /// Serves the resource metrics API: usage figures and autoscaling.
    MetricsServer,
}

impl ComponentKind {
    /// The name a [`Fault`] gives it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::IngressController => "ingress-controller",
            Self::CertManager => "cert-manager",
            Self::ClusterDns => "cluster-dns",
            Self::MetricsServer => "metrics-server",
        }
    }

    const fn sentence(self) -> &'static str {
        match self {
            Self::IngressController => "ingressController",
            Self::CertManager => "certManager",
            Self::ClusterDns => "clusterDns",
            Self::MetricsServer => "metricsServer",
        }
    }
}

/// One component's workload and whether it serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    /// What it does.
    pub kind: ComponentKind,
    /// Its workload, as a complete reference.
    pub workload: Origin,
    /// Replicas it wants.
    pub desired: i32,
    /// Replicas ready.
    pub ready: i32,
    /// The `spec.controller` values of the `IngressClasses` it serves (an ingress controller).
    pub ingress_controllers: Vec<String>,
    /// When it stopped being available, RFC 3339, when the workload says.
    pub since: Option<String>,
}

impl Component {
    /// It wants replicas and none is ready.
    #[must_use]
    pub const fn down(&self) -> bool {
        self.desired > 0 && self.ready == 0
    }
}

/// A node, and whether it is ready.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeState {
    /// Its name.
    pub name: String,
    /// `Ready` is `True`.
    pub ready: bool,
    /// The `Ready` condition's status (`False`, `Unknown`), reason and when it changed.
    pub status: String,
    /// The condition's reason, such as `KubeletNotReady` or `NodeStatusUnknown`.
    pub reason: Option<String>,
    /// When the condition last changed, RFC 3339.
    pub since: Option<String>,
}

fn label<'a>(meta: &'a ObjectMeta, key: &str) -> Option<&'a str> {
    meta.labels.as_ref()?.get(key).map(String::as_str)
}

/// What the labels of a workload say it is: its kind and the ingress controllers it serves.
fn recognise(meta: &ObjectMeta) -> Option<(ComponentKind, Vec<String>)> {
    let name = label(meta, "app.kubernetes.io/name");
    let component = label(meta, "app.kubernetes.io/component");
    let instance = label(meta, "app.kubernetes.io/instance").unwrap_or_default();
    match (name, component) {
        (Some("traefik"), _) => Some((
            ComponentKind::IngressController,
            vec!["traefik.io/ingress-controller".to_owned()],
        )),
        (Some("ingress-nginx"), Some("controller")) => Some((
            ComponentKind::IngressController,
            vec!["k8s.io/ingress-nginx".to_owned()],
        )),
        (Some("cert-manager"), Some("controller")) | (Some("webhook"), Some("webhook"))
            if instance.contains("cert-manager") =>
        {
            Some((ComponentKind::CertManager, Vec::new()))
        }
        (Some("metrics-server"), _) => Some((ComponentKind::MetricsServer, Vec::new())),
        _ => match label(meta, "k8s-app") {
            Some("kube-dns") => Some((ComponentKind::ClusterDns, Vec::new())),
            Some("metrics-server") => Some((ComponentKind::MetricsServer, Vec::new())),
            _ => None,
        },
    }
}

/// The components among `deployments` and `daemon_sets` (of every namespace), in that order.
#[must_use]
pub fn components(deployments: &[Deployment], daemon_sets: &[DaemonSet]) -> Vec<Component> {
    let mut out = Vec::new();
    for deployment in deployments {
        let Some((kind, ingress_controllers)) = recognise(&deployment.metadata) else {
            continue;
        };
        let status = deployment.status.as_ref();
        let since = status
            .into_iter()
            .flat_map(|s| s.conditions.iter().flatten())
            .find(|c| c.type_ == "Available" && c.status != "True")
            .and_then(|c| c.last_transition_time.as_ref())
            .map(|t| t.0.to_string());
        out.push(Component {
            kind,
            workload: complete("apps/v1", "Deployment", &deployment.metadata),
            desired: deployment
                .spec
                .as_ref()
                .and_then(|s| s.replicas)
                .unwrap_or(1),
            ready: status.and_then(|s| s.ready_replicas).unwrap_or(0),
            ingress_controllers,
            since,
        });
    }
    for daemon_set in daemon_sets {
        let Some((kind, ingress_controllers)) = recognise(&daemon_set.metadata) else {
            continue;
        };
        let status = daemon_set.status.as_ref();
        out.push(Component {
            kind,
            workload: complete("apps/v1", "DaemonSet", &daemon_set.metadata),
            desired: status.map_or(0, |s| s.desired_number_scheduled),
            ready: status.map_or(0, |s| s.number_ready),
            ingress_controllers,
            since: None,
        });
    }
    out
}

fn complete(api_version: &str, kind: &str, meta: &ObjectMeta) -> Origin {
    Origin::complete(
        api_version,
        kind,
        meta.namespace.as_deref(),
        meta.name.as_deref().unwrap_or_default(),
    )
}

/// The state of each of `nodes`.
#[must_use]
pub fn node_states(nodes: &[Node]) -> Vec<NodeState> {
    nodes
        .iter()
        .map(|node| {
            let ready = node
                .status
                .as_ref()
                .and_then(|s| s.conditions.as_ref())
                .and_then(|c| c.iter().find(|c| c.type_ == "Ready"));
            NodeState {
                name: node.metadata.name.clone().unwrap_or_default(),
                ready: ready.is_some_and(|c| c.status == "True"),
                status: ready.map_or_else(|| "Unknown".to_owned(), |c| c.status.clone()),
                reason: ready.and_then(|c| c.reason.clone()),
                since: ready
                    .and_then(|c| c.last_transition_time.as_ref())
                    .map(|t| t.0.to_string()),
            }
        })
        .collect()
}

/// What an incident's blast radius is read from: one project, as the person read it.
#[derive(Debug, Clone, Copy)]
pub struct Reach<'a> {
    /// The project, its services' diagnoses filled in.
    pub project: &'a Project,
    /// The namespace as read: its Ingresses and pods.
    pub snapshot: &'a Snapshot,
}

/// What every project the person could read adds to the blast radius when only the cluster's
/// Ingresses and pods are known (the cluster-wide answer): the domains and projects, no services.
#[derive(Debug, Clone, Default)]
pub struct Wide<'a> {
    /// Every Ingress the person could list.
    pub ingresses: &'a [Ingress],
    /// The namespace and node of every pod the person could list.
    pub pods: &'a [(String, String)],
}

/// The incidents `components` and `nodes` make, with what they reach in `projects` and `wide`.
///
/// `classes` are the `IngressClasses`, to tell which controller an Ingress belongs to. Incidents
/// are sorted by id; a component or node that serves gives none.
#[must_use]
pub fn incidents(
    components: &[Component],
    nodes: &[NodeState],
    classes: &[IngressClassFact],
    projects: &[Reach<'_>],
    wide: &Wide<'_>,
) -> Vec<Incident> {
    let mut out = Vec::new();
    for component in components.iter().filter(|c| c.down()) {
        out.push(component_incident(component, classes, projects, wide));
    }
    for node in nodes.iter().filter(|n| !n.ready) {
        out.push(node_incident(node, projects, wide));
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| {
        if a.id == b.id {
            b.evidence.append(&mut a.evidence);
            true
        } else {
            false
        }
    });
    out
}

/// The controller an Ingress's class names; `None` without a class or for an unknown one.
fn controller_of<'a>(ingress: &Ingress, classes: &'a [IngressClassFact]) -> Option<&'a str> {
    let class = ingress
        .spec
        .as_ref()
        .and_then(|s| s.ingress_class_name.as_deref())
        .or_else(|| {
            ingress
                .metadata
                .annotations
                .as_ref()?
                .get("kubernetes.io/ingress.class")
                .map(String::as_str)
        })?;
    classes
        .iter()
        .find(|c| c.name == class)
        .map(|c| c.controller.as_str())
}

/// Whether `component` (an ingress controller) serves `ingress`.
fn serves(component: &Component, ingress: &Ingress, classes: &[IngressClassFact]) -> bool {
    controller_of(ingress, classes).is_none_or(|controller| {
        component
            .ingress_controllers
            .iter()
            .any(|c| c == controller)
    })
}

fn asks_cert_manager(ingress: &Ingress) -> bool {
    ingress.metadata.annotations.as_ref().is_some_and(|a| {
        a.contains_key("cert-manager.io/issuer") || a.contains_key("cert-manager.io/cluster-issuer")
    })
}

/// The hosts of `ingress` that matter for `kind`: every rule's for a controller, the TLS hosts
/// for cert-manager.
fn hosts(ingress: &Ingress, kind: ComponentKind) -> Vec<String> {
    let spec = ingress.spec.as_ref();
    match kind {
        ComponentKind::CertManager => spec
            .into_iter()
            .flat_map(|s| s.tls.iter().flatten())
            .flat_map(|tls| tls.hosts.iter().flatten().cloned())
            .collect(),
        ComponentKind::IngressController
        | ComponentKind::ClusterDns
        | ComponentKind::MetricsServer => spec
            .into_iter()
            .flat_map(|s| s.rules.iter().flatten())
            .filter_map(|rule| rule.host.clone())
            .collect(),
    }
}

/// Whether `component` reaches `ingress`.
fn reaches(component: &Component, ingress: &Ingress, classes: &[IngressClassFact]) -> bool {
    match component.kind {
        ComponentKind::IngressController => serves(component, ingress, classes),
        ComponentKind::CertManager => asks_cert_manager(ingress),
        ComponentKind::ClusterDns | ComponentKind::MetricsServer => false,
    }
}

fn affected(project: &Project, service: &Service, symptoms: Vec<DiagnosisKind>) -> AffectedService {
    AffectedService {
        id: service.id.clone(),
        project: project.namespace.clone(),
        kind: service.workload.kind,
        name: service.workload.name.clone(),
        symptoms,
    }
}

fn component_incident(
    component: &Component,
    classes: &[IngressClassFact],
    projects: &[Reach<'_>],
    wide: &Wide<'_>,
) -> Incident {
    let mut services = BTreeSet::new();
    let mut domains = BTreeSet::new();
    let mut namespaces = BTreeSet::new();
    for reach in projects {
        let project = reach.project;
        let mut routed: BTreeSet<&str> = BTreeSet::new();
        for ingress in &reach.snapshot.ingresses {
            if !reaches(component, ingress, classes) {
                continue;
            }
            domains.extend(hosts(ingress, component.kind));
            if let Some(name) = ingress.metadata.name.as_deref() {
                routed.insert(name);
            }
        }
        let callers: BTreeSet<(WorkloadKind, &str)> = project
            .connections
            .iter()
            .map(|c| (c.from.kind, c.from.name.as_str()))
            .collect();
        for service in &project.services {
            let reached = match component.kind {
                ComponentKind::IngressController | ComponentKind::CertManager => service
                    .route_names
                    .iter()
                    .any(|r| routed.contains(r.as_str())),
                ComponentKind::ClusterDns => {
                    callers.contains(&(service.workload.kind, service.workload.name.as_str()))
                }
                ComponentKind::MetricsServer => false,
            };
            if reached {
                let symptoms = service
                    .diagnoses
                    .iter()
                    .filter(|d| explains_component(component.kind, d))
                    .map(|d| d.kind)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                services.insert(affected(project, service, symptoms));
                namespaces.insert(project.namespace.clone());
            }
        }
    }
    for ingress in wide.ingresses {
        if reaches(component, ingress, classes) {
            let hosts = hosts(ingress, component.kind);
            if !hosts.is_empty()
                && let Some(namespace) = &ingress.metadata.namespace
            {
                namespaces.insert(namespace.clone());
            }
            domains.extend(hosts);
        }
    }
    let workload = &component.workload;
    let sentence = |part: &str| {
        Message::new(&format!(
            "incident.componentDown.{part}.{}",
            component.kind.sentence()
        ))
        .with("workload", &workload.name)
        .with("namespace", workload.namespace.clone().unwrap_or_default())
        .with("desired", component.desired.to_string())
        .with("domains", domains.len().to_string())
        .with("services", services.len().to_string())
    };
    Incident {
        id: format!("incidents/component/{}", component.kind.name()),
        kind: IncidentKind::ComponentDown,
        cause: sentence("cause"),
        why: sentence("why"),
        fix: sentence("fix"),
        fault: Fault::new(FaultOrigin::Component, Fixer::ComponentOwner).of(component.kind.name()),
        services: services.into_iter().collect(),
        domains: domains.into_iter().collect(),
        projects: namespaces.into_iter().collect(),
        evidence: vec![
            workload
                .clone()
                .with_field("status.readyReplicas".to_owned())
                .with_value(component.ready.to_string()),
        ],
        since: component.since.clone(),
    }
}

/// Whether a component being down explains `diagnosis`: a route its controller does not accept.
fn explains_component(kind: ComponentKind, diagnosis: &zeabur_model::Diagnosis) -> bool {
    kind == ComponentKind::IngressController
        && diagnosis.kind == DiagnosisKind::Unreachable
        && diagnosis.cause.key.ends_with(".routeNotAccepted")
}

/// Whether a node that is not ready explains `diagnosis` of a service with pods `on_node` there.
fn explains_node(diagnosis: &zeabur_model::Diagnosis, on_node: &BTreeSet<&str>) -> bool {
    if diagnosis.kind == DiagnosisKind::Unschedulable {
        let message = diagnosis
            .why
            .params
            .get("message")
            .map_or("", String::as_str);
        return message.contains("node.kubernetes.io/not-ready")
            || message.contains("node.kubernetes.io/unreachable");
    }
    let pods_there = diagnosis
        .pods
        .iter()
        .any(|pod| on_node.contains(pod.as_str()));
    pods_there
        || (diagnosis.kind == DiagnosisKind::Unreachable
            && diagnosis.cause.key.ends_with(".noReadyEndpoints")
            && !on_node.is_empty())
}

fn node_of(pod: &k8s_openapi::api::core::v1::Pod) -> Option<&str> {
    pod.spec.as_ref()?.node_name.as_deref()
}

fn node_incident(node: &NodeState, projects: &[Reach<'_>], wide: &Wide<'_>) -> Incident {
    let mut services = BTreeSet::new();
    let mut namespaces = BTreeSet::new();
    for reach in projects {
        let on_node: BTreeSet<&str> = reach
            .snapshot
            .pods
            .iter()
            .filter(|pod| node_of(pod) == Some(node.name.as_str()))
            .filter_map(|pod| pod.metadata.name.as_deref())
            .collect();
        for service in &reach.project.services {
            let mine: BTreeSet<&str> = service
                .pods
                .iter()
                .map(|p| p.name.as_str())
                .filter(|name| on_node.contains(name))
                .collect();
            let symptoms: Vec<DiagnosisKind> = service
                .diagnoses
                .iter()
                .filter(|d| explains_node(d, &mine))
                .map(|d| d.kind)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            if !mine.is_empty() || !symptoms.is_empty() {
                services.insert(affected(reach.project, service, symptoms));
                namespaces.insert(reach.project.namespace.clone());
            }
        }
    }
    let mut pods = 0_usize;
    for (namespace, on) in wide.pods {
        if *on == node.name {
            namespaces.insert(namespace.clone());
            pods += 1;
        }
    }
    let pods = pods.max(services.len());
    let sentence = |part: &str| {
        let message = Message::new(&format!("incident.nodeNotReady.{part}"))
            .with("node", &node.name)
            .with("status", &node.status)
            .with("services", services.len().to_string())
            .with("pods", pods.to_string());
        match &node.reason {
            Some(reason) => message.with("reason", reason),
            None => message,
        }
    };
    Incident {
        id: format!("incidents/node/{}", node.name),
        kind: IncidentKind::NodeNotReady,
        cause: sentence("cause"),
        why: sentence("why"),
        fix: sentence("fix"),
        fault: Fault::new(FaultOrigin::Cluster, Fixer::ClusterAdmin)
            .of(format!("node/{}", node.name)),
        services: services.into_iter().collect(),
        domains: Vec::new(),
        projects: namespaces.into_iter().collect(),
        evidence: vec![
            Origin::complete("v1", "Node", None, &node.name)
                .with_field("status.conditions[type=Ready]".to_owned())
                .with_value(node.status.clone()),
        ],
        since: node.since.clone(),
    }
}

/// Links the diagnoses of `service` that `incidents` explain: each gets the incident's id and
/// fault.
pub fn link(incidents: &[Incident], service: &mut Service) {
    let explained: BTreeMap<DiagnosisKind, &Incident> = incidents
        .iter()
        .flat_map(|incident| {
            incident
                .services
                .iter()
                .filter(|s| s.id == service.id)
                .flat_map(move |s| s.symptoms.iter().map(move |kind| (*kind, incident)))
        })
        .collect();
    for diagnosis in &mut service.diagnoses {
        if let Some(incident) = explained.get(&diagnosis.kind) {
            diagnosis.incident = Some(incident.id.clone());
            diagnosis.fault = incident.fault.clone();
        }
    }
}

/// [`link`] for every service of `project`.
pub fn link_project(incidents: &[Incident], project: &mut Project) {
    for service in &mut project.services {
        link(incidents, service);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn deployment(value: serde_json::Value) -> Deployment {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_component_is_recognised_by_its_labels_and_down_with_no_ready_replica() {
        let traefik = deployment(json!({
            "metadata": {"name": "traefik", "namespace": "kube-system",
                "labels": {"app.kubernetes.io/name": "traefik"}},
            "spec": {"replicas": 1, "selector": {}, "template": {}},
            "status": {"replicas": 1}
        }));
        let other = deployment(json!({
            "metadata": {"name": "webhook", "namespace": "x",
                "labels": {"app.kubernetes.io/name": "webhook", "app.kubernetes.io/component": "webhook"}},
            "spec": {"selector": {}, "template": {}}
        }));
        let found = components(&[traefik, other], &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ComponentKind::IngressController);
        assert!(found[0].down());
    }

    fn shop() -> Snapshot {
        Snapshot {
            namespace: "shop".to_owned(),
            deployments: vec![deployment(json!({
                "metadata": {"name": "web", "namespace": "shop"},
                "spec": {"selector": {"matchLabels": {"app": "web"}},
                    "template": {"metadata": {"labels": {"app": "web"}},
                        "spec": {"containers": [{"name": "web", "image": "nginx:1.29"}]}}}
            }))],
            pods: vec![
                serde_json::from_value(json!({
                    "metadata": {"name": "web-1", "namespace": "shop", "labels": {"app": "web"}},
                    "spec": {"nodeName": "node-b", "containers": [{"name": "web", "image": "nginx:1.29"}]}
                }))
                .unwrap(),
            ],
            services: vec![
                serde_json::from_value(json!({
                    "metadata": {"name": "web", "namespace": "shop"},
                    "spec": {"selector": {"app": "web"}, "ports": [{"port": 80}]}
                }))
                .unwrap(),
            ],
            ingresses: vec![
                serde_json::from_value(json!({
                    "metadata": {"name": "web", "namespace": "shop",
                        "annotations": {"cert-manager.io/issuer": "letsencrypt"}},
                    "spec": {"tls": [{"hosts": ["secure.example.test"]}],
                        "rules": [{"host": "shop.example.test", "http": {"paths": [{"path": "/",
                            "pathType": "Prefix",
                            "backend": {"service": {"name": "web", "port": {"number": 80}}}}]}}]}
                }))
                .unwrap(),
            ],
            ..Snapshot::default()
        }
    }

    fn down(kind: ComponentKind) -> Component {
        Component {
            kind,
            workload: Origin::complete("apps/v1", "Deployment", Some("system"), "x"),
            desired: 1,
            ready: 0,
            ingress_controllers: vec!["k8s.io/ingress-nginx".to_owned()],
            since: None,
        }
    }

    #[test]
    fn each_cause_reaches_what_depends_on_it_and_links_only_what_it_explains() {
        let snapshot = shop();
        let mut project = zeabur_graph::project(&snapshot);
        let crash = |pods: Vec<String>| zeabur_model::Diagnosis {
            kind: DiagnosisKind::CrashLoop,
            container: "web".to_owned(),
            pods,
            cause: Message::new("diagnosis.crashLoop.cause.exitCode"),
            why: Message::new("diagnosis.crashLoop.why"),
            fix: Message::new("diagnosis.crashLoop.fix"),
            evidence: Vec::new(),
            last_seen: None,
            fault: Fault::new(FaultOrigin::Code, Fixer::You),
            incident: None,
        };
        project.services[0].diagnoses = vec![crash(vec!["web-1".to_owned()])];
        let nodes = [NodeState {
            name: "node-b".to_owned(),
            ready: false,
            status: "False".to_owned(),
            reason: Some("KubeletNotReady".to_owned()),
            since: None,
        }];
        let found = incidents(
            &[
                down(ComponentKind::IngressController),
                down(ComponentKind::CertManager),
                down(ComponentKind::MetricsServer),
            ],
            &nodes,
            &[],
            &[Reach {
                project: &project,
                snapshot: &snapshot,
            }],
            &Wide::default(),
        );
        let ids: Vec<&str> = found.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "incidents/component/cert-manager",
                "incidents/component/ingress-controller",
                "incidents/component/metrics-server",
                "incidents/node/node-b",
            ]
        );
        assert_eq!(found[0].domains, ["secure.example.test"]);
        assert_eq!(found[1].domains, ["shop.example.test"]);
        assert_eq!(found[1].services.len(), 1);
        assert_eq!(found[1].services[0].symptoms, []);
        assert_eq!(found[2].services, []);
        assert_eq!(found[3].services[0].symptoms, [DiagnosisKind::CrashLoop]);

        link_project(&found, &mut project);
        let diagnosis = &project.services[0].diagnoses[0];
        assert_eq!(diagnosis.incident.as_deref(), Some("incidents/node/node-b"));
        assert_eq!(diagnosis.fault.fixer, Fixer::ClusterAdmin);
    }

    #[test]
    fn an_ingress_of_another_controllers_class_is_not_reached() {
        let mut snapshot = shop();
        if let Some(spec) = snapshot.ingresses[0].spec.as_mut() {
            spec.ingress_class_name = Some("traefik".to_owned());
        }
        let project = zeabur_graph::project(&snapshot);
        let classes = [IngressClassFact {
            name: "traefik".to_owned(),
            controller: "traefik.io/ingress-controller".to_owned(),
        }];
        let found = incidents(
            &[down(ComponentKind::IngressController)],
            &[],
            &classes,
            &[Reach {
                project: &project,
                snapshot: &snapshot,
            }],
            &Wide::default(),
        );
        assert_eq!(found[0].domains, Vec::<String>::new());
        assert_eq!(found[0].services, []);
    }

    #[test]
    fn a_node_is_ready_only_when_its_condition_says_true() {
        let nodes: Vec<Node> = serde_json::from_value(json!([
            {"metadata": {"name": "a"}, "status": {"conditions": [{"type": "Ready", "status": "True"}]}},
            {"metadata": {"name": "b"}, "status": {"conditions": [{"type": "Ready", "status": "Unknown", "reason": "NodeStatusUnknown"}]}},
            {"metadata": {"name": "c"}}
        ]))
        .unwrap();
        let states = node_states(&nodes);
        assert_eq!(
            states.iter().map(|s| s.ready).collect::<Vec<_>>(),
            [true, false, false]
        );
        assert_eq!(states[1].reason.as_deref(), Some("NodeStatusUnknown"));
    }
}
