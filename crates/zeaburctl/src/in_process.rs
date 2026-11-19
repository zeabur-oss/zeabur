//! The API in this process: requests go to `zeabur_api::router` as a service call, with no
//! listener, so the answers are the web interface's, read with the person's kubeconfig. Requests
//! and answers are the server's own types.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tower::ServiceExt as _;
use zeabur_api::access::SingleCluster;
use zeabur_core::problem::Problem;
use zeabur_kube::UserClient;

use crate::answer::{self, internal};

/// The largest answer read: a project document of a large namespace, with room to spare.
const MAX_BODY: usize = 64 * 1024 * 1024;

/// The settings of Zeabur on this machine: the metrics store `--metrics-service` names, or the
/// default. It keeps no settings of its own (no namespace), so they cannot be changed in it. The
/// interface checks for a newer zeaburctl unless the environment turns update checks off.
pub(crate) fn settings(metrics_service: Option<&str>) -> Result<zeabur_api::Settings, String> {
    let mut settings = zeabur_api::Settings {
        update_check: crate::update::allowed(),
        ..zeabur_api::Settings::default()
    };
    if let Some(service) = metrics_service.filter(|s| !s.is_empty()) {
        settings.metrics_store = zeabur_api::settings::parse_service_port(service)
            .map_err(|e| format!("--metrics-service: {e}"))?;
    }
    Ok(settings)
}

/// The API, called in this process.
#[derive(Debug, Clone)]
pub(crate) struct InProcess {
    router: Router,
}

impl InProcess {
    /// The API acting as the person whose kubeconfig `context` (default: the current one) is.
    pub(crate) async fn from_kubeconfig(
        context: Option<&str>,
        settings: zeabur_api::Settings,
    ) -> Result<Self, String> {
        let cluster = UserClient::from_kubeconfig(context)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self::with_client(cluster, settings))
    }

    /// The API acting as the person `cluster` signs in as.
    pub(crate) fn with_client(cluster: UserClient, settings: zeabur_api::Settings) -> Self {
        Self::new(zeabur_api::router_with(
            Arc::new(SingleCluster(Arc::new(cluster))),
            zeabur_api::Options {
                settings,
                ..zeabur_api::Options::default()
            },
        ))
    }

    /// `router` as the API; tests pass one over an in-memory cluster.
    pub(crate) const fn new(router: Router) -> Self {
        Self { router }
    }

    /// `GET` `path_and_query`: the answer as `T`, or the problem the API answered with.
    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        path_and_query: &str,
    ) -> Result<T, Box<Problem>> {
        self.send(Method::GET, path_and_query, None, Body::empty())
            .await
    }

    /// `POST` `body` as JSON to `path_and_query`.
    pub(crate) async fn post<B: Serialize + Sync, T: DeserializeOwned>(
        &self,
        path_and_query: &str,
        body: &B,
    ) -> Result<T, Box<Problem>> {
        let bytes = serde_json::to_vec(body)
            .map_err(|e| internal(&format!("The request is not JSON: {e}")))?;
        self.send(
            Method::POST,
            path_and_query,
            Some(("application/json", bytes.len())),
            Body::from(bytes),
        )
        .await
    }

    /// `POST` `bytes` to `path_and_query` as `content_type`.
    pub(crate) async fn post_bytes<T: DeserializeOwned>(
        &self,
        path_and_query: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<T, Box<Problem>> {
        let length = bytes.len();
        self.send(
            Method::POST,
            path_and_query,
            Some((content_type, length)),
            Body::from(bytes),
        )
        .await
    }

    async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        path_and_query: &str,
        content: Option<(&str, usize)>,
        body: Body,
    ) -> Result<T, Box<Problem>> {
        let mut request = Request::builder().method(method).uri(path_and_query);
        if let Some((content_type, length)) = content {
            request = request
                .header("content-type", content_type)
                .header("content-length", length);
        }
        let request = request
            .body(body)
            .map_err(|e| internal(&format!("{path_and_query} is not a request path: {e}")))?;
        let Ok(response) = self.router.clone().oneshot(request).await;
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), MAX_BODY)
            .await
            .map_err(|e| internal(&format!("Unable to read the answer: {e}")))?;
        answer::read(status, &body, "The API")
    }
}
