//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! # PR-CR-AR3a RED — constraint-edge enforcement + TPI-at-crossing (tests only)
//!
//! This file is the **RED** slice of milestone M6 / PR-CR-AR3a: the failing
//! test module that pins the constraint-enforcement public surface
//! (`SegmentSpec`, `EnforceError`, `enforce_constraint_segments`,
//! `enforce_constraints`) defined verbatim in
//! `specs/pr_cr_ar3a_constraint_enforcement.md`. **No production code is
//! authored here** — the GREEN sub-agent adds the enforcement port (and the
//! re-exports) in a later sub-step. The not-yet-written production symbols are
//! referenced through `super::`, so this module FAILS TO COMPILE until GREEN
//! lands them (the intended RED state: unresolved-symbol errors for the four
//! missing items, nothing else).
//!
//! The tests cover all five oracle invariants from the spec §Oracle:
//! 1. constraints realized as constraint-flagged edge chains,
//! 2. TPI exactness (`orient3d == Zero` on all three supporting planes, EXACT),
//! 3. valid conforming sub-triangulation (pure-`dashu` covering),
//! 4. no spurious TPI (coincident-edge → flag only; one crossing → one TPI),
//! 5. the hand-verified cases (a) X-crossing, (b) T-junction, (c) edge-coincident.
//!
//! The pure-`dashu` exact helpers (`to_r`, `exact_coords`, `exact_signed_area2_xy`,
//! `find_vert_by_exact`) and the `one_tri` / `xy_triangle_a` / `tp_*` fixtures are
//! copied verbatim from `retriangulate.rs`'s test module (test-only duplication is
//! expected and fine). `find_explicit_vert` is a new FFI-free explicit-coord lookup.

use crate::arrangements::aux_structure::{ConstraintSegment, TypedPoint};
use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::gp_dispatch::{backing, gp, with_gp, Gp};
use crate::arrangements::{FastTrimesh, Plane};
use cad_primitives::Point3;
use indirect_predicates_sidecar_rs::{
    init_fpu, inner_segments_cross, point_in_inner_segment, AsGenericPoint, Sign as IpSign,
};
use std::collections::HashMap;

/// A constraint segment expressed in SUBMESH-vertex-id terms (the form the
/// enforcement core consumes). `v0`/`v1` are submesh vertex ids; `source_tri`
/// is the segment's supporting plane (the OPPOSITE triangle's 3 corners — for
/// an original transversal segment this is exactly
/// `ConstraintSegment.source_tri`).
#[derive(Clone, Debug, PartialEq)]
pub struct SegmentSpec {
    pub v0: u32,
    pub v1: u32,
    pub source_tri: [Point3; 3],
}

/// Error from constraint enforcement.
#[derive(Debug, PartialEq)]
pub enum EnforceError {
    /// A `ConstraintSegment` endpoint's interned coords are not present as a
    /// submesh vertex (the submesh was not produced by `split_single_triangle`
    /// over the same `points` set). `interned_id` is the offending endpoint.
    EndpointNotInSubmesh { interned_id: u32 },
    /// The topology walk could not locate the segment in the submesh (e.g. the
    /// endpoints are not both submesh vertices, or the fan is malformed). Wraps
    /// the offending `(v0, v1)` submesh vertex ids.
    SegmentNotLocatable { v0: u32, v1: u32 },
    /// A crossed constraint edge has no recorded supporting plane, so the TPI's
    /// third plane is unavailable. This is the AR3b global-state wall
    /// (`computeTriangleOfSegment`'s global `seg2tris` / coplanar `jollyPoint`):
    /// a sub-segment born mid-recursion that lost its directly-available
    /// `source_tri`. **STOP and report — do not improvise.** Deferred to AR3b.
    SourcePlaneUnavailable { v0: u32, v1: u32 },
    /// The three TPI supporting planes are not in general position (no single
    /// common intersection point — parallel / shared-line / coplanar). The
    /// coplanar `jollyPoint` fallback is AR3b. **STOP and report.**
    DegenerateTpi,
}

/// A pending constraint segment to realize, in submesh-vertex-id terms. The
/// per-item `source_tri` is AR3a's minimal replacement for the C++ global
/// `seg2tris` / `sub_segs_map`: a sub-segment born from a split inherits its
/// parent's supporting plane (a collinear sub-piece has the same plane).
#[derive(Clone, Debug)]
struct WorkItem {
    v0: u32,
    v1: u32,
    source_tri: [Point3; 3],
}

/// Sorted vertex-id pair (vertex ids are stable under `add_*`/`split_*`; edge
/// ids are not — so the source-plane side map is keyed by vertex pair).
fn sorted_pair(a: u32, b: u32) -> (u32, u32) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Enforce a list of constraint segments (submesh-vertex-id form) into the
/// submesh. Seeds an internal work-list from `specs`, then repeatedly pops a
/// work item and calls the `add_constraint_segment` port until the list is
/// empty. Each resulting constraint edge is flagged via `set_edge_constr`.
/// Orientation is computed once internally from the base corners (submesh
/// vertices 0,1,2 — always explicit, never removed).
pub fn enforce_constraint_segments(
    subm: &mut FastTrimesh,
    specs: &[SegmentSpec],
) -> Result<(), EnforceError> {
    init_fpu();

    // Orientation: computed once from the base corners (submesh vertex ids
    // 0,1,2 — always explicit, never removed). Do NOT use tri_orientation(0):
    // after splits the triangle at slot 0 may have a non-explicit corner.
    let orientation = base_orientation(subm);

    // Source-plane side map (the minimal `TriangleSoup` — see spec).
    let mut constraint_planes: HashMap<(u32, u32), [Point3; 3]> = HashMap::new();

    // Work-list seeded from the specs.
    let mut work: Vec<WorkItem> = specs
        .iter()
        .map(|s| WorkItem {
            v0: s.v0,
            v1: s.v1,
            source_tri: s.source_tri,
        })
        .collect();

    while let Some(item) = work.pop() {
        add_constraint_segment(subm, &item, orientation, &mut constraint_planes, &mut work)?;
    }
    Ok(())
}

/// AR2b adapter: enforce `ConstraintSegment`s (interned-id endpoints) by
/// resolving each endpoint id → its `TypedPoint` coords → the submesh vertex
/// carrying those exact coords (structural `VertexCoords` equality, FFI-free),
/// building `SegmentSpec`s, and delegating to `enforce_constraint_segments`.
/// `points` MUST be the interned set the submesh was built from. Returns
/// `EndpointNotInSubmesh` if a resolution fails.
pub fn enforce_constraints(
    subm: &mut FastTrimesh,
    segments: &[ConstraintSegment],
    points: &[TypedPoint],
) -> Result<(), EnforceError> {
    let resolve = |subm: &FastTrimesh, interned_id: u32| -> Result<u32, EnforceError> {
        let coords = points[interned_id as usize].coords;
        (0..subm.num_verts())
            .find(|&v| *subm.vert_coords(v) == coords)
            .ok_or(EnforceError::EndpointNotInSubmesh { interned_id })
    };

    let mut specs: Vec<SegmentSpec> = Vec::with_capacity(segments.len());
    for seg in segments {
        let v0 = resolve(subm, seg.endpoints.0)?;
        let v1 = resolve(subm, seg.endpoints.1)?;
        specs.push(SegmentSpec {
            v0,
            v1,
            source_tri: seg.source_tri,
        });
    }
    enforce_constraint_segments(subm, &specs)
}

/// The base triangle's 2D orientation sign, computed from submesh vertices
/// 0,1,2 (always explicit, never removed) via the reference-plane `orient2d`.
fn base_orientation(subm: &FastTrimesh) -> IpSign {
    let c0 = subm.vert_coords(0);
    let c1 = subm.vert_coords(1);
    let c2 = subm.vert_coords(2);
    let b0 = backing(c0);
    let b1 = backing(c1);
    let b2 = backing(c2);
    let g0 = gp(c0, &b0);
    let g1 = gp(c1, &b1);
    let g2 = gp(c2, &b2);
    dispatch_orient2d(subm.ref_plane(), &g0, &g1, &g2)
}

/// Port of `addConstraintSegment` (cpp:597). Realizes one work item into the
/// submesh; sub-segments born from a split are pushed back to `work`.
fn add_constraint_segment(
    subm: &mut FastTrimesh,
    item: &WorkItem,
    orientation: IpSign,
    constraint_planes: &mut HashMap<(u32, u32), [Point3; 3]>,
    work: &mut Vec<WorkItem>,
) -> Result<(), EnforceError> {
    let v0_id = item.v0;
    let v1_id = item.v1;

    // Branch 1 — the segment is already an edge: flag it, record its plane.
    if let Some(e) = subm.edge_id(v0_id, v1_id) {
        subm.set_edge_constr(e);
        constraint_planes.insert(sorted_pair(v0_id, v1_id), item.source_tri);
        return Ok(());
    }

    // Start from the lower-valence endpoint (cpp:609).
    let (v_start, v_stop) = if subm.vert_valence(v0_id) < subm.vert_valence(v1_id) {
        (v0_id, v1_id)
    } else {
        (v1_id, v0_id)
    };

    let mut intersected_edges: Vec<u32> = Vec::new();
    let mut intersected_tris: Vec<u32> = Vec::new();

    // Port of findIntersectingElements (cpp:644). May push sub-segments to
    // `work` (T-junction / crossing) and leave `intersected_edges` empty.
    let split_happened = find_intersecting_elements(
        subm,
        v_start,
        v_stop,
        item,
        &mut intersected_edges,
        &mut intersected_tris,
        constraint_planes,
        work,
    )?;

    // A point_inside_segment / crossing split already flagged a sub-edge and
    // pushed the remainder — re-processed from the work-list (cpp:617).
    if split_happened || intersected_edges.is_empty() {
        return Ok(());
    }

    // Branch 3 — non-crossing transversal: re-triangulate the two boundary
    // walks and flag the new (v_start, v_stop) edge (cpp:619-639).
    let h0 = boundary_walker(
        subm,
        v_start,
        v_stop,
        &intersected_tris,
        &intersected_edges,
        false,
    )
    .ok_or(EnforceError::SegmentNotLocatable {
        v0: v_start,
        v1: v_stop,
    })?;
    let h1 = boundary_walker(
        subm,
        v_stop,
        v_start,
        &intersected_tris,
        &intersected_edges,
        true,
    )
    .ok_or(EnforceError::SegmentNotLocatable {
        v0: v_start,
        v1: v_stop,
    })?;

    let mut new_tris: Vec<u32> = Vec::new();
    earcut_linear(subm, &h0, &mut new_tris, orientation);
    earcut_linear(subm, &h1, &mut new_tris, orientation);

    for tri in new_tris.chunks_exact(3) {
        subm.add_tri(tri[0], tri[1], tri[2]);
    }

    subm.remove_tris(intersected_tris);

    let e = subm
        .edge_id(v_start, v_stop)
        .ok_or(EnforceError::SegmentNotLocatable {
            v0: v_start,
            v1: v_stop,
        })?;
    subm.set_edge_constr(e);
    constraint_planes.insert(sorted_pair(v_start, v_stop), item.source_tri);
    Ok(())
}

