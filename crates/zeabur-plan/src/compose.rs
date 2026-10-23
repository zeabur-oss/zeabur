//! A Docker Compose file, read as the services Zeabur makes from it.
//!
//! The conversion follows Kompose [kompose]: each Compose service with an image is a service of
//! its own, its named volumes are claims, `environment` and `env_file` are its variables, and the
//! ports it listens on are a Kubernetes Service. The web interface turns the result into the
//! same objects the "Docker image" and database forms create, and plans them. Nothing keeps the
//! Compose file.
//!
//! - **Interpolation.** `${VAR}`, `${VAR:-default}`, `${VAR-default}`, `${VAR:?error}` and `$VAR`
//!   read the `.env` file when the request has one [interpolation]; a variable it does not set is
//!   empty and listed in [`Conversion::unset`]. `$$` is a `$`.
//! - **Databases.** A service whose image is PostgreSQL, MySQL, MariaDB, MongoDB, Redis, Valkey or
//!   `MinIO` names the operator-managed counterpart in [`ComposeService::counterpart`]. The
//!   variables of other services that point at it say how in [`ComposeVariable::refers`].
//! - **Refused fields.** Fields that need rights on the node are left out and listed in
//!   [`ComposeService::findings`]: `privileged`, `cap_add`, `devices`, host network and process
//!   namespaces, and mounts of host paths, the Docker socket among them. A service that only has
//!   `build:` is not made.
//!
//! [kompose]: https://github.com/kubernetes/kompose/blob/main/docs/conversion.md
//! [interpolation]: https://docs.docker.com/reference/compose-file/interpolation/

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::plan::{ErrorKind, PlanError, error};

/// The longest Compose file read, in bytes.
const MAX_YAML: usize = 256 * 1024;
/// The most services made from one file.
const MAX_SERVICES: usize = 50;
/// The longest Kubernetes name a service gets, as the "Docker image" form allows.
const NAME_MAX: usize = 40;
/// The most replicas a service gets, as the "Docker image" form allows.
const MAX_REPLICAS: u64 = 20;

/// A Compose file to convert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ComposeRequest {
    /// The Compose file, YAML.
    pub yaml: String,
    /// The files `env_file` names, and `.env` for interpolation, by path as the Compose file
    /// writes it.
    #[serde(default)]
    pub env_files: BTreeMap<String, String>,
}

/// What a Compose file becomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Conversion {
    /// The services, by Compose name.
    pub services: Vec<ComposeService>,
    /// Files `env_file` names that the request does not hold.
    pub missing_env_files: Vec<String>,
    /// Variables the file interpolates that nothing sets; they are empty.
    pub unset: Vec<String>,
    /// The image of the init containers that wait for a dependency (`depends_on`).
    pub wait_image: String,
}

/// One Compose service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ComposeService {
    /// Its Kubernetes name, a DNS label.
    pub name: String,
    /// Its name in the Compose file.
    pub compose_name: String,
    /// The image; absent when the service only has `build:`, which is then not made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// The operator-managed counterpart of its image, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterpart: Option<Counterpart>,
    /// The container's command (`entrypoint`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// The container's arguments (`command`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// The TCP ports it listens on (`ports` targets, then `expose`), without repeats.
    pub ports: Vec<u16>,
    /// Compose publishes one of its ports on the host: the service may want a public domain.
    pub published: bool,
    /// Its variables, `env_file` first and `environment` over them.
    pub variables: Vec<ComposeVariable>,
    /// Its volumes.
    pub volumes: Vec<ComposeVolume>,
    /// The services it starts after, by Kubernetes name.
    pub depends_on: Vec<String>,
    /// How many replicas (`deploy.replicas` or `scale`), from 1 to 20.
    pub replicas: u32,
    /// What is left out, and why.
    pub findings: Vec<Finding>,
}

/// The operator-managed counterpart of a database or object-storage image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum Counterpart {
    /// `postgres`: CloudNativePG.
    Postgresql,
    /// `mysql`: MOCO.
    Mysql,
    /// `mariadb`: MariaDB Operator.
    Mariadb,
    /// `mongo`: Percona Operator for MongoDB.
    Mongodb,
    /// `redis`, `valkey`: Dragonfly Operator.
    Dragonfly,
    /// `minio`: the `SeaweedFS` operator.
    ObjectStorage,
}

/// A variable of a service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ComposeVariable {
    /// Its name.
    pub name: String,
    /// Its value, interpolated.
    pub value: String,
    /// The database service it points at, when it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refers: Option<Refers>,
}

/// How a variable points at a database service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Refers {
    /// The database service, by Kubernetes name.
    pub service: String,
    /// What the value is.
    pub part: Part,
}

