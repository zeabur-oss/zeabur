//! The way traffic travels to a service, hop by hop: for each domain an Ingress or `HTTPRoute`
//! routes to it, and for the private address of each of its Services.
//!
//! A domain's hops, in the order a request takes them:
//!
//! 1. **DNS.** The domain resolves, looked up from where Zeabur runs. A name that does not resolve
//!    there is unknown, not failed: split-horizon DNS and resolvers that cannot reach the public
//!    internet answer differently from the browsers that use the domain.
//! 2. **Address.** One of those addresses is the route's: an Ingress's `status.loadBalancer`, where
//!    its controller writes the address it serves on [k8s-ingress]. Other addresses are not a
//!    failure: a CDN or proxy in front of the cluster answers with its own, so that is unknown.
//! 3. **Controller.** The controller of the Ingress's class has a ready replica (see
//!    [`crate::incidents`]); an `ingressClassName` no IngressClass has fails.
//! 4. **Certificate.** For a host listed under `spec.tls`: cert-manager's `Certificate` for its
//!    `secretName` is `Ready` [cert-manager-certificate]; one still being issued is unknown, not
//!    failed. Without such a `Certificate`, the Secret is not checked further, and a missing one is
//!    unknown too: the controller then serves its default certificate, which may be a valid
//!    wildcard Zeabur cannot read.
//! 5. **Route.** An Ingress is taken up once its controller writes an address into its status; an
//!    `HTTPRoute` once a Gateway sets `Accepted` (and `ResolvedRefs` is not `False`)
//!    [gateway-route-status]. Ingress controllers that never write status leave this unknown.
//! 6. **Service.** The Service the route names exists, has the port it names, and selects this
//!    service's pods [k8s-service].
//! 7. **Target port.** The Service's `targetPort` is a port of the containers, as the diagnosis
//!    reads it.
//! 8. **Ready.** At least one pod is ready; a Service sends only to ready pods.
//!
//! A domain with a host also gets a [`DomainCheck`]: the address its DNS should point to, whether
//! it does, and its certificate, for a page that says what to set up rather than which hop failed.
//!
//! The private address `<service>.<namespace>.svc.cluster.local` has the last three. Whether the
//! program listens on all interfaces rather than `127.0.0.1` is not traced: it can only be seen
//! from inside the container.
//!
//! [k8s-ingress]: https://kubernetes.io/docs/reference/kubernetes-api/service-resources/ingress-v1/#IngressStatus
//! [cert-manager-certificate]: https://cert-manager.io/docs/usage/certificate/
//! [gateway-route-status]: https://gateway-api.sigs.k8s.io/reference/spec/#routeconditiontype
//! [k8s-service]: https://kubernetes.io/docs/concepts/services-networking/service/

use std::collections::{BTreeMap, BTreeSet};

use k8s_openapi::api::core::v1::Service as KubeService;
use k8s_openapi::api::networking::v1::{Ingress, IngressServiceBackend};
use serde_json::Value;
use zeabur_graph::Snapshot;
use zeabur_model::{
    AddressKind, CertificateCheck, CertificateState, Diagnosis, DiagnosisKind, DnsState,
    DomainCheck, Health, Hop, HopStatus, HopStep, HttpRoute, Message, Origin, RouteTrace, Service,
};

use crate::diagnose::fault::fault;
use crate::diagnose::traffic::{declared, describe, target_matches, target_of};
use crate::{Component, ComponentKind, IngressClassFact};

/// What looking up a name gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// Its addresses.
    Addresses(Vec<String>),
    /// The lookup failed, with the resolver's words.
    NotFound(String),
    /// No answer in time, or the lookup was not made.
    Unknown,
}

/// cert-manager's `Certificates` of the namespace, as far as they were read.
#[derive(Debug, Clone, Copy)]
pub enum Certificates<'a> {
    /// Listed.
    Listed(&'a [Value]),
    /// cert-manager is not installed.
    NotInstalled,
    /// The person may not list them.
    Unreadable,
}

