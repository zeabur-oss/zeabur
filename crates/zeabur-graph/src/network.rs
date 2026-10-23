//! Service ports and their application protocols; Ingress rules.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Service as KubeService, ServicePort};
use k8s_openapi::api::networking::v1::{Ingress, IngressServiceBackend};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use serde_json::Value;
use zeabur_adapters::error_pages;
use zeabur_model::{InferredFrom, Origin, Port, PortProtocol, ProtocolInference, RouteRule};

use crate::snapshot::name_of;

/// The application protocol of a Service port.
///
/// It comes from the strongest signal present: the declared `appProtocol`, then the convention
/// of the port's name (`http`, `https`, `grpc`, alone or followed by `-…`), then its transport
/// protocol.
///
/// Zeabur keeps no record of its own for this: the Service says it, or it is inferred.
#[must_use]
pub fn port_protocol(port: &ServicePort) -> ProtocolInference {
    let declared = port.app_protocol.as_deref().and_then(|protocol| {
        match protocol.to_ascii_lowercase().as_str() {
            "http" => Some(PortProtocol::Http),
            "https" => Some(PortProtocol::Https),
            "grpc" | "grpc-web" => Some(PortProtocol::Grpc),
            _ => None,
        }
    });
    if let Some(protocol) = declared {
        return ProtocolInference {
            protocol,
            inferred_from: InferredFrom::AppProtocol,
        };
    }
    let name = port
        .name
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let named = |prefix: &str| {
        name == prefix
            || name
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('-'))
    };
    let by_name = if named("http") {
        Some(PortProtocol::Http)
    } else if named("https") {
        Some(PortProtocol::Https)
    } else if named("grpc") {
        Some(PortProtocol::Grpc)
    } else {
        None
    };
    if let Some(protocol) = by_name {
        return ProtocolInference {
            protocol,
            inferred_from: InferredFrom::Name,
        };
    }
    ProtocolInference {
        protocol: if port.protocol.as_deref() == Some("UDP") {
            PortProtocol::Udp
        } else {
            PortProtocol::Tcp
        },
        inferred_from: InferredFrom::Protocol,
    }
}

/// The ports of a Service, in order.
#[must_use]
pub fn ports(service: &KubeService) -> Vec<Port> {
    let service_name = name_of(&service.metadata);
    service
        .spec
        .iter()
        .flat_map(|spec| spec.ports.iter().flatten())
        .map(|port| Port {
            name: port.name.clone().filter(|name| !name.is_empty()),
            port: port.port,
            target_port: port.target_port.as_ref().map(|target| match target {
                IntOrString::Int(number) => number.to_string(),
                IntOrString::String(name) => name.clone(),
            }),
            node_port: port.node_port,
            protocol: port_protocol(port),
            origin: Origin::field(
                "Service",
                service_name,
                format!(
                    "spec.ports[port={},protocol={}]",
                    port.port,
                    port.protocol.as_deref().unwrap_or("TCP")
                ),
            ),
        })
        .collect()
}

/// The Service backends of an Ingress: the default backend first, then each rule's paths in order.
/// Resource backends (`backend.resource`) are left out.
#[must_use]
pub(crate) fn route_rules(ingress: &Ingress) -> Vec<RouteRule> {
    let name = name_of(&ingress.metadata);
    let Some(spec) = ingress.spec.as_ref() else {
        return Vec::new();
    };
    let port = |backend: &IngressServiceBackend| {
        backend.port.as_ref().and_then(|port| {
            port.number
                .map(|number| number.to_string())
                .or_else(|| port.name.clone())
        })
    };
    let default = spec
        .default_backend
        .as_ref()
        .and_then(|backend| backend.service.as_ref())
        .map(|service| RouteRule {
            host: None,
            path: None,
            path_type: None,
            service: service.name.clone(),
            port: port(service),
            origin: Origin::field("Ingress", name, "spec.defaultBackend.service".to_owned()),
        });
    let rules = spec
        .rules
        .iter()
        .flatten()
        .enumerate()
        .flat_map(|(rule_index, rule)| {
            rule.http
                .iter()
                .flat_map(|http| http.paths.iter().enumerate())
                .filter_map(move |(path_index, entry)| {
                    entry.backend.service.as_ref().map(|service| RouteRule {
                        host: rule.host.clone().filter(|host| !host.is_empty()),
                        path: entry.path.clone(),
                        path_type: Some(entry.path_type.clone()),
                        service: service.name.clone(),
                        port: port(service),
                        origin: Origin::field(
                            "Ingress",
                            name,
                            format!(
                                "spec.rules[{rule_index}].http.paths[{path_index}].backend.service"
                            ),
                        ),
                    })
                })
        });
    let mut out: Vec<RouteRule> = default.into_iter().chain(rules).collect();
    shown_through_maintenance(ingress, &mut out);
    out
}

/// A hostname in maintenance keeps showing the Services it had, from
/// [`error_pages::MAINTENANCE_ANNOTATION`], so it stays with its workload.
fn shown_through_maintenance(ingress: &Ingress, rules: &mut [RouteRule]) {
    let Some(maintenance) = error_pages::maintenance(&ingress.metadata) else {
        return;
    };
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for rule in rules.iter_mut() {
        let Some(host) = rule.host.clone() else {
            continue;
        };
        let Some((key, backends)) = maintenance.get_key_value(&host) else {
            continue;
        };
        let index = seen.entry(key.as_str()).or_default();
        let backend = backends.get(*index).and_then(|b| b.get("service"));
        *index += 1;
        let Some(name) = backend.and_then(|s| s.get("name")).and_then(Value::as_str) else {
            continue;
        };
        name.clone_into(&mut rule.service);
        rule.port = backend
            .and_then(|s| s.get("port"))
            .and_then(|p| p.get("number").or_else(|| p.get("name")))
            .map(|p| p.as_str().map_or_else(|| p.to_string(), str::to_owned));
    }
}