/// Port of findIntersectingElements (cpp:644). Returns `Ok(true)` if a split
/// happened (T-junction or — in 3c — a crossing TPI), in which case the
/// remaining sub-segment(s) are already pushed to `work` and `intersected_*`
/// are empty. Returns `Ok(false)` for the non-crossing transversal case, with
/// the intersected edge/tri lists populated.
#[allow(clippy::too_many_arguments)]
fn find_intersecting_elements(
    subm: &mut FastTrimesh,
    v_start: u32,
    v_stop: u32,
    item: &WorkItem,
    intersected_edges: &mut Vec<u32>,
    intersected_tris: &mut Vec<u32>,
    constraint_planes: &mut HashMap<(u32, u32), [Point3; 3]>,
    work: &mut Vec<WorkItem>,
) -> Result<bool, EnforceError> {
    // First loop (cpp:651-698): find the edge in link(v_start) that the
    // segment {v_start, v_stop} crosses, or a vertex it passes through.
    for t_id in subm.adj_v2t(v_start) {
        let e_id = match edge_opp_to_vert(subm, t_id, v_start) {
            Some(e) => e,
            None => continue,
        };
        let ev0_id = subm.edge_vert_id(e_id, 0);
        let ev1_id = subm.edge_vert_id(e_id, 1);
        // The opposite edge cannot contain v_stop (cpp:656 assert).
        if ev0_id == v_stop || ev1_id == v_stop {
            continue;
        }

        if segments_intersect_inside(subm, v_start, v_stop, ev0_id, ev1_id) {
            intersected_edges.push(e_id);
            intersected_tris.push(t_id);
            break;
        } else if point_inside_segment(subm, v_start, v_stop, ev0_id) {
            // T-junction through ev0: flag (v_start, ev0), push the remainder.
            t_junction_split(subm, v_start, ev0_id, v_stop, item, constraint_planes, work)?;
            intersected_edges.clear();
            return Ok(true);
        } else if point_inside_segment(subm, v_start, v_stop, ev1_id) {
            // T-junction through ev1: flag (v_start, ev1), push the remainder.
            t_junction_split(subm, v_start, ev1_id, v_stop, item, constraint_planes, work)?;
            intersected_edges.clear();
            return Ok(true);
        }
    }

    if intersected_edges.is_empty() {
        // No crossed edge AND no T-junction found in link(v_start): the
        // segment is not locatable in the fan.
        return Err(EnforceError::SegmentNotLocatable {
            v0: v_start,
            v1: v_stop,
        });
    }

    // Second loop (cpp:703-801): walk the topology, accumulating the sorted
    // list of crossed edges/tris until reaching v_stop.
    loop {
        let e_id = *intersected_edges.last().expect("non-empty by construction");
        let ev0_id = subm.edge_vert_id(e_id, 0);
        let ev1_id = subm.edge_vert_id(e_id, 1);

        if !subm.edge_is_constr(e_id) {
            let t_id = tri_opp_to_edge(subm, e_id, *intersected_tris.last().unwrap()).ok_or(
                EnforceError::SegmentNotLocatable {
                    v0: v_start,
                    v1: v_stop,
                },
            )?;
            let v2 = subm.tri_vert_opposite_to(t_id, ev0_id, ev1_id).ok_or(
                EnforceError::SegmentNotLocatable {
                    v0: v_start,
                    v1: v_stop,
                },
            )?;

            if segments_intersect_inside(subm, v_start, v_stop, ev0_id, v2) {
                let int_edge =
                    subm.edge_id(ev0_id, v2)
                        .ok_or(EnforceError::SegmentNotLocatable {
                            v0: v_start,
                            v1: v_stop,
                        })?;
                intersected_edges.push(int_edge);
                intersected_tris.push(t_id);
            } else if segments_intersect_inside(subm, v_start, v_stop, ev1_id, v2) {
                let int_edge =
                    subm.edge_id(ev1_id, v2)
                        .ok_or(EnforceError::SegmentNotLocatable {
                            v0: v_start,
                            v1: v_stop,
                        })?;
                intersected_edges.push(int_edge);
                intersected_tris.push(t_id);
            } else if v2 != v_stop {
                // The segment passes through the interior vertex v2 (cpp:731).
                t_junction_split(subm, v_start, v2, v_stop, item, constraint_planes, work)?;
                intersected_edges.clear();
                return Ok(true);
            } else {
                break; // converged (v2 == v_stop)
            }
        } else {
            // e_id is an existing constraint edge — TPI creation (3c).
            constraint_crossing_tpi(
                subm,
                v_start,
                v_stop,
                e_id,
                ev0_id,
                ev1_id,
                item,
                constraint_planes,
                work,
            )?;
            intersected_edges.clear();
            return Ok(true);
        }
    }

    // Append the last triangle (cpp:791-799).
    let e_id = *intersected_edges.last().unwrap();
    let t_id = tri_opp_to_edge(subm, e_id, *intersected_tris.last().unwrap()).ok_or(
        EnforceError::SegmentNotLocatable {
            v0: v_start,
            v1: v_stop,
        },
    )?;
    intersected_tris.push(t_id);

    Ok(false)
}

/// A T-junction split: the segment {v_start, v_stop} passes through the
/// existing vertex `mid`. Flag the sub-edge (v_start, mid) and push the
/// remaining sub-segment (mid, v_stop), carrying the same supporting plane.
fn t_junction_split(
    subm: &mut FastTrimesh,
    v_start: u32,
    mid: u32,
    v_stop: u32,
    item: &WorkItem,
    constraint_planes: &mut HashMap<(u32, u32), [Point3; 3]>,
    work: &mut Vec<WorkItem>,
) -> Result<(), EnforceError> {
    let e = subm
        .edge_id(v_start, mid)
        .ok_or(EnforceError::SegmentNotLocatable {
            v0: v_start,
            v1: mid,
        })?;
    subm.set_edge_constr(e);
    constraint_planes.insert(sorted_pair(v_start, mid), item.source_tri);
    work.push(WorkItem {
        v0: mid,
        v1: v_stop,
        source_tri: item.source_tri,
    });
    Ok(())
}

/// Constraint-crossing TPI construction (port of the cpp:754-787 constraint
/// branch and cpp:1007 `createTPI`). The walk has met an EXISTING constraint
/// edge `e_id = (ev0_id, ev1_id)`: construct the TPI where the base plane and
/// the two crossing segments' planes meet, insert it, split the crossed edge,
/// flag both halves, and push the two sub-segments of the current segment.
#[allow(clippy::too_many_arguments)]
fn constraint_crossing_tpi(
    subm: &mut FastTrimesh,
    v_start: u32,
    v_stop: u32,
    e_id: u32,
    ev0_id: u32,
    ev1_id: u32,
    item: &WorkItem,
    constraint_planes: &mut HashMap<(u32, u32), [Point3; 3]>,
    work: &mut Vec<WorkItem>,
) -> Result<(), EnforceError> {
    // The crossed constraint edge's supporting plane (the AR3b global-state
    // wall): if it was never recorded, the third TPI plane is unavailable.
    let plane_e = *constraint_planes.get(&sorted_pair(ev0_id, ev1_id)).ok_or(
        EnforceError::SourcePlaneUnavailable {
            v0: ev0_id,
            v1: ev1_id,
        },
    )?;

    // Base triangle corners (submesh vertices 0,1,2 — always explicit).
    let base = base_corners(subm);

    // Build the TPI coords (base plane ∩ segment plane ∩ crossed-edge plane),
    // guarding general position exactly (else DegenerateTpi).
    let tpi_coords = create_tpi(base, item.source_tri, plane_e)?;

    // Dedup vs an existing submesh vertex carrying those exact Tpi coords
    // (the C++ `addVertexInSortedList` reuse). For the in-scope single
    // X-crossing this never fires (the crossing is detected once).
    let tpi_vid = match (0..subm.num_verts()).find(|&v| *subm.vert_coords(v) == tpi_coords) {
        Some(v) => v,
        None => subm.add_vert_typed(tpi_coords),
    };

    // Split the crossed constraint edge at the TPI; flag both halves and
    // record the crossed edge's plane for each (so a later crossing of a half
    // resolves its plane).
    subm.split_edge(e_id, tpi_vid);
    let e0 = subm
        .edge_id(ev0_id, tpi_vid)
        .ok_or(EnforceError::SegmentNotLocatable {
            v0: ev0_id,
            v1: tpi_vid,
        })?;
    let e1 = subm
        .edge_id(tpi_vid, ev1_id)
        .ok_or(EnforceError::SegmentNotLocatable {
            v0: tpi_vid,
            v1: ev1_id,
        })?;
    subm.set_edge_constr(e0);
    subm.set_edge_constr(e1);
    constraint_planes.insert(sorted_pair(ev0_id, tpi_vid), plane_e);
    constraint_planes.insert(sorted_pair(tpi_vid, ev1_id), plane_e);

    // Push the two sub-segments of the CURRENT segment (carrying its plane).
    work.push(WorkItem {
        v0: v_start,
        v1: tpi_vid,
        source_tri: item.source_tri,
    });
    work.push(WorkItem {
        v0: tpi_vid,
        v1: v_stop,
        source_tri: item.source_tri,
    });
    Ok(())
}

/// Base triangle corners (submesh vertices 0,1,2 — always explicit, never
/// removed). Used as the TPI's first supporting plane.
fn base_corners(subm: &FastTrimesh) -> [Point3; 3] {
    let corner = |v: u32| match subm.vert_coords(v) {
        VertexCoords::Explicit(p) => *p,
        // Base corners are explicit by construction; fall back to the finite
        // approx only to stay panic-free (never reached for a valid submesh).
        _ => subm.vert(v),
    };
    [corner(0), corner(1), corner(2)]
}

/// Construct a `VertexCoords::Tpi` at the common intersection of three
/// supporting planes (`v` = base triangle, `w` = segment plane, `u` =
/// crossed-edge plane), port of cpp:1007 `createTPI`. The point itself is
/// carried symbolically by its nine generators; here we ONLY validate general
/// position (the three plane normals' exact 3×3 determinant ≠ 0 in `RBig`),
/// returning `DegenerateTpi` otherwise (the coplanar / parallel `jollyPoint`
/// fallback is AR3b).
fn create_tpi(
    v: [Point3; 3],
    w: [Point3; 3],
    u: [Point3; 3],
) -> Result<VertexCoords, EnforceError> {
    if tpi_planes_general_position(&v, &w, &u) {
        Ok(VertexCoords::Tpi { v, w, u })
    } else {
        Err(EnforceError::DegenerateTpi)
    }
}

/// True iff the three planes (each given by a triangle's 3 corners) meet in a
/// single point: the exact `RBig` determinant of their normals is non-zero.
/// Mirrors the `exact_coords` Tpi-arm general-position check; reimplemented in
/// production (does not import the test helper).
fn tpi_planes_general_position(v: &[Point3; 3], w: &[Point3; 3], u: &[Point3; 3]) -> bool {
    use dashu::float::FBig;
    use dashu::rational::RBig;

    let to_r = |x: f64| -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    };
    let to_r3 = |p: &Point3| [to_r(p.x()), to_r(p.y()), to_r(p.z())];
    let sub = |a: &[RBig; 3], b: &[RBig; 3]| -> [RBig; 3] {
        [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
    };
    let cross = |a: &[RBig; 3], b: &[RBig; 3]| -> [RBig; 3] {
        [
            &(&a[1] * &b[2]) - &(&a[2] * &b[1]),
            &(&a[2] * &b[0]) - &(&a[0] * &b[2]),
            &(&a[0] * &b[1]) - &(&a[1] * &b[0]),
        ]
    };
    let dot = |a: &[RBig; 3], b: &[RBig; 3]| -> RBig {
        &(&(&a[0] * &b[0]) + &(&a[1] * &b[1])) + &(&a[2] * &b[2])
    };
    // Per-plane normal = cross of two edge vectors of its generator triangle.
    let normal = |tri: &[Point3; 3]| -> [RBig; 3] {
        let r = to_r3(&tri[0]);
        let s = to_r3(&tri[1]);
        let t = to_r3(&tri[2]);
        cross(&sub(&s, &r), &sub(&t, &r))
    };
    let n0 = normal(v);
    let n1 = normal(w);
    let n2 = normal(u);
    // det of the 3×3 with rows n0,n1,n2 is n0 · (n1 × n2).
    let det = dot(&n0, &cross(&n1, &n2));
    det != RBig::ZERO
}

