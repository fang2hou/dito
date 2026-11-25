use std::io;
use std::process::{Child, Command};

/// Starts `command` without waiting for it; the error is the start's.
pub fn spawn_and_reap(command: &mut Command) -> io::Result<()> {
    reap(command.spawn()?);
    Ok(())
}

/// A dropped `Child` is neither killed nor waited for, so a child that has exited stays a zombie until Dito quits. A
/// thread waits for it instead, which never blocks the caller.
fn reap(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    /// Signal 0 reaches no process and only asks whether `pid` exists: a zombie still answers, a reaped child is gone.
    fn alive(pid: i32) -> bool {
        // SAFETY: `pid` is a child this test started, and signal 0 is never delivered.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    #[test]
    fn a_short_lived_child_is_reaped() {
        let child = Command::new("/usr/bin/true").spawn().expect("start true");
        let pid = child.id() as i32;
        reap(child);
        let deadline = Instant::now() + Duration::from_secs(5);
        while alive(pid) {
            assert!(Instant::now() < deadline, "the child was not reaped");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
