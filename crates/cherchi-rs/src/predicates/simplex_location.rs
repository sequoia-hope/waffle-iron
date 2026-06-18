//! Ported from cinolib's robust geometric predicates (MIT).
//! © Marco Livesu et al. — https://github.com/mlivesu/cinolib
//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! # Finer simplex-location predicates (for `checkSingleCoplanarEdgeIntersections`)
//!
//! The coarse [`super::point_in_triangle_3d`] collapses cinolib's granular
//! ON_VERTi / ON_EDGEj boundary info into a single `OnBoundary`. The
//! single-coplanar-edge classifier (Cherchi 2022
//! `intersection_classification.cpp::checkSingleCoplanarEdgeIntersections`,
//! cpp:422-657) needs to know *which* vertex or edge of the other triangle a
//! coplanar-edge endpoint lands on, to place a vertex-in-edge constraint
//! correctly. This module ports the finer cinolib predicates it consumes:
//!
//! - [`point_in_segment_3d`] (cinolib `predicates.cpp:352-369`) →
//!   [`SegmentLocation`] (ON_VERT0/1, STRICTLY_INSIDE, STRICTLY_OUTSIDE).
//! - [`point_in_triangle_3d_loc`] (cinolib `predicates.cpp:447-481`) →
//!   [`TriangleLocation`] (ON_VERT0/1/2, ON_EDGE0/1/2, STRICTLY_INSIDE,
//!   STRICTLY_OUTSIDE). Edge `j` connects corners `j` and `(j+1)%3`.
//! - [`segment_segment_intersect_3d`] (cinolib `predicates.cpp:668-714`) →
//!   [`SegSegIntersection`] (DO_NOT_INTERSECT, SIMPLICIAL_COMPLEX, INTERSECT,
//!   OVERLAP).
//!
//! All EXACT, no tolerance: collinearity via [`super::points_are_collinear_3d`]
//! (Shewchuk exact `orient2d` on the three axis-drops), betweenness via
//! exact `dashu::rational` comparisons over the `f64` coordinates, and the 2D
//! segment-segment test via Shewchuk exact `orient2d`. Mirrors `cinolib::orient2d`
//! arg order (test point last in `orient2d(a, b, p)`).

use cad_primitives::Point3;

use super::points_are_collinear_3d;

/// Location of a point relative to a 3D segment `s = (s0, s1)`.
///
/// Ports cinolib's `PointInSimplex` subset used for segments
/// (`predicates.h:92-96`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SegmentLocation {
    /// Coincides with `s0`.
    OnVert0,
    /// Coincides with `s1`.
    OnVert1,
    /// Strictly between the endpoints (endpoints excluded).
    StrictlyInside,
    /// Anywhere else (off the line, or beyond an endpoint).
    StrictlyOutside,
}

/// Location of a point relative to a 3D triangle `t = (t0, t1, t2)`.
///
/// Ports cinolib's `PointInSimplex` subset used for triangles
/// (`predicates.h:92-100`). Edge `j` connects corners `j` and `(j+1)%3`:
/// `OnEdge0 = (t0,t1)`, `OnEdge1 = (t1,t2)`, `OnEdge2 = (t2,t0)`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TriangleLocation {
    OnVert0,
    OnVert1,
    OnVert2,
    OnEdge0,
    OnEdge1,
    OnEdge2,
    StrictlyInside,
    StrictlyOutside,
}

/// Result of a 3D coplanar segment-segment intersection test.
///
/// Ports cinolib's `SimplexIntersection` (`predicates.h:116-119`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SegSegIntersection {
    /// Segments are fully disjoint (or non-coplanar in 3D).
    DoNotIntersect,
    /// Segments coincide or intersect only at a shared endpoint.
    SimplicialComplex,
    /// Segments cross at an inner point.
    Intersect,
    /// Segments are collinear and partially overlap.
    Overlap,
}

