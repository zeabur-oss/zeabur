use serde_json::json;

use super::*;

fn snapshot() -> Snapshot {
    Snapshot {
        namespace: "shop".to_owned(),
        deployments: vec![
            serde_json::from_value(json!({
                "metadata": {"name": "web", "namespace": "shop"},
                "spec": {"selector": {"matchLabels": {"app": "web"}},
                    "template": {"metadata": {"labels": {"app": "web"}},
                        "spec": {"containers": [{"name": "web", "image": "nginx:1.29",
                            "ports": [{"name": "http", "containerPort": 8080}]}]}}}
            }))
            .unwrap(),
        ],
        services: vec![
            serde_json::from_value(json!({
                "metadata": {"name": "web", "namespace": "shop"},
                "spec": {"selector": {"app": "web"},
                    "ports": [{"name": "http", "port": 80, "targetPort": "http"}]}
            }))
            .unwrap(),
        ],
        ingresses: vec![
            serde_json::from_value(json!({
                "metadata": {"name": "web", "namespace": "shop"},
                "spec": {"tls": [{"hosts": ["shop.example.test"], "secretName": "web-tls"}],
                    "rules": [{"host": "shop.example.test", "http": {"paths": [{"path": "/",
                        "pathType": "Prefix",
                        "backend": {"service": {"name": "web", "port": {"number": 81}}}}]}}]}
            }))
            .unwrap(),
        ],
        ..Snapshot::default()
    }
}

fn input<'a>(
    snapshot: &'a Snapshot,
    routes: &'a [Value],
    certificates: Certificates<'a>,
    resolved: &'a BTreeMap<String, Resolved>,
) -> TraceInput<'a> {
    TraceInput {
        snapshot,
        http_routes: routes,
        classes: Some(&[]),
        components: Some(&[]),
        certificates,
        resolved,
        services_readable: true,
        secrets_readable: true,
    }
}

fn statuses(trace: &RouteTrace) -> Vec<(HopStep, HopStatus)> {
    trace.hops.iter().map(|h| (h.step, h.status)).collect()
}

#[test]
fn a_certificate_that_is_not_ready_and_a_port_the_service_lacks_fail_their_hops() {
    let snapshot = snapshot();
    let project = zeabur_graph::project(&snapshot);
    let certificates = [json!({
        "metadata": {"name": "web-tls"},
        "spec": {"secretName": "web-tls"},
        "status": {"conditions": [{"type": "Ready", "status": "False", "reason": "Failed",
            "message": "challenge failed"}]}
    })];
    let resolved = BTreeMap::new();
    let input = input(
        &snapshot,
        &[],
        Certificates::Listed(&certificates),
        &resolved,
    );
    assert_eq!(
        names_to_resolve(&input, &project.services[0]),
        ["shop.example.test"]
    );
    let traces = trace(&input, &project.services[0]);
    assert_eq!(traces.len(), 2);
    assert_eq!(
        statuses(&traces[0]),
        [
            (HopStep::Dns, HopStatus::Unknown),
            (HopStep::Address, HopStatus::Unknown),
            (HopStep::Controller, HopStatus::Unknown),
            (HopStep::Certificate, HopStatus::Failed),
            (HopStep::Route, HopStatus::Unknown),
            (HopStep::Service, HopStatus::Failed),
            (HopStep::TargetPort, HopStatus::Unknown),
            (HopStep::Ready, HopStatus::Failed),
        ]
    );
    assert_eq!(traces[0].hops[5].expected.as_deref(), Some("81"));
    assert_eq!(traces[0].hops[5].saw.as_deref(), Some("80"));
    assert_eq!(
        statuses(&traces[1]),
        [
            (HopStep::Service, HopStatus::Ok),
            (HopStep::TargetPort, HopStatus::Ok),
            (HopStep::Ready, HopStatus::Failed),
        ]
    );
    let found = diagnoses(&traces, &project.services[0]);
    assert_eq!(
        found[0].cause.key,
        "diagnosis.unreachable.cause.certificateNotReady"
    );
    assert_eq!(found[0].fault.component.as_deref(), Some("cert-manager"));
}

