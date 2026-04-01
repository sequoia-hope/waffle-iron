//! Yang 2025 hybrid B-Rep/mesh boolean pipeline integration.
//!
//! Bridges WaffleSolid (the kernel's solid representation) with the
//! Yang pipeline stages (tessellate → exact mesh boolean → topology
//! extract → SSI refinement). This module converts between the kernel's
//! native types and the pipeline's mesh-based types.
//!
//! Ref [#24]: Yang, Jia & Yan (2025) — hybrid B-Rep/mesh boolean.

use std::collections::BTreeMap;

use crate::boolean::exact_mesh::MeshId;
use crate::boolean::ssi_refinement::EdgeRefinementMap;
use crate::boolean::topology_extract::ResultTopology;
use crate::boolean::BoolOp;
use crate::boolean::BooleanResult;
use crate::geometry::curve::{Circle3D, CurveGeom, Ellipse3D, Line3D};
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::SurfaceGeom;
use crate::ssi::SSICurve;
use crate::topology::half_edge::{EdgeIdx, FaceIdx, VertexIdx};
use crate::types::{KernelError, RenderMesh};
use crate::waffle_kernel::WaffleSolid;

use crate::boolean::exact_mesh::MeshBooleanOp;
use crate::boolean::ssi_refinement::{classify_intersection_edges, refine_intersection_edges};
use crate::boolean::topology_extract::yang_boolean_pipeline;
use crate::tessellation;
use crate::tessellation::bijective::BijectiveMap;

// ── Mesh conversion helpers ─────────────────────────────────────────────

/// Convert RenderMesh (f32 flat arrays) to the pipeline format (Vec<[f64;3]>, Vec<[usize;3]>).
#[allow(dead_code)] // Phase 5b — used when tessellation bridge is wired
pub(crate) fn render_mesh_to_arrays(mesh: &RenderMesh) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
    let vert_count = mesh.vertices.len() / 3;
    let verts: Vec<[f64; 3]> = (0..vert_count)
        .map(|i| {
            [
                mesh.vertices[i * 3] as f64,
                mesh.vertices[i * 3 + 1] as f64,
                mesh.vertices[i * 3 + 2] as f64,
            ]
        })
        .collect();
    let tri_count = mesh.indices.len() / 3;
    let tris: Vec<[usize; 3]> = (0..tri_count)
        .map(|i| {
            [
                mesh.indices[i * 3] as usize,
                mesh.indices[i * 3 + 1] as usize,
                mesh.indices[i * 3 + 2] as usize,
            ]
        })
        .collect();
    (verts, tris)
}

/// Build the surface_map from two WaffleSolids' face_geometry.
///
/// Maps `(MeshId, FaceIdx) → SurfaceGeom` so that downstream pipeline stages
/// (topology extraction, SSI refinement) can look up which analytical surface
/// each original B-Rep face belongs to.
pub(crate) fn build_surface_map(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
) -> BTreeMap<(MeshId, FaceIdx), SurfaceGeom> {
    let mut map = BTreeMap::new();
    for (&face_idx, geom) in &solid_a.face_geometry {
        map.insert((MeshId::A, face_idx), geom.clone());
    }
    for (&face_idx, geom) in &solid_b.face_geometry {
        map.insert((MeshId::B, face_idx), geom.clone());
    }
    map
}

/// Convert BoolOp (kernel-level) to MeshBooleanOp (exact mesh pipeline level).
pub(crate) fn bool_op_to_mesh_op(op: BoolOp) -> MeshBooleanOp {
    match op {
        BoolOp::Union => MeshBooleanOp::Union,
        BoolOp::Subtract => MeshBooleanOp::Subtract,
        BoolOp::Intersect => MeshBooleanOp::Intersect,
    }
}

// ── SSICurve → CurveGeom conversion ─────────────────────────────────────

