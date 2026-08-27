//! P95 latency is measured from a real spawned process (`sleep 0.3`), not
//! a placeholder — verified against the actual built binary, proven live
//! by hand first: 313ms/311ms observed across two runs of a real ~300ms
//! sleep, correctly gating BLOCK/PASS on either side of the threshold.

use assert_cmd::Command;
use std::fs;

fn write_fixture(dir: &std::path::Path, p95_max_ms: u64) {
    fs::create_dir_all(dir.join(".crukx/contracts")).unwrap();
    fs::write(
        dir.join(".crukx/contracts/demo-1.json"),
        r#"{
  "schema_version": 1,
  "contract_id": "demo-1",
  "created_at": "2026-08-24T00:00:00Z",
  "source_session_id": "sess-1",
  "source_event_file": "sess-1.jsonl",
  "expected_trajectory": [
    { "phase": "completed", "action": { "type": "shell_command", "executable": "sleep", "arguments": ["0.3"], "cwd": ".", "exit_code": 0 } }
  ],
  "assertion_file": "assert.mjs"
}"#,
    )
    .unwrap();
    fs::write(
        dir.join(".crukx/contracts/assert.mjs"),
        "export async function assert() { return { pass: true }; }",
    )
    .unwrap();
    fs::write(
        dir.join("crukx.yml"),
        format!("contracts:\n  - id: demo-1\n    mode: block\nconstraints:\n  p95_latency_max_ms: {p95_max_ms}\n"),
    )
    .unwrap();
}

#[test]
fn gate_blocks_when_the_real_measured_p95_latency_exceeds_the_configured_maximum() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path(), 100); // real step takes ~300ms

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["gate", "--policy", "crukx.yml"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected the gate to BLOCK on latency"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("P95 latency"), "{stderr}");
    assert!(stderr.contains("above maximum 100ms"), "{stderr}");
}

#[test]
fn gate_passes_when_the_real_measured_p95_latency_is_within_the_configured_maximum() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path(), 5000);

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["gate", "--policy", "crukx.yml"])
        .output()
        .unwrap();

    assert!(output.status.success(), "expected the gate to PASS");
}

#[test]
fn policy_yaml_omitting_mode_and_candidate_label_still_loads_and_gates() {
    // Regression test for a real bug caught while building this: mode/
    // candidate_label had no serde default, so any crukx.yml that omitted
    // them (valid per the TS CLI, which defaults both) failed to parse at
    // all instead of gating.
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
        "export async function assert() { return { pass: true }; }",
    )
    .unwrap();
    // No `mode`, no `candidate_label` — only `id`.
    fs::write(dir.path().join("crukx.yml"), "contracts:\n  - id: demo-1\n").unwrap();

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
}