#[test]
fn an_http_route_a_gateway_refused_fails_its_route_hop() {
    let snapshot = snapshot();
    let project = zeabur_graph::project(&snapshot);
    let routes = [json!({
        "apiVersion": "gateway.networking.k8s.io/v1", "kind": "HTTPRoute",
        "metadata": {"name": "web", "namespace": "shop"},
        "spec": {"parentRefs": [{"name": "public"}], "hostnames": ["api.example.test"],
            "rules": [{"backendRefs": [{"name": "web", "port": 80}]}]},
        "status": {"parents": [{"parentRef": {"name": "public"}, "controllerName": "x",
            "conditions": [{"type": "Accepted", "status": "False", "reason": "NotAllowedByListeners",
                "message": "no listener"}]}]}
    })];
    let resolved = BTreeMap::from([(
        "api.example.test".to_owned(),
        Resolved::Addresses(vec!["192.0.2.1".to_owned()]),
    )]);
    let input = input(&snapshot, &routes, Certificates::NotInstalled, &resolved);
    let traces = trace(&input, &project.services[0]);
    let route = traces
        .iter()
        .find(|t| t.address == "api.example.test")
        .unwrap();
    assert_eq!(
        statuses(route)[..2],
        [
            (HopStep::Dns, HopStatus::Ok),
            (HopStep::Route, HopStatus::Failed)
        ]
    );
    assert_eq!(
        route.first_failure().unwrap().message.params.get("reason"),
        Some(&"NotAllowedByListeners".to_owned())
    );
}

#[test]
fn a_domain_that_does_not_resolve_here_and_a_missing_certificate_secret_are_unknown_not_failed() {
    let mut snapshot = snapshot();
    snapshot.ingresses[0] = serde_json::from_value(json!({
        "metadata": {"name": "web", "namespace": "shop"},
        "spec": {"tls": [{"hosts": ["shop.example.test"], "secretName": "web-tls"}],
            "rules": [{"host": "shop.example.test", "http": {"paths": [{"path": "/",
                "pathType": "Prefix",
                "backend": {"service": {"name": "web", "port": {"number": 80}}}}]}}]}
    }))
    .unwrap();
    let project = zeabur_graph::project(&snapshot);
    let resolved = BTreeMap::from([(
        "shop.example.test".to_owned(),
        Resolved::NotFound("no such host".to_owned()),
    )]);
    let input = input(&snapshot, &[], Certificates::Listed(&[]), &resolved);
    let traces = trace(&input, &project.services[0]);
    let domain = &traces[0];
    assert_eq!(domain.hops[0].step, HopStep::Dns);
    assert_eq!(domain.hops[0].status, HopStatus::Unknown);
    assert_eq!(domain.hops[0].saw.as_deref(), Some("no such host"));
    let certificate = domain
        .hops
        .iter()
        .find(|h| h.step == HopStep::Certificate)
        .unwrap();
    assert_eq!(certificate.status, HopStatus::Unknown);
    assert_eq!(certificate.message.key, "trace.certificate.secretMissing");
    assert_eq!(domain.first_failure().map(|h| h.step), Some(HopStep::Ready));
    let causes: Vec<String> = diagnoses(&traces, &project.services[0])
        .into_iter()
        .map(|d| d.cause.key)
        .collect();
    assert!(
        causes
            .iter()
            .all(|c| c == "diagnosis.unreachable.cause.noReadyEndpoints")
    );
}

#[test]
fn hops_that_could_not_be_checked_make_no_diagnosis() {
    let snapshot = snapshot();
    let project = zeabur_graph::project(&snapshot);
    let unknown = RouteTrace {
        kind: AddressKind::Domain,
        address: "shop.example.test".to_owned(),
        via: Origin::object("Ingress", "web"),
        hops: vec![
            hop(HopStep::Dns, HopStatus::Unknown, "notFound").with("host", "shop.example.test"),
            hop(HopStep::Certificate, HopStatus::Unknown, "secretMissing"),
            hop(HopStep::Ready, HopStatus::Ok, "ok"),
        ],
        status: HopStatus::Unknown,
    };
    assert_eq!(diagnoses(&[unknown], &project.services[0]), []);
}
