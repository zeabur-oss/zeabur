//! `zeaburctl ui`: the web interface on this machine, acting with the person's kubeconfig.

use std::process::ExitCode;

use zeabur_local::Settings;
use zeabur_local::telemetry::{Connection, Local};

use crate::output;

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

pub(crate) async fn run(args: &UiArgs) -> ExitCode {
    let settings = match crate::in_process::settings(args.metrics_service.as_deref()) {
        Ok(settings) => settings,
        Err(message) => {
            output::error(&message);
            return ExitCode::FAILURE;
        }
    };
    let telemetry = crate::telemetry::local();
    let served = serve(args, settings, &telemetry).await;
    if let Err(message) = &served {
        output::error(message);
    }
    telemetry.close().await;
    if served.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

async fn serve(args: &UiArgs, settings: Settings, telemetry: &Local) -> Result<(), String> {
    let started = zeabur_local::start(args.context.as_deref(), settings).await;
    telemetry.connected(
        started
            .as_ref()
            .map_or_else(zeabur_local::ServiceError::connection, |_| {
                Connection::Connected
            }),
    );
    let running = started.map_err(|e| e.to_string())?;
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
