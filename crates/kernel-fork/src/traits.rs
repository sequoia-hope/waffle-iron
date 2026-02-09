use crate::types::*;
use std::collections::HashMap;

/// Core geometry kernel trait. Provides all shape construction and modification operations.
/// Implemented by TruckKernel (wraps real truck) and MockKernel (deterministic test double).
pub trait Kernel {
    /// Extrude a planar face along a direction vector.
    fn extrude_face(
        &mut self,
        face: KernelId,
        direction: [f64; 3],
        depth: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Revolve a planar face around an axis.
    fn revolve_face(
        &mut self,
        face: KernelId,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Boolean union of two solids.
    fn boolean_union(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Boolean subtraction: a minus b.
    fn boolean_subtract(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Boolean intersection of two solids.
    fn boolean_intersect(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Fillet (round) the specified edges with the given radius.
    fn fillet_edges(
        &mut self,
        solid: &KernelSolidHandle,
        edges: &[KernelId],
        radius: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Chamfer (bevel) the specified edges with the given distance.
    fn chamfer_edges(
        &mut self,
        solid: &KernelSolidHandle,
        edges: &[KernelId],
        distance: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Shell a solid by removing faces and offsetting remaining faces inward.
    fn shell(
        &mut self,
        solid: &KernelSolidHandle,
        faces_to_remove: &[KernelId],
        thickness: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Tessellate a solid to a triangle mesh.
    fn tessellate(
        &mut self,
        solid: &KernelSolidHandle,
        tolerance: f64,
    ) -> Result<RenderMesh, KernelError>;

    /// Create planar faces from closed sketch profiles.
    fn make_faces_from_profiles(
        &mut self,
        profiles: &[ClosedProfile],
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        plane_x_axis: [f64; 3],
        positions: &HashMap<u32, (f64, f64)>,
    ) -> Result<Vec<KernelId>, KernelError>;
}

/// Topology introspection trait. Provides read-only queries on kernel geometry.
pub trait KernelIntrospect {
    /// List all faces of a solid.
    fn list_faces(&self, solid: &KernelSolidHandle) -> Vec<KernelId>;

    /// List all edges of a solid.
    fn list_edges(&self, solid: &KernelSolidHandle) -> Vec<KernelId>;

    /// List all vertices of a solid.
    fn list_vertices(&self, solid: &KernelSolidHandle) -> Vec<KernelId>;

    /// Get the edges bounding a face.
    fn face_edges(&self, face: KernelId) -> Vec<KernelId>;

    /// Get the faces adjacent to an edge.
    fn edge_faces(&self, edge: KernelId) -> Vec<KernelId>;

    /// Get the vertices at the ends of an edge.
    fn edge_vertices(&self, edge: KernelId) -> (KernelId, KernelId);

    /// Get the faces sharing an edge or vertex with the given face.
    fn face_neighbors(&self, face: KernelId) -> Vec<KernelId>;

    /// Compute the geometric signature of a single entity.
    fn compute_signature(&self, entity: KernelId, kind: TopoKind) -> TopoSignature;

    /// Compute signatures for all entities of a given kind in a solid.
    fn compute_all_signatures(
        &self,
        solid: &KernelSolidHandle,
        kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)>;
}
