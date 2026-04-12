//! Clean Cherchi mesh arrangement module.
//!
//! Implements per-triangle constrained triangulation per Cherchi et al. 2020 [#9]
//! and Livesu & Cherchi 2022 "Deterministic Linear Time Constrained Triangulation
//! Using Simplified Earcut".
//!
//! C++ reference: github.com/gcherchi/FastAndRobustMeshArrangements
//! Key files: triangulation.cpp, fast_trimesh.cpp
//!
//! Architecture: Each intersected input triangle is processed INDEPENDENTLY with
//! its own LocalMesh. Points are inserted first (edge points sorted along edges,
//! then interior points), then constraint segments via Algorithm 1 (boundary
//! walker + linear earcut). This per-triangle approach avoids cross-parent
//! boundary issues that broke previous global-mesh attempts.

/// Local sub-mesh for triangulating a single original triangle.
/// Ported from Cherchi FastTrimesh (fast_trimesh.h).
/// Ref [#9] Cherchi 2020
pub(crate) struct LocalMesh {
    /// Global vertex indices (into the shared vertex array)
    pub global_verts: Vec<usize>,
    /// Triangles as local vertex indices (into self.global_verts)
    pub tris: Vec<[usize; 3]>,
    /// Undirected edges as (local_v0, local_v1) with canonical ordering
    edges: Vec<(usize, usize)>,
    /// Vertex → edge adjacency
    v2e: Vec<Vec<usize>>,
    /// Edge → triangle adjacency
    e2t: Vec<Vec<usize>>,
    /// Per-edge constraint flag
    constrained: Vec<bool>,
    /// Soft-delete for triangles
    removed: Vec<bool>,
}

impl LocalMesh {
    /// Create a LocalMesh from a single triangle with 3 global vertex indices.
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:44-52 (FastTrimesh constructor)
    ///   github.com/gcherchi/FastAndRobustMeshArrangements
    ///
    /// Ref [#9] Cherchi 2020
    pub fn new(v0: usize, v1: usize, v2: usize) -> Self {
        let global_verts = vec![v0, v1, v2];
        let tris = vec![[0, 1, 2]];
        // Edges in canonical (min, max) order: local indices 0,1,2
        let e0 = (0, 1);
        let e1 = (1, 2);
        let e2 = (0, 2);
        let edges = vec![e0, e1, e2];
        // v2e: vertex 0 → edges 0,2; vertex 1 → edges 0,1; vertex 2 → edges 1,2
        let v2e = vec![vec![0, 2], vec![0, 1], vec![1, 2]];
        // e2t: all edges adjacent to triangle 0
        let e2t = vec![vec![0], vec![0], vec![0]];
        let constrained = vec![false; 3];
        let removed = vec![false];

        LocalMesh {
            global_verts,
            tris,
            edges,
            v2e,
            e2t,
            constrained,
            removed,
        }
    }

    /// Add a global vertex to the local mesh, returning its local index.
    /// If already present, returns existing local index.
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:603-612 (addVert)
    ///   github.com/gcherchi/FastAndRobustMeshArrangements
    pub fn add_vert(&mut self, global_idx: usize) -> usize {
        if let Some(li) = self.local_vert(global_idx) {
            return li;
        }
        let li = self.global_verts.len();
        self.global_verts.push(global_idx);
        self.v2e.push(Vec::new());
        li
    }

    /// Look up the local index for a global vertex index.
    pub fn local_vert(&self, global_idx: usize) -> Option<usize> {
        self.global_verts.iter().position(|&g| g == global_idx)
    }

    /// Find the edge index for the undirected edge (lv0, lv1).
    /// Searches through v2e adjacency for efficiency.
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:296-308 (edgeID)
    ///   github.com/gcherchi/FastAndRobustMeshArrangements
    pub fn find_edge(&self, lv0: usize, lv1: usize) -> Option<usize> {
        let canonical = (lv0.min(lv1), lv0.max(lv1));
        for &e_id in &self.v2e[lv0] {
            if self.edges[e_id] == canonical {
                return Some(e_id);
            }
        }
        None
    }

    /// Add an edge between two local vertices (if not already present).
    /// Returns edge index.
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:813-827 (addEdge)
    ///   github.com/gcherchi/FastAndRobustMeshArrangements
    fn add_edge(&mut self, lv0: usize, lv1: usize) -> usize {
        if let Some(e_id) = self.find_edge(lv0, lv1) {
            return e_id;
        }
        let e_id = self.edges.len();
        let canonical = (lv0.min(lv1), lv0.max(lv1));
        self.edges.push(canonical);
        self.e2t.push(Vec::new());
        self.constrained.push(false);
        self.v2e[lv0].push(e_id);
        self.v2e[lv1].push(e_id);
        e_id
    }

