# WORKLOG — CrukxCLI

Owner: Varun workstream (infra/backend). Scope: this repo only.
Brief: `CRUKX_VARUN_AI_EXECUTION_BRIEF.md` (Execution Plan v1.0, 8 Oct 2026).
Note: `crx-rs` is out of our control — no Varun task depending on it is started here.

## 2026-10-16 — Session 1: sync + V0 inspection (repo 2 of 3)

### Sync state
- `git pull --ff-only`: OK. HEAD was `afdb492` (Shafwansafi06: distro-matrix CI).
  Working tree was clean; no user changes overwritten.
- This session changed: `Cargo.lock` (engine pins `c857eb9` → `1b00682`)
  plus this file. See "fix" below.

### What exists (evidence)
- CLI crates: `crukx-cli` (benchmark, deploy, worker, verify-artifact,
  verify-session, compare, report, gate, replay…), `crukx-git`, `crukx-tui`.
- Engine comes from private git dep `Crukx-dev/crukx-core` (`branch = main`).
- Varun-brief product code (registry CLI, `crx://` push/pull, scan, telemetry,
  sync transport): **absent** — repo-wide grep for
  `libcrx|crx://|skypilot|encode_step|telemetry` finds only SECURITY.md
  "no telemetry" statements. No `WORKLOG.md` existed before this file.

### Fix found by running tests (not by CI)
- HEAD did not compile: `src/report.rs` (B7 caller, commit `98aa722`) uses
  the new `build_report(&manifest, &evidence, …)` signature, but `Cargo.lock`
  still pinned `crukx-core` at pre-B7 `c857eb9` → E0308 (`expected &str,
  found &BenchmarkReport`). The B7 caller commit never bumped the lock.
- Fix: `cargo update -p crukx-core -p crukx-runner` → pins `1b00682`
  (latest core, incl. sandbox portability fixes). Lock-only change, no code.

### Validation run (this session, after fix)
- `cargo test --workspace`: all green — 46 unit + 14 e2e files
  (report_e2e 2/2 incl. new-signature path) + 7 crukx-git + 22 TUI = 118 passed, 0 failed.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings` (with `RUSTFLAGS="-D warnings"`): clean.
- Remote CI on pushed HEAD: red — pre-existing `CORE_GIT_TOKEN` secret
  missing in every job (fails at auth step before compiling). Unrelated to
  this fix, but note: even with the secret restored, CI would have failed
  to compile without the lock bump — now covered.

### Blockers / unblock requests
- Same as repo 1: Shafwan-side interfaces (`libcrx`, DNA, sync protocol,
  telemetry schema v1) missing; `CORE_GIT_TOKEN` needs a repo admin.

### Next action
- Repo 3 of 3: `crukx-db` — check whether its `Cargo.lock` has the same
  stale-pin problem, run Rust tests + `tsc --noEmit`, write its `WORKLOG.md`.
- No push performed (brief §7: pushing needs explicit permission).
  Pending push when authorized: this WORKLOG commit + the `Cargo.lock` bump
  (the lock fix is required for CI to ever compile).
