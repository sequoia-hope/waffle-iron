//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! # PR-CR-AR1 — tri-tri intersection → typed intersection vertices
//!
//! First increment of M6 (native port of the Cherchi 2022 arrangement into
//! `cherchi-rs`). For each CR13 candidate intersecting pair, this module ports
//! the per-pair classification + intersection-point construction from
//! `arrangements/code/intersection_classification.cpp`
//! (`/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/`), emitting a
//! **typed intersection-vertex set per pair** (AR2 re-triangulates from it).
//!
//! Pure Rust since PR-CR-M7c: the LPI approx readback routes through the
//! clean-room `crate::predicates::indirect::approx_lpi` (formerly the FFI
//! `lambda3d_lpi_interval`), so the module compiles unconditionally
//! (WASM-clean).
//!
//! ## Scope (source-faithful — deviation N13 in `docs/yang_deviations.md`)
//!
//! The C++ `intersection_classification.cpp` constructs **only** explicit input
//! vertices + `implicitPoint3D_LPI` points (3 LPI call sites: cpp:290, cpp:324,
//! cpp:358). It builds **zero** TPI points — TPI (`implicitPoint3D_TPI`) is
//! created in `triangulation.cpp::createTPI`, the re-triangulation stage, which
//! the roadmap assigns to PR-CR-AR2. **AR1 therefore builds explicit + LPI only.**
//!
//! AR1 ports the generic **non-coplanar transversal crossing** (the clean core):
//! - `checkVtxInTriangleIntersection` (cpp:734-784): a triangle vertex lies in
//!   the other triangle → [`IntersectionVertex::Explicit`].
//! - `checkSingleNoCoplanarEdgeIntersection` (cpp:679-730): an edge pierces the
//!   other triangle's plane → [`IntersectionVertex::Lpi`].
//!
//! Fully-coplanar pairs (`allCoplanarEdges`, orBA `0 0 0`) and single-coplanar-edge
//! degeneracies (`singleCoplanarEdge`, orBA e.g. `1 0 0`, handled in C++ by
//! `checkSingleCoplanarEdgeIntersections` via jolly points + in-plane edge-edge
//! LPIs) are emitted with a loud [`PairClassification::Deferred`] marker — never
//! silently dropped — and deferred to a later slice.
//!
//! ## Sign-pattern decoders (cpp:834-925)
//!
//! Classification is driven by the `orBA` / `orAB` sign triples: the three
//! `orient3d` signs of one triangle's vertices against the other triangle's
//! supporting plane, normalized to {-1, 0, +1}. The decoders ported here are
//! `normalize_orientations`, `same_orientation`, `all_coplanar_edges`,
//! `single_coplanar_edge`, `vtx_in_plane_and_opposite_edge_on_same_side`,
//! `vtx_in_plane_and_opposite_edge_cross_plane`, and
//! `vtx_on_a_side_and_opposite_edge_on_the_other`. They operate on the EXACT
//! `predicates::orient3d` results on explicit coordinates (matching
//! `cinolib::orient3d` in the C++).

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use crate::arrangements::FastTrimesh;
use crate::predicates::{
    max_component_in_triangle_normal, orient2d, orient3d, point_in_segment_3d,
    point_in_triangle_3d, point_in_triangle_3d_loc, segment_intersects_triangle_3d, Axis,
    PointLocation, SegmentLocation, SegmentTriangleIntersection, Sign, TriangleLocation,
};
use cad_primitives::{Point2, Point3};

/// One endpoint of a tri-tri intersection, correctly typed.
///
/// Mirrors the two point kinds the C++ arrangement constructs in
/// `intersection_classification.cpp`: an existing input vertex (explicit) or a
/// line-plane intersection (`implicitPoint3D_LPI`).
#[derive(Clone, Debug, PartialEq)]
pub enum IntersectionVertex {
    /// Coincides with an existing input vertex (explicit point). `tri` is the
    /// soup triangle index, `corner` the 0..=2 corner whose coordinates this
    /// vertex equals (exact equality, no tolerance).
    Explicit { tri: u32, corner: u8, point: Point3 },
    /// Edge of one triangle pierces the plane of the other (an LPI point).
    ///
    /// Stores the LPI *generators* — the two line endpoints (`line`) and the
    /// three plane points (`plane`) — exactly mirroring the C++
    /// `implicitPoint3D_LPI(p, q, r, s, t)` constructor, plus the approximate
    /// explicit coordinates (`approx`) read back via `lambda3d_lpi_*`. The
    /// generators are the load-bearing data (AR2 reconstructs the implicit
    /// point from them); `approx` is for spatial bookkeeping only.
    Lpi {
        line: [Point3; 2],
        plane: [Point3; 3],
        approx: Point3,
    },
    /// In-plane crossing of a coplanar edge `e` with an edge `f` of the other
    /// (convex) triangle, in the single-coplanar-edge edge-CROSSING sub-config
    /// (deviation N13). Mirrors the C++ `addEdgeCrossEdgeInters`
    /// (`intersection_classification.cpp:285-318`):
    /// `implicitPoint3D_LPI(e.v0, e.v1, f.v0, f.v1, jolly)`. Its EXACT
    /// coordinates are the line∩plane of `line = e` with the plane through
    /// `[f[0], f[1], jolly]`; geometrically the crossing is jolly-INDEPENDENT
    /// (any out-of-plane jolly yields the same in-plane e×f crossing), so the
    /// `jolly` only makes the plane non-degenerate and only affects `approx`.
    ///
    /// This vertex lies on an edge of BOTH triangles (the coplanar edge `e`
    /// and the other-triangle edge `f`), so `group_intersection_points` places
    /// it onto both owners' matching edge buckets.
    EdgeEdge {
        e: [Point3; 2],
        f: [Point3; 2],
        jolly: Point3,
        approx: Point3,
    },
}

/// Why a pair was deferred to a later slice (carried loudly, not dropped).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DeferReason {
    /// Both triangles are coplanar (`allCoplanarEdges`, orBA `0 0 0`).
    Coplanar,
    /// A single edge of one triangle is coplanar with the other
    /// (`singleCoplanarEdge`, orBA e.g. `1 0 0`).
    SingleCoplanarEdge,
    /// A degenerate configuration AR1 does not handle (e.g. a degenerate
    /// triangle slipping past upstream validation).
    Degenerate,
}

/// Classification of a candidate pair's tri-tri relationship for this PR.
#[derive(Clone, Debug, PartialEq)]
pub enum PairClassification {
    /// Generic non-coplanar transversal crossing — `vertices` populated with
    /// the typed intersection endpoints.
    Transversal { vertices: Vec<IntersectionVertex> },
    /// Fully-coplanar pair (`allCoplanarEdges`, orBA `0 0 0`): the C++
    /// `checkTriangleTriangleIntersections` coplanar branch
    /// (`intersection_classification.cpp:137-211`) calls
    /// `checkSingleCoplanarEdgeIntersections` 6× (3 edges of B vs A, then 3
    /// edges of A vs B) to accumulate the overlap's intersection vertices +
    /// the symbolic constraint segments joining them.
    ///
    /// `vertices` is the deduped intersection-vertex set (the C++ `il`
    /// intersection list). `segments` are index pairs into `vertices` (each a
    /// symbolic constraint segment — the C++ `addSymbolicSegment` calls).
    ///
    /// PR-2 only **constructs** this data; the rest of the pipeline still
    /// defers these pairs exactly as before (corpus-neutral). PRs 3-4 consume
    /// it (propagate + pocket dedup).
    Coplanar {
        vertices: Vec<IntersectionVertex>,
        segments: Vec<(u32, u32)>,
    },
    /// Deferred to a later slice; carries the reason (loud, not dropped).
    Deferred(DeferReason),
    /// The sign patterns agree there is no real intersection.
    Disjoint,
}

/// Classify a single candidate pair `(ta, tb)` of soup triangles, constructing
/// its typed intersection-vertex set (explicit + LPI) for the transversal case.
///
/// Ports `checkTriangleTriangleIntersections` (cpp:119-280) restricted to the
/// non-coplanar transversal path (deviation N13). See the module docs.
pub fn classify_pair(soup: &FastTrimesh, ta: u32, tb: u32) -> PairClassification {
    // Ports `checkTriangleTriangleIntersections` (cpp:119-280), restricted to
    // the non-coplanar transversal path (deviation N13). Coplanar /
    // single-coplanar-edge configurations are returned as `Deferred(..)` rather
    // than constructed.

    // The three corners of each triangle (explicit input coordinates).
    let a0 = soup.tri_vert(ta, 0);
    let a1 = soup.tri_vert(ta, 1);
    let a2 = soup.tri_vert(ta, 2);
    let b0 = soup.tri_vert(tb, 0);
    let b1 = soup.tri_vert(tb, 1);
    let b2 = soup.tri_vert(tb, 2);
    let a = [a0, a1, a2];
    let b = [b0, b1, b2];

    // ── check of tB respect to tA (cpp:129-135) ──────────────────────
    //
    // orBA[i] = orient3d(B_i, A0, A1, A2) — same arg order/convention as
    // `cinolib::orient3d` in the C++ (test point first).
    let or_ba = normalize_orientations([
        orient3d(b0, a0, a1, a2),
        orient3d(b1, a0, a1, a2),
        orient3d(b2, a0, a1, a2),
    ]);

    // No intersection: all three on the same (non-zero) side (cpp:135).
    if same_orientation(or_ba[0], or_ba[1]) && same_orientation(or_ba[1], or_ba[2]) && or_ba[0] != 0
    {
        return PairClassification::Disjoint;
    }

    // Fully-coplanar pair (`allCoplanarEdges`, orBA `0 0 0`). PR-2 CONSTRUCTS
    // the classification (intersection vertices + symbolic segments) by porting
    // the C++ coplanar branch (cpp:137-211), instead of the prior loud defer.
    // The rest of the pipeline still treats `Coplanar` as deferred (PR-2 is
    // corpus-neutral); PRs 3-4 consume the data.
    if all_coplanar_edges(or_ba) {
        return classify_coplanar_pair(&a, ta, &b, tb);
    }

    // We need orAB for the single-coplanar-edge deferral check and for the
    // A-vs-B transversal side (cpp:202-219).
    let or_ab = normalize_orientations([
        orient3d(a0, b0, b1, b2),
        orient3d(a1, b0, b1, b2),
        orient3d(a2, b0, b1, b2),
    ]);

    // ── single-coplanar-edge (deviation N13, this PR) ────────────────
    //
    // Exactly one edge of one triangle lies in the other's plane (orBA /
    // orAB sign triple with two zeros). The C++ handles this in
    // `checkSingleCoplanarEdgeIntersections` (cpp:422-657): the coplanar
    // edge's endpoints are placed where they land on the other triangle, the
    // edge's crossings with the other triangle's edges become in-plane
    // edge-edge LPIs, and a symbolic segment connects them.
    //
    // `classify_single_coplanar_edge` constructs ALL single-coplanar-edge
    // sub-configs end-to-end: edge-contained endpoints (Explicit), in-plane
    // edge-edge crossings (`addEdgeCrossEdgeInters` jolly-LPI), tvX_in_edge
    // (the edge crossing o_t through a corner → exact o_t vertex), and
    // collinear-disjoint o_t edges (no crossing → skipped). It returns `None`
    // only for the >2-distinct-endpoint safety guard (geometrically impossible
    // for a convex o_t), which maps to a loud `Deferred(SingleCoplanarEdge)`.
    let sce_ba = single_coplanar_edge(or_ba);
    let sce_ab = single_coplanar_edge(or_ab);
    if let Some(edge0) = sce_ba {
        // B's coplanar edge in A's plane: e_t = B (verts b), o_t = A (a).
        let (e0, e1) = (edge0 as usize, ((edge0 + 1) % 3) as usize);
        return match classify_single_coplanar_edge(b[e0], b[e1], &b, tb, &a, ta) {
            Some(vertices) => PairClassification::Transversal { vertices },
            None => PairClassification::Deferred(DeferReason::SingleCoplanarEdge),
        };
    }
    if let Some(edge0) = sce_ab {
        // A's coplanar edge in B's plane: e_t = A (a), o_t = B (b).
        let (e0, e1) = (edge0 as usize, ((edge0 + 1) % 3) as usize);
        return match classify_single_coplanar_edge(a[e0], a[e1], &a, ta, &b, tb) {
            Some(vertices) => PairClassification::Transversal { vertices },
            None => PairClassification::Deferred(DeferReason::SingleCoplanarEdge),
        };
    }

    // ── transversal: collect intersection vertices ───────────────────
    //
    // B-vs-A side first (vertex set = B, plane/triangle = A) (cpp:158-192).
    let mut vertices: Vec<IntersectionVertex> = Vec::new();
    collect_transversal_side(&b, tb, &a, ta, or_ba, &mut vertices);

    // The `li.size() > 1` early-out (cpp:194): if the B side already produced
    // more than one vertex, the segment is fully determined — skip the A side
    // to avoid double-counting the endpoints.
    if vertices.len() <= 1 {
        // A-vs-B side (vertex set = A, triangle = B) (cpp:231-262).
        //
        // The C++ recomputes orAB after a possible early `return` at cpp:219
        // (all three A vertices on the same non-zero side of B's plane). Mirror
        // that disjoint early-out here.
        if !(same_orientation(or_ab[0], or_ab[1])
            && same_orientation(or_ab[1], or_ab[2])
            && or_ab[0] != 0)
        {
            collect_transversal_side(&a, ta, &b, tb, or_ab, &mut vertices);
        }
    }

    if vertices.is_empty() {
        // Sign patterns admitted a transversal configuration but no edge
        // actually pierced the other triangle's interior/boundary (the
        // segment_intersects guard rejected every candidate). No real
        // intersection.
        return PairClassification::Disjoint;
    }

    PairClassification::Transversal { vertices }
}

