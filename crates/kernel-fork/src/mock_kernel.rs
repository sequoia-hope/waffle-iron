//! MockKernel — deterministic test double implementing Kernel + KernelIntrospect.
//!
//! Produces synthetic topology with predictable entity counts and signatures.
//! Used by feature-engine and modeling-ops for unit testing.

use crate::traits::{Kernel, KernelIntrospect};
use crate::types::*;
use std::collections::HashMap;

/// Face definition tuple: (edge_indices, normal, centroid, area, surface_type).
type FaceDef<'a> = (Vec<usize>, [f64; 3], [f64; 3], f64, &'a str);

/// A mock vertex with known position.
#[derive(Debug, Clone)]
struct MockVertex {
    id: KernelId,
    position: [f64; 3],
}

/// A mock edge with known endpoints.
#[derive(Debug, Clone)]
struct MockEdge {
    id: KernelId,
    start: KernelId,
    end: KernelId,
    length: f64,
}

/// A mock face with known properties.
#[derive(Debug, Clone)]
struct MockFace {
    id: KernelId,
    edges: Vec<KernelId>,
    normal: [f64; 3],
    centroid: [f64; 3],
    area: f64,
    surface_type: String,
}

/// A synthetic solid with deterministic topology.
#[derive(Debug, Clone)]
struct MockSolid {
    vertices: Vec<MockVertex>,
    edges: Vec<MockEdge>,
    faces: Vec<MockFace>,
}

/// Deterministic test double for the geometry kernel.
/// Implements both Kernel and KernelIntrospect.
pub struct MockKernel {
    next_id: u64,
    next_handle: u64,
    solids: HashMap<u64, MockSolid>,
    /// Tracks faces created by make_faces_from_profiles for subsequent extrude.
    standalone_faces: HashMap<u64, MockFace>,
}

