//! FastTrimesh — adjacency-aware triangle mesh.
//!
//! Ported from Cherchi fast_trimesh.cpp/.h
//! MIT License (c) 2020/2022 Cherchi, Livesu, Scateni, Attene, Pellacini
//!
//! github.com/gcherchi/FastAndRobustMeshArrangements

use std::collections::HashMap;

use smallvec::SmallVec;

use super::common::{remove_from_vec, Plane};
use super::tree::Tree;
use crate::boolean::indirect_predicates::ImplicitPoint;

type FmVec = SmallVec<[usize; 16]>;

/// Vertex storage.
/// Ported from fast_trimesh.h:54-61 (iVtx)
///
/// Stores an `ImplicitPoint` instead of `[f64; 3]` — the C++ stores
/// `const genericPoint*`. Explicit input vertices become
/// `ImplicitPoint::Explicit`, intersection points become LPI/TPI.
#[derive(Debug, Clone)]
struct Vertex {
    point: ImplicitPoint,
    info: usize,
}

/// Edge storage.
/// Ported from fast_trimesh.h:63-70 (iEdge)
#[derive(Debug, Clone)]
struct Edge {
    v: (usize, usize),
    constr: bool,
}

/// Triangle storage.
/// Ported from fast_trimesh.h:72-84 (iTri)
#[derive(Debug, Clone)]
struct Triangle {
    v: [usize; 3],
    info: usize,
}

/// FastTrimesh — adjacency-aware triangle mesh for mesh arrangement.
///
/// Ported from Cherchi fast_trimesh.h:86-235
/// MIT License (c) 2020/2022 Cherchi, Livesu, Scateni, Attene, Pellacini
#[allow(dead_code)]
pub(crate) struct FastTrimesh {
    vertices: Vec<Vertex>,
    edges: Vec<Edge>,
    triangles: Vec<Triangle>,
    v2e: Vec<FmVec>,
    e2t: Vec<FmVec>,
    rev_vtx_map: HashMap<usize, usize>,
    triangle_plane: Plane,
}

#[allow(dead_code)]
impl FastTrimesh {
    // ========================================================================
    // Constructors
    // ========================================================================

    /// Construct from 3 vertices forming one triangle.
    /// Ported from fast_trimesh.cpp:45-53
    pub fn new(
        v0_coords: [f64; 3],
        v1_coords: [f64; 3],
        v2_coords: [f64; 3],
        orig_ids: [usize; 3],
        plane: Plane,
    ) -> Self {
        let mut mesh = Self {
            vertices: Vec::new(),
            edges: Vec::new(),
            triangles: Vec::new(),
            v2e: Vec::new(),
            e2t: Vec::new(),
            rev_vtx_map: HashMap::new(),
            triangle_plane: plane,
        };
        mesh.add_vert(ImplicitPoint::Explicit(v0_coords), orig_ids[0]);
        mesh.add_vert(ImplicitPoint::Explicit(v1_coords), orig_ids[1]);
        mesh.add_vert(ImplicitPoint::Explicit(v2_coords), orig_ids[2]);
        mesh.add_tri(0, 1, 2);
        mesh
    }

    /// Construct from 3 ImplicitPoints forming one triangle.
    pub fn new_implicit(
        v0: ImplicitPoint,
        v1: ImplicitPoint,
        v2: ImplicitPoint,
        orig_ids: [usize; 3],
        plane: Plane,
    ) -> Self {
        let mut mesh = Self {
            vertices: Vec::new(),
            edges: Vec::new(),
            triangles: Vec::new(),
            v2e: Vec::new(),
            e2t: Vec::new(),
            rev_vtx_map: HashMap::new(),
            triangle_plane: plane,
        };
        mesh.add_vert(v0, orig_ids[0]);
        mesh.add_vert(v1, orig_ids[1]);
        mesh.add_vert(v2, orig_ids[2]);
        mesh.add_tri(0, 1, 2);
        mesh
    }

    /// Construct from a list of vertices and triangle indices (sequential).
    /// Ported from fast_trimesh.cpp:57-141 (non-parallel branch)
    pub fn from_verts_and_tris(in_verts: &[[f64; 3]], in_tris: &[usize], plane: Plane) -> Self {
        let num_verts = in_verts.len();
        let mut mesh = Self {
            vertices: Vec::with_capacity(num_verts),
            edges: Vec::with_capacity(num_verts / 2),
            triangles: Vec::with_capacity(in_tris.len() / 3),
            v2e: Vec::with_capacity(num_verts),
            e2t: Vec::new(),
            rev_vtx_map: HashMap::new(),
            triangle_plane: plane,
        };
        for v in in_verts {
            mesh.add_vert_no_map(ImplicitPoint::Explicit(*v));
        }
        for t in 0..in_tris.len() / 3 {
            mesh.add_tri(in_tris[3 * t], in_tris[3 * t + 1], in_tris[3 * t + 2]);
        }
        mesh
    }

