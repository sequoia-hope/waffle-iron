use cad_primitives::Point3;

// =========================================================================
// Public types
// =========================================================================

/// Reference projection plane for the triangles in a `FastTrimesh`.
///
/// Stored from the constructor; consumed by PR-CR12+ 2D-orientation queries
/// (`tri_orientation`, `tri_verts_are_ccw`). PR-CR11 only stores it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Plane {
    XY,
    YZ,
    ZX,
}

/// Bulk-load error returned by [`FastTrimesh::from_soup`].
///
/// All variants describe caller-supplied data errors. Out-of-range query
/// indices are programmer bugs, not data errors — they trip `debug_assert!`
/// in debug builds.
#[derive(Debug, PartialEq, Eq)]
pub enum FastTrimeshError {
    /// `tris[tri][slot] = vid` but `vid >= verts.len()`.
    VertexIndexOutOfRange {
        tri: u32,
        slot: u8,
        vid: u32,
        n_verts: u32,
    },
    /// `tris[tri]` has two equal vertex indices.
    DegenerateTriangle { tri: u32, vids: [u32; 3] },
    /// `verts.len() > u32::MAX`.
    TooManyVertices { count: usize },
    /// `tris.len() > u32::MAX`.
    TooManyTriangles { count: usize },
}

/// Adjacency-aware triangle soup for mesh arrangement.
///
/// Build via [`FastTrimesh::from_soup`]; query via the methods on this
/// struct. Immutable after construction in PR-CR11 (mutators land in
/// PR-CR12).
#[derive(Debug)]
pub struct FastTrimesh {
    vertices: Vec<Vertex>,
    edges: Vec<Edge>,
    triangles: Vec<Triangle>,
    /// Vertex → incident edges.
    v2e: Vec<Vec<u32>>,
    /// Edge → incident triangles. May exceed length 2 on non-manifold edges.
    e2t: Vec<Vec<u32>>,
    plane: Plane,
}

// =========================================================================
// Internal storage types
// =========================================================================

