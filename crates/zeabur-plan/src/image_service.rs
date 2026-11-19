//! A service from a container image: the objects the "Docker image" form and a Compose file
//! create, as one change.
//!
//! The objects are created as a `create` of each is, in this order: the variables' Secret, the
//! registry login's Secret, the claims, the Deployment, the Service and the Ingress. None carries
//! `app.kubernetes.io/managed-by`: a plan refuses to change an object another tool manages.
//! Neither security profile mounts the service account's token: an ordinary service never calls
//! the Kubernetes API.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec, DeploymentStrategy};
use k8s_openapi::api::core::v1::{
    Capabilities, Container, ContainerPort, EnvFromSource, EnvVar, EnvVarSource,
    LocalObjectReference, PersistentVolumeClaim, PersistentVolumeClaimSpec,
    PersistentVolumeClaimVolumeSource, PodSecurityContext, PodSpec, PodTemplateSpec, Probe,
    ResourceRequirements, SeccompProfile, Secret, SecretEnvSource, SecretKeySelector,
    SecurityContext, Service, ServicePort, ServiceSpec, TCPSocketAction, Volume, VolumeMount,
    VolumeResourceRequirements,
};
use k8s_openapi::api::networking::v1::{
    HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
    IngressServiceBackend, IngressSpec, IngressTLS, ServiceBackendPort,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeabur_model::PodSecurityLevel;
use zeabur_model::kernel::dns::{is_hostname, is_rfc1035_label};

use crate::change::{NameValue, ServiceSize};
use crate::checks::{VolumeRequest, validate_storage};
use crate::compose::{MAX_REPLICAS, SERVICE_NAME_MAX};
use crate::owned::SERVICE_LABEL;
use crate::plan::{ErrorKind, PlanError, Planner, error, to_json};
use crate::rules::{EnvValueFrom, is_c_identifier};

/// An image reference as the form takes it, for the API's description.
pub const IMAGE_PATTERN: &str = "^[a-z0-9][A-Za-z0-9._/:@-]*$";

/// A [`crate::Change::CreateImageService`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ImageServiceChange {
    /// The service's name, which its workload, Service and Ingress share.
    #[cfg_attr(
        feature = "openapi",
        schema(max_length = 40, pattern = "^[a-z]([-a-z0-9]*[a-z0-9])?$")
    )]
    pub name: String,
    /// The image reference.
    #[cfg_attr(feature = "openapi", schema(pattern = "^[a-z0-9][A-Za-z0-9._/:@-]*$"))]
    pub image: String,
    /// The port the app listens on. Left out: a worker, which takes no connections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(minimum = 1))]
    pub port: Option<u16>,
    /// Ports besides `port` the Kubernetes Service carries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub more_ports: Vec<u16>,
    /// A public hostname for the port, with HTTPS where the cluster has an issuer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// The app's variables.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<ServiceVariable>,
    /// Keep plain values in the Secret `<name>-env` rather than in the Deployment. A value that
    /// reads another variable with `$(…)` stays in the Deployment, as Kubernetes expands it only
    /// there.
    pub secret_variables: bool,
    /// Claims `<name>-<volume>`; a service with one runs one replica and replaces its pod by
    /// stopping the old one first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<ServiceVolume>,
    /// What the app asks for.
    pub size: ServiceSize,
    /// How many pods run.
    #[cfg_attr(feature = "openapi", schema(minimum = 1, maximum = 20))]
    pub replicas: u32,
    /// The registry login the image is pulled with, kept in the Secret `<name>-registry`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<RegistryLogin>,
    /// `restricted`, or `baseline` for any other level.
    pub security: PodSecurityLevel,
    /// The container's command, when not the image's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// The container's arguments, when not the image's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Hosts and ports an init container waits for before the app starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_for: Option<WaitFor>,
}

