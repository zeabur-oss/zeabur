//! The server's configuration: flags (or their environment variables) and the files they name.
//! Every value is checked when the server starts, and a bad one stops it with a message that
//! names the flag.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use secrecy::SecretString;
use secrecy::zeroize::Zeroizing;
use zeabur_api::plan_token::PlanKey;
use zeabur_auth::client::ExtraRoots;
use zeabur_auth::session::Limits;
use zeabur_auth::url::BaseUrl;

/// Kubernetes made simple and secure by default: the in-cluster server.
///
/// People sign in through Zeabur Identity; register Zeabur there as a confidential client with the
/// redirect URI `<public URL>/auth/callback` and the audience kube-apiserver accepts.
#[derive(Debug, clap::Parser)]
#[command(version)]
pub(crate) struct Args {
    /// Address to listen on. The default takes IPv4 and IPv6 connections where the node has both
    /// (see `listen` in `main.rs`).
    #[arg(long, env = "ZEABUR_LISTEN", default_value = "[::]:8080")]
    pub(crate) listen: SocketAddr,
    /// The URL people open Zeabur at, such as `https://zeabur.example.com`.
    #[arg(long, env = "ZEABUR_PUBLIC_URL")]
    pub(crate) public_url: String,
    /// Zeabur Identity's issuer URL, such as `https://identity.example.com`.
    #[arg(long, env = "ZEABUR_ISSUER_URL")]
    pub(crate) issuer_url: String,
    /// The issuer's page where people manage how they sign in, linked from their account page.
    /// Empty: the `manage` URL of the issuer's `/.well-known/passkey-endpoints`, if it has one.
    #[arg(long, env = "ZEABUR_ACCOUNT_URL", default_value = "")]
    pub(crate) account_url: String,
    /// A PEM file of certificate authorities to trust for the issuer, besides the image's CA
    /// bundle: for an issuer whose certificate a private CA signed.
    #[arg(long, env = "ZEABUR_ISSUER_CA_FILE")]
    pub(crate) issuer_ca_file: Option<PathBuf>,
    /// The `client_id` Zeabur is registered with.
    #[arg(long, env = "ZEABUR_CLIENT_ID", default_value = "zeabur")]
    pub(crate) client_id: String,
    /// A file holding the client secret (a mounted Secret).
    #[arg(long, env = "ZEABUR_CLIENT_SECRET_FILE")]
    pub(crate) client_secret_file: PathBuf,
    /// A file holding the 32-byte key plan tokens are sealed with (a mounted Secret): 32 raw
    /// bytes, or 64 hexadecimal digits.
    #[arg(long, env = "ZEABUR_PLAN_KEY_FILE")]
    pub(crate) plan_key_file: PathBuf,
    /// Offer "Retry as admin" when the cluster refuses a change. The client must be allowed to
    /// ask for elevation in Zeabur Identity (`--allow-elevation`). As an environment variable,
    /// `true` or `false`.
    #[arg(long, env = "ZEABUR_ELEVATION")]
    pub(crate) elevation: bool,
    /// The chart installed the member roles (`zeabur-member`, `zeabur-project-developer`,
    /// `zeabur-project-viewer`), so members can be given rights in chosen projects. As an
    /// environment variable, `true` or `false`.
    #[arg(long, env = "ZEABUR_MEMBER_ROLES")]
    pub(crate) member_roles: bool,
    /// Minutes a session may go unused before it ends.
    #[arg(long, env = "ZEABUR_SESSION_IDLE_MINUTES", default_value_t = 30)]
    pub(crate) session_idle_minutes: u32,
    /// Hours a session lasts at most, used or not.
    #[arg(long, env = "ZEABUR_SESSION_MAX_HOURS", default_value_t = 8)]
    pub(crate) session_max_hours: u32,
    /// The metrics store history is read from, until someone chooses another in the settings: a
    /// Service port serving the Prometheus HTTP API, as `namespace/name:port`. Empty: where
    /// Zeabur Catalog installs VictoriaMetrics.
    #[arg(long, env = "ZEABUR_METRICS_SERVICE", default_value = "")]
    pub(crate) metrics_service: String,
    /// What the interface calls the cluster, until someone renames it in the settings, such as
    /// `shop-prod`. Empty: a name read from the nodes, else the public URL's host.
    #[arg(long, env = "ZEABUR_CLUSTER_NAME", default_value = "")]
    pub(crate) cluster_name: String,
    /// The domains a new service's public URL is suggested under, as
    /// `<service>-<project>.<suffix>`, comma-separated, such as `apps.example.com`, the first by
    /// default; each needs a wildcard DNS record. A list chosen on the cluster page replaces it.
    /// Empty: people type the whole host.
    #[arg(long, env = "ZEABUR_DOMAIN_SUFFIXES", default_value = "")]
    pub(crate) domain_suffixes: String,
    /// The cert-manager `ClusterIssuer` that signs a new public URL's certificate. Empty: a new
    /// URL is HTTP only.
    #[arg(long, env = "ZEABUR_CLUSTER_ISSUER", default_value = "")]
    pub(crate) cluster_issuer: String,
    /// The registry Zeabur Builder pushes to and nodes pull from, such as `zot.zot.svc:5000`; a
    /// service deployed from Git runs `<registry>/<project>/<service>`. Empty: services are not
    /// deployed from Git.
    #[arg(long, env = "ZEABUR_BUILD_REGISTRY", default_value = "")]
    pub(crate) build_registry: String,
    /// A PEM file of the certificate authorities that sign the build registry's certificate, on
    /// top of the platform's roots. Needs `--build-registry`.
    #[arg(long, env = "ZEABUR_BUILD_REGISTRY_CA_FILE")]
    pub(crate) build_registry_ca_file: Option<PathBuf>,
    /// A PEM file of certificate authorities Zeabur trusts, on top of the platform's roots, for
    /// every HTTPS connection it makes: the issuer, registries, object stores and GitHub.
    #[arg(long, env = "ZEABUR_EXTRA_CA_FILE")]
    pub(crate) extra_ca_file: Option<PathBuf>,
    /// The namespace Zeabur runs in, where the ConfigMap `zeabur-settings` keeps the settings
    /// people choose. Empty: settings cannot be changed.
    #[arg(long, env = "ZEABUR_NAMESPACE", default_value = "")]
    pub(crate) namespace: String,
    /// The image of a debug container, for a terminal in a container without a shell; it needs
    /// `sh`. Empty: busybox, pinned by digest.
    #[arg(long, env = "ZEABUR_DEBUG_IMAGE", default_value = "")]
    pub(crate) debug_image: String,
    /// The image of a tools pod; it needs `sh`, `sleep`, `kubectl` and `helm`. Empty:
    /// `alpine/k8s`, pinned by digest.
    #[arg(long, env = "ZEABUR_TOOLS_IMAGE", default_value = "")]
    pub(crate) tools_image: String,
    /// The namespace all tools pods go in. Only Zeabur's own service account uses it.
    #[arg(long, env = "ZEABUR_TERMINAL_NAMESPACE", default_value = zeabur_api::settings::TERMINAL_NAMESPACE)]
    pub(crate) terminal_namespace: String,
    /// A file with Zeabur's own service account token, which may use pods in
    /// `--terminal-namespace` only. Empty: no tools pods.
    #[arg(long, env = "ZEABUR_TERMINAL_TOKEN_FILE", default_value = "")]
    pub(crate) terminal_token_file: String,
    /// Minutes a tools pod may run at most.
    #[arg(long, env = "ZEABUR_TOOLS_DEADLINE_MINUTES", default_value_t = 30)]
    pub(crate) tools_deadline_minutes: u32,
    /// Seconds after its last terminal closes that a tools pod is deleted.
    #[arg(long, env = "ZEABUR_TOOLS_GRACE_SECONDS", default_value_t = 60)]
    pub(crate) tools_grace_seconds: u32,
    /// The tools container's requests, as `cpu=10m,memory=64Mi`. Empty: these.
    #[arg(long, env = "ZEABUR_TOOLS_REQUESTS", default_value = "")]
    pub(crate) tools_requests: String,
    /// The tools container's limits, as `cpu=1,memory=512Mi`. Empty: these.
    #[arg(long, env = "ZEABUR_TOOLS_LIMITS", default_value = "")]
    pub(crate) tools_limits: String,
    /// Zeabur Proxy, for log history, as `https://host[:port]`. Empty: there is no log history.
    #[arg(long, env = "ZEABUR_PROXY_URL", default_value = "")]
    pub(crate) proxy_url: String,
    /// A PEM file of the certificate authorities that sign Zeabur Proxy's certificate; the only
    /// ones trusted for it. Required with `--proxy-url`.
    #[arg(long, env = "ZEABUR_PROXY_CA_FILE")]
    pub(crate) proxy_ca_file: Option<PathBuf>,
}

