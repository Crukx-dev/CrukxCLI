//! Ports `crukx/packages/cli/src/security/delta.ts` — WeSCE-inspired:
//! measures security drift between two scans of the same code instead of
//! an absolute security score. Refactors and "harmless" feature work move
//! vulnerability posture even when nobody asked for a security change, so
//! every edit gets scored, not just ones that touch auth/crypto/etc.
pub mod delta;
pub mod pathsafe;
pub use delta::*;
pub use pathsafe::*;
