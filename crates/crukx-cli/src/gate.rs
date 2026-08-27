use crate::charts;
use crate::colors::{self, color_enabled};
use crate::replay::replay_contract;
use crate::ui;
use clap::ValueEnum;
use crukx_core::contracts::replay::{ReplayDecision, ReplayOutcome};
use crukx_core::contracts::schema::RegressionContract;
use crukx_core::evidence::{build_evidence_graph, summarize_root_cause};
use crukx_core::policy::consistency::pass_k_lower_bound;
use crukx_core::policy::{
    evaluate_gate, evaluate_reliability_contract, GateDecision, ReliabilityContractInputs,
    ReliabilityContractResult,
};
use crukx_storage::contracts::read_contract;
use crukx_storage::evaluators::{read_judge_baseline, read_judge_current};
use crukx_storage::evidence::write_evidence_graph;
use crukx_storage::policy_file::load_policy;
use crukx_storage::runs::{
    append_run, read_last_green_gate, RunContractSummary, RunRecord,
};
use crukx_storage::security::{read_security_after, read_security_before};
use crukx_storage::state_dir::StateDir;
use serde_json::json;
use std::path::{Path, PathBuf};

/// Output surface for `crukx gate`. `human` (default) is the colored
/// stderr report; the other three are machine surfaces written to stdout
/// so CI pipelines and PR bots can consume them directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
    Junit,
    Markdown,
}

/// One contract's full verdict after pass^k folding — everything every
/// output format needs, collected once during replay.
struct ContractVerdict {
    id: String,
    mode_label: &'static str,
    decision: ReplayDecision,
    fails_gate: bool,
    assertion_reason: Option<String>,
    diverged_steps: Vec<String>,
    root_cause: String,
    evidence_path: Option<PathBuf>,
    /// (passed, k) when repeat > 1.
    pass_k: Option<(usize, usize)>,
    /// Wilson 95% lower bound on the observed pass rate — the honest
    /// floor. Present only for repeated contracts.
    ci_floor: Option<f64>,
}

struct GateReportSnapshot {
    policy_display: String,
    decision: String,
    contracts: Vec<serde_json::Value>,
    violations: Vec<String>,
    regression_details: Vec<String>,
    regression_detected: bool,
    vtr: f64,
    diagnostic_coverage: f64,
    p95_ms: Option<u64>,
    judge_drifted: Option<bool>,
    security_critical_delta: Option<i64>,
    security_high_delta: Option<i64>,
    reliability_pass: bool,
}

