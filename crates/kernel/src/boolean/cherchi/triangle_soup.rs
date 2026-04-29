// MIT License
//
// Copyright (c) 2020 Gianmarco Cherchi, Marco Livesu, Riccardo Scateni e Marco Attene
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! TriangleSoup — input mesh wrapper for the Cherchi 2020 §5 mesh
//! arrangement algorithm.
//!
//! Ported from triangle_soup.h + triangle_soup.cpp in
//! github.com/gcherchi/FastAndRobustMeshArrangements
//! MIT License (c) 2020 Cherchi, Livesu, Scateni, Attene

use std::collections::HashMap;

use super::common::{int_to_plane, Plane};
use crate::boolean::indirect_predicates::ImplicitPoint;

/// Canonical edge representation: (min, max) vertex pair.
/// Ported from triangle_soup.h:54
type Edge = (usize, usize);

/// TriangleSoup — the global input mesh for arrangement.
///
/// Stores vertices as `ImplicitPoint` (Explicit for input vertices, LPI/TPI
/// for intersection points), triangles as flat index triples, per-triangle
/// labels (u32 bitset), per-triangle projection planes, and a global edge map
/// for edge ↔ ID lookup.
///
/// Ported from triangle_soup.h:59-151
#[allow(dead_code)]
pub(crate) struct TriangleSoup {
    /// Vertex implicit points. Index = vertex ID.
    /// Input vertices are `ImplicitPoint::Explicit`, intersection points are LPI/TPI.
    pub vertices: Vec<ImplicitPoint>,
    /// Flat triangle indices: tri t_id has verts at [3*t_id, 3*t_id+1, 3*t_id+2].
    pub triangles: Vec<usize>,
    /// Per-triangle label bitset.
    pub tri_labels: Vec<u32>,
    /// Per-triangle projection plane for 2D orientation tests.
    tri_planes: Vec<Plane>,
    /// Edge list: edge_id → (v0, v1) with v0 < v1.
    edges: Vec<Edge>,
    /// Map from canonical edge (v0, v1) → edge ID.
    edge_map: HashMap<Edge, usize>,
    /// Number of original vertices before implicit points are added.
    num_orig_vtxs: usize,
    /// Number of original triangles.
    num_orig_tris: usize,
    /// Jolly (utility) points for non-coplanar handling.
    jolly_points: Vec<[f64; 3]>,
}

#[allow(dead_code)]
impl TriangleSoup {
    /// Construct a TriangleSoup from coordinates, triangle indices, and labels.
    ///
    /// `multiplier` is the scaling factor for predicate stability (from `compute_multiplier`).
    ///
    /// Ported from triangle_soup.h:63-67, triangle_soup.cpp:43-129 (sequential path)
    pub fn new(
        coords: Vec<[f64; 3]>,
        triangles: Vec<usize>,
        labels: Vec<u32>,
        multiplier: f64,
    ) -> Self {
        let num_orig_vtxs = coords.len();
        let num_orig_tris = triangles.len() / 3;

        // Scale vertices by multiplier and wrap in ImplicitPoint::Explicit
        let mut vertices: Vec<ImplicitPoint> = coords
            .into_iter()
            .map(|c| {
                ImplicitPoint::Explicit([c[0] * multiplier, c[1] * multiplier, c[2] * multiplier])
            })
            .collect();

        let edge_capacity = num_orig_vtxs + num_orig_tris;
        let mut edges = Vec::with_capacity(edge_capacity);
        let mut edge_map = HashMap::with_capacity(edge_capacity);

        // Compute per-triangle projection planes
        // Ported from triangle_soup.cpp:107-113
        let mut tri_planes = Vec::with_capacity(num_orig_tris);
        for t_id in 0..num_orig_tris {
            let v0 = vertices[triangles[3 * t_id]].materialize().unwrap();
            let v1 = vertices[triangles[3 * t_id + 1]].materialize().unwrap();
            let v2 = vertices[triangles[3 * t_id + 2]].materialize().unwrap();
            let plane_idx = max_component_in_triangle_normal(v0, v1, v2);
            tri_planes.push(int_to_plane(plane_idx));
        }

        // Build edge map from triangles
        /// Ported from triangle_soup.cpp:117-125
        for t_id in 0..num_orig_tris {
            let v0_id = triangles[3 * t_id];
            let v1_id = triangles[3 * t_id + 1];
            let v2_id = triangles[3 * t_id + 2];

            add_edge_to(&mut edges, &mut edge_map, v0_id, v1_id);
            add_edge_to(&mut edges, &mut edge_map, v1_id, v2_id);
            add_edge_to(&mut edges, &mut edge_map, v2_id, v0_id);
        }

        // Init jolly points
        let jolly_points = init_jolly_points(multiplier);

        // Append jolly points as vertices (5 points, as in C++)
        for jp in &jolly_points {
            vertices.push(ImplicitPoint::Explicit(*jp));
        }

        Self {
            vertices,
            triangles,
            tri_labels: labels,
            tri_planes,
            edges,
            edge_map,
            num_orig_vtxs,
            num_orig_tris,
            jolly_points,
        }
    }