/// What a variable that points at a database holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum Part {
    /// A connection URL with the database's scheme (`postgres://…@db:5432/app`).
    Url,
    /// The host, alone or with its port (`db`, `db:5432`).
    Host,
    /// The password the database service sets.
    Password,
    /// The user the database service sets.
    User,
    /// The database name the database service sets.
    Database,
    /// Text that names the host in another way, such as a JDBC URL; it stays as written.
    Mention,
}

/// A volume of a service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ComposeVolume {
    /// Its name, a DNS label; the claim is `<service>-<name>`.
    pub name: String,
    /// Where it is mounted.
    pub mount_path: String,
    /// Mounted read-only.
    pub read_only: bool,
}

/// A field of a service that is left out, or a difference to know of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum Finding {
    /// `privileged: true`.
    Privileged,
    /// `network_mode: host`.
    HostNetwork,
    /// `pid: host`.
    HostPid,
    /// `cap_add`.
    CapAdd {
        /// The capabilities.
        capabilities: Vec<String>,
    },
    /// `devices`.
    Devices,
    /// A mount of the Docker socket.
    DockerSocket {
        /// The path on the host.
        source: String,
    },
    /// A mount of another host path; nothing is mounted there.
    HostPath {
        /// The path on the host.
        source: String,
        /// Where it was mounted.
        target: String,
    },
    /// `build:`. With an image the image is run; without one the service is not made.
    Build {
        /// The build context.
        context: String,
    },
    /// `healthcheck`; Zeabur checks the first port instead.
    Healthcheck,
    /// A UDP port, which a Kubernetes Service from this form does not carry.
    UdpPort {
        /// The port.
        port: u16,
    },
    /// A volume other services mount too; each gets a volume of its own.
    SharedVolume {
        /// The volume's name in the Compose file.
        volume: String,
    },
    /// The Compose name is not a DNS label; other services reach it by the new name.
    Renamed,
}

/// Reads `request`; `wait_image` is the image that waits for a dependency.
///
/// # Errors
///
/// When the YAML does not read, is too large, or holds no services.
pub fn convert(request: &ComposeRequest, wait_image: &str) -> Result<Conversion, PlanError> {
    if request.yaml.len() > MAX_YAML {
        return Err(error(
            ErrorKind::BadRequest,
            "too_large",
            "The Compose file is larger than 256 KB.",
        ));
    }
    let root: Value = serde_saphyr::from_str(&request.yaml).map_err(|e| {
        let mut refused = error(
            ErrorKind::BadRequest,
            "invalid_yaml",
            "The YAML does not read: check the indentation and the quotes on this line.",
        );
        refused.line = e.location().map(|l| l.line());
        refused
    })?;
    let Some(services) = root.get("services").and_then(Value::as_object) else {
        return Err(error(
            ErrorKind::BadRequest,
            "no_services",
            "The file has no services: a Compose file lists them under `services:`.",
        ));
    };
    if services.is_empty() || services.len() > MAX_SERVICES {
        return Err(error(
            ErrorKind::BadRequest,
            "service_count",
            "A Compose file to import has 1 to 50 services.",
        ));
    }
    let mut reader = Reader {
        dotenv: request
            .env_files
            .get(".env")
            .map_or_else(BTreeMap::new, |text| parse_env(text).into_iter().collect()),
        unset: BTreeSet::new(),
    };
    let names = kube_names(services.keys());
    let kube_of: BTreeMap<&str, &str> = services
        .keys()
        .map(String::as_str)
        .zip(names.iter().map(String::as_str))
        .collect();
    let mut missing = BTreeSet::new();
    let mut out: Vec<ComposeService> = services
        .iter()
        .zip(&names)
        .map(|((compose_name, spec), name)| {
            let spec = reader.interpolate(spec.clone());
            service(compose_name, name, &spec, &kube_of, request, &mut missing)
        })
        .collect();
    mark_shared_volumes(&mut out, services);
    link_databases(&mut out, services, &mut reader);
    Ok(Conversion {
        services: out,
        missing_env_files: missing.into_iter().collect(),
        unset: reader.unset.into_iter().collect(),
        wait_image: wait_image.to_owned(),
    })
}

struct Reader {
    dotenv: BTreeMap<String, String>,
    unset: BTreeSet<String>,
}

