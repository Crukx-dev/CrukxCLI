// Fixtures are /bin/sh scripts, so this slice only runs on unix.
#![cfg(unix)]

use crukx_core::contracts::artifact::{OptimizationRecord, VerificationRecord};
use crukx_core::deploy::benchmarks::{BenchmarkConfig, BENCHMARK_PROTOCOL_VERSION};
use crukx_core::deploy::devices::DeviceProfile;
use crukx_core::deploy::exporters::{is_releasable, package, sha256_hex, PackageInput};
use crukx_core::deploy::runtimes::{LlamaCppConfig, RuntimeAdapter, RuntimeKind};
use crukx_runner::benchmarks::benchmark;
use crukx_runner::exporters::verify_package;
use crukx_runner::runtimes::LlamaCppAdapter;

#[cfg(unix)]
fn script_binary(name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "crukx-slice-{}-{}-{}.sh",
        name,
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::write(&path, format!("#!/bin/sh\n{}", body)).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn optimize_verify_deploy_slice_with_fixture_runtime() {
    let profile = DeviceProfile::mock_metal();
    let binary = script_binary("llama", "echo 'fixture tokens'\nexit 0\n");

    let config = LlamaCppConfig::from_profile(&profile);
    assert_eq!(config.n_threads, profile.cpu.logical_cores);
    let adapter = LlamaCppAdapter::new(LlamaCppConfig {
        binary: binary.to_string_lossy().to_string(),
        ..config
    });
    assert!(adapter.is_available());

    let bench_config = BenchmarkConfig {
        warmup_runs: 1,
        measured_runs: 2,
        prompt: "fixture prompt".to_string(),
        max_tokens: 8,
        timeout_secs: 30,
        min_successes: None,
    };
    let report = benchmark(&adapter, &profile, "fixture-model.gguf", &bench_config).unwrap();
    assert_eq!(report.protocol_version, BENCHMARK_PROTOCOL_VERSION);
    assert_eq!(report.measurements.len(), 2);
    assert_eq!(report.summary.as_ref().unwrap().samples, 2);

    let peak_kb = report
        .measurements
        .iter()
        .filter_map(|m| m.peak_memory_kb)
        .max();
    let manifest = package(PackageInput {
        model: "Fixture/Model-Small".to_string(),
        source_hash: "sha256:fixture".to_string(),
        workload_id: "fixture-smoke-v1".to_string(),
        optimization: OptimizationRecord {
            transform: "fixture_noop".to_string(),
            config: serde_json::json!({}),
        },
        verification: VerificationRecord {
            suite: "fixture-smoke".to_string(),
            critical_pass: true,
            task_success_delta: Some(0.0),
            confidence: Some(1.0),
        },
        device: profile.clone(),
        runtime: crukx_core::deploy::benchmarks::RuntimeRecord {
            kind: RuntimeKind::LlamaCpp,
            name: "llama.cpp-metal".to_string(),
            version: adapter.version(),
        },
        peak_memory_kb: peak_kb,
        throughput_tok_s: None,
        crukx_version: "2.0.0-alpha".to_string(),
    })
    .unwrap();
    assert!(is_releasable(&manifest));

    let out_dir = std::env::temp_dir().join(format!("crukx-slice-out-{}", std::process::id()));
    let evidence_json = serde_json::to_string(&report).unwrap();
    let manifest_json = serde_json::to_string(&manifest).unwrap();
    let exported =
        crukx_runner::exporters::write_artifact_package(&manifest_json, &evidence_json, &out_dir)
            .unwrap();

    let reopened = verify_package(&exported.dir).unwrap();
    assert_eq!(reopened.artifact_hash, manifest.artifact_hash);
    assert_eq!(reopened.model, "Fixture/Model-Small");
    let _ = sha256_hex(&manifest_json);

    std::fs::remove_file(&binary).ok();
    std::fs::remove_dir_all(&out_dir).ok();
}
