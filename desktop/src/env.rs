//! The environment the app gives to every child process, and the login shell read that fills it.
//!
//! An app started from Finder, the Dock or a desktop launcher does not get the variables set in
//! the person's shell files. The app reads the login shell's environment once and merges it with
//! its own. The same merged environment decides if an exec plugin is found and is passed to every
//! child process with `Command::envs`; the app never calls `std::env::set_var`
//! (<https://doc.rust-lang.org/std/env/fn.set_var.html>).

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

/// The variables for child processes, in the order they were added.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChildEnv {
    vars: Vec<(OsString, OsString)>,
}

/// Windows variable names do not depend on case (`Path` and `PATH` are one variable).
fn same_name(a: &OsStr, b: &OsStr) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

impl ChildEnv {
    pub fn from_process() -> Self {
        Self {
            vars: std::env::vars_os().collect(),
        }
    }

    /// The app's own variables win, except `PATH`: the shell's entries come first, then the app's,
    /// each directory once.
    #[cfg(any(unix, test))]
    pub fn merge(own: Vec<(OsString, OsString)>, shell: &[(OsString, OsString)]) -> Self {
        let mut env = Self { vars: own };
        let mut shell_path = None;
        for (name, value) in shell {
            if same_name(name, OsStr::new("PATH")) {
                shell_path = Some(value.clone());
            } else if env.get(name).is_none() {
                env.vars.push((name.clone(), value.clone()));
            }
        }
        let Some(shell_path) = shell_path else {
            return env;
        };
        let mut dirs: Vec<PathBuf> = Vec::new();
        let own_path = env
            .get(OsStr::new("PATH"))
            .map(OsStr::to_os_string)
            .unwrap_or_default();
        for list in [&shell_path, &own_path] {
            for dir in std::env::split_paths(list) {
                if !dir.as_os_str().is_empty() && !dirs.contains(&dir) {
                    dirs.push(dir);
                }
            }
        }
        if let Ok(joined) = std::env::join_paths(dirs) {
            env.set(OsString::from("PATH"), joined);
        }
        env
    }

    pub fn get(&self, name: &OsStr) -> Option<&OsStr> {
        for (key, value) in &self.vars {
            if same_name(key, name) {
                return Some(value);
            }
        }
        None
    }

    #[cfg(any(unix, test))]
    fn set(&mut self, name: OsString, value: OsString) {
        for (key, old) in &mut self.vars {
            if same_name(key, &name) {
                *old = value;
                return;
            }
        }
        self.vars.push((name, value));
    }

    pub fn path_dirs(&self) -> Vec<PathBuf> {
        match self.get(OsStr::new("PATH")) {
            Some(path) => std::env::split_paths(path).collect(),
            None => Vec::new(),
        }
    }

    /// A command that runs with exactly this environment. On Unix, `Command` also looks the
    /// program up on this `PATH` (<https://doc.rust-lang.org/std/process/struct.Command.html>).
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the Zeabur backend started by Open runs through it"
        )
    )]
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .envs(self.vars.iter().map(|(k, v)| (k, v)));
        command
    }
}

/// Where reading the login shell's environment stands; shown in the window.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum ShellRead {
    Pending,
    #[cfg(unix)]
    Done,
    /// Windows: the app uses its own environment.
    #[cfg(not(unix))]
    Skipped,
    #[cfg(unix)]
    #[serde(rename_all = "camelCase")]
    Failed {
        shell: String,
        timed_out: bool,
        detail: String,
    },
}

#[cfg(unix)]
pub use unix::{login_shell, read_shell_env};

#[cfg(unix)]
mod unix {
    use std::ffi::OsString;
    use std::io::Read;
    use std::os::unix::ffi::OsStringExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    /// The shell from the passwd entry, then `$SHELL`, then `/bin/zsh` on macOS or `/bin/sh`.
    pub fn login_shell() -> PathBuf {
        if let Ok(Some(user)) = nix::unistd::User::from_uid(nix::unistd::Uid::current())
            && !user.shell.as_os_str().is_empty()
        {
            return user.shell;
        }
        if let Some(shell) = std::env::var_os("SHELL").filter(|s| !s.is_empty()) {
            return PathBuf::from(shell);
        }
        PathBuf::from(if cfg!(target_os = "macos") {
            "/bin/zsh"
        } else {
            "/bin/sh"
        })
    }

