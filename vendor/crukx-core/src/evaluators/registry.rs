use serde::{Deserialize, Serialize};

/// Records what a judge was calibrated against. Scope: the record + drift
/// check only. Crukx doesn't run a judge lifecycle (birth set curation,
/// RART rubric tuning, human review gate) — those need a labeled
/// calibration pipeline this crate doesn't own. A caller writes a
/// `JudgeRecord` after every calibration run; `detect_judge_drift` is the
/// part that's Crukx's job.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JudgeRecord {
    pub judge_id: String,
    pub model: String,
    pub rubric_version: String,
    pub calibration_set_id: String,
    pub human_agreement_rate: f64,
    pub evaluated_at: String,
}

#[derive(Debug, Clone)]
pub struct JudgeDriftResult {
    pub drifted: bool,
    pub agreement_delta: f64,
    pub reason: Option<String>,
}

/// Drift = current judge disagrees with humans meaningfully more than its
/// calibration baseline did, or it silently changed model/rubric without a
/// new baseline being recorded (Netflix's lesson: an unlabeled swap is the
/// dangerous case, not just gradual decay). `max_agreement_drop` defaults
/// to 0.1 (10 points) when not supplied by policy constraints.
pub fn detect_judge_drift(
    baseline: &JudgeRecord,
    current: &JudgeRecord,
    max_agreement_drop: Option<f64>,
) -> JudgeDriftResult {
    let max_drop = max_agreement_drop.unwrap_or(0.1);

    if baseline.model != current.model {
        return JudgeDriftResult {
            drifted: true,
            agreement_delta: current.human_agreement_rate - baseline.human_agreement_rate,
            reason: Some(format!(
                "judge model changed ({} -> {}) without recalibration",
                baseline.model, current.model
            )),
        };
    }
    if baseline.rubric_version != current.rubric_version {
        return JudgeDriftResult {
            drifted: true,
            agreement_delta: current.human_agreement_rate - baseline.human_agreement_rate,
            reason: Some(format!(
                "rubric changed ({} -> {}) without recalibration",
                baseline.rubric_version, current.rubric_version
            )),
        };
    }
    // A swapped calibration set is the same "unlabeled swap" risk as a
    // model/rubric change — a new (e.g. easier) calibration set can hide a
    // real agreement drop behind a stable-looking human_agreement_rate.
    // Without this check the function fails open: current.calibration_set_id
    // differing from baseline's was silently ignored and fell through to
    // the agreement-delta-only comparison below.
    if baseline.calibration_set_id != current.calibration_set_id {
        return JudgeDriftResult {
            drifted: true,
            agreement_delta: current.human_agreement_rate - baseline.human_agreement_rate,
            reason: Some(format!(
                "calibration set changed ({} -> {}) without recalibration",
                baseline.calibration_set_id, current.calibration_set_id
            )),
        };
    }

    let agreement_delta = current.human_agreement_rate - baseline.human_agreement_rate;
    if agreement_delta < -max_drop {
        return JudgeDriftResult {
            drifted: true,
            agreement_delta,
            reason: Some(format!(
                "human agreement dropped {:.1}pt ({:.1}% -> {:.1}%)",
                agreement_delta.abs() * 100.0,
                baseline.human_agreement_rate * 100.0,
                current.human_agreement_rate * 100.0
            )),
        };
    }

    JudgeDriftResult {
        drifted: false,
        agreement_delta,
        reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn no_drift_when_agreement_holds_steady() {
        let mut current = judge();
        current.human_agreement_rate = 0.9;
        let result = detect_judge_drift(&judge(), &current, None);
        assert!(!result.drifted);
    }

    #[test]
    fn drift_when_agreement_drops_past_threshold() {
        let mut current = judge();
        current.human_agreement_rate = 0.75;
        let result = detect_judge_drift(&judge(), &current, None);
        assert!(result.drifted);
        assert!(result.reason.unwrap().contains("agreement dropped"));
    }

    #[test]
    fn drift_when_model_swapped_without_recalibration_even_if_agreement_looks_fine() {
        let mut current = judge();
        current.model = "claude-opus-5".to_string();
        current.human_agreement_rate = 0.95;
        let result = detect_judge_drift(&judge(), &current, None);
        assert!(result.drifted);
        assert!(result.reason.unwrap().contains("model changed"));
    }

    #[test]
    fn drift_when_rubric_changed_without_recalibration() {
        let mut current = judge();
        current.rubric_version = "18".to_string();
        let result = detect_judge_drift(&judge(), &current, None);
        assert!(result.drifted);
        assert!(result.reason.unwrap().contains("rubric changed"));
    }

    #[test]
    fn drift_when_calibration_set_swapped_even_if_agreement_looks_stable_or_improved() {
        let mut current = judge();
        current.calibration_set_id = "support-v4-easier".to_string();
        current.human_agreement_rate = 0.93; // looks fine in isolation
        let result = detect_judge_drift(&judge(), &current, None);
        assert!(result.drifted);
        assert!(result.reason.unwrap().contains("calibration set changed"));
    }
}
