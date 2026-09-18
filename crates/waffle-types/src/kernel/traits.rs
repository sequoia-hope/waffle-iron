use super::import::ImportedBodyData;
use super::types::*;
use std::collections::HashMap;

/// Core geometry kernel trait. Provides all shape construction and modification operations.
/// Implemented by `kernel_v2::KernelV2Adapter` (the production kernel since the
/// Phase 6 migration) and `MockKernel` (deterministic test double).
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

    /// Rigid copy of a solid: a NEW solid whose every point, surface and
    /// curve is moved by `placement` (`p' = R·p + t`). The source is
    /// untouched. Exact on analytic geometry (a rotated cylinder is a
    /// cylinder of the same radius). The substrate of circular/linear
    /// patterns (`specs/custom_features_and_modeling_roadmap.md` §B1).
    ///
    /// A placement whose rotation is not a proper rotation (a reflection or
    /// a scaled matrix) is an error, never a silently mirrored copy.
    fn transform_body(
        &mut self,
        _solid: &KernelSolidHandle,
        _placement: &RigidPlacement,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "transform_body".to_string(),
        })
    }

    /// Export a solid as an ISO 10303-21 (STEP, AP214) text file. The
    /// single-body form of [`Kernel::export_step_bodies`].
    fn export_step(
        &mut self,
        _solid: &KernelSolidHandle,
        _file_name: &str,
    ) -> Result<String, KernelError> {
        Err(KernelError::NotSupported {
            operation: "export_step".to_string(),
        })
    }

    /// Export several solids into ONE STEP file, each named and optionally
    /// placed — a multi-body part, or an assembly's leaf bodies at their
    /// world placements. Geometry is written analytically (exact surfaces
    /// and curves), never as a mesh. A body the kernel cannot write (a
    /// mesh-backed imported body) is a typed `NotSupported` naming it.
    fn export_step_bodies(
        &mut self,
        _bodies: &[StepExportBody],
        _file_name: &str,
    ) -> Result<String, KernelError> {
        Err(KernelError::NotSupported {
            operation: "export_step_bodies".to_string(),
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

/// What kind of analytic geometry an [`EntityAxis`] came from. Reported so a
/// consumer can label a derived frame ("cylindrical axis") and decide policy
/// per family without re-deriving the surface type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisKind {
    /// A cylindrical face's axis.
    Cylindrical,
    /// A conical face's axis (`origin` is the apex).
    Conical,
    /// A toroidal face's axis (`origin` is the tube centre circle's centre).
    Toroidal,
    /// A spherical face. There is no intrinsic axis; `origin` is the centre
    /// and `direction` is the kernel's canonical pole axis for a sphere.
    Spherical,
    /// A circular edge (full circle or arc).
    Circular,
    /// An elliptical edge.
    Elliptical,
}

impl AxisKind {
    /// Lower-case label for UI and diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            AxisKind::Cylindrical => "cylindrical",
            AxisKind::Conical => "conical",
            AxisKind::Toroidal => "toroidal",
            AxisKind::Spherical => "spherical",
            AxisKind::Circular => "circular",
            AxisKind::Elliptical => "elliptical",
        }
    }
}

/// The analytic axis of one entity: a rotational surface's axis, or the axis
/// of a circular/elliptical edge (its plane normal through its centre).
///
/// `origin` is the entity's OWN reference point on that axis — the cylinder's
/// `axis_point`, the cone's apex, the torus's/sphere's centre, the circle's
/// centre — never a policy choice. Where a consumer puts a frame on the axis
/// (a rim, the mid of a face's axial extent, the apex) is the consumer's
/// decision; see `feature_engine::connector`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityAxis {
    pub kind: AxisKind,
    /// A point ON the axis (see the type docs).
    pub origin: [f64; 3],
    /// Unit axis direction.
    pub direction: [f64; 3],
    /// Radius, where the family has one (cylinder, sphere, circle; a torus
    /// reports its major radius, an ellipse its semi-major radius).
    pub radius: Option<f64>,
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

    /// The edge's geometry as a 3D polyline from its start vertex to its end
    /// vertex: two points for a straight edge; for a curved edge, its chord
    /// samples at the kernel's render density (a closed circle edge is a
    /// closed polyline whose last point repeats the first). This is what a
    /// consumer must use for the FOOTPRINT of a face — the edge vertices
    /// alone under-represent every curved edge, and a circular cap has a
    /// single seam vertex (no footprint at all).
    ///
    /// The default derives the two endpoint positions from `edge_vertices`
    /// and `compute_signature` — exact for straight edges, the chord for
    /// curved ones. A kernel with curve sampling overrides it.
    fn edge_polyline(&self, edge: KernelId) -> Vec<[f64; 3]> {
        let (a, b) = self.edge_vertices(edge);
        [a, b]
            .into_iter()
            .filter_map(|v| self.compute_signature(v, TopoKind::Vertex).centroid)
            .collect()
    }

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

    /// The solid's volume in m³, integrated exactly from its B-Rep
    /// (`specs/waffle_mcp_server.md` ICR-1). A kernel that cannot integrate a
    /// solid answers an error, never an approximation: a consumer that falls
    /// back to a tessellation must label that number itself. The default is
    /// `NotSupported`, so this is additive for every implementor.
    fn solid_volume(&self, _solid: &KernelSolidHandle) -> Result<f64, crate::kernel::KernelError> {
        Err(crate::kernel::KernelError::NotSupported {
            operation: "exact solid volume".to_string(),
        })
    }

    /// The solid's total surface area in m², exactly from its B-Rep. Same
    /// contract as [`Self::solid_volume`].
    fn solid_surface_area(
        &self,
        _solid: &KernelSolidHandle,
    ) -> Result<f64, crate::kernel::KernelError> {
        Err(crate::kernel::KernelError::NotSupported {
            operation: "exact solid surface area".to_string(),
        })
    }

    /// The entity's analytic axis, when its geometry has one: a cylindrical,
    /// conical, toroidal or spherical FACE, or a circular/elliptical EDGE.
    ///
    /// `None` for a planar face, a straight edge, a freeform/mesh-backed
    /// surface, an entity whose curve has no axis (a surface-pair or
    /// hyperbola piece), and for a kernel that does not track analytic
    /// geometry — the default, so this is additive for every implementor.
    ///
    /// This is the ONLY door out for a rotational surface's axis:
    /// [`TopoSignature::normal`] is the outward normal at the centroid, which
    /// on a cylinder is radial, not axial.
    fn entity_axis(&self, _entity: KernelId, _kind: TopoKind) -> Option<EntityAxis> {
        None
    }

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
