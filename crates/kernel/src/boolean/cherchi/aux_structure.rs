// MIT License
//
// Copyright (c) 2022 G. Cherchi, M. Livesu, R. Scateni, M. Attene and F. Pellacini
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

//! AuxiliaryStructure — intersection bookkeeping for Cherchi mesh arrangement.
//!
//! Tracks per-triangle interior intersection points, per-edge sorted intersection
//! points, per-triangle constraint segments, and coplanar triangle relationships.
//!
//! Ported from Cherchi aux_structure.h + aux_structure.cpp
//! MIT License (c) 2022 Cherchi, Livesu, Scateni, Attene, Pellacini

use std::collections::HashMap;

use super::triangle_soup::TriangleSoup;

/// A pair of vertex IDs, used for segments and edge keys.
type UIPair = (usize, usize);

/// AuxiliaryStructure — coordinates intersection data across triangles.
///
/// Ported from aux_structure.h:134-199
#[allow(dead_code)]
pub(crate) struct AuxiliaryStructure {
    /// Number of original vertices (from TriangleSoup).
    num_original_vtx: usize,
    /// Number of original triangles.
    num_original_tris: usize,

    /// List of intersecting triangle pairs.
    /// Ported from aux_structure.h:187
    intersection_list: Vec<(usize, usize)>,

    /// Per-triangle list of coplanar triangle IDs.
    /// Ported from aux_structure.h:188
    coplanar_tris: Vec<Vec<usize>>,

    /// Per-triangle list of interior intersection point IDs.
    /// Ported from aux_structure.h:189
    tri2pts: Vec<Vec<usize>>,

    /// Per-edge list of intersection point IDs.
    /// Ported from aux_structure.h:190
    edge2pts: Vec<Vec<usize>>,

    /// Per-triangle list of constraint segments (as vertex-pair).
    /// Ported from aux_structure.h:191
    tri2segs: Vec<Vec<UIPair>>,

    /// Map from segment (canonical pair) → list of triangle IDs that share it.
    /// Ported from aux_structure.h:192
    seg2tris: HashMap<UIPair, Vec<usize>>,

    /// Flag per triangle: has any intersection been recorded?
    /// Ported from aux_structure.h:193
    tri_has_intersections: Vec<bool>,

    /// Sorted vertex map: materialized coordinates → vertex ID (for dedup).
    /// In C++, v_map is btree_map<genericPoint*, uint> where genericPoint::operator<
    /// compares geometric content via exact predicates. We approximate this by
    /// keying on bit-exact materialized coordinates.
    /// Ported from aux_structure.h:194 (v_map)
    v_map: HashMap<[u64; 3], usize>,

    /// Visited polygon pockets for dedup during classification.
    /// Ported from aux_structure.h:196
    pockets_map: HashMap<Vec<usize>, usize>,
}

#[allow(dead_code)]
impl AuxiliaryStructure {
    /// Create an empty AuxiliaryStructure.
    /// Ported from aux_structure.h:138
    pub fn new() -> Self {
        Self {
            num_original_vtx: 0,
            num_original_tris: 0,
            intersection_list: Vec::new(),
            coplanar_tris: Vec::new(),
            tri2pts: Vec::new(),
            edge2pts: Vec::new(),
            tri2segs: Vec::new(),
            seg2tris: HashMap::new(),
            tri_has_intersections: Vec::new(),
            v_map: HashMap::new(),
            pockets_map: HashMap::new(),
        }
    }

    /// Initialize from a TriangleSoup, allocating per-triangle and per-edge vectors.
    ///
    /// Ported from aux_structure.cpp:45-64
    pub fn init_from_triangle_soup(&mut self, ts: &TriangleSoup) {
        self.num_original_vtx = ts.num_verts();
        self.num_original_tris = ts.num_tris();

        self.coplanar_tris.resize(ts.num_tris(), Vec::new());
        self.tri2pts.resize(ts.num_tris(), Vec::new());
        self.edge2pts.resize(ts.num_edges(), Vec::new());
        self.tri2segs.resize(ts.num_tris(), Vec::new());
        self.tri_has_intersections.resize(ts.num_tris(), false);

        // Populate v_map with original vertex coordinates.
        // In C++ this inserts genericPoint* → uint; here we key on bit-exact coords.
        for v_id in 0..ts.num_verts() {
            let coords = ts.vert(v_id);
            let key = [
                coords[0].to_bits(),
                coords[1].to_bits(),
                coords[2].to_bits(),
            ];
            self.v_map.insert(key, v_id);
        }
    }