impl MockKernel {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            next_handle: 1,
            solids: HashMap::new(),
            standalone_faces: HashMap::new(),
        }
    }

    fn alloc_id(&mut self) -> KernelId {
        let id = KernelId(self.next_id);
        self.next_id += 1;
        id
    }

    fn alloc_handle(&mut self) -> KernelSolidHandle {
        let h = KernelSolidHandle(self.next_handle);
        self.next_handle += 1;
        h
    }

    /// Create a box solid with 8 vertices, 12 edges, 6 faces.
    /// Origin at (0,0,0), extending to (w,h,d).
    fn make_box_solid(&mut self, w: f64, h: f64, d: f64) -> (KernelSolidHandle, MockSolid) {
        // 8 vertices of a box
        let positions = [
            [0.0, 0.0, 0.0],
            [w, 0.0, 0.0],
            [w, h, 0.0],
            [0.0, h, 0.0],
            [0.0, 0.0, d],
            [w, 0.0, d],
            [w, h, d],
            [0.0, h, d],
        ];

        let verts: Vec<MockVertex> = positions
            .iter()
            .map(|&pos| MockVertex {
                id: self.alloc_id(),
                position: pos,
            })
            .collect();

        // 12 edges of a box: 4 bottom, 4 top, 4 vertical
        let edge_pairs = [
            // Bottom face edges
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            // Top face edges
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            // Vertical edges
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];

        let edges: Vec<MockEdge> = edge_pairs
            .iter()
            .map(|&(si, ei)| {
                let sp = positions[si];
                let ep = positions[ei];
                let dx = ep[0] - sp[0];
                let dy = ep[1] - sp[1];
                let dz = ep[2] - sp[2];
                let length = (dx * dx + dy * dy + dz * dz).sqrt();
                MockEdge {
                    id: self.alloc_id(),
                    start: verts[si].id,
                    end: verts[ei].id,
                    length,
                }
            })
            .collect();

        // 6 faces: bottom (z=0), top (z=d), front (y=0), back (y=h), left (x=0), right (x=w)
        let face_defs: Vec<FaceDef<'_>> = vec![
            // Bottom face (z=0): edges 0,1,2,3
            (
                vec![0, 1, 2, 3],
                [0.0, 0.0, -1.0],
                [w / 2.0, h / 2.0, 0.0],
                w * h,
                "planar",
            ),
            // Top face (z=d): edges 4,5,6,7
            (
                vec![4, 5, 6, 7],
                [0.0, 0.0, 1.0],
                [w / 2.0, h / 2.0, d],
                w * h,
                "planar",
            ),
            // Front face (y=0): edges 0,9,4,8
            (
                vec![0, 9, 4, 8],
                [0.0, -1.0, 0.0],
                [w / 2.0, 0.0, d / 2.0],
                w * d,
                "planar",
            ),
            // Back face (y=h): edges 2,11,6,10
            (
                vec![2, 11, 6, 10],
                [0.0, 1.0, 0.0],
                [w / 2.0, h, d / 2.0],
                w * d,
                "planar",
            ),
            // Left face (x=0): edges 3,8,7,11
            (
                vec![3, 8, 7, 11],
                [-1.0, 0.0, 0.0],
                [0.0, h / 2.0, d / 2.0],
                h * d,
                "planar",
            ),
            // Right face (x=w): edges 1,10,5,9
            (
                vec![1, 10, 5, 9],
                [1.0, 0.0, 0.0],
                [w, h / 2.0, d / 2.0],
                h * d,
                "planar",
            ),
        ];

        let faces: Vec<MockFace> = face_defs
            .into_iter()
            .map(|(edge_indices, normal, centroid, area, stype)| MockFace {
                id: self.alloc_id(),
                edges: edge_indices.iter().map(|&i| edges[i].id).collect(),
                normal,
                centroid,
                area,
                surface_type: stype.to_string(),
            })
            .collect();

        let handle = self.alloc_handle();
        let solid = MockSolid {
            vertices: verts,
            edges,
            faces,
        };

        (handle, solid)
    }

    /// Merge two solids for boolean union: combine all topology with new IDs.
    fn merge_solids(&mut self, a: &MockSolid, b: &MockSolid) -> MockSolid {
        let mut vertices = Vec::new();
        let mut edges = Vec::new();
        let mut faces = Vec::new();

        // Re-ID everything from both solids
        let mut id_map: HashMap<KernelId, KernelId> = HashMap::new();

        for v in &a.vertices {
            let new_id = self.alloc_id();
            id_map.insert(v.id, new_id);
            vertices.push(MockVertex {
                id: new_id,
                position: v.position,
            });
        }
        for v in &b.vertices {
            let new_id = self.alloc_id();
            id_map.insert(v.id, new_id);
            vertices.push(MockVertex {
                id: new_id,
                position: v.position,
            });
        }

        for e in &a.edges {
            let new_id = self.alloc_id();
            id_map.insert(e.id, new_id);
            edges.push(MockEdge {
                id: new_id,
                start: id_map[&e.start],
                end: id_map[&e.end],
                length: e.length,
            });
        }
        for e in &b.edges {
            let new_id = self.alloc_id();
            id_map.insert(e.id, new_id);
            edges.push(MockEdge {
                id: new_id,
                start: id_map[&e.start],
                end: id_map[&e.end],
                length: e.length,
            });
        }

        for f in &a.faces {
            let new_id = self.alloc_id();
            id_map.insert(f.id, new_id);
            faces.push(MockFace {
                id: new_id,
                edges: f.edges.iter().map(|eid| id_map[eid]).collect(),
                normal: f.normal,
                centroid: f.centroid,
                area: f.area,
                surface_type: f.surface_type.clone(),
            });
        }
        for f in &b.faces {
            let new_id = self.alloc_id();
            id_map.insert(f.id, new_id);
            faces.push(MockFace {
                id: new_id,
                edges: f.edges.iter().map(|eid| id_map[eid]).collect(),
                normal: f.normal,
                centroid: f.centroid,
                area: f.area,
                surface_type: f.surface_type.clone(),
            });
        }

        MockSolid {
            vertices,
            edges,
            faces,
        }
    }

    /// Generate a deterministic box mesh: 2 triangles per face = 12 triangles for 6 faces.
    fn tessellate_box(solid: &MockSolid) -> RenderMesh {
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        let mut face_ranges = Vec::new();

        // For each face, generate 2 triangles (a quad split).
        // We'll use the face's stored properties to generate plausible geometry.
        for face in &solid.faces {
            let start_index = indices.len() as u32;
            let base_vertex = (vertices.len() / 3) as u32;

            // Generate a simple quad from centroid, normal, and area
            let c = face.centroid;
            let n = face.normal;
            let half = (face.area.sqrt()) / 2.0;

            // Choose two tangent vectors orthogonal to normal
            let (u, v) = tangent_vectors(n);

            // 4 corners of the quad
            let corners = [
                [
                    c[0] - u[0] * half - v[0] * half,
                    c[1] - u[1] * half - v[1] * half,
                    c[2] - u[2] * half - v[2] * half,
                ],
                [
                    c[0] + u[0] * half - v[0] * half,
                    c[1] + u[1] * half - v[1] * half,
                    c[2] + u[2] * half - v[2] * half,
                ],
                [
                    c[0] + u[0] * half + v[0] * half,
                    c[1] + u[1] * half + v[1] * half,
                    c[2] + u[2] * half + v[2] * half,
                ],
                [
                    c[0] - u[0] * half + v[0] * half,
                    c[1] - u[1] * half + v[1] * half,
                    c[2] - u[2] * half + v[2] * half,
                ],
            ];

            for corner in &corners {
                vertices.extend_from_slice(&[corner[0] as f32, corner[1] as f32, corner[2] as f32]);
                normals.extend_from_slice(&[n[0] as f32, n[1] as f32, n[2] as f32]);
            }

            // Two triangles: 0-1-2 and 0-2-3
            indices.extend_from_slice(&[
                base_vertex,
                base_vertex + 1,
                base_vertex + 2,
                base_vertex,
                base_vertex + 2,
                base_vertex + 3,
            ]);

            let end_index = indices.len() as u32;
            face_ranges.push(FaceRange {
                face_id: face.id,
                start_index,
                end_index,
            });
        }

        RenderMesh {
            vertices,
            normals,
            indices,
            face_ranges,
        }
    }
}

impl Default for MockKernel {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute two tangent vectors orthogonal to a normal.
fn tangent_vectors(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    // Pick a vector not parallel to n
    let up = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };

    // u = normalize(up × n)
    let u = cross(up, n);
    let u_len = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    let u = [u[0] / u_len, u[1] / u_len, u[2] / u_len];

