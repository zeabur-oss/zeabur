//! Zeabur Proxy (`crates/zeabur-proxy`) as Zeabur reaches it: it reads log and usage history for
//! one namespace, as the person whose token Zeabur passes on.

use std::future::Future;
use std::pin::Pin;

use zeabur_kube::UserToken;

/// Zeabur Proxy, as Zeabur reaches it.
pub trait ZeaburProxy: Send + Sync {
    /// `GET path_and_query` with `token` as the bearer token.
    fn get<'a>(
        &'a self,
        token: &'a UserToken,
        path_and_query: String,
    ) -> Pin<Box<dyn Future<Output = Result<ProxyAnswer, String>> + Send + 'a>>;
}

/// What Zeabur Proxy answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyAnswer {
    /// The HTTP status.
    pub status: u16,
    /// The body.
    pub body: Vec<u8>,
}

/// The `detail` of the proxy's problem, else its body as text.
pub(crate) fn detail(body: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|problem| {
            problem
                .get("detail")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_owned())
}
