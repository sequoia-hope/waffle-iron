//! Legacy libslvs-based solver — dev-only parity oracle.
//!
//! Feature-gated behind `legacy-oracle`. Never compiled by default.
//! Used by tests/parity.rs to compare the clean-room solver against
//! the original libslvs implementation. Deleted when parity validation
//! is complete.

pub mod constraint_mapping;
pub mod entity_mapping;
pub mod solver;
pub mod status;

pub use solver::legacy_solve_sketch;
