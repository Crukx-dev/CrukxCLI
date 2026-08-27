//! crukx-tui — Ratatui regression explorer. Ports
//! `crukx/packages/cli/src/tui/*.tsx` (Ink) to ratatui. Depends only on
//! `crukx-core`'s data types (`AgentEvent`) — it renders what it's given,
//! it doesn't know how a session was captured or replayed.
//!
//! `app`/`format` are pure and unit-tested without a terminal. `render`'s
//! draw functions are snapshot-tested via ratatui's `TestBackend`. `run`
//! (the crossterm event loop below) is the one part that genuinely needs a
//! real terminal and isn't unit-tested — same as the TS `App.tsx`, whose
//! only test is a render-without-throwing smoke test, not real keyboard
//! interaction.

pub mod app;
pub mod format;
pub mod render;

pub use app::App;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crukx_core::events::AgentEvent;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;

/// Runs the interactive review TUI until the user quits (`q` or Ctrl+C).
pub fn run(session_id: String, events: Vec<AgentEvent>, unreadable_count: usize) -> io::Result<()> {
    let mut app = App::new(session_id, events, unreadable_count);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn event_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|frame| render::draw(frame, app))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('q') => break,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
            KeyCode::Down | KeyCode::Char('j') => app.move_down(),
            KeyCode::Up | KeyCode::Char('k') => app.move_up(),
            _ => {}
        }
    }
    Ok(())
}