/// Entry point for `crukx gate [--policy crukx.yml]`. Returns the process
/// exit code (0 = PASS, 1 = BLOCK / SLA breach / regression-with-flag).
pub fn run_gate(
    policy_path: &Path,
    cwd: &Path,
    format: OutputFormat,
    fail_on_regression: bool,
) -> i32 {
    let state = StateDir::resolve(cwd);
    let contracts_dir = state.contracts_dir();

    let policy = match load_policy(&cwd.join(policy_path)) {
        Ok(policy) => policy,
        Err(err) => {
            eprintln!(
                "Crukx gate: failed to load {}: {err}",
                policy_path.display()
            );
            return 1;
        }
    };
    if policy.contracts.is_empty() {
        eprintln!(
            "Crukx gate: {} lists no contracts — nothing to check.",
            policy_path.display()
        );
        return 0;
    }

    let color = color_enabled();
    if format == OutputFormat::Human {
        eprintln!(
            "{}",
            ui::header(
                "Crukx gate",
                Some(&format!(
                    "{} · {} contract{}",
                    policy_path.display(),
                    policy.contracts.len(),
                    if policy.contracts.len() == 1 { "" } else { "s" }
                )),
                color
            )
        );
    }

    // Progress feedback only when there's more than one replay to sit
    // through — a single fast replay doesn't need a spinner's worth of
    // ceremony (same judgment call `cargo` makes for one-crate builds).
    let total_replays: usize = policy.contracts.iter().map(|e| e.repeat.max(1)).sum();
    let show_progress = total_replays > 1 && format == OutputFormat::Human;

    let mut completed_replays = 0usize;
    let mut verdicts: Vec<ContractVerdict> = Vec::new();
    let mut raw_entries = Vec::new();

    for entry in &policy.contracts {
        let repeat = entry.repeat.max(1);

        let contract: RegressionContract = match read_contract(&state, &entry.id) {
            Ok(contract) => contract,
            Err(err) => {
                eprintln!("Crukx gate: no contract found for \"{}\": {err}", entry.id);
                eprintln!(
                    "{}",
                    ui::hint(
                        "create one with `crukx capture` after a `crukx run` session",
                        color
                    )
                );
                return 1;
            }
        };

        let mut decisions = Vec::with_capacity(repeat);
        let mut outcome = None;
        for _ in 0..repeat {
            if show_progress {
                completed_replays += 1;
                eprintln!(
                    "  {} {} ({}/{})",
                    colors::dim("▸", color),
                    colors::dim(&entry.id, color),
                    completed_replays,
                    total_replays
                );
            }
            let run_outcome =
                replay_contract(&contract, &contracts_dir, &entry.candidate_label, cwd);
            decisions.push(run_outcome.decision);
            outcome = Some(run_outcome);
        }
        let mut outcome = outcome.expect("at least one replay runs");

        let passed_runs = decisions
            .iter()
            .filter(|d| **d == ReplayDecision::Pass)
            .count();
        let consistent = decisions.iter().all(|d| *d == ReplayDecision::Pass);

        // pass^k folds into the recorded outcome decision; the last run's
        // steps/assertion stay available for evidence detail.
        if !consistent {
            outcome.decision = ReplayDecision::Block;
        }

        // Evidence graph write happens once per blocking contract here —
        // collection phase, not render phase — so machine formats get the
        // same durable evidence as humans do.
        let evidence_path = if outcome.decision == ReplayDecision::Block {
            let graph = build_evidence_graph(&contract, &outcome);
            match write_evidence_graph(&state, &entry.id, &graph) {
                Ok(()) => Some(state.evidence_dir().join(format!("{entry_id}.json", entry_id = entry.id))),
                Err(_) => None,
            }
        } else {
            None
        };

        verdicts.push(ContractVerdict {
            id: entry.id.clone(),
            mode_label: match entry.mode {
                crukx_core::policy::PolicyMode::Block => "block",
                crukx_core::policy::PolicyMode::Warn => "warn",
            },
            decision: outcome.decision,
            // recomputed properly by evaluate_gate below via raw_entries
            fails_gate: false,
            assertion_reason: outcome.assertion.reason.clone(),
            diverged_steps: outcome
                .steps
                .iter()
                .filter(|step| !step.matched_expected)
                .map(|step| format!("{} {}", step.executable, step.arguments.join(" ")))
                .collect(),
            root_cause: summarize_root_cause(&outcome),
            evidence_path,
            pass_k: if repeat > 1 {
                Some((passed_runs, repeat))
            } else {
                None
            },
            ci_floor: if repeat > 1 {
                pass_k_lower_bound(&decisions, 1.96)
            } else {
                None
            },
        });

        raw_entries.push((entry.clone(), outcome));
    }

    let gate = evaluate_gate(raw_entries.clone());
    for (verdict, result) in verdicts.iter_mut().zip(gate.entries.iter()) {
        verdict.fails_gate = result.fails;
    }
    let outcomes: Vec<ReplayOutcome> = raw_entries
        .iter()
        .map(|(_, outcome)| outcome.clone())
        .collect();

    let judge_baseline = read_judge_baseline(&state).unwrap_or(None);
    let judge_current = read_judge_current(&state).unwrap_or(None);
    let security_before = read_security_before(&state).unwrap_or(None);
    let security_after = read_security_after(&state).unwrap_or(None);

    let constraints = policy.constraints.clone().unwrap_or_default();
    let reliability = evaluate_reliability_contract(ReliabilityContractInputs {
        outcomes: &outcomes,
        constraints: constraints.clone(),
        judge: judge_baseline.as_ref().zip(judge_current.as_ref()),
        security: security_before.as_deref().zip(security_after.as_deref()),
    });

    // Honest-worst-case SLA: every repeated contract's Wilson lower
    // bound must clear the configured floor — point estimates lie when
    // k is small.
    let mut sla_violations = Vec::new();
    if let Some(sla) = constraints.reliability_sla_minimum {
        for verdict in &verdicts {
            if let Some(floor) = verdict.ci_floor {
                if floor < sla {
                    sla_violations.push(format!(
                        "pass^{k} floor for {id}: Wilson lower bound {floor:.3} below SLA {sla}",
                        k = verdict.pass_k.map(|(_, k)| k).unwrap_or(0),
                        id = verdict.id,
                    ));
                }
            }
        }
    }

    // Regression diff vs the last green run: "worse than last green?"
    // catches slow decay that absolute thresholds can't see.
    let last_green = read_last_green_gate(&state).unwrap_or(None);
    let (regression_detected, regression_details) = diff_against_last_green(last_green.as_ref(), &verdicts, reliability.vtr);

    let gate_passed = gate.decision == GateDecision::Pass
        && reliability.pass
        && sla_violations.is_empty();
    let overall_decision = if gate_passed { "PASS" } else { "BLOCKED" };
    let blocked_by_regression = fail_on_regression && regression_detected;
    let exit_code = if gate_passed && !blocked_by_regression { 0 } else { 1 };

    record_run_history(
        &state,
        "gate",
        if gate_passed { "PASS" } else { "BLOCKED" },
        &reliability,
        &verdicts,
    );

    let snapshot = GateReportSnapshot::new(
        policy_path,
        &verdicts,
        &reliability,
        &sla_violations,
        &regression_details,
        regression_detected,
        overall_decision,
    );
    match format {
        OutputFormat::Json => print_json_report(&snapshot),
        OutputFormat::Junit => print_junit_report(&snapshot),
        OutputFormat::Markdown => print_markdown_report(&snapshot),
        OutputFormat::Human => render_human(
            &verdicts,
            &reliability,
            &sla_violations,
            overall_decision,
            color,
            &regression_details,
        ),
    }

    if format == OutputFormat::Human && exit_code != 0 {
        print_failure_hint(
            &verdicts,
            constraints.reliability_sla_minimum.is_some(),
            &regression_details,
            blocked_by_regression,
            &state,
            color,
        );
    }

    exit_code
}

