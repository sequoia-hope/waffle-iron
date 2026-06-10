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
//!
//! ## Edge-index convention
//!
//! Edge `i` of a triangle connects corners `i` and `(i + 1) % 3`
//! (`edge0 = (0, 1)`, `edge1 = (1, 2)`, `edge2 = (2, 0)`). The buckets here are
//! indexed identically, matching the AR1 LPI generators and the per-triangle
//! re-triangulator (`retriangulate.rs`).

use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::{FastTrimesh, IntersectionVertex, PairClassification, Plane};
use crate::predicates::{point_in_triangle_3d, PointLocation};
use cad_primitives::Point3;
use indirect_predicates_sidecar_rs::{
    init_fpu, orient2d_xy, orient2d_yz, orient2d_zx, point_in_triangle, AsGenericPoint,
    ExplicitPoint3D, ImplicitPoint3DLpi, Sign as IpSign,
};

/// A typed intersection point destined to become a re-triangulation vertex.
///
/// Wraps the [`VertexCoords`] kind so the global deduplicated set can carry
/// both explicit input points and `Lpi` line-plane intersections by their
/// generators (exact equality via `VertexCoords`'s derived `PartialEq`).
#[derive(Clone, Debug, PartialEq)]
pub struct TypedPoint {
    pub coords: VertexCoords,
}

/// Per-base-triangle bucket of intersection-point ids.
///
/// `interior` holds points strictly inside the triangle; `edges[i]` holds
/// points on edge `i` (corners `i` and `(i + 1) % 3`). Ids index the global
/// `Vec<TypedPoint>` returned alongside the buckets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriangleAuxPoints {
    pub interior: Vec<u32>,
    pub edges: [Vec<u32>; 3],
}

/// Group the AR1 per-pair classification into a global deduplicated typed-point
/// set plus per-base-triangle buckets (interior vs. each of the three edges).
///
/// Mirrors the `AuxiliaryStructure` population in the C++ `triangulation.cpp`
/// setup: each base triangle accumulates the intersection points falling in its
/// interior or on one of its edges, with corner-coincident points dropped from
/// their owner (they are already vertices and introduce no split).
///
/// Only `Transversal` pairs contribute; `Deferred` / `Disjoint` pairs are
/// skipped (loud markers handled upstream in AR1).
pub fn group_intersection_points(
    soup: &FastTrimesh,
    classified: &[((u32, u32), PairClassification)],
) -> (Vec<TypedPoint>, Vec<TriangleAuxPoints>) {
    init_fpu();

    let mut points: Vec<TypedPoint> = Vec::new();
    let mut buckets: Vec<TriangleAuxPoints> =
        vec![TriangleAuxPoints::default(); soup.num_tris() as usize];

    // Intern a `VertexCoords` into the global set, returning its index. Exact
    // structural dedup via `VertexCoords: PartialEq` (mirrors the C++
    // `flat_hash_set` dedup of intersection vertices).
    let intern = |points: &mut Vec<TypedPoint>, coords: VertexCoords| -> u32 {
        if let Some(i) = points.iter().position(|tp| tp.coords == coords) {
            return i as u32;
        }
        points.push(TypedPoint { coords });
        (points.len() - 1) as u32
    };

    for ((ta, tb), classification) in classified {
        let vertices = match classification {
            PairClassification::Transversal { vertices } => vertices,
            PairClassification::Deferred(_) | PairClassification::Disjoint => continue,
        };

        for iv in vertices {
            match iv {
                IntersectionVertex::Explicit { tri, point, .. } => {
                    // The OTHER triangle (the one that does NOT own this corner)
                    // is the pierced triangle; the owner drops the point (it is
                    // already a corner there).
                    let other = if *tri == *ta { *tb } else { *ta };
                    place_point_in_triangle(
                        soup,
                        other,
                        *point,
                        &mut points,
                        &mut buckets,
                        &intern,
                    );
                }
                IntersectionVertex::Lpi { line, plane, .. } => {
                    let coords = VertexCoords::Lpi {
                        line: *line,
                        plane: *plane,
                    };
                    let id = intern(&mut points, coords);

                    // OWNER X: the triangle whose two corners equal `line` — the
                    // LPI sits on that edge.
                    if let Some((x, edge_i)) = owner_edge(soup, *ta, *tb, line) {
                        push_unique(&mut buckets[x as usize].edges[edge_i], id);
                    }

                    // PIERCED Y: the triangle whose 3 corners equal `plane`.
                    if let Some(y) = pierced_triangle(soup, *ta, *tb, plane) {
                        place_lpi_in_pierced(soup, y, line, plane, id, &mut buckets);
                    }
                }
            }
        }
    }

    (points, buckets)
}

