use crukx_core::deploy::benchmarks::BenchmarkReport;
use crukx_core::deploy::pareto::{CandidateStat, SelectionConstraints};
use crukx_core::deploy::report::{build_report, render_markdown};
use crukx_runner::exporters::verify_package;
use std::path::PathBuf;

pub struct ReportArgs {
    pub artifact: PathBuf,
    pub candidates: PathBuf,
    pub max_regression: f64,
    pub max_memory_mb: Option<u64>,
    pub min_tok_s: Option<f64>,
    pub out: Option<PathBuf>,
}

pub fn run_report(args: &ReportArgs) -> i32 {
    let manifest = match verify_package(&args.artifact) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("report: artifact does not verify: {}", e);
            return 1;
        }
    };
    let candidates_text = match std::fs::read_to_string(&args.candidates) {
        Ok(text) => text,
        Err(e) => {
            eprintln!(
                "report: cannot read candidates ({}): {}",
                args.candidates.display(),
                e
            );
            return 2;
        }
    };
    let candidates: Vec<CandidateStat> = match serde_json::from_str(&candidates_text) {
        Ok(candidates) => candidates,
        Err(e) => {
            eprintln!(
                "report: {} is not a candidate list: {}",
                args.candidates.display(),
                e
            );
            return 2;
        }
    };
    let evidence_text = match std::fs::read_to_string(args.artifact.join("evidence.json")) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("report: cannot read evidence: {}", e);
            return 1;
        }
    };
    let evidence: BenchmarkReport = match serde_json::from_str(&evidence_text) {
        Ok(evidence) => evidence,
        Err(e) => {
            eprintln!("report: evidence is not a benchmark report: {}", e);
            return 1;
        }
    };
    let suite = manifest
        .verification
        .as_ref()
        .map(|v| v.suite.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let report = build_report(
        &manifest,
        &suite,
        &candidates,
        &SelectionConstraints {
            max_quality_regression: args.max_regression,
            max_memory_mb: args.max_memory_mb,
            min_tok_s: args.min_tok_s,
        },
    );
    let markdown = render_markdown(&report, &evidence);
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| args.artifact.join("REPORT.md"));
    match std::fs::write(&out, &markdown) {
        Ok(_) => {
            println!("report: {}", out.display());
            0
        }
        Err(e) => {
            eprintln!("report: cannot write {}: {}", out.display(), e);
            1
        }
    }
}
