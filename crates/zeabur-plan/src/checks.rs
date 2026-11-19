//! Checks for storage, probes and YAML edits, each one fixture operation (`fixtures/mutation`).

use k8s_openapi::api::core::v1::{ExecAction, GRPCAction, HTTPGetAction, Probe, TCPSocketAction};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeabur_model::kernel::dns::{is_dns_label, is_dns_subdomain};
use zeabur_model::kernel::quantity;

use crate::json::value_of;
use crate::rules::Refusal;

fn refusal(code: &str) -> Refusal {
    Refusal(code.to_owned())
}

// ── Storage ──────────────────────────────────────────────────────────────────────────────────

/// How the nodes may mount a new claim [k8s-access-modes].
///
/// Whether a mode works is up to the `StorageClass`'s provisioner (k3s's local-path provisioner
/// offers only `ReadWriteOnce` and `ReadWriteOncePod`), so no mode is refused here: the plan's
/// dry run and the provisioner decide.
///
/// [k8s-access-modes]: https://kubernetes.io/docs/concepts/storage/persistent-volumes/#access-modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[expect(
    clippy::enum_variant_names,
    reason = "the names Kubernetes gives the modes, serialized as they are"
)]
pub enum AccessMode {
    /// Read and written by the pods of one node.
    #[default]
    ReadWriteOnce,
    /// Read and written by one pod.
    ReadWriteOncePod,
    /// Read and written by pods on many nodes.
    ReadWriteMany,
    /// Read by pods on many nodes.
    ReadOnlyMany,
}

impl AccessMode {
    /// The mode as Kubernetes names it.
    #[must_use]
    pub const fn as_kubernetes(self) -> &'static str {
        match self {
            Self::ReadWriteOnce => "ReadWriteOnce",
            Self::ReadWriteOncePod => "ReadWriteOncePod",
            Self::ReadWriteMany => "ReadWriteMany",
            Self::ReadOnlyMany => "ReadOnlyMany",
        }
    }
}

/// A new volume for a workload: a PersistentVolumeClaim and where it is mounted.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct VolumeRequest {
    /// The volume's name in the pod.
    #[serde(default)]
    pub name: String,
    /// The claim's name; absent for `<workload>-<name>`. A claim that exists is never reused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_name: Option<String>,
    /// How the nodes may mount the claim; absent for `ReadWriteOnce`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_mode: Option<AccessMode>,
    /// The size, a Kubernetes quantity such as `5Gi`.
    pub size: String,
    /// Where the container sees it; absolute.
    pub mount_path: String,
    /// The `StorageClass`; absent for the cluster's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_class: Option<String>,
}

/// Checks a new volume against the container's mounts (`mutation.validateStorage`).
///
/// A mount path inside or around another would hide files of one behind the other, which the
/// kubelet allows but rarely means what was wanted.
///
/// # Errors
///
/// `missing_volume_name`, `invalid_volume_name`, `invalid_claim_name`, `invalid_quantity`,
/// `relative_mount_path`, `mount_path_in_use`, `mount_path_overlap`.
pub fn validate_storage(request: &VolumeRequest, mount_paths: &[&str]) -> Result<(), Refusal> {
    if request.name.is_empty() {
        return Err(refusal("missing_volume_name"));
    }
    if !is_dns_label(&request.name) {
        return Err(refusal("invalid_volume_name"));
    }
    if request
        .claim_name
        .as_deref()
        .is_some_and(|name| !is_dns_subdomain(name))
    {
        return Err(refusal("invalid_claim_name"));
    }
    if quantity::bytes(&request.size).is_none_or(|b| b == 0) {
        return Err(refusal("invalid_quantity"));
    }
    validate_mount_path(&request.mount_path, mount_paths)
}

/// Checks a mount path against the other mounts of the container.
///
/// # Errors
///
/// `relative_mount_path`, `mount_path_in_use`, `mount_path_overlap`.
pub fn validate_mount_path(path: &str, others: &[&str]) -> Result<(), Refusal> {
    if !path.starts_with('/') {
        return Err(refusal("relative_mount_path"));
    }
    let path = path.trim_end_matches('/');
    for other in others {
        let other = other.trim_end_matches('/');
        if other == path {
            return Err(refusal("mount_path_in_use"));
        }
        if is_under(path, other) || is_under(other, path) {
            return Err(refusal("mount_path_overlap"));
        }
    }
    Ok(())
}

fn is_under(path: &str, parent: &str) -> bool {
    parent.is_empty()
        || path
            .strip_prefix(parent)
            .is_some_and(|r| r.starts_with('/'))
}

// ── Probes ───────────────────────────────────────────────────────────────────────────────────

/// How a probe checks the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ProbeHandler {
    /// An HTTP `GET`; healthy on a status from 200 to 399.
    #[serde(rename = "HTTPGet")]
    HttpGet,
    /// A TCP connection.
    #[serde(rename = "TCPSocket")]
    TcpSocket,
    /// A gRPC health check (`grpc.health.v1.Health`).
    #[serde(rename = "GRPC")]
    Grpc,
    /// A command in the container; healthy on exit status 0.
    Exec,
}

