//! axum's `Json`, `Path` and `WebSocketUpgrade`, refusing as a [`Problem`] like every other
//! answer.
//!
//! A refusal says where the request is wrong, not what serde expected there, which would list
//! every variant of the type.

use std::error::Error as _;

use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use zeabur_core::problem::{Kind, Problem};

use crate::ApiError;

/// A request body as JSON, or a JSON answer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Json<T>(pub T);

/// The path parameters.
#[derive(Debug, Clone, Copy, Default)]
pub struct Path<T>(pub T);

/// A request to open a WebSocket; `.0` is axum's, to answer it.
#[derive(Debug)]
pub struct WebSocketUpgrade(pub axum::extract::ws::WebSocketUpgrade);

fn bad_request(detail: String) -> ApiError {
    ApiError::from(Problem::new(Kind::BadRequest, detail))
}

/// The first error of type `E` behind `rejection`.
fn behind<E: std::error::Error + 'static>(rejection: &JsonRejection) -> Option<&E> {
    let mut source = rejection.source();
    while let Some(error) = source {
        if let Some(found) = error.downcast_ref() {
            return Some(found);
        }
        source = error.source();
    }
    None
}

fn json_refusal(rejection: &JsonRejection) -> ApiError {
    match rejection {
        JsonRejection::MissingJsonContentType(_) => bad_request(
            "Send the request body as JSON, with `Content-Type: application/json`.".to_owned(),
        ),
        JsonRejection::JsonDataError(_) => {
            let path = behind::<serde_path_to_error::Error<serde_json::Error>>(rejection)
                .map(|e| e.path().to_string());
            bad_request(match path.as_deref() {
                None | Some(".") => "The request body is not what this request takes.".to_owned(),
                Some(path) => format!("The request body has a wrong value at `{path}`."),
            })
        }
        JsonRejection::JsonSyntaxError(_) => {
            // Trailing characters come as serde_json's own error, the rest wrapped with a path.
            let position = behind::<serde_path_to_error::Error<serde_json::Error>>(rejection)
                .map(serde_path_to_error::Error::inner)
                .or_else(|| behind::<serde_json::Error>(rejection))
                .map(|e| (e.line(), e.column()));
            bad_request(match position {
                Some((line, column)) => {
                    format!("The request body is not valid JSON at line {line}, column {column}.")
                }
                None => "The request body is not valid JSON.".to_owned(),
            })
        }
        JsonRejection::BytesRejection(_) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            bad_request("The request body is too large.".to_owned())
        }
        JsonRejection::BytesRejection(_) | _ => {
            bad_request("Zeabur couldn't read the request body.".to_owned())
        }
    }
}

impl<S, T> FromRequest<S> for Json<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        match axum::Json::<T>::from_request(request, state).await {
            Ok(axum::Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(json_refusal(&rejection)),
        }
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

impl<S, T> FromRequestParts<S> for Path<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Path(value)) => Ok(Self(value)),
            Err(rejection) => Err(bad_request(PathRejection::body_text(&rejection))),
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for WebSocketUpgrade {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        match axum::extract::ws::WebSocketUpgrade::from_request_parts(parts, state).await {
            Ok(upgrade) => Ok(Self(upgrade)),
            Err(rejection) => Err(bad_request(format!(
                "Open this address as a WebSocket: {}.",
                WebSocketUpgradeRejection::body_text(&rejection)
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::header::CONTENT_TYPE;
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase", tag = "op")]
    enum Change {
        Create,
        Delete,
    }

    #[derive(Debug, Deserialize)]
    struct Body_ {
        #[expect(dead_code, reason = "only parsed")]
        changes: Vec<Change>,
    }

    async fn refusal(content_type: Option<&str>, body: &str) -> Problem {
        let mut request = Request::builder().method("POST").uri("/");
        if let Some(value) = content_type {
            request = request.header(CONTENT_TYPE, value);
        }
        let request = request.body(Body::from(body.to_owned())).unwrap();
        *Json::<Body_>::from_request(request, &())
            .await
            .unwrap_err()
            .0
    }

    #[tokio::test]
    async fn a_wrong_value_names_the_field_and_not_the_type() {
        let problem = refusal(
            Some("application/json"),
            r#"{"changes":[{"op":"creatSe"}]}"#,
        )
        .await;
        assert_eq!(problem.kind, Kind::BadRequest);
        assert_eq!(
            problem.detail,
            "The request body has a wrong value at `changes[0].op`."
        );
    }

    #[tokio::test]
    async fn broken_json_names_the_position() {
        let problem = refusal(Some("application/json"), r#"{"changes":[]} x"#).await;
        assert_eq!(
            problem.detail,
            "The request body is not valid JSON at line 1, column 16."
        );
    }

    #[tokio::test]
    async fn a_body_without_the_json_type_is_refused_as_a_problem() {
        let problem = refusal(None, "{}").await;
        assert_eq!(problem.kind, Kind::BadRequest);
        assert!(
            problem.detail.contains("application/json"),
            "{}",
            problem.detail
        );
    }

    #[tokio::test]
    async fn a_plain_request_to_a_websocket_is_refused_as_a_problem() {
        let request = Request::builder().uri("/").body(Body::empty()).unwrap();
        let (mut parts, _) = request.into_parts();
        let problem = *WebSocketUpgrade::from_request_parts(&mut parts, &())
            .await
            .unwrap_err()
            .0;
        assert_eq!(problem.kind, Kind::BadRequest);
        assert!(
            problem
                .detail
                .starts_with("Open this address as a WebSocket"),
            "{}",
            problem.detail
        );
    }
}