/// A variable of a new service: a value, or a key of a Secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceVariable {
    /// The variable's name.
    pub name: String,
    /// Its value, when it has no `valueFrom`.
    #[serde(default)]
    pub value: String,
    /// The Secret key it reads instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_from: Option<EnvValueFrom>,
}

/// A claim of a new service and where it is mounted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceVolume {
    /// The volume's name; the claim is `<service>-<name>`.
    #[cfg_attr(
        feature = "openapi",
        schema(max_length = 63, pattern = "^[a-z0-9]([-a-z0-9]*[a-z0-9])?$")
    )]
    pub name: String,
    /// Where the container sees it; absolute.
    pub mount_path: String,
    /// The size, a Kubernetes quantity such as `1Gi`.
    pub size: String,
    /// Mounted read-only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
}

/// A login to a private registry.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct RegistryLogin {
    /// The registry's host, such as `ghcr.io`.
    pub server: String,
    /// The user name.
    pub username: String,
    /// The password or token.
    pub password: String,
}

impl std::fmt::Debug for RegistryLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistryLogin")
            .field("server", &self.server)
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

/// Hosts an init container waits for, with the image that waits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct WaitFor {
    /// Each host and port, waited for in turn.
    pub hosts: Vec<HostPort>,
    /// An image with `sh` and `nc`.
    pub image: String,
}

/// A host and a port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct HostPort {
    /// The host.
    pub host: String,
    /// The port.
    pub port: u16,
}

pub(crate) fn refused(code: &str, detail: &str, field: &str) -> PlanError {
    let mut refused = error(ErrorKind::BadRequest, code, detail);
    refused.field = Some(field.to_owned());
    refused
}

/// Whether `image` is a reference the form takes: [`IMAGE_PATTERN`].
fn is_image(image: &str) -> bool {
    let mut bytes = image.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b"._/:@-".contains(&b))
}

pub(crate) fn check_name(name: &str) -> Result<(), PlanError> {
    if is_rfc1035_label(name, SERVICE_NAME_MAX) {
        return Ok(());
    }
    Err(refused(
        "invalid_name",
        "A service name is lower case letters, digits and hyphens, starts with a letter and has at most 40 characters.",
        "name",
    ))
}

pub(crate) fn check_host(host: Option<&str>, port: Option<u16>) -> Result<(), PlanError> {
    if port == Some(0) {
        return Err(refused("invalid_port", "A port is 1 to 65535.", "port"));
    }
    let Some(host) = host else {
        return Ok(());
    };
    if port.is_none() {
        return Err(refused(
            "host_without_port",
            "A public hostname needs the port the app listens on.",
            "host",
        ));
    }
    let host = host.trim();
    if host.contains('.') && is_hostname(host) {
        return Ok(());
    }
    Err(refused(
        "invalid_host",
        "Use a hostname like app.example.com.",
        "host",
    ))
}

pub(crate) fn check_variable_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
    field: &str,
) -> Result<(), PlanError> {
    if names.into_iter().all(is_c_identifier) {
        return Ok(());
    }
    Err(refused(
        "invalid_env_name",
        "A variable name is letters, digits and underscores, and does not start with a digit.",
        field,
    ))
}