/// Port of `boundaryWalker` (cpp:806). Walks the border of the intersected-tri
/// fan from `v_start` to `v_stop`, producing the boundary polygon `h`. When
/// `reversed` is set, the tri/edge cursors advance from the back (the C++
/// `rbegin()` form). Returns `None` if the walk cannot make progress.
fn boundary_walker(
    subm: &FastTrimesh,
    v_start: u32,
    v_stop: u32,
    intersected_tris: &[u32],
    intersected_edges: &[u32],
    reversed: bool,
) -> Option<Vec<u32>> {
    // The intersected-tri list has ONE MORE entry than the edge list (the
    // last triangle is appended after the second walk converges, cpp:796), so
    // the tri and edge cursors range over different lengths.
    let nt = intersected_tris.len();
    let ne = intersected_edges.len();
    debug_assert_eq!(nt, ne + 1, "boundary_walker: tris must be edges + 1");
    // `at` maps the logical step to an index, mirroring the C++ forward
    // (`begin`) / reverse (`rbegin`) iterator pair.
    let tri_at = |i: usize| -> u32 {
        if reversed {
            intersected_tris[nt - 1 - i]
        } else {
            intersected_tris[i]
        }
    };
    let edge_at = |i: usize| -> u32 {
        if reversed {
            intersected_edges[ne - 1 - i]
        } else {
            intersected_edges[i]
        }
    };

    let mut h: Vec<u32> = vec![v_start];
    let mut p = 0usize; // current tri cursor (0..nt)
    let mut e = 0usize; // current edge cursor (0..ne)

    loop {
        let curr_v = *h.last().unwrap();
        let mut curr_p = tri_at(p);
        let off = subm.tri_vert_offset(curr_p, curr_v)?;
        let mut next_v = subm.tri_vert_id(curr_p, (off + 1) % 3);

        // Skip across edges equal to the current intersected edge (cpp:818).
        while e < ne && subm.edge_id(curr_v, next_v) == Some(edge_at(e)) {
            p += 1;
            if p >= nt {
                return None;
            }
            curr_p = tri_at(p);
            if subm.tri_contains_vert(curr_p, v_stop) {
                h.push(v_stop);
                return Some(h);
            }
            e += 1;
            if e >= ne {
                return None;
            }
            let off = subm.tri_vert_offset(curr_p, curr_v)?;
            next_v = subm.tri_vert_id(curr_p, (off + 1) % 3);
        }

        h.push(next_v);
        p += 1;
        if p >= nt {
            // Past the last tri: must have reached v_stop.
            if next_v == v_stop {
                return Some(h);
            }
            return None;
        }
        let curr_p = tri_at(p);
        if subm.tri_contains_vert(curr_p, v_stop) {
            h.push(v_stop);
            return Some(h);
        }
        e += 1;
        if e >= ne && *h.last().unwrap() != v_stop {
            return None;
        }
        if *h.last().unwrap() == v_stop {
            return Some(h);
        }
    }
}

/// Port of `earcutLinear` (cpp:912) — the doubly-linked-list O(n) ear cut. The
/// polygon `poly` is a boundary walk (`v_start ... v_stop`); ears are emitted
/// to `tris` as flat vertex-id triples. The ear test compares the
/// reference-plane `orient2d` sign against `orientation`.
fn earcut_linear(subm: &FastTrimesh, poly: &[u32], tris: &mut Vec<u32>, orientation: IpSign) {
    let size = poly.len();
    debug_assert!(size >= 3, "earcut_linear: poly must have >= 3 verts");
    if size < 3 {
        return;
    }
    if size == 3 {
        tris.extend_from_slice(poly);
        return;
    }

    // Doubly linked list over poly indices.
    let mut prev: Vec<usize> = (0..size)
        .map(|i| if i == 0 { size - 1 } else { i - 1 })
        .collect();
    let mut next: Vec<usize> = (0..size)
        .map(|i| if i == size - 1 { 0 } else { i + 1 })
        .collect();

    let ear_ok = |subm: &FastTrimesh, a: u32, b: u32, c: u32| -> bool {
        let ca = subm.vert_coords(a);
        let cb = subm.vert_coords(b);
        let cc = subm.vert_coords(c);
        let ba = backing(ca);
        let bb = backing(cb);
        let bc = backing(cc);
        let ga = gp(ca, &ba);
        let gb = gp(cb, &bb);
        let gc = gp(cc, &bc);
        let check = dispatch_orient2d(subm.ref_plane(), &ga, &gb, &gc);
        check == orientation
    };

    let mut is_ear = vec![false; size];
    let mut ears: Vec<usize> = Vec::with_capacity(size);

    // Detect all initial ears (convex corners, excluding the constrained-edge
    // endpoints poly[0] and poly[size-1]).
    for curr in 1..size - 1 {
        if prev[curr] != next[curr] && ear_ok(subm, poly[prev[curr]], poly[curr], poly[next[curr]])
        {
            ears.push(curr);
            is_ear[curr] = true;
        }
    }

    let mut length = size;
    while let Some(curr) = ears.pop() {
        // Emit the ear triangle.
        tris.push(poly[prev[curr]]);
        tris.push(poly[curr]);
        tris.push(poly[next[curr]]);

        // Unlink curr.
        let pc = prev[curr];
        let nc = next[curr];
        next[pc] = nc;
        prev[nc] = pc;

        length -= 1;
        if length < 3 {
            return;
        }

        // prev[curr] may become a new ear.
        if !is_ear[pc] && pc != 0 {
            let ppc = prev[pc];
            if ppc != nc && ear_ok(subm, poly[ppc], poly[pc], poly[nc]) {
                ears.push(pc);
                is_ear[pc] = true;
            }
        }
        // next[curr] may become a new ear.
        if !is_ear[nc] && nc < size - 1 {
            let nnc = next[nc];
            if nnc != pc && ear_ok(subm, poly[pc], poly[nc], poly[nnc]) {
                ears.push(nc);
                is_ear[nc] = true;
            }
        }
    }
}

// ── Topology helpers (cpp:328 edgeOppToVert, cpp:470 triOppToEdge) ────

/// The edge of triangle `t` opposite vertex `v` (the edge joining `t`'s two
/// corners other than `v`). Mirrors `edgeOppToVert` (cpp:328). Returns `None`
/// if `t` does not contain `v` or the edge is absent.
fn edge_opp_to_vert(subm: &FastTrimesh, t: u32, v: u32) -> Option<u32> {
    let tri = subm.tri(t);
    let off = subm.tri_vert_offset(t, v)?;
    let a = tri[((off + 1) % 3) as usize];
    let b = tri[((off + 2) % 3) as usize];
    subm.edge_id(a, b)
}

/// The triangle adjacent to edge `e` on the side opposite triangle `t`.
/// Mirrors `triOppToEdge` (cpp:470). Returns `None` for a boundary edge.
fn tri_opp_to_edge(subm: &FastTrimesh, e: u32, t: u32) -> Option<u32> {
    let adj = subm.adj_e2t(e);
    if adj.len() == 1 {
        return None; // boundary edge
    }
    adj.iter().copied().find(|&x| x != t)
}

// ── Predicate dispatchers over Gp handles ─────────────────────────────

/// `orient2d` (in `plane`) over three `Gp` handles.
fn dispatch_orient2d(plane: Plane, a: &Gp, b: &Gp, c: &Gp) -> IpSign {
    use indirect_predicates_sidecar_rs::{orient2d_xy, orient2d_yz, orient2d_zx};
    fn o2d(
        plane: Plane,
        a: &impl AsGenericPoint,
        b: &impl AsGenericPoint,
        c: &impl AsGenericPoint,
    ) -> IpSign {
        match plane {
            Plane::XY => orient2d_xy(a, b, c),
            Plane::YZ => orient2d_yz(a, b, c),
            Plane::ZX => orient2d_zx(a, b, c),
        }
    }
    with_gp!(o2d(plane, a, b, c); a, b, c)
}

/// True iff the open segments {v_start, v_stop} and {ev0, ev1} cross at a point
/// strictly interior to both (port of `segmentsIntersectInside`, cpp:1170 →
/// `innerSegmentsCross`).
fn segments_intersect_inside(
    subm: &FastTrimesh,
    v_start: u32,
    v_stop: u32,
    ev0: u32,
    ev1: u32,
) -> bool {
    let (ca, cb, cc, cd) = (
        subm.vert_coords(v_start),
        subm.vert_coords(v_stop),
        subm.vert_coords(ev0),
        subm.vert_coords(ev1),
    );
    let (ba, bb, bc, bd) = (backing(ca), backing(cb), backing(cc), backing(cd));
    let (ga, gb, gc, gd) = (gp(ca, &ba), gp(cb, &bb), gp(cc, &bc), gp(cd, &bd));
    let (ga, gb, gc, gd) = (&ga, &gb, &gc, &gd);
    with_gp!(inner_segments_cross(ga, gb, gc, gd); ga, gb, gc, gd)
}

/// True iff vertex `p` lies on the OPEN segment (ev0, ev1) — collinear and
/// strictly between, endpoints excluded (port of `pointInsideSegment`,
/// cpp:1178 → `pointInInnerSegment`).
///
/// `pointInInnerSegment(p, v1, v2)` is mathematically symmetric in `v1 ↔ v2`
/// ("p strictly between v1 and v2"), and IS symmetric for implicit (LPI/TPI)
/// endpoints, where `lessThanOn*` returns a real signed −1/0/+1. For two
/// EXPLICIT endpoints the EE branch returns the C++ `bool` `a.X() < b.X()`
/// (0 or 1, never −1), so a single call only fires when `v1 < p < v2`
/// componentwise — i.e. it becomes endpoint-order-sensitive (the documented
/// sidecar EE limitation; see `indirect-predicates-sidecar-rs` smoke tests).
/// To restore the intended symmetric semantics regardless of the
/// valence-chosen endpoint order, query BOTH orders and OR them. For implicit
/// endpoints this is a no-op (the symmetric result is unchanged); for explicit
/// endpoints it makes "strictly inside" order-independent without widening any
/// tolerance or special-casing a fixture.
fn point_inside_segment(subm: &FastTrimesh, ev0: u32, ev1: u32, p: u32) -> bool {
    let (cp, c0, c1) = (
        subm.vert_coords(p),
        subm.vert_coords(ev0),
        subm.vert_coords(ev1),
    );
    let (bp, b0, b1) = (backing(cp), backing(c0), backing(c1));
    let (gpp, g0, g1) = (gp(cp, &bp), gp(c0, &b0), gp(c1, &b1));
    let (gpp, g0, g1) = (&gpp, &g0, &g1);
    let fwd = with_gp!(point_in_inner_segment(gpp, g0, g1); gpp, g0, g1);
    let rev = with_gp!(point_in_inner_segment(gpp, g1, g0); gpp, g0, g1);
    fwd || rev
}

#[cfg(test)]
mod tests {
    //! RED tests for PR-CR-AR3a (`enforce_constraint_segments` /
    //! `enforce_constraints`). These exercise the intended GREEN behaviour
    //! through the public surface the GREEN implementer WILL add — none of which
    //! exists yet, so this module currently FAILS TO RESOLVE against the
    //! not-yet-written API. No production code is authored in this PR.
    //!
    //! All coordinates are hard-coded (determinism); hand-derivations are
    //! documented inline. The AR1/AR2a fixtures are reused so the enforcement is
    //! fed real `classify_all` / `group_constraint_segments` output where the
    //! adapter path is exercised, not a hand-built mock.

