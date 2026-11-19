//! A service from an uploaded folder: Zeabur Builder's `Build` with an `archive` source, and the
//! objects of the service that runs the image it builds, as one change.
//!
//! The web interface and `zeaburctl up` both send [`crate::Change::CreateUploadService`], so they
//! make the same objects. They are created as a `create` of each is: the `Build` last, with the
//! annotation that asks Zeabur Builder for an upload token. The Deployment names the `Build`'s
//! repository with a tag no build makes, and Keel moves it to each image the `Build` pushes.
//! Neither security profile mounts the service account's token: an ordinary service never calls
//! the Kubernetes API.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
    Capabilities, Container, ContainerPort, EnvFromSource, EnvVar, EnvVarSource,
    PodSecurityContext, PodSpec, PodTemplateSpec, Probe, ResourceRequirements, SeccompProfile,
    Secret, SecretEnvSource, SecretKeySelector, SecurityContext, Service, ServicePort, ServiceSpec,
    TCPSocketAction,
};
use k8s_openapi::api::networking::v1::{
    HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
    IngressServiceBackend, IngressSpec, IngressTLS, ServiceBackendPort,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use serde::Serialize;
use serde_json::Value;
use zeabur_model::PodSecurityLevel;
use zeabur_model::kernel::dns::{is_hostname, is_rfc1035_label};

use crate::builds::{
    BUILDER_GROUP, BUILDER_VERSION, KEEL_FORCE, KEEL_POLICY, UPLOAD_TOKEN_REQUEST, build_path,
};
use crate::change::{NameValue, ServiceSize, UploadMethod, UploadServiceChange};
use crate::plan::{ErrorKind, PlanError, Planner, error, to_json};
use crate::rules::is_c_identifier;

/// The tag the Deployment names until the first build.
const PENDING_TAG: &str = "pending";
/// The webhook Keel is told of a new image at.
const KEEL_WEBHOOK: &str = "http://keel.keel.svc.cluster.local:9300/v1/webhooks/native";
/// The longest service name.
const NAME_MAX: usize = 40;
/// The longest upload token request.
const REQUEST_MAX: usize = 63;

fn refused(code: &str, detail: &str, field: &str) -> PlanError {
    let mut refused = error(ErrorKind::BadRequest, code, detail);
    refused.field = Some(field.to_owned());
    refused
}

fn check(change: &UploadServiceChange) -> Result<(), PlanError> {
    if !is_rfc1035_label(&change.name, NAME_MAX) {
        return Err(refused(
            "invalid_name",
            "A service name is lower case letters, digits and hyphens, starts with a letter and has at most 40 characters.",
            "name",
        ));
    }
    if change.request.is_empty()
        || change.request.len() > REQUEST_MAX
        || !change
            .request
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(refused(
            "invalid_request",
            "The upload request must be 1 to 63 letters, digits, dots, dashes or underscores.",
            "request",
        ));
    }
    for (field, path) in [
        ("directory", change.directory.as_deref()),
        ("dockerfile", change.dockerfile.as_deref()),
    ] {
        if let Some(path) = trimmed(path)
            && !build_path(&path)
        {
            return Err(refused(
                "invalid_path",
                "A path in the upload has no segment that starts with a dot.",
                field,
            ));
        }
    }
    if change.port == Some(0) {
        return Err(refused("invalid_port", "A port is 1 to 65535.", "port"));
    }
    if let Some(host) = &change.host {
        if change.port.is_none() {
            return Err(refused(
                "host_without_port",
                "A public hostname needs the port the app listens on.",
                "host",
            ));
        }
        let host = host.trim();
        if !(host.contains('.') && is_hostname(host)) {
            return Err(refused(
                "invalid_host",
                "Use a hostname like app.example.com.",
                "host",
            ));
        }
    }
    for (field, list) in [
        ("variables", &change.variables),
        ("buildVariables", &change.build_variables),
    ] {
        if list.iter().any(|v| !is_c_identifier(&v.name)) {
            return Err(refused(
                "invalid_env_name",
                "A variable name is letters, digits and underscores, and does not start with a digit.",
                field,
            ));
        }
    }
    Ok(())
}

fn trimmed(path: Option<&str>) -> Option<String> {
    let path = path?
        .trim()
        .trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/');
    (!path.is_empty()).then(|| path.to_owned())
}

fn string_data(list: &[NameValue]) -> BTreeMap<String, String> {
    list.iter()
        .map(|v| (v.name.clone(), v.value.clone()))
        .collect()
}