/// The configuration, checked.
#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) public_url: BaseUrl,
    pub(crate) issuer: BaseUrl,
    pub(crate) issuer_ca: ExtraRoots,
    pub(crate) client_id: String,
    pub(crate) client_secret: SecretString,
    pub(crate) plan_key: PlanKey,
    pub(crate) elevation: bool,
    pub(crate) limits: Limits,
    pub(crate) settings: zeabur_api::Settings,
    pub(crate) proxy: Option<crate::proxy::Proxy>,
}

impl Args {
    /// Checks every value and reads the files.
    #[expect(clippy::too_many_lines, reason = "one step per flag")]
    pub(crate) fn check(&self) -> Result<Config, String> {
        let public_url =
            BaseUrl::parse_root(&self.public_url).map_err(|e| format!("--public-url: {e}"))?;
        let issuer = BaseUrl::parse(&self.issuer_url).map_err(|e| format!("--issuer-url: {e}"))?;
        let issuer_ca = match bundle(&[
            (&self.issuer_ca_file, "--issuer-ca-file"),
            (&self.extra_ca_file, "--extra-ca-file"),
        ])? {
            Some(pem) => {
                ExtraRoots::from_pem(&pem).map_err(|e| format!("--issuer-ca-file: {e}"))?
            }
            None => ExtraRoots::default(),
        };
        if self.client_id.is_empty() {
            return Err("--client-id: must not be empty".to_owned());
        }
        let secret = read(&self.client_secret_file, "--client-secret-file")?;
        let secret = std::str::from_utf8(&secret)
            .map_err(|_utf8| "--client-secret-file: the secret is not text".to_owned())?
            .trim();
        if secret.is_empty() {
            return Err(format!(
                "--client-secret-file: {} is empty",
                self.client_secret_file.display()
            ));
        }
        let plan_key =
            plan_key(&read(&self.plan_key_file, "--plan-key-file")?).ok_or_else(|| {
                format!(
                    "--plan-key-file: {} must hold 32 bytes, or 64 hexadecimal digits",
                    self.plan_key_file.display()
                )
            })?;
        let mut settings = zeabur_api::Settings::default();
        if !self.metrics_service.is_empty() {
            settings.metrics_store =
                zeabur_api::settings::parse_service_port(&self.metrics_service)
                    .map_err(|e| format!("--metrics-service: {e}"))?;
        }
        if !self.namespace.is_empty() {
            if !zeabur_plan::checks::is_dns_label(&self.namespace) {
                return Err(format!(
                    "--namespace: {:?} is not a namespace's name",
                    self.namespace
                ));
            }
            settings.namespace = Some(self.namespace.clone());
        }
        if !self.cluster_name.trim().is_empty() {
            settings.cluster_name = Some(
                zeabur_api::settings::parse_cluster_name(&self.cluster_name)
                    .map_err(|e| format!("--cluster-name: {e}"))?,
            );
        }
        (settings.domains, settings.builds) = (domains(self)?, builds(self)?);
        settings.trusted_ca = self.trusted_ca(&settings.builds)?;
        let authority = public_url.authority();
        settings.public_host = Some(
            authority
                .strip_prefix('[')
                .and_then(|rest| rest.split_once(']'))
                .map_or_else(
                    || authority.split(':').next().unwrap_or(authority),
                    |(host, _)| host,
                )
                .to_owned(),
        );
        settings.identity.issuer = Some(issuer.to_string());
        settings.identity.project_roles = self.member_roles;
        if !self.account_url.is_empty() {
            settings.account_url = Some(
                zeabur_auth::url::page_url(&self.account_url)
                    .map_err(|e| format!("--account-url: {e}"))?,
            );
        }
        terminal(self, &mut settings.terminal)?;
        let proxy = match (self.proxy_url.as_str(), &self.proxy_ca_file) {
            ("", None) => None,
            ("", Some(_)) => return Err("--proxy-ca-file needs --proxy-url".to_owned()),
            (_, None) => return Err("--proxy-url needs --proxy-ca-file".to_owned()),
            (url, Some(path)) => Some(
                crate::proxy::Proxy::new(url, &read(path, "--proxy-ca-file")?)
                    .map_err(|e| format!("--proxy-url, --proxy-ca-file: {e}"))?,
            ),
        };
        if self.session_idle_minutes == 0 || self.session_max_hours == 0 {
            return Err(
                "--session-idle-minutes and --session-max-hours must be at least 1".to_owned(),
            );
        }
        Ok(Config {
            public_url,
            issuer,
            issuer_ca,
            client_id: self.client_id.clone(),
            client_secret: SecretString::from(secret),
            plan_key,
            elevation: self.elevation,
            limits: Limits {
                idle: i64::from(self.session_idle_minutes) * 60,
                absolute: i64::from(self.session_max_hours) * 3600,
                ..Limits::default()
            },
            settings,
            proxy,
        })
    }