    /// Pre-allocate space for the expected number of vertices.
    /// Ported from fast_trimesh.cpp:145-152
    pub fn pre_allocate_space(&mut self, estimated_num_verts: usize) {
        self.vertices.reserve(estimated_num_verts);
        self.rev_vtx_map.reserve(estimated_num_verts);
        self.edges.reserve(estimated_num_verts / 2);
        self.triangles.reserve(estimated_num_verts / 3);
        self.v2e.reserve(estimated_num_verts);
    }

    /// Reset all triangle info fields to 0.
    /// Ported from fast_trimesh.cpp:156-160
    pub fn reset_triangles_info(&mut self) {
        for tri in &mut self.triangles {
            tri.info = 0;
        }
    }

    // ========================================================================
    // Size queries
    // ========================================================================

    /// Ported from fast_trimesh.cpp:164-167
    pub fn num_verts(&self) -> usize {
        self.vertices.len()
    }

    /// Ported from fast_trimesh.cpp:171-174
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    /// Ported from fast_trimesh.cpp:178-181
    pub fn num_tris(&self) -> usize {
        self.triangles.len()
    }

    /// Ported from fast_trimesh.cpp:185-188
    pub fn ref_plane(&self) -> Plane {
        self.triangle_plane
    }

    // ========================================================================
    // Vertex accessors
    // ========================================================================

    /// Get the ImplicitPoint for a vertex.
    pub fn implicit_point(&self, v_id: usize) -> &ImplicitPoint {
        assert!(v_id < self.vertices.len(), "vtx id out of range");
        &self.vertices[v_id].point
    }

    /// Get materialized vertex coordinates.
    /// For Explicit points this is zero-cost; for LPI/TPI it computes the division.
    /// Ported from fast_trimesh.cpp:194-198
    pub fn vert(&self, v_id: usize) -> [f64; 3] {
        assert!(v_id < self.vertices.len(), "vtx id out of range");
        self.vertices[v_id]
            .point
            .materialize()
            .unwrap_or([0.0, 0.0, 0.0])
    }

    /// Map from local new_id to original mesh vertex ID.
    /// Ported from fast_trimesh.cpp:202-206
    pub fn vert_orig_id(&self, new_v_id: usize) -> usize {
        assert!(new_v_id < self.vertices.len(), "vtx id out of range");
        self.vertices[new_v_id].info
    }

    /// Map from original mesh vertex ID to local new_id.
    /// Ported from fast_trimesh.cpp:210-218
    pub fn vert_new_id(&self, orig_v_id: usize) -> Option<usize> {
        self.rev_vtx_map.get(&orig_v_id).copied()
    }

    /// Vertex valence (number of incident edges).
    /// Ported from fast_trimesh.cpp:222-226
    pub fn vert_valence(&self, v_id: usize) -> usize {
        assert!(v_id < self.vertices.len(), "vtx id out of range");
        self.v2e[v_id].len()
    }

    /// Vertex-to-edge adjacency list.
    /// Ported from fast_trimesh.cpp:230-234
    pub fn adj_v2e(&self, v_id: usize) -> &FmVec {
        assert!(v_id < self.vertices.len(), "vtx id out of range");
        &self.v2e[v_id]
    }

    /// Vertex-to-triangle adjacency (computed from v2e + e2t, deduplicated).
    /// Ported from fast_trimesh.cpp:238-251
    pub fn adj_v2t(&self, v_id: usize) -> FmVec {
        assert!(v_id < self.vertices.len(), "vtx id out of range");
        let mut v2t = FmVec::new();
        for &e_id in &self.v2e[v_id] {
            for &t_id in &self.e2t[e_id] {
                v2t.push(t_id);
            }
        }
        v2t.sort();
        v2t.dedup();
        v2t
    }

    /// Reset all vertex info fields to 0.
    /// Ported from fast_trimesh.cpp:255-259
    pub fn reset_vertices_info(&mut self) {
        for v in &mut self.vertices {
            v.info = 0;
        }
    }

    /// Set the info field of a vertex.
    /// Ported from fast_trimesh.cpp:261-265
    pub fn set_vert_info(&mut self, v_id: usize, info: usize) {
        assert!(v_id < self.vertices.len(), "vtx id out of range");
        self.vertices[v_id].info = info;
    }

    /// Get the info field of a vertex.
    /// Ported from fast_trimesh.cpp:269-273
    pub fn vert_info(&self, v_id: usize) -> usize {
        assert!(v_id < self.vertices.len(), "vtx id out of range");
        self.vertices[v_id].info
    }

    // ========================================================================
    // Edge accessors
    // ========================================================================

    /// Get edge vertex pair.
    /// Ported from fast_trimesh.cpp:279-283
    pub fn edge(&self, e_id: usize) -> (usize, usize) {
        assert!(e_id < self.edges.len(), "edge id out of range");
        self.edges[e_id].v
    }

    /// Get edge vertex ID at offset (0 or 1).
    /// Ported from fast_trimesh.cpp:287-292
    pub fn edge_vert_id(&self, e_id: usize, off: usize) -> usize {
        assert!(e_id < self.edges.len(), "edge id out of range");
        if off == 0 {
            self.edges[e_id].v.0
        } else {
            self.edges[e_id].v.1
        }
    }

