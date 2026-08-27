use crate::charts;
use crate::colors::{self, color_enabled};
use crate::replay::replay_contract;
use crate::ui;
use crukx_core::contracts::replay::{ReplayDecision, ReplayOutcome};
use crukx_core::contracts::schema::RegressionContract;
use crukx_core::policy::consistency::pass_k;
use crukx_storage::contracts::read_contract;
use crukx_storage::runs::{append_run, RunContractSummary, RunRecord};
use crukx_storage::state_dir::StateDir;
use std::path::Path;

/// Entry point for `crukx replay <contract-id> [--candidate <label>] [--repeat <k>]`.
/// `repeat` defaults to 1 — the single-run report is unchanged from before
/// pass^k existed. `repeat > 1` actually re-spawns the contract's shell
/// steps k separate times (real executions, not a simulated repeat) and
/// reports pass^k: whether *every* run PASSed, not just the last one or
/// the majority — see crukx_core::policy::consistency for why that
/// distinction matters.
pub fn run_replay(contract_id: &str, candidate_label: &str, repeat: usize, cwd: &Path) -> i32 {
    let state = StateDir::resolve(cwd);

    let contract: RegressionContract = match read_contract(&state, contract_id) {
        Ok(contract) => contract,
        Err(err) => {
            eprintln!("Crukx replay: no contract found for \"{contract_id}\": {err}");
            eprintln!(
                "{}",
                ui::hint(
                    "create one with `crukx capture` after a `crukx run` session",
                    color_enabled()
                )
            );
            return 1;
        }
    };

    let color = color_enabled();
    let repeat = repeat.max(1);

    eprintln!(
        "{}",
        ui::header(
            "Crukx replay",
            Some(&format!(
                "contract {} vs candidate \"{}\"{}",
                contract.contract_id,
                candidate_label,
                if repeat > 1 {
                    format!(" · {repeat} runs")
                } else {
                    String::new()
                }
            )),
            color
        )
    );

    let mut outcomes = Vec::with_capacity(repeat);
    for run in 1..=repeat {
        let outcome = replay_contract(&contract, &state.contracts_dir(), candidate_label, cwd);

        if repeat > 1 {
            eprintln!(
                "Crukx replay: contract {} vs candidate \"{}\" (run {run}/{repeat})",
                outcome.contract_id, outcome.candidate_label
            );
        }
        for step in &outcome.steps {
            let mark = if step.matched_expected {
                colors::green("match", color)
            } else {
                colors::red("DIVERGED", color)
            };
            eprintln!(
                "  [{mark}] {} {} (exit {}, expected {})",
                step.executable,
                step.arguments.join(" "),
                step.actual_exit_code,
                step.expected_exit_code
            );
        }
        let assertion_mark = if outcome.assertion.pass {
            colors::green("pass", color)
        } else {
            colors::red("FAIL", color)
        };
        eprintln!(
            "  assertion: {assertion_mark}{}",
            outcome
                .assertion
                .reason
                .as_deref()
                .map(|reason| format!(" — {reason}"))
                .unwrap_or_default()
        );

        outcomes.push(outcome);
    }

    let decisions: Vec<ReplayDecision> = outcomes.iter().map(|o| o.decision).collect();
    let passed = decisions
        .iter()
        .filter(|decision| **decision == ReplayDecision::Pass)
        .count();

    if repeat == 1 {
        print_single_verdict(outcomes[0].decision, color);
        record_history(
            &state,
            &contract.contract_id,
            &decisions,
            &[],
            final_exit_code(decisions[0]),
        );
        return final_exit_code(decisions[0]);
    }

    // Trend of total wall-clock per run + the pass/fail dot strip:
    // flakiness should be visible as shape, not just a number.
    let latencies: Vec<f64> = outcomes
        .iter()
        .map(|outcome| outcome.steps.iter().map(|step| step.duration_ms).sum::<u64>() as f64)
        .collect();
    eprintln!(
        "  latency {}",
        colors::dim(&charts::sparkline(&latencies), color)
    );
    let (dots_pass, dots_fail) = charts::pass_dots_split(passed, repeat);
    eprintln!(
        "  runs     {}{}",
        colors::green(&dots_pass, color),
        colors::red(&dots_fail, color)
    );
    eprintln!();

    let consistent = pass_k(&decisions);
    let verdict = if consistent {
        colors::green("CONSISTENT", color)
    } else {
        colors::red("INCONSISTENT", color)
    };
    eprintln!(
        "{}",
        colors::bold(
            &format!("pass^{repeat}: {passed}/{repeat} runs passed — {verdict}"),
            color
        )
    );

    record_history(&state, &contract.contract_id, &decisions, &outcomes, exit_code_from(consistent));

    exit_code_from(consistent)
}

fn print_single_verdict(decision: ReplayDecision, color: bool) {
    eprintln!(
        "{}",
        if decision == ReplayDecision::Pass {
            colors::green("PASS", color)
        } else {
            colors::red("BLOCKED", color)
        }
    );
}

fn final_exit_code(decision: ReplayDecision) -> i32 {
    if decision == ReplayDecision::Pass {
        0
    } else {
        1
    }
}

fn exit_code_from(pass_k_passed: bool) -> i32 {
    if pass_k_passed {
        0
    } else {
        1
    }
}

fn record_history(
    state: &StateDir,
    contract_id: &str,
    decisions: &[ReplayDecision],
    outcomes: &[ReplayOutcome],
    _exit_code: i32,
) {
    let passed = decisions
        .iter()
        .filter(|d| **d == ReplayDecision::Pass)
        .count();
    let k = decisions.len();
    let all_passed = !decisions.is_empty() && decisions.iter().all(|d| *d == ReplayDecision::Pass);
    let p95_ms = (!outcomes.is_empty())
        .then(|| crukx_core::policy::p95_step_duration_ms(outcomes))
        .flatten();
    let record = RunRecord {
        ts: chrono::Utc::now().to_rfc3339(),
        command: "replay".to_string(),
        decision: if all_passed { "PASS".into() } else { "BLOCKED".into() },
        vtr: None,
        diagnostic_coverage: None,
        p95_ms,
        contracts: vec![RunContractSummary {
            id: contract_id.to_string(),
            decision: if all_passed { "PASS".into() } else { "BLOCK".into() },
            passed: (k > 1).then_some(passed),
            k: (k > 1).then_some(k),
        }],
    };
    let _ = append_run(state, &record);
}