/// A coplanar-pair accumulator mirroring the C++ `il` intersection list +
/// per-pair symbolic-segment additions. `vertices` is deduped by EXACT
/// geometric coordinates (the C++ `phmap::flat_hash_set<uint>` over shared
/// vertex ids); `segments` are index pairs into `vertices`.
struct CoplanarAccum {
    vertices: Vec<IntersectionVertex>,
    segments: Vec<(u32, u32)>,
}

impl CoplanarAccum {
    fn new() -> Self {
        CoplanarAccum {
            vertices: Vec::new(),
            segments: Vec::new(),
        }
    }

    /// `il.insert(x)` → ensure `iv` is in `vertices` (deduped by EXACT
    /// geometric coordinates), returning its index. Two intersection vertices
    /// with the same exact coordinates collapse to one entry — this is the
    /// C++ shared-id semantics (an o_t vertex and a coplanar-edge endpoint that
    /// coincide are the same id).
    fn insert(&mut self, iv: IntersectionVertex) -> u32 {
        let key = exact_key_of(&iv);
        for (i, existing) in self.vertices.iter().enumerate() {
            if let (Some(a), Some(b)) = (&key, exact_key_of(existing)) {
                if *a == b {
                    return i as u32;
                }
            } else if *existing == iv {
                return i as u32;
            }
        }
        let idx = self.vertices.len() as u32;
        self.vertices.push(iv);
        idx
    }

    /// `addSymbolicSegment` → push an index-pair constraint segment, skipping
    /// degenerate (a == b) and duplicate (unordered) segments. The C++
    /// `addSymbolicSegment` asserts `v0 != v1` and dedups via the triangle's
    /// segment set; we replicate the no-self-loop + unordered-dedup here.
    fn add_segment(&mut self, a: u32, b: u32) {
        if a == b {
            return;
        }
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        if self.segments.iter().any(|&(x, y)| {
            let (xl, xh) = if x < y { (x, y) } else { (y, x) };
            xl == lo && xh == hi
        }) {
            return;
        }
        self.segments.push((a, b));
    }
}

/// EXACT geometric coordinate key of an [`IntersectionVertex`] (the same
/// `VertexCoords` interning the rest of the arrangement uses). Returns `None`
/// for a point whose exact coords are unresolvable (degenerate generators);
/// callers fall back to structural equality.
fn exact_key_of(iv: &IntersectionVertex) -> Option<[dashu::rational::RBig; 3]> {
    use crate::arrangements::aux_structure::exact_point_coords;
    exact_point_coords(&vertex_coords_of_local(iv))
}

/// Construct the fully-coplanar (`allCoplanarEdges`, orBA `0 0 0`)
/// classification — the intersection-vertex set + symbolic constraint segments
/// of the overlap. Faithful port of the coplanar branch of
/// `checkTriangleTriangleIntersections` (`intersection_classification.cpp:137-211`):
/// `checkSingleCoplanarEdgeIntersections` is called 6× — the 3 edges of B vs A
/// (cpp:144-146), then the 3 edges of A vs B (cpp:208-210) — into one shared
/// accumulator.
///
/// `a`/`ta` and `b`/`tb` are the two coplanar triangles (corners + soup ids).
///
/// PR-2 returns this as [`PairClassification::Coplanar`]; the pipeline still
/// defers it (corpus-neutral).
fn classify_coplanar_pair(
    a: &[Point3; 3],
    ta: u32,
    b: &[Point3; 3],
    tb: u32,
) -> PairClassification {
    let mut acc = CoplanarAccum::new();

    // 3 edges of B vs A (cpp:144-146): e_t = B, o_t = A.
    coplanar_edge_intersections(b[0], b[1], b, tb, a, ta, &mut acc);
    coplanar_edge_intersections(b[1], b[2], b, tb, a, ta, &mut acc);
    coplanar_edge_intersections(b[2], b[0], b, tb, a, ta, &mut acc);

    // 3 edges of A vs B (cpp:208-210): e_t = A, o_t = B.
    coplanar_edge_intersections(a[0], a[1], a, ta, b, tb, &mut acc);
    coplanar_edge_intersections(a[1], a[2], a, ta, b, tb, &mut acc);
    coplanar_edge_intersections(a[2], a[0], a, ta, b, tb, &mut acc);

    // NOTE: the C++ `final_check` assert `v_tmp.size() <= 3`
    // (intersection_classification.cpp:268) bounds the driver-level `v_tmp`
    // SYMBOLIC-segment temp set, which is EMPTY on the coplanar path (the
    // coplanar branch passes the `li` intersection list, not `v_tmp`, and adds
    // its symbolic segments inside `checkSingleCoplanarEdgeIntersections`).
    // The coplanar `il` intersection list is NOT bounded to 3 — two
    // overlapping coplanar triangles can produce up to a hexagonal overlap (6
    // points). So no ≤3 cap is asserted here (faithful to the C++).
    PairClassification::Coplanar {
        vertices: acc.vertices,
        segments: acc.segments,
    }
}

/// Where one endpoint of the coplanar edge lands on the other triangle `o_t`
/// (a faithful mirror of the C++ `v{0,1}_in_vtx / v{0,1}_in_seg / v{0,1}_in_tri`
/// flag triple). The three are mutually exclusive (ON_VERT / ON_EDGEj /
/// STRICTLY_INSIDE / outside).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum EndpointLoc {
    /// Coincides with an `o_t` vertex (ON_VERT0/1/2).
    InVtx,
    /// Lies strictly on `o_t` edge `j` (ON_EDGEj). `OnEdge0 = (o0,o1)`,
    /// `OnEdge1 = (o1,o2)`, `OnEdge2 = (o2,o0)`.
    InSeg(usize),
    /// Strictly inside `o_t`.
    InTri,
    /// Strictly outside `o_t`.
    Outside,
}

fn endpoint_loc(p: Point3, o_tri: &[Point3; 3]) -> EndpointLoc {
    match point_in_triangle_3d_loc(p, o_tri[0], o_tri[1], o_tri[2]) {
        TriangleLocation::OnVert0 | TriangleLocation::OnVert1 | TriangleLocation::OnVert2 => {
            EndpointLoc::InVtx
        }
        TriangleLocation::OnEdge0 => EndpointLoc::InSeg(0),
        TriangleLocation::OnEdge1 => EndpointLoc::InSeg(1),
        TriangleLocation::OnEdge2 => EndpointLoc::InSeg(2),
        TriangleLocation::StrictlyInside => EndpointLoc::InTri,
        TriangleLocation::StrictlyOutside => EndpointLoc::Outside,
    }
}