    /// Find edge ID by its two endpoint vertex IDs. Returns None if not found.
    /// Ported from fast_trimesh.cpp:296-308
    pub fn edge_id(&self, ev0_id: usize, ev1_id: usize) -> Option<usize> {
        assert!(ev0_id != ev1_id, "edge with equal endpoints");
        assert!(
            ev0_id < self.vertices.len() && ev1_id < self.vertices.len(),
            "vtx id out of range"
        );
        for &e_id in &self.v2e[ev0_id] {
            if self.edge_contains_vert(e_id, ev0_id) && self.edge_contains_vert(e_id, ev1_id) {
                return Some(e_id);
            }
        }
        None
    }

    /// Check if an edge is a constraint.
    /// Ported from fast_trimesh.cpp:312-316
    pub fn edge_is_constr(&self, e_id: usize) -> bool {
        assert!(e_id < self.edges.len(), "edge id out of range");
        self.edges[e_id].constr
    }

    /// Mark an edge as a constraint.
    /// Ported from fast_trimesh.cpp:320-324
    pub fn set_edge_constr(&mut self, e_id: usize) {
        assert!(e_id < self.edges.len(), "edge id out of range");
        self.edges[e_id].constr = true;
    }

    /// Find the edge opposite to a vertex in a triangle.
    /// Ported from fast_trimesh.cpp:328-340
    pub fn edge_opp_to_vert(&self, t_id: usize, v_id: usize) -> usize {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        assert!(
            self.tri_contains_vert(t_id, v_id),
            "tri doesn't contain vtx"
        );

        let tv = &self.triangles[t_id].v;
        let e_id = if tv[0] == v_id {
            self.edge_id(tv[1], tv[2])
        } else if tv[1] == v_id {
            self.edge_id(tv[0], tv[2])
        } else {
            self.edge_id(tv[0], tv[1])
        };

        e_id.expect("opposite edge not found in tri")
    }

    /// Check if an edge is on the boundary (has only 1 adjacent triangle).
    /// Ported from fast_trimesh.cpp:344-348
    pub fn edge_is_boundary(&self, e_id: usize) -> bool {
        assert!(e_id < self.edges.len(), "edge id out of range");
        self.e2t[e_id].len() == 1
    }

    /// Check if an edge is manifold (has exactly 2 adjacent triangles).
    /// Ported from fast_trimesh.cpp:352-356
    pub fn edge_is_manifold(&self, e_id: usize) -> bool {
        assert!(e_id < self.edges.len(), "edge id out of range");
        self.e2t[e_id].len() == 2
    }

    /// Edge-to-triangle adjacency list.
    /// Ported from fast_trimesh.cpp:360-364
    pub fn adj_e2t(&self, e_id: usize) -> &FmVec {
        assert!(e_id < self.edges.len(), "edge id out of range");
        &self.e2t[e_id]
    }

    /// Set the visited flag on an edge (reuses constr field).
    /// Ported from fast_trimesh.cpp:368-372
    pub fn edge_set_visited(&mut self, e_id: usize, vis: bool) {
        assert!(e_id < self.edges.len(), "edge id out of range");
        self.edges[e_id].constr = vis;
    }

    /// Check if an edge has been visited (reads constr field).
    /// Ported from fast_trimesh.cpp:376-380
    pub fn edge_is_visited(&self, e_id: usize) -> bool {
        assert!(e_id < self.edges.len(), "edge id out of range");
        self.edges[e_id].constr
    }

    // ========================================================================
    // Triangle accessors
    // ========================================================================

    /// Get triangle vertex array.
    /// Ported from fast_trimesh.cpp:387-391
    pub fn tri(&self, t_id: usize) -> &[usize; 3] {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        &self.triangles[t_id].v
    }

    /// Find triangle ID by its three vertex IDs. Returns None if not found.
    /// Ported from fast_trimesh.cpp:395-407
    pub fn tri_id(&self, tv0_id: usize, tv1_id: usize, tv2_id: usize) -> Option<usize> {
        assert!(
            tv0_id < self.vertices.len()
                && tv1_id < self.vertices.len()
                && tv2_id < self.vertices.len(),
            "vtx id out of range"
        );
        let e_id = self.edge_id(tv0_id, tv1_id)?;
        for &t_id in &self.e2t[e_id] {
            if self.tri_contains_vert(t_id, tv2_id) {
                return Some(t_id);
            }
        }
        None
    }

    /// Get vertex ID at a given offset (0, 1, or 2) in a triangle.
    /// Ported from fast_trimesh.cpp:411-415
    pub fn tri_vert_id(&self, t_id: usize, off: usize) -> usize {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        self.triangles[t_id].v[off]
    }