    /// One PEM bundle of the extra CAs and the build registry's CA.
    fn trusted_ca(
        &self,
        builds: &zeabur_api::settings::BuildSettings,
    ) -> Result<Option<Vec<u8>>, String> {
        if self.build_registry_ca_file.is_some() && builds.registry.is_none() {
            return Err("--build-registry-ca-file needs --build-registry".to_owned());
        }
        bundle(&[
            (&self.extra_ca_file, "--extra-ca-file"),
            (&self.build_registry_ca_file, "--build-registry-ca-file"),
        ])
    }
}

/// The PEM files given, each checked, joined into one bundle; `None` when none is given.
fn bundle(files: &[(&Option<PathBuf>, &str)]) -> Result<Option<Vec<u8>>, String> {
    let mut bundle: Option<Vec<u8>> = None;
    for &(path, flag) in files {
        let Some(path) = path else { continue };
        let pem = read(path, flag)?;
        ExtraRoots::from_pem(&pem).map_err(|e| format!("{flag}: {}: {e}", path.display()))?;
        let bundle = bundle.get_or_insert_with(Vec::new);
        if !bundle.is_empty() && !bundle.ends_with(b"\n") {
            bundle.push(b'\n');
        }
        bundle.extend_from_slice(&pem);
    }
    Ok(bundle)
}

