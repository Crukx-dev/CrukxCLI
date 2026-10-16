//! crukx-storage — schema v2 JSON (and one YAML file: crukx.yml) read/write
//! under `.crukx/`. No SQLite — files-under-a-directory is what's tested;
//! add a database when file-per-session concurrency actually hurts, not
//! preemptively (see docs/superpowers/specs/2026-08-21-crukx-rs-design.md).

pub mod contracts;
pub mod evaluators;
pub mod events;
pub mod evidence;
pub mod json;
pub mod policy_file;
pub mod runs;
pub mod security;
pub mod state_dir;

pub use contracts::*;
pub use evaluators::*;
pub use events::*;
pub use evidence::*;
pub use json::*;
pub use policy_file::*;
pub use runs::*;
pub use security::*;
pub use state_dir::*;