/// A probe as a person asks for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProbeRequest {
    /// How it checks.
    pub handler: ProbeHandler,
    /// The port, for HTTP and TCP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<i32>,
    /// The path, for HTTP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// `HTTP` (the default) or `HTTPS`, for HTTP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    /// The command, for exec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// Seconds before the first check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_delay_seconds: Option<i32>,
    /// Seconds between checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_seconds: Option<i32>,
    /// Seconds a check may take.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<i32>,
    /// Failures in a row that count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_threshold: Option<i32>,
}

/// The `Probe` object for `request` on a container declaring `ports` (`mutation.probeConfig`).
///
/// A container that declares ports is probed on one of them; one that declares none may be
/// probed anywhere, since `ports` is informational [k8s-ports].
///
/// [k8s-ports]: https://kubernetes.io/docs/reference/kubernetes-api/workload-resources/pod-v1/#ports
///
/// # Errors
///
/// `missing_probe_port`, `invalid_probe_port`, `undeclared_probe_port`, `invalid_probe_path`,
/// `invalid_probe_scheme`, `missing_probe_command`, `invalid_probe_timing`.
pub fn probe_config(request: &ProbeRequest, ports: &[i32]) -> Result<Value, Refusal> {
    let checked_port = || -> Result<i32, Refusal> {
        let port = request.port.ok_or_else(|| refusal("missing_probe_port"))?;
        if !(1..=65_535).contains(&port) {
            return Err(refusal("invalid_probe_port"));
        }
        if !ports.is_empty() && !ports.contains(&port) {
            return Err(refusal("undeclared_probe_port"));
        }
        Ok(port)
    };
    let mut probe = match request.handler {
        ProbeHandler::HttpGet => {
            let port = checked_port()?;
            let path = request.path.clone().unwrap_or_else(|| "/".to_owned());
            if !path.starts_with('/') {
                return Err(refusal("invalid_probe_path"));
            }
            let scheme = match request.scheme.as_deref() {
                None | Some("HTTP") => None,
                Some("HTTPS") => Some("HTTPS".to_owned()),
                Some(_) => return Err(refusal("invalid_probe_scheme")),
            };
            Probe {
                http_get: Some(HTTPGetAction {
                    path: Some(path),
                    port: IntOrString::Int(port),
                    scheme,
                    ..HTTPGetAction::default()
                }),
                ..Probe::default()
            }
        }
        ProbeHandler::TcpSocket => Probe {
            tcp_socket: Some(TCPSocketAction {
                port: IntOrString::Int(checked_port()?),
                host: None,
            }),
            ..Probe::default()
        },
        ProbeHandler::Grpc => Probe {
            grpc: Some(GRPCAction {
                port: checked_port()?,
                service: None,
            }),
            ..Probe::default()
        },
        ProbeHandler::Exec => {
            let command = request
                .command
                .clone()
                .filter(|c| c.first().is_some_and(|first| !first.is_empty()))
                .ok_or_else(|| refusal("missing_probe_command"))?;
            Probe {
                exec: Some(ExecAction {
                    command: Some(command),
                }),
                ..Probe::default()
            }
        }
    };
    // The API's minimums [k8s-probe-fields].
    //
    // [k8s-probe-fields]: https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/#configure-probes
    let timings = [
        (request.initial_delay_seconds, 0),
        (request.period_seconds, 1),
        (request.timeout_seconds, 1),
        (request.failure_threshold, 1),
    ];
    if timings
        .iter()
        .any(|(value, min)| value.is_some_and(|v| v < *min))
    {
        return Err(refusal("invalid_probe_timing"));
    }
    probe.initial_delay_seconds = request.initial_delay_seconds;
    probe.period_seconds = request.period_seconds;
    probe.timeout_seconds = request.timeout_seconds;
    probe.failure_threshold = request.failure_threshold;
    Ok(value_of(&probe))
}

// ── Compute ──────────────────────────────────────────────────────────────────────────────────

/// The resources Zeabur sets on a container; others (GPUs, hugepages, …) are kept as they are.
pub const COMPUTE: [&str; 3] = ["cpu", "memory", "ephemeral-storage"];

/// Parses a quantity in one unit.
type Parse = fn(&str) -> Option<u64>;

/// Checks the CPU, memory and ephemeral storage values of a container's `resources`, and that
/// no request exceeds its limit; other resources are left to kube-apiserver.
///
/// # Errors
///
/// `invalid_quantity`, `request_above_limit`.
pub fn validate_resources(resources: &Value) -> Result<(), Refusal> {
    for section in ["requests", "limits"] {
        for (name, value) in resources
            .get(section)
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let text = value.as_str().unwrap_or_default();
            let parsed = match name.as_str() {
                "cpu" => quantity::millicores(text),
                "memory" | "ephemeral-storage" => quantity::bytes(text),
                _ => continue,
            };
            if parsed.is_none() {
                return Err(refusal("invalid_quantity"));
            }
        }
    }
    let parsers: [(&str, Parse); 3] = [
        ("cpu", quantity::millicores),
        ("memory", quantity::bytes),
        ("ephemeral-storage", quantity::bytes),
    ];
    for (name, parse) in parsers {
        let get = |section: &str| {
            resources
                .pointer(&format!("/{section}/{name}"))
                .and_then(Value::as_str)
                .and_then(parse)
        };
        if let (Some(request), Some(limit)) = (get("requests"), get("limits"))
            && request > limit
        {
            return Err(refusal("request_above_limit"));
        }
    }
    Ok(())
}

