//! A private worker whose lifetime is bounded by its owner, including an abrupt parent exit.

use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::RuntimeError;

// The guard reads the parent's pipe, never a PID that another application could reuse. The shell
// reaps its own worker on exit; a hung worker gets two seconds before escalation.
const WATCHER: &str = r#"
exec 3<&0
"$@" </dev/null 1>&2 3<&- &
worker=$!
printf '%s\n' "$worker"
(
    read -r ignored
    kill -TERM "$worker" 2>/dev/null
    sleep 2
    kill -KILL "$worker" 2>/dev/null
) <&3 &
guard=$!
exec 3<&-
wait "$worker"
result=$?
kill "$guard" 2>/dev/null
wait "$guard" 2>/dev/null
exit "$result"
"#;

pub struct ManagedProcess {
    child: Mutex<Option<Child>>,
    pid: u32,
}

impl ManagedProcess {
    pub fn spawn(program: &std::path::Path, args: &[String], log: std::fs::File) -> Result<Self, RuntimeError> {
        let child = Command::new("/bin/sh")
            .args(["-c", WATCHER, "dito-model-worker"])
            .arg(program)
            .args(args)
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(log)
            .spawn()
            .map_err(RuntimeError::engine)?;
        let mut process = Self { child: Mutex::new(Some(child)), pid: 0 };
        let stdout = process.child.get_mut().unwrap().as_mut().unwrap().stdout.take().unwrap();
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).map_err(RuntimeError::engine)?;
        process.pid = line.trim().parse().map_err(RuntimeError::engine)?;
        Ok(process)
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn failure(&self) -> Option<String> {
        let Ok(mut child) = self.child.try_lock() else {
            return None;
        };
        match child.as_mut() {
            None => Some("model worker is stopped".into()),
            Some(child) => match child.try_wait() {
                Ok(None) => None,
                Ok(Some(status)) => Some(format!("model worker exited: {status}")),
                Err(error) => Some(format!("model worker: {error}")),
            },
        }
    }

    pub fn shutdown(&self) {
        let mut slot = self.child.lock().unwrap_or_else(|error| error.into_inner());
        let Some(mut child) = slot.take() else { return };
        // Released before the wait, so `failure` reports the stopped worker at once instead of finding the lock held.
        drop(slot);
        drop(child.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                _ => break,
            }
        }
        // SAFETY: this unreaped child owns the process group created by process_group(0). Its group
        // cannot be reused while the child is alive, and no unrelated process is added to it.
        unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
        let _ = child.wait();
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn shutdown_stops_a_live_worker_even_when_the_handle_is_retained() {
        let log = tempfile::tempfile().unwrap();
        let process = ManagedProcess::spawn(std::path::Path::new("/bin/sleep"), &["60".into()], log).unwrap();
        assert!(process.failure().is_none());
        process.shutdown();
        assert!(process.failure().is_some());
        // SAFETY: signal 0 checks for existence without changing the target process.
        assert_eq!(unsafe { libc::kill(process.pid() as i32, 0) }, -1);
    }

    const IGNORE_TERM: &str = "DITO_WORKER_TEST_IGNORE_TERM";

    #[test]
    fn a_worker_is_reported_stopped_while_its_shutdown_still_waits_for_it() {
        if std::env::var_os(IGNORE_TERM).is_some() {
            // The worker: it reports once TERM is ignored, then exits on its own.
            // SAFETY: signal changes a process-wide disposition before anything else in this process depends on it.
            unsafe { libc::signal(libc::SIGTERM, libc::SIG_IGN) };
            eprintln!("ready");
            std::thread::sleep(Duration::from_secs(2));
            return;
        }
        // The worker is this test run again with `IGNORE_TERM`, so the shutdown has to wait for it.
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("worker.log");
        let process = ManagedProcess::spawn(
            std::path::Path::new("/usr/bin/env"),
            &[
                format!("{IGNORE_TERM}=1"),
                std::env::current_exe().unwrap().display().to_string(),
                "--exact".into(),
                "managed_process::tests::a_worker_is_reported_stopped_while_its_shutdown_still_waits_for_it".into(),
                "--nocapture".into(),
            ],
            std::fs::File::create(&log).unwrap(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while !std::fs::read_to_string(&log).unwrap_or_default().contains("ready\n") {
            assert!(Instant::now() < deadline, "the worker never ignored TERM");
            std::thread::sleep(Duration::from_millis(5));
        }
        let owner = Arc::new(process);
        let shutting_down = Arc::clone(&owner);
        let shutdown = std::thread::spawn(move || shutting_down.shutdown());
        let deadline = Instant::now() + Duration::from_secs(3);
        while owner.failure().is_none() {
            assert!(Instant::now() < deadline, "the shutdown never took the worker");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(owner.failure().as_deref(), Some("model worker is stopped"));
        // SAFETY: signal 0 checks for existence without changing the target process.
        let alive = unsafe { libc::kill(owner.pid() as i32, 0) };
        assert_eq!(alive, 0, "the stop was reported only after the worker was reaped");
        shutdown.join().unwrap();
    }

    #[test]
    fn worker_exit_is_reported_without_waiting_for_the_owner_to_close() {
        let process =
            ManagedProcess::spawn(std::path::Path::new("/usr/bin/false"), &[], tempfile::tempfile().unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while process.failure().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn an_owner_exit_without_destructors_still_stops_its_worker() {
        const MARKER: &str = "DITO_WORKER_TEST_PID_FILE";
        if let Some(path) = std::env::var_os(MARKER) {
            let worker = ManagedProcess::spawn(
                std::path::Path::new("/bin/sleep"),
                &["60".into()],
                tempfile::tempfile().unwrap(),
            )
            .unwrap();
            std::fs::write(path, worker.pid().to_string()).unwrap();
            std::process::exit(0);
        }
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("worker.pid");
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "managed_process::tests::an_owner_exit_without_destructors_still_stops_its_worker"])
            .env(MARKER, &marker)
            .status()
            .unwrap();
        assert!(status.success());
        let pid: i32 = std::fs::read_to_string(marker).unwrap().parse().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        // SAFETY: signal 0 only queries whether this test's worker still exists.
        while unsafe { libc::kill(pid, 0) } == 0 {
            assert!(Instant::now() < deadline, "worker survived its owner");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
