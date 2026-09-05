# crukx-rs Design Specification

**Date:** 2026-08-21  
**Status:** Implemented (alpha)  
**Version:** 0.1.0-alpha.0

---

## Overview

crukx-rs is a Rust rewrite of the Crukx CLI — a release-gate tool that turns a captured AI-agent session into a regression contract, replays it deterministically, and blocks a release when reliability, security, latency, or consistency constraints aren't met.

## Architecture

### Crate Graph

```
crukx-cli ──→ crukx-tui ──→ crukx-core
    │              │
    ├──→ crukx-runner ──→ crukx-core
    ├──→ crukx-storage ──→ crukx-core
    ├──→ crukx-db ──→ crukx-core
    └──→ crukx-git
```

**crukx-core** is the leaf crate. Everything depends on it, nothing depends back. It has zero I/O, zero process spawning, zero filesystem access. Every public function takes data in and returns data out.

### Module Responsibilities

| Crate | Purpose |
|---|---|
| `crukx-core` | Deterministic reliability engine — pure functions only |
| `crukx-runner` | Process spawning + ExecutionPolicy enforcement |
| `crukx-storage` | Legacy JSON file I/O under `.crukx/` |
| `crukx-db` | SQLite storage layer (replaces crukx-storage) |
| `crukx-git` | Thin git wrappers: repo root, HEAD, tracked diff, sha256 |
| `crukx-tui` | Ratatui terminal interface (`crukx review`) |
| `crukx-cli` | Command entry points; ties the others together |

## Core Concepts

### 1. Events — What Happened

Every action an AI agent takes is recorded as an `AgentEvent`:

```rust
pub struct AgentEvent {
    pub schema_version: u32,
    pub event_id: String,
    pub session_id: String,
    pub sequence: u64,
    pub occurred_at: String,
    pub adapter: AgentAdapter,
    pub repository_root: String,
    pub phase: ActionPhase,
    pub action: AgentAction,
    pub policy: Option<PolicyDecision>,
    pub error: Option<String>,
}
```

`AgentAction` is a tagged enum with variants: `FileRead`, `FileWrite`, `ShellCommand`, `DependencyInstall`, `NetworkRequest`, `GitDiff`, `GitCommit`, `GitPush`, `InfrastructureChange`, `Session`.

### 2. Hash Chains — Evidence Integrity

Each event's hash depends on the previous event's hash, so tampering with any event breaks the chain:

```rust
pub fn compute_chain(events: &[AgentEvent]) -> Vec<String>
pub fn verify_chain(events: &[AgentEvent], chain: &[String]) -> Result<(), usize>
```

### 3. Contracts — What to Replay

A `RegressionContract` captures a session's trajectory and an assertion file:

```rust
pub struct RegressionContract {
    pub schema_version: u32,
    pub contract_id: String,
    pub created_at: String,
    pub source_session_id: String,
    pub source_event_file: String,
    pub expected_trajectory: Vec<ExpectedStep>,
    pub assertion_file: String,
}
```

### 4. Replay — PASS/BLOCK Decisions

`ReplayOutcome` determines whether a contract passes or blocks:

```rust
pub struct ReplayOutcome {
    pub contract_id: String,
    pub candidate_label: String,
    pub decision: ReplayDecision,
    pub trajectory_diverged: bool,
    pub assertion_passed: bool,
    pub steps: Vec<StepResult>,
    pub duration_ms: u64,
}
```

### 5. Gate — Release Decisions

`evaluate_gate` checks all contracts against policy constraints:

```rust
pub fn evaluate_gate(contracts: Vec<ContractVerdict>) -> GateResult
```

### 6. Reliability Dimensions

| Constraint | What it checks |
|---|---|
| `vtr_minimum` | Fraction of contracts that PASSed |
| `diagnostic_coverage_minimum` | Fraction of BLOCKs localized to a pipeline stage |
| `judge_max_agreement_drop` | LLM-judge drift vs calibration baseline |
| `security_critical_delta_max` | New critical security findings |
| `security_high_delta_max` | New high security findings |
| `p95_latency_max_ms` | P95 wall-clock time across replayed steps |
| per-contract `repeat: k` | pass^k — every one of k replays must PASS |
| per-contract `escalation` | Cost-aware ACCEPT/RETRY/ESCALATE |

## Storage

### Current: Flat JSON Files (crukx-storage)

```
.crukx/
├── contracts/
│   └── {id}.assert.mjs
├── events/
│   └── {session-id}.jsonl
├── evaluators/
│   ├── baseline.json
│   └── current.json
├── evidence/
│   └── {contract-id}.json
├── security/
│   ├── before.json
│   └── after.json
├── runs.jsonl
└── chain/
    └── {session-id}.chain.json
```

### Future: SQLite (crukx-db)

SQLite via `rusqlite` with:
- Zero config (single file, no server)
- Embedded (ships with the binary)
- Transactional (concurrent reads, serialized writes)
- `user_version` pragma for schema migrations

## Commands

| Command | Description |
|---|---|
| `crukx` | Status dashboard with local state |
| `crukx doctor` | Verify installation |
| `crukx run -- <cmd>` | Record a session |
| `crukx capture <session>` | Create regression contract |
| `crukx replay <contract>` | Replay a contract |
| `crukx gate --policy crukx.yml` | Release gate evaluation |
| `crukx history` | VTR/P95 trend charts |
| `crukx judge --profile <p>` | LLM-judge calibration |
| `crukx verify-session <id>` | Integrity check |
| `crukx review <session>` | Interactive TUI browser |
| `crukx completion --shell <s>` | Shell completions |

## CI/CD

### CI Workflow (crukx-rs-ci.yml)

Runs on every push/PR to `main`:
- `cargo check --workspace --all-targets`
- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

### Release Workflow (crukx-rs-release.yml)

Runs on tag push (`v*`):
- Builds 5-platform binary matrix:
  - `x86_64-unknown-linux-gnu`
  - `x86_64-unknown-linux-musl`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc`
- Creates GitHub release with checksum-verified binaries

## Distribution

### npm Package (packages/crukx-js)

- Currently: finds and execs locally built binary
- Future: fetches prebuilt binary from GitHub Releases

### GitHub Action (packages/crukx-github-action)

- Thin wrapper around the released binary
- Usage: `uses: crukx/crukx-gate@v1`

## Design Decisions

1. **crukx-core has zero I/O.** Enforced by crate graph, not convention.
2. **TUI renders state and returns intent.** Never executes commands.
3. **pass^k, not pass@k.** All k replays must pass.
4. **Escalation uses real history.** No fabricated signals.
5. **Hash chains for evidence integrity.** Tamper-evident history.
6. **Plain English by default.** Technical terms via progressive disclosure.

## Deferred Work

### OS-Level Sandboxing

`crukx-runner`'s `ExecutionPolicy` enforces what Crukx itself can check, not landlock/seccomp/sandbox-exec confinement. A future `crukx-sandbox` crate will provide real OS-level confinement.

### Judge-vs-System Drift Attribution

`evaluators::attribution` is a standalone pure function that needs a labeled anchor set and a judge-calibration pipeline this crate doesn't own.

## References

- [CTO Handbook](../CTO_HANDBOOK.md)
- [README](../../README.md)
