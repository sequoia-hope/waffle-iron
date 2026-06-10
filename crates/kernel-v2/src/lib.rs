//! Clean-sheet CAD kernel implementation. Replaces `crates/kernel/`.
//!
//! ## Scope
//!
//! - Half-edge B-Rep arena (clean re-implementation) — [`arena`]
//! - Euler operators (`mvfs`, `mev`/`mev_lone`, `mef`, `kemr`, `kfmrh`) with
//!   manifoldness enforced at every operator's exit — [`euler`]
//! - Production validation suite ([`validate::validate_solid`])
//! - (Later phases) primitive constructors, render tessellation, and the
//!   public `Kernel`/`KernelIntrospect` traits; boolean ops delegate to
//!   `yang-rs`
//!
//! ## KV1 status
//!
//! This slice (PR-KV1) implements the foundation: the arena types, the
//! Euler operator set sufficient for planar prismatic solids and
//! through-holes (Stroud 2006, ch. 4 + appendix F), and `validate_solid`.
//! Primitive constructors, tessellation, and the trait surface follow in
//! later slices.
//!
//! ## Invariants enforced at construction
//!
//! - 2-manifold topology only. Operations that would produce non-manifold
//!   edges/vertices return `Err(KernelV2Error::NonManifoldTopology)`.
//! - `face.surface.normal ≡ Newell(face.outer_loop)`. The stored plane
//!   normal is *derived from* the loop walk at every operator exit (the
//!   walk direction is the source of truth), re-asserted by `debug_assert!`
//!   and by `validate_solid`; constructions that would yield an
//!   unorientable face are rejected with `Err(DegenerateFaceNormal)`.
//! - Euler–Poincaré bookkeeping (`V − E + F − R = 2(S − G)`) is
//!   `debug_assert`ed at every operator exit and checked by
//!   `validate_solid`.
//! - No `unsafe`, no `panic!` in production paths, no `catch_unwind`;
//!   stable Rust, wasm32-clean.
//! - No analytical-to-mesh degradation. Surfaces stay analytical;
//!   tessellation (later slice) is an output transformation, never a
//!   storage one.
//!
//! ## Out of scope (deferred indefinitely)
//!
//! - Fillet / chamfer / shell. Per root `CLAUDE.md`. kernel-v2 does NOT
//!   carry the legacy experimental implementations forward.

#![forbid(unsafe_code)]

pub mod arena;
pub mod error;
pub mod euler;
pub mod geom;
pub mod validate;

pub use arena::{
    BrepArena, EulerCounts, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId,
    LoopKind, Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
pub use error::KernelV2Error;
pub use euler::{
    kemr, kfmrh, mef, mev, mev_lone, mvfs, KemrResult, MefResult, MevResult, MvfsResult,
};
pub use validate::{validate_solid, TopologyReport};
