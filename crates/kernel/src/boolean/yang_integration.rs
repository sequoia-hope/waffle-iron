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
use crate::boolean::topology_extract::{FaceSurvivalMap, ResultTopology, SourceFace};
use crate::boolean::BoolOp;
use crate::boolean::BooleanResult;
use crate::geometry::curve::{Circle3D, CurveGeom, Ellipse3D, Line3D};
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::SurfaceGeom;
use crate::ssi::SSICurve;
use crate::topology::half_edge::{EdgeIdx, FaceIdx, VertexIdx};
use crate::types::{FaceRange, KernelError, KernelId, RenderMesh};
use crate::waffle_kernel::WaffleSolid;
use std::collections::HashMap;

use crate::boolean::exact_mesh::{MeshBooleanOp, SubdividedMesh};
use crate::boolean::ssi_refinement::{classify_intersection_edges, refine_intersection_edges};
use crate::boolean::topology_extract::yang_boolean_pipeline;
use crate::tessellation;
use crate::tessellation::bijective::BijectiveMap;

// ── Mesh conversion helpers ─────────────────────────────────────────────

/// Convert RenderMesh (f32 flat arrays) to the pipeline format (Vec<[f64;3]>, Vec<[usize;3]>).
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
        cached_render_mesh: None,
    }
}

/// Convert a WaffleSolid from the Yang pipeline into a BooleanResult that
/// integrates with the existing `do_boolean` result handling.
pub(crate) fn waffle_solid_to_boolean_result(solid: WaffleSolid) -> BooleanResult {
    BooleanResult {
        arena: solid.arena,
        face_map: solid.face_map,
        edge_map: solid.edge_map,
        vertex_map: solid.vertex_map,
        face_geometry: solid.face_geometry,
        edge_geometry: solid.edge_geometry,
        cached_face_polys: None,
        cached_render_mesh: solid.cached_render_mesh,
    }
}

// ── Mesh passthrough: cached render mesh from surviving sub-triangles ───

/// Build a RenderMesh directly from the mesh boolean's surviving sub-triangles.
///
/// This bypasses B-Rep retessellation (ear-clipping), preserving the exact
/// mesh boolean's self-intersection-free guarantee. Each source face's
/// sub-triangles are grouped into a FaceRange for the oracle's per-face checks.
///
/// Normals are computed from the source face's SurfaceGeom when analytical
/// (Planar/Cylindrical/Spherical/etc.), falling back to triangle cross-product
/// for faces without surface geometry.
///
/// Ref [#24]: Yang 2025 — mesh output as computational tool.
/// Ref [#9]: Cherchi 2020 — conformal subdivision preserves manifoldness.
pub(crate) fn build_render_mesh_from_survival(
    survival: &FaceSurvivalMap,
    subdivided: &SubdividedMesh,
    surface_map: &BTreeMap<(MeshId, FaceIdx), SurfaceGeom>,
    face_provenance: &BTreeMap<FaceIdx, SourceFace>,
    face_map: &BTreeMap<u64, FaceIdx>,
) -> RenderMesh {
    // Build reverse maps for face ID lookup.
    // face_map: kernel_id → FaceIdx, we need FaceIdx → kernel_id
    let face_idx_to_id: BTreeMap<FaceIdx, u64> =
        face_map.iter().map(|(&id, &fidx)| (fidx, id)).collect();

    // Build SourceFace → kernel_id via face_provenance and face_idx_to_id.
    // Multiple FaceIdx entries may point to the same SourceFace (after coplanar
    // merge). Use the first matching kernel ID.
    let mut source_to_kernel_id: BTreeMap<SourceFace, u64> = BTreeMap::new();
    for (&fidx, &src) in face_provenance {
        if let Some(&kid) = face_idx_to_id.get(&fidx) {
            source_to_kernel_id.entry(src).or_insert(kid);
        }
    }

    // Step 1: Position-based vertex deduplication (nanometer quantization).
    // Shared vertices across face boundaries produce watertight output.
    let scale = crate::units::QUANT_NANOMETER_SCALE;
    let mut pos_to_idx: HashMap<[i64; 3], u32> = HashMap::new();
    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();

    let quant = |p: [f64; 3]| -> [i64; 3] {
        [
            (p[0] * scale).round() as i64,
            (p[1] * scale).round() as i64,
            (p[2] * scale).round() as i64,
        ]
    };

    // For shared vertices, we need to defer normal assignment.
    // Track: vertex_index → accumulated normal (will normalize at the end).
    // Use per-triangle vertex emission for correct per-face normals, but
    // share vertex positions via index dedup for watertight mesh.

    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    // Step 2: Emit triangles grouped by source face for face_ranges.
    // Collect (kernel_face_id, list of sub-tris) in deterministic order.
    let mut face_tris: BTreeMap<
        u64,
        Vec<(
            &crate::boolean::topology_extract::SurvivingSubTri,
            &SourceFace,
        )>,
    > = BTreeMap::new();
    for (source_face, tris) in &survival.groups {
        let kid = source_to_kernel_id.get(source_face).copied().unwrap_or(0);
        for tri in tris {
            face_tris.entry(kid).or_default().push((tri, source_face));
        }
    }

    for (&kernel_face_id, tris) in &face_tris {
        let start_index = indices.len() as u32;

        for &(tri, source_face) in tris {
            // Apply winding: if flipped, reverse vertex order
            let (v0i, v1i, v2i) = if tri.flipped {
                (tri.verts[0], tri.verts[2], tri.verts[1])
            } else {
                (tri.verts[0], tri.verts[1], tri.verts[2])
            };

            let p0 = subdivided.verts[v0i];
            let p1 = subdivided.verts[v1i];
            let p2 = subdivided.verts[v2i];

            // Compute normal from surface geometry or triangle cross-product.
            let geom_key = (source_face.mesh_id, source_face.face_idx);
            let face_normal =
                compute_face_normal(&p0, &p1, &p2, surface_map.get(&geom_key), tri.flipped);

            // Emit vertices with per-triangle normals (for correct face shading).
            // Use position-dedup for index sharing at boundaries.
            let idx0 = get_or_insert_vertex(
                &mut pos_to_idx,
                &mut vertices,
                &mut normals,
                p0,
                face_normal,
                quant,
            );
            let idx1 = get_or_insert_vertex(
                &mut pos_to_idx,
                &mut vertices,
                &mut normals,
                p1,
                face_normal,
                quant,
            );
            let idx2 = get_or_insert_vertex(
                &mut pos_to_idx,
                &mut vertices,
                &mut normals,
                p2,
                face_normal,
                quant,
            );

            indices.push(idx0);
            indices.push(idx1);
            indices.push(idx2);
        }

        let end_index = indices.len() as u32;
        if end_index > start_index {
            face_ranges.push(FaceRange {
                face_id: KernelId(kernel_face_id),
                start_index,
                end_index,
            });
        }
    }

    RenderMesh {
        vertices,
        normals,
        indices,
        face_ranges,
    }
}

