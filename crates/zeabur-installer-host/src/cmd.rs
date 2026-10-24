//! Running commands.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::HostError;

/// What a command printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
}

/// Runs a command, feeding it `stdin`, and returns its standard output.
///
/// # Errors
///
/// When it cannot start or exits with a failure; the error ends with its standard error.
pub fn run(
    program: impl AsRef<Path>,
    args: &[&str],
    stdin: Option<&[u8]>,
) -> Result<String, HostError> {
    run_with(program, args, &[], stdin).map(|o| o.stdout)
}

/// Runs a command with extra environment variables and returns both outputs.
///
/// # Errors
///
/// When it cannot start or exits with a failure; the error ends with its standard error.
pub fn run_with(
    program: impl AsRef<Path>,
    args: &[&str],
    env: &[(&str, &str)],
    stdin: Option<&[u8]>,
) -> Result<Output, HostError> {
    let program = program.as_ref();
    let line = format!("{} {}", program.display(), args.join(" "));
    let fail = |reason: String| HostError::Command {
        command: line.clone(),
        reason,
    };
    let mut child = Command::new(program)
        .args(args)
        .envs(env.iter().copied())
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| fail(e.to_string()))?;
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(input).map_err(|e| fail(e.to_string()))?;
    }
    let output = child.wait_with_output().map_err(|e| fail(e.to_string()))?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        let lines: Vec<&str> = stderr.trim_end().lines().collect();
        let tail = lines
            .get(lines.len().saturating_sub(20)..)
            .unwrap_or_default();
        return Err(fail(format!("{}\n{}", output.status, tail.join("\n"))));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|e| fail(e.to_string()))?;
    Ok(Output { stdout, stderr })
}

/// Whether a command exits successfully; output is discarded.
#[must_use]
pub fn succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