impl Reader {
    fn interpolate(&mut self, value: Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.text(&text)),
            Value::Array(items) => {
                Value::Array(items.into_iter().map(|v| self.interpolate(v)).collect())
            }
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(k, v)| (k, self.interpolate(v)))
                    .collect(),
            ),
            other @ (Value::Null | Value::Bool(_) | Value::Number(_)) => other,
        }
    }

    fn text(&mut self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(at) = rest.find('$') {
            out.push_str(rest.get(..at).unwrap_or_default());
            let after = rest.get(at + 1..).unwrap_or_default();
            if let Some(tail) = after.strip_prefix('$') {
                out.push('$');
                rest = tail;
            } else if let Some(braced) = after.strip_prefix('{')
                && let Some(end) = braced.find('}')
            {
                out.push_str(&self.lookup(braced.get(..end).unwrap_or_default()));
                rest = braced.get(end + 1..).unwrap_or_default();
            } else {
                let len = after
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(after.len());
                if len == 0 {
                    out.push('$');
                } else {
                    out.push_str(&self.lookup(after.get(..len).unwrap_or_default()));
                }
                rest = after.get(len..).unwrap_or_default();
            }
        }
        out.push_str(rest);
        out
    }

    /// `NAME`, `NAME:-default`, `NAME-default`, `NAME:?error`, `NAME?error`.
    fn lookup(&mut self, expression: &str) -> String {
        let split = expression
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(expression.len());
        let (name, operator) = expression.split_at(split);
        let value = self.dotenv.get(name).cloned();
        let (empty_too, default) = operator
            .strip_prefix(":-")
            .map_or_else(|| (false, operator.strip_prefix('-')), |d| (true, Some(d)));
        match (value, default) {
            (Some(v), Some(d)) if empty_too && v.is_empty() => self.text(d),
            (Some(v), _) => v,
            (None, Some(d)) => self.text(d),
            (None, None) => {
                self.unset.insert(name.to_owned());
                String::new()
            }
        }
    }
}

/// DNS labels for the Compose names, unique, at most [`NAME_MAX`] long.
fn kube_names<'a>(compose: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut taken = BTreeSet::new();
    compose
        .map(|name| {
            let base = label(name, NAME_MAX - 3);
            let base = if base.is_empty() {
                "app".to_owned()
            } else {
                base
            };
            let mut chosen = base.clone();
            let mut n = 2;
            while !taken.insert(chosen.clone()) {
                chosen = format!("{base}-{n}");
                n += 1;
            }
            chosen
        })
        .collect()
}

/// Lowercase letters, digits and single hyphens, starting with a letter.
fn label(text: &str, max: usize) -> String {
    let mut out = String::new();
    for c in text.to_ascii_lowercase().chars() {
        if c.is_ascii_lowercase() || (c.is_ascii_digit() && !out.is_empty()) {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= max {
            break;
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn service(
    compose_name: &str,
    name: &str,
    spec: &Value,
    kube_of: &BTreeMap<&str, &str>,
    request: &ComposeRequest,
    missing: &mut BTreeSet<String>,
) -> ComposeService {
    let mut findings = Vec::new();
    if name != compose_name {
        findings.push(Finding::Renamed);
    }
    let image = spec.get("image").and_then(Value::as_str).map(str::to_owned);
    if let Some(build) = spec.get("build") {
        let context = build
            .as_str()
            .or_else(|| build.get("context").and_then(Value::as_str))
            .unwrap_or(".");
        findings.push(Finding::Build {
            context: context.to_owned(),
        });
    }
    refused_fields(spec, &mut findings);
    if spec.get("healthcheck").is_some() {
        findings.push(Finding::Healthcheck);
    }
    let (ports, published) = ports(spec, &mut findings);
    let depends_on = depends_on(spec)
        .into_iter()
        .filter_map(|d| kube_of.get(d.as_str()).map(|n| (*n).to_owned()))
        .collect();
    ComposeService {
        name: name.to_owned(),
        compose_name: compose_name.to_owned(),
        counterpart: image.as_deref().and_then(counterpart),
        image,
        command: words(spec.get("entrypoint")),
        args: words(spec.get("command")),
        ports,
        published,
        variables: variables(spec, request, missing),
        volumes: volumes(spec, &mut findings),
        depends_on,
        replicas: replicas(spec),
        findings,
    }
}

fn refused_fields(spec: &Value, findings: &mut Vec<Finding>) {
    if spec.get("privileged").and_then(Value::as_bool) == Some(true) {
        findings.push(Finding::Privileged);
    }
    if spec.get("network_mode").and_then(Value::as_str) == Some("host") {
        findings.push(Finding::HostNetwork);
    }
    if spec.get("pid").and_then(Value::as_str) == Some("host") {
        findings.push(Finding::HostPid);
    }
    let capabilities = strings(spec.get("cap_add"));
    if !capabilities.is_empty() {
        findings.push(Finding::CapAdd { capabilities });
    }
    if spec
        .get("devices")
        .and_then(Value::as_array)
        .is_some_and(|d| !d.is_empty())
    {
        findings.push(Finding::Devices);
    }
}

/// The image's repository without registry, namespace or tag: `postgres` for
/// `docker.io/library/postgres:16`.
fn repository(image: &str) -> &str {
    let image = image.split('@').next().unwrap_or(image);
    let last = image.rsplit('/').next().unwrap_or(image);
    last.split(':').next().unwrap_or(last)
}

fn counterpart(image: &str) -> Option<Counterpart> {
    match repository(image) {
        "postgres" | "postgis" => Some(Counterpart::Postgresql),
        "mysql" => Some(Counterpart::Mysql),
        "mariadb" => Some(Counterpart::Mariadb),
        "mongo" | "mongodb" => Some(Counterpart::Mongodb),
        "redis" | "valkey" | "redis-stack-server" => Some(Counterpart::Dragonfly),
        "minio" => Some(Counterpart::ObjectStorage),
        _ => None,
    }
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |items| items.iter().filter_map(scalar).collect())
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// `command` and `entrypoint`: a list as it is, a string split as a shell does.
fn words(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(line)) => split_words(line),
        Some(list @ Value::Array(_)) => strings(Some(list)),
        _ => Vec::new(),
    }
}

fn split_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"') | None, '\\') => {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
                started = true;
            }
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, c) => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// The container ports, and whether Compose publishes any on the host.
fn ports(spec: &Value, findings: &mut Vec<Finding>) -> (Vec<u16>, bool) {
    let mut found: Vec<u16> = Vec::new();
    let mut published = false;
    let mut add = |port: Option<u16>, udp: bool, findings: &mut Vec<Finding>| match port {
        Some(port) if udp => findings.push(Finding::UdpPort { port }),
        Some(port) if !found.contains(&port) => found.push(port),
        _ => {}
    };
    for entry in spec
        .get("ports")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(map) = entry.as_object() {
            published |= map.get("published").is_some();
            let udp = map.get("protocol").and_then(Value::as_str) == Some("udp");
            add(map.get("target").and_then(port_number), udp, findings);
        } else if let Some(text) = scalar(entry) {
            let (body, udp) = match text.split_once('/') {
                Some((body, protocol)) => (body.to_owned(), protocol == "udp"),
                None => (text.clone(), false),
            };
            published |= body.contains(':');
            let target = body.rsplit(':').next().unwrap_or(&body);
            let first = target.split('-').next().unwrap_or(target);
            add(first.parse().ok().filter(|p| *p > 0), udp, findings);
        }
    }
    for entry in spec
        .get("expose")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(text) = scalar(entry) {
            let (body, udp) = match text.split_once('/') {
                Some((body, protocol)) => (body.to_owned(), protocol == "udp"),
                None => (text, false),
            };
            let first = body.split('-').next().unwrap_or(&body);
            add(first.parse().ok().filter(|p| *p > 0), udp, findings);
        }
    }
    (found, published)
}

