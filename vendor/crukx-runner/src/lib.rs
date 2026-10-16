//! crukx-runner — process spawning and ExecutionPolicy enforcement.
//!
//! v1 scope (see docs/superpowers/specs/2026-08-21-crukx-rs-design.md):
//! policy-aware logic Crukx itself enforces — refusing actions the policy
//! disallows, stripping network-relevant env vars — not real OS-level
//! sandboxing (landlock/seccomp/sandbox-exec). That's the deferred
//! crukx-sandbox crate.

pub mod benchmarks;
pub mod devices;
pub mod exporters;
pub mod policy;
pub mod runtimes;
pub mod selection;
pub mod spawn;
pub use devices::*;
pub use policy::*;
pub use spawn::*;
