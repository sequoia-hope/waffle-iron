//! Constrained Delaunay triangulation (CDT) of planar polygons with holes.
//!
//! Backend: the `spade` v2 crate (pure-Rust, WASM-clean — exact
//! orientation / in-circle predicates via `robust`, fixed-seed `foldhash`,
//! single-threaded, no rand / time). Boundary loops (the outer loop plus each
//! hole) are inserted as **hard constraint edges**; only triangles interior to
//! the outer loop and exterior to every hole are returned.
//!
//! Algorithm class: Constrained Delaunay Triangulation. This is *not* plain
//! ear-clipping (forbidden per `docs/yang_deviations.md` D1). The same
//! primitive will back the Cherchi 2022 §4 coplanar handler at roadmap M6, so
//! it lives in `cherchi-rs` (whose curation bar admits exact-predicate deps)
//! rather than in the `yang-rs` consumer.
//!
//! Cherchi 2022 §4 (coplanar / 2D arrangement handling) — future reuse.
//! `spade` is MIT/Apache-2.0 dual-licensed; see `LICENSE-THIRD-PARTY.md`.

use cad_primitives::Point2;
use std::fmt;

/// Error returned by [`cdt_polygon_with_holes`].
///
/// All variants describe caller-supplied data errors or an internal
/// triangulation failure. No production path panics — every failure is a
/// `Result::Err`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdtError {
    /// Input is geometrically degenerate (e.g. the outer loop has fewer than 3
    /// vertices, is collinear, or encloses zero area).
    DegenerateInput,
    /// Two distinct loop indices reference coincident `verts` positions, so the
    /// constraint graph cannot be built without a self-coincident edge.
    DuplicateVertex,
    /// A loop index references a `verts` position that does not exist
    /// (`index >= verts.len()`).
    LoopIndexOutOfRange,
    /// The CDT backend failed to produce a valid triangulation (constraint
    /// insertion conflict, non-simple boundary, etc.).
    TriangulationFailed,
}

impl fmt::Display for CdtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CdtError::DegenerateInput => write!(
                f,
                "degenerate CDT input (too few / collinear / zero-area loop)"
            ),
            CdtError::DuplicateVertex => {
                write!(f, "duplicate (coincident) loop vertex in CDT input")
            }
            CdtError::LoopIndexOutOfRange => {
                write!(f, "loop index out of range of the vertex array")
            }
            CdtError::TriangulationFailed => write!(f, "CDT backend failed to triangulate"),
        }
    }
}

impl std::error::Error for CdtError {}

/// Constrained Delaunay triangulate a planar polygon with holes.
///
/// * `verts` — the shared 2D vertex pool (boundary vertices only; no Steiner
///   points are added).
/// * `outer` — indices into `verts` of the outer loop, in order.
/// * `holes` — each inner vector is one hole's loop, indices into `verts`.
///
/// Returns the **interior** triangles (inside the outer loop AND outside every
/// hole) as index triples into `verts`. The output vertex set equals the input
/// boundary vertex set — no interior Steiner points, no boundary subdivision —
/// so a `TessellationMap` 1:1-on-boundary bijection is preserved. Deterministic:
/// two calls on the same input produce an identical `Vec<[u32; 3]>`.
///
/// # PR-NC1 RED stub
///
/// This is the RED-phase stub: it does no work and always returns
/// `Err(CdtError::TriangulationFailed)`. The GREEN sub-agent implements the
/// real CDT here. The test author writes NO production logic.
pub fn cdt_polygon_with_holes(
    verts: &[Point2],
    outer: &[u32],
    holes: &[Vec<u32>],
) -> Result<Vec<[u32; 3]>, CdtError> {
    // PR-NC1 RED stub — GREEN sub-agent implements this.
    let _ = (verts, outer, holes);
    Err(CdtError::TriangulationFailed)
}