    /// Get mutable reference to intersection list.
    /// Ported from aux_structure.cpp:68-71
    pub fn intersection_list_mut(&mut self) -> &mut Vec<(usize, usize)> {
        &mut self.intersection_list
    }

    /// Get reference to intersection list.
    /// Ported from aux_structure.cpp:75-78
    pub fn intersection_list(&self) -> &[(usize, usize)] {
        &self.intersection_list
    }

    /// Add a vertex ID as an interior intersection point of triangle t_id.
    /// Returns true if newly added, false if already present.
    ///
    /// Ported from aux_structure.cpp:82-90
    pub fn add_vertex_in_triangle(&mut self, t_id: usize, v_id: usize) -> bool {
        debug_assert!(t_id < self.tri2pts.len());
        let points = &mut self.tri2pts[t_id];
        if points.contains(&v_id) {
            return false;
        }
        points.push(v_id);
        true
    }

    /// Add a vertex ID as an intersection point on edge e_id.
    /// Returns true if newly added, false if already present.
    ///
    /// Ported from aux_structure.cpp:94-102
    pub fn add_vertex_in_edge(&mut self, e_id: usize, v_id: usize) -> bool {
        debug_assert!(e_id < self.edge2pts.len());
        let points = &mut self.edge2pts[e_id];
        if points.contains(&v_id) {
            return false;
        }
        points.push(v_id);
        true
    }

    /// Add a constraint segment to triangle t_id.
    /// Returns true if newly added, false if already present.
    ///
    /// Ported from aux_structure.cpp:106-115
    pub fn add_segment_in_triangle(&mut self, t_id: usize, seg: UIPair) -> bool {
        debug_assert!(t_id < self.tri2segs.len());
        let key_seg = unique_pair(seg);
        let segments = &mut self.tri2segs[t_id];
        if segments.contains(&key_seg) {
            return false;
        }
        segments.push(key_seg);
        true
    }

    /// Record that segment `seg` belongs to triangles tA_id and tB_id.
    ///
    /// Ported from aux_structure.cpp:119-133
    pub fn add_triangles_in_segment(&mut self, seg: UIPair, t_a_id: usize, t_b_id: usize) {
        let key_seg = unique_pair(seg);
        let tris = self.seg2tris.entry(key_seg).or_insert_with(Vec::new);
        if t_a_id == t_b_id {
            if !tris.contains(&t_a_id) {
                tris.push(t_a_id);
            }
        } else {
            if !tris.contains(&t_a_id) {
                tris.push(t_a_id);
            }
            if !tris.contains(&t_b_id) {
                tris.push(t_b_id);
            }
        }
    }

    /// Split a segment into two sub-segments at a midpoint, copying triangle associations.
    ///
    /// Ported from aux_structure.cpp:137-144
    pub fn split_segment_in_sub_segments(
        &mut self,
        orig_v0: usize,
        orig_v1: usize,
        midpoint: usize,
    ) {
        let orig_seg = unique_pair((orig_v0, orig_v1));
        let tris = self.seg2tris.get(&orig_seg).cloned().unwrap_or_default();
        let sub_seg0 = unique_pair((orig_v0, midpoint));
        let sub_seg1 = unique_pair((midpoint, orig_v1));
        self.seg2tris.insert(sub_seg0, tris.clone());
        self.seg2tris.insert(sub_seg1, tris);
    }

    /// Record that triangles ta and tb are coplanar (symmetric).
    ///
    /// Ported from aux_structure.cpp:148-157
    pub fn add_coplanar_triangles(&mut self, ta: usize, tb: usize) {
        debug_assert!(ta != tb);
        debug_assert!(ta < self.coplanar_tris.len() && tb < self.coplanar_tris.len());
        self.coplanar_tris[ta].push(tb);
        self.coplanar_tris[tb].push(ta);
    }

    /// Get the list of coplanar triangle IDs for triangle t_id.
    ///
    /// Ported from aux_structure.cpp:161-165
    pub fn coplanar_triangles(&self, t_id: usize) -> &[usize] {
        debug_assert!(t_id < self.coplanar_tris.len());
        &self.coplanar_tris[t_id]
    }

    /// Check if triangle t_id has any coplanar neighbors.
    ///
    /// Ported from aux_structure.cpp:169-173
    pub fn triangle_has_coplanars(&self, t_id: usize) -> bool {
        debug_assert!(t_id < self.coplanar_tris.len());
        !self.coplanar_tris[t_id].is_empty()
    }

