//! The installer's configuration file.
//!
//! One file holds every choice, so the same install can be planned again, kept in version control
//! and used as a test input. Flags only override it. Secret values never appear here: a secret
//! input names a file (mode 0600) the installer reads, or is generated.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::PlanError;

/// The whole configuration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    /// Which components to install.
    #[serde(default)]
    pub preset: Preset,
    /// Capabilities or component ids added to the preset.
    #[serde(default)]
    pub enable: Vec<String>,
    /// Capabilities or component ids taken out of the preset.
    #[serde(default)]
    pub disable: Vec<String>,
    /// `single-node` or `multi-node`; one server is `single-node`.
    #[serde(default = "single_node")]
    pub profile: String,
    /// How people reach Zeabur and Zeabur Identity.
    pub access: Access,
    /// Install Zeabur Identity and Zeabur; `false` installs RKE2 and the components only.
    #[serde(default = "yes")]
    pub zeabur: bool,
    /// Values for catalog inputs, by component id then input name.
    #[serde(default)]
    pub inputs: BTreeMap<String, BTreeMap<String, Value>>,
    /// Files holding secret catalog inputs, by component id then input name.
    #[serde(default)]
    pub secret_files: BTreeMap<String, BTreeMap<String, String>>,
    /// An RKE2 release other than the pinned one, such as `v1.37.1+rke2r1`. Its downloads are
    /// checked only against the release's own checksum list, so the plan warns that it is
    /// unsupported.
    pub rke2_version: Option<String>,
    /// Host settings.
    #[serde(default)]
    pub host: Host,
    /// Releases to install instead of the pinned ones, for development builds.
    #[serde(default)]
    pub sources: Sources,
}

/// A named set of components.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    /// Ingress, certificates and metrics-server: what Zeabur needs to be reached and to show usage.
    Minimal,
    /// Everything the catalog recommends for the profile.
    #[default]
    Standard,
    /// `standard` with runtime detection, vulnerability scanning and intrusion prevention.
    Secure,
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
}

/// Development sources, used while a component has no published, pinned release.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sources {
    /// A catalog `index.json` on disk.
    pub catalog: Option<String>,
    /// Image archives (`docker save` output) RKE2 imports on start.
    #[serde(default)]
    pub image_archives: Vec<String>,
    /// Zeabur Identity's image reference.
    pub identity_image: Option<String>,
    /// A `zeabur-identityctl` binary.
    pub identityctl: Option<String>,
    /// Zeabur's image reference.
    pub zeabur_image: Option<String>,
    /// Zeabur's chart: a directory or a packaged `.tgz`.
    pub zeabur_chart: Option<String>,
}

/// A configuration file: a new cluster, or a node joining one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Document {
    /// The first server: RKE2, the components, Zeabur Identity and Zeabur.
    Install(Config),
    /// Another node: RKE2 only, joined to an existing cluster.
    Join(JoinConfig),
}

impl Document {
    /// Parses a configuration file of either kind; a top-level `join` key makes it a join.
    ///
    /// # Errors
    ///
    /// When the YAML does not match or a value is invalid.
    pub fn parse(yaml: &str) -> Result<Self, PlanError> {
        let value: Value =
            serde_saphyr::from_str(yaml).map_err(|e| PlanError::Config(e.to_string()))?;
        if value.get("join").is_some() {
            JoinConfig::parse(yaml).map(Self::Join)
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
    /// An RKE2 release other than the pinned one; it must match the cluster's.
    pub rke2_version: Option<String>,
    /// Host settings. `tlsSan` applies to servers only.
    #[serde(default)]
    pub host: Host,
    /// Image archives RKE2 imports on start.
    #[serde(default)]
    pub sources: JoinSources,
}

/// What a joining node may take from disk instead of a registry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinSources {
    /// Image archives (`docker save` output) RKE2 imports on start.
    #[serde(default)]
    pub image_archives: Vec<String>,
}

/// The `join` section. `Debug` leaves the token out.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Join {
    /// The RKE2 supervisor of a server: `https://<address>:9345`.
    pub server: String,
    /// `worker` (default) or `server`.
    #[serde(default)]
    pub role: Role,
    /// The join token, `K10<ca-hash>::<credentials>`. The configuration file must then be mode
    /// 0600.
    pub token: Option<String>,
    /// A file (mode 0600) holding the join token.
    pub token_file: Option<String>,
    /// The server's provider and region, as `node add` printed them.
    pub expect: Option<Location>,
    /// Settings every server of the cluster must share, as `node add --role server` printed them.
    pub server_config: Option<ServerConfig>,
    /// The cluster runs Longhorn, so this node gets its iSCSI initiator, NFS client and kernel
    /// module before RKE2 starts.
    #[serde(default)]
    pub longhorn: bool,
}

impl std::fmt::Debug for Join {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Join")
            .field("server", &self.server)
            .field("role", &self.role)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("token_file", &self.token_file)
            .field("expect", &self.expect)
            .field("server_config", &self.server_config)
            .field("longhorn", &self.longhorn)
            .finish()
    }
}

/// What a joining node runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// An RKE2 agent: runs workloads.
    #[default]
    Worker,
    /// An RKE2 server: control plane and etcd, and workloads.
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

/// Where a machine runs, from its cloud's instance metadata.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Location {
    /// `aws`, `gcp`, `azure`, `digitalocean`, `hetzner`, `vultr`, `linode` or `oracle`.
    pub provider: String,
    /// The provider's region, such as `ap-northeast-1` or `fsn1`.
    pub region: String,
    /// The availability zone, when the provider has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
}

