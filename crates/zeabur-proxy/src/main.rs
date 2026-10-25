//! `zeabur-proxy`: reads shared backends for one namespace at a time, as the person asking.

#![forbid(unsafe_code)]

mod output;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use k8s_openapi::jiff::SignedDuration;
use zeabur_proxy::access::KubeReviewer;
use zeabur_proxy::logs::LogsLimits;
use zeabur_proxy::server::{self, BackendConfig, Proxy, Settings};

/// Reads shared backends for one namespace at a time, as the person asking.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// Address to listen on.
    #[arg(long, env = "ZEABUR_PROXY_LISTEN", default_value = "[::]:8443")]
    listen: SocketAddr,
    /// TLS certificate chain (PEM). Without it and the key, the proxy serves plain HTTP.
    #[arg(long, env = "ZEABUR_PROXY_TLS_CERT", requires = "tls_key")]
    tls_cert: Option<PathBuf>,
    /// TLS private key (PEM).
    #[arg(long, env = "ZEABUR_PROXY_TLS_KEY", requires = "tls_cert")]
    tls_key: Option<PathBuf>,
    /// VictoriaLogs' base URL, such as
    /// `http://vlsingle-victoria-logs.observability.svc:9428`. Unset: `/logs/` is not served.
    #[arg(long, env = "ZEABUR_PROXY_LOGS_URL")]
    logs_url: Option<http::Uri>,
    /// The most log lines one query returns.
    #[arg(long, env = "ZEABUR_PROXY_LOGS_MAX_LIMIT", default_value_t = 1000)]
    logs_max_limit: u32,
    /// The longest range one log query covers, in hours.
    #[arg(long, env = "ZEABUR_PROXY_LOGS_MAX_HOURS", default_value_t = 24 * 7)]
    logs_max_hours: i64,
    /// How long a backend may take to answer, in seconds.
    #[arg(long, env = "ZEABUR_PROXY_TIMEOUT", default_value_t = 30)]
    timeout: u64,
    /// The most bytes one answer may have.
    #[arg(long, env = "ZEABUR_PROXY_MAX_BYTES", default_value_t = 16 << 20)]
    max_bytes: u64,
    /// How long an access review's answer is reused, in seconds; 0 asks every time.
    #[arg(long, env = "ZEABUR_PROXY_REVIEW_CACHE", default_value_t = 10)]
    review_cache: u64,
    /// Queries in flight for the whole proxy.
    #[arg(long, env = "ZEABUR_PROXY_MAX_INFLIGHT", default_value_t = 16)]
    max_inflight: usize,
    /// Queries in flight for one person.
    #[arg(
        long,
        env = "ZEABUR_PROXY_MAX_INFLIGHT_PER_PERSON",
        default_value_t = 2
    )]
    max_inflight_per_person: usize,
}

/// How long kube-apiserver may take to answer an access review.
const REVIEW_TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> ExitCode {
    let args = Args::parse();
    tracing_subscriber::fmt().with_target(true).init();
    if let Err(error) = forbid_core_dumps() {
        tracing::warn!(%error, "core dumps stay possible");
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            output::error(&format!("cannot start the async runtime: {error}"));
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            output::error(&message);
            ExitCode::FAILURE
        }
    }
}

/// Stops this process from writing a core dump, which would hold the tokens of requests in
/// progress. The proxy depends on no workspace crate, so this is a copy of
/// `zeabur_core::secret::forbid_core_dumps`.
fn forbid_core_dumps() -> Result<(), String> {
    let mut refused = Vec::new();
    #[cfg(unix)]
    {
        use rustix::process::{Resource, Rlimit, setrlimit};
        let none = Rlimit {
            current: Some(0),
            maximum: Some(0),
        };
        if let Err(error) = setrlimit(Resource::Core, none) {
            refused.push(format!("cannot set RLIMIT_CORE to 0: {error}"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        use rustix::process::{DumpableBehavior, set_dumpable_behavior};
        if let Err(error) = set_dumpable_behavior(DumpableBehavior::NotDumpable) {
            refused.push(format!("cannot make the process non-dumpable: {error}"));
        }
    }
    if refused.is_empty() {
        Ok(())
    } else {
        Err(refused.join("; "))
    }
}

async fn run(args: Args) -> Result<(), String> {
    let reviewer =
        KubeReviewer::in_cluster(REVIEW_TIMEOUT, Duration::from_secs(args.review_cache))?;
    let timeout = Duration::from_secs(args.timeout);
    let logs = match args.logs_url {
        Some(url) if url.scheme_str() == Some("http") && url.host().is_some() => {
            Some(BackendConfig {
                url,
                limits: LogsLimits {
                    max_limit: args.logs_max_limit,
                    max_span: SignedDuration::from_hours(args.logs_max_hours),
                    ..LogsLimits::default()
                },
                timeout,
                max_bytes: args.max_bytes,
            })
        }
        Some(url) => {
            return Err(format!(
                "the logs URL must be http://host[:port], not {url}"
            ));
        }
        None => None,
    };
    let tls = match (args.tls_cert, args.tls_key) {
        (Some(cert), Some(key)) => Some(tokio_rustls::TlsAcceptor::from(Arc::new(
            zeabur_proxy::tls::Reloading::new(cert, key)?.config(),
        ))),
        (None, None) => {
            tracing::warn!("no TLS certificate: serving plain HTTP");
            None
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err("give both the TLS certificate and key".to_owned());
        }
    };
    let proxy = Proxy::new(
        reviewer,
        Settings {
            logs,
            max_inflight: args.max_inflight,
            max_inflight_per_person: args.max_inflight_per_person,
        },
    );
    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .map_err(|e| format!("cannot listen on {}: {e}", args.listen))?;
    tracing::info!(address = %args.listen, "listening");
    tokio::select! {
        served = server::serve(listener, tls, proxy) => served.map_err(|e| e.to_string()),
        () = stopped() => Ok(()),
    }
}

async fn stopped() {
    use tokio::signal::unix::{SignalKind, signal};
    let Ok(mut terminate) = signal(SignalKind::terminate()) else {
        return std::future::pending().await;
    };
    tokio::select! {
        _ = terminate.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}
