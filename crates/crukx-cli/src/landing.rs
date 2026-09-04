//! `crukx` with no subcommand, in a project that already has `.crukx/`.
//! This is the "open the app" moment: a status dashboard showing what's
//! in the local state — contracts, recent run verdicts, judge/security
//! presence — plus the next-action hints. In a fresh directory (no
//! `.crukx`) it stays the onboarding banner + help instead, because a
//! dashboard over nothing is noise.

use crate::banner;
use crate::charts;
use crate::colors::{self, color_enabled};
use crate::ui;
use crukx_storage::runs::{read_runs, RunRecord};
use crukx_storage::state_dir::StateDir;
use std::path::Path;

pub fn run_landing(cwd: &Path) -> i32 {
    let state = StateDir::resolve(cwd);
    if !state.root().is_dir() {
        // Fresh directory — onboarding moment, same as before.
        print!(
            "{}",
            banner::render(env!("CARGO_PKG_VERSION"), colors::color_enabled())
        );
        println!();
        return 2;
    }

    let color = color_enabled();
    println!(
        "{}",
        ui::header(
            "Crukx",
            Some(&format!("{} · {}", cwd.display(), env!("CARGO_PKG_VERSION"))),
            color
        )
    );
    println!();

    let contracts = count_contracts(&state);
    let sessions = sessions_count(&state);
    println!(
        "  {}",
        ui::dim_label(
            &format!(
                "{} contract{}, {} session{}, {}",
                numbers_label(contracts, color),
                if contracts == 1 { "" } else { "s" },
                numbers_label(sessions, color),
                if sessions == 1 { "" } else { "s" },
                judge_status(&state, color),
            ),
            color
        )
    );
    println!();

    match read_runs(&state, 6) {
        Ok(runs) if !runs.is_empty() => {
            println!("{}", ui::section("Recent runs", color));
            print_recent_runs(&runs, color);
            println!();
        }
        _ => {
            println!(
                "{}",
                ui::hint("no runs yet — `crukx run -- <command>` records a session, `crukx gate` checks contracts", color)
            );
            println!();
        }
    }

    println!("{}", ui::hint("`crukx doctor` checks the install · `crukx --help` lists commands", color));
    0
}

fn numbers_label(count: usize, color: bool) -> String {
    crate::colors::bold(&count.to_string(), color)
}

fn count_contracts(state: &StateDir) -> usize {
    std::fs::read_dir(state.contracts_dir())
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .is_some_and(|ext| ext == "json")
                })
                .count()
        })
        .unwrap_or(0)
}

fn sessions_count(state: &StateDir) -> usize {
    std::fs::read_dir(state.events_dir())
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
                .count()
        })
        .unwrap_or(0)
}

fn judge_status(state: &StateDir, color: bool) -> String {
    let has_baseline = state
        .evaluators_dir()
        .join("baseline.json")
        .exists();
    if has_baseline {
        ui::ok_mark(color) + " judge calibrated"
    } else {
        ui::dim_label("no judge baseline", color)
    }
}

fn print_recent_runs(runs: &[RunRecord], color: bool) {
    let vtrs: Vec<f64> = runs.iter().filter_map(|r| r.vtr).collect();
    if vtrs.len() >= 2 {
        println!(
            "  trend {}",
            ui::dim_label(&charts::sparkline(&vtrs), color)
        );
    }
    for run in runs.iter().rev().take(5) {
        let mark = if run.decision == "PASS" {
            ui::ok_mark(color)
        } else {
            ui::fail_mark(color)
        };
        println!(
            "  {} {}  {}  {}",
            mark,
            ui::dim_label(&short_ts(&run.ts), color),
            run.command,
            run.decision
        );
    }
}

fn short_ts(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|ts| ts.format("%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| rfc3339.to_string())
}