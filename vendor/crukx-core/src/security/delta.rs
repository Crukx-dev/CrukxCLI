use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    fn weight(self) -> i64 {
        match self {
            Severity::Low => 1,
            Severity::Medium => 2,
            Severity::High => 4,
            Severity::Critical => 8,
        }
    }
}

/// Findings are matched by `id` across scans — the caller (a scanner) owns
/// giving the same real vulnerability the same id before/after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecurityDelta {
    pub critical_delta: i64,
    pub high_delta: i64,
    pub medium_delta: i64,
    pub low_delta: i64,
    pub overall_risk_delta: i64,
    pub worst_case_severity_before: Option<Severity>,
    pub worst_case_severity_after: Option<Severity>,
    pub worst_case_increased: bool,
    pub introduced: Vec<Finding>,
    pub resolved: Vec<Finding>,
}

/// Three measures per WeSCE: overall risk change (weighted sum), worst-case
/// severity (does the ceiling get worse), and per-severity distribution
/// shift (introduced/resolved).
pub fn compute_security_delta(before: &[Finding], after: &[Finding]) -> SecurityDelta {
    use std::collections::HashSet;

    let before_ids: HashSet<&str> = before.iter().map(|finding| finding.id.as_str()).collect();
    let after_ids: HashSet<&str> = after.iter().map(|finding| finding.id.as_str()).collect();
    let introduced: Vec<Finding> = after
        .iter()
        .filter(|finding| !before_ids.contains(finding.id.as_str()))
        .cloned()
        .collect();
    let resolved: Vec<Finding> = before
        .iter()
        .filter(|finding| !after_ids.contains(finding.id.as_str()))
        .cloned()
        .collect();

    let count = |findings: &[Finding], severity: Severity| {
        findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count() as i64
    };
    let overall_risk = |findings: &[Finding]| {
        findings
            .iter()
            .map(|finding| finding.severity.weight())
            .sum::<i64>()
    };
    let worst_case = |findings: &[Finding]| findings.iter().map(|finding| finding.severity).max();

    let worst_before = worst_case(before);
    let worst_after = worst_case(after);
    let worst_case_increased = match (worst_before, worst_after) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(before_severity), Some(after_severity)) => after_severity > before_severity,
    };

    SecurityDelta {
        critical_delta: count(after, Severity::Critical) - count(before, Severity::Critical),
        high_delta: count(after, Severity::High) - count(before, Severity::High),
        medium_delta: count(after, Severity::Medium) - count(before, Severity::Medium),
        low_delta: count(after, Severity::Low) - count(before, Severity::Low),
        overall_risk_delta: overall_risk(after) - overall_risk(before),
        worst_case_severity_before: worst_before,
        worst_case_severity_after: worst_after,
        worst_case_increased,
        introduced,
        resolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(id: &str, severity: Severity) -> Finding {
        Finding {
            id: id.to_string(),
            severity,
            cwe: None,
            location: None,
        }
    }

    #[test]
    fn no_findings_either_side_yields_a_zero_delta() {
        let delta = compute_security_delta(&[], &[]);
        assert_eq!(delta.overall_risk_delta, 0);
        assert!(!delta.worst_case_increased);
    }

    #[test]
    fn introduced_critical_finding_raises_critical_delta_and_worst_case() {
        let before = vec![finding("a", Severity::Medium)];
        let after = vec![
            finding("a", Severity::Medium),
            finding("b", Severity::Critical),
        ];
        let delta = compute_security_delta(&before, &after);
        assert_eq!(delta.critical_delta, 1);
        assert_eq!(delta.worst_case_severity_before, Some(Severity::Medium));
        assert_eq!(delta.worst_case_severity_after, Some(Severity::Critical));
        assert!(delta.worst_case_increased);
        assert_eq!(delta.introduced.len(), 1);
        assert_eq!(delta.introduced[0].id, "b");
    }

    #[test]
    fn resolved_findings_are_tracked_and_dont_count_as_introduced() {
        let before = vec![finding("a", Severity::High)];
        let after: Vec<Finding> = vec![];
        let delta = compute_security_delta(&before, &after);
        assert_eq!(delta.high_delta, -1);
        assert_eq!(delta.resolved.len(), 1);
        assert_eq!(delta.introduced.len(), 0);
        assert!(!delta.worst_case_increased);
    }

    #[test]
    fn a_harmless_refactor_that_swaps_severity_without_adding_findings_still_moves_overall_risk_delta(
    ) {
        let before = vec![finding("a", Severity::Low)];
        let after = vec![finding("a", Severity::High)];
        let delta = compute_security_delta(&before, &after);
        assert_eq!(delta.introduced.len(), 0);
        assert_eq!(delta.overall_risk_delta, 3); // high(4) - low(1)
    }
}
