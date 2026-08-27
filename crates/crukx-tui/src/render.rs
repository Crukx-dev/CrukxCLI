//! Ports `crukx/packages/cli/src/tui/{Header,EventList,EventDetail,Footer,App}.tsx`
//! as ratatui draw functions. Snapshot-tested via `TestBackend` — same role
//! `ink-testing-library` plays for the TS components.

use crate::app::App;
use crate::format::{glyph_for, phase_color, summarize};
use crukx_core::events::{AgentAction, AgentEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(5),
            Constraint::Length(8),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_event_list(frame, chunks[1], app);
    draw_event_detail(frame, chunks[2], app.selected_event());
    draw_footer(frame, chunks[3]);
}

fn adapter_label(events: &[AgentEvent]) -> String {
    events
        .first()
        .map(|event| format!("{:?}", event.adapter).to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Difference between the first and last event's `occurred_at`, formatted
/// like the TS `Header`'s `durationLabel` (`"12.3s"`). `None` (rendered as
/// "n/a") when there are fewer than two events or a timestamp doesn't parse
/// — matches the TS fallback rather than panicking on malformed input.
fn duration_label(events: &[AgentEvent]) -> String {
    let (Some(first), Some(last)) = (events.first(), events.last()) else {
        return "n/a".to_string();
    };
    let (Ok(first_time), Ok(last_time)) = (
        chrono::DateTime::parse_from_rfc3339(&first.occurred_at),
        chrono::DateTime::parse_from_rfc3339(&last.occurred_at),
    ) else {
        return "n/a".to_string();
    };
    let seconds = (last_time - first_time).num_milliseconds() as f64 / 1000.0;
    format!("{seconds:.1}s")
}

fn closed_session_exit_code(events: &[AgentEvent]) -> Option<i32> {
    events.iter().find_map(|event| match &event.action {
        AgentAction::Session {
            state: crukx_core::events::SessionState::Closed,
            exit_code,
        } => *exit_code,
        _ => None,
    })
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let mut subtitle = format!(
        "adapter={} events={} duration={}",
        adapter_label(&app.events),
        app.events.len(),
        duration_label(&app.events)
    );
    if let Some(code) = closed_session_exit_code(&app.events) {
        subtitle.push_str(&format!(" exit={code}"));
    }
    if app.unreadable_count > 0 {
        subtitle.push_str(&format!(" ({} events unreadable)", app.unreadable_count));
    }

    let text = vec![
        Line::from(vec![
            Span::raw("Crukx review — "),
            Span::styled(app.session_id.clone(), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(Span::styled(
            subtitle,
            Style::default().add_modifier(Modifier::DIM),
        )),
    ];
    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn draw_event_list(frame: &mut Frame, area: Rect, app: &App) {
    let max_visible = area.height.saturating_sub(2) as usize; // minus the block's top/bottom border
    let block = Block::default().borders(Borders::ALL);

    if app.events.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "No events in this session.",
                Style::default().add_modifier(Modifier::DIM),
            ))
            .block(block),
            area,
        );
        return;
    }

    let range = app.visible_range(max_visible);
    let lines: Vec<Line> = app.events[range.clone()]
        .iter()
        .enumerate()
        .map(|(offset, event)| {
            let index = range.start + offset;
            let glyph = glyph_for(&event.action);
            let is_selected = index == app.selected_index;
            let base_style = if is_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Line::from(vec![
                Span::styled(glyph.icon, base_style.fg(glyph.color)),
                Span::styled(" ", base_style),
                Span::styled(
                    format!("{:<9}", format!("{:?}", event.phase).to_lowercase()),
                    base_style.fg(phase_color(event.phase)),
                ),
                Span::styled(
                    format!("{:<11} {}", glyph.label, summarize(&event.action)),
                    base_style,
                ),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_event_detail(frame: &mut Frame, area: Rect, event: Option<&AgentEvent>) {
    let block = Block::default().borders(Borders::ALL);
    let Some(event) = event else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "Select an event to see its full payload.",
                Style::default().add_modifier(Modifier::DIM),
            ))
            .block(block),
            area,
        );
        return;
    };

    let type_name = match &event.action {
        AgentAction::FileRead { .. } => "file_read",
        AgentAction::FileWrite { .. } => "file_write",
        AgentAction::ShellCommand { .. } => "shell_command",
        AgentAction::DependencyInstall { .. } => "dependency_install",
        AgentAction::NetworkRequest { .. } => "network_request",
        AgentAction::GitDiff { .. } => "git_diff",
        AgentAction::GitCommit { .. } => "git_commit",
        AgentAction::GitPush { .. } => "git_push",
        AgentAction::InfrastructureChange { .. } => "infrastructure_change",
        AgentAction::Session { .. } => "session",
    };

    let mut lines = vec![
        Line::from(Span::styled(
            type_name,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(
                "occurred_at={} sequence={}",
                event.occurred_at, event.sequence
            ),
            Style::default().add_modifier(Modifier::DIM),
        )),
    ];

    match &event.action {
        AgentAction::ShellCommand {
            executable,
            arguments,
            exit_code,
            ..
        } => {
            let mut line = std::iter::once(executable.clone())
                .chain(arguments.iter().cloned())
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(code) = exit_code {
                line.push_str(&format!(" (exit {code})"));
            }
            lines.push(Line::from(line));
        }
        AgentAction::GitDiff { patch, .. } => {
            let preview: String = patch.chars().take(400).collect();
            let suffix = if patch.chars().count() > 400 {
                "\n…"
            } else {
                ""
            };
            lines.push(Line::from(format!("{preview}{suffix}")));
        }
        AgentAction::FileWrite {
            operation, path, ..
        } => {
            lines.push(Line::from(format!("{operation:?} {path}").to_lowercase()));
        }
        _ => {}
    }

    if let Some(error) = &event.error {
        lines.push(Line::from(Span::styled(
            format!("error: {error}"),
            Style::default().fg(Color::Red),
        )));
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(Span::styled(
            "↑/↓ or j/k navigate · q or Ctrl+C quit",
            Style::default().add_modifier(Modifier::DIM),
        )),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crukx_core::events::{ActionPhase, AgentAdapter, EVENT_SCHEMA_VERSION};
    use ratatui::backend::{Backend, TestBackend};
    use ratatui::Terminal;

    fn event(action: AgentAction) -> AgentEvent {
        AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            event_id: "evt-1".to_string(),
            session_id: "sess-1".to_string(),
            sequence: 1,
            occurred_at: "2026-08-24T00:00:00Z".to_string(),
            adapter: AgentAdapter::Direct,
            repository_root: "/repo".to_string(),
            phase: ActionPhase::Completed,
            action,
            policy: None,
            error: None,
        }
    }

    fn buffer_text(backend: &TestBackend) -> String {
        backend
            .buffer()
            .content()
            .chunks(backend.size().unwrap().width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn draws_without_panicking_for_an_empty_session() {
        let app = App::new("sess-1".to_string(), vec![], 0);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("No events in this session."));
    }

    #[test]
    fn header_shows_session_id_and_event_count() {
        let app = App::new(
            "sess-1".to_string(),
            vec![event(AgentAction::FileRead {
                path: "a.rs".to_string(),
            })],
            0,
        );
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("sess-1"), "{text}");
        assert!(text.contains("events=1"), "{text}");
    }

    #[test]
    fn header_reports_unreadable_count_when_nonzero() {
        let app = App::new(
            "sess-1".to_string(),
            vec![event(AgentAction::FileRead {
                path: "a.rs".to_string(),
            })],
            2,
        );
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("2 events unreadable"), "{text}");
    }

    #[test]
    fn event_list_shows_the_shell_command_summary() {
        let app = App::new(
            "sess-1".to_string(),
            vec![event(AgentAction::ShellCommand {
                executable: "npm".to_string(),
                arguments: vec!["test".to_string()],
                cwd: ".".to_string(),
                exit_code: Some(0),
                signal: None,
            })],
            0,
        );
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("npm test"), "{text}");
    }

    #[test]
    fn event_detail_shows_placeholder_when_session_is_empty() {
        let app = App::new("sess-1".to_string(), vec![], 0);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(
            text.contains("Select an event to see its full payload."),
            "{text}"
        );
    }

    #[test]
    fn footer_shows_keybindings() {
        let app = App::new("sess-1".to_string(), vec![], 0);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let text = buffer_text(terminal.backend());
        assert!(text.contains("navigate"), "{text}");
        assert!(text.contains("quit"), "{text}");
    }

    #[test]
    fn duration_label_reports_seconds_between_first_and_last_event() {
        let events = vec![
            {
                let mut e = event(AgentAction::FileRead {
                    path: "a".to_string(),
                });
                e.occurred_at = "2026-08-24T00:00:00.000Z".to_string();
                e
            },
            {
                let mut e = event(AgentAction::FileRead {
                    path: "b".to_string(),
                });
                e.occurred_at = "2026-08-24T00:00:02.500Z".to_string();
                e
            },
        ];
        assert_eq!(duration_label(&events), "2.5s");
    }

    #[test]
    fn duration_label_is_n_a_with_no_events() {
        assert_eq!(duration_label(&[]), "n/a");
    }

    #[test]
    fn duration_label_is_zero_for_a_single_event() {
        // first and last are the same element with one event -> 0.0s, not
        // "n/a" — matches the TS Header, which has the same behavior for
        // the same reason (events[0] and events[length-1] are identical).
        assert_eq!(
            duration_label(&[event(AgentAction::FileRead {
                path: "a".to_string()
            })]),
            "0.0s"
        );
    }
}
