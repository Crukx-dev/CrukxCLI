use crate::contracts::schema::{ExpectedStep, RegressionContract, CONTRACT_SCHEMA_VERSION};
use crate::events::{AgentAction, AgentEvent};

/// Pure: builds a contract record from an already-loaded session's events.
/// Drops `session` actions (lifecycle markers, not something a replay
/// re-executes) — everything else becomes an expected trajectory step,
/// verbatim, in recorded order.
pub fn build_contract(
    events: &[AgentEvent],
    session_id: &str,
    source_event_file: &str,
    contract_id: String,
    created_at: String,
) -> RegressionContract {
    let expected_trajectory: Vec<ExpectedStep> = events
        .iter()
        .filter(|event| !matches!(event.action, AgentAction::Session { .. }))
        .map(|event| ExpectedStep {
            phase: event.phase,
            action: event.action.clone(),
        })
        .collect();

    RegressionContract {
        schema_version: CONTRACT_SCHEMA_VERSION,
        contract_id: contract_id.clone(),
        created_at,
        source_session_id: session_id.to_string(),
        source_event_file: source_event_file.to_string(),
        expected_trajectory,
        assertion_file: format!("{contract_id}.assert.mjs"),
    }
}

/// Assertion stub written alongside a new contract. Plain ESM JS
/// (`.mjs`), not TypeScript — see crukx-cli/assets/replay-runner.mjs for
/// why crukx-rs v1 doesn't take a `tsx` dependency the way the TS CLI's
/// stub (`.assert.ts`) does.
pub fn assertion_stub(contract_id: &str) -> String {
    format!(
        r#"// Crukx regression contract assertion for {contract_id}.
// Fill in what "the workflow ended correctly" means for your system —
// Crukx can't infer arbitrary backend state generically (a DB row, an API
// call, a file's contents). Check whatever your workflow actually changes.
//
// Plain ESM JS, run by `crukx gate`/`crukx replay`. No DSL to learn — same
// pattern as any test runner's custom matcher.
//
// AssertContext shape: {{ contract_id, source_session_id, candidate_label }}
// AssertResult shape:  {{ pass, reason? }}

export async function assert(context) {{
  // TODO: replace with a real check against your system's actual state.
  return {{ pass: true, reason: "not yet implemented — always passes" }};
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{ActionPhase, AgentAdapter, EVENT_SCHEMA_VERSION};

    fn event(sequence: u64, phase: ActionPhase, action: AgentAction) -> AgentEvent {
        AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            event_id: format!("evt-{sequence}"),
            session_id: "sess-1".to_string(),
            sequence,
            occurred_at: "2026-08-24T00:00:00Z".to_string(),
            adapter: AgentAdapter::Claude,
            repository_root: "/repo".to_string(),
            phase,
            action,
            policy: None,
            error: None,
        }
    }

    #[test]
    fn build_contract_carries_over_non_session_events_as_expected_steps() {
        let events = vec![
            event(
                1,
                ActionPhase::Proposed,
                AgentAction::FileRead {
                    path: "a.rs".to_string(),
                },
            ),
            event(
                2,
                ActionPhase::Completed,
                AgentAction::ShellCommand {
                    executable: "npm".to_string(),
                    arguments: vec!["test".to_string()],
                    cwd: ".".to_string(),
                    exit_code: Some(0),
                    signal: None,
                },
            ),
        ];

        let contract = build_contract(
            &events,
            "sess-1",
            "sess-1.jsonl",
            "c1".to_string(),
            "2026-08-24T00:00:00Z".to_string(),
        );
        assert_eq!(contract.expected_trajectory.len(), 2);
        assert_eq!(contract.contract_id, "c1");
        assert_eq!(contract.source_session_id, "sess-1");
        assert_eq!(contract.assertion_file, "c1.assert.mjs");
    }

    #[test]
    fn build_contract_drops_session_lifecycle_events() {
        let events = vec![
            event(
                1,
                ActionPhase::Started,
                AgentAction::Session {
                    state: crate::events::SessionState::Opened,
                    exit_code: None,
                },
            ),
            event(
                2,
                ActionPhase::Completed,
                AgentAction::FileRead {
                    path: "a.rs".to_string(),
                },
            ),
            event(
                3,
                ActionPhase::Completed,
                AgentAction::Session {
                    state: crate::events::SessionState::Closed,
                    exit_code: Some(0),
                },
            ),
        ];

        let contract = build_contract(
            &events,
            "sess-1",
            "sess-1.jsonl",
            "c1".to_string(),
            "2026-08-24T00:00:00Z".to_string(),
        );
        assert_eq!(contract.expected_trajectory.len(), 1);
    }

    #[test]
    fn assertion_stub_embeds_the_contract_id_and_exports_assert() {
        let stub = assertion_stub("c1");
        assert!(stub.contains("c1"));
        assert!(stub.contains("export async function assert(context)"));
    }
}
