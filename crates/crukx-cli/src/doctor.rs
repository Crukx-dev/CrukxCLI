//! `crukx doctor` — one command that answers "is my install healthy?"
//! the way `gh auth status` / `opencode doctor` do: a checklist with
//! ✓/✗ per check, exit 0 only when every *critical* check passes.
//! Optional checks (API key, network) report but never fail the run.

use crate::colors::{self, color_enabled};
use crate::ui;
use crukx_storage::state_dir::StateDir;
use std::path::Path;
use std::process::Command;

struct Check {
    name: &'static str,
    critical: bool,
    passed: bool,
    detail: String,
}

pub fn run_doctor(cwd: &Path) -> i32 {
    let color = color_enabled();
    println!(
        "{}",
        ui::header("Crukx doctor", Some(env!("CARGO_PKG_VERSION")), color)
    );
    println!();

    let mut checks = Vec::new();

    // Binary itself: if doctor runs, the binary works — report platform.
    checks.push(Check {
        name: "crukx binary",
        critical: true,
        passed: true,
        detail: format!(
            "{} · {}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS
        ),
    });

    // Git: sessions capture repository state.
    let git_ok = Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    checks.push(Check {
        name: "git",
        critical: true,
        passed: git_ok,
        detail: if git_ok {
            "present".into()
        } else {
            "not found — sessions can't record repository state".into()
        },
    });

    // Node-compatible runtime: assertion files are .mjs.
    let node_ok = ["node", "bun", "deno"]
        .iter()
        .find(|bin| Command::new(bin).arg("--version").output().map(|o| o.status.success()).unwrap_or(false))
        .copied();
    checks.push(Check {
        name: "assertion runtime (.mjs)",
        critical: false,
        passed: node_ok.is_some(),
        detail: match node_ok {
            Some(bin) => format!("{bin} found"),
            None => "no node/bun/deno — JS assertions can't run (shell trajectory checks still work)".into(),
        },
    });

    // State dir writable.
    let state = StateDir::resolve(cwd);
    let state_writable = std::fs::create_dir_all(state.root()).is_ok()
        && std::fs::write(state.root().join(".doctor-probe"), b"ok").is_ok()
        && std::fs::remove_file(state.root().join(".doctor-probe")).is_ok();
    checks.push(Check {
        name: "state dir",
        critical: true,
        passed: state_writable,
        detail: format!("{} {}", state.root().display(), if state_writable { "writable" } else { "NOT writable" }),
    });

    // Repo context.
    let in_repo = Command::new("git")
        .arg("rev-parse")
        .arg("--git-dir")
        .current_dir(cwd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    checks.push(Check {
        name: "git repository",
        critical: false,
        passed: in_repo,
        detail: if in_repo {
            "project is a repository".into()
        } else {
            format!("{} is not inside a git repository", cwd.display())
        },
    });

    // API key — optional, needed for --upload / judge models.
    let has_key = std::env::var_os("CRUKX_API_KEY").is_some_and(|v| !v.is_empty());
    checks.push(Check {
        name: "CRUKX_API_KEY",
        critical: false,
        passed: has_key,
        detail: if has_key {
            "set (upload + judge active)".into()
        } else {
            "not set — needed only for `gate --upload` and judge evaluators".into()
        },
    });

    for check in &checks {
        let mark = if check.passed {
            ui::ok_mark(color)
        } else if check.critical {
            ui::fail_mark(color)
        } else {
            ui::warn_mark(color)
        };
        println!("  {mark} {:<28} {}", check.name, ui::dim_label(&check.detail, color));
    }

    let critical_failed = checks.iter().filter(|c| c.critical && !c.passed).count();
    println!();
    if critical_failed > 0 {
        println!(
            "{}",
            colors::bold(&format!("doctor: {critical_failed} critical check(s) failed"), color)
        );
        1
    } else {
        println!(
            "{}",
            colors::bold("Crukx doctor: all critical checks passed", color)
        );
        0
    }
}