    /// Add a triangle with 3 local vertex indices. Creates missing edges,
    /// updates adjacency. Returns triangle index. If the triangle already
    /// exists, returns the existing index.
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:624-646 (addTri)
    ///   github.com/gcherchi/FastAndRobustMeshArrangements
    fn add_tri(&mut self, lv0: usize, lv1: usize, lv2: usize) -> usize {
        // Check if triangle already exists
        if let Some(t_id) = self.find_tri(lv0, lv1, lv2) {
            return t_id;
        }
        let t_id = self.tris.len();
        self.tris.push([lv0, lv1, lv2]);
        self.removed.push(false);

        let e0 = self.add_edge(lv0, lv1);
        let e1 = self.add_edge(lv1, lv2);
        let e2 = self.add_edge(lv2, lv0);

        self.e2t[e0].push(t_id);
        self.e2t[e1].push(t_id);
        self.e2t[e2].push(t_id);

        t_id
    }

    /// Find a triangle by its 3 local vertex indices (order-independent).
    fn find_tri(&self, lv0: usize, lv1: usize, lv2: usize) -> Option<usize> {
        // Use edge adjacency for efficient lookup
        let e_id = self.find_edge(lv0, lv1)?;
        for &t_id in &self.e2t[e_id] {
            if !self.removed[t_id] && self.tri_contains_vert(t_id, lv2) {
                return Some(t_id);
            }
        }
        None
    }

    /// Check if a triangle contains a given local vertex.
    fn tri_contains_vert(&self, t_id: usize, lv: usize) -> bool {
        let t = self.tris[t_id];
        t[0] == lv || t[1] == lv || t[2] == lv
    }

    /// Return the 3rd vertex of a triangle given two of its vertices.
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:452-466 (triVertOppositeTo)
    fn tri_vert_opposite_to(&self, t_id: usize, lv0: usize, lv1: usize) -> usize {
        let t = self.tris[t_id];
        for &v in &t {
            if v != lv0 && v != lv1 {
                return v;
            }
        }
        panic!("tri_vert_opposite_to: no opposite vertex found");
    }

    /// Which offset (0, 1, or 2) is the given vertex in the triangle?
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:574-581 (triVertOffset)
    fn tri_vert_offset(&self, t_id: usize, lv: usize) -> usize {
        let t = self.tris[t_id];
        for off in 0..3 {
            if t[off] == lv {
                return off;
            }
        }
        panic!("tri_vert_offset: vertex not in triangle");
    }

    /// Return the other triangle sharing this edge, or None if boundary.
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:470-485 (triOppToEdge)
    fn tri_opp_to_edge(&self, edge_id: usize, tri_id: usize) -> Option<usize> {
        for &t_id in &self.e2t[edge_id] {
            if t_id != tri_id && !self.removed[t_id] {
                return Some(t_id);
            }
        }
        None
    }

    /// Return the edge opposite to a vertex in a triangle.
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:328-340 (edgeOppToVert)
    fn edge_opp_to_vert(&self, t_id: usize, lv: usize) -> usize {
        let t = self.tris[t_id];
        let (a, b) = if t[0] == lv {
            (t[1], t[2])
        } else if t[1] == lv {
            (t[0], t[2])
        } else {
            (t[0], t[1])
        };
        self.find_edge(a, b)
            .expect("edge_opp_to_vert: edge not found")
    }

    /// Check if triangle vertices are in CCW order relative to a given pair.
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:539-545 (triVertsAreCCW)
    fn tri_verts_are_ccw(&self, t_id: usize, curr: usize, prev: usize) -> bool {
        let prev_off = self.tri_vert_offset(t_id, prev);
        let curr_off = self.tri_vert_offset(t_id, curr);
        curr_off == (prev_off + 1) % 3
    }

    /// Get triangles adjacent to a vertex (via v2e → e2t).
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:238-251 (adjV2T)
    fn adj_v2t(&self, lv: usize) -> Vec<usize> {
        let mut result = Vec::new();
        for &e_id in &self.v2e[lv] {
            for &t_id in &self.e2t[e_id] {
                if !self.removed[t_id] && !result.contains(&t_id) {
                    result.push(t_id);
                }
            }
        }
        result
    }

    /// Remove a triangle (mark removed, clean up e2t adjacency, remove dangling edges).
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:658-688 (removeTri)
    ///   github.com/gcherchi/FastAndRobustMeshArrangements
    fn remove_tri(&mut self, t_id: usize) {
        if self.removed[t_id] {
            return;
        }
        self.removed[t_id] = true;

        // Remove t_id from e2t for all 3 edges of this triangle
        let t = self.tris[t_id];
        let edge_ids: Vec<usize> = [
            self.find_edge(t[0], t[1]),
            self.find_edge(t[1], t[2]),
            self.find_edge(t[2], t[0]),
        ]
        .iter()
        .filter_map(|e| *e)
        .collect();

        for &e_id in &edge_ids {
            self.e2t[e_id].retain(|&tid| tid != t_id);
        }
    }