/// The [`VertexCoords`] an [`IntersectionVertex`] interns as, identical to the
/// mapping used inside [`group_intersection_points`] (Explicit → `Explicit`,
/// Lpi → `Lpi { line, plane }`). Keeps both call sites' interning consistent.
fn vertex_coords_of(iv: &IntersectionVertex) -> VertexCoords {
    match iv {
        IntersectionVertex::Explicit { point, .. } => VertexCoords::Explicit(*point),
        IntersectionVertex::Lpi { line, plane, .. } => VertexCoords::Lpi {
            line: *line,
            plane: *plane,
        },
    }
}

/// A constraint segment to be enforced as constrained mesh edge(s) within a
/// base triangle's submesh during re-triangulation (AR2b/Cycle C).
///
/// `endpoints` are interned `TypedPoint` ids (positions in the `points` Vec
/// produced by [`group_intersection_points`]); `source_tri` is the OPPOSITE
/// triangle of the originating AR1 pair (its 3 corners define the supporting
/// plane used to construct TPI points where two constraint segments cross —
/// see the spec's `create_tpi` design).
#[derive(Clone, Debug, PartialEq)]
pub struct ConstraintSegment {
    /// Interned `TypedPoint` ids (into the `points` Vec) of the two endpoints.
    pub endpoints: (u32, u32),
    /// The OPPOSITE triangle's 3 corners (the segment's supporting plane).
    pub source_tri: [Point3; 3],
}

/// Extract one constraint segment per `Transversal` pair per base triangle.
///
/// For each `PairClassification::Transversal { vertices }` of pair `(ta, tb)`,
/// the transversal intersection segment is defined by the interned ids of its
/// intersection vertices. A non-degenerate crossing has exactly two distinct
/// endpoint ids → one `ConstraintSegment` is pushed to `result[ta]` (with
/// `source_tri` = `tb`'s 3 corners) and one to `result[tb]` (with `source_tri`
/// = `ta`'s 3 corners). If, after de-duping interned ids, there are not exactly
/// two distinct endpoints (e.g. a single touch point), NO segment is emitted
/// for that pair. `Deferred` / `Disjoint` pairs contribute nothing.
///
/// `points` MUST be the same interned set returned by
/// [`group_intersection_points`] for `(soup, classified)` — ids are resolved by
/// structural equality of each vertex's [`VertexCoords`] against it.
///
/// The returned Vec is indexed by base-triangle id (length == `num_tris`).
pub fn group_constraint_segments(
    soup: &FastTrimesh,
    classified: &[((u32, u32), PairClassification)],
    points: &[TypedPoint],
) -> Vec<Vec<ConstraintSegment>> {
    let mut result: Vec<Vec<ConstraintSegment>> = vec![Vec::new(); soup.num_tris() as usize];

    // Resolve an IntersectionVertex to its interned id in `points` by structural
    // equality of its VertexCoords (same logic `group_intersection_points` uses
    // to intern). Returns None if the vertex was not interned (should not happen
    // when `points` is the matching set, but we never panic in production).
    let interned_id = |iv: &IntersectionVertex| -> Option<u32> {
        let coords = vertex_coords_of(iv);
        points
            .iter()
            .position(|tp| tp.coords == coords)
            .map(|i| i as u32)
    };

    for ((ta, tb), classification) in classified {
        let vertices = match classification {
            PairClassification::Transversal { vertices } => vertices,
            PairClassification::Deferred(_) | PairClassification::Disjoint => continue,
        };

        // Collect distinct interned endpoint ids (order-preserving dedup).
        let mut ids: Vec<u32> = Vec::new();
        for iv in vertices {
            if let Some(id) = interned_id(iv) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }

        // Only a non-degenerate crossing (exactly two distinct endpoints)
        // yields a constraint segment; a single touch contributes nothing.
        if ids.len() != 2 {
            continue;
        }
        let endpoints = (ids[0], ids[1]);

        let corners = |t: u32| -> [Point3; 3] {
            [
                soup.tri_vert(t, 0),
                soup.tri_vert(t, 1),
                soup.tri_vert(t, 2),
            ]
        };

        // In ta's list the source_tri is tb's corners, and vice-versa.
        result[*ta as usize].push(ConstraintSegment {
            endpoints,
            source_tri: corners(*tb),
        });
        result[*tb as usize].push(ConstraintSegment {
            endpoints,
            source_tri: corners(*ta),
        });
    }

    result
}