fn secret(name: &str, labels: &BTreeMap<String, String>, data: &[NameValue]) -> Secret {
    Secret {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            labels: Some(labels.clone()),
            ..ObjectMeta::default()
        },
        type_: Some("Opaque".to_owned()),
        string_data: Some(string_data(data)),
        ..Secret::default()
    }
}

fn security(level: PodSecurityLevel) -> (PodSpec, SecurityContext) {
    let seccomp = Some(SeccompProfile {
        type_: "RuntimeDefault".to_owned(),
        localhost_profile: None,
    });
    let restricted = level == PodSecurityLevel::Restricted;
    let pod = PodSpec {
        automount_service_account_token: Some(false),
        security_context: Some(PodSecurityContext {
            run_as_non_root: restricted.then_some(true),
            seccomp_profile: seccomp,
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

fn resources(size: ServiceSize) -> ResourceRequirements {
    let (cpu, memory, limit) = match size {
        ServiceSize::Small => ("100m", "128Mi", "256Mi"),
        ServiceSize::Medium => ("250m", "256Mi", "512Mi"),
        ServiceSize::Large => ("500m", "512Mi", "1Gi"),
    };
    let quantity = |value: &str| Quantity(value.to_owned());
    ResourceRequirements {
        requests: Some(BTreeMap::from([
            ("cpu".to_owned(), quantity(cpu)),
            ("memory".to_owned(), quantity(memory)),
        ])),
        limits: Some(BTreeMap::from([("memory".to_owned(), quantity(limit))])),
        ..ResourceRequirements::default()
    }
}

fn ingress_object(
    name: &str,
    host: &str,
    port: u16,
    labels: &BTreeMap<String, String>,
    cluster_issuer: Option<&str>,
) -> Ingress {
    let secured = cluster_issuer.is_some();
    Ingress {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            labels: Some(labels.clone()),
            annotations: cluster_issuer.map(|issuer| {
                BTreeMap::from([(
                    "cert-manager.io/cluster-issuer".to_owned(),
                    issuer.to_owned(),
                )])
            }),
            ..ObjectMeta::default()
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
                                    number: Some(i32::from(port)),
                                    name: None,
                                }),
                            }),
                            resource: None,
                        },
                    }],
                }),
            }]),
            tls: secured.then(|| {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Build {
    api_version: String,
    kind: &'static str,
    metadata: ObjectMeta,
    spec: BuildSpec,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildSpec {
    builder_ref: &'static str,
    source: BuildSource,
    output: BuildOutput,
    notify: [BuildNotify; 1],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    env: Vec<EnvVar>,
}

#[derive(Serialize)]
struct BuildSource {
    archive: Archive,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Archive {
    #[serde(skip_serializing_if = "Option::is_none")]
    context_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dockerfile: Option<String>,
}

#[derive(Serialize)]
struct BuildOutput {
    image: String,
}

#[derive(Serialize)]
struct BuildNotify {
    #[serde(rename = "type")]
    kind: &'static str,
    url: &'static str,
}

fn build(
    change: &UploadServiceChange,
    repository: &str,
    build_secret: &str,
    labels: &BTreeMap<String, String>,
) -> Build {
    let dockerfile = (change.method == UploadMethod::Dockerfile)
        .then(|| trimmed(change.dockerfile.as_deref()))
        .flatten();
    let env = change
        .build_variables
        .iter()
        .map(|v| EnvVar {
            name: v.name.clone(),
            value: None,
            value_from: Some(EnvVarSource {
                secret_key_ref: Some(SecretKeySelector {
                    name: build_secret.to_owned(),
                    key: v.name.clone(),
                    optional: None,
                }),
                ..EnvVarSource::default()
            }),
        })
        .collect();
    Build {
        api_version: format!("{BUILDER_GROUP}/{BUILDER_VERSION}"),
        kind: "Build",
        metadata: ObjectMeta {
            name: Some(change.name.clone()),
            labels: Some(labels.clone()),
            annotations: Some(BTreeMap::from([(
                UPLOAD_TOKEN_REQUEST.to_owned(),
                change.request.clone(),
            )])),
            ..ObjectMeta::default()
        },
        spec: BuildSpec {
            builder_ref: match change.method {
                UploadMethod::Railpack => "railpack",
                UploadMethod::Dockerfile => "dockerfile",
            },
            source: BuildSource {
                archive: Archive {
                    context_dir: trimmed(change.directory.as_deref()),
                    dockerfile,
                },
            },
            output: BuildOutput {
                image: repository.to_owned(),
            },
            notify: [BuildNotify {
                kind: "keel",
                url: KEEL_WEBHOOK,
            }],
            env,
        },
    }
}

fn app_container(
    change: &UploadServiceChange,
    repository: &str,
    security: SecurityContext,
    port: Option<i32>,
    env_secret: Option<&str>,
) -> Container {
    Container {
        name: change.name.clone(),
        image: Some(format!("{repository}:{PENDING_TAG}")),
        resources: Some(resources(change.size.unwrap_or(ServiceSize::Small))),
        security_context: Some(security),
        ports: port.map(|p| {
            vec![ContainerPort {
                container_port: p,
                ..ContainerPort::default()
            }]
        }),
        readiness_probe: port.map(|p| Probe {
            tcp_socket: Some(TCPSocketAction {
                port: IntOrString::Int(p),
                host: None,
            }),
            ..Probe::default()
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
        ..Container::default()
    }
}

fn service_object(name: &str, labels: &BTreeMap<String, String>, port: i32) -> Service {
    Service {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            labels: Some(labels.clone()),
            ..ObjectMeta::default()
        },
        spec: Some(ServiceSpec {
            selector: Some(labels.clone()),
            ports: Some(vec![ServicePort {
                name: Some("http".to_owned()),
                port,
                target_port: Some(IntOrString::Int(port)),
                ..ServicePort::default()
            }]),
            ..ServiceSpec::default()
        }),
        ..Service::default()
    }
}

/// The objects of `change` in project `project`, in the order they are created.
fn objects(
    change: &UploadServiceChange,
    project: &str,
    registry: &str,
    cluster_issuer: Option<&str>,
) -> Result<Vec<Value>, PlanError> {
    let name = change.name.as_str();
    let labels = BTreeMap::from([("app.kubernetes.io/name".to_owned(), name.to_owned())]);
    let repository = format!("{}/{project}/{name}", registry.trim_end_matches('/'));
    let level = change.security.unwrap_or(match change.method {
        UploadMethod::Railpack => PodSecurityLevel::Restricted,
        UploadMethod::Dockerfile => PodSecurityLevel::Baseline,
    });
    let (mut pod, container_security) = security(level);
    let mut variables = change.variables.clone();
    if let Some(port) = change.port
        && !variables.iter().any(|v| v.name == "PORT")
    {
        variables.push(NameValue {
            name: "PORT".to_owned(),
            value: port.to_string(),
        });
    }
    let env_secret = format!("{name}-env");
    let build_secret = format!("{name}-build-env");
    let port = change.port.map(i32::from);
    let container = app_container(
        change,
        &repository,
        container_security,
        port,
        (!variables.is_empty()).then_some(env_secret.as_str()),
    );
    pod.containers = vec![container];

    let mut out = Vec::new();
    if !change.build_variables.is_empty() {
        out.push(to_json(&secret(
            &build_secret,
            &labels,
            &change.build_variables,
        ))?);
    }
    if !variables.is_empty() {
        out.push(to_json(&secret(&env_secret, &labels, &variables))?);
    }
    out.push(to_json(&Deployment {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            labels: Some(labels.clone()),
            annotations: Some(BTreeMap::from([(
                KEEL_POLICY.to_owned(),
                KEEL_FORCE.to_owned(),
            )])),
            ..ObjectMeta::default()
        },
        spec: Some(DeploymentSpec {
            replicas: Some(1),
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
    })?);
    if let Some(port) = port {
        out.push(to_json(&service_object(name, &labels, port))?);
        if let (Some(host), Some(port)) = (change.host.as_deref().map(str::trim), change.port) {
            out.push(to_json(&ingress_object(
                name,
                host,
                port,
                &labels,
                cluster_issuer,
            ))?);
        }
    }
    out.push(to_json(&build(
        change,
        &repository,
        &build_secret,
        &labels,
    ))?);
    Ok(out)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    pub(crate) fn create_upload_service(
        &mut self,
        change: &UploadServiceChange,
        index: usize,
    ) -> Result<(), PlanError> {
        check(change)?;
        let Some(registry) = self.input.build_registry.filter(|r| !r.is_empty()) else {
            return Err(error(
                ErrorKind::Invalid,
                "no_registry",
                "Zeabur has no registry for Zeabur Builder to push to. Ask an administrator to set builds.registry.",
            ));
        };
        let project = self.input.snapshot.namespace.clone();
        for object in objects(change, &project, registry, self.input.cluster_issuer)? {
            self.create_object(&object, index)?;
        }
        Ok(())
    }
}
