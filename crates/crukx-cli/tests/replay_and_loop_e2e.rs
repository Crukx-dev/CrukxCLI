//! Two things verified against the real built binary:
//! 1. standalone `crukx replay <contract-id>` (not just via `gate`).
//! 2. the full close-run -> capture -> gate loop with zero hand-authored
//!    fixture JSON — everything after `git init` comes from the tool
//!    itself, mirroring the manual e2e run this was proven against by
//!    hand this session.

use assert_cmd::Command;
use std::fs;
use std::path::Path;

fn init_repo(dir: &Path) {
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .status()
        .unwrap();
}

#[test]
fn standalone_replay_reports_pass_for_a_passing_contract() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".crukx/contracts")).unwrap();
    fs::write(
        dir.path().join(".crukx/contracts/demo-1.json"),
        r#"{
  "schema_version": 1,
  "contract_id": "demo-1",
  "created_at": "2026-08-24T00:00:00Z",
  "source_session_id": "sess-1",
  "source_event_file": "sess-1.jsonl",
  "expected_trajectory": [
    { "phase": "completed", "action": { "type": "shell_command", "executable": "true", "arguments": [], "cwd": ".", "exit_code": 0 } }
  ],
  "assertion_file": "assert.mjs"
}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join(".crukx/contracts/assert.mjs"),
        "export async function assert() { return { pass: true, reason: \"verified\" }; }",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["replay", "demo-1", "--candidate", "ci"])
        .output()
        .unwrap();

    assert!(output.status.success(), "expected replay to PASS");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("contract demo-1 vs candidate \"ci\""),
        "{stderr}"
    );
    assert!(stderr.contains("[match] true"), "{stderr}");
    assert!(stderr.contains("PASS"), "{stderr}");
}

#[test]
fn standalone_replay_reports_diverged_step_and_blocks() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".crukx/contracts")).unwrap();
    // captured exit_code was 0, `false` always exits 1 -> divergence
    fs::write(
        dir.path().join(".crukx/contracts/demo-1.json"),
        r#"{
  "schema_version": 1,
  "contract_id": "demo-1",
  "created_at": "2026-08-24T00:00:00Z",
  "source_session_id": "sess-1",
  "source_event_file": "sess-1.jsonl",
  "expected_trajectory": [
    { "phase": "completed", "action": { "type": "shell_command", "executable": "false", "arguments": [], "cwd": ".", "exit_code": 0 } }
  ],
  "assertion_file": "assert.mjs"
}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join(".crukx/contracts/assert.mjs"),
        "export async function assert() { return { pass: true }; }",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["replay", "demo-1"])
        .output()
        .unwrap();

    assert!(!output.status.success(), "expected replay to BLOCK");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[DIVERGED]"), "{stderr}");
    assert!(stderr.contains("BLOCKED"), "{stderr}");
}

#[test]
fn full_loop_run_then_capture_then_gate_with_no_hand_authored_fixtures() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    std::process::Command::new("git")
        .args(["add", "a.txt"])
        .current_dir(dir.path())
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "init"])
        .current_dir(dir.path())
        .status()
        .unwrap();

    let run_output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["run", "--", "true"])
        .output()
        .unwrap();
    assert!(run_output.status.success(), "expected `run` to succeed");

    let capture_output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["capture", "latest"])
        .output()
        .unwrap();
    assert!(
        capture_output.status.success(),
        "expected `capture` to succeed"
    );

    let contract_id = fs::read_dir(dir.path().join(".crukx/contracts"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .find_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".json").map(String::from)
        })
        .expect("capture should have written a contract .json file");

    fs::write(
        dir.path().join("crukx.yml"),
        format!("contracts:\n  - id: {contract_id}\n    mode: block\n    candidate_label: ci\n"),
    )
    .unwrap();

    let gate_output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["gate", "--policy", "crukx.yml"])
        .output()
        .unwrap();
    assert!(
        gate_output.status.success(),
        "expected the gate to PASS:\n{}",
        String::from_utf8_lossy(&gate_output.stderr)
    );
    let stderr = String::from_utf8_lossy(&gate_output.stderr);
    assert!(stderr.contains("Crukx gate: PASS"), "{stderr}");
}
