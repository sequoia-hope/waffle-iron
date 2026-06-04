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
//! **First FFI consumer inside `cherchi-rs`**: the LPI point construction uses
//! `indirect-predicates-sidecar-rs`, so the whole module is gated behind the
//! off-by-default `indirect-predicates` feature (WASM builds with it off).
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

use crate::arrangements::FastTrimesh;
use crate::predicates::{
    orient3d, point_in_triangle_3d, segment_intersects_triangle_3d, PointLocation,
    SegmentTriangleIntersection, Sign,
};
use cad_primitives::Point3;
use indirect_predicates_sidecar_rs::{init_fpu, lambda3d_lpi_interval, IntervalNumber};

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

    // Coplanar / single-coplanar-edge deferral (deviation N13). The C++
    // handles these via `checkSingleCoplanarEdgeIntersections` (jolly points +
    // in-plane edge-edge LPIs), which AR1 does not construct — defer loudly.
    if all_coplanar_edges(or_ba) {
        return PairClassification::Deferred(DeferReason::Coplanar);
    }

    // We need orAB for the single-coplanar-edge deferral check and for the
    // A-vs-B transversal side (cpp:202-219).
    let or_ab = normalize_orientations([
        orient3d(a0, b0, b1, b2),
        orient3d(a1, b0, b1, b2),
        orient3d(a2, b0, b1, b2),
    ]);

    if single_coplanar_edge(or_ba).is_some() || single_coplanar_edge(or_ab).is_some() {
        return PairClassification::Deferred(DeferReason::SingleCoplanarEdge);
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

/// Approximate explicit coordinates of the line-plane intersection, read back
/// from the indirect-predicates interval lambdas. `approx` is for spatial
/// bookkeeping only (not oracle-checked) — but it should land at the true
/// piercing point. `lambda_d` may be negative, so divide using the true ratio
/// (interval midpoints).
fn lpi_approx(p: Point3, q: Point3, r: Point3, s: Point3, t: Point3) -> Point3 {
    // One-time FPU init (idempotent); safe to call repeatedly.
    init_fpu();

    let iv = |pt: Point3| -> [IntervalNumber; 3] {
        [
            IntervalNumber::point(pt.x()),
            IntervalNumber::point(pt.y()),
            IntervalNumber::point(pt.z()),
        ]
    };
    let res = lambda3d_lpi_interval(iv(p), iv(q), iv(r), iv(s), iv(t));
    let mid = |n: IntervalNumber| -> f64 { (n.inf + n.sup) / 2.0 };

    let d = mid(res.lambda_d);
    if d == 0.0 {
        // Degenerate denominator (line parallel to / in the plane). Fall back to
        // the segment midpoint so `approx` stays finite; the generators (the
        // load-bearing data) are exact regardless.
        return Point3::new(
            (p.x() + q.x()) / 2.0,
            (p.y() + q.y()) / 2.0,
            (p.z() + q.z()) / 2.0,
        );
    }
    Point3::new(
        mid(res.lambda_x) / d,
        mid(res.lambda_y) / d,
        mid(res.lambda_z) / d,
    )
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

/// Conservative port of the cpp:688-691 guard: is any triangle vertex strictly
/// inside the open segment `(p, q)`? Exact (collinear + strictly-between on
/// every coordinate axis), no tolerance.
fn any_triangle_vertex_strictly_inside_segment(p: Point3, q: Point3, plane: &[Point3; 3]) -> bool {
    plane
        .iter()
        .any(|&w| point_strictly_inside_segment(w, p, q))
}

/// True iff `w` is collinear with `(p, q)` and lies strictly between them
/// (excludes the endpoints). Exact.
fn point_strictly_inside_segment(w: Point3, p: Point3, q: Point3) -> bool {
    if w == p || w == q {
        return false;
    }
    // Collinearity: (q - p) × (w - p) == 0.
    let dx1 = q.x() - p.x();
    let dy1 = q.y() - p.y();
    let dz1 = q.z() - p.z();
    let dx2 = w.x() - p.x();
    let dy2 = w.y() - p.y();
    let dz2 = w.z() - p.z();
    let cx = dy1 * dz2 - dz1 * dy2;
    let cy = dz1 * dx2 - dx1 * dz2;
    let cz = dx1 * dy2 - dy1 * dx2;
    if cx != 0.0 || cy != 0.0 || cz != 0.0 {
        return false;
    }
    // Strictly between: w - p and w - q point in opposite directions on the
    // line (dot of (w-p)·(w-q) < 0).
    let ex = w.x() - q.x();
    let ey = w.y() - q.y();
    let ez = w.z() - q.z();
    dx2 * ex + dy2 * ey + dz2 * ez < 0.0
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
    pairs
        .iter()
        .map(|&(ta, tb)| ((ta, tb), classify_pair(soup, ta, tb)))
        .collect()
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
    // Transversal  ⟺ Intersects
    // Deferred(Coplanar|SingleCoplanarEdge) ⟺ Coplanar
    // Disjoint     ⟺ Disjoint
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
            PairClassification::Deferred(DeferReason::Coplanar)
            | PairClassification::Deferred(DeferReason::SingleCoplanarEdge) => {
                assert_eq!(
                    cr9,
                    TriangleIntersection::Coplanar,
                    "Deferred(Coplanar|SingleCoplanarEdge) must agree with CR9 Coplanar"
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
    /// `allCoplanarEdges` → Deferred(Coplanar).
    ///
    /// A = xy_triangle_a (z=0). B = (1,1,0),(3,1,0),(1,3,0): all z=0
    /// (all three orient3d signs == 0) and overlaps A's interior.
    #[test]
    fn pattern_000_all_coplanar_is_deferred_coplanar() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(3.0, 1.0, 0.0),
            Point3::new(1.0, 3.0, 0.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        assert_eq!(
            classify_pair(&soup, 0, 1),
            PairClassification::Deferred(DeferReason::Coplanar)
        );
    }

    /// orBA `1 0 0`-style (single coplanar edge): exactly one edge of B
    /// lies in A's plane, the third vertex off-plane →
    /// `singleCoplanarEdge` → Deferred(SingleCoplanarEdge).
    ///
    /// A = xy_triangle_a. B = (1,1,0),(3,1,0),(2,2,3): B0,B1 in z=0
    /// (signs 0,0), B2 at z=3 off-plane (sign ≠ 0). The coplanar edge
    /// B0-B1 lies along y=1, 1≤x≤3, which crosses A's interior.
    #[test]
    fn pattern_single_coplanar_edge_is_deferred() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(3.0, 1.0, 0.0),
            Point3::new(2.0, 2.0, 3.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        assert_eq!(
            classify_pair(&soup, 0, 1),
            PairClassification::Deferred(DeferReason::SingleCoplanarEdge)
        );
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
        // Disjoint (far away).
        let b_disj = [
            Point3::new(100.0, 100.0, 100.0),
            Point3::new(101.0, 100.0, 100.0),
            Point3::new(100.0, 101.0, 100.0),
        ];

        for b in [b_trans, b_copl, b_disj] {
            let (soup, aa, bb) = soup_pair(a, b);
            let c = classify_pair(&soup, 0, 1);
            assert_cr9_agreement(&c, aa, bb);
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Group 5: coplanar + single-coplanar-edge → Deferred (not panics).
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn coplanar_overlap_deferred() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(3.0, 1.0, 0.0),
            Point3::new(1.0, 3.0, 0.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        assert_eq!(
            classify_pair(&soup, 0, 1),
            PairClassification::Deferred(DeferReason::Coplanar)
        );
    }

    #[test]
    fn single_coplanar_edge_deferred() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(3.0, 1.0, 0.0),
            Point3::new(2.0, 2.0, 3.0),
        ];
        let (soup, _, _) = soup_pair(a, b);
        assert_eq!(
            classify_pair(&soup, 0, 1),
            PairClassification::Deferred(DeferReason::SingleCoplanarEdge)
        );
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
