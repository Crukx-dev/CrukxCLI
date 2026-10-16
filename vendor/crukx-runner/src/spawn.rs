use crate::policy::{evaluate_policy, ExecutionPolicy, NetworkPolicy, PolicyViolation};
use crukx_core::events::AgentAction;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

/// One request to run an executable. `action` is the `AgentAction` this
/// spawn fulfills — checked against `policy` before anything runs.
pub struct SpawnRequest {
    pub executable: String,
    pub arguments: Vec<String>,
    pub cwd: PathBuf,
    pub policy: ExecutionPolicy,
    pub action: AgentAction,
}

#[derive(Debug)]
pub enum SpawnError {
    PolicyDenied(PolicyViolation),
    Io(std::io::Error),
}

#[derive(Debug, Clone, Copy)]
pub struct SpawnOutcome {
    pub exit_code: i32,
    /// Real wall-clock time the child process took, measured around the
    /// actual `Command::status()`/`.output()` call — not an estimate.
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct SpawnCaptureOutcome {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// Policy check + command construction shared by `spawn` and
/// `spawn_capture` — the only difference between them is `.status()` vs
/// `.output()`, everything before that must stay identical so capturing
/// output can never become a way to bypass a check `spawn` enforces.
fn prepare_command(request: &SpawnRequest) -> Result<Command, SpawnError> {
    evaluate_policy(&request.action, &request.policy).map_err(SpawnError::PolicyDenied)?;

    // The policy check above only inspects `request.action` — nothing
    // stops a caller from labeling a destructive command as, say,
    // `FileRead` and getting it waved through. For shell_command actions
    // specifically we can verify the label matches what's actually about
    // to run; refuse on any mismatch instead of trusting the label.
    if let AgentAction::ShellCommand {
        executable,
        arguments,
        cwd,
        ..
    } = &request.action
    {
        if executable != &request.executable
            || arguments != &request.arguments
            || cwd.as_str() != request.cwd.to_string_lossy()
        {
            return Err(SpawnError::PolicyDenied(PolicyViolation {
                reason: format!(
                    "spawn request ({} {:?} in {}) does not match its declared shell_command action ({} {:?} in {}) — refusing",
                    request.executable, request.arguments, request.cwd.display(), executable, arguments, cwd
                ),
            }));
        }
    }

    let mut command = Command::new(&request.executable);
    command.args(&request.arguments).current_dir(&request.cwd);

    if should_strip_proxy_env(&request.policy) {
        for var in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            command.env_remove(var);
        }
    }

    Ok(command)
}

/// Single choke point every process spawn in crukx-core routes through
/// (replay's shell steps, `crukx run`/`agent` capture — wired in a later
/// plan). Checks the policy first; on denial, nothing is spawned.
pub fn spawn(request: SpawnRequest) -> Result<SpawnOutcome, SpawnError> {
    let mut command = prepare_command(&request)?;
    let started = Instant::now();
    let status = command.status().map_err(SpawnError::Io)?;
    Ok(SpawnOutcome {
        exit_code: status.code().unwrap_or(1),
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

/// Same policy enforcement as `spawn`, but captures stdout/stderr instead
/// of inheriting the parent's — needed for anything whose result is its
/// output (the assertion runner, in particular), not just its exit code.
pub fn spawn_capture(request: SpawnRequest) -> Result<SpawnCaptureOutcome, SpawnError> {
    let mut command = prepare_command(&request)?;
    let started = Instant::now();
    let output = command.output().map_err(SpawnError::Io)?;
    Ok(SpawnCaptureOutcome {
        exit_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

/// Neither `Deny` nor `AllowList` can be enforced by env-var stripping
/// alone — an allow-listed hostname isn't something an env var can
/// express, so a shell command running under a network `AllowList` gets
/// exactly the same "can't statically confine it" treatment as `Deny`.
/// Only `Allow` is an actual promise not to interfere. Real per-host
/// confinement needs the deferred crukx-sandbox crate.
fn should_strip_proxy_env(policy: &ExecutionPolicy) -> bool {
    matches!(
        policy,
        ExecutionPolicy::WorkspaceWrite {
            network: NetworkPolicy::Deny
        } | ExecutionPolicy::WorkspaceWrite {
            network: NetworkPolicy::AllowList(_)
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_read_action() -> AgentAction {
        AgentAction::FileRead {
            path: ".".to_string(),
        }
    }

    fn file_write_action() -> AgentAction {
        AgentAction::FileWrite {
            path: "x".to_string(),
            operation: crukx_core::events::FileWriteOperation::Create,
            previous_path: None,
            content_sha256: None,
        }
    }

    #[test]
    fn spawn_runs_the_process_when_the_policy_allows_the_action() {
        let outcome = spawn(SpawnRequest {
            executable: "true".to_string(),
            arguments: vec![],
            cwd: std::env::current_dir().unwrap(),
            policy: ExecutionPolicy::ReadOnly,
            action: file_read_action(),
        })
        .expect("spawn should succeed");
        assert_eq!(outcome.exit_code, 0);
    }

    #[test]
    fn spawn_refuses_without_running_when_the_policy_denies_the_action() {
        let result = spawn(SpawnRequest {
            executable: "true".to_string(),
            arguments: vec![],
            cwd: std::env::current_dir().unwrap(),
            policy: ExecutionPolicy::ReadOnly,
            action: file_write_action(),
        });
        assert!(matches!(result, Err(SpawnError::PolicyDenied(_))));
    }

    #[test]
    fn spawn_refuses_when_the_shell_command_action_does_not_match_what_would_actually_run() {
        let action = AgentAction::ShellCommand {
            executable: "true".to_string(),
            arguments: vec![],
            cwd: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            exit_code: None,
            signal: None,
        };
        // The declared action says `true`; the actual spawn request says `rm -rf /`.
        // A caller mislabeling the action must not slip past the policy check.
        let result = spawn(SpawnRequest {
            executable: "rm".to_string(),
            arguments: vec!["-rf".to_string(), "/".to_string()],
            cwd: std::env::current_dir().unwrap(),
            policy: ExecutionPolicy::DangerFullAccess,
            action,
        });
        assert!(matches!(result, Err(SpawnError::PolicyDenied(_))));
    }

    #[test]
    fn should_strip_proxy_env_is_true_for_deny_and_allow_list_but_not_allow() {
        assert!(should_strip_proxy_env(&ExecutionPolicy::WorkspaceWrite {
            network: NetworkPolicy::Deny
        }));
        assert!(should_strip_proxy_env(&ExecutionPolicy::WorkspaceWrite {
            network: NetworkPolicy::AllowList(vec!["api.example.com".to_string()])
        }));
        assert!(!should_strip_proxy_env(&ExecutionPolicy::WorkspaceWrite {
            network: NetworkPolicy::Allow
        }));
        assert!(!should_strip_proxy_env(&ExecutionPolicy::ReadOnly));
    }

    #[test]
    fn spawn_capture_returns_stdout() {
        let outcome = spawn_capture(SpawnRequest {
            executable: "echo".to_string(),
            arguments: vec!["-n".to_string(), "hello".to_string()],
            cwd: std::env::current_dir().unwrap(),
            policy: ExecutionPolicy::DangerFullAccess,
            action: file_read_action(),
        })
        .expect("spawn_capture should succeed");
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.stdout, "hello");
    }

    #[test]
    fn spawn_capture_enforces_the_same_policy_as_spawn() {
        let result = spawn_capture(SpawnRequest {
            executable: "echo".to_string(),
            arguments: vec!["hi".to_string()],
            cwd: std::env::current_dir().unwrap(),
            policy: ExecutionPolicy::ReadOnly,
            action: file_write_action(),
        });
        assert!(matches!(result, Err(SpawnError::PolicyDenied(_))));
    }

    #[test]
    fn spawn_measures_real_wall_clock_duration() {
        let outcome = spawn(SpawnRequest {
            executable: "sleep".to_string(),
            arguments: vec!["0.2".to_string()],
            cwd: std::env::current_dir().unwrap(),
            policy: ExecutionPolicy::DangerFullAccess,
            action: file_read_action(),
        })
        .expect("spawn should succeed");
        // Real measurement, not a fixed/faked value — allow scheduler slop
        // but a 200ms sleep must take meaningfully longer than a no-op.
        assert!(
            outcome.duration_ms >= 150,
            "expected >= 150ms, got {}",
            outcome.duration_ms
        );
    }

    #[test]
    fn spawn_propagates_a_nonzero_exit_code() {
        let outcome = spawn(SpawnRequest {
            executable: "false".to_string(),
            arguments: vec![],
            cwd: std::env::current_dir().unwrap(),
            policy: ExecutionPolicy::DangerFullAccess,
            action: file_read_action(),
        })
        .expect("spawn should succeed even though the process exits nonzero");
        assert_eq!(outcome.exit_code, 1);
    }
}