    /// Number of vertices (including implicit points added later).
    /// Ported from triangle_soup.cpp:135-138
    pub fn num_verts(&self) -> usize {
        self.vertices.len()
    }

    /// Number of triangles.
    /// Ported from triangle_soup.cpp:142-145
    pub fn num_tris(&self) -> usize {
        self.triangles.len() / 3
    }

    /// Number of edges in the global edge map.
    /// Ported from triangle_soup.cpp:149-152
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    /// Number of original triangles (before subdivision).
    /// Ported from triangle_soup.cpp:157-159
    pub fn num_orig_triangles(&self) -> usize {
        self.num_orig_tris
    }

    /// Number of original vertices (before implicit points).
    pub fn num_orig_verts(&self) -> usize {
        self.num_orig_vtxs
    }

    /// Get the ImplicitPoint for a vertex.
    pub fn implicit_point(&self, v_id: usize) -> &ImplicitPoint {
        debug_assert!(v_id < self.vertices.len(), "vtx id out of range");
        &self.vertices[v_id]
    }

    /// Get materialized vertex coordinates by ID.
    /// Ported from triangle_soup.cpp:163-167
    pub fn vert(&self, v_id: usize) -> [f64; 3] {
        debug_assert!(v_id < self.vertices.len(), "vtx id out of range");
        self.vertices[v_id].materialize().unwrap_or([0.0, 0.0, 0.0])
    }

    /// Get X coordinate of an original vertex.
    /// Ported from triangle_soup.cpp:179-183
    pub fn vert_x(&self, v_id: usize) -> f64 {
        debug_assert!(v_id < self.num_orig_vtxs, "vtx id out of range");
        self.vertices[v_id].materialize().unwrap_or([0.0; 3])[0]
    }

    /// Get Y coordinate of an original vertex.
    /// Ported from triangle_soup.cpp:187-191
    pub fn vert_y(&self, v_id: usize) -> f64 {
        debug_assert!(v_id < self.num_orig_vtxs, "vtx id out of range");
        self.vertices[v_id].materialize().unwrap_or([0.0; 3])[1]
    }

    /// Get Z coordinate of an original vertex.
    /// Ported from triangle_soup.cpp:195-199
    pub fn vert_z(&self, v_id: usize) -> f64 {
        debug_assert!(v_id < self.num_orig_vtxs, "vtx id out of range");
        self.vertices[v_id].materialize().unwrap_or([0.0; 3])[2]
    }

    /// Add an implicit vertex (intersection point) as an ImplicitPoint, return its new ID.
    /// Ported from triangle_soup.cpp:203-207
    pub fn add_impl_point(&mut self, point: ImplicitPoint) -> usize {
        self.vertices.push(point);
        self.vertices.len() - 1
    }

    /// Add an implicit vertex with explicit coordinates, return its new ID.
    /// Convenience wrapper for backward compatibility.
    pub fn add_impl_vert(&mut self, coords: [f64; 3]) -> usize {
        self.add_impl_point(ImplicitPoint::Explicit(coords))
    }

