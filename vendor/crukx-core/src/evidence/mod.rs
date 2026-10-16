//! Ports `crukx/packages/cli/src/evidence/graph.ts` — LEDGER-inspired
//! claim-to-evidence graph, scoped to what one replay actually produces
//! today: a claim ("this contract still holds"), the shell steps that ran,
//! and the assertion that checked the resulting state. No fabricated nodes
//! (state_before/after, retrieval, prompt) — those require capture events
//! this crate doesn't record yet.
pub mod graph;
pub use graph::*;
