use crukx_core::events::AgentAction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkPolicy {
    Deny,
    AllowList(Vec<String>),
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionPolicy {
    ReadOnly,
    WorkspaceWrite { network: NetworkPolicy },
    DangerFullAccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyViolation {
    pub reason: String,
}

/// Pure decision: does `action` — the thing a spawn is fulfilling — pass
/// `policy`? `DangerFullAccess` always allows (opt-in escape hatch, name
/// says what it does). `ReadOnly` refuses any action that mutates state.
/// `WorkspaceWrite` allows mutation; its `network` sub-policy gates
/// `network_request` actions directly, but for `shell_command` actions it
/// can't statically know whether an arbitrary command will reach the
/// network — that's enforced by `spawn()` stripping proxy env vars, a
/// v1 signal, not a guarantee. Real OS-level network confinement is the
/// deferred `crukx-sandbox` crate (see
/// docs/superpowers/specs/2026-08-21-crukx-rs-design.md).
pub fn evaluate_policy(
    action: &AgentAction,
    policy: &ExecutionPolicy,
) -> Result<(), PolicyViolation> {
    match policy {
        ExecutionPolicy::DangerFullAccess => Ok(()),
        // Fail-closed by construction: this is an allow-list of the
        // actions read_only can verify are non-mutating, not a deny-list
        // with a catch-all. A raw shell_command can do anything a
        // file_write/dependency_install/etc. can — there's no way to
        // classify its effect from the action alone, so it's refused
        // alongside the actions we do know mutate state. Any AgentAction
        // variant added later defaults to denied here until explicitly
        // reasoned about, not silently allowed.
        ExecutionPolicy::ReadOnly => match action {
            AgentAction::FileRead { .. } | AgentAction::GitDiff { .. } | AgentAction::Session { .. } => Ok(()),
            AgentAction::FileWrite { path, .. } => Err(PolicyViolation {
                reason: format!("read_only policy refuses file_write to {path}"),
            }),
            AgentAction::ShellCommand { executable, .. } => Err(PolicyViolation {
                reason: format!(
                    "read_only policy refuses shell_command ({executable}) — its effect can't be classified from the action alone"
                ),
            }),
            AgentAction::DependencyInstall { command, .. } => Err(PolicyViolation {
                reason: format!("read_only policy refuses dependency_install ({command})"),
            }),
            AgentAction::NetworkRequest { hostname, .. } => Err(PolicyViolation {
                reason: format!("read_only policy refuses network_request to {hostname} — can't verify it's read-only"),
            }),
            AgentAction::GitCommit { .. } => {
                Err(PolicyViolation { reason: "read_only policy refuses git_commit".to_string() })
            }
            AgentAction::GitPush { .. } => {
                Err(PolicyViolation { reason: "read_only policy refuses git_push".to_string() })
            }
            AgentAction::InfrastructureChange { path, .. } => Err(PolicyViolation {
                reason: format!("read_only policy refuses infrastructure_change to {path}"),
            }),
        },
        ExecutionPolicy::WorkspaceWrite { network } => match action {
            AgentAction::NetworkRequest { hostname, .. } => match network {
                NetworkPolicy::Deny => Err(PolicyViolation {
                    reason: format!("network policy denies network_request to {hostname}"),
                }),
                NetworkPolicy::AllowList(allowed) => {
                    if allowed.iter().any(|allowed_host| allowed_host == hostname) {
                        Ok(())
                    } else {
                        Err(PolicyViolation {
                            reason: format!("network policy: {hostname} is not in the allow-list"),
                        })
                    }
                }
                NetworkPolicy::Allow => Ok(()),
            },
            _ => Ok(()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_write() -> AgentAction {
        AgentAction::FileWrite {
            path: "src/main.rs".to_string(),
            operation: crukx_core::events::FileWriteOperation::Modify,
            previous_path: None,
            content_sha256: None,
        }
    }

    fn file_read() -> AgentAction {
        AgentAction::FileRead {
            path: "src/main.rs".to_string(),
        }
    }

    fn network_request(hostname: &str) -> AgentAction {
        AgentAction::NetworkRequest {
            method: None,
            hostname: hostname.to_string(),
            port: None,
        }
    }

    fn shell_command() -> AgentAction {
        AgentAction::ShellCommand {
            executable: "rm".to_string(),
            arguments: vec!["-rf".to_string(), "/".to_string()],
            cwd: ".".to_string(),
            exit_code: None,
            signal: None,
        }
    }

    #[test]
    fn danger_full_access_allows_everything() {
        assert!(evaluate_policy(&file_write(), &ExecutionPolicy::DangerFullAccess).is_ok());
        assert!(evaluate_policy(
            &network_request("evil.example"),
            &ExecutionPolicy::DangerFullAccess
        )
        .is_ok());
    }

    #[test]
    fn read_only_refuses_file_write() {
        let result = evaluate_policy(&file_write(), &ExecutionPolicy::ReadOnly);
        assert!(result.is_err());
        assert!(result.unwrap_err().reason.contains("file_write"));
    }

    #[test]
    fn read_only_allows_file_read() {
        assert!(evaluate_policy(&file_read(), &ExecutionPolicy::ReadOnly).is_ok());
    }

    #[test]
    fn read_only_refuses_shell_command_since_its_effect_cannot_be_classified() {
        let result = evaluate_policy(&shell_command(), &ExecutionPolicy::ReadOnly);
        assert!(result.is_err());
        assert!(result.unwrap_err().reason.contains("shell_command"));
    }

    #[test]
    fn read_only_refuses_network_request() {
        assert!(evaluate_policy(
            &network_request("api.example.com"),
            &ExecutionPolicy::ReadOnly
        )
        .is_err());
    }

    #[test]
    fn workspace_write_with_network_deny_refuses_network_request() {
        let policy = ExecutionPolicy::WorkspaceWrite {
            network: NetworkPolicy::Deny,
        };
        let result = evaluate_policy(&network_request("api.example.com"), &policy);
        assert!(result.is_err());
    }

    #[test]
    fn workspace_write_with_network_deny_still_allows_file_write() {
        let policy = ExecutionPolicy::WorkspaceWrite {
            network: NetworkPolicy::Deny,
        };
        assert!(evaluate_policy(&file_write(), &policy).is_ok());
    }

    #[test]
    fn workspace_write_allow_list_permits_listed_host_only() {
        let policy = ExecutionPolicy::WorkspaceWrite {
            network: NetworkPolicy::AllowList(vec!["api.example.com".to_string()]),
        };
        assert!(evaluate_policy(&network_request("api.example.com"), &policy).is_ok());
        assert!(evaluate_policy(&network_request("evil.example"), &policy).is_err());
    }
}
