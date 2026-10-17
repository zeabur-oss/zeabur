//! The only place `engawactl` writes to the terminal; everything else logs through `tracing`.

/// Writes an error message for the person at the terminal.
#[expect(
    clippy::print_stderr,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn error(message: &str) {
    eprintln!("engawactl: {message}");
}

/// Writes a message for the person at the terminal.
#[expect(
    clippy::print_stderr,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn info(message: &str) {
    eprintln!("{message}");
}

/// Writes a command's result, such as `engawactl doctor`'s report, to standard output.
#[expect(
    clippy::print_stdout,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn report(text: &str) {
    print!("{text}");
}