    // v = n × u
    let v = cross(n, u);
    (u, v)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

impl Kernel for MockKernel {
    fn extrude_face(
        &mut self,
        face: KernelId,
        direction: [f64; 3],
        depth: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        // Check if we have a standalone face to extrude
        let mock_face = self
            .standalone_faces
            .remove(&face.0)
            .ok_or(KernelError::EntityNotFound { id: face })?;

        // Compute extrusion dimensions from face area and depth
        let side = mock_face.area.sqrt();
        let dir_len = (direction[0] * direction[0]
            + direction[1] * direction[1]
            + direction[2] * direction[2])
            .sqrt();
        let _norm_dir = if dir_len > 1e-12 {
            [
                direction[0] / dir_len,
                direction[1] / dir_len,
                direction[2] / dir_len,
            ]
        } else {
            [0.0, 0.0, 1.0]
        };

        // Produce a box-like solid with 8V, 12E, 6F
        let (handle, solid) = self.make_box_solid(side, side, depth);
        self.solids.insert(handle.id(), solid);
        Ok(handle)
    }

    fn revolve_face(
        &mut self,
        face: KernelId,
        _axis_origin: [f64; 3],
        _axis_direction: [f64; 3],
        _angle: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        // Verify face exists
        if !self.standalone_faces.contains_key(&face.0) {
            return Err(KernelError::EntityNotFound { id: face });
        }
        self.standalone_faces.remove(&face.0);

        // Produce a simplified solid for revolve: use box topology as approximation
        let (handle, solid) = self.make_box_solid(1.0, 1.0, 1.0);
        self.solids.insert(handle.id(), solid);
        Ok(handle)
    }

    fn boolean_union(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        let solid_a = self
            .solids
            .get(&a.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(a.id()),
            })?
            .clone();
        let solid_b = self
            .solids
            .get(&b.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(b.id()),
            })?
            .clone();