    /// Split an edge by inserting a vertex. The triangles adjacent to the edge
    /// are each split into two. New triangles and edges are created,
    /// and adjacency is fully updated. The original triangles are removed.
    ///
    /// For each adjacent triangle T with vertices (a, b, opp) where (a,b) is
    /// the split edge: produces T1=(opp, a, new) and T2=(opp, new, b).
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:708-726 (splitEdge)
    ///   github.com/gcherchi/FastAndRobustMeshArrangements
    ///
    /// Ref [#9] Cherchi 2020, Section 5.2
    pub fn split_edge(&mut self, edge_id: usize, local_vert: usize) {
        let (ev0, ev1) = self.edges[edge_id];

        // Collect adjacent triangles before mutation
        let adj_tris: Vec<usize> = self.e2t[edge_id]
            .iter()
            .copied()
            .filter(|&t| !self.removed[t])
            .collect();

        for t_id in &adj_tris {
            let t_id = *t_id;
            let v_opp = self.tri_vert_opposite_to(t_id, ev0, ev1);

            // Determine winding: if (ev0, ev1) is CCW in this tri, swap
            let (a, b) = if self.tri_verts_are_ccw(t_id, ev0, ev1) {
                (ev1, ev0)
            } else {
                (ev0, ev1)
            };

            self.add_tri(v_opp, a, local_vert);
            self.add_tri(v_opp, local_vert, b);
        }

        // Remove all original adjacent triangles
        for &t_id in &adj_tris {
            self.remove_tri(t_id);
        }
    }

    /// Split a triangle by inserting an interior vertex, producing 3 new triangles:
    /// (v0, v1, new), (v1, v2, new), (v2, v0, new). The original triangle is removed.
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   fast_trimesh.cpp:760-770 (splitTri)
    ///   github.com/gcherchi/FastAndRobustMeshArrangements
    ///
    /// Ref [#9] Cherchi 2020, Section 5.2
    pub fn split_interior(&mut self, tri_id: usize, local_vert: usize) {
        let [v0, v1, v2] = self.tris[tri_id];
        self.add_tri(v0, v1, local_vert);
        self.add_tri(v1, v2, local_vert);
        self.add_tri(v2, v0, local_vert);
        self.remove_tri(tri_id);
    }

    /// Return all non-removed triangles as global vertex indices.
    pub fn active_tris(&self) -> Vec<[usize; 3]> {
        let mut result = Vec::new();
        for (i, tri) in self.tris.iter().enumerate() {
            if !self.removed[i] {
                result.push([
                    self.global_verts[tri[0]],
                    self.global_verts[tri[1]],
                    self.global_verts[tri[2]],
                ]);
            }
        }
        result
    }

    /// Walk from lv_start toward lv_stop through the mesh, collecting
    /// intersected edges and triangles in order. These are the edges/tris
    /// crossed by the straight segment from lv_start to lv_stop.
    ///
    /// Simplified version of Cherchi's findIntersectingElements:
    /// - No TPI creation (constraint-constraint intersection)
    /// - No collinear vertex splitting (handled upstream)
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   triangulation.cpp:649-810 (findIntersectingElements)
    ///   github.com/gcherchi/FastAndRobustMeshArrangements
    ///
    /// Ref [#9] Cherchi 2020, Section 5.3
    pub fn find_intersecting_elements(
        &self,
        lv_start: usize,
        lv_stop: usize,
        all_verts: &[[f64; 3]],
    ) -> (Vec<usize>, Vec<usize>) {
        let mut intersected_edges = Vec::new();
        let mut intersected_tris = Vec::new();

        // Find the first edge in link(lv_start) that intersects segment (lv_start, lv_stop)
        let start_tris = self.adj_v2t(lv_start);

        for &t_id in &start_tris {
            let e_id = self.edge_opp_to_vert(t_id, lv_start);
            let (ev0, ev1) = self.edges[e_id];

            if ev0 == lv_stop || ev1 == lv_stop {
                // lv_stop is an adjacent vertex — no edges to cross
                continue;
            }

            if segments_intersect_inside(all_verts, self, lv_start, lv_stop, ev0, ev1) {
                intersected_edges.push(e_id);
                intersected_tris.push(t_id);
                break;
            }
        }

        if intersected_edges.is_empty() {
            return (intersected_edges, intersected_tris);
        }

        // Walk along the topology finding subsequent intersected edges/tris
        loop {
            let e_id = *intersected_edges.last().unwrap();
            let last_tri = *intersected_tris.last().unwrap();
            let (ev0, ev1) = self.edges[e_id];

            let t_id = match self.tri_opp_to_edge(e_id, last_tri) {
                Some(t) => t,
                None => break, // boundary edge — shouldn't happen in valid mesh
            };

            let v2 = self.tri_vert_opposite_to(t_id, ev0, ev1);

            if v2 == lv_stop {
                // Reached destination — append final triangle
                intersected_tris.push(t_id);
                break;
            }

            // Check which of the two other edges of this triangle is intersected
            if segments_intersect_inside(all_verts, self, lv_start, lv_stop, ev0, v2) {
                let next_e = self.find_edge(ev0, v2).expect("edge should exist");
                intersected_edges.push(next_e);
                intersected_tris.push(t_id);
            } else if segments_intersect_inside(all_verts, self, lv_start, lv_stop, ev1, v2) {
                let next_e = self.find_edge(ev1, v2).expect("edge should exist");
                intersected_edges.push(next_e);
                intersected_tris.push(t_id);
            } else {
                // v2 is on the segment — shouldn't happen if points are pre-inserted
                // but handle gracefully
                intersected_tris.push(t_id);
                break;
            }
        }

        (intersected_edges, intersected_tris)
    }

