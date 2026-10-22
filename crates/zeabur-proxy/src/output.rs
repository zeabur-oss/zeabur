//! The only place `zeabur-proxy` writes to the terminal directly; everything else logs through
//! `tracing`.

/// Writes an error that happened before logging could report it.
#[expect(
    clippy::print_stderr,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn error(message: &str) {
    eprintln!("zeabur-proxy: {message}");
}
