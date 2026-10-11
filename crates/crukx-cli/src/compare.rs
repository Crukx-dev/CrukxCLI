use crukx_core::deploy::benchmarks::BenchmarkReport;
use serde::Serialize;
use std::path::PathBuf;

pub struct CompareArgs {
    pub a: PathBuf,
    pub b: PathBuf,
    pub json: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Comparison {
    pub median_delta_pct: Option<f64>,
    pub p95_delta_pct: Option<f64>,
    pub mean_delta_pct: Option<f64>,
    pub a_runs: usize,
    pub b_runs: usize,
    pub same_device: bool,
    pub same_runtime: bool,
    pub same_runtime_version: bool,
}

fn delta_pct(a: u64, b: u64) -> Option<f64> {
    if a == 0 {
        return None;
    }
    Some((b as f64 - a as f64) / a as f64 * 100.0)
}

pub fn diff_reports(a: &BenchmarkReport, b: &BenchmarkReport) -> Comparison {
    let (median_delta_pct, p95_delta_pct, mean_delta_pct) = match (&a.summary, &b.summary) {
        (Some(x), Some(y)) => (
            delta_pct(x.median_ms, y.median_ms),
            delta_pct(x.p95_ms, y.p95_ms),
            if x.mean_ms == 0.0 {
                None
            } else {
                Some((y.mean_ms - x.mean_ms) / x.mean_ms * 100.0)
            },
        ),
        _ => (None, None, None),
    };
    Comparison {
        median_delta_pct,
        p95_delta_pct,
        mean_delta_pct,
        a_runs: a.measurements.len(),
        b_runs: b.measurements.len(),
        same_device: a.device.device_id == b.device.device_id,
        same_runtime: a.runtime.kind == b.runtime.kind && a.runtime.name == b.runtime.name,
        same_runtime_version: a.runtime.version == b.runtime.version,
    }
}

fn load_report(path: &PathBuf) -> Result<BenchmarkReport, i32> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("compare: cannot read {}: {}", path.display(), e);
        2
    })?;
    serde_json::from_str(&text).map_err(|e| {
        eprintln!(
            "compare: {} is not a benchmark report: {}",
            path.display(),
            e
        );
        2
    })
}

fn render_delta(label: &str, delta: Option<f64>) -> String {
    match delta {
        Some(d) => format!("{}: {:+.1}%", label, d),
        None => format!("{}: n/a", label),
    }
}

pub fn run_compare(args: &CompareArgs) -> i32 {
    let a = match load_report(&args.a) {
        Ok(report) => report,
        Err(code) => return code,
    };
    let b = match load_report(&args.b) {
        Ok(report) => report,
        Err(code) => return code,
    };
    let comparison = diff_reports(&a, &b);
    if args.json {
        match serde_json::to_string_pretty(&comparison) {
            Ok(json) => println!("{}", json),
            Err(e) => {
                eprintln!("compare: cannot render comparison: {}", e);
                return 1;
            }
        }
        return 0;
    }
    println!("A: {} runs · {}", comparison.a_runs, args.a.display());
    println!("B: {} runs · {}", comparison.b_runs, args.b.display());
    println!(
        "device:  {}",
        if comparison.same_device {
            "same"
        } else {
            "DIFFERENT — latency deltas are not comparable"
        }
    );
    println!(
        "runtime: {}{}",
        if comparison.same_runtime {
            "same"
        } else {
            "DIFFERENT"
        },
        if comparison.same_runtime_version {
            ""
        } else {
            " (version differs)"
        }
    );
    println!("{}", render_delta("median", comparison.median_delta_pct));
    println!("{}", render_delta("p95   ", comparison.p95_delta_pct));
    println!("{}", render_delta("mean  ", comparison.mean_delta_pct));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crukx_core::deploy::benchmarks::{BenchmarkConfig, BenchmarkSummary, RuntimeRecord};
    use crukx_core::deploy::devices::DeviceProfile;
    use crukx_core::deploy::runtimes::RuntimeKind;

    fn report(median: u64, p95: u64, mean: f64) -> BenchmarkReport {
        BenchmarkReport {
            protocol_version: 1,
            device: DeviceProfile::mock_cpu_only(),
            runtime: RuntimeRecord {
                kind: RuntimeKind::LlamaCpp,
                name: "llama.cpp".to_string(),
                version: Some("1".to_string()),
            },
            config: BenchmarkConfig {
                warmup_runs: 0,
                measured_runs: 1,
                prompt: "hi".to_string(),
                max_tokens: 8,
                timeout_secs: 5,
            },
            warmup_durations_ms: vec![],
            measurements: vec![],
            summary: Some(BenchmarkSummary {
                samples: 5,
                min_ms: 10,
                max_ms: 100,
                mean_ms: mean,
                median_ms: median,
                p95_ms: p95,
            }),
        }
    }

    #[test]
    fn deltas_match_hand_computation() {
        let comparison = diff_reports(&report(100, 200, 120.0), &report(110, 180, 132.0));
        assert_eq!(comparison.median_delta_pct, Some(10.0));
        assert_eq!(comparison.p95_delta_pct, Some(-10.0));
        assert!((comparison.mean_delta_pct.unwrap() - 10.0).abs() < 1e-9);
        assert!(comparison.same_device);
        assert!(comparison.same_runtime_version);
    }

    #[test]
    fn missing_summaries_yield_no_deltas() {
        let mut a = report(100, 200, 120.0);
        a.summary = None;
        let comparison = diff_reports(&a, &report(110, 180, 132.0));
        assert_eq!(comparison.median_delta_pct, None);
    }

    #[test]
    fn device_change_is_flagged() {
        let mut b = report(100, 200, 120.0);
        b.device = DeviceProfile::mock_metal();
        let comparison = diff_reports(&report(100, 200, 120.0), &b);
        assert!(!comparison.same_device);
        assert!(comparison.same_runtime);
    }
}