/// Place an explicit intersection `point` into base triangle `t`'s buckets,
/// classifying it as interior / on-edge / outside (outside → dropped).
fn place_point_in_triangle(
    soup: &FastTrimesh,
    t: u32,
    point: Point3,
    points: &mut Vec<TypedPoint>,
    buckets: &mut [TriangleAuxPoints],
    intern: &impl Fn(&mut Vec<TypedPoint>, VertexCoords) -> u32,
) {
    let c0 = soup.tri_vert(t, 0);
    let c1 = soup.tri_vert(t, 1);
    let c2 = soup.tri_vert(t, 2);
    match point_in_triangle_3d(point, c0, c1, c2) {
        PointLocation::StrictlyOutside => {}
        PointLocation::StrictlyInside => {
            let id = intern(points, VertexCoords::Explicit(point));
            push_unique(&mut buckets[t as usize].interior, id);
        }
        PointLocation::OnBoundary => {
            let id = intern(points, VertexCoords::Explicit(point));
            if let Some(edge_i) = explicit_edge_of(soup, t, point) {
                push_unique(&mut buckets[t as usize].edges[edge_i], id);
            }
            // On-boundary but no single edge identified (corner-coincident); it
            // is already a vertex of `t` → introduces no split, so drop it.
        }
    }
}

/// Edge index `i` of triangle `t` (corners `i`, `(i + 1) % 3`) on which the
/// explicit `point` lies, via the soup's reference-plane `orient2d` (exact,
/// CR10). Returns `None` if `point` coincides with a corner or lies on no
/// single edge.
fn explicit_edge_of(soup: &FastTrimesh, t: u32, point: Point3) -> Option<usize> {
    let c = [
        soup.tri_vert(t, 0),
        soup.tri_vert(t, 1),
        soup.tri_vert(t, 2),
    ];
    // Corner-coincident → no split edge.
    if point == c[0] || point == c[1] || point == c[2] {
        return None;
    }
    for (i, _) in c.iter().enumerate() {
        let a = c[i];
        let b = c[(i + 1) % 3];
        if orient2d_plane_sign(soup.ref_plane(), a, b, point) == crate::predicates::Sign::Zero {
            return Some(i);
        }
    }
    None
}

/// Exact `orient2d` of three explicit points projected to the soup's reference
/// plane, returning a cherchi-rs [`Sign`](crate::predicates::Sign).
fn orient2d_plane_sign(plane: Plane, a: Point3, b: Point3, p: Point3) -> crate::predicates::Sign {
    use crate::predicates::orient2d;
    use cad_primitives::Point2;
    match plane {
        Plane::XY => orient2d(
            Point2::new(a.x(), a.y()),
            Point2::new(b.x(), b.y()),
            Point2::new(p.x(), p.y()),
        ),
        Plane::YZ => orient2d(
            Point2::new(a.y(), a.z()),
            Point2::new(b.y(), b.z()),
            Point2::new(p.y(), p.z()),
        ),
        Plane::ZX => orient2d(
            Point2::new(a.z(), a.x()),
            Point2::new(b.z(), b.x()),
            Point2::new(p.z(), p.x()),
        ),
    }
}

