//! NetworkPolicy: who may reach each service of a project, and where each may go, from the
//! project's `networking.k8s.io/v1` `NetworkPolicy` objects [k8s-netpol].
//!
//! # What is evaluated
//!
//! The rules of the Kubernetes documentation [k8s-netpol], for traffic between pods:
//!
//! - A pod is isolated for ingress when a policy of its namespace selects it (`podSelector`) and
//!   lists `Ingress` in `policyTypes`; for egress, `Egress`. A policy that names no `policyTypes`
//!   has `Ingress`, and `Egress` too when it has egress rules.
//! - A pod that no policy isolates accepts (or sends) everything. An isolated pod accepts what the
//!   union of its policies' rules allows; a rule without `from` (`to`) allows every peer, a rule
//!   without `ports` every port.
//! - A peer with only `podSelector` means pods of the policy's namespace; with only
//!   `namespaceSelector`, every pod of the matching namespaces; with both, the matching pods of the
//!   matching namespaces. `ipBlock` addresses lie outside the cluster's pods and are read as
//!   "outside".
//! - A connection from one service to another needs both: the caller's egress and the callee's
//!   ingress.
//!
//! Ports are not resolved: a peer a rule admits on some ports only is [`Access::Limited`], never
//! guessed allowed or blocked for one port.
//!
//! # What is assumed
//!
//! - Namespaces carry the label `kubernetes.io/metadata.name` set to their name, which the API
//!   server has added to every namespace since Kubernetes 1.21 [k8s-ns-label]; other namespace
//!   labels are not read, so a `namespaceSelector` on another label is [`Access::Limited`].
//! - The ingress controller is k3s's packaged Traefik: namespace `kube-system`, pods labelled
//!   `app.kubernetes.io/name: traefik` [k3s-traefik].
//! - Cluster DNS runs in `kube-system`, pods labelled `k8s-app: kube-dns`, port 53 [k8s-dns].
//!
//! # What is not known
//!
//! Whether the policies are enforced at all: that is the network plugin's job, and a plugin
//! without NetworkPolicy support accepts the objects and enforces nothing [k8s-netpol]. k3s
//! enforces them with its embedded controller unless it was started with
//! `--disable-network-policy` [k3s-netpol]. Clients say so wherever they show reachability.
//!
//! [k8s-netpol]: https://kubernetes.io/docs/concepts/services-networking/network-policies/
//! [k8s-ns-label]: https://kubernetes.io/docs/reference/labels-annotations-taints/#kubernetes-io-metadata-name
//! [k3s-traefik]: https://docs.k3s.io/networking/networking-services#traefik-ingress-controller
//! [k3s-netpol]: https://docs.k3s.io/networking/basic-network-options#embedded-network-policy-controller
//! [k8s-dns]: https://kubernetes.io/docs/concepts/services-networking/dns-pod-service/

use std::collections::BTreeMap;

use k8s_openapi::api::networking::v1::{NetworkPolicy, NetworkPolicyPeer, NetworkPolicyPort};
use serde::Serialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use zeabur_adapters::selector::{is_empty, matches};
use zeabur_kernel::isolation::{PROJECT_POLICY, project_policy};
use zeabur_kube::DynamicObject;
use zeabur_model::{Citation, Fix, FixKind, Gap, Message, Origin, Severity, WorkloadKind};

use crate::{Finding, Module, Read};

/// The module.
pub static MODULE: Module = Module {
    id: "network-policy",
    capability: "network-policy",
    reads: &[NETWORK_POLICIES],
    http: &[],
};

/// `networking.k8s.io/v1` `NetworkPolicy`, in the project's namespace.
pub const NETWORK_POLICIES: Read = Read {
    group: "networking.k8s.io",
    version: "v1",
    resource: "networkpolicies",
    kind: "NetworkPolicy",
};

/// The namespace label the API server sets to the namespace's name.
pub const NAMESPACE_NAME_LABEL: &str = "kubernetes.io/metadata.name";

/// One service of the project, as reachability sees it: its pods' labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workload {
    /// Its id, `projects/<p>/services/<kind>/<name>`.
    pub id: String,
    /// Its pod template's labels.
    pub labels: BTreeMap<String, String>,
}

/// How far a peer may reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NetworkPolicyAccess)]
pub enum Access {
    /// Every port.
    Allowed,
    /// Some ports, or only when labels Zeabur cannot read match.
    Limited,
    /// Nothing.
    Blocked,
}

impl Access {
    /// The wider of two.
    #[must_use]
    pub fn or(self, other: Self) -> Self {
        self.min(other)
    }

    /// Both must allow.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        self.max(other)
    }
}

/// One service's reachability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NetworkPolicyService)]
pub struct ServiceReachability {
    /// The service's id.
    pub service: String,
    /// The policies that select its pods, by name.
    pub selected_by: Vec<String>,
    /// A policy limits what reaches it.
    pub ingress_isolated: bool,
    /// A policy limits where it goes.
    pub egress_isolated: bool,
    /// Services of the project that may reach it, with how far.
    pub from_services: Vec<Peer>,
    /// The ingress controller (k3s's Traefik).
    pub from_ingress_controller: Access,
    /// Pods of other namespaces, whatever their labels.
    pub from_other_namespaces: Access,
    /// Cluster DNS (port 53 in `kube-system`).
    pub to_dns: Access,
    /// Addresses outside the cluster's pods.
    pub to_outside: Access,
}

/// A service and how far it may reach another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NetworkPolicyPeer)]
pub struct Peer {
    /// The service's id.
    pub service: String,
    /// How far.
    pub access: Access,
    /// The policies, by name, that decide it: those whose rules admit it, on its way out or on the
    /// service's way in; when it is blocked, those that isolate the blocking side and admit
    /// nothing from it. Empty when no policy isolates either side.
    pub via: Vec<String>,
}

/// A connection the project shows (inferred from configuration), and whether the policies let it
/// through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NetworkPolicyConnection)]
pub struct Connection {
    /// The caller's id.
    pub from: String,
    /// The callee's id.
    pub to: String,
    /// The caller's egress and the callee's ingress together.
    pub access: Access,
    /// Where it is stopped, when [`Access::Blocked`]: `egress`, `ingress` or both.
    pub blocked_at: Vec<String>,
}