    /// zsh reads `~/.zshrc` only in an interactive shell, also with `-l`. fish refuses `-i` with
    /// `-c`.
    pub(super) fn shell_args(shell: &Path, script: &str) -> Vec<String> {
        let mut args = vec!["-l".to_owned()];
        if shell.file_name().is_none_or(|name| name != "fish") {
            args.push("-i".to_owned());
        }
        args.push("-c".to_owned());
        args.push(script.to_owned());
        args
    }

    /// The text between the first and the last `marker`, read as `env -0` output. Shell files can
    /// print before the command runs, and `~/.zlogout` after it.
    pub(super) fn parse_env0(output: &[u8], marker: &[u8]) -> Option<Vec<(OsString, OsString)>> {
        let start = find(output, marker, false)? + marker.len();
        let end = find(output, marker, true)?;
        let body = output.get(start..end)?;
        let mut vars = Vec::new();
        for entry in body.split(|b| *b == 0) {
            let Some(eq) = entry.iter().position(|b| *b == b'=') else {
                continue;
            };
            if eq == 0 {
                continue;
            }
            let (name, value) = entry.split_at(eq);
            let value = value.get(1..).unwrap_or_default();
            vars.push((
                OsString::from_vec(name.to_vec()),
                OsString::from_vec(value.to_vec()),
            ));
        }
        Some(vars)
    }

    fn find(haystack: &[u8], needle: &[u8], last: bool) -> Option<usize> {
        let mut found = None;
        for (index, window) in haystack.windows(needle.len()).enumerate() {
            if window == needle {
                found = Some(index);
                if !last {
                    break;
                }
            }
        }
        found
    }

    pub enum Failure {
        Start(std::io::Error),
        TimedOut,
        NoEnvironment(Option<i32>),
    }

    /// Runs the login shell once, with `DISABLE_AUTO_UPDATE=true` so Oh My Zsh does not stop to
    /// ask about updates, no stdin, and the home folder as the working directory.
    pub fn read_shell_env(
        shell: &Path,
        timeout: Duration,
    ) -> Result<Vec<(OsString, OsString)>, Failure> {
        let marker = format!("_ZEABUR_SHELL_ENV_{}_", std::process::id());
        // `/usr/bin/env` by absolute path: `-i` loads aliases, and one could be named `env`.
        let script = format!("printf %s {marker}; /usr/bin/env -0; printf %s {marker}");
        let mut command = Command::new(shell);
        command
            .args(shell_args(shell, &script))
            .env("DISABLE_AUTO_UPDATE", "true")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(home) = std::env::home_dir() {
            command.current_dir(home);
        }
        let mut child = command.spawn().map_err(Failure::Start)?;
        let Some(mut stdout) = child.stdout.take() else {
            return Err(Failure::NoEnvironment(None));
        };
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let read = stdout.read_to_end(&mut output);
            let _sent = send.send(read.map(|_| output));
        });
        let received = receive.recv_timeout(timeout);
        let status = child.try_wait().ok().flatten();
        // The shell is done once its output closes; this only stops one that hangs.
        let _killed = child.kill();
        let _waited = child.wait();
        let Ok(read) = received else {
            return Err(Failure::TimedOut);
        };
        let output = read.map_err(Failure::Start)?;
        parse_env0(&output, marker.as_bytes())
            .ok_or_else(|| Failure::NoEnvironment(status.and_then(|s| s.code())))
    }
}