/// What a trace reads besides the service.
#[derive(Debug, Clone, Copy)]
pub struct TraceInput<'a> {
    /// The namespace as read.
    pub snapshot: &'a Snapshot,
    /// Its `HTTPRoutes`.
    pub http_routes: &'a [Value],
    /// The `IngressClasses`; `None` when they could not be listed.
    pub classes: Option<&'a [IngressClassFact]>,
    /// The cluster's components; `None` when their workloads could not be listed.
    pub components: Option<&'a [Component]>,
    /// The namespace's Certificates.
    pub certificates: Certificates<'a>,
    /// The namespace's ACME `Challenges`; empty when none is under way or they were not read.
    pub challenges: &'a [Value],
    /// The names looked up, by name.
    pub resolved: &'a BTreeMap<String, Resolved>,
    /// Whether the Services could be listed.
    pub services_readable: bool,
    /// Whether the Secrets could be listed.
    pub secrets_readable: bool,
}

/// The names a trace of `service` looks up: its domains, and the hostnames its Ingresses' status
/// names as their address.
#[must_use]
pub fn names_to_resolve(input: &TraceInput<'_>, service: &Service) -> Vec<String> {
    let targets = targets(service);
    let mut out = BTreeSet::new();
    for ingress in &input.snapshot.ingresses {
        let routes = ingress_routes(ingress, &targets);
        if routes.is_empty() {
            continue;
        }
        out.extend(
            routes
                .into_iter()
                .filter_map(|r| r.host)
                .filter(|h| !h.contains('*')),
        );
        out.extend(
            entry(input, ingress)
                .into_iter()
                .filter(|a| a.parse::<std::net::IpAddr>().is_err()),
        );
    }
    for route in http_routes(input.http_routes) {
        if route.local_services().any(|s| targets.contains(s)) {
            out.extend(
                route
                    .spec
                    .hostnames
                    .iter()
                    .filter(|h| !h.contains('*'))
                    .cloned(),
            );
        }
    }
    out.into_iter().collect()
}

/// The names a route to `service` may use: its Services', and its workload's for a backend whose
/// Service is missing.
fn targets(service: &Service) -> BTreeSet<&str> {
    service
        .endpoint_names
        .iter()
        .map(String::as_str)
        .chain([service.workload.name.as_str()])
        .collect()
}

fn http_routes(values: &[Value]) -> Vec<HttpRoute> {
    values
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect()
}

/// One host of an Ingress and the backend it sends to.
struct IngressRoute<'a> {
    host: Option<String>,
    backend: &'a IngressServiceBackend,
    field: String,
}

fn ingress_routes<'a>(ingress: &'a Ingress, targets: &BTreeSet<&str>) -> Vec<IngressRoute<'a>> {
    let Some(spec) = ingress.spec.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, rule) in spec.rules.iter().flatten().enumerate() {
        for (path, entry) in rule.http.iter().flat_map(|h| h.paths.iter().enumerate()) {
            let Some(backend) = entry.backend.service.as_ref() else {
                continue;
            };
            if targets.contains(backend.name.as_str())
                && seen.insert((rule.host.clone(), backend.name.as_str()))
            {
                out.push(IngressRoute {
                    host: rule.host.clone(),
                    backend,
                    field: format!("spec.rules[{index}].http.paths[{path}].backend.service"),
                });
            }
        }
    }
    if let Some(backend) = spec
        .default_backend
        .as_ref()
        .and_then(|b| b.service.as_ref())
        && targets.contains(backend.name.as_str())
        && seen.insert((None, backend.name.as_str()))
    {
        out.push(IngressRoute {
            host: None,
            backend,
            field: "spec.defaultBackend.service".to_owned(),
        });
    }
    out
}

fn load_balancer(ingress: &Ingress) -> Vec<String> {
    ingress
        .status
        .as_ref()
        .and_then(|s| s.load_balancer.as_ref())
        .and_then(|lb| lb.ingress.as_ref())
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ip.clone().or_else(|| entry.hostname.clone()))
        .collect()
}

fn class_of(ingress: &Ingress) -> Option<String> {
    ingress
        .spec
        .as_ref()
        .and_then(|s| s.ingress_class_name.clone())
        .or_else(|| {
            ingress
                .metadata
                .annotations
                .as_ref()?
                .get("kubernetes.io/ingress.class")
                .cloned()
        })
}

/// The addresses a domain of `ingress` should point to: its own status, or, until its controller
/// writes one, the status of the namespace's other Ingresses of the same class.
fn entry(input: &TraceInput<'_>, ingress: &Ingress) -> Vec<String> {
    let own = load_balancer(ingress);
    if !own.is_empty() {
        return own;
    }
    let class = class_of(ingress);
    let mut out = BTreeSet::new();
    for other in &input.snapshot.ingresses {
        if class_of(other) == class {
            out.extend(load_balancer(other));
        }
    }
    out.into_iter().collect()
}

