//! A YAML snippet for each gap fix, for when engawa cannot write the fix itself: the person copies
//! it into their manifests (or into the object's YAML editor) and adjusts the values.
//!
//! Each snippet is the smallest fragment that fixes the gap, in the shape `kubectl apply` and a
//! Helm chart's templates use, with a comment naming the object it belongs to. Values engawa
//! cannot know (how much memory a container needs, which path answers health checks) are
//! starting points marked as such.

use engawa_core::names::GatewayRef;
use engawa_model::{Gap, PortProtocol, Project, Service};

/// The snippet that fixes `gap` in `project`.
///
/// `None` when the fix is not a snippet or the gap's subject is no longer in the project.
/// `gateways` are the Gateways a new `HTTPRoute` could attach to; the first one is used.
#[must_use]
pub fn snippet(gap: &Gap, project: &Project, gateways: &[GatewayRef]) -> Option<String> {
    let param = |name: &str| gap.fix.params.get(name).map(String::as_str);
    match gap.fix.id.as_str() {
        "compute.resources" => {
            let service = find_service(project, param("service")?)?;
            resources(service, param("container")?)
        }
        "probes.readiness" => {
            let service = find_service(project, param("service")?)?;
            readiness(project, service, param("container")?)
        }
        "domains.httpRoute" => http_route(project, param("ingress")?, gateways.first()),
        _ => None,
    }
}

/// YAML written one line at a time, at a fixed indent.
struct Lines {
    out: String,
}

impl Lines {
    const fn new() -> Self {
        Self { out: String::new() }
    }

    fn line(&mut self, depth: usize, text: &str) {
        self.out.push_str(&" ".repeat(depth));
        self.out.push_str(text);
        self.out.push('\n');
    }
}

/// `value` as a YAML double-quoted scalar, for text read from the cluster (an Ingress's paths
/// and hosts) that may hold any character: a quote, a backslash, a line break, a `*` that
/// would start an alias. Line breaks, tabs and every character outside YAML's printable set
/// are written as escapes (YAML 1.2.2 §5.1 and §5.7,
/// <https://yaml.org/spec/1.2.2/#57-escaped-characters>), so the value stays on one line.
fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // YAML 1.1 parsers also break lines at U+2028 and U+2029, and U+FEFF is a byte order
            // mark: escaped too.
            ' '..='~'
            | '\u{a0}'..='\u{2027}'
            | '\u{202a}'..='\u{d7ff}'
            | '\u{e000}'..='\u{fefe}'
            | '\u{ff00}'..='\u{fffd}'
            | '\u{10000}'.. => out.push(c),
            // Everything escaped is below U+10000, so four hex digits hold it.
            _ => {
                out.push_str("\\u");
                let code = u32::from(c);
                for shift in [12, 8, 4, 0] {
                    out.extend(char::from_digit((code >> shift) & 0xF, 16));
                }
            }
        }
    }
    out.push('"');
    out
}

fn find_service<'a>(project: &'a Project, id: &str) -> Option<&'a Service> {
    project.services.iter().find(|s| s.id == id)
}

/// The workload's pod template down to its `spec`, and how deep `containers:` sits below it (a
/// CronJob nests a Job template).
fn template_head(service: &Service) -> (Lines, usize) {
    let kind = service.workload.kind.as_str();
    let mut lines = Lines::new();
    lines.line(0, &format!("# {kind}/{}", service.workload.name));
    lines.line(0, "spec:");
    if kind == "CronJob" {
        lines.line(2, "jobTemplate:");
        lines.line(4, "spec:");
        lines.line(6, "template:");
        lines.line(8, "spec:");
        (lines, 10)
    } else {
        lines.line(2, "template:");
        lines.line(4, "spec:");
        (lines, 6)
    }
}

/// Requests and a memory limit for one container, keeping what it already sets.
///
/// The starting values are small on purpose: requests reserve node capacity, so a value larger
/// than the container uses wastes it [k8s-resources]. They should be raised from observed use.
///
/// [k8s-resources]: https://kubernetes.io/docs/concepts/configuration/manage-resources-containers/
fn resources(service: &Service, container: &str) -> Option<String> {
    let current = service
        .containers
        .iter()
        .find(|c| c.name == container)?
        .resources
        .clone()
        .unwrap_or_default();
    let (mut lines, d) = template_head(service);
    lines.line(d, "containers:");
    lines.line(d + 2, &format!("- name: {container}"));
    lines.line(
        d + 4,
        "# Starting points: adjust them to what the container uses.",
    );
    lines.line(d + 4, "resources:");
    lines.line(d + 6, "requests:");
    lines.line(
        d + 8,
        &format!("cpu: {}", current.requests.cpu.as_deref().unwrap_or("100m")),
    );
    lines.line(
        d + 8,
        &format!(
            "memory: {}",
            current.requests.memory.as_deref().unwrap_or("128Mi")
        ),
    );
    lines.line(d + 6, "limits:");
    if let Some(cpu) = current.limits.cpu.as_deref() {
        lines.line(d + 8, &format!("cpu: {cpu}"));
    }
    lines.line(
        d + 8,
        &format!(
            "memory: {}",
            current.limits.memory.as_deref().unwrap_or("256Mi")
        ),
    );
    Some(lines.out)
}

