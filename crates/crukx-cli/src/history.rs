//! `crukx history` — renders the run ledger (.crukx/runs.jsonl) as
//! terminal charts: a sparkline of the metric over recent runs plus one
//! row per run. This is the "graphs in your terminal" surface: no
//! dashboard, no browser, just the same data `gate` already produced,
//! shaped so a regression is visible at a glance.
//!
//! With --json the raw records are dumped instead, for scripts and CI.

use crate::charts;
use crate::colors::{self, color_enabled};
use crate::ui;
use crukx_storage::runs::{read_runs, RunRecord};
use crukx_storage::state_dir::StateDir;
use std::path::Path;

pub fn run_history(last: usize, json: bool, cwd: &Path) -> i32 {
    let state = StateDir::resolve(cwd);
    let color = color_enabled();

    let records = match read_runs(&state, last) {
        Ok(records) => records,
        Err(err) => {
            eprintln!("Crukx history: failed to read run ledger: {err}");
            return 1;
        }
    };

    if json {
        match serde_json::to_string_pretty(&records) {
            Ok(out) => println!("{out}"),
            Err(err) => {
                eprintln!("Crukx history: failed to serialize records: {err}");
                return 1;
            }
        }
        return 0;
    }

    if records.is_empty() {
        eprintln!(
            "{}",
            ui::header("Crukx history", Some("no runs recorded yet"), color)
        );
        eprintln!(
            "{}",
            ui::hint(
                "run `crukx gate` or `crukx replay` — each run appends to .crukx/runs.jsonl",
                color
            )
        );
        return 0;
    }

    let context = format!(
        "last {} · {}",
        records.len(),
        crukx_storage::runs::runs_file(&state).display()
    );
    eprintln!("{}", ui::header("Crukx history", Some(&context), color));
    eprintln!();

    print_metric_strip(
        "VTR ",
        &vtr_series(&records),
        |v| format!("{:.0}%", v * 100.0),
        true,
        color,
    );

    let p95s: Vec<Option<f64>> = records.iter().map(|r| r.p95_ms.map(|ms| ms as f64)).collect();
    if p95s.iter().any(|p| p.is_some()) {
        print_metric_strip("P95 ", &series_filled(&p95s), format_ms, false, color);
    }

    eprintln!();
    for (index, record) in records.iter().enumerate() {
        print_run_row(index + 1, record, color);
    }

    0
}

fn vtr_series(records: &[RunRecord]) -> Vec<f64> {
    records.iter().filter_map(|r| r.vtr).collect()
}

fn series_filled(values: &[Option<f64>]) -> Vec<f64> {
    values.iter().filter_map(|v| *v).collect()
}

/// One sparkline strip with latest value and direction. `higher_is_better`
/// picks which end of the ramp counts as an improvement for the arrow.
fn print_metric_strip(label: &str, values: &[f64], fmt: impl Fn(f64) -> String, higher_is_better: bool, color: bool) {
    if values.is_empty() {
        return;
    }
    let spark = charts::sparkline(values);
    let latest = values[values.len() - 1];
    let trend = if values.len() >= 2 {
        let earlier: f64 = values[..values.len() - 1].iter().sum::<f64>()
            / (values.len() - 1) as f64;
        let improved = if higher_is_better {
            latest > earlier
        } else {
            latest < earlier
        };
        let worse = if higher_is_better {
            latest < earlier
        } else {
            latest > earlier
        };
        if improved {
            colors::green("↑", color)
        } else if worse {
            colors::red("↓", color)
        } else {
            colors::dim("→", color)
        }
    } else {
        colors::dim("·", color)
    };
    eprintln!(
        "{label} {}   {} {} latest",
        colors::bold(&spark, color),
        trend,
        fmt(latest)
    );
}

fn print_run_row(number: usize, record: &RunRecord, color: bool) {
    let ts_display = chrono::DateTime::parse_from_rfc3339(&record.ts)
        .map(|ts| ts.format("%b %d %H:%M").to_string())
        .unwrap_or_else(|_| record.ts.clone());

    let (mark, verdict) = if record.decision == "PASS" {
        (ui::ok_mark(color), colors::green("PASS", color))
    } else {
        (ui::fail_mark(color), colors::red("BLOCKED", color))
    };

    let mut detail_parts: Vec<String> = Vec::new();
    if let Some(vtr) = record.vtr {
        detail_parts.push(format!("VTR {:.0}%", vtr * 100.0));
    }
    if let Some(p95) = record.p95_ms {
        detail_parts.push(format!("p95 {}", format_ms(p95 as f64)));
    }

    eprintln!(
        "  {:>3}  {}  {:<6}  {} {}{}",
        format!("#{number}"),
        colors::dim(&ts_display, color),
        record.command,
        mark,
        verdict,
        if detail_parts.is_empty() {
            String::new()
        } else {
            format!("  {}", colors::dim(&detail_parts.join("  "), color))
        }
    );
}

fn format_ms(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.1}s", ms / 1000.0)
    } else {
        format!("{ms:.0}ms")
    }
}
