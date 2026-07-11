use super::import::ImportedBodyData;
use super::types::*;
use std::collections::HashMap;

/// Core geometry kernel trait. Provides all shape construction and modification operations.
/// Implemented by WaffleKernel (clean-sheet kernel) and MockKernel (deterministic test double).
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

    /// Boolean union that may produce multiple bodies (e.g., disjoint operands).
    /// Default delegates to `boolean_union` and wraps in a single-element vec.
    fn boolean_union_multi(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<Vec<KernelSolidHandle>, KernelError> {
        Ok(vec![self.boolean_union(a, b)?])
    }

    /// Boolean subtract that may produce multiple bodies.
    /// Default delegates to `boolean_subtract` and wraps in a single-element vec.
    fn boolean_subtract_multi(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<Vec<KernelSolidHandle>, KernelError> {
        Ok(vec![self.boolean_subtract(a, b)?])
    }

    /// Boolean intersect that may produce multiple bodies.
    /// Default delegates to `boolean_intersect` and wraps in a single-element vec.
    fn boolean_intersect_multi(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<Vec<KernelSolidHandle>, KernelError> {
        Ok(vec![self.boolean_intersect(a, b)?])
    }

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

    /// Extract edge polylines for rendering edge overlays.
    fn extract_edges(
        &mut self,
        solid: &KernelSolidHandle,
        tolerance: f64,
    ) -> Result<EdgeRenderData, KernelError>;

    /// Ingest an externally-imported mesh-backed body (STEP import, task
    /// #138 — `docs/step_import_roadmap.md`). The data is already placed in
    /// world coordinates (meters). The body is first-class for rendering,
    /// introspection, and signatures; operations the kernel cannot perform
    /// on it (booleans in SI1) return typed `NotSupported`.
    fn import_body(&mut self, _data: &ImportedBodyData) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "import_body".to_string(),
        })
    }

    /// Export a solid to STEP AP203 format string.
    fn export_step(
        &mut self,
        _solid: &KernelSolidHandle,
        _file_name: &str,
    ) -> Result<String, KernelError> {
        Err(KernelError::NotSupported {
            operation: "export_step".to_string(),
        })
    }

    /// Create planar faces from closed sketch profiles.
    fn make_faces_from_profiles(
        &mut self,
        profiles: &[ClosedProfile],
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        plane_x_axis: [f64; 3],
        positions: &HashMap<u32, (f64, f64)>,
    ) -> Result<Vec<KernelId>, KernelError>;

    /// Create a single planar face from an explicit region boundary: an outer
    /// loop plus zero or more hole loops, in sketch (u, v) coordinates. Each
    /// loop is a closed polyline WITHOUT a repeated closing vertex; winding is
    /// normalized by the kernel.
    ///
    /// Used to extrude minimal sub-regions of overlapping sketch shapes
    /// (annulus, lens, crescent) that no single whole-loop profile denotes. The
    /// region's `*_edges` carry recovered circular arcs so the implementation
    /// can build exact cylinder walls; `outer`/`holes` are the tessellated
    /// fallback.
    fn make_face_from_region(
        &mut self,
        _region: &crate::Region,
        _plane_origin: [f64; 3],
        _plane_normal: [f64; 3],
        _plane_x_axis: [f64; 3],
    ) -> Result<KernelId, KernelError> {
        Err(KernelError::NotSupported {
            operation: "make_face_from_region".to_string(),
        })
    }
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

    /// Persistent-identity provenance of a face (KV13 F5): its persistent id
    /// and its **lineage root** (the id where the geometry was introduced,
    /// through chained booleans). Used by feature-engine (F6) to resolve the
    /// face's *creating* feature — the original extrude/revolve, not the last
    /// boolean. The default returns `None` (a kernel that does not track
    /// persistent identity, e.g. `MockKernel`); `face` should be a face id.
    fn face_provenance(&self, _face: KernelId) -> Option<FaceProvenance> {
        None
    }
}
