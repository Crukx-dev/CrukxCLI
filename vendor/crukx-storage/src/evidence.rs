use crate::json::{write_json, StorageError};
use crate::state_dir::StateDir;
use crukx_core::evidence::EvidenceGraph;
use crukx_core::security::is_safe_path_segment;
use std::io;

pub fn write_evidence_graph(
    state: &StateDir,
    contract_id: &str,
    graph: &EvidenceGraph,
) -> Result<(), StorageError> {
    if !is_safe_path_segment(contract_id) {
        return Err(StorageError::from(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("contract id {contract_id:?} is not a safe path segment"),
        )));
    }
    let path = state.evidence_dir().join(format!("{contract_id}.json"));
    write_json(&path, graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crukx_core::contracts::replay::{ReplayDecision, ReplayOutcome};
    use crukx_core::contracts::schema::{
        AssertResult, RegressionContract, CONTRACT_SCHEMA_VERSION,
    };
    use crukx_core::evidence::build_evidence_graph;

    #[test]
    fn write_evidence_graph_writes_to_evidence_dir_named_after_the_contract_id() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());

        let contract = RegressionContract {
            schema_version: CONTRACT_SCHEMA_VERSION,
            contract_id: "demo-1".to_string(),
            created_at: "2026-08-22T00:00:00Z".to_string(),
            source_session_id: "sess-1".to_string(),
            source_event_file: "sess-1.jsonl".to_string(),
            expected_trajectory: vec![],
            assertion_file: "assert.mjs".to_string(),
        };
        let outcome = ReplayOutcome {
            contract_id: "demo-1".to_string(),
            candidate_label: "ci".to_string(),
            steps: vec![],
            assertion: AssertResult {
                pass: false,
                reason: Some("refund too high".to_string()),
            },
            decision: ReplayDecision::Block,
        };
        let graph = build_evidence_graph(&contract, &outcome);

        write_evidence_graph(&state, "demo-1", &graph).unwrap();

        let written_path = state.evidence_dir().join("demo-1.json");
        assert!(written_path.exists());
        let raw = std::fs::read_to_string(written_path).unwrap();
        assert!(raw.contains("\"contract_id\": \"demo-1\""));
    }

    #[test]
    fn write_evidence_graph_refuses_a_traversal_contract_id() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        let contract = RegressionContract {
            schema_version: CONTRACT_SCHEMA_VERSION,
            contract_id: "demo-1".to_string(),
            created_at: "2026-08-22T00:00:00Z".to_string(),
            source_session_id: "sess-1".to_string(),
            source_event_file: "sess-1.jsonl".to_string(),
            expected_trajectory: vec![],
            assertion_file: "assert.mjs".to_string(),
        };
        let outcome = ReplayOutcome {
            contract_id: "demo-1".to_string(),
            candidate_label: "ci".to_string(),
            steps: vec![],
            assertion: AssertResult {
                pass: false,
                reason: None,
            },
            decision: ReplayDecision::Block,
        };
        let graph = build_evidence_graph(&contract, &outcome);

        assert!(write_evidence_graph(&state, "../../escaped", &graph).is_err());
    }
}
