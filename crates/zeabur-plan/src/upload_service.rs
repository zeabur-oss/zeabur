//! A service from an uploaded folder: Zeabur Builder's `Build` with an `archive` source, and the
//! objects of the service that runs the image it builds, as one change.
//!
//! The web interface and `zeaburctl up` both send [`crate::Change::CreateUploadService`], so they
//! make the same objects. They are created as a `create` of each is: the `Build` last, with the
//! annotation that asks Zeabur Builder for an upload token. The Deployment names the `Build`'s
//! repository with a tag no build makes, and Keel moves it to each image the `Build` pushes.
//! Neither security profile mounts the service account's token: an ordinary service never calls
//! the Kubernetes API.

use serde_json::{Value, json};
use zeabur_model::PodSecurityLevel;

use crate::builds::{
    BUILDER_GROUP, BUILDER_VERSION, KEEL_FORCE, KEEL_POLICY, UPLOAD_TOKEN_REQUEST, build_path,
};
use crate::change::{NameValue, ServiceSize, UploadMethod, UploadServiceChange};
use crate::plan::{ErrorKind, PlanError, Planner, error};
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

fn dns_label(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= NAME_MAX
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && !name.ends_with('-')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn hostname(host: &str) -> bool {
    host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn check(change: &UploadServiceChange) -> Result<(), PlanError> {
    if !dns_label(&change.name) {
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
        if !hostname(host.trim()) {
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

fn string_data(list: &[NameValue]) -> Value {
    Value::Object(
        list.iter()
            .map(|v| (v.name.clone(), json!(v.value)))
            .collect(),
    )
}

fn security(level: PodSecurityLevel) -> (Value, Value) {
    let seccomp = json!({"type": "RuntimeDefault"});
    if level == PodSecurityLevel::Restricted {
        (
            json!({"automountServiceAccountToken": false,
                "securityContext": {"runAsNonRoot": true, "seccompProfile": seccomp}}),
            json!({"allowPrivilegeEscalation": false, "capabilities": {"drop": ["ALL"]}}),
        )
    } else {
        (
            json!({"automountServiceAccountToken": false,
                "securityContext": {"seccompProfile": seccomp}}),
            json!({"allowPrivilegeEscalation": false}),
        )
    }
}

fn resources(size: ServiceSize) -> Value {
    let (cpu, memory, limit) = match size {
        ServiceSize::Small => ("100m", "128Mi", "256Mi"),
        ServiceSize::Medium => ("250m", "256Mi", "512Mi"),
        ServiceSize::Large => ("500m", "512Mi", "1Gi"),
    };
    json!({"requests": {"cpu": cpu, "memory": memory}, "limits": {"memory": limit}})
}

fn ingress(
    name: &str,
    host: &str,
    port: u16,
    labels: &Value,
    cluster_issuer: Option<&str>,
) -> Value {
    let mut metadata = json!({"name": name, "labels": labels});
    let mut spec = json!({"rules": [{"host": host, "http": {"paths": [{
        "path": "/", "pathType": "Prefix",
        "backend": {"service": {"name": name, "port": {"number": port}}}}]}}]});
    if let (Some(issuer), Some(metadata), Some(spec)) = (
        cluster_issuer,
        metadata.as_object_mut(),
        spec.as_object_mut(),
    ) {
        metadata.insert(
            "annotations".to_owned(),
            json!({"cert-manager.io/cluster-issuer": issuer}),
        );
        spec.insert(
            "tls".to_owned(),
            json!([{"hosts": [host], "secretName": format!("{name}-tls")}]),
        );
    }
    json!({"apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
        "metadata": metadata, "spec": spec})
}

fn build(
    change: &UploadServiceChange,
    repository: &str,
    build_secret: &str,
    labels: &Value,
) -> Value {
    let mut archive = serde_json::Map::new();
    if let Some(directory) = trimmed(change.directory.as_deref()) {
        archive.insert("contextDir".to_owned(), json!(directory));
    }
    let dockerfile = (change.method == UploadMethod::Dockerfile)
        .then(|| trimmed(change.dockerfile.as_deref()))
        .flatten();
    if let Some(dockerfile) = dockerfile {
        archive.insert("dockerfile".to_owned(), json!(dockerfile));
    }
    let mut spec = json!({
        "builderRef": match change.method {
            UploadMethod::Railpack => "railpack",
            UploadMethod::Dockerfile => "dockerfile",
        },
        "source": {"archive": archive},
        "output": {"image": repository},
        "notify": [{"type": "keel", "url": KEEL_WEBHOOK}],
    });
    if let (false, Some(spec)) = (change.build_variables.is_empty(), spec.as_object_mut()) {
        let env: Vec<Value> = change
            .build_variables
            .iter()
            .map(|v| {
                json!({"name": v.name,
                    "valueFrom": {"secretKeyRef": {"name": build_secret, "key": v.name}}})
            })
            .collect();
        spec.insert("env".to_owned(), json!(env));
    }
    json!({"apiVersion": format!("{BUILDER_GROUP}/{BUILDER_VERSION}"), "kind": "Build",
        "metadata": {"name": change.name, "labels": labels,
            "annotations": {UPLOAD_TOKEN_REQUEST: change.request}},
        "spec": spec})
}

/// The objects of `change` in project `project`, in the order they are created.
fn objects(
    change: &UploadServiceChange,
    project: &str,
    registry: &str,
    cluster_issuer: Option<&str>,
) -> Vec<Value> {
    let name = change.name.as_str();
    let labels = json!({"app.kubernetes.io/name": name});
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
    let mut container = json!({
        "name": name,
        "image": format!("{repository}:{PENDING_TAG}"),
        "resources": resources(change.size.unwrap_or(ServiceSize::Small)),
        "securityContext": container_security,
    });
    if let Some(container) = container.as_object_mut() {
        if let Some(port) = change.port {
            container.insert("ports".to_owned(), json!([{"containerPort": port}]));
            container.insert(
                "readinessProbe".to_owned(),
                json!({"tcpSocket": {"port": port}}),
            );
        }
        if !variables.is_empty() {
            container.insert(
                "envFrom".to_owned(),
                json!([{"secretRef": {"name": env_secret}}]),
            );
        }
    }
    if let Some(pod) = pod.as_object_mut() {
        pod.insert("containers".to_owned(), json!([container]));
    }

    let mut out = Vec::new();
    if !change.build_variables.is_empty() {
        out.push(json!({"apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": build_secret, "labels": labels},
            "type": "Opaque", "stringData": string_data(&change.build_variables)}));
    }
    if !variables.is_empty() {
        out.push(json!({"apiVersion": "v1", "kind": "Secret",
            "metadata": {"name": env_secret, "labels": labels},
            "type": "Opaque", "stringData": string_data(&variables)}));
    }
    out.push(json!({"apiVersion": "apps/v1", "kind": "Deployment",
        "metadata": {"name": name, "labels": labels,
            "annotations": {KEEL_POLICY: KEEL_FORCE}},
        "spec": {"replicas": 1, "selector": {"matchLabels": labels},
            "template": {"metadata": {"labels": labels}, "spec": pod}}}));
    if let Some(port) = change.port {
        out.push(json!({"apiVersion": "v1", "kind": "Service",
            "metadata": {"name": name, "labels": labels},
            "spec": {"selector": labels,
                "ports": [{"name": "http", "port": port, "targetPort": port}]}}));
        if let Some(host) = change.host.as_deref().map(str::trim) {
            out.push(ingress(name, host, port, &labels, cluster_issuer));
        }
    }
    out.push(build(change, &repository, &build_secret, &labels));
    out
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
        for object in objects(change, &project, registry, self.input.cluster_issuer) {
            self.create_object(&object, index)?;
        }
        Ok(())
    }
}