fn port_number(value: &Value) -> Option<u16> {
    scalar(value)?.parse().ok().filter(|p| *p > 0)
}

fn depends_on(spec: &Value) -> Vec<String> {
    match spec.get("depends_on") {
        Some(Value::Object(map)) => map.keys().cloned().collect(),
        list @ Some(Value::Array(_)) => strings(list),
        _ => Vec::new(),
    }
}

fn replicas(spec: &Value) -> u32 {
    let asked = spec
        .pointer("/deploy/replicas")
        .or_else(|| spec.get("scale"))
        .and_then(Value::as_u64)
        .unwrap_or(1);
    u32::try_from(asked.clamp(1, MAX_REPLICAS)).unwrap_or(1)
}

/// `env_file` first, then `environment` over it.
fn variables(
    spec: &Value,
    request: &ComposeRequest,
    missing: &mut BTreeSet<String>,
) -> Vec<ComposeVariable> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (path, required) in env_file_paths(spec.get("env_file")) {
        let key = path.strip_prefix("./").unwrap_or(&path);
        let text = request
            .env_files
            .get(key)
            .or_else(|| request.env_files.get(&path));
        match text {
            Some(text) => pairs.extend(parse_env(text)),
            None if required => {
                missing.insert(key.to_owned());
            }
            None => {}
        }
    }
    match spec.get("environment") {
        Some(Value::Object(map)) => {
            for (name, value) in map {
                pairs.push((name.clone(), scalar(value).unwrap_or_default()));
            }
        }
        Some(Value::Array(items)) => {
            for item in items.iter().filter_map(scalar) {
                let (name, value) = item.split_once('=').unwrap_or((&item, ""));
                pairs.push((name.to_owned(), value.to_owned()));
            }
        }
        _ => {}
    }
    let mut out: Vec<ComposeVariable> = Vec::new();
    for (name, value) in pairs {
        out.retain(|v| v.name != name);
        out.push(ComposeVariable {
            name,
            value,
            refers: None,
        });
    }
    out
}

