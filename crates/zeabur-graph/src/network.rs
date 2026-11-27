//! Service ports and their application protocols; Ingress rules.

use k8s_openapi::api::core::v1::{Node, Pod, Service as KubeService, ServicePort};
use k8s_openapi::api::networking::v1::{Ingress, IngressServiceBackend};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use zeabur_model::{
    Endpoint, InferredFrom, Origin, Port, PortAddress, PortAddressKind, PortProtocol,
    ProtocolInference, RouteRule,
};

use crate::snapshot::{Snapshot, name_of};

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
            addresses: addresses(service, port.port),
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

/// A host and a port as a client writes them: an IPv6 host in brackets [rfc3986-host].
///
/// [rfc3986-host]: https://www.rfc-editor.org/rfc/rfc3986#section-3.2.2
fn host_port(host: &str, port: i32) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// The addresses of one Service port that the Service alone gives: its name, its DNS name and,
/// for a `LoadBalancer` Service, each address in `status.loadBalancer.ingress`. Node addresses
/// need the nodes: [`with_node_addresses`].
fn addresses(service: &KubeService, port: i32) -> Vec<PortAddress> {
    let name = name_of(&service.metadata);
    let address = |kind, host: &str| PortAddress {
        kind,
        address: host_port(host, port),
    };
    let mut out = vec![address(PortAddressKind::Project, name)];
    if let Some(namespace) = service.metadata.namespace.as_deref() {
        out.push(address(
            PortAddressKind::Cluster,
            &format!("{name}.{namespace}.svc.cluster.local"),
        ));
    }
    let load_balancer =
        service.spec.as_ref().and_then(|s| s.type_.as_deref()) == Some("LoadBalancer");
    out.extend(
        service
            .status
            .iter()
            .filter(|_| load_balancer)
            .filter_map(|status| status.load_balancer.as_ref())
            .flat_map(|lb| lb.ingress.iter().flatten())
            .filter_map(|i| i.ip.as_deref().or(i.hostname.as_deref()))
            .map(|host| address(PortAddressKind::LoadBalancer, host)),
    );
    out
}

/// The address clients use for a node: its `ExternalIP`, else its `InternalIP`
/// [k8s-node-addresses].
///
/// [k8s-node-addresses]: https://kubernetes.io/docs/reference/node/node-status/#addresses
fn node_address(node: &Node) -> Option<&str> {
    let addresses = node.status.as_ref()?.addresses.as_ref()?;
    let of = |kind: &str| {
        addresses
            .iter()
            .find(|a| a.type_ == kind)
            .map(|a| a.address.as_str())
    };
    of("ExternalIP").or_else(|| of("InternalIP"))
}

/// Adds `<node address>:<node port>` to every port with a node port, one per node.
///
/// The nodes keep their order. kube-proxy opens a node port on every node [k8s-nodeport]. With
/// `externalTrafficPolicy: Local` a node drops the traffic unless a ready endpoint of the Service
/// runs on it [k8s-local], so only the nodes of the Service's ready pods are listed.
///
/// [k8s-nodeport]: https://kubernetes.io/docs/concepts/services-networking/service/#type-nodeport
/// [k8s-local]: https://kubernetes.io/docs/reference/networking/virtual-ips/#external-traffic-policy
pub fn with_node_addresses(endpoints: &mut [Endpoint], nodes: &[Node], snapshot: &Snapshot) {
    for endpoint in endpoints.iter_mut() {
        let local = snapshot
            .services
            .iter()
            .find(|s| name_of(&s.metadata) == endpoint.name)
            .and_then(|s| s.spec.as_ref())
            .filter(|spec| spec.external_traffic_policy.as_deref() == Some("Local"));
        let eligible = |node: &&Node| {
            local.is_none_or(|spec| {
                let name = name_of(&node.metadata);
                snapshot
                    .pods
                    .iter()
                    .filter(|pod| ready(pod))
                    .filter(|pod| {
                        pod.spec.as_ref().and_then(|s| s.node_name.as_deref()) == Some(name)
                    })
                    .any(|pod| selected(spec.selector.as_ref(), pod))
            })
        };
        let hosts: Vec<&str> = nodes
            .iter()
            .filter(eligible)
            .filter_map(node_address)
            .collect();
        for port in &mut endpoint.ports {
            let Some(node_port) = port.node_port else {
                continue;
            };
            port.addresses.extend(hosts.iter().map(|host| PortAddress {
                kind: PortAddressKind::Node,
                address: host_port(host, node_port),
            }));
        }
    }
}

/// Whether a pod is a ready endpoint: its `Ready` condition is `True` and it is not being
/// deleted.
fn ready(pod: &Pod) -> bool {
    pod.metadata.deletion_timestamp.is_none()
        && pod
            .status
            .iter()
            .flat_map(|s| s.conditions.iter().flatten())
            .any(|c| c.type_ == "Ready" && c.status == "True")
}

