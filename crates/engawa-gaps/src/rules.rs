//! Gap rules: what a project is missing, each finding citing the guidance it rests on.
//!
//! A rule is a [`Rule`]: an id, its citations, and a check over an [`Input`]. The built-in rules
//! are [`BUILTIN`]; a module adds its own by passing them to [`evaluate`] next to these, so the
//! list stays explicit and the core never depends on a module (a module's rule keeps the facts
//! it needs in its own value).

use std::collections::BTreeSet;

use engawa_core::names::GatewayRef;
use engawa_graph::{Snapshot, ingress_backends};
use engawa_model::{
    Citation, Fix, FixKind, Gap, HttpRoute, Message, Origin, Project, Service, Severity,
    WorkloadKind,
};

/// What a rule may look at.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    /// The project, as projected.
    pub project: &'a Project,
    /// The namespace's objects as read: what a route names may belong to no service, so it is
    /// not in the projection.
    pub snapshot: &'a Snapshot,
    /// The namespace's `HTTPRoutes`.
    pub http_routes: &'a [HttpRoute],
    /// Whether the namespace's Services could be listed. When they could not,
    /// `snapshot.services` is empty because the read was refused, not because there are none,
    /// and no rule says a Service is missing.
    pub services_readable: bool,
    /// Facts about the cluster the project runs in.
    pub cluster: &'a ClusterTraits,
}

/// Facts about the cluster that rules depend on, found by whoever builds the [`Input`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClusterTraits {
    /// Gateway API (`gateway.networking.k8s.io` `HTTPRoute`) is served and a Gateway exists.
    pub gateway_api: bool,
    /// Ingress classes whose controller is retired, such as ingress-nginx's `nginx`.
    pub retired_ingress_classes: BTreeSet<String>,
    /// The Gateways of every namespace; `None` when they could not be listed, and then no rule
    /// says a Gateway is missing.
    pub gateways: Option<Vec<GatewayRef>>,
}

/// One gap rule.
pub trait Rule: Sync {
    /// The stable id, such as `resource-limits`.
    fn id(&self) -> &'static str;
    /// The guidance it rests on.
    fn citations(&self) -> Vec<Citation>;
    /// Its findings in `input`.
    fn check(&self, input: &Input<'_>) -> Vec<Gap>;
}

/// The rules engawa always runs.
pub static BUILTIN: [&dyn Rule; 4] = [
    &ResourceLimits,
    &HealthChecks,
    &IngressToHttpRoute,
    &DanglingRoute,
];

/// Every finding of `rules` in `input`: warnings first, then by rule, subject and service.
#[must_use]
pub fn evaluate(rules: &[&dyn Rule], input: &Input<'_>) -> Vec<Gap> {
    let mut gaps: Vec<Gap> = rules.iter().flat_map(|rule| rule.check(input)).collect();
    gaps.sort_by(|a, b| {
        (a.severity, &a.rule, &a.subject, &a.service)
            .cmp(&(b.severity, &b.rule, &b.subject, &b.service))
    });
    gaps
}

fn cite(title: &str, url: &str) -> Citation {
    Citation {
        title: title.to_owned(),
        url: url.to_owned(),
    }
}

fn plan(template: &str, params: &[(&str, &str)]) -> Fix {
    Fix {
        kind: FixKind::Plan,
        id: template.to_owned(),
        params: params
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    }
}

/// Long-running workloads: what a Service routes traffic to and the kubelet keeps restarting.
const fn long_running(service: &Service) -> bool {
    matches!(
        service.workload.kind,
        WorkloadKind::Deployment | WorkloadKind::StatefulSet | WorkloadKind::DaemonSet
    )
}

// ── Resource limits ─────────────────────────────────────────────────────────────────────────────

/// A container without CPU and memory requests, or without a memory limit.
///
/// Requests are what the scheduler reserves; a container without them may land on a full node.
/// A memory limit bounds what one container can take from its neighbours before the kernel kills
/// it [k8s-resources]. A pod whose containers set none of them is `BestEffort`, the first evicted
/// under node pressure [k8s-qos], so that case is a warning; a partial setting is advice.
///
/// [k8s-resources]: https://kubernetes.io/docs/concepts/configuration/manage-resources-containers/
/// [k8s-qos]: https://kubernetes.io/docs/concepts/workloads/pods/pod-qos/
#[derive(Debug, Clone, Copy)]
pub struct ResourceLimits;

impl Rule for ResourceLimits {
    fn id(&self) -> &'static str {
        "resource-limits"
    }

