//! `engawactl ui`: the web interface on this machine, acting with the person's kubeconfig.

use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::sync::Arc;

use axum::Router;
use engawa_api::access::SingleCluster;
use engawa_api::local::{Access, guard};
use engawa_kube::{Cluster as _, UserClient};

use crate::output;

/// Options of `engawactl ui`.
#[derive(Debug, clap::Args)]
pub(crate) struct UiArgs {
    /// The kubeconfig context to use (default: the current one).
    #[arg(long)]
    context: Option<String>,
    /// Print the link instead of opening a browser.
    #[arg(long)]
    no_browser: bool,
    /// The metrics store history is read from: a Service port serving the Prometheus HTTP API,
    /// as `namespace/name:port` (default: where engawa's recommendation installs
    /// VictoriaMetrics).
    #[arg(long, env = "ENGAWA_METRICS_SERVICE")]
    metrics_service: Option<String>,
}

pub(crate) fn run(args: &UiArgs) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            output::error(&format!("cannot start: {error}"));
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(serve(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            output::error(&message);
            ExitCode::FAILURE
        }
    }
}

async fn serve(args: &UiArgs) -> Result<(), String> {
    let settings = crate::in_process::settings(args.metrics_service.as_deref())?;
    let cluster = UserClient::from_kubeconfig(args.context.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| e.into_problem("your identity").detail)?;

    // Loopback only, on a port the system picks.
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .map_err(|e| format!("cannot listen on 127.0.0.1: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let access = Access::new(port).map_err(|e| e.to_string())?;
    let url = access.launch_url();

    let app = app(cluster, settings).layer(axum::middleware::from_fn_with_state(access, guard));
    output::info(&format!(
        "engawa is running on this machine as {}.\nOpen {url}\nPress Ctrl-C to stop.",
        identity.username
    ));
    if !args.no_browser {
        open_browser(&url);
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
        })
        .await
        .map_err(|e| e.to_string())
}

/// The API, acting as the person whose kubeconfig this is, and the web interface.
fn app(cluster: UserClient, settings: engawa_api::Settings) -> Router {
    let router =
        engawa_api::router_with_settings(Arc::new(SingleCluster(Arc::new(cluster))), settings);
    #[cfg(feature = "ui")]
    let router = router.fallback_service(engawa_ui::router());
    router
}

/// Opens `url` in the default browser; failing that, the printed link is enough.
fn open_browser(url: &str) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let opened = std::process::Command::new(program)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok();
    if !opened {
        output::info("Could not open a browser; open the link above yourself.");
    }
}
