use crate::json::{read_json_if_exists, StorageError};
use crate::state_dir::StateDir;
use crukx_core::evaluators::JudgeRecord;

/// `None` when no baseline was ever recorded for this run — a caller
/// (`crukx-cli`'s reliability-contract wiring) treats that as "judge
/// drift not tracked", not an error.
pub fn read_judge_baseline(state: &StateDir) -> Result<Option<JudgeRecord>, StorageError> {
    read_json_if_exists(&state.evaluators_dir().join("baseline.json"))
}

pub fn read_judge_current(state: &StateDir) -> Result<Option<JudgeRecord>, StorageError> {
    read_json_if_exists(&state.evaluators_dir().join("current.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::write_json;

    fn judge() -> JudgeRecord {
        JudgeRecord {
            judge_id: "j1".to_string(),
            model: "claude-sonnet-5".to_string(),
            rubric_version: "17".to_string(),
            calibration_set_id: "support-v3".to_string(),
            human_agreement_rate: 0.91,
            evaluated_at: "2026-08-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn read_judge_baseline_and_current_round_trip_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        write_json(&state.evaluators_dir().join("baseline.json"), &judge()).unwrap();
        write_json(&state.evaluators_dir().join("current.json"), &judge()).unwrap();

        assert!(read_judge_baseline(&state).unwrap().is_some());
        assert!(read_judge_current(&state).unwrap().is_some());
    }

    #[test]
    fn read_judge_baseline_returns_none_when_not_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let state = StateDir::at(dir.path());
        assert!(read_judge_baseline(&state).unwrap().is_none());
    }
}