/// The project's reachability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NetworkPolicyReachability)]
pub struct Reachability {
    /// The project's policies, by name.
    pub policies: Vec<String>,
    /// The project has its own policy, [`PROJECT_POLICY`].
    pub project_policy: bool,
    /// The other namespaces a policy lets in whole, by `kubernetes.io/metadata.name`.
    pub allowed_namespaces: Vec<AllowedNamespace>,
    /// Every service, in the order given.
    pub services: Vec<ServiceReachability>,
    /// The connections given, in that order.
    pub connections: Vec<Connection>,
}

/// A namespace that a policy lets reach every pod of the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NetworkPolicyAllowedNamespace)]
pub struct AllowedNamespace {
    /// The namespace.
    pub namespace: String,
    /// The policy that lets it in.
    pub policy: String,
}

/// The namespaces `policy` lets reach every pod of `namespace`, on every port. Only a policy of
/// this shape counts, as a person writes it with kubectl:
///
/// ```yaml
/// spec:
///   podSelector: {}
///   policyTypes: [Ingress]
///   ingress:
///     - from:
///         - namespaceSelector:
///             matchLabels: {kubernetes.io/metadata.name: <namespace>}
/// ```
fn allowed_namespaces(policy: &NetworkPolicy, namespace: &str) -> Vec<String> {
    let Some(spec) = &policy.spec else {
        return Vec::new();
    };
    let rules = spec.ingress.as_deref().unwrap_or_default();
    if !is_empty(&spec.pod_selector.clone().unwrap_or_default())
        || types(policy) != (true, false)
        || rules.is_empty()
    {
        return Vec::new();
    }
    let mut found = Vec::new();
    for rule in rules {
        if rule.ports.as_ref().is_some_and(|p| !p.is_empty()) {
            return Vec::new();
        }
        let peers = rule.from.as_deref().unwrap_or_default();
        if peers.is_empty() {
            return Vec::new();
        }
        for peer in peers {
            let name = peer
                .namespace_selector
                .as_ref()
                .filter(|_| peer.ip_block.is_none())
                .filter(|_| peer.pod_selector.as_ref().is_none_or(is_empty))
                .filter(|s| s.match_expressions.as_ref().is_none_or(Vec::is_empty))
                .and_then(|s| s.match_labels.as_ref())
                .filter(|labels| labels.len() == 1)
                .and_then(|labels| labels.get(NAMESPACE_NAME_LABEL));
            match name {
                Some(name) if name != namespace => found.push(name.clone()),
                _ => return Vec::new(),
            }
        }
    }
    found
}

/// What reachability is computed from.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    /// The project's namespace.
    pub namespace: &'a str,
    /// Its services.
    pub workloads: &'a [Workload],
    /// Its NetworkPolicy objects.
    pub policies: &'a [DynamicObject],
    /// The connections to check, `(from, to)` by service id.
    pub connections: &'a [(String, String)],
}

/// A pod somewhere, as a rule's peer: its namespace's name and, where known, its labels.
struct Pod<'a> {
    namespace: &'a str,
    /// `None`: any labels (unknown).
    labels: Option<&'a BTreeMap<String, String>>,
}

/// Parses the objects that are `NetworkPolicy` objects, sorted by name; others are skipped.
fn policies(objects: &[DynamicObject]) -> Vec<NetworkPolicy> {
    let mut out: Vec<NetworkPolicy> = objects
        .iter()
        .filter_map(|o| o.clone().try_parse().ok())
        .collect();
    out.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
    out
}

/// The policy types a policy has, with the documented defaults.
fn types(policy: &NetworkPolicy) -> (bool, bool) {
    let spec = policy.spec.as_ref();
    spec.and_then(|s| s.policy_types.as_ref()).map_or_else(
        || {
            (
                true,
                spec.and_then(|s| s.egress.as_ref())
                    .is_some_and(|e| !e.is_empty()),
            )
        },
        |types| {
            (
                types.iter().any(|t| t == "Ingress"),
                types.iter().any(|t| t == "Egress"),
            )
        },
    )
}

fn selects(policy: &NetworkPolicy, labels: &BTreeMap<String, String>) -> bool {
    policy
        .spec
        .as_ref()
        .is_some_and(|s| s.pod_selector.as_ref().is_none_or(|p| matches(p, labels)))
}

/// How far a rule's peers admit `pod`, evaluated in `namespace` (the policy's). A selector Zeabur
/// cannot evaluate (unknown labels) gives [`Access::Limited`].
fn peer_access(peers: Option<&Vec<NetworkPolicyPeer>>, namespace: &str, pod: &Pod<'_>) -> Access {
    let Some(peers) = peers.filter(|p| !p.is_empty()) else {
        return Access::Allowed;
    };
    let mut best = Access::Blocked;
    for peer in peers {
        if peer.ip_block.is_some() {
            continue;
        }
        let namespace_ok = match &peer.namespace_selector {
            None => Access::from(pod.namespace == namespace),
            // Any other namespace: an empty selector takes them all, another some of them.
            Some(selector) if pod.namespace.is_empty() => {
                if is_empty(selector) {
                    Access::Allowed
                } else {
                    Access::Limited
                }
            }
            Some(selector) => {
                let labels =
                    BTreeMap::from([(NAMESPACE_NAME_LABEL.to_owned(), pod.namespace.to_owned())]);
                if matches(selector, &labels) {
                    Access::Allowed
                } else if selector
                    .match_labels
                    .iter()
                    .flatten()
                    .all(|(k, _)| k == NAMESPACE_NAME_LABEL)
                    && selector
                        .match_expressions
                        .iter()
                        .flatten()
                        .all(|e| e.key == NAMESPACE_NAME_LABEL)
                {
                    // Decided by the one label Zeabur knows: it does not match.
                    Access::Blocked
                } else {
                    Access::Limited
                }
            }
        };
        let pod_ok = match (&peer.pod_selector, pod.labels) {
            (None, _) => Access::Allowed,
            (Some(selector), _) if is_empty(selector) => Access::Allowed,
            (Some(selector), Some(labels)) => Access::from(matches(selector, labels)),
            (Some(_), None) => Access::Limited,
        };
        best = best.or(namespace_ok.and(pod_ok));
    }
    best
}

