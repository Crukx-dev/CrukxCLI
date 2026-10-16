use crate::contracts::replay::{ReplayDecision, ReplayOutcome};
use crate::policy::schema::{PolicyContractEntry, PolicyMode};

#[derive(Debug, Clone)]
pub struct GateEntryResult {
    pub entry: PolicyContractEntry,
    pub outcome: ReplayOutcome,
    /// Whether this entry's result actually fails the gate (BLOCK + mode=block).
    pub fails: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateDecision {
    Pass,
    Block,
}

#[derive(Debug, Clone)]
pub struct GateResult {
    pub entries: Vec<GateEntryResult>,
    pub decision: GateDecision,
}

/// Pure aggregation: given each contract's replay outcome, decides the
/// overall gate result. A "warn" mode contract that BLOCKs is reported but
/// never fails the gate — only "block" mode (the default) does.
pub fn evaluate_gate(entries: Vec<(PolicyContractEntry, ReplayOutcome)>) -> GateResult {
    let results: Vec<GateEntryResult> = entries
        .into_iter()
        .map(|(entry, outcome)| {
            let fails =
                outcome.decision == ReplayDecision::Block && entry.mode == PolicyMode::Block;
            GateEntryResult {
                entry,
                outcome,
                fails,
            }
        })
        .collect();

    let decision = if results.iter().any(|result| result.fails) {
        GateDecision::Block
    } else {
        GateDecision::Pass
    };

    GateResult {
        entries: results,
        decision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::replay::{ReplayDecision, ReplayOutcome};
    use crate::contracts::schema::AssertResult;
    use crate::policy::schema::{PolicyContractEntry, PolicyMode};

    fn outcome(decision: ReplayDecision) -> ReplayOutcome {
        ReplayOutcome {
            contract_id: "c".to_string(),
            candidate_label: "ci".to_string(),
            steps: vec![],
            assertion: AssertResult {
                pass: decision == ReplayDecision::Pass,
                reason: None,
            },
            decision,
        }
    }

    fn entry(id: &str, mode: PolicyMode) -> PolicyContractEntry {
        PolicyContractEntry {
            id: id.to_string(),
            mode,
            candidate_label: "ci".to_string(),
            repeat: 1,
            escalation: None,
        }
    }

    #[test]
    fn evaluate_gate_passes_when_every_contract_passes() {
        let gate = evaluate_gate(vec![(
            entry("c", PolicyMode::Block),
            outcome(ReplayDecision::Pass),
        )]);
        assert_eq!(gate.decision, GateDecision::Pass);
    }

    #[test]
    fn evaluate_gate_blocks_when_a_block_mode_contract_blocks() {
        let gate = evaluate_gate(vec![(
            entry("c", PolicyMode::Block),
            outcome(ReplayDecision::Block),
        )]);
        assert_eq!(gate.decision, GateDecision::Block);
        assert!(gate.entries[0].fails);
    }

    #[test]
    fn evaluate_gate_does_not_fail_for_a_warn_mode_block() {
        let gate = evaluate_gate(vec![(
            entry("c", PolicyMode::Warn),
            outcome(ReplayDecision::Block),
        )]);
        assert_eq!(gate.decision, GateDecision::Pass);
        assert!(!gate.entries[0].fails);
    }

    #[test]
    fn evaluate_gate_blocks_overall_if_any_one_of_several_contracts_fails() {
        let gate = evaluate_gate(vec![
            (entry("a", PolicyMode::Block), outcome(ReplayDecision::Pass)),
            (
                entry("b", PolicyMode::Block),
                outcome(ReplayDecision::Block),
            ),
            (entry("c", PolicyMode::Warn), outcome(ReplayDecision::Block)),
        ]);
        assert_eq!(gate.decision, GateDecision::Block);
    }
}
