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
        subm.edge_id(a, b).map_or(false, |e| subm.edge_is_constr(e))
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
        let tpi =
            ImplicitPoint3DTpi::new(&gv0, &gv1, &gv2, &gw0, &gw1, &gw2, &gu0, &gu1, &gu2);

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
        let a = xy_triangle_a();
        let b = tilted_b();
        let soup = soup_pair(a, b);

        let pairs = detect_intersecting_pairs(&soup);
        let classified = classify_all(&soup, &pairs);
        let (points, buckets) = group_intersection_points(&soup, &classified);
        let seg_lists = group_constraint_segments(&soup, &classified, &points);

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
        let seg_lists = group_constraint_segments(&soup, &classified, &points);
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