    fn citations(&self) -> Vec<Citation> {
        vec![
            cite(
                "Resource Management for Pods and Containers",
                "https://kubernetes.io/docs/concepts/configuration/manage-resources-containers/",
            ),
            cite(
                "Pod Quality of Service Classes",
                "https://kubernetes.io/docs/concepts/workloads/pods/pod-qos/",
            ),
        ]
    }

    fn check(&self, input: &Input<'_>) -> Vec<Gap> {
        let mut gaps = Vec::new();
        for service in &input.project.services {
            for container in service.containers.iter().filter(|c| !c.init) {
                let resources = container.resources.clone().unwrap_or_default();
                let missing: Vec<&str> = [
                    ("requests.cpu", resources.requests.cpu.is_none()),
                    ("requests.memory", resources.requests.memory.is_none()),
                    ("limits.memory", resources.limits.memory.is_none()),
                ]
                .into_iter()
                .filter_map(|(field, absent)| absent.then_some(field))
                .collect();
                if missing.is_empty() {
                    continue;
                }
                let none = container.resources.is_none();
                gaps.push(Gap {
                    rule: self.id().to_owned(),
                    severity: if none {
                        Severity::Warning
                    } else {
                        Severity::Advice
                    },
                    subject: container.origin.clone(),
                    service: Some(service.id.clone()),
                    message: Message::new(if none {
                        "gap.resourceLimits.none"
                    } else {
                        "gap.resourceLimits.partial"
                    })
                    .with("container", &container.name)
                    .with("missing", missing.join(",")),
                    fix: plan(
                        "compute.resources",
                        &[("service", &service.id), ("container", &container.name)],
                    ),
                    citations: self.citations(),
                });
            }
        }
        gaps
    }
}

// ── Health checks ───────────────────────────────────────────────────────────────────────────────

/// A long-running container behind a Kubernetes Service without a readiness probe.
///
/// Without one, a pod receives traffic as soon as its containers start, before the application
/// can answer, and keeps receiving it while it cannot [k8s-probes] [k8s-probes-task]. Jobs and
/// workloads no Service selects are left out: nothing routes to them.
///
/// [k8s-probes]: https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#container-probes
/// [k8s-probes-task]: https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/
#[derive(Debug, Clone, Copy)]
pub struct HealthChecks;

impl Rule for HealthChecks {
    fn id(&self) -> &'static str {
        "health-checks"
    }

    fn citations(&self) -> Vec<Citation> {
        vec![
            cite(
                "Pod Lifecycle: Container probes",
                "https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#container-probes",
            ),
            cite(
                "Configure Liveness, Readiness and Startup Probes",
                "https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/",
            ),
        ]
    }

    fn check(&self, input: &Input<'_>) -> Vec<Gap> {
        let mut gaps = Vec::new();
        for service in input
            .project
            .services
            .iter()
            .filter(|s| long_running(s) && !s.endpoints.is_empty())
        {
            for container in service.containers.iter().filter(|c| !c.init) {
                let probes = container.probes.clone().unwrap_or_default();
                if probes.readiness.is_some() {
                    continue;
                }
                gaps.push(Gap {
                    rule: self.id().to_owned(),
                    severity: Severity::Warning,
                    subject: container.origin.clone(),
                    service: Some(service.id.clone()),
                    message: Message::new(if probes.liveness.is_some() {
                        "gap.healthChecks.livenessOnly"
                    } else {
                        "gap.healthChecks.none"
                    })
                    .with("container", &container.name),
                    fix: plan(
                        "probes.readiness",
                        &[("service", &service.id), ("container", &container.name)],
                    ),
                    citations: self.citations(),
                });
            }
        }
        gaps
    }
}

// ── Ingress to HTTPRoute ────────────────────────────────────────────────────────────────────────

/// An Ingress that should become an HTTPRoute.
///
/// The Ingress API is frozen; new routing features go to Gateway API [k8s-ingress]. An Ingress
/// whose class belongs to a retired controller is a warning: ingress-nginx stopped receiving
/// releases and security fixes in March 2026 [ingress-nginx-retirement]. Otherwise, when the
/// cluster serves Gateway API, moving is advice [gateway-migrating].
///
/// [k8s-ingress]: https://kubernetes.io/docs/concepts/services-networking/ingress/
/// [ingress-nginx-retirement]: https://kubernetes.io/blog/2025/11/11/ingress-nginx-retirement/
/// [gateway-migrating]: https://gateway-api.sigs.k8s.io/guides/getting-started/migrating-from-ingress/
#[derive(Debug, Clone, Copy)]
pub struct IngressToHttpRoute;

