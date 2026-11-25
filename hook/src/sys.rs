//! The one "is this process alive" check (#548), beside the hook's other libc calls. The hook is the lowest crate
//! that needs it, so the server calls it from here.

/// Whether process `pid` exists: signal 0 only checks, it sends nothing. A process the user may not signal (`EPERM`)
/// exists all the same. Pid 0 and below name process groups, never one process, and a pid past `pid_t` names none.
pub fn pid_alive(pid: impl TryInto<libc::pid_t>) -> bool {
    let Ok(pid @ 1..) = pid.try_into() else {
        return false;
    };
    // SAFETY: `kill` takes plain integers and touches no memory; signal 0 delivers no signal.
    unsafe {
        libc::kill(pid, 0) == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

#[cfg(test)]
mod tests {
    use super::pid_alive;

    #[test]
    fn this_process_is_alive_and_a_vanished_pid_is_not() {
        assert!(pid_alive(std::process::id()));
        // A child that has exited and been waited for.
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert!(!pid_alive(pid));
    }

    #[test]
    fn a_process_of_another_user_is_alive() {
        // launchd, or init: root's, so `kill` answers EPERM.
        assert!(pid_alive(1));
    }

    #[test]
    fn pid_zero_a_negative_pid_and_a_pid_past_the_type_are_not_alive() {
        assert!(!pid_alive(0u32));
        assert!(!pid_alive(-1i64));
        assert!(!pid_alive(-i64::from(std::process::id())));
        assert!(!pid_alive(u32::MAX));
        assert!(!pid_alive(i64::MAX));
    }
}
