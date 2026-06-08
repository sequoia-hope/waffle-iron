//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! # PR-CR-AR3b RED — global conforming soup + orchestration (tests only)
//!
//! This file is the **RED** slice of milestone M6 / PR-CR-AR3b: the failing
//! test module that pins the global-assembly public surface
//! (`ArrangementSoup`, `ArrangementError`, `Label`, `mesh_arrangement`, and the
//! `pub` prep helpers `merge_duplicated_vertices` /
//! `remove_degenerate_and_duplicated_triangles`) defined verbatim in
//! `specs/pr_cr_ar3b_global_soup.md`. It ports the C++
//! `meshArrangementPipeline` (`solve_intersections.cpp`), the input-prep
//! (`processing.cpp`), the triangle-soup container + jolly points
//! (`triangle_soup.{h,cpp}`), and the per-base-triangle assembly loop
//! (`triangulation.cpp`).
//!
//! **No production code is authored here** — the GREEN sub-agent adds the
//! orchestration port (and the `arrangements/mod.rs` + `lib.rs` re-exports) in
//! a later sub-step. The not-yet-written production symbols are referenced
//! through `crate::arrangements::{mesh_arrangement, ArrangementSoup,
//! ArrangementError, Label}` and `crate::arrangements::soup::{
//! merge_duplicated_vertices, remove_degenerate_and_duplicated_triangles}`, so
//! this module FAILS TO COMPILE under the `indirect-predicates` feature until
//! GREEN lands them (the intended RED state: unresolved-symbol errors for the
//! missing public items, nothing else).
//!
//! The tests cover all five spec §10 oracle invariants:
//! 1. **Conforming soup (load-bearing, EXACT):** no two output triangles
//!    overlap in their interiors — checked in pure-`dashu` rational arithmetic
//!    (every global vertex's exact coords; exact tri-tri interior-intersection
//!    test). The hand cases are designed to produce only `Explicit` + `Lpi`
//!    vertices (no constraint X-crossing → no TPI), so the exact oracle only
//!    needs explicit coords + line∩plane.
//! 2. **Every detected intersection realized:** each CR13 intersecting pair's
//!    intersection appears as shared/constraint edges; the LPI vertices are
//!    present with a SHARED global id.
//! 3. **Topology sanity:** coincident implicit points share one global id;
//!    every output triangle is non-degenerate (exact area > 0); edge-incidence
//!    / Euler sanity on a closed-input hand case.
//! 4. **Input-prep correctness:** duplicated input vertex merged + tris
//!    remapped; degenerate / duplicate input triangles removed (exact), labels
//!    OR-merged.
//! 5. **Hand cases:** two tetrahedra; axis-aligned two-box overlap; rotated
//!    two-box overlap; non-intersecting pair (soup == inputs modulo prep). Plus
//!    `jolly_count == 5` and the jolly tail present.
//! 6. **Loud deferral:** a coplanar-face pair → `Err(CoplanarPairDeferred)`
//!    (never silent / never a wrong soup).
//!
//! The pure-`dashu` exact helpers (`to_r`, `exact_coords`,
//! `exact_signed_area2`) are copied from `retriangulate.rs` / `enforce.rs` test
//! modules verbatim in style (test-only duplication is expected and fine), then
//! extended with an exact tri-tri interior-intersection test for invariant #1.

#[cfg(test)]
mod tests {
    //! RED tests for PR-CR-AR3b (`mesh_arrangement` + `ArrangementSoup` +
    //! prep). These exercise the intended GREEN behaviour through the public
    //! surface the GREEN implementer WILL add — none of which exists yet, so
    //! this module currently FAILS TO COMPILE/RESOLVE against the not-yet-
    //! written API. No production code is authored in this PR.

    use crate::arrangements::fast_trimesh::VertexCoords;
    // Public surface the spec mandates GREEN re-exports from `arrangements`:
    use crate::arrangements::{mesh_arrangement, ArrangementError, ArrangementSoup, Label};
    // Prep helpers GREEN makes `pub` in this very module:
    use crate::arrangements::soup::{
        merge_duplicated_vertices, remove_degenerate_and_duplicated_triangles,
    };
    use crate::arrangements::DeferReason;
    use crate::labeled_arrangement::InputId;
    use cad_primitives::Point3;
    use dashu::float::FBig;
    use dashu::rational::RBig;

    // ════════════════════════════════════════════════════════════════
    // Exact-rational helpers (pure dashu — independent of the FFI).
    // Copied in style from retriangulate.rs / enforce.rs test modules.
    // ════════════════════════════════════════════════════════════════

