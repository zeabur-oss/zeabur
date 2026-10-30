//! The installer's configuration file.
//!
//! One file holds every choice, so the same install can be planned again, kept in version control
//! and used as a test input. Flags only override it. Secret values never appear here: a secret
//! input names a file (mode 0600) the installer reads, or is generated.

use std::collections::BTreeMap;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::PlanError;
use crate::cluster::Distribution;

/// The whole configuration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    /// `team`, the default; `personal` makes a [`PersonalConfig`].
    #[serde(default, rename = "use", skip_serializing_if = "Use::is_team")]
    pub use_: Use,
    /// Capabilities or component ids added to the defaults.
    #[serde(default)]
    pub enable: Vec<String>,
    /// Capabilities or component ids taken out of the defaults.
    #[serde(default)]
    pub disable: Vec<String>,
    /// `single-node` or `multi-node`; one server is `single-node`.
    #[serde(default = "single_node")]
    pub profile: String,
    /// How people reach Zeabur and Zeabur Identity.
    pub access: Access,
    /// `k3s`, the default, or `rke2` for a control plane of three servers.
    #[serde(default)]
    pub distribution: Distribution,
    /// Install Zeabur Identity and Zeabur; `false` installs Kubernetes and the components only.
    #[serde(default = "yes")]
    pub zeabur: bool,
    /// Values for catalog inputs, by component id then input name.
    #[serde(default)]
    pub inputs: BTreeMap<String, BTreeMap<String, Value>>,
    /// Files holding secret catalog inputs, by component id then input name.
    #[serde(default)]
    pub secret_files: BTreeMap<String, BTreeMap<String, String>>,
    /// Components `apply` may remove although that deletes the data they keep, such as
    /// `replicated-storage/longhorn`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_data_loss: Vec<String>,
    /// An RKE2 release other than the pinned one, such as `v1.37.1+rke2r1`. Its downloads are
    /// checked only against the release's own checksum list, so the plan warns that it is
    /// unsupported.
    pub rke2_version: Option<String>,
    /// A K3s release other than the pinned one, such as `v1.37.1+k3s1`, checked the same way.
    pub k3s_version: Option<String>,
    /// Host settings.
    #[serde(default)]
    pub host: Host,
    /// Releases to install instead of the pinned ones, for development builds.
    #[serde(default)]
    pub sources: Sources,
}

/// Who the install is for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Use {
    /// One person: `KubeSolo` and the components, reached with `zeaburctl ui` and the admin
    /// kubeconfig. No isolation, roles or other nodes.
    Personal,
    /// A team: K3s or RKE2, the components, Zeabur Identity and Zeabur.
    #[default]
    Team,
}

impl Use {
    /// Whether it is [`Use::Team`].
    #[must_use]
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if passes a reference"
    )]
    pub const fn is_team(&self) -> bool {
        matches!(self, Self::Team)
    }
}

/// A personal install: `KubeSolo`, the standalone helm-controller and the components.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonalConfig {
    /// Always `personal`.
    #[serde(rename = "use")]
    pub use_: Use,
    /// Capabilities or component ids added to the personal set.
    #[serde(default)]
    pub enable: Vec<String>,
    /// Capabilities or component ids taken out of the personal set.
    #[serde(default)]
    pub disable: Vec<String>,
    /// Values for catalog inputs, by component id then input name.
    #[serde(default)]
    pub inputs: BTreeMap<String, BTreeMap<String, Value>>,
    /// Files holding secret catalog inputs, by component id then input name.
    #[serde(default)]
    pub secret_files: BTreeMap<String, BTreeMap<String, String>>,
    /// Components `apply` may remove although that deletes the data they keep, such as
    /// `replicated-storage/longhorn`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_data_loss: Vec<String>,
    /// Host settings.
    #[serde(default)]
    pub host: Host,
    /// Releases to use instead of the pinned ones, for development builds.
    #[serde(default)]
    pub sources: PersonalSources,
}

/// Files a development build of a personal install takes from this host.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonalSources {
    /// The catalog's `index.json`.
    pub catalog: Option<String>,
}