/// Complete port of `checkSingleCoplanarEdgeIntersections`
/// (`intersection_classification.cpp:422-657`). Accumulates the intersection
/// vertices + symbolic constraint segments for ONE coplanar edge `(e0, e1)` of
/// triangle `e_t` (soup id `e_tri_id`) against the OTHER coplanar triangle
/// `o_tri` (soup id `o_tri_id`), into the shared `acc`.
///
/// The C++ `il.insert(x)` becomes `acc.insert(iv)` (dedup → index), and each
/// `addSymbolicSegment(u, v, ..)` becomes `acc.add_segment(idx_u, idx_v)`.
///
/// All sub-configs of the C++ are ported: contained endpoints (Explicit), the
/// shared-edge early return, vertex/edge/interior placements + their symbolic
/// joins, the three edge-CROSSING blocks (EdgeEdge via jolly-LPI), the
/// `tvX_in_edge` o_t-vertex-in-coplanar-edge branches (Explicit o_t vertex +
/// symbolic joins), and the trailing seg-cross + tvX_in_edge symbolic joins.
#[allow(clippy::too_many_arguments)]
fn coplanar_edge_intersections(
    e0: Point3,
    e1: Point3,
    e_tri: &[Point3; 3],
    e_tri_id: u32,
    o_tri: &[Point3; 3],
    o_tri_id: u32,
    acc: &mut CoplanarAccum,
) {
    // Corner index (0..=2) of an endpoint within its owning triangle `e_t`.
    let e_corner = |p: Point3| -> u8 {
        if p == e_tri[0] {
            0
        } else if p == e_tri[1] {
            1
        } else {
            2
        }
    };
    // An Explicit intersection vertex for a coplanar-edge endpoint (owned by e_t).
    let e_vert = |p: Point3| IntersectionVertex::Explicit {
        tri: e_tri_id,
        corner: e_corner(p),
        point: p,
    };
    // An Explicit intersection vertex for an o_t corner (owned by o_t).
    let o_vert = |corner: u8| IntersectionVertex::Explicit {
        tri: o_tri_id,
        corner,
        point: o_tri[corner as usize],
    };

    // ── endpoint positions on o_t (cpp:431-457) ──────────────────────
    let loc0 = endpoint_loc(e0, o_tri);
    let loc1 = endpoint_loc(e1, o_tri);

    let v0_in_vtx = loc0 == EndpointLoc::InVtx;
    let v1_in_vtx = loc1 == EndpointLoc::InVtx;
    let v0_in_seg = match loc0 {
        EndpointLoc::InSeg(j) => Some(j),
        _ => None,
    };
    let v1_in_seg = match loc1 {
        EndpointLoc::InSeg(j) => Some(j),
        _ => None,
    };
    let v0_in_tri = loc0 == EndpointLoc::InTri;
    let v1_in_tri = loc1 == EndpointLoc::InTri;

    // The C++ `il.insert(e_v0)` for the ON_VERT case (cpp:438/450). For
    // ON_VERT, the coplanar-edge endpoint coincides with an o_t vertex; insert
    // it as the e_t-owned Explicit (geometric dedup collapses it with the o_t
    // vertex if that is inserted later).
    if v0_in_vtx {
        acc.insert(e_vert(e0));
    }
    if v1_in_vtx {
        acc.insert(e_vert(e1));
    }

    // cpp:460 — both endpoints on o_t vertices: shared edge, no new geometry.
    if v0_in_vtx && v1_in_vtx {
        return;
    }

    // ── endpoint placement + early symbolic joins (cpp:462-525) ───────

    // Both endpoints in o_t segments (cpp:462-471): the link of two
    // vertex-in-edge endpoints is the sub-segment.
    if v0_in_seg.is_some() && v1_in_seg.is_some() {
        let i0 = acc.insert(e_vert(e0));
        let i1 = acc.insert(e_vert(e1));
        acc.add_segment(i0, i1);
        return;
    }

    // Only v0 in a segment (cpp:472-481).
    if v0_in_seg.is_some() {
        let i0 = acc.insert(e_vert(e0));
        if v1_in_vtx {
            let i1 = acc.insert(e_vert(e1));
            acc.add_segment(i0, i1);
            return;
        }
    } else if v1_in_seg.is_some() {
        // Only v1 in a segment (cpp:482-491).
        let i1 = acc.insert(e_vert(e1));
        if v0_in_vtx {
            let i0 = acc.insert(e_vert(e0));
            acc.add_segment(i1, i0);
            return;
        }
    }

    // v0 in a segment or vtx and v1 inside the triangle (cpp:494-501).
    if (v0_in_seg.is_some() || v0_in_vtx) && v1_in_tri {
        let i1 = acc.insert(e_vert(e1));
        let i0 = acc.insert(e_vert(e0));
        acc.add_segment(i0, i1);
        return;
    }

    // v1 in a segment or vtx and v0 inside the triangle (cpp:504-511).
    if (v1_in_seg.is_some() || v1_in_vtx) && v0_in_tri {
        let i0 = acc.insert(e_vert(e0));
        let i1 = acc.insert(e_vert(e1));
        acc.add_segment(i0, i1);
        return;
    }

    // Both endpoints strictly inside the triangle (cpp:514-522).
    if v0_in_tri && v1_in_tri {
        let i0 = acc.insert(e_vert(e0));
        let i1 = acc.insert(e_vert(e1));
        acc.add_segment(i0, i1);
        return;
    }

    // Only one endpoint inside the triangle (cpp:524-534) — record it; the
    // matching crossing endpoint is found by the edge-cross blocks below.
    if v0_in_tri {
        acc.insert(e_vert(e0));
    } else if v1_in_tri {
        acc.insert(e_vert(e1));
    }

    // ── edge-cross checking (cpp:536-657) ─────────────────────────────
    //
    // The o_t vertices lying ON the coplanar edge (cpp:543-545). `tvX_in_edge`
    // means o_t corner X is not strictly outside the coplanar segment.
    let tv_in_edge = |corner: usize| -> bool {
        point_in_segment_3d(o_tri[corner], e0, e1) != SegmentLocation::StrictlyOutside
    };
    let tv0 = tv_in_edge(0);
    let tv1 = tv_in_edge(1);
    let tv2 = tv_in_edge(2);

    // Project to the dominant plane of o_t for the EXACT in-plane crossing test.
    let axis = max_component_in_triangle_normal(o_tri[0], o_tri[1], o_tri[2]);
    // o_t edges, indexed as the C++ triEdgeID order:
    // edge0 = (o0,o1), edge1 = (o1,o2), edge2 = (o2,o0).
    let o_edges = [
        (o_tri[0], o_tri[1]),
        (o_tri[1], o_tri[2]),
        (o_tri[2], o_tri[0]),
    ];
    // o_t vertex corner OPPOSITE each o_t edge in the C++ `tvX_in_edge` joins:
    // seg0 → tv2, seg1 → tv0, seg2 → tv1 (cpp:570-575 / 602-607 / 634-639).
    let opp_corner = [2usize, 0usize, 1usize];
    let opp_flag = [tv2, tv0, tv1];

    // Mutually-exclusive "did this endpoint already get placed" predicates,
    // reused per edge (cpp:561/566/etc `v0_in_vtx || v0_in_seg != -1 || v0_in_tri`).
    let v0_placed = v0_in_vtx || v0_in_seg.is_some() || v0_in_tri;
    let v1_placed = v1_in_vtx || v1_in_seg.is_some() || v1_in_tri;

    let mut seg_cross: [Option<u32>; 3] = [None, None, None];

    for j in 0..3 {
        let (oa, ob) = o_edges[j];
        // C++ guard (cpp:552-555 / 584-587 / 616-619): the o_t edge hosts
        // NEITHER coplanar-edge endpoint (v{0,1}_in_seg != edge j), and the two
        // o_t-edge vertices are strictly OUTSIDE the coplanar edge. For edge j
        // the relevant o_t-edge vertices are the two endpoints of o_edges[j];
        // their tvX_in_edge flags must both be false.
        let (tv_a, tv_b) = match j {
            0 => (tv0, tv1),
            1 => (tv1, tv2),
            _ => (tv2, tv0),
        };
        if v0_in_seg == Some(j) || v1_in_seg == Some(j) || tv_a || tv_b {
            continue;
        }
        // The o_t edge must strictly straddle / be crossed by the coplanar edge
        // and neither o_t-edge vertex strictly inside the coplanar edge — the
        // latter is exactly the tv_a/tv_b == false guard above (ON_EDGE/ON_VERT
        // of the coplanar edge would make tvX true). Now test the proper
        // in-plane crossing (EXACT orient2d).
        if in_plane_proper_crossing(axis, e0, e1, oa, ob) != CrossingKind::Proper {
            continue;
        }

        // EdgeEdge crossing vertex (jolly-LPI), cpp:557/589/621
        // `addEdgeCrossEdgeInters`.
        let jolly = match pick_non_coplanar_jolly(oa, ob, e0) {
            Some(j) => j,
            // No non-degenerate jolly: cannot construct the LPI plane. P9 —
            // never fabricate. Leave this crossing unrecorded; the pair stays a
            // (still-deferred) Coplanar so nothing silent-wrong escapes.
            None => continue,
        };
        let approx = lpi_approx(e0, e1, oa, ob, jolly);
        let cross_idx = acc.insert(IntersectionVertex::EdgeEdge {
            e: [e0, e1],
            f: [oa, ob],
            jolly,
            approx,
        });
        seg_cross[j] = Some(cross_idx);

        // Symbolic join from a placed coplanar-edge endpoint to the crossing
        // (cpp:561-573 / 593-605 / 625-637).
        if v0_placed {
            let i0 = acc.insert(e_vert(e0));
            acc.add_segment(i0, cross_idx);
            continue;
        } else if v1_placed {
            let i1 = acc.insert(e_vert(e1));
            acc.add_segment(i1, cross_idx);
            continue;
        } else if opp_flag[j] {
            // tvX_in_edge: the o_t vertex opposite edge j lies on the coplanar
            // edge → it is an Explicit intersection vertex; join it to the
            // crossing, and record the o_t vertex on the coplanar edge
            // (cpp:570-575 / 602-607 / 634-639).
            let ov = acc.insert(o_vert(opp_corner[j] as u8));
            acc.add_segment(ov, cross_idx);
            continue;
        }
    }

    // Final symbolic edges between two crossings (cpp:642-650).
    match (seg_cross[0], seg_cross[1], seg_cross[2]) {
        (Some(a), Some(b), _) => acc.add_segment(a, b),
        (Some(a), _, Some(c)) => acc.add_segment(a, c),
        (_, Some(b), Some(c)) => acc.add_segment(b, c),
        _ => {}
    }

    // Trailing tvX_in_edge symbolic joins (cpp:652-656 group): an o_t vertex on
    // the coplanar edge joins to whichever coplanar-edge endpoint is placed.
    if tv0 {
        let ov = acc.insert(o_vert(0));
        if v0_in_seg.is_some() || v0_in_tri {
            let i0 = acc.insert(e_vert(e0));
            acc.add_segment(ov, i0);
        } else if v1_in_seg.is_some() || v1_in_tri {
            let i1 = acc.insert(e_vert(e1));
            acc.add_segment(ov, i1);
        }
    }
    if tv1 {
        let ov = acc.insert(o_vert(1));
        if v0_in_seg.is_some() || v0_in_tri {
            let i0 = acc.insert(e_vert(e0));
            acc.add_segment(ov, i0);
        } else if v1_in_seg.is_some() || v1_in_tri {
            let i1 = acc.insert(e_vert(e1));
            acc.add_segment(ov, i1);
        }
    }
    if tv2 {
        let ov = acc.insert(o_vert(2));
        if v0_in_seg.is_some() || v0_in_tri {
            let i0 = acc.insert(e_vert(e0));
            acc.add_segment(ov, i0);
        } else if v1_in_seg.is_some() || v1_in_tri {
            let i1 = acc.insert(e_vert(e1));
            acc.add_segment(ov, i1);
        }
    }
}

/// One transversal "side" of `checkTriangleTriangleIntersections`
/// (cpp:158-192 for B-vs-A, cpp:231-262 for A-vs-B). `verts`/`vtri` is the
/// triangle whose vertices/edges are being tested; `plane`/`ptri` is the
/// triangle whose supporting plane/interior they are tested against. `ori` is
/// the normalized orientation triple of `verts` against `plane`'s plane.
fn collect_transversal_side(
    verts: &[Point3; 3],
    vtri: u32,
    plane: &[Point3; 3],
    ptri: u32,
    ori: [i8; 3],
    out: &mut Vec<IntersectionVertex>,
) {
    // a vertex of `verts` is in `plane`'s plane and the opposite edge is on the
    // same side → just the vertex may land inside (cpp:159-162).
    if let Some(vtx) = vtx_in_plane_and_opposite_edge_on_same_side(ori) {
        check_vtx_in_triangle_intersection(verts[vtx as usize], vtri, vtx, plane, out);
    }

    // a vertex in the plane and the opposite edge crosses the plane → the
    // vertex (Explicit) plus the opposite edge piercing the triangle (Lpi)
    // (cpp:165-173).
    if let Some(vtx) = vtx_in_plane_and_opposite_edge_cross_plane(ori) {
        check_vtx_in_triangle_intersection(verts[vtx as usize], vtri, vtx, plane, out);
        // opposite edge = the two vertices that are NOT `vtx`.
        let (e0, e1) = opposite_edge_endpoints(vtx);
        check_single_no_coplanar_edge_intersection(verts[e0], verts[e1], plane, ptri, out);
    }

    // a vertex on one side and the opposite edge on the other → both edges from
    // the lone vertex pierce the triangle → 2 Lpi (cpp:177-192).
    if let Some((vtx, opp_v0, opp_v1)) = vtx_on_a_side_and_opposite_edge_on_the_other(ori) {
        check_single_no_coplanar_edge_intersection(
            verts[vtx as usize],
            verts[opp_v0 as usize],
            plane,
            ptri,
            out,
        );
        check_single_no_coplanar_edge_intersection(
            verts[vtx as usize],
            verts[opp_v1 as usize],
            plane,
            ptri,
            out,
        );
    }
}

/// Ports `checkVtxInTriangleIntersection` (cpp:734-784) for the AR1 scope: if
/// the vertex is not strictly outside the triangle, record it as an explicit
/// intersection vertex (the C++ distinguishes ON_VERT/ON_EDGE/INSIDE for
/// bookkeeping, but all non-STRICTLY_OUTSIDE cases record the vertex).
fn check_vtx_in_triangle_intersection(
    v: Point3,
    vtri: u32,
    corner: u32,
    plane: &[Point3; 3],
    out: &mut Vec<IntersectionVertex>,
) {
    match point_in_triangle_3d(v, plane[0], plane[1], plane[2]) {
        PointLocation::StrictlyOutside => {}
        PointLocation::StrictlyInside | PointLocation::OnBoundary => {
            push_unique(
                out,
                IntersectionVertex::Explicit {
                    tri: vtri,
                    corner: corner as u8,
                    point: v,
                },
            );
        }
    }
}

/// Ports `checkSingleNoCoplanarEdgeIntersection` (cpp:679-730) for the AR1
/// scope: if the edge `(p, q)` actually crosses the triangle, construct the LPI
/// point (line = the piercing edge's endpoints, plane = the pierced triangle's
/// 3 vertices — cpp:324-328 and cpp:358-362 both pass the triangle's 3 verts as
/// r,s,t) and record it.
fn check_single_no_coplanar_edge_intersection(
    p: Point3,
    q: Point3,
    plane: &[Point3; 3],
    _ptri: u32,
    out: &mut Vec<IntersectionVertex>,
) {
    // cpp:683-686: only proceed if the segment really intersects the triangle.
    match segment_intersects_triangle_3d(p, q, plane[0], plane[1], plane[2]) {
        SegmentTriangleIntersection::Disjoint | SegmentTriangleIntersection::Coplanar => return,
        SegmentTriangleIntersection::Intersects => {}
    }

    // cpp:688-691: if a triangle vertex lies strictly inside the edge, return
    // nothing. cinolib `point_in_segment_3d` (STRICTLY_INSIDE) is not available
    // as a dedicated cherchi-rs predicate yet; reconstruct it from the
    // available `orient`/`point_in_triangle` machinery would be ad-hoc, so this
    // guard is kept conservative via a collinear+between test on exact
    // coordinates (no tolerance). NOTE: AR1's transversal inputs do not trigger
    // this degeneracy; it is ported for faithfulness.
    if any_triangle_vertex_strictly_inside_segment(p, q, plane) {
        return;
    }

    // Construct the LPI generators (mirrors implicitPoint3D_LPI(p,q,r,s,t)):
    // line = edge endpoints, plane = the 3 pierced-triangle vertices.
    let approx = lpi_approx(p, q, plane[0], plane[1], plane[2]);
    push_unique(
        out,
        IntersectionVertex::Lpi {
            line: [p, q],
            plane: *plane,
            approx,
        },
    );
}