/// A readiness probe on the port the service's Kubernetes Service sends traffic to: an HTTP GET
/// when that port speaks HTTP, else a TCP connect [k8s-probes].
///
/// [k8s-probes]: https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/
fn readiness(project: &Project, service: &Service, container: &str) -> Option<String> {
    service.containers.iter().find(|c| c.name == container)?;
    let port = project.endpoints_of(service).flat_map(|e| &e.ports).next();
    let target = port.map_or_else(
        || "8080".to_owned(),
        |p| p.target_port.clone().unwrap_or_else(|| p.port.to_string()),
    );
    let http = port.is_some_and(|p| {
        matches!(
            p.protocol.protocol,
            PortProtocol::Http | PortProtocol::Https
        )
    });
    // A named port is quoted, so YAML keeps it a string.
    let target = if target.parse::<u16>().is_ok() {
        target
    } else {
        format!("\"{target}\"")
    };
    let (mut lines, d) = template_head(service);
    lines.line(d, "containers:");
    lines.line(d + 2, &format!("- name: {container}"));
    lines.line(d + 4, "readinessProbe:");
    if http {
        lines.line(
            d + 6,
            "# A path that answers 200 once the app can serve requests.",
        );
        lines.line(d + 6, "httpGet:");
        lines.line(d + 8, "path: /");
    } else {
        lines.line(d + 6, "tcpSocket:");
    }
    lines.line(d + 8, &format!("port: {target}"));
    lines.line(d + 6, "periodSeconds: 10");
    lines.line(d + 6, "failureThreshold: 3");
    Some(lines.out)
}

/// An `HTTPRoute` that routes what an Ingress routes: its hosts, paths and backends.
///
/// It follows the Gateway API's migration guide [gw-migrate]: `Prefix` paths become
/// `PathPrefix`, `Exact` stays `Exact`, and an implementation-specific path becomes a prefix.
///
/// [gw-migrate]: https://gateway-api.sigs.k8s.io/guides/getting-started/migrating-from-ingress/
fn http_route(project: &Project, ingress: &str, gateway: Option<&GatewayRef>) -> Option<String> {
    let route = project.route(ingress)?;
    let mut lines = Lines::new();
    lines.line(0, &format!("# Replaces Ingress/{ingress}"));
    lines.line(0, "apiVersion: gateway.networking.k8s.io/v1");
    lines.line(0, "kind: HTTPRoute");
    lines.line(0, "metadata:");
    lines.line(2, &format!("name: {ingress}"));
    lines.line(0, "spec:");
    lines.line(2, "parentRefs:");
    match gateway {
        Some(g) => {
            lines.line(4, &format!("- name: {}", g.name));
            lines.line(6, &format!("namespace: {}", g.namespace));
        }
        None => lines.line(
            4,
            "- name: <gateway>  # The Gateway that serves these hosts.",
        ),
    }
    let mut hosts: Vec<&str> = route
        .rules
        .iter()
        .filter_map(|r| r.host.as_deref())
        .collect();
    hosts.sort_unstable();
    hosts.dedup();
    if !hosts.is_empty() {
        lines.line(2, "hostnames:");
        for host in hosts {
            lines.line(4, &format!("- {}", quoted(host)));
        }
    }
    lines.line(2, "rules:");
    for rule in &route.rules {
        let kind = match rule.path_type.as_deref() {
            Some("Exact") => "Exact",
            _ => "PathPrefix",
        };
        lines.line(4, "- matches:");
        lines.line(8, "- path:");
        lines.line(12, &format!("type: {kind}"));
        lines.line(
            12,
            &format!("value: {}", quoted(rule.path.as_deref().unwrap_or("/"))),
        );
        lines.line(6, "backendRefs:");
        lines.line(8, &format!("- name: {}", rule.service));
        if let Some(port) = rule.port.as_deref() {
            // HTTPRoute takes a port number; an Ingress may name the Service's port instead.
            let number = port.parse::<u16>().ok().map(|n| n.to_string()).or_else(|| {
                project
                    .endpoint(&rule.service)
                    .into_iter()
                    .flat_map(|e| &e.ports)
                    .find(|p| p.name.as_deref() == Some(port))
                    .map(|p| p.port.to_string())
            });
            match number {
                Some(number) => lines.line(10, &format!("port: {number}")),
                None => lines.line(
                    10,
                    &format!(
                        "port: <number>  # The Ingress names port {port:?}; HTTPRoute needs its number."
                    ),
                ),
            }
        }
    }
    Some(lines.out)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// What a YAML parser reads back from `value: <quoted>`.
    fn read_back(text: &str) -> String {
        let yaml = format!("value: {}\n", quoted(text));
        assert_eq!(yaml.lines().count(), 1, "{yaml}");
        let map: BTreeMap<String, String> = serde_saphyr::from_str(&yaml).unwrap();
        map["value"].clone()
    }

    #[test]
    fn text_from_the_cluster_is_quoted_on_one_line_and_reads_back_unchanged() {
        for text in [
            "/",
            "/a\"b",
            "/a\nb: c\n  - d",
            "/back\\slash",
            "*.example.com",
            "/tab\there",
            "/del\u{7f}and\u{85}next\u{2028}line",
            "/bom\u{feff}",
            "/emoji \u{1f600}",
        ] {
            assert_eq!(read_back(text), text);
        }
        assert_eq!(quoted("/a\"b\nc"), "\"/a\\\"b\\nc\"");
    }
}
