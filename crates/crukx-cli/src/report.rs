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
    if let Err(e) = std::fs::write(&out, &markdown) {
        eprintln!("report: cannot write {}: {}", out.display(), e);
        return 1;
    }
    println!("report: {}", out.display());
    let script_path = args.artifact.join("reproduce.sh");
    if let Err(e) = std::fs::write(&script_path, reproduce_script(&manifest, &evidence)) {
        eprintln!("report: cannot write {}: {}", script_path.display(), e);
        return 1;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&script_path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            let _ = std::fs::set_permissions(&script_path, permissions);
        }
    }
    println!("reproduce: {}", script_path.display());
    0
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn reproduce_script(
    manifest: &crukx_core::contracts::artifact::ArtifactManifest,
    evidence: &BenchmarkReport,
) -> String {
    let config = &evidence.config;
    format!(
        "#!/bin/sh\n\
         # Reproduce artifact {hash} ({model}).\n\
         # Provide the model file and the verifier-produced record used originally.\n\
         set -e\n\
         DIR=$(cd \"$(dirname \"$0\")\" && pwd)\n\
         : \"${{MODEL:?set MODEL to the model file}}\"\n\
         : \"${{VERIFICATION:?set VERIFICATION to the verifier-produced record}}\"\n\
         crukx benchmark \"$MODEL\" --prompt {prompt} --runs {runs} --warmup {warmup} --max-tokens {tokens} --timeout-secs {timeout} --json > /tmp/crukx-repro-report.json\n\
         crukx compare /tmp/crukx-repro-report.json \"$DIR/evidence.json\"\n",
        hash = manifest.artifact_hash.as_deref().unwrap_or("unknown"),
        model = manifest.model,
        prompt = shell_quote(&config.prompt),
        runs = config.measured_runs,
        warmup = config.warmup_runs,
        tokens = config.max_tokens,
        timeout = config.timeout_secs,
    )
}