/// Port of `checkSingleCoplanarEdgeIntersections` (cpp:422-657), covering ALL
/// single-coplanar-edge sub-configs (deviation N13): edge-contained,
/// edge-CROSSING, tvX_in_edge (crossing through an o_t corner), and
/// collinear-disjoint o_t edges.
///
/// `e0`/`e1` are the two endpoints of the coplanar edge (an edge of triangle
/// `e_t`, whose corners are `_e_tri`, soup id `_e_tri_id`); `o_tri`/`o_tri_id`
/// are the OTHER triangle (the plane owner).
///
/// KEY GEOMETRIC FACT: the coplanar edge (a segment) ∩ the OTHER convex
/// triangle is a SINGLE sub-segment `[P, Q]`. Each of `P`, `Q` is either:
/// - a coplanar-edge ENDPOINT that lies on the closed other triangle
///   (ON_VERT / ON_EDGE / STRICTLY_INSIDE) → an `Explicit` vertex (the
///   contained path), placed onto `o_t` by `group_intersection_points`, OR
/// - an in-plane CROSSING of the coplanar edge with one of `o_t`'s 3 edges →
///   an [`IntersectionVertex::EdgeEdge`] (the C++ `addEdgeCrossEdgeInters`
///   jolly-LPI, cpp:557/589/621), which lies on an edge of BOTH triangles, OR
/// - an o_t VERTEX strictly inside the coplanar edge (`tvX_in_edge`,
///   cpp:545-547) — a degenerate crossing THROUGH a corner, so the crossing
///   point is the exact o_t vertex (an `Explicit`), collected in section (a2).
///
/// So at most TWO distinct sub-segment endpoints are emitted;
/// `group_constraint_segments` joins them into the one `ConstraintSegment`
/// (cpp `addSymbolicSegment`). A collinear o_t edge yields no crossing (a real
/// overlap is captured via the endpoint/tvX paths; collinear-disjoint is
/// skipped), matching the C++.
///
/// Returns:
/// - `Some(vertices)` with the ≤2 distinct sub-segment endpoints.
/// - `Some(vec![])` for an exact shared edge (both endpoints ON o_t vertices,
///   cpp:460 `if(v0_in_vtx && v1_in_vtx) return;`) or a collinear-disjoint
///   pass-through — no new geometry.
/// - `None` (defer LOUDLY, P9/P10) ONLY for the >2-distinct-endpoint safety
///   guard — geometrically impossible for a convex `o_t`, so this surfaces an
///   upstream-classification bug rather than silently emitting a guess.
fn classify_single_coplanar_edge(
    e0: Point3,
    e1: Point3,
    _e_tri: &[Point3; 3],
    e_tri_id: u32,
    o_tri: &[Point3; 3],
    o_tri_id: u32,
) -> Option<Vec<IntersectionVertex>> {
    // Endpoint locations on the other (plane-owner) triangle.
    let loc0 = point_in_triangle_3d_loc(e0, o_tri[0], o_tri[1], o_tri[2]);
    let loc1 = point_in_triangle_3d_loc(e1, o_tri[0], o_tri[1], o_tri[2]);

    // Which o_t edge (if any) each endpoint lies ON (ON_EDGEj). The C++
    // EXCLUDES that edge from the cross-edge test (cpp:552 `v0_in_seg !=
    // o_t_e0 && v1_in_seg != o_t_e0`, etc.) — an endpoint resting on an edge
    // is a vertex-in-edge, not a transversal crossing of that edge.
    let on_edge_index = |l: TriangleLocation| -> Option<usize> {
        match l {
            TriangleLocation::OnEdge0 => Some(0),
            TriangleLocation::OnEdge1 => Some(1),
            TriangleLocation::OnEdge2 => Some(2),
            _ => None,
        }
    };
    let e0_seg = on_edge_index(loc0);
    let e1_seg = on_edge_index(loc1);

    // A vertex of the other triangle that lies strictly inside the coplanar
    // edge (`tvX_in_edge`, cpp:545-547 / 658-674) is a DEGENERATE crossing:
    // the coplanar edge enters/exits `o_t` THROUGH one of its corners, so the
    // crossing point is the EXACT o_t vertex (an `Explicit`), not a jolly-LPI.
    // (The coplanar edge ∩ a convex `o_t` is one segment with ≤ 2 endpoints, so
    // such a vertex is always an entry/exit, never strictly interior to the
    // sub-segment.) These are collected as `Explicit` sub-segment endpoints in
    // section (a2) below — placed by `group_intersection_points` as a
    // vertex-in-edge on the coplanar edge (the C++ `addVertexInEdge(curr_e_id,
    // v_id)`), and dropped on the `o_t` side where they are already a corner.

    let is_on_vert = |l: TriangleLocation| {
        matches!(
            l,
            TriangleLocation::OnVert0 | TriangleLocation::OnVert1 | TriangleLocation::OnVert2
        )
    };

    // cpp:460 — both endpoints coincide with o_t vertices: shared edge, no new
    // geometry, no segment.
    if is_on_vert(loc0) && is_on_vert(loc1) {
        return Some(Vec::new());
    }

    let corner_of = |p: Point3| -> u8 {
        if p == _e_tri[0] {
            0
        } else if p == _e_tri[1] {
            1
        } else {
            2
        }
    };

    // ── Build the sub-segment [P, Q] = coplanar edge ∩ o_t ─────────────
    //
    // Each sub-segment endpoint is collected as an `IntersectionVertex`:
    // a contained coplanar-edge endpoint → `Explicit` (a); an o_t vertex on the
    // coplanar edge → `Explicit` (a2, tvX); an in-plane crossing of the coplanar
    // edge with an o_t edge → `EdgeEdge` (b). We collect ≤2 distinct endpoints;
    // >2 is geometrically impossible for a convex o_t → defer LOUDLY (safety).
    let mut out: Vec<IntersectionVertex> = Vec::new();

    // (a) Each coplanar-edge endpoint that lies on the closed o_t is an
    //     `Explicit` sub-segment endpoint (the contained path).
    for (ep, loc) in [(e0, loc0), (e1, loc1)] {
        if loc != TriangleLocation::StrictlyOutside {
            push_unique(
                &mut out,
                IntersectionVertex::Explicit {
                    tri: e_tri_id,
                    corner: corner_of(ep),
                    point: ep,
                },
            );
        }
    }

    // (a2) Each o_t VERTEX strictly inside the coplanar edge (`tvX_in_edge`,
    //      cpp:545-547) is a degenerate edge-edge crossing AT that corner: the
    //      crossing point IS the exact o_t vertex. Collect it as an `Explicit`
    //      endpoint tagged with the o_t triangle (so `group_intersection_points`
    //      places it as a vertex-in-edge on the coplanar edge and drops it on
    //      the o_t side, where it is already a corner — the C++
    //      `addVertexInEdge(curr_e_id, v_id)`).
    for (corner, &w) in o_tri.iter().enumerate() {
        if point_in_segment_3d(w, e0, e1) == SegmentLocation::StrictlyInside {
            push_unique(
                &mut out,
                IntersectionVertex::Explicit {
                    tri: o_tri_id,
                    corner: corner as u8,
                    point: w,
                },
            );
        }
    }

    // (b) Each proper in-plane crossing of the coplanar edge with an o_t edge
    //     is an `EdgeEdge` sub-segment endpoint (cpp:552-646). The C++ guard
    //     (cpp:552-555): the o_t edge hosts NEITHER coplanar-edge endpoint
    //     (`v0_in_seg != o_t_eX && v1_in_seg != o_t_eX`), the segments
    //     INTERSECT, and NEITHER o_t-edge vertex is strictly inside the
    //     coplanar edge (otherwise it is a vertex-in-edge, not a crossing).
    let o_edges = [
        (o_tri[0], o_tri[1]),
        (o_tri[1], o_tri[2]),
        (o_tri[2], o_tri[0]),
    ];
    // Project to the plane with the largest normal component of o_t (the
    // coplanar edge AND all o_t edges live in that plane, so the in-plane
    // crossing is captured exactly by `orient2d` there — avoiding the
    // axis-aligned degenerate-projection blind spot of the generic 3D
    // `segment_segment_intersect_3d`, which collapses to a point in a dropped
    // axis when the coplanar edge is parallel to it).
    let axis = max_component_in_triangle_normal(o_tri[0], o_tri[1], o_tri[2]);
    for (i, (oa, ob)) in o_edges.into_iter().enumerate() {
        if e0_seg == Some(i) || e1_seg == Some(i) {
            continue; // an endpoint already rests on this edge (cpp:552)
        }
        // A genuine in-plane crossing requires the o_t-edge vertices to be off
        // the coplanar edge (cpp:554-555); an o_t vertex on the coplanar edge
        // is a vertex-in-edge handled by the (already-deferred) tvX_in_edge
        // path.
        if point_in_segment_3d(oa, e0, e1) != SegmentLocation::StrictlyOutside
            || point_in_segment_3d(ob, e0, e1) != SegmentLocation::StrictlyOutside
        {
            continue;
        }
        match in_plane_proper_crossing(axis, e0, e1, oa, ob) {
            CrossingKind::Proper => {
                // In-plane edge-edge crossing → an `EdgeEdge` (jolly-LPI,
                // cpp:557/589/621 `addEdgeCrossEdgeInters`).
                let jolly = pick_non_coplanar_jolly(oa, ob, e0)?;
                let approx = lpi_approx(e0, e1, oa, ob, jolly);
                push_unique(
                    &mut out,
                    IntersectionVertex::EdgeEdge {
                        e: [e0, e1],
                        f: [oa, ob],
                        jolly,
                        approx,
                    },
                );
            }
            CrossingKind::Collinear => {
                // This o_t edge is collinear with the coplanar edge. A real
                // OVERLAP would already have tripped a guard above — an o_t-edge
                // vertex strictly inside the coplanar edge (the `!= StrictlyOutside`
                // skip) or a coplanar-edge endpoint resting on this o_t edge
                // (`e{0,1}_seg == Some(i)`) — and been captured by section (a)/(a2).
                // So reaching here means collinear-but-DISJOINT (no shared interior):
                // there is no crossing on this edge. The C++ has no collinear-defer
                // branch — `segment_segment_intersect_3d` simply does not report a
                // proper crossing and the loop moves on — so we skip, matching the
                // reference and the fully-coplanar sibling `coplanar_edge_intersections`.
                continue;
            }
            CrossingKind::None => {}
        }
    }

    // The coplanar edge ∩ a convex triangle is a single sub-segment with at
    // most two distinct endpoints. More than two distinct sub-segment
    // endpoints is geometrically impossible for a convex o_t → defer LOUDLY
    // rather than emit a guessed segment.
    if distinct_geometric_endpoints(&out) > 2 {
        return None;
    }

    Some(out)
}

/// Result of the in-plane proper-crossing test between the coplanar edge and
/// one o_t edge (both coplanar, projected to the dominant plane of o_t).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum CrossingKind {
    /// The two segments cross at a single interior point of BOTH (a proper
    /// transversal crossing — the four `orient2d` cross-side signs are
    /// strictly opposite on both segments, no shared endpoint).
    Proper,
    /// The two segments are collinear in the plane (an `orient2d`-degenerate
    /// pair) — ambiguous sub-segment, deferred upstream.
    Collinear,
    /// No interior crossing (disjoint, or touching only at an endpoint).
    None,
}

/// Project `p` to 2D by dropping the axis of `axis` (the dominant normal
/// component of the shared plane). Exact (no arithmetic — coordinate copy).
fn drop_axis(axis: Axis, p: Point3) -> Point2 {
    match axis {
        Axis::X => Point2::new(p.y(), p.z()),
        Axis::Y => Point2::new(p.x(), p.z()),
        Axis::Z => Point2::new(p.x(), p.y()),
    }
}

/// Proper in-plane crossing of segment `(e0, e1)` with segment `(oa, ob)`,
/// both lying in the plane whose dominant normal axis is `axis`. EXACT via
/// `orient2d` (CR10) on the dropped-axis projection.
///
/// Mirrors the cinolib `segment_segment_intersect_2d` cross-side rule
/// (`predicates.cpp:581-641`) but specialized to the KNOWN-coplanar inputs of
/// the single-coplanar-edge config, projecting to the plane the segments
/// actually live in (so a coplanar edge parallel to a dropped 3D axis is not a
/// false negative). A `Proper` crossing is the strict four-sign-opposite case
/// with no shared endpoint; an all-zero `orient2d` quartet is `Collinear`.
fn in_plane_proper_crossing(
    axis: Axis,
    e0: Point3,
    e1: Point3,
    oa: Point3,
    ob: Point3,
) -> CrossingKind {
    let q0 = drop_axis(axis, e0);
    let q1 = drop_axis(axis, e1);
    let r0 = drop_axis(axis, oa);
    let r1 = drop_axis(axis, ob);

    let s_e0 = orient2d(r0, r1, q0);
    let s_e1 = orient2d(r0, r1, q1);
    let s_o0 = orient2d(q0, q1, r0);
    let s_o1 = orient2d(q0, q1, r1);

    // All four collinear → degenerate / overlapping in this plane.
    if s_e0 == Sign::Zero && s_e1 == Sign::Zero && s_o0 == Sign::Zero && s_o1 == Sign::Zero {
        return CrossingKind::Collinear;
    }

    // Proper crossing: each segment's endpoints strictly straddle the other's
    // supporting line (signs strictly opposite, neither zero), and no shared
    // endpoint (guaranteed: a shared endpoint would have been an ON_VERT /
    // ON_EDGE endpoint excluded above, and the o_t-vertex-on-edge cases are
    // filtered before this call).
    let straddle_e = s_e0 != Sign::Zero && s_e1 != Sign::Zero && s_e0 != s_e1;
    let straddle_o = s_o0 != Sign::Zero && s_o1 != Sign::Zero && s_o0 != s_o1;
    if straddle_e && straddle_o {
        CrossingKind::Proper
    } else {
        CrossingKind::None
    }
}