fn diff_against_last_green(
    last_green: Option<&RunRecord>,
    verdicts: &[ContractVerdict],
    vtr_now: f64,
) -> (bool, Vec<String>) {
    let Some(green) = last_green else {
        return (false, Vec::new());
    };
    let mut details = Vec::new();
    let green_map: std::collections::HashMap<&str, &str> = green
        .contracts
        .iter()
        .map(|c| (c.id.as_str(), c.decision.as_str()))
        .collect();
    for verdict in verdicts {
        let now = if verdict.decision == ReplayDecision::Pass {
            "PASS"
        } else {
            "BLOCK"
        };
        if green_map.get(verdict.id.as_str()) == Some(&"PASS") && now == "BLOCK" {
            details.push(format!(
                "contract {id} was PASS at last green ({ts}), now BLOCK",
                id = verdict.id,
                ts = green.ts
            ));
        }
    }
    if let Some(vtr_then) = green.vtr {
        if vtr_now < vtr_then - f64::EPSILON {
            details.push(format!(
                "VTR regressed {:.1}% → {:.1}% since last green",
                vtr_then * 100.0,
                vtr_now * 100.0
            ));
        }
    }
    (!details.is_empty(), details)
}

fn render_human(
    verdicts: &[ContractVerdict],
    reliability: &ReliabilityContractResult,
    sla_violations: &[String],
    decision: &str,
    color: bool,
    regression_details: &[String],
) {
    eprintln!();
    for verdict in verdicts {
        let mark = if verdict.decision == ReplayDecision::Pass {
            format!("{} ", ui::ok_mark(color)) + &colors::green("PASS", color)
        } else if verdict.fails_gate {
            format!("{} ", ui::fail_mark(color)) + &colors::red("BLOCK", color)
        } else {
            format!("{} ", ui::warn_mark(color)) + &colors::yellow("warn (non-blocking)", color)
        };
        match verdict.pass_k {
            Some((passed, k)) => {
                let (dots_pass, dots_fail) = charts::pass_dots_split(passed, k);
                let floor_note = verdict
                    .ci_floor
                    .map(|f| format!(" · floor {:.2}", f))
                    .unwrap_or_default();
                eprintln!(
                    "  [{mode}] {id}: {mark} (pass^{k}: {passed}/{k}{floor_note}) {dots_pass}{dots_fail}",
                    mode = verdict.mode_label,
                    id = verdict.id,
                    dots_pass = colors::green(&dots_pass, color),
                    dots_fail = colors::red(&dots_fail, color),
                );
            }
            None => eprintln!("  [{mode}] {id}: {mark}", mode = verdict.mode_label, id = verdict.id),
        }

        if verdict.decision == ReplayDecision::Block {
            if let Some(reason) = &verdict.assertion_reason {
                eprintln!("      assertion: {reason}");
            }
            for step in &verdict.diverged_steps {
                eprintln!("      {} {}", colors::red("diverged:", color), step);
            }
            eprintln!("      {}", verdict.root_cause);
            if let Some(path) = &verdict.evidence_path {
                eprintln!("      evidence: {}", path.display());
            } else {
                eprintln!(
                    "      {}",
                    colors::dim("evidence: unavailable this run", color)
                );
            }
        }
    }

    const METER_WIDTH: usize = 10;
    let vtr_meter = charts::bar_meter(reliability.vtr, 1.0, METER_WIDTH);
    eprintln!("VTR                  {:.1}%  {}", reliability.vtr * 100.0, vtr_meter);
    let coverage_meter = charts::bar_meter(reliability.diagnostic_coverage, 1.0, METER_WIDTH);
    eprintln!(
        "Diagnostic coverage  {:.1}%  {}",
        reliability.diagnostic_coverage * 100.0,
        coverage_meter
    );
    if let Some(p95_ms) = reliability.p95_latency_ms {
        eprintln!("P95 latency          {p95_ms}ms");
    }
    if let Some(drift) = &reliability.judge_drift {
        let status = if drift.drifted {
            colors::red("DRIFTED", color)
        } else {
            colors::green("stable", color)
        };
        let glyph = if drift.drifted {
            ui::fail_mark(color)
        } else {
            ui::ok_mark(color)
        };
        eprintln!("Judge health         {status} {glyph}");
    }
    if let Some(delta) = &reliability.security_delta {
        eprintln!(
            "Security delta       {}{} critical, {}{} high",
            if delta.critical_delta >= 0 { "+" } else { "" },
            delta.critical_delta,
            if delta.high_delta >= 0 { "+" } else { "" },
            delta.high_delta
        );
    }
    for violation in &reliability.violations {
        eprintln!("  {}", colors::red(&format!("✕ {violation}"), color));
    }
    for violation in sla_violations {
        eprintln!("  {}", colors::red(&format!("✕ {violation}"), color));
    }
    for detail in regression_details {
        eprintln!("  {}", colors::yellow(&format!("↘ regression: {detail}"), color));
    }

    let decision_painted = if decision == "PASS" {
        colors::green(decision, color)
    } else {
        colors::red(decision, color)
    };
    eprintln!(
        "{}",
        colors::bold(&format!("Crukx gate: {decision_painted}"), color)
    );
}