        let merged = self.merge_solids(&solid_a, &solid_b);
        let handle = self.alloc_handle();
        self.solids.insert(handle.id(), merged);
        Ok(handle)
    }

    fn boolean_subtract(
        &mut self,
        a: &KernelSolidHandle,
        _b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        // For mock: return a copy of solid A with re-allocated IDs
        let solid_a = self
            .solids
            .get(&a.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(a.id()),
            })?
            .clone();

        // Re-ID all entities to simulate new kernel output
        let empty = MockSolid {
            vertices: Vec::new(),
            edges: Vec::new(),
            faces: Vec::new(),
        };
        let result = self.merge_solids(&solid_a, &empty);
        let handle = self.alloc_handle();
        self.solids.insert(handle.id(), result);
        Ok(handle)
    }

    fn boolean_intersect(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        // For mock: return a small box representing the intersection
        let _solid_a = self
            .solids
            .get(&a.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(a.id()),
            })?;
        let _solid_b = self
            .solids
            .get(&b.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(b.id()),
            })?;

        let (handle, solid) = self.make_box_solid(0.5, 0.5, 0.5);
        self.solids.insert(handle.id(), solid);
        Ok(handle)
    }

    fn fillet_edges(
        &mut self,
        solid: &KernelSolidHandle,
        edges: &[KernelId],
        radius: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        if radius <= 0.0 {
            return Err(KernelError::FilletFailed {
                reason: "radius must be positive".to_string(),
            });
        }
        let source = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?
            .clone();

        // Validate that all requested edges exist
        let all_edge_ids: Vec<KernelId> = source.edges.iter().map(|e| e.id).collect();
        for eid in edges {
            if !all_edge_ids.contains(eid) {
                return Err(KernelError::FilletFailed {
                    reason: format!("edge {:?} not found in solid", eid),
                });
            }
        }

        // For each filleted edge: replace edge with a cylindrical face,
        // add 2 new edges (fillet boundaries) and 2 new vertices.
        // Topology: V+2n, E-n+2n=E+n, F+n where n = edges.len()
        let mut id_map: HashMap<KernelId, KernelId> = HashMap::new();

        // Re-ID existing vertices
        let mut new_vertices = Vec::new();
        for v in &source.vertices {
            let new_id = self.alloc_id();
            id_map.insert(v.id, new_id);
            new_vertices.push(MockVertex {
                id: new_id,
                position: v.position,
            });
        }

        // Re-ID existing edges, skipping filleted ones
        let filleted_set: std::collections::HashSet<KernelId> = edges.iter().copied().collect();
        let mut new_edges = Vec::new();
        for e in &source.edges {
            let new_id = self.alloc_id();
            id_map.insert(e.id, new_id);
            if !filleted_set.contains(&e.id) {
                new_edges.push(MockEdge {
                    id: new_id,
                    start: id_map[&e.start],
                    end: id_map[&e.end],
                    length: e.length,
                });
            }
        }

        // Re-ID existing faces (with updated edge refs)
        let mut new_faces = Vec::new();
        for f in &source.faces {
            let new_id = self.alloc_id();
            id_map.insert(f.id, new_id);
            // Replace filleted edge refs with placeholder; trimmed faces keep other edges
            let face_edges: Vec<KernelId> = f
                .edges
                .iter()
                .filter(|eid| !filleted_set.contains(eid))
                .map(|eid| id_map[eid])
                .collect();
            new_faces.push(MockFace {
                id: new_id,
                edges: face_edges,
                normal: f.normal,
                centroid: f.centroid,
                area: f.area, // trimmed area is approximate
                surface_type: f.surface_type.clone(),
            });
        }

        // Add fillet geometry for each filleted edge
        for orig_eid in edges {
            // Find the original edge to compute fillet geometry
            let orig_edge = source.edges.iter().find(|e| e.id == *orig_eid).unwrap();
            let sv = source
                .vertices
                .iter()
                .find(|v| v.id == orig_edge.start)
                .unwrap();
            let ev = source
                .vertices
                .iter()
                .find(|v| v.id == orig_edge.end)
                .unwrap();

            // Two new vertices at fillet tangent points (offset from original edge endpoints)
            let v1 = MockVertex {
                id: self.alloc_id(),
                position: [
                    sv.position[0] + radius * 0.01,
                    sv.position[1] + radius * 0.01,
                    sv.position[2],
                ],
            };
            let v2 = MockVertex {
                id: self.alloc_id(),
                position: [
                    ev.position[0] + radius * 0.01,
                    ev.position[1] + radius * 0.01,
                    ev.position[2],
                ],
            };

            // Two new edges connecting fillet face to adjacent faces
            let e1 = MockEdge {
                id: self.alloc_id(),
                start: id_map[&orig_edge.start],
                end: v1.id,
                length: radius,
            };
            let e2 = MockEdge {
                id: self.alloc_id(),
                start: id_map[&orig_edge.end],
                end: v2.id,
                length: radius,
            };

            // Fillet face (cylindrical)
            let centroid = [
                (sv.position[0] + ev.position[0]) / 2.0,
                (sv.position[1] + ev.position[1]) / 2.0,
                (sv.position[2] + ev.position[2]) / 2.0,
            ];
            let fillet_face = MockFace {
                id: self.alloc_id(),
                edges: vec![e1.id, e2.id],
                normal: [0.0, 0.0, 1.0], // approximate
                centroid,
                area: orig_edge.length * radius * std::f64::consts::FRAC_PI_2,
                surface_type: "cylindrical".to_string(),
            };

            new_vertices.push(v1);
            new_vertices.push(v2);
            new_edges.push(e1);
            new_edges.push(e2);
            new_faces.push(fillet_face);
        }

        let handle = self.alloc_handle();
        self.solids.insert(
            handle.id(),
            MockSolid {
                vertices: new_vertices,
                edges: new_edges,
                faces: new_faces,
            },
        );
        Ok(handle)
    }

    fn chamfer_edges(
        &mut self,
        solid: &KernelSolidHandle,
        edges: &[KernelId],
        distance: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        if distance <= 0.0 {
            return Err(KernelError::Other {
                message: "chamfer distance must be positive".to_string(),
            });
        }
        let source = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?
            .clone();

        // Validate edges exist
        let all_edge_ids: Vec<KernelId> = source.edges.iter().map(|e| e.id).collect();
        for eid in edges {
            if !all_edge_ids.contains(eid) {
                return Err(KernelError::Other {
                    message: format!("edge {:?} not found in solid", eid),
                });
            }
        }

        // Chamfer: same topology change as fillet but with planar chamfer face
        let mut id_map: HashMap<KernelId, KernelId> = HashMap::new();
        let filleted_set: std::collections::HashSet<KernelId> = edges.iter().copied().collect();

        let mut new_vertices = Vec::new();
        for v in &source.vertices {
            let new_id = self.alloc_id();
            id_map.insert(v.id, new_id);
            new_vertices.push(MockVertex {
                id: new_id,
                position: v.position,
            });
        }

        let mut new_edges = Vec::new();
        for e in &source.edges {
            let new_id = self.alloc_id();
            id_map.insert(e.id, new_id);
            if !filleted_set.contains(&e.id) {
                new_edges.push(MockEdge {
                    id: new_id,
                    start: id_map[&e.start],
                    end: id_map[&e.end],
                    length: e.length,
                });
            }
        }

        let mut new_faces = Vec::new();
        for f in &source.faces {
            let new_id = self.alloc_id();
            id_map.insert(f.id, new_id);
            let face_edges: Vec<KernelId> = f
                .edges
                .iter()
                .filter(|eid| !filleted_set.contains(eid))
                .map(|eid| id_map[eid])
                .collect();
            new_faces.push(MockFace {
                id: new_id,
                edges: face_edges,
                normal: f.normal,
                centroid: f.centroid,
                area: f.area,
                surface_type: f.surface_type.clone(),
            });
        }

        for orig_eid in edges {
            let orig_edge = source.edges.iter().find(|e| e.id == *orig_eid).unwrap();

            let v1 = MockVertex {
                id: self.alloc_id(),
                position: [
                    source
                        .vertices
                        .iter()
                        .find(|v| v.id == orig_edge.start)
                        .unwrap()
                        .position[0],
                    source
                        .vertices
                        .iter()
                        .find(|v| v.id == orig_edge.start)
                        .unwrap()
                        .position[1],
                    source
                        .vertices
                        .iter()
                        .find(|v| v.id == orig_edge.start)
                        .unwrap()
                        .position[2]
                        + distance,
                ],
            };
            let v2 = MockVertex {
                id: self.alloc_id(),
                position: [
                    source
                        .vertices
                        .iter()
                        .find(|v| v.id == orig_edge.end)
                        .unwrap()
                        .position[0],
                    source
                        .vertices
                        .iter()
                        .find(|v| v.id == orig_edge.end)
                        .unwrap()
                        .position[1],
                    source
                        .vertices
                        .iter()
                        .find(|v| v.id == orig_edge.end)
                        .unwrap()
                        .position[2]
                        + distance,
                ],
            };

            let e1 = MockEdge {
                id: self.alloc_id(),
                start: id_map[&orig_edge.start],
                end: v1.id,
                length: distance,
            };
            let e2 = MockEdge {
                id: self.alloc_id(),
                start: id_map[&orig_edge.end],
                end: v2.id,
                length: distance,
            };

            let centroid = [
                (source
                    .vertices
                    .iter()
                    .find(|v| v.id == orig_edge.start)
                    .unwrap()
                    .position[0]
                    + source
                        .vertices
                        .iter()
                        .find(|v| v.id == orig_edge.end)
                        .unwrap()
                        .position[0])
                    / 2.0,
                (source
                    .vertices
                    .iter()
                    .find(|v| v.id == orig_edge.start)
                    .unwrap()
                    .position[1]
                    + source
                        .vertices
                        .iter()
                        .find(|v| v.id == orig_edge.end)
                        .unwrap()
                        .position[1])
                    / 2.0,
                (source
                    .vertices
                    .iter()
                    .find(|v| v.id == orig_edge.start)
                    .unwrap()
                    .position[2]
                    + source
                        .vertices
                        .iter()
                        .find(|v| v.id == orig_edge.end)
                        .unwrap()
                        .position[2])
                    / 2.0,
            ];

            let chamfer_face = MockFace {
                id: self.alloc_id(),
                edges: vec![e1.id, e2.id],
                normal: [0.0, 0.0, 1.0],
                centroid,
                area: orig_edge.length * distance * std::f64::consts::SQRT_2,
                surface_type: "planar".to_string(),
            };

            new_vertices.push(v1);
            new_vertices.push(v2);
            new_edges.push(e1);
            new_edges.push(e2);
            new_faces.push(chamfer_face);
        }

        let handle = self.alloc_handle();
        self.solids.insert(
            handle.id(),
            MockSolid {
                vertices: new_vertices,
                edges: new_edges,
                faces: new_faces,
            },
        );
        Ok(handle)
    }

    fn shell(
        &mut self,
        solid: &KernelSolidHandle,
        faces_to_remove: &[KernelId],
        thickness: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        if thickness <= 0.0 {
            return Err(KernelError::ShellFailed {
                reason: "thickness must be positive".to_string(),
            });
        }
        let source = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?
            .clone();

        // Validate that all faces to remove exist
        let all_face_ids: Vec<KernelId> = source.faces.iter().map(|f| f.id).collect();
        for fid in faces_to_remove {
            if !all_face_ids.contains(fid) {
                return Err(KernelError::ShellFailed {
                    reason: format!("face {:?} not found in solid", fid),
                });
            }
        }

        let remove_set: std::collections::HashSet<KernelId> =
            faces_to_remove.iter().copied().collect();

        // Shell: keep outer faces (minus removed ones), add offset inner faces.
        // For each kept face, create an offset inner face.
        let mut id_map: HashMap<KernelId, KernelId> = HashMap::new();

        // Re-ID outer vertices
        let mut new_vertices = Vec::new();
        for v in &source.vertices {
            let new_id = self.alloc_id();
            id_map.insert(v.id, new_id);
            new_vertices.push(MockVertex {
                id: new_id,
                position: v.position,
            });
        }

        // Re-ID outer edges
        let mut new_edges = Vec::new();
        for e in &source.edges {
            let new_id = self.alloc_id();
            id_map.insert(e.id, new_id);
            new_edges.push(MockEdge {
                id: new_id,
                start: id_map[&e.start],
                end: id_map[&e.end],
                length: e.length,
            });
        }

        // Re-ID outer faces (excluding removed ones)
        let mut new_faces = Vec::new();
        for f in &source.faces {
            if remove_set.contains(&f.id) {
                continue;
            }
            let new_id = self.alloc_id();
            id_map.insert(f.id, new_id);
            new_faces.push(MockFace {
                id: new_id,
                edges: f.edges.iter().map(|eid| id_map[eid]).collect(),
                normal: f.normal,
                centroid: f.centroid,
                area: f.area,
                surface_type: f.surface_type.clone(),
            });
        }

        // Create inner offset faces for each kept face
        for f in &source.faces {
            if remove_set.contains(&f.id) {
                continue;
            }
            // Inner face: offset centroid inward by thickness along inverted normal
            let inner_centroid = [
                f.centroid[0] - f.normal[0] * thickness,
                f.centroid[1] - f.normal[1] * thickness,
                f.centroid[2] - f.normal[2] * thickness,
            ];
            let inner_normal = [-f.normal[0], -f.normal[1], -f.normal[2]];

            // Inner edges (new)
            let mut inner_edge_ids = Vec::new();
            for _ in &f.edges {
                let ie = MockEdge {
                    id: self.alloc_id(),
                    start: self.alloc_id(), // placeholder vertex IDs
                    end: self.alloc_id(),
                    length: f.area.sqrt() * 0.25,
                };
                inner_edge_ids.push(ie.id);
                new_edges.push(ie);
            }

            new_faces.push(MockFace {
                id: self.alloc_id(),
                edges: inner_edge_ids,
                normal: inner_normal,
                centroid: inner_centroid,
                area: f.area * (1.0 - thickness / f.area.sqrt()).max(0.01),
                surface_type: f.surface_type.clone(),
            });
        }

        let handle = self.alloc_handle();
        self.solids.insert(
            handle.id(),
            MockSolid {
                vertices: new_vertices,
                edges: new_edges,
                faces: new_faces,
            },
        );
        Ok(handle)
    }

    fn tessellate(
        &mut self,
        solid: &KernelSolidHandle,
        _tolerance: f64,
    ) -> Result<RenderMesh, KernelError> {
        let s = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?;
        Ok(Self::tessellate_box(s))
    }

    fn make_faces_from_profiles(
        &mut self,
        profiles: &[ClosedProfile],
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        _plane_x_axis: [f64; 3],
        positions: &HashMap<u32, (f64, f64)>,
    ) -> Result<Vec<KernelId>, KernelError> {
        let mut face_ids = Vec::new();

        for profile in profiles {
            // Compute a rough area from the 2D positions
            let pts: Vec<(f64, f64)> = profile
                .entity_ids
                .iter()
                .filter_map(|id| positions.get(id).copied())
                .collect();
            let area = if pts.len() >= 3 {
                shoelace_area(&pts).abs()
            } else {
                1.0
            };

            let face_id = self.alloc_id();
            let mock_face = MockFace {
                id: face_id,
                edges: Vec::new(),
                normal: plane_normal,
                centroid: plane_origin,
                area,
                surface_type: "planar".to_string(),
            };
            self.standalone_faces.insert(face_id.0, mock_face);
            face_ids.push(face_id);
        }

        Ok(face_ids)
    }
}

