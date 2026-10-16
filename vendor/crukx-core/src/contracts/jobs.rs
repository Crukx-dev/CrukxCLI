use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Optimize,
    Verify,
    Benchmark,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Job {
    pub job_id: String,
    pub org_id: String,
    pub project_id: String,
    pub kind: JobKind,
    pub state: JobState,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub artifact_id: Option<String>,
}

impl Job {
    pub fn new(
        job_id: String,
        org_id: String,
        project_id: String,
        kind: JobKind,
        created_at: String,
    ) -> Self {
        Self {
            job_id,
            org_id,
            project_id,
            kind,
            state: JobState::Queued,
            created_at: created_at.clone(),
            updated_at: created_at,
            payload: serde_json::Value::Null,
            result: None,
            artifact_id: None,
        }
    }

    pub fn can_transition(from: JobState, to: JobState) -> bool {
        matches!(
            (from, to),
            (JobState::Queued, JobState::Running)
                | (JobState::Queued, JobState::Cancelled)
                | (JobState::Running, JobState::Succeeded)
                | (JobState::Running, JobState::Failed)
                | (JobState::Running, JobState::Cancelled)
        )
    }

    pub fn transition(&mut self, to: JobState, updated_at: String) -> bool {
        if !Self::can_transition(self.state, to) {
            return false;
        }
        self.state = to;
        self.updated_at = updated_at;
        true
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            JobState::Succeeded | JobState::Failed | JobState::Cancelled
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> Job {
        Job::new(
            "j1".to_string(),
            "o1".to_string(),
            "p1".to_string(),
            JobKind::Benchmark,
            "2026-09-01T00:00:00Z".to_string(),
        )
    }

    #[test]
    fn new_job_starts_queued() {
        assert_eq!(job().state, JobState::Queued);
    }

    #[test]
    fn queued_to_running_is_allowed() {
        let mut j = job();
        assert!(j.transition(JobState::Running, "2026-09-01T00:01:00Z".to_string()));
        assert_eq!(j.state, JobState::Running);
    }

    #[test]
    fn queued_to_succeeded_is_rejected() {
        let mut j = job();
        assert!(!j.transition(JobState::Succeeded, "x".to_string()));
        assert_eq!(j.state, JobState::Queued);
    }

    #[test]
    fn running_to_succeeded_is_terminal() {
        let mut j = job();
        j.transition(JobState::Running, "t".to_string());
        j.transition(JobState::Succeeded, "t".to_string());
        assert!(j.is_terminal());
    }

    #[test]
    fn terminal_states_reject_further_transitions() {
        let mut j = job();
        j.transition(JobState::Running, "t".to_string());
        j.transition(JobState::Failed, "t".to_string());
        assert!(!j.transition(JobState::Succeeded, "t".to_string()));
    }

    #[test]
    fn job_round_trips() {
        let json = serde_json::to_string(&job()).unwrap();
        let back: Job = serde_json::from_str(&json).unwrap();
        assert_eq!(back.job_id, "j1");
    }
}
