//! The only place `xtask` writes to the terminal.

/// Writes a line for the person at the terminal.
#[expect(
    clippy::print_stdout,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn say(line: &str) {
    println!("{line}");
}

/// Writes text another program reads, as it is.
#[expect(
    clippy::print_stdout,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn write(text: &str) {
    print!("{text}");
}

/// Writes an error message for the person at the terminal.
#[expect(
    clippy::print_stderr,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn error(message: &str) {
    eprintln!("xtask: {message}");
}

/// Writes text for the person at the terminal on standard error, as it is.
#[expect(
    clippy::print_stderr,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn usage(text: &str) {
    eprint!("{text}");
}
