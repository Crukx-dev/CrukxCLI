//! Ports `crukx/packages/cli/src/tui/format.ts` — per-action-type glyph,
//! phase color, and one-line summary. Pure, no ratatui types, so this
//! stays testable without a terminal.

use crukx_core::events::{ActionPhase, AgentAction};
use ratatui::style::Color;

pub struct ActionGlyph {
    pub icon: &'static str,
    pub color: Color,
    pub label: &'static str,
}

pub fn glyph_for(action: &AgentAction) -> ActionGlyph {
    match action {
        AgentAction::FileRead { .. } => ActionGlyph {
            icon: "◇",
            color: Color::Cyan,
            label: "file read",
        },
        AgentAction::FileWrite { .. } => ActionGlyph {
            icon: "✎",
            color: Color::Yellow,
            label: "file write",
        },
        AgentAction::ShellCommand { .. } => ActionGlyph {
            icon: "$",
            color: Color::Magenta,
            label: "shell",
        },
        AgentAction::DependencyInstall { .. } => ActionGlyph {
            icon: "▤",
            color: Color::Blue,
            label: "dependency",
        },
        AgentAction::NetworkRequest { .. } => ActionGlyph {
            icon: "→",
            color: Color::Cyan,
            label: "network",
        },
        AgentAction::GitDiff { .. } => ActionGlyph {
            icon: "±",
            color: Color::Green,
            label: "git diff",
        },
        AgentAction::GitCommit { .. } => ActionGlyph {
            icon: "●",
            color: Color::Green,
            label: "git commit",
        },
        AgentAction::GitPush { .. } => ActionGlyph {
            icon: "↑",
            color: Color::Green,
            label: "git push",
        },
        AgentAction::InfrastructureChange { .. } => ActionGlyph {
            icon: "▲",
            color: Color::Red,
            label: "infra",
        },
        AgentAction::Session { .. } => ActionGlyph {
            icon: "◆",
            color: Color::White,
            label: "session",
        },
    }
}

pub fn phase_color(phase: ActionPhase) -> Color {
    match phase {
        ActionPhase::Proposed => Color::Gray,
        ActionPhase::Started => Color::Yellow,
        ActionPhase::Completed => Color::Green,
        ActionPhase::Failed => Color::Red,
    }
}

pub fn summarize(action: &AgentAction) -> String {
    match action {
        AgentAction::FileRead { path } => path.clone(),
        AgentAction::FileWrite { path, .. } => path.clone(),
        AgentAction::ShellCommand {
            executable,
            arguments,
            ..
        } => std::iter::once(executable.clone())
            .chain(arguments.iter().cloned())
            .collect::<Vec<_>>()
            .join(" "),
        AgentAction::DependencyInstall {
            ecosystem,
            packages,
            ..
        } => {
            format!("{ecosystem:?}: {}", packages.join(", "))
        }
        AgentAction::NetworkRequest {
            method,
            hostname,
            port,
        } => {
            let method = method.as_deref().unwrap_or("?");
            match port {
                Some(port) => format!("{method} {hostname}:{port}"),
                None => format!("{method} {hostname}"),
            }
        }
        AgentAction::GitDiff { patch, .. } => {
            format!("{} lines changed", patch.split('\n').count())
        }
        AgentAction::GitCommit { commit_ref } => commit_ref.clone().unwrap_or_default(),
        AgentAction::GitPush { push_ref, remote } => push_ref
            .clone()
            .or_else(|| remote.clone())
            .unwrap_or_default(),
        AgentAction::InfrastructureChange { path, .. } => path.clone(),
        AgentAction::Session { state, .. } => format!("{state:?}").to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_shell_command_joins_executable_and_arguments() {
        let action = AgentAction::ShellCommand {
            executable: "npm".to_string(),
            arguments: vec!["test".to_string(), "--ci".to_string()],
            cwd: ".".to_string(),
            exit_code: None,
            signal: None,
        };
        assert_eq!(summarize(&action), "npm test --ci");
    }

    #[test]
    fn summarize_network_request_includes_port_only_when_present() {
        let with_port = AgentAction::NetworkRequest {
            method: Some("GET".to_string()),
            hostname: "api.example.com".to_string(),
            port: Some(443),
        };
        assert_eq!(summarize(&with_port), "GET api.example.com:443");

        let without_port = AgentAction::NetworkRequest {
            method: None,
            hostname: "api.example.com".to_string(),
            port: None,
        };
        assert_eq!(summarize(&without_port), "? api.example.com");
    }

    #[test]
    fn summarize_git_diff_counts_lines() {
        let action = AgentAction::GitDiff {
            base: "abc".to_string(),
            patch: "line1\nline2\nline3".to_string(),
            patch_sha256: "x".to_string(),
        };
        assert_eq!(summarize(&action), "3 lines changed");
    }

    #[test]
    fn glyph_for_is_distinct_per_action_type() {
        let file_read = glyph_for(&AgentAction::FileRead {
            path: "x".to_string(),
        });
        let shell = glyph_for(&AgentAction::ShellCommand {
            executable: "x".to_string(),
            arguments: vec![],
            cwd: ".".to_string(),
            exit_code: None,
            signal: None,
        });
        assert_ne!(file_read.icon, shell.icon);
        assert_ne!(file_read.label, shell.label);
    }

    #[test]
    fn phase_color_covers_every_phase() {
        assert_eq!(phase_color(ActionPhase::Proposed), Color::Gray);
        assert_eq!(phase_color(ActionPhase::Started), Color::Yellow);
        assert_eq!(phase_color(ActionPhase::Completed), Color::Green);
        assert_eq!(phase_color(ActionPhase::Failed), Color::Red);
    }
}
