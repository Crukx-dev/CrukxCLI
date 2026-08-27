use crukx_core::contracts::schema::{AssertContext, AssertResult};
use crukx_core::events::AgentAction;
use crukx_runner::{spawn_capture, ExecutionPolicy, SpawnRequest};
use std::fs;
use std::path::Path;

const RUNNER_SCRIPT: &str = include_str!("../assets/replay-runner.mjs");

/// Executes a user-authored assertion file in a child `node` process and
/// parses its stdout as the `AssertResult`. v1 scope: assertion files must
/// be ESM JS (`.mjs`/`.js`) — see the comment in
/// crates/crukx-cli/assets/replay-runner.mjs for why this doesn't support
/// `.ts` the way the TS CLI's `tsx`-based runner does.
///
/// Never propagates an `Err` for a failing/unparseable assertion — matches
/// the TS implementation's fallback behavior: a broken assertion is
/// reported as `{ pass: false, reason: ... }`, not a crash, because a
/// broken assertion file is exactly the kind of problem `crukx gate`
/// exists to surface.
pub fn run_assertion(assertion_file: &Path, context: &AssertContext, cwd: &Path) -> AssertResult {
    let tmp_dir = match tempfile::Builder::new().prefix("crukx-assert-").tempdir() {
        Ok(dir) => dir,
        Err(err) => {
            return AssertResult {
                pass: false,
                reason: Some(format!("failed to create temp dir: {err}")),
            };
        }
    };

    let runner_path = tmp_dir.path().join("replay-runner.mjs");
    let context_path = tmp_dir.path().join("context.json");

    if let Err(err) = fs::write(&runner_path, RUNNER_SCRIPT) {
        return AssertResult {
            pass: false,
            reason: Some(format!("failed to write runner script: {err}")),
        };
    }
    let context_json = match serde_json::to_string(context) {
        Ok(json) => json,
        Err(err) => {
            return AssertResult {
                pass: false,
                reason: Some(format!("failed to serialize assertion context: {err}")),
            };
        }
    };
    if let Err(err) = fs::write(&context_path, context_json) {
        return AssertResult {
            pass: false,
            reason: Some(format!("failed to write context file: {err}")),
        };
    }

    let arguments = vec![
        runner_path.to_string_lossy().into_owned(),
        assertion_file.to_string_lossy().into_owned(),
        context_path.to_string_lossy().into_owned(),
    ];
    let action = AgentAction::ShellCommand {
        executable: "node".to_string(),
        arguments: arguments.clone(),
        cwd: cwd.to_string_lossy().into_owned(),
        exit_code: None,
        signal: None,
    };

    let outcome = spawn_capture(SpawnRequest {
        executable: "node".to_string(),
        arguments,
        cwd: cwd.to_path_buf(),
        policy: ExecutionPolicy::DangerFullAccess,
        action,
    });

    match outcome {
        Ok(outcome) => match serde_json::from_str::<AssertResult>(&outcome.stdout) {
            Ok(result) => result,
            Err(_) => AssertResult {
                pass: false,
                reason: Some(format!(
                    "assertion runner produced no parseable result{}",
                    if outcome.stderr.trim().is_empty() {
                        String::new()
                    } else {
                        format!(": {}", outcome.stderr.trim())
                    }
                )),
            },
        },
        Err(err) => AssertResult {
            pass: false,
            reason: Some(format!("failed to run assertion: {err:?}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn context() -> AssertContext {
        AssertContext {
            contract_id: "c1".to_string(),
            source_session_id: "sess-1".to_string(),
            candidate_label: "ci".to_string(),
        }
    }

    #[test]
    fn run_assertion_reports_pass_from_a_passing_assertion_file() {
        let dir = tempfile::tempdir().unwrap();
        let assertion_path = dir.path().join("assert.mjs");
        fs::write(
            &assertion_path,
            "export async function assert() { return { pass: true, reason: \"ok\" }; }",
        )
        .unwrap();

        let result = run_assertion(&assertion_path, &context(), dir.path());
        assert!(result.pass, "expected pass, got: {result:?}");
    }

    #[test]
    fn run_assertion_reports_fail_from_a_failing_assertion_file() {
        let dir = tempfile::tempdir().unwrap();
        let assertion_path = dir.path().join("assert.mjs");
        fs::write(
            &assertion_path,
            "export async function assert() { return { pass: false, reason: \"refund too high\" }; }",
        )
        .unwrap();

        let result = run_assertion(&assertion_path, &context(), dir.path());
        assert!(!result.pass);
        assert_eq!(result.reason.as_deref(), Some("refund too high"));
    }

    #[test]
    fn run_assertion_passes_the_context_through_to_the_assertion_file() {
        let dir = tempfile::tempdir().unwrap();
        let assertion_path = dir.path().join("assert.mjs");
        fs::write(
            &assertion_path,
            "export async function assert(context) { return { pass: context.candidate_label === 'ci', reason: JSON.stringify(context) }; }",
        )
        .unwrap();

        let result = run_assertion(&assertion_path, &context(), dir.path());
        assert!(result.pass, "expected pass, got: {result:?}");
    }

    #[test]
    fn run_assertion_reports_failure_when_the_assertion_file_throws() {
        let dir = tempfile::tempdir().unwrap();
        let assertion_path = dir.path().join("assert.mjs");
        fs::write(
            &assertion_path,
            "export async function assert() { throw new Error('boom'); }",
        )
        .unwrap();

        let result = run_assertion(&assertion_path, &context(), dir.path());
        assert!(!result.pass);
        assert!(result.reason.unwrap().contains("boom"));
    }

    #[test]
    fn run_assertion_reports_failure_when_the_file_has_no_assert_export() {
        let dir = tempfile::tempdir().unwrap();
        let assertion_path = dir.path().join("assert.mjs");
        fs::write(&assertion_path, "export const notAssert = 1;").unwrap();

        let result = run_assertion(&assertion_path, &context(), dir.path());
        assert!(!result.pass);
    }
}
