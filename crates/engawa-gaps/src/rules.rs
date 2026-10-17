//! Gap rules: what a project is missing, each finding citing the guidance it rests on.
//!
//! A rule is a [`Rule`]: an id, its citations, and a check over an [`Input`]. The built-in rules
//! are [`BUILTIN`]; a module adds its own by passing them to [`evaluate`] next to these, so the
//! list stays explicit and the core never depends on a module (a module's rule keeps the facts
//! it needs in its own value).

use std::collections::BTreeSet;

use engawa_model::{
    Citation, Fix, FixKind, Gap, Message, Origin, Project, Service, Severity, WorkloadKind,
};

/// What a rule may look at.
#[derive(Debug, Clone, Copy)]
pub struct Input<'a> {
    /// The project, as projected.
    pub project: &'a Project,
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

/// The rules Engawa always runs.
pub static BUILTIN: [&dyn Rule; 3] = [&ResourceLimits, &HealthChecks, &IngressToHttpRoute];

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
                cluster: &cluster,
            },
        );
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].rule, "module-rule");
    }
}