/// The addresses the namespace's Ingresses are served on, for a domain not added yet.
#[must_use]
pub fn ingress_addresses(snapshot: &Snapshot) -> Vec<String> {
    snapshot
        .ingresses
        .iter()
        .flat_map(load_balancer)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// `target` and the addresses its hostnames resolved to: any of them is the cluster's.
fn wanted(input: &TraceInput<'_>, target: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for address in target {
        out.insert(address.clone());
        if let Some(Resolved::Addresses(more)) = input.resolved.get(address) {
            out.extend(more.iter().cloned());
        }
    }
    out
}

/// What setting up `host` needs, for a host that is not a wildcard.
fn domain_check(
    input: &TraceInput<'_>,
    ingress: &Ingress,
    host: &str,
    certificate: Option<&CertificateCheck>,
) -> Option<DomainCheck> {
    if host.contains('*') {
        return None;
    }
    let target = entry(input, ingress);
    let (dns, resolved) = dns_state(input, host, &target);
    Some(DomainCheck {
        target,
        dns,
        resolved,
        certificate: certificate.cloned(),
    })
}

fn dns_state(input: &TraceInput<'_>, host: &str, target: &[String]) -> (DnsState, Vec<String>) {
    match input.resolved.get(host) {
        Some(Resolved::Addresses(addresses)) if !addresses.is_empty() => {
            let wanted = wanted(input, target);
            let state = if wanted.is_empty() {
                DnsState::Unknown
            } else if addresses.iter().any(|a| wanted.contains(a)) {
                DnsState::PointsHere
            } else {
                DnsState::Elsewhere
            };
            (state, addresses.clone())
        }
        Some(Resolved::NotFound(_)) => (DnsState::Missing, Vec::new()),
        _ => (DnsState::Unknown, Vec::new()),
    }
}

fn hop(step: HopStep, status: HopStatus, variant: &str) -> Hop {
    Hop {
        step,
        status,
        expected: None,
        saw: None,
        message: Message::new(&format!("trace.{}.{variant}", step_key(step))),
        evidence: Vec::new(),
    }
}

const fn step_key(step: HopStep) -> &'static str {
    match step {
        HopStep::Dns => "dns",
        HopStep::Address => "address",
        HopStep::Controller => "controller",
        HopStep::Certificate => "certificate",
        HopStep::Route => "route",
        HopStep::Service => "service",
        HopStep::TargetPort => "targetPort",
        HopStep::Ready => "ready",
    }
}

trait HopExt {
    fn with(self, name: &str, value: impl Into<String>) -> Self;
    fn expected(self, value: impl Into<String>) -> Self;
    fn saw(self, value: impl Into<String>) -> Self;
    fn evidence(self, origin: Origin) -> Self;
}

impl HopExt for Hop {
    fn with(mut self, name: &str, value: impl Into<String>) -> Self {
        self.message = self.message.with(name, value);
        self
    }
    fn expected(mut self, value: impl Into<String>) -> Self {
        self.expected = Some(value.into());
        self
    }
    fn saw(mut self, value: impl Into<String>) -> Self {
        self.saw = Some(value.into());
        self
    }
    fn evidence(mut self, origin: Origin) -> Self {
        self.evidence.push(origin);
        self
    }
}

