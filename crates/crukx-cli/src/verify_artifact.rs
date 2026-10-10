use crukx_core::deploy::benchmarks::BenchmarkReport;
use crukx_runner::exporters::verify_package;
use std::path::PathBuf;

pub struct VerifyArtifactArgs {
    pub dir: PathBuf,
}

pub fn run_verify_artifact(args: &VerifyArtifactArgs) -> i32 {
    if !args.dir.is_dir() {
        eprintln!(
            "verify-artifact: not a directory: {}",
            args.dir.display()
        );
        return 2;
    }
    let manifest = match verify_package(&args.dir) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("verify-artifact: TAMPERED ({})", e);
            return 1;
        }
    };
    let evidence_text =
        match std::fs::read_to_string(args.dir.join("evidence.json")) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("verify-artifact: cannot read evidence: {}", e);
                return 1;
            }
        };
    let evidence_status = match serde_json::from_str::<BenchmarkReport>(&evidence_text) {
        Ok(report) => format!(
            "benchmark evidence: protocol {} · {} measured runs",
            report.protocol_version,
            report.measurements.len()
        ),
        Err(_) => "benchmark evidence: present but not a benchmark report".to_string(),
    };
    let deployment = manifest.deployment.as_ref();
    println!("artifact: {}", manifest.artifact_hash.as_deref().unwrap_or("unknown"));
    println!("model:    {}", manifest.model);
    println!(
        "target:   {}",
        deployment.map(|d| d.target.as_str()).unwrap_or("unknown")
    );
    println!(
        "runtime:  {}",
        deployment.map(|d| d.runtime.as_str()).unwrap_or("unknown")
    );
    println!("verdict:  {}", evidence_status);
    println!(
        "release:  {}",
        if manifest.is_verified() {
            "RELEASABLE"
        } else {
            "NOT RELEASABLE (verification did not pass)"
        }
    );
    0
}
