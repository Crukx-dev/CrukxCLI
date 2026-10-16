use crate::contracts::artifact::ArtifactManifest;
use crate::deploy::benchmarks::{BenchmarkReport, RuntimeRecord};
use crate::deploy::devices::DeviceProfile;
use crate::deploy::pareto::{frontier, is_feasible, select, CandidateStat, SelectionConstraints};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CandidateEntry {
    pub stat: CandidateStat,
    pub feasible: bool,
    #[serde(default)]
    pub rejection_reason: Option<String>,
    pub on_frontier: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AlphaReport {
    pub model: String,
    pub source_hash: String,
    pub workload_id: String,
    pub suite: String,
    pub constraints: SelectionConstraints,
    pub candidates: Vec<CandidateEntry>,
    #[serde(default)]
    pub selected_id: Option<String>,
    #[serde(default)]
    pub artifact_hash: Option<String>,
    pub crukx_version: String,
    /// Reproducibility tuple (§12): the exact device, runtime, and benchmark
    /// protocol that produced the deployment evidence. A report without this
    /// is a claim, not evidence.
    pub device: DeviceProfile,
    pub runtime: RuntimeRecord,
    pub protocol_version: u32,
}

pub fn build_report(
    manifest: &ArtifactManifest,
    evidence: &BenchmarkReport,
    candidates: &[CandidateStat],
    constraints: &SelectionConstraints,
) -> AlphaReport {
    let suite = manifest
        .verification
        .as_ref()
        .map(|v| v.suite.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let frontier_set = frontier(candidates);
    let on_frontier = |id: &str| frontier_set.iter().any(|c| c.id == id);
    let entries = candidates
        .iter()
        .map(|stat| {
            let feasible = is_feasible(stat, constraints);
            let rejection_reason = if feasible {
                None
            } else if stat.quality_regression > constraints.max_quality_regression {
                Some(format!(
                    "quality regression {:.4} exceeds cap {:.4}",
                    stat.quality_regression, constraints.max_quality_regression
                ))
            } else if constraints
                .max_memory_mb
                .is_some_and(|max| stat.peak_memory_mb > max)
            {
                Some(format!(
                    "peak memory {} MB exceeds cap {} MB",
                    stat.peak_memory_mb,
                    constraints.max_memory_mb.unwrap_or(0)
                ))
            } else {
                Some("fails throughput floor".to_string())
            };
            CandidateEntry {
                stat: stat.clone(),
                feasible,
                rejection_reason,
                on_frontier: on_frontier(&stat.id),
            }
        })
        .collect();
    AlphaReport {
        model: manifest.model.clone(),
        source_hash: manifest.source_hash.clone(),
        workload_id: manifest.workload_id.clone(),
        suite,
        constraints: constraints.clone(),
        candidates: entries,
        selected_id: select(candidates, constraints).map(|c| c.id.clone()),
        artifact_hash: manifest.artifact_hash.clone(),
        crukx_version: manifest.crukx_version.clone(),
        device: evidence.device.clone(),
        runtime: evidence.runtime.clone(),
        protocol_version: evidence.protocol_version,
    }
}

pub fn render_markdown(report: &AlphaReport, evidence: &BenchmarkReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Crukx alpha report: {}\n\n", report.model));
    out.push_str(&format!("- workload: {}\n", report.workload_id));
    out.push_str(&format!("- suite: {}\n", report.suite));
    out.push_str(&format!("- source: {}\n", report.source_hash));
    out.push_str(&format!(
        "- quality cap: {:.4}\n",
        report.constraints.max_quality_regression
    ));
    out.push_str(&format!(
        "- device: {} (id `{}`)\n",
        crate::deploy::exporters::target_label(&evidence.device),
        evidence.device.device_id
    ));
    out.push_str(&format!(
        "- runtime: {} {}\n",
        evidence.runtime.name,
        evidence.runtime.version.as_deref().unwrap_or("unknown")
    ));
    out.push_str(&format!("- protocol: v{}\n", evidence.protocol_version));
    out.push_str("\n## Candidates\n\n");
    out.push_str("| id | size | tok/s | regression | RAM | frontier | verdict |\n");
    out.push_str("|---|---|---|---|---|---|---|\n");
    for entry in &report.candidates {
        out.push_str(&format!(
            "| {} | {} | {} | {:.4} | {} MB | {} | {} |\n",
            entry.stat.id,
            entry.stat.size_bytes,
            entry
                .stat
                .tok_s
                .map(|t| t.to_string())
                .unwrap_or_else(|| "-".to_string()),
            entry.stat.quality_regression,
            entry.stat.peak_memory_mb,
            if entry.on_frontier { "yes" } else { "no" },
            match (&entry.feasible, &entry.rejection_reason) {
                (true, _) => "feasible".to_string(),
                (false, Some(reason)) => format!("rejected: {}", reason),
                (false, None) => "rejected".to_string(),
            },
        ));
    }
    out.push_str("\n## Selection\n\n");
    match &report.selected_id {
        Some(id) => out.push_str(&format!(
            "Selected `{}`: smallest feasible candidate on the Pareto frontier.\n",
            id
        )),
        None => out.push_str("No candidate satisfied the constraints.\n"),
    }
    out.push_str("\n## Reproducibility\n\n");
    out.push_str("Every result traces to this tuple (§12):\n\n");
    out.push_str(&format!("- source hash: {}\n", report.source_hash));
    out.push_str(&format!("- suite: {}\n", report.suite));
    out.push_str(&format!(
        "- device: {} (id `{}`)\n",
        crate::deploy::exporters::target_label(&report.device),
        report.device.device_id
    ));
    out.push_str(&format!(
        "- runtime: {} {}\n",
        report.runtime.name,
        report.runtime.version.as_deref().unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "- benchmark protocol: v{}\n",
        report.protocol_version
    ));
    out.push_str(&format!("- crukx version: {}\n", report.crukx_version));
    if let Some(hash) = &report.artifact_hash {
        out.push_str(&format!("\nArtifact: `{}`\n", hash));
    }
    out.push_str(&format!(
        "\nReproduce: re-run the benchmark on device `{}` with crukx {} and compare hashes.\n",
        evidence.device.device_id, report.crukx_version
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::devices::DeviceProfile;
    use crate::deploy::exporters::{package, PackageInput};
    use crate::deploy::runtimes::RuntimeKind;

    fn table() -> Vec<CandidateStat> {
        vec![
            CandidateStat {
                id: "Q8".to_string(),
                size_bytes: 4_500_000_000,
                peak_memory_mb: 6000,
                tok_s: Some(31.0),
                quality_regression: 0.0003,
            },
            CandidateStat {
                id: "Q4".to_string(),
                size_bytes: 2_500_000_000,
                peak_memory_mb: 4700,
                tok_s: Some(51.0),
                quality_regression: 0.0019,
            },
            CandidateStat {
                id: "Q3".to_string(),
                size_bytes: 2_000_000_000,
                peak_memory_mb: 4200,
                tok_s: Some(58.0),
                quality_regression: 0.028,
            },
        ]
    }

    fn manifest() -> ArtifactManifest {
        package(PackageInput {
            model: "Qwen/Qwen3-4B".to_string(),
            source_hash: "sha256:abc".to_string(),
            workload_id: "support-agent-v1".to_string(),
            optimization: crate::contracts::artifact::OptimizationRecord {
                transform: "gguf_quantize".to_string(),
                config: serde_json::json!({}),
            },
            verification: crate::contracts::artifact::VerificationRecord {
                suite: "prod-golden-2026-09".to_string(),
                critical_pass: true,
                task_success_delta: Some(0.0),
                confidence: Some(0.95),
            },
            device: DeviceProfile::mock_metal(),
            runtime: crate::deploy::benchmarks::RuntimeRecord {
                kind: RuntimeKind::LlamaCpp,
                name: "llama.cpp-metal".to_string(),
                version: None,
            },
            peak_memory_kb: None,
            throughput_tok_s: None,
            crukx_version: "2.0.0-alpha".to_string(),
        })
        .unwrap()
    }

    fn evidence() -> BenchmarkReport {
        BenchmarkReport {
            protocol_version: 1,
            device: DeviceProfile::mock_metal(),
            runtime: crate::deploy::benchmarks::RuntimeRecord {
                kind: RuntimeKind::LlamaCpp,
                name: "llama.cpp-metal".to_string(),
                version: None,
            },
            config: crate::deploy::benchmarks::BenchmarkConfig {
                warmup_runs: 0,
                measured_runs: 1,
                prompt: "hi".to_string(),
                max_tokens: 8,
                timeout_secs: 5,
                min_successes: None,
            },
            warmup_durations_ms: vec![],
            measurements: vec![],
            summary: None,
        }
    }

    #[test]
    fn report_selects_and_explains_rejections() {
        let constraints = SelectionConstraints {
            max_quality_regression: 0.0025,
            max_memory_mb: None,
            min_tok_s: None,
        };
        let report = build_report(&manifest(), &evidence(), &table(), &constraints);
        assert_eq!(report.selected_id.as_deref(), Some("Q4"));
        assert_eq!(report.suite, "prod-golden-2026-09");
        assert_eq!(report.device.device_id, evidence().device.device_id);
        assert_eq!(report.runtime.name, "llama.cpp-metal");
        assert_eq!(report.protocol_version, 1);
        let q3 = report
            .candidates
            .iter()
            .find(|c| c.stat.id == "Q3")
            .unwrap();
        assert!(!q3.feasible);
        assert!(q3
            .rejection_reason
            .as_deref()
            .unwrap()
            .contains("exceeds cap"));
        let markdown = render_markdown(&report, &evidence());
        assert!(markdown.contains("Selected `Q4`"));
        assert!(markdown.contains("rejected: quality regression"));
        assert!(markdown.contains("Reproduce:"));
        assert!(markdown.contains("## Reproducibility"));
        assert!(markdown.contains("benchmark protocol: v1"));
    }

    #[test]
    fn empty_selection_is_stated() {
        let constraints = SelectionConstraints {
            max_quality_regression: 0.0,
            max_memory_mb: Some(1000),
            min_tok_s: None,
        };
        let report = build_report(&manifest(), &evidence(), &table(), &constraints);
        assert_eq!(report.selected_id, None);
        let markdown = render_markdown(&report, &evidence());
        assert!(markdown.contains("No candidate satisfied"));
    }
}
