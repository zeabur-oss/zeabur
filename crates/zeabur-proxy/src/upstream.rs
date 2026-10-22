//! Sending the rebuilt request to a backend and streaming its answer back, within a time and a
//! size.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use http::{Request, StatusCode, Uri};
use http_body::{Body, Frame};
use http_body_util::{BodyExt as _, Empty};
use hyper::body::Incoming;
use hyper_util::client::legacy::connect::HttpConnector;
use tokio::time::Sleep;

use crate::audit::{Audit, Outcome};
use crate::reject::Reject;
use crate::rules::Upstream;

/// The HTTP client for backends, inside the cluster.
pub(crate) type Client = hyper_util::client::legacy::Client<HttpConnector, Empty<Bytes>>;

pub(crate) fn client(connect: Duration) -> Client {
    let mut http = HttpConnector::new();
    http.set_connect_timeout(Some(connect));
    hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .pool_timer(hyper_util::rt::TokioTimer::new())
        .pool_idle_timeout(Duration::from_secs(30))
        .build(http)
}

/// What a backend answered.
pub(crate) enum Answer {
    /// The data, to stream back.
    Data {
        content_type: Option<http::HeaderValue>,
        body: Incoming,
    },
    /// The backend refused the query itself (bad LogsQL); its explanation, shortened.
    Refused(String),
}

/// The longest explanation of a refused query passed on.
const MAX_REFUSAL: usize = 2048;

/// Sends `upstream` to the backend at `base` with no header of the person's: the request has
/// only what [`Upstream`] holds.
pub(crate) async fn send(
    client: &Client,
    base: &Uri,
    upstream: &Upstream,
    wait: Duration,
) -> Result<Answer, Reject> {
    let uri: Uri = format!(
        "{}{}",
        base.to_string().trim_end_matches('/'),
        upstream.path_and_query()
    )
    .parse()
    .map_err(|_failed| Reject::Backend)?;
    let request = Request::get(uri)
        .body(Empty::new())
        .map_err(|_failed| Reject::Backend)?;
    let response = tokio::time::timeout(wait, client.request(request))
        .await
        .map_err(|_failed| Reject::Backend)?
        .map_err(|_failed| Reject::Backend)?;
    let status = response.status();
    if status == StatusCode::OK {
        let content_type = response.headers().get(http::header::CONTENT_TYPE).cloned();
        return Ok(Answer::Data {
            content_type,
            body: response.into_body(),
        });
    }
    if status == StatusCode::BAD_REQUEST {
        let mut text = Vec::new();
        let mut body = response.into_body();
        while text.len() < MAX_REFUSAL {
            match tokio::time::timeout(wait, body.frame()).await {
                Ok(Some(Ok(frame))) => {
                    if let Some(data) = frame.data_ref() {
                        text.extend_from_slice(data);
                    }
                }
                Ok(Some(Err(_)) | None) | Err(_) => break,
            }
        }
        text.truncate(MAX_REFUSAL);
        return Ok(Answer::Refused(
            String::from_utf8_lossy(&text).trim().to_owned(),
        ));
    }
    Err(Reject::Backend)
}

/// The backend's answer on its way back: at most `max` bytes and until a deadline, trailers
/// dropped. Ending, however it ends, writes the request's audit line.
pub(crate) struct Capped {
    inner: Incoming,
    sent: u64,
    max: u64,
    deadline: Pin<Box<Sleep>>,
    audit: Option<Audit>,
}

impl Capped {
    pub(crate) fn new(inner: Incoming, max: u64, wait: Duration, audit: Audit) -> Self {
        Self {
            inner,
            sent: 0,
            max,
            deadline: Box::pin(tokio::time::sleep(wait)),
            audit: Some(audit),
        }
    }

    fn finish(&mut self, outcome: Outcome) {
        if let Some(audit) = self.audit.take() {
            audit.streamed(outcome, self.sent);
        }
    }

    fn fail(
        &mut self,
        outcome: Outcome,
        kind: io::ErrorKind,
    ) -> Poll<Option<io::Result<Frame<Bytes>>>> {
        self.finish(outcome);
        Poll::Ready(Some(Err(io::Error::new(kind, outcome.as_str()))))
    }
}

impl Body for Capped {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<io::Result<Frame<Bytes>>>> {
        if self.audit.is_none() {
            return Poll::Ready(None);
        }
        if self.deadline.as_mut().poll(cx).is_ready() {
            return self.fail(Outcome::TimedOut, io::ErrorKind::TimedOut);
        }
        loop {
            return match Pin::new(&mut self.inner).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => {
                    let Ok(data) = frame.into_data() else {
                        continue;
                    };
                    self.sent = self.sent.saturating_add(data.len() as u64);
                    if self.sent > self.max {
                        return self.fail(Outcome::TooLarge, io::ErrorKind::FileTooLarge);
                    }
                    Poll::Ready(Some(Ok(Frame::data(data))))
                }
                Poll::Ready(Some(Err(_))) => {
                    self.fail(Outcome::BackendFailed, io::ErrorKind::Other)
                }
                Poll::Ready(None) => {
                    self.finish(Outcome::Complete);
                    Poll::Ready(None)
                }
                Poll::Pending => Poll::Pending,
            };
        }
    }
}

impl Drop for Capped {
    fn drop(&mut self) {
        self.finish(Outcome::Abandoned);
    }
}
