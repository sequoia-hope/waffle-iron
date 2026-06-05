//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! # PR-CR-AR2a Cycle 3 — single-triangle re-triangulation driver
//!
//! Drives the per-base-triangle re-triangulation: given a 1-triangle
//! `FastTrimesh` submesh (the base triangle, three explicit corners,
//! `tri = [0,1,2]`) and the auxiliary intersection points that fall in its
//! interior / on its edges (from `aux_structure.rs`), insert each point and
//! split the triangle:
//!
//! - an interior point → [`FastTrimesh::split_tri`] (fan into 3),
//! - an on-edge point → [`FastTrimesh::split_edge`] (the incident triangles
//!   each become 2).
//!
//! Mirrors the insertion loop of the C++ `triangulation.cpp` per-triangle
//! re-triangulator (the part that places intersection points before
//! constraint insertion). Constraint/segment insertion and the
//! cross-triangle conformance are AR2b / AR3.
//!
//! **Feature-gated FFI consumer**: locating an `Lpi` point (interior vs. which
//! edge) uses the indirect-predicates sidecar (LGPL FFI), so the whole module
//! is behind the off-by-default `indirect-predicates` feature.
//!
//! ## Scope (RED — Cycle 3a)
//!
//! Insert-and-split only. The covering / winding oracle below checks that the
//! result is an exact sub-triangulation of the base triangle (no flips, no
//! gaps/overlaps), and that every inserted point is incident as a vertex.
//! Segment conformance and cross-triangle parity are AR2b / AR3.

#[cfg(test)]
mod tests {
    //! RED tests for PR-CR-AR2a Cycle 3 (`split_single_triangle`).
    //!
    //! These exercise the intended GREEN behaviour through the public surface
    //! the GREEN implementer WILL add (`split_single_triangle`, `TypedPoint`,
    //! `RetriangulateError`) — none of which exists yet, so this module
    //! currently FAILS TO COMPILE/RESOLVE against the not-yet-written API. No
    //! production code is authored in this PR.
    //!
    //! The exact covering oracle (test `oracle_exact_covering_subtriangulation`)
    //! is a true cross-check: it recomputes every vertex coordinate — including
    //! the line-plane intersection of `Lpi` points — in `dashu::rational::RBig`,
    //! independently of the FFI-driven split path, then asserts the sub-tris
    //! tile the base triangle exactly (signed areas same-sign and summing
    //! exactly to the base).
    //!
    //! Out of scope for these tests (AR2b / AR3): segment conformance,
    //! cross-triangle split parity, full sidecar-corpus parity.

    use crate::arrangements::fast_trimesh::VertexCoords;
    use crate::arrangements::{
        split_single_triangle, FastTrimesh, Plane, RetriangulateError, TypedPoint,
    };
    use cad_primitives::Point3;
    use dashu::float::FBig;
    use dashu::rational::RBig;

    // ── Submesh + TypedPoint helpers ─────────────────────────────────

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

    /// An `Lpi` `TypedPoint` from line + plane generators.
    fn tp_lpi(line: [Point3; 2], plane: [Point3; 3]) -> TypedPoint {
        TypedPoint {
            coords: VertexCoords::Lpi { line, plane },
        }
    }

    // ── Exact-rational helpers (pure dashu — independent of the FFI) ──