#[allow(clippy::too_many_arguments)]
fn print_failure_hint(
    verdicts: &[ContractVerdict],
    sla_configured: bool,
    regression_details: &[String],
    blocked_by_regression: bool,
    _state: &StateDir,
    color: bool,
) {
    if let Some(first_blocked) = verdicts
        .iter()
        .find(|v| v.decision == ReplayDecision::Block && v.fails_gate)
    {
        let evidence = first_blocked
            .evidence_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(".crukx/evidence"));
        eprintln!(
            "{}",
            ui::hint(
                &format!(
                    "inspect evidence at {} · full session view: `crukx review latest`",
                    evidence.display()
                ),
                color
            )
        );
    } else if sla_configured
        && verdicts.iter().any(|v| v.ci_floor.is_some())
    {
        eprintln!(
            "{}",
            ui::hint("an SLA floor was breached — add repeats or fix flakiness; see pass^k floors above", color)
        );
    } else if blocked_by_regression && !regression_details.is_empty() {
        eprintln!(
            "{}",
            ui::hint("--fail-on-regression: fix the decay or re-baseline with a deliberate PASS run", color)
        );
    }
}

// ---- machine formats ----

impl GateReportSnapshot {
    fn new(
        policy_path: &Path,
        verdicts: &[ContractVerdict],
        reliability: &ReliabilityContractResult,
        sla_violations: &[String],
        regression_details: &[String],
        regression_detected: bool,
        decision: &str,
    ) -> Self {
        GateReportSnapshot {
            policy_display: policy_path.display().to_string(),
            decision: decision.to_string(),
            contracts: verdicts.iter().map(contract_to_json).collect(),
            violations: reliability
                .violations
                .iter()
                .chain(sla_violations.iter())
                .cloned()
                .collect(),
            regression_details: regression_details.to_vec(),
            regression_detected,
            vtr: reliability.vtr,
            diagnostic_coverage: reliability.diagnostic_coverage,
            p95_ms: reliability.p95_latency_ms,
            judge_drifted: reliability.judge_drift.as_ref().map(|d| d.drifted),
            security_critical_delta: reliability.security_delta.as_ref().map(|d| d.critical_delta),
            security_high_delta: reliability.security_delta.as_ref().map(|d| d.high_delta),
            reliability_pass: reliability.pass,
        }
    }
}