/// RKE2 server settings that must match on every server.
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
        server_address(&join.server)?;
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
        Ok(())
    }
}

/// The host and port of a join URL, `https://<host>:<port>` with nothing after it.
///
/// # Errors
///
/// When the URL is not of that form.
pub fn server_address(url: &str) -> Result<(String, u16), PlanError> {
    let bad = || {
        PlanError::Config(format!(
            "join.server `{url}` is not of the form https://<address>:9345"
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
        None => 9345,
    };
    if host.is_empty() {
        return Err(bad());
    }
    Ok((host.to_owned(), port))
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
        return Err(PlanError::Config(
            "join token: a bootstrap token (<id>.<secret>) joins workers only; a server needs the              server token from `zeabur-installer node add --role server`"
                .to_owned(),
        ));
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
    /// The configuration used when there is no file: a test machine reached through sslip.io
    /// names for `address`, with the standard preset.
    #[must_use]
    pub fn defaults(address: &str) -> Self {
        let name = address.replace(':', "-");
        Self {
            preset: Preset::default(),
            enable: Vec::new(),
            disable: Vec::new(),
            profile: single_node(),
            access: Access {
                mode: AccessMode::Local,
                identity: format!("login.{name}.sslip.io"),
                zeabur: format!("zeabur.{name}.sslip.io"),
                invitations: true,
            },
            zeabur: true,
            inputs: BTreeMap::new(),
            secret_files: BTreeMap::new(),
            rke2_version: None,
            host: Host::default(),
            sources: Sources::default(),
        }
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
        if self.access.identity == self.access.zeabur {
            return Err(PlanError::Config(
                "access.identity and access.zeabur need different host names: an issuer is a whole origin"
                    .to_owned(),
            ));
        }
        Ok(())
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
        assert_eq!(c.preset, Preset::Standard);
        assert_eq!(c.profile, "single-node");
        assert!(c.zeabur);
        assert!(c.access.invitations);
        assert_eq!(c.issuer(), "https://login.example.com");
        let off = MINIMAL.replace('}', ", invitations: false}");
        assert!(!Config::parse(&off).unwrap().access.invitations);
    }

    #[test]
    fn rejects_urls_and_shared_hosts() {
        let url = MINIMAL.replace("login.example.com", "https://login.example.com");
        assert!(Config::parse(&url).is_err());
        let shared = MINIMAL.replace("zeabur.example.com", "login.example.com");
        assert!(Config::parse(&shared).is_err());
        assert!(Config::parse(&format!("{MINIMAL}typo: 1\n")).is_err());
    }

    const HASH: &str = "2c2a4a3f5e0e5e7f3c1b1d9a8f7e6d5c4b3a29181716151413121110090807aa";

    fn join(extra: &str) -> Result<Document, PlanError> {
        Document::parse(&format!(
            "join:\n  server: https://10.0.0.5:9345\n  token: K10{HASH}::abcdef.0123456789abcdef\n{extra}"
        ))
    }

    #[test]
    fn a_join_key_makes_a_join_configuration() {
        let Document::Join(c) = join("  expect: {provider: hetzner, region: fsn1}\n").unwrap()
        else {
            panic!("not a join");
        };
        assert_eq!(c.join.role, Role::Worker);
        assert_eq!(
            c.join.expect,
            Some(Location {
                provider: "hetzner".to_owned(),
                region: "fsn1".to_owned(),
                zone: None
            })
        );
        assert!(!format!("{c:?}").contains("0123456789abcdef"));
        assert!(matches!(
            Document::parse(MINIMAL).unwrap(),
            Document::Install(_)
        ));
    }

    #[test]
    fn join_rejects_what_does_not_fit() {
        // A joining node installs RKE2 only.
        assert!(join("access: {mode: local, identity: a.b.c, zeabur: d.b.c}\n").is_err());
        assert!(join("host: {tlsSan: [k8s.example.com]}\n").is_err());
        assert!(join("  tokenFile: /etc/zeabur-installer/join-token\n").is_err());
        // Bootstrap tokens cannot join servers.
        let err = join("  role: server\n  serverConfig: {identity: true}\n").unwrap_err();
        assert!(err.to_string().contains("workers only"), "{err}");
        let server = Document::parse(&format!(
            "join:\n  server: https://10.0.0.5:9345\n  role: server\n  token: K10{HASH}::server:s3cret\n  serverConfig: {{disable: [rke2-metrics-server], identity: true}}\n"
        ));
        assert!(matches!(server, Ok(Document::Join(_))), "{server:?}");
        let no_server_config = Document::parse(&format!(
            "join:\n  server: https://10.0.0.5:9345\n  role: server\n  token: K10{HASH}::server:s3cret\n"
        ));
        assert!(no_server_config.is_err());
    }

    #[test]
    fn server_urls_and_tokens() {
        assert_eq!(
            server_address("https://10.0.0.5:9345").unwrap(),
            ("10.0.0.5".to_owned(), 9345)
        );
        assert_eq!(
            server_address("https://[fd00::5]:9345/").unwrap(),
            ("fd00::5".to_owned(), 9345)
        );
        assert_eq!(
            server_address("https://k8s.example.com").unwrap(),
            ("k8s.example.com".to_owned(), 9345)
        );
        for bad in [
            "http://10.0.0.5:9345",
            "https://10.0.0.5:9345/v1",
            "https://:9345",
            "https://fd00::5",
        ] {
            assert!(server_address(bad).is_err(), "{bad}");
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