fn env_file_paths(value: Option<&Value>) -> Vec<(String, bool)> {
    match value {
        Some(Value::String(path)) => vec![(path.clone(), true)],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(path) => Some((path.clone(), true)),
                Value::Object(map) => Some((
                    map.get("path").and_then(Value::as_str)?.to_owned(),
                    map.get("required").and_then(Value::as_bool).unwrap_or(true),
                )),
                Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `KEY=value` lines, as Compose reads an env file.
fn parse_env(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (name, value) = line.split_once('=')?;
            let value = value.trim();
            let value = ['"', '\'']
                .iter()
                .find_map(|q| value.strip_prefix(*q).and_then(|v| v.strip_suffix(*q)))
                .unwrap_or(value);
            Some((name.trim().to_owned(), value.to_owned()))
        })
        .collect()
}

fn volumes(spec: &Value, findings: &mut Vec<Finding>) -> Vec<ComposeVolume> {
    let mut out: Vec<ComposeVolume> = Vec::new();
    for entry in spec
        .get("volumes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (kind, source, target, read_only) = match entry {
            Value::String(text) => short_volume(text),
            Value::Object(map) => long_volume(map),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) => continue,
        };
        let Some(target) = target else { continue };
        match kind {
            Mount::Bind if source.ends_with("docker.sock") => {
                findings.push(Finding::DockerSocket { source });
            }
            Mount::Bind => findings.push(Finding::HostPath { source, target }),
            Mount::Tmpfs => {}
            Mount::Volume => {
                let base = if source.is_empty() {
                    "data".to_owned()
                } else {
                    label(&source, 20)
                };
                let base = if base.is_empty() {
                    "data".to_owned()
                } else {
                    base
                };
                let mut name = base.clone();
                let mut n = 2;
                while out.iter().any(|v| v.name == name) {
                    name = format!("{base}-{n}");
                    n += 1;
                }
                out.push(ComposeVolume {
                    name,
                    mount_path: target,
                    read_only,
                });
            }
        }
    }
    out
}

enum Mount {
    Volume,
    Bind,
    Tmpfs,
}

/// `source:target[:mode]` or `target`; a source that is a path is a bind mount.
fn short_volume(text: &str) -> (Mount, String, Option<String>, bool) {
    let parts: Vec<&str> = text.split(':').collect();
    let (source, target, mode) = match parts.as_slice() {
        [target] => ("", *target, ""),
        [source, target] => (*source, *target, ""),
        [source, target, mode, ..] => (*source, *target, *mode),
        [] => ("", "", ""),
    };
    let read_only = mode.split(',').any(|m| m == "ro");
    let kind = if source.starts_with(['/', '.', '~', '$']) {
        Mount::Bind
    } else {
        Mount::Volume
    };
    let target = (!target.is_empty()).then(|| target.to_owned());
    (kind, source.to_owned(), target, read_only)
}

fn long_volume(map: &Map<String, Value>) -> (Mount, String, Option<String>, bool) {
    let text = |key: &str| map.get(key).and_then(Value::as_str).unwrap_or_default();
    let kind = match text("type") {
        "bind" => Mount::Bind,
        "tmpfs" => Mount::Tmpfs,
        _ => Mount::Volume,
    };
    let target = Some(text("target").to_owned()).filter(|t| !t.is_empty());
    let read_only = map
        .get("read_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (kind, text("source").to_owned(), target, read_only)
}

/// Named volumes that several services mount: each service gets its own claim, so say so.
fn mark_shared_volumes(out: &mut [ComposeService], services: &Map<String, Value>) {
    let mut users: BTreeMap<String, usize> = BTreeMap::new();
    let named: Vec<Vec<String>> = services
        .values()
        .map(|spec| {
            spec.get("volumes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|entry| match entry {
                    Value::String(text) => match short_volume(text) {
                        (Mount::Volume, source, ..) if !source.is_empty() => Some(source),
                        _ => None,
                    },
                    Value::Object(map) => match long_volume(map) {
                        (Mount::Volume, source, ..) if !source.is_empty() => Some(source),
                        _ => None,
                    },
                    Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) => None,
                })
                .collect()
        })
        .collect();
    for volume in named.iter().flatten() {
        *users.entry(volume.clone()).or_default() += 1;
    }
    for (service, volumes) in out.iter_mut().zip(named) {
        for volume in volumes {
            if users.get(&volume).is_some_and(|n| *n > 1) {
                service.findings.push(Finding::SharedVolume { volume });
            }
        }
    }
}

/// What a database service sets for its clients, read from its variables.
#[derive(Default)]
struct Credentials {
    user: Option<String>,
    password: Option<String>,
    database: Option<String>,
    /// The service speaks HTTP: an `http://` or `https://` URL to it is its URL.
    http: bool,
}

