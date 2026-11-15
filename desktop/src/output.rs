//! The only place the app writes to the terminal. A debug build shows it in the console; a Windows
//! release build has no console.

/// Writes a problem that the window does not show.
#[expect(
    clippy::print_stderr,
    reason = "this module is the app's terminal output"
)]
pub(crate) fn warn(message: &str) {
    eprintln!("zeabur-desktop: {message}");
}