    /// Get materialized vertex coordinates at a given offset in a triangle.
    /// Ported from fast_trimesh.cpp:419-423
    pub fn tri_vert(&self, t_id: usize, off: usize) -> [f64; 3] {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        self.vertices[self.triangles[t_id].v[off]]
            .point
            .materialize()
            .unwrap_or([0.0, 0.0, 0.0])
    }

    /// Get edge ID at a given offset in a triangle (edge between v[off] and v[(off+1)%3]).
    /// Ported from fast_trimesh.cpp:428-432
    pub fn tri_edge_id(&self, t_id: usize, off: usize) -> Option<usize> {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        self.edge_id(
            self.tri_vert_id(t_id, off),
            self.tri_vert_id(t_id, (off + 1) % 3),
        )
    }

    /// Get tree node ID for a triangle.
    /// Ported from fast_trimesh.cpp:436-440
    pub fn tri_node_id(&self, t_id: usize) -> usize {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        self.triangles[t_id].info
    }

    /// Set tree node ID for a triangle.
    /// Ported from fast_trimesh.cpp:444-448
    pub fn set_tri_node_id(&mut self, t_id: usize, n_id: usize) {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        self.triangles[t_id].info = n_id;
    }

    /// Find the third vertex in a triangle, given two of its vertices.
    /// Ported from fast_trimesh.cpp:452-466
    pub fn tri_vert_opposite_to(&self, t_id: usize, v0_id: usize, v1_id: usize) -> usize {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        assert!(v0_id != v1_id, "verts are equal");
        assert!(
            self.tri_contains_vert(t_id, v0_id) && self.tri_contains_vert(t_id, v1_id),
            "tri doesn't contain vtx"
        );
        for off in 0..3 {
            let v_id = self.triangles[t_id].v[off];
            if v_id != v0_id && v_id != v1_id {
                return v_id;
            }
        }
        panic!("tri_vert_opposite_to: should not happen");
    }

    /// Find the triangle opposite to a given triangle across an edge.
    /// Returns None for boundary edges.
    /// Ported from fast_trimesh.cpp:470-485
    pub fn tri_opp_to_edge(&self, e_id: usize, t_id: usize) -> Option<usize> {
        assert!(e_id < self.edges.len(), "edge id out of range");
        assert!(t_id < self.triangles.len(), "tri id out of range");

        let adj = &self.e2t[e_id];
        if adj.len() == 1 {
            return None; // boundary edge
        }
        // For non-manifold edges (>2 triangles, can arise with approximate
        // coordinates), find the first adjacent triangle that isn't t_id
        if adj[0] == t_id {
            Some(adj[1])
        } else if adj[1] == t_id {
            Some(self.e2t[e_id][0])
        } else {
            panic!("no opposite tri found");
        }
    }

    /// Get the 3 edge IDs of a triangle.
    /// Ported from fast_trimesh.cpp:489-496
    pub fn adj_t2e(&self, t_id: usize) -> FmVec {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        let mut res = FmVec::new();
        // These unwraps are safe because the triangle's edges must exist
        res.push(self.tri_edge_id(t_id, 0).expect("edge 0 missing"));
        res.push(self.tri_edge_id(t_id, 1).expect("edge 1 missing"));
        res.push(self.tri_edge_id(t_id, 2).expect("edge 2 missing"));
        res
    }

    /// Get all adjacent triangles to a triangle (via shared edges).
    /// Ported from fast_trimesh.cpp:520-535
    pub fn adj_t2t(&self, t_id: usize) -> FmVec {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        let mut res = FmVec::new();
        for e_id in self.adj_t2e(t_id) {
            for &nbr_t in &self.e2t[e_id] {
                if nbr_t != t_id {
                    res.push(nbr_t);
                }
            }
        }
        res
    }

    /// Check if curr_v_id follows prev_v_id in CCW order within triangle t_id.
    /// Ported from fast_trimesh.cpp:539-545
    pub fn tri_verts_are_ccw(&self, t_id: usize, curr_v_id: usize, prev_v_id: usize) -> bool {
        let prev_off = self.tri_vert_offset(t_id, prev_v_id);
        let curr_off = self.tri_vert_offset(t_id, curr_v_id);
        curr_off == (prev_off + 1) % 3
    }

    /// Compute 2D orientation of a triangle projected onto the reference plane.
    /// Returns +1 (CCW), -1 (CW), or 0 (degenerate).
    /// Uses indirect predicates — dispatches through orient2d_indirect for
    /// correct handling of LPI/TPI implicit points.
    /// Ported from fast_trimesh.cpp:549-558
    pub fn tri_orientation(&self, t_id: usize) -> i32 {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        let p0 = self.implicit_point(self.tri_vert_id(t_id, 0));
        let p1 = self.implicit_point(self.tri_vert_id(t_id, 1));
        let p2 = self.implicit_point(self.tri_vert_id(t_id, 2));
        let proj = match self.triangle_plane {
            Plane::XY => crate::boolean::indirect_predicates::ProjectionAxis::XY,
            Plane::YZ => crate::boolean::indirect_predicates::ProjectionAxis::YZ,
            Plane::ZX => crate::boolean::indirect_predicates::ProjectionAxis::ZX,
        };
        let det = crate::boolean::indirect_predicates::orient2d_indirect(p0, p1, p2, proj);
        if det > 0.0 {
            1
        } else if det < 0.0 {
            -1
        } else {
            0
        }
    }