    fn to_r(x: f64) -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    }

    /// Exact coordinates of a stored `VertexCoords`.
    ///
    /// `Explicit(p)` → exact rationals of p. `Lpi { line:[p,q], plane:[r,s,t] }`
    /// → the EXACT line-plane intersection: the point on `p + u(q-p)` lying in
    /// the plane through `r,s,t`, with `u = dot(r-p, n) / dot(q-p, n)` and
    /// `n = (s-r) × (t-r)`, all in `RBig`.
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
        }
    }

    /// Exact signed area (× 2) of a triangle PROJECTED to 2D per the ref
    /// plane. For `Plane::XY`, project to (x, y) and return the determinant
    /// `(b-a) × (c-a)` in `RBig` (== twice the signed area).
    fn exact_signed_area2_xy(a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> RBig {
        // (bx-ax)*(cy-ay) - (by-ay)*(cx-ax)
        let bx_ax = &b[0] - &a[0];
        let by_ay = &b[1] - &a[1];
        let cx_ax = &c[0] - &a[0];
        let cy_ay = &c[1] - &a[1];
        &(&bx_ax * &cy_ay) - &(&by_ay * &cx_ax)
    }

    /// Match a submesh vertex id by exact coordinate equality to a target
    /// `VertexCoords` (so an `Lpi` typed point matches the submesh `Lpi`
    /// vertex carrying the same exact intersection).
    fn find_vert_by_exact(subm: &FastTrimesh, target: &VertexCoords) -> Option<u32> {
        let want = exact_coords(target);
        (0..subm.num_verts()).find(|&v| exact_coords(subm.vert_coords(v)) == want)
    }

    // ════════════════════════════════════════════════════════════════
    // Hand cases (a)-(d): triangle-count contracts.
    // ════════════════════════════════════════════════════════════════

    /// (a) 1 interior point → 3 tris (barycentric fan).
    ///   base A; interior Explicit (2,1,0): x>0,y>0,x+y=3<4 → inside.
    #[test]
    fn case_a_one_interior_three_tris() {
        let a = xy_triangle_a();
        let mut subm = one_tri(a[0], a[1], a[2]);
        let pts = vec![tp_explicit(Point3::new(2.0, 1.0, 0.0))];
        split_single_triangle(&mut subm, &pts).expect("interior split must succeed");
        assert_eq!(subm.num_tris(), 3, "1 interior point → 3 tris");
    }

    /// (b) 1 on-edge point → 2 tris (edge split). Edge (0,1) = A0→A1, y=0.
    ///   Explicit (2,0,0) lies on it.
    #[test]
    fn case_b_one_on_edge_two_tris_explicit() {
        let a = xy_triangle_a();
        let mut subm = one_tri(a[0], a[1], a[2]);
        let pts = vec![tp_explicit(Point3::new(2.0, 0.0, 0.0))];
        split_single_triangle(&mut subm, &pts).expect("on-edge split must succeed");
        assert_eq!(subm.num_tris(), 2, "1 on-edge point → 2 tris");
    }

    /// (b-variant) on-edge point delivered as an `Lpi` whose generators put
    /// the exact intersection on edge (0,1) at (2,0,0). Exercises the implicit
    /// location path.
    ///   line = vertical at (2,0): (2,0,-1)→(2,0,1); plane = A (z=0).
    ///   intersection = (2,0,0), on edge (0,1).
    #[test]
    fn case_b_one_on_edge_two_tris_lpi() {
        let a = xy_triangle_a();
        let mut subm = one_tri(a[0], a[1], a[2]);
        let line = [Point3::new(2.0, 0.0, -1.0), Point3::new(2.0, 0.0, 1.0)];
        let plane = [a[0], a[1], a[2]];

        // Cross-check the fixture exactly: the LPI lands on edge (0,1) at
        // (2,0,0) (so this really is the on-edge implicit path).
        let xc = exact_coords(&VertexCoords::Lpi { line, plane });
        assert_eq!(
            xc,
            [to_r(2.0), to_r(0.0), to_r(0.0)],
            "fixture sanity: LPI must resolve exactly to (2,0,0)"
        );

        let pts = vec![tp_lpi(line, plane)];
        split_single_triangle(&mut subm, &pts).expect("on-edge LPI split must succeed");
        assert_eq!(subm.num_tris(), 2, "1 on-edge LPI point → 2 tris");
    }

    /// (c) 2 interior points → 5 tris (fan, then fan one of the 3).
    ///   (2,1,0) and (1,2,0): both interior (sums 3<4).
    #[test]
    fn case_c_two_interior_five_tris() {
        let a = xy_triangle_a();
        let mut subm = one_tri(a[0], a[1], a[2]);
        let pts = vec![
            tp_explicit(Point3::new(2.0, 1.0, 0.0)),
            tp_explicit(Point3::new(1.0, 2.0, 0.0)),
        ];
        split_single_triangle(&mut subm, &pts).expect("two interior splits must succeed");
        assert_eq!(subm.num_tris(), 5, "2 interior points → 5 tris");
    }

    /// (d) empty points → 1 tri (no-op). Corner-coincident points are dropped
    ///   upstream (aux_structure), so the driver never sees them.
    #[test]
    fn case_d_empty_one_tri() {
        let a = xy_triangle_a();
        let mut subm = one_tri(a[0], a[1], a[2]);
        split_single_triangle(&mut subm, &[]).expect("empty split is a no-op");
        assert_eq!(subm.num_tris(), 1, "no points → 1 tri");
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle: exact covering sub-triangulation (LOAD-BEARING, pure dashu).
    // ════════════════════════════════════════════════════════════════

    /// After inserting an interior Explicit point AND an on-edge `Lpi` point,
    /// the result must EXACTLY tile the base triangle:
    ///   - every sub-tri's signed area has the same sign as the base, and
    ///   - the sum of sub-tri signed areas EXACTLY equals the base's.
    /// All arithmetic in `RBig` (exact), independent of the FFI split path.
    /// Includes an `Lpi` vertex so LPI exact coords are exercised.
    #[test]
    fn oracle_exact_covering_subtriangulation() {
        let a = xy_triangle_a();
        let mut subm = one_tri(a[0], a[1], a[2]);

        // One interior Explicit + one on-edge LPI (on edge (0,1) at (2,0,0)).
        let line = [Point3::new(2.0, 0.0, -1.0), Point3::new(2.0, 0.0, 1.0)];
        let plane = [a[0], a[1], a[2]];
        let pts = vec![
            tp_explicit(Point3::new(1.0, 1.0, 0.0)),
            tp_lpi(line, plane),
        ];
        split_single_triangle(&mut subm, &pts).expect("split must succeed");

        // Base triangle's exact (×2) signed area.
        let ba = exact_coords(&VertexCoords::Explicit(a[0]));
        let bb = exact_coords(&VertexCoords::Explicit(a[1]));
        let bc = exact_coords(&VertexCoords::Explicit(a[2]));
        let base_area2 = exact_signed_area2_xy(&ba, &bb, &bc);
        assert!(
            base_area2 != RBig::ZERO,
            "base triangle must be non-degenerate"
        );
        let base_positive = base_area2 > RBig::ZERO;

        // Sum sub-tri signed areas; each must match the base sign.
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
            let pos = area2 > RBig::ZERO;
            assert_eq!(
                pos, base_positive,
                "sub-tri {t} winding sign disagrees with base (flip)"
            );
            sum = &sum + &area2;
        }
        assert_eq!(
            sum, base_area2,
            "sub-tri signed areas must sum EXACTLY to the base (covering, no gaps/overlaps)"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle: completeness + incidence (FFI — fail-loud if !AVAILABLE).
    // ════════════════════════════════════════════════════════════════

    /// Every input `TypedPoint` is a submesh vertex after insertion (matched
    /// by exact coords). Each interior point's containing sub-tri reports it
    /// boundary-inclusive-inside via the Cycle-1 `point_in_triangle` FFI; each
    /// on-edge point gives `orient2d_*(== Zero)` for exactly its edge and
    /// `!= Zero` for the other two.
    #[test]
    fn oracle_completeness_and_incidence_ffi() {
        use indirect_predicates_sidecar_rs::{
            init_fpu, orient2d_xy, point_in_triangle, ExplicitPoint3D, Sign as IpSign, AVAILABLE,
        };

        if !AVAILABLE {
            panic!(
                "indirect-predicates FFI shim not linked (AVAILABLE == false); \
                 the completeness/incidence oracle cannot run — refusing to pass silently"
            );
        }
        init_fpu();

        let a = xy_triangle_a();
        let mut subm = one_tri(a[0], a[1], a[2]);

        let interior = Point3::new(1.0, 1.0, 0.0);
        let line = [Point3::new(2.0, 0.0, -1.0), Point3::new(2.0, 0.0, 1.0)];
        let plane = [a[0], a[1], a[2]];
        let on_edge = VertexCoords::Lpi { line, plane }; // resolves to (2,0,0)
        let pts = vec![tp_explicit(interior), tp_lpi(line, plane)];
        split_single_triangle(&mut subm, &pts).expect("split must succeed");

        // (1) Completeness: each input point is a submesh vertex.
        let interior_vid = find_vert_by_exact(&subm, &VertexCoords::Explicit(interior))
            .expect("interior point must be a submesh vertex");
        let edge_vid =
            find_vert_by_exact(&subm, &on_edge).expect("on-edge LPI must be a submesh vertex");

        // The three original corners must still be present.
        for corner in a.iter() {
            assert!(
                find_vert_by_exact(&subm, &VertexCoords::Explicit(*corner)).is_some(),
                "original corner {corner:?} must remain a vertex"
            );
        }

        // Build explicit FFI handles for an exact (2,0,0) and the corners.
        let ip = |p: Point3| ExplicitPoint3D::new(p.x(), p.y(), p.z());
        let e_a0 = ip(a[0]);
        let e_a1 = ip(a[1]);
        let e_a2 = ip(a[2]);
        let e_edge = ip(Point3::new(2.0, 0.0, 0.0)); // exact intersection

        // (2) Interior incidence: the interior point lies (boundary-inclusive)
        // inside its containing sub-tri. Find a sub-tri containing the interior
        // vid and assert point_in_triangle on the corners of that sub-tri.
        let e_interior = ip(interior);
        let mut found_interior_host = false;
        for t in 0..subm.num_tris() {
            let vids = subm.tri(t);
            if vids.contains(&interior_vid) {
                found_interior_host = true;
                let c0 = ip(subm.vert(vids[0]));
                let c1 = ip(subm.vert(vids[1]));
                let c2 = ip(subm.vert(vids[2]));
                assert!(
                    point_in_triangle(&e_interior, &c0, &c1, &c2),
                    "interior point must be in its host sub-tri {t}"
                );
            }
        }
        assert!(
            found_interior_host,
            "interior vid {interior_vid} must belong to at least one sub-tri"
        );

        // (3) On-edge incidence: the on-edge point is collinear with base edge
        // (0,1) (orient2d_xy(A0, A1, P) == Zero) and strictly off the other two
        // edges (orient2d_xy != Zero for (A1,A2,P) and (A2,A0,P)).
        let _ = edge_vid; // vid located above; geometry asserted via FFI below.
        assert_eq!(
            orient2d_xy(&e_a0, &e_a1, &e_edge),
            IpSign::Zero,
            "on-edge LPI must be collinear with base edge (0,1)"
        );
        assert_ne!(
            orient2d_xy(&e_a1, &e_a2, &e_edge),
            IpSign::Zero,
            "on-edge LPI must NOT be on edge (1,2)"
        );
        assert_ne!(
            orient2d_xy(&e_a2, &e_a0, &e_edge),
            IpSign::Zero,
            "on-edge LPI must NOT be on edge (2,0)"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Topology validity after insertion.
    // ════════════════════════════════════════════════════════════════

    /// Basic invariants for the 2-interior case (→ 5 tris): no degenerate
    /// sub-tris (exact area != 0), the original 3 corners are still vertices,
    /// and the tri count matches the fan/edge-split Euler expectation.
    #[test]
    fn topology_validity_after_insertion() {
        let a = xy_triangle_a();
        let mut subm = one_tri(a[0], a[1], a[2]);
        let pts = vec![
            tp_explicit(Point3::new(2.0, 1.0, 0.0)),
            tp_explicit(Point3::new(1.0, 2.0, 0.0)),
        ];
        split_single_triangle(&mut subm, &pts).expect("split must succeed");

        // Euler for two successive interior fans: 1 → 3 → 5.
        assert_eq!(subm.num_tris(), 5, "two interior fans → 5 tris");

        // No degenerate sub-tri (exact RBig area != 0).
        for t in 0..subm.num_tris() {
            let v0 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 0)));
            let v1 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 1)));
            let v2 = exact_coords(subm.vert_coords(subm.tri_vert_id(t, 2)));
            assert!(
                exact_signed_area2_xy(&v0, &v1, &v2) != RBig::ZERO,
                "sub-tri {t} must be non-degenerate"
            );
        }

        // All three original corners survive.
        for corner in a.iter() {
            assert!(
                find_vert_by_exact(&subm, &VertexCoords::Explicit(*corner)).is_some(),
                "original corner {corner:?} must remain a vertex"
            );
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Error path: a point outside the base triangle has no host.
    // ════════════════════════════════════════════════════════════════

    /// A point well outside the base triangle cannot be located in any sub-tri
    /// → `RetriangulateError::NoContainingTriangle`. (The driver is fed only
    /// in-triangle points by aux_structure; this asserts the guard exists.)
    #[test]
    fn outside_point_yields_no_containing_triangle_error() {
        let a = xy_triangle_a();
        let mut subm = one_tri(a[0], a[1], a[2]);
        // (10,10,0): x+y = 20 > 4 → far outside A.
        let pts = vec![tp_explicit(Point3::new(10.0, 10.0, 0.0))];
        let err = split_single_triangle(&mut subm, &pts)
            .expect_err("outside point must error, not silently split");
        assert!(
            matches!(err, RetriangulateError::NoContainingTriangle { .. }),
            "expected NoContainingTriangle, got {err:?}"
        );
    }
}