fn check(change: &ImageServiceChange) -> Result<(), PlanError> {
    check_name(&change.name)?;
    if !is_image(change.image.trim()) {
        return Err(refused(
            "invalid_image",
            "Use an image reference like ghcr.io/acme/web:1.4.",
            "image",
        ));
    }
    check_host(change.host.as_deref(), change.port)?;
    if change.more_ports.contains(&0) {
        return Err(refused(
            "invalid_port",
            "A port is 1 to 65535.",
            "morePorts",
        ));
    }
    if !(1..=MAX_REPLICAS).contains(&u64::from(change.replicas)) {
        return Err(refused(
            "invalid_replicas",
            "A new service runs 1 to 20 replicas.",
            "replicas",
        ));
    }
    check_variable_names(
        change.variables.iter().map(|v| v.name.as_str()),
        "variables",
    )?;
    let mut paths: Vec<&str> = Vec::new();
    for volume in &change.volumes {
        let request = VolumeRequest {
            name: volume.name.clone(),
            size: volume.size.trim().to_owned(),
            mount_path: volume.mount_path.trim().to_owned(),
            ..VolumeRequest::default()
        };
        validate_storage(&request, &paths)
            .map_err(|r| refused(&r.0, "The volume is not valid.", "volumes"))?;
        paths.push(volume.mount_path.trim());
    }
    if let Some(registry) = &change.registry
        && [&registry.server, &registry.username, &registry.password]
            .iter()
            .any(|v| v.trim().is_empty())
    {
        return Err(refused(
            "invalid_registry",
            "A registry login needs the server, the user name and the password.",
            "registry",
        ));
    }
    Ok(())
}

/// Neither profile mounts the service account's token. `baseline` keeps the container from
/// gaining privileges and keeps the runtime's seccomp profile; only root is let back in.
pub(crate) fn security(level: PodSecurityLevel) -> (PodSpec, SecurityContext) {
    let restricted = level == PodSecurityLevel::Restricted;
    let pod = PodSpec {
        automount_service_account_token: Some(false),
        security_context: Some(PodSecurityContext {
            run_as_non_root: restricted.then_some(true),
            seccomp_profile: Some(SeccompProfile {
                type_: "RuntimeDefault".to_owned(),
                localhost_profile: None,
            }),
            ..PodSecurityContext::default()
        }),
        ..PodSpec::default()
    };
    let container = SecurityContext {
        allow_privilege_escalation: Some(false),
        capabilities: restricted.then(|| Capabilities {
            drop: Some(vec!["ALL".to_owned()]),
            add: None,
        }),
        ..SecurityContext::default()
    };
    (pod, container)
}

fn quantities(list: &[(&str, &str)]) -> BTreeMap<String, Quantity> {
    list.iter()
        .map(|(key, value)| ((*key).to_owned(), Quantity((*value).to_owned())))
        .collect()
}

fn resources(size: ServiceSize) -> ResourceRequirements {
    let (cpu, memory, limit) = match size {
        ServiceSize::Small => ("100m", "128Mi", "256Mi"),
        ServiceSize::Medium => ("250m", "256Mi", "512Mi"),
        ServiceSize::Large => ("500m", "512Mi", "1Gi"),
    };
    ResourceRequirements {
        requests: Some(quantities(&[("cpu", cpu), ("memory", memory)])),
        limits: Some(quantities(&[("memory", limit)])),
        ..ResourceRequirements::default()
    }
}

fn meta(name: &str, labels: &BTreeMap<String, String>) -> ObjectMeta {
    ObjectMeta {
        name: Some(name.to_owned()),
        labels: Some(labels.clone()),
        ..ObjectMeta::default()
    }
}

/// An `Opaque` Secret of `values`.
pub(crate) fn opaque_secret<'a>(
    name: &str,
    labels: &BTreeMap<String, String>,
    values: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Secret {
    Secret {
        metadata: meta(name, labels),
        type_: Some("Opaque".to_owned()),
        string_data: Some(
            values
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        ),
        ..Secret::default()
    }
}

/// The label set of service `name`.
pub(crate) fn labels_of(name: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(SERVICE_LABEL.to_owned(), name.to_owned())])
}

fn registry_secret(name: &str, login: &RegistryLogin, labels: &BTreeMap<String, String>) -> Secret {
    let auth = STANDARD.encode(format!("{}:{}", login.username, login.password));
    let config = json!({"auths": {login.server.trim(): {
        "username": login.username,
        "password": login.password,
        "auth": auth,
    }}});
    Secret {
        metadata: meta(&format!("{name}-registry"), labels),
        type_: Some("kubernetes.io/dockerconfigjson".to_owned()),
        string_data: Some(BTreeMap::from([(
            ".dockerconfigjson".to_owned(),
            config.to_string(),
        )])),
        ..Secret::default()
    }
}

