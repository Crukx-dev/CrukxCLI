use crate::contracts::replay::ReplayOutcome;
use crate::contracts::schema::RegressionContract;
use crate::diagnostics::stages::{suspect_stage, PipelineStage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceNodeKind {
    Claim,
    ToolCall,
    Validator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceEdgeType {
    DependsOn,
    ValidatedBy,
    Caused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EvidenceNode {
    pub id: String,
    pub kind: EvidenceNodeKind,
    pub label: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EvidenceEdge {
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub edge_type: EvidenceEdgeType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EvidenceGraph {
    pub contract_id: String,
    pub candidate_label: String,
    pub decision: crate::contracts::replay::ReplayDecision,
    pub nodes: Vec<EvidenceNode>,
    pub edges: Vec<EvidenceEdge>,
}

pub fn build_evidence_graph(
    contract: &RegressionContract,
    outcome: &ReplayOutcome,
) -> EvidenceGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let claim_id = format!("claim:{}", outcome.contract_id);
    nodes.push(EvidenceNode {
        id: claim_id.clone(),
        kind: EvidenceNodeKind::Claim,
        label: format!(
            "contract {} holds for \"{}\"",
            outcome.contract_id, outcome.candidate_label
        ),
        data: serde_json::json!({ "source_session_id": contract.source_session_id }),
    });

    for (index, step) in outcome.steps.iter().enumerate() {
        let step_id = format!("step:{}:{}", outcome.contract_id, index);
        nodes.push(EvidenceNode {
            id: step_id.clone(),
            kind: EvidenceNodeKind::ToolCall,
            label: format!("{} {}", step.executable, step.arguments.join(" "))
                .trim()
                .to_string(),
            data: serde_json::json!({
                "expected_exit_code": step.expected_exit_code,
                "actual_exit_code": step.actual_exit_code,
                "matched_expected": step.matched_expected,
            }),
        });
        edges.push(EvidenceEdge {
            from: claim_id.clone(),
            to: step_id.clone(),
            edge_type: EvidenceEdgeType::DependsOn,
        });
        if !step.matched_expected {
            edges.push(EvidenceEdge {
                from: step_id.clone(),
                to: claim_id.clone(),
                edge_type: EvidenceEdgeType::Caused,
            });
        }
    }

    let validator_id = format!("validator:{}", outcome.contract_id);
    nodes.push(EvidenceNode {
        id: validator_id.clone(),
        kind: EvidenceNodeKind::Validator,
        label: contract.assertion_file.clone(),
        data: serde_json::json!({ "pass": outcome.assertion.pass, "reason": outcome.assertion.reason }),
    });
    edges.push(EvidenceEdge {
        from: claim_id.clone(),
        to: validator_id.clone(),
        edge_type: EvidenceEdgeType::ValidatedBy,
    });
    if !outcome.assertion.pass {
        edges.push(EvidenceEdge {
            from: validator_id,
            to: claim_id,
            edge_type: EvidenceEdgeType::Caused,
        });
    }

    EvidenceGraph {
        contract_id: outcome.contract_id.clone(),
        candidate_label: outcome.candidate_label.clone(),
        decision: outcome.decision,
        nodes,
        edges,
    }
}

/// One-line root-cause summary for a BLOCKed graph — what `crukx gate`
/// prints per failing contract.
pub fn summarize_root_cause(outcome: &ReplayOutcome) -> String {
    let suspicion = suspect_stage(outcome);
    if suspicion.stage == PipelineStage::Unknown {
        return "root cause: unknown".to_string();
    }
    let stage_name = match suspicion.stage {
        PipelineStage::ToolExecution => "tool_execution",
        PipelineStage::ResponseValidation => "response_validation",
        PipelineStage::Unknown => unreachable!(),
    };
    format!(
        "root cause: {stage_name} (confidence {:.2})",
        suspicion.confidence
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::replay::{ReplayDecision, StepResult};
    use crate::contracts::schema::{AssertResult, RegressionContract, CONTRACT_SCHEMA_VERSION};

    fn contract() -> RegressionContract {
        RegressionContract {
            schema_version: CONTRACT_SCHEMA_VERSION,
            contract_id: "c1".to_string(),
            created_at: "2026-08-21T00:00:00Z".to_string(),
            source_session_id: "sess-1".to_string(),
            source_event_file: "sess-1.jsonl".to_string(),
            expected_trajectory: vec![],
            assertion_file: "assert.mjs".to_string(),
        }
    }

    #[test]
    fn a_passing_outcome_produces_a_claim_and_validator_node_with_no_caused_edges() {
        let outcome = ReplayOutcome {
            contract_id: "c1".to_string(),
            candidate_label: "ci".to_string(),
            steps: vec![],
            assertion: AssertResult {
                pass: true,
                reason: None,
            },
            decision: ReplayDecision::Pass,
        };
        let graph = build_evidence_graph(&contract(), &outcome);
        assert_eq!(graph.nodes.len(), 2); // claim + validator
        assert!(graph
            .edges
            .iter()
            .all(|e| e.edge_type != EvidenceEdgeType::Caused));
    }

    #[test]
    fn a_diverged_step_adds_a_tool_call_node_and_a_caused_edge_back_to_the_claim() {
        let outcome = ReplayOutcome {
            contract_id: "c1".to_string(),
            candidate_label: "ci".to_string(),
            steps: vec![StepResult {
                executable: "npm".to_string(),
                arguments: vec!["test".to_string()],
                expected_exit_code: 0,
                actual_exit_code: 1,
                matched_expected: false,
                duration_ms: 5,
            }],
            assertion: AssertResult {
                pass: true,
                reason: None,
            },
            decision: ReplayDecision::Block,
        };
        let graph = build_evidence_graph(&contract(), &outcome);
        assert_eq!(graph.nodes.len(), 3); // claim + step + validator
        let caused_edges: Vec<&EvidenceEdge> = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EvidenceEdgeType::Caused)
            .collect();
        assert_eq!(caused_edges.len(), 1);
        assert_eq!(caused_edges[0].to, "claim:c1");
    }

    #[test]
    fn a_failed_assertion_adds_a_caused_edge_from_the_validator() {
        let outcome = ReplayOutcome {
            contract_id: "c1".to_string(),
            candidate_label: "ci".to_string(),
            steps: vec![],
            assertion: AssertResult {
                pass: false,
                reason: Some("refund too high".to_string()),
            },
            decision: ReplayDecision::Block,
        };
        let graph = build_evidence_graph(&contract(), &outcome);
        let caused_edges: Vec<&EvidenceEdge> = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EvidenceEdgeType::Caused)
            .collect();
        assert_eq!(caused_edges.len(), 1);
        assert_eq!(caused_edges[0].from, "validator:c1");
    }

    #[test]
    fn summarize_root_cause_reports_unknown_when_suspect_stage_cannot_localize() {
        let outcome = ReplayOutcome {
            contract_id: "c1".to_string(),
            candidate_label: "ci".to_string(),
            steps: vec![],
            assertion: AssertResult {
                pass: true,
                reason: None,
            },
            decision: ReplayDecision::Block,
        };
        assert_eq!(summarize_root_cause(&outcome), "root cause: unknown");
    }
}