impl From<bool> for Access {
    fn from(allowed: bool) -> Self {
        if allowed {
            Self::Allowed
        } else {
            Self::Blocked
        }
    }
}

/// Whether a rule's ports cover every port (`Allowed`) or some (`Limited`); `dns` asks for port
/// 53 instead.
fn port_access(ports: Option<&Vec<NetworkPolicyPort>>, dns: bool) -> Access {
    let Some(ports) = ports.filter(|p| !p.is_empty()) else {
        return Access::Allowed;
    };
    if !dns {
        return Access::Limited;
    }
    let covers_53 = ports.iter().any(|p| match &p.port {
        None => true,
        Some(k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(n)) => {
            *n == 53 || p.end_port.is_some_and(|end| *n <= 53 && 53 <= end)
        }
        Some(k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String(name)) => {
            name.starts_with("dns")
        }
    });
    if covers_53 {
        Access::Allowed
    } else {
        Access::Blocked
    }
}

/// One side of a connection: how far the policies that isolate it admit the peer, and which.
struct Side<'p> {
    access: Access,
    /// The isolating policies, each with how far its own rules admit the peer.
    by: Vec<(&'p NetworkPolicy, Access)>,
}

impl<'p> Side<'p> {
    fn new(by: Vec<(&'p NetworkPolicy, Access)>) -> Self {
        let access = if by.is_empty() {
            Access::Allowed
        } else {
            by.iter()
                .fold(Access::Blocked, |best, (_, access)| best.or(*access))
        };
        Self { access, by }
    }

    /// The policies that decide it: those that admit the peer, or, when none does, every one that
    /// isolates it.
    fn deciding(&self) -> impl Iterator<Item = &'p NetworkPolicy> + '_ {
        let blocked = self.access == Access::Blocked;
        self.by
            .iter()
            .filter(move |(_, access)| blocked || *access != Access::Blocked)
            .map(|(policy, _)| *policy)
    }
}

/// Ingress to `labels` (pods of `namespace`) from `pod`.
fn ingress_side<'p>(
    policies: &'p [NetworkPolicy],
    namespace: &str,
    labels: &BTreeMap<String, String>,
    pod: &Pod<'_>,
) -> Side<'p> {
    Side::new(
        policies
            .iter()
            .filter(|p| types(p).0 && selects(p, labels))
            .map(|policy| {
                let access = policy
                    .spec
                    .iter()
                    .flat_map(|s| s.ingress.iter().flatten())
                    .fold(Access::Blocked, |best, rule| {
                        let peers = peer_access(rule.from.as_ref(), namespace, pod);
                        best.or(peers.and(port_access(rule.ports.as_ref(), false)))
                    });
                (policy, access)
            })
            .collect(),
    )
}

fn ingress(
    policies: &[NetworkPolicy],
    namespace: &str,
    labels: &BTreeMap<String, String>,
    pod: &Pod<'_>,
) -> Access {
    ingress_side(policies, namespace, labels, pod).access
}

/// Egress from `labels`: to `pod`, or with `pod` `None` to the outside.
fn egress_side<'p>(
    policies: &'p [NetworkPolicy],
    namespace: &str,
    labels: &BTreeMap<String, String>,
    pod: Option<&Pod<'_>>,
    dns: bool,
) -> Side<'p> {
    Side::new(
        policies
            .iter()
            .filter(|p| types(p).1 && selects(p, labels))
            .map(|policy| {
                let access = policy
                    .spec
                    .iter()
                    .flat_map(|s| s.egress.iter().flatten())
                    .fold(Access::Blocked, |best, rule| {
                        let peers = pod.map_or_else(
                            || match rule.to.as_ref().filter(|t| !t.is_empty()) {
                                None => Access::Allowed,
                                Some(to) if to.iter().any(|p| p.ip_block.is_some()) => {
                                    Access::Allowed
                                }
                                Some(_) => Access::Blocked,
                            },
                            |pod| peer_access(rule.to.as_ref(), namespace, pod),
                        );
                        best.or(peers.and(port_access(rule.ports.as_ref(), dns)))
                    });
                (policy, access)
            })
            .collect(),
    )
}

fn egress(
    policies: &[NetworkPolicy],
    namespace: &str,
    labels: &BTreeMap<String, String>,
    pod: Option<&Pod<'_>>,
    dns: bool,
) -> Access {
    egress_side(policies, namespace, labels, pod, dns).access
}

/// Egress of `from` and ingress of `to`, both pods of `namespace`.
fn between<'p>(
    policies: &'p [NetworkPolicy],
    namespace: &str,
    from: &Workload,
    to: &Workload,
) -> (Side<'p>, Side<'p>) {
    let out = egress_side(
        policies,
        namespace,
        &from.labels,
        Some(&Pod {
            namespace,
            labels: Some(&to.labels),
        }),
        false,
    );
    let into = ingress_side(
        policies,
        namespace,
        &to.labels,
        &Pod {
            namespace,
            labels: Some(&from.labels),
        },
    );
    (out, into)
}