impl PersonalConfig {
    /// The configuration used when there is no file.
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            use_: Use::Personal,
            enable: Vec::new(),
            disable: Vec::new(),
            inputs: BTreeMap::new(),
            secret_files: BTreeMap::new(),
            allow_data_loss: Vec::new(),
            host: Host::default(),
            sources: PersonalSources::default(),
        }
    }

    /// Parses and checks a personal configuration file.
    ///
    /// # Errors
    ///
    /// When the YAML does not match or `use` is not `personal`.
    pub fn parse(yaml: &str) -> Result<Self, PlanError> {
        let config: Self =
            serde_saphyr::from_str(yaml).map_err(|e| PlanError::Config(e.to_string()))?;
        config.check()?;
        Ok(config)
    }

    /// Checks the values serde cannot.
    ///
    /// # Errors
    ///
    /// When `use` is not `personal`.
    pub fn check(&self) -> Result<(), PlanError> {
        if self.use_ != Use::Personal {
            return Err(PlanError::Config(
                "use: this file has the fields of a personal install; set `use: personal`"
                    .to_owned(),
            ));
        }
        self.host.check()
    }
}

/// How people reach the two web services.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Access {
    /// Where certificates come from.
    pub mode: AccessMode,
    /// Zeabur Identity's host name; its issuer is `https://<identity>`.
    pub identity: String,
    /// Zeabur's host name.
    pub zeabur: String,
    /// Accept invitation links, the ones Zeabur's Members page makes. Turn off when people and
    /// their groups are managed only from version control.
    #[serde(default = "yes")]
    pub invitations: bool,
    /// The domains new services' URLs are suggested under, the first one by default, such as
    /// `apps.example.com`. Each needs a wildcard record, `*.apps.example.com`, pointing at the
    /// server. Absent: the server's sslip.io name when its address is public, else none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffixes: Option<Vec<String>>,
}

/// Where certificates come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccessMode {
    /// Public DNS names pointing at the server; Let's Encrypt through cert-manager (HTTP-01).
    Domain,
    /// A private CA made on the server, for test machines only: browsers must trust it by hand.
    Local,
}

/// Host settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Host {
    /// Extra names and addresses for the API server certificate.
    #[serde(default)]
    pub tls_san: Vec<String>,
    /// Stop when the root or data directory is not on an encrypted device.
    #[serde(default)]
    pub require_encrypted_disk: bool,
    /// The node's name in Kubernetes. Absent: the host name in lower case, unless it is made
    /// from the host's address, then `node-<6 hex>`. Once installed, a node keeps its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
}

impl Host {
    fn check(&self) -> Result<(), PlanError> {
        self.node_name
            .as_deref()
            .map_or(Ok(()), crate::node_name::check)
    }
}

/// Development sources, used while a component has no published, pinned release.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sources {
    /// A catalog `index.json` on disk.
    pub catalog: Option<String>,
    /// Image archives (`docker save` output) K3s or RKE2 imports on start.
    #[serde(default)]
    pub image_archives: Vec<String>,
    /// Zeabur Identity's image reference.
    pub identity_image: Option<String>,
    /// A `zeabur-identityctl` binary.
    pub identityctl: Option<String>,
    /// Zeabur's image reference.
    pub zeabur_image: Option<String>,
    /// Zeabur Proxy's image reference; the chart's own default when unset.
    pub proxy_image: Option<String>,
    /// Zeabur's chart: a directory or a packaged `.tgz`.
    pub zeabur_chart: Option<String>,
}

/// A configuration file: a new cluster, or a node joining one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Document {
    /// The first server: K3s or RKE2, the components, Zeabur Identity and Zeabur.
    Install(Config),
    /// Another node: K3s or RKE2 only, joined to an existing cluster.
    Join(JoinConfig),
    /// A personal install on `KubeSolo`.
    Personal(PersonalConfig),
}

impl Document {
    /// Parses a configuration file of any kind: a top-level `join` key makes it a join, and
    /// `use: personal` a personal install.
    ///
    /// # Errors
    ///
    /// When the YAML does not match or a value is invalid.
    pub fn parse(yaml: &str) -> Result<Self, PlanError> {
        let value: Value =
            serde_saphyr::from_str(yaml).map_err(|e| PlanError::Config(e.to_string()))?;
        if value.get("join").is_some() {
            JoinConfig::parse(yaml).map(Self::Join)
        } else if value.get("use").and_then(Value::as_str) == Some("personal") {
            PersonalConfig::parse(yaml).map(Self::Personal)
        } else {
            Config::parse(yaml).map(Self::Install)
        }
    }
}