/// Every way to `service`: its domains first, by route and host, then its private addresses.
#[must_use]
pub fn trace(input: &TraceInput<'_>, service: &Service) -> Vec<RouteTrace> {
    let targets = targets(service);
    let mut out = Vec::new();
    for ingress in &input.snapshot.ingresses {
        let name = ingress.metadata.name.as_deref().unwrap_or_default();
        for route in ingress_routes(ingress, &targets) {
            let host = route.host.clone();
            let mut hops = Vec::new();
            let resolved = host.as_deref().and_then(|h| dns(input, h, &mut hops));
            hops.push(address(input, ingress, resolved.as_deref()));
            hops.push(controller(input, ingress));
            let certified = host.as_deref().and_then(|h| certificate(input, ingress, h));
            let domain = host
                .as_deref()
                .and_then(|h| domain_check(input, ingress, h, certified.as_ref().map(|(_, c)| c)));
            hops.extend(certified.map(|(hop, _)| hop));
            let lb = load_balancer(ingress);
            hops.push(if lb.is_empty() {
                hop(HopStep::Route, HopStatus::Unknown, "noStatus").with("route", name)
            } else {
                hop(HopStep::Route, HopStatus::Ok, "ok")
                    .with("route", name)
                    .saw(lb.join(", "))
            });
            let origin = Origin::field("Ingress", name, route.field.clone());
            let port = route.backend.port.as_ref().map(|p| {
                p.number
                    .map_or_else(|| p.name.clone().unwrap_or_default(), |n| n.to_string())
            });
            backend_hops(
                input,
                service,
                &route.backend.name,
                port.as_deref(),
                &origin,
                &mut hops,
            );
            let mut trace = finish(
                AddressKind::Domain,
                host.unwrap_or_else(|| "*".to_owned()),
                Origin::object("Ingress", name),
                hops,
            );
            trace.domain = domain;
            out.push(trace);
        }
    }
    for route in http_routes(input.http_routes) {
        let backends: BTreeSet<&str> = route
            .local_services()
            .filter(|s| targets.contains(s))
            .collect();
        if backends.is_empty() {
            continue;
        }
        let name = route.name().to_owned();
        let hosts: Vec<Option<String>> = if route.spec.hostnames.is_empty() {
            vec![None]
        } else {
            route.spec.hostnames.iter().cloned().map(Some).collect()
        };
        for host in hosts {
            for backend in &backends {
                let mut hops = Vec::new();
                if let Some(h) = host.as_deref() {
                    dns(input, h, &mut hops);
                }
                hops.push(accepted(&route));
                let origin = Origin::field("HTTPRoute", &name, "spec.rules".to_owned());
                backend_hops(input, service, backend, None, &origin, &mut hops);
                out.push(finish(
                    AddressKind::Domain,
                    host.clone().unwrap_or_else(|| "*".to_owned()),
                    Origin::object("HTTPRoute", &name),
                    hops,
                ));
            }
        }
    }
    for endpoint in &service.endpoint_names {
        let mut hops = Vec::new();
        let origin = Origin::object("Service", endpoint);
        backend_hops(input, service, endpoint, None, &origin, &mut hops);
        out.push(finish(
            AddressKind::Private,
            format!("{endpoint}.{}.svc.cluster.local", input.snapshot.namespace),
            origin,
            hops,
        ));
    }
    out
}

fn finish(kind: AddressKind, address: String, via: Origin, hops: Vec<Hop>) -> RouteTrace {
    let status = if hops.iter().any(|h| h.status == HopStatus::Failed) {
        HopStatus::Failed
    } else if hops.iter().any(|h| h.status == HopStatus::Unknown) {
        HopStatus::Unknown
    } else {
        HopStatus::Ok
    };
    RouteTrace {
        kind,
        address,
        via,
        hops,
        status,
        domain: None,
    }
}

/// The DNS hop of `host`; its addresses when it resolved. A wildcard host has none.
fn dns(input: &TraceInput<'_>, host: &str, hops: &mut Vec<Hop>) -> Option<Vec<String>> {
    if host.contains('*') {
        return None;
    }
    let (out, addresses) = match input.resolved.get(host) {
        Some(Resolved::Addresses(addresses)) if !addresses.is_empty() => (
            hop(HopStep::Dns, HopStatus::Ok, "ok")
                .with("host", host)
                .saw(addresses.join(", ")),
            Some(addresses.clone()),
        ),
        Some(Resolved::NotFound(error)) => (
            hop(HopStep::Dns, HopStatus::Unknown, "notFound")
                .with("host", host)
                .saw(error.clone()),
            None,
        ),
        _ => (
            hop(HopStep::Dns, HopStatus::Unknown, "unknown").with("host", host),
            None,
        ),
    };
    hops.push(out);
    addresses
}

fn address(input: &TraceInput<'_>, ingress: &Ingress, resolved: Option<&[String]>) -> Hop {
    let name = ingress.metadata.name.as_deref().unwrap_or_default();
    let lb = load_balancer(ingress);
    let evidence = Origin::field("Ingress", name, "status.loadBalancer.ingress".to_owned());
    if lb.is_empty() {
        return hop(HopStep::Address, HopStatus::Unknown, "noStatus")
            .with("route", name)
            .evidence(evidence);
    }
    let Some(resolved) = resolved else {
        return hop(HopStep::Address, HopStatus::Unknown, "unresolved")
            .with("route", name)
            .expected(lb.join(", "))
            .evidence(evidence);
    };
    let wanted = wanted(input, &lb);
    let base = |status, variant| {
        hop(HopStep::Address, status, variant)
            .with("route", name)
            .expected(lb.join(", "))
            .saw(resolved.join(", "))
            .evidence(evidence.clone())
    };
    if resolved.iter().any(|a| wanted.contains(a)) {
        base(HopStatus::Ok, "ok")
    } else {
        base(HopStatus::Unknown, "elsewhere")
    }
}