    /// Walk the cavity boundary to extract polygon vertices on one side of the
    /// segment being inserted.
    ///
    /// Given the ordered lists of intersected triangles and edges, walk from
    /// v_start toward v_stop collecting boundary vertices of the cavity.
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   triangulation.cpp:812-854 (boundaryWalker)
    ///   github.com/gcherchi/FastAndRobustMeshArrangements
    ///
    /// Ref [#9] Cherchi 2020, Section 5.3
    /// Ref: Livesu & Cherchi 2022 "Deterministic Linear Time Constrained
    ///   Triangulation Using Simplified Earcut"
    pub fn boundary_walker(
        &self,
        v_start: usize,
        v_stop: usize,
        tris: &[usize],
        edges: &[usize],
    ) -> Vec<usize> {
        let mut h = Vec::new();
        h.push(v_start);

        let mut t_idx = 0;
        let mut e_idx = 0;

        loop {
            let curr_v = *h.last().unwrap();
            let off = self.tri_vert_offset(tris[t_idx], curr_v);
            let mut next_v = self.tris[tris[t_idx]][(off + 1) % 3];

            // Skip forward while the next vertex's edge matches the current intersected edge
            while e_idx < edges.len() && self.find_edge(curr_v, next_v) == Some(edges[e_idx]) {
                t_idx += 1;
                if t_idx < tris.len() && self.tri_contains_vert(tris[t_idx], v_stop) {
                    h.push(v_stop);
                    return h;
                }
                e_idx += 1;

                if t_idx >= tris.len() || e_idx > edges.len() {
                    break;
                }

                let off2 = self.tri_vert_offset(tris[t_idx], curr_v);
                next_v = self.tris[tris[t_idx]][(off2 + 1) % 3];
            }

            h.push(next_v);
            t_idx += 1;

            if next_v == v_stop {
                return h;
            }

            if t_idx < tris.len() && self.tri_contains_vert(tris[t_idx], v_stop) {
                h.push(v_stop);
                return h;
            }

            e_idx += 1;

            if t_idx >= tris.len() {
                break;
            }
        }

        h
    }

    /// Insert a constraint segment between two local vertices using Algorithm 1:
    /// 1. If edge already exists → mark constrained, done
    /// 2. Find intersecting elements (walk from lv0 toward lv1)
    /// 3. Boundary walker (two calls: forward + reversed)
    /// 4. Linear earcut (both polygons)
    /// 5. Add new triangles, remove old ones
    /// 6. Mark edge as constrained
    ///
    /// Ported from Cherchi et al. C++ reference:
    ///   triangulation.cpp:602-645 (addConstraintSegment)
    ///   github.com/gcherchi/FastAndRobustMeshArrangements
    ///
    /// Ref [#9] Cherchi 2020, Section 5.3
    pub fn add_constraint_segment(&mut self, lv0: usize, lv1: usize, all_verts: &[[f64; 3]]) {
        // 1. If edge already exists, just mark it constrained
        if let Some(e_id) = self.find_edge(lv0, lv1) {
            self.constrained[e_id] = true;
            return;
        }

        // Choose start from lower-valence vertex for efficiency
        let (v_start, v_stop) = if self.v2e[lv0].len() <= self.v2e[lv1].len() {
            (lv0, lv1)
        } else {
            (lv1, lv0)
        };

        // 2. Find intersecting elements
        let (intersected_edges, intersected_tris) =
            self.find_intersecting_elements(v_start, v_stop, all_verts);

        if intersected_edges.is_empty() {
            return;
        }

        // 3. Boundary walker (two calls: forward + reversed)
        let h0 = self.boundary_walker(v_start, v_stop, &intersected_tris, &intersected_edges);

        let rev_tris: Vec<usize> = intersected_tris.iter().copied().rev().collect();
        let rev_edges: Vec<usize> = intersected_edges.iter().copied().rev().collect();
        let h1 = self.boundary_walker(v_stop, v_start, &rev_tris, &rev_edges);

        // Determine projection axis from triangle plane
        let proj_axis = best_projection_axis(all_verts, self);
        let orientation = tri_orientation(all_verts, self, proj_axis);

        // 4. Linear earcut on both polygons — use global vertices for coordinates
        let h0_global: Vec<usize> = h0.iter().map(|&lv| self.global_verts[lv]).collect();
        let h1_global: Vec<usize> = h1.iter().map(|&lv| self.global_verts[lv]).collect();

        let tris0 = earcut_linear(&h0_global, all_verts, proj_axis, orientation);
        let tris1 = earcut_linear(&h1_global, all_verts, proj_axis, orientation);

        // 5. Add new triangles (converting global back to local)
        for tri in tris0.iter().chain(tris1.iter()) {
            let l0 = self
                .local_vert(tri[0])
                .expect("vertex should be in local mesh");
            let l1 = self
                .local_vert(tri[1])
                .expect("vertex should be in local mesh");
            let l2 = self
                .local_vert(tri[2])
                .expect("vertex should be in local mesh");
            self.add_tri(l0, l1, l2);
        }

        // Remove old intersected triangles
        for &t_id in &intersected_tris {
            self.remove_tri(t_id);
        }

        // 6. Mark constraint edge
        if let Some(e_id) = self.find_edge(v_start, v_stop) {
            self.constrained[e_id] = true;
        }
    }
}