/// A node joining an existing cluster.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinConfig {
    /// The cluster to join.
    pub join: Join,
    /// The cluster's distribution, as `node add` printed it.
    #[serde(default)]
    pub distribution: Distribution,
    /// An RKE2 release other than the pinned one; it must match the cluster's.
    pub rke2_version: Option<String>,
    /// A K3s release other than the pinned one; it must match the cluster's.
    pub k3s_version: Option<String>,
    /// Host settings. `tlsSan` applies to servers only.
    #[serde(default)]
    pub host: Host,
    /// Image archives the node imports on start.
    #[serde(default)]
    pub sources: JoinSources,
}

/// What a joining node may take from disk instead of a registry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinSources {
    /// Image archives (`docker save` output) the node imports on start.
    #[serde(default)]
    pub image_archives: Vec<String>,
}

/// The `join` section. `Debug` leaves the token out.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Join {
    /// The server to join through: `https://<address>:6443` on K3s, `https://<address>:9345`
    /// (RKE2's supervisor) on RKE2.
    pub server: String,
    /// `worker` (default) or `server`.
    #[serde(default)]
    pub role: Role,
    /// The join token, `K10<ca-hash>::<credentials>`. The configuration file must then be mode
    /// 0600.
    pub token: Option<String>,
    /// A file (mode 0600) holding the join token.
    pub token_file: Option<String>,
    /// Settings every server of the cluster must share, as `node add --role server` printed them.
    pub server_config: Option<ServerConfig>,
    /// The cluster runs Longhorn, so this node gets its iSCSI initiator, NFS client and kernel
    /// module before Kubernetes starts on it.
    #[serde(default)]
    pub longhorn: bool,
    /// The cluster runs an in-cluster registry, so this node pulls its images through a mirror, as
    /// `node add` printed it.
    pub registry: Option<RegistryMirror>,
    /// The names of the cluster's nodes, as `node add` printed them; this node takes another.
    #[serde(default)]
    pub node_names: Vec<String>,
}

/// The in-cluster registry's host, fixed `ClusterIP` and the CA of its certificate.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryMirror {
    /// `<service>.<namespace>.svc:<port>`, as images name it.
    pub host: String,
    /// Its Service's `ClusterIP`.
    pub cluster_ip: IpAddr,
    /// The CA, PEM.
    pub ca: String,
}

impl std::fmt::Debug for Join {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Join")
            .field("server", &self.server)
            .field("role", &self.role)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("token_file", &self.token_file)
            .field("server_config", &self.server_config)
            .field("longhorn", &self.longhorn)
            .field("registry", &self.registry)
            .field("node_names", &self.node_names)
            .finish()
    }
}

/// What a joining node runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// An agent: runs workloads.
    #[default]
    Worker,
    /// An RKE2 server: control plane and etcd, and workloads. K3s takes no second server.
    Server,
}

impl Role {
    /// `worker` or `server`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Worker => "worker",
            Self::Server => "server",
        }
    }
}

/// Server settings that must match on every server.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerConfig {
    /// RKE2's packaged components the cluster disables, such as `rke2-metrics-server`.
    #[serde(default)]
    pub disable: Vec<String>,
    /// kube-apiserver trusts Zeabur Identity.
    #[serde(default)]
    pub identity: bool,
}

impl JoinConfig {
    /// Parses and checks a join configuration.
    ///
    /// # Errors
    ///
    /// When the YAML does not match or a value is invalid.
    pub fn parse(yaml: &str) -> Result<Self, PlanError> {
        let config: Self =
            serde_saphyr::from_str(yaml).map_err(|e| PlanError::Config(e.to_string()))?;
        config.check()?;
        Ok(config)
    }

