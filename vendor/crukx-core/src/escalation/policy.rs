use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationAction {
    Accept,
    Retry,
    Escalate,
}

/// `p_failure` is a caller-supplied estimate. The recommended real source
/// is `historical_p_failure` below — this contract's own past pass/fail
/// record, which `crukx gate` now feeds in from `.crukx/runs.jsonl` (no
/// separate probe model to stand up). A caller with a better estimate
/// (a post-answer probe, an anchor-calibrated judge score) can still
/// supply its own `p_failure` instead.
#[derive(Debug, Clone, Copy)]
pub struct EscalationInput {
    /// P(this attempt is a failure), 0-1.
    pub p_failure: f64,
    /// P(a retry succeeds | it's retried), 0-1.
    pub p_recovery_given_retry: f64,
    /// Cost of one retry, same unit as risk_value (e.g. USD).
    pub retry_cost: f64,
    /// Value at stake if the failure ships unfixed, same unit as retry_cost.
    pub risk_value: f64,
    /// Below this, don't bother retrying or escalating — just accept. Defaults to 0.3.
    pub failure_threshold: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct EscalationDecision {
    pub action: EscalationAction,
    pub expected_recovery_value: f64,
}

/// P(this contract fails) from its own recent gate/replay history — plain
/// fraction of `false` (BLOCK) entries in `recent_outcomes`, oldest-first
/// or newest-first, order doesn't matter to a mean. Empty history means
/// no prior signal: returns 0.5, deliberately uninformative rather than
/// optimistic (a new contract shouldn't look "reliable" just because it
/// hasn't run yet) — the caller's `failure_threshold` decides whether
/// that's high enough to act on.
pub fn historical_p_failure(recent_outcomes: &[bool]) -> f64 {
    if recent_outcomes.is_empty() {
        return 0.5;
    }
    let failures = recent_outcomes.iter().filter(|passed| !**passed).count();
    failures as f64 / recent_outcomes.len() as f64
}

pub fn decide_escalation(input: EscalationInput) -> EscalationDecision {
    let failure_threshold = input.failure_threshold.unwrap_or(0.3);
    let expected_recovery_value = input.p_recovery_given_retry * input.risk_value * input.p_failure;

    if input.p_failure < failure_threshold {
        return EscalationDecision {
            action: EscalationAction::Accept,
            expected_recovery_value,
        };
    }
    if expected_recovery_value > input.retry_cost {
        return EscalationDecision {
            action: EscalationAction::Retry,
            expected_recovery_value,
        };
    }
    EscalationDecision {
        action: EscalationAction::Escalate,
        expected_recovery_value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_p_failure_is_the_plain_block_fraction() {
        assert_eq!(historical_p_failure(&[true, true, true, false]), 0.25);
        assert_eq!(historical_p_failure(&[false, false]), 1.0);
        assert_eq!(historical_p_failure(&[true, true]), 0.0);
    }

    #[test]
    fn historical_p_failure_of_empty_history_is_uninformative_not_optimistic() {
        assert_eq!(historical_p_failure(&[]), 0.5);
    }

    #[test]
    fn accepts_when_failure_probability_is_below_threshold() {
        let decision = decide_escalation(EscalationInput {
            p_failure: 0.1,
            p_recovery_given_retry: 0.9,
            retry_cost: 1.0,
            risk_value: 1000.0,
            failure_threshold: None,
        });
        assert_eq!(decision.action, EscalationAction::Accept);
    }

    #[test]
    fn retries_when_recovery_value_beats_retry_cost() {
        let decision = decide_escalation(EscalationInput {
            p_failure: 0.74,
            p_recovery_given_retry: 0.61,
            retry_cost: 0.004,
            risk_value: 1.0,
            failure_threshold: None,
        });
        assert_eq!(decision.action, EscalationAction::Retry);
    }

    #[test]
    fn escalates_when_retry_isnt_worth_it_but_failure_risk_is_real() {
        let decision = decide_escalation(EscalationInput {
            p_failure: 0.68,
            p_recovery_given_retry: 0.05,
            retry_cost: 10.0,
            risk_value: 1.0,
            failure_threshold: None,
        });
        assert_eq!(decision.action, EscalationAction::Escalate);
    }

    #[test]
    fn expensive_retries_with_low_recovery_odds_escalate_even_at_high_failure_risk() {
        let decision = decide_escalation(EscalationInput {
            p_failure: 0.9,
            p_recovery_given_retry: 0.1,
            retry_cost: 5.0,
            risk_value: 10.0,
            failure_threshold: None,
        });
        // expected_recovery_value = 0.1 * 10.0 * 0.9 = 0.9 < retry_cost 5.0
        assert_eq!(decision.action, EscalationAction::Escalate);
    }
}