fn controller(input: &TraceInput<'_>, ingress: &Ingress) -> Hop {
    let name = ingress.metadata.name.as_deref().unwrap_or_default();
    let class = class_of(ingress);
    let evidence = Origin::field("Ingress", name, "spec.ingressClassName".to_owned());
    let controller_name = match (&class, input.classes) {
        (_, None) => {
            return hop(HopStep::Controller, HopStatus::Unknown, "classesUnreadable")
                .with("route", name);
        }
        (Some(class), Some(classes)) => match classes.iter().find(|c| &c.name == class) {
            Some(found) => Some(found.controller.clone()),
            None => {
                return hop(HopStep::Controller, HopStatus::Failed, "classMissing")
                    .with("route", name)
                    .with("class", class)
                    .expected(class.clone())
                    .saw(
                        classes
                            .iter()
                            .map(|c| c.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                    .evidence(evidence);
            }
        },
        (None, Some(_)) => None,
    };
    let Some(components) = input.components else {
        return hop(HopStep::Controller, HopStatus::Unknown, "unreadable").with("route", name);
    };
    let serving: Vec<&Component> = components
        .iter()
        .filter(|c| c.kind == ComponentKind::IngressController)
        .filter(|c| {
            controller_name
                .as_ref()
                .is_none_or(|wanted| c.ingress_controllers.iter().any(|x| x == wanted))
        })
        .collect();
    if serving.is_empty() {
        return hop(HopStep::Controller, HopStatus::Unknown, "notRecognised")
            .with("route", name)
            .with("controller", controller_name.unwrap_or_default());
    }
    let ready = serving.iter().find(|c| !c.down());
    let Some(shown) = ready.copied().or_else(|| serving.first().copied()) else {
        return hop(HopStep::Controller, HopStatus::Unknown, "notRecognised").with("route", name);
    };
    let message = |status, variant| {
        hop(HopStep::Controller, status, variant)
            .with("route", name)
            .with("workload", &shown.workload.name)
            .with(
                "namespace",
                shown.workload.namespace.clone().unwrap_or_default(),
            )
            .expected("1")
            .saw(format!("{}/{}", shown.ready, shown.desired))
            .evidence(
                shown
                    .workload
                    .clone()
                    .with_field("status.readyReplicas".to_owned())
                    .with_value(shown.ready.to_string()),
            )
    };
    if ready.is_some() {
        message(HopStatus::Ok, "ok")
    } else {
        message(HopStatus::Failed, "down")
    }
}

const fn not_checked() -> CertificateCheck {
    CertificateCheck {
        state: CertificateState::NotChecked,
        name: None,
        reason: None,
        message: None,
        not_after: None,
    }
}

fn condition<'a>(certificate: &'a Value, kind: &str) -> Option<&'a Value> {
    certificate
        .pointer("/status/conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|c| c.get("type").and_then(Value::as_str) == Some(kind))
}

fn text(value: Option<&Value>, key: &str) -> String {
    value
        .and_then(|v| v.get(key))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The ACME challenge cert-manager runs for `host`, if one is under way.
fn challenge<'a>(input: &TraceInput<'a>, host: &str) -> Option<&'a Value> {
    input.challenges.iter().find(|c| {
        c.pointer("/spec/dnsName")
            .and_then(Value::as_str)
            .is_some_and(|name| {
                name == host
                    || name
                        .strip_prefix("*.")
                        .is_some_and(|base| host.split_once('.').is_some_and(|(_, r)| r == base))
            })
    })
}

