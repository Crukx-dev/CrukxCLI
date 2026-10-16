//! Append-only run history — one JSON line per completed `gate`/`replay`.
//! This is what makes trends chartable (`crukx history`): every run
//! leaves a small durable record in `.crukx/runs.jsonl`, so the CLI can
//! show VTR/P95 over time without any server or database.
//!
//! Corrupt or unparseable lines are skipped on read, never fatal — a
//! partially-written last line (crash mid-append) must not take down
//! history rendering.

use crate::state_dir::StateDir;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunContractSummary {
    pub id: String,
    /// "PASS" | "BLOCK" for this contract after pass^k folding.
    pub decision: String,
    /// PASS count across the k repeats — present only when repeat > 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub k: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    /// RFC 3339 timestamp of when the run finished.
    pub ts: String,
    /// "gate" | "replay".
    pub command: String,
    /// Final verdict: "PASS" | "BLOCKED" (exit-code contract, same words
    /// as the human-readable verdict line).
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vtr: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_coverage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p95_ms: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub contracts: Vec<RunContractSummary>,
}

pub fn runs_file(state: &StateDir) -> PathBuf {
    state.root().join("runs.jsonl")
}

/// Appends one record. Creates `.crukx/` if missing. Failure is returned,
/// never swallowed — but callers treat history-write failure as
/// non-blocking (the gate result already happened).
pub fn append_run(state: &StateDir, record: &RunRecord) -> io::Result<()> {
    fs::create_dir_all(state.root())?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(runs_file(state))?;
    writeln!(
        file,
        "{}",
        serde_json::to_string(record).expect("record serializes")
    )?;
    Ok(())
}

/// Last `limit` records, oldest first. Unparseable lines are skipped.
pub fn read_runs(state: &StateDir, limit: usize) -> io::Result<Vec<RunRecord>> {
    let content = match fs::read_to_string(runs_file(state)) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    let records: Vec<RunRecord> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let start = records.len().saturating_sub(limit);
    Ok(records[start..].to_vec())
}

/// The most recent *green* gate run — the baseline regression diffing
/// compares against ("worse than last green?" is the question a merge
/// queue asks; absolute thresholds alone can't see slow decay).
pub fn read_last_green_gate(state: &StateDir) -> io::Result<Option<RunRecord>> {
    let records = read_runs(state, usize::MAX)?;
    Ok(records
        .iter()
        .rev()
        .find(|r| r.command == "gate" && r.decision == "PASS")
        .cloned())
}