/// Whether a Service's selector selects a pod. A Service without a selector selects no pod.
fn selected(selector: Option<&std::collections::BTreeMap<String, String>>, pod: &Pod) -> bool {
    let Some(selector) = selector.filter(|s| !s.is_empty()) else {
        return false;
    };
    let labels = pod.metadata.labels.as_ref();
    selector
        .iter()
        .all(|(key, value)| labels.and_then(|l| l.get(key)) == Some(value))
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
    default.into_iter().chain(rules).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::project::endpoint;

    fn service(value: serde_json::Value) -> KubeService {
        serde_json::from_value(value).expect("a Service")
    }

    fn node(name: &str, addresses: &[(&str, &str)]) -> Node {
        let addresses: Vec<_> = addresses
            .iter()
            .map(|(kind, address)| json!({"type": kind, "address": address}))
            .collect();
        serde_json::from_value(
            json!({"metadata": {"name": name}, "status": {"addresses": addresses}}),
        )
        .expect("a Node")
    }

    fn kinds(port: &Port) -> Vec<(PortAddressKind, &str)> {
        port.addresses
            .iter()
            .map(|a| (a.kind, a.address.as_str()))
            .collect()
    }

    #[test]
    fn a_cluster_ip_service_is_reached_by_its_name_and_its_dns_name() {
        let api = service(json!({
            "metadata": {"name": "api", "namespace": "shop"},
            "spec": {"type": "ClusterIP", "ports": [{"port": 8080}]},
        }));
        assert_eq!(
            kinds(&ports(&api)[0]),
            [
                (PortAddressKind::Project, "api:8080"),
                (PortAddressKind::Cluster, "api.shop.svc.cluster.local:8080"),
            ]
        );
    }

    #[test]
    fn a_load_balancer_adds_each_ingress_address_with_the_service_port() {
        let api = service(json!({
            "metadata": {"name": "api-public-8080", "namespace": "shop"},
            "spec": {"type": "LoadBalancer", "ports": [{"port": 12345, "targetPort": 8080}]},
            "status": {"loadBalancer": {"ingress": [
                {"ip": "203.0.113.7"}, {"ip": "2001:db8::7"}, {"hostname": "lb.example.com"},
            ]}},
        }));
        let ports = ports(&api);
        let addresses = kinds(&ports[0]);
        assert_eq!(
            addresses[2..],
            [
                (PortAddressKind::LoadBalancer, "203.0.113.7:12345"),
                (PortAddressKind::LoadBalancer, "[2001:db8::7]:12345"),
                (PortAddressKind::LoadBalancer, "lb.example.com:12345"),
            ]
        );
    }

    #[test]
    fn a_node_port_is_on_each_node_by_its_external_ip_else_its_internal_ip() {
        let legacy = service(json!({
            "metadata": {"name": "legacy", "namespace": "shop"},
            "spec": {"type": "NodePort", "ports": [{"port": 80, "nodePort": 30080}]},
        }));
        let nodes = [
            node(
                "n1",
                &[("InternalIP", "10.0.0.5"), ("ExternalIP", "198.51.100.5")],
            ),
            node("n2", &[("Hostname", "n2"), ("InternalIP", "10.0.0.6")]),
            node("n3", &[("Hostname", "n3")]),
        ];
        let mut endpoints = [endpoint(&legacy)];
        with_node_addresses(&mut endpoints, &nodes, &Snapshot::default());
        assert_eq!(
            kinds(&endpoints[0].ports[0])[2..],
            [
                (PortAddressKind::Node, "198.51.100.5:30080"),
                (PortAddressKind::Node, "10.0.0.6:30080"),
            ]
        );
    }

    #[test]
    fn a_port_without_a_node_port_gets_no_node_address() {
        let api = service(json!({
            "metadata": {"name": "api", "namespace": "shop"},
            "spec": {"type": "ClusterIP", "ports": [{"port": 8080}]},
        }));
        let mut endpoints = [endpoint(&api)];
        with_node_addresses(
            &mut endpoints,
            &[node("n1", &[("InternalIP", "10.0.0.5")])],
            &Snapshot::default(),
        );
        assert!(
            endpoints[0].ports[0]
                .addresses
                .iter()
                .all(|a| a.kind != PortAddressKind::Node)
        );
    }

    fn pod(node: &str, ready: bool) -> Pod {
        let status = if ready { "True" } else { "False" };
        serde_json::from_value(json!({
            "metadata": {"name": format!("api-{node}"), "labels": {"app": "api"}},
            "spec": {"nodeName": node, "containers": []},
            "status": {"conditions": [{"type": "Ready", "status": status}]},
        }))
        .expect("a Pod")
    }

    #[test]
    fn with_a_local_traffic_policy_only_the_nodes_of_ready_pods_are_listed() {
        let api = service(json!({
            "metadata": {"name": "api-public-8080", "namespace": "shop"},
            "spec": {
                "type": "LoadBalancer",
                "externalTrafficPolicy": "Local",
                "selector": {"app": "api"},
                "ports": [{"port": 8080, "nodePort": 30080}],
            },
        }));
        let snapshot = Snapshot {
            services: vec![api.clone()],
            pods: vec![pod("n1", true), pod("n2", false)],
            ..Snapshot::default()
        };
        let nodes = [
            node("n1", &[("InternalIP", "10.0.0.5")]),
            node("n2", &[("InternalIP", "10.0.0.6")]),
            node("n3", &[("InternalIP", "10.0.0.7")]),
        ];
        let mut endpoints = [endpoint(&api)];
        with_node_addresses(&mut endpoints, &nodes, &snapshot);
        let node_addresses: Vec<_> = kinds(&endpoints[0].ports[0])
            .into_iter()
            .filter(|(kind, _)| *kind == PortAddressKind::Node)
            .collect();
        assert_eq!(node_addresses, [(PortAddressKind::Node, "10.0.0.5:30080")]);
    }
}