/// The policies, by name, that decide a connection: when it is blocked, those of each side that
/// blocks it; otherwise those that admit it on either side.
fn via(out: &Side<'_>, into: &Side<'_>) -> Vec<String> {
    let blocked = out.access.and(into.access) == Access::Blocked;
    let mut names: Vec<String> = [out, into]
        .into_iter()
        .filter(|side| !blocked || side.access == Access::Blocked)
        .flat_map(Side::deciding)
        .filter_map(|p| p.metadata.name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// One service's reachability among `workloads`.
fn service(policies: &[NetworkPolicy], input: Input<'_>, w: &Workload) -> ServiceReachability {
    let namespace = input.namespace;
    let isolated = |ingress_side: bool| {
        policies.iter().any(|p| {
            let (i, e) = types(p);
            (if ingress_side { i } else { e }) && selects(p, &w.labels)
        })
    };
    let traefik = BTreeMap::from([("app.kubernetes.io/name".to_owned(), "traefik".to_owned())]);
    let core_dns = BTreeMap::from([("k8s-app".to_owned(), "kube-dns".to_owned())]);
    ServiceReachability {
        service: w.id.clone(),
        selected_by: policies
            .iter()
            .filter(|p| selects(p, &w.labels))
            .filter_map(|p| p.metadata.name.clone())
            .collect(),
        ingress_isolated: isolated(true),
        egress_isolated: isolated(false),
        from_services: input
            .workloads
            .iter()
            .filter(|other| other.id != w.id)
            .map(|other| {
                let (out, into) = between(policies, namespace, other, w);
                Peer {
                    service: other.id.clone(),
                    access: out.access.and(into.access),
                    via: via(&out, &into),
                }
            })
            .collect(),
        from_ingress_controller: ingress(
            policies,
            namespace,
            &w.labels,
            &Pod {
                namespace: "kube-system",
                labels: Some(&traefik),
            },
        ),
        from_other_namespaces: ingress(
            policies,
            namespace,
            &w.labels,
            &Pod {
                namespace: "",
                labels: None,
            },
        ),
        to_dns: egress(
            policies,
            namespace,
            &w.labels,
            Some(&Pod {
                namespace: "kube-system",
                labels: Some(&core_dns),
            }),
            true,
        ),
        to_outside: egress(policies, namespace, &w.labels, None, false),
    }
}

/// The project's reachability.
#[must_use]
pub fn reachability(input: Input<'_>) -> Reachability {
    let policies = policies(input.policies);
    let services = input
        .workloads
        .iter()
        .map(|w| service(&policies, input, w))
        .collect();
    let by_id: BTreeMap<&str, &Workload> =
        input.workloads.iter().map(|w| (w.id.as_str(), w)).collect();
    let connections = input
        .connections
        .iter()
        .filter_map(|(from, to)| {
            let (Some(a), Some(b)) = (by_id.get(from.as_str()), by_id.get(to.as_str())) else {
                return None;
            };
            let (out, into) = between(&policies, input.namespace, a, b);
            let (out, into) = (out.access, into.access);
            let mut blocked_at = Vec::new();
            if out == Access::Blocked {
                blocked_at.push("egress".to_owned());
            }
            if into == Access::Blocked {
                blocked_at.push("ingress".to_owned());
            }
            Some(Connection {
                from: from.clone(),
                to: to.clone(),
                access: out.and(into),
                blocked_at,
            })
        })
        .collect();
    let mut allowed: Vec<AllowedNamespace> = policies
        .iter()
        .flat_map(|p| {
            let name = p.metadata.name.clone().unwrap_or_default();
            allowed_namespaces(p, input.namespace)
                .into_iter()
                .map(move |namespace| AllowedNamespace {
                    namespace,
                    policy: name.clone(),
                })
        })
        .collect();
    allowed.sort_by(|a, b| (&a.namespace, &a.policy).cmp(&(&b.namespace, &b.policy)));
    Reachability {
        policies: policies
            .iter()
            .filter_map(|p| p.metadata.name.clone())
            .collect(),
        project_policy: policies
            .iter()
            .any(|p| p.metadata.name.as_deref() == Some(PROJECT_POLICY)),
        allowed_namespaces: allowed,
        services,
        connections,
    }
}

/// What deleting one policy opens, compared with the project as it is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Opened {
    /// Services, by id, that the policy isolates for ingress and nothing else does.
    pub ingress: Vec<String>,
    /// Services, by id, that the policy isolates for egress and nothing else does.
    pub egress: Vec<String>,
    /// The connections given, `(from, to)` by service id, that reach further without it.
    pub connections: Vec<(String, String)>,
}

/// What deleting the policy `name` opens: reachability evaluated with and without it.
///
/// Rules of different policies add up and never subtract [k8s-netpol], so taking one away can
/// only narrow what is allowed for the pods it isolates, or, when it was the only policy that
/// selected them, lift their isolation altogether: those pods then accept (or send) everything.
///
/// [k8s-netpol]: https://kubernetes.io/docs/concepts/services-networking/network-policies/
#[must_use]
pub fn opened_without(input: Input<'_>, name: &str) -> Opened {
    let rest: Vec<DynamicObject> = input
        .policies
        .iter()
        .filter(|p| p.metadata.name.as_deref() != Some(name))
        .cloned()
        .collect();
    let before = reachability(input);
    let after = reachability(Input {
        policies: &rest,
        ..input
    });
    let mut opened = Opened::default();
    for (was, now) in before.services.iter().zip(&after.services) {
        if was.ingress_isolated && !now.ingress_isolated {
            opened.ingress.push(was.service.clone());
        }
        if was.egress_isolated && !now.egress_isolated {
            opened.egress.push(was.service.clone());
        }
    }
    opened.connections = before
        .connections
        .iter()
        .zip(&after.connections)
        .filter(|(was, now)| now.access < was.access)
        .map(|(was, _)| (was.from.clone(), was.to.clone()))
        .collect();
    opened
}

/// The policies that keep every connection from the internet off service `id`.
///
/// `None` when no policy isolates it for ingress, when one may admit such a connection, or when
/// `id` is not a service of `input`.
///
/// A connection to a `LoadBalancer` port reaches the pod from an address outside the cluster's
/// pods, or, through k3s `ServiceLB`, from a pod in `kube-system` [k3s-servicelb]. Thus a rule
/// admits it when it has no `from`, an `ipBlock` peer, or a peer that may be a pod of
/// `kube-system`. A rule that names ports may admit it, so it counts as admitting.
///
/// [k3s-servicelb]: https://docs.k3s.io/networking/networking-services#how-the-service-lb-works
#[must_use]
pub fn blocks_internet(input: Input<'_>, id: &str) -> Option<Vec<String>> {
    let workload = input.workloads.iter().find(|w| w.id == id)?;
    let policies = policies(input.policies);
    let kube_system = Pod {
        namespace: "kube-system",
        labels: None,
    };
    let isolating: Vec<&NetworkPolicy> = policies
        .iter()
        .filter(|p| types(p).0 && selects(p, &workload.labels))
        .collect();
    if isolating.is_empty() {
        return None;
    }
    let admits = isolating.iter().any(|policy| {
        policy
            .spec
            .iter()
            .flat_map(|s| s.ingress.iter().flatten())
            .any(|rule| {
                rule.from
                    .as_ref()
                    .is_none_or(|from| from.is_empty() || from.iter().any(|p| p.ip_block.is_some()))
                    || peer_access(rule.from.as_ref(), input.namespace, &kube_system)
                        != Access::Blocked
            })
    });
    if admits {
        return None;
    }
    Some(
        isolating
            .iter()
            .filter_map(|p| p.metadata.name.clone())
            .collect(),
    )
}

/// The connections the policies block: what the "blocked connection" gap reports.
#[must_use]
pub fn blocked_connections(reachability: &Reachability) -> Vec<&Connection> {
    reachability
        .connections
        .iter()
        .filter(|c| c.access == Access::Blocked)
        .collect()
}

/// The id of the rule [`gaps`] checks.
pub const BLOCKED_CONNECTION: &str = "blocked-connection";

/// The rule `blocked-connection`: a connection the project's own configuration implies (a
/// variable pointing at another service) that its NetworkPolicy objects block [k8s-netpol].
///
/// A warning: the caller will fail at run time. When only the callee's ingress stops it, the fix
/// is a plan creating the `allow-from-service` preset (see [`presets`]); when the caller's egress
/// stops it, the fix is a plan creating a policy that lets the caller reach the callee. A
/// connection blocked at both ends gets the egress policy; the ingress side is the same preset,
/// offered on the callee's Networking tab.
///
/// [k8s-netpol]: https://kubernetes.io/docs/concepts/services-networking/network-policies/
#[must_use]
pub fn gaps(namespace: &str, reachability: &Reachability, workloads: &[Workload]) -> Vec<Finding> {
    let find = |id: &str| workloads.iter().find(|w| w.id == id);
    blocked_connections(reachability)
        .into_iter()
        .filter_map(|connection| {
            let (from, to) = (find(&connection.from)?, find(&connection.to)?);
            let (_, from_name) = kind_and_name(&from.id)?;
            let (to_kind, to_name) = kind_and_name(&to.id)?;
            let egress = connection.blocked_at.iter().any(|at| at == "egress");
            let ingress = connection.blocked_at.iter().any(|at| at == "ingress");
            let at = match (egress, ingress) {
                (true, true) => "both",
                (true, false) => "egress",
                _ => "ingress",
            };
            let message = Message::new("gap.blockedConnection")
                .with("from", from_name)
                .with("to", to_name)
                .with("at", at);
            let fix = if egress {
                let object = policy(
                    namespace,
                    &policy_name(&format!("allow-to-{to_name}"), from),
                    &json!({
                        "podSelector": selector_value(&from.labels),
                        "policyTypes": ["Egress"],
                        "egress": [{"to": [{"podSelector": selector_value(&to.labels)}]}],
                    }),
                );
                Fix {
                    kind: FixKind::Plan,
                    id: "networkPolicy.allowEgress".to_owned(),
                    params: BTreeMap::from([
                        ("from".to_owned(), from.id.clone()),
                        ("to".to_owned(), to.id.clone()),
                        ("object".to_owned(), object.to_string()),
                    ]),
                }
            } else {
                let preset = presets(namespace, to, std::slice::from_ref(from))
                    .into_iter()
                    .find(|p| p.from.as_deref() == Some(from.id.as_str()))?;
                Fix {
                    kind: FixKind::Plan,
                    id: "networkPolicy.allowFrom".to_owned(),
                    params: BTreeMap::from([
                        ("from".to_owned(), from.id.clone()),
                        ("to".to_owned(), to.id.clone()),
                        ("object".to_owned(), preset.object.to_string()),
                    ]),
                }
            };
            Some(Finding {
                gap: Gap {
                    rule: BLOCKED_CONNECTION.to_owned(),
                    severity: Severity::Warning,
                    subject: Origin::object(to_kind, to_name),
                    service: Some(to.id.clone()),
                    message,
                    fix,
                    citations: vec![Citation {
                        title: "Network Policies".to_owned(),
                        url: "https://kubernetes.io/docs/concepts/services-networking/network-policies/"
                            .to_owned(),
                    }],
                },
            })
        })
        .collect()
}

/// How traffic from outside the project reaches its services, by service id.
#[derive(Debug, Clone, Copy, Default)]
pub struct Exposed<'a> {
    /// Services an Ingress or HTTPRoute sends traffic to, through the ingress controller.
    pub routed: &'a [String],
    /// Services behind a `NodePort` or `LoadBalancer` Service, reached from addresses
    /// NetworkPolicy reads as outside the cluster's pods.
    pub published: &'a [String],
}