    /// Look up edge ID from two vertex IDs. Returns None if edge not found.
    /// Ported from triangle_soup.cpp:213-219
    pub fn edge_id(&self, v0_id: usize, v1_id: usize) -> Option<usize> {
        let e = unique_edge(v0_id, v1_id);
        self.edge_map.get(&e).copied()
    }

    /// Get vertex IDs for an edge.
    /// Ported from triangle_soup.h (edges vector access)
    pub fn edge_verts(&self, e_id: usize) -> (usize, usize) {
        debug_assert!(e_id < self.edges.len(), "e_id out of range");
        self.edges[e_id]
    }

    /// Get the edge opposite to a given vertex in a triangle.
    /// Ported from triangle_soup.cpp:241-254
    pub fn edge_opposite_to_vert(&self, t_id: usize, v_id: usize) -> usize {
        debug_assert!(t_id < self.num_tris(), "t_id out of range");
        debug_assert!(v_id < self.num_verts(), "vtx id out of range");

        let v0 = self.tri_vert_id(t_id, 0);
        let v1 = self.tri_vert_id(t_id, 1);
        let v2 = self.tri_vert_id(t_id, 2);

        let e_id = if v0 == v_id {
            self.edge_id(v1, v2)
        } else if v1 == v_id {
            self.edge_id(v0, v2)
        } else if v2 == v_id {
            self.edge_id(v0, v1)
        } else {
            None
        };

        e_id.expect("Opposite edge not found")
    }

    /// Add an edge to the global edge map (dedup by canonical pair).
    /// Ported from triangle_soup.cpp:258-266
    pub fn add_edge(&mut self, v0_id: usize, v1_id: usize) {
        add_edge_to(&mut self.edges, &mut self.edge_map, v0_id, v1_id);
    }

    /// Get the flat triangles index vector.
    /// Ported from triangle_soup.cpp:272-275
    pub fn tris_vector(&self) -> &[usize] {
        &self.triangles
    }

    /// Get vertex IDs for triangle t_id as a slice of 3.
    /// Ported from triangle_soup.cpp:279-283
    pub fn tri(&self, t_id: usize) -> &[usize] {
        debug_assert!(t_id < self.num_tris(), "t_id out of range");
        &self.triangles[3 * t_id..3 * t_id + 3]
    }

    /// Get vertex ID at offset `off` (0, 1, or 2) in triangle t_id.
    /// Ported from triangle_soup.cpp:287-291
    pub fn tri_vert_id(&self, t_id: usize, off: usize) -> usize {
        debug_assert!(t_id < self.num_tris(), "t_id out of range");
        self.triangles[3 * t_id + off]
    }

    /// Get materialized vertex coordinates for triangle t_id at offset off.
    /// Ported from triangle_soup.cpp:295-299
    pub fn tri_vert(&self, t_id: usize, off: usize) -> [f64; 3] {
        debug_assert!(t_id < self.num_tris(), "t_id out of range");
        self.vertices[self.triangles[3 * t_id + off]]
            .materialize()
            .unwrap_or([0.0, 0.0, 0.0])
    }

    /// Get edge ID for the edge at offset `off` in triangle t_id.
    /// Edge off connects vertex off and vertex (off+1)%3.
    /// Ported from triangle_soup.cpp:309-319
    pub fn tri_edge_id(&self, t_id: usize, off: usize) -> usize {
        debug_assert!(t_id < self.num_tris(), "t_id out of range");
        let v0 = self.triangles[3 * t_id + off];
        let v1 = self.triangles[3 * t_id + (off + 1) % 3];
        self.edge_id(v0, v1).expect("no triangle edge found")
    }

    /// Get the projection plane for triangle t_id.
    /// Ported from triangle_soup.cpp:323-327
    pub fn tri_plane(&self, t_id: usize) -> Plane {
        debug_assert!(t_id < self.num_tris(), "t_id out of range");
        self.tri_planes[t_id]
    }

    /// Check if triangle t_id contains vertex v_id.
    /// Ported from triangle_soup.cpp:331-340
    pub fn tri_contains_vert(&self, t_id: usize, v_id: usize) -> bool {
        debug_assert!(t_id < self.num_tris(), "t_id out of range");
        self.triangles[3 * t_id] == v_id
            || self.triangles[3 * t_id + 1] == v_id
            || self.triangles[3 * t_id + 2] == v_id
    }