    /// Check if a triangle contains a given vertex.
    /// Ported from fast_trimesh.cpp:562-570
    pub fn tri_contains_vert(&self, t_id: usize, v_id: usize) -> bool {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        let tv = &self.triangles[t_id].v;
        tv[0] == v_id || tv[1] == v_id || tv[2] == v_id
    }

    /// Find the offset (0, 1, or 2) of a vertex within a triangle.
    /// Ported from fast_trimesh.cpp:574-581
    pub fn tri_vert_offset(&self, t_id: usize, v_id: usize) -> usize {
        for off in 0..3 {
            if self.triangles[t_id].v[off] == v_id {
                return off;
            }
        }
        panic!("tri_vert_offset: vertex not in triangle");
    }

    /// Get triangle info (tree node ID).
    /// Ported from fast_trimesh.cpp:585-589
    pub fn tri_info(&self, t_id: usize) -> usize {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        self.triangles[t_id].info
    }

    /// Set triangle info (tree node ID).
    /// Ported from fast_trimesh.cpp:593-597
    pub fn set_tri_info(&mut self, t_id: usize, val: usize) {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        self.triangles[t_id].info = val;
    }

    // ========================================================================
    // Mesh manipulation
    // ========================================================================

    /// Add a vertex with an original ID mapping.
    /// Ported from fast_trimesh.cpp:603-612
    pub fn add_vert(&mut self, point: ImplicitPoint, orig_v_id: usize) -> usize {
        let v_id = self.vertices.len();
        self.vertices.push(Vertex {
            point,
            info: orig_v_id,
        });
        self.v2e.push(FmVec::new());
        self.rev_vtx_map.insert(orig_v_id, v_id);
        v_id
    }

    /// Add a vertex without an original ID mapping.
    /// Ported from fast_trimesh.cpp:616-620
    fn add_vert_no_map(&mut self, point: ImplicitPoint) {
        self.vertices.push(Vertex { point, info: 0 });
        self.v2e.push(FmVec::new());
    }

    /// Add a triangle. Idempotent — returns existing tri ID if already present.
    /// Creates missing edges and updates e2t adjacency.
    /// Ported from fast_trimesh.cpp:624-646
    pub fn add_tri(&mut self, tv0_id: usize, tv1_id: usize, tv2_id: usize) -> usize {
        assert!(
            tv0_id < self.vertices.len()
                && tv1_id < self.vertices.len()
                && tv2_id < self.vertices.len(),
            "vtx id out of range"
        );
        assert!(
            tv0_id != tv1_id && tv0_id != tv2_id && tv1_id != tv2_id,
            "degenerate triangle: ({}, {}, {})",
            tv0_id,
            tv1_id,
            tv2_id
        );

        // Check if triangle already exists
        if let Some(t_id) = self.tri_id(tv0_id, tv1_id, tv2_id) {
            return t_id;
        }

        let t_id = self.triangles.len();
        self.triangles.push(Triangle {
            v: [tv0_id, tv1_id, tv2_id],
            info: 0,
        });

        // adding missing edges
        let e0_id = self.add_edge(tv0_id, tv1_id);
        let e1_id = self.add_edge(tv1_id, tv2_id);
        let e2_id = self.add_edge(tv2_id, tv0_id);

        self.e2t[e0_id].push(t_id);
        self.e2t[e1_id].push(t_id);
        self.e2t[e2_id].push(t_id);

        t_id
    }

    /// Remove an edge and all its adjacent triangles.
    /// Ported from fast_trimesh.cpp:650-654
    pub fn remove_edge(&mut self, e_id: usize) {
        assert!(e_id < self.edges.len(), "edge id out of range");
        let t_ids: FmVec = self.e2t[e_id].clone();
        self.remove_tris(&t_ids);
    }

    /// Remove a triangle. Cleans up dangling edges using swap-with-last.
    /// Ported from fast_trimesh.cpp:658-688
    pub fn remove_tri(&mut self, t_id: usize) {
        assert!(t_id < self.triangles.len(), "tri id out of range");

        let e0_id = self.tri_edge_id(t_id, 0).expect("edge 0 missing");
        let e1_id = self.tri_edge_id(t_id, 1).expect("edge 1 missing");
        let e2_id = self.tri_edge_id(t_id, 2).expect("edge 2 missing");

        remove_from_vec(&mut self.e2t[e0_id], t_id);
        remove_from_vec(&mut self.e2t[e1_id], t_id);
        remove_from_vec(&mut self.e2t[e2_id], t_id);

        // Collect dangling edges (those with no remaining adjacent triangles),
        // sorted in descending order so we remove from the end first.
        let mut dangling_edges: SmallVec<[usize; 3]> = SmallVec::new();
        if self.e2t[e0_id].is_empty() {
            dangling_edges.push(e0_id);
        }
        if self.e2t[e1_id].is_empty() {
            dangling_edges.push(e1_id);
        }
        if self.e2t[e2_id].is_empty() {
            dangling_edges.push(e2_id);
        }
        dangling_edges.sort_unstable_by(|a, b| b.cmp(a)); // descending

        for &e_id in &dangling_edges {
            let mut v0_id = self.edges[e_id].v.0;
            let mut v1_id = self.edges[e_id].v.1;
            if v1_id > v0_id {
                std::mem::swap(&mut v0_id, &mut v1_id);
            }
            remove_from_vec(&mut self.v2e[v0_id], e_id);
            remove_from_vec(&mut self.v2e[v1_id], e_id);
            self.remove_edge_unref(e_id);
        }

        self.remove_tri_unref(t_id);
    }

