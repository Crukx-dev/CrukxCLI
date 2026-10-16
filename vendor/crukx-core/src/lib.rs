//! crukx-core — the deterministic, pure-function reliability engine.
//!
//! No process spawning, no file I/O. Every public function here takes data
//! in and returns data out; `crukx-runner` and `crukx-storage` are the
//! crates that touch the outside world. This is the crate `crukx-tui` and
//! `crukx-cli` both depend on for the actual reliability logic.

pub mod contracts;
pub mod deploy;
pub mod diagnostics;
pub mod escalation;
pub mod evaluators;
pub mod events;
pub mod evidence;
pub mod policy;
pub mod security;

/// ```
/// use crukx_core::policy::{PolicyConstraints, evaluate_reliability_contract, ReliabilityContractInputs};
/// use crukx_core::contracts::ReplayOutcome;
///
/// let outcomes: Vec<ReplayOutcome> = vec![];
/// let result = evaluate_reliability_contract(ReliabilityContractInputs {
///     outcomes: &outcomes,
///     constraints: PolicyConstraints::default(),
///     judge: None,
///     security: None,
/// });
/// assert!(result.pass);
/// ```
fn _doctest_anchor() {}