#[derive(Copy, Clone, Debug)]
pub(crate) struct Vertex {
    point: Point3,
    info: u32,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Edge {
    v0: u32,
    v1: u32,
    constr: bool,
    visited: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Triangle {
    v: [u32; 3],
    info: u32,
}

// =========================================================================
// Implementation (RED stubs — see PR-CR11 GREEN commit)
// =========================================================================

impl FastTrimesh {
    /// Build a `FastTrimesh` from raw vertex + triangle arrays.
    ///
    /// Validates input, derives the sorted-unique edge list, and builds
    /// V→E and E→T adjacency. See spec at
    /// `specs/cherchi_rs_fast_trimesh_mvp.md`.
    pub fn from_soup(
        _verts: &[Point3],
        _tris: &[[u32; 3]],
        plane: Plane,
    ) -> Result<Self, FastTrimeshError> {
        // RED stub: returns empty mesh. GREEN will validate + build.
        Ok(Self {
            vertices: Vec::new(),
            edges: Vec::new(),
            triangles: Vec::new(),
            v2e: Vec::new(),
            e2t: Vec::new(),
            plane,
        })
    }

    // ----- Counts -----

    pub fn num_verts(&self) -> u32 {
        self.vertices.len() as u32
    }

    pub fn num_edges(&self) -> u32 {
        self.edges.len() as u32
    }

    pub fn num_tris(&self) -> u32 {
        self.triangles.len() as u32
    }

    pub fn ref_plane(&self) -> Plane {
        self.plane
    }

    // ----- Vertex queries -----

    pub fn vert(&self, _v: u32) -> Point3 {
        Point3::new(0.0, 0.0, 0.0)
    }

    pub fn vert_info(&self, _v: u32) -> u32 {
        0
    }

    pub fn vert_valence(&self, _v: u32) -> u32 {
        0
    }

    pub fn adj_v2e(&self, _v: u32) -> &[u32] {
        &[]
    }

    // ----- Edge queries -----

    pub fn edge(&self, _e: u32) -> (u32, u32) {
        (0, 0)
    }

    pub fn edge_vert_id(&self, _e: u32, _off: u32) -> u32 {
        0
    }

    pub fn edge_id(&self, _u: u32, _v: u32) -> Option<u32> {
        None
    }

    pub fn edge_is_constr(&self, _e: u32) -> bool {
        false
    }

    pub fn edge_is_boundary(&self, _e: u32) -> bool {
        false
    }

    pub fn edge_is_manifold(&self, _e: u32) -> bool {
        false
    }

    pub fn adj_e2t(&self, _e: u32) -> &[u32] {
        &[]
    }

    // ----- Triangle queries -----

    pub fn tri(&self, _t: u32) -> [u32; 3] {
        [0, 0, 0]
    }

    pub fn tri_vert_id(&self, _t: u32, _off: u32) -> u32 {
        0
    }

    pub fn tri_vert(&self, _t: u32, _off: u32) -> Point3 {
        Point3::new(0.0, 0.0, 0.0)
    }

    pub fn tri_vert_offset(&self, _t: u32, _v: u32) -> Option<u32> {
        None
    }

    pub fn tri_contains_vert(&self, _t: u32, _v: u32) -> bool {
        false
    }

    pub fn tri_edges(&self, _t: u32) -> [u32; 3] {
        [0, 0, 0]
    }

    pub fn tri_info(&self, _t: u32) -> u32 {
        0
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // Fixtures
    // -----------------------------------------------------------------

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// Single triangle in the XY plane.
    fn single_tri() -> (Vec<Point3>, Vec<[u32; 3]>) {
        (
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
            vec![[0, 1, 2]],
        )
    }

    /// Two-tri quad: a unit square in the XY plane, diagonal 0→2.
    fn two_tri_quad() -> (Vec<Point3>, Vec<[u32; 3]>) {
        (
            vec![
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(1.0, 1.0, 0.0),
                p(0.0, 1.0, 0.0),
            ],
            vec![[0, 1, 2], [0, 2, 3]],
        )
    }

    /// Closed tetrahedron: 4 verts, 6 edges, 4 tris.
    fn tetrahedron() -> (Vec<Point3>, Vec<[u32; 3]>) {
        (
            vec![
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
                p(0.0, 0.0, 1.0),
            ],
            // 4 outward-normal triangles
            vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        )
    }

    /// Closed icosahedron: 12 verts, 30 edges, 20 tris.
    /// Vertices on golden-ratio scaled cuboctahedron.
    fn icosahedron() -> (Vec<Point3>, Vec<[u32; 3]>) {
        let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
        let verts = vec![
            p(-1.0, phi, 0.0),  // 0
            p(1.0, phi, 0.0),   // 1
            p(-1.0, -phi, 0.0), // 2
            p(1.0, -phi, 0.0),  // 3
            p(0.0, -1.0, phi),  // 4
            p(0.0, 1.0, phi),   // 5
            p(0.0, -1.0, -phi), // 6
            p(0.0, 1.0, -phi),  // 7
            p(phi, 0.0, -1.0),  // 8
            p(phi, 0.0, 1.0),   // 9
            p(-phi, 0.0, -1.0), // 10
            p(-phi, 0.0, 1.0),  // 11
        ];
        let tris = vec![
            [0, 11, 5],
            [0, 5, 1],
            [0, 1, 7],
            [0, 7, 10],
            [0, 10, 11],
            [1, 5, 9],
            [5, 11, 4],
            [11, 10, 2],
            [10, 7, 6],
            [7, 1, 8],
            [3, 9, 4],
            [3, 4, 2],
            [3, 2, 6],
            [3, 6, 8],
            [3, 8, 9],
            [4, 9, 5],
            [2, 4, 11],
            [6, 2, 10],
            [8, 6, 7],
            [9, 8, 1],
        ];
        (verts, tris)
    }

    /// Non-manifold: 3 triangles sharing edge (0, 1).
    fn non_manifold_3_tris() -> (Vec<Point3>, Vec<[u32; 3]>) {
        (
            vec![
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
                p(0.0, -1.0, 0.0),
                p(0.0, 0.0, 1.0),
            ],
            vec![[0, 1, 2], [0, 1, 3], [0, 1, 4]],
        )
    }

    // -----------------------------------------------------------------
    // Group 1: Construction & basic counts
    // -----------------------------------------------------------------

    #[test]
    fn empty_input() {
        let ft = FastTrimesh::from_soup(&[], &[], Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 0);
        assert_eq!(ft.num_edges(), 0);
        assert_eq!(ft.num_tris(), 0);
        assert_eq!(ft.ref_plane(), Plane::XY);
    }

    #[test]
    fn single_triangle_counts() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 3);
        assert_eq!(ft.num_edges(), 3);
        assert_eq!(ft.num_tris(), 1);
    }

    #[test]
    fn two_tri_quad_counts() {
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 4);
        assert_eq!(ft.num_edges(), 5);
        assert_eq!(ft.num_tris(), 2);
    }

