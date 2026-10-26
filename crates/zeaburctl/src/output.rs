//! The only place `zeaburctl` writes to the terminal; everything else logs through `tracing`.

/// Writes an error message for the person at the terminal.
#[expect(
    clippy::print_stderr,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn error(message: &str) {
    eprintln!("zeaburctl: {message}");
}

/// Writes a message for the person at the terminal.
#[expect(
    clippy::print_stderr,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn info(message: &str) {
    eprintln!("{message}");
}

/// Writes a command's result, such as `zeaburctl doctor`'s report, to standard output.
#[expect(
    clippy::print_stdout,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn report(text: &str) {
    print!("{text}");
}

/// Asks a question at the terminal and reads one line of the answer; `None` at the end of input.
#[expect(
    clippy::print_stderr,
    reason = "this module is the binary's terminal output"
)]
pub(crate) fn ask(question: &str) -> Option<String> {
    eprint!("{question}");
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim_end().to_owned()),
    }
}
