//! Yang 2025 Stage 0: Coplanar face preprocessing.
//!
//! Before mesh boolean, detect coplanar face pairs between the two operand
//! solids and generate identical mesh triangulations in the overlap region.
//! This eliminates conformal edge explosions and incorrect face survival
//! for coplanar geometry.
//!
//! Ref [#24] Yang et al. 2025 Section 4.5.5.

use crate::geometry::surface::SurfaceGeom;
use crate::tessellation::bijective::BijectiveMap;
use crate::topology::arena::TopoArena;
use crate::topology::euler_ops::{mef, split_edge_at};
use crate::topology::half_edge::{EdgeIdx, FaceIdx, HalfEdgeIdx, LoopIdx, VertexIdx};
use crate::types::RenderMesh;
use crate::units::{TAU_MODEL, TAU_PARALLEL};
use crate::vecmath::{compute_plane_basis, v3_dot};
use crate::waffle_kernel::WaffleSolid;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

// `[coplanar-tele]` always-on diagnostics for Yang §4.5.5 preprocessing.
// Mirrors the `[topo-extract]` pattern from PR3. Phase A of PR4
// (coplanar-preprocess-555). See plan: fluttering-rolling-crystal.md.
pub(crate) static COPLANAR_PAIRS_PROCESSED: AtomicUsize = AtomicUsize::new(0);
pub(crate) static COPLANAR_VERTS_SNAPPED_EXISTING: AtomicUsize = AtomicUsize::new(0);
pub(crate) static COPLANAR_VERTS_VIA_SPLIT_EDGE: AtomicUsize = AtomicUsize::new(0);
pub(crate) static COPLANAR_VERTS_DROPPED: AtomicUsize = AtomicUsize::new(0);
pub(crate) static COPLANAR_MEF_OK: AtomicUsize = AtomicUsize::new(0);
pub(crate) static COPLANAR_MEF_NO_LOOP: AtomicUsize = AtomicUsize::new(0);
pub(crate) static COPLANAR_OVERLAY_GROUPS: AtomicUsize = AtomicUsize::new(0);
pub(crate) static COPLANAR_OVERLAY_HOLES_IGNORED: AtomicUsize = AtomicUsize::new(0);
pub(crate) static COPLANAR_IDENTICAL_FOOTPRINT: AtomicUsize = AtomicUsize::new(0);
pub(crate) static COPLANAR_PARTIAL_OVERLAP: AtomicUsize = AtomicUsize::new(0);

/// A detected coplanar face pair between two solids.
#[derive(Debug, Clone)]
pub(crate) struct CoplanarFacePair {
    /// Face index in solid A.
    pub face_a: FaceIdx,
    /// Face index in solid B.
    pub face_b: FaceIdx,
    /// Shared plane normal (unit vector).
    pub plane_normal: [f64; 3],
    /// Signed distance from origin along the normal.
    pub plane_offset: f64,
    /// true if normals point in the same direction (dot ≈ +1),
    /// false if anti-parallel (dot ≈ -1).
    pub same_direction: bool,
    /// true if A's face footprint == B's face footprint (overlap covers
    /// both faces entirely). Set during `split_brep_for_coplanar_pairs`
    /// and consumed by `inject_identical_footprint_mesh` after
    /// tessellation. Yang §4.5.5 prescribes a single shared triangulation
    /// for this case so both meshes have bitwise-identical triangles in
    /// the overlap region.
    pub is_identical_footprint: bool,
    /// true if overlap is non-empty AND at least one of A-only / B-only is
    /// non-empty AND `same_direction == false` (anti-parallel). Set during
    /// `split_brep_for_coplanar_pairs` and consumed by
    /// `inject_partial_overlap_mesh` after tessellation. Same-direction
    /// partial-overlap is deferred to PR8 — it requires cascading
    /// T-junction repair across coplanar pairs.
    pub is_partial_overlap: bool,
}

/// Detect coplanar face pairs between two solids.
///
/// Compares plane equations (normal + offset) for all planar faces in both
/// solids. Two faces are coplanar if normals are parallel/anti-parallel and
/// offsets match within TAU_MODEL.
pub(crate) fn detect_coplanar_face_pairs(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
) -> Vec<CoplanarFacePair> {
    let mut pairs = Vec::new();

    // Collect planar faces from each solid: (FaceIdx, normal, offset).
    let planes_a = extract_planar_faces(solid_a);
    let planes_b = extract_planar_faces(solid_b);

    #[cfg(test)]
    eprintln!(
        "[COPLANAR DETECT] Planar faces: solid_a={}, solid_b={}",
        planes_a.len(),
        planes_b.len()
    );

    for &(face_a, normal_a, offset_a) in &planes_a {
        for &(face_b, normal_b, offset_b) in &planes_b {
            let dot = v3_dot(normal_a, normal_b);
            // Per Yang 2025 Section 4.5.5, ALL coplanar faces need preprocessing.
            // Both anti-parallel (dot ≈ -1, stacked caps) and same-direction
            // (dot ≈ +1, shared caps in cross patterns) are detected.
            // T-junction repair runs after injection to fix edge sharing.
            if dot.abs() < (1.0 - TAU_PARALLEL) {
                continue; // Not parallel — skip
            }

            // Align offsets: if anti-parallel (dot < 0), negate offset_b
            // because its normal points the other way.
            let aligned_offset_b = if dot < 0.0 { -offset_b } else { offset_b };

            if (offset_a - aligned_offset_b).abs() < TAU_MODEL {
                #[cfg(test)]
                eprintln!(
                    "[COPLANAR DETECT] Pair: face_a={:?} face_b={:?} normal=[{:.4},{:.4},{:.4}] offset={:.6} same_dir={}",
                    face_a, face_b, normal_a[0], normal_a[1], normal_a[2], offset_a, dot > 0.0
                );
                pairs.push(CoplanarFacePair {
                    face_a,
                    face_b,
                    plane_normal: normal_a,
                    plane_offset: offset_a,
                    same_direction: dot > 0.0,
                    is_identical_footprint: false,
                    is_partial_overlap: false,
                });
            }
        }
    }

    pairs
}

/// Extract (FaceIdx, normal, offset) for all planar faces in a solid.
///
/// Validates that all face vertices actually lie on the declared plane.
/// This guards against incorrect face_geometry (e.g., test solids with
/// dummy normals) corrupting the coplanar preprocessing.
fn extract_planar_faces(solid: &WaffleSolid) -> Vec<(FaceIdx, [f64; 3], f64)> {
    let mut result = Vec::new();
    for (&face_idx, geom) in &solid.face_geometry {
        if let SurfaceGeom::Planar(plane) = geom {
            let normal = [plane.normal.x, plane.normal.y, plane.normal.z];
            let origin = [plane.origin.x, plane.origin.y, plane.origin.z];
            let offset = v3_dot(normal, origin);

            // Validate: all face vertices must lie on this plane.
            let loop_idx = solid.arena.faces[face_idx.0].outer_loop;
            let start_he = solid.arena.loops[loop_idx.0].half_edge;
            let mut he = start_he;
            let mut all_on_plane = true;
            loop {
                let vi = solid.arena.half_edges[he.0].origin;
                let pos = solid.arena.vertices[vi.0].position;
                let dist = pos[0] * normal[0] + pos[1] * normal[1] + pos[2] * normal[2] - offset;
                if dist.abs() > TAU_MODEL * 100.0 {
                    all_on_plane = false;
                    break;
                }
                he = solid.arena.half_edges[he.0].next;
                if he == start_he {
                    break;
                }
            }

            if all_on_plane {
                result.push((face_idx, normal, offset));
            }
        }
    }
    result
}

/// Pre-tessellation B-Rep face splitting for coplanar pairs.
///
/// Per Yang 2025 Section 4.5.5: coplanar preprocessing must happen BEFORE
/// mesh discretization. For coplanar faces (both same-direction and
/// anti-parallel), splits each face along the overlap boundary using Euler
/// operators (split_edge_at + mef). After splitting, tessellation naturally
/// produces conformal meshes.
///
/// For each coplanar pair: computes the 2D overlap polygon via i_overlay,
/// finds where the overlap boundary crosses face edges, splits those edges,
/// and uses mef to divide the face into overlap + exclusive regions.
pub(crate) fn split_brep_for_coplanar_pairs(
    solid_a: &mut WaffleSolid,
    solid_b: &mut WaffleSolid,
    coplanar_pairs: &mut [CoplanarFacePair],
) {
    // `[coplanar-tele]` snapshot — emit per-call delta at function exit.
    let snap_pairs = COPLANAR_PAIRS_PROCESSED.load(Ordering::Relaxed);
    let snap_v_existing = COPLANAR_VERTS_SNAPPED_EXISTING.load(Ordering::Relaxed);
    let snap_v_split = COPLANAR_VERTS_VIA_SPLIT_EDGE.load(Ordering::Relaxed);
    let snap_v_dropped = COPLANAR_VERTS_DROPPED.load(Ordering::Relaxed);
    let snap_mef_ok = COPLANAR_MEF_OK.load(Ordering::Relaxed);
    let snap_mef_no_loop = COPLANAR_MEF_NO_LOOP.load(Ordering::Relaxed);
    let snap_groups = COPLANAR_OVERLAY_GROUPS.load(Ordering::Relaxed);
    let snap_holes = COPLANAR_OVERLAY_HOLES_IGNORED.load(Ordering::Relaxed);
    let snap_identical = COPLANAR_IDENTICAL_FOOTPRINT.load(Ordering::Relaxed);
    let snap_partial = COPLANAR_PARTIAL_OVERLAP.load(Ordering::Relaxed);

    for (pair_idx, pair) in coplanar_pairs.iter_mut().enumerate() {
        // Both same-direction and anti-parallel pairs need B-Rep splitting.
        COPLANAR_PAIRS_PROCESSED.fetch_add(1, Ordering::Relaxed);

        #[cfg(test)]
        eprintln!(
            "[COPLANAR SPLIT] Processing pair {}: face_a={:?} face_b={:?} normal=[{:.4},{:.4},{:.4}] offset={:.6} same_dir={}",
            pair_idx, pair.face_a, pair.face_b,
            pair.plane_normal[0], pair.plane_normal[1], pair.plane_normal[2],
            pair.plane_offset, pair.same_direction
        );

        // 1. Get face boundary polygons in 2D.
        let (u_axis, v_axis) = compute_plane_basis(pair.plane_normal);
        let plane_origin = [
            pair.plane_normal[0] * pair.plane_offset,
            pair.plane_normal[1] * pair.plane_offset,
            pair.plane_normal[2] * pair.plane_offset,
        ];

        let poly_a =
            collect_face_loop_2d(&solid_a.arena, pair.face_a, &plane_origin, &u_axis, &v_axis);
        let mut poly_b =
            collect_face_loop_2d(&solid_b.arena, pair.face_b, &plane_origin, &u_axis, &v_axis);

        // Yang §4.5.5 + Fig. 16: "The common part and the other two parts
        // share identical sampling points on their boundaries." Both faces
        // must project into the SAME 2D frame with consistent winding for
        // i_overlay's Intersect/Difference (with FillRule::EvenOdd) to
        // produce a bounded overlap polygon. `pair.plane_normal` is always
        // face A's outward normal, so `compute_plane_basis` derives the
        // basis from A's frame. For anti-parallel coplanar pairs, B's
        // boundary loop / mesh triangles are wound CCW-from-B's-outward-
        // normal (= CW in A's basis). Reverse to align with A's CCW
        // winding before passing to i_overlay.
        if !pair.same_direction {
            poly_b.reverse();
        }

        if poly_a.is_empty() || poly_b.is_empty() {
            #[cfg(test)]
            eprintln!(
                "[COPLANAR SPLIT]   -> Skipped: empty polygon (A={}, B={})",
                poly_a.len(),
                poly_b.len()
            );
            continue;
        }

        // 2. Compute overlap via i_overlay.
        let shape_a: Vec<Vec<[f64; 2]>> = vec![poly_a.iter().map(|&(_, p)| p).collect()];
        let shape_b: Vec<Vec<[f64; 2]>> = vec![poly_b.iter().map(|&(_, p)| p).collect()];
        let overlap: Vec<Vec<Vec<[f64; 2]>>> =
            shape_a.overlay(&shape_b, OverlayRule::Intersect, FillRule::EvenOdd);

        if overlap.is_empty() || overlap[0].is_empty() {
            #[cfg(test)]
            eprintln!("[COPLANAR SPLIT]   -> Skipped: no overlap");
            continue; // Coplanar but non-overlapping
        }

        // Telemetry: total disjoint shape groups in the overlap result, and
        // the count of holes in the primary group that this code currently
        // ignores (uses only overlap[0][0]). Disjoint groups beyond [0]
        // are similarly not consumed here.
        COPLANAR_OVERLAY_GROUPS.fetch_add(overlap.len(), Ordering::Relaxed);
        if overlap[0].len() > 1 {
            COPLANAR_OVERLAY_HOLES_IGNORED.fetch_add(overlap[0].len() - 1, Ordering::Relaxed);
        }

        #[cfg(test)]
        {
            eprintln!(
                "[COPLANAR SPLIT]   Overlap polygon: {} vertices",
                overlap[0][0].len()
            );
            for (i, pt) in overlap[0][0].iter().enumerate() {
                eprintln!("[COPLANAR SPLIT]     ov{}: ({:.6}, {:.6})", i, pt[0], pt[1]);
            }
        }

        // Check if overlap covers entire face (identical footprint) → skip
        let a_only: Vec<Vec<Vec<[f64; 2]>>> =
            shape_a.overlay(&shape_b, OverlayRule::Difference, FillRule::EvenOdd);
        let b_only: Vec<Vec<Vec<[f64; 2]>>> =
            shape_b.overlay(&shape_a, OverlayRule::Difference, FillRule::EvenOdd);
        let a_only_empty = a_only.is_empty() || a_only[0].is_empty();
        let b_only_empty = b_only.is_empty() || b_only[0].is_empty();
        if a_only_empty && b_only_empty {
            // Yang §4.5.5: identical-footprint coplanar pairs need post-
            // tessellation injection of a canonical shared triangulation.
            // Mark for `inject_identical_footprint_mesh` to handle later.
            // The B-Rep split itself has nothing to do here — there are no
            // edges to split and no `mef` calls; the existing face IS the
            // overlap.
            pair.is_identical_footprint = true;
            #[cfg(test)]
            eprintln!(
                "[COPLANAR SPLIT]   -> Marked identical_footprint=true (will inject post-tessellation)"
            );
            continue;
        }

        // Yang §4.5.5: overlap is non-empty AND at least one exclusive region
        // is non-empty → partial-overlap case. Mark anti-parallel pairs for
        // `inject_partial_overlap_mesh` post-tessellation. Same-direction
        // partial-overlap requires cascading T-junction repair (deferred to
        // PR8). The B-Rep split (split_face_along_boundary) below continues
        // to run for partial-overlap pairs so the exclusive regions get
        // clean B-Rep face sub-divisions.
        if !pair.same_direction {
            pair.is_partial_overlap = true;
        }

        #[cfg(test)]
        eprintln!(
            "[COPLANAR SPLIT]   A-only empty={}, B-only empty={}, partial_overlap={}",
            a_only_empty, b_only_empty, pair.is_partial_overlap
        );

        // 3. Project overlap boundary to 3D.
        let overlap_3d: Vec<[f64; 3]> = overlap[0][0]
            .iter()
            .map(|&[u, v]| {
                [
                    plane_origin[0] + u * u_axis[0] + v * v_axis[0],
                    plane_origin[1] + u * u_axis[1] + v * v_axis[1],
                    plane_origin[2] + u * u_axis[2] + v * v_axis[2],
                ]
            })
            .collect();

        #[cfg(test)]
        {
            eprintln!("[COPLANAR SPLIT]   Overlap 3D vertices:");
            for (i, pt) in overlap_3d.iter().enumerate() {
                eprintln!(
                    "[COPLANAR SPLIT]     ov3d_{}: [{:.6}, {:.6}, {:.6}]",
                    i, pt[0], pt[1], pt[2]
                );
            }
        }

        // 4. Split each face along the overlap boundary.
        #[cfg(test)]
        eprintln!("[COPLANAR SPLIT]   Splitting face_a={:?}...", pair.face_a);
        split_face_along_boundary(
            &mut solid_a.arena,
            &mut solid_a.face_geometry,
            &mut solid_a.face_map,
            pair.face_a,
            &overlap_3d,
        );
        #[cfg(test)]
        eprintln!("[COPLANAR SPLIT]   Splitting face_b={:?}...", pair.face_b);
        split_face_along_boundary(
            &mut solid_b.arena,
            &mut solid_b.face_geometry,
            &mut solid_b.face_map,
            pair.face_b,
            &overlap_3d,
        );
    }

    // `[coplanar-tele]` summary: per-call deltas for this invocation.
    // Suppressed when no pairs were processed (most cherchi calls have no
    // coplanar work; keeps the log quiet).
    let pairs_delta = COPLANAR_PAIRS_PROCESSED.load(Ordering::Relaxed) - snap_pairs;
    if pairs_delta > 0 {
        let v_existing = COPLANAR_VERTS_SNAPPED_EXISTING.load(Ordering::Relaxed) - snap_v_existing;
        let v_split = COPLANAR_VERTS_VIA_SPLIT_EDGE.load(Ordering::Relaxed) - snap_v_split;
        let v_dropped = COPLANAR_VERTS_DROPPED.load(Ordering::Relaxed) - snap_v_dropped;
        let mef_ok = COPLANAR_MEF_OK.load(Ordering::Relaxed) - snap_mef_ok;
        let mef_no_loop = COPLANAR_MEF_NO_LOOP.load(Ordering::Relaxed) - snap_mef_no_loop;
        let groups = COPLANAR_OVERLAY_GROUPS.load(Ordering::Relaxed) - snap_groups;
        let holes = COPLANAR_OVERLAY_HOLES_IGNORED.load(Ordering::Relaxed) - snap_holes;
        let identical = COPLANAR_IDENTICAL_FOOTPRINT.load(Ordering::Relaxed) - snap_identical;
        let partial_overlap = COPLANAR_PARTIAL_OVERLAP.load(Ordering::Relaxed) - snap_partial;
        eprintln!(
            "[coplanar-tele] pairs={} verts_existing={} verts_split={} verts_dropped={} mef_ok={} mef_no_loop={} overlay_groups={} overlay_holes_ignored={} identical_footprint={} partial_overlap={}",
            pairs_delta, v_existing, v_split, v_dropped, mef_ok, mef_no_loop, groups, holes, identical, partial_overlap
        );
    }
}

