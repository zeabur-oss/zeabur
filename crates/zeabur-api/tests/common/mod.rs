//! What the API's test binaries share: the router over one cluster, one request, and plan then
//! apply.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use tower::ServiceExt as _;
use zeabur_api::Settings;
use zeabur_api::access::SingleCluster;
use zeabur_kube::Cluster;

/// The API over `cluster`, with the default settings.
pub(crate) fn app<C: Cluster + Clone + 'static>(cluster: &C) -> Router {
    zeabur_api::router(Arc::new(SingleCluster(Arc::new(cluster.clone()))))
}

/// The API over `cluster`, with `settings`.
pub(crate) fn app_with<C: Cluster + Clone + 'static>(cluster: &C, settings: Settings) -> Router {
    zeabur_api::router_with_settings(Arc::new(SingleCluster(Arc::new(cluster.clone()))), settings)
}

/// Sends one request, with `body` as JSON; the status and the answer as JSON (`null` when it is
/// not JSON).
pub(crate) async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(serde_json::to_vec(b).unwrap())))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Plans `changes` for `project`, then applies the plan: the status and answer of the apply, or
/// of the plan when planning failed.
pub(crate) async fn plan_and_apply(
    app: &Router,
    project: &str,
    changes: Value,
) -> (StatusCode, Value) {
    let (status, planned) = send(
        app,
        "POST",
        &format!("/api/v1/projects/{project}/plan"),
        Some(&json!({"changes": changes})),
    )
    .await;
    if status != StatusCode::OK {
        return (status, planned);
    }
    send(
        app,
        "POST",
        &format!("/api/v1/projects/{project}/apply"),
        Some(&json!({"token": planned["token"]})),
    )
    .await
}

/// Plans and applies `changes` for `project`, both answered `200`; the plan.
pub(crate) async fn planned_and_applied(app: &Router, project: &str, changes: Value) -> Value {
    let (status, planned) = send(
        app,
        "POST",
        &format!("/api/v1/projects/{project}/plan"),
        Some(&json!({"changes": changes})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{planned}");
    let (status, applied) = send(
        app,
        "POST",
        &format!("/api/v1/projects/{project}/apply"),
        Some(&json!({"token": planned["token"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    planned
}