/// The id of the rule [`unprotected`] checks.
pub const NO_NETWORK_POLICY: &str = "no-network-policy";

/// The rule `no-network-policy`: services of the project that no NetworkPolicy isolates for
/// ingress, each accepting connections from every pod of the cluster [k8s-isolation].
///
/// One finding per project, naming those services, so a project without any policy is one line.
///
/// The fix is one plan: the `isolate` preset for each of them, and
/// `allow-from-ingress-controller` for those an Ingress or HTTPRoute sends traffic to. `isolate`
/// admits every pod of the project, so the project's own connections need no `allow-from-service`;
/// the plan is checked with [`reachability`] itself, and what it would cut off that is open today
/// is named in the message as `cuts`: `otherNamespaces`, `outside`, `published`, `connections`.
///
/// Always advice: whether the policies take effect depends on the cluster's network plugin, which
/// the message says. No workloads, no finding.
///
/// [k8s-isolation]: https://kubernetes.io/docs/concepts/services-networking/network-policies/#the-two-sorts-of-pod-isolation
#[must_use]
pub fn unprotected(input: Input<'_>, exposed: Exposed<'_>) -> Option<Finding> {
    let before = reachability(input);
    let open: Vec<&Workload> = input
        .workloads
        .iter()
        .zip(&before.services)
        .filter(|(_, s)| !s.ingress_isolated)
        .map(|(w, _)| w)
        .collect();
    if open.is_empty() {
        return None;
    }
    let objects = vec![project_policy(input.namespace)];
    let mut applied = input.policies.to_vec();
    applied.extend(
        objects
            .iter()
            .filter_map(|o| serde_json::from_value(o.clone()).ok()),
    );
    let after_input = Input {
        policies: &applied,
        ..input
    };
    let after = reachability(after_input);
    let names: Vec<&str> = open
        .iter()
        .filter_map(|w| w.id.rsplit('/').next())
        .collect();
    Some(Finding {
        gap: Gap {
            rule: NO_NETWORK_POLICY.to_owned(),
            severity: Severity::Advice,
            subject: Origin::object("Namespace", input.namespace),
            service: None,
            message: Message::new("gap.noNetworkPolicy")
                .with("services", names.join(","))
                .with(
                    "cuts",
                    cuts(&before, &after, &open, exposed, after_input).join(","),
                ),
            fix: Fix {
                kind: FixKind::Plan,
                id: "networkPolicy.isolateProject".to_owned(),
                params: BTreeMap::from([(
                    "objects".to_owned(),
                    Value::Array(objects).to_string(),
                )]),
            },
            citations: vec![
                Citation {
                    title: "Network Policies: the two sorts of pod isolation".to_owned(),
                    url: "https://kubernetes.io/docs/concepts/services-networking/network-policies/#the-two-sorts-of-pod-isolation"
                        .to_owned(),
                },
                Citation {
                    title: "k3s: Network Policy Controller".to_owned(),
                    url: "https://docs.k3s.io/networking/networking-services#network-policy-controller"
                        .to_owned(),
                },
            ],
        },
    })
}

