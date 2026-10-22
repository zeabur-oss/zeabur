//! `zeabur`: the in-cluster server.
//!
//! **Memory.** The server aims to sit under 30 MiB at rest. The async runtime has two worker
//! threads, enough for one cluster's people, and at most four threads for blocking work (DNS
//! lookups, file reads); a session keeps one connection pool to kube-apiserver.
//!
//! **One replica.** Sessions are held in memory (see `zeabur_auth::session`), so the server runs
//! as one replica; a restart signs everyone out.

#![forbid(unsafe_code)]

mod config;
mod output;
mod pages;
mod proxy;
mod server;

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use secrecy::{ExposeSecret as _, SecretString};
use zeabur_auth::client::{ClientConfig, OAuthClient};
use zeabur_auth::flow::FlowKey;
use zeabur_auth::passkey::PasskeyEndpoints;
use zeabur_auth::session::{Bind, Clock, Sessions};
use zeabur_kube::{ClusterEndpoint, UserClient, UserToken};

use crate::config::{Args, Config};
use crate::server::{Auth, Connection, IssuerPage};

/// Worker threads of the async runtime.
const WORKERS: usize = 2;
/// Threads for blocking work.
const BLOCKING: usize = 4;
/// How often sessions that are over are removed.
const SWEEP: Duration = Duration::from_secs(60);
/// How long answers in progress may take to finish once the server is asked to stop; well inside
/// the 30 s a pod gets by default before it is killed.
const DRAIN: Duration = Duration::from_secs(10);

