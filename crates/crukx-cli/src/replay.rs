use crate::assertion::run_assertion;
use crukx_core::contracts::replay::{ReplayDecision, ReplayOutcome, StepResult};
use crukx_core::contracts::schema::{AssertContext, AssertResult, RegressionContract};
use crukx_core::events::{ActionPhase, AgentAction};
use crukx_runner::{spawn, ExecutionPolicy, SpawnRequest};
use std::path::Path;

/// Replays a contract's captured trajectory: re-executes each recorded
/// completed shell step in the current repo state — the deterministic,
/// honest part, no swapping in a candidate model/agent yet, that's still
/// open design work — and then runs the assertion hook. BLOCKs if either
/// the trajectory diverges from what was captured or the assertion fails.
///
/// Each step is re-labeled with the *current* `cwd` before spawning, not
/// the cwd captured at record time: replay always runs against this
/// checkout, and `crukx-runner::spawn`'s label-must-match-request check
/// exists precisely to catch stale/incorrect labels — carrying the
/// original cwd through would make every replay refuse itself.
pub fn replay_contract(
    contract: &RegressionContract,
    assertion_dir: &Path,
    candidate_label: &str,
    cwd: &Path,
) -> ReplayOutcome {
    let mut steps = Vec::new();

    for expected_step in &contract.expected_trajectory {
        if expected_step.phase != ActionPhase::Completed {
            continue;
        }
        let AgentAction::ShellCommand {
            executable,
            arguments,
            exit_code: expected_exit_code,
            ..
        } = &expected_step.action
        else {
            continue;
        };

        let action = AgentAction::ShellCommand {
            executable: executable.clone(),
            arguments: arguments.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            exit_code: None,
            signal: None,
        };
        let spawn_outcome = spawn(SpawnRequest {
            executable: executable.clone(),
            arguments: arguments.clone(),
            cwd: cwd.to_path_buf(),
            policy: ExecutionPolicy::DangerFullAccess,
            action,
        });
        // policy denial or spawn failure both read as "didn't run as
        // expected"; duration is 0 in that case — there's nothing to time.
        let (actual_exit_code, duration_ms) = match spawn_outcome {
            Ok(outcome) => (outcome.exit_code, outcome.duration_ms),
            Err(_) => (127, 0),
        };

        let expected_exit_code = expected_exit_code.unwrap_or(0);
        steps.push(StepResult {
            executable: executable.clone(),
            arguments: arguments.clone(),
            expected_exit_code,
            actual_exit_code,
            matched_expected: actual_exit_code == expected_exit_code,
            duration_ms,
        });
    }

    let assertion_context = AssertContext {
        contract_id: contract.contract_id.clone(),
        source_session_id: contract.source_session_id.clone(),
        candidate_label: candidate_label.to_string(),
    };
    // `assertion_file` is data read back out of the contract's own JSON —
    // never trust it as a bare path segment. `Path::join` doesn't stop
    // "../../x" or an absolute path from walking straight out of
    // `assertion_dir`, which would point `node` (run with full access, no
    // sandboxing — see crukx-runner's docs) at an arbitrary file to
    // execute. Refuse instead of joining.
    let assertion = if crukx_core::security::is_safe_path_segment(&contract.assertion_file) {
        let assertion_file = assertion_dir.join(&contract.assertion_file);
        run_assertion(&assertion_file, &assertion_context, cwd)
    } else {
        AssertResult {
            pass: false,
            reason: Some(format!(
                "contract's assertion_file {:?} is not a safe path segment — refusing to run it",
                contract.assertion_file
            )),
        }
    };

    let trajectory_ok = steps.iter().all(|step| step.matched_expected);
    let decision = if trajectory_ok && assertion.pass {
        ReplayDecision::Pass
    } else {
        ReplayDecision::Block
    };

    ReplayOutcome {
        contract_id: contract.contract_id.clone(),
        candidate_label: candidate_label.to_string(),
        steps,
        assertion,
        decision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crukx_core::contracts::schema::{ExpectedStep, CONTRACT_SCHEMA_VERSION};
    use std::fs;

    fn shell_step(executable: &str, arguments: Vec<&str>, exit_code: i32) -> ExpectedStep {
        ExpectedStep {
            phase: ActionPhase::Completed,
            action: AgentAction::ShellCommand {
                executable: executable.to_string(),
                arguments: arguments.into_iter().map(String::from).collect(),
                cwd: "/original/capture/cwd".to_string(),
                exit_code: Some(exit_code),
                signal: None,
            },
        }
    }

    #[test]
    fn passing_trajectory_and_passing_assertion_yields_pass() {
        let dir = tempfile::tempdir().unwrap();
        let assertion_path = dir.path().join("assert.mjs");
        fs::write(
            &assertion_path,
            "export async function assert() { return { pass: true }; }",
        )
        .unwrap();

        let contract = RegressionContract {
            schema_version: CONTRACT_SCHEMA_VERSION,
            contract_id: "c1".to_string(),
            created_at: "2026-08-22T00:00:00Z".to_string(),
            source_session_id: "sess-1".to_string(),
            source_event_file: "sess-1.jsonl".to_string(),
            expected_trajectory: vec![shell_step("true", vec![], 0)],
            assertion_file: "assert.mjs".to_string(),
        };

        let outcome = replay_contract(&contract, dir.path(), "ci", dir.path());
        assert_eq!(outcome.decision, ReplayDecision::Pass);
        assert_eq!(outcome.steps.len(), 1);
        assert!(outcome.steps[0].matched_expected);
    }

    #[test]
    fn traversal_assertion_file_is_refused_not_executed() {
        let dir = tempfile::tempdir().unwrap();
        // A real file outside `dir` that the traversal path would resolve
        // to — if this test passed by *reading* it, the escape worked.
        let outside = tempfile::tempdir().unwrap();
        fs::write(
            outside.path().join("payload.mjs"),
            "export async function assert() { return { pass: true, reason: 'escaped' }; }",
        )
        .unwrap();
        let traversal = format!(
            "../{}/payload.mjs",
            outside.path().file_name().unwrap().to_string_lossy()
        );

        let contract = RegressionContract {
            schema_version: CONTRACT_SCHEMA_VERSION,
            contract_id: "c1".to_string(),
            created_at: "2026-08-22T00:00:00Z".to_string(),
            source_session_id: "sess-1".to_string(),
            source_event_file: "sess-1.jsonl".to_string(),
            expected_trajectory: vec![],
            assertion_file: traversal,
        };

        let outcome = replay_contract(&contract, dir.path(), "ci", dir.path());
        assert_eq!(outcome.decision, ReplayDecision::Block);
        assert_ne!(outcome.assertion.reason.as_deref(), Some("escaped"));
    }

    #[test]
    fn diverged_step_yields_block_even_with_a_passing_assertion() {
        let dir = tempfile::tempdir().unwrap();
        let assertion_path = dir.path().join("assert.mjs");
        fs::write(
            &assertion_path,
            "export async function assert() { return { pass: true }; }",
        )
        .unwrap();

        let contract = RegressionContract {
            schema_version: CONTRACT_SCHEMA_VERSION,
            contract_id: "c1".to_string(),
            created_at: "2026-08-22T00:00:00Z".to_string(),
            source_session_id: "sess-1".to_string(),
            source_event_file: "sess-1.jsonl".to_string(),
            // captured exit_code was 0, but `false` always exits 1 -> divergence
            expected_trajectory: vec![shell_step("false", vec![], 0)],
            assertion_file: "assert.mjs".to_string(),
        };

        let outcome = replay_contract(&contract, dir.path(), "ci", dir.path());
        assert_eq!(outcome.decision, ReplayDecision::Block);
        assert!(!outcome.steps[0].matched_expected);
    }

    #[test]
    fn failing_assertion_yields_block_even_with_a_matching_trajectory() {
        let dir = tempfile::tempdir().unwrap();
        let assertion_path = dir.path().join("assert.mjs");
        fs::write(
            &assertion_path,
            "export async function assert() { return { pass: false, reason: 'refund too high' }; }",
        )
        .unwrap();

        let contract = RegressionContract {
            schema_version: CONTRACT_SCHEMA_VERSION,
            contract_id: "c1".to_string(),
            created_at: "2026-08-22T00:00:00Z".to_string(),
            source_session_id: "sess-1".to_string(),
            source_event_file: "sess-1.jsonl".to_string(),
            expected_trajectory: vec![shell_step("true", vec![], 0)],
            assertion_file: "assert.mjs".to_string(),
        };

        let outcome = replay_contract(&contract, dir.path(), "ci", dir.path());
        assert_eq!(outcome.decision, ReplayDecision::Block);
        assert_eq!(outcome.assertion.reason.as_deref(), Some("refund too high"));
    }

    #[test]
    fn non_shell_and_non_completed_steps_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let assertion_path = dir.path().join("assert.mjs");
        fs::write(
            &assertion_path,
            "export async function assert() { return { pass: true }; }",
        )
        .unwrap();

        let contract = RegressionContract {
            schema_version: CONTRACT_SCHEMA_VERSION,
            contract_id: "c1".to_string(),
            created_at: "2026-08-22T00:00:00Z".to_string(),
            source_session_id: "sess-1".to_string(),
            source_event_file: "sess-1.jsonl".to_string(),
            expected_trajectory: vec![
                ExpectedStep {
                    phase: ActionPhase::Proposed,
                    action: AgentAction::ShellCommand {
                        executable: "true".to_string(),
                        arguments: vec![],
                        cwd: "x".to_string(),
                        exit_code: Some(0),
                        signal: None,
                    },
                },
                ExpectedStep {
                    phase: ActionPhase::Completed,
                    action: AgentAction::FileRead {
                        path: "x".to_string(),
                    },
                },
            ],
            assertion_file: "assert.mjs".to_string(),
        };

        let outcome = replay_contract(&contract, dir.path(), "ci", dir.path());
        assert_eq!(outcome.steps.len(), 0);
        assert_eq!(outcome.decision, ReplayDecision::Pass);
    }
}