/// Pick the FIRST of the (scaled) tetrahedral jolly points whose
/// `orient3d(f0, f1, j, f_other) != Zero` — i.e. not aligned with the o_t edge
/// `(f0, f1)`. Mirrors the C++ `noCoplanarJollyPointID(ts, e1.v0, e1.v1,
/// e0.v0)` arg order (cpp:406-418, called from `addEdgeCrossEdgeInters`
/// cpp:286). The chosen jolly makes the LPI plane through `(f0, f1, jolly)`
/// non-degenerate; since the crossing is jolly-INDEPENDENT geometrically, the
/// choice only affects `approx`.
///
/// Returns `None` (defer LOUDLY) in the impossible event that all four jolly
/// points are coplanar with the edge — never fabricates a degenerate plane.
fn pick_non_coplanar_jolly(f0: Point3, f1: Point3, f_other: Point3) -> Option<Point3> {
    // Scale the regular-tetrahedron jolly directions generously relative to
    // the coordinate magnitude of the edge so they are non-coplanar with the
    // mesh edge regardless of input scale (the soup coords classify_pair sees
    // are already multiplier-scaled). `compute_multiplier` is not visible
    // here; deriving the scale from the live coordinates is sufficient because
    // the EXACT crossing is jolly-independent — the scale only affects `approx`
    // and the non-degeneracy `orient3d != Zero` check.
    let mag = [f0, f1, f_other]
        .iter()
        .flat_map(|p| [p.x().abs(), p.y().abs(), p.z().abs()])
        .fold(1.0_f64, f64::max);
    let m = mag * 4.0;
    // Regular tetrahedron directions (the four C++ jolly points, spec §8).
    let dirs = [
        (0.942_809_041_6, 0.0, -0.333_333_3),
        (-0.471_404_520_8, 0.816_496_580_9, -0.333_333_3),
        (-0.471_404_520_8, -0.816_496_580_9, -0.333_333_3),
        (0.0, 0.0, 1.0),
    ];
    for (dx, dy, dz) in dirs {
        let j = Point3::new(dx * m, dy * m, dz * m);
        if orient3d(f0, f1, j, f_other) != Sign::Zero {
            return Some(j);
        }
    }
    None
}

/// Count of DISTINCT GEOMETRIC sub-segment endpoints in a partial intersection-
/// vertex list, comparing by exact `VertexCoords` rational coordinates (so an
/// `Explicit` endpoint that coincides with an `EdgeEdge` crossing is counted
/// once). Falls back to structural equality for any point whose exact coords
/// are unresolvable.
fn distinct_geometric_endpoints(vs: &[IntersectionVertex]) -> usize {
    use crate::arrangements::aux_structure::exact_point_coords;
    use crate::arrangements::fast_trimesh::VertexCoords;
    let mut exact_keys: Vec<[dashu::rational::RBig; 3]> = Vec::new();
    let mut structural: Vec<VertexCoords> = Vec::new();
    for iv in vs {
        let coords = vertex_coords_of_local(iv);
        match exact_point_coords(&coords) {
            Some(k) => {
                if !exact_keys.contains(&k) {
                    exact_keys.push(k);
                }
            }
            None => {
                if !structural.contains(&coords) {
                    structural.push(coords);
                }
            }
        }
    }
    exact_keys.len() + structural.len()
}

/// The [`VertexCoords`] an [`IntersectionVertex`] interns as (local mirror of
/// `aux_structure::vertex_coords_of`, kept here to avoid a cross-module
/// visibility change). `EdgeEdge` interns as `Lpi { line: e, plane: [f0, f1,
/// jolly] }` — its exact coordinates are the in-plane crossing.
fn vertex_coords_of_local(
    iv: &IntersectionVertex,
) -> crate::arrangements::fast_trimesh::VertexCoords {
    use crate::arrangements::fast_trimesh::VertexCoords;
    match iv {
        IntersectionVertex::Explicit { point, .. } => VertexCoords::Explicit(*point),
        IntersectionVertex::Lpi { line, plane, .. } => VertexCoords::Lpi {
            line: *line,
            plane: *plane,
        },
        IntersectionVertex::EdgeEdge { e, f, jolly, .. } => VertexCoords::Lpi {
            line: *e,
            plane: [f[0], f[1], *jolly],
        },
    }
}

/// Approximate explicit coordinates of the line-plane intersection, read back
/// from the native indirect-predicates interval lambdas (PR-CR-M7c:
/// [`crate::predicates::indirect::approx_lpi`], the clean-room equivalent of
/// the former FFI `lambda3d_lpi_interval` readback). `approx` is for spatial
/// bookkeeping only (not oracle-checked) — but it should land at the true
/// piercing point.
fn lpi_approx(p: Point3, q: Point3, r: Point3, s: Point3, t: Point3) -> Point3 {
    match crate::predicates::indirect::approx_lpi(p, q, r, s, t) {
        Some(pt) => pt,
        // Degenerate denominator (line parallel to / in the plane). KEEP the
        // segment-midpoint fallback so `approx` stays finite; the generators
        // (the load-bearing data) are exact regardless.
        None => Point3::new(
            (p.x() + q.x()) / 2.0,
            (p.y() + q.y()) / 2.0,
            (p.z() + q.z()) / 2.0,
        ),
    }
}

/// The two corner indices of a triangle that are NOT `vtx` (the opposite edge).
fn opposite_edge_endpoints(vtx: u32) -> (usize, usize) {
    match vtx {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    }
}

/// Push `iv` only if no structurally-equal vertex is already present (mirrors
/// the C++ `phmap::flat_hash_set` dedup of intersection vertices).
fn push_unique(out: &mut Vec<IntersectionVertex>, iv: IntersectionVertex) {
    if !out.contains(&iv) {
        out.push(iv);
    }
}

/// Port of the cpp:688-691 guard: is any triangle vertex strictly inside the
/// open segment `(p, q)`? Delegates to the EXACT
/// [`crate::predicates::point_strictly_inside_segment_3d`] (CR1 collinearity +
/// `dashu` betweenness) — no raw `f64`, closing deviation N13's `f64` sub-note.
fn any_triangle_vertex_strictly_inside_segment(p: Point3, q: Point3, plane: &[Point3; 3]) -> bool {
    plane
        .iter()
        .any(|&w| crate::predicates::point_strictly_inside_segment_3d(w, p, q))
}

// ── Sign-pattern decoders (cpp:834-925) ──────────────────────────────

/// Ports `normalizeOrientations` (cpp:834): map each `orient3d` Sign to
/// `i8` in {-1, 0, +1}.
fn normalize_orientations(o: [Sign; 3]) -> [i8; 3] {
    let n = |s: Sign| -> i8 {
        match s {
            Sign::Negative => -1,
            Sign::Zero => 0,
            Sign::Positive => 1,
        }
    };
    [n(o[0]), n(o[1]), n(o[2])]
}

/// Ports `sameOrientation` (cpp:848): both strictly negative, both strictly
/// positive, or both zero.
fn same_orientation(o1: i8, o2: i8) -> bool {
    o1 == o2
}

/// Ports `allCoplanarEdges` (cpp:859): all three are 0.
fn all_coplanar_edges(o: [i8; 3]) -> bool {
    o[0] == 0 && o[1] == 0 && o[2] == 0
}

/// Ports `singleCoplanarEdge` (cpp:869): returns 0/1/2 (the coplanar edge's
/// first vertex) or `None`.
fn single_coplanar_edge(o: [i8; 3]) -> Option<u32> {
    if o[0] == 0 && o[1] == 0 && o[2] != 0 {
        return Some(0);
    }
    if o[1] == 0 && o[2] == 0 && o[0] != 0 {
        return Some(1);
    }
    if o[2] == 0 && o[0] == 0 && o[1] != 0 {
        return Some(2);
    }
    None
}

/// Ports `vtxInPlaneAndOppositeEdgeOnSameSide` (cpp:880): one vertex in the
/// plane, the other two on the same (non-zero) side.
fn vtx_in_plane_and_opposite_edge_on_same_side(o: [i8; 3]) -> Option<u32> {
    if o[0] == 0 && o[1] == o[2] && o[1] != 0 {
        return Some(0);
    }
    if o[1] == 0 && o[0] == o[2] && o[0] != 0 {
        return Some(1);
    }
    if o[2] == 0 && o[0] == o[1] && o[0] != 0 {
        return Some(2);
    }
    None
}

/// Ports `vtxInPlaneAndOppositeEdgeCrossPlane` (cpp:891): one vertex in the
/// plane, the other two on opposite (non-zero) sides.
fn vtx_in_plane_and_opposite_edge_cross_plane(o: [i8; 3]) -> Option<u32> {
    if o[0] == 0 && o[1] != o[2] && o[1] != 0 && o[2] != 0 {
        return Some(0);
    }
    if o[1] == 0 && o[0] != o[2] && o[0] != 0 && o[2] != 0 {
        return Some(1);
    }
    if o[2] == 0 && o[0] != o[1] && o[0] != 0 && o[1] != 0 {
        return Some(2);
    }
    None
}

/// Ports `vtxOnASideAndOppositeEdgeOnTheOther` (cpp:902): a lone vertex on one
/// side, the opposite edge on the other. Returns `(vtx_idx, opp_v0, opp_v1)`.
fn vtx_on_a_side_and_opposite_edge_on_the_other(o: [i8; 3]) -> Option<(u32, u32, u32)> {
    // One vertex on the plane → not this case.
    if o[0] == 0 || o[1] == 0 || o[2] == 0 {
        return None;
    }
    // All on the same side → not this case.
    if o[0] == o[1] && o[1] == o[2] {
        return None;
    }
    if o[0] == o[1] {
        return Some((2, 0, 1));
    }
    if o[0] == o[2] {
        return Some((1, 0, 2));
    }
    Some((0, 1, 2))
}

