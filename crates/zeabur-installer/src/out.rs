//! The only place that writes to stdout and stderr directly.

use std::io::Write as _;

/// Writes one line to stdout.
pub(crate) fn line(text: &str) {
    // A closed pipe (`| head`) is not an error worth reporting.
    drop(writeln!(std::io::stdout(), "{text}"));
}

/// Writes one line to stderr.
pub(crate) fn err(text: &str) {
    drop(writeln!(std::io::stderr(), "{text}"));
}

/// Announces a step of `apply`.
pub(crate) fn step(text: &str) {
    err(&format!("==> {text}"));
}
