use crukx_core::contracts::capture::{assertion_stub, build_contract};
use crukx_storage::contracts::{write_assertion_stub_if_missing, write_contract};
use crukx_storage::events::load_session;
use crukx_storage::state_dir::StateDir;
use std::path::Path;

/// Entry point for `crukx capture [session-id]`.
pub fn run_capture(session_id: &str, cwd: &Path) -> i32 {
    let state = StateDir::resolve(cwd);

    let session = match load_session(&state, session_id) {
        Ok(session) => session,
        Err(err) => {
            eprintln!("Crukx capture: {err}");
            return 1;
        }
    };

    let contract_id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    let contract = build_contract(
        &session.events,
        &session.session_id,
        &session.file_path.to_string_lossy(),
        contract_id,
        created_at,
    );

    let contract_path = match write_contract(&state, &contract) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("Crukx capture: failed to write contract: {err}");
            return 1;
        }
    };
    let assertion_path = match write_assertion_stub_if_missing(
        &state,
        &contract.assertion_file,
        &assertion_stub(&contract.contract_id),
    ) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("Crukx capture: failed to write assertion stub: {err}");
            return 1;
        }
    };

    eprintln!(
        "Contract {} captured from session {}",
        contract.contract_id, contract.source_session_id
    );
    eprintln!("  {} expected steps", contract.expected_trajectory.len());
    eprintln!("  Contract:  {}", contract_path.display());
    eprintln!("  Assertion: {}", assertion_path.display());
    eprintln!("Edit the assertion file to define what \"correct\" means for this workflow.");
    0
}
