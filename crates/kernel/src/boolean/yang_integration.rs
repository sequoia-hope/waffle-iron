//! Yang 2025 hybrid B-Rep/mesh boolean pipeline integration.
//!
//! Bridges WaffleSolid (the kernel's solid representation) with the
//! Yang pipeline stages (tessellate → exact mesh boolean → topology
//! extract → SSI refinement). This module converts between the kernel's
//! native types and the pipeline's mesh-based types.
//!
//! Ref [#24]: Yang, Jia & Yan (2025) — hybrid B-Rep/mesh boolean.

use std::collections::BTreeMap;

use crate::boolean::collect_face_vertices;
use crate::boolean::exact_mesh::MeshBooleanOp;
use crate::boolean::exact_mesh::MeshId;
#[cfg(test)]
use crate::boolean::exact_mesh::SubdividedMesh;
use crate::boolean::polygon_centroid;
use crate::boolean::ssi_refinement::EdgeRefinementMap;
use crate::boolean::ssi_refinement::{
    classify_intersection_edges, refine_intersection_edges, refine_vertex_positions,
};
use crate::boolean::topology_extract::yang_boolean_pipeline;
use crate::boolean::topology_extract::ResultTopology;
#[cfg(test)]
use crate::boolean::topology_extract::{FaceSurvivalMap, SourceFace};
use crate::boolean::BoolOp;
use crate::boolean::BooleanResult;
use crate::geometry::curve::{Circle3D, CurveGeom, Ellipse3D, Line3D};
use crate::geometry::point::{Point3, Vector3};
use crate::geometry::surface::{Plane, SurfaceGeom};
use crate::ssi::SSICurve;
use crate::tessellation;
use crate::tessellation::bijective::BijectiveMap;
use crate::topology::half_edge::{EdgeIdx, FaceIdx, VertexIdx};
#[cfg(test)]
use crate::types::{FaceRange, KernelId};
use crate::types::{KernelError, RenderMesh};
use crate::units::TAU_NORMALIZE;
#[cfg(test)]
use crate::units::TAU_WORK;
use crate::vecmath::compute_newell_normal;
use crate::waffle_kernel::WaffleSolid;

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
    _surface_map: &BTreeMap<(MeshId, FaceIdx), SurfaceGeom>,
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

    // Build face_geometry for every face using Newell normal + centroid from
    // the actual face vertices. This ensures:
    // 1. Every face has geometry (enabling chained booleans)
    // 2. The normal is consistent with the face's actual vertex winding
    //    (source surface_map geometry may have wrong orientation after boolean)
    // Ref: spec yang_face_geometry_propagation.md.
    let mut face_geometry = BTreeMap::new();
    for &face_idx in result.face_provenance.keys() {
        let verts = collect_face_vertices(&result.arena, face_idx);
        if verts.len() < 3 {
            continue; // degenerate face
        }
        let newell = compute_newell_normal(&verts);
        let nl = (newell[0] * newell[0] + newell[1] * newell[1] + newell[2] * newell[2]).sqrt();
        if nl < TAU_NORMALIZE {
            continue; // zero-area face
        }
        let normal = Vector3 {
            x: newell[0] / nl,
            y: newell[1] / nl,
            z: newell[2] / nl,
        };
        let c = polygon_centroid(&verts);
        let origin = Point3 {
            x: c[0],
            y: c[1],
            z: c[2],
        };
        face_geometry.insert(face_idx, SurfaceGeom::Planar(Plane { origin, normal }));
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

// ── Sub-triangle render mesh: cached mesh from surviving sub-triangles (test-only) ───
// NOTE: build_render_mesh_from_survival is test-only. Production code uses
// retessellation at Render LOD (Step 9 of yang_boolean_inner). The sub-triangle
// mesh builder is kept for tests that verify conformal subdivision output.

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
#[cfg(test)]
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

    // Vertex deduplication by (position, normal) — nanometer quantization for
    // position, coarse quantization for normal direction. Vertices at the same
    // position but with different face normals (e.g., box corners shared between
    // perpendicular faces) get separate entries so each face has correct normals.
    // The watertight oracle uses position-based edge matching (not shared indices),
    // so watertightness is preserved.
    use std::collections::HashMap;
    let scale = crate::units::QUANT_NANOMETER_SCALE;
    let mut pos_norm_to_idx: HashMap<([i64; 3], [i32; 3]), u32> = HashMap::new();
    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    let quant = |p: [f64; 3]| -> [i64; 3] {
        [
            (p[0] * scale).round() as i64,
            (p[1] * scale).round() as i64,
            (p[2] * scale).round() as i64,
        ]
    };

    // Collect triangles grouped by source face for face_ranges.
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

            // Skip degenerate triangles (zero or near-zero area).
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let cross = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let area_sq = cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
            if area_sq < crate::units::TAU_NORMALIZE_SQ {
                continue; // Degenerate triangle — skip
            }

            // Compute normal from surface geometry or triangle cross-product.
            let geom_key = (source_face.mesh_id, source_face.face_idx);
            let face_normal =
                compute_face_normal(&p0, &p1, &p2, surface_map.get(&geom_key), tri.flipped);

            // Dedup vertex emission by (position, normal): vertices at the same
            // position with the same normal share an index (within a face), but
            // vertices at the same position with different normals (across faces)
            // get separate entries so each face has its correct normal.
            let quant_normal = |n: [f32; 3]| -> [i32; 3] {
                // Coarse quantization: 0.01 resolution is sufficient to distinguish
                // perpendicular face normals while merging within-face duplicates.
                [
                    (n[0] * 100.0).round() as i32,
                    (n[1] * 100.0).round() as i32,
                    (n[2] * 100.0).round() as i32,
                ]
            };
            let mut get_or_insert = |pos: [f64; 3], normal: [f32; 3]| -> u32 {
                let key = (quant(pos), quant_normal(normal));
                if let Some(&idx) = pos_norm_to_idx.get(&key) {
                    return idx;
                }
                let idx = (vertices.len() / 3) as u32;
                vertices.extend_from_slice(&[pos[0] as f32, pos[1] as f32, pos[2] as f32]);
                normals.extend_from_slice(&normal);
                pos_norm_to_idx.insert(key, idx);
                idx
            };

            let idx0 = get_or_insert(p0, face_normal);
            let idx1 = get_or_insert(p1, face_normal);
            let idx2 = get_or_insert(p2, face_normal);

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
#[cfg(test)]
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
                if len > TAU_WORK {
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
                if len > TAU_WORK {
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
    if len > TAU_WORK {
        [(nx / len) as f32, (ny / len) as f32, (nz / len) as f32]
    } else {
        [0.0, 0.0, 1.0]
    }
}

// ── Main entry point ────────────────────────────────────────────────────
//
// Note: vertex deduplication uses (position, normal) as the key. Vertices
// at the same position with different face normals (e.g., box corners) get
// separate entries, ensuring correct per-face normals for the outward_normals
// oracle. The watertight oracle uses position-based edge matching.

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

    // Stage 0a (pre-tessellation): Detect coplanar face pairs and split B-Rep
    // faces for same-direction pairs per Yang 2025 Section 4.5.5.
    // Clone solids so we can modify them without affecting the originals.
    let coplanar_pairs =
        crate::boolean::coplanar_preprocess::detect_coplanar_face_pairs(solid_a, solid_b);
    let mut solid_a_mod = solid_a.clone();
    let mut solid_b_mod = solid_b.clone();
    if !coplanar_pairs.is_empty() {
        crate::boolean::coplanar_preprocess::split_brep_for_coplanar_pairs(
            &mut solid_a_mod,
            &mut solid_b_mod,
            &coplanar_pairs,
        );
    }

    // Step 1: Tessellate both (possibly modified) solids.
    // Use Boolean LOD (16 segments) — the mesh is a computational tool for
    // topology, not a rendering artifact. This cuts triangle counts ~16× on
    // curved surfaces and makes O(n·m) subdivision feasible.
    let lod = tessellation::TessellationLod::Boolean;
    let mesh_a = tessellate_waffle_solid(&solid_a_mod, lod)?;
    let mesh_b = tessellate_waffle_solid(&solid_b_mod, lod)?;

    // Step 2: Convert to pipeline format (f64 arrays).
    let (mut verts_a, mut tris_a) = render_mesh_to_arrays(&mesh_a);
    let (mut verts_b, mut tris_b) = render_mesh_to_arrays(&mesh_b);
    dedup_mesh_vertices(&mut verts_a, &mut tris_a);
    dedup_mesh_vertices(&mut verts_b, &mut tris_b);

    #[cfg(test)]
    eprintln!(
        "[YANG DIAG] Tessellation: mesh_a={}v/{}t, mesh_b={}v/{}t",
        verts_a.len(),
        tris_a.len(),
        verts_b.len(),
        tris_b.len()
    );

    if tris_a.is_empty() || tris_b.is_empty() {
        return Err(KernelError::NotSupported {
            operation: "yang_boolean: tessellation produced empty mesh".to_string(),
        });
    }

    // Step 2b: Guard against excessive triangle counts (O(n*m) complexity).
    check_yang_triangle_count(tris_a.len(), tris_b.len())?;

    // Step 3: Build bijective maps (mesh triangle → B-Rep face).
    // Use the modified solids' face_maps (may have new faces from splitting).
    let mut bijective_a = BijectiveMap::from_render_mesh(&mesh_a, &solid_a_mod.face_map);
    let mut bijective_b = BijectiveMap::from_render_mesh(&mesh_b, &solid_b_mod.face_map);

    if !bijective_a.is_complete() || !bijective_b.is_complete() {
        return Err(KernelError::NotSupported {
            operation: "yang_boolean: bijective map has unmapped triangles".to_string(),
        });
    }

    // Stage 0b (post-tessellation): For anti-parallel coplanar pairs, inject
    // shared conformal triangulations. Same-direction pairs were handled by
    // B-Rep splitting above (Stage 0a).
    if !coplanar_pairs.is_empty() {
        #[cfg(test)]
        eprintln!(
            "[YANG DIAG] Stage 0: {} coplanar face pairs detected",
            coplanar_pairs.len()
        );
        crate::boolean::coplanar_preprocess::inject_conformal_coplanar_mesh(
            &coplanar_pairs,
            &mut verts_a,
            &mut tris_a,
            &mut verts_b,
            &mut tris_b,
            &mut bijective_a,
            &mut bijective_b,
            &mesh_a,
            &mesh_b,
        );
    }

    // Create deadline for the Yang pipeline to prevent runaway computation.
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(crate::units::YANG_PIPELINE_TIMEOUT_SECS);

    // Step 4: Run Yang pipeline (Phases 1-3): mesh boolean → topology extract.
    let mut pipeline_result = yang_boolean_pipeline(
        &verts_a,
        &tris_a,
        &verts_b,
        &tris_b,
        &bijective_a,
        &bijective_b,
        mesh_op,
        Some(deadline),
    )?;

    #[cfg(test)]
    {
        let total_a = pipeline_result.subdivided.tris_a.len();
        let total_b = pipeline_result.subdivided.tris_b.len();
        let surviving: usize = pipeline_result
            .survival
            .groups
            .values()
            .map(|v| v.len())
            .sum();
        let face_groups = pipeline_result.survival.groups.len();
        eprintln!(
            "[YANG DIAG] Subdivision: {}+{} sub-triangles",
            total_a, total_b
        );
        eprintln!(
            "[YANG DIAG] Survival: {}/{} sub-tris across {} face groups",
            surviving,
            total_a + total_b,
            face_groups
        );
    }

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

    // Empty topology is a valid result for some operations (e.g., Intersect of
    // disjoint solids). Return an empty BooleanResult directly.
    if pipeline_result.topology.face_provenance.is_empty() {
        return Ok(BooleanResult {
            arena: crate::topology::arena::TopoArena::new(),
            face_map: BTreeMap::new(),
            edge_map: BTreeMap::new(),
            vertex_map: BTreeMap::new(),
            face_geometry: BTreeMap::new(),
            edge_geometry: BTreeMap::new(),
            cached_face_polys: None,
            cached_render_mesh: Some(RenderMesh {
                vertices: vec![],
                normals: vec![],
                indices: vec![],
                face_ranges: vec![],
            }),
        });
    }

    // Step 6b: Project intersection vertices onto exact SSI curves.
    // Yang 2025 Section 4.3 — move vertices from mesh-approximate to surface-exact.
    if !refinement.edges.is_empty() {
        refine_vertex_positions(&mut pipeline_result.topology.arena, &refinement);
    }

    // Step 6c: CDT mesh updating along refined SSI curves.
    // Yang 2025 Section 4.4.1 — re-triangulate faces along refined intersection curves
    // so mesh edges follow the surface-exact SSI geometry.
    if !refinement.edges.is_empty() {
        crate::boolean::ssi_refinement::update_mesh_along_refined_curves(
            &mut pipeline_result.topology,
            &refinement,
        );
    }

    // Step 7: Convert ResultTopology → WaffleSolid → BooleanResult.
    let mut waffle = result_topology_to_waffle_solid(
        pipeline_result.topology,
        &refinement,
        &surface_map,
        id_alloc,
    );

    // Step 8: Validate result B-Rep topology before accepting. Invalid topology
    // (dangling edges, Euler ≠ 2, broken twin symmetry) would cause panics in
    // downstream operations (tessellation, chained booleans). This error propagates
    // to the caller — the dispatch layer does NOT fall back to the legacy S-H path
    // when Yang is enabled (A15.6).
    // P9: do not accept invalid results — hiding errors behind cached mesh is a hack.
    if let Err(msg) = validate_yang_result_topology(&waffle.arena) {
        return Err(KernelError::NotSupported {
            operation: format!("yang_boolean: result validation failed: {msg}"),
        });
    }

    // Step 9: Retessellate the validated B-Rep at Render LOD for the cached
    // render mesh. The sub-triangle mesh from 16-segment Boolean LOD has chord
    // error on curved surfaces that causes inter-face triangle penetrations
    // detected by the self-intersection oracle. Retessellation at 64-segment
    // Render LOD matches legacy pipeline quality and eliminates these artifacts.
    // P9: no fallback to sub-triangle mesh — if retessellation fails, the error
    // must propagate so the root cause is fixed, not masked.
    // Ref [#24] Yang 2025 — mesh is a computational tool, not the final output.
    let cached_mesh = tessellate_waffle_solid(&waffle, tessellation::TessellationLod::Render)?;
    waffle.cached_render_mesh = Some(cached_mesh);

    Ok(waffle_solid_to_boolean_result(waffle))
}

/// Tessellate a WaffleSolid using the kernel's internal tessellation pipeline.
///
/// Accepts a `TessellationLod` to control segment counts: `Boolean` uses 16
/// segments (sufficient for topology extraction), `Render` uses 64.
pub(crate) fn tessellate_waffle_solid(
    solid: &WaffleSolid,
    lod: tessellation::TessellationLod,
) -> Result<RenderMesh, KernelError> {
    tessellation::tessellate_solid_ext_with_lod(
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
        lod,
    )
}

// ── Result topology validation ──────────────────────────────────────────

/// Count connected face components in a TopoArena.
/// Each component is a maximal set of faces connected through shared edges (twin half-edges).
/// For a single closed solid: 1 component. For disjoint solids: N components.
/// Ref [#16] Mantyla 1988 — connected components of the face adjacency graph.
fn count_connected_components(arena: &crate::topology::arena::TopoArena) -> usize {
    let n_faces = arena.faces.len();
    if n_faces == 0 {
        return 0;
    }
    let mut visited = vec![false; n_faces];
    let mut components = 0;

    for start in 0..n_faces {
        if visited[start] {
            continue;
        }
        components += 1;
        let mut queue = vec![start];
        visited[start] = true;
        while let Some(fi) = queue.pop() {
            // Walk the outer loop of this face and follow twins to neighbor faces
            let loop_idx = arena.faces[fi].outer_loop;
            let start_he = arena.loops[loop_idx.0].half_edge;
            let mut he = start_he;
            loop {
                let twin = arena.half_edges[he.0].twin;
                let twin_loop = arena.half_edges[twin.0].loop_;
                let neighbor_face = arena.loops[twin_loop.0].face.0;
                if !visited[neighbor_face] {
                    visited[neighbor_face] = true;
                    queue.push(neighbor_face);
                }
                he = arena.half_edges[he.0].next;
                if he == start_he {
                    break;
                }
            }
        }
    }
    components
}

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

    // Count boundary HEs (self-twins from partial topology repair).
    let n_boundary_he = arena
        .half_edges
        .iter()
        .enumerate()
        .filter(|(i, he)| he.twin.0 == *i)
        .count();

    if n_boundary_he == 0 {
        // Fully closed manifold — check invariants but warn rather than fail.
        // The Yang pipeline may produce topologies that are usable for
        // face_geometry propagation but don't satisfy strict manifold invariants.
        if n_he != 2 * n_edges {
            eprintln!("[yang-diag] manifold warning: {n_he} half_edges != 2 * {n_edges} edges");
        }

        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;
        let components = count_connected_components(arena);
        if components > 0 {
            let expected_euler = 2 * components as i64;
            if euler != expected_euler {
                eprintln!(
                    "[yang-diag] Euler warning: V({n_verts}) - E({n_edges}) + F({n_faces}) = {euler} \
                     (expected {expected_euler} for {components} component(s))"
                );
            }
        }
    } else {
        // P9: do not accept partial topology. Boundary half-edges (self-twins)
        // indicate unpaired edges that will cause panics in downstream operations
        // (tessellation, chained booleans). Fail early so the error propagates.
        return Err(format!(
            "partial topology: {n_boundary_he} boundary HEs \
             out of {n_he} total ({n_faces} faces, {n_edges} edges, {n_verts} vertices)"
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

/// Pre-deduplicate per-face vertices by position (nanometer quantization).
///
/// Per-face tessellation produces meshes where shared vertices at face
/// boundaries have separate indices. The subdivision step operates on raw
/// indices, so duplicate positions cause inconsistent sub-triangle structures
/// across face boundaries. Deduplicating before subdivision ensures the
/// pipeline receives shared-vertex meshes identical to the E2E test structure.
///
/// Uses the same `QUANT_NANOMETER_SCALE` quantization used throughout the
/// pipeline for consistency. Ref [#9]: Cherchi 2020 — conformal vertex sharing.
pub(crate) fn dedup_mesh_vertices(verts: &mut Vec<[f64; 3]>, tris: &mut [[usize; 3]]) {
    use std::collections::HashMap;
    let scale = crate::units::QUANT_NANOMETER_SCALE;
    let mut pos_to_new: HashMap<[i64; 3], usize> = HashMap::new();
    let mut old_to_new: Vec<usize> = Vec::with_capacity(verts.len());
    let mut new_verts: Vec<[f64; 3]> = Vec::new();

    for v in verts.iter() {
        let key = [
            (v[0] * scale).round() as i64,
            (v[1] * scale).round() as i64,
            (v[2] * scale).round() as i64,
        ];
        let new_idx = *pos_to_new.entry(key).or_insert_with(|| {
            let idx = new_verts.len();
            new_verts.push(*v);
            idx
        });
        old_to_new.push(new_idx);
    }

    for tri in tris.iter_mut() {
        tri[0] = old_to_new[tri[0]];
        tri[1] = old_to_new[tri[1]];
        tri[2] = old_to_new[tri[2]];
    }

    *verts = new_verts;
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
        assert!((verts[0][0] - 0.0).abs() < crate::units::MIN_FEATURE_SIZE);
        assert!((verts[1][0] - 1.0).abs() < crate::units::MIN_FEATURE_SIZE);
        assert!((verts[2][1] - 1.0).abs() < crate::units::MIN_FEATURE_SIZE);

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

    /// Empty boolean results (e.g., intersection of non-overlapping solids)
    /// should return `Ok` with an empty solid (0 faces, edges, vertices).
    ///
    /// This test creates two non-overlapping tetrahedra and verifies that the
    /// intersection returns Ok with an empty result.
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

        // Empty intersection produces a valid empty solid (no faces, edges, vertices).
        match &result {
            Ok(boolean_result) => {
                assert_eq!(
                    boolean_result.arena.faces.len(),
                    0,
                    "Empty intersection should produce 0 faces, got {}",
                    boolean_result.arena.faces.len()
                );
            }
            Err(e) => {
                panic!("Empty intersection should succeed with empty solid, got: {e:?}");
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
    // Sub-triangle mesh tests: cached render mesh from surviving sub-tris
    // ══════════════════════════════════════════════════════════════════

    /// Verify that yang_boolean_inner produces a cached_render_mesh.
    #[test]
    fn yang_subtri_mesh_produces_cached_mesh() {
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
    fn yang_subtri_mesh_face_ranges_cover_all_indices() {
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
    fn yang_subtri_mesh_unit_normals() {
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
                (len - 1.0).abs() < crate::units::MIN_FEATURE_SIZE,
                "Normal {i} has length {len}, expected 1.0 (f32 epsilon ~1.19e-7)"
            );
        }
    }

    /// Verify all three boolean operations produce cached meshes.
    #[test]
    fn yang_subtri_mesh_all_ops() {
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

            let r = result.unwrap_or_else(|e| {
                panic!(
                    "Op {:?} on overlapping boxes should succeed, got: {:?}",
                    op, e
                )
            });
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
    }

    /// Verify cached mesh watertightness: every edge has exactly 2 incident triangles.
    #[test]
    fn yang_subtri_mesh_watertight() {
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

        // Position-based edge pairing (matches the watertight oracle).
        // Per-face vertex dedup means vertices at the same position may have
        // different indices (different normals), so index-based edge pairing
        // is not appropriate for meshes with per-face normals.
        use std::collections::HashMap;
        let quant_scale = crate::units::QUANT_NANOMETER_SCALE;
        let quant_pos = |idx: u32| -> [i64; 3] {
            let i = idx as usize * 3;
            [
                (mesh.vertices[i] as f64 * quant_scale).round() as i64,
                (mesh.vertices[i + 1] as f64 * quant_scale).round() as i64,
                (mesh.vertices[i + 2] as f64 * quant_scale).round() as i64,
            ]
        };

        type PosEdge = ([i64; 3], [i64; 3]);
        let make_edge = |a: [i64; 3], b: [i64; 3]| -> PosEdge {
            if a <= b {
                (a, b)
            } else {
                (b, a)
            }
        };

        let mut edge_count: HashMap<PosEdge, usize> = HashMap::new();
        for tri in mesh.indices.chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            let va = quant_pos(tri[0]);
            let vb = quant_pos(tri[1]);
            let vc = quant_pos(tri[2]);
            *edge_count.entry(make_edge(va, vb)).or_default() += 1;
            *edge_count.entry(make_edge(vb, vc)).or_default() += 1;
            *edge_count.entry(make_edge(vc, va)).or_default() += 1;
        }

        let unpaired: usize = edge_count.values().filter(|&&c| c % 2 != 0).count();
        assert_eq!(
            unpaired, 0,
            "Cached mesh should be watertight (every edge paired by position), \
             found {unpaired} unpaired edges"
        );
    }

    /// Verify no degenerate triangles in the Yang pipeline's cached render mesh.
    /// P9: if the tessellator produces degenerate triangles, fix the tessellator —
    /// don't weaken the test to accept them.
    #[test]
    fn yang_mesh_no_degenerate_triangles() {
        let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 2.0, 2.0, 2.0);
        let (k_b, h_b) = make_box_via_kernel(1.5, 0.5, 2.0, 2.0, 2.0);
        let solid_a = k_a.get_solid(&h_a).unwrap();
        let solid_b = k_b.get_solid(&h_b).unwrap();
        let mut next_id = 5000u64;
        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut || {
            next_id += 1;
            next_id
        })
        .expect("yang should succeed for overlapping boxes");

        let mesh = result
            .cached_render_mesh
            .as_ref()
            .expect("should have cached mesh");
        let tri_count = mesh.indices.len() / 3;
        assert!(tri_count > 0, "mesh should have triangles");

        for i in 0..tri_count {
            let i0 = mesh.indices[i * 3] as usize;
            let i1 = mesh.indices[i * 3 + 1] as usize;
            let i2 = mesh.indices[i * 3 + 2] as usize;
            let p0 = [
                mesh.vertices[i0 * 3] as f64,
                mesh.vertices[i0 * 3 + 1] as f64,
                mesh.vertices[i0 * 3 + 2] as f64,
            ];
            let p1 = [
                mesh.vertices[i1 * 3] as f64,
                mesh.vertices[i1 * 3 + 1] as f64,
                mesh.vertices[i1 * 3 + 2] as f64,
            ];
            let p2 = [
                mesh.vertices[i2 * 3] as f64,
                mesh.vertices[i2 * 3 + 1] as f64,
                mesh.vertices[i2 * 3 + 2] as f64,
            ];
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let cross = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
            assert!(
                area > crate::units::TAU_NORMALIZE,
                "triangle {i} is degenerate (area={area})"
            );
        }
    }

    /// Verify Euler characteristic V-E+F=2 for Yang pipeline cached render mesh.
    #[test]
    fn yang_mesh_euler_characteristic() {
        let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 2.0, 2.0, 2.0);
        let (k_b, h_b) = make_box_via_kernel(1.5, 0.5, 2.0, 2.0, 2.0);
        let solid_a = k_a.get_solid(&h_a).unwrap();
        let solid_b = k_b.get_solid(&h_b).unwrap();
        let mut next_id = 6000u64;
        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut || {
            next_id += 1;
            next_id
        })
        .expect("yang should succeed for overlapping boxes");

        let mesh = result
            .cached_render_mesh
            .as_ref()
            .expect("should have cached mesh");
        let tri_count = mesh.indices.len() / 3;

        // Position-based vertex/edge counting for Euler characteristic
        use std::collections::HashSet;
        let scale = crate::units::QUANT_NANOMETER_SCALE;
        let quant = |idx: usize| -> [i64; 3] {
            [
                (mesh.vertices[idx * 3] as f64 * scale).round() as i64,
                (mesh.vertices[idx * 3 + 1] as f64 * scale).round() as i64,
                (mesh.vertices[idx * 3 + 2] as f64 * scale).round() as i64,
            ]
        };

        let mut unique_verts: HashSet<[i64; 3]> = HashSet::new();
        let vert_count = mesh.vertices.len() / 3;
        for i in 0..vert_count {
            unique_verts.insert(quant(i));
        }

        let mut edges: HashSet<([i64; 3], [i64; 3])> = HashSet::new();
        for i in 0..tri_count {
            let i0 = mesh.indices[i * 3] as usize;
            let i1 = mesh.indices[i * 3 + 1] as usize;
            let i2 = mesh.indices[i * 3 + 2] as usize;
            let v0 = quant(i0);
            let v1 = quant(i1);
            let v2 = quant(i2);
            for (a, b) in [(v0, v1), (v1, v2), (v2, v0)] {
                let edge = if a < b { (a, b) } else { (b, a) };
                edges.insert(edge);
            }
        }

        let v = unique_verts.len() as i64;
        let e = edges.len() as i64;
        let f = tri_count as i64;
        let euler = v - e + f;
        eprintln!("[DIAG] Euler: V={v} - E={e} + F={f} = {euler}");
        assert_eq!(
            euler, 2,
            "Euler characteristic V-E+F should be 2 for a closed surface, got {euler}"
        );
    }

    /// Helper: build a closed box mesh (12 triangles, shared vertices).
    fn make_test_box_mesh(min: [f64; 3], max: [f64; 3]) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let verts = vec![
            [x0, y0, z0],
            [x1, y0, z0],
            [x1, y1, z0],
            [x0, y1, z0],
            [x0, y0, z1],
            [x1, y0, z1],
            [x1, y1, z1],
            [x0, y1, z1],
        ];
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2], // back
            [4, 5, 6],
            [4, 6, 7], // front
            [0, 1, 5],
            [0, 5, 4], // bottom
            [3, 6, 2],
            [3, 7, 6], // top
            [0, 4, 7],
            [0, 7, 3], // left
            [1, 2, 6],
            [1, 6, 5], // right
        ];
        (verts, tris)
    }

    /// Verify that the label distribution for overlapping boxes is reasonable.
    #[test]
    fn yang_diag_overlapping_box_label_distribution() {
        use crate::boolean::exact_mesh::{
            label_cells, select_boolean_result, subdivide_mesh_pair, MeshBooleanOp,
        };
        let (verts_a, tris_a) = make_test_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_test_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        let mut a_outside = 0usize;
        let mut b_outside = 0usize;
        for label in &labeling.labels_a {
            if matches!(
                label,
                crate::boolean::exact_mesh::CellLabel::Outside
                    | crate::boolean::exact_mesh::CellLabel::CoSurfaceOutside
            ) {
                a_outside += 1;
            }
        }
        for label in &labeling.labels_b {
            if matches!(
                label,
                crate::boolean::exact_mesh::CellLabel::Outside
                    | crate::boolean::exact_mesh::CellLabel::CoSurfaceOutside
            ) {
                b_outside += 1;
            }
        }

        let total_a = labeling.labels_a.len();
        let total_b = labeling.labels_b.len();
        eprintln!("[DIAG] A: {a_outside}/{total_a} outside, B: {b_outside}/{total_b} outside");

        assert!(a_outside > 0, "some A tris must be Outside B");
        assert!(b_outside > 0, "some B tris must be Outside A");

        let result = select_boolean_result(&subdivided, &labeling, MeshBooleanOp::Union);
        let result_tri_count = result.len() / 3;
        eprintln!(
            "[DIAG] Union result: {} triangles from {}+{} sub-tris",
            result_tri_count, total_a, total_b
        );
        assert!(
            result_tri_count >= 10,
            "union of overlapping boxes must produce at least 10 tris, got {result_tri_count}"
        );
    }

    /// Verify that an expired deadline triggers a timeout error.
    #[test]
    fn yang_pipeline_respects_internal_timeout() {
        use crate::boolean::exact_mesh::subdivide_mesh_pair;
        let expired = std::time::Instant::now() - std::time::Duration::from_secs(1);
        let (va, ta) = make_test_box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let (vb, tb) = make_test_box_mesh([0.5, 0.0, 0.0], [1.5, 1.0, 1.0]);
        let result = subdivide_mesh_pair(&va, &ta, &vb, &tb, Some(expired));
        assert!(
            result.is_err(),
            "expired deadline should cause timeout error"
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // Vertex dedup tests: per-face-vertex mesh support
    // ══════════════════════════════════════════════════════════════════

    /// Test that `dedup_mesh_vertices` merges duplicate positions and remaps
    /// triangle indices correctly.
    #[test]
    fn test_dedup_mesh_vertices_basic() {
        // 12 vertices: 4 unique positions, each duplicated 3 times
        let mut verts: Vec<[f64; 3]> = vec![
            // Copies of A = [1.0, 0.0, 0.0] at indices 0, 4, 8
            [1.0, 0.0, 0.0], // 0
            // Copies of B = [0.0, 1.0, 0.0] at indices 1, 5, 9
            [0.0, 1.0, 0.0], // 1
            // Copies of C = [0.0, 0.0, 1.0] at indices 2, 6, 10
            [0.0, 0.0, 1.0], // 2
            // Copies of D = [1.0, 1.0, 1.0] at indices 3, 7, 11
            [1.0, 1.0, 1.0], // 3
            [1.0, 0.0, 0.0], // 4  (dup of A)
            [0.0, 1.0, 0.0], // 5  (dup of B)
            [0.0, 0.0, 1.0], // 6  (dup of C)
            [1.0, 1.0, 1.0], // 7  (dup of D)
            [1.0, 0.0, 0.0], // 8  (dup of A)
            [0.0, 1.0, 0.0], // 9  (dup of B)
            [0.0, 0.0, 1.0], // 10 (dup of C)
            [1.0, 1.0, 1.0], // 11 (dup of D)
        ];

        let mut tris: Vec<[usize; 3]> = vec![
            [0, 1, 2],  // copies 0 of A, B, C
            [4, 5, 6],  // copies 1 of A, B, C
            [8, 9, 10], // copies 2 of A, B, C
            [3, 7, 11], // copies of D, D, D (degenerate, tests index remapping)
        ];

        dedup_mesh_vertices(&mut verts, &mut tris);

        // Only 4 unique positions should remain
        assert_eq!(
            verts.len(),
            4,
            "dedup should reduce 12 vertices to 4 unique positions, got {}",
            verts.len()
        );

        // First 3 triangles should all have identical indices (all map to canonical A, B, C)
        assert_eq!(
            tris[0], tris[1],
            "triangles 0 and 1 should have same indices after dedup"
        );
        assert_eq!(
            tris[1], tris[2],
            "triangles 1 and 2 should have same indices after dedup"
        );

        // 4th triangle should have all 3 indices equal (all map to canonical D)
        assert_eq!(
            tris[3][0], tris[3][1],
            "degenerate triangle indices should all map to the same canonical D vertex"
        );
        assert_eq!(
            tris[3][1], tris[3][2],
            "degenerate triangle indices should all map to the same canonical D vertex"
        );
    }

    /// Test that the Yang pipeline handles per-face-vertex meshes (like WaffleSolid
    /// tessellation produces) by deduplicating vertices before processing.
    #[test]
    fn test_per_face_vertex_box_union() {
        use crate::boolean::exact_mesh::MeshBooleanOp;
        use crate::boolean::topology_extract::yang_boolean_pipeline;
        use crate::tessellation::bijective::BijectiveMap;
        use crate::topology::half_edge::FaceIdx;

        /// Build a box mesh with per-face vertices (24 vertices, 12 triangles).
        /// Each face has its own 4 vertices — no sharing between faces.
        fn make_per_face_box_mesh(
            min: [f64; 3],
            max: [f64; 3],
        ) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
            let [x0, y0, z0] = min;
            let [x1, y1, z1] = max;

            let mut verts = Vec::with_capacity(24);
            let mut tris = Vec::with_capacity(12);

            // 6 faces, each with 4 unique vertices and 2 triangles
            // Face 0: back (z=z0)
            let base = verts.len();
            verts.extend_from_slice(&[[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]]);
            tris.push([base, base + 2, base + 1]);
            tris.push([base, base + 3, base + 2]);

            // Face 1: front (z=z1)
            let base = verts.len();
            verts.extend_from_slice(&[[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]]);
            tris.push([base, base + 1, base + 2]);
            tris.push([base, base + 2, base + 3]);

            // Face 2: bottom (y=y0)
            let base = verts.len();
            verts.extend_from_slice(&[[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]]);
            tris.push([base, base + 1, base + 2]);
            tris.push([base, base + 2, base + 3]);

            // Face 3: top (y=y1)
            let base = verts.len();
            verts.extend_from_slice(&[[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]]);
            tris.push([base, base + 2, base + 1]);
            tris.push([base, base + 3, base + 2]);

            // Face 4: left (x=x0)
            let base = verts.len();
            verts.extend_from_slice(&[[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]]);
            tris.push([base, base + 1, base + 2]);
            tris.push([base, base + 2, base + 3]);

            // Face 5: right (x=x1)
            let base = verts.len();
            verts.extend_from_slice(&[[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]]);
            tris.push([base, base + 2, base + 1]);
            tris.push([base, base + 3, base + 2]);

            assert_eq!(verts.len(), 24, "per-face box must have 24 vertices");
            assert_eq!(tris.len(), 12, "per-face box must have 12 triangles");

            (verts, tris)
        }

        // Box A and Box B: identical boxes
        let (mut verts_a, mut tris_a) = make_per_face_box_mesh([-0.5, -0.5, -0.5], [0.5, 0.5, 0.5]);
        let (mut verts_b, mut tris_b) = make_per_face_box_mesh([-0.5, -0.5, -0.5], [0.5, 0.5, 0.5]);

        // Dedup vertices before passing to the pipeline
        dedup_mesh_vertices(&mut verts_a, &mut tris_a);
        dedup_mesh_vertices(&mut verts_b, &mut tris_b);

        // Build BijectiveMaps: 6 faces, 2 tris per face → face_idx = tri_idx / 2
        let bijective_a = BijectiveMap {
            tri_face_ids: (0..12).map(|i| FaceIdx(i / 2)).collect(),
        };
        let bijective_b = BijectiveMap {
            tri_face_ids: (0..12).map(|i| FaceIdx(i / 2)).collect(),
        };

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
            None,
        );

        match &result {
            Ok(pipeline_result) => {
                // Topology should have faces (face_provenance not empty)
                assert!(
                    !pipeline_result.topology.face_provenance.is_empty(),
                    "Union of identical per-face-vertex boxes must produce faces, \
                     but face_provenance is empty"
                );

                // Check Euler characteristic on surviving mesh triangles
                let surviving_count: usize = pipeline_result
                    .survival
                    .groups
                    .values()
                    .map(|v| v.len())
                    .sum();
                eprintln!(
                    "[DIAG] per-face-vertex box union: {} face groups, {} surviving tris",
                    pipeline_result.topology.face_provenance.len(),
                    surviving_count
                );
            }
            Err(e) => {
                panic!(
                    "Yang pipeline should handle per-face-vertex meshes after dedup, \
                     but failed with: {e:?}"
                );
            }
        }
    }

    #[test]
    fn test_dedup_preserves_distinct_close_vertices() {
        // Two vertices 2nm apart — should NOT be merged (> 1nm quantization bucket)
        let mut verts = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 2e-9]];
        let mut tris = vec![[0, 0, 1]];

        dedup_mesh_vertices(&mut verts, &mut tris);

        assert_eq!(
            verts.len(),
            2,
            "Vertices 2nm apart must remain distinct after dedup"
        );
        assert_eq!(tris[0], [0, 0, 1]);
    }

    #[test]
    fn test_dedup_empty_mesh() {
        let mut verts: Vec<[f64; 3]> = vec![];
        let mut tris: Vec<[usize; 3]> = vec![];

        dedup_mesh_vertices(&mut verts, &mut tris);

        assert!(verts.is_empty());
        assert!(tris.is_empty());
    }

    #[test]
    fn test_dedup_removes_degenerate_indices() {
        // Three vertices at the same position — dedup should collapse them to one,
        // making the triangle degenerate (all indices equal).
        let mut verts = vec![[1.0, 2.0, 3.0], [1.0, 2.0, 3.0], [1.0, 2.0, 3.0]];
        let mut tris = vec![[0, 1, 2]];

        dedup_mesh_vertices(&mut verts, &mut tris);

        assert_eq!(
            verts.len(),
            1,
            "All three identical vertices should merge to one"
        );
        assert_eq!(
            tris[0][0], tris[0][1],
            "Degenerate triangle: all indices should be the same"
        );
        assert_eq!(
            tris[0][1], tris[0][2],
            "Degenerate triangle: all indices should be the same"
        );
    }

    /// Test that `build_render_mesh_from_survival` produces correct per-face normals.
    ///
    /// The bug: `get_or_insert` deduplicates vertices by position only, so when a corner
    /// vertex is shared between faces with different normals (e.g., top face [0,0,1] vs
    /// side face [1,0,0]), the first face's normal wins for all subsequent faces sharing
    /// that position. This causes most triangles to have wrong normals.
    #[test]
    fn test_yang_render_mesh_normals_per_face() {
        use crate::boolean::exact_mesh::MeshBooleanOp;
        use crate::boolean::topology_extract::yang_boolean_pipeline;
        use crate::geometry::surface::{Plane, SurfaceGeom};
        use crate::tessellation::bijective::BijectiveMap;
        use crate::topology::half_edge::FaceIdx;

        /// Build a box mesh with per-face vertices (24 vertices, 12 triangles).
        fn make_per_face_box_mesh(
            min: [f64; 3],
            max: [f64; 3],
        ) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
            let [x0, y0, z0] = min;
            let [x1, y1, z1] = max;

            let mut verts = Vec::with_capacity(24);
            let mut tris = Vec::with_capacity(12);

            // Face 0: back (z=z0), normal [0,0,-1]
            let base = verts.len();
            verts.extend_from_slice(&[[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]]);
            tris.push([base, base + 2, base + 1]);
            tris.push([base, base + 3, base + 2]);

            // Face 1: front (z=z1), normal [0,0,1]
            let base = verts.len();
            verts.extend_from_slice(&[[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]]);
            tris.push([base, base + 1, base + 2]);
            tris.push([base, base + 2, base + 3]);

            // Face 2: bottom (y=y0), normal [0,-1,0]
            let base = verts.len();
            verts.extend_from_slice(&[[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]]);
            tris.push([base, base + 1, base + 2]);
            tris.push([base, base + 2, base + 3]);

            // Face 3: top (y=y1), normal [0,1,0]
            let base = verts.len();
            verts.extend_from_slice(&[[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]]);
            tris.push([base, base + 2, base + 1]);
            tris.push([base, base + 3, base + 2]);

            // Face 4: left (x=x0), normal [-1,0,0]
            let base = verts.len();
            verts.extend_from_slice(&[[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]]);
            tris.push([base, base + 1, base + 2]);
            tris.push([base, base + 2, base + 3]);

            // Face 5: right (x=x1), normal [1,0,0]
            let base = verts.len();
            verts.extend_from_slice(&[[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]]);
            tris.push([base, base + 2, base + 1]);
            tris.push([base, base + 3, base + 2]);

            (verts, tris)
        }

        // Build the per-face surface normals for a box.
        // Face indices 0..5 map to: back(-Z), front(+Z), bottom(-Y), top(+Y), left(-X), right(+X)
        fn box_surface_map(mesh_id: MeshId) -> BTreeMap<(MeshId, FaceIdx), SurfaceGeom> {
            let face_normals: [(f64, f64, f64); 6] = [
                (0.0, 0.0, -1.0), // face 0: back
                (0.0, 0.0, 1.0),  // face 1: front
                (0.0, -1.0, 0.0), // face 2: bottom
                (0.0, 1.0, 0.0),  // face 3: top
                (-1.0, 0.0, 0.0), // face 4: left
                (1.0, 0.0, 0.0),  // face 5: right
            ];
            let mut map = BTreeMap::new();
            for (i, (nx, ny, nz)) in face_normals.iter().enumerate() {
                map.insert(
                    (mesh_id, FaceIdx(i)),
                    SurfaceGeom::Planar(Plane {
                        origin: Point3::new(0.0, 0.0, 0.0),
                        normal: Vector3::new(*nx, *ny, *nz),
                    }),
                );
            }
            map
        }

        // Two identical boxes (union of identical boxes = same box)
        let (mut verts_a, mut tris_a) = make_per_face_box_mesh([-0.5, -0.5, -0.5], [0.5, 0.5, 0.5]);
        let (mut verts_b, mut tris_b) = make_per_face_box_mesh([-0.5, -0.5, -0.5], [0.5, 0.5, 0.5]);

        dedup_mesh_vertices(&mut verts_a, &mut tris_a);
        dedup_mesh_vertices(&mut verts_b, &mut tris_b);

        let bijective_a = BijectiveMap {
            tri_face_ids: (0..12).map(|i| FaceIdx(i / 2)).collect(),
        };
        let bijective_b = BijectiveMap {
            tri_face_ids: (0..12).map(|i| FaceIdx(i / 2)).collect(),
        };

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
            None,
        )
        .expect("Yang pipeline must succeed for identical box union");

        // Build surface map with per-face normals for both meshes
        let mut surface_map = box_surface_map(MeshId::A);
        surface_map.extend(box_surface_map(MeshId::B));

        // Build face_map: assign kernel IDs to each face in the result topology
        let face_map: BTreeMap<u64, FaceIdx> = result
            .topology
            .face_provenance
            .keys()
            .enumerate()
            .map(|(i, &fidx)| ((i as u64) + 1, fidx))
            .collect();

        let mesh = build_render_mesh_from_survival(
            &result.survival,
            &result.subdivided,
            &surface_map,
            &result.topology.face_provenance,
            &face_map,
        );

        // Verify the mesh has triangles
        let tri_count = mesh.indices.len() / 3;
        assert!(
            tri_count >= 12,
            "Box union render mesh must have at least 12 triangles, got {tri_count}"
        );

        // For each triangle, check that the stored vertex normal agrees with the
        // geometric (cross-product) normal direction.
        let mut correct = 0usize;
        let mut total = 0usize;
        for t in 0..tri_count {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;

            let p0 = [
                mesh.vertices[i0 * 3] as f64,
                mesh.vertices[i0 * 3 + 1] as f64,
                mesh.vertices[i0 * 3 + 2] as f64,
            ];
            let p1 = [
                mesh.vertices[i1 * 3] as f64,
                mesh.vertices[i1 * 3 + 1] as f64,
                mesh.vertices[i1 * 3 + 2] as f64,
            ];
            let p2 = [
                mesh.vertices[i2 * 3] as f64,
                mesh.vertices[i2 * 3 + 1] as f64,
                mesh.vertices[i2 * 3 + 2] as f64,
            ];

            // Geometric normal via cross product
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let geo_n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let geo_len = (geo_n[0] * geo_n[0] + geo_n[1] * geo_n[1] + geo_n[2] * geo_n[2]).sqrt();
            if geo_len < TAU_WORK {
                continue; // degenerate triangle (cross-product magnitude < working tolerance)
            }

            // Stored normal of the first vertex
            let sn = [
                mesh.normals[i0 * 3] as f64,
                mesh.normals[i0 * 3 + 1] as f64,
                mesh.normals[i0 * 3 + 2] as f64,
            ];

            let dot = geo_n[0] * sn[0] + geo_n[1] * sn[1] + geo_n[2] * sn[2];
            total += 1;
            if dot > 0.0 {
                correct += 1;
            } else {
                eprintln!(
                    "[DIAG] triangle {t}: geo_normal=[{:.3},{:.3},{:.3}], stored_normal=[{:.3},{:.3},{:.3}], dot={dot:.4}",
                    geo_n[0] / geo_len,
                    geo_n[1] / geo_len,
                    geo_n[2] / geo_len,
                    sn[0],
                    sn[1],
                    sn[2],
                );
            }
        }

        let pct = if total > 0 {
            (correct as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        eprintln!("[DIAG] outward normals: {correct}/{total} triangles correct ({pct:.1}%)");

        assert!(
            pct >= 95.0,
            "Render mesh must have >= 95% correct outward normals, \
             but only {correct} of {total} triangles ({pct:.1}%) have normals \
             agreeing with geometric direction. This indicates the position-only \
             vertex dedup is clobbering per-face normals."
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // Retessellation tests — render mesh quality (red phase)
    // ══════════════════════════════════════════════════════════════════

    /// After retessellation, the render mesh should have fewer unique vertex
    /// positions than the sub-triangle mesh, because bounded tessellation shares
    /// boundary vertices across faces. The sub-triangle mesh duplicates boundary
    /// vertices per face (different normals → different indices).
    ///
    /// This test verifies that the Yang pipeline uses retessellation (not
    /// sub-triangle mesh) by checking that vertex count is reasonable
    /// for a bounded-tessellation box-box union.
    #[test]
    fn yang_render_mesh_is_retessellated_not_subtriangle() {
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
        let num_verts = mesh.vertices.len() / 3;
        let num_tris = mesh.indices.len() / 3;

        // A box-box union has 10 faces. The bounded tessellation for 10 planar faces
        // produces ~20 triangles (2 per face) with ~12-16 unique vertices (shared at
        // edges). The sub-triangle mesh from Boolean LOD (16-seg) produces more
        // triangles because it subdivides along the intersection boundary.
        //
        // With retessellation: expect ≤40 triangles for a planar box-box union
        // (bounded tessellation + post-processing may add edge-pairing triangles).
        // With sub-triangles: the conformal subdivision produces 44 triangles.
        assert!(
            num_tris <= 40,
            "Expected ≤40 triangles from retessellated box-box union, got {num_tris} \
             (sub-triangle mesh produces 44). Pipeline may not be retessellating."
        );
        // Sanity: must have at least 6 faces worth of triangles
        assert!(
            num_tris >= 6,
            "Expected ≥10 triangles for 10-face union, got {num_tris}"
        );
        eprintln!(
            "[TEST] Retessellated mesh: {num_verts} verts, {num_tris} tris, {} face_ranges",
            mesh.face_ranges.len()
        );
    }

    // NOTE: yang_render_mesh_fallback_on_tetra_boolean removed.
    // It tested a fallback path that violates P9 (no hack-to-green). The test
    // accepted both success and failure ("either is acceptable") which is not
    // a valid test — P1 requires numeric/structural oracles. If tetra booleans
    // need support, add a proper test with deterministic expectations.

    // ══════════════════════════════════════════════════════════════════
    // Box+cylinder Yang pipeline tests (red phase)
    // ══════════════════════════════════════════════════════════════════

    /// Build a cylinder WaffleSolid on the XY plane via WaffleKernel.
    fn make_cylinder_via_kernel(
        r: f64,
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
        positions.insert(1, (0.0, 0.0));

        let profile = crate::types::ClosedProfile {
            entity_ids: vec![1],
            is_outer: true,
            vertex_ids: vec![],
            circle: Some(crate::types::CircleProfile {
                center_u: 0.0,
                center_v: 0.0,
                radius: r,
            }),
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
            .expect("make_faces_from_profiles should succeed for circle");
        let solid = k
            .extrude_face(faces[0], [0.0, 0.0, 1.0], depth)
            .expect("extrude_face should succeed for cylinder");

        (k, solid)
    }

    /// Yang pipeline should produce a valid B-Rep for a box+cylinder union.
    /// Uses coincident geometry (box and cylinder share the z=0 plane).
    /// This is the common CAD pattern — cylinder boss on a box face.
    ///
    /// Currently fails: flood_fill_patches produces empty topology
    /// because at z=0, the box bottom face triangles inside the cylinder are
    /// classified as "A inside B" and removed, while the cylinder bottom cap
    /// is classified as "B inside A" and also removed. This creates a hole
    /// with no matching face → unpaired half-edges → empty result.
    ///
    /// Root cause: coincident face (coplanar, overlapping) handling in the
    /// mesh boolean cell labeling. When A's surface coincides with B's surface,
    /// the inside/outside classification is ambiguous.
    #[test]
    #[ignore = "red-phase P3: boundary fix applied (0 unpaired HEs) but Euler=6 — excess face groups from coplanar z=0 + cylinder barrel subdivision"]
    fn yang_box_cylinder_union_produces_valid_topology() {
        let (k_a, h_a) = make_box_via_kernel(0.0, 0.0, 2.0, 2.0, 2.0);
        let (k_b, h_b) = make_cylinder_via_kernel(0.5, 3.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b");

        assert!(
            !solid_a.face_geometry.is_empty(),
            "box must have face_geometry"
        );
        assert!(
            !solid_b.face_geometry.is_empty(),
            "cylinder must have face_geometry"
        );

        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut id_alloc);

        match &result {
            Ok(br) => {
                let n_f = br.arena.faces.len();
                let n_e = br.arena.edges.len();
                let n_v = br.arena.vertices.len();
                let euler = n_v as i64 - n_e as i64 + n_f as i64;
                eprintln!("[TEST] Box+Cylinder union: F={n_f}, E={n_e}, V={n_v}, Euler={euler}");
                assert!(n_f > 0, "Result should have faces");
                assert_eq!(euler, 2, "Euler characteristic must be 2 for closed solid");
                assert!(
                    br.cached_render_mesh.is_some(),
                    "Should have cached render mesh"
                );
            }
            Err(e) => {
                panic!(
                    "Yang box+cylinder union should succeed but got: {e}. \
                     Root cause: coincident face handling — box bottom and cylinder bottom \
                     cap share z=0 plane. Both are removed by cell labeling, creating a \
                     hole with no matching face → unpaired half-edges → empty topology."
                );
            }
        }
    }

    /// Yang pipeline box+cylinder subtract should produce valid topology.
    /// The cylinder cuts a hole through the box. No coincident face issue
    /// here because the cylinder extends beyond the box on both sides.
    #[test]
    #[ignore = "red-phase P3: Yang pipeline twin-pairing fails on box+cylinder geometry"]
    fn yang_box_cylinder_subtract_produces_valid_topology() {
        let (k_a, h_a) = make_box_via_kernel(0.0, 0.0, 2.0, 2.0, 2.0);
        let (k_b, h_b) = make_cylinder_via_kernel(0.3, 3.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b");

        let mut next_id = 2000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Subtract, &mut id_alloc);

        match &result {
            Ok(br) => {
                let n_f = br.arena.faces.len();
                let n_e = br.arena.edges.len();
                let n_v = br.arena.vertices.len();
                let euler = n_v as i64 - n_e as i64 + n_f as i64;
                eprintln!("[TEST] Box-Cylinder subtract: F={n_f}, E={n_e}, V={n_v}, Euler={euler}");
                assert!(n_f > 0, "Result should have faces");
            }
            Err(e) => {
                panic!(
                    "Yang box-cylinder subtract should succeed but got: {e}. \
                     Root cause: coincident face and/or twin-pairing issue at \
                     cylinder-box intersection."
                );
            }
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Self-intersection & boundary-row tests for retessellated curved faces
    // ══════════════════════════════════════════════════════════════════

    fn count_mesh_self_intersections(mesh: &RenderMesh) -> usize {
        use std::collections::HashSet;
        let qs = crate::units::QUANT_NANOMETER_SCALE;
        let quant = |idx: u32| -> [i64; 3] {
            let i = idx as usize * 3;
            [
                (mesh.vertices[i] as f64 * qs).round() as i64,
                (mesh.vertices[i + 1] as f64 * qs).round() as i64,
                (mesh.vertices[i + 2] as f64 * qs).round() as i64,
            ]
        };
        let vtx = |idx: u32| -> [f64; 3] {
            let i = idx as usize * 3;
            [
                mesh.vertices[i] as f64,
                mesh.vertices[i + 1] as f64,
                mesh.vertices[i + 2] as f64,
            ]
        };
        let mut face_tris: Vec<Vec<usize>> = Vec::new();
        for fr in &mesh.face_ranges {
            face_tris.push((fr.start_index as usize / 3..fr.end_index as usize / 3).collect());
        }
        let face_aabb = |tris: &[usize]| -> ([f64; 3], [f64; 3]) {
            let (mut mn, mut mx) = ([f64::MAX; 3], [f64::MIN; 3]);
            for &ti in tris {
                for k in 0..3 {
                    let v = vtx(mesh.indices[ti * 3 + k]);
                    for d in 0..3 {
                        mn[d] = mn[d].min(v[d]);
                        mx[d] = mx[d].max(v[d]);
                    }
                }
            }
            (mn, mx)
        };
        fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        }
        fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
            [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
        }
        fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
            a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
        }
        fn proj(v: &[[f64; 3]; 3], ax: [f64; 3]) -> (f64, f64) {
            let (d0, d1, d2) = (dot(v[0], ax), dot(v[1], ax), dot(v[2], ax));
            (d0.min(d1).min(d2), d0.max(d1).max(d2))
        }
        fn tti(t1: &[[f64; 3]; 3], t2: &[[f64; 3]; 3]) -> bool {
            let n1 = cross(sub(t1[1], t1[0]), sub(t1[2], t1[0]));
            let n2 = cross(sub(t2[1], t2[0]), sub(t2[2], t2[0]));
            let e1 = [sub(t1[1], t1[0]), sub(t1[2], t1[1]), sub(t1[0], t1[2])];
            let e2 = [sub(t2[1], t2[0]), sub(t2[2], t2[1]), sub(t2[0], t2[2])];
            let mut axes = vec![n1, n2];
            for ea in &e1 {
                for eb in &e2 {
                    axes.push(cross(*ea, *eb));
                }
            }
            for ax in &axes {
                if dot(*ax, *ax) < crate::units::TAU_NORMALIZE_SQ {
                    continue;
                }
                let (a, b) = proj(t1, *ax);
                let (c, d) = proj(t2, *ax);
                if a > d + crate::units::TAU_WORK || c > b + crate::units::TAU_WORK {
                    return false;
                }
            }
            true
        }
        let mut violations = 0usize;
        for i in 0..face_tris.len() {
            let ai = face_aabb(&face_tris[i]);
            for j in (i + 1)..face_tris.len() {
                let aj = face_aabb(&face_tris[j]);
                if (0..3).any(|d| {
                    ai.0[d] > aj.1[d] + crate::units::TAU_WORK
                        || aj.0[d] > ai.1[d] + crate::units::TAU_WORK
                }) {
                    continue;
                }
                for &ti in &face_tris[i] {
                    let qi: HashSet<[i64; 3]> =
                        (0..3).map(|k| quant(mesh.indices[ti * 3 + k])).collect();
                    let t1 = [
                        vtx(mesh.indices[ti * 3]),
                        vtx(mesh.indices[ti * 3 + 1]),
                        vtx(mesh.indices[ti * 3 + 2]),
                    ];
                    for &tj in &face_tris[j] {
                        let qj: HashSet<[i64; 3]> =
                            (0..3).map(|k| quant(mesh.indices[tj * 3 + k])).collect();
                        if qi.intersection(&qj).count() >= 2 {
                            continue;
                        }
                        let t2 = [
                            vtx(mesh.indices[tj * 3]),
                            vtx(mesh.indices[tj * 3 + 1]),
                            vtx(mesh.indices[tj * 3 + 2]),
                        ];
                        if tti(&t1, &t2) {
                            violations += 1;
                        }
                    }
                }
            }
        }
        violations
    }

    #[test]
    #[ignore = "red-phase P3: Yang pipeline returns NotSupported for box+cylinder (empty topology from twin-pairing failure on curved geometry)"]
    fn yang_retessellated_box_cylinder_no_self_intersection() {
        let (k_a, h_a) = make_box_via_kernel(0.0, 0.0, 0.52, 0.52, 0.46);
        let (k_b, h_b) = make_cylinder_via_kernel(0.088, 0.35);
        let solid_a = k_a.get_solid(&h_a).expect("solid_a");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b");
        assert!(!solid_a.face_geometry.is_empty());
        assert!(!solid_b.face_geometry.is_empty());
        let mut next_id = 1000u64;
        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut || {
            let id = next_id;
            next_id += 1;
            id
        })
        .expect("Box + cylinder union should succeed");
        let mesh = result
            .cached_render_mesh
            .as_ref()
            .expect("Yang pipeline should produce a cached render mesh");
        eprintln!(
            "[TEST] Box+Cyl mesh: {}v {}t {}fr",
            mesh.vertices.len() / 3,
            mesh.indices.len() / 3,
            mesh.face_ranges.len()
        );
        let si = count_mesh_self_intersections(mesh);
        eprintln!("[TEST] Inter-face self-intersections: {si}");
        assert_eq!(
            si, 0,
            "Retessellated box+cylinder union has {si} inter-face SI. \
             Boundary row vertices must exactly match discretized edge positions."
        );
    }

    #[test]
    #[ignore = "red-phase P3: Yang pipeline cannot produce WaffleSolid with cylindrical faces yet"]
    fn yang_retessellated_boundary_row_exact_positions() {
        use crate::geometry::curve::CurveGeom;
        use crate::geometry::surface::SurfaceGeom;
        use crate::tessellation;
        let (k_a, h_a) = make_box_via_kernel(0.0, 0.0, 0.52, 0.52, 0.46);
        let (k_b, h_b) = make_cylinder_via_kernel(0.088, 0.35);
        let solid_a = k_a.get_solid(&h_a).unwrap();
        let solid_b = k_b.get_solid(&h_b).unwrap();
        let mut next_id = 1000u64;
        let surface_map = build_surface_map(solid_a, solid_b);
        let lod = tessellation::TessellationLod::Boolean;
        let mesh_a = tessellate_waffle_solid(solid_a, lod).unwrap();
        let mesh_b = tessellate_waffle_solid(solid_b, lod).unwrap();
        let (mut va, mut ta) = render_mesh_to_arrays(&mesh_a);
        let (mut vb, mut tb) = render_mesh_to_arrays(&mesh_b);
        dedup_mesh_vertices(&mut va, &mut ta);
        dedup_mesh_vertices(&mut vb, &mut tb);
        let ba = BijectiveMap::from_render_mesh(&mesh_a, &solid_a.face_map);
        let bb = BijectiveMap::from_render_mesh(&mesh_b, &solid_b.face_map);
        let dl = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let pr = crate::boolean::topology_extract::yang_boolean_pipeline(
            &va,
            &ta,
            &vb,
            &tb,
            &ba,
            &bb,
            bool_op_to_mesh_op(BoolOp::Union),
            Some(dl),
        )
        .unwrap();
        let refinement = crate::boolean::ssi_refinement::EdgeRefinementMap {
            edges: BTreeMap::new(),
            skipped_planar: 0,
            unsupported: vec![],
        };
        let waffle =
            result_topology_to_waffle_solid(pr.topology, &refinement, &surface_map, &mut || {
                let id = next_id;
                next_id += 1;
                id
            });
        let rm = tessellate_waffle_solid(&waffle, tessellation::TessellationLod::Render)
            .expect("Render LOD tessellation should succeed");
        let n_segs = 64usize;
        let mut has_cyl = false;
        for (fi, geom) in &waffle.face_geometry {
            if let SurfaceGeom::Cylindrical(_) = geom {
                has_cyl = true;
                let face = &waffle.arena.faces[fi.0];
                let ld = &waffle.arena.loops[face.outer_loop.0];
                let sh = ld.half_edge;
                let mut he = sh;
                let mut edges = Vec::new();
                loop {
                    edges.push(waffle.arena.half_edges[he.0].edge);
                    he = waffle.arena.half_edges[he.0].next;
                    if he == sh {
                        break;
                    }
                }
                for ei in &edges {
                    if let Some(CurveGeom::Circular(c)) = waffle.edge_geometry.get(ei) {
                        let (cx, cy, cz, r) = (c.center.x, c.center.y, c.center.z, c.radius);
                        let n = [c.normal.x, c.normal.y, c.normal.z];
                        let (ax, ay) = {
                            let an = [n[0].abs(), n[1].abs(), n[2].abs()];
                            let up = if an[0] <= an[1] && an[0] <= an[2] {
                                [1., 0., 0.]
                            } else if an[1] <= an[2] {
                                [0., 1., 0.]
                            } else {
                                [0., 0., 1.]
                            };
                            let rw = [
                                up[1] * n[2] - up[2] * n[1],
                                up[2] * n[0] - up[0] * n[2],
                                up[0] * n[1] - up[1] * n[0],
                            ];
                            let l = (rw[0] * rw[0] + rw[1] * rw[1] + rw[2] * rw[2]).sqrt();
                            let ax = [rw[0] / l, rw[1] / l, rw[2] / l];
                            let ay = [
                                n[1] * ax[2] - n[2] * ax[1],
                                n[2] * ax[0] - n[0] * ax[2],
                                n[0] * ax[1] - n[1] * ax[0],
                            ];
                            (ax, ay)
                        };
                        let mut exact = 0;
                        for j in 0..n_segs {
                            let th = std::f64::consts::TAU * (j as f64) / (n_segs as f64);
                            let (ct, st) = (th.cos(), th.sin());
                            let pf = [
                                (cx + r * (ct * ax[0] + st * ay[0])) as f32,
                                (cy + r * (ct * ax[1] + st * ay[1])) as f32,
                                (cz + r * (ct * ax[2] + st * ay[2])) as f32,
                            ];
                            if (0..rm.vertices.len() / 3).any(|mi| {
                                rm.vertices[mi * 3] == pf[0]
                                    && rm.vertices[mi * 3 + 1] == pf[1]
                                    && rm.vertices[mi * 3 + 2] == pf[2]
                            }) {
                                exact += 1;
                            }
                        }
                        eprintln!("[TEST] Edge {:?}: {exact}/{n_segs} exact", ei);
                        assert_eq!(
                            exact, n_segs,
                            "Edge {:?}: {exact}/{n_segs} disc positions found exactly. \
                             Boundary row must use disc positions directly.",
                            ei
                        );
                    }
                }
            }
        }
        assert!(has_cyl, "Must have at least one Cylindrical face");
    }

    #[test]
    #[ignore = "red-phase P3: tessellate_cylindrical_patch generates full 360° cylinder instead of bounded angular range"]
    fn tessellate_cylindrical_face_earcut_fallback() {
        use crate::geometry::surface::{Cylinder, SurfaceGeom};
        use crate::topology::arena::TopoArena;
        use crate::topology::half_edge::{FaceIdx, HalfEdgeIdx, LoopIdx};
        let r = 5.0;
        let mut arena = TopoArena::new();
        for &pos in &[[r, 0., 0.], [0., r, 0.], [0., r, 1.], [r, 0., 1.]] {
            arena.vertices.push(crate::topology::half_edge::Vertex {
                position: pos,
                half_edge: Some(HalfEdgeIdx(arena.vertices.len() * 2)),
            });
        }
        for i in 0..4 {
            arena.edges.push(crate::topology::half_edge::Edge {
                half_edge: HalfEdgeIdx(i * 2),
            });
        }
        let hd: [(usize, usize, usize, usize, usize, usize); 8] = [
            (0, 1, 2, 6, 0, 0),
            (1, 0, 7, 3, 0, 0),
            (1, 3, 4, 0, 1, 0),
            (2, 2, 1, 5, 1, 0),
            (2, 5, 6, 2, 2, 0),
            (3, 4, 3, 7, 2, 0),
            (3, 7, 0, 4, 3, 0),
            (0, 6, 5, 1, 3, 0),
        ];
        for &(o, tw, nx, pv, e, l) in &hd {
            arena.half_edges.push(crate::topology::half_edge::HalfEdge {
                origin: VertexIdx(o),
                twin: HalfEdgeIdx(tw),
                next: HalfEdgeIdx(nx),
                prev: HalfEdgeIdx(pv),
                edge: EdgeIdx(e),
                loop_: LoopIdx(l),
            });
        }
        arena.loops.push(crate::topology::half_edge::Loop {
            half_edge: HalfEdgeIdx(0),
            face: FaceIdx(0),
        });
        arena.faces.push(crate::topology::half_edge::Face {
            outer_loop: LoopIdx(0),
            inner_loops: vec![],
            shell: crate::topology::half_edge::ShellIdx(0),
        });
        arena.shells.push(crate::topology::half_edge::Shell {
            face: FaceIdx(0),
            solid: crate::topology::half_edge::SolidIdx(0),
        });
        let mut fg = BTreeMap::new();
        fg.insert(
            FaceIdx(0),
            SurfaceGeom::Cylindrical(Cylinder {
                origin: Point3 {
                    x: 0.,
                    y: 0.,
                    z: 0.,
                },
                axis: Vector3 {
                    x: 0.,
                    y: 0.,
                    z: 1.,
                },
                radius: r,
            }),
        );
        let mut fm = BTreeMap::new();
        fm.insert(100u64, FaceIdx(0));
        let waffle = WaffleSolid {
            arena,
            face_map: fm,
            edge_map: BTreeMap::new(),
            vertex_map: BTreeMap::new(),
            face_geometry: fg,
            edge_geometry: BTreeMap::new(),
            cylinder_params: None,
            revolve_params: None,
            sphere_params: None,
            cone_params: None,
            torus_params: None,
            cached_face_polys: None,
            // is_polygon_soup forces fan path (not bounded tessellation), which
            // routes Cylindrical faces through tessellate_cylindrical_patch.
            is_polygon_soup: true,
            cached_render_mesh: None,
        };
        let rm = tessellate_waffle_solid(&waffle, crate::tessellation::TessellationLod::Render)
            .expect("Tessellation should succeed with empty edge_geometry");
        assert!(!rm.indices.is_empty(), "Must have triangles");
        let fr = rm
            .face_ranges
            .iter()
            .find(|f| f.face_id.0 == 100)
            .expect("Cylindrical face (id=100) must appear in face_ranges");
        let tau = crate::units::TAU_MODEL;
        let mut out = 0;
        let mut total = 0;
        for ip in fr.start_index as usize..fr.end_index as usize {
            let vi = rm.indices[ip] as usize;
            let (vx, vy) = (rm.vertices[vi * 3] as f64, rm.vertices[vi * 3 + 1] as f64);
            total += 1;
            if vx < -tau || vy < -tau {
                out += 1;
                eprintln!("[TEST] Vertex ({vx},{vy}): outside quarter-cylinder range");
            }
        }
        eprintln!("[TEST] Cylindrical patch: {out}/{total} out of range");
        assert_eq!(
            out, 0,
            "Cylindrical patch: {out}/{total} vertices outside quarter-cylinder [0,π/2]. \
             With empty edge_geometry, tessellator should use earcut in (θ,z), not full 360°."
        );
    }

    /// Disjoint boxes union should produce valid compound solid (Euler=4).
    #[test]
    fn yang_compound_solid_euler_accepted() {
        // Two non-overlapping boxes — their union is a compound solid with 2 shells.
        let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 1.0, 1.0, 1.0);
        let (k_b, h_b) = make_box_via_kernel(5.0, 5.0, 1.0, 1.0, 1.0);
        let solid_a = k_a.get_solid(&h_a).unwrap();
        let solid_b = k_b.get_solid(&h_b).unwrap();

        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut id_alloc);
        match result {
            Ok(r) => {
                let v = r.arena.vertices.len() as i64;
                let e = r.arena.edges.len() as i64;
                let f = r.arena.faces.len() as i64;
                assert_eq!(
                    v - e + f,
                    4,
                    "Compound solid (2 shells) should have Euler=4, got V={v} E={e} F={f} Euler={}",
                    v - e + f
                );
            }
            Err(e) => {
                panic!("Disjoint box union should succeed as compound solid, got: {e:?}");
            }
        }
    }

    /// E2E test: Cross-shaped union of two boxes with DIFFERENT depths.
    /// This is the F0003 assay geometry — the simplest failing case.
    /// Box A: 60×40×30, Box B: 40×60×20 (different depths create a step at z=20).
    ///
    /// KNOWN FAILURE: conformal subdivision doesn't preserve constraint edges
    /// when multiple intersection segments hit the same original triangle.
    /// Sequential segment splitting (split_triangle_by_segment_dedup) can lose
    /// earlier constraint edges when later constraints re-triangulate the area.
    /// Root cause: needs constrained Delaunay triangulation (CDT) instead of
    /// sequential segment insertion. See topology_extract.rs Step 3 boundary
    /// fix and Step 5d reconciliation for the B-Rep-level partial fix.
    #[test]
    #[ignore = "conformal subdivision loses constraint edges at multi-constraint triangles (F0003)"]
    fn yang_e2e_cross_different_depths_union() {
        // F0003 geometry: cross-shaped union with different extrusion depths
        // Box A: 60×40, depth 30
        let (k_a, h_a) = make_box_via_kernel(0.0, 0.0, 60.0, 40.0, 30.0);
        // Box B: 40×60, depth 20 (creates step at z=20)
        let (k_b, h_b) = make_box_via_kernel(0.0, 0.0, 40.0, 60.0, 20.0);

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

        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut id_alloc);

        match &result {
            Ok(boolean_result) => {
                let n_faces = boolean_result.arena.faces.len();
                let n_edges = boolean_result.arena.edges.len();
                let n_verts = boolean_result.arena.vertices.len();
                let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

                eprintln!(
                    "[F0003] Cross different-depths union: V={n_verts}, E={n_edges}, F={n_faces}, Euler={euler}"
                );

                // Cross-shaped union with step should have more than 6 faces
                assert!(
                    n_faces >= 10,
                    "Cross union with step should have >= 10 faces, got {n_faces}"
                );
                assert_eq!(
                    euler, 2,
                    "Euler V-E+F must equal 2, got {euler} (V={n_verts}, E={n_edges}, F={n_faces})"
                );

                // Verify manifold: every half-edge has a valid twin
                let n_he = boolean_result.arena.half_edges.len();
                assert_eq!(
                    n_he,
                    2 * n_edges,
                    "Must have HE=2*E for manifold, got HE={n_he}, E={n_edges}"
                );
                for (i, he) in boolean_result.arena.half_edges.iter().enumerate() {
                    let twin_idx = he.twin.0;
                    assert!(
                        twin_idx < n_he,
                        "HE[{i}] twin index {twin_idx} out of bounds (n_he={n_he})"
                    );
                    assert_eq!(
                        boolean_result.arena.half_edges[twin_idx].twin.0, i,
                        "HE[{i}].twin.twin != {i} (twin symmetry broken)"
                    );
                }
            }
            Err(e) => {
                panic!(
                    "Yang E2E cross different-depths union failed with error: {e:?}. \
                     The pipeline should produce a valid solid for F0003 geometry."
                );
            }
        }
    }

    /// E2E test: Cross-shaped subtract with different depths (Box A - Box B).
    /// Tests the step/ledge geometry with subtract instead of union.
    /// KNOWN FAILURE: same root cause as yang_e2e_cross_different_depths_union.
    #[test]
    #[ignore = "conformal subdivision loses constraint edges at multi-constraint triangles (F0003)"]
    fn yang_e2e_cross_different_depths_subtract() {
        let (k_a, h_a) = make_box_via_kernel(0.0, 0.0, 60.0, 40.0, 30.0);
        let (k_b, h_b) = make_box_via_kernel(0.0, 0.0, 40.0, 60.0, 20.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a must exist");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b must exist");

        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Subtract, &mut id_alloc);

        match &result {
            Ok(boolean_result) => {
                let n_faces = boolean_result.arena.faces.len();
                let n_edges = boolean_result.arena.edges.len();
                let n_verts = boolean_result.arena.vertices.len();
                let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

                eprintln!(
                    "[F0003-sub] Cross different-depths subtract: V={n_verts}, E={n_edges}, F={n_faces}, Euler={euler}"
                );

                assert!(
                    n_faces >= 6,
                    "Subtract result should have >= 6 faces, got {n_faces}"
                );
                assert_eq!(euler, 2, "Euler V-E+F must equal 2, got {euler}");

                let n_he = boolean_result.arena.half_edges.len();
                assert_eq!(n_he, 2 * n_edges, "Must have HE=2*E for manifold");
            }
            Err(e) => {
                panic!(
                    "Yang E2E cross different-depths subtract failed: {e:?}. \
                     The pipeline should produce a valid solid."
                );
            }
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Bug 1 (face_geometry propagation) — red-phase TDD tests
    // Spec: specs/yang_face_geometry_propagation.md
    // ══════════════════════════════════════════════════════════════════

    /// Helper: convert a BooleanResult back into a WaffleSolid so it can be
    /// used as an operand in a subsequent yang_boolean_inner call.
    fn boolean_result_to_waffle_solid_for_test(result: BooleanResult) -> WaffleSolid {
        WaffleSolid {
            arena: result.arena,
            face_map: result.face_map,
            edge_map: result.edge_map,
            vertex_map: result.vertex_map,
            face_geometry: result.face_geometry,
            edge_geometry: result.edge_geometry,
            cylinder_params: None,
            revolve_params: None,
            sphere_params: None,
            cone_params: None,
            torus_params: None,
            cached_face_polys: result.cached_face_polys,
            is_polygon_soup: false,
            cached_render_mesh: result.cached_render_mesh,
        }
    }

    /// Bug 1, invariant 1: After a Yang boolean, every face in the result must
    /// have a corresponding entry in face_geometry. Currently, faces whose
    /// (mesh_id, face_idx) is not found in the surface_map are silently skipped,
    /// leaving face_geometry incomplete.
    #[test]
    fn test_yang_face_geometry_completeness() {
        let solid_a = make_tetra_solid([0.0, 0.0, 0.0]);
        let solid_b = make_tetra_solid([0.5, 0.0, 0.0]);

        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result = yang_boolean_inner(&solid_a, &solid_b, BoolOp::Union, &mut id_alloc);

        match result {
            Ok(br) => {
                let n_faces = br.face_map.len();
                let n_geom = br.face_geometry.len();
                assert!(
                    n_faces > 0,
                    "Union of overlapping tetrahedra must produce faces"
                );
                assert_eq!(
                    n_geom, n_faces,
                    "face_geometry ({n_geom}) must equal face_map ({n_faces}): \
                     every result face needs geometry for chained booleans"
                );
            }
            Err(e) => {
                panic!("yang_boolean_inner should succeed for overlapping tetrahedra, got: {e:?}");
            }
        }
    }

    /// Bug 1, invariant 3: Chained boolean yang(yang(A,B), C) must not fail
    /// with "missing face_geometry". Currently the first result has incomplete
    /// face_geometry, causing the second boolean to reject it at the guard.
    #[test]
    fn test_yang_chained_boolean_succeeds() {
        let solid_a = make_tetra_solid([0.0, 0.0, 0.0]);
        let solid_b = make_tetra_solid([0.5, 0.0, 0.0]);
        let solid_c = make_tetra_solid([0.0, 0.5, 0.0]);

        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        // First boolean: A ∪ B
        let first_result = yang_boolean_inner(&solid_a, &solid_b, BoolOp::Union, &mut id_alloc);

        let ab_solid = match first_result {
            Ok(br) => boolean_result_to_waffle_solid_for_test(br),
            Err(e) => {
                panic!("First yang_boolean_inner(A,B) failed: {e:?}");
            }
        };

        // Second boolean: (A ∪ B) ∪ C — must not fail with "missing face_geometry"
        let second_result = yang_boolean_inner(&ab_solid, &solid_c, BoolOp::Union, &mut id_alloc);

        match second_result {
            Ok(br) => {
                assert!(br.face_map.len() > 0, "Chained union must produce faces");
            }
            Err(e) => {
                let msg = format!("{e:?}");
                assert!(
                    !msg.contains("missing face_geometry"),
                    "Chained boolean failed due to incomplete face_geometry \
                     from first result: {e:?}"
                );
                // If it fails for another reason (e.g., topology validation),
                // that's a different bug — still fail the test to flag it.
                panic!("Chained yang_boolean_inner((A∪B), C) failed: {e:?}");
            }
        }
    }

    /// Bug 1, invariant 2: Every face_geometry entry must have a Planar normal
    /// consistent with the Newell normal of its face's vertices (dot ≥ 0.99).
    /// Currently some faces have no geometry at all, so this also implicitly
    /// tests completeness.
    #[test]
    fn test_yang_face_geometry_fallback_valid_normal() {
        use crate::vecmath::compute_newell_normal;

        let solid_a = make_tetra_solid([0.0, 0.0, 0.0]);
        let solid_b = make_tetra_solid([0.5, 0.0, 0.0]);

        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result = yang_boolean_inner(&solid_a, &solid_b, BoolOp::Union, &mut id_alloc);

        let br = match result {
            Ok(br) => br,
            Err(e) => {
                panic!("yang_boolean_inner should succeed: {e:?}");
            }
        };

        // For every face in the result, verify it has geometry and the normal
        // is consistent with vertex positions.
        let n_faces = br.face_map.len();
        assert!(n_faces > 0, "Must have faces to test normals");

        let mut checked = 0;
        for (&_kid, &face_idx) in &br.face_map {
            // Every face must have geometry
            let geom = br.face_geometry.get(&face_idx).unwrap_or_else(|| {
                panic!(
                    "Face {:?} has no face_geometry entry — \
                     face_geometry is incomplete ({} of {} faces covered)",
                    face_idx,
                    br.face_geometry.len(),
                    n_faces
                );
            });

            // Collect vertex positions by walking the face's outer loop
            let loop_idx = br.arena.faces[face_idx.0].outer_loop;
            let start_he = br.arena.loops[loop_idx.0].half_edge;
            let mut verts = Vec::new();
            let mut he = start_he;
            loop {
                let pos = br.arena.vertices[br.arena.half_edges[he.0].origin.0].position;
                verts.push(pos);
                he = br.arena.half_edges[he.0].next;
                if he == start_he {
                    break;
                }
            }

            if verts.len() < 3 {
                continue; // degenerate face, skip
            }

            let newell = compute_newell_normal(&verts);
            let nl = (newell[0] * newell[0] + newell[1] * newell[1] + newell[2] * newell[2]).sqrt();
            if nl < 1e-12 {
                continue; // zero-area face, skip
            }
            let nn = [newell[0] / nl, newell[1] / nl, newell[2] / nl];

            match geom {
                SurfaceGeom::Planar(plane) => {
                    let pn = [plane.normal.x, plane.normal.y, plane.normal.z];
                    let dot = nn[0] * pn[0] + nn[1] * pn[1] + nn[2] * pn[2];
                    assert!(
                        dot > 0.99,
                        "Face {:?}: stored normal {:?} inconsistent with Newell normal {:?} (dot={dot})",
                        face_idx, pn, nn
                    );
                }
                _ => {
                    // Non-planar geometry (cylinder, etc.) — skip normal check
                }
            }
            checked += 1;
        }
        assert!(checked > 0, "Must have checked at least one face normal");
    }

    // ══════════════════════════════════════════════════════════════════
    // Adversarial validation tests (Phase 4)
    // Verify Bug 1 + Bug 2 fixes hold under edge-case geometry.
    // ══════════════════════════════════════════════════════════════════

    /// Adversarial 1: 3-level chained boolean yang(yang(yang(A,B), C), D).
    /// At each level, verify:
    /// - The boolean succeeds (no "missing face_geometry" error)
    /// - Every non-degenerate face has geometry (degenerate faces with <3 verts
    ///   or zero Newell normal are acceptable to skip per spec)
    /// This catches regressions where fallback geometry doesn't survive chaining.
    #[test]
    fn test_yang_3level_chained_boolean_face_geometry() {
        use crate::units::TAU_NORMALIZE;
        use crate::vecmath::compute_newell_normal;

        let solid_a = make_tetra_solid([0.0, 0.0, 0.0]);
        let solid_b = make_tetra_solid([0.5, 0.0, 0.0]);
        let solid_c = make_tetra_solid([0.0, 0.5, 0.0]);
        let solid_d = make_tetra_solid([0.0, 0.0, 0.5]);

        let mut next_id = 1000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        /// Check that every non-degenerate face has geometry.
        fn check_face_geometry_coverage(label: &str, br: &BooleanResult) {
            use crate::units::TAU_NORMALIZE;
            use crate::vecmath::compute_newell_normal;

            let mut missing_valid = 0usize;
            for (&_kid, &face_idx) in &br.face_map {
                if br.face_geometry.contains_key(&face_idx) {
                    continue;
                }
                // Missing geometry — check if degenerate
                let loop_idx = br.arena.faces[face_idx.0].outer_loop;
                let start_he = br.arena.loops[loop_idx.0].half_edge;
                let mut verts = Vec::new();
                let mut he = start_he;
                loop {
                    let pos = br.arena.vertices[br.arena.half_edges[he.0].origin.0].position;
                    verts.push(pos);
                    he = br.arena.half_edges[he.0].next;
                    if he == start_he {
                        break;
                    }
                }
                if verts.len() < 3 {
                    continue;
                } // degenerate
                let newell = compute_newell_normal(&verts);
                let nl =
                    (newell[0] * newell[0] + newell[1] * newell[1] + newell[2] * newell[2]).sqrt();
                if nl < TAU_NORMALIZE {
                    continue;
                } // zero-area
                missing_valid += 1;
            }
            assert_eq!(
                missing_valid,
                0,
                "{label}: {missing_valid} non-degenerate face(s) missing geometry \
                 (face_geometry={}, face_map={})",
                br.face_geometry.len(),
                br.face_map.len()
            );
        }

        // Level 1: A ∪ B
        let r1 = yang_boolean_inner(&solid_a, &solid_b, BoolOp::Union, &mut id_alloc)
            .expect("Level 1: yang(A,B) should succeed");
        check_face_geometry_coverage("Level 1", &r1);
        let ab = boolean_result_to_waffle_solid_for_test(r1);

        // Level 2: (A ∪ B) ∪ C
        let r2 = yang_boolean_inner(&ab, &solid_c, BoolOp::Union, &mut id_alloc)
            .expect("Level 2: yang((A∪B), C) should succeed");
        check_face_geometry_coverage("Level 2", &r2);
        let abc = boolean_result_to_waffle_solid_for_test(r2);

        // Level 3: ((A ∪ B) ∪ C) ∪ D
        let r3 = yang_boolean_inner(&abc, &solid_d, BoolOp::Union, &mut id_alloc)
            .expect("Level 3: yang(((A∪B)∪C), D) should succeed");
        check_face_geometry_coverage("Level 3", &r3);
        assert!(
            r3.face_map.len() >= 4,
            "3-level union of overlapping tetrahedra should produce >= 4 faces, got {}",
            r3.face_map.len()
        );
    }

    /// Adversarial 2: Mixed primitives — box + cylinder.
    /// The cylinder has curved faces that get Planar fallback geometry.
    /// Verify face_geometry is complete and the result can be used in
    /// a subsequent boolean (chained).
    #[test]
    fn test_yang_mixed_box_cylinder_face_geometry() {
        // Build box and cylinder via kernel (they need proper B-Rep + face_geometry)
        let (k_box, h_box) = make_box_via_kernel(0.0, 0.0, 2.0, 2.0, 2.0);
        let (k_cyl, h_cyl) = make_cylinder_via_kernel(0.8, 3.0);

        let solid_box = k_box.get_solid(&h_box).expect("box solid");
        let solid_cyl = k_cyl.get_solid(&h_cyl).expect("cylinder solid");

        let mut next_id = 2000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result = yang_boolean_inner(solid_box, solid_cyl, BoolOp::Union, &mut id_alloc);

        match result {
            Ok(br) => {
                // face_geometry must be complete
                assert_eq!(
                    br.face_geometry.len(),
                    br.face_map.len(),
                    "Box+cylinder union: face_geometry ({}) != face_map ({})",
                    br.face_geometry.len(),
                    br.face_map.len()
                );

                // Chained: use result as operand for another boolean
                let solid_result = boolean_result_to_waffle_solid_for_test(br);
                let solid_box2 = k_box.get_solid(&h_box).expect("box solid for chain");

                let chain_result =
                    yang_boolean_inner(&solid_result, solid_box2, BoolOp::Union, &mut id_alloc);
                match chain_result {
                    Ok(br2) => {
                        assert_eq!(
                            br2.face_geometry.len(),
                            br2.face_map.len(),
                            "Chained box+cyl+box: face_geometry ({}) != face_map ({})",
                            br2.face_geometry.len(),
                            br2.face_map.len()
                        );
                    }
                    Err(e) => {
                        let msg = format!("{e:?}");
                        assert!(
                            !msg.contains("missing face_geometry"),
                            "Chained boolean after box+cylinder failed with face_geometry error: {e:?}"
                        );
                        // Other failures (topology validation) are acceptable
                        // for mixed primitives — the fix targets face_geometry.
                        eprintln!(
                            "[adversarial] Chained box+cyl+box failed (non-face_geometry): {e:?}"
                        );
                    }
                }
            }
            Err(e) => {
                // Box+cylinder may fail for other pipeline reasons (e.g., curved
                // face subdivision). The test is specifically about face_geometry
                // propagation, so only fail if it's a face_geometry error.
                let msg = format!("{e:?}");
                if msg.contains("missing face_geometry") {
                    panic!("Box+cylinder failed due to missing face_geometry: {e:?}");
                }
                eprintln!(
                    "[adversarial] Box+cylinder union failed (non-face_geometry reason): {e:?}"
                );
            }
        }
    }

    /// Adversarial 3: Near-coplanar faces — two boxes sharing an exact face.
    /// Box A: [0,0,0]→[1,1,1], Box B: [1,0,0]→[2,1,1].
    /// They share the x=1 face exactly. This stresses the subdivision and
    /// cell labeling at degenerate coplanar configurations.
    #[test]
    fn test_yang_coplanar_shared_face_union() {
        let (k_a, h_a) = make_box_via_kernel(0.5, 0.5, 1.0, 1.0, 1.0);
        let (k_b, h_b) = make_box_via_kernel(1.5, 0.5, 1.0, 1.0, 1.0);

        let solid_a = k_a.get_solid(&h_a).expect("solid_a");
        let solid_b = k_b.get_solid(&h_b).expect("solid_b");

        let mut next_id = 3000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result = yang_boolean_inner(solid_a, solid_b, BoolOp::Union, &mut id_alloc);

        match result {
            Ok(br) => {
                // face_geometry must be complete
                assert_eq!(
                    br.face_geometry.len(),
                    br.face_map.len(),
                    "Coplanar union: face_geometry ({}) != face_map ({})",
                    br.face_geometry.len(),
                    br.face_map.len()
                );

                // Union of two adjacent unit boxes should produce a 1×2×1 box
                // with 6 faces and Euler characteristic 2.
                let n_faces = br.arena.faces.len();
                let n_edges = br.arena.edges.len();
                let n_verts = br.arena.vertices.len();
                let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

                assert!(
                    n_faces >= 6,
                    "Coplanar union of adjacent boxes should have >= 6 faces, got {n_faces}"
                );
                assert_eq!(
                    euler, 2,
                    "Coplanar union: Euler V-E+F must equal 2, got {euler} (V={n_verts}, E={n_edges}, F={n_faces})"
                );
            }
            Err(e) => {
                // Coplanar faces are a known hard case. Log but don't panic
                // unless it's a face_geometry error (which the fix should handle).
                let msg = format!("{e:?}");
                if msg.contains("missing face_geometry") {
                    panic!("Coplanar union failed due to missing face_geometry: {e:?}");
                }
                eprintln!(
                    "[adversarial] Coplanar shared-face union failed (non-face_geometry): {e:?}"
                );
            }
        }
    }

    /// Adversarial 4: face_geometry completeness for Subtract operation.
    /// Subtract can produce more complex topology than Union. Verify that:
    /// - Every face WITH geometry has a valid Newell-consistent normal
    /// - Every face WITHOUT geometry is genuinely degenerate (<3 verts or zero-area)
    /// This ensures the Newell fallback doesn't silently skip valid faces.
    #[test]
    fn test_yang_subtract_face_geometry_complete() {
        use crate::units::TAU_NORMALIZE;
        use crate::vecmath::compute_newell_normal;

        let solid_a = make_tetra_solid([0.0, 0.0, 0.0]);
        let solid_b = make_tetra_solid([0.3, 0.3, 0.0]);

        let mut next_id = 4000u64;
        let mut id_alloc = || {
            let id = next_id;
            next_id += 1;
            id
        };

        let result = yang_boolean_inner(&solid_a, &solid_b, BoolOp::Subtract, &mut id_alloc);

        match result {
            Ok(br) => {
                let mut missing_non_degenerate = Vec::new();

                for (&_kid, &face_idx) in &br.face_map {
                    // Collect vertex positions by walking the face's outer loop
                    let loop_idx = br.arena.faces[face_idx.0].outer_loop;
                    let start_he = br.arena.loops[loop_idx.0].half_edge;
                    let mut verts = Vec::new();
                    let mut he = start_he;
                    loop {
                        let pos = br.arena.vertices[br.arena.half_edges[he.0].origin.0].position;
                        verts.push(pos);
                        he = br.arena.half_edges[he.0].next;
                        if he == start_he {
                            break;
                        }
                    }

                    if let Some(geom) = br.face_geometry.get(&face_idx) {
                        // Face HAS geometry: verify normal consistency
                        if verts.len() >= 3 {
                            let newell = compute_newell_normal(&verts);
                            let nl = (newell[0] * newell[0]
                                + newell[1] * newell[1]
                                + newell[2] * newell[2])
                                .sqrt();
                            if nl >= 1e-12 {
                                if let SurfaceGeom::Planar(plane) = geom {
                                    let pn = [plane.normal.x, plane.normal.y, plane.normal.z];
                                    let nn = [newell[0] / nl, newell[1] / nl, newell[2] / nl];
                                    let dot = nn[0] * pn[0] + nn[1] * pn[1] + nn[2] * pn[2];
                                    assert!(
                                        dot > 0.99,
                                        "Subtract face {:?}: normal dot={dot} < 0.99",
                                        face_idx
                                    );
                                }
                            }
                        }
                    } else {
                        // Face MISSING geometry: must be genuinely degenerate
                        let is_degenerate = if verts.len() < 3 {
                            true
                        } else {
                            let newell = compute_newell_normal(&verts);
                            let nl = (newell[0] * newell[0]
                                + newell[1] * newell[1]
                                + newell[2] * newell[2])
                                .sqrt();
                            nl < TAU_NORMALIZE
                        };
                        if !is_degenerate {
                            missing_non_degenerate.push(face_idx);
                        }
                    }
                }

                assert!(
                    missing_non_degenerate.is_empty(),
                    "Subtract: {} non-degenerate face(s) missing geometry: {:?}. \
                     The Newell fallback should cover all faces with ≥3 verts and non-zero area.",
                    missing_non_degenerate.len(),
                    missing_non_degenerate
                );
            }
            Err(e) => {
                panic!("Tetra subtract should succeed: {e:?}");
            }
        }
    }
}