fn claim(
    name: &str,
    volume: &ServiceVolume,
    labels: &BTreeMap<String, String>,
) -> PersistentVolumeClaim {
    PersistentVolumeClaim {
        metadata: meta(&format!("{name}-{}", volume.name), labels),
        spec: Some(PersistentVolumeClaimSpec {
            access_modes: Some(vec!["ReadWriteOnce".to_owned()]),
            resources: Some(VolumeResourceRequirements {
                requests: Some(quantities(&[("storage", volume.size.trim())])),
                limits: None,
            }),
            ..PersistentVolumeClaimSpec::default()
        }),
        ..PersistentVolumeClaim::default()
    }
}

/// A plain value that may go in the Secret: one that reads no other variable with `$(…)`.
fn secretable(variable: &ServiceVariable) -> bool {
    variable.value_from.is_none() && !variable.value.contains("$(")
}

fn env_var(variable: &ServiceVariable) -> EnvVar {
    EnvVar {
        name: variable.name.clone(),
        value: variable
            .value_from
            .is_none()
            .then(|| variable.value.clone()),
        value_from: variable.value_from.as_ref().map(|from| EnvVarSource {
            secret_key_ref: Some(SecretKeySelector {
                name: from.secret_name.clone().unwrap_or_default(),
                key: from.key.clone().unwrap_or_default(),
                optional: None,
            }),
            ..EnvVarSource::default()
        }),
    }
}

fn wait_containers(wait: &WaitFor) -> Vec<Container> {
    wait.hosts
        .iter()
        .enumerate()
        .map(|(i, HostPort { host, port })| Container {
            name: format!("wait-{}", i + 1),
            image: Some(wait.image.clone()),
            command: Some(vec![
                "sh".to_owned(),
                "-c".to_owned(),
                format!("until nc -z -w 2 {host} {port}; do sleep 2; done"),
            ]),
            resources: Some(ResourceRequirements {
                requests: Some(quantities(&[("cpu", "10m"), ("memory", "16Mi")])),
                limits: Some(quantities(&[("memory", "32Mi")])),
                ..ResourceRequirements::default()
            }),
            // As nobody, so a pod that must not run as root starts it too.
            security_context: Some(SecurityContext {
                allow_privilege_escalation: Some(false),
                capabilities: Some(Capabilities {
                    drop: Some(vec!["ALL".to_owned()]),
                    add: None,
                }),
                run_as_non_root: Some(true),
                run_as_user: Some(65534),
                ..SecurityContext::default()
            }),
            ..Container::default()
        })
        .collect()
}

fn container(
    change: &ImageServiceChange,
    ports: &[i32],
    env_secret: Option<&str>,
    env: Vec<EnvVar>,
    security: SecurityContext,
) -> Container {
    let first = ports.first().copied();
    Container {
        name: change.name.clone(),
        image: Some(change.image.trim().to_owned()),
        command: (!change.command.is_empty()).then(|| change.command.clone()),
        args: (!change.args.is_empty()).then(|| change.args.clone()),
        ports: first.map(|_| {
            ports
                .iter()
                .map(|&p| ContainerPort {
                    container_port: p,
                    ..ContainerPort::default()
                })
                .collect()
        }),
        env_from: env_secret.map(|secret| {
            vec![EnvFromSource {
                secret_ref: Some(SecretEnvSource {
                    name: secret.to_owned(),
                    optional: None,
                }),
                ..EnvFromSource::default()
            }]
        }),
        env: (!env.is_empty()).then_some(env),
        resources: Some(resources(change.size)),
        readiness_probe: first.map(|p| Probe {
            tcp_socket: Some(TCPSocketAction {
                port: IntOrString::Int(p),
                host: None,
            }),
            ..Probe::default()
        }),
        volume_mounts: (!change.volumes.is_empty()).then(|| {
            change
                .volumes
                .iter()
                .map(|v| VolumeMount {
                    name: v.name.clone(),
                    mount_path: v.mount_path.trim().to_owned(),
                    read_only: v.read_only.then_some(true),
                    ..VolumeMount::default()
                })
                .collect()
        }),
        security_context: Some(security),
        ..Container::default()
    }
}

