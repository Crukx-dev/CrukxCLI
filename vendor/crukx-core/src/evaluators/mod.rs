//! Ports `crukx/packages/cli/src/evaluators/registry.ts` — Netflix
//! LLM-judge-lifecycle-inspired: treats an LLM judge as a maintained
//! production model, not ground truth. `attribution` extends that with a
//! second reliability-research primitive (arxiv 2606.15474): distinguishing
//! whether a drift alarm means the judge changed or the system did.
pub mod attribution;
pub mod registry;
pub use attribution::*;
pub use registry::*;