/// What isolating `open` cuts off that is open today, `before` and `after` the plan:
/// `otherNamespaces` (pods of other namespaces reaching them), `outside` (their way out of the
/// cluster), `published` (`NodePort` and `LoadBalancer` traffic, which arrives from addresses
/// outside the cluster's pods) and `connections` (one of the project's connections; the
/// isolation admits every pod of the project, so the model keeps this off).
fn cuts(
    before: &Reachability,
    after: &Reachability,
    open: &[&Workload],
    exposed: Exposed<'_>,
    applied: Input<'_>,
) -> Vec<&'static str> {
    let cut = |field: fn(&ServiceReachability) -> Access| {
        before.services.iter().zip(&after.services).any(|(b, a)| {
            open.iter().any(|w| w.id == b.service)
                && field(b) == Access::Allowed
                && field(a) > field(b)
        })
    };
    [
        ("otherNamespaces", cut(|s| s.from_other_namespaces)),
        ("outside", cut(|s| s.to_outside)),
        (
            "published",
            open.iter().any(|w| {
                exposed.published.contains(&w.id) && blocks_internet(applied, &w.id).is_some()
            }),
        ),
        (
            "connections",
            before
                .connections
                .iter()
                .zip(&after.connections)
                .any(|(b, a)| b.access != Access::Blocked && a.access == Access::Blocked),
        ),
    ]
    .into_iter()
    .filter_map(|(word, cut)| cut.then_some(word))
    .collect()
}

/// The workload kind (`Deployment`) and name of a service id `projects/<p>/services/<kind>/<name>`.
fn kind_and_name(id: &str) -> Option<(&'static str, &str)> {
    let mut parts = id.rsplit('/');
    let name = parts.next()?;
    let kind = WorkloadKind::parse(parts.next()?)?;
    Some((kind.as_str(), name))
}

/// A ready-made policy for a service, to plan as a new object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NetworkPolicyPreset)]
pub struct Preset {
    /// `isolate`, `allow-from-service` or `allow-from-ingress-controller`.
    pub kind: String,
    /// For `allow-from-service`, the caller's id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// The NetworkPolicy to create.
    #[schema(value_type = Object)]
    pub object: Value,
}

fn selector_value(labels: &BTreeMap<String, String>) -> Value {
    json!({ "matchLabels": labels })
}

/// A name for a policy: `zeabur-<preset>-<workload name>`, cut to 63 characters.
fn policy_name(preset: &str, workload: &Workload) -> String {
    let name = workload.id.rsplit('/').next().unwrap_or_default();
    let mut out = format!("zeabur-{preset}-{name}");
    out.truncate(63);
    out.trim_end_matches('-').to_owned()
}

fn policy(namespace: &str, name: &str, spec: &Value) -> Value {
    json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "NetworkPolicy",
        "metadata": {
            "name": name,
            "namespace": namespace,
            "labels": {"app.kubernetes.io/managed-by": "zeabur"},
        },
        "spec": spec,
    })
}