fn main() -> ExitCode {
    let args = Args::parse();
    tracing_subscriber::fmt().with_target(false).init();
    let config = match args.check() {
        Ok(config) => config,
        Err(message) => {
            output::error(&message);
            return ExitCode::FAILURE;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(WORKERS)
        .max_blocking_threads(BLOCKING)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            output::error(&format!("cannot start the async runtime: {error}"));
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(serve(&args, config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            output::error(&message);
            ExitCode::FAILURE
        }
    }
}

fn clock() -> Clock {
    Arc::new(|| zeabur_kube::k8s_openapi::jiff::Timestamp::now().as_second())
}

/// A client for kube-apiserver that acts as the holder of the token; never Zeabur's own.
fn connect(endpoint: ClusterEndpoint) -> Bind<Connection> {
    Arc::new(move |token| {
        UserClient::with_token(&endpoint, &UserToken::new(token.expose_secret()))
            .map(|client| Connection {
                cluster: Arc::new(client),
                token: Arc::new(SecretString::from(token.expose_secret().to_owned())),
            })
            .map_err(|e| e.to_string())
    })
}

async fn serve(args: &Args, config: Config) -> Result<(), String> {
    let endpoint = ClusterEndpoint::in_cluster().map_err(|e| e.to_string())?;
    let passkeys = PasskeyEndpoints::new(&config.issuer, config.issuer_ca.clone())
        .map_err(|e| e.to_string())?;
    let oauth = Arc::new(
        OAuthClient::new(ClientConfig {
            issuer: config.issuer.clone(),
            public_url: config.public_url.clone(),
            client_id: config.client_id.clone(),
            client_secret: config.client_secret,
            issuer_ca: config.issuer_ca,
        })
        .map_err(|e| e.to_string())?,
    );
    // An issuer that is not up yet is not a configuration error: sign-in asks again later.
    match oauth.discover().await {
        Ok(()) => tracing::info!(
            event = "auth.discovery",
            issuer = %config.issuer,
            redirect_uri = oauth.redirect_uri(),
            "issuer found"
        ),
        Err(error) => tracing::warn!(
            event = "auth.discovery",
            issuer = %config.issuer,
            %error,
            "cannot read the issuer's metadata yet"
        ),
    }
    let connect = connect(endpoint);
    let clock = clock();
    let sessions = Arc::new(Sessions::new(
        Arc::clone(&oauth),
        Arc::clone(&connect),
        config.limits,
        Arc::clone(&clock),
    ));
    let auth = Arc::new(Auth {
        public: config.public_url,
        oauth,
        sessions: Arc::clone(&sessions),
        connect,
        flow_key: FlowKey::random().map_err(|e| e.to_string())?,
        elevation: config.elevation,
        clock,
    });
    let log_proxy = config.proxy.map(|proxy| {
        let proxy: Arc<dyn zeabur_api::services::logs::LogProxy> = Arc::new(proxy);
        proxy
    });
    let app = server::app(
        &auth,
        config.plan_key,
        config.settings,
        Arc::new(IssuerPage(passkeys)),
        log_proxy,
    );

    tokio::spawn(async move {
        let mut every = tokio::time::interval(SWEEP);
        loop {
            every.tick().await;
            let left = sessions.sweep();
            tracing::debug!(event = "session.sweep", sessions = left, "swept sessions");
        }
    });

    let (listener, address) = listen(args.listen).await?;
    tracing::info!(event = "server.listening", %address, "listening");
    let (stopping, mut stopped) = tokio::sync::watch::channel(false);
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        stop_signal().await;
        tracing::info!(event = "server.stopping", "stopping");
        let _sent = stopping.send(true);
    });
    // Answers in progress get `DRAIN` to finish; a log stream would otherwise hold the pod until
    // the kubelet kills it.
    let drained = async move {
        if stopped.wait_for(|stop| *stop).await.is_ok() {
            tokio::time::sleep(DRAIN).await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    tokio::select! {
        result = server.into_future() => {
            result.map_err(|e| format!("cannot serve on {address}: {e}"))
        }
        () = drained => {
            tracing::warn!(event = "server.stopped", "stopped with answers still open");
            Ok(())
        }
    }
}

/// Listens on `address`, and where that is the default `[::]` and the node has no IPv6, on
/// `0.0.0.0` instead; the address listened on.
///
/// A cluster gives pods IPv4 addresses, IPv6 addresses or both [k8s-dual-stack], and a server on
/// `0.0.0.0` cannot be reached over IPv6. A socket on `[::]` takes IPv6 connections and, on
/// Linux, IPv4 ones too (as IPv4-mapped addresses) unless `net.ipv6.bindv6only` is set
/// [linux-ipv6], so one listener serves all three kinds of cluster. A kernel with IPv6 turned
/// off refuses that socket; then only IPv4 is there to serve.
///
/// [k8s-dual-stack]: https://kubernetes.io/docs/concepts/services-networking/dual-stack/
/// [linux-ipv6]: https://man7.org/linux/man-pages/man7/ipv6.7.html
async fn listen(
    address: std::net::SocketAddr,
) -> Result<(tokio::net::TcpListener, std::net::SocketAddr), String> {
    let refused = |address, e: &std::io::Error| format!("cannot listen on {address}: {e}");
    match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => Ok((listener, address)),
        Err(error) if address.ip() == std::net::Ipv6Addr::UNSPECIFIED => {
            let v4 = std::net::SocketAddr::from((std::net::Ipv4Addr::UNSPECIFIED, address.port()));
            let listener = tokio::net::TcpListener::bind(v4)
                .await
                .map_err(|e| format!("{}; {}", refused(address, &error), refused(v4, &e)))?;
            tracing::warn!(event = "server.ipv4_only", %error, "no IPv6 here; listening on IPv4 only");
            Ok((listener, v4))
        }
        Err(error) => Err(refused(address, &error)),
    }
}

/// Resolves on Ctrl-C or on SIGTERM, which the kubelet sends to stop a pod
/// (<https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#pod-termination>).
async fn stop_signal() {
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(event = "server.signal", %error, "cannot listen for Ctrl-C");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                terminate.recv().await;
            }
            Err(error) => {
                tracing::error!(event = "server.signal", %error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}

#[cfg(test)]
mod tests {
    use super::listen;

    /// The default listener takes IPv4 connections too, wherever it ends up listening.
    #[tokio::test]
    async fn the_default_listener_answers_over_ipv4() {
        let (listener, address) = listen("[::]:0".parse().unwrap()).await.unwrap();
        assert!(address.ip().is_unspecified(), "{address}");
        let port = listener.local_addr().unwrap().port();
        let accepted = tokio::spawn(async move { listener.accept().await.map(|_| ()) });
        tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        accepted.await.unwrap().unwrap();
    }
}