/// How new public URLs are made, from `args`.
fn domains(args: &Args) -> Result<zeabur_api::settings::DomainSettings, String> {
    let mut domains = zeabur_api::settings::DomainSettings::default();
    domains.suffixes = zeabur_api::settings::domains::parse_suffixes(&args.domain_suffixes)
        .map_err(|e| format!("--domain-suffixes: {e}"))?;
    let issuer = args.cluster_issuer.trim();
    if !issuer.is_empty() {
        if !zeabur_plan::checks::is_dns_subdomain(issuer) {
            return Err(format!(
                "--cluster-issuer: {:?} is not a ClusterIssuer's name",
                args.cluster_issuer
            ));
        }
        domains.cluster_issuer = Some(issuer.to_owned());
    }
    Ok(domains)
}

/// Where services deployed from Git keep their images, from `args`.
fn builds(args: &Args) -> Result<zeabur_api::settings::BuildSettings, String> {
    let registry = args.build_registry.trim().trim_end_matches('/');
    if registry.is_empty() {
        return Ok(zeabur_api::settings::BuildSettings::default());
    }
    let (host, port) = registry
        .split_once(':')
        .map_or((registry, None), |(host, port)| (host, Some(port)));
    let port_ok = port.is_none_or(|p| p.parse::<u16>().is_ok_and(|p| p > 0));
    if !(zeabur_plan::checks::is_dns_subdomain(host) && port_ok) {
        return Err(format!(
            "--build-registry: {:?} is not a registry such as zot.zot.svc:5000",
            args.build_registry
        ));
    }
    Ok(zeabur_api::settings::BuildSettings {
        registry: Some(registry.to_owned()),
    })
}

/// The terminal's settings from `args`.
fn terminal(
    args: &Args,
    terminal: &mut zeabur_api::settings::TerminalSettings,
) -> Result<(), String> {
    for (flag, image, setting) in [
        (
            "--debug-image",
            &args.debug_image,
            &mut terminal.debug_image,
        ),
        (
            "--tools-image",
            &args.tools_image,
            &mut terminal.tools_image,
        ),
    ] {
        if image.chars().any(char::is_whitespace) {
            return Err(format!("{flag}: {image:?} is not an image reference"));
        }
        if !image.is_empty() {
            image.clone_into(setting);
        }
    }
    if !zeabur_plan::checks::is_dns_label(&args.terminal_namespace) {
        return Err(format!(
            "--terminal-namespace: {:?} is not a namespace's name",
            args.terminal_namespace
        ));
    }
    args.terminal_namespace.clone_into(&mut terminal.namespace);
    if !args.terminal_token_file.is_empty() {
        terminal.token_file = Some(args.terminal_token_file.clone().into());
    }
    if args.tools_deadline_minutes == 0 {
        return Err("--tools-deadline-minutes must be at least 1".to_owned());
    }
    terminal.tools_deadline_seconds = i64::from(args.tools_deadline_minutes) * 60;
    terminal.tools_grace = std::time::Duration::from_secs(u64::from(args.tools_grace_seconds));
    for (flag, text, section) in [
        (
            "--tools-requests",
            &args.tools_requests,
            &mut terminal.tools_resources.requests,
        ),
        (
            "--tools-limits",
            &args.tools_limits,
            &mut terminal.tools_resources.limits,
        ),
    ] {
        if !text.is_empty() {
            *section = quantities(text).map_err(|e| format!("{flag}: {e}"))?;
        }
    }
    let resources = serde_json::json!({
        "requests": terminal.tools_resources.requests,
        "limits": terminal.tools_resources.limits,
    });
    zeabur_plan::checks::validate_resources(&resources).map_err(|_refused| {
        "--tools-requests, --tools-limits: a quantity is not valid, or a request is above \
             its limit"
            .to_owned()
    })
}

