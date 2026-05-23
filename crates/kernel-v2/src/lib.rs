//! Clean-sheet CAD kernel implementation. Replaces `crates/kernel/`.
//!
//! ## Scope
//!
//! - Half-edge B-Rep arena (clean re-implementation)
//! - Euler operators (mvfs, mev, mef, kemr, kfmrh) — manifoldness enforced
//!   at every operator's exit
//! - Primitive constructors (extrude, revolve, sphere, cylinder, cone, torus)
//! - Render tessellation (single canonical implementation; no force-aligning
//!   hacks, no `reverse_outer` masking)
//! - Implements the public `Kernel` and `KernelIntrospect` traits used by
//!   wasm-bridge, feature-engine, etc.
//! - Delegates boolean ops to `yang-rs`
//!
//! ## Invariants enforced at construction
//!
//! - 2-manifold topology only. Operations that would produce non-manifold
//!   edges/vertices return `Err(KernelError::NonManifoldTopology)`.
//! - `face.surface_geom.normal ≡ Newell(face.outer_loop)`. The dummy-normal
//!   pattern that Y63 PR-A had to fix in the legacy kernel cannot happen
//!   here because face geometry is inlined on the Face struct and Euler
//!   operators verify the invariant at construction.
//! - No analytical-to-mesh degradation. Surfaces stay analytical; tessellation
//!   is an output transformation, never a storage one.
//!
//! ## Out of scope (deferred indefinitely)
//!
//! - Fillet / chamfer / shell. Per root `CLAUDE.md`. The current legacy
//!   kernel has experimental implementations; kernel-v2 does NOT carry them
//!   forward. The trait may even drop the methods (or leave them returning
//!   `Err(NotSupported)`).

// Skeleton — content fills in during Phase 4.
