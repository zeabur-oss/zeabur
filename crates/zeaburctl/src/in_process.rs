//! The API in this process: requests go to `zeabur_api::router` as a service call, with no
//! listener, so the answers are the web interface's, read with the person's kubeconfig.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request};
use serde_json::Value;
use tower::ServiceExt as _;
use zeabur_api::access::SingleCluster;
use zeabur_core::problem::{Kind, Problem};
use zeabur_kube::UserClient;

/// The largest answer read: a project document of a large namespace, with room to spare.
const MAX_BODY: usize = 64 * 1024 * 1024;

/// The settings of Zeabur on this machine: the metrics store `--metrics-service` names, or the
/// default. It keeps no settings of its own (no namespace), so they cannot be changed in it.
pub(crate) fn settings(metrics_service: Option<&str>) -> Result<zeabur_api::Settings, String> {
    let mut settings = zeabur_api::Settings::default();
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
        Ok(Self::new(zeabur_api::router_with_settings(
            Arc::new(SingleCluster(Arc::new(cluster))),
            settings,
        )))
    }

    /// `router` as the API; tests pass one over an in-memory cluster.
    pub(crate) const fn new(router: Router) -> Self {
        Self { router }
    }

    /// `GET` `path_and_query`: the JSON of a success, or the problem the API answered with.
    pub(crate) async fn get(&self, path_and_query: &str) -> Result<Value, Value> {
        let request = Request::builder()
            .method(Method::GET)
            .uri(path_and_query)
            .body(Body::empty())
            .map_err(|e| internal(&format!("{path_and_query} is not a request path: {e}")))?;
        let Ok(response) = self.router.clone().oneshot(request).await;
        let success = response.status().is_success();
        let body = axum::body::to_bytes(response.into_body(), MAX_BODY)
            .await
            .map_err(|e| internal(&format!("Unable to read the answer: {e}")))?;
        let value: Value = serde_json::from_slice(&body)
            .map_err(|e| internal(&format!("The answer is not JSON: {e}")))?;
        if success { Ok(value) } else { Err(value) }
    }
}

/// A problem of `zeaburctl`'s own, in the API's shape.
fn internal(detail: &str) -> Value {
    serde_json::to_value(Problem::new(Kind::Internal, detail)).unwrap_or(Value::Null)
}
