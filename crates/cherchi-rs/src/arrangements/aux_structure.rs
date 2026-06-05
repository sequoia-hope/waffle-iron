//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! # PR-CR-AR2a Cycle 3 — auxiliary intersection-point structure
//!
//! Second slice of the native re-triangulation phase (Cherchi 2022
//! `triangulation.cpp` setup, aux-structure portion). Given the AR1
//! per-pair tri-tri classification ([`crate::arrangements::PairClassification`]),
//! this module groups the typed intersection points into a global
//! deduplicated set plus per-base-triangle buckets (interior vs. each of the
//! three edges). The buckets drive the per-triangle re-triangulation in
//! `retriangulate.rs`.
//!
//! Mirrors the C++ `AuxiliaryStructure` that `triangulation.cpp` populates
//! before calling the per-triangle Delaunay re-triangulator: each base
//! triangle accumulates the intersection points that fall in its interior or
//! on one of its edges, with corner-coincident points dropped (they are
//! already vertices, so they introduce no new split).
//!
//! **Feature-gated FFI consumer**: exact "is this point on edge `i` / in the
//! interior" classification of an `Lpi` point routes through the
//! indirect-predicates sidecar (LGPL FFI), so the whole module is behind the
//! off-by-default `indirect-predicates` feature (WASM builds with it off).
//!
//! ## Scope (RED — Cycle 3a)
//!
//! This module groups points into per-triangle buckets. Segment-conformance
//! (which interior segments must appear as constrained edges) and
//! cross-triangle parity (matching split vertices across the shared edge of
//! two base triangles) are **out of scope** here — they are AR2b / AR3. The
//! tests below assert only bucketing, corner-drop, dedup, and length.

#[cfg(test)]
mod tests {
    //! RED tests for PR-CR-AR2a Cycle 3 (`group_intersection_points`).
    //!
    //! These exercise the intended GREEN behaviour through the public surface
    //! the GREEN implementer WILL add (`group_intersection_points`,
    //! `TypedPoint`, `TriangleAuxPoints`) — none of which exists yet, so this
    //! module currently FAILS TO COMPILE/RESOLVE against the not-yet-written
    //! API. No production code is authored in this PR.
    //!
    //! All coordinates are hard-coded (determinism). Hand-derivations are
    //! documented inline. The AR1 transversal fixtures are reused verbatim so
    //! the grouping is fed real `classify_all` output, not a hand-built mock.
    //!
    //! Out of scope for these tests (AR2b / AR3): segment-conformance,
    //! cross-triangle split parity, full sidecar-corpus parity.

    use crate::arrangements::fast_trimesh::VertexCoords;
    use crate::arrangements::{
        classify_all, detect_intersecting_pairs, group_intersection_points, FastTrimesh,
        IntersectionVertex, Plane, TriangleAuxPoints, TypedPoint,
    };
    use cad_primitives::Point3;

    // ── Fixture helpers (mirrors AR1's `soup_pair` / `xy_triangle_a`) ──

    /// Build a 2-triangle soup. Triangle A = index 0 (verts 0,1,2),
    /// triangle B = index 1 (verts 3,4,5).
    fn soup_pair(a: [Point3; 3], b: [Point3; 3]) -> FastTrimesh {
        let verts = vec![a[0], a[1], a[2], b[0], b[1], b[2]];
        let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
        FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap()
    }

    /// A0=(0,0,0), A1=(4,0,0), A2=(0,4,0). Lies in z=0; interior is
    /// `{x>0, y>0, x+y<4}`.
    fn xy_triangle_a() -> [Point3; 3] {
        [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(4.0, 0.0, 0.0),
            Point3::new(0.0, 4.0, 0.0),
        ]
    }

    /// All pairs detected by CR13, classified by AR1.
    fn classify(soup: &FastTrimesh) -> Vec<((u32, u32), crate::arrangements::PairClassification)> {
        let pairs = detect_intersecting_pairs(soup);
        classify_all(soup, &pairs)
    }

    /// Resolve a `TypedPoint`'s coords to a comparable key (the stored enum).
    fn coords_of(tp: &TypedPoint) -> &VertexCoords {
        &tp.coords
    }

    /// True iff `tp` is an `Lpi` typed point with the given generators.
    fn is_lpi_with(tp: &TypedPoint, line: [Point3; 2], plane: [Point3; 3]) -> bool {
        matches!(
            coords_of(tp),
            VertexCoords::Lpi { line: l, plane: p } if *l == line && *p == plane
        )
    }

    /// Count LPI typed points in the global set.
    fn count_lpi(points: &[TypedPoint]) -> usize {
        points
            .iter()
            .filter(|tp| matches!(coords_of(tp), VertexCoords::Lpi { .. }))
            .count()
    }

    /// Does any of a triangle's buckets (interior or any edge) contain `idx`?
    fn buckets_contain(aux: &TriangleAuxPoints, idx: u32) -> bool {
        aux.interior.contains(&idx)
            || aux.edges.iter().any(|e| e.contains(&idx))
    }

    // ════════════════════════════════════════════════════════════════
    // Test 1: Transversal bucketing (tilted 2-LPI case from AR1).
    // ════════════════════════════════════════════════════════════════

