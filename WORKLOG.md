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

## 2026-10-16 — Session 3: green CI without secrets (vendor engine)

### Task (user: "make CI pass without admin access")
- Root-caused the red CI: `CORE_GIT_TOKEN` secret missing AND repo is
  genuinely private (proved via isolated `git ls-remote` → auth required;
  earlier success was the local `gh` credential helper, not anonymous access).
- Fix: vendored the pinned engine crates into `vendor/` (408 KB) +
  `.cargo/config.toml` source replacement + `.gitattributes` (`-text` so
  checksums survive Windows checkouts) + `vendor/README.md` refresh recipe.
  Deleted the 2 hard-failing auth steps from `crukx-rs-ci.yml`.
- Subtlety found: `[source]` table needs `git` + `branch` keys alongside
  `replace-with`, else cargo errors "no source location specified".
- Proof (not assumption): rebuilt engine crates with `~/.cargo/git` moved
  away AND github.com redirected to a dead host — build succeeded, so the
  engine provably comes from `vendor/`, never the network.
- Follow-up fix in same session: `gate_pass_k_e2e` flaky test failed ONLY
  on fedora:41 — its "flakiness" source was child-PID parity, which scored
  8/8 even PIDs there. Replaced with a deterministic counter-file script
  (7/8 → BLOCK, verified manually: `pass^8: 7/8`, `BLOCKED`, exit 1).

### Validation
- Local: full suite green (all binaries `0 failed`), fmt + clippy clean,
  new test 5/5 stable.
- Remote CI on push `6e152eb`: **10/10 success** incl. fedora:41, Windows,
  macOS, arm64, all distro containers. `crukx-db` also 10/10 on its vendor
  push. `crukx-core` green throughout. No secret added anywhere.

### Maintenance warning
- Downstream CI now tests the *vendored* engine snapshot, not live main.
  After any core change: run the `vendor/README.md` refresh + lock bump in
  CLI and db, or CI silently tests stale code. Same cadence as lock bumps.

## 2026-10-19 — Session 4: vendor/lock sync fix + CI green

### Task (user: "there are bugs. fix them")
Found and fixed the real bug: **vendor/Cargo.lock drift**. CLI `Cargo.lock`
pinned crukx-core at `1b00682` but core main had advanced to `4f8c757`
(SkyPilot commit, no Rust changes but new files). If anyone ran
`cargo update -p crukx-core` downstream, Cargo.lock would point to
`4f8c757` which wasn't in `vendor/`, and the build would fail without
git credentials.

### Fix
- `cargo update -p crukx-core -p crukx-runner` → pins `4f8c757`
- Re-vendored engine crates at `4f8c757` (proven: rebuilds with
  `~/.cargo/git` removed and github.com unreachable)
- Updated `vendor/README.md` pinned rev

### Validation
- `cargo test --workspace`: ALL_GREEN, fmt clean, clippy clean
- Remote CI on push `f30a3cd`: **10/10 success** (all OSes + 4 distros)

### Next action
- All three repos in sync, CI green. V1–V4 still blocked on Shafwan's
  interfaces (draft at `crukx-core/INTERFACE_REQUESTS_DRAFT.md`, unsent —
  needs your approval per brief §7).

## 2026-10-21 — Session 5: audit fixes (worker claim/patch, engine re-vendor)

### Task (user: "check the codebase for bugs and issues. fix and push")
Repo-wide audit (findings cross-checked against code before fixing) turned
up one local bug plus engine fixes that had to be re-vendored in.

### Fix
- `crates/crukx-cli/src/worker.rs`: `claim_job` trusted any non-204 response
  as claimed work — a 2xx body that wasn't a job (error envelope, proxy
  page) became a phantom `job_id: "unknown"` and a PATCH to `/api/jobs/unknown`.
  Claim now requires 200 AND a `job_id` before anything runs; `patch_job`
  checks its status explicitly instead of relying on the HTTP client's error
  policy, so a lost job report can never look like success. (Note: with
  ureq 2.x, 4xx/5xx already surfaced as errors — the gap was 2xx-but-not-200
  and unvalidated bodies.)
- 3 new e2e tests with an in-test mock HTTP server:
  `worker_rejects_a_non_job_claim_body`, `worker_rejects_an_unexpected_status_as_a_job`,
  `worker_claims_runs_and_reports_a_real_job`.
- Re-vendored engine at `bbefad1` (Cargo.lock + vendor/ + README rev):
  picks up the crukx-core fixes in crukx-runner (pipe deadlock: child output
  >64KB became a spurious Timeout; truncate_output UTF-8 panic) and
  crukx-storage (verify-session latest chain path; unreadable-line tamper
  gap; append_event path traversal; atomic chain writes).

### Validation
- `cargo test --workspace` ALL_GREEN (worker e2e 6/6), clippy
  `--all-targets -D warnings` clean, fmt clean
- vendored copies byte-identical to crukx-core@bbefad1

### Next action
- All three repos in sync again. crukx-cloud audit fixes land in crukx-db.
