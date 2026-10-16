use crate::contracts::replay::{ReplayDecision, ReplayOutcome};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    ToolExecution,
    ResponseValidation,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StageSuspicion {
    pub stage: PipelineStage,
    /// 0-1, how sure we are this is where the failure actually happened.
    pub confidence: f64,
}

/// Localizes one BLOCKed outcome to a pipeline stage. No ML, no scoring
/// model — a diverged shell step is definitely a tool_execution failure (we
/// watched the exit code disagree), a failed assertion with no diverged
/// steps is a response_validation failure. If neither fired, the outcome
/// shouldn't have BLOCKed — flagged unknown so that case is visible instead
/// of silently miscounted.
pub fn suspect_stage(outcome: &ReplayOutcome) -> StageSuspicion {
    let diverged: Vec<&crate::contracts::replay::StepResult> = outcome
        .steps
        .iter()
        .filter(|step| !step.matched_expected)
        .collect();
    if !diverged.is_empty() {
        return StageSuspicion {
            stage: PipelineStage::ToolExecution,
            confidence: diverged.len() as f64 / outcome.steps.len() as f64,
        };
    }
    if !outcome.assertion.pass {
        return StageSuspicion {
            stage: PipelineStage::ResponseValidation,
            confidence: 1.0,
        };
    }
    StageSuspicion {
        stage: PipelineStage::Unknown,
        confidence: 0.0,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StageSuspicionEntry {
    pub contract_id: String,
    pub suspicion: StageSuspicion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DiagnosticCoverageResult {
    /// Fraction of BLOCKed outcomes localized to a known stage (not "unknown").
    pub coverage: f64,
    pub localized: usize,
    pub total_blocked: usize,
    pub per_outcome: Vec<StageSuspicionEntry>,
}

/// Diagnostic Coverage Rate (D²ACCI-inspired): can Crukx say *why*, not
/// just *that*, it failed.
pub fn diagnostic_coverage(outcomes: &[ReplayOutcome]) -> DiagnosticCoverageResult {
    let blocked: Vec<&ReplayOutcome> = outcomes
        .iter()
        .filter(|outcome| outcome.decision == ReplayDecision::Block)
        .collect();
    let per_outcome: Vec<StageSuspicionEntry> = blocked
        .iter()
        .map(|outcome| StageSuspicionEntry {
            contract_id: outcome.contract_id.clone(),
            suspicion: suspect_stage(outcome),
        })
        .collect();
    let localized = per_outcome
        .iter()
        .filter(|entry| entry.suspicion.stage != PipelineStage::Unknown)
        .count();

    DiagnosticCoverageResult {
        coverage: if blocked.is_empty() {
            1.0
        } else {
            localized as f64 / blocked.len() as f64
        },
        localized,
        total_blocked: blocked.len(),
        per_outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::replay::{ReplayDecision, StepResult};
    use crate::contracts::schema::AssertResult;

    fn outcome(
        decision: ReplayDecision,
        steps: Vec<StepResult>,
        assertion_pass: bool,
    ) -> ReplayOutcome {
        ReplayOutcome {
            contract_id: "c".to_string(),
            candidate_label: "ci".to_string(),
            steps,
            assertion: AssertResult {
                pass: assertion_pass,
                reason: None,
            },
            decision,
        }
    }

    #[test]
    fn suspect_stage_localizes_a_diverged_step_to_tool_execution() {
        let suspicion = suspect_stage(&outcome(
            ReplayDecision::Block,
            vec![StepResult {
                executable: "npm".to_string(),
                arguments: vec!["test".to_string()],
                expected_exit_code: 0,
                actual_exit_code: 1,
                matched_expected: false,
                duration_ms: 5,
            }],
            true,
        ));
        assert_eq!(suspicion.stage, PipelineStage::ToolExecution);
        assert_eq!(suspicion.confidence, 1.0);
    }

    #[test]
    fn suspect_stage_localizes_a_failed_assertion_with_no_divergence_to_response_validation() {
        let suspicion = suspect_stage(&outcome(ReplayDecision::Block, vec![], false));
        assert_eq!(suspicion.stage, PipelineStage::ResponseValidation);
    }

    #[test]
    fn suspect_stage_returns_unknown_when_nothing_explains_the_block() {
        let suspicion = suspect_stage(&outcome(ReplayDecision::Block, vec![], true));
        assert_eq!(suspicion.stage, PipelineStage::Unknown);
    }

    #[test]
    fn diagnostic_coverage_is_100_percent_localized_when_every_block_has_an_explanation() {
        let outcomes = vec![
            outcome(ReplayDecision::Pass, vec![], true),
            outcome(ReplayDecision::Block, vec![], false),
        ];
        let result = diagnostic_coverage(&outcomes);
        assert_eq!(result.total_blocked, 1);
        assert_eq!(result.coverage, 1.0);
    }

    #[test]
    fn diagnostic_coverage_defaults_to_100_percent_when_there_are_no_blocks() {
        let outcomes = vec![outcome(ReplayDecision::Pass, vec![], true)];
        let result = diagnostic_coverage(&outcomes);
        assert_eq!(result.coverage, 1.0);
        assert_eq!(result.total_blocked, 0);
    }
}
