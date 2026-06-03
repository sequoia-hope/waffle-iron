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

use cad_primitives::Point2 as CadPoint2;
use spade::handles::FixedVertexHandle;
use spade::{
    ConstrainedDelaunayTriangulation, InsertionError, Point2 as SpadePoint2, Triangulation,
};
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
/// # Determinism
///
/// `spade` is deterministic given a fixed insertion order (it uses a fixed-seed
/// `foldhash` and exact `robust` predicates — no threads, rand, or system
/// time). We insert vertices in the caller's array order. To guarantee
/// byte-identical output regardless of spade's internal face-iteration order,
/// the returned triangle list is canonicalized: each triangle's three indices
/// are rotated so the smallest index is first (winding preserved), then the
/// whole list is sorted by the index triple.
///
/// # Errors
///
/// * [`CdtError::LoopIndexOutOfRange`] — a loop index `>= verts.len()`.
/// * [`CdtError::DegenerateInput`] — outer loop has fewer than 3 vertices.
/// * [`CdtError::DuplicateVertex`] — two distinct loop indices map to
///   coincident positions (spade would merge them into one handle, collapsing a
///   constraint edge).
/// * [`CdtError::TriangulationFailed`] — spade rejected an insertion, or a
///   constraint edge would intersect another constraint edge (malformed /
///   self-intersecting boundary). We never silently introduce a Steiner split
///   point to resolve such a conflict.
pub fn cdt_polygon_with_holes(
    verts: &[CadPoint2],
    outer: &[u32],
    holes: &[Vec<u32>],
) -> Result<Vec<[u32; 3]>, CdtError> {
    // ---- 1. Validate loop indices are in range. -------------------------
    let n_verts = verts.len();
    let in_range = |idx: u32| (idx as usize) < n_verts;
    if !outer.iter().copied().all(in_range) {
        return Err(CdtError::LoopIndexOutOfRange);
    }
    for hole in holes {
        if !hole.iter().copied().all(in_range) {
            return Err(CdtError::LoopIndexOutOfRange);
        }
    }

    // An outer loop with fewer than 3 vertices encloses no area.
    if outer.len() < 3 {
        return Err(CdtError::DegenerateInput);
    }

    // ---- 2. Insert exactly the referenced vertices, in caller order. ----
    //
    // We only insert vertices that participate in a loop (outer or a hole) so
    // that the triangulation domain is well-defined. Inserting an unreferenced
    // vertex could create a Steiner-like attractor inside the domain; the
    // contract is "output indexes into `verts`", which we honor via `local`.
    let mut cdt: ConstrainedDelaunayTriangulation<SpadePoint2<f64>> =
        ConstrainedDelaunayTriangulation::new();

    // Map: caller `verts` index -> spade FixedVertexHandle (only for inserted).
    let mut handle_of: Vec<Option<FixedVertexHandle>> = vec![None; n_verts];

    // Insert helper: insert a vertex (once), recording its handle. Detects
    // duplicate positions (spade returns the SAME handle for coincident points)
    // when two DISTINCT caller indices collapse to one handle.
    let insert_vertex = |cdt: &mut ConstrainedDelaunayTriangulation<SpadePoint2<f64>>,
                         handle_of: &mut Vec<Option<FixedVertexHandle>>,
                         idx: u32|
     -> Result<FixedVertexHandle, CdtError> {
        if let Some(h) = handle_of[idx as usize] {
            return Ok(h);
        }
        let p = verts[idx as usize];
        let h = cdt
            .insert(SpadePoint2::new(p.x(), p.y()))
            .map_err(map_insertion_error)?;
        // If spade merged this into an already-used handle, two distinct caller
        // indices are coincident -> degenerate constraint graph.
        if handle_of.contains(&Some(h)) {
            return Err(CdtError::DuplicateVertex);
        }
        handle_of[idx as usize] = Some(h);
        Ok(h)
    };

    // Order of first-touch insertion: outer loop, then each hole, in order.
    for &idx in outer {
        insert_vertex(&mut cdt, &mut handle_of, idx)?;
    }
    for hole in holes {
        for &idx in hole {
            insert_vertex(&mut cdt, &mut handle_of, idx)?;
        }
    }

    // ---- 3. Add each loop as hard constraint edges. ---------------------
    //
    // `add_constraint` (handle form) panics if a new constraint intersects an
    // existing constraint edge, so we guard every edge with `can_add_constraint`
    // and surface a conflict as `TriangulationFailed` (malformed boundary)
    // rather than panicking or silently splitting (no Steiner points).
    let add_loop = |cdt: &mut ConstrainedDelaunayTriangulation<SpadePoint2<f64>>,
                    handle_of: &[Option<FixedVertexHandle>],
                    loop_idx: &[u32]|
     -> Result<(), CdtError> {
        let m = loop_idx.len();
        for i in 0..m {
            let a = handle_of[loop_idx[i] as usize].ok_or(CdtError::TriangulationFailed)?;
            let b =
                handle_of[loop_idx[(i + 1) % m] as usize].ok_or(CdtError::TriangulationFailed)?;
            if a == b {
                // Self-loop edge (e.g. a repeated index) — degenerate.
                return Err(CdtError::DegenerateInput);
            }
            // Already present (e.g. a shared edge) is fine; only a genuine
            // crossing of a different constraint is an error.
            if cdt.exists_constraint(a, b) {
                continue;
            }
            if !cdt.can_add_constraint(a, b) {
                return Err(CdtError::TriangulationFailed);
            }
            cdt.add_constraint(a, b);
        }
        Ok(())
    };

    add_loop(&mut cdt, &handle_of, outer)?;
    for hole in holes {
        if hole.len() >= 2 {
            add_loop(&mut cdt, &handle_of, hole)?;
        }
    }

    // ---- 4. Build local <-> spade-handle index translation. -------------
    //
    // `handle_of[caller_idx] = Some(spade_handle)`. We invert it so that, given
    // a spade vertex's `.index()`, we recover the caller index. spade vertex
    // indices are dense `0..num_vertices`, matching insertion order with no
    // Steiner points (we add only boundary constraints between existing pts).
    if cdt.num_vertices() != count_inserted(&handle_of) {
        // spade added a Steiner / split vertex — only happens on intersecting
        // constraints, which we already reject. Defensive guard.
        return Err(CdtError::TriangulationFailed);
    }
    let mut caller_of_spade: Vec<u32> = vec![u32::MAX; cdt.num_vertices()];
    for (caller_idx, slot) in handle_of.iter().enumerate() {
        if let Some(h) = slot {
            caller_of_spade[h.index()] = caller_idx as u32;
        }
    }

    // ---- 5. Classify interior faces by centroid + emit triangles. -------
    //
    // A face is interior iff its centroid is inside the outer loop AND outside
    // every hole. Centroid (average of 3 verts) of a triangle is strictly
    // interior to that triangle, so it never lands on a constraint edge —
    // point-in-polygon parity is well-defined.
    let outer_pts: Vec<CadPoint2> = outer.iter().map(|&i| verts[i as usize]).collect();
    let hole_pts: Vec<Vec<CadPoint2>> = holes
        .iter()
        .map(|h| h.iter().map(|&i| verts[i as usize]).collect())
        .collect();

    let mut tris: Vec<[u32; 3]> = Vec::new();
    for face in cdt.inner_faces() {
        let vs = face.vertices();
        let li = [
            caller_of_spade[vs[0].index()],
            caller_of_spade[vs[1].index()],
            caller_of_spade[vs[2].index()],
        ];
        if li.contains(&u32::MAX) {
            return Err(CdtError::TriangulationFailed);
        }

        let a = verts[li[0] as usize];
        let b = verts[li[1] as usize];
        let c = verts[li[2] as usize];
        let cx = (a.x() + b.x() + c.x()) / 3.0;
        let cy = (a.y() + b.y() + c.y()) / 3.0;
        let centroid = CadPoint2::new(cx, cy);

        let inside_outer = point_in_polygon(centroid, &outer_pts);
        let in_a_hole = hole_pts.iter().any(|h| point_in_polygon(centroid, h));
        if inside_outer && !in_a_hole {
            tris.push(li);
        }
    }

    // ---- 6. Canonicalize for byte-identical determinism. ----------------
    //
    // Rotate each triangle so its smallest index is first (winding preserved),
    // then sort the list. spade's `inner_faces` order is deterministic given
    // fixed insertion order, but we sort defensively so the contract holds
    // even if a future spade revision changes iteration order.
    for t in &mut tris {
        rotate_min_first(t);
    }
    tris.sort_unstable();

    // A valid (≥3-vertex, in-range, non-crossing) outer loop that yields ZERO
    // interior triangles encloses no area — a collinear / zero-area outer loop
    // (or one fully covered by holes). That is degenerate input, NOT a valid
    // empty result: spade accepts distinct-but-collinear points + non-crossing
    // constraints and `inner_faces()` is simply empty, so without this guard the
    // function would silently return `Ok(vec![])` (PR-NC1 adversary finding). A
    // thin sliver with positive area still yields ≥1 interior triangle and is
    // unaffected.
    if tris.is_empty() {
        return Err(CdtError::DegenerateInput);
    }

    Ok(tris)
}