/// Exact `orient2d` sign as `i8` in {-1, 0, +1}, matching cinolib's
/// `(det>0)?1:((det<0)?-1:0)` reduction (`predicates.cpp:594-597`).
/// Arg order mirrors `cinolib::orient2d(a, b, p)` — test point last.
fn orient2d_sign(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> i8 {
    let det = geometry_predicates::orient2d(a, b, p);
    if det > 0.0 {
        1
    } else if det < 0.0 {
        -1
    } else {
        0
    }
}

/// Classify a point `p` relative to the 3D segment `(s0, s1)`.
///
/// Ports cinolib `point_in_segment_3d` (`predicates.cpp:352-369`):
/// vertex-coincidence first (exact `Point3` equality), then exact
/// collinearity, then exact strict-betweenness. The cinolib min/max
/// coordinate test is replaced by the EXACT [`super::point_strictly_inside_segment_3d`]
/// (which the existing CR collinearity module already provides over `dashu`),
/// preserving the STRICTLY_INSIDE semantics without raw-`f64` comparison.
pub fn point_in_segment_3d(p: Point3, s0: Point3, s1: Point3) -> SegmentLocation {
    if p == s0 {
        return SegmentLocation::OnVert0;
    }
    if p == s1 {
        return SegmentLocation::OnVert1;
    }
    if !points_are_collinear_3d(s0, s1, p) {
        return SegmentLocation::StrictlyOutside;
    }
    if super::point_strictly_inside_segment_3d(p, s0, s1) {
        SegmentLocation::StrictlyInside
    } else {
        SegmentLocation::StrictlyOutside
    }
}

/// Classify a point `p` relative to the 3D triangle `(t0, t1, t2)`, returning
/// the granular simplex location.
///
/// Ports cinolib `point_in_triangle_3d` (`predicates.cpp:447-481`): vertex
/// coincidence, then on-edge via [`point_in_segment_3d`] STRICTLY_INSIDE on
/// each of the three edges (edge `j` = corners `j`, `(j+1)%3`), then the
/// interior via the three axis-drop 2D projections AND-combined (any
/// projection STRICTLY_OUTSIDE → outside; else inside).
pub fn point_in_triangle_3d_loc(p: Point3, t0: Point3, t1: Point3, t2: Point3) -> TriangleLocation {
    if p == t0 {
        return TriangleLocation::OnVert0;
    }
    if p == t1 {
        return TriangleLocation::OnVert1;
    }
    if p == t2 {
        return TriangleLocation::OnVert2;
    }

    if point_in_segment_3d(p, t0, t1) == SegmentLocation::StrictlyInside {
        return TriangleLocation::OnEdge0;
    }
    if point_in_segment_3d(p, t1, t2) == SegmentLocation::StrictlyInside {
        return TriangleLocation::OnEdge1;
    }
    if point_in_segment_3d(p, t2, t0) == SegmentLocation::StrictlyInside {
        return TriangleLocation::OnEdge2;
    }

    // Interior: project on each axis-drop and require none reports
    // StrictlyOutside (cinolib's all-three-projection AND). cinolib's
    // point_in_triangle_2d returns STRICTLY_INSIDE/OUTSIDE/ON_* — here only
    // the STRICTLY_OUTSIDE verdict short-circuits (vertex/edge cases were
    // already handled in 3D above), matching cpp:468/473/478.
    let drop_x = |q: Point3| [q.y(), q.z()];
    let drop_y = |q: Point3| [q.x(), q.z()];
    let drop_z = |q: Point3| [q.x(), q.y()];
    let projections = [
        (drop_x(p), drop_x(t0), drop_x(t1), drop_x(t2)),
        (drop_y(p), drop_y(t0), drop_y(t1), drop_y(t2)),
        (drop_z(p), drop_z(t0), drop_z(t1), drop_z(t2)),
    ];
    for (p2, a2, b2, c2) in projections {
        if point_in_triangle_2d_is_strictly_outside(p2, a2, b2, c2) {
            return TriangleLocation::StrictlyOutside;
        }
    }
    TriangleLocation::StrictlyInside
}

/// True iff `p` is STRICTLY_OUTSIDE triangle `(a, b, c)` in 2D, by cinolib's
/// `point_in_triangle_2d` sign rule (`predicates.cpp:404-420`): the three
/// `orient2d` half-plane signs must NOT all be `>= 0` nor all `<= 0`.
fn point_in_triangle_2d_is_strictly_outside(
    p: [f64; 2],
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
) -> bool {
    let e0 = orient2d_sign(a, b, p);
    let e1 = orient2d_sign(b, c, p);
    let e2 = orient2d_sign(c, a, p);
    let all_nonneg = e0 >= 0 && e1 >= 0 && e2 >= 0;
    let all_nonpos = e0 <= 0 && e1 <= 0 && e2 <= 0;
    !(all_nonneg || all_nonpos)
}

/// 2D segment-segment intersection, EXACT.
///
/// Ports cinolib `segment_segment_intersect_2d` (`predicates.cpp:581-641`):
/// the four `orient2d` cross-side signs decide a single crossing point
/// (shared endpoint → SIMPLICIAL_COMPLEX, otherwise INTERSECT); the all-zero
/// collinear branch decides OVERLAP via exact betweenness of any endpoint of
/// one segment inside the other (replacing the raw-`f64` min/max test with
/// the exact [`super::point_strictly_inside_segment_3d`] on the lifted 3D
/// points — both inputs lie in the shared 2D plane so this is exact).
fn segment_segment_intersect_2d(
    s00: Point3,
    s01: Point3,
    s10: Point3,
    s11: Point3,
    proj: fn(Point3) -> [f64; 2],
) -> SegSegIntersection {
    let q = |x: Point3| proj(x);
    let det_s00 = orient2d_sign(q(s10), q(s11), q(s00));
    let det_s01 = orient2d_sign(q(s10), q(s11), q(s01));
    let det_s10 = orient2d_sign(q(s00), q(s01), q(s10));
    let det_s11 = orient2d_sign(q(s00), q(s01), q(s11));

    // Single crossing point.
    if det_s00 != det_s01 && det_s10 != det_s11 {
        if eq2(q(s00), q(s10)) || eq2(q(s00), q(s11)) || eq2(q(s01), q(s10)) || eq2(q(s01), q(s11))
        {
            return SegSegIntersection::SimplicialComplex;
        }
        return SegSegIntersection::Intersect;
    }

    // Collinear (all four signs zero).
    if det_s00 == 0 && det_s01 == 0 && det_s10 == 0 && det_s11 == 0 {
        if (eq2(q(s00), q(s10)) && eq2(q(s01), q(s11)))
            || (eq2(q(s00), q(s11)) && eq2(q(s01), q(s10)))
        {
            return SegSegIntersection::SimplicialComplex;
        }
        // OVERLAP iff any endpoint of one segment lies strictly inside the
        // other (exact, on the 3D points which are collinear in this branch).
        if super::point_strictly_inside_segment_3d(s00, s10, s11)
            || super::point_strictly_inside_segment_3d(s01, s10, s11)
            || super::point_strictly_inside_segment_3d(s10, s00, s01)
            || super::point_strictly_inside_segment_3d(s11, s00, s01)
        {
            return SegSegIntersection::Intersect;
        }
    }
    SegSegIntersection::DoNotIntersect
}

fn eq2(a: [f64; 2], b: [f64; 2]) -> bool {
    a[0] == b[0] && a[1] == b[1]
}

/// 3D segment-segment intersection, EXACT.
///
/// Ports cinolib `segment_segment_intersect_3d` (`predicates.cpp:668-714`):
/// non-coplanar → DO_NOT_INTERSECT; coincident → SIMPLICIAL_COMPLEX; else the
/// three axis-drop 2D tests (`DO_NOT_INTERSECT` in any projection → disjoint;
/// `OVERLAP` in ≥2 projections → overlap; else INTERSECT).
pub fn segment_segment_intersect_3d(
    s00: Point3,
    s01: Point3,
    s10: Point3,
    s11: Point3,
) -> SegSegIntersection {
    // Coplanarity of the 4 segment endpoints (exact orient3d == 0).
    if super::orient3d(s00, s01, s10, s11) != super::Sign::Zero {
        return SegSegIntersection::DoNotIntersect;
    }

    let s00_sh = s00 == s10 || s00 == s11;
    let s01_sh = s01 == s10 || s01 == s11;
    let s10_sh = s10 == s00 || s10 == s01;
    let s11_sh = s11 == s00 || s11 == s01;
    if s00_sh && s01_sh && s10_sh && s11_sh {
        return SegSegIntersection::SimplicialComplex;
    }

    let drop_x = |q: Point3| [q.y(), q.z()];
    let drop_y = |q: Point3| [q.x(), q.z()];
    let drop_z = |q: Point3| [q.x(), q.y()];

    let x_res = segment_segment_intersect_2d(s00, s01, s10, s11, drop_x);
    if x_res == SegSegIntersection::DoNotIntersect {
        return SegSegIntersection::DoNotIntersect;
    }
    let y_res = segment_segment_intersect_2d(s00, s01, s10, s11, drop_y);
    if y_res == SegSegIntersection::DoNotIntersect {
        return SegSegIntersection::DoNotIntersect;
    }
    let z_res = segment_segment_intersect_2d(s00, s01, s10, s11, drop_z);
    if z_res == SegSegIntersection::DoNotIntersect {
        return SegSegIntersection::DoNotIntersect;
    }

    // cinolib deems the pair overlapping iff ≥2 of 3 projections report
    // OVERLAP (predicates.cpp:705-710). NOTE: the 2D routine
    // (`segment_segment_intersect_2d`) never returns OVERLAP — its collinear
    // branch returns INTERSECT (predicates.cpp:637) — so in practice this
    // count is always 0 and a collinear-overlapping pair reports INTERSECT.
    // The aggregation is ported verbatim for faithfulness; it is not dead in
    // intent (it tracks the cinolib enum contract), only in the current 2D
    // implementation.
    let overlap_count = [x_res, y_res, z_res]
        .iter()
        .filter(|&&r| r == SegSegIntersection::Overlap)
        .count();
    if overlap_count >= 2 {
        return SegSegIntersection::Overlap;
    }
    SegSegIntersection::Intersect
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    // ── Group 1: point_in_segment_3d — canonical placements ───────────

    #[test]
    fn pis_on_vert0_and_vert1() {
        let s0 = pt(0.0, 0.0, 0.0);
        let s1 = pt(4.0, 0.0, 0.0);
        assert_eq!(point_in_segment_3d(s0, s0, s1), SegmentLocation::OnVert0);
        assert_eq!(point_in_segment_3d(s1, s0, s1), SegmentLocation::OnVert1);
    }

    #[test]
    fn pis_strictly_inside_axis_and_tilted() {
        assert_eq!(
            point_in_segment_3d(pt(2.0, 0.0, 0.0), pt(0.0, 0.0, 0.0), pt(4.0, 0.0, 0.0)),
            SegmentLocation::StrictlyInside
        );
        assert_eq!(
            point_in_segment_3d(pt(1.0, 2.0, 3.0), pt(0.0, 0.0, 0.0), pt(2.0, 4.0, 6.0)),
            SegmentLocation::StrictlyInside
        );
    }

    #[test]
    fn pis_outside_beyond_and_off_line() {
        // collinear but beyond the endpoint
        assert_eq!(
            point_in_segment_3d(pt(6.0, 0.0, 0.0), pt(0.0, 0.0, 0.0), pt(4.0, 0.0, 0.0)),
            SegmentLocation::StrictlyOutside
        );
        // off the supporting line
        assert_eq!(
            point_in_segment_3d(pt(2.0, 1.0, 0.0), pt(0.0, 0.0, 0.0), pt(4.0, 0.0, 0.0)),
            SegmentLocation::StrictlyOutside
        );
    }

    // ── Group 2: point_in_triangle_3d_loc — verts / edges / interior ──

    fn tri() -> (Point3, Point3, Point3) {
        (pt(0.0, 0.0, 0.0), pt(4.0, 0.0, 0.0), pt(0.0, 4.0, 0.0))
    }

    #[test]
    fn pit_on_verts() {
        let (a, b, c) = tri();
        assert_eq!(
            point_in_triangle_3d_loc(a, a, b, c),
            TriangleLocation::OnVert0
        );
        assert_eq!(
            point_in_triangle_3d_loc(b, a, b, c),
            TriangleLocation::OnVert1
        );
        assert_eq!(
            point_in_triangle_3d_loc(c, a, b, c),
            TriangleLocation::OnVert2
        );
    }

    #[test]
    fn pit_on_edges() {
        let (a, b, c) = tri();
        // edge0 = (a,b) → y=0, 0<x<4
        assert_eq!(
            point_in_triangle_3d_loc(pt(2.0, 0.0, 0.0), a, b, c),
            TriangleLocation::OnEdge0
        );
        // edge1 = (b,c) → x+y=4
        assert_eq!(
            point_in_triangle_3d_loc(pt(2.0, 2.0, 0.0), a, b, c),
            TriangleLocation::OnEdge1
        );
        // edge2 = (c,a) → x=0, 0<y<4
        assert_eq!(
            point_in_triangle_3d_loc(pt(0.0, 2.0, 0.0), a, b, c),
            TriangleLocation::OnEdge2
        );
    }

    #[test]
    fn pit_interior_and_outside() {
        let (a, b, c) = tri();
        assert_eq!(
            point_in_triangle_3d_loc(pt(1.0, 1.0, 0.0), a, b, c),
            TriangleLocation::StrictlyInside
        );
        // coplanar but outside
        assert_eq!(
            point_in_triangle_3d_loc(pt(3.0, 3.0, 0.0), a, b, c),
            TriangleLocation::StrictlyOutside
        );
        // off the plane (over interior) → outside (cinolib 3-projection)
        assert_eq!(
            point_in_triangle_3d_loc(pt(1.0, 1.0, 5.0), a, b, c),
            TriangleLocation::StrictlyOutside
        );
    }

    // ── Group 3: segment_segment_intersect_3d — crossing cases ────────

    #[test]
    fn ssi_crossing_inner_point() {
        // two diagonals of a unit square in z=0 cross at (0.5,0.5,0)
        assert_eq!(
            segment_segment_intersect_3d(
                pt(0.0, 0.0, 0.0),
                pt(1.0, 1.0, 0.0),
                pt(1.0, 0.0, 0.0),
                pt(0.0, 1.0, 0.0)
            ),
            SegSegIntersection::Intersect
        );
    }

    #[test]
    fn ssi_fully_coincident_is_simplicial() {
        // Only FULL coincidence (every endpoint shared) is caught as
        // SIMPLICIAL_COMPLEX by the 3D routine's top guard
        // (predicates.cpp:678-684). A merely-shared SINGLE endpoint loses the
        // per-projection SIMPLICIAL verdict and reports INTERSECT (the 3D
        // routine only special-cases full coincidence, DO_NOT_INTERSECT, and
        // OVERLAP) — asserted in `ssi_shared_single_endpoint_is_intersect`.
        assert_eq!(
            segment_segment_intersect_3d(
                pt(0.0, 0.0, 0.0),
                pt(2.0, 1.0, 1.0),
                pt(0.0, 0.0, 0.0),
                pt(2.0, 1.0, 1.0)
            ),
            SegSegIntersection::SimplicialComplex
        );
    }

    #[test]
    fn ssi_shared_single_endpoint_is_intersect() {
        // Two non-collinear segments meeting only at the shared endpoint
        // (0,0,0). cinolib's 3D routine returns INTERSECT (it does not
        // propagate the per-projection SIMPLICIAL_COMPLEX verdict).
        assert_eq!(
            segment_segment_intersect_3d(
                pt(0.0, 0.0, 0.0),
                pt(2.0, 1.0, 1.0),
                pt(0.0, 0.0, 0.0),
                pt(1.0, 2.0, 3.0)
            ),
            SegSegIntersection::Intersect
        );
    }

    #[test]
    fn ssi_collinear_overlap_returns_intersect() {
        // [0,3] and [1,4] on the x axis overlap on [1,3]. cinolib's 2D
        // segment-segment test returns INTERSECT (not OVERLAP) for a
        // collinear-overlapping pair (predicates.cpp:637) — the documented
        // OVERLAP verdict requires ≥2 projections to BOTH return OVERLAP, and
        // the 2D routine never returns OVERLAP, so it is unreachable here.
        // We assert the FAITHFUL behaviour: collinear overlap → INTERSECT.
        assert_eq!(
            segment_segment_intersect_3d(
                pt(0.0, 0.0, 0.0),
                pt(3.0, 0.0, 0.0),
                pt(1.0, 0.0, 0.0),
                pt(4.0, 0.0, 0.0)
            ),
            SegSegIntersection::Intersect
        );
    }

    // ── Group 4: disjoint / non-coplanar ──────────────────────────────

    #[test]
    fn ssi_disjoint_coplanar() {
        assert_eq!(
            segment_segment_intersect_3d(
                pt(0.0, 0.0, 0.0),
                pt(1.0, 0.0, 0.0),
                pt(0.0, 1.0, 0.0),
                pt(1.0, 1.0, 0.0)
            ),
            SegSegIntersection::DoNotIntersect
        );
    }

    #[test]
    fn ssi_non_coplanar_is_do_not_intersect() {
        // skew segments in 3D (not coplanar) → DO_NOT_INTERSECT
        assert_eq!(
            segment_segment_intersect_3d(
                pt(0.0, 0.0, 0.0),
                pt(1.0, 0.0, 0.0),
                pt(0.0, 0.0, 1.0),
                pt(0.0, 1.0, 1.0)
            ),
            SegSegIntersection::DoNotIntersect
        );
    }

    // ── Group 5: exactness / order invariance ─────────────────────────

    #[test]
    fn ssi_t_junction_endpoint_on_other_interior_is_intersect() {
        // s1 endpoint touches the interior of s0: a "T" — orient signs make
        // the crossing branch fire (det_s10 != det_s11 with det_s00=det_s01=0
        // is NOT this case; here s0 spans x, s1 drops onto it at (2,0,0)).
        // (2,0,0) is interior to s0=(0,0,0)-(4,0,0); s1=(2,0,0)-(2,2,0).
        assert_eq!(
            segment_segment_intersect_3d(
                pt(0.0, 0.0, 0.0),
                pt(4.0, 0.0, 0.0),
                pt(2.0, 0.0, 0.0),
                pt(2.0, 2.0, 0.0)
            ),
            SegSegIntersection::Intersect
        );
    }

    #[test]
    fn pit_order_invariant_interior() {
        let (a, b, c) = tri();
        let p = pt(1.0, 1.0, 0.0);
        // interior verdict is order-agnostic
        assert_eq!(
            point_in_triangle_3d_loc(p, a, b, c),
            TriangleLocation::StrictlyInside
        );
        assert_eq!(
            point_in_triangle_3d_loc(p, b, c, a),
            TriangleLocation::StrictlyInside
        );
        assert_eq!(
            point_in_triangle_3d_loc(p, c, a, b),
            TriangleLocation::StrictlyInside
        );
    }
}
