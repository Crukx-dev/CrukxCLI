use serde::{Deserialize, Serialize};

/// One example in a fixed, human-labeled calibration set the judge
/// re-scores at intervals. Ports the "anchor set" idea from an
/// anytime-valid attribution paper for LLM evaluation pipelines
/// (arxiv 2606.15474, "Who Drifted: the System or the Judge?"): because
/// only the judge — never the evaluated system — can change how these
/// fixed, unchanging examples get scored, a shift in the judge-vs-human
/// gap on anchors alone isolates judge drift from system drift. A shift
/// in production agreement with the anchor gap held steady means the
/// system got worse, not the judge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AnchorScore {
    pub anchor_id: String,
    pub human_label: f64,
    pub judge_score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftAttribution {
    None,
    Judge,
    System,
}

#[derive(Debug, Clone)]
pub struct AttributionResult {
    pub attribution: DriftAttribution,
    pub anchor_gap_delta: f64,
    pub production_agreement_delta: f64,
}

fn mean_absolute_gap(anchors: &[AnchorScore]) -> f64 {
    if anchors.is_empty() {
        return 0.0;
    }
    anchors
        .iter()
        .map(|anchor| (anchor.judge_score - anchor.human_label).abs())
        .sum::<f64>()
        / anchors.len() as f64
}

/// Simplified, threshold-based approximation of the source paper's
/// anytime-valid betting e-process — a real sequential hypothesis test
/// with anytime-valid error-rate guarantees is real statistical machinery
/// this function does not implement. What it keeps from the paper is the
/// actually load-bearing insight: **one-way identification**. The anchor
/// set's human labels never change, so if the judge's mean gap against
/// those labels moves, that can only be the judge changing — the system
/// under evaluation has no way to touch a fixed anchor set's score. Only
/// once the anchor gap is ruled out does a production agreement drop get
/// attributed to the system.
pub fn attribute_drift(
    baseline_anchors: &[AnchorScore],
    current_anchors: &[AnchorScore],
    production_agreement_baseline: f64,
    production_agreement_current: f64,
    anchor_gap_threshold: f64,
) -> AttributionResult {
    let baseline_gap = mean_absolute_gap(baseline_anchors);
    let current_gap = mean_absolute_gap(current_anchors);
    let anchor_gap_delta = current_gap - baseline_gap;
    let production_agreement_delta = production_agreement_current - production_agreement_baseline;

    let judge_moved = anchor_gap_delta.abs() > anchor_gap_threshold;
    let production_worsened = production_agreement_delta < -anchor_gap_threshold;

    let attribution = if judge_moved {
        DriftAttribution::Judge
    } else if production_worsened {
        DriftAttribution::System
    } else {
        DriftAttribution::None
    };

    AttributionResult {
        attribution,
        anchor_gap_delta,
        production_agreement_delta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(id: &str, human_label: f64, judge_score: f64) -> AnchorScore {
        AnchorScore {
            anchor_id: id.to_string(),
            human_label,
            judge_score,
        }
    }

    #[test]
    fn attributes_to_judge_when_the_anchor_gap_moves_even_if_production_also_worsened() {
        // Anchors: baseline judge nearly matches humans (gap ~0.02),
        // current judge is way off (gap ~0.4) -> only the judge could
        // have changed how it scores a fixed, human-labeled set.
        let baseline_anchors = vec![anchor("a1", 0.9, 0.92), anchor("a2", 0.5, 0.48)];
        let current_anchors = vec![anchor("a1", 0.9, 0.5), anchor("a2", 0.5, 0.1)];

        let result = attribute_drift(&baseline_anchors, &current_anchors, 0.91, 0.7, 0.1);
        assert_eq!(result.attribution, DriftAttribution::Judge);
    }

    #[test]
    fn attributes_to_system_when_production_worsens_with_a_stable_anchor_gap() {
        let baseline_anchors = vec![anchor("a1", 0.9, 0.92), anchor("a2", 0.5, 0.48)];
        let current_anchors = vec![anchor("a1", 0.9, 0.91), anchor("a2", 0.5, 0.49)];

        let result = attribute_drift(&baseline_anchors, &current_anchors, 0.91, 0.7, 0.1);
        assert_eq!(result.attribution, DriftAttribution::System);
    }

    #[test]
    fn attributes_to_none_when_both_the_anchor_gap_and_production_agreement_are_stable() {
        let baseline_anchors = vec![anchor("a1", 0.9, 0.92)];
        let current_anchors = vec![anchor("a1", 0.9, 0.91)];

        let result = attribute_drift(&baseline_anchors, &current_anchors, 0.91, 0.9, 0.1);
        assert_eq!(result.attribution, DriftAttribution::None);
    }

    #[test]
    fn empty_anchor_sets_fall_back_to_production_agreement_only() {
        let result = attribute_drift(&[], &[], 0.91, 0.6, 0.1);
        assert_eq!(result.attribution, DriftAttribution::System);
        assert_eq!(result.anchor_gap_delta, 0.0);
    }
}
