use crukx_core::contracts::artifact::{OptimizationRecord, VerificationRecord};
use crukx_core::deploy::benchmarks::BenchmarkReport;
use crukx_core::deploy::exporters::{is_releasable, package, PackageInput};
use crukx_runner::exporters::{verify_package, write_artifact_package};
use std::path::PathBuf;

pub struct DeployArgs {
    pub report: PathBuf,
    pub model: String,
    pub source_hash: String,
    pub workload: String,
    pub transform: String,
    pub transform_config: Option<String>,
    pub verification: PathBuf,
    pub throughput_tok_s: Option<f64>,
    pub out: PathBuf,
    pub json: bool,
}

fn read_json(path: &PathBuf, what: &str) -> Result<String, i32> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("deploy: cannot read {} ({}): {}", what, path.display(), e);
        2
    })
}

pub fn run_deploy(args: &DeployArgs) -> i32 {
    let report_text = match read_json(&args.report, "benchmark report") {
        Ok(text) => text,
        Err(code) => return code,
    };
    let report: BenchmarkReport = match serde_json::from_str(&report_text) {
        Ok(report) => report,
        Err(e) => {
            eprintln!(
                "deploy: {} is not a benchmark report: {}",
                args.report.display(),
                e
            );
            return 2;
        }
    };
    let verification_text = match read_json(&args.verification, "verification record") {
        Ok(text) => text,
        Err(code) => return code,
    };
    let verification: VerificationRecord = match serde_json::from_str(&verification_text) {
        Ok(record) => record,
        Err(e) => {
            eprintln!(
                "deploy: {} is not a verification record: {}",
                args.verification.display(),
                e
            );
            return 2;
        }
    };
    let transform_config = match &args.transform_config {
        None => serde_json::Value::Object(Default::default()),
        Some(path) => {
            let text = match read_json(&PathBuf::from(path), "transform config") {
                Ok(text) => text,
                Err(code) => return code,
            };
            match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(e) => {
                    eprintln!("deploy: {} is not valid JSON: {}", path, e);
                    return 2;
                }
            }
        }
    };
    let peak_memory_kb = report
        .measurements
        .iter()
        .filter_map(|m| m.peak_memory_kb)
        .max();
    let manifest = match package(PackageInput {
        model: args.model.clone(),
        source_hash: args.source_hash.clone(),
        workload_id: args.workload.clone(),
        optimization: OptimizationRecord {
            transform: args.transform.clone(),
            config: transform_config,
        },
        verification,
        device: report.device.clone(),
        runtime: report.runtime.clone(),
        peak_memory_kb,
        throughput_tok_s: args.throughput_tok_s,
        crukx_version: env!("CARGO_PKG_VERSION").to_string(),
    }) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("deploy: cannot package artifact: {}", e);
            return 2;
        }
    };
    let manifest_json = match serde_json::to_string_pretty(&manifest) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("deploy: cannot render manifest: {}", e);
            return 1;
        }
    };
    let exported = match write_artifact_package(&manifest_json, &report_text, &args.out) {
        Ok(exported) => exported,
        Err(e) => {
            eprintln!("deploy: {}", e);
            return 1;
        }
    };
    match verify_package(&exported.dir) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("deploy: packaged artifact failed verification: {}", e);
            return 1;
        }
    }
    let releasable = is_releasable(&manifest);
    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "manifest": exported.manifest_path,
                "evidence": exported.evidence_path,
                "artifact_hash": manifest.artifact_hash,
                "releasable": releasable,
            })
        );
    } else {
        println!("manifest: {}", exported.manifest_path.display());
        println!("evidence: {}", exported.evidence_path.display());
        println!(
            "verdict:  {}",
            if releasable {
                "RELEASABLE"
            } else {
                "NOT RELEASABLE (verification did not pass)"
            }
        );
    }
    if releasable {
        0
    } else {
        1
    }
}
