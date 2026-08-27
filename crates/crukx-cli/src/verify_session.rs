use crukx_storage::events::{verify_session_integrity, IntegrityError};
use crukx_storage::state_dir::StateDir;
use std::path::Path;

/// Entry point for `crukx verify-session <session-id>`. Recomputes the
/// session's hash chain from its event log and compares it to what
/// `crukx run` recorded when the session finished — catches any edit,
/// deletion, or reordering of the log since then.
pub fn run_verify_session(session_id: &str, cwd: &Path) -> i32 {
    let state = StateDir::resolve(cwd);

    match verify_session_integrity(&state, session_id) {
        Ok(()) => {
            eprintln!("Crukx verify-session: {session_id} — history intact");
            0
        }
        Err(IntegrityError::Tampered(violation)) => {
            eprintln!(
                "Crukx verify-session: {session_id} — TAMPERED (diverges from the recorded chain at event index {})",
                violation.first_divergent_index
            );
            1
        }
        Err(err) => {
            eprintln!("Crukx verify-session: {err}");
            1
        }
    }
}
