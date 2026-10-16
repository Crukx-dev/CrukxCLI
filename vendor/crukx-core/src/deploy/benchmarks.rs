use crate::deploy::devices::DeviceProfile;
use crate::deploy::runtimes::{ExecutionResult, RuntimeKind};
use serde::{Deserialize, Serialize};

pub const BENCHMARK_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct BenchmarkConfig {
    pub warmup_runs: u32,
    pub measured_runs: u32,
    pub prompt: String,
    pub max_tokens: u32,
    pub timeout_secs: u64,
    #[serde(default)]
    pub min_successes: Option<u32>,
}

impl BenchmarkConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.measured_runs == 0 {
            return Err("measured_runs must be greater than zero".to_string());
        }
        if self.prompt.is_empty() {
            return Err("prompt must not be empty".to_string());
        }
        if self.max_tokens == 0 {
            return Err("max_tokens must be greater than zero".to_string());
        }
        if self.timeout_secs == 0 {
            return Err("timeout_secs must be greater than zero".to_string());
        }
        if self.min_successes.is_some_and(|m| m == 0) {
            return Err("min_successes must be greater than zero when set".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RawMeasurement {
    pub run_index: u32,
    pub duration_ms: u64,
    pub exit_code: i32,
    pub timed_out: bool,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    #[serde(default)]
    pub peak_memory_kb: Option<u64>,
    #[serde(default)]
    pub tok_s: Option<f64>,
}

impl RawMeasurement {
    pub fn from_result(run_index: u32, result: &ExecutionResult) -> Self {
        Self {
            run_index,
            duration_ms: result.duration_ms,
            exit_code: result.exit_code,
            timed_out: result.timed_out,
            stdout_bytes: result.stdout.len() as u64,
            stderr_bytes: result.stderr.len() as u64,
            peak_memory_kb: result.peak_memory_kb,
            tok_s: result.tok_s,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct BenchmarkSummary {
    pub samples: u32,
    pub min_ms: u64,
    pub max_ms: u64,
    pub mean_ms: f64,
    pub median_ms: u64,
    pub p95_ms: u64,
    #[serde(default)]
    pub tok_s_mean: Option<f64>,
}

pub fn summarize(durations_ms: &[u64]) -> Option<BenchmarkSummary> {
    summarize_with_throughput(durations_ms, &[])
}

pub fn summarize_with_throughput(
    durations_ms: &[u64],
    tok_s_values: &[f64],
) -> Option<BenchmarkSummary> {
    if durations_ms.is_empty() {
        return None;
    }
    let mut sorted = durations_ms.to_vec();
    sorted.sort_unstable();
    let samples = sorted.len() as u32;
    let sum: u128 = sorted.iter().map(|&d| d as u128).sum();
    let measured: Vec<f64> = tok_s_values
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v > 0.0)
        .collect();
    Some(BenchmarkSummary {
        samples,
        min_ms: sorted[0],
        max_ms: sorted[sorted.len() - 1],
        mean_ms: sum as f64 / samples as f64,
        median_ms: percentile(&sorted, 50),
        p95_ms: percentile(&sorted, 95),
        tok_s_mean: if measured.is_empty() {
            None
        } else {
            Some(measured.iter().sum::<f64>() / measured.len() as f64)
        },
    })
}

fn percentile(sorted: &[u64], pct: u32) -> u64 {
    let rank = (pct as f64 / 100.0 * sorted.len() as f64).ceil() as usize;
    sorted[rank.max(1).min(sorted.len()) - 1]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeRecord {
    pub kind: RuntimeKind,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct BenchmarkReport {
    pub protocol_version: u32,
    pub device: DeviceProfile,
    pub runtime: RuntimeRecord,
    pub config: BenchmarkConfig,
    #[serde(default)]
    pub warmup_durations_ms: Vec<u64>,
    #[serde(default)]
    pub measurements: Vec<RawMeasurement>,
    #[serde(default)]
    pub summary: Option<BenchmarkSummary>,
}

impl BenchmarkReport {
    pub fn completed_durations(&self) -> Vec<u64> {
        self.measurements
            .iter()
            .filter(|m| !m.timed_out)
            .map(|m| m.duration_ms)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_validation() {
        let good = BenchmarkConfig {
            warmup_runs: 1,
            measured_runs: 5,
            prompt: "hi".to_string(),
            max_tokens: 32,
            timeout_secs: 60,
            min_successes: None,
        };
        assert!(good.validate().is_ok());
        let bad = BenchmarkConfig {
            measured_runs: 0,
            ..good.clone()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn summarize_known_values() {
        let summary = summarize(&[10, 20, 30, 40, 50]).unwrap();
        assert_eq!(summary.samples, 5);
        assert_eq!(summary.min_ms, 10);
        assert_eq!(summary.max_ms, 50);
        assert_eq!(summary.mean_ms, 30.0);
        assert_eq!(summary.median_ms, 30);
        assert_eq!(summary.p95_ms, 50);
    }

    #[test]
    fn summarize_single_sample() {
        let summary = summarize(&[42]).unwrap();
        assert_eq!(summary.min_ms, 42);
        assert_eq!(summary.max_ms, 42);
        assert_eq!(summary.median_ms, 42);
        assert_eq!(summary.p95_ms, 42);
    }

    #[test]
    fn summarize_empty_is_none() {
        assert_eq!(summarize(&[]), None);
    }

    #[test]
    fn throughput_mean_ignores_missing_and_invalid() {
        let summary = summarize_with_throughput(&[100, 200], &[51.2, f64::NAN, -3.0]).unwrap();
        assert_eq!(summary.tok_s_mean, Some(51.2));
        let absent = summarize(&[100, 200]).unwrap();
        assert_eq!(absent.tok_s_mean, None);
    }

    #[test]
    fn completed_durations_skips_timeouts() {
        let mut report = BenchmarkReport {
            protocol_version: BENCHMARK_PROTOCOL_VERSION,
            device: DeviceProfile::mock_cpu_only(),
            runtime: RuntimeRecord {
                kind: RuntimeKind::LlamaCpp,
                name: "llama-cli".to_string(),
                version: None,
            },
            config: BenchmarkConfig {
                warmup_runs: 0,
                measured_runs: 2,
                prompt: "hi".to_string(),
                max_tokens: 8,
                timeout_secs: 5,
                min_successes: None,
            },
            warmup_durations_ms: vec![],
            measurements: vec![
                RawMeasurement {
                    run_index: 0,
                    duration_ms: 100,
                    exit_code: 0,
                    timed_out: false,
                    stdout_bytes: 10,
                    stderr_bytes: 0,
                    peak_memory_kb: None,
                    tok_s: Some(50.0),
                },
                RawMeasurement {
                    run_index: 1,
                    duration_ms: 5000,
                    exit_code: 1,
                    timed_out: true,
                    stdout_bytes: 0,
                    stderr_bytes: 0,
                    peak_memory_kb: None,
                    tok_s: None,
                },
            ],
            summary: None,
        };
        assert_eq!(report.completed_durations(), vec![100]);
        report.summary = summarize(&report.completed_durations());
        assert_eq!(report.summary.unwrap().samples, 1);
    }

    #[test]
    fn report_round_trips() {
        let report = BenchmarkReport {
            protocol_version: BENCHMARK_PROTOCOL_VERSION,
            device: DeviceProfile::mock_metal(),
            runtime: RuntimeRecord {
                kind: RuntimeKind::LlamaCpp,
                name: "llama-cli".to_string(),
                version: Some("1234".to_string()),
            },
            config: BenchmarkConfig {
                warmup_runs: 1,
                measured_runs: 3,
                prompt: "hi".to_string(),
                max_tokens: 8,
                timeout_secs: 30,
                min_successes: None,
            },
            warmup_durations_ms: vec![120],
            measurements: vec![],
            summary: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: BenchmarkReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back, report);
    }
}