fn credentials(service: &ComposeService) -> Credentials {
    let get = |names: &[&str]| {
        names.iter().find_map(|n| {
            service
                .variables
                .iter()
                .find(|v| v.name == *n && !v.value.is_empty())
                .map(|v| v.value.clone())
        })
    };
    match service.counterpart {
        Some(Counterpart::Postgresql) => {
            let user = get(&["POSTGRES_USER"]).unwrap_or_else(|| "postgres".to_owned());
            Credentials {
                password: get(&["POSTGRES_PASSWORD"]),
                database: get(&["POSTGRES_DB"]).or_else(|| Some(user.clone())),
                user: Some(user),
                http: false,
            }
        }
        Some(Counterpart::Mysql | Counterpart::Mariadb) => Credentials {
            user: get(&["MYSQL_USER", "MARIADB_USER"]),
            password: get(&[
                "MYSQL_PASSWORD",
                "MARIADB_PASSWORD",
                "MYSQL_ROOT_PASSWORD",
                "MARIADB_ROOT_PASSWORD",
            ]),
            database: get(&["MYSQL_DATABASE", "MARIADB_DATABASE"]),
            http: false,
        },
        Some(Counterpart::Mongodb) => Credentials {
            user: get(&["MONGO_INITDB_ROOT_USERNAME"]),
            password: get(&["MONGO_INITDB_ROOT_PASSWORD"]),
            database: get(&["MONGO_INITDB_DATABASE"]),
            http: false,
        },
        Some(Counterpart::Dragonfly) => Credentials {
            password: get(&["REDIS_PASSWORD", "VALKEY_PASSWORD"]).or_else(|| {
                let args = &service.args;
                args.iter()
                    .position(|a| a == "--requirepass")
                    .and_then(|i| args.get(i + 1).cloned())
            }),
            ..Credentials::default()
        },
        // The root user and password, or their older names.
        Some(Counterpart::ObjectStorage) => Credentials {
            user: get(&["MINIO_ROOT_USER", "MINIO_ACCESS_KEY"]),
            password: get(&["MINIO_ROOT_PASSWORD", "MINIO_SECRET_KEY"]),
            database: None,
            http: true,
        },
        None => Credentials::default(),
    }
}

const URL_SCHEMES: [&str; 8] = [
    "postgres",
    "postgresql",
    "mysql",
    "mariadb",
    "mongodb",
    "redis",
    "rediss",
    "valkey",
];

/// Marks the variables of other services that point at a database service.
fn link_databases(out: &mut [ComposeService], services: &Map<String, Value>, reader: &mut Reader) {
    let databases: Vec<(String, Vec<String>, Credentials)> = out
        .iter()
        .zip(services)
        .filter(|(s, _)| s.counterpart.is_some())
        .map(|(s, (compose_name, spec))| {
            let mut hosts = vec![s.name.clone(), compose_name.clone()];
            if let Some(container) = spec.get("container_name").and_then(Value::as_str) {
                hosts.push(reader.text(container));
            }
            (s.name.clone(), hosts, credentials(s))
        })
        .collect();
    for service in out.iter_mut() {
        for variable in &mut service.variables {
            variable.refers = databases
                .iter()
                .filter(|(db, ..)| *db != service.name)
                .find_map(|(db, hosts, creds)| {
                    part_of(variable, hosts, creds).map(|part| Refers {
                        service: db.clone(),
                        part,
                    })
                });
        }
    }
}

fn part_of(variable: &ComposeVariable, hosts: &[String], creds: &Credentials) -> Option<Part> {
    let value = variable.value.as_str();
    let upper = variable.name.to_ascii_uppercase();
    if let Some((scheme, rest)) = value.split_once("://") {
        let authority = rest.split(['/', '?']).next().unwrap_or(rest);
        let host_port = authority.rsplit('@').next().unwrap_or(authority);
        let host = host_port.split(':').next().unwrap_or(host_port);
        if hosts.iter().any(|h| h == host) {
            return Some(
                if URL_SCHEMES.contains(&scheme)
                    || (creds.http && matches!(scheme, "http" | "https"))
                {
                    Part::Url
                } else {
                    Part::Mention
                },
            );
        }
    }
    let host = value.split(':').next().unwrap_or(value);
    if hosts.iter().any(|h| h == host)
        && value
            .split(':')
            .nth(1)
            .is_none_or(|p| p.parse::<u16>().is_ok())
    {
        return Some(Part::Host);
    }
    let same = |wanted: &Option<String>| wanted.as_deref() == Some(value);
    // The user and the password can have the same value, for example `minioadmin`.
    // Then the variable name selects the part.
    let secret = upper.contains("PASS") || upper.contains("SECRET");
    if same(&creds.user)
        && !secret
        && (upper.contains("USER") || (creds.http && upper.contains("ACCESS")))
    {
        return Some(Part::User);
    }
    if same(&creds.password) {
        return Some(Part::Password);
    }
    if same(&creds.database)
        && (upper.contains("DB") || upper.contains("DATABASE") || upper.contains("NAME"))
    {
        return Some(Part::Database);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const WAIT: &str = "busybox:1.37.0";

    fn convert_yaml(yaml: &str, env_files: &[(&str, &str)]) -> Conversion {
        let request = ComposeRequest {
            yaml: yaml.to_owned(),
            env_files: env_files
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        };
        convert(&request, WAIT).unwrap()
    }

    fn find<'a>(c: &'a Conversion, name: &str) -> &'a ComposeService {
        c.services.iter().find(|s| s.name == name).unwrap()
    }

    fn refers(s: &ComposeService, variable: &str) -> Option<(String, Part)> {
        let found = s.variables.iter().find(|v| v.name == variable);
        let r = found.and_then(|v| v.refers.as_ref())?;
        Some((r.service.clone(), r.part))
    }

    const WEB_APP: &str = r#"
