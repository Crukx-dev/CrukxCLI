//! Tamper-evident session history, verified against the real built
//! binary — proven live by hand first: `crukx run -- true` auto-writes a
//! chain file, `crukx verify-session` reports the untampered log intact,
//! and correctly detects a hand-appended fabricated event afterward
//! (named the exact divergent index).

use assert_cmd::Command;
use std::fs;
use std::process::Command as StdCommand;

fn init_repo(dir: &std::path::Path) {
    StdCommand::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .status()
        .unwrap();
    StdCommand::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir)
        .status()
        .unwrap();
    StdCommand::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .status()
        .unwrap();
}

fn session_id_from(dir: &std::path::Path) -> String {
    fs::read_dir(dir.join(".crukx/events"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .find_map(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .strip_suffix(".jsonl")
                .map(String::from)
        })
        .expect("crukx run should have written a session event log")
}

#[test]
fn crukx_run_writes_a_chain_file_and_verify_session_reports_it_intact() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    let run_output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["run", "--", "true"])
        .output()
        .unwrap();
    assert!(run_output.status.success());

    let session_id = session_id_from(dir.path());
    assert!(
        dir.path()
            .join(format!(".crukx/events/{session_id}.chain.json"))
            .exists(),
        "chain file should exist after a real run"
    );

    let verify_output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["verify-session", &session_id])
        .output()
        .unwrap();
    assert!(
        verify_output.status.success(),
        "expected an untampered session to verify clean"
    );
    assert!(String::from_utf8_lossy(&verify_output.stderr).contains("history intact"));
}

#[test]
fn verify_session_detects_a_hand_appended_event_after_a_real_run() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());

    Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["run", "--", "true"])
        .output()
        .unwrap();
    let session_id = session_id_from(dir.path());

    let log_path = dir.path().join(format!(".crukx/events/{session_id}.jsonl"));
    let fake_event = format!(
        r#"{{"schema_version":1,"event_id":"fake","session_id":"{session_id}","sequence":99,"occurred_at":"2026-01-01T00:00:00Z","adapter":"direct","repository_root":".","phase":"completed","action":{{"type":"file_read","path":"secret.txt"}},"policy":null,"error":null}}"#
    );
    let mut raw = fs::read_to_string(&log_path).unwrap();
    raw.push_str(&fake_event);
    raw.push('\n');
    fs::write(&log_path, raw).unwrap();

    let verify_output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["verify-session", &session_id])
        .output()
        .unwrap();
    assert!(
        !verify_output.status.success(),
        "expected a tampered session to fail verification"
    );
    assert!(String::from_utf8_lossy(&verify_output.stderr).contains("TAMPERED"));
}
