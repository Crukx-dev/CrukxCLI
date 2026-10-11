use assert_cmd::Command;

const REPORT_JSON: &str = r#"{
  "protocol_version": 1,
  "device": {
    "schema_version": 1,
    "device_id": "abc",
    "cpu": {"arch": "x86_64", "logical_cores": 8},
    "memory": {},
    "os": {"name": "linux", "arch": "x86_64"}
  },
  "runtime": {"kind": "llama_cpp", "name": "llama.cpp"},
  "config": {"warmup_runs": 0, "measured_runs": 1, "prompt": "hi", "max_tokens": 8, "timeout_secs": 5}
}"#;

const VERIFICATION_JSON: &str = r#"{
  "suite": "smoke",
  "critical_pass": true,
  "task_success_delta": 0.0,
  "confidence": 1.0
}"#;

const CANDIDATES_JSON: &str = r#"[
  {"id": "Q4", "size_bytes": 2500000000, "peak_memory_mb": 4700, "tok_s": 51.0, "quality_regression": 0.0019},
  {"id": "Q3", "size_bytes": 2000000000, "peak_memory_mb": 4200, "tok_s": 58.0, "quality_regression": 0.028}
]"#;

#[test]
fn deploy_then_report_names_selection_and_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("report.json");
    let verification = dir.path().join("verification.json");
    let candidates = dir.path().join("candidates.json");
    let out = dir.path().join("artifact");
    let report_md = out.join("REPORT.md");
    std::fs::write(&report, REPORT_JSON).unwrap();
    std::fs::write(&verification, VERIFICATION_JSON).unwrap();
    std::fs::write(&candidates, CANDIDATES_JSON).unwrap();

    let deploy = Command::cargo_bin("crukx")
        .unwrap()
        .args([
            "deploy",
            "--report",
            report.to_str().unwrap(),
            "--model",
            "Fixture/Model",
            "--source-hash",
            "sha256:fixture",
            "--workload",
            "smoke",
            "--transform",
            "fixture_noop",
            "--verification",
            verification.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(deploy.status.code(), Some(0));

    let rendered = Command::cargo_bin("crukx")
        .unwrap()
        .args([
            "report",
            "--artifact",
            out.to_str().unwrap(),
            "--candidates",
            candidates.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(rendered.status.code(), Some(0));
    let markdown = std::fs::read_to_string(&report_md).unwrap();
    assert!(
        markdown.contains("Selected `Q4`"),
        "unexpected report:\n{markdown}"
    );
    assert!(
        markdown.contains("rejected: quality regression"),
        "unexpected report:\n{markdown}"
    );
}

#[test]
fn report_missing_candidates_exits_2() {
    let output = Command::cargo_bin("crukx")
        .unwrap()
        .args([
            "report",
            "--artifact",
            "/nonexistent-artifact",
            "--candidates",
            "/nonexistent-candidates.json",
        ])
        .output()
        .unwrap();
    assert!(matches!(output.status.code(), Some(1) | Some(2)));
}