/// Compute orient2d for 3 points projected to 2D.
/// Returns positive for CCW, negative for CW, zero for collinear.
fn orient2d_projected(a: &[f64; 3], b: &[f64; 3], c: &[f64; 3], proj: (usize, usize)) -> f64 {
    geometry_predicates::orient2d(
        [a[proj.0], a[proj.1]],
        [b[proj.0], b[proj.1]],
        [c[proj.0], c[proj.1]],
    )
}

/// Test if segments (a,b) and (c,d) intersect strictly in their interiors
/// (not at endpoints).
///
/// Ported from Cherchi et al. C++ reference:
///   triangulation.h:111 (segmentsIntersectInside)
fn segments_intersect_inside(
    all_verts: &[[f64; 3]],
    mesh: &LocalMesh,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
) -> bool {
    let proj = best_projection_axis(all_verts, mesh);
    let ga = all_verts[mesh.global_verts[a]];
    let gb = all_verts[mesh.global_verts[b]];
    let gc = all_verts[mesh.global_verts[c]];
    let gd = all_verts[mesh.global_verts[d]];

    let d1 = orient2d_projected(&ga, &gb, &gc, proj);
    let d2 = orient2d_projected(&ga, &gb, &gd, proj);
    let d3 = orient2d_projected(&gc, &gd, &ga, proj);
    let d4 = orient2d_projected(&gc, &gd, &gb, proj);

    // Segments cross strictly inside if each segment separates the
    // other's endpoints to opposite sides
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// Choose the best 2D projection axis by looking at the triangle's normal.
/// Picks the axis pair that maximizes projected area (drops the coordinate
/// with the largest absolute normal component).
fn best_projection_axis(all_verts: &[[f64; 3]], mesh: &LocalMesh) -> (usize, usize) {
    // Use the first 3 vertices (the original triangle)
    if mesh.global_verts.len() < 3 {
        return (0, 1);
    }
    let a = all_verts[mesh.global_verts[0]];
    let b = all_verts[mesh.global_verts[1]];
    let c = all_verts[mesh.global_verts[2]];

    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];

    // Cross product = normal
    let nx = (ab[1] * ac[2] - ab[2] * ac[1]).abs();
    let ny = (ab[2] * ac[0] - ab[0] * ac[2]).abs();
    let nz = (ab[0] * ac[1] - ab[1] * ac[0]).abs();

    if nx >= ny && nx >= nz {
        (1, 2) // YZ — drop X
    } else if ny >= nx && ny >= nz {
        (2, 0) // ZX — drop Y
    } else {
        (0, 1) // XY — drop Z
    }
}

/// Determine the orientation of the first triangle in the local mesh.
fn tri_orientation(all_verts: &[[f64; 3]], mesh: &LocalMesh, proj: (usize, usize)) -> i32 {
    let a = all_verts[mesh.global_verts[0]];
    let b = all_verts[mesh.global_verts[1]];
    let c = all_verts[mesh.global_verts[2]];
    let o = orient2d_projected(&a, &b, &c, proj);
    if o > 0.0 {
        1
    } else {
        -1
    }
}

/// Simplified linear earcut triangulation for a simple polygon.
///
/// All internal convex vertices (not the first/last segment endpoints)
/// are safe ears. Uses a doubly linked list via prev/next arrays for O(n) time.
///
/// Ported from Cherchi et al. C++ reference:
///   triangulation.cpp:917-970 (earcutLinear)
///   github.com/gcherchi/FastAndRobustMeshArrangements
///
/// Ref: Livesu & Cherchi 2022 "Deterministic Linear Time Constrained
///   Triangulation Using Simplified Earcut", Algorithm 1
///
/// `poly` — polygon vertex indices (into `all_verts`)
/// `all_verts` — coordinate array indexed by `poly` entries
/// `proj_axis` — which two coordinate axes to use for 2D projection
/// `orientation` — +1 for CCW, -1 for CW
pub fn earcut_linear(
    poly: &[usize],
    all_verts: &[[f64; 3]],
    proj_axis: (usize, usize),
    orientation: i32,
) -> Vec<[usize; 3]> {
    let size = poly.len();
    assert!(size >= 3, "polygon must have at least 3 vertices");

    if size == 3 {
        return vec![[poly[0], poly[1], poly[2]]];
    }

    let mut tris = Vec::new();

    // Doubly linked list
    let mut prev: Vec<usize> = (0..size)
        .map(|i| if i == 0 { size - 1 } else { i - 1 })
        .collect();
    let mut next: Vec<usize> = (0..size)
        .map(|i| if i == size - 1 { 0 } else { i + 1 })
        .collect();

    // Detect all safe ears: convex interior vertices (not endpoints of the constrained edge)
    let mut ears: Vec<usize> = Vec::with_capacity(size);
    let mut is_ear = vec![false; size];

    for curr in 1..size - 1 {
        let p0 = &all_verts[poly[prev[curr]]];
        let p1 = &all_verts[poly[curr]];
        let p2 = &all_verts[poly[next[curr]]];

        let check = orient2d_projected(p0, p1, p2, proj_axis);

        if prev[curr] != next[curr]
            && ((check > 0.0 && orientation > 0) || (check < 0.0 && orientation < 0))
        {
            ears.push(curr);
            is_ear[curr] = true;
        }
    }

    // Progressively clip ears
    let mut length = size;
    while let Some(curr) = ears.pop() {
        // Skip if this ear was already removed from the polygon
        if prev[next[curr]] != curr && next[prev[curr]] != curr {
            continue;
        }

        tris.push([poly[prev[curr]], poly[curr], poly[next[curr]]]);

        // Remove curr from polygon
        next[prev[curr]] = next[curr];
        prev[next[curr]] = prev[curr];

        length -= 1;
        if length < 3 {
            return tris;
        }

        // Check if prev[curr] has become a new ear
        let p = prev[curr];
        if !is_ear[p] && p != 0 {
            let p0 = &all_verts[poly[prev[p]]];
            let p1 = &all_verts[poly[p]];
            let p2 = &all_verts[poly[next[p]]];
            let check = orient2d_projected(p0, p1, p2, proj_axis);

            if prev[p] != next[p]
                && ((check > 0.0 && orientation > 0) || (check < 0.0 && orientation < 0))
            {
                ears.push(p);
                is_ear[p] = true;
            }
        }

        // Check if next[curr] has become a new ear
        let n = next[curr];
        if !is_ear[n] && n < size - 1 {
            let p0 = &all_verts[poly[prev[n]]];
            let p1 = &all_verts[poly[n]];
            let p2 = &all_verts[poly[next[n]]];
            let check = orient2d_projected(p0, p1, p2, proj_axis);

            if next[n] != prev[n]
                && ((check > 0.0 && orientation > 0) || (check < 0.0 && orientation < 0))
            {
                ears.push(n);
                is_ear[n] = true;
            }
        }
    }

    tris
}