    /// Checks the values serde cannot.
    ///
    /// # Errors
    ///
    /// The first invalid value.
    pub fn check(&self) -> Result<(), PlanError> {
        let join = &self.join;
        check_version(
            self.distribution,
            self.rke2_version.as_ref(),
            self.k3s_version.as_ref(),
        )?;
        if join.role == Role::Server && !self.distribution.takes_servers() {
            return Err(PlanError::SecondServer);
        }
        server_address(&join.server, self.distribution.join_port())?;
        if let Some(registry) = &join.registry
            && !registry
                .ca
                .trim_start()
                .starts_with("-----BEGIN CERTIFICATE-----")
        {
            return Err(PlanError::Config(
                "join.registry.ca is not a PEM certificate: copy it from the output of `zeabur-installer node add`"
                    .to_owned(),
            ));
        }
        match (&join.token, &join.token_file) {
            (Some(_), Some(_)) => {
                return Err(PlanError::Config(
                    "join: set token or tokenFile, not both".to_owned(),
                ));
            }
            (None, None) => {
                return Err(PlanError::Config(
                    "join: set tokenFile (a file, mode 0600, holding the token `node add` printed) or token"
                        .to_owned(),
                ));
            }
            (Some(token), None) => {
                check_token(token, join.role)?;
            }
            (None, Some(_)) => {}
        }
        if join.role == Role::Worker {
            if !self.host.tls_san.is_empty() {
                return Err(PlanError::Config(
                    "host.tlsSan applies to servers; a worker serves no Kubernetes API".to_owned(),
                ));
            }
            if join.server_config.is_some() {
                return Err(PlanError::Config(
                    "join.serverConfig applies to role: server".to_owned(),
                ));
            }
        } else if join.server_config.is_none() {
            return Err(PlanError::Config(
                "join.serverConfig is missing: copy it from the output of `zeabur-installer node add --role server`"
                    .to_owned(),
            ));
        }
        self.host.check()
    }
}

/// The host and port of a join URL, `https://<host>:<port>` with nothing after it; without a
/// port, `default_port`.
///
/// # Errors
///
/// When the URL is not of that form.
pub fn server_address(url: &str, default_port: u16) -> Result<(String, u16), PlanError> {
    let bad = || {
        PlanError::Config(format!(
            "join.server `{url}` is not of the form https://<address>:{default_port}"
        ))
    };
    let rest = url.strip_prefix("https://").ok_or_else(bad)?;
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    if rest.contains('/') || rest.contains('@') || rest.is_empty() {
        return Err(bad());
    }
    let (host, port) = if let Some(v6) = rest.strip_prefix('[') {
        let (host, after) = v6.split_once(']').ok_or_else(bad)?;
        (host, after.strip_prefix(':'))
    } else {
        match rest.rsplit_once(':') {
            Some((host, port)) if !host.contains(':') => (host, Some(port)),
            Some(_) => return Err(bad()),
            None => (rest, None),
        }
    };
    let port = match port {
        Some(p) => p.parse::<u16>().ok().ok_or_else(bad)?,
        None => default_port,
    };
    if host.is_empty() {
        return Err(bad());
    }
    Ok((host.to_owned(), port))
}

/// The release override of the other distribution is a mistake: it would be ignored.
fn check_version(
    distribution: Distribution,
    rke2: Option<&String>,
    k3s: Option<&String>,
) -> Result<(), PlanError> {
    match (distribution, rke2, k3s) {
        (Distribution::K3s, Some(_), _) => Err(PlanError::Config(
            "rke2Version applies to distribution: rke2; this one is k3s (set k3sVersion)"
                .to_owned(),
        )),
        (Distribution::Rke2, _, Some(_)) => Err(PlanError::Config(
            "k3sVersion applies to distribution: k3s; this one is rke2 (set rke2Version)"
                .to_owned(),
        )),
        _ => Ok(()),
    }
}

/// A join token: `K10<64 hex>::<credentials>`, or bare credentials (no CA pinning).
///
/// # Errors
///
/// When it is malformed, or a bootstrap token is given to a server.
pub fn check_token(token: &str, role: Role) -> Result<TokenKind, PlanError> {
    let token = token.trim();
    let credentials = match token.strip_prefix("K10") {
        Some(rest) => {
            let (hash, credentials) = rest.split_once("::").ok_or_else(|| {
                PlanError::Config("join token: expected K10<ca-hash>::<credentials>".to_owned())
            })?;
            let hex = hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit());
            if !hash.is_empty() && !hex {
                return Err(PlanError::Config(
                    "join token: the CA hash is not 64 hexadecimal digits".to_owned(),
                ));
            }
            credentials
        }
        None => token,
    };
    if credentials.is_empty() || credentials.chars().any(char::is_whitespace) {
        return Err(PlanError::Config("join token: no credentials".to_owned()));
    }
    let kind = if is_bootstrap_token(credentials) {
        TokenKind::Bootstrap
    } else {
        TokenKind::Server
    };
    if kind == TokenKind::Bootstrap && role == Role::Server {
        return Err(PlanError::BootstrapTokenForServer);
    }
    Ok(kind)
}