/// The presets for `target`:
///
/// - `isolate`: only the project's own pods reach it, and it reaches only the project and cluster
///   DNS (port 53, UDP and TCP) — without the DNS rule every name lookup would fail;
/// - `allow-from-ingress-controller`: k3s's Traefik (`kube-system`) may reach it;
/// - `allow-from-service`, one per other service: that service's pods may reach it.
///
/// Allow rules add to an isolation; on their own they also isolate the service for ingress
/// (a policy that selects pods admits only what its rules allow).
#[must_use]
pub fn presets(namespace: &str, target: &Workload, others: &[Workload]) -> Vec<Preset> {
    let pods = selector_value(&target.labels);
    let mut out = vec![
        Preset {
            kind: "isolate".to_owned(),
            from: None,
            object: policy(
                namespace,
                &policy_name("isolate", target),
                &json!({
                    "podSelector": pods,
                    "policyTypes": ["Ingress", "Egress"],
                    "ingress": [{"from": [{"podSelector": {}}]}],
                    "egress": [
                        {"to": [{"podSelector": {}}]},
                        {
                            "to": [{
                                "namespaceSelector": {"matchLabels": {NAMESPACE_NAME_LABEL: "kube-system"}},
                                "podSelector": {"matchLabels": {"k8s-app": "kube-dns"}},
                            }],
                            "ports": [{"protocol": "UDP", "port": 53}, {"protocol": "TCP", "port": 53}],
                        },
                    ],
                }),
            ),
        },
        Preset {
            kind: "allow-from-ingress-controller".to_owned(),
            from: None,
            object: policy(
                namespace,
                &policy_name("allow-ingress", target),
                &json!({
                    "podSelector": pods,
                    "policyTypes": ["Ingress"],
                    "ingress": [{"from": [{
                        "namespaceSelector": {"matchLabels": {NAMESPACE_NAME_LABEL: "kube-system"}},
                        "podSelector": {"matchLabels": {"app.kubernetes.io/name": "traefik"}},
                    }]}],
                }),
            ),
        },
    ];
    for other in others.iter().filter(|o| o.id != target.id) {
        let from_name = other.id.rsplit('/').next().unwrap_or_default();
        out.push(Preset {
            kind: "allow-from-service".to_owned(),
            from: Some(other.id.clone()),
            object: policy(
                namespace,
                &policy_name(&format!("allow-{from_name}"), target),
                &json!({
                    "podSelector": pods,
                    "policyTypes": ["Ingress"],
                    "ingress": [{"from": [{"podSelector": selector_value(&other.labels)}]}],
                }),
            ),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workload(name: &str, app: &str) -> Workload {
        Workload {
            id: format!("projects/shop/services/deployment/{name}"),
            labels: BTreeMap::from([("app".to_owned(), app.to_owned())]),
        }
    }

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn the_isolate_preset_blocks_the_internet_until_a_rule_admits_an_ip_block() {
        let workloads = [workload("db", "db")];
        let isolate = presets("shop", &workloads[0], &workloads)
            .into_iter()
            .find(|p| p.kind == "isolate")
            .unwrap();
        let open = object(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": "db-from-internet", "namespace": "shop"},
            "spec": {"podSelector": {"matchLabels": {"app": "db"}},
                "ingress": [{"from": [{"ipBlock": {"cidr": "0.0.0.0/0"}}], "ports": [{"port": 5432}]}]}
        }));
        let blocked = |policies: &[DynamicObject]| {
            blocks_internet(
                Input {
                    namespace: "shop",
                    workloads: &workloads,
                    policies,
                    connections: &[],
                },
                &workloads[0].id,
            )
        };
        let isolated = [object(isolate.object)];
        assert_eq!(blocked(&isolated).map(|p| p.len()), Some(1));
        assert_eq!(blocked(&[isolated[0].clone(), open]), None);
        assert_eq!(blocked(&[]), None);
    }

    #[test]
    fn the_isolate_preset_keeps_the_project_and_dns_reachable() {
        let workloads = [workload("web", "web"), workload("api", "api")];
        let preset = presets("shop", &workloads[1], &workloads)
            .into_iter()
            .find(|p| p.kind == "isolate")
            .unwrap();
        let r = reachability(Input {
            namespace: "shop",
            workloads: &workloads,
            policies: &[object(preset.object)],
            connections: &[(workloads[0].id.clone(), workloads[1].id.clone())],
        });
        let api = &r.services[1];
        assert_eq!(r.connections[0].access, Access::Allowed);
        assert_eq!(api.to_dns, Access::Allowed);
        assert_eq!(api.to_outside, Access::Blocked);
        assert_eq!(api.from_ingress_controller, Access::Blocked);
        assert_eq!(api.from_other_namespaces, Access::Blocked);
    }

    fn input<'a>(
        workloads: &'a [Workload],
        policies: &'a [DynamicObject],
        connections: &'a [(String, String)],
    ) -> Input<'a> {
        Input {
            namespace: "shop",
            workloads,
            policies,
            connections,
        }
    }

    fn deny(name: &str, selector: &Value) -> DynamicObject {
        object(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": name, "namespace": "shop"},
            "spec": {"podSelector": selector}
        }))
    }

    #[test]
    fn deleting_the_only_policy_of_a_service_lifts_its_isolation_and_opens_its_connections() {
        let workloads = [workload("web", "web"), workload("api", "api")];
        let connections = [(workloads[0].id.clone(), workloads[1].id.clone())];
        let policies = [
            deny("deny-api", &json!({"matchLabels": {"app": "api"}})),
            deny("deny-all", &json!({})),
        ];
        // Another policy still isolates both: deleting one opens nothing.
        let opened = opened_without(input(&workloads, &policies, &connections), "deny-api");
        assert_eq!(opened, Opened::default());
        // The only one left: both services accept everything again, and web reaches api.
        let opened = opened_without(input(&workloads, &policies[1..], &connections), "deny-all");
        assert_eq!(
            opened.ingress,
            vec![workloads[0].id.clone(), workloads[1].id.clone()]
        );
        assert_eq!(opened.egress, Vec::<String>::new());
        assert_eq!(opened.connections, connections.to_vec());
    }

    #[test]
    fn deleting_an_allow_rule_opens_nothing() {
        let workloads = [workload("web", "web"), workload("api", "api")];
        let connections = [(workloads[0].id.clone(), workloads[1].id.clone())];
        let allow = object(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": "allow-web", "namespace": "shop"},
            "spec": {"podSelector": {"matchLabels": {"app": "api"}},
                "ingress": [{"from": [{"podSelector": {"matchLabels": {"app": "web"}}}]}]}
        }));
        let policies = [
            deny("deny-api", &json!({"matchLabels": {"app": "api"}})),
            allow,
        ];
        let opened = opened_without(input(&workloads, &policies, &connections), "allow-web");
        assert_eq!(opened, Opened::default());
    }

    #[test]
    fn a_project_without_policies_is_one_finding_whose_plan_keeps_what_works() {
        let workloads = [workload("web", "web"), workload("api", "api")];
        let connections = [(workloads[0].id.clone(), workloads[1].id.clone())];
        let finding = unprotected(
            input(&workloads, &[], &connections),
            Exposed {
                routed: std::slice::from_ref(&workloads[0].id),
                published: std::slice::from_ref(&workloads[1].id),
            },
        )
        .unwrap();
        let gap = &finding.gap;
        assert_eq!(gap.rule, NO_NETWORK_POLICY);
        assert_eq!(gap.severity, Severity::Advice);
        assert_eq!(gap.subject, Origin::object("Namespace", "shop"));
        assert_eq!(gap.message.params["services"], "web,api");
        // Only other projects lose access: the internet, egress and the ingress controller stay.
        assert_eq!(gap.message.params["cuts"], "otherNamespaces");
        assert_eq!(gap.fix.kind, FixKind::Plan);
        let objects: Vec<Value> = serde_json::from_str(&gap.fix.params["objects"]).unwrap();
        assert_eq!(objects, [project_policy("shop")]);
        let applied: Vec<DynamicObject> = objects.into_iter().map(object).collect();
        let r = reachability(input(&workloads, &applied, &connections));
        assert!(r.project_policy);
        assert_eq!(r.connections[0].access, Access::Allowed);
        assert!(r.services.iter().all(|s| s.ingress_isolated
            && s.from_ingress_controller == Access::Allowed
            && s.from_other_namespaces != Access::Allowed
            && s.to_outside == Access::Allowed));
        assert!(
            unprotected(
                input(&workloads, &applied, &connections),
                Exposed::default()
            )
            .is_none()
        );
    }

    #[test]
    fn a_policy_selecting_every_pod_leaves_nothing_to_report() {
        let workloads = [workload("web", "web"), workload("api", "api")];
        let policies = [deny("default-deny", &json!({}))];
        assert!(unprotected(input(&workloads, &policies, &[]), Exposed::default()).is_none());
    }

    #[test]
    fn a_policy_selecting_some_pods_names_only_the_others() {
        let workloads = [workload("web", "web"), workload("api", "api")];
        let policies = [deny("deny-web", &json!({"matchLabels": {"app": "web"}}))];
        let finding = unprotected(input(&workloads, &policies, &[]), Exposed::default()).unwrap();
        assert_eq!(finding.gap.message.params["services"], "api");
        let objects: Vec<Value> = serde_json::from_str(&finding.gap.fix.params["objects"]).unwrap();
        assert_eq!(objects, [project_policy("shop")]);
    }

    fn allow_from(name: &str, from: &Value) -> DynamicObject {
        object(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": name, "namespace": "shop"},
            "spec": {"podSelector": {}, "policyTypes": ["Ingress"], "ingress": [{"from": from}]}
        }))
    }

    #[test]
    fn a_policy_that_lets_a_namespace_in_whole_is_listed() {
        let workloads = [workload("api", "api")];
        let policies = [
            object(project_policy("shop")),
            allow_from(
                "zeabur-allow-from-blog",
                &json!([{"namespaceSelector": {"matchLabels": {NAMESPACE_NAME_LABEL: "blog"}}}]),
            ),
            // Only some pods of the namespace, or only one port: not the whole namespace.
            allow_from(
                "some-pods",
                &json!([{"namespaceSelector": {"matchLabels": {NAMESPACE_NAME_LABEL: "wiki"}},
                    "podSelector": {"matchLabels": {"app": "x"}}}]),
            ),
            allow_from(
                "by-team",
                &json!([{"namespaceSelector": {"matchLabels": {"team": "a"}}}]),
            ),
        ];
        let r = reachability(input(&workloads, &policies, &[]));
        assert!(r.project_policy);
        assert_eq!(
            r.allowed_namespaces,
            [AllowedNamespace {
                namespace: "blog".to_owned(),
                policy: "zeabur-allow-from-blog".to_owned(),
            }]
        );
    }

    #[test]
    fn an_egress_only_policy_does_not_protect_a_service() {
        let workloads = [workload("api", "api")];
        let policies = [object(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": "egress", "namespace": "shop"},
            "spec": {"podSelector": {}, "policyTypes": ["Egress"]}
        }))];
        let finding = unprotected(input(&workloads, &policies, &[]), Exposed::default()).unwrap();
        assert_eq!(finding.gap.message.params["services"], "api");
    }

    #[test]
    fn no_workloads_no_finding() {
        assert!(unprotected(input(&[], &[], &[]), Exposed::default()).is_none());
    }

    #[test]
    fn a_namespace_selector_on_an_unknown_label_is_limited() {
        let workloads = [workload("api", "api")];
        let policy = object(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": "team", "namespace": "shop"},
            "spec": {"podSelector": {}, "ingress": [{"from": [
                {"namespaceSelector": {"matchLabels": {"team": "payments"}}}
            ]}]}
        }));
        let r = reachability(Input {
            namespace: "shop",
            workloads: &workloads,
            policies: &[policy],
            connections: &[],
        });
        assert_eq!(r.services[0].from_other_namespaces, Access::Limited);
    }

    fn allow(name: &str, to: &str, from: &str, ports: Option<Value>) -> DynamicObject {
        let mut rule = json!({"from": [{"podSelector": {"matchLabels": {"app": from}}}]});
        if let Some(ports) = ports {
            rule["ports"] = ports;
        }
        object(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": name, "namespace": "shop"},
            "spec": {"podSelector": {"matchLabels": {"app": to}}, "ingress": [rule]}
        }))
    }

    /// `via` of each peer of the service `of`, by the peer's name.
    fn via_of(r: &Reachability, of: usize) -> Vec<(&str, Access, Vec<&str>)> {
        r.services[of]
            .from_services
            .iter()
            .map(|p| {
                (
                    p.service.rsplit('/').next().unwrap(),
                    p.access,
                    p.via.iter().map(String::as_str).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn without_policies_no_peer_names_one() {
        let workloads = [workload("web", "web"), workload("api", "api")];
        let r = reachability(input(&workloads, &[], &[]));
        assert_eq!(via_of(&r, 1), [("web", Access::Allowed, vec![])]);
    }

    #[test]
    fn a_peer_names_the_policies_that_admit_it_or_all_that_block_it() {
        let workloads = [
            workload("web", "web"),
            workload("worker", "worker"),
            workload("cron", "cron"),
            workload("api", "api"),
        ];
        let policies = [
            deny("deny-api", &json!({"matchLabels": {"app": "api"}})),
            allow("allow-web", "api", "web", None),
            allow(
                "allow-worker",
                "api",
                "worker",
                Some(json!([{"port": 8080}])),
            ),
        ];
        let r = reachability(input(&workloads, &policies, &[]));
        assert_eq!(
            via_of(&r, 3),
            [
                ("web", Access::Allowed, vec!["allow-web"]),
                ("worker", Access::Limited, vec!["allow-worker"]),
                (
                    "cron",
                    Access::Blocked,
                    vec!["allow-web", "allow-worker", "deny-api"]
                ),
            ]
        );
    }

    #[test]
    fn a_peer_blocked_on_its_way_out_names_only_its_own_egress_policy() {
        let workloads = [workload("web", "web"), workload("api", "api")];
        let egress = object(json!({
            "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
            "metadata": {"name": "web-out", "namespace": "shop"},
            "spec": {"podSelector": {"matchLabels": {"app": "web"}}, "policyTypes": ["Egress"],
                "egress": [{"to": [{"podSelector": {"matchLabels": {"app": "db"}}}]}]}
        }));
        let policies = [egress, allow("allow-web", "api", "web", None)];
        let r = reachability(input(&workloads, &policies, &[]));
        assert_eq!(via_of(&r, 1), [("web", Access::Blocked, vec!["web-out"])]);
        // Nothing isolates api's way out or web's way in.
        assert_eq!(via_of(&r, 0), [("api", Access::Allowed, vec![])]);
    }
}
