//! Shared geometric primitives and constants for the clean-sheet kernel rewrite.
//!
//! This crate is the foundation depended on by `cherchi-rs`, `ssi-rs`,
//! `yang-rs`, and `kernel-v2`. It holds **types only** — no algorithms.
//!
//! ## Scope discipline
//!
//! Things that belong here:
//! - Geometric primitive types (`Point3`, `Vector3`)
//! - Distance/angle tolerance constants (`TAU_MODEL`, `MIN_FEATURE_SIZE`)
//! - Boolean operation enum (`BoolOp`)
//! - Cross-crate error type (`KernelError`)
//!
//! Things that do NOT belong here:
//! - Mesh data structures (live in `cherchi-rs` or `yang-rs`'s internal mesh)
//! - B-Rep data structures (live in `kernel-v2`)
//! - Any algorithm — predicates, intersections, tessellation, anything
//!
//! When in doubt: if it has a `fn` doing computation, it does not belong here.

// Skeleton — content fills in during Phase 1+.