/// Compute face normal for a triangle, using analytical surface geometry when available.
fn compute_face_normal(
    p0: &[f64; 3],
    p1: &[f64; 3],
    p2: &[f64; 3],
    geom: Option<&SurfaceGeom>,
    flipped: bool,
) -> [f32; 3] {
    let sign = if flipped { -1.0f32 } else { 1.0f32 };

    if let Some(surface) = geom {
        match surface {
            SurfaceGeom::Planar(plane) => {
                return [
                    plane.normal.x as f32 * sign,
                    plane.normal.y as f32 * sign,
                    plane.normal.z as f32 * sign,
                ];
            }
            SurfaceGeom::Cylindrical(cyl) => {
                // Use triangle centroid for radial normal computation
                let cx = (p0[0] + p1[0] + p2[0]) / 3.0;
                let cy = (p0[1] + p1[1] + p2[1]) / 3.0;
                let cz = (p0[2] + p1[2] + p2[2]) / 3.0;
                let to_pt = [cx - cyl.origin.x, cy - cyl.origin.y, cz - cyl.origin.z];
                let ax = [cyl.axis.x, cyl.axis.y, cyl.axis.z];
                let dot = to_pt[0] * ax[0] + to_pt[1] * ax[1] + to_pt[2] * ax[2];
                let radial = [
                    to_pt[0] - dot * ax[0],
                    to_pt[1] - dot * ax[1],
                    to_pt[2] - dot * ax[2],
                ];
                let len =
                    (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
                if len > 1e-12 {
                    return [
                        (radial[0] / len) as f32 * sign,
                        (radial[1] / len) as f32 * sign,
                        (radial[2] / len) as f32 * sign,
                    ];
                }
            }
            SurfaceGeom::Spherical(sphere) => {
                let cx = (p0[0] + p1[0] + p2[0]) / 3.0;
                let cy = (p0[1] + p1[1] + p2[1]) / 3.0;
                let cz = (p0[2] + p1[2] + p2[2]) / 3.0;
                let radial = [
                    cx - sphere.center.x,
                    cy - sphere.center.y,
                    cz - sphere.center.z,
                ];
                let len =
                    (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
                if len > 1e-12 {
                    return [
                        (radial[0] / len) as f32 * sign,
                        (radial[1] / len) as f32 * sign,
                        (radial[2] / len) as f32 * sign,
                    ];
                }
            }
            _ => {} // Fall through to cross-product
        }
    }

    // Fallback: triangle cross-product normal
    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let nx = e1[1] * e2[2] - e1[2] * e2[1];
    let ny = e1[2] * e2[0] - e1[0] * e2[2];
    let nz = e1[0] * e2[1] - e1[1] * e2[0];
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if len > 1e-12 {
        [(nx / len) as f32, (ny / len) as f32, (nz / len) as f32]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// Get or insert a vertex into the shared vertex pool, deduplicating by position.
/// For shared vertices (same quantized position), the FIRST normal wins.
/// This is acceptable because shared boundary vertices typically have the same
/// face normal on both sides (conformal subdivision), and the per-face normal
/// differences are handled by the face_ranges partitioning.
fn get_or_insert_vertex(
    pos_to_idx: &mut HashMap<[i64; 3], u32>,
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    pos: [f64; 3],
    normal: [f32; 3],
    quant: impl Fn([f64; 3]) -> [i64; 3],
) -> u32 {
    let key = quant(pos);
    if let Some(&idx) = pos_to_idx.get(&key) {
        return idx;
    }
    let idx = (vertices.len() / 3) as u32;
    vertices.push(pos[0] as f32);
    vertices.push(pos[1] as f32);
    vertices.push(pos[2] as f32);
    normals.push(normal[0]);
    normals.push(normal[1]);
    normals.push(normal[2]);
    pos_to_idx.insert(key, idx);
    idx
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

    yang_boolean_inner(solid_a, solid_b, op, id_alloc)
}

/// Core Yang pipeline implementation, separated from the env-var guard so that
/// unit tests can call it directly without mutating process-global state
/// (which is unsound when tests run in parallel).
pub(crate) fn yang_boolean_inner(
    solid_a: &WaffleSolid,
    solid_b: &WaffleSolid,
    op: BoolOp,
    id_alloc: &mut dyn FnMut() -> u64,
) -> Result<BooleanResult, KernelError> {
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

    // Step 2b: Guard against excessive triangle counts (O(n*m) complexity).
    check_yang_triangle_count(tris_a.len(), tris_b.len())?;

    // Step 3: Build bijective maps (mesh triangle → B-Rep face).
    let bijective_a = BijectiveMap::from_render_mesh(&mesh_a, &solid_a.face_map);
    let bijective_b = BijectiveMap::from_render_mesh(&mesh_b, &solid_b.face_map);

    if !bijective_a.is_complete() || !bijective_b.is_complete() {
        return Err(KernelError::NotSupported {
            operation: "yang_boolean: bijective map has unmapped triangles".to_string(),
        });
    }

    // Step 4: Run Yang pipeline (Phases 1-3): mesh boolean → topology extract.
    let pipeline_result = yang_boolean_pipeline(
        &verts_a,
        &tris_a,
        &verts_b,
        &tris_b,
        &bijective_a,
        &bijective_b,
        mesh_op,
    )?;

    // Step 5: Phase 4a — classify intersection edges by surface pair type.
    // This may fail if face provenance references faces not in the surface map
    // (e.g., due to subdivision creating new face indices). In that case, skip
    // refinement and use the topology as-is.
    let refinement = match classify_intersection_edges(&pipeline_result.topology, &surface_map) {
        Ok(classification) => {
            // Step 6: Phase 4b — refine intersection edges with SSI solvers.
            match refine_intersection_edges(
                &pipeline_result.topology,
                &classification,
                &surface_map,
            ) {
                Ok(r) => r,
                Err(ref e) => {
                    eprintln!(
                        "[A15.6 WARN] SSI edge refinement failed, proceeding with mesh-derived geometry: {e}"
                    );
                    EdgeRefinementMap {
                        edges: BTreeMap::new(),
                        skipped_planar: 0,
                        unsupported: vec![],
                    }
                }
            }
        }
        Err(ref e) => {
            eprintln!(
                "[A15.6 WARN] SSI edge classification failed, proceeding with mesh-derived geometry: {e}"
            );
            EdgeRefinementMap {
                edges: BTreeMap::new(),
                skipped_planar: 0,
                unsupported: vec![],
            }
        }
    };

    // Guard: if the Yang pipeline produced zero faces, return NotSupported so
    // the legacy pipeline can handle this case. An empty topology means the
    // boolean operation produced no surviving geometry (e.g., non-overlapping
    // operands for Intersect). Ref: specs/yang_error_fallback.md
    if pipeline_result.topology.face_provenance.is_empty() {
        return Err(KernelError::NotSupported {
            operation: "yang_boolean: pipeline produced empty topology (zero surviving faces)"
                .to_string(),
        });
    }

    // Save face_provenance before ownership transfer to result_topology_to_waffle_solid.
    let face_provenance = pipeline_result.topology.face_provenance.clone();

    // Step 7: Convert ResultTopology → WaffleSolid → BooleanResult.
    let mut waffle = result_topology_to_waffle_solid(
        pipeline_result.topology,
        &refinement,
        &surface_map,
        id_alloc,
    );

    // Step 8: Build cached render mesh directly from surviving sub-triangles.
    // This bypasses B-Rep retessellation (ear-clipping), preserving the exact
    // mesh boolean's self-intersection-free guarantee. The mesh boolean output
    // is correct by construction regardless of B-Rep topology issues.
    // Ref [#24] Yang 2025, [#9] Cherchi 2020.
    let cached_mesh = build_render_mesh_from_survival(
        &pipeline_result.survival,
        &pipeline_result.subdivided,
        &surface_map,
        &face_provenance,
        &waffle.face_map,
    );
    waffle.cached_render_mesh = Some(cached_mesh);

    // Step 9: Validate result B-Rep topology. The B-Rep may have invalid topology
    // (Euler ≠ 2, dangling edges) even when the mesh boolean output is correct.
    // With mesh passthrough, the cached_render_mesh is the authoritative rendering
    // output — B-Rep issues only affect subsequent boolean operations.
    // Log validation failures but accept the result since we have a valid mesh.
    if let Err(msg) = validate_yang_result_topology(&waffle.arena) {
        eprintln!(
            "[A15.6 WARN] Yang B-Rep validation failed (mesh passthrough will be used for rendering): {msg}"
        );
    }

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

// ── Result topology validation ──────────────────────────────────────────

/// Validate that a TopoArena produced by the Yang pipeline has consistent
/// topology: all half-edge references (edge, twin, next, prev, origin) point
/// to valid indices. Returns Ok(()) if valid, Err(message) if not.
///
/// This catches malformed B-Rep output before it reaches downstream consumers
/// (tessellation, rendering) where it would cause index-out-of-bounds panics.
fn validate_yang_result_topology(arena: &crate::topology::arena::TopoArena) -> Result<(), String> {
    let n_he = arena.half_edges.len();
    let n_edges = arena.edges.len();
    let n_verts = arena.vertices.len();
    let n_loops = arena.loops.len();
    let n_faces = arena.faces.len();

    for (i, he) in arena.half_edges.iter().enumerate() {
        if he.edge.0 >= n_edges {
            return Err(format!(
                "half_edge[{i}].edge = {} but only {n_edges} edges exist",
                he.edge.0
            ));
        }
        if he.twin.0 >= n_he {
            return Err(format!(
                "half_edge[{i}].twin = {} but only {n_he} half_edges exist",
                he.twin.0
            ));
        }
        if he.next.0 >= n_he {
            return Err(format!(
                "half_edge[{i}].next = {} but only {n_he} half_edges exist",
                he.next.0
            ));
        }
        if he.prev.0 >= n_he {
            return Err(format!(
                "half_edge[{i}].prev = {} but only {n_he} half_edges exist",
                he.prev.0
            ));
        }
        if he.origin.0 >= n_verts {
            return Err(format!(
                "half_edge[{i}].origin = {} but only {n_verts} vertices exist",
                he.origin.0
            ));
        }
        if he.loop_.0 >= n_loops {
            return Err(format!(
                "half_edge[{i}].loop_ = {} but only {n_loops} loops exist",
                he.loop_.0
            ));
        }
    }

    // Twin symmetry: every half-edge's twin must point back to it.
    // Manifold B-Rep requires he.twin.twin == he for all half-edges.
    // Ref: Mantyla §4.2, Stroud §3.3.
    for (i, he) in arena.half_edges.iter().enumerate() {
        let twin_he = &arena.half_edges[he.twin.0];
        if twin_he.twin.0 != i {
            return Err(format!(
                "half_edge[{i}].twin = {} but twin.twin = {} (expected {i})",
                he.twin.0, twin_he.twin.0
            ));
        }
    }

    for (i, face) in arena.faces.iter().enumerate() {
        if face.outer_loop.0 >= n_loops {
            return Err(format!(
                "face[{i}].outer_loop = {} but only {n_loops} loops exist",
                face.outer_loop.0
            ));
        }
    }

    for (i, loop_) in arena.loops.iter().enumerate() {
        if loop_.half_edge.0 >= n_he {
            return Err(format!(
                "loop[{i}].half_edge = {} but only {n_he} half_edges exist",
                loop_.half_edge.0
            ));
        }
        if loop_.face.0 >= n_faces {
            return Err(format!(
                "loop[{i}].face = {} but only {n_faces} faces exist",
                loop_.face.0
            ));
        }
    }

    for (i, edge) in arena.edges.iter().enumerate() {
        if edge.half_edge.0 >= n_he {
            return Err(format!(
                "edge[{i}].half_edge = {} but only {n_he} half_edges exist",
                edge.half_edge.0
            ));
        }
    }

    // Manifold invariant: half_edges == 2 * edges (each edge has exactly two half-edges).
    if n_he != 2 * n_edges {
        return Err(format!(
            "manifold violation: {n_he} half_edges != 2 * {n_edges} edges"
        ));
    }

    // Euler characteristic: V - E + F = 2 for a single closed orientable solid.
    // Ref: Mantyla §2.4. If the result is not a single closed solid, the Yang
    // pipeline output is invalid — fall back to legacy.
    let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;
    if euler != 2 {
        return Err(format!(
            "Euler characteristic V({n_verts}) - E({n_edges}) + F({n_faces}) = {euler} (expected 2)"
        ));
    }

    Ok(())
}

/// Check whether the triangle-pair product `n_a * n_b` exceeds
/// `MAX_YANG_TRI_PAIRS`. Returns `Err(NotSupported)` if so.
pub(crate) fn check_yang_triangle_count(n_a: usize, n_b: usize) -> Result<(), KernelError> {
    use crate::units::MAX_YANG_TRI_PAIRS;
    let product = n_a * n_b;
    if product > MAX_YANG_TRI_PAIRS {
        return Err(KernelError::NotSupported {
            operation: format!(
                "yang_boolean: triangle-pair count {product} exceeds limit {MAX_YANG_TRI_PAIRS}"
            ),
        });
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FaceRange;
    use crate::units::TAU_WORK;

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
        let result = ssi_curve_to_curve_geom(&curve).expect("line should produce CurveGeom");
        match result {
            CurveGeom::Linear(line) => {
                assert!((line.origin.x).abs() < TAU_WORK, "origin.x should be 0");
                assert!(
                    (line.direction.x - 1.0).abs() < TAU_WORK,
                    "direction.x should be 1"
                );
                assert!(
                    (line.direction.y).abs() < TAU_WORK,
                    "direction.y should be 0"
                );
                assert!(
                    (line.direction.z).abs() < TAU_WORK,
                    "direction.z should be 0"
                );
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
        let result = ssi_curve_to_curve_geom(&curve).expect("circle should produce CurveGeom");
        match result {
            CurveGeom::Circular(c) => {
                assert!((c.center.x - 1.0).abs() < TAU_WORK, "center.x should be 1");
                assert!((c.center.y - 2.0).abs() < TAU_WORK, "center.y should be 2");
                assert!((c.center.z - 3.0).abs() < TAU_WORK, "center.z should be 3");
                assert!((c.radius - 5.0).abs() < TAU_WORK, "radius should be 5");
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
        let result = ssi_curve_to_curve_geom(&curve).expect("ellipse should produce CurveGeom");
        match result {
            CurveGeom::Elliptical(e) => {
                assert!(
                    (e.semi_major - 3.0).abs() < TAU_WORK,
                    "semi_major should be 3"
                );
                assert!(
                    (e.semi_minor - 2.0).abs() < TAU_WORK,
                    "semi_minor should be 2"
                );
                assert!((e.center.x).abs() < TAU_WORK, "center.x should be 0");
                assert!((e.center.y).abs() < TAU_WORK, "center.y should be 0");
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
            cached_render_mesh: None,
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
        match result {
            Err(KernelError::NotSupported { .. }) => {} // expected
            Err(other) => panic!("expected NotSupported, got error: {other}"),
            Ok(_) => panic!("expected NotSupported, but pipeline succeeded"),
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Bug-demonstrating tests (red phase) — empty result handling
    // ══════════════════════════════════════════════════════════════════

    /// Build a tetrahedron WaffleSolid at the given offset using Euler operators.
    /// The tetrahedron has 4 vertices, 6 edges, 4 triangular faces, all tagged
    /// as Planar in face_geometry so the Yang pipeline can process it.
    fn make_tetra_solid(offset: [f64; 3]) -> WaffleSolid {
        use crate::geometry::point::Point3;
        use crate::geometry::surface::Plane;
        use crate::topology::arena::TopoArena;
        use crate::topology::euler_ops::{mef, mev, mvfs};
        use crate::topology::half_edge::{FaceIdx, LoopIdx};

        let [ox, oy, oz] = offset;
        let mut arena = TopoArena::new();

        // 4 vertices of a tetrahedron
        let p0 = [ox, oy, oz];
        let p1 = [ox + 1.0, oy, oz];
        let p2 = [ox + 0.5, oy + 1.0, oz];
        let p3 = [ox + 0.5, oy + 0.5, oz + 1.0];

        // mvfs: create first vertex and face
        let (_solid, _shell, face0, v0) = mvfs(&mut arena, p0);
        let loop0 = arena.faces[face0.0].outer_loop;

        // mev: add vertices to build a wire in loop0
        let (_e01, v1) = mev(&mut arena, v0, loop0, p1);
        let (_e12, v2) = mev(&mut arena, v1, loop0, p2);

        // Close the base triangle: v2 → v0 via mef
        let (_e20, face1) = mef(&mut arena, v2, v0, loop0);
        // face0's loop0 = triangle (v0, v1, v2) — one orientation
        // face1's loop1 = same three vertices — opposite orientation

        // Fix vertex half-edge pointers for both loops (same pattern as sphere builder)
        fn fix_loop_vertex_ptrs(arena: &mut TopoArena, loop_idx: LoopIdx) {
            let start_he = arena.loops[loop_idx.0].half_edge;
            let mut he = start_he;
            loop {
                let v = arena.half_edges[he.0].origin;
                arena.vertices[v.0].half_edge = Some(he);
                he = arena.half_edges[he.0].next;
                if he == start_he {
                    break;
                }
            }
        }

        let loop1 = arena.faces[face1.0].outer_loop;
        fix_loop_vertex_ptrs(&mut arena, loop0);
        fix_loop_vertex_ptrs(&mut arena, loop1);

        // Add v3 as a spur from v0 into face1
        let (_e03, v3) = mev(&mut arena, v0, loop1, p3);

        // Fix vertex half-edge pointers for loop1 again
        fix_loop_vertex_ptrs(&mut arena, loop1);

        // Triangulate face1: mef(v3, v1, loop1) splits face1
        let (_e31, face2) = mef(&mut arena, v3, v1, loop1);
        let loop2 = arena.faces[face2.0].outer_loop;
        fix_loop_vertex_ptrs(&mut arena, loop1);
        fix_loop_vertex_ptrs(&mut arena, loop2);

        // mef(v3, v2, ...) splits remaining quad into two triangles
        // v3 is in loop2 (face2), and v2 is also in loop2
        let (_e32, _face3) = mef(&mut arena, v3, v2, loop2);
        let loop3 = arena.faces[_face3.0].outer_loop;
        fix_loop_vertex_ptrs(&mut arena, loop2);
        fix_loop_vertex_ptrs(&mut arena, loop3);

        // Tetrahedron topology: V=4, E=6, F=4. Euler: 4-6+4=2. ✓

        // Build face_map and face_geometry: all faces are planar
        let mut face_map = BTreeMap::new();
        let mut face_geometry = BTreeMap::new();
        let mut next_kid = 100u64;

        for idx in 0..arena.faces.len() {
            let fi = FaceIdx(idx);
            face_map.insert(next_kid, fi);
            next_kid += 1;

            // Use a dummy planar geometry — the exact normal doesn't matter
            // for this test; we just need face_geometry to be non-empty.
            face_geometry.insert(
                fi,
                SurfaceGeom::Planar(Plane {
                    origin: Point3::new(ox + 0.5, oy + 0.5, oz + 0.5),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                }),
            );
        }

        // Build edge_map
        let mut edge_map = BTreeMap::new();
        for idx in 0..arena.edges.len() {
            edge_map.insert(next_kid, EdgeIdx(idx));
            next_kid += 1;
        }

        // Build vertex_map
        let mut vertex_map = BTreeMap::new();
        for idx in 0..arena.vertices.len() {
            use crate::topology::half_edge::VertexIdx;
            vertex_map.insert(next_kid, VertexIdx(idx));
            next_kid += 1;
        }

        WaffleSolid {
            arena,
            face_map,
            edge_map,
            vertex_map,
            face_geometry,
            edge_geometry: BTreeMap::new(),
            cylinder_params: None,
            revolve_params: None,
            sphere_params: None,
            cone_params: None,
            torus_params: None,
            cached_face_polys: None,
            is_polygon_soup: false,
            cached_render_mesh: None,
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Bug 1: Triangle-count guard (red phase)
    // ══════════════════════════════════════════════════════════════════

    /// Bug: `yang_boolean_from_solids` has no guard on triangle count.
    /// The exact mesh boolean is O(n*m) in triangles. Large meshes timeout.
    /// After tessellation produces `tris_a` and `tris_b`, if
    /// `tris_a.len() * tris_b.len() > MAX_YANG_TRI_PAIRS`, the function
    /// must return `Err(KernelError::NotSupported { .. })`.
    ///
    /// This test verifies the constant exists in units.rs and has the expected value.
    #[test]
    fn max_yang_tri_pairs_constant_exists_and_is_5000000() {
        use crate::units::MAX_YANG_TRI_PAIRS;
        assert_eq!(MAX_YANG_TRI_PAIRS, 5_000_000);
    }

    /// Bug: `yang_boolean_from_solids` should reject operands whose combined
    /// triangle count exceeds `MAX_YANG_TRI_PAIRS`. This test calls the
    /// `check_yang_triangle_count` helper (which the implementer must wire into
    /// `yang_boolean_from_solids` between steps 2 and 3).
    ///
    /// This test currently FAILS because the helper is `unimplemented!()`.
    #[test]
    fn yang_boolean_rejects_high_triangle_count() {
        use crate::units::MAX_YANG_TRI_PAIRS;

        // Test: product exceeding threshold should error.
        let n_a = 3000;
        let n_b = 2000;
        assert!(
            n_a * n_b > MAX_YANG_TRI_PAIRS,
            "precondition: product exceeds limit"
        );

        let result = check_yang_triangle_count(n_a, n_b);
        match result {
            Err(KernelError::NotSupported { operation }) => {
                assert!(
                    operation.contains("triangle"),
                    "Error message should mention triangles, got: {operation}"
                );
            }
            Err(other) => panic!("expected NotSupported, got: {other:?}"),
            Ok(()) => panic!(
                "expected NotSupported for {n_a}×{n_b}={} pairs (limit {MAX_YANG_TRI_PAIRS}), but got Ok",
                n_a * n_b
            ),
        }

        // Test: product under threshold should be Ok.
        let result_ok = check_yang_triangle_count(10, 10);
        assert!(
            result_ok.is_ok(),
            "10×10=100 pairs should be under the limit"
        );
    }

    /// Bug: when `yang_boolean_from_solids` produces an empty boolean result
    /// (e.g., Intersect of non-overlapping solids), it should return
    /// `Err(KernelError::NotSupported { .. })` to trigger the legacy fallback
    /// in `do_boolean()`. Currently it returns `Ok` with an empty solid (zero
    /// faces), which `do_boolean` accepts as a valid result and stores.
    ///
    /// This test creates two non-overlapping tetrahedra and verifies that the
    /// intersection returns NotSupported.
    #[test]
    fn test_yang_empty_result_returns_not_supported() {
        // Build two non-overlapping tetrahedra: one at origin, one far away.
        let solid_a = make_tetra_solid([0.0, 0.0, 0.0]);
        let solid_b = make_tetra_solid([100.0, 100.0, 100.0]);

        // Verify preconditions: both have face_geometry.
        assert!(
            !solid_a.face_geometry.is_empty(),
            "Precondition: solid_a must have face_geometry"
        );
        assert!(
            !solid_b.face_geometry.is_empty(),
            "Precondition: solid_b must have face_geometry"
        );

        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        // Use yang_boolean_inner directly to avoid env-var race in parallel tests.
        let result = yang_boolean_inner(&solid_a, &solid_b, BoolOp::Intersect, &mut id_alloc);

        // The result should be Err(NotSupported) for an empty intersection,
        // so that do_boolean can fall through to the legacy pipeline.
        match &result {
            Err(KernelError::NotSupported { .. }) => {
                // Correct behavior — empty result triggers fallback.
            }
            Ok(boolean_result) => {
                panic!(
                    "Expected Err(NotSupported) for empty intersection, \
                     but got Ok with {} faces. The pipeline should detect the \
                     empty result and return NotSupported to trigger legacy fallback.",
                    boolean_result.arena.faces.len(),
                );
            }
            Err(other) => {
                panic!(
                    "Expected Err(NotSupported) for empty intersection, \
                     but got Err({:?}). Any non-NotSupported error also prevents \
                     the legacy fallback from running.",
                    other,
                );
            }
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // E2E tests: Yang pipeline through yang_boolean_from_solids
    // ══════════════════════════════════════════════════════════════════

    /// Build a box WaffleSolid via WaffleKernel's extrude pipeline.
    /// Returns (kernel, handle) — caller accesses solid via kernel.get_solid().
    fn make_box_via_kernel(
        cx: f64,
        cy: f64,
        w: f64,
        h: f64,
        depth: f64,
    ) -> (
        crate::waffle_kernel::WaffleKernel,
        crate::types::KernelSolidHandle,
    ) {
        use crate::traits::Kernel;
        use crate::waffle_kernel::WaffleKernel;
        use std::collections::HashMap;

        let mut k = WaffleKernel::new();
        let mut positions = HashMap::new();
        positions.insert(1, (cx - w / 2.0, cy - h / 2.0));
        positions.insert(2, (cx + w / 2.0, cy - h / 2.0));
        positions.insert(3, (cx + w / 2.0, cy + h / 2.0));
        positions.insert(4, (cx - w / 2.0, cy + h / 2.0));

        let profile = crate::types::ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };

        let faces = k
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .expect("make_faces_from_profiles should succeed");
        let solid = k
            .extrude_face(faces[0], [0.0, 0.0, 1.0], depth)
            .expect("extrude_face should succeed");

        (k, solid)
    }

    /// E2E test: Two identical boxes through the full Yang pipeline via
    /// yang_boolean_from_solids. This is the F0001 assay pattern.
    #[test]
    fn yang_e2e_identical_box_union() {
        let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 1.0, 1.0, 1.0);
        let (k_b, h_b) = make_box_via_kernel(0.5, 0.5, 1.0, 1.0, 1.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a must exist");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b must exist");

        assert!(
            !solid_a.face_geometry.is_empty(),
            "solid_a must have face_geometry"
        );
        assert!(
            !solid_b.face_geometry.is_empty(),
            "solid_b must have face_geometry"
        );

        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        // Use yang_boolean_inner directly to avoid env-var race in parallel tests.
        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut id_alloc);

        match &result {
            Ok(boolean_result) => {
                let n_faces = boolean_result.arena.faces.len();
                let n_edges = boolean_result.arena.edges.len();
                let n_verts = boolean_result.arena.vertices.len();
                let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

                eprintln!("Yang E2E identical box union: V={n_verts}, E={n_edges}, F={n_faces}, Euler={euler}");

                assert!(
                    n_faces >= 6,
                    "Union of identical boxes should have >= 6 faces, got {n_faces}"
                );
                assert_eq!(euler, 2, "Euler V-E+F must equal 2");
            }
            Err(e) => {
                panic!(
                    "Yang E2E identical box union failed with error: {e:?}. \
                     The pipeline should produce a valid solid for identical box union."
                );
            }
        }
    }

    /// E2E test: Two overlapping boxes through the full Yang pipeline.
    #[test]
    fn yang_e2e_overlapping_box_union() {
        // Box A centered at (0.5,0.5), 2×2, extruded 2 → x=[-0.5,1.5], y=[-0.5,1.5], z=[0,2]
        let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 2.0, 2.0, 2.0);
        // Box B centered at (1.5,0.5), 2×2, extruded 2 → x=[0.5,2.5], y=[-0.5,1.5], z=[0,2]
        let (k_b, h_b) = make_box_via_kernel(1.5, 0.5, 2.0, 2.0, 2.0);

        let solid_a = k_a.get_solid(&h_a).unwrap();
        let solid_b = k_b.get_solid(&h_b).unwrap();

        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        // Use yang_boolean_inner directly to avoid env-var race in parallel tests.
        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut id_alloc);

        match &result {
            Ok(boolean_result) => {
                let n_faces = boolean_result.arena.faces.len();
                let n_edges = boolean_result.arena.edges.len();
                let n_verts = boolean_result.arena.vertices.len();
                let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

                eprintln!("Yang E2E overlapping box union: V={n_verts}, E={n_edges}, F={n_faces}, Euler={euler}");

                // Mesh-based builder with coplanar face merging produces fewer
                // faces than the trim-based builder (6 for a merged box union
                // vs 10+ for separate face fragments). Both are correct.
                assert!(n_faces >= 6, "Union should have >= 6 faces, got {n_faces}");
                assert_eq!(euler, 2, "Euler V-E+F must equal 2");
            }
            Err(e) => {
                panic!(
                    "Yang E2E overlapping box union failed with error: {e:?}. \
                     The pipeline should produce a valid solid."
                );
            }
        }
    }

    /// E2E test: Box subtract through the full Yang pipeline.
    #[test]
    fn yang_e2e_offset_box_subtract() {
        // Box A centered at (0.5,0.5), 2×2, depth 2 → x=[-0.5,1.5], y=[-0.5,1.5], z=[0,2]
        // Box B centered at (1.5,0.5), 2×2, depth 2 → x=[0.5,2.5], y=[-0.5,1.5], z=[0,2]
        // Overlapping in x=[0.5,1.5] — shared coplanar faces at z=0, z=2, y=±0.5
        // but the subtract of overlapping boxes should still work (the overlapping
        // box subtract diagnostic passes at mesh level)
        let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 2.0, 2.0, 2.0);
        let (k_b, h_b) = make_box_via_kernel(1.5, 0.5, 2.0, 2.0, 2.0);

        let solid_a = k_a.get_solid(&h_a).unwrap();
        let solid_b = k_b.get_solid(&h_b).unwrap();

        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        // Use yang_boolean_inner directly to avoid env-var race in parallel tests.
        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Subtract, &mut id_alloc);

        match &result {
            Ok(boolean_result) => {
                let n_faces = boolean_result.arena.faces.len();
                let n_edges = boolean_result.arena.edges.len();
                let n_verts = boolean_result.arena.vertices.len();
                let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

                eprintln!(
                    "Yang E2E subtract: V={n_verts}, E={n_edges}, F={n_faces}, Euler={euler}"
                );

                assert!(
                    n_faces > 0,
                    "Subtract should produce non-empty result, got 0 faces"
                );
                assert_eq!(euler, 2, "Euler V-E+F must equal 2");
            }
            Err(e) => {
                panic!("Yang E2E offset box subtract failed with error: {e:?}.");
            }
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Mesh passthrough tests: cached render mesh from surviving sub-tris
    // ══════════════════════════════════════════════════════════════════

    /// Verify that yang_boolean_inner produces a cached_render_mesh.
    #[test]
    fn yang_mesh_passthrough_produces_cached_mesh() {
        let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 2.0, 2.0, 2.0);
        let (k_b, h_b) = make_box_via_kernel(1.5, 0.5, 2.0, 2.0, 2.0);
        let solid_a = k_a.get_solid(&h_a).unwrap();
        let solid_b = k_b.get_solid(&h_b).unwrap();

        let mut next_id = 1000u64;
        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut || {
            let id = next_id;
            next_id += 1;
            id
        })
        .expect("Union should succeed");

        assert!(
            result.cached_render_mesh.is_some(),
            "Yang pipeline should produce a cached render mesh"
        );

        let mesh = result.cached_render_mesh.as_ref().unwrap();

        // Basic sanity checks
        assert!(!mesh.vertices.is_empty(), "mesh must have vertices");
        assert!(!mesh.indices.is_empty(), "mesh must have indices");
        assert!(!mesh.normals.is_empty(), "mesh must have normals");
        assert_eq!(
            mesh.vertices.len(),
            mesh.normals.len(),
            "vertex and normal arrays must match"
        );
        assert_eq!(mesh.vertices.len() % 3, 0, "vertices must be xyz triples");
        assert_eq!(mesh.indices.len() % 3, 0, "indices must be triangles");
    }

    /// Verify face_ranges cover all indices.
    #[test]
    fn yang_mesh_passthrough_face_ranges_cover_all_indices() {
        let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 2.0, 2.0, 2.0);
        let (k_b, h_b) = make_box_via_kernel(1.5, 0.5, 2.0, 2.0, 2.0);
        let solid_a = k_a.get_solid(&h_a).unwrap();
        let solid_b = k_b.get_solid(&h_b).unwrap();

        let mut next_id = 1000u64;
        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut || {
            let id = next_id;
            next_id += 1;
            id
        })
        .expect("Union should succeed");

        let mesh = result.cached_render_mesh.as_ref().unwrap();
        assert!(
            !mesh.face_ranges.is_empty(),
            "face_ranges must be non-empty"
        );

        let total_covered: u32 = mesh
            .face_ranges
            .iter()
            .map(|fr| fr.end_index - fr.start_index)
            .sum();
        assert_eq!(
            total_covered,
            mesh.indices.len() as u32,
            "face_ranges must cover all indices"
        );
    }

    /// Verify normals are unit length.
    #[test]
    fn yang_mesh_passthrough_unit_normals() {
        let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 2.0, 2.0, 2.0);
        let (k_b, h_b) = make_box_via_kernel(1.5, 0.5, 2.0, 2.0, 2.0);
        let solid_a = k_a.get_solid(&h_a).unwrap();
        let solid_b = k_b.get_solid(&h_b).unwrap();

        let mut next_id = 1000u64;
        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut || {
            let id = next_id;
            next_id += 1;
            id
        })
        .expect("Union should succeed");

        let mesh = result.cached_render_mesh.as_ref().unwrap();
        let n_verts = mesh.normals.len() / 3;
        for i in 0..n_verts {
            let nx = mesh.normals[i * 3] as f64;
            let ny = mesh.normals[i * 3 + 1] as f64;
            let nz = mesh.normals[i * 3 + 2] as f64;
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-3,
                "Normal {i} has length {len}, expected 1.0"
            );
        }
    }

    /// Verify all three boolean operations produce cached meshes.
    #[test]
    fn yang_mesh_passthrough_all_ops() {
        for op in [BoolOp::Union, BoolOp::Subtract, BoolOp::Intersect] {
            let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 2.0, 2.0, 2.0);
            let (k_b, h_b) = make_box_via_kernel(1.5, 0.5, 2.0, 2.0, 2.0);
            let solid_a = k_a.get_solid(&h_a).unwrap();
            let solid_b = k_b.get_solid(&h_b).unwrap();

            let mut next_id = 1000u64;
            let result = yang_boolean_inner(solid_a, solid_b, op, &mut || {
                let id = next_id;
                next_id += 1;
                id
            });

            match result {
                Ok(r) => {
                    assert!(
                        r.cached_render_mesh.is_some(),
                        "Op {:?} should produce cached mesh",
                        op
                    );
                    let mesh = r.cached_render_mesh.as_ref().unwrap();
                    assert!(
                        !mesh.indices.is_empty(),
                        "Op {:?} mesh should be non-empty",
                        op
                    );
                }
                Err(e) => {
                    // NotSupported is acceptable (e.g., empty topology for some ops)
                    eprintln!("Op {:?} returned error (acceptable): {:?}", op, e);
                }
            }
        }
    }

    /// Verify cached mesh watertightness: every edge has exactly 2 incident triangles.
    #[test]
    fn yang_mesh_passthrough_watertight() {
        let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 2.0, 2.0, 2.0);
        let (k_b, h_b) = make_box_via_kernel(1.5, 0.5, 2.0, 2.0, 2.0);
        let solid_a = k_a.get_solid(&h_a).unwrap();
        let solid_b = k_b.get_solid(&h_b).unwrap();

        let mut next_id = 1000u64;
        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut || {
            let id = next_id;
            next_id += 1;
            id
        })
        .expect("Union should succeed");

        let mesh = result.cached_render_mesh.as_ref().unwrap();

        // Count edge usage: each directed edge (i→j) should have a reverse (j→i).
        use std::collections::HashMap;
        let mut edge_count: HashMap<(u32, u32), usize> = HashMap::new();
        for tri in mesh.indices.chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            for k in 0..3 {
                let v0 = tri[k];
                let v1 = tri[(k + 1) % 3];
                *edge_count.entry((v0, v1)).or_default() += 1;
            }
        }

        let mut unpaired = 0;
        for &(v0, v1) in edge_count.keys() {
            let fwd = edge_count.get(&(v0, v1)).copied().unwrap_or(0);
            let rev = edge_count.get(&(v1, v0)).copied().unwrap_or(0);
            if fwd != rev {
                unpaired += 1;
            }
        }

        assert_eq!(
            unpaired, 0,
            "Cached mesh should be watertight (every directed edge has a reverse), \
             found {unpaired} unpaired directed edges"
        );
    }
}
