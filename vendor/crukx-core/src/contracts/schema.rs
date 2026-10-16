use crate::events::{ActionPhase, AgentAction};
use serde::{Deserialize, Serialize};

pub const CONTRACT_SCHEMA_VERSION: u32 = 1;

/// One step of the trajectory Crukx expects a replay to reproduce. Derived
/// from a captured session's proposed/completed actions — not hand-written,
/// so it stays honest about what actually happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExpectedStep {
    pub phase: ActionPhase,
    pub action: AgentAction,
}

/// A regression contract: the trajectory Crukx captured from a real
/// session, plus a pointer to the assertion file that decides whether a
/// replay's resulting backend state is acceptable. Crukx does not generate
/// assertion *logic* — generic backend-state verification can't be
/// automatic across arbitrary customer stacks. It generates the stub; the
/// user fills in what "correct end state" means for their system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegressionContract {
    pub schema_version: u32,
    pub contract_id: String,
    pub created_at: String,
    pub source_session_id: String,
    pub source_event_file: String,
    pub expected_trajectory: Vec<ExpectedStep>,
    pub assertion_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AssertContext {
    pub contract_id: String,
    pub source_session_id: String,
    pub candidate_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AssertResult {
    pub pass: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{ActionPhase, AgentAction};

    #[test]
    fn regression_contract_round_trips_with_one_expected_step() {
        let contract = RegressionContract {
            schema_version: CONTRACT_SCHEMA_VERSION,
            contract_id: "c1".to_string(),
            created_at: "2026-08-21T00:00:00Z".to_string(),
            source_session_id: "sess-1".to_string(),
            source_event_file: "sess-1.jsonl".to_string(),
            expected_trajectory: vec![ExpectedStep {
                phase: ActionPhase::Completed,
                action: AgentAction::ShellCommand {
                    executable: "true".to_string(),
                    arguments: vec![],
                    cwd: ".".to_string(),
                    exit_code: Some(0),
                    signal: None,
                },
            }],
            assertion_file: "assert.mjs".to_string(),
        };

        let json = serde_json::to_string(&contract).unwrap();
        let round_tripped: RegressionContract = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped.contract_id, "c1");
        assert_eq!(round_tripped.expected_trajectory.len(), 1);
    }

    #[test]
    fn assert_result_reason_is_omitted_when_none() {
        let result = AssertResult {
            pass: true,
            reason: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert_eq!(json, r#"{"pass":true}"#);
    }
}