impl Rule for IngressToHttpRoute {
    fn id(&self) -> &'static str {
        "ingress-to-httproute"
    }

    fn citations(&self) -> Vec<Citation> {
        vec![
            cite(
                "Ingress",
                "https://kubernetes.io/docs/concepts/services-networking/ingress/",
            ),
            cite(
                "Ingress NGINX Retirement: What You Need to Know",
                "https://kubernetes.io/blog/2025/11/11/ingress-nginx-retirement/",
            ),
            cite(
                "Migrating from Ingress",
                "https://gateway-api.sigs.k8s.io/guides/getting-started/migrating-from-ingress/",
            ),
        ]
    }

    fn check(&self, input: &Input<'_>) -> Vec<Gap> {
        let mut seen = BTreeSet::new();
        let mut gaps = Vec::new();
        for service in &input.project.services {
            for route in &service.routes {
                // An Ingress shared by services is one finding.
                if !seen.insert(route.name.clone()) {
                    continue;
                }
                let retired = route
                    .class
                    .as_ref()
                    .filter(|class| input.cluster.retired_ingress_classes.contains(*class));
                let (severity, message) = match retired {
                    Some(class) => (
                        Severity::Warning,
                        Message::new("gap.ingressToHttpRoute.retired")
                            .with("ingress", &route.name)
                            .with("class", class),
                    ),
                    None if input.cluster.gateway_api => (
                        Severity::Advice,
                        Message::new("gap.ingressToHttpRoute.available")
                            .with("ingress", &route.name),
                    ),
                    None => continue,
                };
                gaps.push(Gap {
                    rule: self.id().to_owned(),
                    severity,
                    subject: Origin::object("Ingress", &route.name),
                    service: None,
                    message,
                    fix: plan("domains.httpRoute", &[("ingress", &route.name)]),
                    citations: self.citations(),
                });
            }
        }
        gaps
    }
}

// ── Routes that lead nowhere ────────────────────────────────────────────────────────────────────

/// An Ingress or `HTTPRoute` whose traffic cannot arrive anywhere. A project-level finding: such
/// a route reaches no workload, so it belongs to no service.
///
/// - **No Service.** An Ingress sends to the Services of its default backend and its rules' paths
///   [k8s-ingress]; an `HTTPRoute` to its `backendRefs`, of kind `Service` in the core group and
///   the route's namespace unless they say otherwise [gateway-backendref]. A backend no Service of
///   the namespace has, a mistyped name most often, leads nowhere. A backend of weight 0 is left
///   out: "no traffic should be forwarded for this entry" [gateway-httpbackendref], so a missing
///   Service there loses no request. One finding per route names every such Service. A service page
///   may say the same from the workload's side (`noService` when a workload has that name); the
///   project's list says it from the route's, so it is reported either way. When the Services could
///   not be listed ([`Input::services_readable`]), whether one exists is unknown and this case is
///   not checked; the refused read is named in the answer's `unreadable` list.
/// - **No Gateway.** A `parentRef` names a Gateway (group `gateway.networking.k8s.io` and kind
///   `Gateway` when absent) in the route's namespace unless it names another
///   [gateway-parentreference]. One no namespace has is a finding, said only when the Gateways
///   could be listed ([`ClusterTraits::gateways`]); a parent of another kind, such as a mesh's
///   Service, is not checked.
/// - **Not accepted.** A controller adds an entry to `status.parents` for each parent it manages,
///   and an empty list means the route is attached to no Gateway; a parent that rejects the route
///   sets `Accepted` to `False` [gateway-routeparentstatus]. With no entry, or every entry refused,
///   nothing serves the route. A parent with no `Accepted` condition, or one that is `Unknown`, is
///   not taken as a refusal. When a named Gateway is missing, that finding is the cause and this
///   one is not added: a controller does not list a parent it cannot resolve. A route just created
///   has no entry until its controller sees it, so it can show for a moment.
///
///   Only the route's Gateway parents are read. A parent may also be a Service, under the Mesh
///   conformance profile: "There are two kinds of parent resources with 'Core' support: Gateway
///   (Gateway conformance profile) Service (Mesh conformance profile, ClusterIP Services only)"
///   [gateway-parentreference]. A mesh route carries no domain, and its parent's acceptance is
///   the mesh's to report, so a route whose parents are all of another kind has no finding
///   here, and a Service parent's entry neither raises nor silences one. A route with no
///   `parentRefs` at all attaches to nothing and receives no traffic, a common omission: it is a
///   finding of its own (`gap.danglingRoute.noParent`) about `spec.parentRefs`, unless it asks
///   for default Gateways with `useDefaultGateways: All` (experimental): "if set, it will be
///   attached to any default Gateway supporting the named scope" [gateway-commonroutespec].
///   Such a route's Gateways are the ones that claimed it, so every Gateway entry of its
///   `status.parents` is read: all of them refused is the not-accepted finding, and no entry is
///   none, since whether a default Gateway exists at all is not known here. `None`, an unset
///   field or an unknown value attaches to no default Gateway.
///
///   Only entries for a Gateway `spec.parentRefs` names now count
///   ([`HttpRoute::gateway_parent_statuses`]): when a route moves to another Gateway, or from one
///   listener (`sectionName`) to the whole Gateway, the old entry's `Accepted=True` can stay in
///   the list until its controller cleans it up, and it says nothing about the new parent. Until
///   the new parent's controller adds its entry, the route reads as not accepted, as a new route
///   does.
///
///   A condition's `observedGeneration` older than the route's `metadata.generation` means "the
///   condition is out of date with respect to the current state of the instance"
///   [k8s-condition]. Such a condition is still read as it stands: it is the controller's last
///   word about that parent, and a route that was accepted mostly stays accepted after an edit,
///   so treating it as unknown would warn on every edit until the controller catches up. A stale
///   refusal is read as a refusal for the same reason.
///
/// [k8s-ingress]: https://kubernetes.io/docs/concepts/services-networking/ingress/
/// [k8s-condition]: https://github.com/kubernetes/apimachinery/blob/v0.34.0/pkg/apis/meta/v1/types.go
/// [gateway-backendref]: https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#backendref
/// [gateway-httpbackendref]: https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#httpbackendref
/// [gateway-parentreference]: https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#parentreference
/// [gateway-commonroutespec]: https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#commonroutespec
/// [gateway-routeparentstatus]: https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#routeparentstatus
#[derive(Debug, Clone, Copy)]
pub struct DanglingRoute;

