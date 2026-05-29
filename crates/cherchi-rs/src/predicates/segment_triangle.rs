//! 3D segment-triangle intersection classification.
//!
//! `segment_intersects_triangle_3d` is the core primitive of Cherchi
//! 2022 §3's non-coplanar triangle-triangle intersection branch:
//! for each pair of (T1 edge, T2 triangle) and (T2 edge, T1 triangle),
//! this test determines whether they share any point.
//!
//! Cherchi 2022 §3 (triangle-triangle intersection; non-coplanar branch).
//! Shewchuk 1997 §2.1 (orient3d as the foundational predicate).
//!
//! No specific cinolib function flagged in audit for this predicate —
//! the algorithm (5 orient3d tests + Sign-pattern combination) is
//! standard computational geometry (Möller-Trumbore-style, adapted to
//! exact arithmetic). The orient3d primitive is from PR-CR6's wrapper
//! over `geometry-predicates` (MIT).
//!
//! The 3-state enum (Disjoint / Intersects / Coplanar) collapses
//! cinolib's richer variants (interior / boundary / on-vertex /
//! on-edge) per YAGNI; see spec §"Scope discipline" for the rationale.

use cad_primitives::Point3;

/// Classification of a 3D segment's spatial relationship to a triangle.
///
/// Returned by [`segment_intersects_triangle_3d`].
///
/// **Deliberate simplification**: cinolib distinguishes interior /
/// boundary / on-vertex / on-edge variants. We collapse those to
/// `Intersects` (caller doesn't currently need the granular info).
/// See `specs/cherchi_rs_segment_triangle_3d.md` §"Scope discipline".
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SegmentTriangleIntersection {
    Disjoint,
    Intersects,
    Coplanar,
}