/// This contract's own PASS/BLOCK record across its last `limit`
/// appearances in `.crukx/runs.jsonl` — oldest first. The real data
/// source `crukx-core::escalation::historical_p_failure` was built for:
/// a contract that isn't in any recorded run yet returns an empty slice,
/// which that function treats as "no signal" rather than "reliable".
pub fn contract_history(
    state: &StateDir,
    contract_id: &str,
    limit: usize,
) -> io::Result<Vec<bool>> {
    let records = read_runs(state, usize::MAX)?;
    let outcomes: Vec<bool> = records
        .iter()
        .flat_map(|r| r.contracts.iter())
        .filter(|c| c.id == contract_id)
        .map(|c| c.decision == "PASS")
        .collect();
    let start = outcomes.len().saturating_sub(limit);
    Ok(outcomes[start..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample(vtr: f64) -> RunRecord {
        RunRecord {
            ts: "2026-08-26T10:00:00Z".into(),
            command: "gate".into(),
            decision: "PASS".into(),
            vtr: Some(vtr),
            diagnostic_coverage: None,
            p95_ms: None,
            contracts: vec![RunContractSummary {
                id: "demo-1".into(),
                decision: "PASS".into(),
                passed: None,
                k: None,
            }],
        }
    }

    #[test]
    fn append_then_read_round_trips_records() {
        let dir = tempdir().unwrap();
        let state = StateDir::at(dir.path());
        append_run(&state, &sample(0.5)).unwrap();
        append_run(&state, &sample(1.0)).unwrap();
        let records = read_runs(&state, 10).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].vtr, Some(1.0));
    }

    #[test]
    fn read_on_missing_file_is_empty_not_an_error() {
        let dir = tempdir().unwrap();
        let state = StateDir::at(dir.path());
        assert!(read_runs(&state, 5).unwrap().is_empty());
    }

    #[test]
    fn limit_returns_only_the_last_n() {
        let dir = tempdir().unwrap();
        let state = StateDir::at(dir.path());
        for vtr in [0.1, 0.2, 0.3] {
            append_run(&state, &sample(vtr)).unwrap();
        }
        let records = read_runs(&state, 2).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].vtr, Some(0.2));
        assert_eq!(records[1].vtr, Some(0.3));
    }

    #[test]
    fn corrupt_lines_are_skipped_not_fatal() {
        let dir = tempdir().unwrap();
        let state = StateDir::at(dir.path());
        append_run(&state, &sample(0.4)).unwrap();
        fs::write(
            runs_file(&state),
            format!(
                "garbage not json\n{}\n",
                serde_json::to_string(&sample(0.9)).unwrap()
            ),
        )
        .unwrap();
        let records = read_runs(&state, 10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].vtr, Some(0.9));
    }

    #[test]
    fn last_green_gate_finds_the_most_recent_passing_gate_run_only() {
        let dir = tempdir().unwrap();
        let state = StateDir::at(dir.path());
        // replay PASS must not count; only gate records qualify
        let mut replay = sample(1.0);
        replay.command = "replay".into();
        append_run(&state, &replay).unwrap();

        let mut green_old = sample(0.9);
        green_old.command = "gate".into();
        green_old.decision = "PASS".into();
        append_run(&state, &green_old).unwrap();

        let mut blocked = sample(0.5);
        blocked.command = "gate".into();
        blocked.decision = "BLOCKED".into();
        append_run(&state, &blocked).unwrap();

        let mut green_new = sample(0.8);
        green_new.command = "gate".into();
        green_new.decision = "PASS".into();
        append_run(&state, &green_new).unwrap();

        let last_green = read_last_green_gate(&state).unwrap().unwrap();
        assert_eq!(last_green.vtr, Some(0.8));
    }

    #[test]
    fn last_green_gate_is_none_when_no_gate_ever_passed() {
        let dir = tempdir().unwrap();
        let state = StateDir::at(dir.path());
        let mut blocked = sample(0.5);
        blocked.command = "gate".into();
        blocked.decision = "BLOCKED".into();
        append_run(&state, &blocked).unwrap();
        assert!(read_last_green_gate(&state).unwrap().is_none());
    }

    #[test]
    fn contract_history_extracts_one_contracts_pass_block_record_oldest_first() {
        let dir = tempdir().unwrap();
        let state = StateDir::at(dir.path());
        for decision in ["PASS", "BLOCK", "PASS"] {
            let mut record = sample(0.9);
            record.contracts = vec![RunContractSummary {
                id: "demo-1".into(),
                decision: decision.into(),
                passed: None,
                k: None,
            }];
            append_run(&state, &record).unwrap();
        }
        let history = contract_history(&state, "demo-1", 10).unwrap();
        assert_eq!(history, vec![true, false, true]);
    }

    #[test]
    fn contract_history_ignores_other_contracts_and_respects_limit() {
        let dir = tempdir().unwrap();
        let state = StateDir::at(dir.path());
        for (id, decision) in [("demo-1", "PASS"), ("demo-2", "BLOCK"), ("demo-1", "BLOCK")] {
            let mut record = sample(0.9);
            record.contracts = vec![RunContractSummary {
                id: id.into(),
                decision: decision.into(),
                passed: None,
                k: None,
            }];
            append_run(&state, &record).unwrap();
        }
        let history = contract_history(&state, "demo-1", 1).unwrap();
        assert_eq!(history, vec![false]);
    }

    #[test]
    fn contract_history_of_a_never_seen_contract_is_empty() {
        let dir = tempdir().unwrap();
        let state = StateDir::at(dir.path());
        assert!(contract_history(&state, "unknown", 10).unwrap().is_empty());
    }

    #[test]
    fn ledger_with_ten_thousand_records_reads_within_reason() {
        let dir = tempdir().unwrap();
        let state = StateDir::at(dir.path());
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(runs_file(&state))
            .unwrap();
        for i in 0..10_000 {
            writeln!(
                file,
                "{}",
                serde_json::to_string(&RunRecord {
                    ts: format!("2026-01-01T00:{:02}:00Z", i % 60),
                    command: "gate".into(),
                    decision: if i % 2 == 0 { "PASS" } else { "BLOCKED" }.into(),
                    vtr: Some(i as f64 / 10_000.0),
                    diagnostic_coverage: None,
                    p95_ms: None,
                    contracts: Vec::new(),
                })
                .unwrap()
            )
            .unwrap();
        }
        let started = std::time::Instant::now();
        let all = read_runs(&state, usize::MAX).unwrap();
        let last_green = read_last_green_gate(&state).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(all.len(), 10_000);
        assert!(last_green.is_some());
        // Generous CI-runner ceiling: linear parse of 10k small lines
        // should be far under this even on slow machines.
        assert!(
            elapsed.as_millis() < 5_000,
            "10k-record ledger read took {elapsed:?}"
        );
    }
}