    use crate::arrangements::fast_trimesh::VertexCoords;
    use crate::arrangements::{
        classify_all, detect_intersecting_pairs, group_constraint_segments,
        group_intersection_points, split_single_triangle, FastTrimesh, Plane, TypedPoint,
    };
    use cad_primitives::Point3;
    use dashu::float::FBig;
    use dashu::rational::RBig;

    // The not-yet-written production surface (RED: these fail to resolve).
    use super::{enforce_constraint_segments, enforce_constraints, EnforceError, SegmentSpec};

    // ── Submesh + TypedPoint fixtures (copied from retriangulate.rs tests) ──

    /// A 1-triangle submesh: 3 explicit corners, `tri = [0,1,2]`, XY plane.
    fn one_tri(c0: Point3, c1: Point3, c2: Point3) -> FastTrimesh {
        FastTrimesh::from_soup(&[c0, c1, c2], &[[0u32, 1, 2]], Plane::XY).unwrap()
    }

    /// A0=(0,0,0), A1=(4,0,0), A2=(0,4,0); z=0, interior `{x>0,y>0,x+y<4}`.
    fn xy_triangle_a() -> [Point3; 3] {
        [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(4.0, 0.0, 0.0),
            Point3::new(0.0, 4.0, 0.0),
        ]
    }

    /// An explicit `TypedPoint`.
    fn tp_explicit(p: Point3) -> TypedPoint {
        TypedPoint {
            coords: VertexCoords::Explicit(p),
        }
    }

    // ── Exact-rational helpers (pure dashu — copied from retriangulate.rs) ──

