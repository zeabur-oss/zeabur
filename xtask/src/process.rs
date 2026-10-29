//! External tools, run as subprocesses, and waiting on what they report.

use std::ffi::OsStr;
use std::fmt::{self, Write as _};
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// How often [`wait_for`] asks again.
const POLL: Duration = Duration::from_secs(2);

/// A command line, with what to write to its standard input.
#[derive(Debug)]
pub(crate) struct Cmd {
    command: Command,
    input: Option<Vec<u8>>,
}

impl Cmd {
    pub(crate) fn new(program: impl AsRef<OsStr>) -> Self {
        let mut command = Command::new(program);
        command.stdin(Stdio::null());
        Self {
            command,
            input: None,
        }
    }

    pub(crate) fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.command.arg(arg);
        self
    }

    pub(crate) fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command.args(args);
        self
    }

    pub(crate) fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.command.env(key, value);
        self
    }

    pub(crate) fn current_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.command.current_dir(dir);
        self
    }

    pub(crate) fn input(mut self, input: impl Into<Vec<u8>>) -> Self {
        self.input = Some(input.into());
        self
    }

    /// Runs it with its output on the terminal.
    pub(crate) fn run(self) -> Result<(), String> {
        self.exec(Stdio::inherit(), Stdio::inherit()).map(drop)
    }

    /// Runs it with its standard output discarded and its errors on the terminal.
    pub(crate) fn quiet(self) -> Result<(), String> {
        self.exec(Stdio::null(), Stdio::inherit()).map(drop)
    }

    /// Its standard output; its errors go to the terminal.
    pub(crate) fn read(self) -> Result<String, String> {
        self.exec(Stdio::piped(), Stdio::inherit()).map(stdout)
    }

    /// Its standard output; its errors go only into the error, for checks that are retried.
    pub(crate) fn probe(self) -> Result<String, String> {
        self.exec(Stdio::piped(), Stdio::piped()).map(stdout)
    }

    /// Whether it succeeds, with nothing on the terminal.
    pub(crate) fn attempt(self) -> bool {
        let shown = self.to_string();
        let Self { mut command, input } = self;
        spawn(&mut command, input, Stdio::null(), Stdio::null(), &shown)
            .is_ok_and(|output| output.status.success())
    }

    /// Runs it with its output on standard error, as context for a failure, whatever it returns.
    pub(crate) fn report(self) {
        let shown = self.to_string();
        let Self { mut command, input } = self;
        if let Err(e) = spawn(
            &mut command,
            input,
            std::io::stderr().into(),
            Stdio::inherit(),
            &shown,
        ) {
            crate::output::error(&e);
        }
    }

    /// Its whole output, whether it succeeds or not.
    pub(crate) fn output(self) -> Result<Output, String> {
        let shown = self.to_string();
        let Self { mut command, input } = self;
        spawn(&mut command, input, Stdio::piped(), Stdio::piped(), &shown)
    }

    fn exec(self, out: Stdio, err: Stdio) -> Result<Output, String> {
        let shown = self.to_string();
        let Self { mut command, input } = self;
        let output = spawn(&mut command, input, out, err, &shown)?;
        if output.status.success() {
            return Ok(output);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            Err(format!("{shown} failed ({})", output.status))
        } else {
            Err(format!("{shown} failed ({}): {stderr}", output.status))
        }
    }
}

impl fmt::Display for Cmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}", self.command.get_program().to_string_lossy())?;
        for arg in self.command.get_args() {
            write!(f, " {}", arg.to_string_lossy())?;
        }
        f.write_str("`")
    }
}

fn spawn(
    command: &mut Command,
    input: Option<Vec<u8>>,
    out: Stdio,
    err: Stdio,
    shown: &str,
) -> Result<Output, String> {
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    command.stdout(out).stderr(err);
    let mut child = command
        .spawn()
        .map_err(|e| format!("running {shown}: {e}"))?;
    let writer = match (input, child.stdin.take()) {
        (Some(input), Some(mut stdin)) => Some(thread::spawn(move || stdin.write_all(&input))),
        _ => None,
    };
    let output = child
        .wait_with_output()
        .map_err(|e| format!("waiting for {shown}: {e}"))?;
    if let Some(Ok(Err(e))) = writer.map(thread::JoinHandle::join)
        && output.status.success()
    {
        return Err(format!("writing to {shown}: {e}"));
    }
    Ok(output)
}

fn stdout(output: Output) -> String {
    String::from_utf8(output.stdout)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Asks `check` every two seconds until it succeeds or `timeout` passes; the error names what was
/// awaited and what `check` last said.
pub(crate) fn wait_for(
    what: &str,
    timeout: Duration,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        match check() {
            Ok(()) => return Ok(()),
            Err(last) if Instant::now() >= deadline => {
                return Err(format!(
                    "timed out after {}s waiting for {what}; last: {last}",
                    timeout.as_secs()
                ));
            }
            Err(_) => thread::sleep(POLL),
        }
    }
}

/// `bytes` random bytes in lower-case hex.
pub(crate) fn random_hex(bytes: usize) -> Result<String, String> {
    let mut buffer = vec![0; bytes];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut random| random.read_exact(&mut buffer))
        .map_err(|e| format!("reading /dev/urandom: {e}"))?;
    let mut hex = String::with_capacity(bytes * 2);
    for byte in buffer {
        write!(hex, "{byte:02x}").map_err(|e| e.to_string())?;
    }
    Ok(hex)
}

/// Whether `tool` is a file in one of `PATH`'s directories.
pub(crate) fn on_path(tool: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(tool).is_file()))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Cmd, random_hex, wait_for};

    #[test]
    fn a_failed_command_reports_its_line_and_errors() {
        let error = Cmd::new("sh")
            .args(["-c", "echo gone >&2; exit 3"])
            .probe()
            .unwrap_err();
        assert!(error.contains("`sh -c echo gone >&2; exit 3`"), "{error}");
        assert!(error.ends_with(": gone"), "{error}");
    }

    #[test]
    fn input_reaches_the_command() {
        let out = Cmd::new("cat").input("to cat").read().unwrap();
        assert_eq!(out, "to cat");
    }

    #[test]
    fn a_timeout_names_what_was_awaited_and_the_last_error() {
        let mut tries = 0;
        let error = wait_for("the answer", Duration::ZERO, || {
            tries += 1;
            Err(format!("try {tries}"))
        })
        .unwrap_err();
        assert_eq!(
            error,
            "timed out after 0s waiting for the answer; last: try 1"
        );
    }

    #[test]
    fn waiting_ends_when_the_check_succeeds() {
        wait_for("nothing", Duration::from_secs(60), || Ok(())).unwrap();
    }

    #[test]
    fn random_hex_is_hex_of_the_length_asked() {
        let one = random_hex(16).unwrap();
        assert_eq!(one.len(), 32);
        assert!(one.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(one, random_hex(16).unwrap());
    }
}
