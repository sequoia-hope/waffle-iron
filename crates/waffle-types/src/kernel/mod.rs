//! The kernel contract: the `Kernel`/`KernelIntrospect` traits, their shared
//! types (handles, errors, render meshes), and the tolerance constants.
//!
//! Moved here from the legacy `crates/kernel/` at the Phase 6 migration
//! (2026-06-11) so that implementors (`kernel-v2`) and consumers
//! (modeling-ops, feature-engine, wasm-bridge, file-format, test-harness)
//! share the contract without depending on any kernel implementation.
//!
//! `MockKernel` (the deterministic test double) is feature-gated behind
//! `mock-kernel` so production builds — including the WASM bundle — never
//! compile it; enable it from `[dev-dependencies]`.

pub mod traits;
pub mod types;
pub mod units;

#[cfg(feature = "mock-kernel")]
pub mod mock;

pub use traits::*;
pub use types::*;

#[cfg(feature = "mock-kernel")]
pub use mock::MockKernel;