impl Rule for DanglingRoute {
    fn id(&self) -> &'static str {
        "dangling-route"
    }

    fn citations(&self) -> Vec<Citation> {
        vec![
            cite(
                "Ingress",
                "https://kubernetes.io/docs/concepts/services-networking/ingress/",
            ),
            cite(
                "HTTPRoute",
                "https://gateway-api.sigs.k8s.io/reference/api-types/httproute/",
            ),
            cite(
                "Gateway API reference: RouteParentStatus",
                "https://gateway-api.sigs.k8s.io/reference/api-spec/1.4/spec/#routeparentstatus",
            ),
        ]
    }

    fn check(&self, input: &Input<'_>) -> Vec<Gap> {
        // `None`: the Services could not be listed, so none is known to be missing.
        let services: Option<BTreeSet<&str>> = input.services_readable.then(|| {
            input
                .snapshot
                .services
                .iter()
                .filter_map(|s| s.metadata.name.as_deref())
                .collect()
        });
        let services = services.as_ref();
        let mut gaps = Vec::new();
        for ingress in &input.snapshot.ingresses {
            let name = ingress.metadata.name.as_deref().unwrap_or_default();
            let backends = ingress_backends(ingress)
                .into_iter()
                .map(|(service, path)| (path, service));
            gaps.extend(self.no_service("Ingress", name, backends, services));
        }
        for route in input.http_routes {
            let name = route.name();
            gaps.extend(self.no_service("HTTPRoute", name, route.local_service_refs(), services));
            let namespace = route
                .metadata
                .namespace
                .as_deref()
                .unwrap_or(&input.project.namespace);
            gaps.extend(
                self.no_gateway(route, namespace, input.cluster.gateways.as_deref())
                    .or_else(|| self.not_accepted(route)),
            );
        }
        gaps
    }
}

impl DanglingRoute {
    fn gap(self, subject: Origin, message: Message, fix: Fix) -> Gap {
        Gap {
            rule: self.id().to_owned(),
            severity: Severity::Warning,
            subject,
            service: None,
            message,
            fix,
            citations: self.citations(),
        }
    }