    fn to_r(x: f64) -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    }

    fn sub3(a: &[RBig; 3], b: &[RBig; 3]) -> [RBig; 3] {
        [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
    }
    fn cross3(a: &[RBig; 3], b: &[RBig; 3]) -> [RBig; 3] {
        [
            &(&a[1] * &b[2]) - &(&a[2] * &b[1]),
            &(&a[2] * &b[0]) - &(&a[0] * &b[2]),
            &(&a[0] * &b[1]) - &(&a[1] * &b[0]),
        ]
    }
    fn dot3(a: &[RBig; 3], b: &[RBig; 3]) -> RBig {
        &(&(&a[0] * &b[0]) + &(&a[1] * &b[1])) + &(&a[2] * &b[2])
    }

    /// Exact coordinates of a stored `VertexCoords`.
    ///
    /// `Explicit(p)` → exact rationals of p. `Lpi { line:[p,q], plane:[r,s,t] }`
    /// → the EXACT line-plane intersection (point on `p + u(q-p)` lying in the
    /// plane through `r,s,t`, `u = dot(r-p,n)/dot(q-p,n)`, `n = (s-r)×(t-r)`).
    /// `Tpi { v,w,u }` → the EXACT common intersection of the three supporting
    /// planes (Cramer's rule). All in `RBig`. The hand corpus is designed to
    /// avoid Tpi (so #1 needs only line∩plane), but the arm is included for
    /// completeness / safety.
    fn exact_coords(c: &VertexCoords) -> [RBig; 3] {
        let to_r3 = |p: &Point3| [to_r(p.x()), to_r(p.y()), to_r(p.z())];
        match c {
            VertexCoords::Explicit(p) => to_r3(p),
            VertexCoords::Lpi { line, plane } => {
                let p = to_r3(&line[0]);
                let q = to_r3(&line[1]);
                let r = to_r3(&plane[0]);
                let s = to_r3(&plane[1]);
                let t = to_r3(&plane[2]);
                let n = cross3(&sub3(&s, &r), &sub3(&t, &r));
                let rp = sub3(&r, &p);
                let qp = sub3(&q, &p);
                let num = dot3(&rp, &n);
                let den = dot3(&qp, &n);
                assert!(
                    den != RBig::ZERO,
                    "exact_coords: LPI line parallel to plane (den == 0) — bad fixture"
                );
                let u = &num / &den;
                [
                    &p[0] + &(&u * &qp[0]),
                    &p[1] + &(&u * &qp[1]),
                    &p[2] + &(&u * &qp[2]),
                ]
            }
            VertexCoords::Tpi { v, w, u } => {
                let plane_eqn = |tri: &[Point3; 3]| -> ([RBig; 3], RBig) {
                    let r = to_r3(&tri[0]);
                    let s = to_r3(&tri[1]);
                    let t = to_r3(&tri[2]);
                    let n = cross3(&sub3(&s, &r), &sub3(&t, &r));
                    let d = dot3(&n, &r);
                    (n, d)
                };
                let (n0, d0) = plane_eqn(v);
                let (n1, d1) = plane_eqn(w);
                let (n2, d2) = plane_eqn(u);
                let det_rows = |r0: &[RBig; 3], r1: &[RBig; 3], r2: &[RBig; 3]| -> RBig {
                    dot3(r0, &cross3(r1, r2))
                };
                let det = det_rows(&n0, &n1, &n2);
                assert!(
                    det != RBig::ZERO,
                    "exact_coords: TPI planes not in general position (det == 0) — bad fixture"
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
                [
                    &det_rows(&mx[0], &mx[1], &mx[2]) / &det,
                    &det_rows(&my[0], &my[1], &my[2]) / &det,
                    &det_rows(&mz[0], &mz[1], &mz[2]) / &det,
                ]
            }
        }
    }

    /// Exact signed area (× 2) of a triangle PROJECTED to the plane dropping the
    /// `axis`-th coordinate (0=x → YZ, 1=y → ZX, 2=z → XY). Returns the 2D
    /// determinant `(b-a) × (c-a)` in `RBig` (== twice the projected signed
    /// area). Used both for non-degeneracy (≠ 0 under the dominant axis) and for
    /// covering checks.
    fn exact_signed_area2(axis: usize, a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> RBig {
        // The two surviving coordinate indices after dropping `axis`.
        let (i, j) = match axis {
            0 => (1, 2),
            1 => (2, 0),
            _ => (0, 1),
        };
        let bx = &b[i] - &a[i];
        let by = &b[j] - &a[j];
        let cx = &c[i] - &a[i];
        let cy = &c[j] - &a[j];
        &(&bx * &cy) - &(&by * &cx)
    }

    /// The dominant-axis of a triangle's exact normal (index of the largest
    /// |component|). 0=x,1=y,2=z. Used to pick a non-degenerate 2D projection.
    fn dominant_axis(a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> usize {
        let n = cross3(&sub3(b, a), &sub3(c, a));
        let abs = |r: &RBig| if r < &RBig::ZERO { -r.clone() } else { r.clone() };
        let nx = abs(&n[0]);
        let ny = abs(&n[1]);
        let nz = abs(&n[2]);
        if nx >= ny && nx >= nz {
            0
        } else if ny >= nz {
            1
        } else {
            2
        }
    }

    /// Exact coords of every triangle of a soup, as `[[RBig;3];3]`.
    fn tri_exact(soup: &ArrangementSoup, t: usize) -> [[RBig; 3]; 3] {
        let [a, b, c] = soup.tris[t];
        [
            exact_coords(&soup.verts[a as usize]),
            exact_coords(&soup.verts[b as usize]),
            exact_coords(&soup.verts[c as usize]),
        ]
    }

    // ── Exact 3D tri-tri interior-intersection test (invariant #1) ────
    //
    // Two triangles "overlap in their interiors" iff they share a region of
    // positive area. We test this EXACTLY by: (1) if the two triangles are
    // coplanar, run an exact 2D overlap test (segment crossings + interior
    // containment) in their common dominant-axis projection; (2) if not
    // coplanar, they can only meet along a 1D segment (measure-zero, NOT an
    // interior area overlap), so they do NOT overlap. A conforming soup has NO
    // interior-overlapping pair.

    /// True iff all four points are coplanar (exact orient3d == 0).
    fn coplanar4(p: &[RBig; 3], a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> bool {
        // orient3d = (a-p)·((b-p)×(c-p)); zero ⇔ coplanar.
        let det = dot3(&sub3(a, p), &cross3(&sub3(b, p), &sub3(c, p)));
        det == RBig::ZERO
    }

    /// Are two triangles (each 3 exact pts) coplanar with each other?
    fn tris_coplanar(t0: &[[RBig; 3]; 3], t1: &[[RBig; 3]; 3]) -> bool {
        // Every vertex of t1 lies in the plane of t0.
        t1.iter()
            .all(|p| coplanar4(&t0[0], &t0[1], &t0[2], p))
    }

    /// Exact 2D point-strictly-inside-triangle (projected to `axis`). Strict:
    /// returns false on the boundary.
    fn point_strictly_in_tri2(
        axis: usize,
        p: &[RBig; 3],
        a: &[RBig; 3],
        b: &[RBig; 3],
        c: &[RBig; 3],
    ) -> bool {
        let d0 = exact_signed_area2(axis, a, b, p);
        let d1 = exact_signed_area2(axis, b, c, p);
        let d2 = exact_signed_area2(axis, c, a, p);
        let pos = d0 > RBig::ZERO && d1 > RBig::ZERO && d2 > RBig::ZERO;
        let neg = d0 < RBig::ZERO && d1 < RBig::ZERO && d2 < RBig::ZERO;
        pos || neg
    }

    /// Exact 2D open-segment proper-crossing test (projected to `axis`): do the
    /// open segments (p0,p1) and (q0,q1) cross at a single interior point? Uses
    /// strict orientation sign opposition on both sides (proper crossing only —
    /// shared endpoints / collinear overlap are NOT a proper crossing, so a
    /// conforming soup's shared edges don't count as interior overlap).
    fn segments_properly_cross2(
        axis: usize,
        p0: &[RBig; 3],
        p1: &[RBig; 3],
        q0: &[RBig; 3],
        q1: &[RBig; 3],
    ) -> bool {
        let o1 = exact_signed_area2(axis, p0, p1, q0);
        let o2 = exact_signed_area2(axis, p0, p1, q1);
        let o3 = exact_signed_area2(axis, q0, q1, p0);
        let o4 = exact_signed_area2(axis, q0, q1, p1);
        let opp = |a: &RBig, b: &RBig| {
            (a > &RBig::ZERO && b < &RBig::ZERO) || (a < &RBig::ZERO && b > &RBig::ZERO)
        };
        opp(&o1, &o2) && opp(&o3, &o4)
    }

    /// EXACT test: do triangles `t0` and `t1` overlap in their INTERIORS
    /// (share positive area)? Used to assert the conforming-soup invariant #1.
    fn tris_interiors_overlap(t0: &[[RBig; 3]; 3], t1: &[[RBig; 3]; 3]) -> bool {
        // Non-coplanar triangles can only meet along a measure-zero segment.
        if !tris_coplanar(t0, t1) {
            return false;
        }
        // Coplanar: project to the common dominant axis and do an exact 2D
        // overlap test.
        let axis = dominant_axis(&t0[0], &t0[1], &t0[2]);
        // (a) any vertex of one strictly inside the other.
        for p in t1.iter() {
            if point_strictly_in_tri2(axis, p, &t0[0], &t0[1], &t0[2]) {
                return true;
            }
        }
        for p in t0.iter() {
            if point_strictly_in_tri2(axis, p, &t1[0], &t1[1], &t1[2]) {
                return true;
            }
        }
        // (b) any pair of edges properly crosses.
        let e0 = [(0usize, 1usize), (1, 2), (2, 0)];
        for (a, b) in e0 {
            for (c, d) in e0 {
                if segments_properly_cross2(axis, &t0[a], &t0[b], &t1[c], &t1[d]) {
                    return true;
                }
            }
        }
        false
    }

    /// Assert no two output triangles of `soup` overlap in their interiors
    /// (invariant #1). Real (non-jolly) triangles only.
    fn assert_conforming(soup: &ArrangementSoup) {
        let n = soup.tris.len();
        let exacts: Vec<[[RBig; 3]; 3]> = (0..n).map(|t| tri_exact(soup, t)).collect();
        for a in 0..n {
            for b in (a + 1)..n {
                assert!(
                    !tris_interiors_overlap(&exacts[a], &exacts[b]),
                    "output triangles {a} and {b} overlap in their interiors — soup not conforming"
                );
            }
        }
    }

    /// Assert every output triangle is non-degenerate (exact area ≠ 0 under its
    /// dominant axis) — invariant #3.
    fn assert_no_degenerate_tris(soup: &ArrangementSoup) {
        for t in 0..soup.tris.len() {
            let [a, b, c] = tri_exact(soup, t);
            let axis = dominant_axis(&a, &b, &c);
            assert!(
                exact_signed_area2(axis, &a, &b, &c) != RBig::ZERO,
                "output triangle {t} is degenerate (exact zero area)"
            );
        }
    }

    /// Number of distinct welded global ids actually referenced by triangles.
    fn referenced_vertex_count(soup: &ArrangementSoup) -> usize {
        let mut ids: Vec<u32> = soup.tris.iter().flatten().copied().collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    }

    /// Assert the jolly tail: exactly 5 jolly points appended, NOT referenced by
    /// any triangle. Returns the count of real (non-jolly) verts.
    fn assert_jolly_tail(soup: &ArrangementSoup) -> usize {
        assert_eq!(soup.jolly_count, 5, "jolly_count must be exactly 5");
        let n = soup.verts.len();
        assert!(n >= 5, "verts must include the 5 jolly points");
        let real = n - 5;
        // No triangle references a jolly id (>= real).
        for tri in &soup.tris {
            for &id in tri {
                assert!(
                    (id as usize) < real,
                    "triangle references a jolly point id {id} (real verts = {real})"
                );
            }
        }
        real
    }

    /// Assert `labels` is 1:1 with `tris`.
    fn assert_label_alignment(soup: &ArrangementSoup) {
        assert_eq!(
            soup.tris.len(),
            soup.labels.len(),
            "labels must be 1:1 with tris"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Hand corpus — closed solids as triangle soups (flat coords + tris +
    // per-triangle labels). All transversal / non-coplanar so they avoid the
    // TPI path (no constraint X-crossing): a simple two-box / two-tetra overlap
    // whose per-face intersection is a single non-self-crossing segment yields
    // only Explicit + Lpi vertices.
    // ════════════════════════════════════════════════════════════════

    const A: InputId = InputId(0);
    const B: InputId = InputId(1);

    /// Axis-aligned unit cube `[x0,x0+1]×[y0,y0+1]×[z0,z0+1]` as 8 corners + 12
    /// triangles, with every triangle carrying `label`. Returns (flat coords,
    /// tris, labels). Outward-CCW winding.
    fn cube(x0: f64, y0: f64, z0: f64, side: f64, label: InputId) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let x1 = x0 + side;
        let y1 = y0 + side;
        let z1 = z0 + side;
        // 8 corners, ids 0..8.
        let corners = [
            (x0, y0, z0), // 0
            (x1, y0, z0), // 1
            (x1, y1, z0), // 2
            (x0, y1, z0), // 3
            (x0, y0, z1), // 4
            (x1, y0, z1), // 5
            (x1, y1, z1), // 6
            (x0, y1, z1), // 7
        ];
        let mut coords = Vec::with_capacity(24);
        for (x, y, z) in corners {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
        // 12 outward-facing triangles (2 per face).
        let tris = vec![
            // bottom z=z0 (normal -z)
            [0, 2, 1],
            [0, 3, 2],
            // top z=z1 (normal +z)
            [4, 5, 6],
            [4, 6, 7],
            // front y=y0 (normal -y)
            [0, 1, 5],
            [0, 5, 4],
            // back y=y1 (normal +y)
            [3, 7, 6],
            [3, 6, 2],
            // left x=x0 (normal -x)
            [0, 4, 7],
            [0, 7, 3],
            // right x=x1 (normal +x)
            [1, 2, 6],
            [1, 6, 5],
        ];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    /// A regular-ish tetrahedron: 4 corners + 4 triangles, all `label`.
    /// `o` is the apex-origin; spans roughly `[o, o+s]`.
    fn tetra(ox: f64, oy: f64, oz: f64, s: f64, label: InputId) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let corners = [
            (ox, oy, oz),         // 0
            (ox + s, oy, oz),     // 1
            (ox, oy + s, oz),     // 2
            (ox, oy, oz + s),     // 3
        ];
        let mut coords = Vec::with_capacity(12);
        for (x, y, z) in corners {
            coords.push(x);
            coords.push(y);
            coords.push(z);
        }
        // 4 faces (outward winding for an apex-at-origin tetra).
        let tris = vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let labels = vec![vec![label]; tris.len()];
        (coords, tris, labels)
    }

    /// Concatenate two solids' (coords, tris, labels) into one soup. Triangle
    /// indices of the second solid are offset by the first's vertex count.
    fn concat(
        s0: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
        s1: (Vec<f64>, Vec<[u32; 3]>, Vec<Label>),
    ) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Label>) {
        let (mut coords, mut tris, mut labels) = s0;
        let off = (coords.len() / 3) as u32;
        coords.extend_from_slice(&s1.0);
        for t in s1.1 {
            tris.push([t[0] + off, t[1] + off, t[2] + off]);
        }
        labels.extend(s1.2);
        (coords, tris, labels)
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #4 — input-prep correctness (called DIRECTLY, no full run).
    // ════════════════════════════════════════════════════════════════

    /// `merge_duplicated_vertices`: a duplicated coordinate triple collapses to
    /// ONE global id and every referencing triangle index is remapped.
    ///
    /// Input: 4 vertex slots where slot 3 == slot 0 exactly. Two triangles:
    /// [0,1,2] and [3,1,2] (== [0,1,2] after merge). Expect 3 surviving verts;
    /// both tris remap to indices over {0,1,2} with slot-3 → 0.
    #[test]
    fn prep_merge_duplicated_vertices_collapses_and_remaps() {
        // slots: 0=(0,0,0) 1=(1,0,0) 2=(0,1,0) 3=(0,0,0) duplicate of 0.
        let coords = vec![
            0.0, 0.0, 0.0, // 0
            1.0, 0.0, 0.0, // 1
            0.0, 1.0, 0.0, // 2
            0.0, 0.0, 0.0, // 3 == 0
        ];
        let tris = vec![[0u32, 1, 2], [3u32, 1, 2]];
        let (verts, remapped) = merge_duplicated_vertices(&coords, &tris);

        // Only the 3 distinct referenced coordinates survive (insertion-order).
        assert_eq!(verts.len(), 3, "duplicate vertex must collapse to 3 verts");
        assert_eq!(verts[0], Point3::new(0.0, 0.0, 0.0));
        assert_eq!(verts[1], Point3::new(1.0, 0.0, 0.0));
        assert_eq!(verts[2], Point3::new(0.0, 1.0, 0.0));

        // Both triangles remap to the same global ids (slot-3 dup → 0).
        assert_eq!(remapped[0], [0, 1, 2]);
        assert_eq!(remapped[1], [0, 1, 2], "duplicated vertex slot 3 remaps to 0");
    }

    /// `remove_degenerate_and_duplicated_triangles`:
    /// (a) an exactly-collinear (degenerate) triangle is dropped;
    /// (b) a duplicate triangle (same sorted verts) with a DIFFERENT label is
    ///     dropped and its label OR-merged into the survivor.
    #[test]
    fn prep_remove_degenerate_and_dup_triangles() {
        // verts: 0,1,2 a real tri; 3 collinear with 0,1 (on x-axis) makes a
        // degenerate tri [0,1,3]. Tri [2,1,0] is [0,1,2] sorted == survivor's,
        // a duplicate with a different label.
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0), // 0
            Point3::new(2.0, 0.0, 0.0), // 1
            Point3::new(0.0, 2.0, 0.0), // 2
            Point3::new(1.0, 0.0, 0.0), // 3 collinear with 0,1
        ];
        let tris = vec![
            [0u32, 1, 2], // survivor, label A
            [0u32, 1, 3], // degenerate (0,1,3 collinear) → dropped
            [2u32, 1, 0], // duplicate of [0,1,2] (sorted), label B → merged
        ];
        let labels = vec![vec![A], vec![A], vec![B]];

        let (kept_tris, kept_labels) =
            remove_degenerate_and_duplicated_triangles(&verts, &tris, &labels);

        assert_eq!(
            kept_tris.len(),
            1,
            "degenerate + duplicate triangles must both be removed → 1 survivor"
        );
        assert_eq!(kept_tris[0], [0, 1, 2], "survivor keeps first-seen winding");

        // Label OR-merged (sorted-unique union of A and B).
        let mut got = kept_labels[0].clone();
        got.sort_by_key(|i| i.0);
        assert_eq!(
            got,
            vec![A, B],
            "duplicate triangle's label must be OR-merged into the survivor"
        );
    }

    /// `remove_degenerate_and_duplicated_triangles`: a duplicate of the SAME
    /// label is still dropped but the survivor's label is unchanged (idempotent
    /// OR-merge).
    #[test]
    fn prep_duplicate_same_label_merges_idempotently() {
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(2.0, 0.0, 0.0),
            Point3::new(0.0, 2.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [1u32, 2, 0]]; // same sorted set
        let labels = vec![vec![A], vec![A]];
        let (kept_tris, kept_labels) =
            remove_degenerate_and_duplicated_triangles(&verts, &tris, &labels);
        assert_eq!(kept_tris.len(), 1, "duplicate dropped");
        assert_eq!(kept_labels[0], vec![A], "idempotent OR-merge keeps just A");
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #5(d) + jolly tail — a NON-intersecting pair: soup == inputs
    // modulo prep (same triangle count, no new vertices except the jolly tail).
    // ════════════════════════════════════════════════════════════════

    /// Two disjoint cubes (no overlap, no shared face). After `mesh_arrangement`
    /// the output triangle count equals the input (24, all pass-through), no new
    /// real vertices are introduced, and the 5-point jolly tail is appended.
    #[test]
    fn case_d_non_intersecting_pair_is_passthrough() {
        let a = cube(0.0, 0.0, 0.0, 1.0, A);
        let b = cube(5.0, 5.0, 5.0, 1.0, B); // far away, no intersection
        let (coords, tris, labels) = concat(a, b);
        let n_in_tris = tris.len();
        let n_in_verts = coords.len() / 3;

        let soup =
            mesh_arrangement(&coords, &tris, &labels).expect("disjoint pair must not error");

        assert_label_alignment(&soup);
        let real = assert_jolly_tail(&soup);

        // No new real vertices beyond the (deduped) inputs; here all 16 corners
        // are distinct so real == 16.
        assert_eq!(
            real, n_in_verts,
            "disjoint pair introduces no new real vertices"
        );
        // No splits → same triangle count.
        assert_eq!(
            soup.tris.len(),
            n_in_tris,
            "disjoint pair is straight pass-through (no new triangles)"
        );
        assert_conforming(&soup);
        assert_no_degenerate_tris(&soup);
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #5(b) — axis-aligned two-box overlap → conforming soup.
    // ════════════════════════════════════════════════════════════════

    /// Box A = [0,2]^3, Box B = [1,3]^3. They interpenetrate; each box face that
    /// the other box crosses is split along the intersection curve. The result
    /// must be a conforming soup (no interior overlaps), all non-degenerate,
    /// labels 1:1, jolly tail present.
    #[test]
    fn case_b_axis_aligned_box_overlap_conforming() {
        let a = cube(0.0, 0.0, 0.0, 2.0, A);
        let b = cube(1.0, 1.0, 1.0, 2.0, B);
        let (coords, tris, labels) = concat(a, b);

        let soup =
            mesh_arrangement(&coords, &tris, &labels).expect("box overlap must not error");

        assert_label_alignment(&soup);
        assert_jolly_tail(&soup);
        assert_no_degenerate_tris(&soup);
        // Invariant #1 (load-bearing): no two output triangles overlap interiors.
        assert_conforming(&soup);

        // Invariant #2/#3: intersection introduced NEW real vertices (the soup
        // is not a pure pass-through — the boxes were cut).
        let real = soup.verts.len() - 5;
        assert!(
            real > 16,
            "interpenetrating boxes must introduce new intersection vertices \
             (real verts {real} should exceed the 16 input corners)"
        );
        assert!(
            soup.tris.len() > 24,
            "interpenetrating boxes must produce more than the 24 input triangles"
        );

        // Every label is a non-empty subset of {A, B} carried from a parent.
        for lab in &soup.labels {
            assert!(!lab.is_empty(), "every output label must be non-empty");
            for id in lab {
                assert!(*id == A || *id == B, "labels only over input solids A/B");
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #2 — every detected intersection realized (shared welded id).
    //
    // Uses a minimal two-triangle transversal pair (not two closed solids) so
    // the exact LPI intersection vertices are easy to hand-derive, and asserts
    // they appear with a SINGLE welded global id shared across BOTH triangles'
    // sub-triangulations (the load-bearing interner weld, spec §7).
    // ════════════════════════════════════════════════════════════════

    /// Triangle Ta in the z=0 plane and triangle Tb crossing it transversally.
    /// Tb pierces Ta's interior along a segment; the two piercing points are
    /// LPI vertices. After arrangement, each LPI vertex must be present EXACTLY
    /// ONCE in `verts` (welded id) and referenced by sub-triangles of BOTH base
    /// triangles.
    ///
    /// Ta: (0,0,0),(4,0,0),(0,4,0)  — z=0.
    /// Tb: (1,1,-1),(1,1,1),(3,0,1) crossing the z=0 plane.
    /// The exact z=0 crossings of Tb's edges with Ta lie on the segment Ta∩Tb.
    #[test]
    fn case_intersection_realized_welded_lpi_ids() {
        // Two single triangles forming a transversal X. Each triangle is its own
        // "solid" label for this structural check.
        let coords = vec![
            // Ta (z=0)
            0.0, 0.0, 0.0, // 0
            4.0, 0.0, 0.0, // 1
            0.0, 4.0, 0.0, // 2
            // Tb (crosses z=0)
            1.0, 1.0, -1.0, // 3
            1.0, 1.0, 1.0, // 4
            3.0, 0.0, 1.0, // 5
        ];
        let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
        let labels = vec![vec![A], vec![B]];

        let soup =
            mesh_arrangement(&coords, &tris, &labels).expect("transversal pair must not error");

        assert_label_alignment(&soup);
        assert_jolly_tail(&soup);
        assert_no_degenerate_tris(&soup);
        assert_conforming(&soup);

        // The intersection of Ta with Tb's edges produces LPI vertices. Collect
        // all real (non-jolly) vertices that are NOT plain input corners; each
        // such intersection vertex must appear EXACTLY once (welded) and be
        // referenced by triangles inheriting BOTH the A label and the B label.
        let real = soup.verts.len() - 5;
        let input_corners: Vec<[RBig; 3]> = (0..6)
            .map(|i| {
                exact_coords(&VertexCoords::Explicit(Point3::new(
                    coords[i * 3],
                    coords[i * 3 + 1],
                    coords[i * 3 + 2],
                )))
            })
            .collect();

        let mut intersection_vids: Vec<u32> = Vec::new();
        for v in 0..real as u32 {
            let xc = exact_coords(&soup.verts[v as usize]);
            let is_corner = input_corners.iter().any(|c| *c == xc);
            if !is_corner {
                intersection_vids.push(v);
            }
        }
        assert!(
            !intersection_vids.is_empty(),
            "the transversal crossing must realize at least one intersection vertex"
        );

        // Each intersection vertex's EXACT coords are unique across `verts`
        // (welded to ONE id — no duplicate implicit point).
        for &v in &intersection_vids {
            let xv = exact_coords(&soup.verts[v as usize]);
            let dup_count = (0..real as u32)
                .filter(|&w| exact_coords(&soup.verts[w as usize]) == xv)
                .count();
            assert_eq!(
                dup_count, 1,
                "intersection vertex {v} must be welded to a SINGLE global id (found {dup_count})"
            );
        }

        // At least one intersection vertex is shared by triangles carrying A and
        // by triangles carrying B (it lies on the conformed edge of both).
        let mut shared = false;
        for &v in &intersection_vids {
            let mut on_a = false;
            let mut on_b = false;
            for (t, tri) in soup.tris.iter().enumerate() {
                if tri.contains(&v) {
                    if soup.labels[t].contains(&A) {
                        on_a = true;
                    }
                    if soup.labels[t].contains(&B) {
                        on_b = true;
                    }
                }
            }
            if on_a && on_b {
                shared = true;
            }
        }
        assert!(
            shared,
            "an intersection vertex must be shared (welded) across both A and B sub-triangles"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #5(a) — two tetrahedra overlapping → conforming soup.
    // ════════════════════════════════════════════════════════════════

    /// Two interpenetrating tetrahedra. Tetra A at origin, Tetra B offset so it
    /// overlaps A's interior but is not coplanar with any A face (transversal
    /// only → no TPI). Result must be a conforming soup.
    #[test]
    fn case_a_two_tetrahedra_overlap_conforming() {
        let a = tetra(0.0, 0.0, 0.0, 3.0, A);
        // B offset along the (1,1,1) diagonal so it pierces A transversally.
        let b = tetra(1.0, 1.0, 1.0, 3.0, B);
        let (coords, tris, labels) = concat(a, b);

        let soup =
            mesh_arrangement(&coords, &tris, &labels).expect("tetra overlap must not error");

        assert_label_alignment(&soup);
        assert_jolly_tail(&soup);
        assert_no_degenerate_tris(&soup);
        assert_conforming(&soup);

        // The tetra pair interpenetrates → soup is not a pure pass-through.
        assert!(
            soup.tris.len() > 8,
            "interpenetrating tetrahedra must produce more than the 8 input faces"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #5(c) — a ROTATED two-box overlap → conforming soup.
    // ════════════════════════════════════════════════════════════════

    /// Box A axis-aligned [0,2]^3; Box B is a unit-ish box rotated 45° about the
    /// z-axis and shifted so it interpenetrates A while NOT sharing any coplanar
    /// face (the rotated side faces are oblique to A's). Transversal only → no
    /// TPI. Result must be conforming.
    #[test]
    fn case_c_rotated_box_overlap_conforming() {
        // Box B: a rectangular prism whose footprint is a 45°-rotated square
        // centred at (1,1, z) spanning z in [-1, 3] (so it pierces A top/bottom
        // and the oblique side walls cross A's vertical faces transversally).
        // Footprint square corners (rotated 45°, "radius" r = 1.2):
        //   (1+r, 1, *), (1, 1+r, *), (1-r, 1, *), (1, 1-r, *)
        let r = 1.2;
        let zlo = -1.0;
        let zhi = 3.0;
        let b_corners = [
            (1.0 + r, 1.0, zlo), // 0
            (1.0, 1.0 + r, zlo), // 1
            (1.0 - r, 1.0, zlo), // 2
            (1.0, 1.0 - r, zlo), // 3
            (1.0 + r, 1.0, zhi), // 4
            (1.0, 1.0 + r, zhi), // 5
            (1.0 - r, 1.0, zhi), // 6
            (1.0, 1.0 - r, zhi), // 7
        ];
        let mut bcoords = Vec::with_capacity(24);
        for (x, y, z) in b_corners {
            bcoords.push(x);
            bcoords.push(y);
            bcoords.push(z);
        }
        // 12 outward triangles: bottom (0,1,2,3), top (4,5,6,7), 4 sides.
        let btris = vec![
            // bottom z=zlo (normal -z): wind CW seen from below
            [0u32, 2, 1],
            [0, 3, 2],
            // top z=zhi (normal +z)
            [4u32, 5, 6],
            [4, 6, 7],
            // sides (each quad → 2 tris), outward
            [0u32, 1, 5],
            [0, 5, 4],
            [1u32, 2, 6],
            [1, 6, 5],
            [2u32, 3, 7],
            [2, 7, 6],
            [3u32, 0, 4],
            [3, 4, 7],
        ];
        let blabels = vec![vec![B]; btris.len()];

        let (coords, tris, labels) =
            concat(cube(0.0, 0.0, 0.0, 2.0, A), (bcoords, btris, blabels));

        let soup =
            mesh_arrangement(&coords, &tris, &labels).expect("rotated box overlap must not error");

        assert_label_alignment(&soup);
        assert_jolly_tail(&soup);
        assert_no_degenerate_tris(&soup);
        assert_conforming(&soup);
        assert!(
            soup.tris.len() > 24,
            "rotated interpenetrating box must produce more than the 24 input triangles"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #3 — Euler / edge-incidence sanity on a closed-input hand case.
    // ════════════════════════════════════════════════════════════════

    /// On the disjoint-cubes case (a clean closed input that passes straight
    /// through), the output triangle soup is a union of two closed 2-manifolds:
    /// every (undirected) edge is shared by exactly 2 triangles, and V−E+F = 2·2
    /// = 4 (two genus-0 components). This is the edge-incidence / Euler sanity
    /// check on closed input (invariant #3) — done on the pass-through case so
    /// the topology is exactly predictable (no cut-triangle skips).
    #[test]
    fn euler_edge_incidence_on_closed_passthrough() {
        let a = cube(0.0, 0.0, 0.0, 1.0, A);
        let b = cube(10.0, 0.0, 0.0, 1.0, B);
        let (coords, tris, labels) = concat(a, b);

        let soup = mesh_arrangement(&coords, &tris, &labels).expect("closed input must not error");
        assert_jolly_tail(&soup);

        // Build undirected-edge incidence over real triangles.
        use std::collections::HashMap;
        let mut edge_count: HashMap<(u32, u32), u32> = HashMap::new();
        for tri in &soup.tris {
            for &(i, j) in &[(0usize, 1usize), (1, 2), (2, 0)] {
                let a = tri[i];
                let b = tri[j];
                let key = if a < b { (a, b) } else { (b, a) };
                *edge_count.entry(key).or_insert(0) += 1;
            }
        }
        for (e, c) in &edge_count {
            assert_eq!(
                *c, 2,
                "edge {e:?} must be shared by exactly 2 triangles (closed 2-manifold)"
            );
        }

        // Euler: V − E + F = 4 for two disjoint genus-0 closed surfaces.
        let v = referenced_vertex_count(&soup) as i64;
        let e = edge_count.len() as i64;
        let f = soup.tris.len() as i64;
        assert_eq!(
            v - e + f,
            4,
            "two disjoint closed cubes: V−E+F must equal 4 (got V={v} E={e} F={f})"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Invariant #6 — loud deferral. A coplanar-face pair must return a
    // classified ArrangementError, NEVER a silent / wrong soup.
    // ════════════════════════════════════════════════════════════════

    /// Two triangles in the SAME plane (z=0) that overlap. AR1 classifies this
    /// pair `Deferred(Coplanar | SingleCoplanarEdge)`, which the orchestration
    /// must surface as `ArrangementError::CoplanarPairDeferred` — loud, never a
    /// silent pass or a wrong soup.
    #[test]
    fn coplanar_pair_is_loudly_deferred() {
        // Ta and Tb both in z=0, overlapping (Tb shifted into Ta's interior).
        let coords = vec![
            // Ta
            0.0, 0.0, 0.0, // 0
            4.0, 0.0, 0.0, // 1
            0.0, 4.0, 0.0, // 2
            // Tb (same plane z=0, overlapping Ta)
            1.0, 1.0, 0.0, // 3
            5.0, 1.0, 0.0, // 4
            1.0, 5.0, 0.0, // 5
        ];
        let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
        let labels = vec![vec![A], vec![B]];

        let err = mesh_arrangement(&coords, &tris, &labels)
            .expect_err("coplanar overlapping pair must be loudly deferred, not silently handled");
        assert!(
            matches!(
                err,
                ArrangementError::CoplanarPairDeferred {
                    reason: DeferReason::Coplanar | DeferReason::SingleCoplanarEdge,
                    ..
                }
            ),
            "expected CoplanarPairDeferred(Coplanar|SingleCoplanarEdge), got {err:?}"
        );

        // NOTE: the N16 deep-recursion wall (DeepRecursionRequired) is NOT
        // readily constructible as a deterministic single hand case here — it
        // requires a constraint segment crossing MULTIPLE existing constraints
        // (the global seg2tris / coplanar jollyPoint path). Per the task brief,
        // that sub-case is skipped; the coplanar deferral above is the required
        // loud-deferral test. If/when a deterministic deep-recursion fixture is
        // found, add an analogous `expect_err(... DeepRecursionRequired ...)`.
    }

    // ════════════════════════════════════════════════════════════════
    // Error path — label/triangle count mismatch is loud.
    // ════════════════════════════════════════════════════════════════

    /// `in_labels` not 1:1 with `tris` → `LabelCountMismatch`, never a silent
    /// truncation.
    #[test]
    fn label_count_mismatch_is_loud() {
        let (coords, tris, mut labels) = cube(0.0, 0.0, 0.0, 1.0, A);
        labels.pop(); // now labels.len() == tris.len() - 1
        let err = mesh_arrangement(&coords, &tris, &labels)
            .expect_err("label/tri count mismatch must error");
        assert!(
            matches!(err, ArrangementError::LabelCountMismatch { .. }),
            "expected LabelCountMismatch, got {err:?}"
        );
    }
}