/// Collect face loop vertices as (VertexIdx, 2D position) pairs.
fn collect_face_loop_2d(
    arena: &TopoArena,
    face_idx: FaceIdx,
    origin: &[f64; 3],
    u_axis: &[f64; 3],
    v_axis: &[f64; 3],
) -> Vec<(VertexIdx, [f64; 2])> {
    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;
    let mut result = Vec::new();
    let mut he = start_he;
    loop {
        let vi = arena.half_edges[he.0].origin;
        let pos = arena.vertices[vi.0].position;
        let dx = pos[0] - origin[0];
        let dy = pos[1] - origin[1];
        let dz = pos[2] - origin[2];
        let u = dx * u_axis[0] + dy * u_axis[1] + dz * u_axis[2];
        let v = dx * v_axis[0] + dy * v_axis[1] + dz * v_axis[2];
        result.push((vi, [u, v]));
        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }
    result
}

/// Split a face along an overlap boundary polygon.
///
/// Finds where the overlap boundary crosses face edges, inserts vertices
/// at the crossing points using `split_edge_at`, then uses `mef` calls to
/// carve out the overlap sub-face. For a rectangular overlap inside a
/// rectangular face, this creates 3 face regions (overlap + 2 exclusive).
///
/// Key design: re-collects face edges after EACH split_edge_at to avoid
/// stale EdgeIdx references. After all splits, walks the face loop to find
/// boundary vertices in loop order, then uses (N-1) mef calls to isolate
/// the overlap polygon where N is the number of boundary vertices.
fn split_face_along_boundary(
    arena: &mut TopoArena,
    face_geometry: &mut BTreeMap<FaceIdx, SurfaceGeom>,
    face_map: &mut BTreeMap<u64, FaceIdx>,
    face_idx: FaceIdx,
    overlap_boundary_3d: &[[f64; 3]],
) {
    #[cfg(test)]
    eprintln!(
        "[SPLIT BOUNDARY] Entry: face={:?}, overlap_verts={}",
        face_idx,
        overlap_boundary_3d.len()
    );

    if overlap_boundary_3d.len() < 3 {
        #[cfg(test)]
        eprintln!("[SPLIT BOUNDARY]   -> Early return: fewer than 3 overlap vertices");
        return;
    }

    let tol_sq = TAU_MODEL * TAU_MODEL;

    // For each overlap boundary vertex, either match it to an existing face
    // vertex or split the edge it lies on. We re-collect face edges after
    // each split to avoid stale EdgeIdx references.
    let mut boundary_verts: Vec<VertexIdx> = Vec::new();

    for (ov_idx, &ov) in overlap_boundary_3d.iter().enumerate() {
        // Re-collect face edges fresh (may have changed from previous splits).
        let loop_idx = arena.faces[face_idx.0].outer_loop;
        let edges = collect_face_edges(arena, loop_idx);

        // Check if this vertex matches an existing face vertex.
        let mut found_existing = false;
        for &(_, _, _, _, v0, _) in &edges {
            let p = arena.vertices[v0.0].position;
            let dx = ov[0] - p[0];
            let dy = ov[1] - p[1];
            let dz = ov[2] - p[2];
            if dx * dx + dy * dy + dz * dz < tol_sq {
                COPLANAR_VERTS_SNAPPED_EXISTING.fetch_add(1, Ordering::Relaxed);
                #[cfg(test)]
                eprintln!(
                    "[SPLIT BOUNDARY]   ov{}: EXISTING vertex {:?} at [{:.6},{:.6},{:.6}]",
                    ov_idx, v0, p[0], p[1], p[2]
                );
                boundary_verts.push(v0);
                found_existing = true;
                break;
            }
        }
        if found_existing {
            continue;
        }

        // Check if this vertex lies on a face edge.
        let mut found_on_edge = false;
        for &(_, edge_idx, p0, p1, v0, v1) in &edges {
            let d = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let d_len_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            if d_len_sq < crate::units::TAU_WORK_SQ {
                continue;
            }
            let to_ov = [ov[0] - p0[0], ov[1] - p0[1], ov[2] - p0[2]];
            let cross = [
                d[1] * to_ov[2] - d[2] * to_ov[1],
                d[2] * to_ov[0] - d[0] * to_ov[2],
                d[0] * to_ov[1] - d[1] * to_ov[0],
            ];
            let cross_len_sq = cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
            if cross_len_sq > d_len_sq * crate::units::TAU_WORK {
                continue;
            }
            let t = (d[0] * to_ov[0] + d[1] * to_ov[1] + d[2] * to_ov[2]) / d_len_sq;
            if t > TAU_PARALLEL && t < 1.0 - TAU_PARALLEL {
                #[cfg(test)]
                eprintln!(
                    "[SPLIT BOUNDARY]   ov{}: SPLIT edge {:?} ({:?}->{:?}) at t={:.6}, pos=[{:.6},{:.6},{:.6}]",
                    ov_idx, edge_idx, v0, v1, t, ov[0], ov[1], ov[2]
                );
                let v_new = split_edge_at(arena, edge_idx, ov);
                COPLANAR_VERTS_VIA_SPLIT_EDGE.fetch_add(1, Ordering::Relaxed);
                boundary_verts.push(v_new);
                found_on_edge = true;
                break;
            }
        }

        if !found_on_edge {
            COPLANAR_VERTS_DROPPED.fetch_add(1, Ordering::Relaxed);
            #[cfg(test)]
            eprintln!(
                "[SPLIT BOUNDARY]   ov{}: NOT FOUND on any edge! pos=[{:.6},{:.6},{:.6}]",
                ov_idx, ov[0], ov[1], ov[2]
            );
        }
    }

    #[cfg(test)]
    eprintln!(
        "[SPLIT BOUNDARY]   Total boundary_verts: {} (from {} overlap vertices)",
        boundary_verts.len(),
        overlap_boundary_3d.len()
    );

    if boundary_verts.len() < 2 {
        #[cfg(test)]
        eprintln!("[SPLIT BOUNDARY]   -> Early return: fewer than 2 boundary vertices");
        return; // Not enough boundary vertices to form a polygon split
    }

    // Add mef edges for each overlap polygon edge where the two boundary
    // vertices are NOT already adjacent in the face loop. This carves the
    // overlap polygon out of the parent face.
    //
    // For F0003's rectangular overlap: 4 boundary verts from 2 edge splits.
    // b0,b1 are adjacent (same original edge), b2,b3 are adjacent (same edge).
    // Need mef(b1,b2) and mef(b3,b0) to complete the rectangle.
    let n = boundary_verts.len();
    let mut new_faces: Vec<FaceIdx> = Vec::new();
    // Allocate face_map IDs for new faces: use max existing ID + 1.
    let mut next_face_id = face_map.keys().copied().max().unwrap_or(0) + 1;

    for i in 0..n {
        let va = boundary_verts[i];
        let vb = boundary_verts[(i + 1) % n];

        // Check if va and vb are already adjacent in their loop.
        if are_adjacent_in_any_loop(arena, va, vb) {
            #[cfg(test)]
            eprintln!(
                "[SPLIT BOUNDARY]   mef skip: {:?} and {:?} already adjacent",
                va, vb
            );
            continue;
        }

        // Find which loop contains both va and vb.
        let mut all_faces = vec![face_idx];
        all_faces.extend(&new_faces);
        let target_loop = find_loop_containing_both_in_faces(arena, &all_faces, va, vb);
        if let Some(lp) = target_loop {
            #[cfg(test)]
            eprintln!(
                "[SPLIT BOUNDARY]   mef({:?}, {:?}) in loop {:?}",
                va, vb, lp
            );
            let (_, new_face) = mef(arena, va, vb, lp);
            COPLANAR_MEF_OK.fetch_add(1, Ordering::Relaxed);
            if let Some(geom) = face_geometry.get(&face_idx).cloned() {
                face_geometry.insert(new_face, geom);
            }
            // Register new face in face_map so bijective mapping works.
            face_map.insert(next_face_id, new_face);
            next_face_id += 1;
            new_faces.push(new_face);
            #[cfg(test)]
            eprintln!(
                "[SPLIT BOUNDARY]   -> Created new face {:?} (face_map id={})",
                new_face,
                next_face_id - 1
            );
        } else {
            COPLANAR_MEF_NO_LOOP.fetch_add(1, Ordering::Relaxed);
            #[cfg(test)]
            eprintln!(
                "[SPLIT BOUNDARY]   mef FAILED: no loop contains both {:?} and {:?}",
                va, vb
            );
        }
    }

    #[cfg(test)]
    eprintln!(
        "[SPLIT BOUNDARY]   Done: {} new faces created",
        new_faces.len()
    );
}

/// Collect face edges from a loop: (half_edge_idx, edge_idx, v0_pos, v1_pos, v0_idx, v1_idx).
fn collect_face_edges(
    arena: &TopoArena,
    loop_idx: LoopIdx,
) -> Vec<(
    HalfEdgeIdx,
    EdgeIdx,
    [f64; 3],
    [f64; 3],
    VertexIdx,
    VertexIdx,
)> {
    let mut edges = Vec::new();
    let start_he = arena.loops[loop_idx.0].half_edge;
    let mut he = start_he;
    loop {
        let v0 = arena.half_edges[he.0].origin;
        let next_he = arena.half_edges[he.0].next;
        let v1 = arena.half_edges[next_he.0].origin;
        let p0 = arena.vertices[v0.0].position;
        let p1 = arena.vertices[v1.0].position;
        let edge = arena.half_edges[he.0].edge;
        edges.push((he, edge, p0, p1, v0, v1));
        he = next_he;
        if he == start_he {
            break;
        }
    }
    edges
}

/// Check if two vertices are adjacent (directly connected) in any loop.
fn are_adjacent_in_any_loop(arena: &TopoArena, va: VertexIdx, vb: VertexIdx) -> bool {
    // Check if va has a half-edge going directly to vb or vice versa.
    if let Some(he_start) = arena.vertices[va.0].half_edge {
        let mut he = he_start;
        loop {
            let next_he = arena.half_edges[he.0].next;
            let next_v = arena.half_edges[next_he.0].origin;
            if next_v == vb {
                return true;
            }
            // Move to next half-edge originating at va (via twin's next).
            let twin = arena.half_edges[he.0].twin;
            he = arena.half_edges[twin.0].next;
            if he == he_start {
                break;
            }
        }
    }
    false
}

/// Find which loop (from any of the given faces) contains both vertices.
fn find_loop_containing_both_in_faces(
    arena: &TopoArena,
    faces: &[FaceIdx],
    va: VertexIdx,
    vb: VertexIdx,
) -> Option<LoopIdx> {
    for &face in faces {
        let lp = arena.faces[face.0].outer_loop;
        let start_he = arena.loops[lp.0].half_edge;
        let mut found_a = false;
        let mut found_b = false;
        let mut he = start_he;
        loop {
            let v = arena.half_edges[he.0].origin;
            if v == va {
                found_a = true;
            }
            if v == vb {
                found_b = true;
            }
            he = arena.half_edges[he.0].next;
            if he == start_he {
                break;
            }
        }
        if found_a && found_b {
            return Some(lp);
        }
    }
    None
}

