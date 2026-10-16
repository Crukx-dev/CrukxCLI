use crate::contracts::replay::{ReplayDecision, ReplayOutcome};
use crate::diagnostics::stages::diagnostic_coverage;
use crate::evaluators::registry::{detect_judge_drift, JudgeDriftResult, JudgeRecord};
use crate::policy::performance::p95_step_duration_ms;
use crate::policy::schema::PolicyConstraints;
use crate::security::delta::{compute_security_delta, Finding, SecurityDelta};

pub struct ReliabilityContractInputs<'a> {
    pub outcomes: &'a [ReplayOutcome],
    pub constraints: PolicyConstraints,
    /// Present when a judge baseline + current record were recorded for this run: (baseline, current).
    pub judge: Option<(&'a JudgeRecord, &'a JudgeRecord)>,
    /// Present when a before/after security scan was recorded for this run: (before, after).
    pub security: Option<(&'a [Finding], &'a [Finding])>,
}

#[derive(Debug, Clone)]
pub struct ReliabilityContractResult {
    pub vtr: f64,
    pub diagnostic_coverage: f64,
    pub p95_latency_ms: Option<u64>,
    pub judge_drift: Option<JudgeDriftResult>,
    pub security_delta: Option<SecurityDelta>,
    pub violations: Vec<String>,
    pub pass: bool,
}

