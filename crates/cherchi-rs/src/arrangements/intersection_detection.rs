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
pub fn detect_intersecting_pairs(_soup: &FastTrimesh) -> Vec<(u32, u32)> {
    // RED stub
    Vec::new()
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
    fn aabb_overlap_disjoint_geometry() {
        // Two triangles in z=0 plane whose AABBs overlap but the
        // triangles themselves don't share area. Proves AABB doesn't
        // false-positive into the output.
        let verts = vec![
            // Tri 0: lower-left triangle in [0,2]×[0,2]
            p(0.0, 0.0, 0.0),
            p(2.0, 0.0, 0.0),
            p(0.0, 2.0, 0.0),
            // Tri 1: upper-right corner of [0,2]×[0,2] — AABB overlaps
            // tri 0's AABB but the actual triangles share no area.
            p(2.0, 2.0, 0.0),
            p(1.5, 2.0, 0.0),
            p(2.0, 1.5, 0.0),
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
        assert!(!pairs.is_empty(), "two overlapping cubes should produce pairs");
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

    fn assert_pruned_matches_brute(soup: &FastTrimesh) {
        let pruned = detect_intersecting_pairs(soup);
        let brute = brute_force_pairs(soup);
        let mut pruned_sorted = pruned.clone();
        pruned_sorted.sort();
        let mut brute_sorted = brute.clone();
        brute_sorted.sort();
        assert_eq!(
            pruned_sorted, brute_sorted,
            "AABB-pruned result diverged from brute-force"
        );
    }

    #[test]
    fn property_pruned_matches_brute_force() {
        // Tetrahedron — every face pair intersects.
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let tetra = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_pruned_matches_brute(&tetra);

        // Two cubes offset
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
        assert_pruned_matches_brute(&two_cubes);

        // Empty mesh
        let empty = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        assert_pruned_matches_brute(&empty);

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
        assert_pruned_matches_brute(&disjoint);

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
        assert_pruned_matches_brute(&mixed);
    }
}