    /// Mark triangle t_id as having intersections.
    ///
    /// Ported from aux_structure.cpp:177-181
    pub fn set_triangle_has_intersections(&mut self, t_id: usize) {
        debug_assert!(t_id < self.tri_has_intersections.len());
        self.tri_has_intersections[t_id] = true;
    }

    /// Check if triangle t_id has any intersections.
    ///
    /// Ported from aux_structure.cpp:185-189
    pub fn triangle_has_intersections(&self, t_id: usize) -> bool {
        debug_assert!(t_id < self.tri_has_intersections.len());
        self.tri_has_intersections[t_id]
    }

    /// Get interior intersection points for triangle t_id.
    ///
    /// Ported from aux_structure.cpp:194-198
    pub fn triangle_points_list(&self, t_id: usize) -> &[usize] {
        debug_assert!(t_id < self.tri2pts.len());
        &self.tri2pts[t_id]
    }

    /// Get intersection points on edge e_id.
    /// THE critical query for edge conformality.
    ///
    /// Ported from aux_structure.cpp:202-206
    pub fn edge_points_list(&self, e_id: usize) -> &[usize] {
        debug_assert!(e_id < self.edge2pts.len());
        &self.edge2pts[e_id]
    }

    /// Get constraint segments for triangle t_id.
    ///
    /// Ported from aux_structure.cpp:210-214
    pub fn triangle_segments_list(&self, t_id: usize) -> &[UIPair] {
        debug_assert!(t_id < self.tri2segs.len());
        &self.tri2segs[t_id]
    }

    /// Get triangles sharing a given segment.
    ///
    /// Ported from aux_structure.cpp:218-226
    pub fn segment_triangles_list(&self, seg: UIPair) -> &[usize] {
        let key_seg = unique_pair(seg);
        self.seg2tris
            .get(&key_seg)
            .map(|v| v.as_slice())
            .expect("segment not found in seg2tris")
    }

    /// Add a vertex to the sorted vertex map, keyed by materialized coordinates.
    /// Returns (vertex_id, is_new). If a geometrically identical point already
    /// exists, returns the existing vertex ID.
    ///
    /// In C++, v_map is btree_map<genericPoint*, uint> where operator< compares
    /// geometric content via exact predicates. We approximate this by keying on
    /// bit-exact materialized coordinates.
    ///
    /// Ported from aux_structure.cpp:230-236
    pub fn add_vertex_in_sorted_list(&mut self, coords: [f64; 3], pos: usize) -> (usize, bool) {
        let key = [
            coords[0].to_bits(),
            coords[1].to_bits(),
            coords[2].to_bits(),
        ];
        match self.v_map.entry(key) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(pos);
                (pos, true)
            }
            std::collections::hash_map::Entry::Occupied(e) => (*e.get(), false),
        }
    }

    /// Add a visited polygon pocket. Returns -1 (as None) if not present yet,
    /// or the stored position (as Some) if already visited.
    ///
    /// Ported from aux_structure.cpp:240-247
    pub fn add_visited_polygon_pocket(&mut self, polygon: &[usize], pos: usize) -> Option<usize> {
        let key = polygon.to_vec();
        match self.pockets_map.entry(key) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(pos);
                None // not present yet
            }
            std::collections::hash_map::Entry::Occupied(e) => Some(*e.get()),
        }
    }

    /// Get reference to the vertex map.
    /// Ported from aux_structure.h:178
    pub fn get_vmap(&self) -> &HashMap<[u64; 3], usize> {
        &self.v_map
    }

    /// Get mutable reference to the vertex map.
    /// Ported from aux_structure.h:179
    pub fn get_vmap_mut(&mut self) -> &mut HashMap<[u64; 3], usize> {
        &mut self.v_map
    }
}

