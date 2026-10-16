//! Ports `crukx/packages/cli/src/policy/` — the `crukx.yml` shape, gate
//! aggregation, and the CAPO-inspired reliability contract. `performance`
//! adds a P95-latency release dimension on top, wired to real measured
//! spawn durations. `consistency` adds pass^k / VTRₖ, wired to real
//! repeated replay runs (crukx-cli actually re-spawns k times).
pub mod consistency;
pub mod gate;
pub mod performance;
pub mod reliability_contract;
pub mod schema;
pub use consistency::*;
pub use gate::*;
pub use performance::*;
pub use reliability_contract::*;
pub use schema::*;
