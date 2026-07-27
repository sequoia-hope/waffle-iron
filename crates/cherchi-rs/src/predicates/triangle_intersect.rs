//! 3D triangle-triangle intersection classification — the algorithmic
//! payoff of PR-CR1 through PR-CR8's foundations.
//!
//! `triangle_intersects_triangle_3d` is the central algorithm of
//! Cherchi 2022 §3: mesh arrangement processes pairs of triangles via
//! this primitive. Branches on coplanarity (PR-CR7) and dispatches to
//! 6 edge-triangle tests (PR-CR8) in the non-coplanar case.
//!
//! Cherchi 2022 §3 (triangle-triangle intersection; full algorithm).
//! Shewchuk 1997 §2.1 (orient3d as the foundational predicate).
//!
//! The 3-state enum (Disjoint / Intersects / Coplanar) collapses
//! intersection types per YAGNI: callers needing granular info
//! (interior vs edge vs vertex) can probe via direct PR-CR8 calls.
//!
//! **Discovery during implementation**: shared-edge cases return
//! `Intersects` (not `Coplanar` as originally specified) — the
//! algorithm's secondary line-test propagation via vertex coincidence
//! correctly detects the shared edge as a true intersection. The
//! `Coplanar` return is reserved for cases requiring caller's 2D
//! refinement: full coplanar OR edge-in-other-plane without vertex
//! coincidence. See `specs/cherchi_rs_triangle_intersect_3d.md`
//! §"Why Coplanar covers both cases" for the full discussion.

use cad_primitives::Point3;

use super::{max_component_in_triangle_normal, Axis};

/// #200 Direction-B coplanar-edge reject — ALWAYS-ON (dev A/B knob only).
/// Flipped on after the full corpus proved category-identical
/// (255C/0W/55E/0T, 2026-07-27) with the reject enabled. Set
/// `CHERCHI_SCE_SHORTCIRCUIT=0` (or `off`) to disable for an A/B comparison;
/// the disabled path is the exact pre-#200 behaviour. Read once via `OnceLock`
/// (WASM-safe: `env::var` returns `Err` off-native → default enabled).
fn coplanar_edge_reject_enabled() -> bool {
    use std::sync::OnceLock;
    static E: OnceLock<bool> = OnceLock::new();
    *E.get_or_init(|| {
        !matches!(
            std::env::var("CHERCHI_SCE_SHORTCIRCUIT").as_deref(),
            Ok("0") | Ok("off")
        )
    })
}

/// #200 EXACT, parity-preserving refinement: a segment `[p,q]` that lies in
/// triangle `(d,e,f)`'s plane (`segment_intersects_triangle_3d` returned
/// `Coplanar`) can only intersect the triangle IN that plane. Projecting both
/// onto the two axes other than the triangle's dominant normal axis is an
/// INJECTIVE map of the plane, so if their projected boxes are strictly
/// disjoint the segment is provably OUTSIDE the triangle — it contributes no
/// intersection, and the conservative `Coplanar` becomes a precise `Disjoint`.
/// The test is EXACT (component extraction + `min`/`max` + strict `<`, no
/// rounding arithmetic); `<` (not `<=`) keeps a touching edge on the full path.
/// Only [`triangle_intersects_triangle_3d`] (→ `detect_intersecting_pairs`)
/// consumes this, so tightening `Coplanar`→`Disjoint` for a genuinely-outside
/// coplanar edge merely drops a pair the downstream classify would have
/// resolved to the empty set — byte-identical arrangement, fewer pairs.
fn coplanar_edge_outside_triangle(p: Point3, q: Point3, d: Point3, e: Point3, f: Point3) -> bool {
    let axis = max_component_in_triangle_normal(d, e, f);
    let proj = |pt: Point3| -> (f64, f64) {
        let a = pt.as_array();
        match axis {
            Axis::X => (a[1], a[2]),
            Axis::Y => (a[0], a[2]),
            Axis::Z => (a[0], a[1]),
        }
    };
    let (pu, pv) = proj(p);
    let (qu, qv) = proj(q);
    let (eu_lo, eu_hi) = (pu.min(qu), pu.max(qu));
    let (ev_lo, ev_hi) = (pv.min(qv), pv.max(qv));
    let mut tu_lo = f64::INFINITY;
    let mut tu_hi = f64::NEG_INFINITY;
    let mut tv_lo = f64::INFINITY;
    let mut tv_hi = f64::NEG_INFINITY;
    for w in [d, e, f] {
        let (u, v) = proj(w);
        tu_lo = tu_lo.min(u);
        tu_hi = tu_hi.max(u);
        tv_lo = tv_lo.min(v);
        tv_hi = tv_hi.max(v);
    }
    eu_hi < tu_lo || tu_hi < eu_lo || ev_hi < tv_lo || tv_hi < ev_lo
}

