use crukx_core::deploy::benchmarks::{
    summarize_with_throughput, BenchmarkConfig, BenchmarkReport, RawMeasurement, RuntimeRecord,
    BENCHMARK_PROTOCOL_VERSION,
};
use crukx_core::deploy::devices::DeviceProfile;
use crukx_core::deploy::runtimes::{ExecutionRequest, RuntimeAdapter, RuntimeError};

pub fn benchmark(
    adapter: &dyn RuntimeAdapter,
    profile: &DeviceProfile,
    model_path: &str,
    config: &BenchmarkConfig,
) -> Result<BenchmarkReport, RuntimeError> {
    config.validate().map_err(RuntimeError::InvalidConfig)?;

    let mut warmup_durations_ms = Vec::new();
    for _ in 0..config.warmup_runs {
        let result = adapter.execute(&ExecutionRequest {
            model_path: model_path.to_string(),
            prompt: config.prompt.clone(),
            max_tokens: config.max_tokens,
            timeout_secs: config.timeout_secs,
        })?;
        warmup_durations_ms.push(result.duration_ms);
    }

    let mut measurements = Vec::new();
    for index in 0..config.measured_runs {
        let result = adapter.execute(&ExecutionRequest {
            model_path: model_path.to_string(),
            prompt: config.prompt.clone(),
            max_tokens: config.max_tokens,
            timeout_secs: config.timeout_secs,
        })?;
        measurements.push(RawMeasurement::from_result(index, &result));
    }

    let durations: Vec<u64> = measurements
        .iter()
        .filter(|m| m.exit_code == 0 && !m.timed_out)
        .map(|m| m.duration_ms)
        .collect();
    if config
        .min_successes
        .is_some_and(|m| durations.len() < m as usize)
    {
        return Err(RuntimeError::ExecutionFailed(format!(
            "only {} of {} runs succeeded (required {})",
            durations.len(),
            measurements.len(),
            config.min_successes.unwrap_or(0)
        )));
    }
    let tok_s_values: Vec<f64> = measurements
        .iter()
        .filter(|m| m.exit_code == 0 && !m.timed_out)
        .filter_map(|m| m.tok_s)
        .collect();
    Ok(BenchmarkReport {
        protocol_version: BENCHMARK_PROTOCOL_VERSION,
        device: profile.clone(),
        runtime: RuntimeRecord {
            kind: adapter.kind(),
            name: adapter.kind().as_str().to_string(),
            version: adapter.version(),
        },
        config: config.clone(),
        warmup_durations_ms,
        measurements,
        summary: summarize_with_throughput(&durations, &tok_s_values),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtimes::LlamaCppAdapter;
    use crukx_core::deploy::devices::DeviceProfile;
    use crukx_core::deploy::runtimes::LlamaCppConfig;

    #[cfg(unix)]
    fn script_adapter(body: &str) -> (LlamaCppAdapter, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "crukx-bench-test-{}-{}-{}.sh",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst),
            body.len()
        ));
        std::fs::write(&path, format!("#!/bin/sh\n{}", body)).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        let adapter = LlamaCppAdapter::new(LlamaCppConfig {
            binary: path.to_string_lossy().to_string(),
            ..LlamaCppConfig::default()
        });
        (adapter, path)
    }

    fn config() -> BenchmarkConfig {
        BenchmarkConfig {
            warmup_runs: 1,
            measured_runs: 3,
            prompt: "hello".to_string(),
            max_tokens: 8,
            timeout_secs: 10,
            min_successes: None,
        }
    }

    #[cfg(unix)]
    #[test]
    fn perf_lines_flow_into_summary_throughput() {
        let (adapter, path) = script_adapter(
            "echo 'eval time =     200.00 ms /    16 runs (   12.50 ms per token,    51.20 tokens per second)' >&2\nexit 0\n",
        );
        let report = benchmark(
            &adapter,
            &DeviceProfile::mock_cpu_only(),
            "model.gguf",
            &BenchmarkConfig {
                warmup_runs: 0,
                measured_runs: 2,
                prompt: "hi".to_string(),
                max_tokens: 8,
                timeout_secs: 10,
                min_successes: None,
            },
        )
        .unwrap();
        assert_eq!(report.measurements.len(), 2);
        assert!(report.measurements.iter().all(|m| m.tok_s == Some(51.2)));
        assert_eq!(report.summary.as_ref().unwrap().tok_s_mean, Some(51.2));
        std::fs::remove_file(&path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn success_gate_rejects_flaky_runs() {
        let (adapter, path) = script_adapter("exit 3\n");
        let gated = BenchmarkConfig {
            min_successes: Some(1),
            ..config()
        };
        let result = benchmark(
            &adapter,
            &DeviceProfile::mock_cpu_only(),
            "model.gguf",
            &gated,
        );
        assert!(matches!(result, Err(RuntimeError::ExecutionFailed(_))));
        let ungated = config();
        assert_eq!(ungated.min_successes, None);
        let report = benchmark(
            &adapter,
            &DeviceProfile::mock_cpu_only(),
            "model.gguf",
            &ungated,
        )
        .unwrap();
        assert_eq!(report.measurements.len(), 3);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn invalid_config_is_rejected() {
        let adapter = LlamaCppAdapter::new(LlamaCppConfig::default());
        let bad = BenchmarkConfig {
            measured_runs: 0,
            ..config()
        };
        assert!(matches!(
            benchmark(
                &adapter,
                &DeviceProfile::mock_cpu_only(),
                "model.gguf",
                &bad
            ),
            Err(RuntimeError::InvalidConfig(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn harness_collects_raw_and_summary() {
        let (adapter, path) = script_adapter("echo tokens output\nexit 0\n");
        let profile = DeviceProfile::mock_cpu_only();
        let report = benchmark(&adapter, &profile, "model.gguf", &config()).unwrap();
        assert_eq!(report.protocol_version, BENCHMARK_PROTOCOL_VERSION);
        assert_eq!(report.device.device_id, profile.device_id);
        assert_eq!(report.warmup_durations_ms.len(), 1);
        assert_eq!(report.measurements.len(), 3);
        assert_eq!(
            report
                .measurements
                .iter()
                .map(|m| m.run_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        let summary = report.summary.clone().unwrap();
        assert_eq!(summary.samples, 3);
        assert!(summary.min_ms <= summary.median_ms);
        assert!(summary.median_ms <= summary.p95_ms);
        assert!(summary.p95_ms <= summary.max_ms);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains(&profile.device_id));
        std::fs::remove_file(&path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn nonzero_exit_runs_recorded_but_excluded_from_summary() {
        let (adapter, path) = script_adapter("exit 3\n");
        let report = benchmark(
            &adapter,
            &DeviceProfile::mock_cpu_only(),
            "model.gguf",
            &config(),
        )
        .unwrap();
        assert_eq!(report.measurements.len(), 3);
        assert_eq!(report.summary, None);
        std::fs::remove_file(&path).ok();
    }
}