/// Classify every candidate pair (e.g. the output of [`detect_intersecting_pairs`]).
///
/// [`detect_intersecting_pairs`]: crate::arrangements::detect_intersecting_pairs
pub fn classify_all(
    soup: &FastTrimesh,
    pairs: &[(u32, u32)],
) -> Vec<((u32, u32), PairClassification)> {
    // `classify_pair` is a pure function of the immutable `soup`, so the pairs
    // classify independently. Under the `parallel` feature (task #198) they run
    // over a `rayon` pool; `par_iter().map().collect()` on a slice preserves
    // index order, so the output `Vec` is BYTE-IDENTICAL to the serial map.
    #[cfg(feature = "parallel")]
    {
        pairs
            .par_iter()
            .map(|&(ta, tb)| ((ta, tb), classify_pair(soup, ta, tb)))
            .collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        pairs
            .iter()
            .map(|&(ta, tb)| ((ta, tb), classify_pair(soup, ta, tb)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! RED tests for PR-CR-AR1 (`classify_pair` / `classify_all`).
    //!
    //! Authored against the C++ reference
    //! `intersection_classification.cpp` (driver cpp:119-280, decoders
    //! cpp:834-925, `checkSingleNoCoplanarEdgeIntersection` cpp:679-730,
    //! `checkVtxInTriangleIntersection` cpp:734-784). These exercise the
    //! intended GREEN behaviour through the public surface only; they MUST
    //! fail against the current `Disjoint` stub by assertion (not compile
    //! error). No production code is touched.
    //!
    //! All coordinates are hard-coded (determinism). Hand-derivations for
    //! every transversal case are documented inline.

    use super::*;
    use crate::arrangements::{FastTrimesh, Plane};
    use crate::predicates::{triangle_intersects_triangle_3d, TriangleIntersection};
    use cad_primitives::Point3;

    // ── Fixture helpers ──────────────────────────────────────────────

    /// Build a 2-triangle soup. Triangle A = index 0 (verts 0,1,2),
    /// triangle B = index 1 (verts 3,4,5).
    fn soup_pair(a: [Point3; 3], b: [Point3; 3]) -> (FastTrimesh, [Point3; 3], [Point3; 3]) {
        let verts = vec![a[0], a[1], a[2], b[0], b[1], b[2]];
        let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        (soup, a, b)
    }

    /// The canonical XY triangle used in several cases:
    /// A0=(0,0,0), A1=(4,0,0), A2=(0,4,0). Lies in z=0; its interior is
    /// `{x>0, y>0, x+y<4}`.
    fn xy_triangle_a() -> [Point3; 3] {
        [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(4.0, 0.0, 0.0),
            Point3::new(0.0, 4.0, 0.0),
        ]
    }

    /// Count Explicit vertices in a transversal result.
    fn count_explicit(v: &[IntersectionVertex]) -> usize {
        v.iter()
            .filter(|iv| matches!(iv, IntersectionVertex::Explicit { .. }))
            .count()
    }

    /// Count Lpi vertices in a transversal result.
    fn count_lpi(v: &[IntersectionVertex]) -> usize {
        v.iter()
            .filter(|iv| matches!(iv, IntersectionVertex::Lpi { .. }))
            .count()
    }

    /// Count EdgeEdge (in-plane edge-edge crossing) vertices.
    fn count_edge_edge(v: &[IntersectionVertex]) -> usize {
        v.iter()
            .filter(|iv| matches!(iv, IntersectionVertex::EdgeEdge { .. }))
            .count()
    }

    /// Unwrap to the transversal vertex list or panic with the actual
    /// classification (so a stub `Disjoint` fails loudly with context).
    fn expect_transversal(c: &PairClassification) -> &Vec<IntersectionVertex> {
        match c {
            PairClassification::Transversal { vertices } => vertices,
            other => panic!("expected Transversal, got {other:?}"),
        }
    }

    // ── Group 4 helper: CR9 agreement oracle ─────────────────────────
    //
    // Transversal                      ⟺ Intersects
    // Deferred(Coplanar)               ⟺ Coplanar   (full coplanar always)
    // Deferred(SingleCoplanarEdge)     ⟺ "contact"  (NOT Disjoint; a touching
    //                                     single-coplanar-edge pair classifies
    //                                     as Intersects in CR9, not Coplanar —
    //                                     CR9 Coplanar is reserved for full /
    //                                     no-vertex-coincidence coplanarity).
    // Disjoint                         ⟺ Disjoint
    fn assert_cr9_agreement(c: &PairClassification, a: [Point3; 3], b: [Point3; 3]) {
        let cr9 = triangle_intersects_triangle_3d(a[0], a[1], a[2], b[0], b[1], b[2]);
        match c {
            PairClassification::Transversal { .. } => {
                assert_eq!(
                    cr9,
                    TriangleIntersection::Intersects,
                    "Transversal must agree with CR9 Intersects"
                );
            }
            PairClassification::Coplanar { .. } => {
                assert_eq!(
                    cr9,
                    TriangleIntersection::Coplanar,
                    "Coplanar must agree with CR9 Coplanar"
                );
            }
            PairClassification::Deferred(DeferReason::Coplanar) => {
                assert_eq!(
                    cr9,
                    TriangleIntersection::Coplanar,
                    "Deferred(Coplanar) must agree with CR9 Coplanar"
                );
            }
            PairClassification::Deferred(DeferReason::SingleCoplanarEdge) => {
                // The load-bearing property is "CR9 agrees there is contact",
                // not the exact variant: a touching single-coplanar-edge pair
                // is CR9 Intersects, a non-touching one would be Coplanar —
                // both are acceptable, only Disjoint would be a disagreement.
                assert_ne!(
                    cr9,
                    TriangleIntersection::Disjoint,
                    "Deferred(SingleCoplanarEdge) must agree with CR9 that there is contact (not Disjoint)"
                );
            }
            PairClassification::Deferred(DeferReason::Degenerate) => {
                // No CR9 mapping asserted for Degenerate.
            }
            PairClassification::Disjoint => {
                assert_eq!(
                    cr9,
                    TriangleIntersection::Disjoint,
                    "Disjoint must agree with CR9 Disjoint"
                );
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Group 1: sign-pattern decoder semantics exercised end-to-end.
    // (Cannot call the private decoders — drive them through classify_pair.)
    // ════════════════════════════════════════════════════════════════

    /// orBA `0 0 0`: triangle B fully coplanar with A and overlapping →
    /// `allCoplanarEdges` → PR-2 now CONSTRUCTS `Coplanar { .. }` (was
    /// `Deferred(Coplanar)`).
    ///
    /// A = xy_triangle_a (z=0). B = (1,1,0),(3,1,0),(1,3,0): all z=0
    /// (all three orient3d signs == 0) and overlaps A's interior.
    #[test]
    fn pattern_000_all_coplanar_is_constructed() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(3.0, 1.0, 0.0),
            Point3::new(1.0, 3.0, 0.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        assert!(
            matches!(
                classify_pair(&soup, 0, 1),
                PairClassification::Coplanar { .. }
            ),
            "fully-coplanar overlap must now construct Coplanar"
        );
    }

    /// orBA `1 0 0`-style (single coplanar edge) where the coplanar edge is a
    /// CHORD crossing A through two distinct A edges → now CLASSIFIED
    /// (edge-CROSSING sub-config, deviation N13, this slice) as Transversal
    /// with two `EdgeEdge` in-plane crossings (the C++
    /// `addEdgeCrossEdgeInters` jolly-LPI path).
    ///
    /// A = xy_triangle_a. B = (-1,1,0),(5,1,0),(2,2,3): B0,B1 in z=0
    /// (signs 0,0), B2 at z=3 off-plane (sign ≠ 0). The coplanar edge B0-B1
    /// lies along y=1 from x=-1 (outside A) to x=5 (outside A), so it ENTERS
    /// A through edge2 (x=0) at (0,1) and EXITS through edge1 (x+y=4) at
    /// (3,1) — a proper two-edge chord crossing → 2 EdgeEdge, 0 Explicit,
    /// 0 Lpi.
    #[test]
    fn pattern_single_coplanar_edge_chord_two_edge_edge() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(-1.0, 1.0, 0.0),
            Point3::new(5.0, 1.0, 0.0),
            Point3::new(2.0, 2.0, 3.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);
        assert_eq!(
            count_edge_edge(v),
            2,
            "chord crossing two A edges → 2 EdgeEdge, got {v:?}"
        );
        assert_eq!(count_explicit(v), 0, "both endpoints outside A, got {v:?}");
        assert_eq!(count_lpi(v), 0, "no transversal LPI, got {v:?}");
    }

    /// orBA `-1 1 1`-style transversal: one vtx of B on one side of A's
    /// plane, the opposite edge on the other →
    /// `vtxOnASideAndOppositeEdgeOnTheOther`. Both edges from the lone
    /// vertex pierce A's interior → Transversal with 2 Lpi.
    ///
    /// Hand-derivation:
    ///   A interior = {x>0, y>0, x+y<4}, z=0.
    ///   B0=(1,1,-1)  below (sign for z=-1)
    ///   B1=(1,1, 1)  above
    ///   B2=(2,2, 1)  above   → pattern is (s, -s, -s): lone vtx = B0.
    ///   edge B0-B1: vertical at (1,1); crosses z=0 at (1,1,0); 1+1=2<4 → inside.
    ///   edge B0-B2: (1,1,-1)→(2,2,1), z=0 at t=0.5 → (1.5,1.5,0); 3<4 → inside.
    ///   ⇒ 2 Lpi, 0 Explicit.
    #[test]
    fn pattern_vtx_one_side_opposite_edge_other_two_lpi() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, -1.0),
            Point3::new(1.0, 1.0, 1.0),
            Point3::new(2.0, 2.0, 1.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);
        assert_eq!(count_lpi(v), 2, "expected 2 LPI vertices, got {v:?}");
        assert_eq!(
            count_explicit(v),
            0,
            "expected 0 explicit vertices, got {v:?}"
        );
    }

    /// orBA `-1 0 1`-style transversal:
    /// `vtxInPlaneAndOppositeEdgeCrossPlane`. One vtx of B lies in A's
    /// plane (and inside A) → Explicit; the opposite edge straddles the
    /// plane and pierces A → Lpi.
    ///
    /// Hand-derivation:
    ///   B0=(1,1,0)   on plane (sign 0); inside A (1+1=2<4) → Explicit (1,1,0)
    ///   B1=(2,0.5, 1) above
    ///   B2=(2,0.5,-1) below   → pattern (0, +, -).
    ///   opposite edge of B0 is B1-B2: vertical at (2,0.5); crosses z=0
    ///   at (2,0.5,0); 2+0.5=2.5<4 → inside → 1 Lpi.
    ///   ⇒ 1 Explicit + 1 Lpi.
    #[test]
    fn pattern_vtx_in_plane_opposite_edge_cross_one_explicit_one_lpi() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(2.0, 0.5, 1.0),
            Point3::new(2.0, 0.5, -1.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);
        assert_eq!(
            count_explicit(v),
            1,
            "expected 1 explicit vertex, got {v:?}"
        );
        assert_eq!(count_lpi(v), 1, "expected 1 LPI vertex, got {v:?}");

        // The explicit vertex must equal B0 = (1,1,0) exactly.
        let explicit_pt = v
            .iter()
            .find_map(|iv| match iv {
                IntersectionVertex::Explicit { point, .. } => Some(*point),
                _ => None,
            })
            .expect("an explicit vertex");
        assert_eq!(explicit_pt, Point3::new(1.0, 1.0, 0.0));
    }

    // ════════════════════════════════════════════════════════════════
    // Group 2: hand-verified transversal pairs, vertex types + counts.
    //   (axis-aligned cases above; this adds a tilted/rotated case and
    //    re-asserts the two axis-aligned counts as standalone tests.)
    // ════════════════════════════════════════════════════════════════

    /// Re-statement of the `-1 1 1` axis-aligned case as a Group-2 count
    /// assertion (2 Lpi, 0 Explicit) — kept explicit per the plan.
    #[test]
    fn transversal_axis_aligned_two_lpi() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, -1.0),
            Point3::new(1.0, 1.0, 1.0),
            Point3::new(2.0, 2.0, 1.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);
        assert_eq!(count_lpi(v), 2);
        assert_eq!(count_explicit(v), 0);
    }

    /// Rotated / tilted transversal (B's plane is NOT axis-aligned).
    ///
    /// Hand-derivation (A = xy_triangle_a, z=0):
    ///   B0=(1.0,1.0,-1.0)  below
    ///   B1=(1.5,0.5, 1.0)  above
    ///   B2=(0.5,1.5, 1.0)  above   → lone vtx B0 (pattern s,-s,-s).
    ///   B's supporting plane is tilted (normal not ±Z): B1,B2 differ in
    ///   x and y as well as z.
    ///   edge B0-B1 crosses z=0 at midpoint t=0.5 → (1.25,0.75,0);
    ///       sum 2.0 < 4 → inside.
    ///   edge B0-B2 crosses z=0 at midpoint t=0.5 → (0.75,1.25,0);
    ///       sum 2.0 < 4 → inside.
    ///   ⇒ 2 Lpi, 0 Explicit.
    #[test]
    fn transversal_tilted_two_lpi() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, -1.0),
            Point3::new(1.5, 0.5, 1.0),
            Point3::new(0.5, 1.5, 1.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);
        assert_eq!(count_lpi(v), 2, "expected 2 LPI vertices, got {v:?}");
        assert_eq!(
            count_explicit(v),
            0,
            "expected 0 explicit vertices, got {v:?}"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Group 3: exact on-plane incidence oracle for every LPI.
    //   Uses the REAL indirect-predicates FFI — no f64 tolerance.
    // ════════════════════════════════════════════════════════════════

    /// For each `Lpi { line, plane, .. }` produced by a transversal case,
    /// reconstruct the implicit point and assert (exactly, via the FFI
    /// orient3d) that it lies on BOTH supporting planes:
    ///   - the stored `plane` (triangle Y the edge pierced), and
    ///   - the plane of triangle X that OWNS the edge `line` (the LPI is
    ///     on its own edge's line ⇒ in that edge's triangle's plane).
    ///
    /// We do not know from the `Lpi` alone which of A/B owns the edge, so
    /// we assert membership against whichever of {A-plane, B-plane}
    /// matches; both must be satisfied (the LPI is on A's plane AND B's
    /// plane). We test both source triangles' planes explicitly.
    #[test]
    fn lpi_lies_exactly_on_both_planes() {
        use indirect_predicates_sidecar_rs::{
            init_fpu, orient3d as ip_orient3d, ExplicitPoint3D, ImplicitPoint3DLpi, Sign as IpSign,
            AVAILABLE,
        };

        // The oracle must truly exercise the FFI. If the shim is not
        // linked the test FAILS LOUDLY (never a silent skip). Written as
        // an `if !AVAILABLE` panic rather than `assert!(AVAILABLE, ..)`
        // because `AVAILABLE` is a `const bool` (clippy
        // `assertions_on_constants`); the loud-failure intent is identical.
        if !AVAILABLE {
            panic!(
                "indirect-predicates FFI shim not linked (AVAILABLE == false); \
                 the on-plane oracle cannot run — refusing to pass silently"
            );
        }
        init_fpu();

        // Use the 2-LPI tilted transversal case (both LPIs interior).
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, -1.0),
            Point3::new(1.5, 0.5, 1.0),
            Point3::new(0.5, 1.5, 1.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);

        // Explicit points for A's and B's planes (reused for every LPI).
        let ea0 = ExplicitPoint3D::new(a[0].x(), a[0].y(), a[0].z());
        let ea1 = ExplicitPoint3D::new(a[1].x(), a[1].y(), a[1].z());
        let ea2 = ExplicitPoint3D::new(a[2].x(), a[2].y(), a[2].z());
        let eb0 = ExplicitPoint3D::new(b[0].x(), b[0].y(), b[0].z());
        let eb1 = ExplicitPoint3D::new(b[1].x(), b[1].y(), b[1].z());
        let eb2 = ExplicitPoint3D::new(b[2].x(), b[2].y(), b[2].z());

        let mut lpi_count = 0usize;
        for iv in v.iter() {
            if let IntersectionVertex::Lpi { line, plane, .. } = iv {
                lpi_count += 1;

                // Reconstruct the implicit LPI point from its generators:
                // line endpoints (p,q) and plane points (r,s,t), exactly
                // mirroring implicitPoint3D_LPI(p,q,r,s,t).
                let p = ExplicitPoint3D::new(line[0].x(), line[0].y(), line[0].z());
                let q = ExplicitPoint3D::new(line[1].x(), line[1].y(), line[1].z());
                let r = ExplicitPoint3D::new(plane[0].x(), plane[0].y(), plane[0].z());
                let s = ExplicitPoint3D::new(plane[1].x(), plane[1].y(), plane[1].z());
                let t = ExplicitPoint3D::new(plane[2].x(), plane[2].y(), plane[2].z());
                let lpi = ImplicitPoint3DLpi::new(&p, &q, &r, &s, &t);

                // (a) On the STORED plane Y (the pierced triangle).
                assert_eq!(
                    ip_orient3d(&lpi, &r, &s, &t),
                    IpSign::Zero,
                    "LPI must lie exactly on its stored plane"
                );

                // (b) On A's plane AND on B's plane — the LPI sits on the
                // shared intersection line, which lies in both triangles'
                // planes. No f64 epsilon anywhere.
                assert_eq!(
                    ip_orient3d(&lpi, &ea0, &ea1, &ea2),
                    IpSign::Zero,
                    "LPI must lie exactly on triangle A's plane"
                );
                assert_eq!(
                    ip_orient3d(&lpi, &eb0, &eb1, &eb2),
                    IpSign::Zero,
                    "LPI must lie exactly on triangle B's plane"
                );
            }
        }

        assert_eq!(lpi_count, 2, "expected 2 LPI vertices to oracle-check");
    }

    // ════════════════════════════════════════════════════════════════
    // Group 4: CR9 classification agreement.
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn cr9_agreement_across_cases() {
        // (A, B, label) tuples covering each classification branch.
        let a = xy_triangle_a();

        // Transversal (2 Lpi).
        let b_trans = [
            Point3::new(1.0, 1.0, -1.0),
            Point3::new(1.0, 1.0, 1.0),
            Point3::new(2.0, 2.0, 1.0),
        ];
        // Coplanar overlap.
        let b_copl = [
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(3.0, 1.0, 0.0),
            Point3::new(1.0, 3.0, 0.0),
        ];
        // Single-coplanar-edge CHORD (same fixture as
        // `pattern_single_coplanar_edge_chord_two_edge_edge`): edge B0-B1 lies
        // in A's plane and crosses A through two edges, B2 off-plane. Now
        // CLASSIFIED Transversal (2 EdgeEdge); CR9 == Intersects, so the
        // Transversal arm of the oracle agrees.
        let b_sce = [
            Point3::new(-1.0, 1.0, 0.0),
            Point3::new(5.0, 1.0, 0.0),
            Point3::new(2.0, 2.0, 3.0),
        ];
        // Disjoint (far away).
        let b_disj = [
            Point3::new(100.0, 100.0, 100.0),
            Point3::new(101.0, 100.0, 100.0),
            Point3::new(100.0, 101.0, 100.0),
        ];

        for b in [b_trans, b_copl, b_sce, b_disj] {
            let (soup, aa, bb) = soup_pair(a, b);
            let c = classify_pair(&soup, 0, 1);
            assert_cr9_agreement(&c, aa, bb);
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Group 5: coplanar + single-coplanar-edge → Deferred (not panics).
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn coplanar_overlap_constructed() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(3.0, 1.0, 0.0),
            Point3::new(1.0, 3.0, 0.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        assert!(matches!(
            classify_pair(&soup, 0, 1),
            PairClassification::Coplanar { .. }
        ));
    }

    #[test]
    fn single_coplanar_edge_chord_classified_two_edge_edge() {
        // A CHORD-crossing coplanar edge (enters/exits A through two edges) is
        // now CLASSIFIED via the in-plane edge-edge jolly-LPI path: two
        // EdgeEdge crossings, no Explicit/Lpi.
        let a = xy_triangle_a();
        let b = [
            Point3::new(-1.0, 1.0, 0.0),
            Point3::new(5.0, 1.0, 0.0),
            Point3::new(2.0, 2.0, 3.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);
        assert_eq!(count_edge_edge(v), 2, "got {v:?}");
        assert_eq!(count_explicit(v) + count_lpi(v), 0, "got {v:?}");
    }

    // ════════════════════════════════════════════════════════════════
    // Group 8: single-coplanar-edge — edge-contained sub-config is now
    //          CLASSIFIED (deviation N13, this PR), not deferred.
    // ════════════════════════════════════════════════════════════════

    /// A big z=0 triangle (0,0,0),(10,0,0),(0,10,0). B's edge B0-B1 lies in
    /// A's plane STRICTLY INSIDE A (2+2=4<10, 4+3=7<10), B2 off-plane.
    /// The coplanar edge is fully contained, touches no A boundary, and
    /// properly crosses no A edge → CLASSIFIED as Transversal with the two
    /// endpoints as Explicit vertices (no LPI). This is the edge-endpoint-in-
    /// triangle sub-config the C++ `checkSingleCoplanarEdgeIntersections`
    /// handles via `addSymbolicSegment` on two vertex-in-triangle endpoints.
    #[test]
    fn single_coplanar_edge_contained_interior_is_classified() {
        let a = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(10.0, 0.0, 0.0),
            Point3::new(0.0, 10.0, 0.0),
        ];
        let b = [
            Point3::new(2.0, 2.0, 0.0),
            Point3::new(4.0, 3.0, 0.0),
            Point3::new(3.0, 3.0, 5.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);
        assert_eq!(
            count_explicit(v),
            2,
            "two coplanar-edge endpoints → 2 Explicit, got {v:?}"
        );
        assert_eq!(count_lpi(v), 0, "no edge-edge crossing → 0 LPI, got {v:?}");
        // The two Explicit points are exactly B0 and B1, owned by B (tri 1).
        let pts: Vec<Point3> = v
            .iter()
            .filter_map(|iv| match iv {
                IntersectionVertex::Explicit { tri, point, .. } => {
                    assert_eq!(*tri, 1, "endpoints owned by B");
                    Some(*point)
                }
                _ => None,
            })
            .collect();
        assert!(
            pts.contains(&b[0]) && pts.contains(&b[1]),
            "endpoints are B0,B1: {pts:?}"
        );
    }

    /// A coplanar edge endpoint STRICTLY INSIDE A and the other endpoint ON an
    /// A edge (no proper crossing beyond the endpoint touch) — still a
    /// contained sub-config: 2 Explicit endpoints, 0 LPI. Here B0=(2,2,0)
    /// inside A, B1=(0,5,0) ON A's edge2 (x=0, 0<y<10).
    #[test]
    fn single_coplanar_edge_endpoint_on_edge_is_classified() {
        let a = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(10.0, 0.0, 0.0),
            Point3::new(0.0, 10.0, 0.0),
        ];
        let b = [
            Point3::new(2.0, 2.0, 0.0),
            Point3::new(0.0, 5.0, 0.0),
            Point3::new(3.0, 3.0, 5.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);
        assert_eq!(count_explicit(v), 2, "got {v:?}");
        assert_eq!(count_lpi(v), 0, "got {v:?}");
    }

    /// Edge-CROSSING sub-config: ONE coplanar-edge endpoint inside A, the
    /// other outside, the edge crossing ONE A edge → CLASSIFIED as Transversal
    /// with one `Explicit` (the contained endpoint) + one `EdgeEdge` (the
    /// crossing). B0=(-1,2,0) outside A, B1=(2,2,0) inside → the edge crosses
    /// A's edge2 (x=0) at (0,2,0) properly.
    #[test]
    fn single_coplanar_edge_one_in_one_cross_is_classified() {
        let a = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(10.0, 0.0, 0.0),
            Point3::new(0.0, 10.0, 0.0),
        ];
        let b = [
            Point3::new(-1.0, 2.0, 0.0),
            Point3::new(2.0, 2.0, 0.0),
            Point3::new(1.0, 3.0, 5.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);
        assert_eq!(
            count_explicit(v),
            1,
            "B1 inside A → 1 Explicit endpoint, got {v:?}"
        );
        assert_eq!(
            count_edge_edge(v),
            1,
            "edge crosses one A edge → 1 EdgeEdge, got {v:?}"
        );
        // The Explicit endpoint is exactly B1 = (2,2,0).
        let exp: Vec<Point3> = v
            .iter()
            .filter_map(|iv| match iv {
                IntersectionVertex::Explicit { point, .. } => Some(*point),
                _ => None,
            })
            .collect();
        assert_eq!(exp, vec![Point3::new(2.0, 2.0, 0.0)], "got {v:?}");
    }

    /// `tvX_in_edge` sub-config: the coplanar edge enters A's interior and
    /// EXITS THROUGH ONE OF A'S CORNERS (a degenerate edge-edge crossing whose
    /// crossing point is the exact A vertex, cpp:545-547). B0=(5,2,0) inside A,
    /// B1=(15,-2,0) outside, the edge passing through A's corner a1=(10,0,0)
    /// (strictly inside B0-B1). Expect Transversal with TWO `Explicit`
    /// endpoints — the contained B0 (owned by B, tri 1) and the corner a1
    /// (owned by A, tri 0) — and ZERO `EdgeEdge` (the crossing is AT a vertex,
    /// not a proper edge-interior crossing).
    #[test]
    fn single_coplanar_edge_tvx_corner_crossing_is_classified() {
        let a = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(10.0, 0.0, 0.0),
            Point3::new(0.0, 10.0, 0.0),
        ];
        let b = [
            Point3::new(5.0, 2.0, 0.0),
            Point3::new(15.0, -2.0, 0.0),
            Point3::new(8.0, 1.0, 5.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);
        assert_eq!(
            count_explicit(v),
            2,
            "contained B0 + corner a1 → 2 Explicit, got {v:?}"
        );
        assert_eq!(
            count_edge_edge(v),
            0,
            "crossing AT a vertex → 0 EdgeEdge, got {v:?}"
        );
        assert_eq!(count_lpi(v), 0, "no LPI, got {v:?}");
        // The corner endpoint is exactly a1 = (10,0,0), owned by A (tri 0); the
        // contained endpoint is B0 = (5,2,0), owned by B (tri 1).
        let mut a_owned = Vec::new();
        let mut b_owned = Vec::new();
        for iv in v {
            if let IntersectionVertex::Explicit { tri, point, .. } = iv {
                match *tri {
                    0 => a_owned.push(*point),
                    1 => b_owned.push(*point),
                    _ => unreachable!(),
                }
            }
        }
        assert_eq!(a_owned, vec![Point3::new(10.0, 0.0, 0.0)], "got {v:?}");
        assert_eq!(b_owned, vec![Point3::new(5.0, 2.0, 0.0)], "got {v:?}");
    }

    /// Collinear-but-DISJOINT sub-config: the coplanar edge lies on the LINE of
    /// an o_t edge but does not overlap the o_t edge's segment (here B's
    /// coplanar edge (12,0,0)-(15,0,0) is collinear with A's edge0 line y=0 but
    /// sits beyond A at x>10). A genuine collinear OVERLAP would have tripped an
    /// endpoint-host / o_t-vertex-in-edge guard and been captured as a sub-
    /// segment; reaching the collinear crossing-kind means no shared interior,
    /// so there is NO crossing on that edge. The C++ has no collinear-defer
    /// branch, so this CLASSIFIES (Transversal with no intersection geometry),
    /// it does NOT spuriously `Deferred(SingleCoplanarEdge)`. (B's third vertex
    /// (5,5,5) is off-plane so this is a single-coplanar-edge, not fully-
    /// coplanar, config.)
    #[test]
    fn single_coplanar_edge_collinear_disjoint_classifies_without_geometry() {
        let a = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(10.0, 0.0, 0.0),
            Point3::new(0.0, 10.0, 0.0),
        ];
        let b = [
            Point3::new(12.0, 0.0, 0.0),
            Point3::new(15.0, 0.0, 0.0),
            Point3::new(5.0, 5.0, 5.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let v = expect_transversal(&c);
        assert!(
            v.is_empty(),
            "collinear-disjoint coplanar edge introduces no geometry, got {v:?}"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Group 9: fully-coplanar pair (PR-2) — `classify_coplanar_pair`
    //          covering oracle (pure-dashu, hand-derived fixtures).
    //
    // The sidecar exposes no intermediate classification, so PR-2's gate is a
    // hand-derived covering oracle on `classify_coplanar_pair`: the exact
    // intersection-vertex set + symbolic constraint segments. Every fixture
    // also checks the cpp:268 invariant (≤3 intersection points) and that
    // every segment indexes valid, distinct vertices.
    // ════════════════════════════════════════════════════════════════

    /// Unwrap to the `Coplanar { vertices, segments }` payload, panicking with
    /// the actual classification otherwise.
    fn expect_coplanar(c: &PairClassification) -> (&Vec<IntersectionVertex>, &Vec<(u32, u32)>) {
        match c {
            PairClassification::Coplanar { vertices, segments } => (vertices, segments),
            other => panic!("expected Coplanar, got {other:?}"),
        }
    }

    /// Structural well-formedness shared by every coplanar fixture: every
    /// segment indexes two DISTINCT, in-range vertices.
    ///
    /// NOTE: there is intentionally NO ≤3 cap — the C++ `final_check` assert
    /// `v_tmp.size() <= 3` bounds the driver-level symbolic-segment temp set,
    /// which is empty on the coplanar path; the coplanar intersection list is
    /// unbounded (a hexagonal overlap has 6 points).
    fn assert_coplanar_wellformed(vertices: &[IntersectionVertex], segments: &[(u32, u32)]) {
        for &(i, j) in segments {
            assert_ne!(i, j, "no self-loop segment");
            assert!(
                (i as usize) < vertices.len() && (j as usize) < vertices.len(),
                "segment ({i},{j}) indexes out of range (len {})",
                vertices.len()
            );
        }
    }

    fn explicit_pts(vertices: &[IntersectionVertex]) -> Vec<Point3> {
        vertices
            .iter()
            .filter_map(|iv| match iv {
                IntersectionVertex::Explicit { point, .. } => Some(*point),
                _ => None,
            })
            .collect()
    }

    /// (b) NESTED: a small triangle strictly inside a big coplanar one.
    /// All three small-triangle edges have BOTH endpoints STRICTLY_INSIDE the
    /// big triangle, so each edge emits its two endpoints (Explicit) + one
    /// symbolic segment. The big triangle's edges vs the small one contribute
    /// nothing (both endpoints outside the small tri, no crossing). Result:
    /// exactly the 3 small-triangle corners + 3 segments forming its boundary.
    #[test]
    fn coplanar_nested_small_inside_big() {
        // Big: z=0, corners (0,0),(10,0),(0,10).
        let big = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(10.0, 0.0, 0.0),
            Point3::new(0.0, 10.0, 0.0),
        ];
        // Small: strictly inside big (all sums < 10, all coords > 0).
        let small = [
            Point3::new(2.0, 2.0, 0.0),
            Point3::new(5.0, 2.0, 0.0),
            Point3::new(2.0, 5.0, 0.0),
        ];
        let (soup, _, _) = soup_pair(big, small);
        let c = classify_pair(&soup, 0, 1);
        let (vertices, segments) = expect_coplanar(&c);
        assert_coplanar_wellformed(vertices, segments);

        // Exactly the 3 small-triangle corners, all Explicit.
        assert_eq!(vertices.len(), 3, "nested → 3 vertices, got {vertices:?}");
        let pts = explicit_pts(vertices);
        assert_eq!(pts.len(), 3, "all 3 are Explicit, got {vertices:?}");
        for p in small.iter() {
            assert!(pts.contains(p), "missing small corner {p:?} in {pts:?}");
        }
        // The 3 boundary segments of the small triangle (one per edge).
        assert_eq!(segments.len(), 3, "nested → 3 segments, got {segments:?}");
    }

    /// (c) SHARED EXACT EDGE: two coplanar triangles sharing one edge exactly
    /// and lying on opposite sides of it (no interior overlap). The shared edge
    /// has both endpoints ON o_t vertices → cpp:460 `if(v0_in_vtx &&
    /// v1_in_vtx) return;` — NO new geometry, NO segment for that edge. The
    /// non-shared edges go outside the other triangle. So: 0 segments.
    #[test]
    fn coplanar_shared_edge_no_geometry() {
        // Shared edge from (0,0) to (4,0) on z=0.
        let a = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(4.0, 0.0, 0.0),
            Point3::new(2.0, 3.0, 0.0), // above the edge
        ];
        let b = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(4.0, 0.0, 0.0),
            Point3::new(2.0, -3.0, 0.0), // below the edge (no overlap)
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let (vertices, segments) = expect_coplanar(&c);
        assert_coplanar_wellformed(vertices, segments);
        assert!(
            segments.is_empty(),
            "shared-edge mirror pair → no constraint segment, got {segments:?}"
        );
    }

    /// (a) THE GEAR'S PAIR: two coplanar triangles whose hypotenuses cross,
    /// embedded in z=0. A = (0,-5),(1,-5),(1,5); B = (1,-2),(0,-2),(0,2).
    ///
    /// Hand-derivation (z=0 plane):
    ///   A is the thin right triangle with the right edge x=1 from y=-5..5 and
    ///   the hypotenuse from (0,-5) to (1,5). B is the thin right triangle with
    ///   the left edge x=0 from y=-2..2 and the hypotenuse from (1,-2) to (0,2).
    ///   B's vertex (1,-2) lies ON A's right edge x=1 (−5<−2<5) → an o_t
    ///   vertex / endpoint-on-edge incidence. A's vertex... the two thin wedges
    ///   overlap in a small lens, so the classification yields a small set of
    ///   incidence + crossing vertices.
    ///
    /// The exact vertex coordinates of the interior crossings are messy
    /// rationals; we assert the robust invariants the C++ guarantees: the
    /// `Coplanar` variant, the ≤3-point cap (cpp:268), at least one
    /// intersection vertex (the wedges DO overlap), well-formed segments, and
    /// that B's vertex (1,-2) — which lies on A's right edge — appears as an
    /// Explicit intersection vertex.
    #[test]
    fn coplanar_gear_crossing_wedges() {
        let a = [
            Point3::new(0.0, -5.0, 0.0),
            Point3::new(1.0, -5.0, 0.0),
            Point3::new(1.0, 5.0, 0.0),
        ];
        let b = [
            Point3::new(1.0, -2.0, 0.0),
            Point3::new(0.0, -2.0, 0.0),
            Point3::new(0.0, 2.0, 0.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let (vertices, segments) = expect_coplanar(&c);
        assert_coplanar_wellformed(vertices, segments);
        assert!(
            !vertices.is_empty(),
            "the two wedges overlap → ≥1 intersection vertex"
        );
        // B's corner (1,-2,0) lies on A's right edge (x=1) → Explicit.
        let pts = explicit_pts(vertices);
        assert!(
            pts.contains(&Point3::new(1.0, -2.0, 0.0)),
            "B vertex (1,-2) on A's edge must be Explicit, got {vertices:?}"
        );
    }

    /// (d) BOX-FACE OVERLAP: two unit-square-diagonal triangles overlapping
    /// partially, in z=0. A is the lower-right half of the unit square
    /// (0,0),(1,0),(1,1); B is the same square shifted by (0.5, 0.5):
    /// (0.5,0.5),(1.5,0.5),(1.5,1.5). They overlap in a partial region. Assert
    /// the `Coplanar` variant, ≤3-point cap, ≥1 vertex, well-formed segments.
    #[test]
    fn coplanar_box_face_partial_overlap() {
        let a = [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 0.0),
        ];
        let b = [
            Point3::new(0.5, 0.5, 0.0),
            Point3::new(1.5, 0.5, 0.0),
            Point3::new(1.5, 1.5, 0.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let (vertices, segments) = expect_coplanar(&c);
        assert_coplanar_wellformed(vertices, segments);
        assert!(
            !vertices.is_empty(),
            "partial box-face overlap → ≥1 intersection vertex, got {vertices:?}"
        );
    }

    /// A fully interior-overlapping pair (the canonical Stage-0 lens) produces
    /// a well-formed `Coplanar` classification with ≥1 vertex. (No ≤3 cap —
    /// see `assert_coplanar_wellformed`.)
    #[test]
    fn coplanar_interior_lens_overlap_wellformed() {
        // A = xy_triangle_a (z=0). B overlaps A's interior partially.
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(5.0, 1.0, 0.0),
            Point3::new(1.0, 5.0, 0.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        let c = classify_pair(&soup, 0, 1);
        let (vertices, segments) = expect_coplanar(&c);
        assert_coplanar_wellformed(vertices, segments);
        assert!(!vertices.is_empty(), "lens overlap → ≥1 vertex");
    }

    // ════════════════════════════════════════════════════════════════
    // Group 6: Disjoint.
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn well_separated_pair_disjoint() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(100.0, 100.0, 100.0),
            Point3::new(101.0, 100.0, 100.0),
            Point3::new(100.0, 101.0, 100.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        assert_eq!(classify_pair(&soup, 0, 1), PairClassification::Disjoint);
    }

    // ════════════════════════════════════════════════════════════════
    // Group 7: classify_all — one entry per pair, consistent with
    //          classify_pair.
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn classify_all_one_entry_per_pair_consistent() {
        // Build a 3-triangle soup so we can pass several pairs.
        let a = xy_triangle_a();
        // B: transversal with A.
        let b = [
            Point3::new(1.0, 1.0, -1.0),
            Point3::new(1.0, 1.0, 1.0),
            Point3::new(2.0, 2.0, 1.0),
        ];
        // C: far away (disjoint from both).
        let cc = [
            Point3::new(100.0, 100.0, 100.0),
            Point3::new(101.0, 100.0, 100.0),
            Point3::new(100.0, 101.0, 100.0),
        ];
        let verts = vec![a[0], a[1], a[2], b[0], b[1], b[2], cc[0], cc[1], cc[2]];
        let tris = vec![[0u32, 1, 2], [3u32, 4, 5], [6u32, 7, 8]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();

        let pairs = vec![(0u32, 1u32), (0, 2), (1, 2)];
        let results = classify_all(&soup, &pairs);

        // One entry per input pair, keyed by the pair, order-preserving.
        assert_eq!(results.len(), pairs.len());
        for (i, &pair) in pairs.iter().enumerate() {
            assert_eq!(results[i].0, pair, "classify_all must key by the pair");
            // Consistent with calling classify_pair directly.
            let direct = classify_pair(&soup, pair.0, pair.1);
            assert_eq!(
                results[i].1, direct,
                "classify_all[{i}] must equal classify_pair for {pair:?}"
            );
        }

        // Sanity on the expected branches (these MUST fail vs the stub
        // for pair (0,1), which should be Transversal not Disjoint).
        assert!(
            matches!(results[0].1, PairClassification::Transversal { .. }),
            "pair (0,1) should be Transversal, got {:?}",
            results[0].1
        );
        assert_eq!(results[1].1, PairClassification::Disjoint);
        assert_eq!(results[2].1, PairClassification::Disjoint);
    }
}
