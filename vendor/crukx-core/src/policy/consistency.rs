use crate::contracts::replay::ReplayDecision;

/// `pass^k` (distinct from `pass@k`): true only if *every* one of k
/// independent replay runs PASSed — not "at least one." This is the
/// consistency metric a ReliabilityBench-style reliability surface
/// R(k, ε, λ) is built on (arxiv 2601.06112), and matches the "Verified
/// Task Reliability at k repeated runs" (VTRₖ) concept from this
/// project's own founding design discussion: an agent that passes 60% of
/// the time on a single try but only 25% of the time across five
/// independent tries is not actually reliable, and pass@1 alone can't see
/// that — you have to actually run it more than once.
///
/// Empty input is `false`, not vacuously `true` — "we ran it zero times"
/// is not evidence of consistency.
pub fn pass_k(runs: &[ReplayDecision]) -> bool {
    !runs.is_empty()
        && runs
            .iter()
            .all(|decision| *decision == ReplayDecision::Pass)
}

#[derive(Debug, Clone)]
pub struct VtrKResult {
    pub k: usize,
    /// Fraction of contracts whose k runs all PASSed.
    pub vtr_k: f64,
    /// Per-contract pass^k verdict, in input order.
    pub per_contract: Vec<(String, bool)>,
}

/// Aggregates `pass_k` across multiple contracts, each replayed k times.
/// `contract_runs` is `(contract_id, decisions_from_k_runs)` — the caller
/// (crukx-cli, which actually spawns the k real replays) supplies real
/// repeated-execution results; this function only aggregates them.
pub fn compute_vtr_k(contract_runs: &[(String, Vec<ReplayDecision>)], k: usize) -> VtrKResult {
    let per_contract: Vec<(String, bool)> = contract_runs
        .iter()
        .map(|(id, runs)| (id.clone(), pass_k(runs)))
        .collect();

    let vtr_k = if per_contract.is_empty() {
        1.0
    } else {
        per_contract.iter().filter(|(_, passed)| *passed).count() as f64 / per_contract.len() as f64
    };

    VtrKResult {
        k,
        vtr_k,
        per_contract,
    }
}

/// Wilson score interval on a binomial proportion — `(lower, upper)` at
/// confidence `z` (1.96 ≈ 95%). Unlike the naive ±1.96·√(p(1-p)/n) band
/// it stays inside [0, 1] and doesn't collapse at the extremes: 5/5
/// reads [0.566, 1.000], not [1.0, 1.0], so a lucky small-n run can't
/// dress itself up as certainty. This is the honesty layer on pass^k:
/// a point estimate from k=5 runs claims far more precision than the
/// data holds, and any SLA gate worth its name must be evaluated against
/// the *lower* bound — the worst case still consistent with what was
/// observed (see `pass_k_lower_bound`).
///
/// Returns `None` for zero trials ("we ran it zero times" has no
/// interval), and treats non-finite/absurd `z` defensively by clamping.
pub fn wilson_interval(passed: usize, total: usize, z: f64) -> Option<(f64, f64)> {
    if total == 0 {
        return None;
    }
    let z = if z.is_finite() && z > 0.0 { z } else { 1.96 };
    let n = total as f64;
    let p = passed.min(total) as f64 / n;
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let half = (z / denom) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    Some((
        (center - half).clamp(0.0, 1.0),
        (center + half).clamp(0.0, 1.0),
    ))
}

