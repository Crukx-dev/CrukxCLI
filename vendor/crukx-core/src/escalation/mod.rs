//! Ports `crukx/packages/cli/src/escalation/policy.ts` — cost-aware
//! escalation, inspired by the failure-prediction paper's finding: a
//! simple confidence gate beat an elaborate LLM router that tried to pick
//! the "best" collaboration protocol per case. Three actions, no router.
pub mod policy;
pub use policy::*;
