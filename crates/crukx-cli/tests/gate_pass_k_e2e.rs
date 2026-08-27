//! `crukx.yml`'s per-contract `repeat: k` threaded through the full gate,
//! verified live by hand first: a flaky contract (exit code = each real
//! spawned process's own PID parity) at repeat: 8 scored 2/8 real passes
//! and correctly BLOCKED; a deterministic contract at repeat: 5 scored
//! 5/5 and correctly PASSed. VTR/diagnostic-coverage/P95-latency all kept
//! working unmodified, since pass^k collapses into the same
//! ReplayOutcome.decision those already read.

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
fn gate_blocks_a_flaky_contract_via_pass_k_even_with_repeat_configured_in_crukx_yml() {
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "flaky", "sh", &["-c", "exit $(( $$ % 2 ))"]);
    fs::write(
        dir.path().join("crukx.yml"),
        "contracts:\n  - id: flaky\n    mode: block\n    repeat: 8\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["gate", "--policy", "crukx.yml"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected the gate to BLOCK on pass^k inconsistency"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("pass^8:"), "{stderr}");
    assert!(stderr.contains("Crukx gate: BLOCKED"), "{stderr}");
}

#[test]
fn gate_passes_a_deterministic_contract_at_repeat_5() {
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "steady", "true", &[]);
    fs::write(
        dir.path().join("crukx.yml"),
        "contracts:\n  - id: steady\n    mode: block\n    repeat: 5\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["gate", "--policy", "crukx.yml"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected the gate to PASS: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("pass^5: 5/5"), "{stderr}");
    assert!(stderr.contains("VTR                  100.0%"), "{stderr}");
}

#[test]
fn gate_without_repeat_is_unchanged_from_the_single_run_report() {
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "steady", "true", &[]);
    fs::write(
        dir.path().join("crukx.yml"),
        "contracts:\n  - id: steady\n    mode: block\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["gate", "--policy", "crukx.yml"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("pass^"),
        "repeat=1 (default) should not print a pass^k annotation:\n{stderr}"
    );
}