/// Of the pair `(ta, tb)`, the triangle whose two corners equal the LPI's
/// `line` endpoints (exact `Point3` eq), plus the edge index `i` on which the
/// line lies (corners `i`, `(i + 1) % 3`).
fn owner_edge(soup: &FastTrimesh, ta: u32, tb: u32, line: &[Point3; 2]) -> Option<(u32, usize)> {
    for &t in &[ta, tb] {
        let c = [
            soup.tri_vert(t, 0),
            soup.tri_vert(t, 1),
            soup.tri_vert(t, 2),
        ];
        for (i, _) in c.iter().enumerate() {
            let a = c[i];
            let b = c[(i + 1) % 3];
            let unordered_match = (a == line[0] && b == line[1]) || (a == line[1] && b == line[0]);
            if unordered_match {
                return Some((t, i));
            }
        }
    }
    None
}

/// Of the pair `(ta, tb)`, the triangle whose three corners equal the LPI's
/// `plane` generators (exact `Point3` eq, order-agnostic).
fn pierced_triangle(soup: &FastTrimesh, ta: u32, tb: u32, plane: &[Point3; 3]) -> Option<u32> {
    for &t in &[ta, tb] {
        let c = [
            soup.tri_vert(t, 0),
            soup.tri_vert(t, 1),
            soup.tri_vert(t, 2),
        ];
        if same_point_set(&c, plane) {
            return Some(t);
        }
    }
    None
}

/// True iff the two 3-point arrays are the same set (order-agnostic, exact eq).
fn same_point_set(a: &[Point3; 3], b: &[Point3; 3]) -> bool {
    a.iter().all(|p| b.contains(p)) && b.iter().all(|p| a.contains(p))
}

/// Place an LPI (by interned `id`) into the pierced triangle `y`'s buckets:
/// on the unique edge whose supporting line passes through the LPI (exact FFI
/// `orient2d == Zero`), else in the interior.
fn place_lpi_in_pierced(
    soup: &FastTrimesh,
    y: u32,
    line: &[Point3; 2],
    plane: &[Point3; 3],
    id: u32,
    buckets: &mut [TriangleAuxPoints],
) {
    let c = [
        soup.tri_vert(y, 0),
        soup.tri_vert(y, 1),
        soup.tri_vert(y, 2),
    ];

    // Backing generators (kept alive for the LPI handle) + corner handles.
    let gens = lpi_backing(line, plane);
    let lpi = ImplicitPoint3DLpi::new(&gens[0], &gens[1], &gens[2], &gens[3], &gens[4]);
    let ce: [ExplicitPoint3D; 3] = [explicit(c[0]), explicit(c[1]), explicit(c[2])];

    // Confirm the LPI is in `y` (boundary-inclusive). If not, drop it.
    if !point_in_triangle(&lpi, &ce[0], &ce[1], &ce[2]) {
        return;
    }

    let mut on_edge: Option<usize> = None;
    for (i, _) in ce.iter().enumerate() {
        let a = &ce[i];
        let b = &ce[(i + 1) % 3];
        if ip_orient2d_plane(soup.ref_plane(), a, b, &lpi) == IpSign::Zero {
            on_edge = Some(i);
            break;
        }
    }

    match on_edge {
        Some(i) => push_unique(&mut buckets[y as usize].edges[i], id),
        None => push_unique(&mut buckets[y as usize].interior, id),
    }
}

/// The five explicit generators backing an LPI handle (`line` = `p, q`;
/// `plane` = `r, s, t`). Kept in a separate array so the handle can borrow them
/// without a self-referential struct.
fn lpi_backing(line: &[Point3; 2], plane: &[Point3; 3]) -> [ExplicitPoint3D; 5] {
    [
        explicit(line[0]),
        explicit(line[1]),
        explicit(plane[0]),
        explicit(plane[1]),
        explicit(plane[2]),
    ]
}

/// Build an FFI explicit-point handle from a `Point3`.
fn explicit(p: Point3) -> ExplicitPoint3D {
    ExplicitPoint3D::new(p.x(), p.y(), p.z())
}

