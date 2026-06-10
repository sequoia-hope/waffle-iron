//! Render tessellation (PR-KV3, Phase 4a): solid → triangle mesh.
//!
//! ## Single canonical path (crate hard rule 5)
//!
//! ONE implementation per surface type — Phase 4a has one surface type
//! (planar), so there is exactly one tessellation routine: exact-rational
//! ear clipping of the face's outer loop with hole loops bridged in. No
//! `reverse_outer` masking, no `bulk_flip`, no force-aligning: the polygon
//! walk direction IS the source of truth, and the emitted triangle winding
//! follows it (triangle normals equal the face's Newell normal by
//! construction, never by post-hoc correction).
//!
//! ## Why ear clipping with exact predicates (documented decision)
//!
//! The reuse-first check required by the KV3 mandate: yang-rs's Stage-1
//! tessellation machinery is **not public** — `yang_rs::BRep::new`
//! tessellates eagerly but exposes neither the per-face triangulation nor
//! the CDT it delegates to (`cherchi_rs::cdt_polygon_with_holes` is public
//! *on cherchi-rs*, which kernel-v2 must not depend on directly, and
//! yang-rs does not re-export it). Render tessellation is, per yang-rs's
//! own scope rules, "entirely out of scope [for yang-rs] — render
//! tessellation is in kernel-v2". So kernel-v2 implements its own planar
//! routine, following the KV2 pattern: **all orientation decisions are
//! exact** (`dashu` rationals — every finite `f64` converts losslessly, so
//! orient2d sign evaluations are decision procedures, not approximations).
//! Plain f64 ear clipping is exactly the silent-wrong failure mode this
//! rewrite exists to eliminate (a mis-signed near-degenerate ear produces
//! an overlapping or inverted triangulation with no error).
//!
//! Boolean results make non-convexity and collinear chain vertices (split
//! edges) the NORMAL case, and holed faces (through-cuts) are first-class:
//! holes are bridged into the outer loop with exactly-validated bridge
//! segments, then the merged (weakly simple) polygon is ear-clipped.
//!
//! ## Output shape
//!
//! [`RenderMesh`] is flat-array oriented for downstream render consumers:
//! `positions`/`normals` are `3·N` coordinate arrays, `indices` is `3·T`
//! vertex indices, and `face_ranges` maps each face to its contiguous
//! index range (per-face vertex duplication — vertices are NOT shared
//! across faces, so per-face flat normals are exact and per-face picking
//! is a range lookup).
//!
//! ## Exactness guarantees (asserted by the KV3 oracles)
//!
//! - Triangle area sums to the face area exactly in rational arithmetic
//!   (ear clipping is an exact partition of the polygon-with-holes); the
//!   f64 oracle tolerance only absorbs summation rounding.
//! - Every triangle winds with the face: its normal direction equals the
//!   face plane normal.
//! - Mesh signed volume equals the solid's B-Rep signed volume (same
//!   region, exact partition).

use crate::arena::{BrepArena, FaceId, SolidId};
use crate::error::KernelV2Error;

/// Flat-array triangle mesh for rendering, with per-face index ranges.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderMesh {
    /// Vertex positions, `[x0, y0, z0, x1, …]` (meters). Vertices are
    /// per-face (not shared across faces).
    pub positions: Vec<f64>,
    /// Per-vertex unit normals, same layout as `positions`. Planar faces
    /// are flat-shaded: every vertex of a face carries the face normal.
    pub normals: Vec<f64>,
    /// Triangle vertex indices into `positions`/`normals`, `3·T` entries.
    pub indices: Vec<u32>,
    /// Per-face contiguous ranges of `indices`, in solid face walk order.
    pub face_ranges: Vec<FaceRange>,
}

impl RenderMesh {
    /// Number of (per-face) vertices.
    pub fn num_vertices(&self) -> usize {
        self.positions.len() / 3
    }

    /// Number of triangles.
    pub fn num_triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

/// One face's contiguous range in [`RenderMesh::indices`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceRange {
    /// The arena face this range tessellates.
    pub face: FaceId,
    /// Offset into `indices` (index entries, not triangles).
    pub start: u32,
    /// Number of index entries (a multiple of 3).
    pub count: u32,
}

/// Tessellate every face of `solid` into a [`RenderMesh`].
///
/// Deterministic: faces in shell walk order, loop points in walk order,
/// exact-arithmetic ear selection with fixed scan order. Errors are loud:
/// a face that cannot be tessellated returns
/// [`KernelV2Error::TessellationFailed`] (never a silent skip, never an
/// f64 guess).
pub fn tessellate(_arena: &BrepArena, _solid: SolidId) -> Result<RenderMesh, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("tessellate"))
}
