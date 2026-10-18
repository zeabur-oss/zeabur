//! How a workload is reached.

use serde::{Deserialize, Serialize};

use crate::provenance::Provenance;
use crate::source::Origin;

/// The application protocol a Service port speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
pub enum PortProtocol {
    /// HTTP.
    #[serde(rename = "HTTP")]
    Http,
    /// HTTP over TLS.
    #[serde(rename = "HTTPS")]
    Https,
    /// gRPC.
    #[serde(rename = "gRPC")]
    Grpc,
    /// Plain TCP (also SCTP, which clients treat the same way).
    #[serde(rename = "TCP")]
    Tcp,
    /// UDP.
    #[serde(rename = "UDP")]
    Udp,
}

/// Which signal decided a [`PortProtocol`], so a client can tell a declared protocol from a
/// guessed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum InferredFrom {
    /// The port's `appProtocol`.
    AppProtocol,
    /// The port name's convention, such as `http` or `grpc-…`.
    Name,
    /// The transport protocol.
    Protocol,
}

/// A port's application protocol and how it was decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolInference {
    /// The protocol.
    pub protocol: PortProtocol,
    /// The signal that decided it.
    pub inferred_from: InferredFrom,
}

/// One port of a Kubernetes Service.
#[expect(
    clippy::struct_field_names,
    reason = "the field names are the Kubernetes ones"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    /// The port's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The Service port.
    pub port: i32,
    /// The target port, a number or a container port name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_port: Option<String>,
    /// The node port, for `NodePort` and `LoadBalancer` Services.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_port: Option<i32>,
    /// The application protocol and how it was inferred.
    #[serde(flatten)]
    pub protocol: ProtocolInference,
    /// The port in its Service.
    pub origin: Origin,
}

/// A Kubernetes Service selecting a workload's pods: how it is reached inside the cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    /// The Service's name.
    pub name: String,
    /// The Service type, such as `ClusterIP`.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    /// The cluster IP.
    #[serde(default, rename = "clusterIP", skip_serializing_if = "Option::is_none")]
    pub cluster_ip: Option<String>,
    /// The ports.
    pub ports: Vec<Port>,
    /// The tool that manages the Service, if any.
    pub provenance: Provenance,
    /// The Service object.
    pub origin: Origin,
}

/// An Ingress routing to a workload's Services: how it is reached from outside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// The Ingress's name.
    pub name: String,
    /// The hosts its rules match.
    pub hostnames: Vec<String>,
    /// The Services it routes to, sorted, without duplicates.
    pub backends: Vec<String>,
    /// Its class: `spec.ingressClassName`, else the older `kubernetes.io/ingress.class`
    /// annotation [k8s-ingress-class].
    ///
    /// [k8s-ingress-class]: https://kubernetes.io/docs/concepts/services-networking/ingress/#deprecated-annotation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Which requests reach which Service port: the default backend first, then each rule's
    /// paths in order. Every rule of the Ingress is listed, also those for other workloads'
    /// Services, so a client filters by `service`.
    pub rules: Vec<RouteRule>,
    /// The hosts its `spec.tls` serves over HTTPS, sorted, without duplicates. It says HTTPS is
    /// configured, not that the certificate is valid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tls_hosts: Vec<String>,
    /// The tool that manages the Ingress, if any.
    pub provenance: Provenance,
    /// The Ingress object.
    pub origin: Origin,
}

/// One backend of an Ingress: the requests that reach a Service port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RouteRule {
    /// The host it matches; absent matches every host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// The path it matches; absent for the default backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// `Prefix`, `Exact` or `ImplementationSpecific`; absent for the default backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_type: Option<String>,
    /// The backend Service's name.
    pub service: String,
    /// The backend Service port, a number or a port name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    /// The backend in its Ingress.
    pub origin: Origin,
}