/// Convert an SSICurve (from the SSI solver) to the kernel's CurveGeom.
///
/// Only the basic analytical types are converted: Line, Circle, Ellipse.
/// Higher-order curves (Parabola, Hyperbola, Degree4*) return `None` for now
/// — these edges will use mesh-derived polyline geometry instead.
pub(crate) fn ssi_curve_to_curve_geom(curve: &SSICurve) -> Option<CurveGeom> {
    match curve {
        SSICurve::Line { start, end } => {
            let origin = Point3::new(start[0], start[1], start[2]);
            let end_pt = Point3::new(end[0], end[1], end[2]);
            let dir = Vector3::new(
                end_pt.x - origin.x,
                end_pt.y - origin.y,
                end_pt.z - origin.z,
            );
            Some(CurveGeom::Linear(Line3D {
                origin,
                direction: dir,
            }))
        }
        SSICurve::Circle {
            center,
            normal,
            radius,
        } => Some(CurveGeom::Circular(Circle3D {
            center: Point3::new(center[0], center[1], center[2]),
            normal: Vector3::new(normal[0], normal[1], normal[2]),
            radius: *radius,
        })),
        SSICurve::Ellipse {
            center,
            normal,
            major_axis,
            semi_major,
            semi_minor,
        } => Some(CurveGeom::Elliptical(Ellipse3D {
            center: Point3::new(center[0], center[1], center[2]),
            normal: Vector3::new(normal[0], normal[1], normal[2]),
            major_axis: Vector3::new(major_axis[0], major_axis[1], major_axis[2]),
            semi_major: *semi_major,
            semi_minor: *semi_minor,
        })),
        // Higher-order curves not yet converted to CurveGeom
        SSICurve::Parabola { .. }
        | SSICurve::Hyperbola { .. }
        | SSICurve::Degree4CylCyl { .. }
        | SSICurve::Degree4ConeSphere { .. }
        | SSICurve::Degree4CylSphere { .. }
        | SSICurve::Degree4CylCone { .. }
        | SSICurve::Degree4ConeCone { .. }
        | SSICurve::Degree4PlaneTorus { .. }
        | SSICurve::Degree4SphereTorus { .. } => None,
    }
}

// ── Result topology → WaffleSolid ───────────────────────────────────────

/// Convert a ResultTopology (from the Yang pipeline Phase 3) plus SSI refinement
/// (Phase 4) back into a WaffleSolid that the kernel can store and tessellate.
///
/// Assigns new unique IDs to all topological entities (faces, edges, vertices)
/// via `id_alloc`. Surface geometry is propagated from the original solids via
/// `surface_map` (face provenance). Edge geometry is derived from SSI refinement
/// curves where available.
#[allow(dead_code)] // Phase 5b — used when full pipeline is wired
pub(crate) fn result_topology_to_waffle_solid(
    result: ResultTopology,
    refinement: &EdgeRefinementMap,
    surface_map: &BTreeMap<(MeshId, FaceIdx), SurfaceGeom>,
    id_alloc: &mut dyn FnMut() -> u64,
) -> WaffleSolid {
    // Build face_map: assign a unique u64 ID to each face
    let mut face_map = BTreeMap::new();
    for &face_idx in result.face_provenance.keys() {
        face_map.insert(id_alloc(), face_idx);
    }

    // Build edge_map: assign IDs to intersection edges first
    let mut edge_map = BTreeMap::new();
    for &edge_idx in result.edge_is_intersection.keys() {
        edge_map.insert(id_alloc(), edge_idx);
    }
    // Also add non-intersection edges from the arena
    for i in 0..result.arena.edges.len() {
        let eidx = EdgeIdx(i);
        if !edge_map.values().any(|&e| e == eidx) {
            edge_map.insert(id_alloc(), eidx);
        }
    }

    // Build vertex_map
    let mut vertex_map = BTreeMap::new();
    for i in 0..result.arena.vertices.len() {
        vertex_map.insert(id_alloc(), VertexIdx(i));
    }

    // Build face_geometry from provenance: look up each face's source in the
    // surface_map to propagate analytical surface types through the boolean.
    let mut face_geometry = BTreeMap::new();
    for (&face_idx, source) in &result.face_provenance {
        if let Some(geom) = surface_map.get(&(source.mesh_id, source.face_idx)) {
            face_geometry.insert(face_idx, geom.clone());
        }
    }

    // Build edge_geometry from SSI refinement curves
    let mut edge_geometry = BTreeMap::new();
    for (&edge_idx, curve) in &refinement.edges {
        if let Some(curve_geom) = ssi_curve_to_curve_geom(curve) {
            edge_geometry.insert(edge_idx, curve_geom);
        }
    }

    WaffleSolid {
        arena: result.arena,
        face_map,
        edge_map,
        vertex_map,
        face_geometry,
        edge_geometry,
        cylinder_params: None,
        revolve_params: None,
        sphere_params: None,
        cone_params: None,
        torus_params: None,
        cached_face_polys: None,
        is_polygon_soup: false,
    }
}