// ── YAML edits ───────────────────────────────────────────────────────────────────────────────

/// A namespaced resource, as a request path names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ResourceRef {
    /// The API group; empty for the core group.
    #[serde(default)]
    pub group: String,
    /// The version.
    pub version: String,
    /// The resource (plural).
    pub resource: String,
}

impl ResourceRef {
    /// `group/version`, or `version` for the core group.
    #[must_use]
    pub fn api_version(&self) -> String {
        if self.group.is_empty() {
            self.version.clone()
        } else {
            format!("{}/{}", self.group, self.version)
        }
    }
}

/// Whether a resource may be written as YAML, and if not why (`mutation.resourceAllowed`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allowed {
    /// It may.
    pub allowed: bool,
    /// Why not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Whether `resource` may be edited as YAML.
///
/// - Pods, `ReplicaSets`, `ControllerRevisions`, Endpoints and `EndpointSlices` belong to a
///   controller, which would undo the edit: `controller_managed`.
/// - Namespaces are above a project: `tenant_level`.
/// - RBAC objects grant rights: `privilege`.
/// - Gateways and `GatewayClasses` are shared by every project: `shared_infrastructure`.
#[must_use]
pub fn resource_allowed(resource: &ResourceRef) -> Allowed {
    let reason = match (resource.group.as_str(), resource.resource.as_str()) {
        ("", "pods" | "endpoints")
        | ("apps", "replicasets" | "controllerrevisions")
        | ("discovery.k8s.io", "endpointslices") => Some("controller_managed"),
        ("", "namespaces") => Some("tenant_level"),
        ("rbac.authorization.k8s.io", _) => Some("privilege"),
        ("gateway.networking.k8s.io", "gateways" | "gatewayclasses") => {
            Some("shared_infrastructure")
        }
        _ => None,
    };
    Allowed {
        allowed: reason.is_none(),
        reason: reason.map(str::to_owned),
    }
}

/// Checks that an edited body names the object the request addresses
/// (`mutation.reconcileIdentity`).
///
/// # Errors
///
/// `missing_api_version_or_kind`, `api_version_mismatch`, `kind_mismatch`, `name_mismatch`,
/// `namespace_mismatch`.
pub fn reconcile_identity(
    resource: &ResourceRef,
    kind: &str,
    namespace: &str,
    name: &str,
    body: &Value,
) -> Result<(), Refusal> {
    let (Some(api_version), Some(body_kind)) = (
        body.get("apiVersion").and_then(Value::as_str),
        body.get("kind").and_then(Value::as_str),
    ) else {
        return Err(refusal("missing_api_version_or_kind"));
    };
    if api_version != resource.api_version() {
        return Err(refusal("api_version_mismatch"));
    }
    if !body_kind.eq_ignore_ascii_case(kind) {
        return Err(refusal("kind_mismatch"));
    }
    if body.pointer("/metadata/name").and_then(Value::as_str) != Some(name) {
        return Err(refusal("name_mismatch"));
    }
    if body
        .pointer("/metadata/namespace")
        .and_then(Value::as_str)
        .is_some_and(|ns| ns != namespace)
    {
        return Err(refusal("namespace_mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn mount_paths_must_not_nest() {
        assert!(validate_mount_path("/data", &["/var/lib"]).is_ok());
        assert_eq!(
            validate_mount_path("/data/", &["/data"]),
            Err(refusal("mount_path_in_use"))
        );
        assert_eq!(
            validate_mount_path("/", &["/data"]),
            Err(refusal("mount_path_overlap"))
        );
        assert!(validate_mount_path("/database", &["/data"]).is_ok());
    }

    #[test]
    fn exec_probes_need_a_command_and_timings_are_kept() {
        let probe = ProbeRequest {
            handler: ProbeHandler::Exec,
            port: None,
            path: None,
            scheme: None,
            command: Some(vec!["pg_isready".to_owned()]),
            initial_delay_seconds: Some(5),
            period_seconds: None,
            timeout_seconds: None,
            failure_threshold: Some(3),
        };
        assert_eq!(
            probe_config(&probe, &[]),
            Ok(
                json!({"exec": {"command": ["pg_isready"]}, "initialDelaySeconds": 5, "failureThreshold": 3})
            )
        );
        let empty = ProbeRequest {
            command: Some(Vec::new()),
            ..probe
        };
        assert_eq!(
            probe_config(&empty, &[]),
            Err(refusal("missing_probe_command"))
        );
    }
}