x-common: &common
  restart: unless-stopped
services:
  web:
    <<: *common
    image: ghcr.io/acme/web:${WEB_TAG:-1.4}
    command: node server.js --port "3000"
    ports: ["8080:3000"]
    env_file: [./web.env, {path: ./optional.env, required: false}]
    environment:
      DATABASE_URL: postgres://acme:${DB_PASSWORD}@db:5432/acme
      DB_HOST: db
      DB_USER: acme
      DB_NAME: acme
      DB_PASSWORD: ${DB_PASSWORD}
      REDIS_URL: redis://cache:6379
      JDBC: jdbc:postgresql://db:5432/acme
      PRICE: $$5
    depends_on:
      db: {condition: service_healthy}
      cache: {condition: service_started}
    deploy: {replicas: 3}
  db:
    image: postgres:16
    environment:
      - POSTGRES_USER=acme
      - POSTGRES_PASSWORD=${DB_PASSWORD}
      - POSTGRES_DB=acme
    volumes: [db_data:/var/lib/postgresql/data]
    healthcheck: {test: ["CMD", "pg_isready"]}
  cache:
    image: valkey/valkey:8
    expose: ["6379"]
volumes:
  db_data: {}
"#;

    #[test]
    fn web_app_with_postgres_and_valkey() {
        let c = convert_yaml(
            WEB_APP,
            &[
                (".env", "DB_PASSWORD=s3cret\n"),
                ("web.env", "LOG=info\nDB_HOST=old\n"),
            ],
        );
        assert_eq!(c.missing_env_files, Vec::<String>::new());
        assert_eq!(c.unset, Vec::<String>::new());
        assert_eq!(c.wait_image, WAIT);

        let web = find(&c, "web");
        assert_eq!(web.image.as_deref(), Some("ghcr.io/acme/web:1.4"));
        assert_eq!(web.counterpart, None);
        assert_eq!(web.args, ["node", "server.js", "--port", "3000"]);
        assert_eq!(web.ports, [3000]);
        assert!(web.published);
        assert_eq!(web.replicas, 3);
        assert_eq!(web.depends_on, ["cache", "db"]);
        let names: Vec<&str> = web.variables.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(
            names[..2],
            ["LOG", "DATABASE_URL"],
            "environment wins over env_file"
        );
        assert_eq!(refers(web, "DATABASE_URL"), Some(("db".into(), Part::Url)));
        assert_eq!(refers(web, "DB_HOST"), Some(("db".into(), Part::Host)));
        assert_eq!(
            refers(web, "DB_PASSWORD"),
            Some(("db".into(), Part::Password))
        );
        assert_eq!(refers(web, "DB_USER"), Some(("db".into(), Part::User)));
        assert_eq!(refers(web, "DB_NAME"), Some(("db".into(), Part::Database)));
        assert_eq!(refers(web, "REDIS_URL"), Some(("cache".into(), Part::Url)));
        assert_eq!(refers(web, "JDBC"), Some(("db".into(), Part::Mention)));
        assert_eq!(refers(web, "PRICE"), None);
        assert_eq!(
            web.variables
                .iter()
                .find(|v| v.name == "PRICE")
                .unwrap()
                .value,
            "$5"
        );

        let db = find(&c, "db");
        assert_eq!(db.counterpart, Some(Counterpart::Postgresql));
        assert!(!db.published);
        assert_eq!(db.volumes[0].name, "db-data");
        assert_eq!(db.volumes[0].mount_path, "/var/lib/postgresql/data");
        assert_eq!(db.findings, [Finding::Healthcheck]);
        assert_eq!(find(&c, "cache").counterpart, Some(Counterpart::Dragonfly));
        assert_eq!(find(&c, "cache").ports, [6379]);
    }

    #[test]
    fn minio_clients_point_at_its_url_and_keys() {
        let c = convert_yaml(
            r"
services:
  app:
    image: ghcr.io/acme/app:1
    environment:
      S3_ENDPOINT: http://minio:9000
      S3_ACCESS_KEY_ID: acme
      S3_SECRET_ACCESS_KEY: s3cret
      S3_BUCKET: uploads
  minio:
    image: minio/minio:RELEASE.2025-01-01T00-00-00Z
    command: server /data
    environment:
      MINIO_ROOT_USER: acme
      MINIO_ROOT_PASSWORD: s3cret
",
            &[],
        );
        let app = find(&c, "app");
        assert_eq!(
            find(&c, "minio").counterpart,
            Some(Counterpart::ObjectStorage)
        );
        assert_eq!(
            refers(app, "S3_ENDPOINT"),
            Some(("minio".into(), Part::Url))
        );
        assert_eq!(
            refers(app, "S3_ACCESS_KEY_ID"),
            Some(("minio".into(), Part::User))
        );
        assert_eq!(
            refers(app, "S3_SECRET_ACCESS_KEY"),
            Some(("minio".into(), Part::Password))
        );
        assert_eq!(refers(app, "S3_BUCKET"), None);

        let same = convert_yaml(
            r"
services:
  app:
    image: ghcr.io/acme/app:1
    environment:
      S3_ACCESS_KEY_ID: minioadmin
      S3_SECRET_ACCESS_KEY: minioadmin
  minio:
    image: minio/minio:RELEASE.2025-01-01T00-00-00Z
    environment:
      MINIO_ROOT_USER: minioadmin
      MINIO_ROOT_PASSWORD: minioadmin
",
            &[],
        );
        let app = find(&same, "app");
        assert_eq!(
            refers(app, "S3_ACCESS_KEY_ID"),
            Some(("minio".into(), Part::User))
        );
        assert_eq!(
            refers(app, "S3_SECRET_ACCESS_KEY"),
            Some(("minio".into(), Part::Password))
        );
    }

    #[test]
    fn missing_files_and_unset_variables_are_listed() {
        let c = convert_yaml(WEB_APP, &[]);
        assert_eq!(c.missing_env_files, ["web.env"]);
        assert_eq!(c.unset, ["DB_PASSWORD"]);
        // An empty password matches nothing.
        assert_eq!(refers(find(&c, "web"), "DB_PASSWORD"), None);
    }

    #[test]
    fn host_access_is_listed_and_left_out() {
        let c = convert_yaml(
            r#"
services:
  agent:
    build: ./agent
    privileged: true
    network_mode: host
    cap_add: [NET_ADMIN]
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - ./config:/etc/agent:ro
      - type: tmpfs
        target: /tmp
  Proxy_1:
    image: traefik:v3.2
    build: {context: ./proxy}
    ports: ["80:80", "53:53/udp", {target: 443, published: 443}]
    volumes: [certs:/certs, shared:/srv]
  files:
    image: nginx:1.29
    volumes: [shared:/usr/share/nginx/html:ro]
"#,
            &[],
        );
        let agent = find(&c, "agent");
        assert_eq!(agent.image, None);
        assert_eq!(agent.volumes, []);
        assert_eq!(
            agent.findings,
            [
                Finding::Build {
                    context: "./agent".into()
                },
                Finding::Privileged,
                Finding::HostNetwork,
                Finding::CapAdd {
                    capabilities: vec!["NET_ADMIN".into()]
                },
                Finding::DockerSocket {
                    source: "/var/run/docker.sock".into()
                },
                Finding::HostPath {
                    source: "./config".into(),
                    target: "/etc/agent".into()
                },
            ]
        );
        let proxy = find(&c, "proxy-1");
        assert_eq!(proxy.compose_name, "Proxy_1");
        assert_eq!(proxy.ports, [80, 443]);
        assert!(proxy.findings.contains(&Finding::Renamed));
        assert!(proxy.findings.contains(&Finding::UdpPort { port: 53 }));
        assert!(proxy.findings.contains(&Finding::SharedVolume {
            volume: "shared".into()
        }));
        let files = find(&c, "files");
        assert!(files.volumes[0].read_only);
        assert_eq!(
            files.findings,
            [Finding::SharedVolume {
                volume: "shared".into()
            }]
        );
    }

    #[test]
    fn bad_input_is_refused_with_a_code() {
        let code = |yaml: &str| {
            let request = ComposeRequest {
                yaml: yaml.to_owned(),
                env_files: BTreeMap::new(),
            };
            convert(&request, WAIT).unwrap_err()
        };
        let broken = code("services:\n  web:\n    image: [a\n");
        assert_eq!(broken.code, "invalid_yaml");
        assert!(broken.line.is_some());
        assert_eq!(code("version: '3'\n").code, "no_services");
        assert_eq!(code(&"#".repeat(MAX_YAML + 1)).code, "too_large");
    }

    #[test]
    fn shell_words() {
        assert_eq!(
            split_words(r#"sh -c "echo 'a b'" c\ d '' e"#),
            ["sh", "-c", "echo 'a b'", "c d", "", "e"]
        );
    }
}