/// Canonical pair: (min, max).
/// Ported from aux_structure.cpp:251-255
fn unique_pair(uip: UIPair) -> UIPair {
    if uip.0 < uip.1 {
        uip
    } else {
        (uip.1, uip.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_triangle_soup() -> TriangleSoup {
        // Two triangles sharing edge (0,2)
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
    fn test_aux_structure_init() {
        let ts = make_triangle_soup();
        let mut aux = AuxiliaryStructure::new();
        aux.init_from_triangle_soup(&ts);

        // Should have 2 triangles worth of structures
        assert_eq!(aux.triangle_points_list(0).len(), 0);
        assert_eq!(aux.triangle_points_list(1).len(), 0);
        assert_eq!(aux.triangle_segments_list(0).len(), 0);
        assert!(!aux.triangle_has_intersections(0));
        assert!(!aux.triangle_has_intersections(1));
        // edge2pts should have entries for all 5 edges
        assert_eq!(aux.edge_points_list(0).len(), 0);
    }

    #[test]
    fn test_aux_structure_edge_points() {
        let ts = make_triangle_soup();
        let mut aux = AuxiliaryStructure::new();
        aux.init_from_triangle_soup(&ts);

        // Find the shared edge (0,2)
        let shared_e = ts.edge_id(0, 2).unwrap();

        // Add intersection points to shared edge
        assert!(aux.add_vertex_in_edge(shared_e, 100));
        assert!(aux.add_vertex_in_edge(shared_e, 101));
        assert!(aux.add_vertex_in_edge(shared_e, 102));
        // Duplicate should return false
        assert!(!aux.add_vertex_in_edge(shared_e, 100));

        let pts = aux.edge_points_list(shared_e);
        assert_eq!(pts.len(), 3);
        assert!(pts.contains(&100));
        assert!(pts.contains(&101));
        assert!(pts.contains(&102));
    }

    #[test]
    fn test_aux_structure_triangle_points() {
        let ts = make_triangle_soup();
        let mut aux = AuxiliaryStructure::new();
        aux.init_from_triangle_soup(&ts);

        assert!(aux.add_vertex_in_triangle(0, 50));
        assert!(aux.add_vertex_in_triangle(0, 51));
        assert!(!aux.add_vertex_in_triangle(0, 50)); // dup

        assert_eq!(aux.triangle_points_list(0).len(), 2);
    }

    #[test]
    fn test_aux_structure_segments() {
        let ts = make_triangle_soup();
        let mut aux = AuxiliaryStructure::new();
        aux.init_from_triangle_soup(&ts);

        // Add segment to triangle 0
        assert!(aux.add_segment_in_triangle(0, (10, 20)));
        assert!(!aux.add_segment_in_triangle(0, (20, 10))); // canonical dup
        assert_eq!(aux.triangle_segments_list(0).len(), 1);

        // Add triangles to segment
        aux.add_triangles_in_segment((10, 20), 0, 1);
        let tris = aux.segment_triangles_list((10, 20));
        assert_eq!(tris.len(), 2);
        assert!(tris.contains(&0));
        assert!(tris.contains(&1));
    }

    #[test]
    fn test_aux_structure_coplanar() {
        let ts = make_triangle_soup();
        let mut aux = AuxiliaryStructure::new();
        aux.init_from_triangle_soup(&ts);

        assert!(!aux.triangle_has_coplanars(0));
        aux.add_coplanar_triangles(0, 1);
        assert!(aux.triangle_has_coplanars(0));
        assert!(aux.triangle_has_coplanars(1));
        assert_eq!(aux.coplanar_triangles(0), &[1]);
        assert_eq!(aux.coplanar_triangles(1), &[0]);
    }

    #[test]
    fn test_aux_structure_has_intersections() {
        let ts = make_triangle_soup();
        let mut aux = AuxiliaryStructure::new();
        aux.init_from_triangle_soup(&ts);

        assert!(!aux.triangle_has_intersections(0));
        aux.set_triangle_has_intersections(0);
        assert!(aux.triangle_has_intersections(0));
        assert!(!aux.triangle_has_intersections(1));
    }

    #[test]
    fn test_aux_structure_split_segment() {
        let ts = make_triangle_soup();
        let mut aux = AuxiliaryStructure::new();
        aux.init_from_triangle_soup(&ts);

        aux.add_triangles_in_segment((10, 20), 0, 1);
        aux.split_segment_in_sub_segments(10, 20, 15);

        // Both sub-segments should have the same triangle associations
        let sub0 = aux.segment_triangles_list((10, 15));
        let sub1 = aux.segment_triangles_list((15, 20));
        assert_eq!(sub0.len(), 2);
        assert_eq!(sub1.len(), 2);
    }

    #[test]
    fn test_aux_structure_visited_pocket() {
        let mut aux = AuxiliaryStructure::new();
        let polygon = vec![1, 2, 3];

        // First time → None
        assert_eq!(aux.add_visited_polygon_pocket(&polygon, 42), None);
        // Second time → Some(42)
        assert_eq!(aux.add_visited_polygon_pocket(&polygon, 99), Some(42));
    }

    #[test]
    fn test_unique_pair() {
        assert_eq!(unique_pair((3, 7)), (3, 7));
        assert_eq!(unique_pair((7, 3)), (3, 7));
        assert_eq!(unique_pair((5, 5)), (5, 5));
    }
}
