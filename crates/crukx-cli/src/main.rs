//! crukx-cli — command entry points.
//!
//! `run`, `capture`, `replay`, `gate`, and `review` are wired — every
//! command the TS CLI has.

mod assertion;
mod banner;
mod capture;
mod charts;
mod colors;
mod gate;
mod history;
mod replay;
mod review;
mod run;
mod run_replay;
mod ui;
mod verify_session;

use clap::{CommandFactory, Parser, Subcommand};
use crukx_core::events::AgentAdapter;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "crukx",
    version,
    about = "Crukx — security gate for AI-written software",
    long_about = None,
    after_help = EXAMPLES
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

const EXAMPLES: &str = "Examples:
  crukx run -- npm test        Run a command as a captured Crukx session
  crukx capture                Turn the latest session into a contract
  crukx replay demo-1 -r 5     Replay a contract 5 times, report pass^5
  crukx gate                   PASS/BLOCK every contract in crukx.yml
  crukx history                Chart VTR/P95 trends across recent runs";

#[derive(Subcommand)]
enum Command {
    /// Run a command as a captured Crukx session.
    Run {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        command: Vec<String>,
    },
    /// Turn a captured session into a regression contract (defaults to latest).
    Capture {
        #[arg(default_value = "latest")]
        session_id: String,
    },
    /// Replay one contract's trajectory + run its assertion. --repeat > 1
    /// re-runs it k times and reports pass^k (all k must PASS), not just
    /// pass@k — a flaky contract that passes 4/5 times is not consistent.
    Replay {
        contract_id: String,
        #[arg(long, default_value = "unlabeled")]
        candidate: String,
        #[arg(long, default_value_t = 1)]
        repeat: usize,
    },
    /// Replay every contract in crukx.yml, PASS/BLOCK.
    Gate {
        #[arg(long, default_value = "crukx.yml")]
        policy: PathBuf,
        /// Output surface: human (colored stderr), json, junit (CI test
        /// reporters), or markdown (PR comments). Machine formats go to
        /// stdout; human stays on stderr.
        #[arg(long, value_enum, default_value = "human")]
        format: gate::OutputFormat,
        /// Exit 1 even when the gate PASSes but metrics/contracts regressed
        /// versus the last green run — catches slow decay absolute
        /// thresholds can't see.
        #[arg(long)]
        fail_on_regression: bool,
    },
    /// Interactive TUI over a captured session (defaults to latest).
    Review {
        #[arg(default_value = "latest")]
        session_id: String,
    },
    /// Verify a session's event log is untampered since `crukx run` recorded it.
    VerifySession { session_id: String },
    /// Chart VTR/P95 trends from recent gate/replay runs.
    History {
        /// How many of the most recent runs to show.
        #[arg(short, long, default_value_t = 15)]
        last: usize,
        /// Dump the raw run records as JSON instead of charts (for scripts/CI).
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    let Some(command) = cli.command else {
        // Bare `crukx`, nothing after it — the same first-impression
        // moment `claude`/`gh`/`docker` give you. Every other invocation
        // (including `--help`, which clap intercepts before this point)
        // stays plain and script-safe.
        print!(
            "{}",
            banner::render(env!("CARGO_PKG_VERSION"), colors::color_enabled())
        );
        println!();
        Cli::command().print_help().ok();
        println!();
        std::process::exit(2);
    };

    let cwd = std::env::current_dir().expect("failed to read current directory");

    let exit_code = match command {
        Command::Run { command } => {
            // clap leaves a literal "--" separator in place for
            // trailing_var_arg; strip it if present so `crukx run -- npm
            // test` and `crukx run npm test` behave identically.
            let mut parts = command.into_iter();
            let executable = match parts.next() {
                Some(first) if first == "--" => match parts.next() {
                    Some(executable) => executable,
                    None => {
                        eprintln!("Usage: crukx run -- <command> [arguments]");
                        std::process::exit(2);
                    }
                },
                Some(executable) => executable,
                None => {
                    eprintln!("Usage: crukx run -- <command> [arguments]");
                    std::process::exit(2);
                }
            };
            let arguments: Vec<String> = parts.collect();
            let result = run::run_session(AgentAdapter::Direct, executable, arguments, &cwd);
            eprintln!("\nCrukx session: {}", result.session_id);
            eprintln!("Local evidence: {}", result.event_file.display());
            result.exit_code
        }
        Command::Capture { session_id } => capture::run_capture(&session_id, &cwd),
        Command::Replay {
            contract_id,
            candidate,
            repeat,
        } => run_replay::run_replay(&contract_id, &candidate, repeat, &cwd),
        Command::Gate {
            policy,
            format,
            fail_on_regression,
        } => gate::run_gate(&policy, &cwd, format, fail_on_regression),
        Command::Review { session_id } => review::run_review(&session_id, &cwd),
        Command::VerifySession { session_id } => {
            verify_session::run_verify_session(&session_id, &cwd)
        }
        Command::History { last, json } => history::run_history(last, json, &cwd),
    };

    std::process::exit(exit_code);
}
