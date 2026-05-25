// Function body is `unimplemented!()` during the RED phase (Test Author
// commit). The Implementer commit replaces the body. The per-file MIT
// attribution header lands in a separate commit after GREEN per PR-CR7
// sequencing.

use cad_primitives::Point3;

/// Test whether two 3D triangles `(a, b, c)` and `(d, e, f)` lie in
/// the same plane.
///
/// Returns `true` iff all 6 vertices lie in a common plane (per
/// Shewchuk's exact `orient3d`).
///
/// This is the first step of Cherchi 2022 §3's triangle-triangle
/// intersection algorithm: the algorithm branches on coplanarity,
/// with different sub-algorithms for the coplanar vs non-coplanar cases.
///
/// See `specs/cherchi_rs_triangles_coplanar.md` for the full contract,
/// including the robustness rationale for using 6 `orient3d` tests
/// (rather than 3).
///
/// # Failure modes
///
/// NaN / infinite inputs → undefined. Degenerate (collinear-vertex)
/// triangles → deterministic but may false-report coplanar; caller's
/// responsibility to filter via [`points_are_collinear_3d`] first.
///
/// [`points_are_collinear_3d`]: super::collinearity::points_are_collinear_3d
pub fn triangles_are_coplanar(
    _a: Point3,
    _b: Point3,
    _c: Point3,
    _d: Point3,
    _e: Point3,
    _f: Point3,
) -> bool {
    unimplemented!("PR-CR7 RED phase — Implementer fills body in next commit")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Group 1: Canonical coplanar ───────────────────────────────────

    /// Two distinct triangles, both in z=0 plane.
    #[test]
    fn two_distinct_in_z0_plane() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(2.0, 2.0, 0.0);
        let e = Point3::new(3.0, 2.0, 0.0);
        let f = Point3::new(2.0, 3.0, 0.0);
        assert!(triangles_are_coplanar(a, b, c, d, e, f));
    }

    /// Same triangle compared to itself.
    #[test]
    fn identical_triangle() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        assert!(triangles_are_coplanar(a, b, c, a, b, c));
    }

    /// Translated copy of triangle, same z=0 plane.
    #[test]
    fn translated_copy_in_z0_plane() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let shift = |p: Point3| Point3::new(p.x() + 5.0, p.y() - 3.0, p.z());
        let (d, e, f) = (shift(a), shift(b), shift(c));
        assert!(triangles_are_coplanar(a, b, c, d, e, f));
    }

    /// Both triangles in tilted plane x + y + z = 1.
    #[test]
    fn both_in_tilted_plane() {
        // Vertices on axis intercepts of plane x+y+z=1
        let a = Point3::new(1.0, 0.0, 0.0);
        let b = Point3::new(0.0, 1.0, 0.0);
        let c = Point3::new(0.0, 0.0, 1.0);
        // Another triangle in the same plane: midpoints of T1's edges
        let d = Point3::new(0.5, 0.5, 0.0); // midpoint of (a, b)
        let e = Point3::new(0.0, 0.5, 0.5); // midpoint of (b, c)
        let f = Point3::new(0.5, 0.0, 0.5); // midpoint of (a, c)
        assert!(triangles_are_coplanar(a, b, c, d, e, f));
    }

    /// Two triangles sharing the edge (a, b) in z=0 plane.
    #[test]
    fn shared_edge_in_z0_plane() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let f = Point3::new(0.0, -1.0, 0.0); // T2 = (a, b, f), shares (a, b)
        assert!(triangles_are_coplanar(a, b, c, a, b, f));
    }

    // ── Group 2: Canonical non-coplanar ───────────────────────────────

    /// Triangle in z=0 plane, triangle in z=1 plane (parallel planes).
    #[test]
    fn parallel_planes_z0_z1() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(0.0, 0.0, 1.0);
        let e = Point3::new(1.0, 0.0, 1.0);
        let f = Point3::new(0.0, 1.0, 1.0);
        assert!(!triangles_are_coplanar(a, b, c, d, e, f));
    }

    /// Triangle in XY, triangle in XZ — share origin vertex but different planes.
    #[test]
    fn shared_vertex_different_planes_xy_xz() {
        // T1 in XY plane (z=0):
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        // T2 in XZ plane (y=0), sharing origin:
        let d = Point3::new(0.0, 0.0, 0.0);
        let e = Point3::new(1.0, 0.0, 0.0);
        let f = Point3::new(0.0, 0.0, 1.0);
        // T1 and T2 share edge (origin, (1,0,0)) but T2's third
        // vertex is off T1's plane.
        assert!(!triangles_are_coplanar(a, b, c, d, e, f));
    }

    /// Triangle in z=0, triangle in tilted plane x+y+z=1.
    #[test]
    fn z0_plane_vs_tilted_plane() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(1.0, 0.0, 0.0);
        let e = Point3::new(0.0, 1.0, 0.0);
        let f = Point3::new(0.0, 0.0, 1.0);
        assert!(!triangles_are_coplanar(a, b, c, d, e, f));
    }

    // ── Group 3: Properties ───────────────────────────────────────────

    #[test]
    fn symmetry_under_triangle_swap_coplanar() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(2.0, 2.0, 0.0);
        let e = Point3::new(3.0, 2.0, 0.0);
        let f = Point3::new(2.0, 3.0, 0.0);
        let forward = triangles_are_coplanar(a, b, c, d, e, f);
        let swapped = triangles_are_coplanar(d, e, f, a, b, c);
        assert_eq!(forward, swapped);
        assert!(forward); // both should be true (coplanar)
    }

    #[test]
    fn symmetry_under_triangle_swap_non_coplanar() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(0.0, 0.0, 1.0);
        let e = Point3::new(1.0, 0.0, 1.0);
        let f = Point3::new(0.0, 1.0, 1.0);
        let forward = triangles_are_coplanar(a, b, c, d, e, f);
        let swapped = triangles_are_coplanar(d, e, f, a, b, c);
        assert_eq!(forward, swapped);
        assert!(!forward); // both should be false (non-coplanar)
    }

    #[test]
    fn vertex_permutation_invariance_coplanar() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(2.0, 2.0, 0.0);
        let e = Point3::new(3.0, 2.0, 0.0);
        let f = Point3::new(2.0, 3.0, 0.0);
        // All 6 permutations of T1's vertices yield true
        assert!(triangles_are_coplanar(a, b, c, d, e, f));
        assert!(triangles_are_coplanar(a, c, b, d, e, f));
        assert!(triangles_are_coplanar(b, a, c, d, e, f));
        assert!(triangles_are_coplanar(b, c, a, d, e, f));
        assert!(triangles_are_coplanar(c, a, b, d, e, f));
        assert!(triangles_are_coplanar(c, b, a, d, e, f));
    }

    #[test]
    fn vertex_permutation_invariance_non_coplanar() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(0.0, 0.0, 1.0);
        let e = Point3::new(1.0, 0.0, 1.0);
        let f = Point3::new(0.0, 1.0, 1.0);
        // All 6 permutations of T1's vertices yield false
        assert!(!triangles_are_coplanar(a, b, c, d, e, f));
        assert!(!triangles_are_coplanar(a, c, b, d, e, f));
        assert!(!triangles_are_coplanar(b, a, c, d, e, f));
        assert!(!triangles_are_coplanar(b, c, a, d, e, f));
        assert!(!triangles_are_coplanar(c, a, b, d, e, f));
        assert!(!triangles_are_coplanar(c, b, a, d, e, f));
    }

    // ── Group 4: Determinism ──────────────────────────────────────────

    #[test]
    fn deterministic_under_repeated_runs() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(0.0, 0.0, 1.0);
        let e = Point3::new(1.0, 0.0, 1.0);
        let f = Point3::new(0.0, 1.0, 1.0);
        let first = triangles_are_coplanar(a, b, c, d, e, f);
        for _ in 0..100 {
            assert_eq!(triangles_are_coplanar(a, b, c, d, e, f), first);
        }
    }
}