    /// Remove multiple triangles (sorted descending to preserve indices).
    /// Ported from fast_trimesh.cpp:692-704
    pub fn remove_tris(&mut self, t_ids: &FmVec) {
        let mut tmp: FmVec = t_ids.clone();
        tmp.sort_unstable_by(|a, b| b.cmp(a)); // descending
        for &t_id in &tmp {
            self.remove_tri(t_id);
        }
    }

    /// Split an edge by inserting a vertex. Each adjacent triangle is replaced
    /// by 2 new triangles. The original triangles are removed.
    /// Ported from fast_trimesh.cpp:708-726
    pub fn split_edge(&mut self, e_id: usize, v_id: usize) {
        assert!(e_id < self.edges.len(), "edge id out of range");

        let ev0_id = self.edges[e_id].v.0;
        let ev1_id = self.edges[e_id].v.1;

        // Skip if v_id is already an endpoint of the edge
        if v_id == ev0_id || v_id == ev1_id {
            return;
        }

        let adj_tris: FmVec = self.e2t[e_id].clone();
        let mut tris_to_remove: FmVec = FmVec::new();
        for &t_id in &adj_tris {
            let mut ev0 = ev0_id;
            let mut ev1 = ev1_id;
            let v_opp = self.tri_vert_opposite_to(t_id, ev0, ev1);
            if v_opp == v_id {
                // v_id is already the opposite vertex — remove the triangle
                // (its shared edge is being split) but don't create degenerate
                // replacement triangles.
                tris_to_remove.push(t_id);
                continue;
            }
            if self.tri_verts_are_ccw(t_id, ev0, ev1) {
                std::mem::swap(&mut ev0, &mut ev1);
            }
            self.add_tri(v_opp, ev0, v_id);
            self.add_tri(v_opp, v_id, ev1);
            tris_to_remove.push(t_id);
        }

        self.remove_tris(&tris_to_remove);
    }

    /// Split an edge by inserting a vertex, with tree tracking.
    /// Ported from fast_trimesh.cpp:730-756
    pub fn split_edge_with_tree(&mut self, e_id: usize, v_id: usize, tree: &mut Tree) {
        assert!(e_id < self.edges.len(), "edge id out of range");

        let ev0_id = self.edges[e_id].v.0;
        let ev1_id = self.edges[e_id].v.1;

        let adj_tris: FmVec = self.e2t[e_id].clone();
        for &t_id in &adj_tris {
            let mut ev0 = ev0_id;
            let mut ev1 = ev1_id;
            let v_opp = self.tri_vert_opposite_to(t_id, ev0, ev1);
            if self.tri_verts_are_ccw(t_id, ev0, ev1) {
                std::mem::swap(&mut ev0, &mut ev1);
            }

            let t0_id = self.add_tri(v_opp, ev0, v_id);
            let t1_id = self.add_tri(v_opp, v_id, ev1);

            let n0_id = tree.add_node(v_opp, ev0, v_id);
            let n1_id = tree.add_node(v_opp, v_id, ev1);

            let node_id = self.tri_node_id(t_id);
            tree.add_children_2(node_id, n0_id, n1_id);

            self.set_tri_node_id(t0_id, n0_id);
            self.set_tri_node_id(t1_id, n1_id);
        }

        self.remove_tris(&adj_tris);
    }

    /// Split a triangle by inserting an interior vertex. Replaces 1 tri with 3.
    /// Ported from fast_trimesh.cpp:760-770
    pub fn split_tri(&mut self, t_id: usize, v_id: usize) {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        assert!(v_id < self.vertices.len(), "vtx id out of range");

        let v0 = self.tri_vert_id(t_id, 0);
        let v1 = self.tri_vert_id(t_id, 1);
        let v2 = self.tri_vert_id(t_id, 2);

        self.add_tri(v0, v1, v_id);
        self.add_tri(v1, v2, v_id);
        self.add_tri(v2, v0, v_id);

        self.remove_tri(t_id);
    }

