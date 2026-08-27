//! Codifies the manual fixture run this crate was proven against by hand:
//! a passing contract, gated together with an aggregate reliability
//! constraint (security_critical_delta_max) that BLOCKs the release even
//! though the contract itself PASSes. This is the whole point of the
//! CAPO-inspired reliability contract — verified against the actual built
//! binary, not just the pure functions it calls.

use assert_cmd::Command;
use std::fs;
use std::path::Path;

fn write_fixture(dir: &Path, after_findings: &str) {
    fs::create_dir_all(dir.join(".crukx/contracts")).unwrap();
    fs::create_dir_all(dir.join(".crukx/security")).unwrap();

    fs::write(
        dir.join(".crukx/contracts/demo-1.json"),
        r#"{
  "schema_version": 1,
  "contract_id": "demo-1",
  "created_at": "2026-08-23T00:00:00Z",
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
        dir.join(".crukx/contracts/assert.mjs"),
        "export async function assert() { return { pass: true, reason: \"verified\" }; }",
    )
    .unwrap();

    fs::write(
        dir.join("crukx.yml"),
        r#"contracts:
  - id: demo-1
    mode: block
    candidate_label: ci
constraints:
  vtr_minimum: 0.98
  security_critical_delta_max: 0
"#,
    )
    .unwrap();

    fs::write(dir.join(".crukx/security/before.json"), "[]").unwrap();
    fs::write(dir.join(".crukx/security/after.json"), after_findings).unwrap();
}

#[test]
fn gate_blocks_on_an_injected_critical_finding_even_though_the_contract_itself_passes() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(
        dir.path(),
        r#"[{"id":"vuln-1","severity":"critical","cwe":"CWE-78","location":"src/utils/execute.ts:84"}]"#,
    );

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["gate", "--policy", "crukx.yml"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected the gate to BLOCK (nonzero exit)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("demo-1: ✓ PASS"),
        "contract itself should PASS:\n{stderr}"
    );
    assert!(
        stderr.contains("Crukx gate: BLOCKED"),
        "gate should BLOCK on the aggregate constraint:\n{stderr}"
    );
    assert!(
        stderr.contains("+1 critical"),
        "security delta should be reported:\n{stderr}"
    );
}

#[test]
fn gate_passes_when_the_security_state_is_clean() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path(), "[]");

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["gate", "--policy", "crukx.yml"])
        .output()
        .unwrap();

    assert!(output.status.success(), "expected the gate to PASS");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Crukx gate: PASS"), "{stderr}");
}
