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
    AngleLimit, ConstrainedDelaunayTriangulation, InsertionError, Point2 as SpadePoint2,
    RefinementParameters, Triangulation,
};
use std::collections::{HashSet, VecDeque};
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

/// Like [`cdt_polygon_with_holes`] but with the OUTER interior/exterior
/// classification done by a **flood-fill from the convex hull across
/// non-constraint edges** (the `_refined` §6a mechanism) instead of f64
/// centroid parity.
///
/// Same contract as [`cdt_polygon_with_holes`] in every other respect —
/// boundary-only (NO Steiner points; the `num_vertices != inserted` guard is
/// kept), output indexes the caller `verts` pool, canonicalized for
/// byte-identical determinism, empty result ⇒ [`CdtError::DegenerateInput`].
/// Hole exclusion stays per-hole centroid parity (a triangle whose centroid
/// falls inside any hole loop is dropped).
///
/// # Why a separate variant
///
/// The centroid-parity outer classification drops interior triangles on a
/// finely-sampled (near-collinear) outer boundary — the F0047 barrel-cut
/// "parity slitting" regression (spec `kv2_cdt_triangulation_core` §6b, M2):
/// a thin near-boundary triangle's f64 centroid can land the wrong side of a
/// near-collinear boundary run, slitting the mesh. The flood-fill classifies
/// topologically on the CDT dual graph (no coordinates): every inner face NOT
/// reachable from the convex hull across non-constraint edges lies inside the
/// outer constraint loop and is kept, slivers included, so the result stays
/// watertight. For a convex domain whose hull edges are all constraints,
/// nothing is seeded and every face is kept.
pub fn cdt_polygon_with_holes_floodfill(
    verts: &[CadPoint2],
    outer: &[u32],
    holes: &[Vec<u32>],
) -> Result<Vec<[u32; 3]>, CdtError> {
    // ---- 1-4. Same constrained-CDT setup as `cdt_polygon_with_holes`, with
    // SHARED-VERTEX WELDING (spec §6b M3b) in the vertex-insertion step. ----
    // (Duplicated rather than factored: the sibling boundary-only functions
    // already each carry this setup, and `cdt_polygon_with_holes` is a
    // yang-rs Stage-1 dependency whose behavior must not shift.)
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
    if outer.len() < 3 {
        return Err(CdtError::DegenerateInput);
    }

    let mut cdt: ConstrainedDelaunayTriangulation<SpadePoint2<f64>> =
        ConstrainedDelaunayTriangulation::new();
    let mut handle_of: Vec<Option<FixedVertexHandle>> = vec![None; n_verts];
    // SHARED-VERTEX WELDING (spec §6b M3b, flood-fill variant ONLY): unlike the
    // plain `cdt_polygon_with_holes`, coincident caller positions are allowed to
    // weld to the SAME spade handle instead of returning `DuplicateVertex`. A
    // tangent hole shares exactly one geometric point with the outer ring (the
    // keyhole pinch); spade supports constraints meeting at a shared vertex. A
    // constraint whose two endpoints weld to one handle (a consecutive
    // duplicate) still fails loudly via the `a == b` check in `add_loop` below.
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
        handle_of[idx as usize] = Some(h);
        Ok(h)
    };
    for &idx in outer {
        insert_vertex(&mut cdt, &mut handle_of, idx)?;
    }
    for hole in holes {
        for &idx in hole {
            insert_vertex(&mut cdt, &mut handle_of, idx)?;
        }
    }
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
                return Err(CdtError::DegenerateInput);
            }
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

    // No-Steiner guard: with welding, several caller indices may share one
    // spade handle, so the guard counts DISTINCT welded handles (not inserted
    // caller indices). A mismatch means spade added a Steiner/split vertex,
    // which only happens on a crossing constraint we already reject.
    let distinct_handles: HashSet<usize> = handle_of.iter().flatten().map(|h| h.index()).collect();
    if cdt.num_vertices() != distinct_handles.len() {
        return Err(CdtError::TriangulationFailed);
    }
    // Inverse map: a welded handle has several caller indices — keep the
    // FIRST-inserted one (first-set-wins in caller-index order) so output
    // triangles reference a single, deterministic pool index per position.
    let mut caller_of_spade: Vec<u32> = vec![u32::MAX; cdt.num_vertices()];
    for (caller_idx, slot) in handle_of.iter().enumerate() {
        if let Some(h) = slot {
            if caller_of_spade[h.index()] == u32::MAX {
                caller_of_spade[h.index()] = caller_idx as u32;
            }
        }
    }

    // ---- 5a. Mark the OUTER exterior region topologically (flood-fill). ---
    // Robust where the float centroid test is not (§6b M2): a near-collinear
    // outer boundary makes the centroid test drop thin near-boundary
    // triangles, slitting the mesh; the flood-fill keeps every interior face.
    let exterior = floodfill_outer_exterior(&cdt);

    // ---- 5b. Emit kept faces (not outer-exterior, centroid out of holes). -
    let hole_pts: Vec<Vec<CadPoint2>> = holes
        .iter()
        .map(|h| h.iter().map(|&i| verts[i as usize]).collect())
        .collect();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for face in cdt.inner_faces() {
        if exterior.contains(&face.fix()) {
            continue;
        }
        let vs = face.vertices();
        let li = [
            caller_of_spade[vs[0].index()],
            caller_of_spade[vs[1].index()],
            caller_of_spade[vs[2].index()],
        ];
        if li.contains(&u32::MAX) {
            return Err(CdtError::TriangulationFailed);
        }
        if !hole_pts.is_empty() {
            let a = verts[li[0] as usize];
            let b = verts[li[1] as usize];
            let c = verts[li[2] as usize];
            // EXACT hole parity (M8 holed-disc increment 3): the f64 centroid
            // test misclassifies ULP-twin femto slivers along a hole chord
            // (kept inside the hole → the constraint edge is used twice and
            // the cap self-overlaps). Rational parity is decision-exact.
            if hole_pts
                .iter()
                .any(|h| centroid_in_polygon_exact(a, b, c, h))
            {
                continue;
            }
        }
        tris.push(li);
    }

    // ---- 6. Canonicalize for byte-identical determinism. ----------------
    for t in &mut tris {
        rotate_min_first(t);
    }
    tris.sort_unstable();
    if tris.is_empty() {
        return Err(CdtError::DegenerateInput);
    }
    Ok(tris)
}

/// Like [`cdt_polygon_with_holes`] but additionally inserts a caller-provided set
/// of INTERIOR vertices that are KEPT (triangulated against the boundary, NOT
/// Steiner-refined and NOT dropped).
///
/// This is the primitive Yang §4.4.1 mesh-updating needs to re-triangulate a
/// surface patch **in its parametric domain while preserving the patch's interior
/// mesh vertices** — a curved patch (cylinder/sphere/cone) carries shape in its
/// interior, so unlike a flat patch its interior points cannot be discarded. The
/// patch boundary loops (`outer` + `holes`, which include the intersection-curve
/// chain) are the hard constraints; the interior points fill the patch so no
/// triangle spans three collinear boundary points (the relocation sliver).
///
/// * `verts` — the shared 2D pool: boundary vertices AND interior vertices.
/// * `outer`, `holes` — boundary loops, indices into `verts` (as
///   [`cdt_polygon_with_holes`]).
/// * `interior` — additional vertices to insert and keep, indices into `verts`.
///   Each MUST lie strictly inside the outer loop and outside every hole; an
///   interior index coincident with a boundary vertex, or lying ON a boundary
///   constraint edge, yields [`CdtError::DuplicateVertex`] / `TriangulationFailed`
///   (spade would split a constraint — we never silently introduce Steiner
///   points).
///
/// Output indexes into `verts` (no new points) and is canonicalized for
/// byte-identical determinism exactly like [`cdt_polygon_with_holes`], so the
/// boundary vertex set is preserved and the patch stays conformal with its
/// un-remeshed neighbours.
pub fn cdt_polygon_with_holes_keep_interior(
    verts: &[CadPoint2],
    outer: &[u32],
    holes: &[Vec<u32>],
    interior: &[u32],
) -> Result<Vec<[u32; 3]>, CdtError> {
    // ---- 1. Validate all indices are in range. --------------------------
    let n_verts = verts.len();
    let in_range = |idx: u32| (idx as usize) < n_verts;
    if !outer.iter().copied().all(in_range)
        || holes.iter().flatten().any(|&i| !in_range(i))
        || !interior.iter().copied().all(in_range)
    {
        return Err(CdtError::LoopIndexOutOfRange);
    }
    if outer.len() < 3 {
        return Err(CdtError::DegenerateInput);
    }

    // ---- 2. Insert boundary (outer, holes) then interior vertices. ------
    let mut cdt: ConstrainedDelaunayTriangulation<SpadePoint2<f64>> =
        ConstrainedDelaunayTriangulation::new();
    let mut handle_of: Vec<Option<FixedVertexHandle>> = vec![None; n_verts];
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
        if handle_of.contains(&Some(h)) {
            return Err(CdtError::DuplicateVertex);
        }
        handle_of[idx as usize] = Some(h);
        Ok(h)
    };
    for &idx in outer {
        insert_vertex(&mut cdt, &mut handle_of, idx)?;
    }
    for hole in holes {
        for &idx in hole {
            insert_vertex(&mut cdt, &mut handle_of, idx)?;
        }
    }
    for &idx in interior {
        insert_vertex(&mut cdt, &mut handle_of, idx)?;
    }

    // ---- 3. Add boundary loops as hard constraints. ---------------------
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
                return Err(CdtError::DegenerateInput);
            }
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

    // ---- 4. Local <-> spade-handle translation; no-Steiner guard. -------
    // Every inserted vertex (boundary + interior) is caller-provided; spade adds
    // a vertex ONLY when a constraint crossing forces a split, which we reject.
    if cdt.num_vertices() != count_inserted(&handle_of) {
        return Err(CdtError::TriangulationFailed);
    }
    let mut caller_of_spade: Vec<u32> = vec![u32::MAX; cdt.num_vertices()];
    for (caller_idx, slot) in handle_of.iter().enumerate() {
        if let Some(h) = slot {
            caller_of_spade[h.index()] = caller_idx as u32;
        }
    }

    // ---- 5a. Mark the OUTER exterior region topologically (flood-fill). ---
    // #146 inc-3b (task #180, spec `yang_146_keep_interior_floodfill.md`):
    // the former f64 centroid parity classification misclassifies the flap
    // between a near-collinear boundary chain and its chord (a junction
    // pierce point sits ~1e-9 off the chord), keeping a triangle OUTSIDE the
    // boundary — the boundary constraint edge is then used by two triangles
    // and the caller's mesh goes non-2-manifold. Classify the outer region
    // topologically instead, exactly like `cdt_polygon_with_holes_floodfill`
    // (the F0047/#179 migration, next instance): flood the dual graph from
    // the infinite face inward, crossing only NON-constraint edges. Interior
    // vertices only add inner faces; the boundary loops remain the
    // constraint walls, so the flood never leaks into the domain.
    let exterior = floodfill_outer_exterior(&cdt);

    // ---- 5b. Emit kept faces (not outer-exterior, centroid out of holes). -
    // Holes are enclosed by constraint walls the flood cannot cross, so they
    // are classified by EXACT parity (the rational tier decides inside the
    // f64 uncertainty band) — same as the flood-fill variant.
    let hole_pts: Vec<Vec<CadPoint2>> = holes
        .iter()
        .map(|h| h.iter().map(|&i| verts[i as usize]).collect())
        .collect();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for face in cdt.inner_faces() {
        if exterior.contains(&face.fix()) {
            continue;
        }
        let vs = face.vertices();
        let li = [
            caller_of_spade[vs[0].index()],
            caller_of_spade[vs[1].index()],
            caller_of_spade[vs[2].index()],
        ];
        if li.contains(&u32::MAX) {
            return Err(CdtError::TriangulationFailed);
        }
        if !hole_pts.is_empty() {
            let a = verts[li[0] as usize];
            let b = verts[li[1] as usize];
            let c = verts[li[2] as usize];
            if hole_pts
                .iter()
                .any(|h| centroid_in_polygon_exact(a, b, c, h))
            {
                continue;
            }
        }
        tris.push(li);
    }

    // ---- 6. Canonicalize for byte-identical determinism. ----------------
    for t in &mut tris {
        rotate_min_first(t);
    }
    tris.sort_unstable();
    if tris.is_empty() {
        return Err(CdtError::DegenerateInput);
    }
    Ok(tris)
}