    #[test]
    fn tetrahedron_counts() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 4);
        assert_eq!(ft.num_edges(), 6);
        assert_eq!(ft.num_tris(), 4);
    }

    #[test]
    fn icosahedron_counts() {
        let (v, t) = icosahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 12);
        assert_eq!(ft.num_edges(), 30);
        assert_eq!(ft.num_tris(), 20);
    }

    #[test]
    fn ref_plane_is_stored() {
        let ft = FastTrimesh::from_soup(&[], &[], Plane::YZ).unwrap();
        assert_eq!(ft.ref_plane(), Plane::YZ);
        let ft = FastTrimesh::from_soup(&[], &[], Plane::ZX).unwrap();
        assert_eq!(ft.ref_plane(), Plane::ZX);
    }

    // -----------------------------------------------------------------
    // Group 2: Vertex / triangle accessors
    // -----------------------------------------------------------------

    #[test]
    fn vert_returns_input_point() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.vert(0), v[0]);
        assert_eq!(ft.vert(1), v[1]);
        assert_eq!(ft.vert(2), v[2]);
    }

    #[test]
    fn tri_returns_input_triple_in_order() {
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.tri(0), [0, 1, 2]);
        assert_eq!(ft.tri(1), [0, 2, 3]);
    }

    #[test]
    fn tri_vert_id_indirects_through_tri() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for off in 0..3 {
            assert_eq!(ft.tri_vert_id(0, off), t[0][off as usize]);
        }
    }

    #[test]
    fn tri_vert_returns_point() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for off in 0..3 {
            assert_eq!(ft.tri_vert(0, off), v[t[0][off as usize] as usize]);
        }
    }

    #[test]
    fn tri_contains_vert_matrix() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Tri 0 = [0, 2, 1]
        assert!(ft.tri_contains_vert(0, 0));
        assert!(ft.tri_contains_vert(0, 1));
        assert!(ft.tri_contains_vert(0, 2));
        assert!(!ft.tri_contains_vert(0, 3));
    }

    #[test]
    fn tri_vert_offset_returns_offset_or_none() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Tri 0 = [0, 2, 1]
        assert_eq!(ft.tri_vert_offset(0, 0), Some(0));
        assert_eq!(ft.tri_vert_offset(0, 2), Some(1));
        assert_eq!(ft.tri_vert_offset(0, 1), Some(2));
        assert_eq!(ft.tri_vert_offset(0, 3), None);
    }

    #[test]
    fn vert_info_is_zero_in_pr_cr11() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for i in 0..ft.num_verts() {
            assert_eq!(ft.vert_info(i), 0);
        }
    }

    #[test]
    fn tri_info_is_zero_in_pr_cr11() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for i in 0..ft.num_tris() {
            assert_eq!(ft.tri_info(i), 0);
        }
    }

    // -----------------------------------------------------------------
    // Group 3: Edge derivation correctness
    // -----------------------------------------------------------------

    #[test]
    fn single_tri_edges_sorted() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_edges(), 3);
        for e in 0..ft.num_edges() {
            let (v0, v1) = ft.edge(e);
            assert!(v0 < v1, "edge {e} = ({v0}, {v1}) not sorted");
        }
    }

    #[test]
    fn tetrahedron_has_six_edges() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        assert_eq!(ft.num_edges(), 6);
    }

    #[test]
    fn two_tri_quad_shares_one_edge() {
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // 2 tris × 3 edges = 6 incidences. 5 unique edges means 1 shared.
        assert_eq!(ft.num_edges(), 5);
    }

    #[test]
    fn edge_id_is_argument_order_independent() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for u in 0..ft.num_verts() {
            for w in 0..ft.num_verts() {
                if u != w {
                    assert_eq!(
                        ft.edge_id(u, w),
                        ft.edge_id(w, u),
                        "edge_id({u}, {w}) != edge_id({w}, {u})"
                    );
                }
            }
        }
    }

    #[test]
    fn edge_id_returns_none_for_missing_edge() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // No vertex 99
        assert_eq!(ft.edge_id(0, 99), None);
    }

    #[test]
    fn edge_vert_id_matches_edge() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for e in 0..ft.num_edges() {
            let (a, b) = ft.edge(e);
            assert_eq!(ft.edge_vert_id(e, 0), a);
            assert_eq!(ft.edge_vert_id(e, 1), b);
        }
    }

    #[test]
    fn edge_is_constr_false_in_pr_cr11() {
        let (v, t) = single_tri();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for e in 0..ft.num_edges() {
            assert!(!ft.edge_is_constr(e));
        }
    }

    // -----------------------------------------------------------------
    // Group 4: Adjacency correctness
    // -----------------------------------------------------------------

    #[test]
    fn tetrahedron_vertex_valences_all_three() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for i in 0..ft.num_verts() {
            assert_eq!(ft.vert_valence(i), 3, "vertex {i}");
        }
    }

    #[test]
    fn tetrahedron_every_edge_has_two_tris() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for e in 0..ft.num_edges() {
            assert_eq!(ft.adj_e2t(e).len(), 2, "edge {e}");
            assert!(ft.edge_is_manifold(e));
            assert!(!ft.edge_is_boundary(e));
        }
    }

    #[test]
    fn icosahedron_every_edge_has_two_tris() {
        let (v, t) = icosahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for e in 0..ft.num_edges() {
            assert_eq!(ft.adj_e2t(e).len(), 2);
            assert!(ft.edge_is_manifold(e));
        }
    }

    #[test]
    fn two_tri_quad_diagonal_is_shared() {
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Diagonal is edge (0, 2)
        let e = ft.edge_id(0, 2).expect("diagonal edge");
        assert_eq!(ft.adj_e2t(e).len(), 2);
        assert!(!ft.edge_is_boundary(e));
    }

    #[test]
    fn two_tri_quad_boundary_has_one_tri() {
        let (v, t) = two_tri_quad();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Edge (0, 1) is boundary
        let e = ft.edge_id(0, 1).expect("boundary edge");
        assert_eq!(ft.adj_e2t(e).len(), 1);
        assert!(ft.edge_is_boundary(e));
    }

    #[test]
    fn non_manifold_edge_detected() {
        let (v, t) = non_manifold_3_tris();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        // Edge (0, 1) is shared by 3 tris
        let e = ft.edge_id(0, 1).expect("non-manifold edge");
        assert_eq!(ft.adj_e2t(e).len(), 3);
        assert!(!ft.edge_is_manifold(e));
    }

    #[test]
    fn valence_sum_equals_twice_num_edges() {
        let (v, t) = icosahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let total_valence: u32 = (0..ft.num_verts()).map(|i| ft.vert_valence(i)).sum();
        assert_eq!(total_valence, 2 * ft.num_edges());
    }

    #[test]
    fn e2t_sum_equals_thrice_num_tris() {
        let (v, t) = icosahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        let total_incidences: usize = (0..ft.num_edges())
            .map(|e| ft.adj_e2t(e).len())
            .sum();
        assert_eq!(total_incidences, 3 * ft.num_tris() as usize);
    }

    #[test]
    fn tri_edges_reference_their_tri() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for ti in 0..ft.num_tris() {
            let es = ft.tri_edges(ti);
            for &e in &es {
                assert!(
                    ft.adj_e2t(e).contains(&ti),
                    "tri {ti} edge {e} doesn't list it back"
                );
            }
        }
    }

    #[test]
    fn adj_v2e_contains_only_incident_edges() {
        let (v, t) = tetrahedron();
        let ft = FastTrimesh::from_soup(&v, &t, Plane::XY).unwrap();
        for vi in 0..ft.num_verts() {
            for &e in ft.adj_v2e(vi) {
                let (a, b) = ft.edge(e);
                assert!(a == vi || b == vi, "edge {e} listed in v2e[{vi}] but doesn't touch it");
            }
        }
    }

    // -----------------------------------------------------------------
    // Group 5: Error / edge cases
    // -----------------------------------------------------------------

    #[test]
    fn out_of_range_vertex_index_is_err() {
        let verts = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        // Triangle references vertex 99 (doesn't exist)
        let tris = vec![[0u32, 1, 99]];
        let err = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap_err();
        assert_eq!(
            err,
            FastTrimeshError::VertexIndexOutOfRange {
                tri: 0,
                slot: 2,
                vid: 99,
                n_verts: 3,
            }
        );
    }

    #[test]
    fn degenerate_triangle_is_err() {
        let verts = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        // Triangle [0, 1, 0] has a repeated vertex
        let tris = vec![[0u32, 1, 0]];
        let err = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap_err();
        assert_eq!(
            err,
            FastTrimeshError::DegenerateTriangle {
                tri: 0,
                vids: [0, 1, 0],
            }
        );
    }

    #[test]
    fn empty_tris_with_isolated_verts_is_ok() {
        let verts = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0)];
        let ft = FastTrimesh::from_soup(&verts, &[], Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 2);
        assert_eq!(ft.num_edges(), 0);
        assert_eq!(ft.num_tris(), 0);
        assert_eq!(ft.vert_valence(0), 0);
        assert_eq!(ft.vert_valence(1), 0);
    }

    #[test]
    fn isolated_vertex_alongside_triangle_is_ok() {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(5.0, 5.0, 5.0), // isolated
        ];
        let tris = vec![[0u32, 1, 2]];
        let ft = FastTrimesh::from_soup(&verts, &tris, Plane::XY).unwrap();
        assert_eq!(ft.num_verts(), 4);
        assert_eq!(ft.vert_valence(3), 0);
    }
}