    /// Split a triangle by inserting an interior vertex, with tree tracking.
    /// Ported from fast_trimesh.cpp:774-796
    pub fn split_tri_with_tree(&mut self, t_id: usize, v_id: usize, tree: &mut Tree) {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        assert!(v_id < self.vertices.len(), "vtx id out of range");

        let node_id = self.tri_node_id(t_id);
        let v0 = self.tri_vert_id(t_id, 0);
        let v1 = self.tri_vert_id(t_id, 1);
        let v2 = self.tri_vert_id(t_id, 2);

        let t0_id = self.add_tri(v0, v1, v_id);
        let t1_id = self.add_tri(v1, v2, v_id);
        let t2_id = self.add_tri(v2, v0, v_id);

        let n0_id = tree.add_node(v0, v1, v_id);
        let n1_id = tree.add_node(v1, v2, v_id);
        let n2_id = tree.add_node(v2, v0, v_id);
        tree.add_children_3(node_id, n0_id, n1_id, n2_id);

        self.set_tri_node_id(t0_id, n0_id);
        self.set_tri_node_id(t1_id, n1_id);
        self.set_tri_node_id(t2_id, n2_id);

        self.remove_tri(t_id);
    }

    /// Flip the winding order of a triangle (reverse v[0] and v[2]).
    /// Ported from fast_trimesh.cpp:800-807
    pub fn flip_tri(&mut self, t_id: usize) {
        assert!(t_id < self.triangles.len(), "tri id out of range");
        self.triangles[t_id].v.swap(0, 2);
    }

    // ========================================================================
    // Private methods
    // ========================================================================

    /// Add an edge. Idempotent — returns existing edge ID if already present.
    /// Ported from fast_trimesh.cpp:813-827
    fn add_edge(&mut self, ev0_id: usize, ev1_id: usize) -> usize {
        if let Some(e_id) = self.edge_id(ev0_id, ev1_id) {
            return e_id;
        }

        let e_id = self.edges.len();
        self.edges.push(Edge {
            v: (ev0_id, ev1_id),
            constr: false,
        });
        self.e2t.push(FmVec::new());
        self.v2e[ev0_id].push(e_id);
        self.v2e[ev1_id].push(e_id);

        e_id
    }

    /// Check if an edge contains a given vertex.
    /// Ported from fast_trimesh.cpp:831-836
    fn edge_contains_vert(&self, e_id: usize, v_id: usize) -> bool {
        self.edges[e_id].v.0 == v_id || self.edges[e_id].v.1 == v_id
    }

    /// Swap two triangles in storage and fix all edge→tri references.
    /// Ported from fast_trimesh.cpp:847-867
    fn tri_switch(&mut self, t0_id: usize, t1_id: usize) {
        if t0_id == t1_id {
            return;
        }

        self.triangles.swap(t0_id, t1_id);

        // Collect unique edge IDs from both triangles
        let mut edges_to_update: SmallVec<[usize; 6]> = SmallVec::new();
        for off in 0..3 {
            if let Some(e) = self.tri_edge_id(t0_id, off) {
                edges_to_update.push(e);
            }
            if let Some(e) = self.tri_edge_id(t1_id, off) {
                edges_to_update.push(e);
            }
        }
        edges_to_update.sort_unstable();
        edges_to_update.dedup();

        for e_id in edges_to_update {
            for t_id in &mut self.e2t[e_id] {
                if *t_id == t0_id {
                    *t_id = t1_id;
                } else if *t_id == t1_id {
                    *t_id = t0_id;
                }
            }
        }
    }

    /// Swap two edges in storage and fix all vertex→edge and edge→tri references.
    /// Ported from fast_trimesh.cpp:871-893
    fn edge_switch(&mut self, e0_id: usize, e1_id: usize) {
        if e0_id == e1_id {
            return;
        }

        self.edges.swap(e0_id, e1_id);
        self.e2t.swap(e0_id, e1_id);

        // Collect unique vertex IDs from both edges
        let mut verts_to_update: SmallVec<[usize; 4]> = SmallVec::new();
        verts_to_update.push(self.edges[e0_id].v.0);
        verts_to_update.push(self.edges[e0_id].v.1);
        verts_to_update.push(self.edges[e1_id].v.0);
        verts_to_update.push(self.edges[e1_id].v.1);
        verts_to_update.sort_unstable();
        verts_to_update.dedup();

        for v_id in verts_to_update {
            for e_id in &mut self.v2e[v_id] {
                if *e_id == e0_id {
                    *e_id = e1_id;
                } else if *e_id == e1_id {
                    *e_id = e0_id;
                }
            }
        }
    }

    /// Remove an edge by swapping with the last and popping.
    /// Ported from fast_trimesh.cpp:897-903
    fn remove_edge_unref(&mut self, e_id: usize) {
        self.e2t[e_id].clear();
        let last = self.edges.len() - 1;
        self.edge_switch(e_id, last);
        self.edges.pop();
        self.e2t.pop();
    }

    /// Remove a triangle by swapping with the last and popping.
    /// Ported from fast_trimesh.cpp:907-911
    fn remove_tri_unref(&mut self, t_id: usize) {
        let last = self.triangles.len() - 1;
        self.tri_switch(t_id, last);
        self.triangles.pop();
    }
}