/// `orient2d` of `(a, b, p)` projected to `plane`, via the FFI (so an implicit
/// `p` is handled exactly).
fn ip_orient2d_plane(
    plane: Plane,
    a: &impl AsGenericPoint,
    b: &impl AsGenericPoint,
    p: &impl AsGenericPoint,
) -> IpSign {
    match plane {
        Plane::XY => orient2d_xy(a, b, p),
        Plane::YZ => orient2d_yz(a, b, p),
        Plane::ZX => orient2d_zx(a, b, p),
    }
}

/// Push `id` into `vec` only if not already present.
fn push_unique(vec: &mut Vec<u32>, id: u32) {
    if !vec.contains(&id) {
        vec.push(id);
    }
}

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
        aux.interior.contains(&idx) || aux.edges.iter().any(|e| e.contains(&idx))
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
        crate::arrangements::require_ffi_shim();
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
        let lpi_b0b1 = ([b[0], b[1]], [a[0], a[1], a[2]]);
        let lpi_b0b2 = ([b[0], b[2]], [a[0], a[1], a[2]]);

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
        let count_in_edges =
            |idx: u32| -> usize { aux_b.edges.iter().filter(|e| e.contains(&idx)).count() };
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
        crate::arrangements::require_ffi_shim();
        let a = xy_triangle_a();
        let pp = Point3::new(1.0, 1.0, -1.0);
        let qq = Point3::new(1.0, 1.0, 1.0);
        let b = [pp, qq, Point3::new(2.0, 2.0, 1.0)];
        let c = [pp, qq, Point3::new(2.0, 2.0, -1.0)];

        // Build a 3-triangle soup sharing the P,Q vertices between B and C so
        // the piercing edge (and thus the LPI generators) are byte-identical.
        let verts = vec![
            a[0], a[1], a[2], // 0,1,2  (A)
            pp, qq,   // 3,4  (shared P,Q)
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

    // ════════════════════════════════════════════════════════════════
    // PR-CR-AR2b Cycle B, Deliverable 3 — constraint-segment extraction.
    //
    // GREEN will add to this module:
    //   pub struct ConstraintSegment {
    //       pub endpoints: (u32, u32),
    //       pub source_tri: [Point3; 3],
    //   }
    //   pub fn group_constraint_segments(
    //       soup: &FastTrimesh,
    //       classified: &[((u32, u32), PairClassification)],
    //       points: &[TypedPoint],
    //   ) -> Vec<Vec<ConstraintSegment>>   // indexed by base-tri id
    // (re-exported from `arrangements/mod.rs`).
    //
    // Contract (the load-bearing shape these tests pin): for each
    // `Transversal` pair (ta, tb) yielding two intersection vertices, BOTH
    // base triangles ta and tb receive ONE ConstraintSegment whose two
    // `endpoints` are the interned ids (in the `points`/`TypedPoint` set
    // from `group_intersection_points`) of the pair's two intersection
    // vertices, and whose `source_tri` is the OTHER triangle's 3 corners
    // (in ta's list source_tri = tb's corners; in tb's list = ta's).
    //
    // FEATURE-GATED: this whole module is `#[cfg(feature =
    // "indirect-predicates")]`, so run with `--features indirect-predicates`.
    // These tests MUST fail to RESOLVE against the missing symbols (RED by
    // compile error). No production code is authored here.
    //
    // GREEN may adjust the param list slightly to reuse interned ids; the
    // tests below depend only on the contract above, not the exact params,
    // EXCEPT they call the documented 3-arg signature. If GREEN narrows the
    // signature it must keep this call shape (soup, classified, points).
    // ════════════════════════════════════════════════════════════════

    use crate::arrangements::{group_constraint_segments, ConstraintSegment};

    /// The AR2a tilted 2-LPI transversal fixture (same as
    /// `transversal_two_lpi_bucketing`): A = xy_triangle_a; B forms one
    /// intersection SEGMENT with A from two LPIs (edges B0-B1 and B0-B2
    /// piercing A). This yields exactly one constraint segment per base
    /// triangle of the pair.
    fn tilted_b() -> [Point3; 3] {
        [
            Point3::new(1.0, 1.0, -1.0),
            Point3::new(1.5, 0.5, 1.0),
            Point3::new(0.5, 1.5, 1.0),
        ]
    }

    /// True iff the two 3-point arrays are the same SET (order-agnostic).
    fn same_corner_set(a: &[Point3; 3], b: &[Point3; 3]) -> bool {
        a.iter().all(|p| b.contains(p)) && b.iter().all(|p| a.contains(p))
    }

    /// The interned id (in `points`) of the LPI typed point with the given
    /// generators. Panics if absent.
    fn lpi_id(points: &[TypedPoint], line: [Point3; 2], plane: [Point3; 3]) -> u32 {
        points
            .iter()
            .position(|tp| is_lpi_with(tp, line, plane))
            .unwrap_or_else(|| panic!("LPI generator not interned: {points:?}")) as u32
    }

    /// Each `Transversal` pair contributes ONE ConstraintSegment to BOTH
    /// base triangles' lists; the returned Vec is length == num_tris; the
    /// segment's source_tri is the OPPOSITE triangle's 3 corners; and its
    /// endpoints are the interned ids of the two intersection vertices.
    #[test]
    fn transversal_one_constraint_segment_per_base_triangle() {
        let a = xy_triangle_a();
        let b = tilted_b();
        let soup = soup_pair(a, b);
        let classified = classify(&soup);
        // Interned point set (same call the extraction reuses).
        let (points, _buckets) = group_intersection_points(&soup, &classified);

        let segs: Vec<Vec<ConstraintSegment>> =
            group_constraint_segments(&soup, &classified, &points);

        // Length == num_tris (indexed by base-tri id).
        assert_eq!(
            segs.len() as u32,
            soup.num_tris(),
            "constraint-segment lists must be indexed by base-tri id"
        );

        // Triangle A (id 0) has exactly one ConstraintSegment.
        assert_eq!(
            segs[0].len(),
            1,
            "triangle A must have exactly one constraint segment, got {:?}",
            segs[0]
        );
        // Triangle B (id 1) has exactly one ConstraintSegment.
        assert_eq!(
            segs[1].len(),
            1,
            "triangle B must have exactly one constraint segment, got {:?}",
            segs[1]
        );

        let seg_a = &segs[0][0];
        let seg_b = &segs[1][0];

        // source_tri: A's segment carries B's corners; B's carries A's.
        assert!(
            same_corner_set(&seg_a.source_tri, &b),
            "triangle A's constraint segment source_tri must be B's corners, got {:?}",
            seg_a.source_tri
        );
        assert!(
            same_corner_set(&seg_b.source_tri, &a),
            "triangle B's constraint segment source_tri must be A's corners, got {:?}",
            seg_b.source_tri
        );

        // endpoints: the interned ids of the two intersection vertices
        // (the two LPIs of this pair). For this fixture both LPIs share
        // plane = A and have line = B0-B1 and B0-B2 respectively.
        let id01 = lpi_id(&points, [b[0], b[1]], [a[0], a[1], a[2]]);
        let id02 = lpi_id(&points, [b[0], b[2]], [a[0], a[1], a[2]]);
        let expected: std::collections::BTreeSet<u32> = [id01, id02].into_iter().collect();

        let endpoints_set = |s: &ConstraintSegment| -> std::collections::BTreeSet<u32> {
            [s.endpoints.0, s.endpoints.1].into_iter().collect()
        };

        assert_eq!(
            endpoints_set(seg_a),
            expected,
            "A's constraint-segment endpoints must be the two interned LPI ids"
        );
        assert_eq!(
            endpoints_set(seg_b),
            expected,
            "B's constraint-segment endpoints must be the two interned LPI ids"
        );

        // Endpoints are distinct (a real segment, not a degenerate point).
        assert_ne!(
            seg_a.endpoints.0, seg_a.endpoints.1,
            "A's constraint segment endpoints must be distinct"
        );
        assert_ne!(
            seg_b.endpoints.0, seg_b.endpoints.1,
            "B's constraint segment endpoints must be distinct"
        );
    }

    /// A disjoint pair contributes NO constraint segment to any base
    /// triangle's list.
    #[test]
    fn disjoint_pair_contributes_no_constraint_segment() {
        let a = xy_triangle_a();
        // Far-away B — well separated, Disjoint per AR1.
        let b = [
            Point3::new(100.0, 100.0, 100.0),
            Point3::new(101.0, 100.0, 100.0),
            Point3::new(100.0, 101.0, 100.0),
        ];
        let soup = soup_pair(a, b);
        let classified = classify(&soup);
        let (points, _buckets) = group_intersection_points(&soup, &classified);

        let segs = group_constraint_segments(&soup, &classified, &points);

        assert_eq!(segs.len(), 2, "indexed by base-tri id");
        assert!(
            segs.iter().all(|list| list.is_empty()),
            "a disjoint pair must contribute no constraint segments, got {segs:?}"
        );
    }
}

