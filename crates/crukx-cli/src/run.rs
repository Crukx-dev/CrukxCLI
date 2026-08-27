use crukx_core::events::{
    ActionPhase, AgentAction, AgentAdapter, AgentEvent, SessionState, EVENT_SCHEMA_VERSION,
};
use crukx_runner::{spawn, ExecutionPolicy, SpawnRequest};
use crukx_storage::events::{append_event, session_file_path, write_chain_file};
use crukx_storage::state_dir::StateDir;
use std::path::{Path, PathBuf};

pub struct RunSessionResult {
    pub session_id: String,
    pub event_file: PathBuf,
    pub exit_code: i32,
}

/// Runs `executable arguments` under `cwd`, recording the whole thing as a
/// Crukx session: opened -> proposed shell_command -> started shell_command
/// -> completed/failed shell_command -> (git_diff, if the tracked tree
/// changed) -> closed. This is what `crukx capture` later turns into a
/// regression contract.
///
/// Not sandboxed — same as the TS CLI's `runSession`; a real
/// `ExecutionPolicy` for interactive `run`/`agent` sessions (as opposed to
/// `crukx gate`'s deterministic replay) is future work, not silently
/// pretended here.
pub fn run_session(
    adapter: AgentAdapter,
    executable: String,
    arguments: Vec<String>,
    cwd: &Path,
) -> RunSessionResult {
    let repository_root = crukx_git::resolve_repository_root(cwd);
    let state = StateDir::resolve(&repository_root);

    let adapter_label = match adapter {
        AgentAdapter::Claude => "claude",
        AgentAdapter::Codex => "codex",
        AgentAdapter::Direct => "direct",
        AgentAdapter::System => "system",
    };
    let timestamp = chrono::Utc::now()
        .format("%Y-%m-%dT%H-%M-%S%.3fZ")
        .to_string();
    let session_id = format!(
        "{adapter_label}-{timestamp}-{}",
        &uuid::Uuid::new_v4().to_string()[..8]
    );
    let cwd_string = cwd.to_string_lossy().into_owned();
    let repository_root_string = repository_root.to_string_lossy().into_owned();

    let mut sequence: u64 = 0;
    let mut record = |phase: ActionPhase, action: AgentAction, error: Option<String>| {
        sequence += 1;
        let event = AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            event_id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            sequence,
            occurred_at: chrono::Utc::now().to_rfc3339(),
            adapter,
            repository_root: repository_root_string.clone(),
            phase,
            action,
            policy: None,
            error,
        };
        // Best-effort: a failed event write shouldn't crash the session
        // the user is actually running.
        let _ = append_event(&state, &session_id, &event);
    };

    let initial_diff = crukx_git::read_tracked_diff(&repository_root);
    let base = crukx_git::read_head(&repository_root);

    let shell_action = || AgentAction::ShellCommand {
        executable: executable.clone(),
        arguments: arguments.clone(),
        cwd: cwd_string.clone(),
        exit_code: None,
        signal: None,
    };

    record(
        ActionPhase::Started,
        AgentAction::Session {
            state: SessionState::Opened,
            exit_code: None,
        },
        None,
    );
    record(ActionPhase::Proposed, shell_action(), None);
    record(ActionPhase::Started, shell_action(), None);

    let spawn_result = spawn(SpawnRequest {
        executable: executable.clone(),
        arguments: arguments.clone(),
        cwd: cwd.to_path_buf(),
        policy: ExecutionPolicy::DangerFullAccess,
        action: shell_action(),
    });

    let (exit_code, spawn_error) = match spawn_result {
        Ok(outcome) => (outcome.exit_code, None),
        Err(err) => (127, Some(format!("{err:?}"))),
    };

    record(
        if spawn_error.is_some() {
            ActionPhase::Failed
        } else {
            ActionPhase::Completed
        },
        AgentAction::ShellCommand {
            executable: executable.clone(),
            arguments: arguments.clone(),
            cwd: cwd_string.clone(),
            exit_code: Some(exit_code),
            signal: None,
        },
        spawn_error,
    );

    let final_diff = crukx_git::read_tracked_diff(&repository_root);
    if final_diff != initial_diff {
        record(
            ActionPhase::Completed,
            AgentAction::GitDiff {
                base: base.clone(),
                patch: final_diff.clone(),
                patch_sha256: crukx_git::sha256_hex(&final_diff),
            },
            None,
        );
    }

    record(
        ActionPhase::Completed,
        AgentAction::Session {
            state: SessionState::Closed,
            exit_code: Some(exit_code),
        },
        None,
    );

    // Recorded once, now that the session is complete — not on every
    // append_event, which would recompute the whole chain per event for
    // no benefit. Best-effort, same as append_event above: a session the
    // user actually ran should never fail because of a bookkeeping step
    // after the fact.
    let _ = write_chain_file(&state, &session_id);

    RunSessionResult {
        session_id: session_id.clone(),
        event_file: session_file_path(&state, &session_id),
        exit_code,
    }
}