    /// Check if triangle t_id contains the edge (ev0_id, ev1_id).
    /// Ported from triangle_soup.cpp:344-347
    pub fn tri_contains_edge(&self, t_id: usize, ev0_id: usize, ev1_id: usize) -> bool {
        self.tri_contains_vert(t_id, ev0_id) && self.tri_contains_vert(t_id, ev1_id)
    }

    /// Get the label bitset for triangle t_id.
    /// Ported from triangle_soup.cpp:351-355
    pub fn tri_label(&self, t_id: usize) -> u32 {
        debug_assert!(t_id < self.num_tris(), "t_id out of range");
        self.tri_labels[t_id]
    }

    /// Get jolly point at offset (0..4).
    /// Ported from triangle_soup.cpp:361-365
    pub fn jolly_point(&self, off: usize) -> &[f64; 3] {
        debug_assert!(off < 5, "jolly point id out of range");
        &self.jolly_points[off]
    }
}

/// Create a canonical edge representation: (min, max).
/// Ported from triangle_soup.cpp:393-397
fn unique_edge(v0_id: usize, v1_id: usize) -> Edge {
    if v0_id < v1_id {
        (v0_id, v1_id)
    } else {
        (v1_id, v0_id)
    }
}

/// Add an edge to edges vec + edge_map (dedup by canonical pair).
/// Ported from triangle_soup.cpp:258-266
fn add_edge_to(edges: &mut Vec<Edge>, edge_map: &mut HashMap<Edge, usize>, v0: usize, v1: usize) {
    let e = unique_edge(v0, v1);
    let next_id = edges.len();
    if let std::collections::hash_map::Entry::Vacant(entry) = edge_map.entry(e) {
        entry.insert(next_id);
        edges.push(e);
    }
}

/// Compute the axis with the largest component of the triangle normal.
/// Returns 0 (X), 1 (Y), or 2 (Z).
///
/// Replaces `genericPoint::maxComponentInTriangleNormal` from the C++ code.
/// Ported from triangle_soup.cpp:65-67, implicit_point.h
fn max_component_in_triangle_normal(v0: [f64; 3], v1: [f64; 3], v2: [f64; 3]) -> u32 {
    // Cross product of (v1 - v0) × (v2 - v0)
    let ux = v1[0] - v0[0];
    let uy = v1[1] - v0[1];
    let uz = v1[2] - v0[2];
    let vx = v2[0] - v0[0];
    let vy = v2[1] - v0[1];
    let vz = v2[2] - v0[2];

    let nx = (uy * vz - uz * vy).abs();
    let ny = (uz * vx - ux * vz).abs();
    let nz = (ux * vy - uy * vx).abs();

    if nx >= ny && nx >= nz {
        0 // X axis → YZ plane
    } else if ny >= nx && ny >= nz {
        1 // Y axis → ZX plane
    } else {
        2 // Z axis → XY plane
    }
}

