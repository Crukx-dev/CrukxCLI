//! Ports `crukx/packages/cli/src/diagnostics/stages.ts` — D²ACCI-inspired
//! failure localization. Only the stages Crukx can actually observe today
//! (replay only re-executes shell steps and runs one assertion hook) — not
//! the full ten-stage taxonomy, which needs prompt/retrieval/inference
//! capture events this crate doesn't record yet.
pub mod stages;
pub use stages::*;
