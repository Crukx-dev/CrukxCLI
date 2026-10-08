use crukx_core::deploy::benchmarks::BenchmarkConfig;
use crukx_runner::benchmarks::benchmark;
use crukx_runner::devices::detect;
use crukx_runner::selection::resolve_adapter;
use std::path::PathBuf;

pub struct BenchmarkArgs {
    pub model: PathBuf,
    pub prompt: String,
    pub runs: u32,
    pub warmup: u32,
    pub max_tokens: u32,
    pub timeout_secs: u64,
    pub json: bool,
}

pub fn run_benchmark(args: &BenchmarkArgs) -> i32 {
    if !args.model.is_file() {
        eprintln!(
            "benchmark: model file not found: {}",
            args.model.display()
        );
        return 2;
    }
    if args.runs == 0 {
        eprintln!("benchmark: --runs must be greater than zero");
        return 2;
    }
    let profile = detect();
    let adapter = match resolve_adapter(&profile) {
        Ok(adapter) => adapter,
        Err(e) => {
            eprintln!("benchmark: no usable runtime: {}", e);
            return 1;
        }
    };
    let config = BenchmarkConfig {
        warmup_runs: args.warmup,
        measured_runs: args.runs,
        prompt: args.prompt.clone(),
        max_tokens: args.max_tokens,
        timeout_secs: args.timeout_secs,
    };
    let report = match benchmark(
        &adapter,
        &profile,
        &args.model.to_string_lossy(),
        &config,
    ) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("benchmark: run failed: {}", e);
            return 1;
        }
    };
    if args.json {
        match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{}", json),
            Err(e) => {
                eprintln!("benchmark: cannot render report: {}", e);
                return 1;
            }
        }
        return 0;
    }
    let device_label = crukx_core::deploy::exporters::target_label(&profile);
    println!("device:  {}", device_label);
    println!(
        "runtime: {} {}",
        report.runtime.name,
        report.runtime.version.as_deref().unwrap_or("unknown")
    );
    println!(
        "runs:    {} measured + {} warmup",
        report.measurements.len(),
        report.warmup_durations_ms.len()
    );
    match &report.summary {
        Some(summary) => {
            println!(
                "latency: min {}ms · median {}ms · p95 {}ms · max {}ms",
                summary.min_ms, summary.median_ms, summary.p95_ms, summary.max_ms
            );
            let peak = report
                .measurements
                .iter()
                .filter_map(|m| m.peak_memory_kb)
                .max();
            match peak {
                Some(kb) => println!("peak:    {} MB", kb / 1024),
                None => println!("peak:    unknown"),
            }
            0
        }
        None => {
            eprintln!("benchmark: no successful runs, nothing to summarize");
            1
        }
    }
}
