//! End-to-end smoke: the real binary, real HTTP provider code, in-process mock cloud.
//! Needs a macOS GUI session (GPUI starts NSApplication).

use std::process::Command;
use std::time::Duration;

#[test]
fn mock_dictation_runs_end_to_end_through_the_http_providers() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dito"))
        .args(["--mock", "--log-sink", "--demo-run", "--data-dir"])
        .arg(dir.path())
        .output()
        .expect("run dito");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "exit {:?}\n{stdout}\n{}", out.status, String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("HUD states: Listening { mode: Toggle } -> Recognizing"), "{stdout}");
    assert!(stdout.contains("-> Polishing -> Inserted"), "{stdout}");
    // The mock server's transcript, polished by its rule-based chat endpoint.
    assert!(stdout.contains("polished=Some(\"明日の3時に会議があります。\")"), "{stdout}");
    assert!(stdout.contains("demo-run: OK"), "{stdout}");
    let _ = Duration::from_secs(0);
}

#[test]
fn unknown_arguments_fail_with_usage() {
    let out = Command::new(env!("CARGO_BIN_EXE_dito")).arg("--nope").output().expect("run dito");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown argument: --nope"));
}