#[cfg(test)]
mod ar3c_tests {
    //! RED oracles for PR-CR-AR3c — point identity must be GEOMETRIC, not
    //! structural (mirrors the C++ `aux_structure.cpp:230
    //! addVertexInSortedList`, whose comparator is `genericPoint::lessThan` —
    //! EXACT geometric order — so one geometric point gets ONE identity
    //! regardless of which side's generators construct it).
    //!
    //! The anchor: `classify_pair` is presentation-dependent in how many
    //! STRUCTURAL vertices it emits for a pair whose intersection-segment
    //! endpoint lies ON an edge of the pierced triangle. With the pair
    //! presented (cube-tri, peg-tri), the B-vs-A side finds both endpoints
    //! (2 LPIs) and the `li.size() > 1` early-out skips the A side → 2
    //! vertices. Presented (peg-tri, cube-tri), the first side finds only ONE
    //! endpoint (the cube-diagonal LPI at the shared point), so the second
    //! side ALSO runs and re-derives the same geometric point with DIFFERENT
    //! generators (peg-edge × cube-plane) plus the second endpoint → 3
    //! structural vertices for 2 geometric points. Structural interning then
    //! over-counts to 3 ids, and `group_constraint_segments`'
    //! `ids.len() != 2` guard SILENTLY drops the pair's constraint segment
    //! from BOTH triangles (the BL1 fence-gap → flood-leak witness).
    //!
    //! Oracle (geometric identity): grouping must yield exactly TWO distinct
    //! GEOMETRIC endpoints for this pair under BOTH presentations, and a
    //! constraint segment must be recorded for BOTH triangles under BOTH
    //! presentations. Exactness via pure-`dashu` rational line∩plane (no
    //! FFI, no tolerance).