fn deployment(
    change: &ImageServiceChange,
    container: Container,
    labels: &BTreeMap<String, String>,
    annotations: Option<BTreeMap<String, String>>,
) -> Deployment {
    let name = change.name.as_str();
    let stateful = !change.volumes.is_empty();
    let (mut pod, _) = security(change.security);
    pod.containers = vec![container];
    pod.image_pull_secrets = change.registry.as_ref().map(|_| {
        vec![LocalObjectReference {
            name: format!("{name}-registry"),
        }]
    });
    pod.init_containers = change
        .wait_for
        .as_ref()
        .filter(|w| !w.hosts.is_empty())
        .map(wait_containers);
    pod.volumes = stateful.then(|| {
        change
            .volumes
            .iter()
            .map(|v| Volume {
                name: v.name.clone(),
                persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                    claim_name: format!("{name}-{}", v.name),
                    read_only: None,
                }),
                ..Volume::default()
            })
            .collect()
    });
    Deployment {
        metadata: ObjectMeta {
            annotations,
            ..meta(name, labels)
        },
        spec: Some(DeploymentSpec {
            replicas: Some(if stateful {
                1
            } else {
                i32::try_from(change.replicas).unwrap_or(1)
            }),
            // A ReadWriteOnce claim is mounted by one node at a time: the old pod stops first.
            strategy: stateful.then(|| DeploymentStrategy {
                type_: Some("Recreate".to_owned()),
                rolling_update: None,
            }),
            selector: LabelSelector {
                match_labels: Some(labels.clone()),
                match_expressions: None,
            },
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels.clone()),
                    ..ObjectMeta::default()
                }),
                spec: Some(pod),
            },
            ..DeploymentSpec::default()
        }),
        ..Deployment::default()
    }
}

fn service(name: &str, ports: &[i32], labels: &BTreeMap<String, String>) -> Service {
    Service {
        metadata: meta(name, labels),
        spec: Some(ServiceSpec {
            selector: Some(labels.clone()),
            ports: Some(
                ports
                    .iter()
                    .enumerate()
                    .map(|(i, &port)| ServicePort {
                        name: Some(if i == 0 {
                            "http".to_owned()
                        } else {
                            format!("port-{port}")
                        }),
                        port,
                        target_port: Some(IntOrString::Int(port)),
                        ..ServicePort::default()
                    })
                    .collect(),
            ),
            ..ServiceSpec::default()
        }),
        ..Service::default()
    }
}

fn ingress(
    name: &str,
    host: &str,
    port: i32,
    labels: &BTreeMap<String, String>,
    cluster_issuer: Option<&str>,
) -> Ingress {
    Ingress {
        metadata: ObjectMeta {
            annotations: cluster_issuer.map(|issuer| {
                BTreeMap::from([(
                    "cert-manager.io/cluster-issuer".to_owned(),
                    issuer.to_owned(),
                )])
            }),
            ..meta(name, labels)
        },
        spec: Some(IngressSpec {
            rules: Some(vec![IngressRule {
                host: Some(host.to_owned()),
                http: Some(HTTPIngressRuleValue {
                    paths: vec![HTTPIngressPath {
                        path: Some("/".to_owned()),
                        path_type: "Prefix".to_owned(),
                        backend: IngressBackend {
                            service: Some(IngressServiceBackend {
                                name: name.to_owned(),
                                port: Some(ServiceBackendPort {
                                    number: Some(port),
                                    name: None,
                                }),
                            }),
                            resource: None,
                        },
                    }],
                }),
            }]),
            tls: cluster_issuer.map(|_| {
                vec![IngressTLS {
                    hosts: Some(vec![host.to_owned()]),
                    secret_name: Some(format!("{name}-tls")),
                }]
            }),
            ..IngressSpec::default()
        }),
        ..Ingress::default()
    }
}