/// Reads the login shell's environment and returns the merged environment and how the read went.
pub fn load(own: ChildEnv) -> (ChildEnv, ShellRead) {
    #[cfg(unix)]
    {
        let shell = login_shell();
        let name = shell.display().to_string();
        match read_shell_env(&shell, std::time::Duration::from_secs(10)) {
            Ok(vars) => (ChildEnv::merge(own.vars, &vars), ShellRead::Done),
            Err(failure) => {
                let (timed_out, detail) = match failure {
                    unix::Failure::Start(err) => (false, err.to_string()),
                    unix::Failure::TimedOut => (true, String::new()),
                    unix::Failure::NoEnvironment(Some(code)) => {
                        (false, format!("exit status {code}"))
                    }
                    unix::Failure::NoEnvironment(None) => (false, String::new()),
                };
                (
                    own,
                    ShellRead::Failed {
                        shell: name,
                        timed_out,
                        detail,
                    },
                )
            }
        }
    }
    #[cfg(not(unix))]
    {
        (own, ShellRead::Skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        let mut out = Vec::new();
        for (k, v) in pairs {
            out.push((OsString::from(k), OsString::from(v)));
        }
        out
    }

    fn joined(dirs: &[&str]) -> String {
        std::env::join_paths(dirs).unwrap().into_string().unwrap()
    }

    #[test]
    fn own_variables_win_except_path_which_puts_the_shell_first() {
        let own = vars(&[
            ("PATH", &joined(&["/usr/bin", "/bin"])),
            ("AWS_PROFILE", "app"),
        ]);
        let shell = vars(&[
            (
                "PATH",
                &joined(&["/opt/homebrew/bin", "", "/usr/bin", "/opt/homebrew/bin"]),
            ),
            ("AWS_PROFILE", "shell"),
            ("CLOUDSDK_CONFIG", "/home/a/gcloud"),
        ]);
        let env = ChildEnv::merge(own, &shell);
        let path: Vec<PathBuf> = env.path_dirs();
        let want: Vec<PathBuf> = ["/opt/homebrew/bin", "/usr/bin", "/bin"]
            .iter()
            .map(PathBuf::from)
            .collect();
        assert_eq!(path, want);
        assert_eq!(env.get(OsStr::new("AWS_PROFILE")), Some(OsStr::new("app")));
        assert_eq!(
            env.get(OsStr::new("CLOUDSDK_CONFIG")),
            Some(OsStr::new("/home/a/gcloud"))
        );
    }

    #[test]
    fn without_a_shell_path_the_own_path_stays() {
        let own = vars(&[("PATH", &joined(&["/usr/bin", "/bin"]))]);
        let env = ChildEnv::merge(own.clone(), &vars(&[("HOME", "/home/a")]));
        assert_eq!(
            env.get(OsStr::new("PATH")),
            own.first().map(|(_, v)| v.as_os_str())
        );
    }

    #[test]
    fn a_command_gets_exactly_the_merged_environment() {
        let env = ChildEnv::merge(vars(&[("A", "1")]), &vars(&[("B", "2")]));
        let command = env.command("kubectl");
        let mut got: Vec<(String, Option<String>)> = Vec::new();
        for (k, v) in command.get_envs() {
            got.push((
                k.to_string_lossy().into_owned(),
                v.map(|v| v.to_string_lossy().into_owned()),
            ));
        }
        assert_eq!(
            got,
            vec![
                ("A".into(), Some("1".into())),
                ("B".into(), Some("2".into()))
            ]
        );
    }

    #[cfg(unix)]
    mod unix_shell {
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;
        use std::time::Duration;

        use super::super::unix::{Failure, parse_env0, read_shell_env, shell_args};
        use super::*;

        #[test]
        fn env0_output_is_read_between_the_markers() {
            let output = b"Welcome!\nMARKA=1\0MULTI=line one\nline two\0EQ=a=b\0\0MARKbye\n";
            let parsed = parse_env0(output, b"MARK").unwrap();
            assert_eq!(
                parsed,
                vars(&[("A", "1"), ("MULTI", "line one\nline two"), ("EQ", "a=b")])
            );
            assert_eq!(parse_env0(b"no markers here", b"MARK"), None);
        }

        #[test]
        fn fish_gets_no_interactive_flag() {
            assert_eq!(
                shell_args(Path::new("/opt/homebrew/bin/fish"), "x"),
                ["-l", "-c", "x"]
            );
            assert_eq!(
                shell_args(Path::new("/bin/zsh"), "x"),
                ["-l", "-i", "-c", "x"]
            );
        }

        fn fake_shell(name: &str, body: &str) -> PathBuf {
            let dir =
                std::env::temp_dir().join(format!("zeabur-desktop-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("zsh");
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        #[test]
        fn a_shell_that_prints_around_the_command_still_gives_its_environment() {
            // zsh gets `-l -i -c <script>`; the script is the fourth argument.
            let shell = fake_shell(
                "noisy",
                r#"echo "last login"; export SHELL_ONLY=yes; /bin/sh -c "$4""#,
            );
            let vars = read_shell_env(&shell, Duration::from_secs(10))
                .ok()
                .unwrap();
            let env = ChildEnv::merge(Vec::new(), &vars);
            assert_eq!(env.get(OsStr::new("SHELL_ONLY")), Some(OsStr::new("yes")));
            assert_eq!(
                env.get(OsStr::new("DISABLE_AUTO_UPDATE")),
                Some(OsStr::new("true"))
            );
        }

        #[test]
        fn a_hanging_shell_times_out() {
            let shell = fake_shell("hang", "exec sleep 30");
            let result = read_shell_env(&shell, Duration::from_millis(300));
            assert!(matches!(result, Err(Failure::TimedOut)));
        }
    }
}