/// Convert a WaffleSolid from the Yang pipeline into a BooleanResult that
/// integrates with the existing `do_boolean` result handling.
#[allow(dead_code)] // Phase 5b — used when yang_boolean_from_solids returns Ok
pub(crate) fn waffle_solid_to_boolean_result(solid: WaffleSolid) -> BooleanResult {
    BooleanResult {
        arena: solid.arena,
        face_map: solid.face_map,
        edge_map: solid.edge_map,
        vertex_map: solid.vertex_map,
        face_geometry: solid.face_geometry,
        edge_geometry: solid.edge_geometry,
        cached_face_polys: None,
    }
}

// ── Main entry point ────────────────────────────────────────────────────

/// Run the full Yang hybrid boolean pipeline on two WaffleSolids.
///
/// This is the Phase 5a integration entry point. It performs:
/// 1. Tessellation of both operands (via the kernel's internal tessellate path)
/// 2. Mesh conversion to pipeline format
/// 3. (Future) Exact mesh boolean (subdivide + label + select)
/// 4. (Future) Topology extraction (face survival → trim boundaries → B-Rep)
/// 5. (Future) SSI refinement of intersection edges
/// 6. Conversion back to BooleanResult
///
/// Currently returns `NotSupported` — the tessellation-to-exact-mesh bridge
/// is not yet wired. The helper functions above are tested independently and
/// ready for when the bridge is completed.
///
/// Ref [#24]: Yang, Jia & Yan (2025) — full 6-stage pipeline.
pub(crate) fn yang_boolean_from_solids(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
    // Guard: Yang pipeline is not yet the default path. It produces correct
    // topology but the WaffleSolid output format is not yet compatible with
    // the kernel's tessellation and face-count expectations. Enable only
    // when YANG_BOOLEAN=1 environment variable is set (for testing).
    // Phase 5b will make this the default path.
    if std::env::var("YANG_BOOLEAN").unwrap_or_default() != "1" {
        return Err(KernelError::NotSupported {
            operation: "yang_boolean: not enabled (set YANG_BOOLEAN=1 to activate)".to_string(),
        });
    }

    // Guard: both solids must have face_geometry to run the Yang pipeline.
    // Solids without face geometry cannot be mapped back to analytical surfaces.
    if solid_a.face_geometry.is_empty() || solid_b.face_geometry.is_empty() {
        return Err(KernelError::NotSupported {
            operation: "yang_boolean: one or both solids missing face_geometry".to_string(),
        });
    }

    let mesh_op = bool_op_to_mesh_op(op);
    let surface_map = build_surface_map(solid_a, solid_b);

    // Step 1: Tessellate both solids via the kernel's internal tessellation.
    let mesh_a = tessellate_waffle_solid(solid_a)?;
    let mesh_b = tessellate_waffle_solid(solid_b)?;

    // Step 2: Convert to pipeline format (f64 arrays).
    let (verts_a, tris_a) = render_mesh_to_arrays(&mesh_a);
    let (verts_b, tris_b) = render_mesh_to_arrays(&mesh_b);

    if tris_a.is_empty() || tris_b.is_empty() {
        return Err(KernelError::NotSupported {
            operation: "yang_boolean: tessellation produced empty mesh".to_string(),
        });
    }

    // Step 3: Build bijective maps (mesh triangle → B-Rep face).
    let bijective_a = BijectiveMap::from_render_mesh(&mesh_a, &solid_a.face_map);
    let bijective_b = BijectiveMap::from_render_mesh(&mesh_b, &solid_b.face_map);

    if !bijective_a.is_complete() || !bijective_b.is_complete() {
        return Err(KernelError::NotSupported {
            operation: "yang_boolean: bijective map has unmapped triangles".to_string(),
        });
    }

    // Step 4: Run Yang pipeline (Phases 1-3): mesh boolean → topology extract.
    let result_topo = yang_boolean_pipeline(
        &verts_a,
        &tris_a,
        &verts_b,
        &tris_b,
        &bijective_a,
        &bijective_b,
        mesh_op,
    );

    // Step 5: Phase 4a — classify intersection edges by surface pair type.
    // This may fail if face provenance references faces not in the surface map
    // (e.g., due to subdivision creating new face indices). In that case, skip
    // refinement and use the topology as-is.
    let refinement = match classify_intersection_edges(&result_topo, &surface_map) {
        Ok(classification) => {
            // Step 6: Phase 4b — refine intersection edges with SSI solvers.
            match refine_intersection_edges(&result_topo, &classification, &surface_map) {
                Ok(r) => r,
                Err(_) => EdgeRefinementMap {
                    edges: BTreeMap::new(),
                    skipped_planar: 0,
                    unsupported: vec![],
                },
            }
        }
        Err(_) => EdgeRefinementMap {
            edges: BTreeMap::new(),
            skipped_planar: 0,
            unsupported: vec![],
        },
    };

    // Step 7: Convert ResultTopology → WaffleSolid → BooleanResult.
    let waffle = result_topology_to_waffle_solid(result_topo, &refinement, &surface_map, id_alloc);
    Ok(waffle_solid_to_boolean_result(waffle))
}