/// Like [`cdt_polygon_with_holes`] but with INTERIOR STEINER REFINEMENT: spade's
/// Delaunay refinement inserts interior points until every emitted triangle has
/// area ≤ `max_area` — the size budget a curved-surface caller derives from its
/// chord-error tolerance (a torus / cylinder UV patch maps `max_area` from the
/// surface curvature so the 3D sagitta of each triangle stays bounded). This is
/// the primitive `cdt_polygon_with_holes` could not provide ("no interior
/// Steiner points"), the gateway to curved-patch tessellation (KV6d torus
/// booleans, non-convex CDT profiles).
///
/// Boundary CONSTRAINT edges are kept (`keep_constraint_edges`): the boundary
/// vertex set is preserved bit-for-bit, so a patch stays conformal with its
/// neighbours; only the interior gains points. `exclude_outer_faces` confines
/// refinement to inside the boundary (the exterior strip up to spade's convex
/// hull is left coarse and dropped at emit).
///
/// Returns a FRESH vertex pool — the boundary vertices (in spade insertion
/// order: `outer` then each hole) followed by the Steiner points — and triangles
/// indexing it. Unlike [`cdt_polygon_with_holes`] the output is NOT over the
/// caller's `verts` (it has new points). `max_area` non-finite or ≤ 0 ⇒ no
/// refinement (the boundary-only CDT, re-indexed into the fresh pool).
///
/// f64-robust like [`cdt_polygon_with_holes`] (spade predicates): a TESSELLATION
/// primitive, not the exact boolean arrangement — the same precision posture the
/// existing planar CDT consumer already relies on.
pub fn cdt_polygon_with_holes_refined(
    verts: &[CadPoint2],
    outer: &[u32],
    holes: &[Vec<u32>],
    max_area: f64,
) -> Result<(Vec<CadPoint2>, Vec<[u32; 3]>), CdtError> {
    cdt_refined_impl(verts, outer, holes, max_area, None)
}

/// [`cdt_polygon_with_holes_refined`] plus DETERMINISTIC interior grid
/// seeding at `seed_spacing = [hx, hy]` (the chord-band fix, 2026-08-08 —
/// `docs/audits/volume_oracle_flags_anchored.md` §deficit-class).
///
/// An AREA budget alone does not bound EDGE length: area-only Steiner
/// refinement leaves large/skinny interior triangles whose chords sag ~8×
/// the canonical render band (measured on the R0057/R0059 boolean-output
/// torus patches). Seeding the interior with a grid at the caller's
/// band-matched spacing bounds interior edge lengths by construction — the
/// same spacing, and therefore the same sagitta band, as the structured
/// tessellator's rings. Grid lines sit at absolute multiples of the spacing
/// (phase-aligned with the structured rings, which sample at fixed angular
/// steps from the parameterization origin); points within `min(hx, hy)/2`
/// of a constraint segment are skipped so no constraint is split and the
/// boundary strip stays sliver-free. A pure function of
/// `(verts, loops, spacing)` — no adaptivity.
pub fn cdt_polygon_with_holes_refined_seeded(
    verts: &[CadPoint2],
    outer: &[u32],
    holes: &[Vec<u32>],
    max_area: f64,
    seed_spacing: [f64; 2],
) -> Result<(Vec<CadPoint2>, Vec<[u32; 3]>), CdtError> {
    cdt_refined_impl(verts, outer, holes, max_area, Some(seed_spacing))
}