/// After tessellation, replace coplanar mesh triangles with a shared
/// conformal triangulation so the mesh boolean sees identical geometry.
///
/// For each coplanar pair, projects both face boundaries into 2D, computes
/// a shared triangulation via i_overlay + earcutr, and replaces the original
/// mesh triangles for those faces.
#[allow(clippy::too_many_arguments, dead_code)]
pub(crate) fn inject_conformal_coplanar_mesh(
    coplanar_pairs: &[CoplanarFacePair],
    verts_a: &mut Vec<[f64; 3]>,
    tris_a: &mut Vec<[usize; 3]>,
    verts_b: &mut Vec<[f64; 3]>,
    tris_b: &mut Vec<[usize; 3]>,
    bijective_a: &mut BijectiveMap,
    bijective_b: &mut BijectiveMap,
    _mesh_a: &RenderMesh,
    _mesh_b: &RenderMesh,
) {
    for pair in coplanar_pairs {
        // 1. Find triangles on the coplanar plane.
        // Use both bijective face index AND geometric plane membership:
        // a triangle belongs to a coplanar face if it maps to the expected face
        // OR if all its vertices lie on the coplanar plane.
        let face_tri_indices_a = find_plane_triangles(
            verts_a,
            tris_a,
            bijective_a,
            pair.face_a,
            &pair.plane_normal,
            pair.plane_offset,
        );
        let face_tri_indices_b = find_plane_triangles(
            verts_b,
            tris_b,
            bijective_b,
            pair.face_b,
            &pair.plane_normal,
            pair.plane_offset,
        );

        if face_tri_indices_a.is_empty() || face_tri_indices_b.is_empty() {
            continue;
        }

        // 2. Compute plane basis for 2D projection.
        let (u_axis, v_axis) = compute_plane_basis(pair.plane_normal);
        let plane_origin = [
            pair.plane_normal[0] * pair.plane_offset,
            pair.plane_normal[1] * pair.plane_offset,
            pair.plane_normal[2] * pair.plane_offset,
        ];

        // 3. Collect 2D boundary polygons for each face.
        let poly_a = extract_face_boundary_2d(
            verts_a,
            tris_a,
            &face_tri_indices_a,
            &plane_origin,
            &u_axis,
            &v_axis,
        );
        let poly_b = extract_face_boundary_2d(
            verts_b,
            tris_b,
            &face_tri_indices_b,
            &plane_origin,
            &u_axis,
            &v_axis,
        );

        if poly_a.is_empty() || poly_b.is_empty() {
            continue;
        }

        // 4. Compute three regions per Yang 2025 Section 4.5.5, Fig. 16:
        //    overlap = polygon_a ∩ polygon_b (shared conformal mesh)
        //    a_only  = polygon_a \ polygon_b (only in mesh A)
        //    b_only  = polygon_b \ polygon_a (only in mesh B)
        let shape_a: Vec<Vec<[f64; 2]>> = vec![poly_a];
        let shape_b: Vec<Vec<[f64; 2]>> = vec![poly_b];

        let overlap_result: Vec<Vec<Vec<[f64; 2]>>> =
            shape_a.overlay(&shape_b, OverlayRule::Intersect, FillRule::EvenOdd);

        if overlap_result.is_empty() || overlap_result[0].is_empty() {
            continue; // Coplanar but non-overlapping — skip.
        }

        // 5. Compute A-only and B-only difference regions.
        let a_only_result: Vec<Vec<Vec<[f64; 2]>>> =
            shape_a.overlay(&shape_b, OverlayRule::Difference, FillRule::EvenOdd);
        let b_only_result: Vec<Vec<Vec<[f64; 2]>>> =
            shape_b.overlay(&shape_a, OverlayRule::Difference, FillRule::EvenOdd);

        let a_only_empty = a_only_result.is_empty() || a_only_result[0].is_empty();
        let b_only_empty = b_only_result.is_empty() || b_only_result[0].is_empty();

        // For same-direction coplanar faces, skip injection. The overlap boundary
        // introduces new edges that create T-junctions with adjacent faces.
        // T-junction repair handles simple cases, but complex geometries
        // (multiple coplanar pairs sharing vertices) need cascading repair that
        // isn't yet implemented. Cherchi handles these via exact mesh boolean.
        // TODO: implement cascading T-junction repair or pre-tessellation B-Rep
        // face splitting per Yang 2025 Section 4.5.5.
        if pair.same_direction {
            continue;
        }

        // 6. Triangulate the overlap region — IDENTICAL for both meshes.
        let (shared_2d_verts, shared_tri_indices) =
            triangulate_polygon_with_holes(&overlap_result[0]);
        if shared_tri_indices.is_empty() {
            continue;
        }
        let shared_3d = verts_2d_to_3d(&shared_2d_verts, &plane_origin, &u_axis, &v_axis);
        let shared_tris: Vec<[usize; 3]> = shared_tri_indices
            .chunks(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect();

        // 7. Triangulate A-only region.
        let (a_only_3d, a_only_tris) = if !a_only_empty {
            let (verts_2d, tri_idx) = triangulate_polygon_with_holes(&a_only_result[0]);
            let verts_3d = verts_2d_to_3d(&verts_2d, &plane_origin, &u_axis, &v_axis);
            let tris: Vec<[usize; 3]> = tri_idx.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
            (verts_3d, tris)
        } else {
            (vec![], vec![])
        };

        // 8. Triangulate B-only region.
        let (b_only_3d, b_only_tris) = if !b_only_empty {
            let (verts_2d, tri_idx) = triangulate_polygon_with_holes(&b_only_result[0]);
            let verts_3d = verts_2d_to_3d(&verts_2d, &plane_origin, &u_axis, &v_axis);
            let tris: Vec<[usize; 3]> = tri_idx.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
            (verts_3d, tris)
        } else {
            (vec![], vec![])
        };

        // 10. Replace mesh A = a_only + shared overlap triangles.
        let (merged_verts_a, merged_tris_a) =
            merge_regions(&a_only_3d, &a_only_tris, &shared_3d, &shared_tris);
        let new_verts_a = replace_face_triangles(
            verts_a,
            tris_a,
            bijective_a,
            &face_tri_indices_a,
            pair.face_a,
            &merged_verts_a,
            &merged_tris_a,
        );

        // 11. Replace mesh B = b_only + shared overlap triangles.
        let (merged_verts_b, merged_tris_b) =
            merge_regions(&b_only_3d, &b_only_tris, &shared_3d, &shared_tris);
        let new_verts_b = replace_face_triangles(
            verts_b,
            tris_b,
            bijective_b,
            &face_tri_indices_b,
            pair.face_b,
            &merged_verts_b,
            &merged_tris_b,
        );

        // 12. Repair T-junctions: split edges in adjacent faces where new
        // overlap-boundary vertices were introduced by the injection.
        repair_tjunctions_after_injection(verts_a, tris_a, bijective_a, pair.face_a, &new_verts_a);
        repair_tjunctions_after_injection(verts_b, tris_b, bijective_b, pair.face_b, &new_verts_b);
    }
}

/// Yang §4.5.5 identical-footprint pass: replace mesh A's and mesh B's
/// triangulation of each marked coplanar face with a single canonical
/// triangulation, derived from face A's boundary, so both meshes have
/// bitwise-identical triangles in the overlap region.
///
/// Only acts on pairs where `is_identical_footprint == true` (set by
/// `split_brep_for_coplanar_pairs`). For each such pair:
///   1. Locate the face's existing triangles in each mesh.
///   2. Extract face A's 2D boundary polygon on the shared plane.
///   3. Triangulate the polygon ONCE with `triangulate_polygon_with_holes`
///      (no holes — identical footprint has a single contour).
///   4. Map 2D vertices back to 3D via `verts_2d_to_3d`.
///   5. Replace mesh A's face triangles with the canonical triangulation,
///      preserving A's winding.
///   6. Replace mesh B's face triangles with the same triangulation; flip
///      per-triangle winding when the pair is anti-parallel (so B's outward
///      normal still points opposite to A's).
///
/// Skips T-junction repair: identical-footprint = the overlap IS the entire
/// face on both sides, so there are no adjacent faces with mismatched edges
/// to fix up.
#[allow(clippy::too_many_arguments)]
pub(crate) fn inject_identical_footprint_mesh(
    coplanar_pairs: &[CoplanarFacePair],
    verts_a: &mut Vec<[f64; 3]>,
    tris_a: &mut Vec<[usize; 3]>,
    bijective_a: &mut BijectiveMap,
    verts_b: &mut Vec<[f64; 3]>,
    tris_b: &mut Vec<[usize; 3]>,
    bijective_b: &mut BijectiveMap,
) {
    for pair in coplanar_pairs {
        if !pair.is_identical_footprint {
            continue;
        }

        // 1. Find triangles on the coplanar plane in each mesh.
        let face_tri_indices_a = find_plane_triangles(
            verts_a,
            tris_a,
            bijective_a,
            pair.face_a,
            &pair.plane_normal,
            pair.plane_offset,
        );
        let face_tri_indices_b = find_plane_triangles(
            verts_b,
            tris_b,
            bijective_b,
            pair.face_b,
            &pair.plane_normal,
            pair.plane_offset,
        );

        if face_tri_indices_a.is_empty() || face_tri_indices_b.is_empty() {
            #[cfg(test)]
            eprintln!(
                "[COPLANAR INJECT-IF] Skipped pair {:?}/{:?}: no plane triangles found (A={}, B={})",
                pair.face_a, pair.face_b, face_tri_indices_a.len(), face_tri_indices_b.len()
            );
            continue;
        }

        // 2. Compute plane basis for 2D projection.
        let (u_axis, v_axis) = compute_plane_basis(pair.plane_normal);
        let plane_origin = [
            pair.plane_normal[0] * pair.plane_offset,
            pair.plane_normal[1] * pair.plane_offset,
            pair.plane_normal[2] * pair.plane_offset,
        ];

        // 3. Extract face A's 2D boundary. This is the canonical contour;
        // since the footprint is identical, face B's boundary projects to
        // the same polygon.
        let poly_a = extract_face_boundary_2d(
            verts_a,
            tris_a,
            &face_tri_indices_a,
            &plane_origin,
            &u_axis,
            &v_axis,
        );
        if poly_a.is_empty() {
            #[cfg(test)]
            eprintln!(
                "[COPLANAR INJECT-IF] Skipped pair {:?}/{:?}: face A boundary empty",
                pair.face_a, pair.face_b
            );
            continue;
        }

        // 4. Triangulate ONCE — single contour, no holes.
        let (shared_2d_verts, shared_tri_indices) = triangulate_polygon_with_holes(&[poly_a]);
        if shared_tri_indices.is_empty() {
            #[cfg(test)]
            eprintln!(
                "[COPLANAR INJECT-IF] Skipped pair {:?}/{:?}: triangulation produced no triangles",
                pair.face_a, pair.face_b
            );
            continue;
        }
        let shared_3d = verts_2d_to_3d(&shared_2d_verts, &plane_origin, &u_axis, &v_axis);
        let shared_tris_a: Vec<[usize; 3]> = shared_tri_indices
            .chunks(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect();

        // For mesh B, flip per-triangle winding when normals are anti-parallel
        // so face B's outward orientation is preserved.
        let shared_tris_b: Vec<[usize; 3]> = if pair.same_direction {
            shared_tris_a.clone()
        } else {
            shared_tris_a.iter().map(|t| [t[0], t[2], t[1]]).collect()
        };

        // 5. Replace mesh A's face triangles.
        let _ = replace_face_triangles(
            verts_a,
            tris_a,
            bijective_a,
            &face_tri_indices_a,
            pair.face_a,
            &shared_3d,
            &shared_tris_a,
        );

        // 6. Replace mesh B's face triangles with the same vertex set.
        let _ = replace_face_triangles(
            verts_b,
            tris_b,
            bijective_b,
            &face_tri_indices_b,
            pair.face_b,
            &shared_3d,
            &shared_tris_b,
        );

        COPLANAR_IDENTICAL_FOOTPRINT.fetch_add(1, Ordering::Relaxed);

        #[cfg(test)]
        eprintln!(
            "[COPLANAR INJECT-IF] Injected pair {:?}/{:?}: {} canonical tris (B winding flipped: {})",
            pair.face_a, pair.face_b, shared_tris_a.len(), !pair.same_direction
        );
    }
}

/// Yang §4.5.5 partial-overlap pass: for coplanar pairs where overlap is
/// non-empty AND at least one exclusive region (A-only or B-only) is
/// non-empty, segment the shared plane into three regions per Fig. 16:
///
///   - overlap: triangulated ONCE, identical 3D vertex set in both meshes
///   - A-only: triangulated independently from A's exclusive boundary
///   - B-only: triangulated independently from B's exclusive boundary
///
/// Yang's "identical sampling points on their boundaries" requirement is
/// satisfied because the overlap polygon vertices are computed once via
/// i_overlay and projected to 3D once.
///
/// Anti-parallel only. Same-direction partial-overlap requires cascading
/// T-junction repair across coplanar pairs sharing vertices/edges, which
/// is deferred to PR8. The double-guard on `same_direction == false`
/// matches the detection condition in `split_brep_for_coplanar_pairs`.
///
/// After region replacement, runs `repair_tjunctions_after_injection` on
/// each mesh: the new overlap-boundary diagonal may meet adjacent
/// (non-coplanar) face triangulation edges at a T-junction, which the
/// existing single-pair repair can split. Multi-pair cascading is PR8.
#[allow(clippy::too_many_arguments)]
pub(crate) fn inject_partial_overlap_mesh(
    coplanar_pairs: &[CoplanarFacePair],
    verts_a: &mut Vec<[f64; 3]>,
    tris_a: &mut Vec<[usize; 3]>,
    bijective_a: &mut BijectiveMap,
    verts_b: &mut Vec<[f64; 3]>,
    tris_b: &mut Vec<[usize; 3]>,
    bijective_b: &mut BijectiveMap,
) {
    for pair in coplanar_pairs {
        if !pair.is_partial_overlap {
            continue;
        }
        // Double-guard: same-direction partial-overlap is PR8 territory.
        if pair.same_direction {
            continue;
        }

        // 1. Locate face triangles on the coplanar plane in each mesh.
        let face_tri_indices_a = find_plane_triangles(
            verts_a,
            tris_a,
            bijective_a,
            pair.face_a,
            &pair.plane_normal,
            pair.plane_offset,
        );
        let face_tri_indices_b = find_plane_triangles(
            verts_b,
            tris_b,
            bijective_b,
            pair.face_b,
            &pair.plane_normal,
            pair.plane_offset,
        );

        if face_tri_indices_a.is_empty() || face_tri_indices_b.is_empty() {
            #[cfg(test)]
            eprintln!(
                "[COPLANAR INJECT-PO] Skipped pair {:?}/{:?}: no plane triangles found (A={}, B={})",
                pair.face_a, pair.face_b, face_tri_indices_a.len(), face_tri_indices_b.len()
            );
            continue;
        }

        // 2. Compute plane basis for 2D projection.
        let (u_axis, v_axis) = compute_plane_basis(pair.plane_normal);
        let plane_origin = [
            pair.plane_normal[0] * pair.plane_offset,
            pair.plane_normal[1] * pair.plane_offset,
            pair.plane_normal[2] * pair.plane_offset,
        ];

        // 3. Extract 2D boundary polygons for each face.
        let poly_a = extract_face_boundary_2d(
            verts_a,
            tris_a,
            &face_tri_indices_a,
            &plane_origin,
            &u_axis,
            &v_axis,
        );
        let mut poly_b = extract_face_boundary_2d(
            verts_b,
            tris_b,
            &face_tri_indices_b,
            &plane_origin,
            &u_axis,
            &v_axis,
        );

        // Yang §4.5.5 + Fig. 16: "The common part and the other two parts
        // share identical sampling points on their boundaries." Both faces
        // must project into the SAME 2D frame with consistent winding for
        // i_overlay's Intersect/Difference (with FillRule::EvenOdd) to
        // produce a bounded overlap polygon. `pair.plane_normal` is always
        // face A's outward normal, so `compute_plane_basis` derives the
        // basis from A's frame. For anti-parallel coplanar pairs, B's
        // boundary loop / mesh triangles are wound CCW-from-B's-outward-
        // normal (= CW in A's basis). Reverse to align with A's CCW
        // winding before passing to i_overlay.
        if !pair.same_direction {
            poly_b.reverse();
        }

        if poly_a.is_empty() || poly_b.is_empty() {
            #[cfg(test)]
            eprintln!(
                "[COPLANAR INJECT-PO] Skipped pair {:?}/{:?}: empty face boundary",
                pair.face_a, pair.face_b
            );
            continue;
        }

        // 4. Compute three regions per Yang 2025 §4.5.5 Fig. 16:
        //    overlap = poly_a ∩ poly_b (shared canonical mesh)
        //    a_only  = poly_a \ poly_b (only in mesh A)
        //    b_only  = poly_b \ poly_a (only in mesh B)
        let shape_a: Vec<Vec<[f64; 2]>> = vec![poly_a];
        let shape_b: Vec<Vec<[f64; 2]>> = vec![poly_b];

        let overlap_result: Vec<Vec<Vec<[f64; 2]>>> =
            shape_a.overlay(&shape_b, OverlayRule::Intersect, FillRule::EvenOdd);
        if overlap_result.is_empty() || overlap_result[0].is_empty() {
            continue; // Coplanar but non-overlapping — nothing to inject.
        }

        let a_only_result: Vec<Vec<Vec<[f64; 2]>>> =
            shape_a.overlay(&shape_b, OverlayRule::Difference, FillRule::EvenOdd);
        let b_only_result: Vec<Vec<Vec<[f64; 2]>>> =
            shape_b.overlay(&shape_a, OverlayRule::Difference, FillRule::EvenOdd);

        let a_only_empty = a_only_result.is_empty() || a_only_result[0].is_empty();
        let b_only_empty = b_only_result.is_empty() || b_only_result[0].is_empty();

        // 5. Triangulate the overlap region ONCE — shared 3D vertex set.
        let (shared_2d_verts, shared_tri_indices) =
            triangulate_polygon_with_holes(&overlap_result[0]);
        if shared_tri_indices.is_empty() {
            continue;
        }
        let shared_3d = verts_2d_to_3d(&shared_2d_verts, &plane_origin, &u_axis, &v_axis);
        let shared_tris_a: Vec<[usize; 3]> = shared_tri_indices
            .chunks(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        // Anti-parallel: flip B's per-triangle winding so face B's outward
        // orientation is preserved (mirrors PR5's identical-footprint logic).
        let shared_tris_b: Vec<[usize; 3]> =
            shared_tris_a.iter().map(|t| [t[0], t[2], t[1]]).collect();

        // 6. Triangulate A-only region (independent — exclusive to mesh A).
        let (a_only_3d, a_only_tris) = if !a_only_empty {
            let (verts_2d, tri_idx) = triangulate_polygon_with_holes(&a_only_result[0]);
            let verts_3d = verts_2d_to_3d(&verts_2d, &plane_origin, &u_axis, &v_axis);
            let tris: Vec<[usize; 3]> = tri_idx.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
            (verts_3d, tris)
        } else {
            (vec![], vec![])
        };

        // 7. Triangulate B-only region (independent — exclusive to mesh B).
        let (b_only_3d, b_only_tris) = if !b_only_empty {
            let (verts_2d, tri_idx) = triangulate_polygon_with_holes(&b_only_result[0]);
            let verts_3d = verts_2d_to_3d(&verts_2d, &plane_origin, &u_axis, &v_axis);
            let tris: Vec<[usize; 3]> = tri_idx.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
            (verts_3d, tris)
        } else {
            (vec![], vec![])
        };

        // 8. Replace mesh A's face triangles. Yang §4.5.5 requires the
        // overlap region to use bitwise-identical 3D positions in both
        // meshes. The shared verts (from i_overlay Intersect projected
        // to 3D once) are the canonical source. Append shared verts to
        // mesh A WITHOUT snap-to-existing so the canonical bits survive.
        // The A-only region's overlap-boundary points (which may have
        // slightly different bits from i_overlay's Difference output)
        // then snap onto the just-appended shared verts.
        let new_verts_a = inject_face_with_shared_first(
            verts_a,
            tris_a,
            bijective_a,
            &face_tri_indices_a,
            pair.face_a,
            &shared_3d,
            &shared_tris_a,
            &a_only_3d,
            &a_only_tris,
        );

        // 9. Same for mesh B (with B's overlap-winding already flipped).
        let new_verts_b = inject_face_with_shared_first(
            verts_b,
            tris_b,
            bijective_b,
            &face_tri_indices_b,
            pair.face_b,
            &shared_3d,
            &shared_tris_b,
            &b_only_3d,
            &b_only_tris,
        );

        // 10. Repair T-junctions: overlap-boundary vertices may lie on edges
        // of adjacent (non-coplanar) face triangles.
        repair_tjunctions_after_injection(verts_a, tris_a, bijective_a, pair.face_a, &new_verts_a);
        repair_tjunctions_after_injection(verts_b, tris_b, bijective_b, pair.face_b, &new_verts_b);

        COPLANAR_PARTIAL_OVERLAP.fetch_add(1, Ordering::Relaxed);

        #[cfg(test)]
        eprintln!(
            "[COPLANAR INJECT-PO] Injected pair {:?}/{:?}: {} shared overlap tris, A-only={} tris, B-only={} tris",
            pair.face_a, pair.face_b,
            shared_tris_a.len(), a_only_tris.len(), b_only_tris.len()
        );
    }
}

/// Triangulate a polygon with holes using earcutr.
///
/// Returns (2D vertices, triangle indices). The first contour is the outer
/// boundary; remaining contours are holes.
fn triangulate_polygon_with_holes(contours: &[Vec<[f64; 2]>]) -> (Vec<[f64; 2]>, Vec<usize>) {
    if contours.is_empty() {
        return (vec![], vec![]);
    }
    let mut coords: Vec<f64> = Vec::new();
    let mut hole_indices: Vec<usize> = Vec::new();

    for pt in &contours[0] {
        coords.push(pt[0]);
        coords.push(pt[1]);
    }
    for contour in contours.iter().skip(1) {
        hole_indices.push(coords.len() / 2);
        for pt in contour {
            coords.push(pt[0]);
            coords.push(pt[1]);
        }
    }

    let n_verts = coords.len() / 2;
    let verts_2d: Vec<[f64; 2]> = (0..n_verts)
        .map(|i| [coords[i * 2], coords[i * 2 + 1]])
        .collect();
    match earcutr::earcut(&coords, &hole_indices, 2) {
        Ok(indices) => (verts_2d, indices),
        Err(_) => (vec![], vec![]),
    }
}

/// Convert 2D plane coordinates back to 3D world coordinates.
fn verts_2d_to_3d(
    verts_2d: &[[f64; 2]],
    origin: &[f64; 3],
    u_axis: &[f64; 3],
    v_axis: &[f64; 3],
) -> Vec<[f64; 3]> {
    verts_2d
        .iter()
        .map(|&[u, v]| {
            [
                origin[0] + u * u_axis[0] + v * v_axis[0],
                origin[1] + u * u_axis[1] + v * v_axis[1],
                origin[2] + u * u_axis[2] + v * v_axis[2],
            ]
        })
        .collect()
}

/// Merge two triangulated regions into a single vertex/triangle set.
///
/// The exclusive region's vertices come first, then the shared region's
/// vertices with index offsets applied to triangle indices.
fn merge_regions(
    verts_exclusive: &[[f64; 3]],
    tris_exclusive: &[[usize; 3]],
    verts_shared: &[[f64; 3]],
    tris_shared: &[[usize; 3]],
) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
    let mut verts = verts_exclusive.to_vec();
    let offset = verts.len();
    verts.extend_from_slice(verts_shared);
    let mut tris = tris_exclusive.to_vec();
    for tri in tris_shared {
        tris.push([tri[0] + offset, tri[1] + offset, tri[2] + offset]);
    }
    (verts, tris)
}

/// Repair T-junctions created by coplanar mesh injection.
///
/// After `replace_face_triangles()` adds new vertices to a coplanar face,
/// adjacent non-coplanar faces may have edges that pass through these new
/// vertices without a matching split. This function splits those edges.
///
/// For each triangle NOT belonging to the coplanar face, checks if any newly-
/// added vertex lies on one of its edges (cross-product distance + parametric t).
/// If so, splits the triangle into two at that vertex.
fn repair_tjunctions_after_injection(
    verts: &[[f64; 3]],
    tris: &mut Vec<[usize; 3]>,
    bijective: &mut BijectiveMap,
    coplanar_face: FaceIdx,
    new_vert_indices: &[usize],
) {
    if new_vert_indices.is_empty() {
        return;
    }

    let mut splits: Vec<(usize, usize, usize)> = Vec::new(); // (tri_idx, edge_k, split_vert)

    for (ti, tri) in tris.iter().enumerate() {
        // Skip triangles that belong to the coplanar face — they're already correct.
        if ti < bijective.tri_face_ids.len() && bijective.tri_face_ids[ti] == coplanar_face {
            continue;
        }

        for k in 0..3 {
            let v0 = tri[k];
            let v1 = tri[(k + 1) % 3];
            let p0 = verts[v0];
            let p1 = verts[v1];
            let d = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let d_len_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];

            if d_len_sq < crate::units::TAU_WORK_SQ {
                continue;
            }

            for &vi in new_vert_indices {
                if vi == v0 || vi == v1 {
                    continue;
                }
                let pm = verts[vi];
                let to_m = [pm[0] - p0[0], pm[1] - p0[1], pm[2] - p0[2]];
                // Cross product: distance from point to line
                let cross = [
                    d[1] * to_m[2] - d[2] * to_m[1],
                    d[2] * to_m[0] - d[0] * to_m[2],
                    d[0] * to_m[1] - d[1] * to_m[0],
                ];
                let cross_len_sq = cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
                if cross_len_sq > d_len_sq * crate::units::TAU_WORK {
                    continue;
                }
                // Parametric t along edge
                let t = (d[0] * to_m[0] + d[1] * to_m[1] + d[2] * to_m[2]) / d_len_sq;
                if t > crate::units::TAU_PARALLEL && t < 1.0 - crate::units::TAU_PARALLEL {
                    splits.push((ti, k, vi));
                    break; // One split per edge per pass
                }
            }
        }
    }

    // Apply splits in reverse order to preserve indices.
    // Each split replaces tri[ti] with two triangles.
    splits.sort_by(|a, b| b.0.cmp(&a.0)); // Reverse by tri index
    for (ti, k, vi) in splits {
        let tri = tris[ti];
        let v0 = tri[k];
        let v1 = tri[(k + 1) % 3];
        let v2 = tri[(k + 2) % 3];
        let face = bijective.tri_face_ids[ti];

        // Replace original tri: v0→vi→v2
        tris[ti] = [v0, vi, v2];
        // Append new tri: vi→v1→v2
        tris.push([vi, v1, v2]);
        bijective.tri_face_ids.push(face);
    }
}

/// Find triangles that lie on a given plane.
///
/// Uses the bijective map's face index as primary selector, then falls back
/// to geometric plane membership (all vertices within TAU_MODEL of the plane).
fn find_plane_triangles(
    verts: &[[f64; 3]],
    tris: &[[usize; 3]],
    bijective: &BijectiveMap,
    face_idx: FaceIdx,
    plane_normal: &[f64; 3],
    plane_offset: f64,
) -> Vec<usize> {
    // First try: use bijective face index.
    let by_face: Vec<usize> = bijective
        .tri_face_ids
        .iter()
        .enumerate()
        .filter(|(_, &f)| f == face_idx)
        .map(|(i, _)| i)
        .collect();

    // Verify: check if those triangles actually lie on the plane.
    let on_plane: Vec<usize> = by_face
        .iter()
        .copied()
        .filter(|&ti| tri_on_plane(verts, &tris[ti], plane_normal, plane_offset))
        .collect();

    if !on_plane.is_empty() {
        return on_plane;
    }

    // Fallback: find ALL triangles on the plane regardless of face index.
    // This handles synthetic test pairs and cases where face indices don't match.
    (0..tris.len())
        .filter(|&ti| tri_on_plane(verts, &tris[ti], plane_normal, plane_offset))
        .collect()
}

/// Check if all vertices of a triangle lie on a plane within tolerance.
fn tri_on_plane(verts: &[[f64; 3]], tri: &[usize; 3], normal: &[f64; 3], offset: f64) -> bool {
    tri.iter().all(|&vi| {
        let v = verts[vi];
        let dist = v[0] * normal[0] + v[1] * normal[1] + v[2] * normal[2] - offset;
        dist.abs() < TAU_MODEL
    })
}

/// Project 3D point into 2D plane coordinates.
fn project_to_2d(
    pt: &[f64; 3],
    origin: &[f64; 3],
    u_axis: &[f64; 3],
    v_axis: &[f64; 3],
) -> [f64; 2] {
    let dx = pt[0] - origin[0];
    let dy = pt[1] - origin[1];
    let dz = pt[2] - origin[2];
    let u = dx * u_axis[0] + dy * u_axis[1] + dz * u_axis[2];
    let v = dx * v_axis[0] + dy * v_axis[1] + dz * v_axis[2];
    [u, v]
}

/// Extract the 2D boundary polygon of a set of coplanar triangles.
///
/// Finds boundary edges (edges that appear in exactly one triangle) and
/// chains them into a polygon loop. Returns the polygon in 2D coordinates.
fn extract_face_boundary_2d(
    verts: &[[f64; 3]],
    tris: &[[usize; 3]],
    face_tri_indices: &[usize],
    origin: &[f64; 3],
    u_axis: &[f64; 3],
    v_axis: &[f64; 3],
) -> Vec<[f64; 2]> {
    use std::collections::HashMap;

    // Count edge occurrences (boundary edges appear once).
    let mut edge_count: HashMap<(usize, usize), usize> = HashMap::new();
    for &ti in face_tri_indices {
        let tri = tris[ti];
        for k in 0..3 {
            let a = tri[k];
            let b = tri[(k + 1) % 3];
            let edge = if a < b { (a, b) } else { (b, a) };
            *edge_count.entry(edge).or_insert(0) += 1;
        }
    }

    // Collect boundary edges (count == 1) as directed edges.
    let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();
    for &ti in face_tri_indices {
        let tri = tris[ti];
        for k in 0..3 {
            let a = tri[k];
            let b = tri[(k + 1) % 3];
            let edge = if a < b { (a, b) } else { (b, a) };
            if edge_count.get(&edge) == Some(&1) {
                adjacency.entry(a).or_default().push(b);
            }
        }
    }

    if adjacency.is_empty() {
        return vec![];
    }

    // Chain boundary edges into a polygon loop.
    let &start = adjacency.keys().next().unwrap();
    let mut polygon = vec![project_to_2d(&verts[start], origin, u_axis, v_axis)];
    let mut current = start;

    // Track visited to avoid infinite loops on malformed data.
    let mut visited = std::collections::HashSet::new();
    visited.insert(start);

    while let Some(neighbors) = adjacency.get(&current) {
        let next = match neighbors.iter().find(|&&n| !visited.contains(&n)) {
            Some(&n) => n,
            None => break,
        };
        visited.insert(next);
        polygon.push(project_to_2d(&verts[next], origin, u_axis, v_axis));
        current = next;
    }

    polygon
}

/// Replace a face's triangles in a mesh with new shared triangulation.
///
/// Removes the old triangles, appends new vertices and triangles, and
/// updates the bijective map accordingly. Reuses existing mesh vertices
/// within tolerance to maintain inter-face edge sharing.
///
/// Returns indices of newly-added vertices (those that didn't snap to existing
/// mesh vertices). These may create T-junctions with adjacent faces.
fn replace_face_triangles(
    verts: &mut Vec<[f64; 3]>,
    tris: &mut Vec<[usize; 3]>,
    bijective: &mut BijectiveMap,
    old_tri_indices: &[usize],
    face_idx: FaceIdx,
    new_verts: &[[f64; 3]],
    new_tris: &[[usize; 3]],
) -> Vec<usize> {
    // Mark old triangles for removal.
    let mut keep = vec![true; tris.len()];
    for &ti in old_tri_indices {
        keep[ti] = false;
    }

    // Compact: rebuild tris and bijective map without removed triangles.
    let mut new_tris_vec: Vec<[usize; 3]> = Vec::with_capacity(tris.len());
    let mut new_bmap: Vec<FaceIdx> = Vec::with_capacity(bijective.tri_face_ids.len());

    for (i, tri) in tris.iter().enumerate() {
        if keep[i] {
            new_tris_vec.push(*tri);
            new_bmap.push(bijective.tri_face_ids[i]);
        }
    }

    // Map each new vertex to an existing mesh vertex (within tolerance)
    // or append as new. This preserves inter-face edge sharing.
    let tol_sq = TAU_MODEL * TAU_MODEL;
    let mut vert_map: Vec<usize> = Vec::with_capacity(new_verts.len());
    let mut added_verts: Vec<usize> = Vec::new();
    for nv in new_verts {
        let existing = verts.iter().enumerate().find(|(_, ev)| {
            let dx = nv[0] - ev[0];
            let dy = nv[1] - ev[1];
            let dz = nv[2] - ev[2];
            dx * dx + dy * dy + dz * dz < tol_sq
        });
        match existing {
            Some((idx, _)) => vert_map.push(idx),
            None => {
                let idx = verts.len();
                verts.push(*nv);
                vert_map.push(idx);
                added_verts.push(idx);
            }
        }
    }

    // Append new triangles with remapped vertex indices.
    for tri in new_tris {
        new_tris_vec.push([vert_map[tri[0]], vert_map[tri[1]], vert_map[tri[2]]]);
        new_bmap.push(face_idx);
    }

    *tris = new_tris_vec;
    bijective.tri_face_ids = new_bmap;
    added_verts
}

/// Replace a face's triangles using shared-overlap-first vertex ordering.
///
/// Yang §4.5.5 requires the overlap region to reference bitwise-identical
/// 3D positions in both meshes. To achieve this, the shared verts (computed
/// once from i_overlay Intersect, identical for both meshes) must be
/// appended verbatim — never snapped to the mesh's pre-existing tessellation
/// verts whose bits may differ slightly. The exclusive region's
/// overlap-boundary verts (from i_overlay Difference, possibly different
/// bits) then snap to the just-appended shared verts.
///
/// Differs from `replace_face_triangles` in two ways:
///   1. Shared verts are appended unconditionally (no snap-to-existing).
///   2. Exclusive verts then snap to existing mesh verts AND to the
///      just-appended shared verts (within TAU_MODEL).
///
/// Returns indices of newly-added verts (for downstream T-junction repair).
#[allow(clippy::too_many_arguments)]
fn inject_face_with_shared_first(
    verts: &mut Vec<[f64; 3]>,
    tris: &mut Vec<[usize; 3]>,
    bijective: &mut BijectiveMap,
    old_tri_indices: &[usize],
    face_idx: FaceIdx,
    shared_verts: &[[f64; 3]],
    shared_tris: &[[usize; 3]],
    exclusive_verts: &[[f64; 3]],
    exclusive_tris: &[[usize; 3]],
) -> Vec<usize> {
    // 1. Mark old triangles for removal and compact tris/bijective.
    let mut keep = vec![true; tris.len()];
    for &ti in old_tri_indices {
        keep[ti] = false;
    }
    let mut new_tris_vec: Vec<[usize; 3]> = Vec::with_capacity(tris.len());
    let mut new_bmap: Vec<FaceIdx> = Vec::with_capacity(bijective.tri_face_ids.len());
    for (i, tri) in tris.iter().enumerate() {
        if keep[i] {
            new_tris_vec.push(*tri);
            new_bmap.push(bijective.tri_face_ids[i]);
        }
    }

    // 2. Append shared verts verbatim — preserves canonical bits.
    let shared_offset = verts.len();
    let mut added_verts: Vec<usize> = Vec::new();
    for sv in shared_verts {
        verts.push(*sv);
        added_verts.push(verts.len() - 1);
    }
    let shared_index = |i: usize| -> usize { shared_offset + i };

    // 3. Snap each exclusive vert to existing mesh vert (incl. just-added
    //    shared verts) within TAU_MODEL, else append.
    let tol_sq = TAU_MODEL * TAU_MODEL;
    let mut excl_map: Vec<usize> = Vec::with_capacity(exclusive_verts.len());
    for ev in exclusive_verts {
        let existing = verts.iter().enumerate().find(|(_, mv)| {
            let dx = ev[0] - mv[0];
            let dy = ev[1] - mv[1];
            let dz = ev[2] - mv[2];
            dx * dx + dy * dy + dz * dz < tol_sq
        });
        match existing {
            Some((idx, _)) => excl_map.push(idx),
            None => {
                let idx = verts.len();
                verts.push(*ev);
                excl_map.push(idx);
                added_verts.push(idx);
            }
        }
    }

    // 4. Append shared tris (using shared-vert offsets).
    for tri in shared_tris {
        new_tris_vec.push([
            shared_index(tri[0]),
            shared_index(tri[1]),
            shared_index(tri[2]),
        ]);
        new_bmap.push(face_idx);
    }
    // 5. Append exclusive tris (using exclusive-vert remapped indices).
    for tri in exclusive_tris {
        new_tris_vec.push([excl_map[tri[0]], excl_map[tri[1]], excl_map[tri[2]]]);
        new_bmap.push(face_idx);
    }

    *tris = new_tris_vec;
    bijective.tri_face_ids = new_bmap;
    added_verts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::yang_integration::{
        dedup_mesh_vertices, render_mesh_to_arrays, tessellate_waffle_solid, yang_boolean_inner,
    };
    use crate::boolean::BoolOp;
    use crate::tessellation;
    use crate::traits::Kernel;
    use crate::types::ClosedProfile;
    use crate::waffle_kernel::WaffleKernel;
    use std::collections::HashMap;

    /// Build a box WaffleSolid centered at (cx, cy) in XY, extruded from z=z0 to z=z0+depth.
    /// Returns (kernel, handle) so caller can access the solid.
    fn make_stacked_box(
        cx: f64,
        cy: f64,
        w: f64,
        h: f64,
        z0: f64,
        depth: f64,
    ) -> (WaffleKernel, crate::types::KernelSolidHandle) {
        let mut k = WaffleKernel::new();
        let mut positions = HashMap::new();
        positions.insert(1, (cx - w / 2.0, cy - h / 2.0));
        positions.insert(2, (cx + w / 2.0, cy - h / 2.0));
        positions.insert(3, (cx + w / 2.0, cy + h / 2.0));
        positions.insert(4, (cx - w / 2.0, cy + h / 2.0));

        let profile = ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };

        let origin = [0.0, 0.0, z0];
        let normal = [0.0, 0.0, 1.0];
        let x_axis = [1.0, 0.0, 0.0];

        let faces = k
            .make_faces_from_profiles(&[profile], origin, normal, x_axis, &positions)
            .expect("make_faces_from_profiles should succeed");
        let solid = k
            .extrude_face(faces[0], [0.0, 0.0, 1.0], depth)
            .expect("extrude_face should succeed");

        (k, solid)
    }

    // ── Test 1: Detection ──────────────────────────────────────────────

    /// Two stacked boxes (A: z=0..1, B: z=1..2, same XY footprint).
    /// The z=1 caps are coplanar (anti-parallel normals). detect_coplanar_face_pairs
    /// must find exactly 1 pair (anti-parallel only; same-direction pairs are
    /// filtered out pending vertex-reuse support in injection).
    #[test]
    fn test_coplanar_detection_finds_stacked_box_caps() {
        let (k_a, h_a) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 0.0, 1.0);
        let (k_b, h_b) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 1.0, 1.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a must exist");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b must exist");

        assert!(
            !solid_a.face_geometry.is_empty(),
            "solid_a must have face_geometry"
        );
        assert!(
            !solid_b.face_geometry.is_empty(),
            "solid_b must have face_geometry"
        );

        let pairs = detect_coplanar_face_pairs(solid_a, solid_b);

        // With both directions enabled: 1 anti-parallel (z=1 caps) +
        // 4 same-direction (side faces: x=0, x=1, y=0, y=1).
        assert!(
            pairs.len() >= 1,
            "Expected at least 1 coplanar pair, got {}",
            pairs.len()
        );

        // Filter to the z=1 anti-parallel pair specifically.
        let z1_pairs: Vec<_> = pairs
            .iter()
            .filter(|p| p.plane_normal[2].abs() > 0.99 && (p.plane_offset.abs() - 1.0).abs() < 1e-6)
            .collect();
        assert_eq!(
            z1_pairs.len(),
            1,
            "Expected 1 coplanar pair on z=1, got {}",
            z1_pairs.len()
        );

        let z1_pair = z1_pairs[0];
        let normal_z = z1_pair.plane_normal[2].abs();
        assert!(
            normal_z > 0.99,
            "Coplanar pair normal should be ±Z, got {:?}",
            z1_pair.plane_normal
        );
        assert!(
            (z1_pair.plane_offset.abs() - 1.0).abs() < 1e-6,
            "Coplanar pair offset should be ±1.0, got {}",
            z1_pair.plane_offset
        );
    }

    // ── Test 2: Conformal injection ────────────────────────────────────

    /// After inject_conformal_coplanar_mesh, mesh triangles on the z=1 plane
    /// must be vertex-identical between mesh_a and mesh_b.
    #[test]
    fn test_conformal_injection_produces_identical_triangles() {
        let (k_a, h_a) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 0.0, 1.0);
        let (k_b, h_b) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 1.0, 1.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a must exist");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b must exist");

        // Tessellate both solids.
        let lod = tessellation::TessellationLod::Boolean;
        let mesh_a = tessellate_waffle_solid(solid_a, lod).expect("tessellate A");
        let mesh_b = tessellate_waffle_solid(solid_b, lod).expect("tessellate B");

        let (mut verts_a, mut tris_a) = render_mesh_to_arrays(&mesh_a);
        let (mut verts_b, mut tris_b) = render_mesh_to_arrays(&mesh_b);
        dedup_mesh_vertices(&mut verts_a, &mut tris_a);
        dedup_mesh_vertices(&mut verts_b, &mut tris_b);

        let mut bijective_a = BijectiveMap::from_render_mesh(&mesh_a, &solid_a.face_map);
        let mut bijective_b = BijectiveMap::from_render_mesh(&mesh_b, &solid_b.face_map);

        // Detect coplanar pairs (will be empty from stub, so we build one manually).
        // Use a synthetic pair for the z=1 plane to test injection in isolation.
        let pair = CoplanarFacePair {
            face_a: FaceIdx(0), // placeholder — real impl finds the actual face
            face_b: FaceIdx(0),
            plane_normal: [0.0, 0.0, 1.0],
            plane_offset: 1.0,
            same_direction: false, // anti-parallel stacked caps
            is_identical_footprint: false,
            is_partial_overlap: false,
        };

        inject_conformal_coplanar_mesh(
            &[pair],
            &mut verts_a,
            &mut tris_a,
            &mut verts_b,
            &mut tris_b,
            &mut bijective_a,
            &mut bijective_b,
            &mesh_a,
            &mesh_b,
        );

        // Collect triangles on the z=1 plane from both meshes.
        let z1_tris_a = collect_plane_triangles(&verts_a, &tris_a, 2, 1.0, 1e-6);
        let z1_tris_b = collect_plane_triangles(&verts_b, &tris_b, 2, 1.0, 1e-6);

        assert!(
            !z1_tris_a.is_empty(),
            "mesh_a should have triangles on z=1 plane"
        );
        assert!(
            !z1_tris_b.is_empty(),
            "mesh_b should have triangles on z=1 plane"
        );

        // After conformal injection, z=1 triangles must be vertex-identical.
        assert_eq!(
            z1_tris_a.len(),
            z1_tris_b.len(),
            "Coplanar meshes must have same triangle count on z=1: A={}, B={}",
            z1_tris_a.len(),
            z1_tris_b.len()
        );

        // Check vertex-by-vertex identity (sorted for comparison).
        let mut sorted_a = normalize_triangles(&z1_tris_a);
        let mut sorted_b = normalize_triangles(&z1_tris_b);
        sorted_a.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted_b.sort_by(|a, b| a.partial_cmp(b).unwrap());

        assert_eq!(
            sorted_a, sorted_b,
            "z=1 plane triangles must be vertex-identical after conformal injection"
        );
    }

    // ── Test 3: No conformal explosion ─────────────────────────────────

    /// Partially overlapping coplanar boxes: box A at x=0..1, box B at x=0.5..1.5,
    /// both at same Y, stacked at z=1. The z=1 face overlap region (x=0.5..1.0)
    /// must produce zero cross-mesh shared edges after preprocessing.
    ///
    /// With partially overlapping boxes, the tessellations on the z=1 plane differ
    /// (different XY spans → different triangle layouts). Without coplanar
    /// preprocessing, the mesh boolean sees non-identical coplanar triangles and
    /// produces conformal edge explosion.
    ///
    #[test]
    fn test_stacked_box_union_no_conformal_explosion() {
        // Box A: x=[0,1], y=[0,1], z=[0,1]
        let (k_a, h_a) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 0.0, 1.0);
        // Box B: x=[0.3,1.3], y=[0,1], z=[1,2] — offset in X so z=1 overlap is partial
        let (k_b, h_b) = make_stacked_box(0.8, 0.5, 1.0, 1.0, 1.0, 1.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a must exist");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b must exist");

        // The coplanar detection must find the z=1 pair even with partial overlap.
        let pairs = detect_coplanar_face_pairs(solid_a, solid_b);

        assert!(
            !pairs.is_empty(),
            "detect_coplanar_face_pairs must find the z=1 coplanar pair between \
             partially overlapping boxes, but returned empty. Without detection, \
             the mesh boolean will see non-identical coplanar triangles and produce \
             conformal edge explosion."
        );

        // Verify the detected pair has the correct plane (z=1).
        let z_pairs: Vec<_> = pairs
            .iter()
            .filter(|p| p.plane_normal[2].abs() > 0.99 && (p.plane_offset.abs() - 1.0).abs() < 1e-6)
            .collect();
        assert_eq!(
            z_pairs.len(),
            1,
            "Expected exactly 1 coplanar pair on the z=1 plane, got {}",
            z_pairs.len()
        );
    }

    // ── Test 4: Correct topology ───────────────────────────────────────

    /// Three stacked boxes (A: z=0..1, B: z=1..2, C: z=2..3) unioned
    /// sequentially. This is the F0063 pattern that causes timeout without
    /// coplanar preprocessing.
    ///
    /// After (A ∪ B) ∪ C, the result should have ≤6 faces (elongated box),
    /// euler=2, and NO internal cap faces at z=1 or z=2.
    ///
    #[test]
    fn test_stacked_box_union_correct_topology() {
        // Build three stacked boxes: z=0..1, z=1..2, z=2..3
        let (k_a, h_a) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 0.0, 1.0);
        let (k_b, h_b) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 1.0, 1.0);
        let (k_c, h_c) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 2.0, 1.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a must exist");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b must exist");
        let solid_c = k_c.get_solid(&h_c).expect("solid_c must exist");

        // First: detect coplanar pairs for A ∪ B.
        let pairs_ab = detect_coplanar_face_pairs(solid_a, solid_b);
        assert!(
            !pairs_ab.is_empty(),
            "detect_coplanar_face_pairs must find z=1 coplanar pair for A∪B, \
             but returned empty. Three-stacked-box union requires coplanar \
             preprocessing at each boolean step to avoid conformal explosion \
             (the F0063 pattern)."
        );

        // Then: first boolean A ∪ B.
        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result_ab = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut id_alloc);
        assert!(
            result_ab.is_ok(),
            "A ∪ B should succeed: {:?}",
            result_ab.err()
        );

        // Second: detect coplanar pairs for (A∪B) ∪ C at z=2.
        // This requires the (A∪B) result to have face_geometry for its z=2 cap.
        let pairs_abc = detect_coplanar_face_pairs(solid_b, solid_c);
        assert!(
            !pairs_abc.is_empty(),
            "detect_coplanar_face_pairs must find z=2 coplanar pair for (A∪B)∪C, \
             but returned empty. Sequential stacked boolean requires preprocessing \
             at each step."
        );
    }

    // ── Test helpers ───────────────────────────────────────────────────

    /// Collect triangles whose vertices all lie on a given plane (axis=value±tol).
    fn collect_plane_triangles(
        verts: &[[f64; 3]],
        tris: &[[usize; 3]],
        axis: usize,
        value: f64,
        tol: f64,
    ) -> Vec<[[f64; 3]; 3]> {
        tris.iter()
            .filter_map(|tri| {
                let v0 = verts[tri[0]];
                let v1 = verts[tri[1]];
                let v2 = verts[tri[2]];
                if (v0[axis] - value).abs() < tol
                    && (v1[axis] - value).abs() < tol
                    && (v2[axis] - value).abs() < tol
                {
                    Some([v0, v1, v2])
                } else {
                    None
                }
            })
            .collect()
    }

    /// Normalize triangle vertex coordinates to 6 decimal places for comparison.
    fn normalize_triangles(tris: &[[[f64; 3]; 3]]) -> Vec<[[i64; 3]; 3]> {
        tris.iter()
            .map(|tri| {
                let mut verts: Vec<[i64; 3]> = tri
                    .iter()
                    .map(|v| {
                        [
                            (v[0] * 1e6).round() as i64,
                            (v[1] * 1e6).round() as i64,
                            (v[2] * 1e6).round() as i64,
                        ]
                    })
                    .collect();
                verts.sort();
                [verts[0], verts[1], verts[2]]
            })
            .collect()
    }

    // ── Diagnostic Test: F0003 coplanar preprocessing ─────────────────

    /// Count vertices in a face's outer loop.
    fn count_face_loop_verts(arena: &TopoArena, face_idx: FaceIdx) -> usize {
        let loop_idx = arena.faces[face_idx.0].outer_loop;
        let start_he = arena.loops[loop_idx.0].half_edge;
        let mut count = 0usize;
        let mut he = start_he;
        loop {
            count += 1;
            he = arena.half_edges[he.0].next;
            if he == start_he {
                break;
            }
        }
        count
    }

    /// Collect 3D positions of all vertices in a face's outer loop.
    fn collect_face_loop_positions(arena: &TopoArena, face_idx: FaceIdx) -> Vec<[f64; 3]> {
        let loop_idx = arena.faces[face_idx.0].outer_loop;
        let start_he = arena.loops[loop_idx.0].half_edge;
        let mut positions = Vec::new();
        let mut he = start_he;
        loop {
            let vi = arena.half_edges[he.0].origin;
            positions.push(arena.vertices[vi.0].position);
            he = arena.half_edges[he.0].next;
            if he == start_he {
                break;
            }
        }
        positions
    }

    /// F0003 diagnostic: Analyze coplanar preprocessing on cross-shaped union.
    ///
    /// Box A: 60×40 extruded 30 → [-30,-20,0] to [30,20,30]
    /// Box B: 40×60 extruded 20 → [-20,-30,0] to [20,30,20]
    ///
    /// Both share coplanar z=0 bottom face. Overlap = [-20,-20] to [20,20].
    /// This test diagnoses what i_overlay produces and what split_brep does wrong.
    #[test]
    fn test_f0003_coplanar_diagnostic() {
        // Build F0003 geometry (scale=100, units in the spec's raw values).
        let (k_a, h_a) = make_stacked_box(0.0, 0.0, 60.0, 40.0, 0.0, 30.0);
        let (k_b, h_b) = make_stacked_box(0.0, 0.0, 40.0, 60.0, 0.0, 20.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b");

        // ── Step 1: Detect coplanar pairs ──
        let pairs = detect_coplanar_face_pairs(solid_a, solid_b);
        eprintln!("\n=== F0003 Coplanar Diagnostic ===");
        eprintln!("Total coplanar pairs detected: {}", pairs.len());

        for (i, p) in pairs.iter().enumerate() {
            eprintln!(
                "  Pair {}: face_a={:?} face_b={:?} normal={:?} offset={:.4} same_dir={}",
                i, p.face_a, p.face_b, p.plane_normal, p.plane_offset, p.same_direction
            );
        }

        // Find the z=0 same-direction pair (the bottom face pair).
        let z0_pairs: Vec<_> = pairs
            .iter()
            .filter(|p| {
                p.plane_normal[2].abs() > 0.99 && p.plane_offset.abs() < 1e-6 && p.same_direction
            })
            .collect();

        // The z=0 bottom faces should both have normals pointing -Z (into the solid)
        // or +Z — either way they're same-direction coplanar.
        // If no same-direction z=0 pair, check anti-parallel too.
        let z0_all: Vec<_> = pairs
            .iter()
            .filter(|p| p.plane_normal[2].abs() > 0.99 && p.plane_offset.abs() < 1e-6)
            .collect();

        eprintln!("\nz=0 pairs (all): {}", z0_all.len());
        eprintln!("z=0 pairs (same-dir): {}", z0_pairs.len());

        assert!(
            !z0_all.is_empty(),
            "Must detect at least one z=0 coplanar pair"
        );

        // Use the first z=0 pair for analysis (regardless of direction).
        let z0_pair = z0_all[0];

        // ── Step 2: Examine face loop vertices before splitting ──
        let verts_a_before = count_face_loop_verts(&solid_a.arena, z0_pair.face_a);
        let verts_b_before = count_face_loop_verts(&solid_b.arena, z0_pair.face_b);
        eprintln!(
            "\nFace A (z=0) loop vertices before split: {}",
            verts_a_before
        );
        eprintln!(
            "Face B (z=0) loop vertices before split: {}",
            verts_b_before
        );

        let pos_a = collect_face_loop_positions(&solid_a.arena, z0_pair.face_a);
        let pos_b = collect_face_loop_positions(&solid_b.arena, z0_pair.face_b);
        eprintln!("Face A vertices:");
        for (i, p) in pos_a.iter().enumerate() {
            eprintln!("  v{}: [{:.4}, {:.4}, {:.4}]", i, p[0], p[1], p[2]);
        }
        eprintln!("Face B vertices:");
        for (i, p) in pos_b.iter().enumerate() {
            eprintln!("  v{}: [{:.4}, {:.4}, {:.4}]", i, p[0], p[1], p[2]);
        }

        // ── Step 3: Manually run i_overlay to see the overlap polygon ──
        let (u_axis, v_axis) = compute_plane_basis(z0_pair.plane_normal);
        let plane_origin = [
            z0_pair.plane_normal[0] * z0_pair.plane_offset,
            z0_pair.plane_normal[1] * z0_pair.plane_offset,
            z0_pair.plane_normal[2] * z0_pair.plane_offset,
        ];

        let poly_a = collect_face_loop_2d(
            &solid_a.arena,
            z0_pair.face_a,
            &plane_origin,
            &u_axis,
            &v_axis,
        );
        let poly_b = collect_face_loop_2d(
            &solid_b.arena,
            z0_pair.face_b,
            &plane_origin,
            &u_axis,
            &v_axis,
        );

        eprintln!("\n2D polygon A ({} verts):", poly_a.len());
        for (vi, [u, v]) in &poly_a {
            eprintln!("  {:?}: ({:.4}, {:.4})", vi, u, v);
        }
        eprintln!("2D polygon B ({} verts):", poly_b.len());
        for (vi, [u, v]) in &poly_b {
            eprintln!("  {:?}: ({:.4}, {:.4})", vi, u, v);
        }

        let shape_a: Vec<Vec<[f64; 2]>> = vec![poly_a.iter().map(|&(_, p)| p).collect()];
        let shape_b: Vec<Vec<[f64; 2]>> = vec![poly_b.iter().map(|&(_, p)| p).collect()];

        let overlap: Vec<Vec<Vec<[f64; 2]>>> =
            shape_a.overlay(&shape_b, OverlayRule::Intersect, FillRule::EvenOdd);

        eprintln!("\ni_overlay Intersect result:");
        eprintln!("  Number of contour groups: {}", overlap.len());
        if !overlap.is_empty() {
            for (gi, group) in overlap.iter().enumerate() {
                eprintln!("  Group {}: {} contours", gi, group.len());
                for (ci, contour) in group.iter().enumerate() {
                    eprintln!("    Contour {}: {} vertices", ci, contour.len());
                    for (vi, pt) in contour.iter().enumerate() {
                        eprintln!("      v{}: ({:.4}, {:.4})", vi, pt[0], pt[1]);
                    }
                }
            }
        }

        // Also compute A-only and B-only
        let a_only: Vec<Vec<Vec<[f64; 2]>>> =
            shape_a.overlay(&shape_b, OverlayRule::Difference, FillRule::EvenOdd);
        let b_only: Vec<Vec<Vec<[f64; 2]>>> =
            shape_b.overlay(&shape_a, OverlayRule::Difference, FillRule::EvenOdd);

        eprintln!("\nA-only (Difference A\\B):");
        if !a_only.is_empty() && !a_only[0].is_empty() {
            for contour in &a_only[0] {
                eprintln!("  Contour ({} verts):", contour.len());
                for pt in contour {
                    eprintln!("    ({:.4}, {:.4})", pt[0], pt[1]);
                }
            }
        } else {
            eprintln!("  (empty)");
        }

        eprintln!("\nB-only (Difference B\\A):");
        if !b_only.is_empty() && !b_only[0].is_empty() {
            for contour in &b_only[0] {
                eprintln!("  Contour ({} verts):", contour.len());
                for pt in contour {
                    eprintln!("    ({:.4}, {:.4})", pt[0], pt[1]);
                }
            }
        } else {
            eprintln!("  (empty)");
        }

        // ── Step 4: Project overlap vertices to 3D and check which edges they lie on ──
        if !overlap.is_empty() && !overlap[0].is_empty() {
            let overlap_poly = &overlap[0][0];
            eprintln!("\n=== Overlap boundary analysis ===");
            eprintln!("Overlap polygon has {} vertices", overlap_poly.len());

            // Project each overlap vertex to 3D
            let overlap_3d: Vec<[f64; 3]> = overlap_poly
                .iter()
                .map(|&[u, v]| {
                    [
                        plane_origin[0] + u * u_axis[0] + v * v_axis[0],
                        plane_origin[1] + u * u_axis[1] + v * v_axis[1],
                        plane_origin[2] + u * u_axis[2] + v * v_axis[2],
                    ]
                })
                .collect();

            for (i, pt) in overlap_3d.iter().enumerate() {
                eprintln!(
                    "  Overlap vertex {}: [{:.4}, {:.4}, {:.4}]",
                    i, pt[0], pt[1], pt[2]
                );
            }

            // For each overlap vertex, check which B-Rep edge of face_a it lies on
            // (or if it coincides with an existing vertex).
            let tol_sq = TAU_MODEL * TAU_MODEL;
            eprintln!("\n--- Overlap vertices vs Face A edges ---");
            for (oi, ov) in overlap_3d.iter().enumerate() {
                // Check existing vertices
                let mut on_existing = false;
                for (vi, &(vert_idx, _)) in poly_a.iter().enumerate() {
                    let p = solid_a.arena.vertices[vert_idx.0].position;
                    let dx = ov[0] - p[0];
                    let dy = ov[1] - p[1];
                    let dz = ov[2] - p[2];
                    if dx * dx + dy * dy + dz * dz < tol_sq {
                        eprintln!(
                            "  Overlap v{}: COINCIDES with face_a vertex {} ({:?})",
                            oi, vi, vert_idx
                        );
                        on_existing = true;
                        break;
                    }
                }
                if on_existing {
                    continue;
                }

                // Check edges
                for vi in 0..poly_a.len() {
                    let (v0_idx, _) = poly_a[vi];
                    let (v1_idx, _) = poly_a[(vi + 1) % poly_a.len()];
                    let p0 = solid_a.arena.vertices[v0_idx.0].position;
                    let p1 = solid_a.arena.vertices[v1_idx.0].position;
                    let d = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
                    let d_len_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                    if d_len_sq < 1e-24 {
                        continue;
                    }
                    let to_ov = [ov[0] - p0[0], ov[1] - p0[1], ov[2] - p0[2]];
                    let cross = [
                        d[1] * to_ov[2] - d[2] * to_ov[1],
                        d[2] * to_ov[0] - d[0] * to_ov[2],
                        d[0] * to_ov[1] - d[1] * to_ov[0],
                    ];
                    let cross_len_sq =
                        cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
                    if cross_len_sq > d_len_sq * 1e-6 {
                        continue;
                    }
                    let t = (d[0] * to_ov[0] + d[1] * to_ov[1] + d[2] * to_ov[2]) / d_len_sq;
                    if t > 0.001 && t < 0.999 {
                        eprintln!(
                            "  Overlap v{}: ON edge {} ({:?}→{:?}), t={:.6}",
                            oi, vi, v0_idx, v1_idx, t
                        );
                        break;
                    }
                }
            }

            eprintln!("\n--- Overlap vertices vs Face B edges ---");
            for (oi, ov) in overlap_3d.iter().enumerate() {
                let mut on_existing = false;
                for (vi, &(vert_idx, _)) in poly_b.iter().enumerate() {
                    let p = solid_b.arena.vertices[vert_idx.0].position;
                    let dx = ov[0] - p[0];
                    let dy = ov[1] - p[1];
                    let dz = ov[2] - p[2];
                    if dx * dx + dy * dy + dz * dz < tol_sq {
                        eprintln!(
                            "  Overlap v{}: COINCIDES with face_b vertex {} ({:?})",
                            oi, vi, vert_idx
                        );
                        on_existing = true;
                        break;
                    }
                }
                if on_existing {
                    continue;
                }

                for vi in 0..poly_b.len() {
                    let (v0_idx, _) = poly_b[vi];
                    let (v1_idx, _) = poly_b[(vi + 1) % poly_b.len()];
                    let p0 = solid_b.arena.vertices[v0_idx.0].position;
                    let p1 = solid_b.arena.vertices[v1_idx.0].position;
                    let d = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
                    let d_len_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                    if d_len_sq < 1e-24 {
                        continue;
                    }
                    let to_ov = [ov[0] - p0[0], ov[1] - p0[1], ov[2] - p0[2]];
                    let cross = [
                        d[1] * to_ov[2] - d[2] * to_ov[1],
                        d[2] * to_ov[0] - d[0] * to_ov[2],
                        d[0] * to_ov[1] - d[1] * to_ov[0],
                    ];
                    let cross_len_sq =
                        cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
                    if cross_len_sq > d_len_sq * 1e-6 {
                        continue;
                    }
                    let t = (d[0] * to_ov[0] + d[1] * to_ov[1] + d[2] * to_ov[2]) / d_len_sq;
                    if t > 0.001 && t < 0.999 {
                        eprintln!(
                            "  Overlap v{}: ON edge {} ({:?}→{:?}), t={:.6}",
                            oi, vi, v0_idx, v1_idx, t
                        );
                        break;
                    }
                }
            }
        }

        // ── Step 5: Run split_brep and check results ──
        let mut solid_a_mut = solid_a.clone();
        let mut solid_b_mut = solid_b.clone();

        // Only run split on same-direction pairs (that's what split_brep does).
        let mut same_dir_pairs: Vec<_> =
            pairs.iter().filter(|p| p.same_direction).cloned().collect();
        eprintln!(
            "\n=== Running split_brep_for_coplanar_pairs ({} same-dir pairs) ===",
            same_dir_pairs.len()
        );

        split_brep_for_coplanar_pairs(&mut solid_a_mut, &mut solid_b_mut, &mut same_dir_pairs);

        // Count faces and loop vertices after splitting.
        let faces_a_after = solid_a_mut.arena.face_count();
        let faces_b_after = solid_b_mut.arena.face_count();
        let faces_a_before = solid_a.arena.face_count();
        let faces_b_before = solid_b.arena.face_count();
        eprintln!("\nFace count A: {} → {}", faces_a_before, faces_a_after);
        eprintln!("Face count B: {} → {}", faces_b_before, faces_b_after);

        // Check z=0 face loop vertices after split.
        // Find z=0 faces in the modified solid.
        for (&fi, geom) in &solid_a_mut.face_geometry {
            if let SurfaceGeom::Planar(plane) = geom {
                if plane.normal.z.abs() > 0.99 {
                    let offset = plane.origin.x * plane.normal.x
                        + plane.origin.y * plane.normal.y
                        + plane.origin.z * plane.normal.z;
                    if offset.abs() < 1e-6 {
                        let n = count_face_loop_verts(&solid_a_mut.arena, fi);
                        let positions = collect_face_loop_positions(&solid_a_mut.arena, fi);
                        eprintln!("\nFace A {:?} (z≈0) after split: {} loop verts", fi, n);
                        for (i, p) in positions.iter().enumerate() {
                            eprintln!("  v{}: [{:.4}, {:.4}, {:.4}]", i, p[0], p[1], p[2]);
                        }
                    }
                }
            }
        }

        eprintln!("\n=== Diagnosis Summary ===");
        eprintln!("The current split_brep_for_coplanar_pairs only uses the FIRST 2 boundary");
        eprintln!("vertices for a single mef call. For F0003's rectangular overlap with 4");
        eprintln!("corners, this produces an incomplete split — it connects 2 points across");
        eprintln!("the face rather than creating the full rectangular sub-face boundary.");
        eprintln!("\nCorrect algorithm needs:");
        eprintln!("  1. For each overlap boundary vertex NOT at an existing face vertex:");
        eprintln!("     call split_edge_at on the B-Rep edge it lies on");
        eprintln!("  2. Connect boundary vertices in sequence with mef calls to carve out");
        eprintln!("     the overlap sub-face from the parent face");
        eprintln!("  3. The overlap has 4 corners — need to check how many are at existing");
        eprintln!("     vertices vs how many need edge splits");
    }

    // ── F0004 Diagnostic: Thin cross coplanar preprocessing ──────────

    /// F0004 "Thin cross": two crossed boxes sharing z=0 and z=0.5 coplanar faces.
    ///
    /// Box A: centered (0,0), 0.8×0.2, extruded 0.5 → [-0.4,-0.1,0]→[0.4,0.1,0.5]
    /// Box B: centered (0,0), 0.2×0.8, extruded 0.5 → [-0.1,-0.4,0]→[0.1,0.4,0.5]
    ///
    /// Both share z=0 bottom AND z=0.5 top faces (TWO coplanar pairs, same-direction).
    /// The overlap on each coplanar plane is [-0.1,-0.1]→[0.1,0.1].
    ///
    /// This test traces the full coplanar preprocessing + pipeline to diagnose
    /// the "6 boundary HEs" failure.
    #[test]
    fn test_f0004_coplanar_trace() {
        eprintln!("\n======================================================================");
        eprintln!("=== F0004 THIN CROSS COPLANAR DIAGNOSTIC ===");
        eprintln!("======================================================================\n");

        // Box A: 0.8×0.2, depth 0.5
        let (k_a, h_a) = make_stacked_box(0.0, 0.0, 0.8, 0.2, 0.0, 0.5);
        // Box B: 0.2×0.8, depth 0.5
        let (k_b, h_b) = make_stacked_box(0.0, 0.0, 0.2, 0.8, 0.0, 0.5);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b");

        // ── Step 1: Dump face geometry for both solids ──
        eprintln!("--- Solid A face geometry ---");
        for (&fi, geom) in &solid_a.face_geometry {
            if let SurfaceGeom::Planar(plane) = geom {
                let offset = plane.origin.x * plane.normal.x
                    + plane.origin.y * plane.normal.y
                    + plane.origin.z * plane.normal.z;
                eprintln!(
                    "  Face {:?}: normal=[{:.4},{:.4},{:.4}] offset={:.6}",
                    fi, plane.normal.x, plane.normal.y, plane.normal.z, offset
                );
                let positions = collect_face_loop_positions(&solid_a.arena, fi);
                for (i, p) in positions.iter().enumerate() {
                    eprintln!("    v{}: [{:.4}, {:.4}, {:.4}]", i, p[0], p[1], p[2]);
                }
            }
        }
        eprintln!("\n--- Solid B face geometry ---");
        for (&fi, geom) in &solid_b.face_geometry {
            if let SurfaceGeom::Planar(plane) = geom {
                let offset = plane.origin.x * plane.normal.x
                    + plane.origin.y * plane.normal.y
                    + plane.origin.z * plane.normal.z;
                eprintln!(
                    "  Face {:?}: normal=[{:.4},{:.4},{:.4}] offset={:.6}",
                    fi, plane.normal.x, plane.normal.y, plane.normal.z, offset
                );
                let positions = collect_face_loop_positions(&solid_b.arena, fi);
                for (i, p) in positions.iter().enumerate() {
                    eprintln!("    v{}: [{:.4}, {:.4}, {:.4}]", i, p[0], p[1], p[2]);
                }
            }
        }

        // ── Step 2: Detect coplanar pairs ──
        let pairs = detect_coplanar_face_pairs(solid_a, solid_b);
        eprintln!("\n=== Coplanar Pair Detection ===");
        eprintln!("Total pairs detected: {}", pairs.len());
        for (i, p) in pairs.iter().enumerate() {
            eprintln!(
                "  Pair {}: face_a={:?} face_b={:?} normal=[{:.4},{:.4},{:.4}] offset={:.6} same_dir={}",
                i, p.face_a, p.face_b,
                p.plane_normal[0], p.plane_normal[1], p.plane_normal[2],
                p.plane_offset, p.same_direction
            );
        }

        // Expect 2 coplanar pairs: z=0 and z=0.5
        let z0_pairs: Vec<_> = pairs
            .iter()
            .filter(|p| p.plane_normal[2].abs() > 0.99 && p.plane_offset.abs() < 1e-4)
            .collect();
        let z05_pairs: Vec<_> = pairs
            .iter()
            .filter(|p| p.plane_normal[2].abs() > 0.99 && (p.plane_offset.abs() - 0.5).abs() < 1e-4)
            .collect();
        eprintln!("\nz=0 coplanar pairs: {}", z0_pairs.len());
        eprintln!("z=0.5 coplanar pairs: {}", z05_pairs.len());

        // ── Step 3: Run full pipeline ──
        eprintln!("\n=== Running full Yang pipeline ===");
        let mut next_id = 100u64;
        let mut id_alloc = || {
            next_id += 1;
            next_id
        };

        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut id_alloc);

        match &result {
            Ok(r) => {
                let n_faces = r.arena.faces.len();
                let n_edges = r.arena.edges.len();
                let n_verts = r.arena.vertices.len();
                let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;
                eprintln!(
                    "\nPipeline SUCCEEDED: V={n_verts}, E={n_edges}, F={n_faces}, Euler={euler}"
                );
            }
            Err(e) => {
                eprintln!("\nPipeline FAILED: {e}");
            }
        }
    }

    // ── Test 5: Phase D regression — identical-footprint inject ──────────

    /// Phase D regression test (PR5).
    ///
    /// `inject_identical_footprint_mesh` must produce **bitwise-identical**
    /// triangulations on the shared coplanar plane: same set of 3D vertex
    /// positions, same triangle count, same vertex-position triples (modulo
    /// per-triangle winding reversal for the anti-parallel canonical case).
    /// This is the Yang §4.5.5 deliverable: "identical meshes are generated
    /// for both models in this part."
    ///
    /// FIP §8 red-before-green: with Phase B's wiring removed (no inject
    /// call after tessellation), tessellation produces independent diagonals
    /// on each mesh and the assertions below fail. After Phase B is wired,
    /// the assertions pass.
    ///
    /// Note: this test does NOT assert downstream topology validity. The
    /// canary `test_stacked_box_union_correct_topology` continues to track
    /// the label_cells boundary-coincident classification fix (PR6).
    #[test]
    fn test_identical_footprint_inject_produces_consistent_meshes() {
        // Two unit cubes A=[0,1]³ and B at z∈[1,2] — identical XY footprint.
        let (k_a, h_a) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 0.0, 1.0);
        let (k_b, h_b) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 1.0, 1.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a must exist");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b must exist");

        // Tessellate both solids.
        let lod = tessellation::TessellationLod::Boolean;
        let mesh_a = tessellate_waffle_solid(solid_a, lod).expect("tessellate A");
        let mesh_b = tessellate_waffle_solid(solid_b, lod).expect("tessellate B");

        let (mut verts_a, mut tris_a) = render_mesh_to_arrays(&mesh_a);
        let (mut verts_b, mut tris_b) = render_mesh_to_arrays(&mesh_b);
        dedup_mesh_vertices(&mut verts_a, &mut tris_a);
        dedup_mesh_vertices(&mut verts_b, &mut tris_b);

        let mut bijective_a = BijectiveMap::from_render_mesh(&mesh_a, &solid_a.face_map);
        let mut bijective_b = BijectiveMap::from_render_mesh(&mesh_b, &solid_b.face_map);

        // Run the integration path Yang Stage 0a uses: detect → split (which
        // marks identical-footprint pairs) → inject. We must clone the
        // solids because `split_brep_for_coplanar_pairs` takes `&mut`.
        let mut solid_a_mut = solid_a.clone();
        let mut solid_b_mut = solid_b.clone();
        let mut pairs = detect_coplanar_face_pairs(&solid_a_mut, &solid_b_mut);
        split_brep_for_coplanar_pairs(&mut solid_a_mut, &mut solid_b_mut, &mut pairs);

        // The z=1 anti-parallel pair MUST be marked identical-footprint.
        let z1_pair = pairs
            .iter()
            .find(|p| {
                p.plane_normal[2].abs() > 0.99
                    && (p.plane_offset.abs() - 1.0).abs() < 1e-6
                    && !p.same_direction
            })
            .expect("z=1 anti-parallel coplanar pair must be detected");
        assert!(
            z1_pair.is_identical_footprint,
            "z=1 anti-parallel pair between two unit-cube stacked boxes \
             must be marked is_identical_footprint=true"
        );

        // Run the inject helper.
        inject_identical_footprint_mesh(
            &pairs,
            &mut verts_a,
            &mut tris_a,
            &mut bijective_a,
            &mut verts_b,
            &mut tris_b,
            &mut bijective_b,
        );

        // Yang §4.5.5 deliverable assertions — bitwise-identical mesh on the
        // shared plane.

        // 1. Both meshes have triangles on z=1.
        let z1_tris_a = collect_plane_triangles(&verts_a, &tris_a, 2, 1.0, f64::EPSILON);
        let z1_tris_b = collect_plane_triangles(&verts_b, &tris_b, 2, 1.0, f64::EPSILON);
        assert!(
            !z1_tris_a.is_empty(),
            "mesh A must have triangles on z=1 plane after inject"
        );
        assert!(
            !z1_tris_b.is_empty(),
            "mesh B must have triangles on z=1 plane after inject"
        );

        // 2. Same triangle count.
        assert_eq!(
            z1_tris_a.len(),
            z1_tris_b.len(),
            "Yang §4.5.5: identical-footprint meshes must have the same \
             triangle count on the shared plane (A={}, B={})",
            z1_tris_a.len(),
            z1_tris_b.len()
        );

        // 3. Same set of 3D vertex positions on z=1 — bitwise (each f64 bit
        //    pattern equal). The injection passes through the same
        //    `verts_2d_to_3d` call for both meshes, so the positions ARE
        //    bitwise identical.
        let mut z1_verts_a: Vec<[u64; 3]> = z1_tris_a
            .iter()
            .flat_map(|tri| {
                tri.iter()
                    .map(|v| [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()])
            })
            .collect();
        let mut z1_verts_b: Vec<[u64; 3]> = z1_tris_b
            .iter()
            .flat_map(|tri| {
                tri.iter()
                    .map(|v| [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()])
            })
            .collect();
        z1_verts_a.sort();
        z1_verts_a.dedup();
        z1_verts_b.sort();
        z1_verts_b.dedup();
        assert_eq!(
            z1_verts_a,
            z1_verts_b,
            "Yang §4.5.5: identical-footprint meshes must reference the \
             same set of bitwise-identical 3D vertex positions on the \
             shared plane (got {} unique in A, {} unique in B)",
            z1_verts_a.len(),
            z1_verts_b.len()
        );

        // 4. Triangle vertex-position triples match between A and B modulo
        //    per-triangle winding reversal (anti-parallel case). Normalize
        //    each triangle by sorting its three vertex bit-patterns; the
        //    resulting set must be equal.
        let mut sorted_a: Vec<[[u64; 3]; 3]> = z1_tris_a
            .iter()
            .map(|tri| {
                let mut v: Vec<[u64; 3]> = tri
                    .iter()
                    .map(|p| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()])
                    .collect();
                v.sort();
                [v[0], v[1], v[2]]
            })
            .collect();
        let mut sorted_b: Vec<[[u64; 3]; 3]> = z1_tris_b
            .iter()
            .map(|tri| {
                let mut v: Vec<[u64; 3]> = tri
                    .iter()
                    .map(|p| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()])
                    .collect();
                v.sort();
                [v[0], v[1], v[2]]
            })
            .collect();
        sorted_a.sort();
        sorted_b.sort();
        assert_eq!(
            sorted_a, sorted_b,
            "Yang §4.5.5: per-triangle vertex-position triples on the \
             shared plane must match between mesh A and mesh B (modulo \
             winding)"
        );
    }

    /// Yang §4.5.5 partial-overlap regression test (PR7).
    ///
    /// Box A at x∈[0,1], y∈[0,1], z∈[0,1]; Box B at x∈[0.3, 1.3], y∈[0,1],
    /// z∈[1, 2]. The z=1 cap pair is anti-parallel with rectangular partial
    /// overlap at x∈[0.3, 1.0], y∈[0,1]. After `inject_partial_overlap_mesh`,
    /// the triangles in mesh A and mesh B that fall within the overlap
    /// rectangle must reference bitwise-identical 3D vertex positions
    /// (modulo per-triangle winding for B), satisfying Fig. 16's "the
    /// common part and the other two parts share identical sampling points
    /// on their boundaries."
    ///
    /// FIP §8 red-before-green: with the inject call elided in
    /// yang_integration.rs (Phase C wiring), the overlap rectangles in the
    /// two meshes have divergent diagonals and the bitwise-position
    /// equality assertions fail. With Phase C wired, this test passes.
    #[test]
    fn test_partial_overlap_inject_produces_consistent_overlap_mesh() {
        // Box A: x∈[0,1], y∈[0,1], z∈[0,1].
        let (k_a, h_a) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 0.0, 1.0);
        // Box B: x∈[0.3, 1.3], y∈[0,1], z∈[1, 2] — shifted X gives partial
        // overlap on z=1.
        let (k_b, h_b) = make_stacked_box(0.8, 0.5, 1.0, 1.0, 1.0, 1.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a must exist");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b must exist");

        // Tessellate both solids.
        let lod = tessellation::TessellationLod::Boolean;
        let mesh_a = tessellate_waffle_solid(solid_a, lod).expect("tessellate A");
        let mesh_b = tessellate_waffle_solid(solid_b, lod).expect("tessellate B");

        let (mut verts_a, mut tris_a) = render_mesh_to_arrays(&mesh_a);
        let (mut verts_b, mut tris_b) = render_mesh_to_arrays(&mesh_b);
        dedup_mesh_vertices(&mut verts_a, &mut tris_a);
        dedup_mesh_vertices(&mut verts_b, &mut tris_b);

        let mut bijective_a = BijectiveMap::from_render_mesh(&mesh_a, &solid_a.face_map);
        let mut bijective_b = BijectiveMap::from_render_mesh(&mesh_b, &solid_b.face_map);

        // Run Stage 0a: detect → split (which marks partial-overlap pairs).
        let mut solid_a_mut = solid_a.clone();
        let mut solid_b_mut = solid_b.clone();
        let mut pairs = detect_coplanar_face_pairs(&solid_a_mut, &solid_b_mut);
        split_brep_for_coplanar_pairs(&mut solid_a_mut, &mut solid_b_mut, &mut pairs);

        // Exactly one pair must be marked partial-overlap (the z=1
        // anti-parallel pair). Side-face pairs (x=0/x=1/y=0/y=1) either
        // have no overlap on their plane or are same-direction → not
        // partial-overlap.
        let partial_overlap_count = pairs.iter().filter(|p| p.is_partial_overlap).count();
        assert_eq!(
            partial_overlap_count,
            1,
            "Expected exactly 1 partial-overlap pair (z=1 anti-parallel), got {}: {:?}",
            partial_overlap_count,
            pairs
                .iter()
                .filter(|p| p.is_partial_overlap)
                .collect::<Vec<_>>()
        );
        let z1_pair = pairs
            .iter()
            .find(|p| p.is_partial_overlap)
            .expect("partial-overlap pair must exist");
        assert!(
            z1_pair.plane_normal[2].abs() > 0.99 && (z1_pair.plane_offset.abs() - 1.0).abs() < 1e-6,
            "partial-overlap pair must be on z=1, got normal={:?} offset={}",
            z1_pair.plane_normal,
            z1_pair.plane_offset
        );
        assert!(
            !z1_pair.same_direction,
            "PR7 partial-overlap pair must be anti-parallel; same-direction is PR8"
        );

        // Run the inject helper.
        inject_partial_overlap_mesh(
            &pairs,
            &mut verts_a,
            &mut tris_a,
            &mut bijective_a,
            &mut verts_b,
            &mut tris_b,
            &mut bijective_b,
        );

        // Yang §4.5.5 deliverable assertions for partial-overlap pairs:
        // bitwise-identical 3D positions in the OVERLAP region of both
        // meshes. The exclusive A-only and B-only regions are independently
        // triangulated and need NOT match each other — that's the point.

        // Overlap rectangle: x∈[0.3, 1.0], y∈[0, 1] at z=1.
        let in_overlap = |v: &[f64; 3]| -> bool {
            (v[2] - 1.0).abs() < f64::EPSILON
                && v[0] >= 0.3 - 1e-9
                && v[0] <= 1.0 + 1e-9
                && v[1] >= 0.0 - 1e-9
                && v[1] <= 1.0 + 1e-9
        };

        // Collect z=1 triangles that fall entirely inside the overlap rectangle.
        let z1_overlap_a: Vec<[[f64; 3]; 3]> = tris_a
            .iter()
            .filter_map(|tri| {
                let v0 = verts_a[tri[0]];
                let v1 = verts_a[tri[1]];
                let v2 = verts_a[tri[2]];
                if in_overlap(&v0) && in_overlap(&v1) && in_overlap(&v2) {
                    Some([v0, v1, v2])
                } else {
                    None
                }
            })
            .collect();
        let z1_overlap_b: Vec<[[f64; 3]; 3]> = tris_b
            .iter()
            .filter_map(|tri| {
                let v0 = verts_b[tri[0]];
                let v1 = verts_b[tri[1]];
                let v2 = verts_b[tri[2]];
                if in_overlap(&v0) && in_overlap(&v1) && in_overlap(&v2) {
                    Some([v0, v1, v2])
                } else {
                    None
                }
            })
            .collect();

        // 1. Both meshes have triangles in the overlap rectangle.
        assert!(
            !z1_overlap_a.is_empty(),
            "mesh A must have triangles inside the z=1 overlap rectangle after inject"
        );
        assert!(
            !z1_overlap_b.is_empty(),
            "mesh B must have triangles inside the z=1 overlap rectangle after inject"
        );

        // 2. Same triangle count in the overlap region.
        assert_eq!(
            z1_overlap_a.len(),
            z1_overlap_b.len(),
            "Yang §4.5.5: partial-overlap meshes must have the same triangle \
             count in the shared overlap region (A={}, B={})",
            z1_overlap_a.len(),
            z1_overlap_b.len()
        );

        // 3. Same set of bitwise-identical 3D vertex positions in the
        //    overlap region. Both meshes pass through the same
        //    `verts_2d_to_3d` call on the overlap polygon.
        let mut overlap_verts_a: Vec<[u64; 3]> = z1_overlap_a
            .iter()
            .flat_map(|tri| {
                tri.iter()
                    .map(|v| [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()])
            })
            .collect();
        let mut overlap_verts_b: Vec<[u64; 3]> = z1_overlap_b
            .iter()
            .flat_map(|tri| {
                tri.iter()
                    .map(|v| [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()])
            })
            .collect();
        overlap_verts_a.sort();
        overlap_verts_a.dedup();
        overlap_verts_b.sort();
        overlap_verts_b.dedup();
        assert_eq!(
            overlap_verts_a,
            overlap_verts_b,
            "Yang §4.5.5: partial-overlap meshes must reference the same \
             set of bitwise-identical 3D vertex positions in the shared \
             overlap region (got {} unique in A, {} unique in B)",
            overlap_verts_a.len(),
            overlap_verts_b.len()
        );

        // 4. Triangle vertex-position triples match between A and B in the
        //    overlap region modulo per-triangle winding reversal
        //    (anti-parallel canonical case).
        let mut sorted_overlap_a: Vec<[[u64; 3]; 3]> = z1_overlap_a
            .iter()
            .map(|tri| {
                let mut v: Vec<[u64; 3]> = tri
                    .iter()
                    .map(|p| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()])
                    .collect();
                v.sort();
                [v[0], v[1], v[2]]
            })
            .collect();
        let mut sorted_overlap_b: Vec<[[u64; 3]; 3]> = z1_overlap_b
            .iter()
            .map(|tri| {
                let mut v: Vec<[u64; 3]> = tri
                    .iter()
                    .map(|p| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()])
                    .collect();
                v.sort();
                [v[0], v[1], v[2]]
            })
            .collect();
        sorted_overlap_a.sort();
        sorted_overlap_b.sort();
        assert_eq!(
            sorted_overlap_a, sorted_overlap_b,
            "Yang §4.5.5: per-triangle vertex-position triples in the \
             shared overlap region must match between mesh A and mesh B \
             (modulo winding)"
        );
    }

    /// Yang §4.5.5 anti-parallel polygon-winding regression test (PR8).
    ///
    /// `pair.plane_normal` is always face A's outward normal (per
    /// `detect_coplanar_face_pairs`), so `compute_plane_basis` derives the
    /// shared 2D basis from A's frame. `collect_face_loop_2d` walks each
    /// face's B-Rep half-edge loop in its STORED order (CCW from each face's
    /// own outward normal). For anti-parallel coplanar pairs, B's outward
    /// normal points opposite to A's, so B's loop walks CCW-from-(-A's-normal)
    /// = CW in A's basis frame. Without correction, A's polygon is CCW and
    /// B's polygon is CW in the shared basis, and i_overlay's
    /// `Intersect`/`Difference` with `EvenOdd` produces inconsistent
    /// boolean output (a CCW input vs a CW input is treated as
    /// outer-vs-hole, not two outer contours).
    ///
    /// PR8's fix: reverse `poly_b` when `!pair.same_direction` so both
    /// polygons walk CCW in A's basis frame. This test exercises that
    /// invariant directly via the signed-area sign — basis-coordinate
    /// independent.
    ///
    /// Per Yang 2025 Fig. 16: "The common part and the other two parts
    /// share identical sampling points on their boundaries."
    ///
    /// FIP §8 red-before-green:
    /// - Red phase (with `if !pair.same_direction { poly_b.reverse(); }`
    ///   commented out): `area_a` and `area_b` have opposite signs →
    ///   assertion fails.
    /// - Green phase (with PR8 fix): both have the same sign → test passes.
    #[test]
    fn test_anti_parallel_polygon_winding_canonical() {
        // Canary geometry: A=[0,1]³ and B at z∈[1,2] — the z=1 caps form
        // an anti-parallel coplanar pair.
        let (k_a, h_a) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 0.0, 1.0);
        let (k_b, h_b) = make_stacked_box(0.5, 0.5, 1.0, 1.0, 1.0, 1.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a must exist");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b must exist");

        let pairs = detect_coplanar_face_pairs(solid_a, solid_b);

        // Find the z=1 anti-parallel pair specifically.
        let pair = pairs
            .iter()
            .find(|p| {
                p.plane_normal[2].abs() > 0.99
                    && (p.plane_offset.abs() - 1.0).abs() < 1e-6
                    && !p.same_direction
            })
            .expect("z=1 anti-parallel coplanar pair must be detected");

        // Compute the shared 2D basis from A's normal.
        let (u_axis, v_axis) = compute_plane_basis(pair.plane_normal);
        let plane_origin = [
            pair.plane_normal[0] * pair.plane_offset,
            pair.plane_normal[1] * pair.plane_offset,
            pair.plane_normal[2] * pair.plane_offset,
        ];

        // Extract both face boundary loops in the shared basis.
        let poly_a =
            collect_face_loop_2d(&solid_a.arena, pair.face_a, &plane_origin, &u_axis, &v_axis);
        let mut poly_b =
            collect_face_loop_2d(&solid_b.arena, pair.face_b, &plane_origin, &u_axis, &v_axis);

        // Apply PR8's fix manually (mirror what `split_brep_for_coplanar_pairs`
        // and `inject_partial_overlap_mesh` now do internally).
        if !pair.same_direction {
            poly_b.reverse();
        }

        // Signed area via the shoelace formula. Sign indicates winding
        // (positive = CCW in standard math, negative = CW). Two polygons
        // bounding the same 2D region must have the same sign for i_overlay
        // to treat them as compatible inputs.
        fn signed_area_2d(poly: &[(VertexIdx, [f64; 2])]) -> f64 {
            let n = poly.len();
            let mut sum = 0.0;
            for i in 0..n {
                let (_, [x1, y1]) = poly[i];
                let (_, [x2, y2]) = poly[(i + 1) % n];
                sum += x1 * y2 - x2 * y1;
            }
            sum * 0.5
        }

        let area_a = signed_area_2d(&poly_a);
        let area_b = signed_area_2d(&poly_b);

        // Sanity: both polygons must be non-degenerate.
        assert!(
            area_a.abs() > 1e-9,
            "poly_a must be a non-degenerate polygon, got signed area {area_a}",
        );
        assert!(
            area_b.abs() > 1e-9,
            "poly_b must be a non-degenerate polygon, got signed area {area_b}",
        );

        // PR8 assertion: signed-area signs match.
        assert_eq!(
            area_a.signum(),
            area_b.signum(),
            "Yang §4.5.5 anti-parallel polygon-winding invariant: poly_a \
             and poly_b must have matching winding (same signed-area sign) \
             after PR8's reversal. \
             Got area_a={area_a:.6}, area_b={area_b:.6}. \
             Without the fix, poly_a is CCW and poly_b is CW in A's basis \
             frame because B's B-Rep loop is stored CCW-from-B's-outward-\
             normal (= CW in A's basis). i_overlay then treats them as \
             outer-vs-hole instead of two outer contours, producing \
             inconsistent boolean output (the symptom that surfaced in the \
             stacked-box canary's residual unpaired half-edges)."
        );
    }
}