/// Classification of two 3D triangles' spatial relationship.
///
/// Returned by [`triangle_intersects_triangle_3d`].
///
/// The `Coplanar` variant covers BOTH "full coplanar" (both triangles in
/// the same plane) AND "partial coplanar" (an edge of one triangle lies
/// in the other's plane). Both require the caller to run a 2D handler
/// to refine. See `specs/cherchi_rs_triangle_intersect_3d.md` §"Why
/// Coplanar covers both cases".
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TriangleIntersection {
    Disjoint,
    Intersects,
    Coplanar,
}

/// Classify whether two 3D triangles `(a, b, c)` and `(d, e, f)`
/// intersect, are disjoint, or require 2D refinement.
///
/// The central algorithm of Cherchi 2022 §3 — composes
/// [`triangles_are_coplanar`] (PR-CR7) for the coplanarity branch and
/// [`segment_intersects_triangle_3d`] (PR-CR8) for the non-coplanar
/// edge-triangle tests.
///
/// See `specs/cherchi_rs_triangle_intersect_3d.md` for the full contract.
///
/// # Failure modes
///
/// NaN / infinite inputs → undefined. Degenerate (collinear-vertex)
/// triangles → deterministic but may misclassify; caller's
/// responsibility to filter via [`points_are_collinear_3d`].
///
/// [`triangles_are_coplanar`]: super::triangle_pair::triangles_are_coplanar
/// [`segment_intersects_triangle_3d`]: super::segment_triangle::segment_intersects_triangle_3d
/// [`points_are_collinear_3d`]: super::collinearity::points_are_collinear_3d
pub fn triangle_intersects_triangle_3d(
    a: Point3,
    b: Point3,
    c: Point3,
    d: Point3,
    e: Point3,
    f: Point3,
) -> TriangleIntersection {
    tti_impl(a, b, c, d, e, f, coplanar_edge_reject_enabled())
}

