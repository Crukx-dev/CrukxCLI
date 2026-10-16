use crate::contracts::replay::ReplayOutcome;

/// P95 wall-clock duration across every replayed step in every outcome —
/// a release-gate dimension alongside VTR/diagnostic-coverage/judge-drift/
/// security-delta, inspired by a self-testing quality-gate design (release
/// decisions across task success rate, latency, safety, and evidence
/// coverage). Unlike the judge-attribution and escalation-policy modules
/// added earlier this session, this one is wired to *real* measured data —
/// `crukx-runner::spawn` times every step it runs, not a placeholder.
///
/// `None` when there are no steps to measure (e.g. every contract's
/// trajectory is assertion-only) — a missing P95 is not the same as a
/// P95 of zero, and callers must not treat them the same.
pub fn p95_step_duration_ms(outcomes: &[ReplayOutcome]) -> Option<u64> {
    let mut durations: Vec<u64> = outcomes
        .iter()
        .flat_map(|outcome| outcome.steps.iter().map(|step| step.duration_ms))
        .collect();
    if durations.is_empty() {
        return None;
    }
    durations.sort_unstable();
    // Nearest-rank method: index = ceil(0.95 * n) - 1, clamped into range.
    let rank = ((durations.len() as f64) * 0.95).ceil() as usize;
    let index = rank.saturating_sub(1).min(durations.len() - 1);
    Some(durations[index])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::replay::{ReplayDecision, StepResult};
    use crate::contracts::schema::AssertResult;

    fn outcome_with_durations(durations: &[u64]) -> ReplayOutcome {
        ReplayOutcome {
            contract_id: "c".to_string(),
            candidate_label: "ci".to_string(),
            steps: durations
                .iter()
                .map(|&duration_ms| StepResult {
                    executable: "true".to_string(),
                    arguments: vec![],
                    expected_exit_code: 0,
                    actual_exit_code: 0,
                    matched_expected: true,
                    duration_ms,
                })
                .collect(),
            assertion: AssertResult {
                pass: true,
                reason: None,
            },
            decision: ReplayDecision::Pass,
        }
    }

    #[test]
    fn p95_is_none_when_there_are_no_steps() {
        assert_eq!(p95_step_duration_ms(&[outcome_with_durations(&[])]), None);
        assert_eq!(p95_step_duration_ms(&[]), None);
    }

    #[test]
    fn p95_of_a_single_value_is_that_value() {
        assert_eq!(
            p95_step_duration_ms(&[outcome_with_durations(&[42])]),
            Some(42)
        );
    }

    #[test]
    fn p95_picks_the_19th_of_20_sorted_values() {
        let durations: Vec<u64> = (1..=20).collect(); // 1..=20
        let outcome = outcome_with_durations(&durations);
        // ceil(0.95 * 20) = 19th ranked value (1-indexed) = 19
        assert_eq!(p95_step_duration_ms(&[outcome]), Some(19));
    }

    #[test]
    fn p95_aggregates_steps_across_multiple_outcomes() {
        let outcomes = vec![
            outcome_with_durations(&[10, 20]),
            outcome_with_durations(&[30, 1000]),
        ];
        // sorted: [10, 20, 30, 1000], n=4, ceil(0.95*4)=4 -> last element
        assert_eq!(p95_step_duration_ms(&outcomes), Some(1000));
    }
}
