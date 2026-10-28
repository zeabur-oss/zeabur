//! The only place `xtask` writes to the terminal.

/// Writes an error message for the person at the terminal.
#[expect(
    clippy::print_stderr,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn error(message: &str) {
    eprintln!("xtask: {message}");
}