/// The certificate hop of `host` and how its certificate stands, when `ingress` lists the host
/// for HTTPS.
///
/// cert-manager sets `Issuing` while it requests a certificate and `Ready` once one is valid; a
/// failed request leaves `Issuing` `False` with reason `Failed` until it retries
/// [cert-manager-conditions]. While it issues, the ACME `Challenge` for the host says what it
/// waits for, such as an HTTP-01 self check the domain does not pass yet.
///
/// [cert-manager-conditions]: https://cert-manager.io/docs/reference/api-docs/#cert-manager.io/v1.CertificateCondition
#[expect(
    clippy::too_many_lines,
    reason = "one hop, each state of cert-manager in turn"
)]
fn certificate(
    input: &TraceInput<'_>,
    ingress: &Ingress,
    host: &str,
) -> Option<(Hop, CertificateCheck)> {
    let name = ingress.metadata.name.as_deref().unwrap_or_default();
    let (index, tls) = ingress
        .spec
        .as_ref()?
        .tls
        .iter()
        .flatten()
        .enumerate()
        .find(|(_, tls)| tls.hosts.iter().flatten().any(|h| h == host))?;
    let field = format!("spec.tls[{index}].secretName");
    let Some(secret) = tls.secret_name.as_deref() else {
        return Some((
            hop(
                HopStep::Certificate,
                HopStatus::Unknown,
                "defaultCertificate",
            )
            .with("host", host)
            .with("route", name)
            .evidence(Origin::field("Ingress", name, field)),
            not_checked(),
        ));
    };
    let base = |status, variant| {
        hop(HopStep::Certificate, status, variant)
            .with("host", host)
            .with("route", name)
            .with("secret", secret)
    };
    if let Certificates::Listed(certificates) = input.certificates
        && let Some(certificate) = certificates
            .iter()
            .find(|c| c.pointer("/spec/secretName").and_then(Value::as_str) == Some(secret))
    {
        let certificate_name = certificate
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let ready = condition(certificate, "Ready");
        let issuing = condition(certificate, "Issuing");
        let origin = Origin::complete(
            "cert-manager.io/v1",
            "Certificate",
            Some(&input.snapshot.namespace),
            certificate_name,
        )
        .with_field("status.conditions[type=Ready]".to_owned());
        let not_after = certificate
            .pointer("/status/notAfter")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let check = |state, reason: String, message: String| CertificateCheck {
            state,
            name: Some(certificate_name.to_owned()),
            reason: (!reason.is_empty()).then_some(reason),
            message: (!message.is_empty()).then_some(message),
            not_after: not_after.clone(),
        };
        let challenge = challenge(input, host);
        let challenge_state = text(challenge.and_then(|c| c.get("status")), "state");
        let challenge_reason = text(challenge.and_then(|c| c.get("status")), "reason");
        let challenge_failed = matches!(challenge_state.as_str(), "invalid" | "errored");
        let issuing_now = text(issuing, "status") == "True";
        let (hop, check) = if text(ready, "status") == "True" {
            (
                base(HopStatus::Ok, "ok")
                    .with("until", not_after.clone().unwrap_or_default())
                    .saw("True"),
                check(CertificateState::Ready, String::new(), String::new()),
            )
        } else if issuing_now && challenge_failed {
            (
                base(HopStatus::Failed, "notReady")
                    .with("certificate", certificate_name)
                    .with("reason", &challenge_state)
                    .expected("True")
                    .saw(format!("{challenge_state}: {challenge_reason}")),
                check(CertificateState::Failed, challenge_state, challenge_reason),
            )
        } else if issuing_now {
            let (reason, message) = if challenge.is_some() {
                (challenge_state, challenge_reason)
            } else {
                (text(issuing, "reason"), text(issuing, "message"))
            };
            (
                base(HopStatus::Unknown, "issuing").with("certificate", certificate_name),
                check(CertificateState::Issuing, reason, message),
            )
        } else if text(ready, "status") == "False" {
            let source = if text(issuing, "status") == "False" {
                issuing
            } else {
                ready
            };
            let (reason, message) = (text(source, "reason"), text(source, "message"));
            (
                base(HopStatus::Failed, "notReady")
                    .with("certificate", certificate_name)
                    .with("reason", &reason)
                    .expected("True")
                    .saw(format!("False: {message}")),
                check(CertificateState::Failed, reason, message),
            )
        } else {
            (
                base(HopStatus::Unknown, "pending").with("certificate", certificate_name),
                check(CertificateState::Issuing, String::new(), String::new()),
            )
        };
        return Some((
            hop.evidence(origin.with_value(text(ready, "status"))),
            check,
        ));
    }
    if !input.secrets_readable {
        return Some((base(HopStatus::Unknown, "unreadable"), not_checked()));
    }
    let present = input
        .snapshot
        .secrets
        .iter()
        .any(|s| s.metadata.name.as_deref() == Some(secret));
    Some(if present {
        (
            base(HopStatus::Unknown, "notChecked").evidence(Origin::object("Secret", secret)),
            not_checked(),
        )
    } else {
        (
            base(HopStatus::Unknown, "secretMissing")
                .expected(secret)
                .evidence(Origin::field("Ingress", name, field)),
            not_checked(),
        )
    })
}