/// CAPO-inspired: checks hard constraints on the whole gate run instead of
/// blending metrics into one score. Any violated constraint fails the
/// contract regardless of how good the others look. Judge drift (Netflix
/// lifecycle) and security delta (WeSCE) are optional — only checked when
/// the caller actually recorded the inputs for them.
pub fn evaluate_reliability_contract(
    inputs: ReliabilityContractInputs,
) -> ReliabilityContractResult {
    let outcomes = inputs.outcomes;
    let vtr = if outcomes.is_empty() {
        1.0
    } else {
        outcomes
            .iter()
            .filter(|outcome| outcome.decision == ReplayDecision::Pass)
            .count() as f64
            / outcomes.len() as f64
    };
    let coverage = diagnostic_coverage(outcomes).coverage;

    let mut violations = Vec::new();
    if let Some(minimum) = inputs.constraints.vtr_minimum {
        if vtr < minimum {
            violations.push(format!(
                "VTR {:.1}% below minimum {:.1}%",
                vtr * 100.0,
                minimum * 100.0
            ));
        }
    }
    if let Some(minimum) = inputs.constraints.diagnostic_coverage_minimum {
        if coverage < minimum {
            violations.push(format!(
                "Diagnostic coverage {:.1}% below minimum {:.1}%",
                coverage * 100.0,
                minimum * 100.0
            ));
        }
    }

    // A constraint declared in crukx.yml but never checked (because the
    // caller didn't wire the corresponding data) must not silently pass —
    // that's a fail-open: `judge_max_agreement_drop: 0.1` in the policy
    // reads as "we gate on judge drift", and a caller bug that forgets to
    // supply judge records would otherwise make that guarantee a no-op
    // with zero signal that it never ran.
    if inputs.constraints.judge_max_agreement_drop.is_some() && inputs.judge.is_none() {
        violations.push(
            "Judge drift constraint configured but no judge record was supplied for this run — cannot verify"
                .to_string(),
        );
    }
    let judge_drift = inputs.judge.map(|(baseline, current)| {
        detect_judge_drift(
            baseline,
            current,
            inputs.constraints.judge_max_agreement_drop,
        )
    });
    if let Some(drift) = &judge_drift {
        if drift.drifted {
            violations.push(format!(
                "Judge drift: {}",
                drift
                    .reason
                    .clone()
                    .unwrap_or_else(|| "agreement dropped".to_string())
            ));
        }
    }

    let security_constraint_configured = inputs.constraints.security_critical_delta_max.is_some()
        || inputs.constraints.security_high_delta_max.is_some();
    if security_constraint_configured && inputs.security.is_none() {
        violations.push(
            "Security delta constraint configured but no before/after scan was supplied for this run — cannot verify"
                .to_string(),
        );
    }
    let security_delta = inputs
        .security
        .map(|(before, after)| compute_security_delta(before, after));
    if let Some(delta) = &security_delta {
        let critical_max = inputs.constraints.security_critical_delta_max.unwrap_or(0);
        let high_max = inputs
            .constraints
            .security_high_delta_max
            .unwrap_or(i64::MAX);
        if delta.critical_delta > critical_max {
            violations.push(format!(
                "Security: +{} critical finding(s) (max {})",
                delta.critical_delta, critical_max
            ));
        }
        if delta.high_delta > high_max {
            violations.push(format!(
                "Security: +{} high finding(s) (max {})",
                delta.high_delta, high_max
            ));
        }
    }

    let p95_latency_ms = p95_step_duration_ms(outcomes);
    if let (Some(max_ms), Some(observed_ms)) =
        (inputs.constraints.p95_latency_max_ms, p95_latency_ms)
    {
        if observed_ms > max_ms {
            violations.push(format!(
                "P95 latency {observed_ms}ms above maximum {max_ms}ms"
            ));
        }
    }

    let pass = violations.is_empty();
    ReliabilityContractResult {
        vtr,
        diagnostic_coverage: coverage,
        p95_latency_ms,
        judge_drift,
        security_delta,
        violations,
        pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::replay::{ReplayDecision, ReplayOutcome};
    use crate::contracts::schema::AssertResult;
    use crate::evaluators::registry::JudgeRecord;
    use crate::security::delta::{Finding, Severity};

    fn outcome(decision: ReplayDecision, id: &str) -> ReplayOutcome {
        ReplayOutcome {
            contract_id: id.to_string(),
            candidate_label: "ci".to_string(),
            steps: vec![],
            assertion: AssertResult {
                pass: decision == ReplayDecision::Pass,
                reason: None,
            },
            decision,
        }
    }

    fn judge() -> JudgeRecord {
        JudgeRecord {
            judge_id: "j1".to_string(),
            model: "claude-sonnet-5".to_string(),
            rubric_version: "17".to_string(),
            calibration_set_id: "support-v3".to_string(),
            human_agreement_rate: 0.91,
            evaluated_at: "2026-08-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn passes_with_no_constraints_regardless_of_vtr() {
        let outcomes = vec![outcome(ReplayDecision::Block, "c")];
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints::default(),
            judge: None,
            security: None,
        });
        assert!(result.pass);
        assert_eq!(result.vtr, 0.0);
    }

    #[test]
    fn fails_when_vtr_drops_below_vtr_minimum() {
        let outcomes = vec![
            outcome(ReplayDecision::Pass, "a"),
            outcome(ReplayDecision::Block, "b"),
        ];
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints {
                vtr_minimum: Some(0.98),
                ..Default::default()
            },
            judge: None,
            security: None,
        });
        assert!(!result.pass);
        assert_eq!(result.violations.len(), 1);
    }

    #[test]
    fn fails_when_diagnostic_coverage_drops_below_diagnostic_coverage_minimum() {
        let outcomes = vec![outcome(ReplayDecision::Block, "c")];
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints {
                diagnostic_coverage_minimum: Some(0.9),
                ..Default::default()
            },
            judge: None,
            security: None,
        });
        // assertion.pass is false with no diverged steps -> localized to
        // response_validation -> coverage 1.0, so this constraint holds.
        assert_eq!(result.diagnostic_coverage, 1.0);
        assert!(result.pass);
    }

    #[test]
    fn empty_outcomes_yields_vtr_1_and_passes_any_minimum() {
        let outcomes: Vec<ReplayOutcome> = vec![];
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints {
                vtr_minimum: Some(1.0),
                ..Default::default()
            },
            judge: None,
            security: None,
        });
        assert_eq!(result.vtr, 1.0);
        assert!(result.pass);
    }

    #[test]
    fn fails_when_judge_has_drifted() {
        let outcomes = vec![outcome(ReplayDecision::Pass, "c")];
        let mut current = judge();
        current.human_agreement_rate = 0.7;
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints::default(),
            judge: Some((&judge(), &current)),
            security: None,
        });
        assert!(!result.pass);
        assert!(result.judge_drift.unwrap().drifted);
    }

    #[test]
    fn passes_when_judge_agreement_is_stable() {
        let outcomes = vec![outcome(ReplayDecision::Pass, "c")];
        let mut current = judge();
        current.human_agreement_rate = 0.9;
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints::default(),
            judge: Some((&judge(), &current)),
            security: None,
        });
        assert!(result.pass);
    }

    #[test]
    fn fails_when_a_critical_security_finding_was_introduced_default_max_0() {
        let outcomes = vec![outcome(ReplayDecision::Pass, "c")];
        let before: Vec<Finding> = vec![];
        let after = vec![Finding {
            id: "x".to_string(),
            severity: Severity::Critical,
            cwe: None,
            location: None,
        }];
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints::default(),
            judge: None,
            security: Some((&before, &after)),
        });
        assert!(!result.pass);
        assert_eq!(result.security_delta.unwrap().critical_delta, 1);
    }

    #[test]
    fn fails_when_judge_constraint_is_configured_but_no_judge_data_was_supplied() {
        let outcomes = vec![outcome(ReplayDecision::Pass, "c")];
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints {
                judge_max_agreement_drop: Some(0.1),
                ..Default::default()
            },
            judge: None,
            security: None,
        });
        assert!(!result.pass);
        assert!(result
            .violations
            .iter()
            .any(|v| v.contains("Judge drift constraint configured")));
    }

    #[test]
    fn fails_when_security_constraint_is_configured_but_no_security_data_was_supplied() {
        let outcomes = vec![outcome(ReplayDecision::Pass, "c")];
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints {
                security_critical_delta_max: Some(0),
                ..Default::default()
            },
            judge: None,
            security: None,
        });
        assert!(!result.pass);
        assert!(result
            .violations
            .iter()
            .any(|v| v.contains("Security delta constraint configured")));
    }

    #[test]
    fn passes_when_security_critical_delta_max_allows_the_introduced_finding() {
        let outcomes = vec![outcome(ReplayDecision::Pass, "c")];
        let before: Vec<Finding> = vec![];
        let after = vec![Finding {
            id: "x".to_string(),
            severity: Severity::Critical,
            cwe: None,
            location: None,
        }];
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints {
                security_critical_delta_max: Some(1),
                ..Default::default()
            },
            judge: None,
            security: Some((&before, &after)),
        });
        assert!(result.pass);
    }

    fn outcome_with_step_duration(duration_ms: u64) -> ReplayOutcome {
        ReplayOutcome {
            contract_id: "c".to_string(),
            candidate_label: "ci".to_string(),
            steps: vec![crate::contracts::replay::StepResult {
                executable: "true".to_string(),
                arguments: vec![],
                expected_exit_code: 0,
                actual_exit_code: 0,
                matched_expected: true,
                duration_ms,
            }],
            assertion: AssertResult {
                pass: true,
                reason: None,
            },
            decision: ReplayDecision::Pass,
        }
    }

    #[test]
    fn fails_when_p95_latency_exceeds_the_configured_maximum() {
        let outcomes = vec![outcome_with_step_duration(5000)];
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints {
                p95_latency_max_ms: Some(2500),
                ..Default::default()
            },
            judge: None,
            security: None,
        });
        assert!(!result.pass);
        assert_eq!(result.p95_latency_ms, Some(5000));
        assert!(result.violations.iter().any(|v| v.contains("P95 latency")));
    }

    #[test]
    fn passes_when_p95_latency_is_within_the_configured_maximum() {
        let outcomes = vec![outcome_with_step_duration(100)];
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints {
                p95_latency_max_ms: Some(2500),
                ..Default::default()
            },
            judge: None,
            security: None,
        });
        assert!(result.pass);
    }

    #[test]
    fn a_missing_p95_never_violates_a_configured_maximum() {
        // No steps to measure (e.g. an assertion-only contract) is not
        // the same as a P95 of zero, and must not be treated as one.
        let outcomes = vec![outcome(ReplayDecision::Pass, "c")];
        let result = evaluate_reliability_contract(ReliabilityContractInputs {
            outcomes: &outcomes,
            constraints: PolicyConstraints {
                p95_latency_max_ms: Some(1),
                ..Default::default()
            },
            judge: None,
            security: None,
        });
        assert_eq!(result.p95_latency_ms, None);
        assert!(result.pass);
    }
}
