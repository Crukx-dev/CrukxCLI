use crate::json::{read_json_if_exists, StorageError};
use crate::state_dir::StateDir;
use crukx_core::security::Finding;

/// `None` when no scan was ever recorded for this run — a caller treats
/// that as "security delta not tracked", not an error.
pub fn read_security_before(state: &StateDir) -> Result<Option<Vec<Finding>>, StorageError> {
    read_json_if_exists(&state.security_dir().join("before.json"))
}

pub fn read_security_after(state: &StateDir) -> Result<Option<Vec<Finding>>, StorageError> {
    read_json_if_exists(&state.security_dir().join("after.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::write_json;
    use crukx_core::security::Severity;

    #[test]
    fn read_security_before_and_after_round_trip_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        let findings = vec![Finding {
            id: "f1".to_string(),
            severity: Severity::Critical,
            cwe: None,
            location: None,
        }];
        write_json(
            &state.security_dir().join("before.json"),
            &Vec::<Finding>::new(),
        )
        .unwrap();
        write_json(&state.security_dir().join("after.json"), &findings).unwrap();

        let before = read_security_before(&state).unwrap();
        let after = read_security_after(&state).unwrap();
        assert_eq!(before, Some(vec![]));
        assert_eq!(after.unwrap().len(), 1);
    }

    #[test]
    fn read_security_before_returns_none_when_not_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        assert!(read_security_before(&state).unwrap().is_none());
    }
}