fn accepted(route: &HttpRoute) -> Hop {
    let name = route.name();
    let evidence = Origin::field("HTTPRoute", name, "status.parents".to_owned());
    let conditions: Vec<_> = route
        .gateway_parent_statuses()
        .flat_map(|p| &p.conditions)
        .collect();
    if let Some(refused) = conditions
        .iter()
        .find(|c| matches!(c.type_.as_str(), "Accepted" | "ResolvedRefs") && c.status == "False")
    {
        return hop(HopStep::Route, HopStatus::Failed, "notAccepted")
            .with("route", name)
            .with("condition", &refused.type_)
            .with("reason", &refused.reason)
            .expected("True")
            .saw(format!("False: {}", refused.message))
            .evidence(evidence);
    }
    if conditions
        .iter()
        .any(|c| c.type_ == "Accepted" && c.status == "True")
    {
        hop(HopStep::Route, HopStatus::Ok, "accepted").with("route", name)
    } else {
        hop(HopStep::Route, HopStatus::Unknown, "noStatus")
            .with("route", name)
            .evidence(evidence)
    }
}

fn kube_service<'a>(input: &TraceInput<'a>, name: &str) -> Option<&'a KubeService> {
    input
        .snapshot
        .services
        .iter()
        .find(|s| s.metadata.name.as_deref() == Some(name))
}