/// Body of [`triangle_intersects_triangle_3d`], parameterised on the #200
/// `reject` gate so both paths are unit-testable without the process-global
/// env `OnceLock`.
fn tti_impl(
    a: Point3,
    b: Point3,
    c: Point3,
    d: Point3,
    e: Point3,
    f: Point3,
    reject: bool,
) -> TriangleIntersection {
    use super::segment_triangle::{
        segment_intersects_triangle_3d, SegmentTriangleIntersection as STI,
    };
    use super::triangle_pair::triangles_are_coplanar;
    use TriangleIntersection::*;

    // 1. Full coplanarity branch: PR-CR7.
    if triangles_are_coplanar(a, b, c, d, e, f) {
        return Coplanar;
    }

    // 2. Non-coplanar branch: iterate 6 edge-triangle pairs (PR-CR8).
    let mut any_intersects = false;
    let mut any_coplanar = false;

    let t1_edges: [(Point3, Point3); 3] = [(a, b), (b, c), (c, a)];
    let t2_edges: [(Point3, Point3); 3] = [(d, e), (e, f), (f, d)];

    // Edges of T1 against T2
    for (p, q) in t1_edges {
        match segment_intersects_triangle_3d(p, q, d, e, f) {
            STI::Intersects => any_intersects = true,
            // #200: a coplanar edge whose projected box is outside T2 cannot
            // intersect it — drop it precisely (exact, parity-preserving).
            STI::Coplanar if reject && coplanar_edge_outside_triangle(p, q, d, e, f) => {}
            STI::Coplanar => any_coplanar = true,
            STI::Disjoint => {}
        }
    }

    // Edges of T2 against T1
    for (p, q) in t2_edges {
        match segment_intersects_triangle_3d(p, q, a, b, c) {
            STI::Intersects => any_intersects = true,
            STI::Coplanar if reject && coplanar_edge_outside_triangle(p, q, a, b, c) => {}
            STI::Coplanar => any_coplanar = true,
            STI::Disjoint => {}
        }
    }

    // 3. Aggregate per priority: Intersects > Coplanar > Disjoint.
    if any_intersects {
        Intersects
    } else if any_coplanar {
        Coplanar
    } else {
        Disjoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── #200 Direction-B coplanar-edge reject ───────────────────────────

    #[test]
    fn coplanar_edge_outside_triangle_box_test() {
        // Triangle in z=1 (dominant normal axis Z → projects to XY).
        let (d, e, f) = (
            Point3::new(0.0, 0.0, 1.0),
            Point3::new(1.0, 0.0, 1.0),
            Point3::new(0.0, 1.0, 1.0),
        );
        // Edge in z=1 clearly right of the triangle → outside.
        assert!(coplanar_edge_outside_triangle(
            Point3::new(5.0, 0.0, 1.0),
            Point3::new(6.0, 0.0, 1.0),
            d,
            e,
            f
        ));
        // Edge overlapping the triangle box → NOT outside (stays on full path).
        assert!(!coplanar_edge_outside_triangle(
            Point3::new(0.2, 0.2, 1.0),
            Point3::new(0.8, 0.2, 1.0),
            d,
            e,
            f
        ));
        // Touching box (strict <) → NOT outside.
        assert!(!coplanar_edge_outside_triangle(
            Point3::new(1.0, 0.0, 1.0),
            Point3::new(2.0, 0.0, 1.0),
            d,
            e,
            f
        ));
    }

    #[test]
    fn reject_turns_outside_coplanar_edge_from_coplanar_to_disjoint() {
        // A: horizontal triangle in z=0.
        let (a, b, c) = (
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        );
        // B: vertical triangle whose BOTTOM edge lies in A's plane (z=0) but
        // well to the right of A (x∈[5,6]); apex lifts out of the plane. This
        // is a single-coplanar-edge config with the coplanar edge OUTSIDE A.
        let (d, e, f) = (
            Point3::new(5.0, 0.0, 0.0),
            Point3::new(6.0, 0.0, 0.0),
            Point3::new(5.5, 0.0, 1.0),
        );
        // Gate OFF (reject=false): the conservative full path returns Coplanar.
        assert_eq!(
            tti_impl(a, b, c, d, e, f, false),
            TriangleIntersection::Coplanar,
            "without the reject, an outside coplanar edge reports Coplanar"
        );
        // Gate ON (reject=true): the exact box test makes it precisely Disjoint —
        // the pair drops out of detect and never reaches classify.
        assert_eq!(
            tti_impl(a, b, c, d, e, f, true),
            TriangleIntersection::Disjoint,
            "with the reject, an outside coplanar edge is precisely Disjoint"
        );
    }

    #[test]
    fn reject_preserves_coplanar_edge_whose_box_overlaps() {
        // A genuine Coplanar case that must SURVIVE the reject: a wholly-in-plane
        // configuration where an edge of B lies in A's plane and its projected
        // box OVERLAPS A (so the box test does NOT fire), with no edge piercing
        // (so the result stays Coplanar, not Intersects). A is the lower-left
        // triangle of the unit box; B's coplanar edge sits in the upper-right
        // corner region — outside A's hypotenuse but inside A's bounding box —
        // and B lifts out of plane without touching A.
        let (a, b, c) = (
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        );
        // Coplanar edge (0.9,0.9,0)-(0.95,0.6,0): in A's plane (z=0), box overlaps
        // A's box [0,1]² but the edge is beyond A's hypotenuse (x+y>1). Apex
        // above the plane; its slanted edges stay in x+y>1 and never touch A.
        let (d, e, f) = (
            Point3::new(0.9, 0.9, 0.0),
            Point3::new(0.95, 0.6, 0.0),
            Point3::new(0.9, 0.7, 1.0),
        );
        // Box overlaps → the reject must NOT drop this coplanar edge.
        assert!(!coplanar_edge_outside_triangle(d, e, a, b, c));
        assert_eq!(
            tti_impl(a, b, c, d, e, f, false),
            tti_impl(a, b, c, d, e, f, true),
            "the reject must not change the result when the coplanar edge's box \
             overlaps the triangle"
        );
    }

    /// Standard unit triangle in XY plane.
    fn xy_triangle() -> (Point3, Point3, Point3) {
        (
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        )
    }

    /// Standard unit triangle in XZ plane (y = 0).
    fn xz_triangle() -> (Point3, Point3, Point3) {
        (
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 0.0, 1.0),
        )
    }

    // ── Group 1: Disjoint cases ───────────────────────────────────────

    #[test]
    fn far_apart_triangles_disjoint() {
        let (a, b, c) = xy_triangle();
        // T2 shifted by (100, 100, 100)
        let d = Point3::new(100.0, 100.0, 100.0);
        let e = Point3::new(101.0, 100.0, 100.0);
        let f = Point3::new(100.0, 101.0, 100.0);
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Disjoint
        );
    }

    #[test]
    fn parallel_planes_disjoint() {
        let (a, b, c) = xy_triangle();
        // Same XY shape but at z=1
        let d = Point3::new(0.0, 0.0, 1.0);
        let e = Point3::new(1.0, 0.0, 1.0);
        let f = Point3::new(0.0, 1.0, 1.0);
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Disjoint
        );
    }

    #[test]
    fn t1_entirely_on_one_side_of_t2_plane_disjoint() {
        // T1 in z=0 plane, T2 in y=2 plane.
        // CRITICAL: T2's vertices are all at z > 0 to ensure no edge of
        // T2 lies in T1's plane (z=0). Otherwise segment-triangle would
        // return Coplanar for that edge, escalating to overall Coplanar.
        // With T2 strictly above z=0, all 6 edge tests return Disjoint.
        let (a, b, c) = xy_triangle();
        let d = Point3::new(0.0, 2.0, 1.0);
        let e = Point3::new(1.0, 2.0, 1.0);
        let f = Point3::new(0.0, 2.0, 2.0);
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Disjoint
        );
    }

    // ── Group 2: Intersects cases ─────────────────────────────────────

    /// T1 in z=0 (XY), T2 perpendicular in y=0.5 plane crossing through
    /// T1's interior.
    #[test]
    fn perpendicular_triangles_crossing_interior() {
        let (a, b, c) = xy_triangle();
        // T2: triangle perpendicular to XY, in y=0.25 plane, spans z=-1..1
        let d = Point3::new(0.25, 0.25, -1.0);
        let e = Point3::new(0.5, 0.25, 1.0);
        let f = Point3::new(0.25, 0.25, 1.0);
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Intersects
        );
    }

    /// T1 and T2 share vertex `a` with non-coplanar planes; T2 has
    /// edges crossing T1's plane on the other side.
    #[test]
    fn shared_vertex_non_coplanar_with_crossing_edges() {
        let (a, b, c) = xy_triangle();
        // T2 shares vertex a=(0,0,0), points upward, edges cross XY plane
        let d = Point3::new(0.0, 0.0, 0.0); // shared with a
        let e = Point3::new(0.5, 0.5, 1.0);
        let f = Point3::new(0.3, 0.3, -1.0);
        // T2's edge (e, f) crosses XY plane between (0.5,0.5,1) and (0.3,0.3,-1)
        // → the crossing point is (~0.4, ~0.4, 0) which is inside T1's interior
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Intersects
        );
    }

    /// Edge of T1 crosses T2's interior — vertical X-axis triangle
    /// through unit XY triangle.
    #[test]
    fn edge_of_t1_crosses_t2_interior() {
        // T1 is a vertical triangle that crosses XY plane through (0.25, 0.25, 0)
        let a = Point3::new(0.25, 0.25, 1.0);
        let b = Point3::new(0.25, 0.25, -1.0);
        let c = Point3::new(0.5, 0.5, 1.0);
        // T2 is unit XY triangle
        let (d, e, f) = xy_triangle();
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Intersects
        );
    }

    // ── Group 3: Coplanar cases ───────────────────────────────────────

    /// Two distinct triangles, both in z=0 plane → full coplanar.
    #[test]
    fn full_coplanar_two_distinct_triangles() {
        let (a, b, c) = xy_triangle();
        // Another XY-plane triangle, far away
        let d = Point3::new(5.0, 5.0, 0.0);
        let e = Point3::new(6.0, 5.0, 0.0);
        let f = Point3::new(5.0, 6.0, 0.0);
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Coplanar
        );
    }

    /// Same triangle compared to itself → full coplanar.
    #[test]
    fn full_coplanar_identical_triangle() {
        let (a, b, c) = xy_triangle();
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, a, b, c),
            TriangleIntersection::Coplanar
        );
    }

    /// Non-coplanar triangles where an edge of T1 lies in T2's plane
    /// but is far from T2's region — pure `Coplanar` return case.
    ///
    /// Geometry: T2 is the unit XY triangle; T1 has an edge (a, b) on
    /// the z=0 plane (so in T2's plane) at (5,5,0)-(6,5,0) — far from T2.
    /// T1's third vertex c=(5,5,1) is above z=0, so T1 isn't coplanar
    /// with T2.
    ///
    /// Why this isn't the shared-edge case: when triangles share an edge
    /// (vertex coincidence), the line tests on the OTHER edges of T1 fire
    /// via the degenerate vertex coincidence, propagating Intersects.
    /// The geometrically true answer for shared edges IS Intersects.
    /// This test uses the "edge in plane but no vertex coincidence" case
    /// to exercise the pure Coplanar return.
    #[test]
    fn edge_in_other_plane_far_from_triangle_returns_disjoint() {
        let (d, e, f) = xy_triangle(); // T2 = unit XY triangle near origin
        let a = Point3::new(5.0, 5.0, 0.0); // in z=0 plane (T2's), far away
        let b = Point3::new(6.0, 5.0, 0.0); // in z=0 plane, far away
        let c = Point3::new(5.0, 5.0, 1.0); // above plane
                                            // Edge (a, b) lies in T2's plane but is FAR from T2 itself, so
                                            // the pre-#200 conservative result was Coplanar (deferring the
                                            // "is the edge actually inside" question to the caller's 2D
                                            // refinement). #200's exact projected-box reject now does that
                                            // refinement in the predicate: the coplanar edge's box is disjoint
                                            // from T2's, so it contributes nothing and the precise answer is
                                            // Disjoint. With the reject disabled (CHERCHI_SCE_SHORTCIRCUIT=0)
                                            // this is the old Coplanar (see `tti_impl(..,false)` below).
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Disjoint,
            "#200: a coplanar edge far outside the triangle is precisely Disjoint"
        );
        assert_eq!(
            tti_impl(a, b, c, d, e, f, false),
            TriangleIntersection::Coplanar,
            "reject-disabled path is the exact pre-#200 conservative Coplanar"
        );
    }

    /// Non-coplanar triangles sharing an edge — returns `Intersects`
    /// via secondary line-test propagation (vertex coincidence causes
    /// degenerate orient3d Zero → all-same-sign branch fires → Intersects
    /// for the touching edge).
    ///
    /// This is GEOMETRICALLY CORRECT: a shared edge IS an intersection
    /// (the triangles share that segment). The Intersects return is the
    /// correct classification; no caller refinement needed.
    #[test]
    fn non_coplanar_shared_edge_returns_intersects() {
        let (a, b, c) = xy_triangle(); // (0,0,0),(1,0,0),(0,1,0)
        let (d, e, f) = xz_triangle(); // (0,0,0),(1,0,0),(0,0,1)
                                       // Triangles share edge (0,0,0)-(1,0,0). The geometric truth is
                                       // Intersects (shared edge = intersection segment).
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Intersects
        );
    }

    // ── Group 4: Properties ───────────────────────────────────────────

    #[test]
    fn symmetry_under_swap_disjoint() {
        let (a, b, c) = xy_triangle();
        let d = Point3::new(100.0, 100.0, 100.0);
        let e = Point3::new(101.0, 100.0, 100.0);
        let f = Point3::new(100.0, 101.0, 100.0);
        let forward = triangle_intersects_triangle_3d(a, b, c, d, e, f);
        let swapped = triangle_intersects_triangle_3d(d, e, f, a, b, c);
        assert_eq!(forward, swapped);
        assert_eq!(forward, TriangleIntersection::Disjoint);
    }

    #[test]
    fn symmetry_under_swap_intersects() {
        let a = Point3::new(0.25, 0.25, 1.0);
        let b = Point3::new(0.25, 0.25, -1.0);
        let c = Point3::new(0.5, 0.5, 1.0);
        let (d, e, f) = xy_triangle();
        let forward = triangle_intersects_triangle_3d(a, b, c, d, e, f);
        let swapped = triangle_intersects_triangle_3d(d, e, f, a, b, c);
        assert_eq!(forward, swapped);
        assert_eq!(forward, TriangleIntersection::Intersects);
    }

    #[test]
    fn vertex_permutation_invariance_intersects() {
        let a = Point3::new(0.25, 0.25, 1.0);
        let b = Point3::new(0.25, 0.25, -1.0);
        let c = Point3::new(0.5, 0.5, 1.0);
        let (d, e, f) = xy_triangle();
        // All 6 permutations of T1's vertices yield Intersects
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Intersects
        );
        assert_eq!(
            triangle_intersects_triangle_3d(a, c, b, d, e, f),
            TriangleIntersection::Intersects
        );
        assert_eq!(
            triangle_intersects_triangle_3d(b, a, c, d, e, f),
            TriangleIntersection::Intersects
        );
        assert_eq!(
            triangle_intersects_triangle_3d(b, c, a, d, e, f),
            TriangleIntersection::Intersects
        );
        assert_eq!(
            triangle_intersects_triangle_3d(c, a, b, d, e, f),
            TriangleIntersection::Intersects
        );
        assert_eq!(
            triangle_intersects_triangle_3d(c, b, a, d, e, f),
            TriangleIntersection::Intersects
        );
    }

    // ── Group 5: Determinism ──────────────────────────────────────────

    #[test]
    fn deterministic_under_repeated_runs() {
        let a = Point3::new(0.25, 0.25, 1.0);
        let b = Point3::new(0.25, 0.25, -1.0);
        let c = Point3::new(0.5, 0.5, 1.0);
        let (d, e, f) = xy_triangle();
        let first = triangle_intersects_triangle_3d(a, b, c, d, e, f);
        for _ in 0..100 {
            assert_eq!(triangle_intersects_triangle_3d(a, b, c, d, e, f), first);
        }
    }
}
