use serde::{Deserialize, Serialize};

pub const EVENT_SCHEMA_VERSION: u32 = 1;

/// Which harness produced this event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAdapter {
    Claude,
    Codex,
    Direct,
    System,
}

/// Where in an action's lifecycle this event was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionPhase {
    Proposed,
    Started,
    Completed,
    Failed,
}

/// What Crukx's policy layer decided about the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    Allow,
    Ask,
    Block,
    Isolate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileWriteOperation {
    Create,
    Modify,
    Delete,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyEcosystem {
    Npm,
    Pypi,
    Cargo,
    Go,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Opened,
    Closed,
}

/// One recorded agent action. Discriminated by `type` (matches the TS
/// discriminated union's tag field name, kept for readability even though
/// schema v2 otherwise redesigns freely).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentAction {
    FileRead {
        path: String,
    },
    FileWrite {
        path: String,
        operation: FileWriteOperation,
        #[serde(skip_serializing_if = "Option::is_none")]
        previous_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content_sha256: Option<String>,
    },
    ShellCommand {
        executable: String,
        arguments: Vec<String>,
        cwd: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        signal: Option<String>,
    },
    DependencyInstall {
        ecosystem: DependencyEcosystem,
        packages: Vec<String>,
        command: String,
    },
    NetworkRequest {
        #[serde(skip_serializing_if = "Option::is_none")]
        method: Option<String>,
        hostname: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        port: Option<u16>,
    },
    GitDiff {
        base: String,
        patch: String,
        patch_sha256: String,
    },
    /// Schema v2: was `GitRefAction { type: "git_commit" }` in TS. Split so
    /// each variant only carries fields that apply to it.
    GitCommit {
        #[serde(skip_serializing_if = "Option::is_none")]
        commit_ref: Option<String>,
    },
    /// Schema v2: was `GitRefAction { type: "git_push" }` in TS.
    GitPush {
        #[serde(skip_serializing_if = "Option::is_none")]
        push_ref: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        remote: Option<String>,
    },
    InfrastructureChange {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
    },
    Session {
        state: SessionState,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EventPolicy {
    pub decision: PolicyDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One line of a session's event log. `sequence` orders events within a
/// session; `schema_version` lets a future reader detect an old-format file
/// instead of guessing from field presence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentEvent {
    pub schema_version: u32,
    pub event_id: String,
    pub session_id: String,
    pub sequence: u64,
    pub occurred_at: String,
    pub adapter: AgentAdapter,
    pub repository_root: String,
    pub phase: ActionPhase,
    pub action: AgentAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<EventPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What a caller supplies to record a new event; `schema_version`,
/// `event_id` and `occurred_at` are assigned by the event store, not by
/// whatever's constructing this.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EventInput {
    pub session_id: String,
    pub sequence: u64,
    pub adapter: AgentAdapter,
    pub repository_root: String,
    pub phase: ActionPhase,
    pub action: AgentAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<EventPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_command_action_round_trips_through_json_with_snake_case_tag() {
        let event = AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            event_id: "evt-1".to_string(),
            session_id: "sess-1".to_string(),
            sequence: 1,
            occurred_at: "2026-08-21T00:00:00Z".to_string(),
            adapter: AgentAdapter::Claude,
            repository_root: "/repo".to_string(),
            phase: ActionPhase::Completed,
            action: AgentAction::ShellCommand {
                executable: "npm".to_string(),
                arguments: vec!["test".to_string()],
                cwd: ".".to_string(),
                exit_code: Some(0),
                signal: None,
            },
            policy: None,
            error: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"shell_command\""));
        assert!(json.contains("\"adapter\":\"claude\""));

        let round_tripped: AgentEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped.event_id, "evt-1");
        match round_tripped.action {
            AgentAction::ShellCommand { executable, .. } => assert_eq!(executable, "npm"),
            other => panic!("expected ShellCommand, got {other:?}"),
        }
    }

    #[test]
    fn git_commit_and_git_push_are_distinct_variants() {
        // TS modeled these as one GitRefAction with type: "git_commit"|"git_push".
        // Schema v2 redesign: split into two proper enum variants.
        let commit = AgentAction::GitCommit {
            commit_ref: Some("abc123".to_string()),
        };
        let push = AgentAction::GitPush {
            push_ref: Some("main".to_string()),
            remote: Some("origin".to_string()),
        };
        assert!(matches!(commit, AgentAction::GitCommit { .. }));
        assert!(matches!(push, AgentAction::GitPush { .. }));
    }
}