    use crate::arrangements::fast_trimesh::VertexCoords;
    use crate::arrangements::{
        classify_pair, group_constraint_segments, group_intersection_points, FastTrimesh,
        PairClassification, Plane,
    };
    use cad_primitives::Point3;
    use dashu::float::FBig;
    use dashu::rational::RBig;

    // ── pure-dashu exact coords of a stored VertexCoords (test-local copy,
    //    same style as the retriangulate/enforce/soup test modules) ──

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

    /// Exact coordinates of `Explicit` / `Lpi` (the anchor produces no Tpi).
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
                let num = dot3(&sub3(&r, &p), &n);
                let den = dot3(&sub3(&q, &p), &n);
                assert!(
                    den != RBig::ZERO,
                    "LPI line parallel to plane — bad fixture"
                );
                let u = &num / &den;
                let qp = sub3(&q, &p);
                [
                    &p[0] + &(&u * &qp[0]),
                    &p[1] + &(&u * &qp[1]),
                    &p[2] + &(&u * &qp[2]),
                ]
            }
            VertexCoords::Tpi { .. } => panic!("anchor fixture must not produce Tpi"),
        }
    }

    /// The anchor pair, in already-scaled (multiplier-applied) coordinates
    /// from the through-cut fixture: a cube bottom-face triangle whose
    /// DIAGONAL edge passes through the intersection-segment endpoint
    /// (2,2,0), and a peg wall triangle (plane x=2) one of whose edges
    /// pierces the cube face at the SAME point (2,2,0).
    ///
    /// Hand derivation (z=0 ∧ x=2):
    ///   peg ∩ {z=0}: edge (2,2,-4)-(2,2,12) at (2,2,0); edge
    ///     (2,2,12)-(2,6,-4) at (2,5,0)            → segment (2,2,0)-(2,5,0)
    ///   cube-tri ∩ {x=2}: diagonal (8,8,0)-(0,0,0) at (2,2,0); edge
    ///     (0,8,0)-(8,8,0) at (2,8,0)              → segment (2,2,0)-(2,8,0)
    ///   ⇒ the pair's intersection segment is (2,2,0)-(2,5,0); the endpoint
    ///     (2,2,0) is reachable BOTH as Lpi{peg-edge × cube-plane} and as
    ///     Lpi{cube-diagonal × peg-plane}.
    fn cube_tri() -> [Point3; 3] {
        [
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(0.0, 8.0, 0.0),
            Point3::new(8.0, 8.0, 0.0),
        ]
    }
    fn peg_tri() -> [Point3; 3] {
        [
            Point3::new(2.0, 2.0, -4.0),
            Point3::new(2.0, 2.0, 12.0),
            Point3::new(2.0, 6.0, -4.0),
        ]
    }

    /// Anchor repro: under BOTH pair presentations, grouping/interning must
    /// resolve the pair to exactly 2 distinct GEOMETRIC endpoints — (2,2,0)
    /// and (2,5,0) — and record one constraint segment on BOTH triangles.
    ///
    /// Pre-AR3c this FAILS for the (peg, cube) presentation: 3 structural
    /// ids are interned for the 2 geometric points and the segment is
    /// silently dropped from both triangles.
    #[test]
    fn anchor_pair_order_invariant_geometric_endpoints() {
        crate::arrangements::require_ffi_shim();

        for (name, first, second) in [
            ("cube-first", cube_tri(), peg_tri()),
            ("peg-first", peg_tri(), cube_tri()),
        ] {
            let verts = vec![
                first[0], first[1], first[2], second[0], second[1], second[2],
            ];
            let tris = vec![[0u32, 1, 2], [3u32, 4, 5]];
            let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();

            let c = classify_pair(&soup, 0, 1);
            assert!(
                matches!(c, PairClassification::Transversal { .. }),
                "{name}: anchor pair must classify Transversal, got {c:?}"
            );
            let classified = vec![((0u32, 1u32), c)];

            let (points, _buckets) = group_intersection_points(&soup, &classified);

            // GEOMETRIC identity: the interned set must contain exactly 2
            // distinct exact-coordinate groups (one id per geometric point).
            let mut exact: Vec<[RBig; 3]> =
                points.iter().map(|tp| exact_coords(&tp.coords)).collect();
            exact.sort();
            exact.dedup();
            assert_eq!(
                exact.len(),
                2,
                "{name}: the pair has exactly 2 geometric intersection points"
            );
            assert_eq!(
                points.len(),
                2,
                "{name}: one interned id per GEOMETRIC point (got {} ids for 2 \
                 geometric points — structural over-count)",
                points.len()
            );

            // Both expected endpoints present (exact).
            let expect = |x: f64, y: f64, z: f64| [to_r(x), to_r(y), to_r(z)];
            for e in [expect(2.0, 2.0, 0.0), expect(2.0, 5.0, 0.0)] {
                assert!(
                    exact.iter().any(|xc| *xc == e),
                    "{name}: expected geometric endpoint {e:?} missing"
                );
            }

            // The constraint segment must be recorded for BOTH triangles
            // (the silent `ids.len() != 2` drop is the bug under test).
            let segs = group_constraint_segments(&soup, &classified, &points);
            assert_eq!(
                segs[0].len(),
                1,
                "{name}: triangle 0 must record the pair's constraint segment"
            );
            assert_eq!(
                segs[1].len(),
                1,
                "{name}: triangle 1 must record the pair's constraint segment"
            );
            let s = &segs[0][0];
            assert_ne!(s.endpoints.0, s.endpoints.1, "{name}: real segment");
            let e0 = exact_coords(&points[s.endpoints.0 as usize].coords);
            let e1 = exact_coords(&points[s.endpoints.1 as usize].coords);
            assert_ne!(
                e0, e1,
                "{name}: segment endpoints must be geometrically distinct"
            );
        }
    }
}
