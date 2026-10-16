use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    #[default]
    Block,
    Warn,
}

fn default_candidate_label() -> String {
    "ci".to_string()
}

fn default_repeat() -> usize {
    1
}

/// Opts a contract into cost-aware escalation (`crukx-core::escalation`)
/// when it BLOCKs. `p_failure` is this contract's own real historical
/// BLOCK rate from `.crukx/runs.jsonl` — not requested here, `crukx gate`
/// fills it in. Only `retry_cost` and `risk_value` are configured — both
/// required together, in whatever unit the team uses consistently (USD,
/// engineer-minutes, anything comparable across contracts).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EscalationConfig {
    pub retry_cost: f64,
    pub risk_value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_threshold: Option<f64>,
    /// How many past runs of this contract to fold into `p_failure`.
    /// Defaults to 20 — long enough to smooth one-off flakiness, short
    /// enough to react to a contract that's actually degraded recently.
    #[serde(default = "default_history_window")]
    pub history_window: usize,
}

fn default_history_window() -> usize {
    20
}

/// One entry in `crukx.yml`'s `contracts:` list — which regression
/// contract gates a release, and how. `mode` defaults to `block`,
/// `candidate_label` defaults to `"ci"`, `repeat` defaults to `1` — all
/// optional in the YAML, matching the TS CLI's `load-policy.ts` (only
/// `id` is required). `repeat > 1` gates on pass^k (arxiv 2601.06112):
/// this contract must PASS on *every* one of `repeat` real replays, not
/// just once, before the gate accepts it — flaky-prone contracts opt into
/// more repeats individually rather than every contract paying the cost.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PolicyContractEntry {
    pub id: String,
    #[serde(default)]
    pub mode: PolicyMode,
    #[serde(default = "default_candidate_label")]
    pub candidate_label: String,
    #[serde(default = "default_repeat")]
    pub repeat: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escalation: Option<EscalationConfig>,
}

/// Optional reliability contract: hard constraints on the aggregate gate
/// run, not just per-contract PASS/BLOCK. CAPO-inspired — a constraint
/// that fails BLOCKs the release regardless of how good the average looks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PolicyConstraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vtr_minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_coverage_minimum: Option<f64>,
    /// Max allowed drop in judge/human agreement before gating on it is refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge_max_agreement_drop: Option<f64>,
    /// Max allowed net new critical security findings. Default 0 when checked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_critical_delta_max: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_high_delta_max: Option<i64>,
    /// Max allowed P95 wall-clock duration (ms) across every replayed
    /// step. `None` when there are no steps to measure never violates
    /// this — see `p95_step_duration_ms`'s doc comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p95_latency_max_ms: Option<u64>,
    /// Honest worst-case pass^k floor (0.0–1.0): every repeated contract's
    /// Wilson-interval LOWER bound must clear this, not just its point
    /// estimate. A contract passing 3/5 runs has a 95% lower bound near
    /// 0.23 — an SLA of 0.5 blocks it even though the point estimate
    /// clears, because two lucky runs are not evidence of reliability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reliability_sla_minimum: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PolicyFile {
    pub contracts: Vec<PolicyContractEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<PolicyConstraints>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_contract_entry_defaults_mode_candidate_label_and_repeat_when_omitted() {
        let entry: PolicyContractEntry = serde_json::from_str(r#"{"id":"demo-1"}"#).unwrap();
        assert_eq!(entry.mode, PolicyMode::Block);
        assert_eq!(entry.candidate_label, "ci");
        assert_eq!(entry.repeat, 1);
    }

    #[test]
    fn policy_contract_entry_respects_explicit_repeat() {
        let entry: PolicyContractEntry =
            serde_json::from_str(r#"{"id":"demo-1","repeat":8}"#).unwrap();
        assert_eq!(entry.repeat, 8);
    }

    #[test]
    fn policy_contract_entry_respects_explicit_mode_and_candidate_label() {
        let entry: PolicyContractEntry =
            serde_json::from_str(r#"{"id":"demo-1","mode":"warn","candidate_label":"pr-42"}"#)
                .unwrap();
        assert_eq!(entry.mode, PolicyMode::Warn);
        assert_eq!(entry.candidate_label, "pr-42");
    }
}