/// `cpu=10m,memory=64Mi` as quantities by resource.
fn quantities(text: &str) -> Result<std::collections::BTreeMap<String, String>, String> {
    text.split(',')
        .map(|pair| {
            let (name, value) = pair
                .trim()
                .split_once('=')
                .filter(|(n, v)| zeabur_plan::checks::COMPUTE.contains(n) && !v.is_empty())
                .ok_or_else(|| {
                    format!("{pair:?} is not resource=quantity (cpu, memory or ephemeral-storage)")
                })?;
            Ok((name.to_owned(), value.to_owned()))
        })
        .collect()
}

/// The file's bytes, cleared on drop: the client secret and the plan key are read this way.
fn read(path: &Path, flag: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    std::fs::read(path)
        .map(Zeroizing::new)
        .map_err(|e| format!("{flag}: cannot read {}: {e}", path.display()))
}

/// The key in `bytes`: exactly 32 bytes (a Secret made with Helm's `randBytes 32`, mounted as a
/// file, holds the decoded bytes), or 64 hexadecimal digits with optional surrounding space.
fn plan_key(bytes: &[u8]) -> Option<PlanKey> {
    if let Ok(raw) = <[u8; 32]>::try_from(bytes) {
        return Some(PlanKey::from_bytes(raw));
    }
    let text = std::str::from_utf8(bytes).ok()?.trim();
    if text.len() != 64 {
        return None;
    }
    let mut key = [0_u8; 32];
    for (byte, pair) in key.iter_mut().zip(text.as_bytes().chunks(2)) {
        let pair = std::str::from_utf8(pair).ok()?;
        *byte = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(PlanKey::from_bytes(key))
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::*;

    struct Files(PathBuf);

    impl Files {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("zeabur-config-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, name: &str, content: &[u8]) -> String {
            let path = self.0.join(name);
            std::fs::write(&path, content).unwrap();
            path.display().to_string()
        }
    }

    impl Drop for Files {
        fn drop(&mut self) {
            let _removed = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A valid command line, with the flags in `extra` replacing or adding to it.
    fn args(files: &Files, extra: &[&str]) -> Result<Config, String> {
        let secret = files.write("secret", b"s3cret\n");
        let key = files.write("key", &[7; 32]);
        let mut flags: Vec<(&str, Option<&str>)> = vec![
            ("--public-url", Some("https://zeabur.example.com")),
            ("--issuer-url", Some("https://identity.example.com")),
            ("--client-secret-file", Some(&secret)),
            ("--plan-key-file", Some(&key)),
        ];
        let mut extra = extra.iter().peekable();
        while let Some(flag) = extra.next() {
            let value = extra.next_if(|v| !v.starts_with("--")).copied();
            flags.retain(|(name, _)| name != flag);
            flags.push((flag, value));
        }
        let mut argv = vec!["zeabur"];
        for (flag, value) in flags {
            argv.push(flag);
            argv.extend(value);
        }
        Args::try_parse_from(argv)
            .map_err(|e| e.to_string())?
            .check()
    }

    #[test]
    fn a_complete_configuration_is_accepted() {
        let files = Files::new("ok");
        let config = args(&files, &["--elevation"]).unwrap();
        assert_eq!(config.public_url.origin(), "https://zeabur.example.com");
        assert_eq!(config.client_id, "zeabur");
        assert!(config.elevation);
        assert_eq!(config.limits.idle, 1800);
        assert_eq!(config.limits.absolute, 8 * 3600);
        assert!(!format!("{config:?}").contains("s3cret"));
    }

    #[test]
    fn bad_values_name_their_flag() {
        let files = Files::new("bad");
        let error = args(&files, &["--public-url", "http://zeabur.example.com"]).unwrap_err();
        assert!(error.starts_with("--public-url:"), "{error}");
        let error = args(&files, &["--public-url", "https://example.com/zeabur"]).unwrap_err();
        assert!(error.starts_with("--public-url:"), "{error}");
        let error = args(&files, &["--issuer-url", "zeabur-identity"]).unwrap_err();
        assert!(error.starts_with("--issuer-url:"), "{error}");
        let error = args(&files, &["--account-url", "http://id.example.com/me"]).unwrap_err();
        assert!(error.starts_with("--account-url:"), "{error}");
        let empty = files.write("empty", b"\n");
        let error = args(&files, &["--client-secret-file", &empty]).unwrap_err();
        assert!(error.starts_with("--client-secret-file:"), "{error}");
        let short = files.write("short", b"0123");
        let error = args(&files, &["--plan-key-file", &short]).unwrap_err();
        assert!(error.starts_with("--plan-key-file:"), "{error}");
        let error = args(&files, &["--plan-key-file", "/nonexistent/key"]).unwrap_err();
        assert!(error.contains("cannot read"), "{error}");
        let error = args(&files, &["--session-idle-minutes", "0"]).unwrap_err();
        assert!(error.contains("at least 1"), "{error}");
        let error = args(&files, &["--metrics-service", "prometheus:9090"]).unwrap_err();
        assert!(error.starts_with("--metrics-service:"), "{error}");
        let error = args(&files, &["--namespace", "Default"]).unwrap_err();
        assert!(error.starts_with("--namespace:"), "{error}");
        let error = args(&files, &["--terminal-namespace", "Tools"]).unwrap_err();
        assert!(error.starts_with("--terminal-namespace:"), "{error}");
        let error = args(&files, &["--tools-requests", "gpu=1"]).unwrap_err();
        assert!(error.starts_with("--tools-requests:"), "{error}");
        let error = args(&files, &["--tools-requests", "memory=1Gi"]).unwrap_err();
        assert!(error.contains("above"), "{error}");
        let error = args(&files, &["--tools-deadline-minutes", "0"]).unwrap_err();
        assert!(error.contains("at least 1"), "{error}");
    }

    #[test]
    fn the_terminal_images_namespace_and_limits_can_be_chosen() {
        let files = Files::new("terminal");
        let config = args(
            &files,
            &[
                "--debug-image",
                "registry.example.com/busybox:1",
                "--terminal-namespace",
                "ops-terminals",
                "--terminal-token-file",
                "/var/run/zeabur/terminal-token/token",
                "--tools-deadline-minutes",
                "10",
                "--tools-grace-seconds",
                "5",
                "--tools-limits",
                "cpu=500m,memory=256Mi",
            ],
        )
        .unwrap();
        let terminal = &config.settings.terminal;
        assert_eq!(terminal.debug_image, "registry.example.com/busybox:1");
        assert_eq!(terminal.tools_image, zeabur_api::settings::TOOLS_IMAGE);
        assert_eq!(terminal.namespace, "ops-terminals");
        assert_eq!(
            terminal.token_file.as_deref(),
            Some(Path::new("/var/run/zeabur/terminal-token/token"))
        );
        assert_eq!(terminal.tools_deadline_seconds, 600);
        assert_eq!(terminal.tools_grace, std::time::Duration::from_secs(5));
        assert_eq!(terminal.tools_resources.limits["memory"], "256Mi");
        assert_eq!(terminal.tools_resources.requests["cpu"], "10m");
    }

    #[test]
    fn the_metrics_store_and_the_settings_namespace_are_optional() {
        let files = Files::new("settings");
        let config = args(&files, &[]).unwrap();
        assert_eq!(
            config.settings,
            zeabur_api::Settings {
                public_host: Some("zeabur.example.com".to_owned()),
                identity: zeabur_api::settings::IdentitySettings {
                    issuer: Some("https://identity.example.com".to_owned()),
                    ..zeabur_api::settings::IdentitySettings::default()
                },
                ..zeabur_api::Settings::default()
            }
        );
        let config = args(
            &files,
            &["--account-url", "https://id.example.com/me?tab=keys"],
        )
        .unwrap();
        assert_eq!(
            config.settings.account_url.as_deref(),
            Some("https://id.example.com/me?tab=keys")
        );
        let config = args(&files, &["--cluster-name", " shop-prod ", "--member-roles"]).unwrap();
        assert_eq!(config.settings.cluster_name.as_deref(), Some("shop-prod"));
        assert!(config.settings.identity.project_roles);
        let error = args(&files, &["--cluster-name", &"x".repeat(64)]).unwrap_err();
        assert!(error.starts_with("--cluster-name:"), "{error}");
        let config = args(
            &files,
            &[
                "--domain-suffixes",
                "*.apps.example.com, example.com",
                "--cluster-issuer",
                "letsencrypt",
            ],
        )
        .unwrap();
        assert_eq!(
            config.settings.domains,
            zeabur_api::settings::DomainSettings {
                suffixes: vec!["apps.example.com".to_owned(), "example.com".to_owned()],
                cluster_issuer: Some("letsencrypt".to_owned()),
            }
        );
        let error = args(&files, &["--domain-suffixes", "Apps_example"]).unwrap_err();
        assert!(error.starts_with("--domain-suffixes:"), "{error}");
        let error = args(&files, &["--cluster-issuer", "Let's Encrypt"]).unwrap_err();
        assert!(error.starts_with("--cluster-issuer:"), "{error}");
        let config = args(&files, &["--build-registry", "zot.zot.svc:5000/"]).unwrap();
        assert_eq!(
            config.settings.builds.registry.as_deref(),
            Some("zot.zot.svc:5000")
        );
        assert_eq!(args(&files, &[]).unwrap().settings.builds.registry, None);
        for bad in ["zot:http", "Zot_Registry", "zot.svc/team"] {
            let error = args(&files, &["--build-registry", bad]).unwrap_err();
            assert!(error.starts_with("--build-registry:"), "{error}");
        }
        let config = args(&files, &["--public-url", "https://zeabur.localhost:8443"]).unwrap();
        assert_eq!(
            config.settings.public_host.as_deref(),
            Some("zeabur.localhost")
        );
        let config = args(
            &files,
            &[
                "--metrics-service",
                "monitoring/prometheus-operated:9090",
                "--namespace",
                "zeabur",
            ],
        )
        .unwrap();
        assert_eq!(
            config.settings.metrics_store.to_string(),
            "monitoring/prometheus-operated:9090"
        );
        assert_eq!(config.settings.namespace.as_deref(), Some("zeabur"));
    }

    /// A CA certificate (P-256, self-signed, valid until 2126).
    const CA: &str = "-----BEGIN CERTIFICATE-----
MIIBijCCAS+gAwIBAgIUTAkyfSekaru/E/zhMgRNcVk+Qe0wCgYIKoZIzj0EAwIw
GTEXMBUGA1UEAwwOZW5nYXdhLXRlc3QtY2EwIBcNMjYxMDE1MTcxNTM4WhgPMjEy
NjA5MjExNzE1MzhaMBkxFzAVBgNVBAMMDmVuZ2F3YS10ZXN0LWNhMFkwEwYHKoZI
zj0CAQYIKoZIzj0DAQcDQgAE720TSZbicUQ1m/v3x51a1BQVi2hq4Mjf4FS6laTL
WZ2wI+YLgtRh622S7Pciy8BSjR16JycnxhrDcwTCIa9sLaNTMFEwHQYDVR0OBBYE
FEsUIXZLs5y6TfytXJNiclk6fRD8MB8GA1UdIwQYMBaAFEsUIXZLs5y6TfytXJNi
clk6fRD8MA8GA1UdEwEB/wQFMAMBAf8wCgYIKoZIzj0EAwIDSQAwRgIhAIqSqg3C
RJbgDzng/5L4lBJcsdbrYMMCDGvbt+PBjLMGAiEA2vHazr8gyrA6Gc+COXzzRY0R
qkDcEXA7cf4S+NsEneg=
-----END CERTIFICATE-----
";

    #[test]
    fn an_issuer_ca_file_adds_its_certificates() {
        let files = Files::new("ca");
        assert!(args(&files, &[]).unwrap().issuer_ca.is_empty());
        let ca = files.write("ca.crt", format!("# a private CA\n{CA}").as_bytes());
        assert_eq!(
            args(&files, &["--issuer-ca-file", &ca])
                .unwrap()
                .issuer_ca
                .len(),
            1
        );
        let none = files.write("none.crt", b"not a certificate\n");
        let error = args(&files, &["--issuer-ca-file", &none]).unwrap_err();
        assert!(error.starts_with("--issuer-ca-file:"), "{error}");
        let error = args(&files, &["--issuer-ca-file", "/nonexistent/ca.crt"]).unwrap_err();
        assert!(error.starts_with("--issuer-ca-file:"), "{error}");
    }

    #[test]
    fn a_build_registry_ca_file_is_checked_and_needs_a_registry() {
        let files = Files::new("registry-ca");
        assert_eq!(args(&files, &[]).unwrap().settings.trusted_ca, None);
        let ca = files.write("ca.crt", CA.as_bytes());
        let config = args(
            &files,
            &[
                "--build-registry",
                "zot.zot.svc:5000",
                "--build-registry-ca-file",
                &ca,
            ],
        )
        .unwrap();
        assert_eq!(config.settings.trusted_ca.as_deref(), Some(CA.as_bytes()));
        let error = args(&files, &["--build-registry-ca-file", &ca]).unwrap_err();
        assert!(
            error.starts_with("--build-registry-ca-file needs"),
            "{error}"
        );
        let none = files.write("none.crt", b"not a certificate\n");
        let error = args(
            &files,
            &[
                "--build-registry",
                "zot.zot.svc:5000",
                "--build-registry-ca-file",
                &none,
            ],
        )
        .unwrap_err();
        assert!(error.starts_with("--build-registry-ca-file:"), "{error}");
    }

    #[test]
    fn an_extra_ca_file_is_trusted_with_the_build_registry_ca() {
        let files = Files::new("extra-ca");
        let ca = files.write("ca.crt", CA.as_bytes());
        let alone = args(&files, &["--extra-ca-file", &ca]).unwrap();
        assert_eq!(alone.settings.trusted_ca.as_deref(), Some(CA.as_bytes()));
        let both = args(
            &files,
            &[
                "--extra-ca-file",
                &ca,
                "--build-registry",
                "zot.zot.svc:5000",
                "--build-registry-ca-file",
                &ca,
            ],
        )
        .unwrap();
        let bundle = String::from_utf8(both.settings.trusted_ca.unwrap()).unwrap();
        assert_eq!(
            bundle.trim_end(),
            format!("{}\n{}", CA.trim_end(), CA.trim_end())
        );
        let none = files.write("none.crt", b"not a certificate\n");
        let error = args(&files, &["--extra-ca-file", &none]).unwrap_err();
        assert!(error.starts_with("--extra-ca-file:"), "{error}");
    }

    #[test]
    fn the_issuer_trusts_the_extra_ca_file_too() {
        let files = Files::new("issuer-extra-ca");
        let ca = files.write("ca.crt", CA.as_bytes());
        assert!(
            !args(&files, &["--extra-ca-file", &ca])
                .unwrap()
                .issuer_ca
                .is_empty()
        );
        let both = args(&files, &["--extra-ca-file", &ca, "--issuer-ca-file", &ca]).unwrap();
        assert_eq!(both.issuer_ca.len(), 2);
    }

    #[test]
    fn zeabur_proxy_takes_an_https_origin_and_its_ca() {
        let files = Files::new("proxy");
        assert!(args(&files, &[]).unwrap().proxy.is_none());
        let ca = files.write("ca.crt", CA.as_bytes());
        let url = "https://zeabur-proxy.zeabur.svc";
        assert!(
            args(&files, &["--proxy-url", url, "--proxy-ca-file", &ca])
                .unwrap()
                .proxy
                .is_some()
        );
        for (flags, starts) in [
            (vec!["--proxy-url", url], "--proxy-url needs"),
            (vec!["--proxy-ca-file", &ca], "--proxy-ca-file needs"),
            (
                vec!["--proxy-url", "http://zeabur-proxy", "--proxy-ca-file", &ca],
                "--proxy-url, --proxy-ca-file:",
            ),
        ] {
            let error = args(&files, &flags).unwrap_err();
            assert!(error.starts_with(starts), "{error}");
        }
        let none = files.write("none.crt", b"not a certificate\n");
        let error = args(&files, &["--proxy-url", url, "--proxy-ca-file", &none]).unwrap_err();
        assert!(
            error.starts_with("--proxy-url, --proxy-ca-file:"),
            "{error}"
        );
    }

    #[test]
    fn a_plan_key_is_raw_bytes_or_hex() {
        assert!(plan_key(&[1; 32]).is_some());
        assert!(plan_key(format!("{}\n", "ab".repeat(32)).as_bytes()).is_some());
        assert!(plan_key("zz".repeat(32).as_bytes()).is_none());
        assert!(plan_key(&[1; 31]).is_none());
        assert!(plan_key("ab".repeat(33).as_bytes()).is_none());
    }
}