/// The Service, target port and readiness hops of a route sending to `backend` (on `port`, when
/// the route names one).
fn backend_hops(
    input: &TraceInput<'_>,
    service: &Service,
    backend: &str,
    port: Option<&str>,
    via: &Origin,
    hops: &mut Vec<Hop>,
) {
    let base = |status, variant| {
        hop(HopStep::Service, status, variant)
            .with("endpoint", backend)
            .with("service", &service.workload.name)
    };
    let object = kube_service(input, backend);
    let ports: Vec<_> = object
        .and_then(|s| s.spec.as_ref())
        .and_then(|s| s.ports.as_ref())
        .into_iter()
        .flatten()
        .collect();
    let selects = service.endpoint_names.iter().any(|n| n == backend);
    let chosen: Vec<zeabur_model::Port> = if !input.services_readable {
        hops.push(base(HopStatus::Unknown, "unreadable"));
        Vec::new()
    } else if object.is_none() {
        hops.push(
            base(HopStatus::Failed, "missing")
                .expected(backend)
                .evidence(via.clone()),
        );
        Vec::new()
    } else if let Some(wanted) = port.filter(|wanted| {
        !ports
            .iter()
            .any(|p| p.port.to_string() == *wanted || p.name.as_deref() == Some(*wanted))
    }) {
        hops.push(
            base(HopStatus::Failed, "portMissing")
                .with("port", wanted)
                .expected(wanted)
                .saw(
                    ports
                        .iter()
                        .map(|p| p.port.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
                .evidence(via.clone()),
        );
        Vec::new()
    } else if !selects {
        hops.push(
            base(HopStatus::Failed, "selectsOther").evidence(Origin::field(
                "Service",
                backend,
                "spec.selector".to_owned(),
            )),
        );
        Vec::new()
    } else {
        hops.push(base(HopStatus::Ok, "ok").evidence(Origin::object("Service", backend)));
        object
            .map_or_else(Vec::new, |o| zeabur_graph::endpoint(o).ports)
            .into_iter()
            .filter(|p| {
                port.is_none_or(|wanted| {
                    p.port.to_string() == wanted || p.name.as_deref() == Some(wanted)
                })
            })
            .collect()
    };
    hops.push(target_port(input, service, backend, &chosen));
    hops.push(ready(service));
}

fn target_port(
    input: &TraceInput<'_>,
    service: &Service,
    backend: &str,
    chosen: &[zeabur_model::Port],
) -> Hop {
    let workload = zeabur_graph::workloads(input.snapshot)
        .into_iter()
        .find(|w| w.kind() == service.workload.kind && w.name() == service.workload.name);
    let Some(workload) = workload else {
        return hop(HopStep::TargetPort, HopStatus::Unknown, "unknown").with("endpoint", backend);
    };
    if chosen.is_empty() {
        return hop(HopStep::TargetPort, HopStatus::Unknown, "notReached")
            .with("endpoint", backend);
    }
    let declared = declared(workload);
    for port in chosen {
        let target = target_of(port);
        if !target_matches(&target, &declared) {
            return hop(HopStep::TargetPort, HopStatus::Failed, "mismatch")
                .with("endpoint", backend)
                .with("targetPort", &target)
                .with("ports", describe(&declared))
                .expected(describe(&declared))
                .saw(target)
                .evidence(port.origin.clone());
        }
    }
    let targets: Vec<String> = chosen.iter().map(target_of).collect();
    if declared.is_empty() {
        return hop(HopStep::TargetPort, HopStatus::Unknown, "undeclared")
            .with("endpoint", backend)
            .saw(targets.join(", "));
    }
    hop(HopStep::TargetPort, HopStatus::Ok, "ok")
        .with("endpoint", backend)
        .expected(describe(&declared))
        .saw(targets.join(", "))
}

fn ready(service: &Service) -> Hop {
    let ready = service.pods.iter().filter(|p| p.ready).count();
    let total = service.pods.len();
    let desired = service.workload.replicas.map(|r| r.desired);
    let base = |status, variant| {
        hop(HopStep::Ready, status, variant)
            .with("service", &service.workload.name)
            .with("ready", ready.to_string())
            .with("pods", total.to_string())
            .saw(format!("{ready}/{total}"))
    };
    if desired == Some(0) {
        return base(HopStatus::Failed, "scaledToZero").expected("1");
    }
    if ready > 0 {
        return base(HopStatus::Ok, "ok");
    }
    if service.status.health == Health::Progress {
        return base(HopStatus::Unknown, "starting");
    }
    base(HopStatus::Failed, "noneReady").expected("1")
}

/// The diagnoses the traces make: the first failed hop of each, once per cause. A hop that could
/// not be checked is never one.
#[must_use]
pub fn diagnoses(traces: &[RouteTrace], service: &Service) -> Vec<Diagnosis> {
    let mut out: Vec<Diagnosis> = Vec::new();
    for trace in traces {
        let Some(hop) = trace.first_failure() else {
            continue;
        };
        let Some(variant) = cause_of(hop) else {
            continue;
        };
        let sentence = |part: &str| {
            let mut message = Message::new(&format!("diagnosis.unreachable.{part}.{variant}"))
                .with("service", &service.workload.name)
                .with("address", &trace.address);
            for (key, value) in &hop.message.params {
                message = message.with(key, value.clone());
            }
            message
        };
        let cause = sentence("cause");
        if out.iter().any(|d| d.cause == cause) {
            continue;
        }
        out.push(Diagnosis {
            kind: DiagnosisKind::Unreachable,
            container: String::new(),
            pods: Vec::new(),
            fault: fault(DiagnosisKind::Unreachable, &cause),
            cause,
            why: sentence("why"),
            fix: sentence("fix"),
            evidence: hop.evidence.clone(),
            last_seen: None,
            incident: (variant == "controllerDown")
                .then(|| "incidents/component/ingress-controller".to_owned()),
        });
    }
    out
}

/// The diagnosis cause a failed hop is.
fn cause_of(hop: &Hop) -> Option<&'static str> {
    let variant = hop.message.key.rsplit('.').next().unwrap_or_default();
    Some(match (hop.step, variant) {
        (HopStep::Controller, "classMissing") => "ingressClassMissing",
        (HopStep::Controller, _) => "controllerDown",
        (HopStep::Certificate, _) => "certificateNotReady",
        (HopStep::Route, _) => "routeNotAccepted",
        (HopStep::Service, "missing") => "noService",
        (HopStep::Service, "portMissing") => "backendPortMissing",
        (HopStep::Service, _) => "selectsOther",
        (HopStep::TargetPort, _) => "targetPort",
        (HopStep::Ready, "scaledToZero") => "scaledToZero",
        (HopStep::Ready, _) => "noReadyEndpoints",
        (HopStep::Dns | HopStep::Address, _) => return None,
    })
}

#[cfg(test)]
mod tests;
