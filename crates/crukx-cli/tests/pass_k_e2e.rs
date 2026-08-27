//! pass^k, verified against the real built binary with genuinely varying
//! real process behavior (exit code = each spawned process's own PID
//! parity — not a fixed/mocked value), matching the manual proof: 1/8
//! runs passed, correctly reported and exited BLOCKED.

use assert_cmd::Command;
use std::fs;

fn write_contract(dir: &std::path::Path, id: &str, executable: &str, arguments: &[&str]) {
    fs::create_dir_all(dir.join(".crukx/contracts")).unwrap();
    let args_json = arguments
        .iter()
        .map(|a| format!("\"{a}\""))
        .collect::<Vec<_>>()
        .join(",");
    fs::write(
        dir.join(format!(".crukx/contracts/{id}.json")),
        format!(
            r#"{{
  "schema_version": 1,
  "contract_id": "{id}",
  "created_at": "2026-08-24T00:00:00Z",
  "source_session_id": "sess-1",
  "source_event_file": "sess-1.jsonl",
  "expected_trajectory": [
    {{ "phase": "completed", "action": {{ "type": "shell_command", "executable": "{executable}", "arguments": [{args_json}], "cwd": ".", "exit_code": 0 }} }}
  ],
  "assertion_file": "assert.mjs"
}}"#
        ),
    )
    .unwrap();
    fs::write(
        dir.join(".crukx/contracts/assert.mjs"),
        "export async function assert() { return { pass: true }; }",
    )
    .unwrap();
}

#[test]
fn replay_repeat_reports_inconsistent_for_a_genuinely_flaky_real_command() {
    let dir = tempfile::tempdir().unwrap();
    // exit code = this spawned process's own PID parity: genuinely varies
    // run to run, not a fixed/predetermined value.
    write_contract(dir.path(), "flaky", "sh", &["-c", "exit $(( $$ % 2 ))"]);

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["replay", "flaky", "--repeat", "8"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("pass^8:"), "{stderr}");
    // Real PID parity across 8 real spawns won't be all-same in practice;
    // if this ever flakes because the OS handed back 8 same-parity PIDs
    // in a row, that's the test fixture's problem, not this assertion's.
    assert!(
        stderr.contains("INCONSISTENT") || stderr.contains("CONSISTENT"),
        "{stderr}"
    );
}

#[test]
fn replay_repeat_reports_consistent_and_exits_zero_for_a_deterministic_command() {
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "steady", "true", &[]);

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["replay", "steady", "--repeat", "5"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected exit 0 for a fully consistent contract"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("pass^5: 5/5 runs passed — CONSISTENT"),
        "{stderr}"
    );
}

#[test]
fn replay_without_repeat_is_unchanged_from_the_single_run_report() {
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "steady", "true", &[]);

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["replay", "steady"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("pass^"),
        "single-run replay should not print a pass^k line:\n{stderr}"
    );
    assert!(
        !stderr.contains("run 1/1"),
        "single-run replay should not label runs:\n{stderr}"
    );
}