/// Compute area of a 2D polygon using the shoelace formula.
fn shoelace_area(pts: &[(f64, f64)]) -> f64 {
    let n = pts.len();
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += pts[i].0 * pts[j].1;
        area -= pts[j].0 * pts[i].1;
    }
    area / 2.0
}

impl KernelIntrospect for MockKernel {
    fn list_faces(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        self.solids
            .get(&solid.id())
            .map(|s| s.faces.iter().map(|f| f.id).collect())
            .unwrap_or_default()
    }

    fn list_edges(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        self.solids
            .get(&solid.id())
            .map(|s| s.edges.iter().map(|e| e.id).collect())
            .unwrap_or_default()
    }

    fn list_vertices(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        self.solids
            .get(&solid.id())
            .map(|s| s.vertices.iter().map(|v| v.id).collect())
            .unwrap_or_default()
    }

    fn face_edges(&self, face: KernelId) -> Vec<KernelId> {
        for solid in self.solids.values() {
            for f in &solid.faces {
                if f.id == face {
                    return f.edges.clone();
                }
            }
        }
        Vec::new()
    }

    fn edge_faces(&self, edge: KernelId) -> Vec<KernelId> {
        let mut result = Vec::new();
        for solid in self.solids.values() {
            for f in &solid.faces {
                if f.edges.contains(&edge) {
                    result.push(f.id);
                }
            }
        }
        result
    }