    /// A = xy_triangle_a (z=0). B = (1,1,-1),(1.5,0.5,1),(0.5,1.5,1):
    /// AR1 yields 2 LPIs, both interior to A. The two piercing edges
    /// (B0-B1, B0-B2) belong to triangle B; the pierced plane is A.
    ///
    /// Expected grouping:
    ///   - 2 distinct LPI `TypedPoint`s in the global set.
    ///   - Triangle A (id 0): both LPIs in `interior` (both strictly inside A,
    ///     neither on an A edge — sums 2.0 < 4, x>0, y>0).
    ///   - Triangle B (id 1): each LPI sits ON the B edge that produced it,
    ///     so it lands in the matching `edges[i]` bucket of B, NOT in B's
    ///     interior.
    ///   - Returned `Vec<TriangleAuxPoints>` length == num_tris == 2.
    #[test]
    fn transversal_two_lpi_bucketing() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, -1.0),
            Point3::new(1.5, 0.5, 1.0),
            Point3::new(0.5, 1.5, 1.0),
        ];
        let soup = soup_pair(a, b);
        let classified = classify(&soup);
        let (points, buckets) = group_intersection_points(&soup, &classified);

        // Exactly two LPI points produced and deduped.
        assert_eq!(
            count_lpi(&points),
            2,
            "expected exactly 2 LPI typed points, got {points:?}"
        );

        // One bucket entry per base triangle.
        assert_eq!(
            buckets.len() as u32,
            soup.num_tris(),
            "buckets must be indexed by base-tri id (len == num_tris)"
        );

        // The two LPI generators (per AR1: line = piercing edge, plane = A).
        let lpi_b0b1 = (
            [b[0], b[1]],
            [a[0], a[1], a[2]],
        );
        let lpi_b0b2 = (
            [b[0], b[2]],
            [a[0], a[1], a[2]],
        );

        // Find the global indices of each LPI typed point.
        let idx_of = |line: [Point3; 2], plane: [Point3; 3]| -> u32 {
            points
                .iter()
                .position(|tp| is_lpi_with(tp, line, plane))
                .unwrap_or_else(|| panic!("LPI generator not in global set: {points:?}"))
                as u32
        };
        let i01 = idx_of(lpi_b0b1.0, lpi_b0b1.1);
        let i02 = idx_of(lpi_b0b2.0, lpi_b0b2.1);

        // Triangle A (pierced) — both LPIs interior, none on an A edge.
        let aux_a = &buckets[0];
        assert!(
            aux_a.interior.contains(&i01) && aux_a.interior.contains(&i02),
            "triangle A must record both LPIs in `interior`, got {aux_a:?}"
        );
        for e in &aux_a.edges {
            assert!(
                !e.contains(&i01) && !e.contains(&i02),
                "triangle A must NOT record either LPI on an edge, got {aux_a:?}"
            );
        }

        // Triangle B (owns the piercing edges) — each LPI on the B edge that
        // produced it, in exactly one `edges[i]` bucket, never interior.
        let aux_b = &buckets[1];
        assert!(
            !aux_b.interior.contains(&i01) && !aux_b.interior.contains(&i02),
            "triangle B must NOT record either LPI as interior, got {aux_b:?}"
        );
        let count_in_edges = |idx: u32| -> usize {
            aux_b.edges.iter().filter(|e| e.contains(&idx)).count()
        };
        assert_eq!(
            count_in_edges(i01),
            1,
            "B's LPI from edge B0-B1 must be in exactly one edge bucket, got {aux_b:?}"
        );
        assert_eq!(
            count_in_edges(i02),
            1,
            "B's LPI from edge B0-B2 must be in exactly one edge bucket, got {aux_b:?}"
        );

        // B's edges are: edge0 = (B0,B1) = tri verts (0,1), edge1 = (B1,B2) =
        // (1,2), edge2 = (B2,B0) = (2,0). So the LPI on B0-B1 belongs to edge
        // slot 0; the LPI on B0-B2 (== edge (2,0)) belongs to edge slot 2.
        assert!(
            aux_b.edges[0].contains(&i01),
            "LPI from B0-B1 must be in B.edges[0], got {aux_b:?}"
        );
        assert!(
            aux_b.edges[2].contains(&i02),
            "LPI from B0-B2 must be in B.edges[2] (edge (2,0)), got {aux_b:?}"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Test 2: corner-coincident Explicit is dropped from its owner.
    // ════════════════════════════════════════════════════════════════

    /// AR1 `vtx_in_plane_opposite_edge_cross` case:
    ///   A = xy_triangle_a. B = (1,1,0),(2,0.5,1),(2,0.5,-1).
    ///   B0=(1,1,0) is a CORNER of B (corner 0) and lies inside A.
    ///   AR1 records B0 as an Explicit intersection vertex (tri=B, corner=0)
    ///   plus 1 LPI on edge B1-B2.
    ///
    /// Grouping contract: an explicit vertex that coincides with a corner of
    /// its OWNING triangle introduces no new split there → it must be ABSENT
    /// from B's buckets. But it IS interior to A (the pierced triangle), so it
    /// must land in A's `interior`.
    #[test]
    fn corner_coincident_explicit_dropped_from_owner() {
        let a = xy_triangle_a();
        let b = [
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(2.0, 0.5, 1.0),
            Point3::new(2.0, 0.5, -1.0),
        ];
        let soup = soup_pair(a, b);
        let classified = classify(&soup);
        let (points, buckets) = group_intersection_points(&soup, &classified);

        assert_eq!(
            buckets.len() as u32,
            soup.num_tris(),
            "buckets indexed by base-tri id"
        );

        // Locate the explicit typed point at B0 = (1,1,0).
        let b0 = Point3::new(1.0, 1.0, 0.0);
        let b0_idx = points
            .iter()
            .position(|tp| matches!(coords_of(tp), VertexCoords::Explicit(p) if *p == b0))
            .map(|i| i as u32);

        // The explicit B0 point may or may not be retained in the global set
        // (it is a real intersection vertex), but it MUST NOT appear in B's
        // own buckets — it is a corner of B.
        let aux_b = &buckets[1];
        if let Some(idx) = b0_idx {
            assert!(
                !buckets_contain(aux_b, idx),
                "corner-coincident B0 must be absent from triangle B's buckets, got {aux_b:?}"
            );
            // ... but it must be recorded interior to A (the pierced tri).
            let aux_a = &buckets[0];
            assert!(
                aux_a.interior.contains(&idx),
                "B0 (inside A, not an A corner) must be in A.interior, got {aux_a:?}"
            );
            for e in &aux_a.edges {
                assert!(
                    !e.contains(&idx),
                    "B0 is strictly inside A, not on an A edge, got {aux_a:?}"
                );
            }
        } else {
            panic!(
                "expected the explicit B0 intersection vertex to be present in \
                 the global typed-point set (interior to A); not found in {points:?}"
            );
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Test 3: dedup — same LPI generators from two pairs → one id.
    // ════════════════════════════════════════════════════════════════

    /// Two B triangles whose B0-B1 edge is IDENTICAL (shared edge) and which
    /// each pierce A at the same point produce the same LPI generators twice
    /// across two pairs. The global set must dedup them to ONE `TypedPoint`.
    ///
    /// Construction: A = xy_triangle_a (z=0).
    ///   Shared piercing edge E: P=(1,1,-1) → Q=(1,1,1) (vertical at (1,1),
    ///   crosses z=0 at (1,1,0), interior to A: 1+1=2<4).
    ///   B  = (P, Q, (2,2,1))   [tri 1]: lone vtx P, edges P-Q and P-(2,2,1).
    ///   C  = (P, Q, (2,2,-1))  [tri 2]: lone vtx Q? Either way both pierce A
    ///        along the SAME edge P-Q at the SAME point (1,1,0).
    /// Both pairs (A,B) and (A,C) emit an LPI with line=[P,Q], plane=A.
    /// Dedup ⇒ that generator appears once in the global set.
    #[test]
    fn dedup_identical_lpi_across_pairs() {
        let a = xy_triangle_a();
        let pp = Point3::new(1.0, 1.0, -1.0);
        let qq = Point3::new(1.0, 1.0, 1.0);
        let b = [pp, qq, Point3::new(2.0, 2.0, 1.0)];
        let c = [pp, qq, Point3::new(2.0, 2.0, -1.0)];

        // Build a 3-triangle soup sharing the P,Q vertices between B and C so
        // the piercing edge (and thus the LPI generators) are byte-identical.
        let verts = vec![
            a[0], a[1], a[2], // 0,1,2  (A)
            pp, qq, // 3,4  (shared P,Q)
            b[2], // 5  (B apex)
            c[2], // 6  (C apex)
        ];
        let tris = vec![[0u32, 1, 2], [3u32, 4, 5], [3u32, 4, 6]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();

        let pairs = detect_intersecting_pairs(&soup);
        let classified = classify_all(&soup, &pairs);
        let (points, buckets) = group_intersection_points(&soup, &classified);

        assert_eq!(buckets.len(), 3, "buckets indexed by base-tri id");

        // The shared-edge LPI generator: line = [P,Q], plane = A's corners.
        let line = [pp, qq];
        let plane = [a[0], a[1], a[2]];
        let matches: Vec<usize> = points
            .iter()
            .enumerate()
            .filter(|(_, tp)| is_lpi_with(tp, line, plane))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "identical LPI generators from two pairs must dedup to one TypedPoint, got {matches:?} in {points:?}"
        );

        // Sanity: the deduped point is recorded interior to A once (not twice).
        let idx = matches[0] as u32;
        let aux_a = &buckets[0];
        assert_eq!(
            aux_a.interior.iter().filter(|&&x| x == idx).count(),
            1,
            "deduped LPI must appear at most once in A.interior, got {aux_a:?}"
        );
    }

    // A `use` to keep IntersectionVertex referenced even if a future edit
    // drops a direct mention (the GREEN surface re-uses it in TypedPoint
    // conversions). Harmless in RED.
    #[allow(unused_imports)]
    use IntersectionVertex as _Ar1Vertex;
}
