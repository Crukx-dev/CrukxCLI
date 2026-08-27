use crukx_storage::events::load_session;
use crukx_storage::state_dir::StateDir;
use std::path::Path;

/// Entry point for `crukx review [session-id]` — launches the interactive
/// TUI. This is the one wired command that isn't safe to run under
/// `assert_cmd` in a test (it takes over the terminal and blocks on
/// keyboard input); `crukx-tui`'s pure app/format logic and its render
/// output are what's actually tested (see crates/crukx-tui/src).
pub fn run_review(session_id: &str, cwd: &Path) -> i32 {
    let state = StateDir::resolve(cwd);

    let session = match load_session(&state, session_id) {
        Ok(session) => session,
        Err(err) => {
            eprintln!("Crukx review: {err}");
            return 1;
        }
    };

    match crukx_tui::run(session.session_id, session.events, session.unreadable_count) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("Crukx review: {err}");
            1
        }
    }
}
