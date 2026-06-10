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
//! ## Status
//!
//! - **PR-KV1** (done): the arena types, the Euler operator set sufficient
//!   for planar prismatic solids and through-holes (Stroud 2006, ch. 4 +
//!   appendix F), and `validate_solid`.
//! - **PR-KV2** (done): planar primitive constructors — [`Profile`]
//!   (validated planar region, exact simplicity check),
//!   [`make_face_from_profile`] (lamina), [`extrude`] (linear sweep with
//!   through-holes; Stroud 2006 §6.2), plus the [`geom::signed_volume`]
//!   orientation oracle.
//! - **PR-KV3** (this slice): boolean ops via yang-rs ([`boolean`] —
//!   B-Rep conversion at the kernel-v2/yang-rs boundary + typed error
//!   mapping), render tessellation ([`tessellate`] — exact-rational ear
//!   clipping with hole bridging), and introspection basics
//!   ([`introspect`] — edge extraction, surface area, face plane).
//! - Later slices: revolve + curved primitives, curved tessellation, and
//!   the trait surface.
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
pub mod boolean;
pub mod construct;
pub mod error;
pub mod euler;
pub mod geom;
pub mod introspect;
pub mod profile;
pub mod tessellate;
pub mod validate;

pub use arena::{
    BrepArena, EulerCounts, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId,
    LoopKind, Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
pub use boolean::{boolean_op, from_yang_brep, to_yang_brep};
pub use construct::{extrude, make_face_from_profile, ExtrudeResult, LaminaResult};
pub use error::KernelV2Error;
pub use euler::{
    kemr, kfmrh, mef, mev, mev_lone, mvfs, KemrResult, MefResult, MevResult, MvfsResult,
};
pub use introspect::{extract_edges, face_plane, surface_area};
pub use profile::Profile;
pub use tessellate::{tessellate, FaceRange, RenderMesh};
pub use validate::{validate_solid, TopologyReport};
