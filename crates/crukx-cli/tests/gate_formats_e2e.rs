//! E2E coverage for the gate's machine surfaces and honesty features:
//! `--format json|junit|markdown`, the Wilson-floor SLA constraint
//! (`reliability_sla_minimum`), and regression diffing versus the last
//! green run (`--fail-on-regression`). Machine output must go to *stdout*
//! (CI captures stdout), human output stays on stderr.

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
fn json_format_goes_to_stdout_and_carries_the_full_report() {
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "steady", "true", &[]);
    fs::write(
        dir.path().join("crukx.yml"),
        "contracts:\n  - id: steady\n    mode: block\n",
    )
    .unwrap();

    let output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["gate", "--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("stdout is not valid JSON ({err}):\n{stdout}"));
    assert_eq!(parsed["decision"], "PASS");
    assert_eq!(parsed["tool"], "crukx");
    assert_eq!(parsed["contracts"][0]["id"], "steady");
    assert_eq!(parsed["reliability"]["vtr"], 1.0);
    // Human report stays on stderr even in machine mode.
    assert!(String::from_utf8_lossy(&output.stderr).is_empty());
}

#[test]
fn junit_format_empts_parseable_xml_with_a_failing_testcase_on_block() {
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "broken", "false", &[]);
    fs::write(
        dir.path().join("crukx.yml"),
        "contracts:\n  - id: broken\n    mode: block\n",
    )
    .unwrap();

    let output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["gate", "--format", "junit"])
        .output()
        .unwrap();
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("<testsuites"), "{stdout}");
    // 1 failing contract + 1 failing reliability/regression testcase.
    assert!(stdout.contains("failures=\"1\""), "{stdout}");
    assert!(
        stdout.contains("<failure message="),
        "failing testcase must carry a <failure>: {stdout}"
    );
}

#[test]
fn markdown_format_renders_a_pr_ready_table() {
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "steady", "true", &[]);
    fs::write(
        dir.path().join("crukx.yml"),
        "contracts:\n  - id: steady\n    mode: block\n",
    )
    .unwrap();

    let output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["gate", "--format", "markdown"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("## Crukx gate:"), "{stdout}");
    assert!(stdout.contains("| Contract | Mode | Result | pass^k | Floor (Wilson 95%) |"));
    assert!(stdout.contains("**VTR** 100.0%"), "{stdout}");
}

#[test]
fn sla_floor_blocks_even_when_the_point_estimate_clears() {
    // flaky at repeat 5 with ~50% real pass rate: point estimate may
    // hover near 3/5, but its Wilson lower bound (~0.19) can never clear
    // an SLA of 0.9 — the gate must BLOCK regardless of luck.
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "flaky", "sh", &["-c", "exit $(( $$ % 2 ))"]);
    fs::write(
        dir.path().join("crukx.yml"),
        "contracts:\n  - id: flaky\n    mode: block\n    repeat: 5\nconstraints:\n  reliability_sla_minimum: 0.9\n",
    )
    .unwrap();

    for _ in 0..3 {
        let output = Command::cargo_bin("crukx")
            .unwrap()
            .current_dir(dir.path())
            .args(["gate"])
            .env_remove("NO_COLOR")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "SLA floor must block a coin-flip contract no matter the luck:\n{stderr}"
        );
        assert!(
            stderr.contains("below SLA"),
            "expected an SLA violation line:\n{stderr}"
        );
    }
}

#[test]
fn sla_floor_passes_a_deterministic_contract_with_high_k() {
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "steady", "true", &[]);
    fs::write(
        dir.path().join("crukx.yml"),
        "contracts:\n  - id: steady\n    mode: block\n    repeat: 20\nconstraints:\n  reliability_sla_minimum: 0.7\n",
    )
    .unwrap();

    let output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["gate"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "20/20 deterministic runs have a high Wilson floor:\n{stderr}"
    );
    assert!(!stderr.contains("below SLA"), "{stderr}");
}

#[test]
fn fail_on_regression_flags_vtr_decay_against_last_green() {
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "a", "true", &[]);
    // Same contract id as the green baseline, but failing today — that
    // flip (PASS at last green → BLOCK now) is exactly what the diff
    // must catch.
    write_contract(dir.path(), "b", "false", &[]);
    std::fs::create_dir_all(dir.path().join(".crukx")).unwrap();
    fs::write(
        dir.path().join(".crukx/runs.jsonl"),
        r#"{"ts":"2026-08-25T10:00:00Z","command":"gate","decision":"PASS","vtr":1.0,"diagnostic_coverage":1.0,"contracts":[{"id":"a","decision":"PASS"},{"id":"b","decision":"PASS"}]}
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("crukx.yml"),
        "contracts:\n  - id: a\n    mode: block\n  - id: b\n    mode: block\n",
    )
    .unwrap();

    let output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["gate", "--fail-on-regression"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("was PASS at last green"),
        "contract-level regression should be reported:\n{stderr}"
    );

    // Without the flag, the plain BLOCK still exits 1 — flag changes the
    // margin, not the verdict mechanics.
    let output_plain = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["gate"])
        .output()
        .unwrap();
    assert!(!output_plain.status.success());
}

#[test]
fn fail_on_regression_catches_pure_metric_decay_that_still_passes_thresholds() {
    let dir = tempfile::tempdir().unwrap();
    write_contract(dir.path(), "a", "true", &[]);
    // Last green: VTR 100%. Now: single contract passes → VTR 100% too.
    // No decay detectable here without a second data source, so instead
    // seed a green run whose vtr was higher than today's achievable one:
    // one passing contract gives vtr=1.0... use a warn-mode failing
    // contract to drag VTR to 50% while the gate itself PASSes.
    write_contract(dir.path(), "soft-fail", "false", &[]);
    fs::write(
        dir.path().join(".crukx/contracts/soft-fail.json"),
        r#"{
  "schema_version": 1,
  "contract_id": "soft-fail",
  "created_at": "2026-08-24T00:00:00Z",
  "source_session_id": "s",
  "source_event_file": "s.jsonl",
  "expected_trajectory": [
    { "phase": "completed", "action": { "type": "shell_command", "executable": "false", "arguments": [], "cwd": ".", "exit_code": 0 } }
  ],
  "assertion_file": "assert.mjs"
}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("crukx.yml"),
        "contracts:\n  - id: a\n    mode: block\n  - id: soft-fail\n    mode: warn\nconstraints:\n  vtr_minimum: 0.4\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join(".crukx")).unwrap();
    fs::write(
        dir.path().join(".crukx/runs.jsonl"),
        r#"{"ts":"2026-08-25T10:00:00Z","command":"gate","decision":"PASS","vtr":1.0,"diagnostic_coverage":1.0,"contracts":[{"id":"a","decision":"PASS"}]}
"#,
    )
    .unwrap();

    let with_flag = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["gate", "--fail-on-regression"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&with_flag.stderr);
    assert!(
        !with_flag.status.success(),
        "VTR 100%→50% decay must fail under --fail-on-regression even though thresholds hold:\n{stderr}"
    );
    assert!(stderr.contains("VTR regressed"), "{stderr}");

    let without_flag = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["gate"])
        .output()
        .unwrap();
    assert!(
        without_flag.status.success(),
        "without the flag the same run PASSes on absolute thresholds"
    );
}
