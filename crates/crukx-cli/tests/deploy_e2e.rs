use assert_cmd::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("crukx")
        .unwrap()
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn benchmark_help_lists_model_positional() {
    let output = run(&["benchmark", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("<MODEL>"), "unexpected help:\n{stdout}");
}

#[test]
fn benchmark_missing_model_file_exits_2() {
    let output = run(&["benchmark", "/nonexistent-model.gguf"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("model file not found"),
        "unexpected stderr:\n{stderr}"
    );
}

#[test]
fn deploy_missing_report_exits_2() {
    let output = run(&[
        "deploy",
        "--report",
        "/nonexistent-report.json",
        "--model",
        "M",
        "--source-hash",
        "H",
        "--workload",
        "W",
        "--transform",
        "T",
        "--verification",
        "/nonexistent-verification.json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot read benchmark report"),
        "unexpected stderr:\n{stderr}"
    );
}

#[test]
fn deploy_rejects_malformed_verification_record() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("report.json");
    std::fs::write(
        &report,
        r#"{
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
}"#,
    )
    .unwrap();
    let bad = dir.path().join("verification.json");
    std::fs::write(&bad, r#"{"not": "a verification record"}"#).unwrap();
    let output = Command::cargo_bin("crukx")
        .unwrap()
        .args([
            "deploy",
            "--report",
            report.to_str().unwrap(),
            "--model",
            "M",
            "--source-hash",
            "H",
            "--workload",
            "W",
            "--transform",
            "T",
            "--verification",
            bad.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a verification record"),
        "unexpected stderr:\n{stderr}"
    );
}