/// Triangulate a single original triangle that has intersection points and
/// constraint segments. This is the main per-triangle entry point.
///
/// Steps:
/// 1. Build LocalMesh from the 3 original vertices
/// 2. Insert edge points (sorted along each edge) → split edges
/// 3. Insert interior points → split containing triangles
/// 4. For each constraint segment: add_constraint_segment (Algorithm 1)
/// 5. Collect active triangles as output
///
/// Ported from Cherchi et al. C++ reference:
///   triangulation.cpp:53-134 (triangulateSingleTriangle)
///   github.com/gcherchi/FastAndRobustMeshArrangements
///
/// Ref [#9] Cherchi 2020
///
/// `tri_global_verts` — the 3 global vertex indices of the original triangle
/// `edge_points` — for each of the 3 edges, the sorted intersection points
///                  (edge 0 = v0→v1, edge 1 = v1→v2, edge 2 = v2→v0)
/// `interior_points` — global indices of points inside the triangle
/// `segments` — constraint segments as pairs of global vertex indices
/// `all_verts` — the full shared vertex coordinate array
///
/// Returns sub-triangles as global vertex index triples.
pub fn triangulate_single_triangle(
    tri_global_verts: [usize; 3],
    edge_points: [&[usize]; 3],
    interior_points: &[usize],
    segments: &[[usize; 2]],
    all_verts: &[[f64; 3]],
) -> Vec<[usize; 3]> {
    let mut mesh = LocalMesh::new(
        tri_global_verts[0],
        tri_global_verts[1],
        tri_global_verts[2],
    );

    // Edge endpoints for the 3 edges:
    // edge 0: v0→v1, edge 1: v1→v2, edge 2: v2→v0
    let edge_endpoints = [
        (0usize, 1usize), // local indices for edge 0
        (1, 2),           // edge 1
        (2, 0),           // edge 2
    ];

    // Insert edge points by splitting edges in sequence
    for (edge_idx, points) in edge_points.iter().enumerate() {
        let (lv_start, lv_end) = edge_endpoints[edge_idx];

        // For each point on this edge, find the sub-edge it lies on and split it
        let mut prev_local = lv_start;
        for &gv in *points {
            let new_lv = mesh.add_vert(gv);
            // Find the edge from prev_local toward lv_end that we need to split
            let e_id = mesh
                .find_edge(prev_local, lv_end)
                .expect("edge should exist for edge point insertion");
            mesh.split_edge(e_id, new_lv);
            prev_local = new_lv;
        }
    }

    // Insert interior points by splitting their containing triangles
    for &gv in interior_points {
        let new_lv = mesh.add_vert(gv);
        // Find which active triangle contains this point
        if let Some(t_id) = find_containing_triangle(&mesh, new_lv, all_verts) {
            mesh.split_interior(t_id, new_lv);
        }
    }

    // Insert constraint segments
    for seg in segments {
        let lv0 = mesh
            .local_vert(seg[0])
            .expect("segment vertex should be in local mesh");
        let lv1 = mesh
            .local_vert(seg[1])
            .expect("segment vertex should be in local mesh");
        mesh.add_constraint_segment(lv0, lv1, all_verts);
    }

    mesh.active_tris()
}