    fn edge_vertices(&self, edge: KernelId) -> (KernelId, KernelId) {
        for solid in self.solids.values() {
            for e in &solid.edges {
                if e.id == edge {
                    return (e.start, e.end);
                }
            }
        }
        (KernelId(0), KernelId(0))
    }

    fn face_neighbors(&self, face: KernelId) -> Vec<KernelId> {
        // Find all faces sharing an edge with the given face
        let face_edge_ids = self.face_edges(face);
        let mut neighbors = Vec::new();
        for solid in self.solids.values() {
            for f in &solid.faces {
                if f.id != face && f.edges.iter().any(|e| face_edge_ids.contains(e)) {
                    neighbors.push(f.id);
                }
            }
        }
        neighbors
    }

    fn compute_signature(&self, entity: KernelId, kind: TopoKind) -> TopoSignature {
        for solid in self.solids.values() {
            match kind {
                TopoKind::Face => {
                    for f in &solid.faces {
                        if f.id == entity {
                            return TopoSignature {
                                surface_type: Some(f.surface_type.clone()),
                                area: Some(f.area),
                                centroid: Some(f.centroid),
                                normal: Some(f.normal),
                                bbox: None,
                                adjacency_hash: None,
                                length: None,
                            };
                        }
                    }
                }
                TopoKind::Edge => {
                    for e in &solid.edges {
                        if e.id == entity {
                            let sv = solid.vertices.iter().find(|v| v.id == e.start);
                            let ev = solid.vertices.iter().find(|v| v.id == e.end);
                            let centroid = match (sv, ev) {
                                (Some(s), Some(e)) => Some([
                                    (s.position[0] + e.position[0]) / 2.0,
                                    (s.position[1] + e.position[1]) / 2.0,
                                    (s.position[2] + e.position[2]) / 2.0,
                                ]),
                                _ => None,
                            };
                            return TopoSignature {
                                surface_type: Some("line".to_string()),
                                area: None,
                                centroid,
                                normal: None,
                                bbox: None,
                                adjacency_hash: None,
                                length: Some(e.length),
                            };
                        }
                    }
                }
                TopoKind::Vertex => {
                    for v in &solid.vertices {
                        if v.id == entity {
                            return TopoSignature {
                                surface_type: Some("point".to_string()),
                                area: None,
                                centroid: Some(v.position),
                                normal: None,
                                bbox: None,
                                adjacency_hash: None,
                                length: None,
                            };
                        }
                    }
                }
                _ => {}
            }
        }
        TopoSignature::empty()
    }

    fn compute_all_signatures(
        &self,
        solid: &KernelSolidHandle,
        kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)> {
        let ids = match kind {
            TopoKind::Face => self.list_faces(solid),
            TopoKind::Edge => self.list_edges(solid),
            TopoKind::Vertex => self.list_vertices(solid),
            _ => Vec::new(),
        };
        ids.into_iter()
            .map(|id| {
                let sig = self.compute_signature(id, kind);
                (id, sig)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_faces_and_extrude_produces_box_topology() {
        let mut kernel = MockKernel::new();

        // Create a rectangular profile
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (2.0, 0.0));
        positions.insert(3, (2.0, 3.0));
        positions.insert(4, (0.0, 3.0));

        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();

        assert_eq!(face_ids.len(), 1);

        // Extrude the face
        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], 5.0)
            .unwrap();

        // Verify box topology: 8V, 12E, 6F
        let faces = kernel.list_faces(&handle);
        let edges = kernel.list_edges(&handle);
        let vertices = kernel.list_vertices(&handle);

        assert_eq!(vertices.len(), 8, "Box should have 8 vertices");
        assert_eq!(edges.len(), 12, "Box should have 12 edges");
        assert_eq!(faces.len(), 6, "Box should have 6 faces");
    }

