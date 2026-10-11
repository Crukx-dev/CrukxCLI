use assert_cmd::Command;

const REPORT_A: &str = r#"{
  "protocol_version": 1,
  "device": {
    "schema_version": 1,
    "device_id": "abc",
    "cpu": {"arch": "x86_64", "logical_cores": 8},
    "memory": {},
    "os": {"name": "linux", "arch": "x86_64"}
  },
  "runtime": {"kind": "llama_cpp", "name": "llama.cpp", "version": "1"},
  "config": {"warmup_runs": 0, "measured_runs": 2, "prompt": "hi", "max_tokens": 8, "timeout_secs": 5},
  "measurements": [],
  "summary": {"samples": 2, "min_ms": 90, "max_ms": 110, "mean_ms": 100.0, "median_ms": 100, "p95_ms": 110}
}"#;

const REPORT_B: &str = r#"{
  "protocol_version": 1,
  "device": {
    "schema_version": 1,
    "device_id": "abc",
    "cpu": {"arch": "x86_64", "logical_cores": 8},
    "memory": {},
    "os": {"name": "linux", "arch": "x86_64"}
  },
  "runtime": {"kind": "llama_cpp", "name": "llama.cpp", "version": "1"},
  "config": {"warmup_runs": 0, "measured_runs": 2, "prompt": "hi", "max_tokens": 8, "timeout_secs": 5},
  "measurements": [],
  "summary": {"samples": 2, "min_ms": 99, "max_ms": 121, "mean_ms": 110.0, "median_ms": 110, "p95_ms": 121}
}"#;

#[test]
fn compare_reports_ten_percent_regression() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.json");
    let b = dir.path().join("b.json");
    std::fs::write(&a, REPORT_A).unwrap();
    std::fs::write(&b, REPORT_B).unwrap();
    let output = Command::cargo_bin("crukx")
        .unwrap()
        .args(["compare", a.to_str().unwrap(), b.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("same"), "unexpected stdout:\n{stdout}");
    assert!(stdout.contains("+10.0%"), "unexpected stdout:\n{stdout}");
}

#[test]
fn compare_flags_different_devices() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.json");
    let b = dir.path().join("b.json");
    std::fs::write(&a, REPORT_A).unwrap();
    std::fs::write(
        &b,
        REPORT_B.replace("\"device_id\": \"abc\"", "\"device_id\": \"xyz\""),
    )
    .unwrap();
    let output = Command::cargo_bin("crukx")
        .unwrap()
        .args([
            "compare",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("compare --json must be valid JSON");
    assert_eq!(value["same_device"], serde_json::Value::Bool(false));
}

#[test]
fn compare_missing_file_exits_2() {
    let output = Command::cargo_bin("crukx")
        .unwrap()
        .args(["compare", "/nonexistent-a.json", "/nonexistent-b.json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
