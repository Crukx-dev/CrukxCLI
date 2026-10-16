//! Ports `crukx/packages/cli/src/contracts/` — the regression-contract wire
//! format and (in a later plan) the code that replays one against the
//! current repo state.
pub mod artifact;
pub mod capture;
pub mod jobs;
pub mod replay;
pub mod schema;
pub use artifact::*;
pub use capture::*;
pub use jobs::*;
pub use replay::*;
pub use schema::*;
