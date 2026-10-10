use assert_cmd::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("crukx")
        .unwrap()
        .args(args)
        .output()
        .unwrap()
}

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

#[test]
fn deploy_then_verify_artifact_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("report.json");
    let verification = dir.path().join("verification.json");
    let out = dir.path().join("artifact");
    std::fs::write(&report, REPORT_JSON).unwrap();
    std::fs::write(&verification, VERIFICATION_JSON).unwrap();

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

    let verify = run(&["verify-artifact", out.to_str().unwrap()]);
    assert_eq!(verify.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(stdout.contains("RELEASABLE"), "unexpected stdout:\n{stdout}");
    assert!(stdout.contains("Fixture/Model"), "unexpected stdout:\n{stdout}");
}

#[test]
fn verify_artifact_rejects_tampered_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let pkg = dir.path().join("pkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("manifest.json"), r#"{"tampered": true}"#).unwrap();
    std::fs::write(pkg.join("evidence.json"), r#"{}"#).unwrap();
    let output = run(&["verify-artifact", pkg.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("TAMPERED"), "unexpected stderr:\n{stderr}");
}

#[test]
fn verify_artifact_missing_dir_exits_2() {
    let output = run(&["verify-artifact", "/nonexistent-artifact-dir"]);
    assert_eq!(output.status.code(), Some(2));
}
