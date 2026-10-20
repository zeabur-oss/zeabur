#![doc = include_str!("../README.md")]

mod reads;
mod server;

use std::future::Future;

pub use reads::ALLOWLIST;
pub use server::Server;

/// A problem as the API answers it (RFC 9457 Problem Details, with Zeabur's `kind`, `fix`,
/// `denied` and `kubernetes` members), passed through to the client unchanged.
pub type Problem = serde_json::Value;

/// Reads the Zeabur API as the person running the server.
///
/// The only operation is a `GET`: there is nothing here a tool could use to write, reveal a
/// Secret's values or open a terminal. Every path a tool asks for is one of [`ALLOWLIST`].
pub trait Reader: Send + Sync + 'static {
    /// `GET` `path_and_query` (such as `/api/v1/projects/shop/events?limit=20`): the JSON body
    /// of a success, or the problem of a refusal or failure.
    fn get(
        &self,
        path_and_query: &str,
    ) -> impl Future<Output = Result<serde_json::Value, Problem>> + Send;
}