fn cdt_refined_impl(
    verts: &[CadPoint2],
    outer: &[u32],
    holes: &[Vec<u32>],
    max_area: f64,
    seed_spacing: Option<[f64; 2]>,
) -> Result<(Vec<CadPoint2>, Vec<[u32; 3]>), CdtError> {
    // ---- 1-3. Same constrained-CDT setup as `cdt_polygon_with_holes`. ----
    let n_verts = verts.len();
    let in_range = |idx: u32| (idx as usize) < n_verts;
    if !outer.iter().copied().all(in_range) || holes.iter().flatten().any(|&i| !in_range(i)) {
        return Err(CdtError::LoopIndexOutOfRange);
    }
    if outer.len() < 3 {
        return Err(CdtError::DegenerateInput);
    }

    let mut cdt: ConstrainedDelaunayTriangulation<SpadePoint2<f64>> =
        ConstrainedDelaunayTriangulation::new();
    let mut handle_of: Vec<Option<FixedVertexHandle>> = vec![None; n_verts];
    let insert = |cdt: &mut ConstrainedDelaunayTriangulation<SpadePoint2<f64>>,
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
        if handle_of.contains(&Some(h)) {
            return Err(CdtError::DuplicateVertex);
        }
        handle_of[idx as usize] = Some(h);
        Ok(h)
    };
    for &idx in outer {
        insert(&mut cdt, &mut handle_of, idx)?;
    }
    for hole in holes {
        for &idx in hole {
            insert(&mut cdt, &mut handle_of, idx)?;
        }
    }
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
                return Err(CdtError::DegenerateInput);
            }
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

    // ---- 3b. Deterministic interior grid seeding (chord-band fix). -------
    // See `cdt_polygon_with_holes_refined_seeded`: interior points at absolute
    // multiples of the spacing, inside the outer loop, outside every hole,
    // and at least `min(hx, hy)/2` clear of every constraint segment (so no
    // constraint is split and the boundary strip stays sliver-free).
    if let Some([hx, hy]) = seed_spacing {
        if hx.is_finite() && hx > 0.0 && hy.is_finite() && hy > 0.0 {
            let loop_pts = |idx: &[u32]| -> Vec<CadPoint2> {
                idx.iter().map(|&i| verts[i as usize]).collect()
            };
            let outer_pts = loop_pts(outer);
            let hole_pts: Vec<Vec<CadPoint2>> = holes.iter().map(|h| loop_pts(h)).collect();
            let mut segments: Vec<(CadPoint2, CadPoint2)> = Vec::new();
            for l in std::iter::once(&outer_pts).chain(hole_pts.iter()) {
                let m = l.len();
                for i in 0..m {
                    segments.push((l[i], l[(i + 1) % m]));
                }
            }
            let dist2_seg = |p: CadPoint2, (a, b): &(CadPoint2, CadPoint2)| -> f64 {
                let (ax, ay) = (a.x(), a.y());
                let (dx, dy) = (b.x() - ax, b.y() - ay);
                let len2 = dx * dx + dy * dy;
                let t = if len2 > 0.0 {
                    (((p.x() - ax) * dx + (p.y() - ay) * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (qx, qy) = (ax + t * dx, ay + t * dy);
                (p.x() - qx) * (p.x() - qx) + (p.y() - qy) * (p.y() - qy)
            };
            let clear2 = {
                let c = 0.5 * hx.min(hy);
                c * c
            };
            let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for p in &outer_pts {
                lo[0] = lo[0].min(p.x());
                lo[1] = lo[1].min(p.y());
                hi[0] = hi[0].max(p.x());
                hi[1] = hi[1].max(p.y());
            }
            let (ix0, ix1) = ((lo[0] / hx).ceil() as i64, (hi[0] / hx).floor() as i64);
            let (iy0, iy1) = ((lo[1] / hy).ceil() as i64, (hi[1] / hy).floor() as i64);
            for iy in iy0..=iy1 {
                for ix in ix0..=ix1 {
                    let p = CadPoint2::new(ix as f64 * hx, iy as f64 * hy);
                    if !point_in_polygon(p, &outer_pts) {
                        continue;
                    }
                    if hole_pts.iter().any(|h| point_in_polygon(p, h)) {
                        continue;
                    }
                    if segments.iter().any(|s| dist2_seg(p, s) < clear2) {
                        continue;
                    }
                    // Exact duplicates return the existing handle; a NaN-free
                    // grid point cannot otherwise fail.
                    let _ = cdt.insert(SpadePoint2::new(p.x(), p.y()));
                }
            }
        }
    }

    // ---- 4. Interior Steiner refinement. --------------------------------
    // `keep_constraint_edges` preserves the boundary vertex set (conformality
    // with the neighbouring patch — the caller's boundary samples are NOT moved
    // or split). We do NOT use `exclude_outer_faces`: spade computes its
    // inside/outside partition by peeling layers from the convex hull, and on a
    // coarse non-convex mesh (e.g. the 4-triangle L-shape) it over-excludes the
    // interior, suppressing all refinement. Instead we refine the whole convex
    // hull and drop the exterior strip at emit (§6) via the centroid test.
    //
    // The angle limit is set to 0° (disabled): we want ONLY the area bound. The
    // spade default is a 30° minimum-angle (Ruppert) limit, which on a boundary
    // sampled with float noise — so its "collinear" runs have ~1e-16 kinks —
    // chases those artificial small angles and over-refines wildly (it cannot
    // fix them under keep_constraint_edges, so it loops inserting interior points
    // and spawns slivers near the boundary that then fail the centroid test).
    // With only the area bound, refinement is well-behaved and the centroid
    // emit is reliable.
    if max_area.is_finite() && max_area > 0.0 {
        cdt.refine(
            RefinementParameters::<f64>::new()
                .keep_constraint_edges()
                .with_angle_limit(AngleLimit::from_deg(0.0))
                .with_max_allowed_area(max_area),
        );
    }

    // ---- 5. Fresh vertex pool (boundary + Steiner) in spade index order. -
    let mut out_verts: Vec<CadPoint2> = vec![CadPoint2::new(0.0, 0.0); cdt.num_vertices()];
    for v in cdt.vertices() {
        let p = v.position();
        out_verts[v.index()] = CadPoint2::new(p.x, p.y);
    }

    // ---- 6a. Mark the OUTER exterior region topologically. --------------
    // Robust where the float centroid test is not: a finely-sampled
    // (near-collinear) outer boundary makes refinement spawn ~0-area slivers,
    // and a centroid test drops them, slitting the mesh; the flood-fill keeps
    // every interior face (slivers included), so the result stays watertight.
    // For a convex domain whose hull edges are all constraints, nothing is
    // seeded and every face is kept.
    let exterior = floodfill_outer_exterior(&cdt);

    // ---- 6b. Emit kept faces (not outer-exterior, centroid out of holes). -
    let hole_pts: Vec<Vec<CadPoint2>> = holes
        .iter()
        .map(|h| h.iter().map(|&i| verts[i as usize]).collect())
        .collect();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for face in cdt.inner_faces() {
        if exterior.contains(&face.fix()) {
            continue;
        }
        let vs = face.vertices();
        let idx = [
            vs[0].index() as u32,
            vs[1].index() as u32,
            vs[2].index() as u32,
        ];
        if !hole_pts.is_empty() {
            let a = out_verts[idx[0] as usize];
            let b = out_verts[idx[1] as usize];
            let c = out_verts[idx[2] as usize];
            let centroid =
                CadPoint2::new((a.x() + b.x() + c.x()) / 3.0, (a.y() + b.y() + c.y()) / 3.0);
            if hole_pts.iter().any(|h| point_in_polygon(centroid, h)) {
                continue;
            }
        }
        tris.push(idx);
    }
    for t in &mut tris {
        rotate_min_first(t);
    }
    tris.sort_unstable();
    if tris.is_empty() {
        return Err(CdtError::DegenerateInput);
    }
    Ok((out_verts, tris))
}

/// Constrained Delaunay triangulate a planar patch with INTERIOR constraint
/// edges in addition to the boundary loops.
///
/// This is the CDT primitive behind Yang 2025 §4.4.1 "mesh updating" (Fig 11):
/// an intersection polyline is inserted as a chain of constrained edges, and the
/// trimmed patch is re-triangulated so each polyline segment is an edge of the
/// result on BOTH sides (the paper's `split`). Unlike
/// [`cdt_polygon_with_holes`], a `constraints` edge lies in the patch INTERIOR —
/// it need not be part of any boundary loop.
///
/// * `verts` — the shared 2D pool (boundary vertices, kept interior vertices,
///   and polyline points).
/// * `outer`, `holes` — boundary loops, indices into `verts` (as
///   [`cdt_polygon_with_holes`]).
/// * `interior` — extra interior vertices to insert and keep (e.g. the Fig 11
///   loop-`insert` point), indices into `verts`; may be empty. Each MUST lie
///   strictly inside the outer loop and outside every hole.
/// * `constraints` — interior constraint edges `[a, b]` (index pairs into
///   `verts`), e.g. consecutive intersection-polyline points. Both endpoints are
///   inserted and kept. A constraint edge that would cross a boundary or another
///   constraint (forcing a Steiner split) is rejected with
///   [`CdtError::TriangulationFailed`] — we never silently Steiner-split
///   (P9/P10).
///
/// Returns ALL interior triangles (inside `outer`, outside every hole) as index
/// triples into `verts`. The polyline appears as shared edges of the triangles
/// straddling it. Output is canonicalized for byte-identical determinism exactly
/// like the sibling functions. No interior Steiner points are added.
///
/// # Errors
///
/// Same set as [`cdt_polygon_with_holes_keep_interior`], plus a constraint whose
/// endpoints are coincident (`DegenerateInput`) or whose insertion conflicts
/// with the boundary / another constraint (`TriangulationFailed`).
pub fn cdt_with_interior_constraints(
    verts: &[CadPoint2],
    outer: &[u32],
    holes: &[Vec<u32>],
    interior: &[u32],
    constraints: &[[u32; 2]],
) -> Result<Vec<[u32; 3]>, CdtError> {
    // ---- 1. Validate all indices are in range. --------------------------
    let n_verts = verts.len();
    let in_range = |idx: u32| (idx as usize) < n_verts;
    if !outer.iter().copied().all(in_range)
        || holes.iter().flatten().any(|&i| !in_range(i))
        || !interior.iter().copied().all(in_range)
        || constraints.iter().flatten().any(|&i| !in_range(i))
    {
        return Err(CdtError::LoopIndexOutOfRange);
    }
    if outer.len() < 3 {
        return Err(CdtError::DegenerateInput);
    }

    // ---- 2. Insert boundary, interior, then constraint-endpoint verts. --
    let mut cdt: ConstrainedDelaunayTriangulation<SpadePoint2<f64>> =
        ConstrainedDelaunayTriangulation::new();
    let mut handle_of: Vec<Option<FixedVertexHandle>> = vec![None; n_verts];
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
        if handle_of.contains(&Some(h)) {
            return Err(CdtError::DuplicateVertex);
        }
        handle_of[idx as usize] = Some(h);
        Ok(h)
    };
    for &idx in outer {
        insert_vertex(&mut cdt, &mut handle_of, idx)?;
    }
    for hole in holes {
        for &idx in hole {
            insert_vertex(&mut cdt, &mut handle_of, idx)?;
        }
    }
    for &idx in interior {
        insert_vertex(&mut cdt, &mut handle_of, idx)?;
    }
    for e in constraints {
        insert_vertex(&mut cdt, &mut handle_of, e[0])?;
        insert_vertex(&mut cdt, &mut handle_of, e[1])?;
    }

    // ---- 3. Add boundary loops AND interior constraints as hard edges. --
    let add_edge = |cdt: &mut ConstrainedDelaunayTriangulation<SpadePoint2<f64>>,
                    handle_of: &[Option<FixedVertexHandle>],
                    ia: u32,
                    ib: u32|
     -> Result<(), CdtError> {
        let a = handle_of[ia as usize].ok_or(CdtError::TriangulationFailed)?;
        let b = handle_of[ib as usize].ok_or(CdtError::TriangulationFailed)?;
        if a == b {
            return Err(CdtError::DegenerateInput);
        }
        if cdt.exists_constraint(a, b) {
            return Ok(());
        }
        if !cdt.can_add_constraint(a, b) {
            return Err(CdtError::TriangulationFailed);
        }
        cdt.add_constraint(a, b);
        Ok(())
    };
    let add_loop = |cdt: &mut ConstrainedDelaunayTriangulation<SpadePoint2<f64>>,
                    handle_of: &[Option<FixedVertexHandle>],
                    loop_idx: &[u32]|
     -> Result<(), CdtError> {
        let m = loop_idx.len();
        for i in 0..m {
            add_edge(cdt, handle_of, loop_idx[i], loop_idx[(i + 1) % m])?;
        }
        Ok(())
    };
    add_loop(&mut cdt, &handle_of, outer)?;
    for hole in holes {
        if hole.len() >= 2 {
            add_loop(&mut cdt, &handle_of, hole)?;
        }
    }
    for e in constraints {
        add_edge(&mut cdt, &handle_of, e[0], e[1])?;
    }

    // ---- 4. No-Steiner guard (a constraint crossing would add a vertex). -
    if cdt.num_vertices() != count_inserted(&handle_of) {
        return Err(CdtError::TriangulationFailed);
    }
    let mut caller_of_spade: Vec<u32> = vec![u32::MAX; cdt.num_vertices()];
    for (caller_idx, slot) in handle_of.iter().enumerate() {
        if let Some(h) = slot {
            caller_of_spade[h.index()] = caller_idx as u32;
        }
    }

    // ---- 5. Classify interior faces + emit. -----------------------------
    // #146 inc-3b (task #180, spec `yang_146_keep_interior_floodfill.md`):
    // outer region topologically (flood-fill — the f64 centroid parity test
    // keeps exterior flaps over near-collinear boundary chains), holes by
    // EXACT parity. Interior constraint edges lie inside the domain, so the
    // exterior flood never reaches them; they cannot wall off exterior faces.
    let exterior = floodfill_outer_exterior(&cdt);
    let hole_pts: Vec<Vec<CadPoint2>> = holes
        .iter()
        .map(|h| h.iter().map(|&i| verts[i as usize]).collect())
        .collect();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for face in cdt.inner_faces() {
        if exterior.contains(&face.fix()) {
            continue;
        }
        let vs = face.vertices();
        let li = [
            caller_of_spade[vs[0].index()],
            caller_of_spade[vs[1].index()],
            caller_of_spade[vs[2].index()],
        ];
        if li.contains(&u32::MAX) {
            return Err(CdtError::TriangulationFailed);
        }
        if !hole_pts.is_empty() {
            let a = verts[li[0] as usize];
            let b = verts[li[1] as usize];
            let c = verts[li[2] as usize];
            if hole_pts
                .iter()
                .any(|h| centroid_in_polygon_exact(a, b, c, h))
            {
                continue;
            }
        }
        tris.push(li);
    }

    // ---- 6. Canonicalize for byte-identical determinism. ----------------
    for t in &mut tris {
        rotate_min_first(t);
    }
    tris.sort_unstable();
    if tris.is_empty() {
        return Err(CdtError::DegenerateInput);
    }
    Ok(tris)
}

/// Mark the OUTER exterior region of a constrained CDT topologically: flood
/// the dual graph from the infinite face inward, crossing only NON-constraint
/// edges. Any inner face reached lies outside the outer constraint loop (the
/// convex-hull notch of a non-convex domain, or the strip between hull and
/// boundary). Decision-exact where the f64 centroid test is not (§6b M2 /
/// F0047 / #179 / #180): a near-collinear outer boundary makes the centroid
/// test both drop thin interior triangles (slitting) and keep exterior flaps
/// (over-coverage). Shared by the flood-fill, refined, keep-interior, and
/// interior-constraints variants.
fn floodfill_outer_exterior(
    cdt: &ConstrainedDelaunayTriangulation<SpadePoint2<f64>>,
) -> HashSet<spade::handles::FixedFaceHandle<spade::handles::InnerTag>> {
    let mut exterior = HashSet::new();
    let mut queue: VecDeque<_> = VecDeque::new();
    for hull_edge in cdt.convex_hull() {
        if hull_edge.is_constraint_edge() {
            continue;
        }
        if let Some(f) = hull_edge.rev().face().as_inner() {
            if exterior.insert(f.fix()) {
                queue.push_back(f.fix());
            }
        }
    }
    while let Some(f) = queue.pop_front() {
        for edge in cdt.face(f).adjacent_edges() {
            if edge.is_constraint_edge() {
                continue;
            }
            if let Some(nb) = edge.rev().face().as_inner() {
                if exterior.insert(nb.fix()) {
                    queue.push_back(nb.fix());
                }
            }
        }
    }
    exterior
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

/// EXACT even-odd point-in-polygon parity for a TRIANGLE CENTROID, in pure
/// rational arithmetic (dashu `RBig`; every f64 is an exact rational, so the
/// decision is exact — no femto-sliver misclassification).
///
/// M8 holed-disc increment 3 (spec `m8_holed_disc_coplanar_overlay` §8): the
/// f64 [`point_in_polygon`] misclassifies the centroid of a ULP-twin femto
/// sliver lying along a boundary chord — the same "parity slitting" class as
/// F0047 on the outer loop, but on a HOLE loop, where the flood-fill cannot
/// help (hole interiors are decided geometrically, not topologically). The
/// query point is passed as the triangle's three corners; the centroid
/// `(Σx/3, Σy/3)` is formed exactly. Crossing test per edge `(p, q)`:
/// `(p.y > cy) != (q.y > cy)` (half-open rule, exact) and the edge's x at
/// `cy` exceeds `cx`, evaluated division-free as
/// `sign((p.x−cx)·dy + (cy−p.y)·(q.x−p.x)) == sign(dy)` with `dy = q.y−p.y`.
fn centroid_in_polygon_exact(a: CadPoint2, b: CadPoint2, c: CadPoint2, poly: &[CadPoint2]) -> bool {
    // Filtered fast path (the PR-CR4 filtered+exact cascade house pattern):
    // evaluate every edge decision in f64 with a conservative forward error
    // bound; only when SOME decision falls inside its uncertainty band does
    // the call pay for the rational tier. The exact tier is the sole decision
    // authority in the uncertain band — the filter never changes an answer,
    // it only skips provably-safe work (hole loops are hot in the kernel-v2
    // render channel; unconditional per-triangle RBig parity is not viable).
    if let Some(inside) = centroid_in_polygon_filtered(a, b, c, poly) {
        return inside;
    }
    centroid_in_polygon_rational(a, b, c, poly)
}

/// f64 tier of [`centroid_in_polygon_exact`]. Returns `None` when any edge
/// decision is within its rounding-error band (→ caller uses the exact tier).
fn centroid_in_polygon_filtered(
    a: CadPoint2,
    b: CadPoint2,
    c: CadPoint2,
    poly: &[CadPoint2],
) -> Option<bool> {
    let n = poly.len();
    if n < 3 {
        return Some(false);
    }
    let eps = f64::EPSILON;
    // Centroid with a conservative absolute error bound per coordinate
    // (two adds + one divide → ≤ 3 roundings on magnitudes ≤ the term sum).
    let sx = a.x().abs() + b.x().abs() + c.x().abs();
    let sy = a.y().abs() + b.y().abs() + c.y().abs();
    let cx = (a.x() + b.x() + c.x()) / 3.0;
    let cy = (a.y() + b.y() + c.y()) / 3.0;
    let ecx = 2.0 * eps * sx;
    let ecy = 2.0 * eps * sy;

    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (px, py) = (poly[j].x(), poly[j].y());
        let (qx, qy) = (poly[i].x(), poly[i].y());
        j = i;
        // Straddle test: strict comparisons against cy — uncertain when an
        // endpoint is within the centroid's y error band.
        if (py - cy).abs() <= ecy || (qy - cy).abs() <= ecy {
            return None;
        }
        if (py > cy) == (qy > cy) {
            continue;
        }
        // Crossing side: sign of D = (px−cx)·dy + (cy−py)·(qx−px) vs sign of
        // dy. Conservative first-order bound on D's rounding error, with the
        // centroid error bounds propagated through their factors.
        let dy = qy - py;
        let d = (px - cx) * dy + (cy - py) * (qx - px);
        let e_d = ecx * dy.abs()
            + ecy * (qx - px).abs()
            + 8.0
                * eps
                * ((px.abs() + cx.abs()) * dy.abs()
                    + (cy.abs() + py.abs()) * (qx.abs() + px.abs()));
        if d.abs() <= e_d {
            return None;
        }
        let crosses = if dy > 0.0 { d > 0.0 } else { d < 0.0 };
        if crosses {
            inside = !inside;
        }
    }
    Some(inside)
}

/// Exact (rational) tier of [`centroid_in_polygon_exact`] — the decision
/// authority when the f64 filter is uncertain.
fn centroid_in_polygon_rational(
    a: CadPoint2,
    b: CadPoint2,
    c: CadPoint2,
    poly: &[CadPoint2],
) -> bool {
    use dashu::rational::RBig;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let rat = |x: f64| -> RBig {
        // Finite f64 → exact rational; tessellation coordinates are finite.
        RBig::try_from(
            dashu::float::FBig::<dashu::float::round::mode::Zero>::try_from(x)
                .expect("finite f64 -> FBig is total on tessellation coordinates"),
        )
        .expect("FBig -> RBig is total")
    };
    let three = RBig::from(3);
    let cx = (rat(a.x()) + rat(b.x()) + rat(c.x())) / &three;
    let cy = (rat(a.y()) + rat(b.y()) + rat(c.y())) / &three;
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (pxf, pyf) = (poly[j].x(), poly[j].y());
        let (qxf, qyf) = (poly[i].x(), poly[i].y());
        j = i;
        let py = rat(pyf);
        let qy = rat(qyf);
        if (py > cy) == (qy > cy) {
            continue;
        }
        let dy = &qy - &py;
        let num = (rat(pxf) - &cx) * &dy + (&cy - &py) * (rat(qxf) - rat(pxf));
        let crosses = if dy > RBig::ZERO {
            num > RBig::ZERO
        } else {
            num < RBig::ZERO
        };
        if crosses {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- seeded refined CDT (chord-band fix, 2026-08-08) -----------------

    /// A finely-sampled square boundary (production contract: callers sample
    /// loops at chord density). Points every 1.0 along the perimeter.
    fn sampled_square(side: f64, step: f64) -> (Vec<CadPoint2>, Vec<u32>) {
        let n = (side / step).round() as i64;
        let mut pts = Vec::new();
        for i in 0..n {
            pts.push(CadPoint2::new(i as f64 * step, 0.0));
        }
        for i in 0..n {
            pts.push(CadPoint2::new(side, i as f64 * step));
        }
        for i in 0..n {
            pts.push(CadPoint2::new(side - i as f64 * step, side));
        }
        for i in 0..n {
            pts.push(CadPoint2::new(0.0, side - i as f64 * step));
        }
        let idx = (0..pts.len() as u32).collect();
        (pts, idx)
    }

    fn max_edge_len(vs: &[CadPoint2], tris: &[[u32; 3]]) -> f64 {
        let mut m: f64 = 0.0;
        for t in tris {
            for e in 0..3 {
                let a = vs[t[e] as usize];
                let b = vs[t[(e + 1) % 3] as usize];
                m = m.max(((a.x() - b.x()).powi(2) + (a.y() - b.y()).powi(2)).sqrt());
            }
        }
        m
    }

    #[test]
    fn seeded_cdt_inserts_band_grid_and_bounds_edges() {
        let (verts, outer) = sampled_square(10.0, 1.0);
        // Infinite area budget ⇒ the spade area refinement is OFF: every
        // interior vertex comes from the seeding alone.
        let (vs, tris) =
            cdt_polygon_with_holes_refined_seeded(&verts, &outer, &[], f64::INFINITY, [1.0, 1.0])
                .expect("seeded CDT");
        // Interior grid: integer points 1..=9 × 1..=9 (boundary-line points are
        // within the ½-cell clearance of a constraint and are skipped).
        assert_eq!(vs.len(), verts.len() + 81, "9×9 interior grid seeded");
        // Every edge (constraints included — the boundary is sampled at the
        // same step) is bounded by the cell diagonal.
        let max_e = max_edge_len(&vs, &tris);
        assert!(
            max_e <= 2.0_f64.sqrt() + 1e-9,
            "edge length must be bounded by the seed-cell diagonal, got {max_e}"
        );
    }

    #[test]
    fn seeded_cdt_respects_holes_and_clearance() {
        let (mut verts, outer) = sampled_square(10.0, 1.0);
        // Hole square [4,6]², sampled at the same step, wound opposite.
        let hole_pts = [
            (4.0, 4.0),
            (4.0, 5.0),
            (4.0, 6.0),
            (5.0, 6.0),
            (6.0, 6.0),
            (6.0, 5.0),
            (6.0, 4.0),
            (5.0, 4.0),
        ];
        let hole_base = verts.len() as u32;
        verts.extend(hole_pts.iter().map(|&(x, y)| CadPoint2::new(x, y)));
        let hole: Vec<u32> = (0..hole_pts.len() as u32).map(|i| hole_base + i).collect();
        let (vs, tris) = cdt_polygon_with_holes_refined_seeded(
            &verts,
            &outer,
            std::slice::from_ref(&hole),
            f64::INFINITY,
            [1.0, 1.0],
        )
        .expect("seeded CDT with hole");
        // 81 grid candidates minus the 3×3 block covered by the hole and its
        // clearance band (integer points with 4 ≤ x,y ≤ 6).
        assert_eq!(vs.len(), verts.len() + 81 - 9, "hole block excluded");
        // No emitted triangle centroid inside the hole.
        for t in &tris {
            let (a, b, c) = (vs[t[0] as usize], vs[t[1] as usize], vs[t[2] as usize]);
            let cx = (a.x() + b.x() + c.x()) / 3.0;
            let cy = (a.y() + b.y() + c.y()) / 3.0;
            assert!(
                !(cx > 4.0 && cx < 6.0 && cy > 4.0 && cy < 6.0),
                "triangle centroid ({cx},{cy}) inside the hole"
            );
        }
    }

    #[test]
    fn seeded_cdt_is_deterministic() {
        let (verts, outer) = sampled_square(10.0, 1.0);
        let a = cdt_polygon_with_holes_refined_seeded(&verts, &outer, &[], 2.0, [1.0, 1.0])
            .expect("run 1");
        let b = cdt_polygon_with_holes_refined_seeded(&verts, &outer, &[], 2.0, [1.0, 1.0])
            .expect("run 2");
        assert_eq!(a.1, b.1, "triangles identical across runs");
        assert_eq!(a.0.len(), b.0.len());
        for (p, q) in a.0.iter().zip(b.0.iter()) {
            assert_eq!((p.x(), p.y()), (q.x(), q.y()));
        }
    }

    #[test]
    fn seeded_cdt_degenerate_spacing_equals_unseeded() {
        let (verts, outer) = sampled_square(10.0, 1.0);
        let seeded = cdt_polygon_with_holes_refined_seeded(&verts, &outer, &[], 5.0, [0.0, 1.0])
            .expect("degenerate spacing");
        let plain = cdt_polygon_with_holes_refined(&verts, &outer, &[], 5.0).expect("unseeded");
        assert_eq!(seeded.1, plain.1, "zero spacing must behave as unseeded");
    }

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

    // ---- keep-interior CDT (Yang §4.4.1 patch re-triangulation) ---------

    #[test]
    fn keep_interior_preserves_interior_point_and_tiles() {
        // Unit square (0..3) + one interior point (4). The point must be KEPT
        // (referenced by some output triangle) and the triangles tile the square.
        let verts = [
            CadPoint2::new(0.0, 0.0),
            CadPoint2::new(1.0, 0.0),
            CadPoint2::new(1.0, 1.0),
            CadPoint2::new(0.0, 1.0),
            CadPoint2::new(0.5, 0.5),
        ];
        let tris =
            cdt_polygon_with_holes_keep_interior(&verts, &[0, 1, 2, 3], &[], &[4]).expect("ok");
        assert!(
            tris.iter().any(|t| t.contains(&4)),
            "interior vertex 4 must be kept, not dropped: {tris:?}"
        );
        assert!(
            (total_area(&verts, &tris) - 1.0).abs() < 1e-9,
            "triangles must tile the unit square exactly"
        );
    }

    #[test]
    fn keep_interior_collinear_boundary_yields_no_degenerate_triangle() {
        // THE band-remesh property: a boundary with a COLLINEAR run (the bottom
        // edge has three collinear points 0,1,2) plus an interior point must NOT
        // produce a zero-area triangle spanning the collinear trio — exactly the
        // relocation sliver this primitive exists to avoid.
        let verts = [
            CadPoint2::new(0.0, 0.0), // 0  ┐
            CadPoint2::new(0.5, 0.0), // 1  ├ collinear bottom edge
            CadPoint2::new(1.0, 0.0), // 2  ┘
            CadPoint2::new(1.0, 1.0), // 3
            CadPoint2::new(0.0, 1.0), // 4
            CadPoint2::new(0.5, 0.5), // 5  interior
        ];
        let tris =
            cdt_polygon_with_holes_keep_interior(&verts, &[0, 1, 2, 3, 4], &[], &[5]).expect("ok");
        for t in &tris {
            let area = tri_area(
                verts[t[0] as usize],
                verts[t[1] as usize],
                verts[t[2] as usize],
            );
            assert!(
                area > 1e-9,
                "no degenerate (collinear) triangle: {t:?} area={area}"
            );
        }
        // The collinear middle point (1) and the interior point (5) are both kept.
        assert!(tris.iter().any(|t| t.contains(&1)));
        assert!(tris.iter().any(|t| t.contains(&5)));
        assert!((total_area(&verts, &tris) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn keep_interior_near_collinear_boundary_chain_is_conformal() {
        // #146 inc-3b (task #180, spec `yang_146_keep_interior_floodfill.md`):
        // bit-exact local CDT input of F0084 gate-ON operand-B face 8 (via
        // YANG_CDT_PROBE=8). The boundary chain 1→2→3 carries a junction
        // pierce point (2) within ~1e-9 of the chord 1–3; the f64 centroid
        // parity classifier keeps the exterior flap triangle [1,3,2] between
        // chord and chain, so the boundary constraint edges (1,2)/(2,3) are
        // each used by TWO triangles — the rebuilt operand mesh goes
        // non-2-manifold (fwd=1/rev=2 + an open chord edge).
        let p = |xb: u64, yb: u64| CadPoint2::new(f64::from_bits(xb), f64::from_bits(yb));
        let verts = [
            p(0x3ff568f26569b709, 0xbfc07a6c7f8299ef), // 0
            p(0x3ff506a3a58c722c, 0x3f9aea02bb63da84), // 1
            p(0x3ff110b06008354b, 0xbf88aa0b92fafea0), // 2 pierce point on chord 1–3
            p(0x3ff103284f1f115b, 0xbf89b6351183fca0), // 3
            p(0x3ff11f561e474f8c, 0xbfad2db6c6cf0030), // 4
            p(0x3ff165770efc5638, 0xbfc573102807550c), // 5
            p(0x3ff124312e8f4f2f, 0xbfad1b11a376fee6), // 6 interior junction point
        ];
        let outer: [u32; 6] = [0, 1, 2, 3, 4, 5];
        let tris = cdt_polygon_with_holes_keep_interior(&verts, &outer, &[], &[6]).expect("ok");
        // Every boundary constraint edge is used by EXACTLY one triangle
        // (single outer loop, no holes: the polygon interior is on one side).
        let mut use_count = std::collections::HashMap::new();
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *use_count.entry((a.min(b), a.max(b))).or_insert(0u32) += 1;
            }
        }
        for i in 0..outer.len() {
            let (a, b) = (outer[i], outer[(i + 1) % outer.len()]);
            let c = use_count.get(&(a.min(b), a.max(b))).copied().unwrap_or(0);
            assert_eq!(
                c, 1,
                "boundary edge ({a},{b}) must be used by exactly 1 triangle, got {c}: {tris:?}"
            );
        }
        // The interior junction point is still consumed.
        assert!(tris.iter().any(|t| t.contains(&6)));
    }

    #[test]
    fn keep_interior_rejects_coincident_interior_vertex() {
        // An interior index whose position coincides with a boundary vertex would
        // collapse a constraint — rejected, never silently split.
        let verts = [
            CadPoint2::new(0.0, 0.0),
            CadPoint2::new(1.0, 0.0),
            CadPoint2::new(1.0, 1.0),
            CadPoint2::new(0.0, 1.0),
            CadPoint2::new(0.0, 0.0), // 4 == vertex 0
        ];
        let r = cdt_polygon_with_holes_keep_interior(&verts, &[0, 1, 2, 3], &[], &[4]);
        assert_eq!(r, Err(CdtError::DuplicateVertex));
    }

    // ---- interior-Steiner refinement -----------------------------------

    fn tri_area(a: CadPoint2, b: CadPoint2, c: CadPoint2) -> f64 {
        0.5 * ((b.x() - a.x()) * (c.y() - a.y()) - (b.y() - a.y()) * (c.x() - a.x())).abs()
    }
    fn total_area(verts: &[CadPoint2], tris: &[[u32; 3]]) -> f64 {
        tris.iter()
            .map(|t| {
                tri_area(
                    verts[t[0] as usize],
                    verts[t[1] as usize],
                    verts[t[2] as usize],
                )
            })
            .sum()
    }

    #[test]
    fn refine_square_adds_interior_steiner_and_tiles_exactly() {
        let sq = [
            CadPoint2::new(0.0, 0.0),
            CadPoint2::new(1.0, 0.0),
            CadPoint2::new(1.0, 1.0),
            CadPoint2::new(0.0, 1.0),
        ];
        let (verts, tris) =
            cdt_polygon_with_holes_refined(&sq, &[0, 1, 2, 3], &[], 0.02).expect("refine ok");
        // Steiner points were added (a coarse boundary still refines the interior).
        assert!(
            verts.len() > 4,
            "expected interior Steiner points, got {} verts",
            verts.len()
        );
        assert!(
            tris.len() > 2,
            "expected many triangles, got {}",
            tris.len()
        );
        // The 4 corners are preserved bit-for-bit at the insertion-order indices
        // (keep_constraint_edges + insertion order).
        for (i, &c) in sq.iter().enumerate() {
            assert_eq!(verts[i], c, "boundary corner {i} must be preserved");
        }
        // Coverage: emitted triangles exactly tile the unit square (area 1.0).
        assert!(
            (total_area(&verts, &tris) - 1.0).abs() < 1e-9,
            "area = {}",
            total_area(&verts, &tris)
        );
    }

    #[test]
    fn refine_bounds_all_triangles_given_a_fine_boundary() {
        // The realistic use case: the patch boundary arrives pre-sampled (the
        // mesh boolean's intersection curve), so keep_constraint_edges achieves
        // the size bound everywhere. Build a unit square with each side
        // subdivided into 10 segments (40 boundary verts).
        let n = 10usize;
        let mut verts = Vec::new();
        let mut outer = Vec::new();
        let side = |s: usize,
                    from: (f64, f64),
                    to: (f64, f64),
                    verts: &mut Vec<CadPoint2>,
                    outer: &mut Vec<u32>| {
            for k in 0..s {
                let t = k as f64 / s as f64;
                outer.push(verts.len() as u32);
                verts.push(CadPoint2::new(
                    from.0 + (to.0 - from.0) * t,
                    from.1 + (to.1 - from.1) * t,
                ));
            }
        };
        side(n, (0.0, 0.0), (1.0, 0.0), &mut verts, &mut outer);
        side(n, (1.0, 0.0), (1.0, 1.0), &mut verts, &mut outer);
        side(n, (1.0, 1.0), (0.0, 1.0), &mut verts, &mut outer);
        side(n, (0.0, 1.0), (0.0, 0.0), &mut verts, &mut outer);

        let max_area = 0.02;
        let (rv, tris) =
            cdt_polygon_with_holes_refined(&verts, &outer, &[], max_area).expect("refine ok");
        assert!(
            (total_area(&rv, &tris) - 1.0).abs() < 1e-9,
            "coverage area = {}",
            total_area(&rv, &tris)
        );
        for t in &tris {
            let a = tri_area(rv[t[0] as usize], rv[t[1] as usize], rv[t[2] as usize]);
            assert!(
                a <= max_area + 1e-9,
                "triangle area {a} exceeds max {max_area}"
            );
        }
    }

    #[test]
    fn refine_zero_max_area_is_boundary_only() {
        let sq = [
            CadPoint2::new(0.0, 0.0),
            CadPoint2::new(2.0, 0.0),
            CadPoint2::new(2.0, 2.0),
            CadPoint2::new(0.0, 2.0),
        ];
        let (verts, tris) =
            cdt_polygon_with_holes_refined(&sq, &[0, 1, 2, 3], &[], 0.0).expect("ok");
        assert_eq!(verts.len(), 4, "no Steiner points when max_area <= 0");
        assert_eq!(
            tris.len(),
            2,
            "a square is two triangles with no refinement"
        );
        assert!((total_area(&verts, &tris) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn refine_nonconvex_l_shape_interior_only() {
        // An L-shape (non-convex): area = 3 (a 2×2 square minus a 1×1 corner).
        let l = [
            CadPoint2::new(0.0, 0.0),
            CadPoint2::new(2.0, 0.0),
            CadPoint2::new(2.0, 1.0),
            CadPoint2::new(1.0, 1.0),
            CadPoint2::new(1.0, 2.0),
            CadPoint2::new(0.0, 2.0),
        ];
        let (verts, tris) =
            cdt_polygon_with_holes_refined(&l, &[0, 1, 2, 3, 4, 5], &[], 0.05).expect("ok");
        assert!(verts.len() > 6, "expected interior Steiner points");
        // Interior only: the re-entrant corner's exterior is NOT triangulated, so
        // the total area is the L's area (3.0), not the convex hull's (4.0). This
        // is the load-bearing flood-fill exterior exclusion (§6a): the notch
        // faces, reachable from the convex hull across non-constraint edges, are
        // dropped while the L interior is kept.
        assert!(
            (total_area(&verts, &tris) - 3.0).abs() < 1e-9,
            "area = {}",
            total_area(&verts, &tris)
        );
    }

    #[test]
    fn refine_offset_tall_rectangle_tiles_exactly() {
        // Reproduces the torus UV-patch case: an offset, high-aspect rectangle
        // [0.2,1.2] x [1.5,5.4] with 8 collinear samples per side (32 verts).
        let (x0, x1, y0, y1) = (0.2_f64, 1.2, 1.5, 5.4);
        let ns = 8usize;
        let mut verts = Vec::new();
        let mut outer = Vec::new();
        let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        for s in 0..4 {
            let (fx, fy) = corners[s];
            let (tx, ty) = corners[(s + 1) % 4];
            for k in 0..ns {
                let t = k as f64 / ns as f64;
                outer.push(verts.len() as u32);
                verts.push(CadPoint2::new(fx + (tx - fx) * t, fy + (ty - fy) * t));
            }
        }
        let (rv, tris) =
            cdt_polygon_with_holes_refined(&verts, &outer, &[], 0.05).expect("refine ok");
        let area = total_area(&rv, &tris);
        let mut edges: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let bnd = edges.values().filter(|&&c| c == 1).count();
        assert_eq!(bnd, outer.len(), "boundary edges {bnd} != {}", outer.len());
        assert!(
            (area - 3.9).abs() < 1e-9,
            "coverage area = {area}, expected 3.9 (= 1.0 x 3.9)"
        );
    }

    #[test]
    fn refine_conditioned_near_collinear_boundary_is_watertight() {
        // The EXACT 32 scaled (u*r, v*R) points the torus UV-CDT consumer feeds
        // for a straight-seam patch edge, carrying ~1e-16 atan2 round-trip noise
        // on coordinates that should be exactly collinear. The consumer snaps
        // them to the 1e-12 working grid first (done here); the primitive must
        // then refine without over-refining and emit a watertight (un-slit) mesh
        // — the flood-fill exterior test keeps every interior face, including the
        // thin near-boundary triangles a centroid test would have dropped.
        let raw: [(f64, f64); 32] = [
            (2.00000000000000038858e-1, 1.5e0),
            (3.25000000000000011102e-1, 1.5e0),
            (4.50000000000000066613e-1, 1.49999999999999977796e0),
            (5.75000000000000066613e-1, 1.5e0),
            (6.99999999999999844569e-1, 1.5e0),
            (8.24999999999999955591e-1, 1.5e0),
            (9.49999999999999733546e-1, 1.49999999999999977796e0),
            (1.07499999999999995559e0, 1.5e0),
            (1.19999999999999973355e0, 1.49999999999999977796e0),
            (1.20000000000000017764e0, 1.98749999999999982236e0),
            (1.20000000000000017764e0, 2.47499999999999964473e0),
            (1.20000000000000017764e0, 2.96250000000000035527e0),
            (1.20000000000000017764e0, 3.44999999999999973355e0),
            (1.20000000000000017764e0, 3.9375e0),
            (1.20000000000000062172e0, 4.42500000000000071054e0),
            (1.20000000000000017764e0, 4.91249999999999964473e0),
            (1.19999999999999973355e0, 5.40000000000000035527e0),
            (1.07499999999999995559e0, 5.40000000000000035527e0),
            (9.50000000000000066613e-1, 5.40000000000000035527e0),
            (8.24999999999999955591e-1, 5.40000000000000035527e0),
            (6.99999999999999844569e-1, 5.40000000000000035527e0),
            (5.75000000000000288658e-1, 5.40000000000000035527e0),
            (4.50000000000000233147e-1, 5.40000000000000035527e0),
            (3.24999999999999955591e-1, 5.40000000000000035527e0),
            (2.00000000000000038858e-1, 5.40000000000000035527e0),
            (2.00000000000000038858e-1, 4.91249999999999964473e0),
            (2.00000000000000038858e-1, 4.42500000000000071054e0),
            (2.00000000000000038858e-1, 3.9375e0),
            (2.00000000000000038858e-1, 3.44999999999999973355e0),
            (1.99999999999999955591e-1, 2.96250000000000035527e0),
            (2.00000000000000038858e-1, 2.47499999999999964473e0),
            (2.00000000000000038858e-1, 1.98750000000000026645e0),
        ];
        let round = |v: f64| (v * 1e12).round() / 1e12;
        let verts: Vec<CadPoint2> = raw
            .iter()
            .map(|&(x, y)| CadPoint2::new(round(x), round(y)))
            .collect();
        let outer: Vec<u32> = (0..32).collect();
        let (rv, tris) =
            cdt_polygon_with_holes_refined(&verts, &outer, &[], 0.05).expect("refine ok");
        // Not over-refined (the raw, un-snapped points explode to >350 verts).
        assert!(rv.len() < 120, "over-refined: {} verts", rv.len());
        // Exact coverage and a watertight boundary (every boundary edge once,
        // every interior edge twice — no slits from dropped slivers).
        assert!(
            (total_area(&rv, &tris) - 3.9).abs() < 1e-6,
            "area {}",
            total_area(&rv, &tris)
        );
        let mut edges: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(
            edges.values().all(|&c| c == 1 || c == 2),
            "non-manifold edge"
        );
        assert_eq!(
            edges.values().filter(|&&c| c == 1).count(),
            32,
            "boundary must be the original 32-gon (no slits, no boundary splits)"
        );
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

    // ---- cdt_with_interior_constraints (Yang §4.4.1 mesh-updating CDT) ----

    /// True iff the undirected edge (a,b) is an edge of some triangle in `tris`.
    fn edge_present(tris: &[[u32; 3]], a: u32, b: u32) -> bool {
        tris.iter().any(|t| {
            let e = [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])];
            e.iter().any(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a))
        })
    }

    #[test]
    fn interior_constraint_chord_becomes_an_edge() {
        // Unit square, outer loop 0,1,2,3; a horizontal chord across the middle
        // via two new boundary-midpoint verts 4 (left, x=0) and 5 (right, x=1).
        // The chord 4-5 must appear as a shared edge on both sides.
        let verts = vec![
            CadPoint2::new(0.0, 0.0), // 0
            CadPoint2::new(1.0, 0.0), // 1
            CadPoint2::new(1.0, 1.0), // 2
            CadPoint2::new(0.0, 1.0), // 3
            CadPoint2::new(0.0, 0.5), // 4  left edge midpoint
            CadPoint2::new(1.0, 0.5), // 5  right edge midpoint
        ];
        // Outer loop weaves the two midpoints into the left/right edges.
        let outer = vec![0u32, 1, 5, 2, 3, 4];
        let tris = cdt_with_interior_constraints(&verts, &outer, &[], &[], &[[4, 5]]).unwrap();
        assert!(
            edge_present(&tris, 4, 5),
            "interior chord 4-5 must be an edge of the triangulation"
        );
        // Area conservation: the unit square, total = 1.0.
        let area: f64 = tris
            .iter()
            .map(|t| {
                signed_area(
                    verts[t[0] as usize],
                    verts[t[1] as usize],
                    verts[t[2] as usize],
                )
            })
            .sum();
        assert!(
            (area.abs() - 1.0).abs() < 1e-12,
            "area must equal 1, got {area}"
        );
        // No flips: all triangles share one winding sign.
        assert!(
            tris.iter().all(|t| signed_area(
                verts[t[0] as usize],
                verts[t[1] as usize],
                verts[t[2] as usize]
            ) > 0.0)
                || tris.iter().all(|t| signed_area(
                    verts[t[0] as usize],
                    verts[t[1] as usize],
                    verts[t[2] as usize]
                ) < 0.0),
            "no flipped triangles"
        );
    }

    #[test]
    fn interior_constraint_deterministic() {
        let verts = vec![
            CadPoint2::new(0.0, 0.0),
            CadPoint2::new(1.0, 0.0),
            CadPoint2::new(1.0, 1.0),
            CadPoint2::new(0.0, 1.0),
            CadPoint2::new(0.0, 0.5),
            CadPoint2::new(1.0, 0.5),
        ];
        let outer = vec![0u32, 1, 5, 2, 3, 4];
        let a = cdt_with_interior_constraints(&verts, &outer, &[], &[], &[[4, 5]]).unwrap();
        let b = cdt_with_interior_constraints(&verts, &outer, &[], &[], &[[4, 5]]).unwrap();
        assert_eq!(a, b, "byte-identical output on identical input");
    }

    #[test]
    fn interior_constraint_out_of_range_rejected() {
        let verts = vec![
            CadPoint2::new(0.0, 0.0),
            CadPoint2::new(1.0, 0.0),
            CadPoint2::new(1.0, 1.0),
        ];
        let outer = vec![0u32, 1, 2];
        assert_eq!(
            cdt_with_interior_constraints(&verts, &outer, &[], &[], &[[0, 9]]),
            Err(CdtError::LoopIndexOutOfRange)
        );
    }

    #[test]
    fn interior_constraint_closed_loop_kept_both_sides() {
        // Square with a triangular interior loop inserted as constraints (a
        // closed intersection loop). Both the annulus and the loop interior are
        // kept (constraints, not holes): the loop edges must all be present.
        let verts = vec![
            CadPoint2::new(0.0, 0.0), // 0
            CadPoint2::new(3.0, 0.0), // 1
            CadPoint2::new(3.0, 3.0), // 2
            CadPoint2::new(0.0, 3.0), // 3
            CadPoint2::new(1.0, 1.0), // 4 loop
            CadPoint2::new(2.0, 1.0), // 5 loop
            CadPoint2::new(1.5, 2.0), // 6 loop
        ];
        let outer = vec![0u32, 1, 2, 3];
        let tris = cdt_with_interior_constraints(
            &verts,
            &outer,
            &[],
            &[4, 5, 6],
            &[[4, 5], [5, 6], [6, 4]],
        )
        .unwrap();
        assert!(edge_present(&tris, 4, 5));
        assert!(edge_present(&tris, 5, 6));
        assert!(edge_present(&tris, 6, 4));
        let area: f64 = tris
            .iter()
            .map(|t| {
                signed_area(
                    verts[t[0] as usize],
                    verts[t[1] as usize],
                    verts[t[2] as usize],
                )
            })
            .sum();
        assert!(
            (area.abs() - 9.0).abs() < 1e-12,
            "3x3 square area 9, got {area}"
        );
    }

    fn signed_area(a: CadPoint2, b: CadPoint2, c: CadPoint2) -> f64 {
        0.5 * ((b.x() - a.x()) * (c.y() - a.y()) - (c.x() - a.x()) * (b.y() - a.y()))
    }
}

#[cfg(test)]
mod floodfill_red_tests {
    //! Round-2 M2 contract (spec `kv2_cdt_triangulation_core.md` §6b): the
    //! boundary-only CDT's interior/exterior classification must switch from
    //! f64 centroid parity to a flood-fill from the convex hull across
    //! non-constraint edges (the `_refined` §6a mechanism), KEEPING the
    //! no-Steiner guard, in a new variant
    //! `cdt_polygon_with_holes_floodfill(verts, outer, holes)`. This fixes the
    //! F0047 barrel-cut "parity slitting" regression (95 boundary-unpaired) the
    //! centroid path introduced.
    //!
    //! * The RED test below pins that new variant's contract. It references a
    //!   function that DOES NOT EXIST yet, so it is left COMMENTED OUT to keep
    //!   the tree compiling for everyone else; uncomment it with the M2
    //!   implementation.
    //! * The compiling GUARD pins the clean-case coverage the flood-fill
    //!   variant must PRESERVE (a mutation tripwire: flood-fill must not regress
    //!   the well-conditioned case).
    //!
    //! NOTE (Test Author, 2026-07-03): the task also called for a compiling
    //! *defect-pin* exercising a centroid-parity UNDER-coverage at the
    //! primitive level. After several tries — the snapped 32-point
    //! near-collinear ring plus four perturbed near-collinear boundary patterns
    //! (bulge-up, dip-down, alternating, slanted, ~1e-13 off-line) — the plain
    //! `cdt_polygon_with_holes` covered every fixture EXACTLY (area to 1e-12,
    //! boundary edge counts correct, no edge >2). The centroid path is robust
    //! at these synthetic scales; the F0047 slit needs the specific barrel-cut
    //! geometry. Per the spec, the E2E full-assay F0047 diff is the binding
    //! oracle for M2, so the primitive-level defect-pin (5b) is omitted, not
    //! faked.
    use super::*;
    use cad_primitives::Point2 as CadPoint2;

    /// The snapped 32-point near-collinear ring (shared with
    /// `tests::refine_conditioned_near_collinear_boundary_is_watertight`).
    fn snapped_near_collinear_32() -> Vec<CadPoint2> {
        let raw: [(f64, f64); 32] = [
            (2.00000000000000038858e-1, 1.5e0),
            (3.25000000000000011102e-1, 1.5e0),
            (4.50000000000000066613e-1, 1.49999999999999977796e0),
            (5.75000000000000066613e-1, 1.5e0),
            (6.99999999999999844569e-1, 1.5e0),
            (8.24999999999999955591e-1, 1.5e0),
            (9.49999999999999733546e-1, 1.49999999999999977796e0),
            (1.07499999999999995559e0, 1.5e0),
            (1.19999999999999973355e0, 1.49999999999999977796e0),
            (1.20000000000000017764e0, 1.98749999999999982236e0),
            (1.20000000000000017764e0, 2.47499999999999964473e0),
            (1.20000000000000017764e0, 2.96250000000000035527e0),
            (1.20000000000000017764e0, 3.44999999999999973355e0),
            (1.20000000000000017764e0, 3.9375e0),
            (1.20000000000000062172e0, 4.42500000000000071054e0),
            (1.20000000000000017764e0, 4.91249999999999964473e0),
            (1.19999999999999973355e0, 5.40000000000000035527e0),
            (1.07499999999999995559e0, 5.40000000000000035527e0),
            (9.50000000000000066613e-1, 5.40000000000000035527e0),
            (8.24999999999999955591e-1, 5.40000000000000035527e0),
            (6.99999999999999844569e-1, 5.40000000000000035527e0),
            (5.75000000000000288658e-1, 5.40000000000000035527e0),
            (4.50000000000000233147e-1, 5.40000000000000035527e0),
            (3.24999999999999955591e-1, 5.40000000000000035527e0),
            (2.00000000000000038858e-1, 5.40000000000000035527e0),
            (2.00000000000000038858e-1, 4.91249999999999964473e0),
            (2.00000000000000038858e-1, 4.42500000000000071054e0),
            (2.00000000000000038858e-1, 3.9375e0),
            (2.00000000000000038858e-1, 3.44999999999999973355e0),
            (1.99999999999999955591e-1, 2.96250000000000035527e0),
            (2.00000000000000038858e-1, 2.47499999999999964473e0),
            (2.00000000000000038858e-1, 1.98750000000000026645e0),
        ];
        let round = |v: f64| (v * 1e12).round() / 1e12;
        raw.iter()
            .map(|&(x, y)| CadPoint2::new(round(x), round(y)))
            .collect()
    }

    fn tri_area(a: CadPoint2, b: CadPoint2, c: CadPoint2) -> f64 {
        0.5 * ((b.x() - a.x()) * (c.y() - a.y()) - (b.y() - a.y()) * (c.x() - a.x())).abs()
    }

    // RED (M2): the flood-fill variant must classify interior faces by
    // flood-fill (not centroid parity) and so keep every interior face on the
    // parity-fragile F0047-class ring: full area coverage (3.9) and a
    // watertight 32-edge boundary (every undirected edge count 1 or 2), with
    // no Steiner points.
    #[test]
    fn floodfill_variant_keeps_parity_fragile_interior() {
        let verts = snapped_near_collinear_32();
        let outer: Vec<u32> = (0..32).collect();
        let tris = cdt_polygon_with_holes_floodfill(&verts, &outer, &[])
            .expect("M2: flood-fill variant must triangulate the near-collinear ring");
        let area: f64 = tris
            .iter()
            .map(|t| {
                tri_area(
                    verts[t[0] as usize],
                    verts[t[1] as usize],
                    verts[t[2] as usize],
                )
            })
            .sum();
        assert!(
            (area - 3.9).abs() < 1e-9,
            "M2: full coverage 3.9, got {area}"
        );
        let mut edges: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(edges.values().all(|&c| c == 1 || c == 2), "M2: watertight");
        assert_eq!(
            edges.values().filter(|&&c| c == 1).count(),
            32,
            "M2: no slits — boundary is the original 32-gon"
        );
    }

    /// GUARD (M2 mutation tripwire): the well-conditioned (snapped) near-
    /// collinear ring already covers EXACTLY under the existing centroid path.
    /// The M2 flood-fill variant must PRESERVE this — full area 3.9, watertight
    /// 32-edge boundary, no Steiner points. Passes today; must keep passing
    /// (re-target at `cdt_polygon_with_holes_floodfill` once it exists).
    #[test]
    fn floodfill_variant_must_preserve_clean_coverage() {
        let verts = snapped_near_collinear_32();
        let outer: Vec<u32> = (0..32).collect();
        let tris = cdt_polygon_with_holes(&verts, &outer, &[]).expect("clean ring triangulates");
        // No Steiner points: an exact partition of the simple 32-gon = 30 tris.
        assert_eq!(tris.len(), 30, "exact partition of the 32-gon");
        let area: f64 = tris
            .iter()
            .map(|t| {
                tri_area(
                    verts[t[0] as usize],
                    verts[t[1] as usize],
                    verts[t[2] as usize],
                )
            })
            .sum();
        assert!((area - 3.9).abs() < 1e-9, "full coverage 3.9, got {area}");
        let mut edges: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(
            edges.values().all(|&c| c == 1 || c == 2),
            "watertight — no edge shared by >2 triangles"
        );
        assert_eq!(
            edges.values().filter(|&&c| c == 1).count(),
            32,
            "boundary is the original 32-gon (no slits)"
        );
    }

    // ── ROUND 3 (M3b shared-vertex welding, spec §6b M3 amendment) ─────────

    /// A tangent-hole fixture: a CCW square whose bottom edge is pinched at
    /// (0,-2), plus a CW diamond HOLE that shares exactly that one point. The
    /// pinch position appears TWICE in the pool — once referenced by `outer`
    /// (index 4), once by the `hole` loop (index 5) — a tangent contact.
    fn tangent_hole_fixture() -> (Vec<CadPoint2>, Vec<u32>, Vec<u32>) {
        let verts = vec![
            CadPoint2::new(2.0, -2.0),  // 0
            CadPoint2::new(2.0, 2.0),   // 1
            CadPoint2::new(-2.0, 2.0),  // 2
            CadPoint2::new(-2.0, -2.0), // 3
            CadPoint2::new(0.0, -2.0),  // 4  pinch (outer)
            CadPoint2::new(0.0, -2.0),  // 5  pinch (hole) — SAME position as 4
            CadPoint2::new(-0.5, -1.2), // 6  ┐ diamond hole, CW
            CadPoint2::new(0.0, -0.6),  // 7  │
            CadPoint2::new(0.5, -1.2),  // 8  ┘
        ];
        let outer = vec![0u32, 1, 2, 3, 4];
        let hole = vec![5u32, 6, 7, 8];
        (verts, outer, hole)
    }

    /// RED (M3b): the flood-fill variant must WELD the shared tangent-contact
    /// vertex (coincident caller positions → one spade handle) instead of
    /// rejecting the ring. TODAY it returns `DuplicateVertex` (welding is the
    /// M3b GREEN work), so `expect` panics — RED. TARGET: Ok, exact coverage
    /// square − diamond = 15.3, and the diamond hole excluded (no triangle
    /// centroid inside it).
    #[test]
    fn red_floodfill_welds_tangent_hole() {
        let (verts, outer, hole) = tangent_hole_fixture();
        let tris = cdt_polygon_with_holes_floodfill(&verts, &outer, std::slice::from_ref(&hole))
            .expect(
                "M3b: the flood-fill variant must weld the tangent-contact vertex \
             (RED today: coincident outer/hole pinch → DuplicateVertex)",
            );
        // Exact coverage: square 16 − diamond 0.7 = 15.3.
        const KEYHOLE_AREA: f64 = 16.0 - 0.7;
        let area: f64 = tris
            .iter()
            .map(|t| {
                tri_area(
                    verts[t[0] as usize],
                    verts[t[1] as usize],
                    verts[t[2] as usize],
                )
            })
            .sum();
        assert!(
            (area - KEYHOLE_AREA).abs() < 1e-9,
            "M3b: coverage {area} != square − diamond {KEYHOLE_AREA}"
        );
        // Hole exclusion: no triangle centroid inside the diamond lobe.
        let diamond: Vec<CadPoint2> = hole.iter().map(|&i| verts[i as usize]).collect();
        for t in &tris {
            let (a, b, c) = (
                verts[t[0] as usize],
                verts[t[1] as usize],
                verts[t[2] as usize],
            );
            let centroid =
                CadPoint2::new((a.x() + b.x() + c.x()) / 3.0, (a.y() + b.y() + c.y()) / 3.0);
            assert!(
                !point_in_polygon(centroid, &diamond),
                "M3b: triangle {t:?} centroid lies inside the excluded diamond hole"
            );
        }
    }

    /// GUARD: welding is granted to the flood-fill variant ONLY. The plain
    /// `cdt_polygon_with_holes` keeps its strict contract — the same
    /// tangent-hole fixture must still return `DuplicateVertex` (yang-rs
    /// Stage-1 unchanged). Passes today and must keep passing after M3b.
    #[test]
    fn plain_cdt_keeps_duplicate_vertex_contract() {
        let (verts, outer, hole) = tangent_hole_fixture();
        assert_eq!(
            cdt_polygon_with_holes(&verts, &outer, &[hole]),
            Err(CdtError::DuplicateVertex),
            "the plain variant must keep rejecting coincident pool vertices"
        );
    }
}

#[cfg(test)]
mod floodfill_adversary_tests {
    //! ADVERSARY block (FIP Phase 4) for the M3b shared-vertex WELDING in
    //! `cdt_polygon_with_holes_floodfill` (spec `kv2_cdt_triangulation_core.md`
    //! §6b M3 amendment). Attacks the welding path with pathological tangent
    //! configurations beyond the round-3 single-tangent-hole fixture. Every case
    //! either produces a correct partition (verified area + watertight pairing)
    //! or fails loudly — no silent-wrong output was found.
    use super::*;
    use cad_primitives::Point2 as CadPoint2;

    fn tri_area(a: CadPoint2, b: CadPoint2, c: CadPoint2) -> f64 {
        0.5 * ((b.x() - a.x()) * (c.y() - a.y()) - (b.y() - a.y()) * (c.x() - a.x())).abs()
    }
    fn total_area(verts: &[CadPoint2], tris: &[[u32; 3]]) -> f64 {
        tris.iter()
            .map(|t| {
                tri_area(
                    verts[t[0] as usize],
                    verts[t[1] as usize],
                    verts[t[2] as usize],
                )
            })
            .sum()
    }
    fn max_incidence(tris: &[[u32; 3]]) -> u32 {
        let mut m: std::collections::BTreeMap<(u32, u32), u32> = std::collections::BTreeMap::new();
        for t in tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *m.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        m.values().copied().max().unwrap_or(0)
    }

    /// A hole tangent to the OUTER boundary at TWO points, disconnecting the
    /// interior into a left and a right region. The bar (rhombus) hole shares
    /// two welded handles with the outer ring. The partition must still cover
    /// exactly square − bar = 16 − 1.2 = 14.8 with watertight local pairing.
    #[test]
    fn adversary_hole_tangent_outer_at_two_points() {
        let verts = vec![
            CadPoint2::new(-2.0, -2.0), // 0
            CadPoint2::new(0.0, -2.0),  // 1 T1 (outer)
            CadPoint2::new(2.0, -2.0),  // 2
            CadPoint2::new(2.0, 2.0),   // 3
            CadPoint2::new(0.0, 2.0),   // 4 T2 (outer)
            CadPoint2::new(-2.0, 2.0),  // 5
            CadPoint2::new(0.0, -2.0),  // 6 T1 (hole) — welds with 1
            CadPoint2::new(-0.3, 0.0),  // 7 ┐ CW rhombus bar
            CadPoint2::new(0.0, 2.0),   // 8 T2 (hole) — welds with 4
            CadPoint2::new(0.3, 0.0),   // 9 ┘
        ];
        let outer = vec![0u32, 1, 2, 3, 4, 5];
        let hole = vec![6u32, 7, 8, 9];
        let tris = cdt_polygon_with_holes_floodfill(&verts, &outer, std::slice::from_ref(&hole))
            .expect("two-point-tangent hole must weld and triangulate");
        assert!(
            (total_area(&verts, &tris) - 14.8).abs() < 1e-9,
            "square − bar = 14.8"
        );
        assert!(max_incidence(&tris) <= 2, "watertight local pairing");
    }

    /// TWO holes tangent to EACH OTHER (sharing one welded interior vertex), not
    /// to the outer. Both holes must be excluded: area = 36 − 1 − 1 = 34.
    #[test]
    fn adversary_two_holes_tangent_each_other() {
        let verts = vec![
            CadPoint2::new(-3.0, -3.0), // 0
            CadPoint2::new(3.0, -3.0),  // 1
            CadPoint2::new(3.0, 3.0),   // 2
            CadPoint2::new(-3.0, 3.0),  // 3
            CadPoint2::new(0.0, 0.0),   // 4 shared (hole 1)
            CadPoint2::new(-1.0, 0.5),  // 5 ┐ CW diamond, left
            CadPoint2::new(-2.0, 0.0),  // 6 │
            CadPoint2::new(-1.0, -0.5), // 7 ┘
            CadPoint2::new(0.0, 0.0),   // 8 shared (hole 2) — welds with 4
            CadPoint2::new(1.0, -0.5),  // 9 ┐ CW diamond, right
            CadPoint2::new(2.0, 0.0),   // 10 │
            CadPoint2::new(1.0, 0.5),   // 11 ┘
        ];
        let outer = vec![0u32, 1, 2, 3];
        let h1 = vec![4u32, 5, 6, 7];
        let h2 = vec![8u32, 9, 10, 11];
        let tris = cdt_polygon_with_holes_floodfill(&verts, &outer, &[h1, h2])
            .expect("two mutually-tangent holes must weld and triangulate");
        assert!(
            (total_area(&verts, &tris) - 34.0).abs() < 1e-9,
            "36 − 1 − 1 = 34"
        );
        assert!(max_incidence(&tris) <= 2, "watertight local pairing");
    }

    /// THREE pool indices at ONE position: the outer ring and TWO holes all
    /// tangent at a single point (a triple weld to one spade handle). Both holes
    /// excluded: area = 36 − 1.5 − 1.5 = 33.
    #[test]
    fn adversary_three_indices_one_position() {
        let verts = vec![
            CadPoint2::new(-3.0, -3.0), // 0
            CadPoint2::new(0.0, -3.0),  // 1 outer touches (0,-3)
            CadPoint2::new(3.0, -3.0),  // 2
            CadPoint2::new(3.0, 3.0),   // 3
            CadPoint2::new(-3.0, 3.0),  // 4
            CadPoint2::new(0.0, -3.0),  // 5 hole1 — welds with 1
            CadPoint2::new(-1.0, -1.0), // 6
            CadPoint2::new(-2.0, -2.0), // 7
            CadPoint2::new(0.0, -3.0),  // 8 hole2 — welds with 1 & 5 (triple)
            CadPoint2::new(2.0, -2.0),  // 9
            CadPoint2::new(1.0, -1.0),  // 10
        ];
        let outer = vec![0u32, 1, 2, 3, 4];
        let h1 = vec![5u32, 6, 7];
        let h2 = vec![8u32, 9, 10];
        let tris = cdt_polygon_with_holes_floodfill(&verts, &outer, &[h1, h2])
            .expect("a triple weld at one position must triangulate");
        assert!(
            (total_area(&verts, &tris) - 33.0).abs() < 1e-9,
            "36 − 1.5 − 1.5 = 33"
        );
        assert!(max_incidence(&tris) <= 2, "watertight local pairing");
    }

    /// GUARD: welding must NOT mask a genuinely self-crossing outer ring — a
    /// bow-tie boundary must still fail loudly (the no-Steiner / can_add_constraint
    /// guard), never weld its way to a silent partition.
    #[test]
    fn guard_self_crossing_outer_stays_loud() {
        let verts = vec![
            CadPoint2::new(0.0, 0.0),
            CadPoint2::new(2.0, 2.0),
            CadPoint2::new(2.0, 0.0),
            CadPoint2::new(0.0, 2.0),
        ];
        let outer = vec![0u32, 1, 2, 3]; // bow-tie: edges 0-1 and 2-3 cross
        assert!(
            cdt_polygon_with_holes_floodfill(&verts, &outer, &[]).is_err(),
            "a self-crossing outer ring must fail loudly even with welding enabled"
        );
    }
}
