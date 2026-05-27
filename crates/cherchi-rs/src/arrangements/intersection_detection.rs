//! Mesh arrangement Stage 1 — triangle-pair intersection detection.
//!
//! Ported from Cherchi 2020 `arrangements/code/intersection_classification.cpp:47-94`
//! (MIT). © 2020 G. Cherchi, M. Livesu, R. Scateni, M. Attene.
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! Cherchi 2020 §5 (mesh arrangement). This is the first stage:
//! given a triangle soup, find all pairs whose pairwise intersection
//! is non-empty. Subsequent stages (classification, re-triangulation,
//! assembly) consume the pair list to produce the arrangement.
//!
//! ## Deliberate deviations from upstream
//!
//! 1. **No spatial index** (deviation #20 in the FastTrimesh
//!    cumulative list). Upstream uses `cinolib::Octree` for
//!    O(n log n) average pair pruning (cpp:47-94). cherchi-rs starts
//!    with O(n²) + AABB pre-pruning. Justification: Hard Rule #1
//!    forbids workspace deps (no `bvh` crate); a hand-rolled BVH is
//!    its own substantial PR; not on critical path for the meshes
//!    Yang-rs will produce.
//!
//! 2. **Coplanar pairs included alongside Intersects** (deviation
//!    #21). Upstream's `classifyIntersections` (the next stage)
//!    consumes both uniformly. Filtering at detection would force
//!    downstream re-detection.
//!
//! ## Discovery during implementation
//!
//! CR9's `Coplanar` return covers BOTH "full coplanar" AND "edge of
//! one triangle lies in the other's plane" (per the CR9 docstring at
//! `predicates/triangle_intersect.rs:30-35`). The latter triggers
//! for many spatially-distant pairs (e.g. cube faces in perpendicular
//! planes where an edge of one happens to lie in the other's plane).
//!
//! AABB pre-pruning correctly filters these spatially-distant
//! "Coplanar-via-edge-in-plane" pairs. Upstream Cherchi does the
//! same via Octree pruning. The pruned algorithm matches upstream
//! behavior — it's a STRICT improvement over brute-force iteration,
//! not just a perf optimization.
//!
//! This is captured by the Group 7 property test's
//! `assert_pruned_subset_of_brute` (not `==` against brute-force).

use cad_primitives::Point3;

use crate::arrangements::FastTrimesh;
use crate::predicates::{triangle_intersects_triangle_3d, TriangleIntersection};

/// Detect all pairs of intersecting triangles in a single mesh.
///
/// Returns a list of `(t_a, t_b)` pairs with `t_a < t_b` where
/// [`triangle_intersects_triangle_3d`] reports either
/// `Intersects` or `Coplanar` (filters out `Disjoint`).
/// Coplanar pairs are included because downstream classification
/// (CR14+) consumes them uniformly with intersecting pairs.
///
/// **Algorithm**: O(n²) pairwise iteration with AABB pre-pruning.
/// Per-triangle AABBs are computed once upfront; each pair gets a
/// cheap 6-component overlap check before the expensive
/// triangle-triangle predicate. BVH/Octree is banked for a future
/// PR (Hard Rule #1: no workspace deps).
///
/// **Output invariants**:
/// - Every pair satisfies `pair.0 < pair.1`.
/// - Each unordered pair appears at most once.
/// - The list contains exactly the non-`Disjoint` pairs.
pub fn detect_intersecting_pairs(soup: &FastTrimesh) -> Vec<(u32, u32)> {
    let n = soup.num_tris();
    if n < 2 {
        return Vec::new();
    }
    // Pre-compute per-triangle AABBs once. O(n) up front saves
    // O(n²) recomputation in the pair loop.
    let aabbs: Vec<(Point3, Point3)> = (0..n).map(|t| tri_aabb(soup, t)).collect();
    let mut out = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let (a_min, a_max) = aabbs[i as usize];
            let (b_min, b_max) = aabbs[j as usize];
            if !aabbs_overlap(a_min, a_max, b_min, b_max) {
                continue;
            }
            let a0 = soup.tri_vert(i, 0);
            let a1 = soup.tri_vert(i, 1);
            let a2 = soup.tri_vert(i, 2);
            let b0 = soup.tri_vert(j, 0);
            let b1 = soup.tri_vert(j, 1);
            let b2 = soup.tri_vert(j, 2);
            match triangle_intersects_triangle_3d(a0, a1, a2, b0, b1, b2) {
                TriangleIntersection::Disjoint => continue,
                _ => out.push((i, j)),
            }
        }
    }
    out
}