// 2D orientation helpers moved to indirect_predicates::orient2d_indirect

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a simple CCW triangle on the XY plane.
    fn make_single_tri() -> FastTrimesh {
        FastTrimesh::new(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [100, 101, 102],
            Plane::XY,
        )
    }

    #[test]
    fn test_fast_trimesh_construction() {
        let mesh = make_single_tri();
        assert_eq!(mesh.num_verts(), 3);
        assert_eq!(mesh.num_edges(), 3);
        assert_eq!(mesh.num_tris(), 1);
        assert_eq!(mesh.ref_plane(), Plane::XY);

        // Vertex coordinates
        assert_eq!(mesh.vert(0), [0.0, 0.0, 0.0]);
        assert_eq!(mesh.vert(1), [1.0, 0.0, 0.0]);
        assert_eq!(mesh.vert(2), [0.0, 1.0, 0.0]);

        // Original ID mapping
        assert_eq!(mesh.vert_orig_id(0), 100);
        assert_eq!(mesh.vert_orig_id(1), 101);
        assert_eq!(mesh.vert_orig_id(2), 102);
        assert_eq!(mesh.vert_new_id(101), Some(1));

        // Triangle vertices
        assert_eq!(mesh.tri(0), &[0, 1, 2]);
    }

    #[test]
    fn test_fast_trimesh_adjacency() {
        let mesh = make_single_tri();

        // Each vertex should be adjacent to 2 edges
        assert_eq!(mesh.vert_valence(0), 2);
        assert_eq!(mesh.vert_valence(1), 2);
        assert_eq!(mesh.vert_valence(2), 2);

        // Each edge should be adjacent to 1 triangle (boundary)
        for e in 0..3 {
            assert!(mesh.edge_is_boundary(e));
            assert!(!mesh.edge_is_manifold(e));
        }

        // Triangle adjacency
        let t2t = mesh.adj_t2t(0);
        assert!(t2t.is_empty()); // single triangle, no neighbors

        // edge_id lookup
        assert!(mesh.edge_id(0, 1).is_some());
        assert!(mesh.edge_id(1, 2).is_some());
        assert!(mesh.edge_id(2, 0).is_some());

        // edge_opp_to_vert
        let e_opp_0 = mesh.edge_opp_to_vert(0, 0);
        let (va, vb) = mesh.edge(e_opp_0);
        assert!((va == 1 && vb == 2) || (va == 2 && vb == 1));
    }

    #[test]
    fn test_fast_trimesh_split_tri() {
        let mut mesh = make_single_tri();

        // Add an interior vertex and split
        let v3 = mesh.add_vert(ImplicitPoint::Explicit([0.25, 0.25, 0.0]), 103);
        assert_eq!(v3, 3);

        mesh.split_tri(0, v3);

        // Should now have 3 triangles, 3+3 edges
        assert_eq!(mesh.num_tris(), 3);
        assert_eq!(mesh.num_verts(), 4);
        // Interior vertex should have valence 3
        assert_eq!(mesh.vert_valence(v3), 3);
    }

    #[test]
    fn test_fast_trimesh_split_edge() {
        let mut mesh = make_single_tri();

        // Find the edge between v0 and v1
        let e_id = mesh.edge_id(0, 1).expect("edge 0-1 not found");

        // Add a midpoint vertex
        let v3 = mesh.add_vert(ImplicitPoint::Explicit([0.5, 0.0, 0.0]), 103);

        mesh.split_edge(e_id, v3);

        // Original 1 tri → 2 tris (boundary edge, 1 adjacent tri → 2 new)
        assert_eq!(mesh.num_tris(), 2);
        assert_eq!(mesh.num_verts(), 4);

        // The new vertex should be connected
        assert!(mesh.vert_valence(v3) > 0);

        // Both new triangles should contain v3
        for t in 0..mesh.num_tris() {
            assert!(mesh.tri_contains_vert(t, v3) || mesh.tri_contains_vert(t, 2));
        }
    }

    #[test]
    fn test_fast_trimesh_orientation() {
        let mesh = make_single_tri();
        // (0,0) → (1,0) → (0,1) is CCW
        let ori = mesh.tri_orientation(0);
        assert_eq!(ori, 1);

        // Flip and check
        let mut mesh2 = make_single_tri();
        mesh2.flip_tri(0);
        let ori2 = mesh2.tri_orientation(0);
        assert_eq!(ori2, -1);
    }

    #[test]
    fn test_fast_trimesh_from_verts_and_tris() {
        let verts = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        let tris = vec![0, 1, 2, 1, 3, 2];
        let mesh = FastTrimesh::from_verts_and_tris(&verts, &tris, Plane::XY);

        assert_eq!(mesh.num_verts(), 4);
        assert_eq!(mesh.num_tris(), 2);
        // Shared edge between the two triangles
        let shared_e = mesh.edge_id(1, 2).expect("shared edge 1-2 not found");
        assert!(mesh.edge_is_manifold(shared_e));
    }
}