/// Classify whether a 3D segment `(p, q)` intersects a 3D triangle
/// `(a, b, c)`.
///
/// Returns:
/// - `Disjoint` if segment and triangle don't share any point
/// - `Intersects` if they share any point (interior, edge, vertex, or
///   endpoint coincidence)
/// - `Coplanar` if segment lies in triangle's plane (caller must run
///   a 2D segment-triangle algorithm to refine)
///
/// Core primitive of Cherchi 2022 §3's non-coplanar triangle-triangle
/// intersection branch. See `specs/cherchi_rs_segment_triangle_3d.md`
/// for the full contract.
///
/// # Failure modes
///
/// NaN / infinite inputs → undefined. Degenerate (collinear-vertex)
/// triangles → deterministic but may misclassify; caller's
/// responsibility to filter via [`points_are_collinear_3d`].
///
/// [`points_are_collinear_3d`]: super::collinearity::points_are_collinear_3d
pub fn segment_intersects_triangle_3d(
    p: Point3,
    q: Point3,
    a: Point3,
    b: Point3,
    c: Point3,
) -> SegmentTriangleIntersection {
    use super::orient::{orient3d, Sign};
    use SegmentTriangleIntersection::*;

    // 1. Which side of triangle's plane is each segment endpoint on?
    let s_p = orient3d(a, b, c, p);
    let s_q = orient3d(a, b, c, q);

    // 2. Both endpoints on plane → caller handles 2D case.
    if s_p == Sign::Zero && s_q == Sign::Zero {
        return Coplanar;
    }

    // 3. Both endpoints on same non-zero side → segment doesn't cross plane.
    let same_side_pos = s_p == Sign::Positive && s_q == Sign::Positive;
    let same_side_neg = s_p == Sign::Negative && s_q == Sign::Negative;
    if same_side_pos || same_side_neg {
        return Disjoint;
    }

    // 4. Compute line-vs-triangle-edge orientations.
    let l_ab = orient3d(p, q, a, b);
    let l_bc = orient3d(p, q, b, c);
    let l_ca = orient3d(p, q, c, a);

    // 5. If mixed signs (any Positive AND any Negative) → line passes
    //    outside the triangle.
    let any_pos = l_ab == Sign::Positive || l_bc == Sign::Positive || l_ca == Sign::Positive;
    let any_neg = l_ab == Sign::Negative || l_bc == Sign::Negative || l_ca == Sign::Negative;

    if any_pos && any_neg {
        Disjoint
    } else {
        // All same sign (zeros allowed) → line passes through or along
        // an edge of the triangle, AND the segment endpoints span the
        // plane (or one is on it), so the segment touches the triangle.
        Intersects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard unit triangle in XY plane.
    fn xy_triangle() -> (Point3, Point3, Point3) {
        (
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        )
    }

    // ── Group 1: Disjoint cases ───────────────────────────────────────

    #[test]
    fn segment_far_above_triangle_disjoint() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.25, 0.25, 5.0);
        let q = Point3::new(0.25, 0.25, 10.0);
        assert_eq!(
            segment_intersects_triangle_3d(p, q, a, b, c),
            SegmentTriangleIntersection::Disjoint
        );
    }

    #[test]
    fn segment_far_below_triangle_disjoint() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.25, 0.25, -5.0);
        let q = Point3::new(0.25, 0.25, -10.0);
        assert_eq!(
            segment_intersects_triangle_3d(p, q, a, b, c),
            SegmentTriangleIntersection::Disjoint
        );
    }

    /// Segment crosses z=0 plane at (5, 5, 0) — well outside the
    /// unit triangle near the origin.
    #[test]
    fn segment_crosses_plane_outside_triangle_disjoint() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(5.0, 5.0, 1.0);
        let q = Point3::new(5.0, 5.0, -1.0);
        assert_eq!(
            segment_intersects_triangle_3d(p, q, a, b, c),
            SegmentTriangleIntersection::Disjoint
        );
    }

    // ── Group 2: Intersects cases ─────────────────────────────────────

    /// Segment crosses straight through interior: vertical line at
    /// triangle's centroid (1/3, 1/3, 0).
    #[test]
    fn segment_crosses_through_interior() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(1.0 / 3.0, 1.0 / 3.0, 1.0);
        let q = Point3::new(1.0 / 3.0, 1.0 / 3.0, -1.0);
        assert_eq!(
            segment_intersects_triangle_3d(p, q, a, b, c),
            SegmentTriangleIntersection::Intersects
        );
    }

    /// Segment endpoint on triangle interior; other endpoint above.
    #[test]
    fn segment_endpoint_on_triangle_interior() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.25, 0.25, 0.0); // on triangle interior
        let q = Point3::new(0.25, 0.25, 1.0); // above
        assert_eq!(
            segment_intersects_triangle_3d(p, q, a, b, c),
            SegmentTriangleIntersection::Intersects
        );
    }

    /// Segment endpoint == triangle vertex `a`; other endpoint above.
    #[test]
    fn segment_endpoint_on_triangle_vertex() {
        let (a, b, c) = xy_triangle();
        let p = a; // on vertex
        let q = Point3::new(0.0, 0.0, 1.0); // above
        assert_eq!(
            segment_intersects_triangle_3d(p, q, a, b, c),
            SegmentTriangleIntersection::Intersects
        );
    }

    /// Segment endpoint on edge midpoint of (a, b); other endpoint above.
    #[test]
    fn segment_endpoint_on_triangle_edge_midpoint() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.5, 0.0, 0.0); // midpoint of edge (a, b)
        let q = Point3::new(0.5, 0.0, 1.0); // above
        assert_eq!(
            segment_intersects_triangle_3d(p, q, a, b, c),
            SegmentTriangleIntersection::Intersects
        );
    }

    // ── Group 3: Coplanar case ────────────────────────────────────────

    /// Both endpoints lie in triangle's plane (z=0) — caller must
    /// handle 2D case separately.
    #[test]
    fn segment_in_triangle_plane_returns_coplanar() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.25, 0.25, 0.0); // in plane, inside triangle
        let q = Point3::new(2.0, 2.0, 0.0); // in plane, outside triangle
        assert_eq!(
            segment_intersects_triangle_3d(p, q, a, b, c),
            SegmentTriangleIntersection::Coplanar
        );
    }

    // ── Group 4: Properties ───────────────────────────────────────────

    #[test]
    fn endpoint_swap_symmetry_intersects() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(1.0 / 3.0, 1.0 / 3.0, 1.0);
        let q = Point3::new(1.0 / 3.0, 1.0 / 3.0, -1.0);
        let forward = segment_intersects_triangle_3d(p, q, a, b, c);
        let swapped = segment_intersects_triangle_3d(q, p, a, b, c);
        assert_eq!(forward, swapped);
        assert_eq!(forward, SegmentTriangleIntersection::Intersects);
    }

    #[test]
    fn endpoint_swap_symmetry_disjoint() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.25, 0.25, 5.0);
        let q = Point3::new(0.25, 0.25, 10.0);
        let forward = segment_intersects_triangle_3d(p, q, a, b, c);
        let swapped = segment_intersects_triangle_3d(q, p, a, b, c);
        assert_eq!(forward, swapped);
        assert_eq!(forward, SegmentTriangleIntersection::Disjoint);
    }

    /// Cyclic vertex permutation: `(a, b, c) → (b, c, a) → (c, a, b)`
    /// preserves the result.
    #[test]
    fn cyclic_vertex_permutation_invariance_intersects() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(1.0 / 3.0, 1.0 / 3.0, 1.0);
        let q = Point3::new(1.0 / 3.0, 1.0 / 3.0, -1.0);
        let r_abc = segment_intersects_triangle_3d(p, q, a, b, c);
        let r_bca = segment_intersects_triangle_3d(p, q, b, c, a);
        let r_cab = segment_intersects_triangle_3d(p, q, c, a, b);
        assert_eq!(r_abc, r_bca);
        assert_eq!(r_bca, r_cab);
        assert_eq!(r_abc, SegmentTriangleIntersection::Intersects);
    }

    // ── Group 5: Determinism ──────────────────────────────────────────

    #[test]
    fn deterministic_under_repeated_runs() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(1.0 / 3.0, 1.0 / 3.0, 1.0);
        let q = Point3::new(1.0 / 3.0, 1.0 / 3.0, -1.0);
        let first = segment_intersects_triangle_3d(p, q, a, b, c);
        for _ in 0..100 {
            assert_eq!(segment_intersects_triangle_3d(p, q, a, b, c), first);
        }
    }
}