/// Whether a join token pins the cluster CA.
#[must_use]
pub fn token_pins_ca(token: &str) -> bool {
    token
        .trim()
        .strip_prefix("K10")
        .and_then(|rest| rest.split_once("::"))
        .is_some_and(|(hash, _)| !hash.is_empty())
}

/// What a join token authenticates as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// A Kubernetes bootstrap token, `<id>.<secret>`: joins workers, expires.
    Bootstrap,
    /// The cluster's server or agent token.
    Server,
}

/// `[a-z0-9]{6}.[a-z0-9]{16}`.
#[must_use]
pub fn is_bootstrap_token(credentials: &str) -> bool {
    let ok = |s: &str, n: usize| {
        s.len() == n
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    };
    credentials
        .split_once('.')
        .is_some_and(|(id, secret)| ok(id, 6) && ok(secret, 16))
}

fn single_node() -> String {
    "single-node".to_owned()
}

const fn yes() -> bool {
    true
}

impl Config {
    /// The configuration used when there is no file: sslip.io names for the first public address,
    /// else for the first address, with the default components. A public address gets Let's Encrypt
    /// certificates; another address is a test machine with a local CA.
    #[must_use]
    pub fn defaults(addresses: &[String]) -> Option<Self> {
        let address = addresses
            .iter()
            .find(|a| crate::plan::is_public(a))
            .or_else(|| addresses.first())?;
        let mode = if crate::plan::is_public(address) {
            AccessMode::Domain
        } else {
            AccessMode::Local
        };
        let name = address.replace(':', "-");
        Some(Self {
            use_: Use::Team,
            enable: Vec::new(),
            disable: Vec::new(),
            profile: single_node(),
            access: Access {
                mode,
                identity: format!("login.{name}.sslip.io"),
                zeabur: format!("zeabur.{name}.sslip.io"),
                invitations: true,
                suffixes: None,
            },
            zeabur: true,
            inputs: BTreeMap::new(),
            secret_files: BTreeMap::new(),
            allow_data_loss: Vec::new(),
            distribution: Distribution::default(),
            rke2_version: None,
            k3s_version: None,
            host: Host::default(),
            sources: Sources::default(),
        })
    }

    /// Parses and checks a configuration file.
    ///
    /// # Errors
    ///
    /// When the YAML does not match or a value is invalid.
    pub fn parse(yaml: &str) -> Result<Self, PlanError> {
        let config: Self =
            serde_saphyr::from_str(yaml).map_err(|e| PlanError::Config(e.to_string()))?;
        config.check()?;
        Ok(config)
    }

    /// Checks the values serde cannot.
    ///
    /// # Errors
    ///
    /// The first invalid value.
    pub fn check(&self) -> Result<(), PlanError> {
        if self.use_ != Use::Team {
            return Err(PlanError::Config(
                "use: personal takes only enable, disable, inputs, secretFiles, host and sources"
                    .to_owned(),
            ));
        }
        check_version(
            self.distribution,
            self.rke2_version.as_ref(),
            self.k3s_version.as_ref(),
        )?;
        if !matches!(self.profile.as_str(), "single-node" | "multi-node") {
            return Err(PlanError::Config(format!(
                "profile `{}` is not single-node or multi-node",
                self.profile
            )));
        }
        for (what, host) in [
            ("access.identity", &self.access.identity),
            ("access.zeabur", &self.access.zeabur),
        ] {
            if !is_host_name(host) {
                return Err(PlanError::Config(format!(
                    "{what} `{host}` is not a host name: write it without https:// or a path"
                )));
            }
        }
        for suffix in self.access.suffixes.iter().flatten() {
            if !is_host_name(suffix) {
                return Err(PlanError::Config(format!(
                    "access.suffixes: `{suffix}` is not a domain name such as apps.example.com; \
                     write it without `*.`"
                )));
            }
        }
        if self.access.identity == self.access.zeabur {
            return Err(PlanError::Config(
                "access.identity and access.zeabur need different host names: an issuer is a whole origin"
                    .to_owned(),
            ));
        }
        self.host.check()
    }

    /// The release chosen instead of the pinned one, for the distribution.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        match self.distribution {
            Distribution::K3s => self.k3s_version.as_deref(),
            Distribution::Rke2 => self.rke2_version.as_deref(),
        }
    }

    /// Zeabur Identity's issuer URL.
    #[must_use]
    pub fn issuer(&self) -> String {
        format!("https://{}", self.access.identity)
    }

    /// Zeabur's public URL.
    #[must_use]
    pub fn zeabur_url(&self) -> String {
        format!("https://{}", self.access.zeabur)
    }
}

