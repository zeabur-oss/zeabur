//! Fewer copies of a person's tokens in memory, and none in a core dump.

/// Joins `parts` into a string with no spare capacity.
///
/// A string that grows moves to a larger buffer and frees the old one without clearing it. This
/// string never grows, so it leaves no copy of a secret part behind.
#[must_use]
pub fn join(parts: &[&str]) -> String {
    let mut joined = String::with_capacity(parts.iter().map(|part| part.len()).sum());
    for part in parts {
        joined.push_str(part);
    }
    joined
}

/// Stops this process from writing a core dump: `RLIMIT_CORE` becomes 0 and, on Linux, the
/// process becomes non-dumpable (`PR_SET_DUMPABLE` 0).
///
/// A non-dumpable process also refuses `ptrace` and `/proc/<pid>/mem` to other processes of its
/// user. It can still read its own `/proc/self`, except files with mode `0400`, such as
/// `environ`, which then belong to root [proc].
///
/// # Errors
///
/// For each setting that the kernel refused, a message. The other setting is still applied.
///
/// [proc]: https://man7.org/linux/man-pages/man5/proc_pid.5.html
pub fn forbid_core_dumps() -> Result<(), String> {
    let mut refused = Vec::new();
    #[cfg(unix)]
    {
        use rustix::process::{Resource, Rlimit, setrlimit};
        let none = Rlimit {
            current: Some(0),
            maximum: Some(0),
        };
        if let Err(error) = setrlimit(Resource::Core, none) {
            refused.push(format!("cannot set RLIMIT_CORE to 0: {error}"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        use rustix::process::{DumpableBehavior, set_dumpable_behavior};
        if let Err(error) = set_dumpable_behavior(DumpableBehavior::NotDumpable) {
            refused.push(format!("cannot make the process non-dumpable: {error}"));
        }
    }
    if refused.is_empty() {
        Ok(())
    } else {
        Err(refused.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_joined_string_has_no_spare_capacity() {
        let joined = join(&["Bearer ", "eyJhbGciOi.secret"]);
        assert_eq!(joined, "Bearer eyJhbGciOi.secret");
        assert_eq!(joined.capacity(), joined.len());
    }

    #[cfg(unix)]
    #[test]
    fn after_the_call_the_process_writes_no_core_dump() {
        forbid_core_dumps().unwrap();
        let core = rustix::process::getrlimit(rustix::process::Resource::Core);
        assert_eq!((core.current, core.maximum), (Some(0), Some(0)));
        #[cfg(target_os = "linux")]
        assert_eq!(
            rustix::process::dumpable_behavior().unwrap(),
            rustix::process::DumpableBehavior::NotDumpable
        );
    }
}