    /// The finding for `backends` (field path, Service name) of the route `kind`/`name` that no
    /// Service in `services` has; the subject is the first of them. Nothing when the Services are
    /// unknown (`None`).
    fn no_service<'a>(
        self,
        kind: &str,
        name: &str,
        backends: impl Iterator<Item = (String, &'a str)>,
        services: Option<&BTreeSet<&str>>,
    ) -> Option<Gap> {
        let services = services?;
        let mut first = None;
        let mut missing: Vec<&str> = Vec::new();
        for (path, service) in backends.filter(|(_, s)| !services.contains(s)) {
            first.get_or_insert(path);
            if !missing.contains(&service) {
                missing.push(service);
            }
        }
        let path = first?;
        let existing = services.iter().copied().collect::<Vec<_>>().join(",");
        let mut params = vec![("kind", kind), ("route", name)];
        if !existing.is_empty() {
            params.push(("existing", existing.as_str()));
        }
        Some(
            self.gap(
                Origin::field(kind, name, path),
                Message::new("gap.danglingRoute.noService")
                    .with("kind", kind)
                    .with("route", name)
                    .with("services", missing.join(",")),
                plan("domains.backend", &params),
            ),
        )
    }

    /// The finding for the Gateways `route` names that are not among `gateways`; nothing when
    /// they could not be listed.
    fn no_gateway(
        self,
        route: &HttpRoute,
        namespace: &str,
        gateways: Option<&[GatewayRef]>,
    ) -> Option<Gap> {
        let gateways = gateways?;
        let mut first = None;
        let mut missing: Vec<String> = Vec::new();
        for (index, parent) in route.spec.parent_refs.iter().enumerate() {
            let Some((ns, name)) = parent.gateway(namespace) else {
                continue;
            };
            if gateways.iter().any(|g| g.namespace == ns && g.name == name) {
                continue;
            }
            first.get_or_insert(index);
            // A Gateway of the route's own namespace by its name, another by `namespace/name`.
            let shown = if ns == namespace {
                name.to_owned()
            } else {
                format!("{ns}/{name}")
            };
            if !missing.contains(&shown) {
                missing.push(shown);
            }
        }
        let index = first?;
        Some(
            self.gap(
                Origin::field(
                    "HTTPRoute",
                    route.name(),
                    format!("spec.parentRefs[{index}]"),
                ),
                Message::new("gap.danglingRoute.noGateway")
                    .with("route", route.name())
                    .with("gateways", missing.join(",")),
                plan("domains.parent", &[("route", route.name())]),
            ),
        )
    }

    /// The finding when the route names no parent at all, or names a Gateway parent and none
    /// lists it, or every one that does refused it.
    fn not_accepted(self, route: &HttpRoute) -> Option<Gap> {
        let parents: Vec<_> = if !route.spec.parent_refs.is_empty() {
            route.gateway_parents().next()?;
            route.gateway_parent_statuses().collect()
        } else if route.uses_default_gateways() {
            // The default Gateways that claimed it; none may exist, so no entry is no finding.
            let claimed: Vec<_> = route.default_gateway_statuses().collect();
            if claimed.is_empty() {
                return None;
            }
            claimed
        } else {
            return Some(self.gap(
                Origin::field("HTTPRoute", route.name(), "spec.parentRefs".to_owned()),
                Message::new("gap.danglingRoute.noParent").with("route", route.name()),
                plan("domains.parent", &[("route", route.name())]),
            ));
        };
        let refused: Vec<&str> = parents
            .iter()
            .filter_map(|p| p.accepted().filter(|c| c.status == "False"))
            .map(|c| c.reason.as_str())
            .collect();
        if !parents.is_empty() && refused.len() < parents.len() {
            return None;
        }
        let mut message = Message::new("gap.danglingRoute.notAccepted").with("route", route.name());
        if let Some(reason) = refused.first().filter(|r| !r.is_empty()) {
            message = message.with("reason", *reason);
        }
        Some(self.gap(
            Origin::field("HTTPRoute", route.name(), "status.parents".to_owned()),
            message,
            plan("domains.parent", &[("route", route.name())]),
        ))
    }
}

// ── Cluster ─────────────────────────────────────────────────────────────────────────────────────

/// IngressClass controllers whose project is retired: ingress-nginx, retired in March 2026
/// [ingress-nginx-retirement].
///
/// [ingress-nginx-retirement]: https://kubernetes.io/blog/2025/11/11/ingress-nginx-retirement/
pub const RETIRED_INGRESS_CONTROLLERS: [&str; 1] = ["k8s.io/ingress-nginx"];