/// Count how many caller vertices were actually inserted into spade.
fn count_inserted(handle_of: &[Option<FixedVertexHandle>]) -> usize {
    handle_of.iter().filter(|h| h.is_some()).count()
}

/// Rotate a triangle's indices so the smallest is first, preserving the cyclic
/// winding order (so the triangle's orientation / normal sign is unchanged).
fn rotate_min_first(t: &mut [u32; 3]) {
    let min_pos = if t[0] <= t[1] && t[0] <= t[2] {
        0
    } else if t[1] <= t[0] && t[1] <= t[2] {
        1
    } else {
        2
    };
    if min_pos != 0 {
        let rotated = [t[min_pos], t[(min_pos + 1) % 3], t[(min_pos + 2) % 3]];
        *t = rotated;
    }
}

/// Map a spade `InsertionError` to a [`CdtError`]. Every variant
/// (`NAN` / `TooSmall` / `TooLarge`) is an invalid coordinate, i.e.
/// degenerate input.
fn map_insertion_error(e: InsertionError) -> CdtError {
    match e {
        InsertionError::NAN | InsertionError::TooSmall | InsertionError::TooLarge => {
            CdtError::DegenerateInput
        }
    }
}

/// Even-odd point-in-polygon test (ray casting). `poly` is the ordered loop
/// vertices. Used only on triangle centroids, which never lie on a boundary
/// edge, so the boundary case is irrelevant.
fn point_in_polygon(p: CadPoint2, poly: &[CadPoint2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let (px, py) = (p.x(), p.y());
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[i].x(), poly[i].y());
        let (xj, yj) = (poly[j].x(), poly[j].y());
        let intersects = ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi);
        if intersects {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_in_polygon_unit_square() {
        let sq = [
            CadPoint2::new(0.0, 0.0),
            CadPoint2::new(1.0, 0.0),
            CadPoint2::new(1.0, 1.0),
            CadPoint2::new(0.0, 1.0),
        ];
        assert!(point_in_polygon(CadPoint2::new(0.5, 0.5), &sq));
        assert!(!point_in_polygon(CadPoint2::new(1.5, 0.5), &sq));
        assert!(!point_in_polygon(CadPoint2::new(-0.5, 0.5), &sq));
    }

    #[test]
    fn rotate_min_first_preserves_winding() {
        let mut t = [2u32, 0, 1];
        rotate_min_first(&mut t);
        assert_eq!(t, [0, 1, 2]);
        let mut t2 = [1u32, 2, 0];
        rotate_min_first(&mut t2);
        assert_eq!(t2, [0, 1, 2]);
    }
}
