use crate::json::{read_json, write_json, StorageError};
use crate::state_dir::StateDir;
use crukx_core::contracts::RegressionContract;
use crukx_core::security::is_safe_path_segment;
use std::fs;
use std::io;
use std::path::PathBuf;

fn reject_unsafe_segment(label: &str, s: &str) -> Result<(), StorageError> {
    if is_safe_path_segment(s) {
        Ok(())
    } else {
        Err(StorageError::from(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{label} {s:?} is not a safe path segment (no \"/\", \"..\", or absolute path)"
            ),
        )))
    }
}

pub fn read_contract(
    state: &StateDir,
    contract_id: &str,
) -> Result<RegressionContract, StorageError> {
    reject_unsafe_segment("contract id", contract_id)?;
    let path = state.contracts_dir().join(format!("{contract_id}.json"));
    read_json(&path)
}

pub fn write_contract(
    state: &StateDir,
    contract: &RegressionContract,
) -> Result<PathBuf, StorageError> {
    reject_unsafe_segment("contract id", &contract.contract_id)?;
    let path = state
        .contracts_dir()
        .join(format!("{}.json", contract.contract_id));
    write_json(&path, contract)?;
    Ok(path)
}

/// Never overwrites an existing assertion file — that's where the user's
/// hand-written verification logic lives once they've filled in the stub.
/// Returns the path either way, so a caller always knows where to point
/// the user regardless of whether this call wrote anything.
pub fn write_assertion_stub_if_missing(
    state: &StateDir,
    file_name: &str,
    stub_content: &str,
) -> Result<PathBuf, StorageError> {
    reject_unsafe_segment("assertion file name", file_name)?;
    let path = state.contracts_dir().join(file_name);
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, stub_content)?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::write_json;

    fn contract() -> RegressionContract {
        RegressionContract {
            schema_version: crukx_core::contracts::CONTRACT_SCHEMA_VERSION,
            contract_id: "demo-1".to_string(),
            created_at: "2026-08-22T00:00:00Z".to_string(),
            source_session_id: "sess-1".to_string(),
            source_event_file: "sess-1.jsonl".to_string(),
            expected_trajectory: vec![],
            assertion_file: "assert.mjs".to_string(),
        }
    }

    #[test]
    fn read_contract_reads_the_file_named_after_the_contract_id() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        write_json(&state.contracts_dir().join("demo-1.json"), &contract()).unwrap();

        let read_back = read_contract(&state, "demo-1").unwrap();
        assert_eq!(read_back.contract_id, "demo-1");
    }

    #[test]
    fn read_contract_refuses_a_traversal_id_instead_of_escaping_contracts_dir() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        // Plant a real file outside .crukx/contracts/ that a traversal id
        // would resolve to, so a passing test here would mean the escape
        // actually worked, not just that a lookup failed for some other reason.
        std::fs::write(dir.path().join("secret.json"), "{\"leaked\":true}").unwrap();

        let result = read_contract(&state, "../secret");
        assert!(result.is_err());
    }

    #[test]
    fn write_contract_refuses_a_traversal_id() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        let mut evil = contract();
        evil.contract_id = "../../escaped".to_string();

        assert!(write_contract(&state, &evil).is_err());
        assert!(!dir.path().join("../escaped.json").exists());
    }

    #[test]
    fn write_assertion_stub_if_missing_refuses_a_traversal_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        assert!(write_assertion_stub_if_missing(&state, "../../evil.mjs", "// x").is_err());
    }

    #[test]
    fn read_contract_errors_when_no_file_matches_the_id() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        assert!(read_contract(&state, "missing").is_err());
    }

    #[test]
    fn write_contract_then_read_contract_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        write_contract(&state, &contract()).unwrap();
        let read_back = read_contract(&state, "demo-1").unwrap();
        assert_eq!(read_back.contract_id, "demo-1");
    }

    #[test]
    fn write_assertion_stub_if_missing_writes_a_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        let path = write_assertion_stub_if_missing(&state, "c1.assert.mjs", "// stub").unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "// stub");
    }

    #[test]
    fn write_assertion_stub_if_missing_never_overwrites_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        std::fs::create_dir_all(state.contracts_dir()).unwrap();
        std::fs::write(
            state.contracts_dir().join("c1.assert.mjs"),
            "// hand-written",
        )
        .unwrap();

        let path = write_assertion_stub_if_missing(&state, "c1.assert.mjs", "// stub").unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "// hand-written");
    }
}