/// Find the active triangle containing a local vertex (by point-in-triangle test).
///
/// Ported from Cherchi et al. C++ reference:
///   triangulation.cpp:459-469 (findContainingTriangle)
fn find_containing_triangle(mesh: &LocalMesh, lv: usize, all_verts: &[[f64; 3]]) -> Option<usize> {
    let proj = best_projection_axis(all_verts, mesh);
    let p = &all_verts[mesh.global_verts[lv]];

    for (t_id, tri) in mesh.tris.iter().enumerate() {
        if mesh.removed[t_id] {
            continue;
        }
        let a = &all_verts[mesh.global_verts[tri[0]]];
        let b = &all_verts[mesh.global_verts[tri[1]]];
        let c = &all_verts[mesh.global_verts[tri[2]]];

        // Point-in-triangle: check that p is on the same side of all 3 edges
        let d0 = orient2d_projected(a, b, p, proj);
        let d1 = orient2d_projected(b, c, p, proj);
        let d2 = orient2d_projected(c, a, p, proj);

        // Accept if all same sign (strictly inside) or any zero (on edge)
        let has_neg = d0 < 0.0 || d1 < 0.0 || d2 < 0.0;
        let has_pos = d0 > 0.0 || d1 > 0.0 || d2 > 0.0;
        if !(has_neg && has_pos) {
            return Some(t_id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shared test vertices: right triangle at origin in XY plane
    fn test_verts() -> Vec<[f64; 3]> {
        vec![
            [0.0, 0.0, 0.0], // 0
            [1.0, 0.0, 0.0], // 1
            [0.0, 1.0, 0.0], // 2
        ]
    }

    /// Test 1: LocalMesh adjacency after construction.
    /// Create LocalMesh with 3 vertices (triangle at (0,0,0), (1,0,0), (0,1,0)).
    /// Verify: 1 triangle, 3 edges, v2e has 2 edges per vertex, e2t has 1 tri per edge.
    #[test]
    fn test_local_mesh_adjacency() {
        let mesh = LocalMesh::new(0, 1, 2);

        // 1 active triangle
        assert_eq!(mesh.tris.len(), 1);
        assert_eq!(mesh.removed.len(), 1);
        assert!(!mesh.removed[0]);

        // 3 edges
        assert_eq!(mesh.edges.len(), 3);

        // 3 vertices
        assert_eq!(mesh.global_verts.len(), 3);

        // Each vertex touches exactly 2 edges
        assert_eq!(mesh.v2e.len(), 3);
        for adj in &mesh.v2e {
            assert_eq!(adj.len(), 2, "each vertex should touch 2 edges");
        }

        // Each edge is adjacent to exactly 1 triangle
        assert_eq!(mesh.e2t.len(), 3);
        for adj in &mesh.e2t {
            assert_eq!(adj.len(), 1, "each edge should touch 1 triangle");
        }

        // All constraint flags start as false
        assert_eq!(mesh.constrained.len(), 3);
        assert!(mesh.constrained.iter().all(|&c| !c));
    }

    /// Test 2: Edge split.
    /// Create LocalMesh triangle. Add a 4th vertex at midpoint of edge 0.
    /// Call split_edge. Verify: 2 active triangles, both reference the new vertex,
    /// adjacency consistent.
    #[test]
    fn test_local_mesh_edge_split() {
        let mut verts = test_verts();
        verts.push([0.5, 0.0, 0.0]); // midpoint of edge (0,0,0)-(1,0,0)

        let mut mesh = LocalMesh::new(0, 1, 2);
        let lv3 = mesh.add_vert(3);

        // Find the edge between local vertices 0 and 1 (global 0 and 1)
        let edge_id = mesh.find_edge(0, 1).expect("edge 0-1 should exist");
        mesh.split_edge(edge_id, lv3);

        // Should now have 2 active triangles (original removed, 2 new)
        let active = mesh.active_tris();
        assert_eq!(
            active.len(),
            2,
            "edge split on boundary should produce 2 triangles"
        );

        // Both triangles should reference the new vertex (global index 3)
        for tri in &active {
            assert!(
                tri.contains(&3),
                "each split triangle should contain the new vertex"
            );
        }
    }

    /// Test 3: Interior split.
    /// Create LocalMesh triangle. Add vertex at centroid. Call split_interior.
    /// Verify: 3 active triangles, all reference new vertex.
    #[test]
    fn test_local_mesh_interior_split() {
        let mut verts = test_verts();
        verts.push([1.0 / 3.0, 1.0 / 3.0, 0.0]); // centroid

        let mut mesh = LocalMesh::new(0, 1, 2);
        let lv3 = mesh.add_vert(3);

        mesh.split_interior(0, lv3);

        // Should now have 3 active triangles
        let active = mesh.active_tris();
        assert_eq!(active.len(), 3, "interior split should produce 3 triangles");

        // All triangles should reference the new vertex (global index 3)
        for tri in &active {
            assert!(
                tri.contains(&3),
                "each split triangle should contain the new vertex"
            );
        }
    }

    /// Test 4: Add constraint segment between two edge points.
    /// Create triangle, insert 2 points on different edges. Add constraint segment
    /// between them. Verify: the segment is now a mesh edge, constraint flag set.
    #[test]
    fn test_add_constraint_segment_simple() {
        let mut verts = test_verts();
        // Point on edge 0-1 (midpoint)
        verts.push([0.5, 0.0, 0.0]); // global 3
                                     // Point on edge 0-2 (midpoint)
        verts.push([0.0, 0.5, 0.0]); // global 4

        let mut mesh = LocalMesh::new(0, 1, 2);
        let lv3 = mesh.add_vert(3);
        let lv4 = mesh.add_vert(4);

        // Split edges to insert the points
        let e01 = mesh.find_edge(0, 1).expect("edge 0-1");
        mesh.split_edge(e01, lv3);
        let e02 = mesh.find_edge(0, 2).expect("edge 0-2");
        mesh.split_edge(e02, lv4);

        // Add constraint segment between the two edge points
        mesh.add_constraint_segment(lv3, lv4, &verts);

        // The constraint segment should now be a mesh edge
        let seg_edge = mesh
            .find_edge(lv3, lv4)
            .expect("constraint segment should be a mesh edge");
        assert!(
            mesh.constrained[seg_edge],
            "constraint flag should be set on the segment edge"
        );
    }

    /// Test 5: Full triangulate_single_triangle pipeline.
    /// Triangle with 2 edge points + 1 segment. Call triangulate_single_triangle.
    /// Verify: output has >1 sub-triangle, all sub-triangles use valid vertex indices.
    #[test]
    fn test_triangulate_single_triangle() {
        let mut verts = test_verts();
        // Point on edge 0→1
        verts.push([0.5, 0.0, 0.0]); // global 3
                                     // Point on edge 0→2
        verts.push([0.0, 0.5, 0.0]); // global 4

        let edge_points: [&[usize]; 3] = [
            &[3], // edge 0→1 has one intersection point
            &[],  // edge 1→2 has none
            &[4], // edge 2→0 has one intersection point
        ];
        let segments = [[3, 4]]; // constraint between the two edge points

        let result = triangulate_single_triangle([0, 1, 2], edge_points, &[], &segments, &verts);

        // Should produce more than 1 sub-triangle
        assert!(
            result.len() > 1,
            "triangulated result should have multiple sub-triangles, got {}",
            result.len()
        );

        // All vertex indices should be valid
        for tri in &result {
            for &vi in tri {
                assert!(vi < verts.len(), "vertex index {} out of range", vi);
            }
        }
    }

    /// Test 6: earcut_linear on a convex polygon (square).
    /// 4 vertices forming a square. Should produce 2 triangles.
    #[test]
    fn test_earcut_linear_convex() {
        let verts = vec![
            [0.0, 0.0, 0.0], // 0
            [1.0, 0.0, 0.0], // 1
            [1.0, 1.0, 0.0], // 2
            [0.0, 1.0, 0.0], // 3
        ];
        let poly = vec![0, 1, 2, 3];

        let tris = earcut_linear(&poly, &verts, (0, 1), 1);

        assert_eq!(tris.len(), 2, "square should produce 2 triangles");

        // All triangles should use valid vertex indices from the polygon
        for tri in &tris {
            for &vi in tri {
                assert!(poly.contains(&vi), "triangle vertex {} not in polygon", vi);
            }
        }
    }

    /// Test 7: earcut_linear on a concave L-shaped polygon.
    /// 6 vertices forming an L-shape. Should produce 4 triangles.
    #[test]
    fn test_earcut_linear_concave() {
        // L-shaped polygon (CCW):
        //  (0,2)---(1,2)
        //    |       |
        //  (0,1)---(1,1)---(2,1)
        //                    |
        //          (1,0)---(2,0)
        let verts = vec![
            [0.0, 0.0, 0.0], // 0 — unused padding for index alignment
            [1.0, 0.0, 0.0], // 1
            [2.0, 0.0, 0.0], // 2
            [2.0, 1.0, 0.0], // 3
            [1.0, 1.0, 0.0], // 4
            [1.0, 2.0, 0.0], // 5
            [0.0, 2.0, 0.0], // 6
            [0.0, 1.0, 0.0], // 7
        ];
        // L-shape polygon indices (CCW)
        let poly = vec![1, 2, 3, 4, 5, 6, 7, 4];
        // Wait — that's not a simple polygon. Let me use the correct L:
        // Actually the L-shape as a simple polygon:
        //   7---6
        //   |   |
        //   4---5
        //       |
        //   1---2---3
        //           |
        // Hmm, let's use a clean 6-vertex L:
        let verts_l = vec![
            [0.0, 0.0, 0.0], // 0
            [2.0, 0.0, 0.0], // 1
            [2.0, 1.0, 0.0], // 2
            [1.0, 1.0, 0.0], // 3
            [1.0, 2.0, 0.0], // 4
            [0.0, 2.0, 0.0], // 5
        ];
        let poly_l = vec![0, 1, 2, 3, 4, 5];

        let tris = earcut_linear(&poly_l, &verts_l, (0, 1), 1);

        // 6-vertex simple polygon → 4 triangles
        assert_eq!(
            tris.len(),
            4,
            "L-shaped 6-vertex polygon should produce 4 triangles"
        );

        // Verify correct orientation: all triangles should have positive area in XY
        for tri in &tris {
            let a = verts_l[tri[0]];
            let b = verts_l[tri[1]];
            let c = verts_l[tri[2]];
            let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            assert!(
                cross > 0.0,
                "triangle {:?} should have positive (CCW) orientation, got cross={}",
                tri,
                cross
            );
        }
    }
}