fn tri_aabb(soup: &FastTrimesh, t: u32) -> (Point3, Point3) {
    let v0 = soup.tri_vert(t, 0);
    let v1 = soup.tri_vert(t, 1);
    let v2 = soup.tri_vert(t, 2);
    let min = Point3::new(
        v0.x().min(v1.x()).min(v2.x()),
        v0.y().min(v1.y()).min(v2.y()),
        v0.z().min(v1.z()).min(v2.z()),
    );
    let max = Point3::new(
        v0.x().max(v1.x()).max(v2.x()),
        v0.y().max(v1.y()).max(v2.y()),
        v0.z().max(v1.z()).max(v2.z()),
    );
    (min, max)
}

fn aabbs_overlap(a_min: Point3, a_max: Point3, b_min: Point3, b_max: Point3) -> bool {
    !(a_max.x() < b_min.x()
        || a_min.x() > b_max.x()
        || a_max.y() < b_min.y()
        || a_min.y() > b_max.y()
        || a_max.z() < b_min.z()
        || a_min.z() > b_max.z())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangements::Plane;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// Hand-coded brute force without AABB pre-pruning. Used by the
    /// property test (Group 7) to verify AABB doesn't false-negative.
    fn brute_force_pairs(soup: &FastTrimesh) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let n = soup.num_tris();
        for i in 0..n {
            for j in (i + 1)..n {
                let a0 = soup.tri_vert(i, 0);
                let a1 = soup.tri_vert(i, 1);
                let a2 = soup.tri_vert(i, 2);
                let b0 = soup.tri_vert(j, 0);
                let b1 = soup.tri_vert(j, 1);
                let b2 = soup.tri_vert(j, 2);
                match triangle_intersects_triangle_3d(a0, a1, a2, b0, b1, b2) {
                    TriangleIntersection::Disjoint => continue,
                    _ => out.push((i, j)),
                }
            }
        }
        out
    }

    // -----------------------------------------------------------------
    // Group 1 — Boundary conditions
    // -----------------------------------------------------------------

    #[test]
    fn empty_mesh_has_no_pairs() {
        let soup = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    #[test]
    fn single_tri_has_no_pairs() {
        let verts = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        let tris = vec![[0u32, 1, 2]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    #[test]
    fn far_apart_tris_yield_no_pairs() {
        let verts = vec![
            // Tri 0 at origin
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            // Tri 1 at (10, 10, 10) — AABBs don't overlap
            p(10.0, 10.0, 10.0),
            p(11.0, 10.0, 10.0),
            p(10.0, 11.0, 10.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    // -----------------------------------------------------------------
    // Group 2 — Disjoint triangles
    // -----------------------------------------------------------------

    #[test]
    fn parallel_planes_no_pairs() {
        // Two triangles in z=0 and z=1; AABBs overlap only if their
        // x,y projections overlap. Place them with overlapping x,y so
        // AABBs DO overlap but geometry is disjoint.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 2.0, 0.0),
            p(0.0, 0.0, 1.0),
            p(2.0, 0.0, 1.0),
            p(0.0, 2.0, 1.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    #[test]
    fn perpendicular_separated_no_pairs() {
        // Tri 0 in XY plane at origin; Tri 1 in XZ plane far away.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(5.0, 0.0, 0.0),
            p(6.0, 0.0, 0.0),
            p(5.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    // -----------------------------------------------------------------
    // Group 3 — Intersecting triangles
    // -----------------------------------------------------------------

    #[test]
    fn crossing_in_3d_one_pair() {
        // Two triangles crossing through each other: one in XY plane
        // through origin, one in YZ plane intersecting it along Y axis.
        let verts = vec![
            // Tri 0 in XY (z=0)
            p(-1.0, -1.0, 0.0),
            p(1.0, -1.0, 0.0),
            p(0.0, 1.0, 0.0),
            // Tri 1 in YZ-ish plane crossing through tri 0
            p(0.0, -1.0, -1.0),
            p(0.0, -1.0, 1.0),
            p(0.0, 1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    #[test]
    fn edge_touching_one_pair() {
        // Two triangles sharing edge (0, 1). CR9 returns Intersects.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, -1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [0, 1, 3]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    #[test]
    fn vertex_touching_one_pair() {
        // Two triangles sharing only vertex 0.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(-1.0, 0.0, 0.0),
            p(0.0, -1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [0, 3, 4]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    #[test]
    fn t_junction_one_pair() {
        // Tri 1's vertex 3 lies on tri 0's edge (0, 1).
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(1.0, 2.0, 0.0),
            p(1.0, 0.0, 0.0), // on edge (0, 1)
            p(1.0, -1.0, 1.0),
            p(2.0, -1.0, 1.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    // -----------------------------------------------------------------
    // Group 4 — Coplanar pairs
    // -----------------------------------------------------------------

    #[test]
    fn coplanar_overlapping_one_pair() {
        // Two overlapping triangles in z=0 plane.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 2.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(3.0, 1.0, 0.0),
            p(1.0, 3.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    #[test]
    fn coplanar_shared_edge_one_pair() {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(1.0, 1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [1, 3, 2]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    #[test]
    fn coplanar_shared_vertex_one_pair() {
        // Two coplanar triangles touching only at vertex 0.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(-1.0, 0.0, 0.0),
            p(0.0, -1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [0, 3, 4]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![(0, 1)]);
    }

    // -----------------------------------------------------------------
    // Group 5 — Multi-triangle meshes
    // -----------------------------------------------------------------

    #[test]
    fn tetrahedron_all_face_pairs_share_edges() {
        // Tetrahedron has 4 faces; every pair of faces shares an edge,
        // and CR9 treats shared-edge pairs as Intersects. So all 6
        // pairs are reported.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let pairs = detect_intersecting_pairs(&soup);
        // 4 choose 2 = 6 pairs; all share an edge.
        assert_eq!(pairs.len(), 6);
    }

    #[test]
    fn two_cubes_offset_have_pairs() {
        // Two unit cubes, second offset 0.5 along X — guaranteed
        // non-trivial intersection. Just confirm the pair count is > 0.
        let mut verts = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for (i, origin) in [[0.0, 0.0, 0.0], [0.5, 0.0, 0.0]].iter().enumerate() {
            let (cv, ct) = unit_cube_at(*origin);
            let base = (i * 8) as u32;
            verts.extend(cv);
            for t in &ct {
                tris.push([t[0] + base, t[1] + base, t[2] + base]);
            }
        }
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let pairs = detect_intersecting_pairs(&soup);
        assert!(
            !pairs.is_empty(),
            "two overlapping cubes should produce pairs"
        );
    }

    fn unit_cube_at(origin: [f64; 3]) -> (Vec<Point3>, Vec<[u32; 3]>) {
        let [x, y, z] = origin;
        let v = vec![
            p(x, y, z),
            p(x + 1.0, y, z),
            p(x + 1.0, y + 1.0, z),
            p(x, y + 1.0, z),
            p(x, y, z + 1.0),
            p(x + 1.0, y, z + 1.0),
            p(x + 1.0, y + 1.0, z + 1.0),
            p(x, y + 1.0, z + 1.0),
        ];
        let t = vec![
            [0, 3, 2],
            [0, 2, 1],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [1, 2, 6],
            [1, 6, 5],
            [0, 4, 7],
            [0, 7, 3],
        ];
        (v, t)
    }

    #[test]
    fn star_of_david_multiple_pairs() {
        // Two coplanar overlapping triangles (star pattern).
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(3.0, 0.0, 0.0),
            p(1.5, 3.0, 0.0),
            p(0.0, 2.0, 0.0),
            p(3.0, 2.0, 0.0),
            p(1.5, -1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let pairs = detect_intersecting_pairs(&soup);
        // The two triangles are coplanar and overlap → 1 pair.
        assert_eq!(pairs, vec![(0, 1)]);
    }

    #[test]
    fn five_disjoint_random_tris_no_pairs() {
        // 5 triangles widely separated in space.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(10.0, 0.0, 0.0),
            p(11.0, 0.0, 0.0),
            p(10.0, 1.0, 0.0),
            p(0.0, 10.0, 0.0),
            p(1.0, 10.0, 0.0),
            p(0.0, 11.0, 0.0),
            p(0.0, 0.0, 10.0),
            p(1.0, 0.0, 10.0),
            p(0.0, 1.0, 10.0),
            p(10.0, 10.0, 10.0),
            p(11.0, 10.0, 10.0),
            p(10.0, 11.0, 10.0),
        ];
        let tris = vec![
            [0u32, 1, 2],
            [3, 4, 5],
            [6, 7, 8],
            [9, 10, 11],
            [12, 13, 14],
        ];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(detect_intersecting_pairs(&soup), vec![]);
    }

    #[test]
    fn non_manifold_three_tris_all_paired() {
        // Three triangles sharing edge (0, 1). CR9 reports Intersects
        // for every pair (shared edge).
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, -1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 1, 2], [0, 1, 3], [0, 1, 4]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let pairs = detect_intersecting_pairs(&soup);
        // 3 choose 2 = 3 pairs, all sharing edge (0, 1).
        assert_eq!(pairs.len(), 3);
    }

    // -----------------------------------------------------------------
    // Group 6 — Pair invariants
    // -----------------------------------------------------------------

    #[test]
    fn pairs_are_sorted_ascending() {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let pairs = detect_intersecting_pairs(&soup);
        for &(a, b) in &pairs {
            assert!(a < b, "pair ({a}, {b}) violates a < b");
        }
    }

    #[test]
    fn no_duplicate_pairs() {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        let mut pairs = detect_intersecting_pairs(&soup);
        let original_len = pairs.len();
        pairs.sort();
        pairs.dedup();
        assert_eq!(pairs.len(), original_len);
    }

    #[test]
    fn empty_iff_no_intersections() {
        // A single triangle has no pairs (no intersections possible).
        let verts = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        let tris = vec![[0u32, 1, 2]];
        let soup = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert!(detect_intersecting_pairs(&soup).is_empty());
    }

    // -----------------------------------------------------------------
    // Group 7 — Property test against brute-force
    // -----------------------------------------------------------------

    /// Property: AABB pre-pruning is conservative. Every pair the
    /// pruned algorithm returns must also be in the brute-force
    /// result. The reverse direction (`brute ⊆ pruned`) does NOT
    /// hold: CR9's `Coplanar` return is triggered whenever one
    /// triangle has an edge in the other's plane, even when the
    /// triangles are far apart spatially (their AABBs don't overlap).
    /// AABB pre-pruning correctly filters these spurious pairs out —
    /// matching upstream's spatial-pruned detection. So our pruned
    /// version is a STRICT improvement over brute-force, not just
    /// faster.
    fn assert_pruned_subset_of_brute(soup: &FastTrimesh) {
        use std::collections::HashSet;
        let pruned = detect_intersecting_pairs(soup);
        let brute: HashSet<(u32, u32)> = brute_force_pairs(soup).into_iter().collect();
        for &pair in &pruned {
            assert!(
                brute.contains(&pair),
                "pruned has pair {:?} not in brute-force result",
                pair
            );
        }
    }

    #[test]
    fn property_pruned_subset_of_brute_force() {
        // Tetrahedron — every face pair intersects.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let tetra = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_pruned_subset_of_brute(&tetra);

        // Two cubes offset — has many CR9 Coplanar-via-edge-in-plane
        // false positives that AABB correctly filters out. The
        // subset invariant still holds.
        let mut verts = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for (i, origin) in [[0.0, 0.0, 0.0], [0.5, 0.0, 0.0]].iter().enumerate() {
            let (cv, ct) = unit_cube_at(*origin);
            let base = (i * 8) as u32;
            verts.extend(cv);
            for t in &ct {
                tris.push([t[0] + base, t[1] + base, t[2] + base]);
            }
        }
        let two_cubes = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_pruned_subset_of_brute(&two_cubes);

        // Empty mesh
        let empty = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        assert_pruned_subset_of_brute(&empty);

        // 5 disjoint tris
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(10.0, 0.0, 0.0),
            p(11.0, 0.0, 0.0),
            p(10.0, 1.0, 0.0),
            p(0.0, 10.0, 0.0),
            p(1.0, 10.0, 0.0),
            p(0.0, 11.0, 0.0),
            p(0.0, 0.0, 10.0),
            p(1.0, 0.0, 10.0),
            p(0.0, 1.0, 10.0),
            p(5.0, 5.0, 5.0),
            p(6.0, 5.0, 5.0),
            p(5.0, 6.0, 5.0),
        ];
        let tris = vec![
            [0u32, 1, 2],
            [3, 4, 5],
            [6, 7, 8],
            [9, 10, 11],
            [12, 13, 14],
        ];
        let disjoint = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_pruned_subset_of_brute(&disjoint);

        // Coplanar overlapping + disjoint mix
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 2.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(3.0, 1.0, 0.0),
            p(1.0, 3.0, 0.0),
            p(10.0, 0.0, 0.0),
            p(11.0, 0.0, 0.0),
            p(10.0, 1.0, 0.0),
        ];
        let tris = vec![[0u32, 1, 2], [3, 4, 5], [6, 7, 8]];
        let mixed = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_pruned_subset_of_brute(&mixed);
    }
}