/// Lower edge of the Wilson interval for a pass^k run's observed pass
/// count — the "honest worst case" number an SLA gates on. A contract
/// that passed 3 of 5 runs reports pass^5 = 60% as its point estimate
/// but only ~23% as its defensible floor; gating on the floor is what
/// stops two lucky runs from certifying an agent.
pub fn pass_k_lower_bound(decisions: &[ReplayDecision], z: f64) -> Option<f64> {
    let passed = decisions
        .iter()
        .filter(|d| **d == ReplayDecision::Pass)
        .count();
    wilson_interval(passed, decisions.len(), z).map(|(lower, _)| lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_k_is_true_only_when_every_run_passed() {
        assert!(pass_k(&[
            ReplayDecision::Pass,
            ReplayDecision::Pass,
            ReplayDecision::Pass
        ]));
        assert!(!pass_k(&[
            ReplayDecision::Pass,
            ReplayDecision::Block,
            ReplayDecision::Pass
        ]));
    }

    #[test]
    fn pass_k_is_false_for_zero_runs_not_vacuously_true() {
        assert!(!pass_k(&[]));
    }

    #[test]
    fn pass_k_distinguishes_from_pass_at_k_which_would_accept_any_single_pass() {
        // 4 of 5 runs pass -> pass@k would call this a win, pass^k must not.
        let mostly_passing = [
            ReplayDecision::Pass,
            ReplayDecision::Pass,
            ReplayDecision::Pass,
            ReplayDecision::Pass,
            ReplayDecision::Block,
        ];
        assert!(!pass_k(&mostly_passing));
    }

    #[test]
    fn compute_vtr_k_is_the_fraction_of_contracts_that_are_fully_consistent() {
        let runs = vec![
            (
                "a".to_string(),
                vec![ReplayDecision::Pass, ReplayDecision::Pass],
            ),
            (
                "b".to_string(),
                vec![ReplayDecision::Pass, ReplayDecision::Block],
            ),
            (
                "c".to_string(),
                vec![ReplayDecision::Pass, ReplayDecision::Pass],
            ),
        ];
        let result = compute_vtr_k(&runs, 2);
        assert_eq!(result.vtr_k, 2.0 / 3.0);
        assert_eq!(
            result.per_contract,
            vec![
                ("a".to_string(), true),
                ("b".to_string(), false),
                ("c".to_string(), true)
            ]
        );
    }

    #[test]
    fn compute_vtr_k_with_no_contracts_is_1_0() {
        let result = compute_vtr_k(&[], 5);
        assert_eq!(result.vtr_k, 1.0);
    }

    #[test]
    fn wilson_interval_is_none_for_zero_trials() {
        assert!(wilson_interval(0, 0, 1.96).is_none());
        assert!(pass_k_lower_bound(&[], 1.96).is_none());
    }

    #[test]
    fn wilson_interval_for_perfect_small_sample_stays_honest_not_degenerate() {
        // 5/5 must NOT read [1.0, 1.0] — the whole point of Wilson over
        // the naive band. Literature anchor: ≈[0.566, 0.999…→clamped 1.0].
        let (lower, upper) = wilson_interval(5, 5, 1.96).unwrap();
        assert!((lower - 0.5656).abs() < 0.01, "lower {lower}");
        assert!((upper - 1.0).abs() < 1e-9, "upper {upper}");
    }

    #[test]
    fn wilson_interval_matches_known_anchor_at_eight_of_ten() {
        let (lower, upper) = wilson_interval(8, 10, 1.96).unwrap();
        assert!((lower - 0.4902).abs() < 0.005, "lower {lower}");
        assert!((upper - 0.9433).abs() < 0.005, "upper {upper}");
    }

    #[test]
    fn wilson_interval_always_contains_the_point_estimate() {
        for total in [1usize, 2, 3, 7, 10, 50, 100] {
            for passed in 0..=total {
                let (lower, upper) = wilson_interval(passed, total, 1.96).unwrap();
                let p = passed as f64 / total as f64;
                assert!(
                    lower <= p + f64::EPSILON && p <= upper + f64::EPSILON,
                    "interval [{lower}, {upper}] missed point {p} at {passed}/{total}"
                );
            }
        }
    }

    #[test]
    fn wilson_interval_bounds_stay_inside_unit_range_even_at_extremes() {
        for (passed, total) in [(0usize, 3usize), (3, 3), (1, 1000), (999, 1000)] {
            let (lower, upper) = wilson_interval(passed, total, 2.576).unwrap();
            assert!((0.0..=1.0).contains(&lower));
            assert!((0.0..=1.0).contains(&upper));
        }
    }

    #[test]
    fn wilson_interval_widens_with_confidence_level() {
        let narrow = wilson_interval(6, 10, 1.64).unwrap();
        let wide = wilson_interval(6, 10, 2.576).unwrap();
        assert!(wide.0 < narrow.0 && wide.1 > narrow.1);
    }

    #[test]
    fn pass_k_lower_bound_counts_only_real_passes() {
        let decisions = [
            ReplayDecision::Pass,
            ReplayDecision::Pass,
            ReplayDecision::Pass,
            ReplayDecision::Block,
            ReplayDecision::Block,
        ];
        let lower = pass_k_lower_bound(&decisions, 1.96).unwrap();
        let (expected_lower, _) = wilson_interval(3, 5, 1.96).unwrap();
        assert!((lower - expected_lower).abs() < 1e-12);
    }

    #[test]
    fn absurd_z_values_fall_back_to_the_default_instead_of_panicking() {
        let a = wilson_interval(3, 5, f64::NAN).unwrap();
        let b = wilson_interval(3, 5, 0.0).unwrap();
        let c = wilson_interval(3, 5, 1.96).unwrap();
        assert_eq!(a, c);
        assert_eq!(b, c);
    }
}