/// An IngressClass, as the cluster rules read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngressClassFact {
    /// Its name.
    pub name: String,
    /// `spec.controller`.
    pub controller: String,
}

/// The names of `classes` whose controller is retired, for
/// [`ClusterTraits::retired_ingress_classes`].
#[must_use]
pub fn retired_classes(classes: &[IngressClassFact]) -> BTreeSet<String> {
    classes
        .iter()
        .filter(|c| RETIRED_INGRESS_CONTROLLERS.contains(&c.controller.as_str()))
        .map(|c| c.name.clone())
        .collect()
}

/// What the cluster itself is missing, apart from any project: an IngressClass served by a
/// retired controller. Its fix is to install a maintained controller from the catalogue.
#[must_use]
pub fn cluster_gaps(classes: &[IngressClassFact]) -> Vec<Gap> {
    let rule = IngressToHttpRoute;
    let mut gaps: Vec<Gap> = classes
        .iter()
        .filter(|c| RETIRED_INGRESS_CONTROLLERS.contains(&c.controller.as_str()))
        .map(|class| Gap {
            rule: "retired-ingress-controller".to_owned(),
            severity: Severity::Warning,
            subject: Origin::object("IngressClass", &class.name),
            service: None,
            message: Message::new("gap.retiredIngressController")
                .with("class", &class.name)
                .with("controller", &class.controller),
            fix: Fix {
                kind: FixKind::Install,
                id: "ingress/traefik".to_owned(),
                params: std::collections::BTreeMap::new(),
            },
            citations: rule.citations(),
        })
        .collect();
    gaps.sort_by(|a, b| a.subject.cmp(&b.subject));
    gaps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retired_controllers_class_is_a_cluster_gap_with_an_install_fix() {
        let classes = [
            IngressClassFact {
                name: "nginx".to_owned(),
                controller: "k8s.io/ingress-nginx".to_owned(),
            },
            IngressClassFact {
                name: "traefik".to_owned(),
                controller: "traefik.io/ingress-controller".to_owned(),
            },
        ];
        let gaps = cluster_gaps(&classes);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].subject, Origin::object("IngressClass", "nginx"));
        assert_eq!(gaps[0].fix.kind, FixKind::Install);
        assert_eq!(gaps[0].fix.id, "ingress/traefik");
        assert_eq!(
            retired_classes(&classes).into_iter().collect::<Vec<_>>(),
            ["nginx"]
        );
    }

    #[test]
    fn every_builtin_rule_cites_https_sources_and_has_a_unique_id() {
        let mut ids = BTreeSet::new();
        for rule in BUILTIN {
            assert!(ids.insert(rule.id()), "duplicate id {}", rule.id());
            let citations = rule.citations();
            assert!(!citations.is_empty(), "{} cites nothing", rule.id());
            for citation in citations {
                assert!(citation.url.starts_with("https://"), "{}", citation.url);
                assert_ne!(citation.title, "");
            }
        }
    }

    /// A module's rule joins the built-in ones through `evaluate`, carrying its own facts.
    #[test]
    fn a_module_rule_runs_beside_the_builtin_ones() {
        struct Always(&'static str);
        impl Rule for Always {
            fn id(&self) -> &'static str {
                "module-rule"
            }
            fn citations(&self) -> Vec<Citation> {
                vec![cite("x", "https://example.test")]
            }
            fn check(&self, _: &Input<'_>) -> Vec<Gap> {
                vec![Gap {
                    rule: self.id().to_owned(),
                    severity: Severity::Advice,
                    subject: Origin::object("PersistentVolumeClaim", self.0),
                    service: None,
                    message: Message::new("gap.module"),
                    fix: Fix {
                        kind: FixKind::Install,
                        id: "k8up".to_owned(),
                        params: std::collections::BTreeMap::new(),
                    },
                    citations: self.citations(),
                }]
            }
        }
        let project = Project {
            namespace: "demo".to_owned(),
            services: Vec::new(),
            resources: Vec::new(),
            relations: Vec::new(),
            connections: Vec::new(),
        };
        let cluster = ClusterTraits::default();
        let module = Always("data");
        let mut rules: Vec<&dyn Rule> = BUILTIN.to_vec();
        rules.push(&module);
        let gaps = evaluate(
            &rules,
            &Input {
                project: &project,
                snapshot: &Snapshot::default(),
                http_routes: &[],
                services_readable: true,
                cluster: &cluster,
            },
        );
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].rule, "module-rule");
    }
}
