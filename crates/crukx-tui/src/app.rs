//! Ports `crukx/packages/cli/src/tui/App.tsx`'s state (selection index,
//! navigation clamping, visible-window scrolling) as plain data + pure
//! functions — no crossterm/ratatui types here, so navigation logic is
//! testable without a terminal.

use crukx_core::events::AgentEvent;

pub struct App {
    pub session_id: String,
    pub events: Vec<AgentEvent>,
    pub unreadable_count: usize,
    pub selected_index: usize,
}

impl App {
    pub fn new(session_id: String, events: Vec<AgentEvent>, unreadable_count: usize) -> Self {
        App {
            session_id,
            events,
            unreadable_count,
            selected_index: 0,
        }
    }

    pub fn move_down(&mut self) {
        if self.events.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1).min(self.events.len() - 1);
    }

    pub fn move_up(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn selected_event(&self) -> Option<&AgentEvent> {
        self.events.get(self.selected_index)
    }

    /// The `max_visible`-sized window of events to render, centered on the
    /// current selection where possible — same clamping the TS `EventList`
    /// does: never scroll past either end of the list.
    pub fn visible_range(&self, max_visible: usize) -> std::ops::Range<usize> {
        if self.events.is_empty() || max_visible == 0 {
            return 0..0;
        }
        let max_visible = max_visible.min(self.events.len());
        let ideal_start = self.selected_index as isize - (max_visible / 2) as isize;
        let max_start = (self.events.len() - max_visible) as isize;
        let start = ideal_start.clamp(0, max_start.max(0)) as usize;
        start..(start + max_visible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crukx_core::events::{ActionPhase, AgentAction, AgentAdapter, EVENT_SCHEMA_VERSION};

    fn events(count: usize) -> Vec<AgentEvent> {
        (0..count)
            .map(|i| AgentEvent {
                schema_version: EVENT_SCHEMA_VERSION,
                event_id: format!("evt-{i}"),
                session_id: "sess-1".to_string(),
                sequence: i as u64,
                occurred_at: "2026-08-24T00:00:00Z".to_string(),
                adapter: AgentAdapter::Direct,
                repository_root: "/repo".to_string(),
                phase: ActionPhase::Completed,
                action: AgentAction::FileRead {
                    path: format!("file-{i}.rs"),
                },
                policy: None,
                error: None,
            })
            .collect()
    }

    #[test]
    fn move_down_stops_at_the_last_event() {
        let mut app = App::new("sess-1".to_string(), events(3), 0);
        app.move_down();
        app.move_down();
        app.move_down();
        app.move_down();
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn move_up_stops_at_zero() {
        let mut app = App::new("sess-1".to_string(), events(3), 0);
        app.move_up();
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn move_down_on_an_empty_session_does_not_panic() {
        let mut app = App::new("sess-1".to_string(), vec![], 0);
        app.move_down();
        assert_eq!(app.selected_index, 0);
        assert!(app.selected_event().is_none());
    }

    #[test]
    fn selected_event_returns_the_event_at_the_selected_index() {
        let mut app = App::new("sess-1".to_string(), events(3), 0);
        app.move_down();
        assert_eq!(app.selected_event().unwrap().event_id, "evt-1");
    }

    #[test]
    fn visible_range_never_exceeds_the_event_count() {
        let app = App::new("sess-1".to_string(), events(3), 0);
        let range = app.visible_range(10);
        assert_eq!(range, 0..3);
    }

    #[test]
    fn visible_range_centers_on_the_selection_when_possible() {
        let mut app = App::new("sess-1".to_string(), events(20), 0);
        for _ in 0..10 {
            app.move_down();
        }
        let range = app.visible_range(5);
        assert!(
            range.contains(&10),
            "range {range:?} should contain the selected index 10"
        );
        assert_eq!(range.len(), 5);
    }

    #[test]
    fn visible_range_does_not_scroll_past_the_end() {
        let mut app = App::new("sess-1".to_string(), events(10), 0);
        for _ in 0..9 {
            app.move_down();
        }
        let range = app.visible_range(5);
        assert_eq!(range, 5..10);
    }

    #[test]
    fn visible_range_on_an_empty_session_is_empty() {
        let app = App::new("sess-1".to_string(), vec![], 0);
        assert_eq!(app.visible_range(5), 0..0);
    }
}
