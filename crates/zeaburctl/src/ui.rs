//! `zeaburctl ui`: the web interface on this machine, acting with the person's kubeconfig.

use std::process::ExitCode;
use std::sync::Arc;

use zeabur_kube::{Cluster as _, UserClient};
use zeabur_telemetry::Connection;

use crate::output;
use crate::telemetry::{Local, outcome};

/// Options of `zeaburctl ui`.
#[derive(Debug, clap::Args)]
pub(crate) struct UiArgs {
    /// The kubeconfig context to use (default: the current one).
    #[arg(long)]
    context: Option<String>,
    /// Print the link instead of opening a browser.
    #[arg(long)]
    no_browser: bool,
    /// The metrics store history is read from: a Service port serving the Prometheus HTTP API,
    /// as `namespace/name:port` (default: where Zeabur Catalog installs
    /// VictoriaMetrics).
    #[arg(long, env = "ZEABUR_METRICS_SERVICE")]
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
    let connected = connect(args.context.as_deref()).await;
    Local::from_env()
        .connected(match &connected {
            Ok(_) => Connection::Connected,
            Err((outcome, _)) => *outcome,
        })
        .await;
    let cluster = connected.map_err(|(_, message)| message)?;
    let running = zeabur_local::start_with(Arc::new(cluster), settings)
        .await
        .map_err(|e| e.to_string())?;
    let url = running.url();
    output::info(&format!(
        "Zeabur is running on this machine as {}.\nOpen {url}\nPress Ctrl-C to stop.",
        running.username()
    ));
    if !args.no_browser {
        open_browser(url);
    }
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
    running.stop().await.map_err(|e| e.to_string())
}

/// The client for the kubeconfig once the cluster has said who it signs in as, or how the
/// connection ended and what to tell the person.
async fn connect(context: Option<&str>) -> Result<UserClient, (Connection, String)> {
    let cluster = UserClient::from_kubeconfig(context)
        .await
        .map_err(|e| (Connection::Other, e.to_string()))?;
    match cluster.whoami().await {
        Ok(_) => Ok(cluster),
        Err(error) => Err((outcome(&error), error.into_problem("your identity").detail)),
    }
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
        output::info("Could not open a browser. Open the link above.");
    }
}
