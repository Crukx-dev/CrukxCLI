//! Ports `crukx/packages/cli/src/events/schema.ts` — the wire format for one
//! recorded agent action. Schema v2: snake_case, real enums instead of TS
//! string unions, `GitRefAction`'s two-purpose `type` field split into
//! `GitCommit`/`GitPush`. `chain` adds tamper-evidence (hash-chained
//! history), inspired by "history integrity" as a named guarantee in the
//! runtime-verification literature (arxiv 2607.05397) — the TS gateway's
//! `AuditEntry` already carries `hash`/`previousHash` fields for this same
//! reason; the event log crukx-rs actually writes didn't have it yet.
pub mod chain;
pub mod schema;
pub use chain::*;
pub use schema::*;