/// Initialize 5 jolly (utility) points for non-coplanar handling.
/// These are vertices of a regular tetrahedron + one extra point, scaled by multiplier.
///
/// Ported from triangle_soup.cpp:382-389
fn init_jolly_points(multiplier: f64) -> Vec<[f64; 3]> {
    vec![
        [
            0.94280904158 * multiplier,
            0.0 * multiplier,
            -0.333333333 * multiplier,
        ],
        [
            -0.47140452079 * multiplier,
            0.81649658092 * multiplier,
            -0.333333333 * multiplier,
        ],
        [
            -0.47140452079 * multiplier,
            -0.81649658092 * multiplier,
            -0.333333333 * multiplier,
        ],
        [0.0 * multiplier, 0.0 * multiplier, 1.0 * multiplier],
        [multiplier, 0.0, 0.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: two triangles sharing an edge (a quad split diagonally).
    fn two_triangle_soup() -> TriangleSoup {
        // v0=(0,0,0), v1=(1,0,0), v2=(1,1,0), v3=(0,1,0)
        // tri0: v0,v1,v2  tri1: v0,v2,v3
        let coords = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let tris = vec![0, 1, 2, 0, 2, 3];
        let labels = vec![1, 2];
        TriangleSoup::new(coords, tris, labels, 1.0)
    }

    #[test]
    fn test_triangle_soup_basic() {
        let ts = two_triangle_soup();
        // 4 original verts + 5 jolly = 9
        assert_eq!(ts.num_orig_verts(), 4);
        assert_eq!(ts.num_verts(), 9);
        assert_eq!(ts.num_tris(), 2);
        assert_eq!(ts.num_orig_triangles(), 2);
        // 5 unique edges: (0,1),(1,2),(0,2),(2,3),(0,3)
        assert_eq!(ts.num_edges(), 5);
    }

    #[test]
    fn test_triangle_soup_edge_map() {
        let ts = two_triangle_soup();

        // Shared edge (0,2) should exist
        let shared = ts.edge_id(0, 2);
        assert!(shared.is_some());
        // Same edge reversed
        assert_eq!(ts.edge_id(2, 0), shared);

        // All edges from tri 0
        let e01 = ts.edge_id(0, 1);
        let e12 = ts.edge_id(1, 2);
        let e02 = ts.edge_id(0, 2);
        assert!(e01.is_some());
        assert!(e12.is_some());
        assert!(e02.is_some());
        // All different
        assert_ne!(e01, e12);
        assert_ne!(e01, e02);
        assert_ne!(e12, e02);

        // tri_edge_id
        let te0 = ts.tri_edge_id(0, 0); // edge v0→v1
        assert_eq!(te0, e01.unwrap());
    }

    #[test]
    fn test_triangle_soup_planes() {
        let ts = two_triangle_soup();
        // Both triangles in XY plane → normal along Z → plane = XY
        assert_eq!(ts.tri_plane(0), Plane::XY);
        assert_eq!(ts.tri_plane(1), Plane::XY);
    }

    #[test]
    fn test_triangle_soup_labels() {
        let ts = two_triangle_soup();
        assert_eq!(ts.tri_label(0), 1);
        assert_eq!(ts.tri_label(1), 2);
    }

    #[test]
    fn test_triangle_soup_contains() {
        let ts = two_triangle_soup();
        assert!(ts.tri_contains_vert(0, 0));
        assert!(ts.tri_contains_vert(0, 1));
        assert!(ts.tri_contains_vert(0, 2));
        assert!(!ts.tri_contains_vert(0, 3));
        assert!(ts.tri_contains_edge(0, 0, 1));
        assert!(!ts.tri_contains_edge(0, 0, 3));
    }

    #[test]
    fn test_triangle_soup_edge_opposite() {
        let ts = two_triangle_soup();
        // In tri 0 (v0,v1,v2), edge opposite to v0 is (v1,v2)
        let e_id = ts.edge_opposite_to_vert(0, 0);
        let (a, b) = ts.edge_verts(e_id);
        assert!((a == 1 && b == 2) || (a == 2 && b == 1));
    }

    #[test]
    fn test_triangle_soup_jolly_points() {
        let ts = two_triangle_soup();
        // 5 jolly points
        let jp0 = ts.jolly_point(0);
        assert!((jp0[0] - 0.94280904158).abs() < 1e-9);
        let jp4 = ts.jolly_point(4);
        assert!((jp4[0] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_triangle_soup_multiplier() {
        let coords = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let tris = vec![0, 1, 2];
        let labels = vec![1];
        let ts = TriangleSoup::new(coords, tris, labels, 2.0);
        // Vertex 1 should be scaled: (1*2, 0, 0) = (2, 0, 0)
        assert!((ts.vert_x(0) - 0.0).abs() < 1e-9);
        // vertex 1 is at index 1 in the original range
        let v1_coords = ts.vert(1);
        assert!((v1_coords[0] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_max_component_in_triangle_normal() {
        // Triangle in XY plane → normal along Z → component 2
        assert_eq!(
            max_component_in_triangle_normal([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            2
        );
        // Triangle in YZ plane → normal along X → component 0
        assert_eq!(
            max_component_in_triangle_normal([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            0
        );
        // Triangle in ZX plane → normal along Y → component 1
        assert_eq!(
            max_component_in_triangle_normal([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            1
        );
    }

    /// A-02 RED test (audit `docs/audits/cherchi_port_audit.md` Cluster I).
    ///
    /// Demonstrates that the current inexact f64 cross-product disagrees
    /// with Shewchuk-exact `geometry_predicates::orient2d` on the choice
    /// of triangle-normal max-magnitude axis. The Plane returned drives 2D
    /// projection for ALL downstream `orient2d` calls on this triangle, so
    /// picking the wrong axis flips orientation signs and breaks the
    /// arrangement algorithm's topological invariants.
    ///
    /// Fixture (verified in cargo's debug profile, no FMA):
    ///   `v0 = (0, 0, 0)`, `v1 = (2*0.11, 3*0.19, 7*0.1)`, `v2 = (3*0.19, 7*0.1, 2*0.13)`.
    ///   The integer-times-fraction construction guarantees specific f64
    ///   bit patterns so the test reproduces deterministically across
    ///   platforms.
    ///
    ///   Computed `(v1 - v0) × (v2 - v0)` magnitudes:
    ///     - f64 path: `|nx| = |ny| = 3.41800000000000103739e-1` (the two
    ///       products `uy*vz` and `uz*vy` happen to round to identical f64
    ///       values), `|nz| = 1.70900000000000051870e-1`. f64 reports
    ///       `|nx| >= |ny| >= |nz|`, breaks the tie in favor of axis 0 (X),
    ///       so returns `0`.
    ///     - Shewchuk-exact `orient2d` on three orthogonal projections:
    ///       `|dx| = 3.41800000000000048228e-1`, `|dy| = 3.41800000000000103739e-1`
    ///       — exact arithmetic distinguishes them by 1 ULP. So `|dy| > |dx|`
    ///       and a Shewchuk-exact-faithful predicate must return `1` (Y axis).
    ///
    /// **Pre-fix behavior:** returns 0 (X axis) — wrong axis selection driven
    /// by an f64 rounding tie that exact arithmetic resolves.
    ///
    /// **Post-fix expected behavior (this test asserts):** returns 1 (Y axis)
    /// to match the Shewchuk-exact answer. The audit's recommended fix is to
    /// port `genericPoint::maxComponentInTriangleNormal` from C++
    /// `implicit_point.hpp:937-1029` — a cascaded filtered+exact predicate.
    /// The C++ filtered path uses an `8.88395e-016 * max_var^2` epsilon
    /// commit-test; for this fixture, `max_var ≈ 0.7` so `epsilon ≈ 4.4e-16`,
    /// far below the cross magnitudes of `~3.4e-1`. The C++ filtered path
    /// would commit. C++'s filtered path uses different edge ordering than
    /// current Rust (`(v3-v2) × (v2-v1)` vs Rust's `(v1-v0) × (v2-v0)`),
    /// which may resolve the f64 tie differently. If the C++ cascade
    /// committed in the filtered path with the same answer as current Rust
    /// (axis 0), this test would correctly fail and force the implementer
    /// to honor the exact answer instead.
    ///
    /// **Note on the stronger-vs-weaker correctness target:** this test
    /// asserts the **stronger** target — match Shewchuk-exact `orient2d`
    /// regardless of whether C++'s filtered path would commit. Per
    /// `feedback_yang_only.md`, the goal is faithful Cherchi 2020 §3
    /// implementation; per analytical-primacy invariant A15.6, indirect
    /// predicates are the foundation of the Yang pipeline. If the
    /// implementer can defend a weaker C++-cascade-faithful target on
    /// audit-grounds and the test fails, that's a legitimate signal to
    /// the team-lead — but they must NOT silently accept the f64 tie-break
    /// answer (which is what current code does and what the audit flags
    /// as the bug).
    ///
    /// Refs: audit A-02; C++ `genericPoint::maxComponentInTriangleNormal`
    /// (`implicit_point.hpp:1024-1029`) and its filtered+exact helpers
    /// (`implicit_point.hpp:937-1022`); `geometry_predicates::orient2d`
    /// usage pattern in `mesh_arrangement.rs:600-604`.
    #[test]
    fn test_max_component_in_triangle_normal_handles_f64_rounding() {
        // Integer-times-fraction construction for deterministic f64 bit patterns.
        let v0 = [0.0_f64, 0.0, 0.0];
        let v1 = [2.0_f64 * 0.11, 3.0 * 0.19, 7.0 * 0.1];
        let v2 = [3.0_f64 * 0.19, 7.0 * 0.1, 2.0 * 0.13];

        // Sanity: confirm the f64 path produces an exact tie between |nx| and
        // |ny|, with both well above |nz|. This documents the pre-fix
        // tie-break behavior (returns 0 because nx is checked first with `>=`).
        let ux = v1[0] - v0[0];
        let uy = v1[1] - v0[1];
        let uz = v1[2] - v0[2];
        let vx = v2[0] - v0[0];
        let vy = v2[1] - v0[1];
        let vz = v2[2] - v0[2];
        let nx_inexact = (uy * vz - uz * vy).abs();
        let ny_inexact = (uz * vx - ux * vz).abs();
        let nz_inexact = (ux * vy - uy * vx).abs();
        assert_eq!(
            nx_inexact, ny_inexact,
            "fixture sanity (debug profile, no FMA): f64 |nx| and |ny| must \
             round to identical values — this is the rounding-induced tie \
             that pre-fix breaks toward X. If this assertion fires, the f64 \
             rounding behavior on this platform differs from what was \
             verified during fixture construction (likely FMA enabled)."
        );
        assert!(
            nx_inexact > nz_inexact,
            "fixture sanity: f64 |nx| must exceed |nz| (so X-vs-Z is unambiguous)"
        );

        // Independently confirm the exact answer differs from the f64 answer,
        // by computing both and comparing. We compute the exact answer via
        // `geometry_predicates::orient2d` on the three orthogonal projections
        // (the same approach used in `mesh_arrangement.rs:600-604`).
        let dx =
            geometry_predicates::orient2d([v0[1], v0[2]], [v1[1], v1[2]], [v2[1], v2[2]]).abs();
        let dy =
            geometry_predicates::orient2d([v0[0], v0[2]], [v1[0], v1[2]], [v2[0], v2[2]]).abs();
        assert!(
            dy > dx,
            "fixture sanity: Shewchuk-exact |dy| must exceed |dx| — this is \
             the disagreement with f64 (where they tie). If this assertion \
             fires, geometry_predicates' orient2d behavior on this platform \
             does not match what was verified during fixture construction."
        );

        // The actual red-vs-green assertion. Pre-fix: f64 returns axis 0
        // (X) due to the rounding tie. Post-fix (Shewchuk-exact): exact
        // arithmetic resolves the tie in favor of Y, so the function must
        // return 1.
        let result = max_component_in_triangle_normal(v0, v1, v2);
        assert_eq!(
            result, 1,
            "post-fix Shewchuk-exact predicates resolve the f64 tie between \
             |nx| and |ny| in favor of Y — so the max-component axis is 1 \
             (Y), not 0 (X). The current inexact implementation falsely \
             reports a tie between |nx| and |ny| because the products \
             `uy*vz` and `uz*vy` round to identical f64 values, and the \
             tie-break picks 0. This is a Cluster I predicate-kernel bug \
             per audit A-02."
        );

        // Regression guard: axis-aligned triangles (the existing test cases)
        // must continue to return the right axis under the new exact
        // implementation. Re-asserted here so a single test run catches both
        // the new bug fix and basic-case regressions.
        assert_eq!(
            max_component_in_triangle_normal([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            2,
            "regression guard: XY-plane triangle → normal along Z → axis 2"
        );
        assert_eq!(
            max_component_in_triangle_normal([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            0,
            "regression guard: YZ-plane triangle → normal along X → axis 0"
        );
        assert_eq!(
            max_component_in_triangle_normal([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            1,
            "regression guard: ZX-plane triangle → normal along Y → axis 1"
        );
    }
}
