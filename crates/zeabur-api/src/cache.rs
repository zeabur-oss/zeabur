//! No API answer is kept by a browser or a proxy.
//!
//! Every response under `/api` carries `Cache-Control: no-store`, which tells a cache it "MUST NOT
//! store any part of either the immediate request or the response" [rfc9111] (§5.2.2.5).
//! Answers are read as the person making the request and include values such as environment
//! variables, so they stay out of the browser's disk cache. Nothing is lost by it: the interface
//! polls a few kilobytes and keeps its own copy in memory, and a revalidation would still read
//! kube-apiserver as for a full answer. A handler that sets its own `Cache-Control` keeps it.
//!
//! # References
//!
//! - [rfc9111] R. Fielding, M. Nottingham and J. Reschke, Eds., "HTTP Caching," RFC 9111, IETF,
//!   June 2022. <https://www.rfc-editor.org/rfc/rfc9111>
//!
//! [rfc9111]: https://www.rfc-editor.org/rfc/rfc9111

use axum::extract::Request;
use axum::http::{HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;

/// Adds `Cache-Control: no-store` to every answer that has no `Cache-Control` of its own.
pub(crate) async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static("no-store"));
    response
}