    fn to_r(x: f64) -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    }

    /// Exact coordinates of a stored `VertexCoords` (Explicit / Lpi / Tpi).
    /// Identical to retriangulate.rs's helper.
    fn exact_coords(c: &VertexCoords) -> [RBig; 3] {
        match c {
            VertexCoords::Explicit(p) => [to_r(p.x()), to_r(p.y()), to_r(p.z())],
            VertexCoords::Lpi { line, plane } => {
                let p = [to_r(line[0].x()), to_r(line[0].y()), to_r(line[0].z())];
                let q = [to_r(line[1].x()), to_r(line[1].y()), to_r(line[1].z())];
                let r = [to_r(plane[0].x()), to_r(plane[0].y()), to_r(plane[0].z())];
                let s = [to_r(plane[1].x()), to_r(plane[1].y()), to_r(plane[1].z())];
                let t = [to_r(plane[2].x()), to_r(plane[2].y()), to_r(plane[2].z())];

                let sub = |a: &[RBig; 3], b: &[RBig; 3]| -> [RBig; 3] {
                    [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
                };
                let cross = |a: &[RBig; 3], b: &[RBig; 3]| -> [RBig; 3] {
                    [
                        &(&a[1] * &b[2]) - &(&a[2] * &b[1]),
                        &(&a[2] * &b[0]) - &(&a[0] * &b[2]),
                        &(&a[0] * &b[1]) - &(&a[1] * &b[0]),
                    ]
                };
                let dot = |a: &[RBig; 3], b: &[RBig; 3]| -> RBig {
                    &(&(&a[0] * &b[0]) + &(&a[1] * &b[1])) + &(&a[2] * &b[2])
                };

                let n = cross(&sub(&s, &r), &sub(&t, &r));
                let rp = sub(&r, &p);
                let qp = sub(&q, &p);
                let num = dot(&rp, &n);
                let den = dot(&qp, &n);
                assert!(
                    den != RBig::ZERO,
                    "exact_coords: LPI line is parallel to plane (den == 0) — bad fixture"
                );
                let u = &num / &den;
                [
                    &p[0] + &(&u * &qp[0]),
                    &p[1] + &(&u * &qp[1]),
                    &p[2] + &(&u * &qp[2]),
                ]
            }
            VertexCoords::Tpi { v, w, u } => {
                let to_r3 = |p: &Point3| [to_r(p.x()), to_r(p.y()), to_r(p.z())];
                let sub = |a: &[RBig; 3], b: &[RBig; 3]| -> [RBig; 3] {
                    [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
                };
                let cross = |a: &[RBig; 3], b: &[RBig; 3]| -> [RBig; 3] {
                    [
                        &(&a[1] * &b[2]) - &(&a[2] * &b[1]),
                        &(&a[2] * &b[0]) - &(&a[0] * &b[2]),
                        &(&a[0] * &b[1]) - &(&a[1] * &b[0]),
                    ]
                };
                let dot = |a: &[RBig; 3], b: &[RBig; 3]| -> RBig {
                    &(&(&a[0] * &b[0]) + &(&a[1] * &b[1])) + &(&a[2] * &b[2])
                };

                let plane_eqn = |tri: &[Point3; 3]| -> ([RBig; 3], RBig) {
                    let r = to_r3(&tri[0]);
                    let s = to_r3(&tri[1]);
                    let t = to_r3(&tri[2]);
                    let n = cross(&sub(&s, &r), &sub(&t, &r));
                    let d = dot(&n, &r);
                    (n, d)
                };
                let (n0, d0) = plane_eqn(v);
                let (n1, d1) = plane_eqn(w);
                let (n2, d2) = plane_eqn(u);

                let det_rows = |r0: &[RBig; 3], r1: &[RBig; 3], r2: &[RBig; 3]| -> RBig {
                    dot(r0, &cross(r1, r2))
                };
                let det = det_rows(&n0, &n1, &n2);
                assert!(
                    det != RBig::ZERO,
                    "exact_coords: TPI planes are not in general position (det == 0) — bad fixture"
                );
                let rhs = [d0, d1, d2];
                let sub_col = |k: usize| -> [[RBig; 3]; 3] {
                    let mut rows = [n0.clone(), n1.clone(), n2.clone()];
                    rows[0][k] = rhs[0].clone();
                    rows[1][k] = rhs[1].clone();
                    rows[2][k] = rhs[2].clone();
                    rows
                };
                let mx = sub_col(0);
                let my = sub_col(1);
                let mz = sub_col(2);
                let det_x = det_rows(&mx[0], &mx[1], &mx[2]);
                let det_y = det_rows(&my[0], &my[1], &my[2]);
                let det_z = det_rows(&mz[0], &mz[1], &mz[2]);
                [&det_x / &det, &det_y / &det, &det_z / &det]
            }
        }
    }

    /// Exact signed area (× 2) of a triangle projected to (x, y) per `Plane::XY`.
    fn exact_signed_area2_xy(a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> RBig {
        let bx_ax = &b[0] - &a[0];
        let by_ay = &b[1] - &a[1];
        let cx_ax = &c[0] - &a[0];
        let cy_ay = &c[1] - &a[1];
        &(&bx_ax * &cy_ay) - &(&by_ay * &cx_ax)
    }

    /// Submesh vertex id matching a target `VertexCoords` by EXACT coordinates
    /// (dashu-based; located for `Tpi` lookups). Copied from retriangulate.rs.
    fn find_vert_by_exact(subm: &FastTrimesh, target: &VertexCoords) -> Option<u32> {
        let want = exact_coords(target);
        (0..subm.num_verts()).find(|&v| exact_coords(subm.vert_coords(v)) == want)
    }

    /// Submesh vertex id carrying exactly `VertexCoords::Explicit(p)` (FFI-free,
    /// no dashu). Used to resolve the explicit fixture vertices.
    fn find_explicit_vert(subm: &FastTrimesh, p: Point3) -> Option<u32> {
        (0..subm.num_verts()).find(|&v| *subm.vert_coords(v) == VertexCoords::Explicit(p))
    }

    /// True iff the submesh has an edge between vertex ids `a` and `b` AND that
    /// edge is constraint-flagged.
    fn edge_is_constr_between(subm: &FastTrimesh, a: u32, b: u32) -> bool {
        subm.edge_id(a, b).is_some_and(|e| subm.edge_is_constr(e))
    }

    // ════════════════════════════════════════════════════════════════
    // Test 1 — a segment already an edge → flagged, NO new vertex.
    //          (oracle 4, hand-case (c))
    // ════════════════════════════════════════════════════════════════

    /// Insert ONE interior Explicit point P=(1,1,0) (fan into 3) so the spoke
    /// edge (A0, P) already exists. Enforcing the segment (A0, P) must flag that
    /// existing edge and add NO new vertex (no spurious TPI).
    #[test]
    fn segment_already_an_edge_flags_no_new_vertex() {
        crate::arrangements::require_ffi_shim();
        let a = xy_triangle_a();
        let p = Point3::new(1.0, 1.0, 0.0);
        let mut subm = one_tri(a[0], a[1], a[2]);
        split_single_triangle(&mut subm, &[tp_explicit(p)]).expect("interior split must succeed");

        let v_p = find_explicit_vert(&subm, p).expect("P must be a vertex");
        let v_a0 = find_explicit_vert(&subm, a[0]).expect("A0 must be a vertex");
        // The fan spoke (A0, P) exists.
        assert!(
            subm.edge_id(v_a0, v_p).is_some(),
            "fan spoke edge (A0, P) must exist after the interior split"
        );

        let nverts_before = subm.num_verts();

        // source_tri: any plane through the segment endpoints lifted out of z=0.
        enforce_constraint_segments(
            &mut subm,
            &[SegmentSpec {
                v0: v_a0,
                v1: v_p,
                source_tri: [a[0], p, Point3::new(0.0, 0.0, 5.0)],
            }],
        )
        .expect("flagging an already-present edge must succeed");

        // No new vertex (no spurious TPI).
        assert_eq!(
            subm.num_verts(),
            nverts_before,
            "flagging an existing edge must NOT add a vertex"
        );
        // The edge (A0, P) is now constrained.
        assert!(
            edge_is_constr_between(&subm, v_a0, v_p),
            "the existing edge (A0, P) must be constraint-flagged"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Test 2 — T-junction: segment through an existing interior vertex.
    //          (oracle 5 case (b))
    // ════════════════════════════════════════════════════════════════

    /// Insert TWO interior collinear points M=(1,1,0) and P=(1.5,1.5,0) (both on
    /// y=x, both interior). Enforcing the segment A0→P passes through the
    /// existing vertex M, so it splits at M (no TPI / no new vertex) and is
    /// realized as the constraint chain (A0,M) + (M,P).
    #[test]
    fn t_junction_segment_through_interior_vertex() {
        crate::arrangements::require_ffi_shim();
        let a = xy_triangle_a();
        let m = Point3::new(1.0, 1.0, 0.0);
        let p = Point3::new(1.5, 1.5, 0.0);
        let mut subm = one_tri(a[0], a[1], a[2]);
        split_single_triangle(&mut subm, &[tp_explicit(m), tp_explicit(p)])
            .expect("two interior splits must succeed");

        let v_a0 = find_explicit_vert(&subm, a[0]).expect("A0 vertex");
        let v_m = find_explicit_vert(&subm, m).expect("M vertex");
        let v_p = find_explicit_vert(&subm, p).expect("P vertex");

        let nverts_before = subm.num_verts();

        // Source plane = y=x lifted out of z=0.
        enforce_constraint_segments(
            &mut subm,
            &[SegmentSpec {
                v0: v_a0,
                v1: v_p,
                source_tri: [a[0], p, Point3::new(1.0, 1.0, 5.0)],
            }],
        )
        .expect("T-junction enforcement must succeed");

        // A T-junction splits the segment at the existing vertex M — no TPI.
        assert_eq!(
            subm.num_verts(),
            nverts_before,
            "T-junction through an existing vertex must NOT add a vertex"
        );

        // The chain is realized as constraint edges through M.
        assert!(
            edge_is_constr_between(&subm, v_a0, v_m),
            "sub-edge (A0, M) must be a constraint edge"
        );
        assert!(
            edge_is_constr_between(&subm, v_m, v_p),
            "sub-edge (M, P) must be a constraint edge"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Test 3 — X-crossing of two transversal segments → ONE TPI on 3 planes.
    //          (oracles 2, 4, 5 case (a)) — THE load-bearing test.
    // ════════════════════════════════════════════════════════════════

    // S1 source plane (x=1) third generator.
    fn s1_third() -> Point3 {
        Point3::new(1.0, 0.0, 5.0)
    }
    // S2 source plane (y=1) third generator.
    fn s2_third() -> Point3 {
        Point3::new(0.0, 1.0, 5.0)
    }

    /// Two constraint segments crossing at the interior point (1,1,0):
    ///   S1 (line x=1): s1a=(1,0,0) [on edge A0A1], s1b=(1,3,0) [on edge A1A2].
    ///   S2 (line y=1): s2a=(0,1,0) [on edge A2A0], s2b=(3,1,0) [on edge A1A2].
    /// Their crossing (1,1,0) is the base(z=0) ∩ x=1 ∩ y=1 TPI.
    ///
    /// Enforcing both in one call must add EXACTLY one new vertex (a `Tpi` at
    /// (1,1,0)) and realize both segments as constraint chains through it.
    #[test]
    fn x_crossing_creates_one_tpi_on_three_planes() {
        use indirect_predicates_sidecar_rs::{
            init_fpu, orient3d, ExplicitPoint3D, ImplicitPoint3DTpi, Sign as IpSign, AVAILABLE,
        };

        if !AVAILABLE {
            panic!(
                "indirect-predicates FFI shim not linked (AVAILABLE == false); \
                 the X-crossing TPI oracle cannot run — refusing to pass silently"
            );
        }
        init_fpu();

        let a = xy_triangle_a();
        let s1a = Point3::new(1.0, 0.0, 0.0);
        let s1b = Point3::new(1.0, 3.0, 0.0);
        let s2a = Point3::new(0.0, 1.0, 0.0);
        let s2b = Point3::new(3.0, 1.0, 0.0);

        let mut subm = one_tri(a[0], a[1], a[2]);
        // Insert the four on-edge endpoint points first.
        split_single_triangle(
            &mut subm,
            &[
                tp_explicit(s1a),
                tp_explicit(s1b),
                tp_explicit(s2a),
                tp_explicit(s2b),
            ],
        )
        .expect("inserting the four on-edge endpoints must succeed");

        let v_s1a = find_explicit_vert(&subm, s1a).expect("s1a vertex");
        let v_s1b = find_explicit_vert(&subm, s1b).expect("s1b vertex");
        let v_s2a = find_explicit_vert(&subm, s2a).expect("s2a vertex");
        let v_s2b = find_explicit_vert(&subm, s2b).expect("s2b vertex");

        let nverts_before = subm.num_verts();

        enforce_constraint_segments(
            &mut subm,
            &[
                SegmentSpec {
                    v0: v_s1a,
                    v1: v_s1b,
                    source_tri: [s1a, s1b, s1_third()],
                },
                SegmentSpec {
                    v0: v_s2a,
                    v1: v_s2b,
                    source_tri: [s2a, s2b, s2_third()],
                },
            ],
        )
        .expect("X-crossing enforcement must succeed");

        // Exactly ONE new vertex (the TPI at the crossing).
        assert_eq!(
            subm.num_verts(),
            nverts_before + 1,
            "an X-crossing of two segments must add exactly one TPI vertex"
        );

        // Locate the TPI vertex robustly: exact coords == (1,1,0) AND it is a Tpi.
        let want = [to_r(1.0), to_r(1.0), to_r(0.0)];
        let is_tpi_at_crossing = |v: u32| -> bool {
            matches!(subm.vert_coords(v), VertexCoords::Tpi { .. })
                && exact_coords(subm.vert_coords(v)) == want
        };
        let tpi_vid = (0..subm.num_verts())
            .find(|&v| is_tpi_at_crossing(v))
            .expect("a Tpi vertex at the exact crossing (1,1,0) must have been inserted");

        // (Oracle 2) TPI exactness — EXACT orient3d == Zero on ALL THREE planes.
        // Read back the stored generators (must be a Tpi) and build a real handle.
        let (gv, gw, gu) = match subm.vert_coords(tpi_vid) {
            VertexCoords::Tpi { v, w, u } => (*v, *w, *u),
            other => panic!("TPI vertex must store VertexCoords::Tpi, got {other:?}"),
        };
        let ip = |p: Point3| ExplicitPoint3D::new(p.x(), p.y(), p.z());
        let (gv0, gv1, gv2) = (ip(gv[0]), ip(gv[1]), ip(gv[2]));
        let (gw0, gw1, gw2) = (ip(gw[0]), ip(gw[1]), ip(gw[2]));
        let (gu0, gu1, gu2) = (ip(gu[0]), ip(gu[1]), ip(gu[2]));
        let tpi = ImplicitPoint3DTpi::new(&gv0, &gv1, &gv2, &gw0, &gw1, &gw2, &gu0, &gu1, &gu2);

        // Base triangle A (z=0).
        let (ea0, ea1, ea2) = (ip(a[0]), ip(a[1]), ip(a[2]));
        // S1 plane (x=1).
        let (e_s1_0, e_s1_1, e_s1_2) = (ip(s1a), ip(s1b), ip(s1_third()));
        // S2 plane (y=1).
        let (e_s2_0, e_s2_1, e_s2_2) = (ip(s2a), ip(s2b), ip(s2_third()));

        assert_eq!(
            orient3d(&ea0, &ea1, &ea2, &tpi),
            IpSign::Zero,
            "TPI must lie exactly on the base plane A (z=0)"
        );
        assert_eq!(
            orient3d(&e_s1_0, &e_s1_1, &e_s1_2, &tpi),
            IpSign::Zero,
            "TPI must lie exactly on segment S1's source plane (x=1)"
        );
        assert_eq!(
            orient3d(&e_s2_0, &e_s2_1, &e_s2_2, &tpi),
            IpSign::Zero,
            "TPI must lie exactly on segment S2's source plane (y=1)"
        );

        // (Oracle 1 / case (a)) Both segments realized through the TPI. Count
        // constraint-flagged edges incident to the TPI vertex.
        //
        // Confidence note: the X-crossing splits BOTH segments at the TPI, so the
        // TPI vertex has four constraint half-edges meeting at it — one toward
        // each of the four endpoints' sides (s1a, s1b, s2a, s2b). The exact "4"
        // is what the spec's case (a) ("both segments realized") forces for two
        // transversal segments crossing at a single interior point: each segment
        // contributes its two sub-edges incident to the crossing. There is no
        // collinearity that would merge any pair, so 4 is the forced count. I
        // assert `>= 4` (rather than `== 4`) to stay robust to GREEN also
        // flagging additional incident edges that happen to be collinear sub-
        // pieces, while still being the strongest count the spec guarantees.
        let mut constr_incident = 0usize;
        for other in 0..subm.num_verts() {
            if other == tpi_vid {
                continue;
            }
            if let Some(e) = subm.edge_id(tpi_vid, other) {
                if subm.edge_is_constr(e) {
                    constr_incident += 1;
                }
            }
        }
        assert!(
            constr_incident >= 4,
            "the TPI vertex must have >= 4 constraint-flagged incident edges \
             (both crossing segments realized through it), got {constr_incident}"
        );

        // Additionally, each segment line must reach the TPI as a constraint
        // edge directly from its on-edge endpoints (no segment crossing a
        // non-vertex edge interior). The direct endpoint→TPI edges exist & are
        // constrained for all four endpoints.
        for &endpoint in &[v_s1a, v_s1b, v_s2a, v_s2b] {
            assert!(
                edge_is_constr_between(&subm, endpoint, tpi_vid),
                "each segment endpoint must connect to the TPI by a constraint edge"
            );
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Test 4 — X-crossing exact covering sub-triangulation (oracle 3, dashu).
    // ════════════════════════════════════════════════════════════════

    /// Same X-crossing fixture/enforcement as Test 3. After enforcement the
    /// submesh must still tile base A EXACTLY: every sub-tri shares the base
    /// winding sign and the exact (`RBig`) signed areas sum EXACTLY to the
    /// base's; no degenerate sub-tri. Pure `RBig`, independent of the FFI path.
    #[test]
    fn x_crossing_exact_covering_subtriangulation() {
        crate::arrangements::require_ffi_shim();
        let a = xy_triangle_a();
        let s1a = Point3::new(1.0, 0.0, 0.0);
        let s1b = Point3::new(1.0, 3.0, 0.0);
        let s2a = Point3::new(0.0, 1.0, 0.0);
        let s2b = Point3::new(3.0, 1.0, 0.0);

        let mut subm = one_tri(a[0], a[1], a[2]);
        split_single_triangle(
            &mut subm,
            &[
                tp_explicit(s1a),
                tp_explicit(s1b),
                tp_explicit(s2a),
                tp_explicit(s2b),
            ],
        )
        .expect("inserting the four on-edge endpoints must succeed");

        let v_s1a = find_explicit_vert(&subm, s1a).expect("s1a vertex");
        let v_s1b = find_explicit_vert(&subm, s1b).expect("s1b vertex");
        let v_s2a = find_explicit_vert(&subm, s2a).expect("s2a vertex");
        let v_s2b = find_explicit_vert(&subm, s2b).expect("s2b vertex");

        enforce_constraint_segments(
            &mut subm,
            &[
                SegmentSpec {
                    v0: v_s1a,
                    v1: v_s1b,
                    source_tri: [s1a, s1b, s1_third()],
                },
                SegmentSpec {
                    v0: v_s2a,
                    v1: v_s2b,
                    source_tri: [s2a, s2b, s2_third()],
                },
            ],
        )
        .expect("X-crossing enforcement must succeed");

        let ba = exact_coords(&VertexCoords::Explicit(a[0]));
        let bb = exact_coords(&VertexCoords::Explicit(a[1]));
        let bc = exact_coords(&VertexCoords::Explicit(a[2]));
        let base_area2 = exact_signed_area2_xy(&ba, &bb, &bc);
        assert!(
            base_area2 != RBig::ZERO,
            "base triangle must be non-degenerate"
        );
        let base_positive = base_area2 > RBig::ZERO;

        let mut sum = RBig::ZERO;
        for t in 0..subm.num_tris() {
            let v0 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 0)));
            let v1 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 1)));
            let v2 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 2)));
            let area2 = exact_signed_area2_xy(&v0, &v1, &v2);
            assert!(
                area2 != RBig::ZERO,
                "sub-tri {t} is degenerate (exact zero area)"
            );
            assert_eq!(
                area2 > RBig::ZERO,
                base_positive,
                "sub-tri {t} winding sign disagrees with base (flip)"
            );
            sum = &sum + &area2;
        }
        assert_eq!(
            sum, base_area2,
            "post-enforcement sub-tri signed areas must sum EXACTLY to the base \
             (covering, no gaps/overlaps)"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Test 5 — the AR2b integration path (`enforce_constraints` adapter).
    // ════════════════════════════════════════════════════════════════

    /// The AR1/AR2a tilted transversal fixture's B triangle.
    fn tilted_b() -> [Point3; 3] {
        [
            Point3::new(1.0, 1.0, -1.0),
            Point3::new(1.5, 0.5, 1.0),
            Point3::new(0.5, 1.5, 1.0),
        ]
    }

    /// Build the 2-triangle soup (A = index 0, B = index 1).
    fn soup_pair(a: [Point3; 3], b: [Point3; 3]) -> FastTrimesh {
        let verts = vec![a[0], a[1], a[2], b[0], b[1], b[2]];
        let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
        FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap()
    }

    /// Drive the `enforce_constraints` adapter end-to-end from real AR2b output:
    /// resolve interned-id endpoints → submesh vertices, enforce, and assert A's
    /// one constraint segment is realized as a constraint-flagged edge between
    /// the two submesh vertices carrying the endpoint coords.
    #[test]
    fn enforce_constraints_adapter_resolves_interned_endpoints() {
        crate::arrangements::require_ffi_shim();
        let a = xy_triangle_a();
        let b = tilted_b();
        let soup = soup_pair(a, b);

        let pairs = detect_intersecting_pairs(&soup);
        let classified = classify_all(&soup, &pairs);
        let (points, buckets) = group_intersection_points(&soup, &classified);
        // AR3c: Result-returning (>2 geometric endpoints is loud — C++
        // final_check); these fixtures are clean 2-endpoint crossings.
        let seg_lists = group_constraint_segments(&soup, &classified, &points)
            .expect("clean transversal fixture must not over-count endpoints");

        // A's constraint-segment list must have exactly one segment.
        assert_eq!(
            seg_lists[0].len(),
            1,
            "triangle A must have exactly one constraint segment, got {:?}",
            seg_lists[0]
        );
        let seg = &seg_lists[0][0];

        // Build the base-triangle-A submesh and insert triangle A's bucket
        // points (interior ++ all three edges), deduped, preserving the global
        // `points` coords.
        let mut subm = one_tri(a[0], a[1], a[2]);
        let aux_a = &buckets[0];
        let mut ids: Vec<u32> = Vec::new();
        for &id in aux_a.interior.iter() {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        for edge in aux_a.edges.iter() {
            for &id in edge.iter() {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        let insert_pts: Vec<TypedPoint> =
            ids.iter().map(|&id| points[id as usize].clone()).collect();
        split_single_triangle(&mut subm, &insert_pts).expect("inserting A's bucket points");

        // Enforce A's constraint segments via the adapter.
        enforce_constraints(&mut subm, &seg_lists[0], &points)
            .expect("adapter enforcement over A's segments must succeed");

        // A's one constraint segment is realized: a constraint-flagged edge
        // between the two submesh vertices carrying the endpoint coords.
        let c0 = &points[seg.endpoints.0 as usize].coords;
        let c1 = &points[seg.endpoints.1 as usize].coords;
        let ev0 = find_vert_by_exact(&subm, c0)
            .expect("segment endpoint 0 must resolve to a submesh vertex");
        let ev1 = find_vert_by_exact(&subm, c1)
            .expect("segment endpoint 1 must resolve to a submesh vertex");
        assert!(
            edge_is_constr_between(&subm, ev0, ev1),
            "A's constraint segment must be realized as a constraint-flagged edge"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // ── PR-CR-AR3a Adversary ──────────────────────────────────────
    //
    // Independent re-verification of the GREEN exactness claims and an
    // attempt to break the implementation with pathological inputs. All
    // probes drive the PUBLIC surface (`enforce_constraint_segments` /
    // `enforce_constraints`). NO production code is modified by this block.
    //
    // Background re-verified directly against the upstream C++
    // (`implicit_point.hpp`, vendored at /home/claude/cherchi2022/...):
    //   * `pointInInnerSegment` (cpp:1103) routes through `lessThanOnX/Y/Z`,
    //     whose explicit-explicit branch (cpp:73/83/93) returns the C++ `bool`
    //     `a.X() < b.X()` — 0 or 1, NEVER −1. For a DESCENDING explicit segment
    //     `lt2 = lessThanOnX(v1, p)` is 0, the branch is skipped, and a segment
    //     purely along that descending axis returns `false`. This confirms the
    //     EE endpoint-order asymmetry GREEN's `point_inside_segment` repairs by
    //     OR-ing both endpoint orders.
    //   * `innerSegmentsCross` (cpp:1038), by contrast, routes through
    //     `orient2Dxy/yz/zx` whose EE branch is `orient2d_EEE` (cpp:137) — a real
    //     signed Shewchuk determinant (−1/0/+1). Its decision `o11==o12 &&
    //     o21==o22` is fully sign-aware and symmetric under A↔B / P↔Q swaps, so
    //     it does NOT share the EE asymmetry. Probe 2 confirms this empirically
    //     (a reversed-order crossing still yields exactly one correct TPI).
    // ════════════════════════════════════════════════════════════════

    // ── Probe 1: segment coincident with an existing BASE edge → flag only ──

    /// A constraint segment lying exactly along a base-triangle edge (A0,A1),
    /// distinct from the existing `segment_already_an_edge_flags_no_new_vertex`
    /// fixture (which uses a fan spoke born of an interior split). No points are
    /// inserted, so edge (A0,A1) is a pristine base edge. Enforcing it must flag
    /// that edge and add NO vertex and NO TPI (oracle 4).
    #[test]
    fn adv_segment_coincident_with_base_edge_flags_no_new_vertex() {
        let a = xy_triangle_a();
        let mut subm = one_tri(a[0], a[1], a[2]);

        let v_a0 = find_explicit_vert(&subm, a[0]).expect("A0 vertex");
        let v_a1 = find_explicit_vert(&subm, a[1]).expect("A1 vertex");
        assert!(
            subm.edge_id(v_a0, v_a1).is_some(),
            "base edge (A0,A1) must exist in a bare 1-tri submesh"
        );
        let nverts_before = subm.num_verts();
        let ntris_before = subm.num_tris();

        enforce_constraint_segments(
            &mut subm,
            &[SegmentSpec {
                v0: v_a0,
                v1: v_a1,
                // A plane through the base edge, lifted out of z=0.
                source_tri: [a[0], a[1], Point3::new(2.0, 0.0, 5.0)],
            }],
        )
        .expect("flagging a base edge must succeed");

        assert_eq!(
            subm.num_verts(),
            nverts_before,
            "flagging an existing base edge must NOT add a vertex"
        );
        assert_eq!(
            subm.num_tris(),
            ntris_before,
            "flagging an existing base edge must NOT re-triangulate"
        );
        assert!(
            edge_is_constr_between(&subm, v_a0, v_a1),
            "the base edge (A0,A1) must be constraint-flagged"
        );
    }

    // ── Probe 2: reversed-order X-crossing (the EE-asymmetry probe) ──

    /// THE highest-value probe. Identical fixture to Test 3's X-crossing, but
    /// BOTH segments are given with their endpoints SWAPPED (descending v0/v1)
    /// AND the spec order is reversed (S2 before S1). If the
    /// `inner_segments_cross` / `segmentsIntersectInside` path shared the EE
    /// `lessThanOn*` asymmetry, a descending/reversed crossing could miss the
    /// crossing, place the TPI wrong, or error. The reference analysis says it
    /// does NOT (it uses sign-aware `orient2d`). Verify exactly ONE Tpi at the
    /// exact (1,1,0), on all three planes via EXACT orient3d == Zero, and an
    /// exact covering. ANY ordering breakage here is a real defect → STOP.
    #[test]
    fn adv_reversed_order_x_crossing_one_tpi_on_three_planes() {
        use indirect_predicates_sidecar_rs::{
            init_fpu, orient3d, ExplicitPoint3D, ImplicitPoint3DTpi, Sign as IpSign, AVAILABLE,
        };

        if !AVAILABLE {
            panic!(
                "indirect-predicates FFI shim not linked (AVAILABLE == false); \
                 the reversed-order X-crossing probe cannot run — refusing to pass silently"
            );
        }
        init_fpu();

        let a = xy_triangle_a();
        let s1a = Point3::new(1.0, 0.0, 0.0);
        let s1b = Point3::new(1.0, 3.0, 0.0);
        let s2a = Point3::new(0.0, 1.0, 0.0);
        let s2b = Point3::new(3.0, 1.0, 0.0);

        let mut subm = one_tri(a[0], a[1], a[2]);
        split_single_triangle(
            &mut subm,
            &[
                tp_explicit(s1a),
                tp_explicit(s1b),
                tp_explicit(s2a),
                tp_explicit(s2b),
            ],
        )
        .expect("inserting the four on-edge endpoints must succeed");

        let v_s1a = find_explicit_vert(&subm, s1a).expect("s1a vertex");
        let v_s1b = find_explicit_vert(&subm, s1b).expect("s1b vertex");
        let v_s2a = find_explicit_vert(&subm, s2a).expect("s2a vertex");
        let v_s2b = find_explicit_vert(&subm, s2b).expect("s2b vertex");

        let nverts_before = subm.num_verts();

        // Reversed: endpoints swapped (v0=b, v1=a) AND S2 listed before S1.
        enforce_constraint_segments(
            &mut subm,
            &[
                SegmentSpec {
                    v0: v_s2b,
                    v1: v_s2a,
                    source_tri: [s2a, s2b, s2_third()],
                },
                SegmentSpec {
                    v0: v_s1b,
                    v1: v_s1a,
                    source_tri: [s1a, s1b, s1_third()],
                },
            ],
        )
        .expect("reversed-order X-crossing enforcement must succeed");

        // Exactly ONE new vertex — the TPI at the crossing — regardless of order.
        assert_eq!(
            subm.num_verts(),
            nverts_before + 1,
            "reversed-order X-crossing must STILL add exactly one TPI vertex"
        );

        let want = [to_r(1.0), to_r(1.0), to_r(0.0)];
        let tpi_vid = (0..subm.num_verts())
            .find(|&v| {
                matches!(subm.vert_coords(v), VertexCoords::Tpi { .. })
                    && exact_coords(subm.vert_coords(v)) == want
            })
            .expect("a Tpi at the exact crossing (1,1,0) must exist for reversed order");

        // EXACT orient3d == Zero on all three supporting planes.
        let (gv, gw, gu) = match subm.vert_coords(tpi_vid) {
            VertexCoords::Tpi { v, w, u } => (*v, *w, *u),
            other => panic!("TPI vertex must store VertexCoords::Tpi, got {other:?}"),
        };
        let ip = |p: Point3| ExplicitPoint3D::new(p.x(), p.y(), p.z());
        let (gv0, gv1, gv2) = (ip(gv[0]), ip(gv[1]), ip(gv[2]));
        let (gw0, gw1, gw2) = (ip(gw[0]), ip(gw[1]), ip(gw[2]));
        let (gu0, gu1, gu2) = (ip(gu[0]), ip(gu[1]), ip(gu[2]));
        let tpi = ImplicitPoint3DTpi::new(&gv0, &gv1, &gv2, &gw0, &gw1, &gw2, &gu0, &gu1, &gu2);
        let (ea0, ea1, ea2) = (ip(a[0]), ip(a[1]), ip(a[2]));
        let (e_s1_0, e_s1_1, e_s1_2) = (ip(s1a), ip(s1b), ip(s1_third()));
        let (e_s2_0, e_s2_1, e_s2_2) = (ip(s2a), ip(s2b), ip(s2_third()));
        assert_eq!(
            orient3d(&ea0, &ea1, &ea2, &tpi),
            IpSign::Zero,
            "reversed: TPI must lie exactly on base plane A (z=0)"
        );
        assert_eq!(
            orient3d(&e_s1_0, &e_s1_1, &e_s1_2, &tpi),
            IpSign::Zero,
            "reversed: TPI must lie exactly on S1's source plane (x=1)"
        );
        assert_eq!(
            orient3d(&e_s2_0, &e_s2_1, &e_s2_2, &tpi),
            IpSign::Zero,
            "reversed: TPI must lie exactly on S2's source plane (y=1)"
        );

        // Each endpoint connects to the TPI by a constraint edge (both realized).
        for &endpoint in &[v_s1a, v_s1b, v_s2a, v_s2b] {
            assert!(
                edge_is_constr_between(&subm, endpoint, tpi_vid),
                "reversed: each endpoint must connect to the TPI by a constraint edge"
            );
        }

        // Exact covering: sub-tri signed areas sum exactly to the base, same sign.
        let ba = exact_coords(&VertexCoords::Explicit(a[0]));
        let bb = exact_coords(&VertexCoords::Explicit(a[1]));
        let bc = exact_coords(&VertexCoords::Explicit(a[2]));
        let base_area2 = exact_signed_area2_xy(&ba, &bb, &bc);
        let base_positive = base_area2 > RBig::ZERO;
        let mut sum = RBig::ZERO;
        for t in 0..subm.num_tris() {
            let v0 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 0)));
            let v1 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 1)));
            let v2 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 2)));
            let area2 = exact_signed_area2_xy(&v0, &v1, &v2);
            assert!(area2 != RBig::ZERO, "reversed: sub-tri {t} degenerate");
            assert_eq!(
                area2 > RBig::ZERO,
                base_positive,
                "reversed: sub-tri {t} winding flip"
            );
            sum = &sum + &area2;
        }
        assert_eq!(
            sum, base_area2,
            "reversed-order enforcement must still tile the base exactly"
        );
    }

    // ── Probe 3: two independent interior crossings → two TPIs (or AR3b wall) ──

    /// Three mutually transversal constraint segments producing TWO distinct
    /// interior crossings:
    ///   V  (line x=1): from (1,0,0) to (1,3,0)            — plane x=1
    ///   H1 (line y=0.5): from (0,0.5,0) to (3,0.5,0)      — plane y=0.5,
    ///        crosses V at (1,0.5,0)
    ///   H2 (line y=2): from (0,2,0) to (2,2,0)            — plane y=2,
    ///        crosses V at (1,2,0)
    /// Both crossings are strictly interior to base A {x>0,y>0,x+y<4}:
    ///   (1,0.5): 1.5<4 ok;  (1,2): 3<4 ok.
    /// Each crossing's third plane (the crossed edge's plane) is a sub-edge of V
    /// whose plane was recorded when V's first crossing was enforced, so the
    /// in-scope path SHOULD resolve both. If a second crossing trips the AR3b
    /// wall (`SourcePlaneUnavailable` / `DegenerateTpi`), this asserts the
    /// specific error and documents the deferral rather than asserting success.
    #[test]
    fn adv_two_interior_crossings_two_tpis_or_ar3b_wall() {
        use indirect_predicates_sidecar_rs::AVAILABLE;
        if !AVAILABLE {
            panic!("FFI shim not linked; multi-crossing probe cannot run");
        }

        let a = xy_triangle_a();
        let v0 = Point3::new(1.0, 0.0, 0.0);
        let v1 = Point3::new(1.0, 3.0, 0.0);
        let h1a = Point3::new(0.0, 0.5, 0.0);
        let h1b = Point3::new(3.0, 0.5, 0.0);
        let h2a = Point3::new(0.0, 2.0, 0.0);
        let h2b = Point3::new(2.0, 2.0, 0.0);

        let mut subm = one_tri(a[0], a[1], a[2]);
        split_single_triangle(
            &mut subm,
            &[
                tp_explicit(v0),
                tp_explicit(v1),
                tp_explicit(h1a),
                tp_explicit(h1b),
                tp_explicit(h2a),
                tp_explicit(h2b),
            ],
        )
        .expect("inserting the six on-edge endpoints must succeed");

        let r = |p: Point3| find_explicit_vert(&subm, p).expect("endpoint vertex");
        let (vv0, vv1) = (r(v0), r(v1));
        let (vh1a, vh1b) = (r(h1a), r(h1b));
        let (vh2a, vh2b) = (r(h2a), r(h2b));

        let nverts_before = subm.num_verts();

        let result = enforce_constraint_segments(
            &mut subm,
            &[
                SegmentSpec {
                    v0: vv0,
                    v1: vv1,
                    source_tri: [v0, v1, Point3::new(1.0, 0.0, 5.0)],
                },
                SegmentSpec {
                    v0: vh1a,
                    v1: vh1b,
                    source_tri: [h1a, h1b, Point3::new(0.0, 0.5, 5.0)],
                },
                SegmentSpec {
                    v0: vh2a,
                    v1: vh2b,
                    source_tri: [h2a, h2b, Point3::new(0.0, 2.0, 5.0)],
                },
            ],
        );

        match result {
            Ok(()) => {
                // Count the genuine interior crossings realized as Tpi vertices.
                let want_lo = [to_r(1.0), to_r(0.5), to_r(0.0)];
                let want_hi = [to_r(1.0), to_r(2.0), to_r(0.0)];
                let mut tpi_lo = None;
                let mut tpi_hi = None;
                let mut tpi_count = 0usize;
                for v in 0..subm.num_verts() {
                    if matches!(subm.vert_coords(v), VertexCoords::Tpi { .. }) {
                        tpi_count += 1;
                        let ec = exact_coords(subm.vert_coords(v));
                        if ec == want_lo {
                            tpi_lo = Some(v);
                        } else if ec == want_hi {
                            tpi_hi = Some(v);
                        }
                    }
                }
                assert_eq!(
                    tpi_count, 2,
                    "two interior crossings must create exactly two Tpi vertices, got {tpi_count}"
                );
                assert_eq!(
                    subm.num_verts(),
                    nverts_before + 2,
                    "two crossings must add exactly two vertices"
                );
                let tpi_lo = tpi_lo.expect("a Tpi at (1,0.5,0) must exist");
                let tpi_hi = tpi_hi.expect("a Tpi at (1,2,0) must exist");

                // Each crossing exactly on its three planes (base + the two
                // crossing segments). Recompute from stored generators.
                exact_tpi_on_three_planes(&subm, tpi_lo);
                exact_tpi_on_three_planes(&subm, tpi_hi);

                // Exact covering still tiles base A.
                assert_exact_covering(&subm, a);
            }
            Err(EnforceError::SourcePlaneUnavailable { .. }) | Err(EnforceError::DegenerateTpi) => {
                // Documented AR3b wall: a second crossing whose crossed-edge
                // plane is not directly available halts the cycle. This is the
                // correct deferral, not a defect. Assert the specific error
                // variant so a future regression (e.g. a silent wrong answer)
                // is caught.
            }
            Err(other) => panic!(
                "multi-crossing produced an unexpected error variant: {other:?} \
                 (expected Ok with two TPIs, or the documented AR3b wall)"
            ),
        }
    }

    /// Recompute and assert (EXACT orient3d == Zero) that the `Tpi` vertex
    /// `tpi_vid` lies on each of its three stored supporting planes.
    fn exact_tpi_on_three_planes(subm: &FastTrimesh, tpi_vid: u32) {
        use indirect_predicates_sidecar_rs::{
            orient3d, ExplicitPoint3D, ImplicitPoint3DTpi, Sign as IpSign,
        };
        let (gv, gw, gu) = match subm.vert_coords(tpi_vid) {
            VertexCoords::Tpi { v, w, u } => (*v, *w, *u),
            other => panic!("expected Tpi, got {other:?}"),
        };
        let ip = |p: Point3| ExplicitPoint3D::new(p.x(), p.y(), p.z());
        let g = |t: [Point3; 3]| (ip(t[0]), ip(t[1]), ip(t[2]));
        let (gv0, gv1, gv2) = g(gv);
        let (gw0, gw1, gw2) = g(gw);
        let (gu0, gu1, gu2) = g(gu);
        let tpi = ImplicitPoint3DTpi::new(&gv0, &gv1, &gv2, &gw0, &gw1, &gw2, &gu0, &gu1, &gu2);
        for tri in [gv, gw, gu] {
            let (p0, p1, p2) = (ip(tri[0]), ip(tri[1]), ip(tri[2]));
            assert_eq!(
                orient3d(&p0, &p1, &p2, &tpi),
                IpSign::Zero,
                "Tpi must lie exactly on each stored supporting plane"
            );
        }
    }

    /// Assert (pure dashu) that the submesh exactly tiles base triangle `a`.
    fn assert_exact_covering(subm: &FastTrimesh, a: [Point3; 3]) {
        let ba = exact_coords(&VertexCoords::Explicit(a[0]));
        let bb = exact_coords(&VertexCoords::Explicit(a[1]));
        let bc = exact_coords(&VertexCoords::Explicit(a[2]));
        let base_area2 = exact_signed_area2_xy(&ba, &bb, &bc);
        let base_positive = base_area2 > RBig::ZERO;
        let mut sum = RBig::ZERO;
        for t in 0..subm.num_tris() {
            let p0 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 0)));
            let p1 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 1)));
            let p2 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 2)));
            let area2 = exact_signed_area2_xy(&p0, &p1, &p2);
            assert!(area2 != RBig::ZERO, "sub-tri {t} degenerate");
            assert_eq!(
                area2 > RBig::ZERO,
                base_positive,
                "sub-tri {t} winding flip"
            );
            sum = &sum + &area2;
        }
        assert_eq!(sum, base_area2, "sub-tris must tile base exactly");
    }

    // ── Probe 4: degenerate / collinear pathologies → no spurious TPI ──

    /// (a) Two segments that SHARE an endpoint (meet at a common interior
    /// vertex), not a transversal X. `inner_segments_cross` must reject this
    /// (shared-endpoint is excluded), so NO TPI is created. Both segments are
    /// realized as constraint chains through the shared vertex.
    #[test]
    fn adv_shared_endpoint_creates_no_tpi() {
        use indirect_predicates_sidecar_rs::AVAILABLE;
        if !AVAILABLE {
            panic!("FFI shim not linked; shared-endpoint probe cannot run");
        }

        let a = xy_triangle_a();
        // Shared interior vertex M; two arms going to distinct on-edge points.
        let m = Point3::new(1.0, 1.0, 0.0);
        let arm1 = Point3::new(2.0, 0.0, 0.0); // on edge A0A1 (y=0)
        let arm2 = Point3::new(0.0, 2.0, 0.0); // on edge A0A2 (x=0)

        let mut subm = one_tri(a[0], a[1], a[2]);
        split_single_triangle(
            &mut subm,
            &[tp_explicit(m), tp_explicit(arm1), tp_explicit(arm2)],
        )
        .expect("inserting M + two arm endpoints must succeed");

        let v_m = find_explicit_vert(&subm, m).expect("M vertex");
        let v_a1 = find_explicit_vert(&subm, arm1).expect("arm1 vertex");
        let v_a2 = find_explicit_vert(&subm, arm2).expect("arm2 vertex");

        let nverts_before = subm.num_verts();

        enforce_constraint_segments(
            &mut subm,
            &[
                SegmentSpec {
                    v0: v_m,
                    v1: v_a1,
                    source_tri: [m, arm1, Point3::new(1.0, 1.0, 5.0)],
                },
                SegmentSpec {
                    v0: v_m,
                    v1: v_a2,
                    source_tri: [m, arm2, Point3::new(1.0, 1.0, 7.0)],
                },
            ],
        )
        .expect("two segments sharing an endpoint must enforce without a crossing");

        assert_eq!(
            subm.num_verts(),
            nverts_before,
            "a shared endpoint is NOT a transversal crossing — no TPI must be created"
        );
        assert!(
            !(0..subm.num_verts()).any(|v| matches!(subm.vert_coords(v), VertexCoords::Tpi { .. })),
            "no Tpi vertex must exist for a shared-endpoint configuration"
        );
        assert!(
            edge_is_constr_between(&subm, v_m, v_a1),
            "arm (M, arm1) must be a constraint edge"
        );
        assert!(
            edge_is_constr_between(&subm, v_m, v_a2),
            "arm (M, arm2) must be a constraint edge"
        );
    }

    /// (b) Collinear-overlap: two segments on the SAME supporting line (y=x),
    /// where the second is contained in the first. `innerSegmentsCross` excludes
    /// collinear-overlap, so no spurious TPI must appear; the shorter segment is
    /// realized as a sub-chain of the longer via T-junction splits at the shared
    /// interior vertices. No panic, no error, no new vertex.
    #[test]
    fn adv_collinear_overlap_no_spurious_tpi() {
        use indirect_predicates_sidecar_rs::AVAILABLE;
        if !AVAILABLE {
            panic!("FFI shim not linked; collinear-overlap probe cannot run");
        }

        let a = xy_triangle_a();
        // Four collinear interior points on y=x: q1<q2<q3<q4.
        let q1 = Point3::new(0.5, 0.5, 0.0);
        let q2 = Point3::new(1.0, 1.0, 0.0);
        let q3 = Point3::new(1.5, 1.5, 0.0);
        let q4 = Point3::new(2.0, 2.0, 0.0);

        let mut subm = one_tri(a[0], a[1], a[2]);
        split_single_triangle(
            &mut subm,
            &[
                tp_explicit(q1),
                tp_explicit(q2),
                tp_explicit(q3),
                tp_explicit(q4),
            ],
        )
        .expect("inserting four collinear interior points must succeed");

        let r = |p: Point3| find_explicit_vert(&subm, p).expect("collinear vertex");
        let (vq1, vq2, vq3, vq4) = (r(q1), r(q2), r(q3), r(q4));

        let nverts_before = subm.num_verts();

        // Long segment q1→q4 and an overlapping inner segment q2→q3 (same line).
        let plane = [q1, q4, Point3::new(1.0, 1.0, 5.0)]; // y=x lifted
        enforce_constraint_segments(
            &mut subm,
            &[
                SegmentSpec {
                    v0: vq1,
                    v1: vq4,
                    source_tri: plane,
                },
                SegmentSpec {
                    v0: vq2,
                    v1: vq3,
                    source_tri: plane,
                },
            ],
        )
        .expect("collinear-overlap segments must enforce without a transversal crossing");

        assert_eq!(
            subm.num_verts(),
            nverts_before,
            "collinear-overlap must NOT add a vertex (no transversal crossing)"
        );
        assert!(
            !(0..subm.num_verts()).any(|v| matches!(subm.vert_coords(v), VertexCoords::Tpi { .. })),
            "no Tpi vertex must exist for a collinear-overlap configuration"
        );
        // The overlap interior chain (q2,q3) is realized as a constraint edge.
        assert!(
            edge_is_constr_between(&subm, vq2, vq3),
            "the overlapping inner segment (q2,q3) must be a constraint edge"
        );
    }

    // ── Probe 5: TPI-exactness re-verification WITH a negative control ──

    /// Forward X-crossing. Locate the TPI by exact coords, re-derive its three
    /// supporting planes from the stored generators, and assert EXACT orient3d
    /// == Zero on all three (positive control — the exactness claim). Then assert
    /// the NEGATIVE control the spec implies: a point perturbed off the crossing
    /// (an ExplicitPoint3D at (1,1,ε)) is NOT on plane z=0. This proves the
    /// exactness assertion has teeth (isn't trivially Zero for everything).
    #[test]
    fn adv_tpi_exactness_with_negative_control() {
        use indirect_predicates_sidecar_rs::{
            init_fpu, orient3d, ExplicitPoint3D, Sign as IpSign, AVAILABLE,
        };
        if !AVAILABLE {
            panic!("FFI shim not linked; TPI-exactness probe cannot run");
        }
        init_fpu();

        let a = xy_triangle_a();
        let s1a = Point3::new(1.0, 0.0, 0.0);
        let s1b = Point3::new(1.0, 3.0, 0.0);
        let s2a = Point3::new(0.0, 1.0, 0.0);
        let s2b = Point3::new(3.0, 1.0, 0.0);

        let mut subm = one_tri(a[0], a[1], a[2]);
        split_single_triangle(
            &mut subm,
            &[
                tp_explicit(s1a),
                tp_explicit(s1b),
                tp_explicit(s2a),
                tp_explicit(s2b),
            ],
        )
        .expect("inserting endpoints must succeed");

        let (v_s1a, v_s1b, v_s2a, v_s2b) = (
            find_explicit_vert(&subm, s1a).expect("s1a vertex"),
            find_explicit_vert(&subm, s1b).expect("s1b vertex"),
            find_explicit_vert(&subm, s2a).expect("s2a vertex"),
            find_explicit_vert(&subm, s2b).expect("s2b vertex"),
        );
        enforce_constraint_segments(
            &mut subm,
            &[
                SegmentSpec {
                    v0: v_s1a,
                    v1: v_s1b,
                    source_tri: [s1a, s1b, s1_third()],
                },
                SegmentSpec {
                    v0: v_s2a,
                    v1: v_s2b,
                    source_tri: [s2a, s2b, s2_third()],
                },
            ],
        )
        .expect("X-crossing enforcement must succeed");

        let want = [to_r(1.0), to_r(1.0), to_r(0.0)];
        let tpi_vid = (0..subm.num_verts())
            .find(|&v| {
                matches!(subm.vert_coords(v), VertexCoords::Tpi { .. })
                    && exact_coords(subm.vert_coords(v)) == want
            })
            .expect("a Tpi at (1,1,0) must exist");

        // POSITIVE control: exactly on all three planes (generator-order-agnostic).
        exact_tpi_on_three_planes(&subm, tpi_vid);

        // NEGATIVE control: a point lifted off z=0 must NOT be on the base plane.
        let ip = |p: Point3| ExplicitPoint3D::new(p.x(), p.y(), p.z());
        let (ea0, ea1, ea2) = (ip(a[0]), ip(a[1]), ip(a[2]));
        let off_plane = ExplicitPoint3D::new(1.0, 1.0, 0.5);
        assert_ne!(
            orient3d(&ea0, &ea1, &ea2, &off_plane),
            IpSign::Zero,
            "a point at (1,1,0.5) must NOT be on base plane z=0 — the exactness \
             assertion would be vacuous if everything returned Zero"
        );
    }

    // ── Probe 6: gp_dispatch refactor parity (sub-step 3a was a pure move) ──

    /// The 3a refactor moved `Gp`/`backing`/`gp`/`dispatch_orient2d`/
    /// `dispatch_point_in_triangle` out of `retriangulate.rs` verbatim. Exercise
    /// the moved helpers directly on a known Explicit input and assert the
    /// expected sign/bool, demonstrating identical behaviour post-move. (The full
    /// retriangulate suite — run separately — confirms no behavioural regression.)
    #[test]
    fn adv_gp_dispatch_refactor_parity() {
        use crate::arrangements::gp_dispatch::{
            backing, dispatch_orient2d, dispatch_point_in_triangle, gp,
        };
        use indirect_predicates_sidecar_rs::{init_fpu, Sign as IpSign, AVAILABLE};
        if !AVAILABLE {
            panic!("FFI shim not linked; gp_dispatch parity probe cannot run");
        }
        init_fpu();

        // CCW triangle in XY → orient2d positive; a known interior point inside.
        let c0 = VertexCoords::Explicit(Point3::new(0.0, 0.0, 0.0));
        let c1 = VertexCoords::Explicit(Point3::new(4.0, 0.0, 0.0));
        let c2 = VertexCoords::Explicit(Point3::new(0.0, 4.0, 0.0));
        let cp = VertexCoords::Explicit(Point3::new(1.0, 1.0, 0.0));

        let (b0, b1, b2, bp) = (backing(&c0), backing(&c1), backing(&c2), backing(&cp));
        let (g0, g1, g2, gpp) = (gp(&c0, &b0), gp(&c1, &b1), gp(&c2, &b2), gp(&cp, &bp));

        assert_eq!(
            dispatch_orient2d(Plane::XY, &g0, &g1, &g2),
            IpSign::Positive,
            "CCW explicit triangle must give positive XY orient2d via gp_dispatch"
        );
        assert!(
            dispatch_point_in_triangle(&gpp, &g0, &g1, &g2),
            "an interior explicit point must test inside via gp_dispatch"
        );
        // A point clearly outside must test outside.
        let cout = VertexCoords::Explicit(Point3::new(5.0, 5.0, 0.0));
        let bout = backing(&cout);
        let gout = gp(&cout, &bout);
        assert!(
            !dispatch_point_in_triangle(&gout, &g0, &g1, &g2),
            "an exterior explicit point must test outside via gp_dispatch"
        );
    }

    // ── Probe 7: edge_is_constr_between semantics after the helper migration ──

    /// GREEN migrated `edge_is_constr_between`'s `map_or(false, ..)` →
    /// `is_some_and(..)`. Both forms return `false` when the edge is absent.
    /// Assert that for a non-adjacent vertex pair (no edge between them) the
    /// helper returns `false` (rather than panicking or returning `true`).
    #[test]
    fn adv_edge_is_constr_between_false_for_nonadjacent() {
        crate::arrangements::require_ffi_shim();
        let a = xy_triangle_a();
        // Insert an interior point so a non-adjacent pair (the interior point and
        // a base corner not on its fan… here all corners ARE adjacent to a single
        // interior fan point) — use two interior points instead so they are
        // mutually non-adjacent across the fan.
        let p1 = Point3::new(1.0, 1.0, 0.0);
        let p2 = Point3::new(2.0, 1.0, 0.0);
        let mut subm = one_tri(a[0], a[1], a[2]);
        split_single_triangle(&mut subm, &[tp_explicit(p1), tp_explicit(p2)])
            .expect("two interior splits must succeed");

        let v_p1 = find_explicit_vert(&subm, p1).expect("p1 vertex");
        let v_p2 = find_explicit_vert(&subm, p2).expect("p2 vertex");

        // Whatever the fan topology, the helper must be total: if no edge exists
        // between the pair it returns false; if one exists it is simply not
        // constraint-flagged (nothing was enforced) → also false.
        assert!(
            !edge_is_constr_between(&subm, v_p1, v_p2),
            "edge_is_constr_between must be false when the edge is absent or unflagged"
        );
    }

    /// Negative case: a `ConstraintSegment` whose endpoint coords are NOT present
    /// in the submesh must return `EnforceError::EndpointNotInSubmesh`. Built by
    /// passing A's segment list to a BARE base submesh (no inserted points), so
    /// neither endpoint coord is a submesh vertex.
    #[test]
    fn enforce_constraints_missing_endpoint_errors() {
        let a = xy_triangle_a();
        let b = tilted_b();
        let soup = soup_pair(a, b);

        let pairs = detect_intersecting_pairs(&soup);
        let classified = classify_all(&soup, &pairs);
        let (points, _buckets) = group_intersection_points(&soup, &classified);
        // AR3c: Result-returning (>2 geometric endpoints is loud — C++
        // final_check); these fixtures are clean 2-endpoint crossings.
        let seg_lists = group_constraint_segments(&soup, &classified, &points)
            .expect("clean transversal fixture must not over-count endpoints");
        assert_eq!(seg_lists[0].len(), 1, "A must have one constraint segment");

        // Bare base submesh: no intersection points inserted, so the segment's
        // (LPI) endpoints are absent from the submesh.
        let mut subm = one_tri(a[0], a[1], a[2]);

        let err = enforce_constraints(&mut subm, &seg_lists[0], &points)
            .expect_err("a segment whose endpoints aren't in the submesh must error");
        assert!(
            matches!(err, EnforceError::EndpointNotInSubmesh { .. }),
            "expected EndpointNotInSubmesh, got {err:?}"
        );
    }
}
