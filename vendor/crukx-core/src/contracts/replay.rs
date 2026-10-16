use crate::contracts::schema::AssertResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StepResult {
    pub executable: String,
    pub arguments: Vec<String>,
    pub expected_exit_code: i32,
    pub actual_exit_code: i32,
    pub matched_expected: bool,
    /// Real wall-clock time this step's spawn took, from
    /// `crukx_runner::SpawnOutcome`. `#[serde(default)]` so contract
    /// fixtures written before this field existed still parse (as 0, which
    /// reads honestly as "not measured" rather than a fabricated value).
    #[serde(default)]
    pub duration_ms: u64,
}

/// Kept as SCREAMING_SNAKE_CASE on the wire — matches how the TS CLI prints
/// it today (`PASS`/`BLOCK` in terminal output and JSON), and it's the one
/// enum a human reads directly in CI logs, so the visual shout is worth
/// keeping through the schema v2 redesign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReplayDecision {
    Pass,
    Block,
}

/// The result of replaying one contract's captured trajectory against the
/// current repo state and running its assertion hook. Deterministic — no
/// candidate model/agent swapping yet, that's still open design work
/// (carried over from the TS implementation's own comment).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReplayOutcome {
    pub contract_id: String,
    pub candidate_label: String,
    pub steps: Vec<StepResult>,
    pub assertion: AssertResult,
    pub decision: ReplayDecision,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::schema::AssertResult;

    #[test]
    fn replay_outcome_round_trips_and_decision_serializes_screaming_snake_case() {
        let outcome = ReplayOutcome {
            contract_id: "c1".to_string(),
            candidate_label: "ci".to_string(),
            steps: vec![StepResult {
                executable: "true".to_string(),
                arguments: vec![],
                expected_exit_code: 0,
                actual_exit_code: 0,
                matched_expected: true,
                duration_ms: 12,
            }],
            assertion: AssertResult {
                pass: true,
                reason: None,
            },
            decision: ReplayDecision::Pass,
        };
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"decision\":\"PASS\""));
        let round_tripped: ReplayOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped.decision, ReplayDecision::Pass);
    }

    #[test]
    fn step_result_without_duration_ms_still_parses_defaulting_to_zero() {
        // A contract captured before this field existed shouldn't refuse
        // to load.
        let json = r#"{"executable":"true","arguments":[],"expected_exit_code":0,"actual_exit_code":0,"matched_expected":true}"#;
        let step: StepResult = serde_json::from_str(json).unwrap();
        assert_eq!(step.duration_ms, 0);
    }
}
