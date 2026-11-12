#![doc = include_str!("../README.md")]

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use zeabur_api::Settings;
use zeabur_api::access::SingleCluster;
use zeabur_api::local::{Access, guard};
use zeabur_core::crypto::RandomError;
use zeabur_kube::{Cluster, ConnectError, UserClient};

/// Why the local service did not start, or stopped with an error.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// The kubeconfig or its context is not usable.
    #[error(transparent)]
    Kubeconfig(#[from] ConnectError),
    /// The cluster did not say who the kubeconfig signs in as. The text is the detail of the
    /// problem.
    #[error("{0}")]
    Identity(String),
    /// The service cannot listen on a loopback port.
    #[error("cannot listen on 127.0.0.1: {0}")]
    Listen(#[source] io::Error),
    /// The secrets of the launch link cannot be made.
    #[error(transparent)]
    Random(#[from] RandomError),
    /// The server stopped with an error.
    #[error(transparent)]
    Serve(io::Error),
}

/// A local service that runs on a task of the Tokio runtime that started it.
///
/// If you drop it, the service stops when its open connections close. To wait for the stop,
/// call [`Running::stop`].
#[derive(Debug)]
pub struct Running {
    url: String,
    username: String,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<io::Result<()>>,
}

impl Running {
    /// The one-time link that opens the interface. Only the first request to it starts a session.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The Kubernetes user name that the cluster gives the kubeconfig.
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Stops the service and waits until its open connections close.
    ///
    /// # Errors
    ///
    /// When the server stopped with an error or its task panicked.
    pub async fn stop(self) -> Result<(), ServiceError> {
        let Self { shutdown, task, .. } = self;
        drop(shutdown);
        match task.await {
            Ok(served) => served.map_err(ServiceError::Serve),
            Err(join) => Err(ServiceError::Serve(io::Error::other(join))),
        }
    }
}

/// Starts the local service for the kubeconfig `context` (default: the current one).
///
/// Call it inside a Tokio runtime. The service runs on a task of that runtime.
///
/// # Errors
///
/// When the kubeconfig is not usable, the cluster does not answer who the person is, or no
/// loopback port is available.
pub async fn start(context: Option<&str>, settings: Settings) -> Result<Running, ServiceError> {
    let cluster = UserClient::from_kubeconfig(context).await?;
    start_with(Arc::new(cluster), settings).await
}

/// Starts the local service that acts as the person `cluster` signs in as.
///
/// Call it inside a Tokio runtime. The service runs on a task of that runtime.
///
/// # Errors
///
/// When the cluster does not answer who the person is, or no loopback port is available.
pub async fn start_with(
    cluster: Arc<dyn Cluster>,
    settings: Settings,
) -> Result<Running, ServiceError> {
    let identity = cluster
        .whoami()
        .await
        .map_err(|e| ServiceError::Identity(e.into_problem("your identity").detail))?;

    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .map_err(ServiceError::Listen)?;
    let port = listener.local_addr().map_err(ServiceError::Listen)?.port();
    let access = Access::new(port)?;
    let url = access.launch_url();
    let app = app(cluster, settings).layer(axum::middleware::from_fn_with_state(access, guard));

    let (shutdown, stopped) = oneshot::channel::<()>();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        // A send and a drop of the sender both stop the server.
        stopped.await.unwrap_or(());
    });
    let task = tokio::spawn(server.into_future());
    Ok(Running {
        url,
        username: identity.username,
        shutdown,
        task,
    })
}

/// The API, acting as the person `cluster` signs in as, and the web interface.
fn app(cluster: Arc<dyn Cluster>, settings: Settings) -> Router {
    let router = zeabur_api::router_with(
        Arc::new(SingleCluster(cluster)),
        zeabur_api::Options {
            settings,
            ..zeabur_api::Options::default()
        },
    );
    #[cfg(feature = "ui")]
    let router =
        router.fallback_service(zeabur_ui::router_for(zeabur_ui::assets(), zeabur_ui::CSP));
    router
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::TcpStream;
    use zeabur_kube::Identity;
    use zeabur_kube::memory::{MemoryCluster, State};

    use super::*;

    /// Sends `GET path` to `authority` on a new connection and returns the status and the
    /// whole answer.
    async fn get(authority: &str, path: &str, cookie: Option<&str>) -> (u16, String) {
        let mut stream = TcpStream::connect(authority).await.unwrap();
        let cookie = cookie.map_or_default(|c| format!("Cookie: {c}\r\n"));
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {authority}\r\n{cookie}Connection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).await.unwrap();
        let status = answer.split(' ').nth(1).unwrap().parse().unwrap();
        (status, answer)
    }

    fn session_cookie(answer: &str) -> &str {
        let line = answer
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("set-cookie:"))
            .unwrap();
        line.split_once(':')
            .unwrap()
            .1
            .split(';')
            .next()
            .unwrap()
            .trim()
    }

    #[tokio::test]
    async fn the_launch_link_opens_the_service_and_stop_closes_the_port() {
        let cluster = MemoryCluster::new(State {
            identity: Identity {
                username: "zeabur:alice".to_owned(),
                groups: Vec::new(),
            },
            ..State::default()
        });
        let running = start_with(Arc::new(cluster), Settings::default())
            .await
            .unwrap();
        assert_eq!(running.username(), "zeabur:alice");
        let (authority, launch) = running
            .url()
            .strip_prefix("http://")
            .and_then(|rest| rest.split_once('/'))
            .unwrap();
        let authority = authority.to_owned();
        let launch = format!("/{launch}");
        assert!(authority.starts_with("127.0.0.1:"), "{authority}");

        let (status, _) = get(&authority, "/api/v1/whoami", None).await;
        assert_eq!(status, 401);

        let (status, answer) = get(&authority, &launch, None).await;
        assert_eq!(status, 303);
        let cookie = session_cookie(&answer).to_owned();
        let (status, answer) = get(&authority, "/api/v1/whoami", Some(&cookie)).await;
        assert_eq!(status, 200);
        assert!(answer.contains("zeabur:alice"), "{answer}");

        running.stop().await.unwrap();
        assert!(TcpStream::connect(&authority).await.is_err());
    }
}
