//! `crukx.yml`'s per-contract `repeat: k` threaded through the full gate.
//! The flaky contract fails deterministically on exactly one of its runs
//! via a counter file in the shared replay cwd (all repeats run
//! sequentially in the same directory): 7/8 passes and the gate BLOCKs.
//! A previous revision used child-PID parity as the flakiness source, but
//! PID allocation patterns are environment-dependent — it scored 8/8 even
//! PIDs on fedora:41 CI and the gate wrongly PASSed. Wall-clock, PID, and
//! scheduling state must never decide this test's outcome.

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
    // Deterministic flake: passes runs 1-7, fails run 8. Every repeat runs
    // sequentially with cwd = dir, so the counter file is exact — no PID,
    // timing, or scheduler dependence.
    write_contract(
        dir.path(),
        "flaky",
        "sh",
        &[
            "-c",
            "c=$(cat .flaky_count 2>/dev/null || echo 0); echo $((c+1)) > .flaky_count; [ $c -lt 7 ]",
        ],
    );
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