    #[test]
    fn test_euler_formula_box() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(handle.id(), solid);

        let v = kernel.list_vertices(&handle).len() as i64;
        let e = kernel.list_edges(&handle).len() as i64;
        let f = kernel.list_faces(&handle).len() as i64;

        // Euler's formula for genus-0: V - E + F = 2
        assert_eq!(v - e + f, 2, "Euler formula V-E+F=2 must hold for a box");
    }

    #[test]
    fn test_deterministic_ids() {
        // Two kernels with same operations should produce same ID sequences
        let mut k1 = MockKernel::new();
        let mut k2 = MockKernel::new();

        let (h1, s1) = k1.make_box_solid(1.0, 2.0, 3.0);
        let (h2, s2) = k2.make_box_solid(1.0, 2.0, 3.0);

        k1.solids.insert(h1.id(), s1);
        k2.solids.insert(h2.id(), s2);

        let faces1 = k1.list_faces(&h1);
        let faces2 = k2.list_faces(&h2);

        assert_eq!(faces1.len(), faces2.len());
        for (f1, f2) in faces1.iter().zip(faces2.iter()) {
            assert_eq!(f1, f2, "IDs should be deterministically assigned");
        }
    }

    #[test]
    fn test_face_edges_returns_4_edges_per_box_face() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(handle.id(), solid);

        let faces = kernel.list_faces(&handle);
        for face in &faces {
            let edges = kernel.face_edges(*face);
            assert_eq!(edges.len(), 4, "Each box face should have 4 edges");
        }
    }

    #[test]
    fn test_edge_vertices_are_valid() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(handle.id(), solid);

        let all_verts = kernel.list_vertices(&handle);
        let edges = kernel.list_edges(&handle);

        for edge in &edges {
            let (v1, v2) = kernel.edge_vertices(*edge);
            assert!(all_verts.contains(&v1), "Edge start vertex must exist");
            assert!(all_verts.contains(&v2), "Edge end vertex must exist");
            assert_ne!(v1, v2, "Edge endpoints must be distinct");
        }
    }

    #[test]
    fn test_face_neighbors_box() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(handle.id(), solid);

        let faces = kernel.list_faces(&handle);
        for face in &faces {
            let neighbors = kernel.face_neighbors(*face);
            // Each face of a box has 4 neighbors (shares an edge with 4 other faces)
            assert_eq!(neighbors.len(), 4, "Each box face should have 4 neighbors");
            assert!(
                !neighbors.contains(face),
                "A face should not be its own neighbor"
            );
        }
    }

    #[test]
    fn test_compute_signature_face() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(2.0, 3.0, 4.0);
        kernel.solids.insert(handle.id(), solid);

        let faces = kernel.list_faces(&handle);
        let sig = kernel.compute_signature(faces[0], TopoKind::Face);

        assert_eq!(sig.surface_type.as_deref(), Some("planar"));
        assert!(sig.area.is_some());
        assert!(sig.centroid.is_some());
        assert!(sig.normal.is_some());
    }

    #[test]
    fn test_tessellate_box() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(handle.id(), solid);

        let mesh = kernel.tessellate(&handle, 0.1).unwrap();

        // 6 faces × 2 triangles × 3 indices = 36 indices
        assert_eq!(
            mesh.indices.len(),
            36,
            "Box should have 36 triangle indices"
        );
        // 6 faces × 4 vertices × 3 components = 72 vertex floats
        assert_eq!(mesh.vertices.len(), 72, "Box should have 72 vertex floats");
        assert_eq!(mesh.normals.len(), 72, "Normals should match vertices");
        assert_eq!(mesh.face_ranges.len(), 6, "Should have 6 face ranges");

        // Verify face_ranges cover all indices
        for (i, fr) in mesh.face_ranges.iter().enumerate() {
            assert_eq!(fr.start_index, (i * 6) as u32);
            assert_eq!(fr.end_index, ((i + 1) * 6) as u32);
        }
    }

    #[test]
    fn test_boolean_union_combines_topology() {
        let mut kernel = MockKernel::new();
        let (h1, s1) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(h1.id(), s1);
        let (h2, s2) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(h2.id(), s2);

        let result = kernel.boolean_union(&h1, &h2).unwrap();

        let faces = kernel.list_faces(&result);
        let edges = kernel.list_edges(&result);
        let vertices = kernel.list_vertices(&result);

        // Union of two boxes: 12F, 24E, 16V (simple merge)
        assert_eq!(faces.len(), 12);
        assert_eq!(edges.len(), 24);
        assert_eq!(vertices.len(), 16);
    }

    #[test]
    fn test_boolean_subtract_preserves_topology() {
        let mut kernel = MockKernel::new();
        let (h1, s1) = kernel.make_box_solid(2.0, 2.0, 2.0);
        kernel.solids.insert(h1.id(), s1);
        let (h2, s2) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(h2.id(), s2);

        let result = kernel.boolean_subtract(&h1, &h2).unwrap();

        // Subtract in mock returns copy of A with re-allocated IDs
        let faces = kernel.list_faces(&result);
        assert_eq!(faces.len(), 6);
    }

    #[test]
    fn test_fillet_single_edge() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(2.0, 2.0, 2.0);
        kernel.solids.insert(handle.id(), solid.clone());

        // Fillet the first edge
        let edge_id = solid.edges[0].id;
        let result = kernel.fillet_edges(&handle, &[edge_id], 0.2).unwrap();

        let faces = kernel.list_faces(&result);
        let edges = kernel.list_edges(&result);
        let vertices = kernel.list_vertices(&result);

        // Original: 6F, 12E, 8V. Fillet 1 edge: +1F, -1E+2E=+1E, +2V
        assert_eq!(faces.len(), 7, "Fillet adds 1 cylindrical face");
        assert_eq!(edges.len(), 13, "Fillet replaces 1 edge with 2 new edges");
        assert_eq!(vertices.len(), 10, "Fillet adds 2 vertices");

        // Verify the fillet face is cylindrical
        let fillet_face_sig = kernel.compute_signature(faces[6], TopoKind::Face);
        assert_eq!(fillet_face_sig.surface_type.as_deref(), Some("cylindrical"));
    }

    #[test]
    fn test_fillet_invalid_radius() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(handle.id(), solid);

        let result = kernel.fillet_edges(&handle, &[], -0.1);
        assert!(matches!(result, Err(KernelError::FilletFailed { .. })));
    }

    #[test]
    fn test_fillet_invalid_edge() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(handle.id(), solid);

        let result = kernel.fillet_edges(&handle, &[KernelId(99999)], 0.1);
        assert!(matches!(result, Err(KernelError::FilletFailed { .. })));
    }

    #[test]
    fn test_chamfer_single_edge() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(2.0, 2.0, 2.0);
        kernel.solids.insert(handle.id(), solid.clone());

        let edge_id = solid.edges[0].id;
        let result = kernel.chamfer_edges(&handle, &[edge_id], 0.3).unwrap();

        let faces = kernel.list_faces(&result);
        let edges = kernel.list_edges(&result);

        // Same topology change as fillet
        assert_eq!(faces.len(), 7, "Chamfer adds 1 planar face");
        assert_eq!(edges.len(), 13, "Chamfer replaces 1 edge with 2");

        // Chamfer face should be planar
        let chamfer_face_sig = kernel.compute_signature(faces[6], TopoKind::Face);
        assert_eq!(chamfer_face_sig.surface_type.as_deref(), Some("planar"));
    }

    #[test]
    fn test_chamfer_invalid_distance() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(handle.id(), solid);

        let result = kernel.chamfer_edges(&handle, &[], -0.1);
        assert!(matches!(result, Err(KernelError::Other { .. })));
    }

    #[test]
    fn test_shell_removes_face_and_adds_inner() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(2.0, 2.0, 2.0);
        kernel.solids.insert(handle.id(), solid.clone());

        // Remove one face (e.g., the top face)
        let top_face_id = solid.faces[1].id; // top face (z=d)
        let result = kernel.shell(&handle, &[top_face_id], 0.2).unwrap();

        let faces = kernel.list_faces(&result);

        // Original 6 faces - 1 removed = 5 outer faces + 5 inner faces = 10 faces
        assert_eq!(faces.len(), 10, "Shell: 5 outer + 5 inner faces");
    }

    #[test]
    fn test_shell_invalid_thickness() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(handle.id(), solid);

        let result = kernel.shell(&handle, &[], -0.1);
        assert!(matches!(result, Err(KernelError::ShellFailed { .. })));
    }

    #[test]
    fn test_shell_invalid_face() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(handle.id(), solid);

        let result = kernel.shell(&handle, &[KernelId(99999)], 0.1);
        assert!(matches!(result, Err(KernelError::ShellFailed { .. })));
    }

    #[test]
    fn test_fillet_multiple_edges() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(2.0, 2.0, 2.0);
        kernel.solids.insert(handle.id(), solid.clone());

        // Fillet 3 edges
        let edge_ids: Vec<KernelId> = solid.edges[0..3].iter().map(|e| e.id).collect();
        let result = kernel.fillet_edges(&handle, &edge_ids, 0.1).unwrap();

        let faces = kernel.list_faces(&result);
        let edges = kernel.list_edges(&result);
        let vertices = kernel.list_vertices(&result);

        // 3 filleted edges: +3F, +3E, +6V
        assert_eq!(faces.len(), 9);
        assert_eq!(edges.len(), 15);
        assert_eq!(vertices.len(), 14);
    }

    #[test]
    fn test_compute_all_signatures() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 2.0, 3.0);
        kernel.solids.insert(handle.id(), solid);

        let sigs = kernel.compute_all_signatures(&handle, TopoKind::Face);
        assert_eq!(sigs.len(), 6);

        for (_id, sig) in &sigs {
            assert_eq!(sig.surface_type.as_deref(), Some("planar"));
            assert!(sig.area.unwrap() > 0.0);
        }
    }

    #[test]
    fn test_edge_faces_each_edge_has_two_faces() {
        let mut kernel = MockKernel::new();
        let (handle, solid) = kernel.make_box_solid(1.0, 1.0, 1.0);
        kernel.solids.insert(handle.id(), solid);

        let edges = kernel.list_edges(&handle);
        for edge in &edges {
            let faces = kernel.edge_faces(*edge);
            assert_eq!(
                faces.len(),
                2,
                "Each box edge should be shared by exactly 2 faces"
            );
        }
    }
}
