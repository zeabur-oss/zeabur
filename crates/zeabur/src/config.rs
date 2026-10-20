//! The server's configuration: flags (or their environment variables) and the files they name.
//! Every value is checked when the server starts, and a bad one stops it with a message that
//! names the flag.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use secrecy::SecretString;
use zeabur_auth::client::ExtraRoots;
use zeabur_auth::session::Limits;
use zeabur_auth::url::BaseUrl;
use zeabur_plan::token::PlanKey;

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
    /// Minutes a session may go unused before it ends.
    #[arg(long, env = "ZEABUR_SESSION_IDLE_MINUTES", default_value_t = 30)]
    pub(crate) session_idle_minutes: u32,
    /// Hours a session lasts at most, used or not.
    #[arg(long, env = "ZEABUR_SESSION_MAX_HOURS", default_value_t = 8)]
    pub(crate) session_max_hours: u32,
    /// The metrics store history is read from, until someone chooses another in the settings: a
    /// Service port serving the Prometheus HTTP API, as `namespace/name:port`. Empty: where
    /// Zeabur's recommendation installs VictoriaMetrics.
    #[arg(long, env = "ZEABUR_METRICS_SERVICE", default_value = "")]
    pub(crate) metrics_service: String,
    /// The namespace Zeabur runs in, where the ConfigMap `zeabur-settings` keeps the settings
    /// people choose. Empty: settings cannot be changed.
    #[arg(long, env = "ZEABUR_NAMESPACE", default_value = "")]
    pub(crate) namespace: String,
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
}

impl Args {
    /// Checks every value and reads the files.
    pub(crate) fn check(&self) -> Result<Config, String> {
        let public_url =
            BaseUrl::parse_root(&self.public_url).map_err(|e| format!("--public-url: {e}"))?;
        let issuer = BaseUrl::parse(&self.issuer_url).map_err(|e| format!("--issuer-url: {e}"))?;
        let issuer_ca = match &self.issuer_ca_file {
            Some(path) => ExtraRoots::from_pem(&read(path, "--issuer-ca-file")?)
                .map_err(|e| format!("--issuer-ca-file: {}: {e}", path.display()))?,
            None => ExtraRoots::default(),
        };
        if self.client_id.is_empty() {
            return Err("--client-id: must not be empty".to_owned());
        }
        let secret = read(&self.client_secret_file, "--client-secret-file")?;
        let secret = String::from_utf8(secret)
            .map_err(|_utf8| "--client-secret-file: the secret is not text".to_owned())?;
        let secret = secret.trim();
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
            client_secret: SecretString::from(secret.to_owned()),
            plan_key,
            elevation: self.elevation,
            limits: Limits {
                idle: i64::from(self.session_idle_minutes) * 60,
                absolute: i64::from(self.session_max_hours) * 3600,
                ..Limits::default()
            },
            settings,
        })
    }
}

fn read(path: &Path, flag: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{flag}: cannot read {}: {e}", path.display()))
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
    }

    #[test]
    fn the_metrics_store_and_the_settings_namespace_are_optional() {
        let files = Files::new("settings");
        let config = args(&files, &[]).unwrap();
        assert_eq!(config.settings, zeabur_api::Settings::default());
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
    fn a_plan_key_is_raw_bytes_or_hex() {
        assert!(plan_key(&[1; 32]).is_some());
        assert!(plan_key(format!("{}\n", "ab".repeat(32)).as_bytes()).is_some());
        assert!(plan_key("zz".repeat(32).as_bytes()).is_none());
        assert!(plan_key(&[1; 31]).is_none());
        assert!(plan_key("ab".repeat(33).as_bytes()).is_none());
    }
}