/// The objects of `change`, in the order they are created. `annotations` go on the Deployment.
pub(crate) fn image_objects(
    change: &ImageServiceChange,
    cluster_issuer: Option<&str>,
    annotations: Option<BTreeMap<String, String>>,
) -> Result<Vec<Value>, PlanError> {
    let name = change.name.as_str();
    let labels = labels_of(name);
    let ports: Vec<i32> = change
        .port
        .into_iter()
        .chain(
            change
                .port
                .map(|_| change.more_ports.iter().copied())
                .into_iter()
                .flatten(),
        )
        .map(i32::from)
        .collect();
    let in_secret = |v: &&ServiceVariable| change.secret_variables && secretable(v);
    let plain: Vec<&ServiceVariable> = change.variables.iter().filter(in_secret).collect();
    let env_secret = (!plain.is_empty()).then(|| format!("{name}-env"));
    let env = change
        .variables
        .iter()
        .filter(|v| !in_secret(v))
        .map(env_var)
        .collect();
    let (_, container_security) = security(change.security);
    let container = container(
        change,
        &ports,
        env_secret.as_deref(),
        env,
        container_security,
    );

    let mut out = Vec::new();
    if let Some(secret) = &env_secret {
        let values = plain.iter().map(|v| (v.name.as_str(), v.value.as_str()));
        out.push(to_json(&opaque_secret(secret, &labels, values))?);
    }
    if let Some(login) = &change.registry {
        out.push(to_json(&registry_secret(name, login, &labels))?);
    }
    for volume in &change.volumes {
        out.push(to_json(&claim(name, volume, &labels))?);
    }
    out.push(to_json(&deployment(
        change,
        container,
        &labels,
        annotations,
    ))?);
    if let Some(&port) = ports.first() {
        out.push(to_json(&service(name, &ports, &labels))?);
        if let Some(host) = change.host.as_deref().map(str::trim) {
            out.push(to_json(&ingress(
                name,
                host,
                port,
                &labels,
                cluster_issuer,
            ))?);
        }
    }
    Ok(out)
}

/// A service built from `variables` as a `Build` runs it: every variable a plain value.
pub(crate) fn plain_variables(variables: &[NameValue]) -> Vec<ServiceVariable> {
    variables
        .iter()
        .map(|v| ServiceVariable {
            name: v.name.clone(),
            value: v.value.clone(),
            value_from: None,
        })
        .collect()
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    pub(crate) fn create_image_service(
        &mut self,
        change: &ImageServiceChange,
        index: usize,
    ) -> Result<(), PlanError> {
        check(change)?;
        for object in image_objects(change, self.input.cluster_issuer, None)? {
            self.create_object(&object, index)?;
        }
        Ok(())
    }
}

/// The objects a creation change makes, for a person to edit them as YAML before planning.
///
/// # Errors
///
/// What the plan refuses about the change.
pub fn image_service_objects(
    change: &ImageServiceChange,
    cluster_issuer: Option<&str>,
) -> Result<Vec<Value>, PlanError> {
    check(change)?;
    image_objects(change, cluster_issuer, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_image_rule_is_the_described_pattern() {
        let pattern = regex::Regex::new(IMAGE_PATTERN).unwrap();
        for image in [
            "nginx",
            "ghcr.io/acme/web:1.4",
            "nginx@sha256:abc",
            "Nginx",
            "-x",
            "a b",
            "",
            "registry.example.com:5000/a/b_c",
        ] {
            assert_eq!(is_image(image), pattern.is_match(image), "{image}");
        }
    }
}