/// A DNS name of letters, digits and hyphens, at least two labels.
fn is_host_name(name: &str) -> bool {
    let labels: Vec<&str> = name.split('.').collect();
    name.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str =
        "access: {mode: domain, identity: login.example.com, zeabur: zeabur.example.com}\n";

    #[test]
    fn defaults() {
        let c = Config::parse(MINIMAL).unwrap();
        assert_eq!(c.profile, "single-node");
        assert!(c.zeabur);
        assert!(c.access.invitations);
        assert_eq!(c.issuer(), "https://login.example.com");
        let off = MINIMAL.replace('}', ", invitations: false}");
        assert!(!Config::parse(&off).unwrap().access.invitations);
        let wildcard = MINIMAL.replace('}', ", suffixes: ['*.apps.example.com']}");
        assert!(Config::parse(&wildcard).is_err());
    }

    #[test]
    fn without_a_file_a_public_address_gets_lets_encrypt() {
        let defaults = |addresses: &[&str]| {
            let addresses: Vec<String> = addresses.iter().map(|&a| a.to_owned()).collect();
            Config::defaults(&addresses).map(|c| (c.access.mode, c.access.zeabur))
        };
        assert_eq!(
            defaults(&["10.0.0.5", "203.0.114.7"]),
            Some((AccessMode::Domain, "zeabur.203.0.114.7.sslip.io".to_owned()))
        );
        for private in ["10.0.0.5", "100.64.0.1", "169.254.1.1"] {
            assert_eq!(
                defaults(&[private]),
                Some((AccessMode::Local, format!("zeabur.{private}.sslip.io")))
            );
        }
        assert_eq!(defaults(&[]), None);
    }

    #[test]
    fn rejects_urls_and_shared_hosts() {
        let url = MINIMAL.replace("login.example.com", "https://login.example.com");
        assert!(Config::parse(&url).is_err());
        let shared = MINIMAL.replace("zeabur.example.com", "login.example.com");
        assert!(Config::parse(&shared).is_err());
        assert!(Config::parse(&format!("{MINIMAL}typo: 1\n")).is_err());
    }

    #[test]
    fn a_node_name_must_be_a_kubernetes_name() {
        let named = Config::parse(&format!("{MINIMAL}host: {{nodeName: db-1}}\n")).unwrap();
        assert_eq!(named.host.node_name.as_deref(), Some("db-1"));
        assert!(Config::parse(&format!("{MINIMAL}host: {{nodeName: DB_1}}\n")).is_err());
        assert!(PersonalConfig::parse("use: personal\nhost: {nodeName: -solo}\n").is_err());
        assert!(join("host: {nodeName: Worker.1}\n").is_err());
        assert!(join("host: {nodeName: worker-1}\n").is_ok());
    }

    #[test]
    fn use_personal_makes_a_personal_configuration() {
        let Document::Personal(c) = Document::parse(
            "use: personal\nenable: [databases]\nhost: {tlsSan: [a.example.com]}\n",
        )
        .unwrap() else {
            panic!("not personal");
        };
        assert_eq!(c.enable, ["databases"]);
        assert!(Document::parse("use: personal\naccess: {mode: local}\n").is_err());
        assert!(Document::parse("use: personal\nprofile: multi-node\n").is_err());
        let team = format!("use: team\n{MINIMAL}");
        assert!(matches!(
            Document::parse(&team).unwrap(),
            Document::Install(_)
        ));
        assert!(PersonalConfig::parse("use: team\n").is_err());
    }

    const HASH: &str = "2c2a4a3f5e0e5e7f3c1b1d9a8f7e6d5c4b3a29181716151413121110090807aa";

    fn join(extra: &str) -> Result<Document, PlanError> {
        Document::parse(&format!(
            "distribution: rke2\njoin:\n  server: https://10.0.0.5:9345\n  token: K10{HASH}::abcdef.0123456789abcdef\n{extra}"
        ))
    }

    #[test]
    fn a_join_key_makes_a_join_configuration() {
        let Document::Join(c) = join("").unwrap() else {
            panic!("not a join");
        };
        assert_eq!(c.join.role, Role::Worker);
        assert!(!format!("{c:?}").contains("0123456789abcdef"));
        assert!(matches!(
            Document::parse(MINIMAL).unwrap(),
            Document::Install(_)
        ));
    }

    #[test]
    fn join_rejects_what_does_not_fit() {
        // A joining node installs K3s or RKE2 only.
        assert!(join("access: {mode: local, identity: a.b.c, zeabur: d.b.c}\n").is_err());
        assert!(join("host: {tlsSan: [k8s.example.com]}\n").is_err());
        assert!(join("  tokenFile: /etc/zeabur-installer/join-token\n").is_err());
        // Bootstrap tokens cannot join servers.
        let err = join("  role: server\n  serverConfig: {identity: true}\n").unwrap_err();
        assert!(matches!(err, PlanError::BootstrapTokenForServer), "{err}");
        let server = Document::parse(&format!(
            "distribution: rke2\njoin:\n  server: https://10.0.0.5:9345\n  role: server\n  token: K10{HASH}::server:s3cret\n  serverConfig: {{disable: [rke2-metrics-server], identity: true}}\n"
        ));
        assert!(matches!(server, Ok(Document::Join(_))), "{server:?}");
        let no_server_config = Document::parse(&format!(
            "distribution: rke2\njoin:\n  server: https://10.0.0.5:9345\n  role: server\n  token: K10{HASH}::server:s3cret\n"
        ));
        assert!(no_server_config.is_err());
    }

    #[test]
    fn k3s_takes_workers_and_points_a_second_server_at_rke2() {
        let worker = Document::parse(&format!(
            "join:\n  server: https://10.0.0.5:6443\n  token: K10{HASH}::abcdef.0123456789abcdef\n"
        ));
        let Ok(Document::Join(worker)) = worker else {
            panic!("{worker:?}");
        };
        assert_eq!(worker.distribution, Distribution::K3s);
        let err = Document::parse(&format!(
            "join:\n  server: https://10.0.0.5:6443\n  role: server\n  token: K10{HASH}::server:s3cret\n  serverConfig: {{identity: true}}\n"
        ))
        .unwrap_err();
        assert!(matches!(err, PlanError::SecondServer), "{err}");
        assert!(err.to_string().contains("distribution: rke2"), "{err}");
    }

    #[test]
    fn a_release_override_must_match_the_distribution() {
        assert!(Config::parse(&format!("{MINIMAL}k3sVersion: v1.37.1+k3s1\n")).is_ok());
        assert!(Config::parse(&format!("{MINIMAL}rke2Version: v1.37.1+rke2r1\n")).is_err());
        let rke2 = Config::parse(&format!(
            "{MINIMAL}distribution: rke2\nrke2Version: v1.37.1+rke2r1\n"
        ))
        .unwrap();
        assert_eq!(rke2.version(), Some("v1.37.1+rke2r1"));
        assert!(
            Config::parse(&format!(
                "{MINIMAL}distribution: rke2\nk3sVersion: v1.37.1+k3s1\n"
            ))
            .is_err()
        );
    }

    #[test]
    fn server_urls_and_tokens() {
        assert_eq!(
            server_address("https://10.0.0.5:9345", 6443).unwrap(),
            ("10.0.0.5".to_owned(), 9345)
        );
        assert_eq!(
            server_address("https://[fd00::5]:9345/", 9345).unwrap(),
            ("fd00::5".to_owned(), 9345)
        );
        assert_eq!(
            server_address("https://k8s.example.com", 6443).unwrap(),
            ("k8s.example.com".to_owned(), 6443)
        );
        for bad in [
            "http://10.0.0.5:9345",
            "https://10.0.0.5:9345/v1",
            "https://:9345",
            "https://fd00::5",
        ] {
            assert!(server_address(bad, 9345).is_err(), "{bad}");
        }
        assert_eq!(
            check_token("abcdef.0123456789abcdef", Role::Worker).unwrap(),
            TokenKind::Bootstrap
        );
        assert!(!token_pins_ca("abcdef.0123456789abcdef"));
        assert!(token_pins_ca(&format!(
            "K10{HASH}::abcdef.0123456789abcdef"
        )));
        assert!(check_token("K10abc::abcdef.0123456789abcdef", Role::Worker).is_err());
        assert!(check_token("K10", Role::Worker).is_err());
        assert_eq!(
            check_token(&format!("K10{HASH}::server:x"), Role::Server).unwrap(),
            TokenKind::Server
        );
    }
}
