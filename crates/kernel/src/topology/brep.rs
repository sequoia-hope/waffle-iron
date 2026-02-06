use serde::{Deserialize, Serialize};
use slotmap::{new_key_type, SlotMap};

use crate::geometry::curves::Curve;
use crate::geometry::point::Point3d;
use crate::geometry::surfaces::Surface;
use crate::geometry::transform::BoundingBox;
use crate::geometry::vector::Vec3;

// ─── Entity Keys ─────────────────────────────────────────────────────────────

new_key_type! {
    pub struct VertexId;
    pub struct EdgeId;
    pub struct HalfEdgeId;
    pub struct LoopId;
    pub struct FaceId;
    pub struct ShellId;
    pub struct SolidId;
}

// ─── Topological Entities ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vertex {
    pub point: Point3d,
    pub tolerance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub curve: Curve,
    pub half_edges: (HalfEdgeId, HalfEdgeId),
    pub start_vertex: VertexId,
    pub end_vertex: VertexId,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HalfEdge {
    pub edge: EdgeId,
    pub twin: HalfEdgeId,
    pub face: FaceId,
    pub loop_id: LoopId,
    pub start_vertex: VertexId,
    pub end_vertex: VertexId,
    /// Parameter range on the edge's curve.
    pub t_start: f64,
    pub t_end: f64,
    /// true if this half-edge traverses the curve in the forward direction.
    pub forward: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Loop {
    pub half_edges: Vec<HalfEdgeId>,
    pub face: FaceId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Face {
    pub surface: Surface,
    pub outer_loop: LoopId,
    pub inner_loops: Vec<LoopId>,
    /// true if the face normal agrees with the surface normal.
    pub same_sense: bool,
    pub shell: ShellId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellOrientation {
    /// Outer shell (normals point outward).
    Outward,
    /// Void shell (normals point inward, represents a cavity).
    Inward,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shell {
    pub faces: Vec<FaceId>,
    pub orientation: ShellOrientation,
    pub solid: SolidId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solid {
    pub shells: Vec<ShellId>,
}

// ─── Entity Store ────────────────────────────────────────────────────────────

/// Arena-based storage for all topological entities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityStore {
    pub vertices: SlotMap<VertexId, Vertex>,
    pub edges: SlotMap<EdgeId, Edge>,
    pub half_edges: SlotMap<HalfEdgeId, HalfEdge>,
    pub loops: SlotMap<LoopId, Loop>,
    pub faces: SlotMap<FaceId, Face>,
    pub shells: SlotMap<ShellId, Shell>,
    pub solids: SlotMap<SolidId, Solid>,
}

impl EntityStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Count topological entities for a shell: (vertices, edges, faces).
    pub fn count_topology(&self, shell_id: ShellId) -> (usize, usize, usize) {
        let shell = &self.shells[shell_id];
        let face_count = shell.faces.len();

        let mut edge_set = std::collections::HashSet::new();
        let mut vertex_set = std::collections::HashSet::new();

        for &face_id in &shell.faces {
            let face = &self.faces[face_id];
            self.collect_loop_topology(face.outer_loop, &mut edge_set, &mut vertex_set);
            for &inner_loop in &face.inner_loops {
                self.collect_loop_topology(inner_loop, &mut edge_set, &mut vertex_set);
            }
        }

        (vertex_set.len(), edge_set.len(), face_count)
    }

    fn collect_loop_topology(
        &self,
        loop_id: LoopId,
        edges: &mut std::collections::HashSet<u64>,
        vertices: &mut std::collections::HashSet<u64>,
    ) {
        let loop_data = &self.loops[loop_id];
        for &he_id in &loop_data.half_edges {
            let he = &self.half_edges[he_id];
            // Use edge key data as a unique identifier
            edges.insert(edge_id_to_u64(he.edge));
            vertices.insert(vertex_id_to_u64(he.start_vertex));
            vertices.insert(vertex_id_to_u64(he.end_vertex));
        }
    }

    /// Compute axis-aligned bounding box for a solid.
    pub fn solid_bounding_box(&self, solid_id: SolidId) -> BoundingBox {
        let solid = &self.solids[solid_id];
        let mut bb = BoundingBox::empty();

        for &shell_id in &solid.shells {
            let shell = &self.shells[shell_id];
            for &face_id in &shell.faces {
                let face = &self.faces[face_id];
                self.loop_bounding_box(face.outer_loop, &mut bb);
            }
        }

        bb
    }

    fn loop_bounding_box(&self, loop_id: LoopId, bb: &mut BoundingBox) {
        let loop_data = &self.loops[loop_id];
        for &he_id in &loop_data.half_edges {
            let he = &self.half_edges[he_id];
            bb.expand_to_include(&self.vertices[he.start_vertex].point);
            bb.expand_to_include(&self.vertices[he.end_vertex].point);
            // Sample the curve for better bounds
            let edge = &self.edges[he.edge];
            let num_samples = 8;
            for i in 1..num_samples {
                let t = he.t_start + (he.t_end - he.t_start) * (i as f64 / num_samples as f64);
                let p = edge.curve.evaluate(t);
                bb.expand_to_include(&p);
            }
        }
    }

    /// Get the outward-facing normal of a face at a parameter point.
    pub fn face_normal(&self, face_id: FaceId, u: f64, v: f64) -> Vec3 {
        let face = &self.faces[face_id];
        let n = face.surface.normal_at(u, v);
        if face.same_sense { n } else { -n }
    }
}

// Helper to convert slotmap keys to u64 for HashSet usage.
fn edge_id_to_u64(id: EdgeId) -> u64 {
    use slotmap::Key;
    id.data().as_ffi()
}

fn vertex_id_to_u64(id: VertexId) -> u64 {
    use slotmap::Key;
    id.data().as_ffi()
}

// ─── Topology Audit ─────────────────────────────────────────────────────────

/// Result of a topological consistency check.
#[derive(Debug, Clone)]
pub struct TopologyAudit {
    pub euler_valid: bool,
    pub all_edges_two_faced: bool,
    pub all_faces_closed: bool,
    pub no_dangling_vertices: bool,
    pub shells_closed: bool,
    pub normals_consistent: bool,
    pub errors: Vec<TopologyError>,
}

#[derive(Debug, Clone)]
pub enum TopologyError {
    EulerViolation {
        shell: ShellId,
        v: usize,
        e: usize,
        f: usize,
        expected_chi: i64,
        actual_chi: i64,
    },
    OpenLoop {
        loop_id: LoopId,
    },
    DanglingVertex {
        vertex: VertexId,
    },
    HalfEdgeTwinMismatch {
        half_edge: HalfEdgeId,
    },
    VertexPositionMismatch {
        vertex: VertexId,
        edge: EdgeId,
        expected: Point3d,
        actual: Point3d,
        distance: f64,
    },
}

impl TopologyAudit {
    pub fn all_valid(&self) -> bool {
        self.euler_valid
            && self.all_edges_two_faced
            && self.all_faces_closed
            && self.no_dangling_vertices
            && self.shells_closed
    }
}

/// Perform a full topology audit on a solid.
pub fn audit_solid(store: &EntityStore, solid_id: SolidId) -> TopologyAudit {
    let solid = &store.solids[solid_id];
    let mut errors = Vec::new();
    let mut euler_valid = true;
    let mut all_faces_closed = true;
    let mut all_edges_two_faced = true;

    for &shell_id in &solid.shells {
        // Check Euler-Poincaré: V - E + F = 2 for genus-0 closed shells
        let (v, e, f) = store.count_topology(shell_id);
        let chi = v as i64 - e as i64 + f as i64;
        if chi != 2 {
            euler_valid = false;
            errors.push(TopologyError::EulerViolation {
                shell: shell_id,
                v,
                e,
                f,
                expected_chi: 2,
                actual_chi: chi,
            });
        }

        // Check that all loops are closed
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face = &store.faces[face_id];
            if !is_loop_closed(store, face.outer_loop) {
                all_faces_closed = false;
                errors.push(TopologyError::OpenLoop {
                    loop_id: face.outer_loop,
                });
            }
            for &inner_loop in &face.inner_loops {
                if !is_loop_closed(store, inner_loop) {
                    all_faces_closed = false;
                    errors.push(TopologyError::OpenLoop {
                        loop_id: inner_loop,
                    });
                }
            }
        }
    }

    // Check half-edge twin consistency
    for (he_id, he) in &store.half_edges {
        if let Some(twin) = store.half_edges.get(he.twin) {
            if twin.twin != he_id {
                all_edges_two_faced = false;
                errors.push(TopologyError::HalfEdgeTwinMismatch { half_edge: he_id });
            }
        }
    }

    TopologyAudit {
        euler_valid,
        all_edges_two_faced,
        all_faces_closed,
        no_dangling_vertices: true, // TODO: implement
        shells_closed: euler_valid && all_faces_closed,
        normals_consistent: true, // TODO: implement
        errors,
    }
}

fn is_loop_closed(store: &EntityStore, loop_id: LoopId) -> bool {
    let loop_data = &store.loops[loop_id];
    if loop_data.half_edges.is_empty() {
        return false;
    }
    let first_he = &store.half_edges[loop_data.half_edges[0]];
    let last_he = &store.half_edges[*loop_data.half_edges.last().unwrap()];
    // The last half-edge's end should be the first half-edge's start
    first_he.start_vertex == last_he.end_vertex
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::point::Point3d;

    #[test]
    fn test_entity_store_creation() {
        let store = EntityStore::new();
        assert_eq!(store.vertices.len(), 0);
        assert_eq!(store.edges.len(), 0);
    }

    #[test]
    fn test_vertex_insertion() {
        let mut store = EntityStore::new();
        let v = store.vertices.insert(Vertex {
            point: Point3d::new(1.0, 2.0, 3.0),
            tolerance: 1e-7,
        });
        assert_eq!(store.vertices[v].point.x, 1.0);
    }
}