/// Tessellate a WaffleSolid using the kernel's internal tessellation pipeline.
fn tessellate_waffle_solid(solid: &WaffleSolid) -> Result<RenderMesh, KernelError> {
    tessellation::tessellate_solid_ext(
        &solid.arena,
        &solid.face_map,
        &solid.face_geometry,
        &solid.edge_geometry,
        solid.cylinder_params.as_ref(),
        solid.revolve_params.as_ref(),
        solid.sphere_params.as_ref(),
        solid.cone_params.as_ref(),
        solid.torus_params.as_ref(),
        solid.is_polygon_soup,
    )
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FaceRange;

    #[test]
    fn render_mesh_to_arrays_basic() {
        // A single triangle: 3 vertices, 1 triangle
        let mesh = RenderMesh {
            vertices: vec![
                0.0, 0.0, 0.0, // v0
                1.0, 0.0, 0.0, // v1
                0.0, 1.0, 0.0, // v2
            ],
            normals: vec![
                0.0, 0.0, 1.0, // n0
                0.0, 0.0, 1.0, // n1
                0.0, 0.0, 1.0, // n2
            ],
            indices: vec![0, 1, 2],
            face_ranges: vec![FaceRange {
                face_id: crate::types::KernelId(1),
                start_index: 0,
                end_index: 3,
            }],
        };

        let (verts, tris) = render_mesh_to_arrays(&mesh);

        assert_eq!(verts.len(), 3);
        assert_eq!(tris.len(), 1);

        // Check vertex values (f32 → f64 conversion)
        assert!((verts[0][0] - 0.0).abs() < 1e-6);
        assert!((verts[1][0] - 1.0).abs() < 1e-6);
        assert!((verts[2][1] - 1.0).abs() < 1e-6);

        // Check triangle indices
        assert_eq!(tris[0], [0, 1, 2]);
    }

    #[test]
    fn render_mesh_to_arrays_two_triangles() {
        let mesh = RenderMesh {
            vertices: vec![
                0.0, 0.0, 0.0, // v0
                1.0, 0.0, 0.0, // v1
                1.0, 1.0, 0.0, // v2
                0.0, 1.0, 0.0, // v3
            ],
            normals: vec![0.0; 12],
            indices: vec![0, 1, 2, 0, 2, 3],
            face_ranges: vec![],
        };

        let (verts, tris) = render_mesh_to_arrays(&mesh);
        assert_eq!(verts.len(), 4);
        assert_eq!(tris.len(), 2);
        assert_eq!(tris[0], [0, 1, 2]);
        assert_eq!(tris[1], [0, 2, 3]);
    }

    #[test]
    fn render_mesh_to_arrays_empty() {
        let mesh = RenderMesh {
            vertices: vec![],
            normals: vec![],
            indices: vec![],
            face_ranges: vec![],
        };

        let (verts, tris) = render_mesh_to_arrays(&mesh);
        assert!(verts.is_empty());
        assert!(tris.is_empty());
    }

    #[test]
    fn build_surface_map_basic() {
        use crate::geometry::point::Point3;
        use crate::geometry::surface::Plane;
        use crate::topology::half_edge::FaceIdx;

        let mut solid_a = empty_waffle_solid();
        let mut solid_b = empty_waffle_solid();

        let plane_a = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        let plane_b = SurfaceGeom::Planar(Plane {
            origin: Point3::new(0.0, 0.0, 1.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        });

        solid_a.face_geometry.insert(FaceIdx(0), plane_a.clone());
        solid_b.face_geometry.insert(FaceIdx(0), plane_b.clone());

        let map = build_surface_map(&solid_a, &solid_b);

        assert_eq!(map.len(), 2);
        assert!(map.contains_key(&(MeshId::A, FaceIdx(0))));
        assert!(map.contains_key(&(MeshId::B, FaceIdx(0))));
    }

    #[test]
    fn bool_op_to_mesh_op_conversion() {
        assert_eq!(bool_op_to_mesh_op(BoolOp::Union), MeshBooleanOp::Union);
        assert_eq!(
            bool_op_to_mesh_op(BoolOp::Subtract),
            MeshBooleanOp::Subtract
        );
        assert_eq!(
            bool_op_to_mesh_op(BoolOp::Intersect),
            MeshBooleanOp::Intersect
        );
    }

    #[test]
    fn ssi_curve_to_curve_geom_line() {
        let curve = SSICurve::Line {
            start: [0.0, 0.0, 0.0],
            end: [1.0, 0.0, 0.0],
        };
        let result = ssi_curve_to_curve_geom(&curve);
        assert!(result.is_some());
        match result.unwrap() {
            CurveGeom::Linear(line) => {
                assert!((line.origin.x - 0.0).abs() < 1e-12);
                assert!((line.direction.x - 1.0).abs() < 1e-12);
            }
            _ => panic!("Expected Linear"),
        }
    }

    #[test]
    fn ssi_curve_to_curve_geom_circle() {
        let curve = SSICurve::Circle {
            center: [1.0, 2.0, 3.0],
            normal: [0.0, 0.0, 1.0],
            radius: 5.0,
        };
        let result = ssi_curve_to_curve_geom(&curve);
        assert!(result.is_some());
        match result.unwrap() {
            CurveGeom::Circular(c) => {
                assert!((c.center.x - 1.0).abs() < 1e-12);
                assert!((c.center.y - 2.0).abs() < 1e-12);
                assert!((c.radius - 5.0).abs() < 1e-12);
            }
            _ => panic!("Expected Circular"),
        }
    }

    #[test]
    fn ssi_curve_to_curve_geom_ellipse() {
        let curve = SSICurve::Ellipse {
            center: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            major_axis: [1.0, 0.0, 0.0],
            semi_major: 3.0,
            semi_minor: 2.0,
        };
        let result = ssi_curve_to_curve_geom(&curve);
        assert!(result.is_some());
        match result.unwrap() {
            CurveGeom::Elliptical(e) => {
                assert!((e.semi_major - 3.0).abs() < 1e-12);
                assert!((e.semi_minor - 2.0).abs() < 1e-12);
            }
            _ => panic!("Expected Elliptical"),
        }
    }

    #[test]
    fn ssi_curve_to_curve_geom_higher_order_returns_none() {
        let curve = SSICurve::Parabola {
            vertex: [0.0, 0.0, 0.0],
            axis_dir: [0.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            focal_length: 1.0,
            t_range: (-1.0, 1.0),
        };
        assert!(ssi_curve_to_curve_geom(&curve).is_none());
    }

    /// Helper to create an empty WaffleSolid for testing.
    fn empty_waffle_solid() -> WaffleSolid {
        use crate::topology::arena::TopoArena;
        WaffleSolid {
            arena: TopoArena::new(),
            face_map: BTreeMap::new(),
            edge_map: BTreeMap::new(),
            vertex_map: BTreeMap::new(),
            face_geometry: BTreeMap::new(),
            edge_geometry: BTreeMap::new(),
            cylinder_params: None,
            revolve_params: None,
            sphere_params: None,
            cone_params: None,
            torus_params: None,
            cached_face_polys: None,
            is_polygon_soup: false,
        }
    }

    #[test]
    fn yang_pipeline_disabled_by_default() {
        // Without YANG_BOOLEAN=1, the pipeline should return NotSupported.
        let solid = empty_waffle_solid();
        let mut next_id = 1u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };
        let result = yang_boolean_from_solids(&solid, &solid, BoolOp::Union, &mut id_alloc);
        assert!(
            result.is_err(),
            "Yang pipeline should be disabled by default"
        );
    }
}