fn contract_to_json(v: &ContractVerdict) -> serde_json::Value {
    json!({
        "id": v.id,
        "mode": v.mode_label,
        "decision": if v.decision == ReplayDecision::Pass { "PASS" } else { "BLOCK" },
        "fails_gate": v.fails_gate,
        "assertion_reason": v.assertion_reason,
        "diverged_steps": v.diverged_steps,
        "root_cause": v.root_cause,
        "evidence": v.evidence_path.as_ref().map(|p| p.display().to_string()),
        "pass_k": v.pass_k.map(|(passed, k)| json!({
            "passed": passed,
            "k": k,
            "point_estimate": passed as f64 / k.max(1) as f64,
            "wilson_lower_95": v.ci_floor,
        })),
    })
}

fn print_json_report(report: &GateReportSnapshot) {
    let out = json!({
        "tool": "crukx",
        "command": "gate",
        "policy": report.policy_display,
        "decision": report.decision,
        "contracts": report.contracts,
        "reliability": {
            "vtr": report.vtr,
            "diagnostic_coverage": report.diagnostic_coverage,
            "p95_latency_ms": report.p95_ms,
            "judge_drifted": report.judge_drifted,
            "security_critical_delta": report.security_critical_delta,
            "security_high_delta": report.security_high_delta,
            "pass": report.reliability_pass,
            "violations": report.violations,
        },
        "regression_vs_last_green": {
            "detected": report.regression_detected,
            "details": report.regression_details,
        },
        "recorded_at": chrono::Utc::now().to_rfc3339(),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn print_junit_report(report: &GateReportSnapshot) {
    let failures = report
        .contracts
        .iter()
        .filter(|c| c["fails_gate"].as_bool() == Some(true))
        .count();
    println!(
        "<testsuites name=\"crukx gate\" tests=\"{}\" failures=\"{}\">",
        report.contracts.len() + 2,
        failures
            + usize::from(!report.violations.is_empty())
            + usize::from(report.regression_detected)
    );
    println!("  <testsuite name=\"crukx.contracts\">");
    for contract in &report.contracts {
        let id = contract["id"].as_str().unwrap_or("unknown");
        if contract["fails_gate"].as_bool() == Some(true) {
            println!(
                "    <testcase name=\"{name}\" classname=\"crukx.contract\"><failure message=\"{msg}\">{detail}</failure></testcase>",
                name = xml_escape(id),
                msg = xml_escape(contract["root_cause"].as_str().unwrap_or("blocked")),
                detail = xml_escape(contract.to_string().as_str()),
            );
        } else {
            println!(
                "    <testcase name=\"{name}\" classname=\"crukx.contract\"/>",
                name = xml_escape(id)
            );
        }
    }
    println!("  </testsuite>");
    println!("  <testsuite name=\"crukx.reliability\">");
    if report.violations.is_empty() {
        println!("    <testcase name=\"reliability-contract\" classname=\"crukx.reliability\"/>");
    } else {
        println!(
            "    <testcase name=\"reliability-contract\" classname=\"crukx.reliability\"><failure message=\"{msg}\"/></testcase>",
            msg = xml_escape(&report.violations.join("; "))
        );
    }
    if report.regression_detected {
        println!(
            "    <testcase name=\"regression-vs-last-green\" classname=\"crukx.reliability\"><failure message=\"{msg}\"/></testcase>",
            msg = xml_escape(&report.regression_details.join("; "))
        );
    } else {
        println!("    <testcase name=\"regression-vs-last-green\" classname=\"crukx.reliability\"/>");
    }
    println!("  </testsuite>");
    println!("</testsuites>");
}

fn print_markdown_report(report: &GateReportSnapshot) {
    let icon = if report.decision == "PASS" { "✅" } else { "❌" };
    println!("## Crukx gate: {icon} {}\n", report.decision);
    println!("| Contract | Mode | Result | pass^k | Floor (Wilson 95%) |");
    println!("|---|---|---|---|---|");
    for contract in &report.contracts {
        let pass_k_cell = contract["pass_k"]
            .as_object()
            .map(|pk| {
                format!(
                    "{}/{}",
                    pk["passed"].as_i64().unwrap_or(0),
                    pk["k"].as_i64().unwrap_or(0)
                )
            })
            .unwrap_or_else(|| "—".into());
        let floor_cell = contract["pass_k"]
            .as_object()
            .and_then(|pk| pk["wilson_lower_95"].as_f64())
            .map(|f| format!("{f:.3}"))
            .unwrap_or_else(|| "—".into());
        println!(
            "| {id} | {mode} | {result} | {pass_k_cell} | {floor_cell} |",
            id = contract["id"].as_str().unwrap_or("?"),
            mode = contract["mode"].as_str().unwrap_or("?"),
            result = contract["decision"].as_str().unwrap_or("?"),
        );
    }
    println!(
        "\n**VTR** {:.1}% · **Coverage** {:.1}%{}{}\n",
        report.vtr * 100.0,
        report.diagnostic_coverage * 100.0,
        report
            .p95_ms
            .map(|p| format!(" · **P95** {p}ms"))
            .unwrap_or_default(),
        report
            .judge_drifted
            .map(|d| format!(" · **Judge** {}", if d { "DRIFTED" } else { "stable" }))
            .unwrap_or_default(),
    );
    if !report.violations.is_empty() {
        println!("### Violations\n");
        for violation in &report.violations {
            println!("- {violation}");
        }
        println!();
    }
    if report.regression_detected {
        println!("### Regression vs last green\n");
        for detail in &report.regression_details {
            println!("- {detail}");
        }
        println!();
    }
}

// ---- history ----

fn record_run_history(
    state: &StateDir,
    command: &str,
    decision: &str,
    reliability: &ReliabilityContractResult,
    verdicts: &[ContractVerdict],
) {
    let contracts = verdicts
        .iter()
        .map(|v| RunContractSummary {
            id: v.id.clone(),
            decision: if v.decision == ReplayDecision::Pass {
                "PASS".into()
            } else {
                "BLOCK".into()
            },
            passed: v.pass_k.map(|(passed, _)| passed),
            k: v.pass_k.map(|(_, k)| k),
        })
        .collect();

    let record = RunRecord {
        ts: chrono::Utc::now().to_rfc3339(),
        command: command.to_string(),
        decision: decision.to_string(),
        vtr: Some(reliability.vtr),
        diagnostic_coverage: Some(reliability.diagnostic_coverage),
        p95_ms: reliability.p95_latency_ms,
        contracts,
    };
    if let Err(err) = append_run(state, &record) {
        eprintln!("Crukx gate: note — could not record run history ({err})");
    }
}
