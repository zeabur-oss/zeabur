//! The only place that writes to stdout and stderr directly.

use std::io::Write as _;

/// Writes one line to stdout.
pub(crate) fn line(text: &str) {
    if captured(false, text) {
        return;
    }
    // A closed pipe (`| head`) is not an error worth reporting.
    drop(writeln!(std::io::stdout(), "{text}"));
}

/// Writes one line to stderr.
pub(crate) fn err(text: &str) {
    if captured(true, text) {
        return;
    }
    drop(writeln!(std::io::stderr(), "{text}"));
}

/// Announces a step of `apply`.
pub(crate) fn step(text: &str) {
    err(&format!("==> {text}"));
}

#[cfg(test)]
thread_local! {
    static CAPTURE: std::cell::RefCell<Option<Vec<(bool, String)>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn captured(stderr: bool, text: &str) -> bool {
    CAPTURE.with_borrow_mut(|capture| {
        let Some(lines) = capture else {
            return false;
        };
        lines.push((stderr, text.to_owned()));
        true
    })
}

#[cfg(not(test))]
const fn captured(_: bool, _: &str) -> bool {
    false
}

/// The lines `f` writes on this thread, with `true` for stderr.
#[cfg(test)]
pub(crate) fn capture(f: impl FnOnce()) -> Vec<(bool, String)> {
    CAPTURE.set(Some(Vec::new()));
    f();
    CAPTURE.take().unwrap_or_default()
}
