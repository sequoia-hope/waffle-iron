//! Tessellation — converting B-Rep faces to triangle meshes.
//!
//! Handles flat (planar) face triangulation using ear-clipping (non-convex)
//! or fan decomposition (convex fast-path), and geometry-driven tessellation
//! for cylindrical faces and circular caps.

mod analytic;
pub mod bijective;
pub mod cdt;
mod diagnostics;
pub mod pr7_classify;

use crate::geometry::curve::CurveGeom;
use crate::geometry::surface::SurfaceGeom;
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
use crate::units::{
    MIN_FEATURE_SIZE, TAU_MODEL, TAU_NORMALIZE, TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN, TAU_WORK,
};
use crate::vecmath::{
    compute_plane_basis, v3_add, v3_cross, v3_dot, v3_length, v3_normalize, v3_scale, v3_sub,
};
use crate::waffle_kernel::{
    rotate_point_around_axis, ConeParams, CylinderParams, RevolveParams, SphereParams, TorusParams,
};
use std::collections::BTreeMap;

use self::analytic::{
    tessellate_cone_solid, tessellate_sphere_face, tessellate_sphere_solid, tessellate_torus_solid,
};
use self::diagnostics::count_unpaired_in_mesh;

/// Default number of segments for circular/cylindrical tessellation.
const CIRCLE_SEGMENTS_DEFAULT: usize = 64;

/// Level of detail for tessellation output.
///
/// The Yang hybrid boolean pipeline [#24] uses mesh geometry only as a
/// computational tool for topology extraction — rendering quality is irrelevant.
/// Boolean LOD uses far fewer segments, reducing triangle counts by ~16× for
/// curved surfaces and making O(n·m) subdivision feasible for complex models.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub(crate) enum TessellationLod {
    /// Full rendering quality (circle_segments = 64).
    Render,
    /// Reduced quality for boolean computation (circle_segments = 16).
    Boolean,
    /// Error-bounded adaptive segments per Yang 2025 Section 4.1.
    /// d_epsilon = max allowable surface-to-mesh chord error.
    Adaptive { d_epsilon: f64 },
}

impl TessellationLod {
    /// Circle segment count for this LOD level.
    /// For Adaptive, returns the fallback; use `resolve_adaptive_segments`
    /// for per-solid computation.
    pub(crate) fn circle_segments(self) -> usize {
        match self {
            TessellationLod::Render => 64,
            TessellationLod::Boolean => 16,
            TessellationLod::Adaptive { .. } => 16, // fallback; overridden in dispatch
        }
    }
}

/// Compute minimum circle segment count so that chord error < d_epsilon.
///
/// For a circle of radius `r` discretized into `n` equal chords,
/// the sagitta (max chord-to-arc distance) is: r * (1 - cos(π/n)).
/// Solving for n: n >= π / acos(1 - d_epsilon/r).
///
/// Returns the segment count clamped to [min_segments, 256].
/// Ref [#24]: Yang 2025 Section 4.1 — error-bounded surface discretization.
pub(crate) fn adaptive_circle_segments(radius: f64, d_epsilon: f64, min_segments: usize) -> usize {
    if d_epsilon <= 0.0 || radius <= 0.0 {
        return min_segments;
    }
    let ratio = d_epsilon / radius;
    if ratio >= 2.0 {
        return min_segments; // d_epsilon >= diameter; coarse is fine
    }
    let n = std::f64::consts::PI / (1.0 - ratio).acos();
    let n = n.ceil() as usize;
    n.clamp(min_segments, 256)
}

use std::cell::{Cell, RefCell};

thread_local! {
    /// Thread-local override for circle segment count. Defaults to 64 (render quality).
    /// Set temporarily by `tessellate_solid_ext_with_lod` for boolean LOD.
    static CIRCLE_SEGMENTS_OVERRIDE: Cell<usize> = const { Cell::new(CIRCLE_SEGMENTS_DEFAULT) };

    /// D1 Tier 2a probe: thread-local copy of the current WaffleSolid's
    /// `edge_is_intersection` marker. Set at the tessellate-entry point in
    /// yang_integration.rs and consulted by `tessellate_planar_face_bounded`
    /// when `Y47T2_INTERSECTION_PROBE=1`. Default-off byte-identical.
    pub(crate) static EDGE_IS_INTERSECTION_PROBE: RefCell<BTreeMap<EdgeIdx, bool>> = RefCell::new(BTreeMap::new());
}

/// Get the current circle segment count (respects thread-local LOD override).
#[inline]
fn circle_segments() -> usize {
    CIRCLE_SEGMENTS_OVERRIDE.with(|c| c.get())
}

// All references to the old `CIRCLE_SEGMENTS` const in this module and
// submodules have been replaced with `circle_segments()` calls.

/// Tessellate all faces in a solid, dispatching per-face based on geometry type.
///
/// For polygon (box) solids: uses fan triangulation (same as before).
/// For cylinder solids: uses geometry-driven circular cap + cylindrical side tessellation.
#[allow(dead_code)] // Called from test code in analytical.rs and waffle_kernel_tests.rs
pub(crate) fn tessellate_solid(
    arena: &TopoArena,
    face_map: &BTreeMap<u64, FaceIdx>,
    face_geometry: &BTreeMap<FaceIdx, SurfaceGeom>,
    _edge_geometry: &BTreeMap<EdgeIdx, CurveGeom>,
    cylinder_params: Option<&CylinderParams>,
    revolve_params: Option<&RevolveParams>,
    is_polygon_soup: bool,
) -> Result<RenderMesh, KernelError> {
    tessellate_solid_ext(
        arena,
        face_map,
        face_geometry,
        _edge_geometry,
        cylinder_params,
        revolve_params,
        None,
        None,
        None,
        is_polygon_soup,
    )
}

/// Extended tessellation with explicit LOD control.
///
/// Sets the thread-local circle segment count for the duration of the call,
/// then delegates to `tessellate_solid_ext`. This is used by the Yang boolean
/// pipeline to reduce tessellation density (Boolean LOD = 16 segments vs
/// Render LOD = 64 segments), cutting triangle counts by ~16× on curved surfaces.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_solid_ext_with_lod(
    arena: &TopoArena,
    face_map: &BTreeMap<u64, FaceIdx>,
    face_geometry: &BTreeMap<FaceIdx, SurfaceGeom>,
    edge_geometry: &BTreeMap<EdgeIdx, CurveGeom>,
    cylinder_params: Option<&CylinderParams>,
    revolve_params: Option<&RevolveParams>,
    sphere_params: Option<&SphereParams>,
    cone_params: Option<&ConeParams>,
    torus_params: Option<&TorusParams>,
    is_polygon_soup: bool,
    lod: TessellationLod,
) -> Result<RenderMesh, KernelError> {
    let segs = match lod {
        TessellationLod::Adaptive { d_epsilon } => {
            // Compute max segments needed across all curved faces.
            // Per Yang 2025 Section 4.1: surface-to-mesh distance < d_epsilon.
            let mut max_segs = 4usize;
            for geom in face_geometry.values() {
                let radius = geom
                    .characteristic_radius()
                    .or_else(|| cone_params.map(|cp| cp.radius));
                if let Some(r) = radius {
                    let n = adaptive_circle_segments(r, d_epsilon, 4);
                    max_segs = max_segs.max(n);
                }
            }
            max_segs.clamp(4, 256)
        }
        other => other.circle_segments(),
    };
    CIRCLE_SEGMENTS_OVERRIDE.with(|c| {
        let prev = c.get();
        c.set(segs);
        let result = tessellate_solid_ext(
            arena,
            face_map,
            face_geometry,
            edge_geometry,
            cylinder_params,
            revolve_params,
            sphere_params,
            cone_params,
            torus_params,
            is_polygon_soup,
        );
        c.set(prev); // restore previous value
        result
    })
}

/// Extended tessellation function with sphere params support.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_solid_ext(
    arena: &TopoArena,
    face_map: &BTreeMap<u64, FaceIdx>,
    face_geometry: &BTreeMap<FaceIdx, SurfaceGeom>,
    _edge_geometry: &BTreeMap<EdgeIdx, CurveGeom>,
    cylinder_params: Option<&CylinderParams>,
    revolve_params: Option<&RevolveParams>,
    sphere_params: Option<&SphereParams>,
    cone_params: Option<&ConeParams>,
    torus_params: Option<&TorusParams>,
    is_polygon_soup: bool,
) -> Result<RenderMesh, KernelError> {
    // Boolean results have no CylinderParams/RevolveParams. Use edge-first
    // bounded tessellation for watertight-by-construction output.
    // Requirements for bounded path:
    //   1. Must NOT have arc edges. Parallel cyl-cyl booleans produce arc edges
    //      whose trimmed-cylinder face topology the bounded path's ring-building
    //      logic doesn't yet handle correctly. These go through the fan path
    //      with post-hoc position-based vertex welding for cross-face sharing.
    //   2. Must NOT be polygon-soup. Polygon-soup B-Rep from S-H clipping may
    //      contain internal faces; bounded tessellation's shared vertices make
    //      these indistinguishable from external faces, preventing removal.
    //      The fan path's per-face vertices allow `remove_isolated_triangles`
    //      to identify and remove internal face fragments.
    // Track whether the fan tessellation path is used (needs post-hoc
    // position-based vertex welding for cross-face index sharing).
    // Spec: full_edge_vertex_welding.md — weld ALL shared edges, not just arcs.
    let mut needs_fan_welding = false;

    if cylinder_params.is_none()
        && revolve_params.is_none()
        && sphere_params.is_none()
        && cone_params.is_none()
        && torus_params.is_none()
        && !is_polygon_soup
    {
        let has_arcs = _edge_geometry
            .values()
            .any(|e| matches!(e, CurveGeom::Arc(_)));
        if !has_arcs {
            return tessellate_solid_bounded(arena, face_map, face_geometry, _edge_geometry);
        }
        // Arc-edge boolean results: fall through to fan path below, which
        // handles trimmed cylindrical face topology correctly. After the fan
        // path produces the mesh, we apply position-based vertex welding to
        // create cross-face index sharing at all shared edge positions.
        needs_fan_welding = true;
    }

    // Sphere solids: tessellate all faces as a single shared-vertex mesh.
    // This ensures vertices on shared edges match exactly, avoiding
    // remove_isolated_triangles stripping at small radii.
    if let Some(sp) = sphere_params {
        return tessellate_sphere_solid(arena, face_map, sp);
    }

    if let Some(cp) = cone_params {
        return tessellate_cone_solid(arena, face_map, face_geometry, cp);
    }

    if let Some(tp) = torus_params {
        return tessellate_torus_solid(face_map, tp);
    }

    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    // Sort face_map entries for deterministic tessellation order.
    let mut sorted_faces: Vec<(u64, FaceIdx)> = face_map.iter().map(|(&k, &v)| (k, v)).collect();
    sorted_faces.sort_by_key(|(k, _)| *k);

    // When spherical faces are present without top-level sphere_params
    // (boolean results with sphere geometry), enable fan welding so that
    // per-face tessellation produces watertight shared edges.
    if sphere_params.is_none()
        && sorted_faces
            .iter()
            .any(|&(_, fi)| matches!(face_geometry.get(&fi), Some(SurfaceGeom::Spherical(_))))
    {
        needs_fan_welding = true;
    }

    // Pre-compute the revolve boundary-position pool: shared f64 positions
    // for the start cap (θ=0) and end cap (θ=angle_rad, partial only).
    // Mirrors `tessellate_solid_bounded`'s `discretize_edges` pattern: build
    // a shared position pool once, then have each face consume from it for
    // boundary vertices. Per Yang §4.1.1: identical f64 source → identical
    // f32 emit → bijective rendermesh edges → reciprocal half-edge twins.
    // Audit D-10 (Cluster I).
    let revolve_pool = revolve_params.map(RevolvePool::from_params);

    for &(kid, face_idx) in &sorted_faces {
        let geom = face_geometry.get(&face_idx);

        // Check if this face is a revolve lateral face
        if let Some(rp) = revolve_params {
            if let Some((lateral_idx, _lateral)) = rp
                .lateral_faces
                .iter()
                .enumerate()
                .find(|(_, (fi, _, _))| *fi == face_idx)
            {
                let pool = revolve_pool
                    .as_ref()
                    .expect("revolve_pool present whenever revolve_params is Some");
                let start_index = indices.len() as u32;
                tessellate_revolve_lateral(
                    arena,
                    face_idx,
                    pool,
                    &rp.axis_origin,
                    &rp.axis_dir,
                    rp.angle_rad,
                    rp.full_revolution,
                    geom,
                    &mut vertices,
                    &mut normals,
                    &mut indices,
                    kid,
                    lateral_idx,
                );
                let end_index = indices.len() as u32;
                face_ranges.push(FaceRange {
                    face_id: KernelId(kid),
                    start_index,
                    end_index,
                });
                continue;
            }
            // For full revolution, skip cap faces (they exist in topology but aren't rendered)
            if rp.full_revolution {
                continue;
            }
        }

        match geom {
            Some(SurfaceGeom::Cylindrical(cyl)) => {
                if let Some(cp) = cylinder_params {
                    // Cylindrical side face with CylinderParams — full parametric tessellation
                    let start_index = indices.len() as u32;
                    tessellate_cylindrical_face(
                        cp,
                        &[cyl.axis.x, cyl.axis.y, cyl.axis.z],
                        &mut vertices,
                        &mut normals,
                        &mut indices,
                    );
                    let end_index = indices.len() as u32;
                    face_ranges.push(FaceRange {
                        face_id: KernelId(kid),
                        start_index,
                        end_index,
                    });
                } else {
                    // Cylindrical face from boolean result — derive tessellation from geometry + boundary
                    let start_index = indices.len() as u32;
                    tessellate_cylindrical_patch(
                        arena,
                        face_idx,
                        cyl,
                        _edge_geometry,
                        &mut vertices,
                        &mut normals,
                        &mut indices,
                    );
                    let end_index = indices.len() as u32;
                    face_ranges.push(FaceRange {
                        face_id: KernelId(kid),
                        start_index,
                        end_index,
                    });
                }
            }
            Some(SurfaceGeom::Planar(plane)) => {
                // Check if this face has inner loops (holes)
                let has_inner = !arena.faces[face_idx.0].inner_loops.is_empty();

                if has_inner {
                    // Planar face with hole — use annular tessellation
                    let start_index = indices.len() as u32;
                    tessellate_planar_face_with_hole(
                        arena,
                        face_idx,
                        plane,
                        _edge_geometry,
                        &mut vertices,
                        &mut normals,
                        &mut indices,
                    );
                    let end_index = indices.len() as u32;
                    face_ranges.push(FaceRange {
                        face_id: KernelId(kid),
                        start_index,
                        end_index,
                    });
                } else {
                    // Check if this is a circular cap (self-loop face in cylinder)
                    if let Some(cp) = cylinder_params {
                        let loop_idx = arena.faces[face_idx.0].outer_loop;
                        let start_he = arena.loops[loop_idx.0].half_edge;
                        let is_self_loop = arena.half_edges[start_he.0].next == start_he;

                        if is_self_loop {
                            let cap_center = [plane.origin.x, plane.origin.y, plane.origin.z];
                            let cap_normal = [plane.normal.x, plane.normal.y, plane.normal.z];
                            let start_index = indices.len() as u32;
                            tessellate_circular_cap(
                                &cap_center,
                                cp.radius,
                                &cap_normal,
                                &cp.x_axis,
                                &cp.y_axis,
                                &mut vertices,
                                &mut normals,
                                &mut indices,
                            );
                            let end_index = indices.len() as u32;
                            face_ranges.push(FaceRange {
                                face_id: KernelId(kid),
                                start_index,
                                end_index,
                            });
                            continue;
                        }
                    }

                    // Check for self-loop face in boolean results (no cylinder_params)
                    let loop_idx = arena.faces[face_idx.0].outer_loop;
                    let start_he = arena.loops[loop_idx.0].half_edge;
                    let is_self_loop = arena.half_edges[start_he.0].next == start_he;
                    if is_self_loop {
                        // Circular cap without CylinderParams — derive from edge geometry
                        if let Some(CurveGeom::Circular(circle)) =
                            _edge_geometry.get(&arena.half_edges[start_he.0].edge)
                        {
                            let cap_center = [circle.center.x, circle.center.y, circle.center.z];
                            let cap_normal = [plane.normal.x, plane.normal.y, plane.normal.z];
                            let (cx_axis, cy_axis) = make_circle_axes(&cap_normal);
                            let start_index = indices.len() as u32;
                            tessellate_circular_cap(
                                &cap_center,
                                circle.radius,
                                &cap_normal,
                                &cx_axis,
                                &cy_axis,
                                &mut vertices,
                                &mut normals,
                                &mut indices,
                            );
                            let end_index = indices.len() as u32;
                            face_ranges.push(FaceRange {
                                face_id: KernelId(kid),
                                start_index,
                                end_index,
                            });
                            continue;
                        }
                    }

                    // Check for cap face bounded by arc edges (cyl-cyl boolean result)
                    let arc_bounded = is_arc_bounded_face(arena, face_idx, _edge_geometry);
                    if arc_bounded {
                        let start_index = indices.len() as u32;
                        tessellate_arc_bounded_cap(
                            arena,
                            face_idx,
                            plane,
                            _edge_geometry,
                            &mut vertices,
                            &mut normals,
                            &mut indices,
                        );
                        let end_index = indices.len() as u32;
                        face_ranges.push(FaceRange {
                            face_id: KernelId(kid),
                            start_index,
                            end_index,
                        });
                    } else if revolve_params.is_some() {
                        // Partial-revolve cap polygon: walk arena natural order
                        // (no Newell-vs-stored-normal flip) so that the cap's
                        // emitted directed edges reciprocate the lateral's
                        // arena-walked half-edge twins. Yang §4.1.1 reciprocity
                        // contract; audit D-10 (Cluster I).
                        tessellate_revolve_cap_polygon(
                            arena,
                            face_idx,
                            plane,
                            kid,
                            &mut vertices,
                            &mut normals,
                            &mut indices,
                            &mut face_ranges,
                        );
                    } else {
                        // Regular polygon face — fan triangulation
                        tessellate_polygon_face(
                            arena,
                            face_idx,
                            plane,
                            kid,
                            &mut vertices,
                            &mut normals,
                            &mut indices,
                            &mut face_ranges,
                        );
                    }
                }
            }
            Some(SurfaceGeom::Spherical(s)) => {
                // Use explicit sphere_params if available, otherwise derive
                // from the face's SurfaceGeom (for boolean results that carry
                // spherical face geometry without top-level sphere_params).
                let derived_sp;
                let sp_ref = if let Some(sp) = sphere_params {
                    sp
                } else {
                    derived_sp = SphereParams {
                        center: s.center.to_array(),
                        radius: s.radius,
                    };
                    &derived_sp
                };
                let start_index = indices.len() as u32;
                tessellate_sphere_face(
                    arena,
                    face_idx,
                    sp_ref,
                    &mut vertices,
                    &mut normals,
                    &mut indices,
                );
                let end_index = indices.len() as u32;
                face_ranges.push(FaceRange {
                    face_id: KernelId(kid),
                    start_index,
                    end_index,
                });
            }
            Some(SurfaceGeom::Conical(_)) | Some(SurfaceGeom::Toroidal(_)) => {
                // Analytic tessellation not yet implemented — use polygon fallback
                tessellate_polygon_face_fallback(
                    arena,
                    face_idx,
                    kid,
                    &mut vertices,
                    &mut normals,
                    &mut indices,
                    &mut face_ranges,
                );
            }
            None => {
                // No geometry — try polygon fallback
                tessellate_polygon_face_fallback(
                    arena,
                    face_idx,
                    kid,
                    &mut vertices,
                    &mut normals,
                    &mut indices,
                    &mut face_ranges,
                );
            }
        }
    }

    // D2 (2026-05-18): post-tessellation repair pipeline removed. Yang §4.4.3
    // says watertightness is "inherited from the mesh Boolean output" — no
    // post-processing cleanup. The ~225 LOC of fix_winding/remove_degenerate/
    // weld/fill/T-junction/non-manifold-removal passes that lived here were
    // legacy S-H clipping residue, masking upstream defects.

    // For all fan-path tessellations, weld vertices at shared positions across
    // face boundaries. The fan path produces per-face vertex blocks (no index
    // sharing), but watertight meshes require cross-face shared indices at
    // shared topological edges. We remap all face-local vertices at matching
    // positions to a single shared vertex index per unique position.
    // Spec: full_edge_vertex_welding.md — Invariant 1 (watertight mesh).
    if needs_fan_welding {
        weld_shared_edge_vertices(&vertices, &mut indices, &mut face_ranges);
        // Compact: remove unreferenced vertices left after welding remapped
        // their indices to earlier entries. This ensures used == total for
        // the vertex_sharing_stats oracle.
        compact_unreferenced_vertices(&mut vertices, &mut normals, &mut indices);
    }

    Ok(RenderMesh {
        vertices,
        normals,
        indices,
        face_ranges,
    })
}

/// Remove unreferenced vertex entries from the vertex/normal arrays and
/// remap indices to the compacted layout. After welding, some vertex entries
/// are no longer referenced by any triangle index; this removes them so that
/// total vertex count equals referenced vertex count.
fn compact_unreferenced_vertices(
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    indices: &mut [u32],
) {
    let n_verts = vertices.len() / 3;
    if n_verts == 0 {
        return;
    }

    // Find which vertices are referenced
    let mut used = vec![false; n_verts];
    for &idx in indices.iter() {
        if (idx as usize) < n_verts {
            used[idx as usize] = true;
        }
    }

    // Build remap: old index → new index (or u32::MAX if unreferenced)
    let mut remap: Vec<u32> = vec![u32::MAX; n_verts];
    let mut new_idx: u32 = 0;
    for i in 0..n_verts {
        if used[i] {
            remap[i] = new_idx;
            new_idx += 1;
        }
    }

    let new_count = new_idx as usize;
    if new_count == n_verts {
        return; // Nothing to compact
    }

    // Compact vertex and normal arrays in-place
    let mut write = 0;
    for read in 0..n_verts {
        if used[read] {
            vertices[write * 3] = vertices[read * 3];
            vertices[write * 3 + 1] = vertices[read * 3 + 1];
            vertices[write * 3 + 2] = vertices[read * 3 + 2];
            if normals.len() >= (read + 1) * 3 {
                normals[write * 3] = normals[read * 3];
                normals[write * 3 + 1] = normals[read * 3 + 1];
                normals[write * 3 + 2] = normals[read * 3 + 2];
            }
            write += 1;
        }
    }
    vertices.truncate(new_count * 3);
    normals.truncate(new_count * 3);

    // Remap indices
    for idx in indices.iter_mut() {
        *idx = remap[*idx as usize];
    }
}

/// Weld mesh vertices at shared positions so that adjacent faces sharing
/// a topological edge reference the same vertex index. This creates
/// cross-face index sharing for watertight mesh output from the fan
/// tessellation path (which produces per-face vertex blocks with no
/// index sharing).
///
/// All vertices at matching quantized positions are welded, regardless of
/// edge type (linear, arc, or other). This generalizes the former
/// arc-edge-only welding to all shared topological edges.
///
/// Spec: full_edge_vertex_welding.md
/// - Invariant 1: watertight mesh (every triangle edge paired)
/// - Invariant 2: no geometry change (index remapping only)
/// - Invariant 3: deterministic output (quantized grid + ordered iteration)
/// - Invariant 5: no degenerate triangles (removed after welding)
pub(crate) fn weld_shared_edge_vertices(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_verts = vertices.len() / 3;
    if n_verts == 0 {
        return;
    }

    // Build position map: quantize each vertex position to i64 grid at TAU_MODEL_RECIP
    // (resolution TAU_MODEL = 1e-7 m, one order below MIN_FEATURE_SIZE).
    // Map each unique quantized position to the first vertex index at that position.
    // All co-located vertices are welded unconditionally — this creates cross-face
    // index sharing for watertight meshes. Normals at shared vertices may belong
    // to different faces (hard edges), but this is acceptable because the
    // watertight oracle checks position-based edge pairing, not normal agreement.
    let mut position_map: BTreeMap<(i64, i64, i64), u32> = BTreeMap::new();
    let mut remap: Vec<u32> = (0..n_verts as u32).collect();

    let q = crate::units::TAU_MODEL_RECIP;
    for vi in 0..n_verts {
        let key = (
            (vertices[vi * 3] as f64 * q).round() as i64,
            (vertices[vi * 3 + 1] as f64 * q).round() as i64,
            (vertices[vi * 3 + 2] as f64 * q).round() as i64,
        );
        let first = *position_map.entry(key).or_insert(vi as u32);
        remap[vi] = first;
    }

    // Apply remap to indices.
    for idx in indices.iter_mut() {
        *idx = remap[*idx as usize];
    }

    // Remove degenerate triangles where welding collapsed two or more vertices
    // of a triangle to the same index (Invariant 5).
    let mut new_indices: Vec<u32> = Vec::with_capacity(indices.len());
    let mut new_face_ranges: Vec<FaceRange> = Vec::new();

    for fr in face_ranges.iter() {
        let new_start = new_indices.len() as u32;
        let tri_start = fr.start_index as usize;
        let tri_end = fr.end_index as usize;
        for tri in (tri_start..tri_end).step_by(3) {
            if tri + 2 >= indices.len() {
                break;
            }
            let a = indices[tri];
            let b = indices[tri + 1];
            let c = indices[tri + 2];
            // Skip degenerate triangles (two or more vertices mapped to same index)
            if a != b && b != c && a != c {
                new_indices.push(a);
                new_indices.push(b);
                new_indices.push(c);
            }
        }
        let new_end = new_indices.len() as u32;
        if new_end > new_start {
            new_face_ranges.push(FaceRange {
                face_id: fr.face_id,
                start_index: new_start,
                end_index: new_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_face_ranges;
}

/// Tessellate a circular cap as a fan: center + N perimeter vertices, N triangles.
#[allow(clippy::too_many_arguments)]
fn tessellate_circular_cap(
    center: &[f64; 3],
    radius: f64,
    normal: &[f64; 3],
    x_axis: &[f64; 3],
    y_axis: &[f64; 3],
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    let n = circle_segments();
    let base_vertex = vertices.len() as u32 / 3;
    let nf = [normal[0] as f32, normal[1] as f32, normal[2] as f32];

    // Center vertex
    vertices.push(center[0] as f32);
    vertices.push(center[1] as f32);
    vertices.push(center[2] as f32);
    normals.push(nf[0]);
    normals.push(nf[1]);
    normals.push(nf[2]);

    // Perimeter vertices
    for i in 0..n {
        let theta = std::f64::consts::TAU * (i as f64) / (n as f64);
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        let px = center[0] + radius * (cos_t * x_axis[0] + sin_t * y_axis[0]);
        let py = center[1] + radius * (cos_t * x_axis[1] + sin_t * y_axis[1]);
        let pz = center[2] + radius * (cos_t * x_axis[2] + sin_t * y_axis[2]);
        vertices.push(px as f32);
        vertices.push(py as f32);
        vertices.push(pz as f32);
        normals.push(nf[0]);
        normals.push(nf[1]);
        normals.push(nf[2]);
    }

    // Fan triangles: center, perimeter[i], perimeter[(i+1) % n]
    // Winding must match normal direction. If normal points in +Z, CCW from above.
    // We check: if normal dot (p1-center) x (p2-center) > 0, winding is correct.
    // For the first triangle, check and potentially reverse.
    let center_idx = base_vertex;
    let first_peri = base_vertex + 1;

    // Determine winding: compute cross product of first two perimeter vectors
    let p1_idx = first_peri as usize * 3;
    let p2_idx = (first_peri + 1) as usize * 3;
    let v1 = [
        vertices[p1_idx] as f64 - center[0],
        vertices[p1_idx + 1] as f64 - center[1],
        vertices[p1_idx + 2] as f64 - center[2],
    ];
    let v2 = [
        vertices[p2_idx] as f64 - center[0],
        vertices[p2_idx + 1] as f64 - center[1],
        vertices[p2_idx + 2] as f64 - center[2],
    ];
    let cross = v3_cross(v1, v2);
    let dot = cross[0] * normal[0] + cross[1] * normal[1] + cross[2] * normal[2];
    let reverse = dot < 0.0;

    for i in 0..n as u32 {
        let next = (i + 1) % n as u32;
        if reverse {
            indices.push(center_idx);
            indices.push(first_peri + next);
            indices.push(first_peri + i);
        } else {
            indices.push(center_idx);
            indices.push(first_peri + i);
            indices.push(first_peri + next);
        }
    }
}

/// Tessellate a cylindrical face as N quads (2N triangles).
fn tessellate_cylindrical_face(
    cp: &CylinderParams,
    _axis: &[f64; 3],
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    let n = circle_segments();
    let base_vertex = vertices.len() as u32 / 3;

    // Generate 2 rows of vertices: bottom ring and top ring
    for row in 0..2 {
        let z_offset = if row == 0 { 0.0 } else { cp.depth };
        for i in 0..n {
            let theta = std::f64::consts::TAU * (i as f64) / (n as f64);
            let cos_t = theta.cos();
            let sin_t = theta.sin();

            // Position
            let px = cp.center_bottom[0]
                + cp.radius * (cos_t * cp.x_axis[0] + sin_t * cp.y_axis[0])
                + z_offset * cp.direction[0];
            let py = cp.center_bottom[1]
                + cp.radius * (cos_t * cp.x_axis[1] + sin_t * cp.y_axis[1])
                + z_offset * cp.direction[1];
            let pz = cp.center_bottom[2]
                + cp.radius * (cos_t * cp.x_axis[2] + sin_t * cp.y_axis[2])
                + z_offset * cp.direction[2];
            vertices.push(px as f32);
            vertices.push(py as f32);
            vertices.push(pz as f32);

            // Normal = radial outward direction
            let nx = cos_t * cp.x_axis[0] + sin_t * cp.y_axis[0];
            let ny = cos_t * cp.x_axis[1] + sin_t * cp.y_axis[1];
            let nz = cos_t * cp.x_axis[2] + sin_t * cp.y_axis[2];
            normals.push(nx as f32);
            normals.push(ny as f32);
            normals.push(nz as f32);
        }
    }

    // Generate quads as 2 triangles each
    let n32 = n as u32;
    for i in 0..n32 {
        let next = (i + 1) % n32;
        let bot = base_vertex + i;
        let bot_next = base_vertex + next;
        let top = base_vertex + n32 + i;
        let top_next = base_vertex + n32 + next;

        // Two triangles per quad, wound for outward-facing normals
        indices.push(bot);
        indices.push(bot_next);
        indices.push(top);

        indices.push(top);
        indices.push(bot_next);
        indices.push(top_next);
    }
}

/// Shared boundary-position pool for revolve primitive tessellation.
///
/// Per Yang 2025 §4.1.1 bijective tessellation contract: the cap and lateral
/// faces of a revolve solid share B-Rep edges at θ=0 (start cap edges) and
/// θ=angle_rad (end cap edges, partial revolves only). The cap and lateral
/// tessellators must emit byte-identical f64 positions for these shared
/// boundary vertices, and reciprocal directed mesh edges (per the half-edge
/// twin convention).
///
/// `start_ring[i]` is the f64 position at θ=0 for profile-vertex i. By
/// construction in `revolve_polygon` (`waffle_kernel.rs:1498-1504`), this
/// equals the byte-identical f64 position stored in
/// `arena.vertices[bottom_verts[i]].position` — so a cap face emitting
/// `arena.vertices[v].position as f32` and a lateral face emitting
/// `pool.start_ring[i] as f32` produce identical f32 vertices.
///
/// `end_ring[i]` is the f64 position at θ=angle_rad. Computed via the SAME
/// `rotate_point_around_axis` function `revolve_polygon` uses for
/// `end_verts[i]` (`waffle_kernel.rs:1490-1493`) — guaranteeing byte-identical
/// f64 with `arena.vertices[top_verts[i]].position` (which equals
/// `end_verts[i]`). Empty for full revolutions.
///
/// Mirrors the `EdgeDiscretization.positions` pool that `tessellate_solid_bounded`
/// uses (`mod.rs:2657-2662`). Cap faces consume the pool implicitly via
/// arena-stored positions; the lateral tessellator consumes it explicitly to
/// replace its inline Rodrigues rotation (which produces ULP-divergent f64
/// positions due to operation-order differences from `rotate_point_around_axis`).
///
/// Audit D-10 (Cluster I, blocked-by-tessellation) in
/// `docs/audits/cherchi_port_audit.md`.
struct RevolvePool {
    start_ring: Vec<[f64; 3]>,
    end_ring: Vec<[f64; 3]>,
}

impl RevolvePool {
    /// Build the pool from a revolve solid's `RevolveParams`.
    ///
    /// `start_ring` is recovered from `lateral_faces`: walking the lateral
    /// faces in order, `lateral_faces[i].1 = start_verts[i]` (per
    /// `revolve_polygon` line 1613: `let v_a = start_verts[i];`). So the
    /// start-ring profile sequence is `lateral_faces.iter().map(|(_, va, _)| va)`.
    ///
    /// `end_ring[i]` is `rotate_point_around_axis(start_ring[i], ...)`,
    /// using the same function that `revolve_polygon` calls to compute
    /// `end_verts` — guaranteeing byte-identical f64 with arena's stored
    /// top-vertex positions.
    fn from_params(params: &RevolveParams) -> Self {
        let start_ring: Vec<[f64; 3]> = params.lateral_faces.iter().map(|(_, va, _)| *va).collect();

        let end_ring: Vec<[f64; 3]> = if params.full_revolution {
            Vec::new()
        } else {
            start_ring
                .iter()
                .map(|&p| {
                    rotate_point_around_axis(
                        p,
                        params.axis_origin,
                        params.axis_dir,
                        params.angle_rad,
                    )
                })
                .collect()
        };

        RevolvePool {
            start_ring,
            end_ring,
        }
    }

    /// Look up profile index for a position via byte-equality.
    ///
    /// Returns `Some((profile_idx, is_end))` if the position byte-matches a
    /// pool entry: `is_end=false` for start_ring (θ=0), `is_end=true` for
    /// end_ring (θ=angle_rad). Returns `None` if no match — typically means
    /// the position is interior to a lateral face and not on the cap boundary.
    ///
    /// Byte equality (via `f64::to_bits`) is required for bijectivity: the
    /// rendermesh oracle hashes vertex positions through f32 → f64 promotion,
    /// so any ULP divergence in the f64 source produces non-bijective pairs.
    fn lookup(&self, pos: &[f64; 3]) -> Option<(usize, bool)> {
        let key = [pos[0].to_bits(), pos[1].to_bits(), pos[2].to_bits()];
        for (i, p) in self.start_ring.iter().enumerate() {
            if [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()] == key {
                return Some((i, false));
            }
        }
        for (i, p) in self.end_ring.iter().enumerate() {
            if [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()] == key {
                return Some((i, true));
            }
        }
        None
    }
}

/// Tessellate one lateral face of a revolve solid (cylindrical or planar annular).
///
/// For partial revolves: generates a grid of (N+1) x 2 vertices, producing 2N triangles.
/// For full revolution: generates N x 2 vertices and wraps the last ring back to the first.
///
/// Yang 2025 §4.1.1 bijective tessellation: ring 0 (θ=0) and ring N (θ=angle_rad)
/// boundary positions are sourced from `pool` rather than recomputed inline. This
/// guarantees byte-identical f64 with the cap face's arena-stored vertices, so
/// f32 conversion produces identical vertex positions in the rendermesh — a
/// prerequisite for the Yang reciprocity contract.
///
/// The lateral's outer loop is walked via arena half-edges to determine which
/// profile-edge endpoints `(profile_idx_a, profile_idx_b)` and traversal
/// direction the lateral assigns to its start-cap edge. The natural quad
/// triangulation `(v00, v01, v10), (v10, v01, v11)` then emits θ=0 boundary
/// directed edges `v00 → v01` matching the half-edge loop walk — automatically
/// reciprocal to the start-cap face's twin half-edge walk on the same B-Rep
/// edge. Same applies to the end cap at ring N.
#[allow(clippy::too_many_arguments)]
fn tessellate_revolve_lateral(
    arena: &TopoArena,
    face_idx: FaceIdx,
    pool: &RevolvePool,
    axis_origin: &[f64; 3],
    axis_dir: &[f64; 3],
    angle_rad: f64,
    full_revolution: bool,
    _geom: Option<&SurfaceGeom>,
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    kid: u64,
    lateral_idx: usize,
) {
    let revolve_debug = std::env::var("REVOLVE_DEBUG").as_deref() == Ok("1");

    let n = circle_segments();
    let base_vertex = vertices.len() as u32 / 3;

    // Walk the lateral face's outer loop to determine the (profile_a,
    // profile_b) profile-vertex indices and the half-edge traversal
    // direction at θ=0. The lateral face has 4 vertices: bottom_a/bottom_b
    // at θ=0 and top_a/top_b at θ=angle_rad (or wraps to bottom for full
    // revolution). The half-edge twin convention guarantees that the
    // lateral's loop walks the start-cap edge OPPOSITE the start-cap face's
    // walk on the same edge — so emitting ring-0 boundary vertices in the
    // order the lateral walks them produces directed mesh edges that are
    // automatic twins of the cap's emission. Same for ring N at θ=angle_rad
    // for partial revolves.
    //
    // Mirrors `collect_loop_boundary` (mod.rs:2748+) used by
    // `tessellate_solid_bounded`. Per Yang §4.1.1 bijective tessellation:
    // shared-pool positions + arena-loop walk = automatic reciprocity.
    let (profile_a, profile_b) = {
        let outer_loop = arena.faces[face_idx.0].outer_loop;
        let start_he = arena.loops[outer_loop.0].half_edge;
        // Walk the loop, find a start-cap edge: an edge whose two endpoints
        // are both in `pool.start_ring`. The lateral has exactly one such
        // edge (the bottom horizontal edge at θ=0).
        let mut found = None;
        let mut he = start_he;
        loop {
            let v_origin = arena.half_edges[he.0].origin;
            let next_he = arena.half_edges[he.0].next;
            let v_dest = arena.half_edges[next_he.0].origin;
            let p_origin = arena.vertices[v_origin.0].position;
            let p_dest = arena.vertices[v_dest.0].position;
            if let (Some((idx_a, false)), Some((idx_b, false))) =
                (pool.lookup(&p_origin), pool.lookup(&p_dest))
            {
                found = Some((idx_a, idx_b));
                break;
            }
            he = next_he;
            if he == start_he {
                break;
            }
        }
        // Fallback: if no start-cap edge identified (full revolution has no
        // distinct cap edges), fall back to lateral_idx-based profile pairing
        // matching `revolve_polygon`'s `(start_verts[i], start_verts[(i+1)%n])`.
        found.unwrap_or_else(|| {
            let p_count = pool.start_ring.len();
            (lateral_idx, (lateral_idx + 1) % p_count)
        })
    };

    if revolve_debug {
        eprintln!(
            "[revolve-tess] face={} face_kind=lateral parent_edge={} angle_rad={:.6} \
             full_rev={} ring_count={} profile_a={} profile_b={} \
             axis_origin=({:.6},{:.6},{:.6}) axis_dir=({:.6},{:.6},{:.6}) base_vertex={}",
            kid,
            lateral_idx,
            angle_rad,
            full_revolution,
            if full_revolution { n } else { n + 1 },
            profile_a,
            profile_b,
            axis_origin[0],
            axis_origin[1],
            axis_origin[2],
            axis_dir[0],
            axis_dir[1],
            axis_dir[2],
            base_vertex,
        );
    }

    // Source positions: at θ=0 use pool.start_ring (byte-identical to
    // arena.vertices[bottom_verts[i]].position). At θ=angle_rad use
    // pool.end_ring (byte-identical to arena.vertices[top_verts[i]].position
    // via the same `rotate_point_around_axis` formula).
    //
    // The lateral's column 0 corresponds to profile_a (origin of the
    // half-edge walk's start-cap edge), column 1 to profile_b (destination).
    // The natural quad triangulation `(v00, v01, v10), (v10, v01, v11)`
    // emits ring-0 boundary `v00 → v01` = `start_ring[a] → start_ring[b]`,
    // which matches the lateral's half-edge walk and is automatically the
    // twin of the start-cap face's walk on the same B-Rep edge.
    let p_start_a = pool.start_ring[profile_a];
    let p_start_b = pool.start_ring[profile_b];

    // For full revolution, generate N rings (last wraps to first).
    // For partial, generate N+1 rings (start and end are distinct).
    let ring_count = if full_revolution { n } else { n + 1 };

    // Generate ring_count x 2 vertex grid
    for i in 0..ring_count {
        // Source positions for this ring's column 0 and column 1.
        let mut rotated_pair: [[f64; 3]; 2];
        if i == 0 {
            // Ring 0: pull from pool's start_ring (Yang §4.1.1 bijective
            // boundary contract — must byte-match cap face's emission).
            rotated_pair = [p_start_a, p_start_b];
        } else if !full_revolution && i == n {
            // Ring N (partial revolve only): pull from pool's end_ring.
            rotated_pair = [pool.end_ring[profile_a], pool.end_ring[profile_b]];
        } else {
            // Interior ring: Rodrigues rotation. These positions are not on
            // any cap boundary, so byte-equality with the cap is irrelevant;
            // only consistency across adjacent laterals (which share the
            // ring's positions for the seam edge) matters.
            let theta = angle_rad * (i as f64) / (n as f64);
            let cos_t = theta.cos();
            let sin_t = theta.sin();
            rotated_pair = [[0.0_f64; 3]; 2];
            for (si, sv) in [&p_start_a, &p_start_b].iter().enumerate() {
                let v = v3_sub(**sv, *axis_origin);
                let k_dot_v = v3_dot(*axis_dir, v);
                let k_cross_v = v3_cross(*axis_dir, v);
                rotated_pair[si] = [
                    axis_origin[0]
                        + v[0] * cos_t
                        + k_cross_v[0] * sin_t
                        + axis_dir[0] * k_dot_v * (1.0 - cos_t),
                    axis_origin[1]
                        + v[1] * cos_t
                        + k_cross_v[1] * sin_t
                        + axis_dir[1] * k_dot_v * (1.0 - cos_t),
                    axis_origin[2]
                        + v[2] * cos_t
                        + k_cross_v[2] * sin_t
                        + axis_dir[2] * k_dot_v * (1.0 - cos_t),
                ];
            }
        }

        // Profile tangent direction at this ring (col 0 → col 1)
        let profile_dir = v3_normalize(v3_sub(rotated_pair[1], rotated_pair[0]));

        // Emit position + analytic revolve normal for each vertex
        for rotated in &rotated_pair {
            vertices.push(rotated[0] as f32);
            vertices.push(rotated[1] as f32);
            vertices.push(rotated[2] as f32);

            // Analytic surface-of-revolution normal:
            //   radial = vertex - projection_onto_axis
            //   circ_tangent = normalize(axis × radial)
            //   normal = normalize(profile_tangent × circ_tangent)
            let v_rel = v3_sub(*rotated, *axis_origin);
            let along_axis = v3_dot(v_rel, *axis_dir);
            let axis_point = v3_add(*axis_origin, v3_scale(*axis_dir, along_axis));
            let radial = v3_sub(*rotated, axis_point);
            let radial_len = v3_length(radial);

            if radial_len > TAU_NORMALIZE {
                let circ_tangent = v3_normalize(v3_cross(*axis_dir, radial));
                let normal = v3_normalize(v3_cross(profile_dir, circ_tangent));
                normals.push(normal[0] as f32);
                normals.push(normal[1] as f32);
                normals.push(normal[2] as f32);
            } else {
                // Vertex on axis — degenerate, use axis direction
                normals.push(axis_dir[0] as f32);
                normals.push(axis_dir[1] as f32);
                normals.push(axis_dir[2] as f32);
            }
        }
    }

    // Generate quads
    for i in 0..n as u32 {
        let next = if full_revolution {
            (i + 1) % (n as u32)
        } else {
            i + 1
        };
        let v00 = base_vertex + i * 2;
        let v01 = base_vertex + i * 2 + 1;
        let v10 = base_vertex + next * 2;
        let v11 = base_vertex + next * 2 + 1;

        indices.push(v00);
        indices.push(v01);
        indices.push(v10);

        indices.push(v10);
        indices.push(v01);
        indices.push(v11);

        if revolve_debug {
            let read_v = |idx: u32| -> [f64; 3] {
                let i = idx as usize * 3;
                [
                    vertices[i] as f64,
                    vertices[i + 1] as f64,
                    vertices[i + 2] as f64,
                ]
            };
            let p00 = read_v(v00);
            let p01 = read_v(v01);
            let p10 = read_v(v10);
            let p11 = read_v(v11);
            eprintln!(
                "[revolve-tess] face={} face_kind=lateral parent_edge={} ring_idx={} tri=A \
                 verts=[{},{},{}] coords=[({:.6},{:.6},{:.6}),({:.6},{:.6},{:.6}),({:.6},{:.6},{:.6})]",
                kid, lateral_idx, i, v00, v01, v10,
                p00[0], p00[1], p00[2],
                p01[0], p01[1], p01[2],
                p10[0], p10[1], p10[2],
            );
            eprintln!(
                "[revolve-tess] face={} face_kind=lateral parent_edge={} ring_idx={} tri=B \
                 verts=[{},{},{}] coords=[({:.6},{:.6},{:.6}),({:.6},{:.6},{:.6}),({:.6},{:.6},{:.6})]",
                kid, lateral_idx, i, v10, v01, v11,
                p10[0], p10[1], p10[2],
                p01[0], p01[1], p01[2],
                p11[0], p11[1], p11[2],
            );
        }
    }

    // Post-fix: check first triangle's geometric normal against stored normal.
    // If they disagree, flip stored normals (not winding) to match geometry.
    {
        let first_tri_start = indices.len() - (n * 2) * 3;
        let i0 = indices[first_tri_start] as usize;
        let i1 = indices[first_tri_start + 1] as usize;
        let i2 = indices[first_tri_start + 2] as usize;
        let p0 = [
            vertices[i0 * 3] as f64,
            vertices[i0 * 3 + 1] as f64,
            vertices[i0 * 3 + 2] as f64,
        ];
        let p1 = [
            vertices[i1 * 3] as f64,
            vertices[i1 * 3 + 1] as f64,
            vertices[i1 * 3 + 2] as f64,
        ];
        let p2 = [
            vertices[i2 * 3] as f64,
            vertices[i2 * 3 + 1] as f64,
            vertices[i2 * 3 + 2] as f64,
        ];
        let e1 = v3_sub(p1, p0);
        let e2 = v3_sub(p2, p0);
        let geo_normal = v3_cross(e1, e2);
        let stored = [
            normals[i0 * 3] as f64,
            normals[i0 * 3 + 1] as f64,
            normals[i0 * 3 + 2] as f64,
        ];
        if v3_dot(geo_normal, stored) < 0.0 {
            // Flip all stored normals for this face
            let n_verts = ring_count * 2;
            let normals_start = base_vertex as usize * 3;
            for j in 0..n_verts {
                normals[normals_start + j * 3] = -normals[normals_start + j * 3];
                normals[normals_start + j * 3 + 1] = -normals[normals_start + j * 3 + 1];
                normals[normals_start + j * 3 + 2] = -normals[normals_start + j * 3 + 2];
            }
        }
    }
}

/// Tessellate a polygon face with known planar geometry (fan triangulation).
#[allow(clippy::too_many_arguments)]
fn tessellate_polygon_face(
    arena: &TopoArena,
    face_idx: FaceIdx,
    plane: &crate::geometry::surface::Plane,
    kid: u64,
    vertices: &mut Vec<f32>,
    normals_out: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    // PR14 instrumentation: REVOLVE_DEBUG=1 emits per-triangle traces for
    // planar polygon tessellation. Used to diagnose cap-fan / cap-lateral
    // overlap hypotheses (a, e) per the PR14 plan.
    let revolve_debug = std::env::var("REVOLVE_DEBUG").as_deref() == Ok("1");

    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;

    let mut loop_verts: Vec<[f64; 3]> = Vec::new();
    let mut he = start_he;
    loop {
        let v = arena.half_edges[he.0].origin;
        loop_verts.push(arena.vertices[v.0].position);
        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }

    if loop_verts.len() < 3 {
        return;
    }

    if revolve_debug {
        eprintln!(
            "[revolve-tess] face={} face_kind=cap loop_n={} plane_origin=({:.6},{:.6},{:.6}) \
             plane_normal=({:.6},{:.6},{:.6})",
            kid,
            loop_verts.len(),
            plane.origin.x,
            plane.origin.y,
            plane.origin.z,
            plane.normal.x,
            plane.normal.y,
            plane.normal.z,
        );
        for (i, v) in loop_verts.iter().enumerate() {
            eprintln!(
                "[revolve-tess] face={} face_kind=cap loop_vert[{}]=({:.6},{:.6},{:.6})",
                kid, i, v[0], v[1], v[2],
            );
        }
    }

    let normal = [
        plane.normal.x as f32,
        plane.normal.y as f32,
        plane.normal.z as f32,
    ];

    // Check if loop winding matches stored normal using Newell method.
    // If it disagrees, reverse loop vertices BEFORE emitting to the buffer
    // and tessellating. This ensures ALL triangles have consistent winding.
    let n = loop_verts.len();
    let stored_normal = [plane.normal.x, plane.normal.y, plane.normal.z];
    let mut newell = [0.0f64; 3];
    for i in 0..n {
        let curr = loop_verts[i];
        let next = loop_verts[(i + 1) % n];
        newell[0] += (curr[1] - next[1]) * (curr[2] + next[2]);
        newell[1] += (curr[2] - next[2]) * (curr[0] + next[0]);
        newell[2] += (curr[0] - next[0]) * (curr[1] + next[1]);
    }
    let dot = v3_dot(newell, stored_normal);
    if dot < 0.0 {
        // Winding disagrees with stored normal — reverse loop vertices
        loop_verts.reverse();
    }

    let base_vertex = vertices.len() as u32 / 3;
    let start_index = indices.len() as u32;

    for v in &loop_verts {
        vertices.push(v[0] as f32);
        vertices.push(v[1] as f32);
        vertices.push(v[2] as f32);
        normals_out.push(normal[0]);
        normals_out.push(normal[1]);
        normals_out.push(normal[2]);
    }

    // Convexity check: for each consecutive triple of edges, verify the cross
    // product agrees in sign with the stored face normal. If all agree → convex.
    let is_convex = {
        let mut convex = true;
        for i in 0..n {
            let a = loop_verts[i];
            let b = loop_verts[(i + 1) % n];
            let c = loop_verts[(i + 2) % n];
            let ab = v3_sub(b, a);
            let bc = v3_sub(c, b);
            let cross = v3_cross(ab, bc);
            if v3_dot(cross, stored_normal) < 0.0 {
                convex = false;
                break;
            }
        }
        convex
    };

    if is_convex {
        // Fast path: fan triangulation for convex polygons.
        // Find a fan center that avoids degenerate (collinear) triangles.
        let fan_center = (0..n).find(|&j| {
            // Check all fan triangles: (j, j+1, j+2), (j, j+2, j+3), ... wrapping
            (1..n - 1).all(|i| {
                let a = (j + i) % n;
                let b = (j + i + 1) % n;
                let e1 = v3_sub(loop_verts[a], loop_verts[j]);
                let e2 = v3_sub(loop_verts[b], loop_verts[j]);
                let cr = v3_cross(e1, e2);
                v3_dot(cr, cr) > TAU_TESS_GRID_MIN * TAU_TESS_GRID_MIN
            })
        });
        if let Some(fc) = fan_center {
            if revolve_debug {
                eprintln!(
                    "[revolve-tess] face={} face_kind=cap strategy=fan fan_center={}",
                    kid, fc,
                );
            }
            for i in 1..n - 1 {
                let a = (fc + i) % n;
                let b = (fc + i + 1) % n;
                indices.push(base_vertex + fc as u32);
                indices.push(base_vertex + a as u32);
                indices.push(base_vertex + b as u32);
            }
        } else {
            if revolve_debug {
                eprintln!(
                    "[revolve-tess] face={} face_kind=cap strategy=earcut_convex_fallback",
                    kid,
                );
            }
            // All fan centers produce degenerate triangles; fall back to ear-clip
            let (u_axis, v_axis) = compute_plane_basis(stored_normal);
            let coords_2d: Vec<f64> = loop_verts
                .iter()
                .flat_map(|v| {
                    let d = v3_sub(*v, loop_verts[0]);
                    vec![v3_dot(d, u_axis), v3_dot(d, v_axis)]
                })
                .collect();
            let tri_indices = cdt::cdt_triangulate_flat(&coords_2d, &[])
                .expect("CDT failed on convex polygon");
            for chunk in tri_indices.chunks(3) {
                indices.push(base_vertex + chunk[0] as u32);
                indices.push(base_vertex + chunk[1] as u32);
                indices.push(base_vertex + chunk[2] as u32);
            }
        }
    } else {
        if revolve_debug {
            eprintln!(
                "[revolve-tess] face={} face_kind=cap strategy=cdt_nonconvex",
                kid,
            );
        }
        // Non-convex path: CDT via spade
        // Project onto 2D using stored face normal as the projection axis
        let (u_axis, v_axis) = compute_plane_basis(stored_normal);

        let coords_2d: Vec<f64> = loop_verts
            .iter()
            .flat_map(|v| {
                let d = v3_sub(*v, loop_verts[0]);
                vec![v3_dot(d, u_axis), v3_dot(d, v_axis)]
            })
            .collect();

        let tri_indices = cdt::cdt_triangulate_flat(&coords_2d, &[])
            .expect("CDT failed on polygon face");

        for chunk in tri_indices.chunks(3) {
            indices.push(base_vertex + chunk[0] as u32);
            indices.push(base_vertex + chunk[1] as u32);
            indices.push(base_vertex + chunk[2] as u32);
        }
    }

    // Per-triangle winding correction pass using f64 precision from loop_verts.
    // Catches any remaining mismatches after the global winding correction.
    // For degenerate triangles (near-zero cross product), use the bulk winding decision
    // from the first non-degenerate triangle to avoid noise-driven flips.
    let tri_start = start_index as usize;
    let tri_end = indices.len();

    // Determine bulk winding from first non-degenerate triangle
    let mut bulk_flip = false;
    for t in (tri_start..tri_end).step_by(3) {
        let li0 = (indices[t] - base_vertex) as usize;
        let li1 = (indices[t + 1] - base_vertex) as usize;
        let li2 = (indices[t + 2] - base_vertex) as usize;
        let v0 = loop_verts[li0];
        let v1 = loop_verts[li1];
        let v2 = loop_verts[li2];
        let e1 = v3_sub(v1, v0);
        let e2 = v3_sub(v2, v0);
        let tri_normal = v3_cross(e1, e2);
        if v3_length(tri_normal) > TAU_WORK {
            bulk_flip = v3_dot(tri_normal, stored_normal) < 0.0;
            break;
        }
    }

    for t in (tri_start..tri_end).step_by(3) {
        let li0 = (indices[t] - base_vertex) as usize;
        let li1 = (indices[t + 1] - base_vertex) as usize;
        let li2 = (indices[t + 2] - base_vertex) as usize;
        let v0 = loop_verts[li0];
        let v1 = loop_verts[li1];
        let v2 = loop_verts[li2];
        let e1 = v3_sub(v1, v0);
        let e2 = v3_sub(v2, v0);
        let tri_normal = v3_cross(e1, e2);
        let should_flip = if v3_length(tri_normal) > TAU_WORK {
            v3_dot(tri_normal, stored_normal) < 0.0
        } else {
            bulk_flip
        };
        if should_flip {
            indices.swap(t + 1, t + 2);
        }
    }

    if revolve_debug {
        let read_v = |idx: u32| -> [f64; 3] {
            let i = idx as usize * 3;
            [
                vertices[i] as f64,
                vertices[i + 1] as f64,
                vertices[i + 2] as f64,
            ]
        };
        for (tri_idx, t) in (tri_start..tri_end).step_by(3).enumerate() {
            let i0 = indices[t];
            let i1 = indices[t + 1];
            let i2 = indices[t + 2];
            let p0 = read_v(i0);
            let p1 = read_v(i1);
            let p2 = read_v(i2);
            eprintln!(
                "[revolve-tess] face={} face_kind=cap tri_idx={} verts=[{},{},{}] \
                 coords=[({:.6},{:.6},{:.6}),({:.6},{:.6},{:.6}),({:.6},{:.6},{:.6})]",
                kid,
                tri_idx,
                i0,
                i1,
                i2,
                p0[0],
                p0[1],
                p0[2],
                p1[0],
                p1[1],
                p1[2],
                p2[0],
                p2[1],
                p2[2],
            );
        }
    }

    let end_index = indices.len() as u32;
    face_ranges.push(FaceRange {
        face_id: KernelId(kid),
        start_index,
        end_index,
    });
}

/// Tessellate a revolve cap polygon face (start cap or end cap of a partial revolve).
///
/// Mirrors `tessellate_polygon_face` but DROPS the pre-emit Newell-vs-stored-normal
/// loop reversal. The reversal in `tessellate_polygon_face` exists to align cap
/// triangle winding with the stored normal, but for revolve caps it breaks the
/// Yang §4.1.1 reciprocity contract with the lateral face: the lateral now walks
/// arena natural order (per `tessellate_revolve_lateral`), and any tessellator-level
/// flip on the cap desynchronizes the cap's emitted directed edge from the lateral's
/// twin half-edge direction.
///
/// Instead, this function walks the cap's arena outer-loop in its natural cyclic
/// order and emits triangles whose winding follows that walk. If the resulting
/// geometric normal disagrees with the stored normal, the STORED NORMALS are
/// flipped (not the winding) — the same post-fix pattern `tessellate_revolve_lateral`
/// uses. This keeps stored normals visually correct while preserving arena-order
/// emission for bijectivity.
///
/// Three.js renders all CAD meshes with `THREE.DoubleSide` (no backface culling),
/// so the absolute winding direction is irrelevant for rendering — only the
/// stored-normal alignment matters for shading. The bijectivity oracle, in
/// contrast, is sensitive to winding direction at face boundaries (it counts
/// directed mesh edges).
///
/// Audit D-10 (Cluster I, blocked-by-tessellation) in
/// `docs/audits/cherchi_port_audit.md`. Yang §4.1.1 bijective tessellation.
#[allow(clippy::too_many_arguments)]
fn tessellate_revolve_cap_polygon(
    arena: &TopoArena,
    face_idx: FaceIdx,
    plane: &crate::geometry::surface::Plane,
    kid: u64,
    vertices: &mut Vec<f32>,
    normals_out: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;

    let loop_verts: Vec<[f64; 3]> = {
        let mut v = Vec::new();
        let mut he = start_he;
        loop {
            let vi = arena.half_edges[he.0].origin;
            v.push(arena.vertices[vi.0].position);
            he = arena.half_edges[he.0].next;
            if he == start_he {
                break;
            }
        }
        v
    };

    if loop_verts.len() < 3 {
        return;
    }

    let normal = [
        plane.normal.x as f32,
        plane.normal.y as f32,
        plane.normal.z as f32,
    ];
    let stored_normal = [plane.normal.x, plane.normal.y, plane.normal.z];

    // Emit f64 → f32 vertices in arena-natural order. Each loop_verts[i] is
    // byte-identical to either pool.start_ring[profile_i] (start cap) or
    // pool.end_ring[profile_i] (end cap), so the emitted f32 positions match
    // the lateral's ring-0 / ring-N emission.
    let n = loop_verts.len();
    let base_vertex = vertices.len() as u32 / 3;
    let start_index = indices.len() as u32;

    for v in &loop_verts {
        vertices.push(v[0] as f32);
        vertices.push(v[1] as f32);
        vertices.push(v[2] as f32);
        normals_out.push(normal[0]);
        normals_out.push(normal[1]);
        normals_out.push(normal[2]);
    }

    // Convexity check: cross product of every consecutive triple must agree
    // in sign with stored_normal (interpreted in the loop's actual orientation).
    // Take an absolute-value approach: convex iff all cross products have the
    // SAME sign relative to stored_normal (regardless of which sign).
    let cross_signs: Vec<f64> = (0..n)
        .map(|i| {
            let a = loop_verts[i];
            let b = loop_verts[(i + 1) % n];
            let c = loop_verts[(i + 2) % n];
            let ab = v3_sub(b, a);
            let bc = v3_sub(c, b);
            let cross = v3_cross(ab, bc);
            v3_dot(cross, stored_normal)
        })
        .collect();
    let nonzero_signs: Vec<f64> = cross_signs
        .iter()
        .copied()
        .filter(|s| s.abs() > TAU_WORK)
        .collect();
    let is_convex = if nonzero_signs.is_empty() {
        true
    } else {
        let first_sign = nonzero_signs[0].signum();
        nonzero_signs.iter().all(|s| s.signum() == first_sign)
    };

    // Compute the input polygon's signed area in the (u, v) basis. Earcut
    // ALWAYS produces CCW triangles in 2D regardless of input winding (a
    // normalization). If the input arena loop is CW in 2D, earcut's output
    // boundary directed edges run OPPOSITE to the input loop direction —
    // breaking the half-edge twin convention with the lateral face's
    // arena-walked emission. To preserve arena-order boundary directed edges
    // (so cap and lateral reciprocate), we flip each output triangle's
    // winding when the input is CW.
    //
    // Fan triangulation does not have this normalization — it preserves input
    // winding by construction — so the flip is only applied on the earcut paths.
    let (u_axis, v_axis) = compute_plane_basis(stored_normal);
    let signed_area_2d: f64 = (0..n)
        .map(|i| {
            let curr = loop_verts[i];
            let next = loop_verts[(i + 1) % n];
            let dc = v3_sub(curr, loop_verts[0]);
            let dn = v3_sub(next, loop_verts[0]);
            let cu = v3_dot(dc, u_axis);
            let cv = v3_dot(dc, v_axis);
            let nu = v3_dot(dn, u_axis);
            let nv = v3_dot(dn, v_axis);
            cu * nv - nu * cv
        })
        .sum::<f64>()
        * 0.5;
    let input_is_cw_2d = signed_area_2d < 0.0;

    let push_triangle = |indices: &mut Vec<u32>, a: u32, b: u32, c: u32, flip: bool| {
        if flip {
            indices.push(a);
            indices.push(c);
            indices.push(b);
        } else {
            indices.push(a);
            indices.push(b);
            indices.push(c);
        }
    };

    if is_convex {
        // Fan triangulation from a vertex that produces no degenerate fan triangles.
        let fan_center = (0..n).find(|&j| {
            (1..n - 1).all(|i| {
                let a = (j + i) % n;
                let b = (j + i + 1) % n;
                let e1 = v3_sub(loop_verts[a], loop_verts[j]);
                let e2 = v3_sub(loop_verts[b], loop_verts[j]);
                let cr = v3_cross(e1, e2);
                v3_dot(cr, cr) > TAU_TESS_GRID_MIN * TAU_TESS_GRID_MIN
            })
        });
        if let Some(fc) = fan_center {
            // Fan preserves input winding (vertices are emitted in input order
            // along the polygon boundary). No flip needed.
            for i in 1..n - 1 {
                let a = (fc + i) % n;
                let b = (fc + i + 1) % n;
                indices.push(base_vertex + fc as u32);
                indices.push(base_vertex + a as u32);
                indices.push(base_vertex + b as u32);
            }
        } else {
            // All fan centers produce degenerate triangles; fall back to ear-clip.
            let coords_2d: Vec<f64> = loop_verts
                .iter()
                .flat_map(|v| {
                    let d = v3_sub(*v, loop_verts[0]);
                    vec![v3_dot(d, u_axis), v3_dot(d, v_axis)]
                })
                .collect();
            let tri_indices = cdt::cdt_triangulate_flat(&coords_2d, &[])
                .expect("CDT failed on convex revolve cap");
            for chunk in tri_indices.chunks(3) {
                push_triangle(
                    indices,
                    base_vertex + chunk[0] as u32,
                    base_vertex + chunk[1] as u32,
                    base_vertex + chunk[2] as u32,
                    input_is_cw_2d,
                );
            }
        }
    } else {
        // Non-convex path: CDT via spade.
        let coords_2d: Vec<f64> = loop_verts
            .iter()
            .flat_map(|v| {
                let d = v3_sub(*v, loop_verts[0]);
                vec![v3_dot(d, u_axis), v3_dot(d, v_axis)]
            })
            .collect();
        let tri_indices = cdt::cdt_triangulate_flat(&coords_2d, &[])
            .expect("CDT failed on revolve cap polygon");
        for chunk in tri_indices.chunks(3) {
            push_triangle(
                indices,
                base_vertex + chunk[0] as u32,
                base_vertex + chunk[1] as u32,
                base_vertex + chunk[2] as u32,
                input_is_cw_2d,
            );
        }
    }

    // Post-fix: if first triangle's geometric normal disagrees with stored
    // normal, flip stored normals (NOT winding) to match. Mirrors the lateral's
    // post-fix pattern; preserves arena-order winding for bijectivity.
    let tri_count = (indices.len() as u32 - start_index) / 3;
    if tri_count > 0 {
        let i0 = indices[start_index as usize] as usize;
        let i1 = indices[start_index as usize + 1] as usize;
        let i2 = indices[start_index as usize + 2] as usize;
        let p0 = [
            vertices[i0 * 3] as f64,
            vertices[i0 * 3 + 1] as f64,
            vertices[i0 * 3 + 2] as f64,
        ];
        let p1 = [
            vertices[i1 * 3] as f64,
            vertices[i1 * 3 + 1] as f64,
            vertices[i1 * 3 + 2] as f64,
        ];
        let p2 = [
            vertices[i2 * 3] as f64,
            vertices[i2 * 3 + 1] as f64,
            vertices[i2 * 3 + 2] as f64,
        ];
        let e1 = v3_sub(p1, p0);
        let e2 = v3_sub(p2, p0);
        let geo_normal = v3_cross(e1, e2);
        if v3_dot(geo_normal, stored_normal) < 0.0 {
            // Flip all stored normals for this face's emitted vertices.
            let normals_start = base_vertex as usize * 3;
            for j in 0..n {
                normals_out[normals_start + j * 3] = -normals_out[normals_start + j * 3];
                normals_out[normals_start + j * 3 + 1] = -normals_out[normals_start + j * 3 + 1];
                normals_out[normals_start + j * 3 + 2] = -normals_out[normals_start + j * 3 + 2];
            }
        }
    }

    let end_index = indices.len() as u32;
    face_ranges.push(FaceRange {
        face_id: KernelId(kid),
        start_index,
        end_index,
    });
}

/// Tessellate a polygon face without known geometry (fallback with computed normal).
fn tessellate_polygon_face_fallback(
    arena: &TopoArena,
    face_idx: FaceIdx,
    kid: u64,
    vertices: &mut Vec<f32>,
    normals_out: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;

    let mut loop_verts: Vec<[f64; 3]> = Vec::new();
    let mut he = start_he;
    loop {
        let v = arena.half_edges[he.0].origin;
        loop_verts.push(arena.vertices[v.0].position);
        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }

    if loop_verts.len() < 3 {
        return;
    }

    // Compute Newell normal for robust non-convex handling
    let nv = loop_verts.len();
    let mut newell = [0.0f64; 3];
    for i in 0..nv {
        let curr = loop_verts[i];
        let next = loop_verts[(i + 1) % nv];
        newell[0] += (curr[1] - next[1]) * (curr[2] + next[2]);
        newell[1] += (curr[2] - next[2]) * (curr[0] + next[0]);
        newell[2] += (curr[0] - next[0]) * (curr[1] + next[1]);
    }
    let newell_len = v3_dot(newell, newell).sqrt();
    let normal = if newell_len < TAU_NORMALIZE {
        [0.0f32, 0.0, 1.0]
    } else {
        [
            (newell[0] / newell_len) as f32,
            (newell[1] / newell_len) as f32,
            (newell[2] / newell_len) as f32,
        ]
    };

    let base_vertex = vertices.len() as u32 / 3;
    let start_index = indices.len() as u32;

    for v in &loop_verts {
        vertices.push(v[0] as f32);
        vertices.push(v[1] as f32);
        vertices.push(v[2] as f32);
        normals_out.push(normal[0]);
        normals_out.push(normal[1]);
        normals_out.push(normal[2]);
    }

    // Convexity check
    let is_convex = if newell_len < TAU_NORMALIZE {
        true
    } else {
        let nn = [
            newell[0] / newell_len,
            newell[1] / newell_len,
            newell[2] / newell_len,
        ];
        let mut convex = true;
        for i in 0..nv {
            let a = loop_verts[i];
            let b = loop_verts[(i + 1) % nv];
            let c = loop_verts[(i + 2) % nv];
            let ab = v3_sub(b, a);
            let bc = v3_sub(c, b);
            let cross = v3_cross(ab, bc);
            if v3_dot(cross, nn) < 0.0 {
                convex = false;
                break;
            }
        }
        convex
    };

    if is_convex {
        // Find a fan center that avoids degenerate (collinear) triangles.
        let fan_center = (0..nv).find(|&j| {
            (1..nv - 1).all(|i| {
                let a = (j + i) % nv;
                let b = (j + i + 1) % nv;
                let e1 = v3_sub(loop_verts[a], loop_verts[j]);
                let e2 = v3_sub(loop_verts[b], loop_verts[j]);
                let cr = v3_cross(e1, e2);
                v3_dot(cr, cr) > TAU_TESS_GRID_MIN * TAU_TESS_GRID_MIN
            })
        });
        if let Some(fc) = fan_center {
            for i in 1..nv - 1 {
                let a = (fc + i) % nv;
                let b = (fc + i + 1) % nv;
                indices.push(base_vertex + fc as u32);
                indices.push(base_vertex + a as u32);
                indices.push(base_vertex + b as u32);
            }
        } else {
            // Fall back to ear-clip if no good fan center
            let nn = if newell_len < TAU_NORMALIZE {
                [0.0, 0.0, 1.0]
            } else {
                [
                    newell[0] / newell_len,
                    newell[1] / newell_len,
                    newell[2] / newell_len,
                ]
            };
            let (u_axis, v_axis) = compute_plane_basis(nn);
            let coords_2d: Vec<f64> = loop_verts
                .iter()
                .flat_map(|v| {
                    let d = v3_sub(*v, loop_verts[0]);
                    vec![v3_dot(d, u_axis), v3_dot(d, v_axis)]
                })
                .collect();
            let tri_indices = cdt::cdt_triangulate_flat(&coords_2d, &[])
                .expect("CDT failed on polygon face (fallback convex)");
            for chunk in tri_indices.chunks(3) {
                indices.push(base_vertex + chunk[0] as u32);
                indices.push(base_vertex + chunk[1] as u32);
                indices.push(base_vertex + chunk[2] as u32);
            }
        }
    } else {
        let nn = if newell_len < TAU_NORMALIZE {
            [0.0, 0.0, 1.0]
        } else {
            [
                newell[0] / newell_len,
                newell[1] / newell_len,
                newell[2] / newell_len,
            ]
        };
        let (u_axis, v_axis) = compute_plane_basis(nn);

        let coords_2d: Vec<f64> = loop_verts
            .iter()
            .flat_map(|v| {
                let d = v3_sub(*v, loop_verts[0]);
                vec![v3_dot(d, u_axis), v3_dot(d, v_axis)]
            })
            .collect();

        let tri_indices = cdt::cdt_triangulate_flat(&coords_2d, &[])
            .expect("CDT failed on polygon face (fallback)");

        for chunk in tri_indices.chunks(3) {
            indices.push(base_vertex + chunk[0] as u32);
            indices.push(base_vertex + chunk[1] as u32);
            indices.push(base_vertex + chunk[2] as u32);
        }
    }

    // Per-triangle winding correction using f64 loop_verts for precision.
    // For degenerate triangles, fall back to bulk winding decision.
    let face_n = [normal[0] as f64, normal[1] as f64, normal[2] as f64];
    let tri_start = start_index as usize;
    let tri_end = indices.len();

    let mut bulk_flip = false;
    for t in (tri_start..tri_end).step_by(3) {
        let li0 = (indices[t] - base_vertex) as usize;
        let li1 = (indices[t + 1] - base_vertex) as usize;
        let li2 = (indices[t + 2] - base_vertex) as usize;
        let e1 = v3_sub(loop_verts[li1], loop_verts[li0]);
        let e2 = v3_sub(loop_verts[li2], loop_verts[li0]);
        let tri_normal = v3_cross(e1, e2);
        if v3_length(tri_normal) > TAU_WORK {
            bulk_flip = v3_dot(tri_normal, face_n) < 0.0;
            break;
        }
    }

    for t in (tri_start..tri_end).step_by(3) {
        let li0 = (indices[t] - base_vertex) as usize;
        let li1 = (indices[t + 1] - base_vertex) as usize;
        let li2 = (indices[t + 2] - base_vertex) as usize;
        let e1 = v3_sub(loop_verts[li1], loop_verts[li0]);
        let e2 = v3_sub(loop_verts[li2], loop_verts[li0]);
        let tri_normal = v3_cross(e1, e2);
        let should_flip = if v3_length(tri_normal) > TAU_WORK {
            v3_dot(tri_normal, face_n) < 0.0
        } else {
            bulk_flip
        };
        if should_flip {
            indices.swap(t + 1, t + 2);
        }
    }

    let end_index = indices.len() as u32;
    face_ranges.push(FaceRange {
        face_id: KernelId(kid),
        start_index,
        end_index,
    });
}

/// Check if an edge lies between two faces that share the same cylindrical surface,
/// meaning it's an internal seam that should not be rendered.
fn is_smooth_edge(
    arena: &TopoArena,
    edge_idx: EdgeIdx,
    face_geometry: &BTreeMap<FaceIdx, SurfaceGeom>,
) -> bool {
    let he_a = arena.edges[edge_idx.0].half_edge;
    // PR-Y20-MODE-A: NMM (twin=None) — no opposing face; cannot be a
    // smooth seam between two co-cylindrical faces. Treat as not smooth.
    let he_b = match arena.half_edges[he_a.0].twin {
        Some(t) => t,
        None => return false,
    };
    let face_a = arena.loops[arena.half_edges[he_a.0].loop_.0].face;
    let face_b = arena.loops[arena.half_edges[he_b.0].loop_.0].face;

    match (face_geometry.get(&face_a), face_geometry.get(&face_b)) {
        (Some(SurfaceGeom::Planar(pa)), Some(SurfaceGeom::Planar(pb))) => {
            // Co-planar: same normal direction and same plane distance from origin
            let normal_match = v3_dot(
                [pa.normal.x, pa.normal.y, pa.normal.z],
                [pb.normal.x, pb.normal.y, pb.normal.z],
            )
            .abs()
                > 1.0 - TAU_MODEL;
            let diff = [
                pb.origin.x - pa.origin.x,
                pb.origin.y - pa.origin.y,
                pb.origin.z - pa.origin.z,
            ];
            let distance_match =
                v3_dot(diff, [pa.normal.x, pa.normal.y, pa.normal.z]).abs() < TAU_MODEL;
            normal_match && distance_match
        }
        (Some(SurfaceGeom::Cylindrical(ca)), Some(SurfaceGeom::Cylindrical(cb))) => {
            // Same cylinder: same origin, axis, and radius (within tolerance)
            let origin_match = ca.origin.distance_to(cb.origin) < TAU_MODEL;
            let axis_match = v3_dot(ca.axis.to_array(), cb.axis.to_array()).abs() > 1.0 - TAU_MODEL;
            let radius_match = (ca.radius - cb.radius).abs() < TAU_MODEL;
            origin_match && axis_match && radius_match
        }
        (Some(SurfaceGeom::Conical(ca)), Some(SurfaceGeom::Conical(cb))) => {
            // Co-conical: same apex, axis, and half-angle
            let apex_match = ca.apex.distance_to(cb.apex) < TAU_MODEL;
            let axis_match = v3_dot(ca.axis.to_array(), cb.axis.to_array()).abs() > 1.0 - TAU_MODEL;
            let angle_match = (ca.half_angle - cb.half_angle).abs() < TAU_MODEL;
            apex_match && axis_match && angle_match
        }
        (Some(SurfaceGeom::Spherical(sa)), Some(SurfaceGeom::Spherical(sb))) => {
            // Co-spherical: same center and radius
            let center_match = sa.center.distance_to(sb.center) < TAU_MODEL;
            let radius_match = (sa.radius - sb.radius).abs() < TAU_MODEL;
            center_match && radius_match
        }
        (Some(SurfaceGeom::Toroidal(ta)), Some(SurfaceGeom::Toroidal(tb))) => {
            // Co-toroidal: same center, axis, major and minor radii
            let center_match = ta.center.distance_to(tb.center) < TAU_MODEL;
            let axis_match = v3_dot(ta.axis.to_array(), tb.axis.to_array()).abs() > 1.0 - TAU_MODEL;
            let major_match = (ta.major_radius - tb.major_radius).abs() < TAU_MODEL;
            let minor_match = (ta.minor_radius - tb.minor_radius).abs() < TAU_MODEL;
            center_match && axis_match && major_match && minor_match
        }
        _ => false,
    }
}

/// Extract edge line segments for rendering edge overlays.
/// Supports both linear (2-point) and circular (polyline) edges.
/// Edges between co-cylindrical faces (smooth seams) are suppressed.
pub(crate) fn extract_edges(
    arena: &TopoArena,
    edge_map: &BTreeMap<u64, EdgeIdx>,
    edge_geometry: &BTreeMap<EdgeIdx, CurveGeom>,
    face_geometry: &BTreeMap<FaceIdx, SurfaceGeom>,
) -> Result<EdgeRenderData, KernelError> {
    let mut vertices: Vec<f32> = Vec::new();
    let mut edge_ranges: Vec<EdgeRange> = Vec::new();

    for (&kid, &edge_idx) in edge_map {
        // Skip smooth edges (internal seams between co-cylindrical faces)
        if is_smooth_edge(arena, edge_idx, face_geometry) {
            continue;
        }
        let start_vertex = vertices.len() as u32 / 3;

        match edge_geometry.get(&edge_idx) {
            Some(CurveGeom::Arc(arc)) => {
                // Partial arc edge: generate polyline spanning sweep_angle
                let normal = [arc.normal.x, arc.normal.y, arc.normal.z];
                let center = [arc.center.x, arc.center.y, arc.center.z];
                let start_pt = [arc.start_point.x, arc.start_point.y, arc.start_point.z];
                // Derive x_axis from start point relative to center
                let radial = v3_sub(start_pt, center);
                let len =
                    (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
                let x_axis = [radial[0] / len, radial[1] / len, radial[2] / len];
                let y_axis = v3_cross(normal, x_axis);

                // Number of segments proportional to sweep angle
                let n_segs = ((circle_segments() as f64) * arc.sweep_angle / std::f64::consts::TAU)
                    .ceil()
                    .max(4.0) as usize;

                for i in 0..=n_segs {
                    let theta = arc.sweep_angle * (i as f64) / (n_segs as f64);
                    let cos_t = theta.cos();
                    let sin_t = theta.sin();
                    let px = center[0] + arc.radius * (cos_t * x_axis[0] + sin_t * y_axis[0]);
                    let py = center[1] + arc.radius * (cos_t * x_axis[1] + sin_t * y_axis[1]);
                    let pz = center[2] + arc.radius * (cos_t * x_axis[2] + sin_t * y_axis[2]);
                    vertices.push(px as f32);
                    vertices.push(py as f32);
                    vertices.push(pz as f32);
                }

                let end_vertex = vertices.len() as u32 / 3;
                edge_ranges.push(EdgeRange {
                    edge_id: KernelId(kid),
                    start_vertex,
                    end_vertex,
                });
            }
            Some(CurveGeom::Circular(circle)) => {
                // Circular edge: generate N+1 point polyline
                let n = circle_segments();
                // Derive x_axis and y_axis from circle normal
                let normal = [circle.normal.x, circle.normal.y, circle.normal.z];
                let (cx_axis, cy_axis) = make_circle_axes(&normal);

                for i in 0..=n {
                    let theta = std::f64::consts::TAU * (i as f64) / (n as f64);
                    let cos_t = theta.cos();
                    let sin_t = theta.sin();
                    let px =
                        circle.center.x + circle.radius * (cos_t * cx_axis[0] + sin_t * cy_axis[0]);
                    let py =
                        circle.center.y + circle.radius * (cos_t * cx_axis[1] + sin_t * cy_axis[1]);
                    let pz =
                        circle.center.z + circle.radius * (cos_t * cx_axis[2] + sin_t * cy_axis[2]);
                    vertices.push(px as f32);
                    vertices.push(py as f32);
                    vertices.push(pz as f32);
                }

                let end_vertex = vertices.len() as u32 / 3;
                edge_ranges.push(EdgeRange {
                    edge_id: KernelId(kid),
                    start_vertex,
                    end_vertex,
                });
            }
            _ => {
                // Linear edge (default): 2-point segment
                let he_a = arena.edges[edge_idx.0].half_edge;
                // PR-Y20-MODE-A: NMM (twin=None) — derive p1 from
                // he_a.next.origin (the destination vertex of he_a).
                let p0 = arena.vertices[arena.half_edges[he_a.0].origin.0].position;
                let p1 = match arena.half_edges[he_a.0].twin {
                    Some(t) => arena.vertices[arena.half_edges[t.0].origin.0].position,
                    None => {
                        let next_he = arena.half_edges[he_a.0].next;
                        arena.vertices[arena.half_edges[next_he.0].origin.0].position
                    }
                };

                vertices.push(p0[0] as f32);
                vertices.push(p0[1] as f32);
                vertices.push(p0[2] as f32);
                vertices.push(p1[0] as f32);
                vertices.push(p1[1] as f32);
                vertices.push(p1[2] as f32);

                edge_ranges.push(EdgeRange {
                    edge_id: KernelId(kid),
                    start_vertex,
                    end_vertex: start_vertex + 2,
                });
            }
        }
    }

    Ok(EdgeRenderData {
        vertices,
        edge_ranges,
    })
}

// ── Cylindrical patch tessellation ───────────────────────────────────────

/// Tessellate a cylindrical face from a boolean result (no CylinderParams available).
/// Derives angular range from boundary arc edges and height from linear edges.
#[allow(clippy::too_many_arguments)]
fn tessellate_cylindrical_patch(
    arena: &TopoArena,
    face_idx: FaceIdx,
    cyl: &crate::geometry::surface::Cylinder,
    edge_geometry: &std::collections::BTreeMap<EdgeIdx, CurveGeom>,
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    let origin = [cyl.origin.x, cyl.origin.y, cyl.origin.z];
    // Negative radius signals inward-facing normals (hole surface)
    let inward = cyl.radius < 0.0;
    let r = cyl.radius.abs();
    let axis = [cyl.axis.x, cyl.axis.y, cyl.axis.z];
    let (cx_axis, cy_axis) = make_circle_axes(&axis);

    // Walk face boundary to find axial range and angular range.
    // Uses axis-generic projection: axial position = dot(pos - origin, axis).
    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;

    let mut t_min = f64::INFINITY;
    let mut t_max = f64::NEG_INFINITY;
    let mut angle_start: Option<f64> = None;
    let mut total_sweep = 0.0_f64;

    let mut he = start_he;
    let mut has_circular_edge = false;
    let mut has_arc_edge = false;
    let mut first_arc: Option<(f64, f64)> = None; // (start_angle, sweep_angle)
                                                  // For partial patches: use the arc geometry's normal to derive a consistent
                                                  // local frame. The arc's start_point and sweep were computed in this frame,
                                                  // so tessellation vertex placement must use the same frame.
    let mut arc_cx_axis: Option<[f64; 3]> = None;
    let mut arc_cy_axis: Option<[f64; 3]> = None;
    loop {
        let v = arena.half_edges[he.0].origin;
        let pos = arena.vertices[v.0].position;
        // Project onto cylinder axis for axial range
        let dp = [pos[0] - origin[0], pos[1] - origin[1], pos[2] - origin[2]];
        let t = dp[0] * axis[0] + dp[1] * axis[1] + dp[2] * axis[2];
        t_min = t_min.min(t);
        t_max = t_max.max(t);

        let edge = arena.half_edges[he.0].edge;
        if let Some(CurveGeom::Arc(ref arc)) = edge_geometry.get(&edge) {
            if arc_cx_axis.is_none() {
                // Derive local frame from the arc's normal (not the face axis)
                // to ensure angular consistency with arc start_point/sweep.
                let arc_n = [arc.normal.x, arc.normal.y, arc.normal.z];
                let (acx, acy) = make_circle_axes(&arc_n);
                arc_cx_axis = Some(acx);
                arc_cy_axis = Some(acy);
            }
            if first_arc.is_none() {
                let acx = arc_cx_axis.unwrap();
                let acy = arc_cy_axis.unwrap();
                let sp = [
                    arc.start_point.x - origin[0],
                    arc.start_point.y - origin[1],
                    arc.start_point.z - origin[2],
                ];
                let sp_cx = sp[0] * acx[0] + sp[1] * acx[1] + sp[2] * acx[2];
                let sp_cy = sp[0] * acy[0] + sp[1] * acy[1] + sp[2] * acy[2];
                first_arc = Some((sp_cy.atan2(sp_cx), arc.sweep_angle.abs()));
            }
            total_sweep += arc.sweep_angle.abs();
            // Project vertex radial vector into arc's frame for angle
            let acx = arc_cx_axis.unwrap();
            let acy = arc_cy_axis.unwrap();
            let v_cx = dp[0] * acx[0] + dp[1] * acx[1] + dp[2] * acx[2];
            let v_cy = dp[0] * acy[0] + dp[1] * acy[1] + dp[2] * acy[2];
            let a = v_cy.atan2(v_cx);
            if angle_start.is_none() {
                angle_start = Some(a);
            }
            has_arc_edge = true;
        } else if let Some(CurveGeom::Circular(_)) = edge_geometry.get(&edge) {
            total_sweep = std::f64::consts::TAU;
            has_circular_edge = true;
        }

        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }

    let is_full = has_circular_edge
        || (total_sweep > std::f64::consts::TAU - crate::units::FULL_CIRCLE_MARGIN
            && !has_arc_edge);

    if is_full || angle_start.is_none() {
        // Full cylinder: tessellate using axis-generic parametric placement
        let n = circle_segments();
        let base_vertex = vertices.len() as u32 / 3;
        let normal_sign = if inward { -1.0_f64 } else { 1.0_f64 };

        // Determine number of axial rows based on height-to-circumference ratio.
        // A cylinder is a ruled surface — 2 rows is geometrically exact, but
        // adding intermediate rows prevents 3D AABB collapse detection in
        // boolean results where all vertices landing on cap planes is degenerate.
        // Ref #33 Stroud — boundary-adaptive tessellation density.
        let height = (t_max - t_min).abs();
        let circumference = std::f64::consts::TAU * r;
        let n_axial = if height < TAU_WORK {
            2
        } else {
            let seg_width = circumference / (n as f64);
            let aspect = height / seg_width;
            (aspect.ceil() as usize).clamp(2, 16)
        };

        for row in 0..n_axial {
            let t = t_min + (t_max - t_min) * (row as f64) / ((n_axial - 1) as f64);
            // Center of cross-section at axial position t
            let base = [
                origin[0] + t * axis[0],
                origin[1] + t * axis[1],
                origin[2] + t * axis[2],
            ];
            for i in 0..n {
                let theta = std::f64::consts::TAU * (i as f64) / (n as f64);
                let cos_t = theta.cos();
                let sin_t = theta.sin();
                let px = base[0] + r * (cos_t * cx_axis[0] + sin_t * cy_axis[0]);
                let py = base[1] + r * (cos_t * cx_axis[1] + sin_t * cy_axis[1]);
                let pz = base[2] + r * (cos_t * cx_axis[2] + sin_t * cy_axis[2]);
                vertices.push(px as f32);
                vertices.push(py as f32);
                vertices.push(pz as f32);
                let nx = normal_sign * (cos_t * cx_axis[0] + sin_t * cy_axis[0]);
                let ny = normal_sign * (cos_t * cx_axis[1] + sin_t * cy_axis[1]);
                let nz = normal_sign * (cos_t * cx_axis[2] + sin_t * cy_axis[2]);
                normals.push(nx as f32);
                normals.push(ny as f32);
                normals.push(nz as f32);
            }
        }

        let n32 = n as u32;
        for row_idx in 0..(n_axial as u32 - 1) {
            for i in 0..n32 {
                let next = (i + 1) % n32;
                let bot = base_vertex + row_idx * n32 + i;
                let bot_next = base_vertex + row_idx * n32 + next;
                let top = base_vertex + (row_idx + 1) * n32 + i;
                let top_next = base_vertex + (row_idx + 1) * n32 + next;
                if inward {
                    indices.push(bot);
                    indices.push(top);
                    indices.push(bot_next);
                    indices.push(top);
                    indices.push(top_next);
                    indices.push(bot_next);
                } else {
                    indices.push(bot);
                    indices.push(bot_next);
                    indices.push(top);
                    indices.push(top);
                    indices.push(bot_next);
                    indices.push(top_next);
                }
            }
        }
    } else {
        // Partial cylinder patch: use angular range from arc edge geometry.
        // IMPORTANT: Use the arc's local frame (arc_cx_axis/arc_cy_axis) for vertex
        // placement, since the arc angles (start_point, sweep) were computed in that
        // frame. When the face axis is antiparallel to the arc normal (e.g., face
        // axis=[0,0,-1] but arc.normal=[0,0,1]), make_circle_axes returns different
        // frames, so we must use the arc's frame consistently.
        let pcx = arc_cx_axis.unwrap_or(cx_axis);
        let pcy = arc_cy_axis.unwrap_or(cy_axis);
        // The arc normal may be antiparallel to the face axis. For axial base
        // positioning, we use the face axis (which determines the actual axial
        // extent of the face geometry).
        let (a_start, sweep) =
            first_arc.unwrap_or((angle_start.unwrap_or(0.0), std::f64::consts::TAU));
        let normal_sign = if inward { -1.0_f64 } else { 1.0_f64 };

        let n = ((circle_segments() as f64) * sweep / std::f64::consts::TAU)
            .ceil()
            .max(4.0) as usize;
        let base_vertex = vertices.len() as u32 / 3;

        let height = (t_max - t_min).abs();
        let circumference = std::f64::consts::TAU * r;
        let seg_width = circumference * sweep / std::f64::consts::TAU / (n as f64);
        let n_axial = if height < TAU_WORK || seg_width < TAU_WORK {
            2
        } else {
            let aspect = height / seg_width;
            (aspect.ceil() as usize).clamp(2, 16)
        };

        for row in 0..n_axial {
            let t = t_min + (t_max - t_min) * (row as f64) / ((n_axial - 1) as f64);
            let base = [
                origin[0] + t * axis[0],
                origin[1] + t * axis[1],
                origin[2] + t * axis[2],
            ];
            for i in 0..=n {
                let theta = a_start + sweep * (i as f64) / (n as f64);
                let cos_t = theta.cos();
                let sin_t = theta.sin();
                let px = base[0] + r * (cos_t * pcx[0] + sin_t * pcy[0]);
                let py = base[1] + r * (cos_t * pcx[1] + sin_t * pcy[1]);
                let pz = base[2] + r * (cos_t * pcx[2] + sin_t * pcy[2]);
                vertices.push(px as f32);
                vertices.push(py as f32);
                vertices.push(pz as f32);
                let nx = normal_sign * (cos_t * pcx[0] + sin_t * pcy[0]);
                let ny = normal_sign * (cos_t * pcx[1] + sin_t * pcy[1]);
                let nz = normal_sign * (cos_t * pcx[2] + sin_t * pcy[2]);
                normals.push(nx as f32);
                normals.push(ny as f32);
                normals.push(nz as f32);
            }
        }

        let m = (n + 1) as u32;
        for row_idx in 0..(n_axial as u32 - 1) {
            for i in 0..n as u32 {
                let bot = base_vertex + row_idx * m + i;
                let bot_next = base_vertex + row_idx * m + i + 1;
                let top = base_vertex + (row_idx + 1) * m + i;
                let top_next = base_vertex + (row_idx + 1) * m + i + 1;
                if inward {
                    indices.push(bot);
                    indices.push(top);
                    indices.push(bot_next);
                    indices.push(top);
                    indices.push(top_next);
                    indices.push(bot_next);
                } else {
                    indices.push(bot);
                    indices.push(bot_next);
                    indices.push(top);
                    indices.push(top);
                    indices.push(bot_next);
                    indices.push(top_next);
                }
            }
        }
    }
}

// ── Planar face with hole tessellation ──────────────────────────────────

/// Tessellate a planar face with inner loops (holes).
/// Uses bridge + ear-clipping for the annular region.
#[allow(clippy::too_many_arguments)]
fn tessellate_planar_face_with_hole(
    arena: &TopoArena,
    face_idx: FaceIdx,
    plane: &crate::geometry::surface::Plane,
    edge_geometry: &std::collections::BTreeMap<EdgeIdx, CurveGeom>,
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    let normal = [
        plane.normal.x as f32,
        plane.normal.y as f32,
        plane.normal.z as f32,
    ];

    // Collect outer boundary vertices
    let outer_loop_idx = arena.faces[face_idx.0].outer_loop;
    let outer_start_he = arena.loops[outer_loop_idx.0].half_edge;
    let outer_is_self_loop = arena.half_edges[outer_start_he.0].next == outer_start_he;

    let outer_verts: Vec<[f64; 3]> = if outer_is_self_loop {
        // Self-loop: generate circle vertices from edge geometry (e.g., tube annular cap)
        let edge = arena.half_edges[outer_start_he.0].edge;
        if let Some(CurveGeom::Circular(ref circle)) = edge_geometry.get(&edge) {
            let cap_normal = [plane.normal.x, plane.normal.y, plane.normal.z];
            let (cx_axis, cy_axis) = make_circle_axes(&cap_normal);
            let n = circle_segments();
            (0..n)
                .map(|i| {
                    let theta = std::f64::consts::TAU * (i as f64) / (n as f64);
                    let cos_t = theta.cos();
                    let sin_t = theta.sin();
                    [
                        circle.center.x + circle.radius * (cos_t * cx_axis[0] + sin_t * cy_axis[0]),
                        circle.center.y + circle.radius * (cos_t * cx_axis[1] + sin_t * cy_axis[1]),
                        circle.center.z + circle.radius * (cos_t * cx_axis[2] + sin_t * cy_axis[2]),
                    ]
                })
                .collect()
        } else {
            return;
        }
    } else {
        collect_loop_verts(arena, outer_loop_idx)
    };

    // Collect inner boundary vertices (from first inner loop)
    let inner_loops = &arena.faces[face_idx.0].inner_loops;
    if inner_loops.is_empty() || outer_verts.len() < 3 {
        return;
    }

    let inner_loop_idx = inner_loops[0];
    // For inner loops that are circles (self-loop), generate circle points
    let inner_start_he = arena.loops[inner_loop_idx.0].half_edge;
    let is_self_loop = arena.half_edges[inner_start_he.0].next == inner_start_he;

    let inner_verts: Vec<[f64; 3]> = if is_self_loop {
        // Self-loop: generate circle vertices from edge geometry
        let edge = arena.half_edges[inner_start_he.0].edge;
        if let Some(CurveGeom::Circular(ref circle)) = edge_geometry.get(&edge) {
            let cap_normal = [plane.normal.x, plane.normal.y, plane.normal.z];
            let (cx_axis, cy_axis) = make_circle_axes(&cap_normal);
            let n = circle_segments();
            (0..n)
                .map(|i| {
                    let theta = std::f64::consts::TAU * (i as f64) / (n as f64);
                    let cos_t = theta.cos();
                    let sin_t = theta.sin();
                    [
                        circle.center.x + circle.radius * (cos_t * cx_axis[0] + sin_t * cy_axis[0]),
                        circle.center.y + circle.radius * (cos_t * cx_axis[1] + sin_t * cy_axis[1]),
                        circle.center.z + circle.radius * (cos_t * cx_axis[2] + sin_t * cy_axis[2]),
                    ]
                })
                .collect()
        } else {
            return;
        }
    } else {
        collect_loop_verts(arena, inner_loop_idx)
    };

    if inner_verts.len() < 3 {
        return;
    }

    // Tessellate the annular region using advancing-front triangulation.
    // The outer loop winds CCW and inner CW (relative to some viewing direction).
    // We add a winding check to ensure triangles match the face normal.

    let base = vertices.len() as u32 / 3;

    // Add outer vertices
    for v in &outer_verts {
        vertices.push(v[0] as f32);
        vertices.push(v[1] as f32);
        vertices.push(v[2] as f32);
        normals.push(normal[0]);
        normals.push(normal[1]);
        normals.push(normal[2]);
    }

    // Add inner vertices
    for v in &inner_verts {
        vertices.push(v[0] as f32);
        vertices.push(v[1] as f32);
        vertices.push(v[2] as f32);
        normals.push(normal[0]);
        normals.push(normal[1]);
        normals.push(normal[2]);
    }

    let n_outer = outer_verts.len();
    let n_inner = inner_verts.len();
    let outer_start = base;
    let inner_start = base + n_outer as u32;

    // Find the closest inner vertex to outer[0]
    let mut nearest_inner = 0;
    let mut min_dist = f64::INFINITY;
    for (i, iv) in inner_verts.iter().enumerate() {
        let d = dist_sq_3d(&outer_verts[0], iv);
        if d < min_dist {
            min_dist = d;
            nearest_inner = i;
        }
    }

    // Generate triangles into a temporary buffer so we can check/fix winding
    let mut tri_buf: Vec<(u32, u32, u32)> = Vec::new();

    // Advancing-front triangulation between outer and inner.
    // Outer loop goes CCW; inner loop must go CW (opposite direction) so the
    // advancing front sweeps the annular region without self-intersection.
    // Inner circle vertices as generated by make_circle_axes wind CW from the
    // face normal view, so we traverse them FORWARD (incrementing index).
    let mut oi = 0usize;
    let mut ii = nearest_inner;
    let mut outer_advanced = 0usize;
    let mut inner_advanced = 0usize;
    let total = n_outer + n_inner;

    while outer_advanced + inner_advanced < total {
        let o_cur = oi % n_outer;
        let o_nxt = (oi + 1) % n_outer;
        let i_cur = ii % n_inner;
        let i_nxt = (ii + 1) % n_inner;

        let can_outer = outer_advanced < n_outer;
        let can_inner = inner_advanced < n_inner;

        if can_outer && can_inner {
            let d_outer = dist_sq_3d(&outer_verts[o_nxt], &inner_verts[i_cur]);
            let d_inner = dist_sq_3d(&outer_verts[o_cur], &inner_verts[i_nxt]);

            if d_outer <= d_inner {
                // Outer advance: (outer_cur, outer_nxt, inner_cur)
                // Outer edge is CCW → triangle is CCW from face normal
                tri_buf.push((
                    outer_start + o_cur as u32,
                    outer_start + o_nxt as u32,
                    inner_start + i_cur as u32,
                ));
                oi += 1;
                outer_advanced += 1;
            } else {
                // Inner advance: (outer_cur, inner_nxt, inner_cur)
                // Inner edge goes CW, so swap to get CCW triangle
                tri_buf.push((
                    outer_start + o_cur as u32,
                    inner_start + i_nxt as u32,
                    inner_start + i_cur as u32,
                ));
                ii = (ii + 1) % n_inner;
                inner_advanced += 1;
            }
        } else if can_outer {
            tri_buf.push((
                outer_start + o_cur as u32,
                outer_start + o_nxt as u32,
                inner_start + i_cur as u32,
            ));
            oi += 1;
            outer_advanced += 1;
        } else if can_inner {
            tri_buf.push((
                outer_start + o_cur as u32,
                inner_start + i_nxt as u32,
                inner_start + i_cur as u32,
            ));
            ii = (ii + 1) % n_inner;
            inner_advanced += 1;
        } else {
            break;
        }
    }

    // Per-triangle winding correction against face normal.
    // The advancing-front may produce mixed winding for some triangles,
    // so correct each one individually (same approach as tessellate_polygon_face).
    let stored_n = [plane.normal.x, plane.normal.y, plane.normal.z];
    let get_pos = |idx: u32| -> [f64; 3] {
        let i = (idx - base) as usize;
        if i < n_outer {
            outer_verts[i]
        } else {
            inner_verts[i - n_outer]
        }
    };

    // Determine bulk winding from first non-degenerate triangle
    let mut bulk_reverse = false;
    for &(a, b, c) in &tri_buf {
        let pa = get_pos(a);
        let pb = get_pos(b);
        let pc = get_pos(c);
        let ab = v3_sub(pb, pa);
        let ac = v3_sub(pc, pa);
        let cr = v3_cross(ab, ac);
        let cr_len = v3_length(cr);
        if cr_len > TAU_WORK {
            let dot = cr[0] * stored_n[0] + cr[1] * stored_n[1] + cr[2] * stored_n[2];
            bulk_reverse = dot < 0.0;
            break;
        }
    }

    for (a, b, c) in tri_buf {
        let pa = get_pos(a);
        let pb = get_pos(b);
        let pc = get_pos(c);
        let ab = v3_sub(pb, pa);
        let ac = v3_sub(pc, pa);
        let cr = v3_cross(ab, ac);
        let cr_len = v3_length(cr);
        // For degenerate triangles (near-zero cross product), use bulk winding
        let should_reverse = if cr_len > TAU_WORK {
            let dot = cr[0] * stored_n[0] + cr[1] * stored_n[1] + cr[2] * stored_n[2];
            dot < 0.0
        } else {
            bulk_reverse
        };
        if should_reverse {
            indices.push(a);
            indices.push(c);
            indices.push(b);
        } else {
            indices.push(a);
            indices.push(b);
            indices.push(c);
        }
    }
}

/// Collect vertex positions from a face loop.
fn collect_loop_verts(arena: &TopoArena, loop_idx: LoopIdx) -> Vec<[f64; 3]> {
    let start_he = arena.loops[loop_idx.0].half_edge;
    let mut verts = Vec::new();
    let mut he = start_he;
    loop {
        let v = arena.half_edges[he.0].origin;
        verts.push(arena.vertices[v.0].position);
        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }
    verts
}

fn dist_sq_3d(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

// ── Arc-bounded cap tessellation (cyl-cyl boolean results) ──────────────

/// Check if a face is bounded by arc edges (from cyl-cyl boolean).
fn is_arc_bounded_face(
    arena: &TopoArena,
    face_idx: FaceIdx,
    edge_geometry: &std::collections::BTreeMap<EdgeIdx, CurveGeom>,
) -> bool {
    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;
    let mut he = start_he;
    let mut has_arc = false;
    loop {
        let edge = arena.half_edges[he.0].edge;
        if let Some(CurveGeom::Arc(_)) = edge_geometry.get(&edge) {
            has_arc = true;
        }
        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }
    has_arc
}

/// Tessellate a planar cap bounded by arc edges (cyl-cyl boolean result).
/// Generates arc point polylines and fans from centroid.
#[allow(clippy::too_many_arguments)]
fn tessellate_arc_bounded_cap(
    arena: &TopoArena,
    face_idx: FaceIdx,
    plane: &crate::geometry::surface::Plane,
    edge_geometry: &std::collections::BTreeMap<EdgeIdx, CurveGeom>,
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    let normal = [
        plane.normal.x as f32,
        plane.normal.y as f32,
        plane.normal.z as f32,
    ];
    let z = plane.origin.z;

    // Collect boundary points by walking the face loop and expanding arcs
    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;
    let mut boundary_pts: Vec<[f64; 3]> = Vec::new();

    let mut he = start_he;
    loop {
        let edge = arena.half_edges[he.0].edge;
        let origin = arena.half_edges[he.0].origin;
        let origin_pos = arena.vertices[origin.0].position;

        match edge_geometry.get(&edge) {
            Some(CurveGeom::Arc(ref arc)) => {
                // Determine if we're traversing the arc forward or backward
                let arc_center = [arc.center.x, arc.center.y];
                let arc_start_pt = [arc.start_point.x, arc.start_point.y, arc.start_point.z];

                // Check if origin matches arc start
                let origin_is_start = dist_sq_3d(&origin_pos, &arc_start_pt) < MIN_FEATURE_SIZE;
                let sweep = arc.sweep_angle;
                let r = arc.radius;

                let n_segs = ((circle_segments() as f64) * sweep.abs() / std::f64::consts::TAU)
                    .ceil()
                    .max(4.0) as usize;

                let start_angle =
                    (arc.start_point.y - arc.center.y).atan2(arc.start_point.x - arc.center.x);

                if origin_is_start {
                    for i in 0..n_segs {
                        let theta = start_angle + sweep * (i as f64) / (n_segs as f64);
                        let px = arc_center[0] + r * theta.cos();
                        let py = arc_center[1] + r * theta.sin();
                        boundary_pts.push([px, py, z]);
                    }
                } else {
                    // Reverse traversal
                    let end_angle = start_angle + sweep;
                    for i in 0..n_segs {
                        let theta = end_angle - sweep * (i as f64) / (n_segs as f64);
                        let px = arc_center[0] + r * theta.cos();
                        let py = arc_center[1] + r * theta.sin();
                        boundary_pts.push([px, py, z]);
                    }
                }
            }
            _ => {
                boundary_pts.push(origin_pos);
            }
        }

        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }

    if boundary_pts.len() < 3 {
        return;
    }

    // Compute centroid
    let mut cx = 0.0;
    let mut cy = 0.0;
    for p in &boundary_pts {
        cx += p[0];
        cy += p[1];
    }
    cx /= boundary_pts.len() as f64;
    cy /= boundary_pts.len() as f64;

    // Fan triangulation from centroid
    let base = vertices.len() as u32 / 3;

    // Add centroid
    vertices.push(cx as f32);
    vertices.push(cy as f32);
    vertices.push(z as f32);
    normals.push(normal[0]);
    normals.push(normal[1]);
    normals.push(normal[2]);

    // Add boundary points
    for p in &boundary_pts {
        vertices.push(p[0] as f32);
        vertices.push(p[1] as f32);
        vertices.push(p[2] as f32);
        normals.push(normal[0]);
        normals.push(normal[1]);
        normals.push(normal[2]);
    }

    let n = boundary_pts.len() as u32;
    let center_idx = base;
    let first_peri = base + 1;

    // Check winding direction
    if n >= 2 {
        let p1 = &boundary_pts[0];
        let p2 = &boundary_pts[1];
        let v1 = [p1[0] - cx, p1[1] - cy, 0.0];
        let v2 = [p2[0] - cx, p2[1] - cy, 0.0];
        let cross_z = v1[0] * v2[1] - v1[1] * v2[0];
        let dot_n = cross_z * plane.normal.z;
        let reverse = dot_n < 0.0;

        for i in 0..n {
            let next = (i + 1) % n;
            if reverse {
                indices.push(center_idx);
                indices.push(first_peri + next);
                indices.push(first_peri + i);
            } else {
                indices.push(center_idx);
                indices.push(first_peri + i);
                indices.push(first_peri + next);
            }
        }
    }
}

// ── Boundary-constrained tessellation (Sprint H) ────────────────────────
//
// Edge-first tessellation for boolean results. Discretizes B-Rep edges into a
// shared f64 vertex pool, then tessellates each face using those shared boundary
// vertices. Watertight by construction: adjacent faces reference identical
// vertex positions from the same pool.

/// Shared vertex pool from edge discretization.
pub(crate) struct EdgeDiscretization {
    /// Vertex positions in f64 (converted to f32 once during face tessellation).
    pub(crate) positions: Vec<[f64; 3]>,
    /// Ordered vertex indices per edge (from origin to destination).
    pub(crate) edge_verts: BTreeMap<EdgeIdx, Vec<usize>>,
    /// D1 Tier 2a: per-edge intersection-curve marker (propagated from
    /// `WaffleSolid.edge_is_intersection`, originally from
    /// `ResultTopology.edge_is_intersection`). True if this edge was born
    /// from a boolean intersection (face-shared between meshes). Empty for
    /// non-Yang-boolean solids.
    pub(crate) edge_is_intersection: BTreeMap<EdgeIdx, bool>,
}

/// Discretize all edges in a solid into a shared vertex pool.
pub(crate) fn discretize_edges(
    arena: &TopoArena,
    edge_geometry: &BTreeMap<EdgeIdx, CurveGeom>,
) -> EdgeDiscretization {
    discretize_edges_with_marker(arena, edge_geometry, &BTreeMap::new())
}

/// Discretize all edges in a solid into a shared vertex pool, preserving the
/// per-edge intersection-curve marker.
pub(crate) fn discretize_edges_with_marker(
    arena: &TopoArena,
    edge_geometry: &BTreeMap<EdgeIdx, CurveGeom>,
    edge_is_intersection: &BTreeMap<EdgeIdx, bool>,
) -> EdgeDiscretization {
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut edge_verts: BTreeMap<EdgeIdx, Vec<usize>> = BTreeMap::new();

    for (i, edge) in arena.edges.iter().enumerate() {
        let edge_idx = EdgeIdx(i);
        let he_a = edge.half_edge;
        let origin_v = arena.half_edges[he_a.0].origin;
        // PR-Y20-MODE-A: NMM (twin=None) — derive dest from
        // he_a.next.origin instead of twin.origin.
        let dest_v = match arena.half_edges[he_a.0].twin {
            Some(t) => arena.half_edges[t.0].origin,
            None => {
                let next_he = arena.half_edges[he_a.0].next;
                arena.half_edges[next_he.0].origin
            }
        };

        match edge_geometry.get(&edge_idx) {
            Some(CurveGeom::Circular(circle)) => {
                // Full circle: circle_segments points
                let n = circle_segments();
                let normal = [circle.normal.x, circle.normal.y, circle.normal.z];
                let (cx, cy) = make_circle_axes(&normal);
                let mut verts = Vec::with_capacity(n);
                for j in 0..n {
                    let theta = std::f64::consts::TAU * (j as f64) / (n as f64);
                    let cos_t = theta.cos();
                    let sin_t = theta.sin();
                    let px = circle.center.x + circle.radius * (cos_t * cx[0] + sin_t * cy[0]);
                    let py = circle.center.y + circle.radius * (cos_t * cx[1] + sin_t * cy[1]);
                    let pz = circle.center.z + circle.radius * (cos_t * cx[2] + sin_t * cy[2]);
                    let idx = positions.len();
                    positions.push([px, py, pz]);
                    verts.push(idx);
                }
                edge_verts.insert(edge_idx, verts);
            }
            Some(CurveGeom::Arc(arc)) => {
                // Proportional segments based on sweep angle
                let n = ((circle_segments() as f64) * arc.sweep_angle.abs() / std::f64::consts::TAU)
                    .ceil()
                    .max(4.0) as usize;
                let mut verts = Vec::with_capacity(n + 1);
                for j in 0..=n {
                    let t = arc.sweep_angle * (j as f64) / (n as f64);
                    let pt = arc.evaluate(t);
                    let idx = positions.len();
                    positions.push([pt.x, pt.y, pt.z]);
                    verts.push(idx);
                }
                edge_verts.insert(edge_idx, verts);
            }
            Some(CurveGeom::Elliptical(ellipse)) => {
                // Full ellipse: circle_segments points
                let n = circle_segments();
                let mut verts = Vec::with_capacity(n);
                for j in 0..n {
                    let t = std::f64::consts::TAU * (j as f64) / (n as f64);
                    let pt = ellipse.evaluate(t);
                    let idx = positions.len();
                    positions.push([pt.x, pt.y, pt.z]);
                    verts.push(idx);
                }
                edge_verts.insert(edge_idx, verts);
            }
            Some(CurveGeom::Linear(_)) | None => {
                // Linear edge or no geometry: 2 points from arena vertex positions
                let p0 = arena.vertices[origin_v.0].position;
                let p1 = arena.vertices[dest_v.0].position;
                let idx0 = positions.len();
                positions.push(p0);
                let idx1 = positions.len();
                positions.push(p1);
                edge_verts.insert(edge_idx, vec![idx0, idx1]);
            }
        }
    }

    EdgeDiscretization {
        positions,
        edge_verts,
        edge_is_intersection: edge_is_intersection.clone(),
    }
}

/// Collect boundary vertex indices for a loop by walking half-edges.
/// Returns indices into the EdgeDiscretization.positions pool.
pub(crate) fn collect_loop_boundary(
    arena: &TopoArena,
    loop_idx: LoopIdx,
    disc: &EdgeDiscretization,
) -> Vec<usize> {
    let start_he = arena.loops[loop_idx.0].half_edge;
    let mut boundary = Vec::new();
    let mut he = start_he;

    loop {
        let edge_idx = arena.half_edges[he.0].edge;
        let edge = &arena.edges[edge_idx.0];

        if let Some(verts) = disc.edge_verts.get(&edge_idx) {
            // Determine direction: if this half-edge is the "primary" one
            // (same as edge.half_edge), use forward order; otherwise reverse.
            let is_primary = edge.half_edge == he;

            // For self-loop edges (circular caps), the half-edge loops back
            // to itself. Include all vertices.
            let is_self_loop = arena.half_edges[he.0].next == he;

            if is_self_loop {
                // Full circle: include all vertices in appropriate order
                if is_primary {
                    boundary.extend_from_slice(verts);
                } else {
                    boundary.extend(verts.iter().rev());
                }
            } else if verts.len() <= 2 {
                // Linear edge: include only the origin vertex (destination is
                // the next half-edge's origin)
                if is_primary {
                    boundary.push(verts[0]);
                } else {
                    boundary.push(verts[verts.len() - 1]);
                }
            } else {
                // Curved edge (arc or full circle used as part of a multi-edge loop).
                // For full circles (64 pts covering 0°..354.375°), include ALL vertices
                // since none duplicate — the next edge starts at the seam vertex which
                // coincides with verts[0] but the linear edge contributes that separately.
                // For arcs, the last vertex IS the arc endpoint which coincides with the
                // next edge's start, so drop it to avoid duplication.
                let is_full_circle = verts.len() == circle_segments();
                if is_full_circle {
                    // Include all vertices (no overlap with next edge)
                    if is_primary {
                        boundary.extend_from_slice(verts);
                    } else {
                        boundary.extend(verts.iter().rev());
                    }
                } else if is_primary {
                    boundary.extend_from_slice(&verts[..verts.len() - 1]);
                } else {
                    for &v in verts.iter().rev().skip(1) {
                        boundary.push(v);
                    }
                }
            }
        }

        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }

    boundary
}

/// Tessellate a planar face using shared boundary vertices.
///
/// `plane_origin` is the surface-intrinsic 2D origin for the CDT projection
/// (typically `plane.origin` from `SurfaceGeom::Planar`). Using the plane's
/// own origin instead of a vertex-derived one ensures coplanar adjacent
/// faces produce byte-identical 2D coordinates for shared 3D points — the
/// cross-face consistency property Yang §4.1.2 / §4.4.1 rely on.
fn tessellate_planar_face_bounded(
    boundary: &[usize],
    positions: &[[f64; 3]],
    normal: [f32; 3],
    plane_origin: [f64; 3],
    out_verts: &mut Vec<f32>,
    out_normals: &mut Vec<f32>,
    out_indices: &mut Vec<u32>,
    inner_boundaries: &[Vec<usize>],
) {
    // PR-Y41 dispatch probe (env-gated, default-off byte-identical):
    // record per-call entry state for later per-triangle quantization analysis.
    let y41_on = y41_probe_enabled();
    let y41_start_idx_count = if y41_on { out_indices.len() } else { 0 };
    let y41_start_vert_count = if y41_on { out_verts.len() / 3 } else { 0 };
    let y41_boundary_positions: Vec<[f64; 3]> = if y41_on {
        boundary.iter().map(|&i| positions[i]).collect()
    } else {
        Vec::new()
    };
    let y41_inner_count = if y41_on { inner_boundaries.len() } else { 0 };
    let y41_boundary_size = if y41_on { boundary.len() } else { 0 };

    if boundary.len() < 3 {
        if y41_on {
            y41_push_record(Y41DispatchRecord {
                dispatch_type: "planar",
                boundary_size: y41_boundary_size,
                inner_count: y41_inner_count,
                indices_emitted: 0,
                distinct_quantized_tris: 0,
                degenerate_collapse_count: 0,
                single_vert_collision_count: 0,
                boundary_positions: y41_boundary_positions,
            });
        }
        return;
    }

    let base_vertex = out_verts.len() as u32 / 3;

    // Collect loop vertices in f64
    let loop_verts: Vec<[f64; 3]> = boundary.iter().map(|&i| positions[i]).collect();

    // Check winding against stored normal using Newell method
    let stored_normal = [normal[0] as f64, normal[1] as f64, normal[2] as f64];
    let n = loop_verts.len();
    let mut newell = [0.0f64; 3];
    for i in 0..n {
        let curr = loop_verts[i];
        let next = loop_verts[(i + 1) % n];
        newell[0] += (curr[1] - next[1]) * (curr[2] + next[2]);
        newell[1] += (curr[2] - next[2]) * (curr[0] + next[0]);
        newell[2] += (curr[0] - next[0]) * (curr[1] + next[1]);
    }
    let dot = v3_dot(newell, stored_normal);
    let reverse_outer = dot < 0.0;

    // Emit outer boundary vertices from shared pool
    let ordered_verts: Vec<[f64; 3]> = if reverse_outer {
        loop_verts.iter().rev().copied().collect()
    } else {
        loop_verts.clone()
    };

    for v in &ordered_verts {
        out_verts.push(v[0] as f32);
        out_verts.push(v[1] as f32);
        out_verts.push(v[2] as f32);
        out_normals.push(normal[0]);
        out_normals.push(normal[1]);
        out_normals.push(normal[2]);
    }

    if inner_boundaries.is_empty() {
        // No holes: use fan or earclip
        let is_convex = {
            let mut convex = true;
            for i in 0..n {
                let a = ordered_verts[i];
                let b = ordered_verts[(i + 1) % n];
                let c = ordered_verts[(i + 2) % n];
                let ab = v3_sub(b, a);
                let bc = v3_sub(c, b);
                let cross = v3_cross(ab, bc);
                if v3_dot(cross, stored_normal) < 0.0 {
                    convex = false;
                    break;
                }
            }
            convex
        };

        // Check for collinear consecutive vertices — fan from vertex 0
        // would produce degenerate (zero-area) triangles when vertex 0 lies
        // on the line through (i, i+1). This happens when Yang coplanar merge
        // keeps intersection-plane vertices on merged face boundaries.
        let has_collinear = is_convex && n >= 4 && {
            (0..n).any(|i| {
                let a = ordered_verts[i];
                let b = ordered_verts[(i + 1) % n];
                let c = ordered_verts[(i + 2) % n];
                let ab = v3_sub(b, a);
                let bc = v3_sub(c, b);
                let cross = v3_cross(ab, bc);
                v3_length(cross) < crate::units::TAU_NORMALIZE
            })
        };

        if is_convex && n <= 8 && !has_collinear {
            // Fan triangulation for simple convex faces without collinear vertices
            for i in 1..n - 1 {
                out_indices.push(base_vertex);
                out_indices.push(base_vertex + i as u32);
                out_indices.push(base_vertex + (i + 1) as u32);
            }
        } else if has_collinear {
            // Centroid-fan: add polygon centroid as a Steiner vertex and fan
            // from it to each boundary edge. The centroid is interior to the
            // convex polygon, so no triangle can be degenerate. All boundary
            // edges (including collinear segments) are preserved for cross-face
            // edge pairing.
            let mut cx = 0.0_f64;
            let mut cy = 0.0_f64;
            let mut cz = 0.0_f64;
            for v in &ordered_verts {
                cx += v[0];
                cy += v[1];
                cz += v[2];
            }
            let inv_n = 1.0 / n as f64;
            cx *= inv_n;
            cy *= inv_n;
            cz *= inv_n;

            // Emit centroid vertex
            let centroid_idx = out_verts.len() as u32 / 3;
            out_verts.push(cx as f32);
            out_verts.push(cy as f32);
            out_verts.push(cz as f32);
            out_normals.push(normal[0]);
            out_normals.push(normal[1]);
            out_normals.push(normal[2]);

            // Fan from centroid to each boundary edge
            for i in 0..n {
                out_indices.push(centroid_idx);
                out_indices.push(base_vertex + i as u32);
                out_indices.push(base_vertex + ((i + 1) % n) as u32);
            }
        } else {
            // CDT triangulation per Yang 2025 §4.4.1 (deviation D1 remediation).
            // Constraint edges = every consecutive segment of the outer boundary.
            // D5/D9 (2026-05-18): 2D origin is the plane's intrinsic origin
            // (`plane_origin`), not `ordered_verts[0]`. Two adjacent coplanar
            // faces share `plane.origin`, so the same 3D point projects to
            // byte-identical 2D coordinates in both faces' CDT inputs.
            let (u_axis, v_axis) = compute_plane_basis(stored_normal);
            let points_2d: Vec<(f64, f64)> = ordered_verts
                .iter()
                .map(|v| {
                    let d = v3_sub(*v, plane_origin);
                    (v3_dot(d, u_axis), v3_dot(d, v_axis))
                })
                .collect();
            let outer_loop: Vec<usize> = (0..points_2d.len()).collect();
            let cdt_result =
                cdt::cdt_triangulate_2d_with_loops(&points_2d, &[outer_loop]);
            match cdt_result {
                Ok(triangles) => {
                    for t in &triangles {
                        out_indices.push(base_vertex + t[0] as u32);
                        out_indices.push(base_vertex + t[1] as u32);
                        out_indices.push(base_vertex + t[2] as u32);
                    }
                }
                Err(e) => {
                    // No fallback: Yang §4.4.1 mandates CDT. Falling back to a
                    // non-CDT triangulator would be a deviation. Surface the
                    // failure visibly; downstream sees an empty face.
                    eprintln!(
                        "[yang §4.4.1] CDT failed on planar face (no holes, {} verts): {:?}",
                        points_2d.len(),
                        e
                    );
                }
            }
        }
    } else {
        // Face with holes: CDT per Yang 2025 §4.4.1.
        // D5/D9 (2026-05-18): 2D origin is plane-intrinsic (`plane_origin`).
        let (u_axis, v_axis) = compute_plane_basis(stored_normal);
        let mut points_2d: Vec<(f64, f64)> = Vec::new();
        let mut loops: Vec<Vec<usize>> = Vec::new();

        // Outer ring
        let outer_start = points_2d.len();
        for v in &ordered_verts {
            let d = v3_sub(*v, plane_origin);
            points_2d.push((v3_dot(d, u_axis), v3_dot(d, v_axis)));
        }
        loops.push((outer_start..points_2d.len()).collect());

        // Inner rings (holes)
        for inner_b in inner_boundaries {
            let hole_start = points_2d.len();
            let inner_verts: Vec<[f64; 3]> = inner_b.iter().map(|&i| positions[i]).collect();
            for v in &inner_verts {
                out_verts.push(v[0] as f32);
                out_verts.push(v[1] as f32);
                out_verts.push(v[2] as f32);
                out_normals.push(normal[0]);
                out_normals.push(normal[1]);
                out_normals.push(normal[2]);
                let d = v3_sub(*v, plane_origin);
                points_2d.push((v3_dot(d, u_axis), v3_dot(d, v_axis)));
            }
            loops.push((hole_start..points_2d.len()).collect());
        }

        let cdt_result = cdt::cdt_triangulate_2d_with_loops(&points_2d, &loops);
        match cdt_result {
            Ok(triangles) => {
                for t in &triangles {
                    out_indices.push(base_vertex + t[0] as u32);
                    out_indices.push(base_vertex + t[1] as u32);
                    out_indices.push(base_vertex + t[2] as u32);
                }
            }
            Err(e) => {
                // No fallback: Yang §4.4.1 mandates CDT. Falling back to a
                // non-CDT triangulator would be a deviation. Surface the
                // failure visibly; downstream sees an empty face.
                eprintln!(
                    "[yang §4.4.1] CDT failed on planar face ({} verts, {} loops): {:?}",
                    points_2d.len(),
                    loops.len(),
                    e
                );
            }
        }
    }

    // PR-Y41 dispatch probe exit: classify each emitted triangle's quantization.
    if y41_on {
        let indices_emitted = out_indices.len() - y41_start_idx_count;
        let inv_grid = y41_inv_grid_from_verts(out_verts);
        let mut distinct_q_tris = 0usize;
        let mut degen = 0usize;
        let mut single_coll = 0usize;
        let tri_count = indices_emitted / 3;
        for t in 0..tri_count {
            let base = y41_start_idx_count + t * 3;
            let ia = out_indices[base] as usize;
            let ib = out_indices[base + 1] as usize;
            let ic = out_indices[base + 2] as usize;
            let qa = y41_quantize_f32_vert(out_verts, ia, inv_grid);
            let qb = y41_quantize_f32_vert(out_verts, ib, inv_grid);
            let qc = y41_quantize_f32_vert(out_verts, ic, inv_grid);
            if qa == qb && qb == qc {
                degen += 1;
            } else if qa == qb || qb == qc || qa == qc {
                single_coll += 1;
            } else {
                distinct_q_tris += 1;
            }
        }
        let _ = y41_start_vert_count; // reserved for future use (vertex-count delta)
        y41_push_record(Y41DispatchRecord {
            dispatch_type: "planar",
            boundary_size: y41_boundary_size,
            inner_count: y41_inner_count,
            indices_emitted,
            distinct_quantized_tris: distinct_q_tris,
            degenerate_collapse_count: degen,
            single_vert_collision_count: single_coll,
            boundary_positions: y41_boundary_positions,
        });
    }
}

/// Tessellate a cylindrical face using shared boundary vertices.
///
/// For full cylinders (self-loop edge): builds a quad strip tube.
/// For partial patches: uses earclip triangulation of the boundary polygon
/// with cylinder-derived normals, guaranteeing edge-matching with adjacent faces.
#[allow(clippy::too_many_arguments)]
fn tessellate_cylindrical_face_bounded(
    arena: &TopoArena,
    face_idx: FaceIdx,
    cyl: &crate::geometry::surface::Cylinder,
    disc: &EdgeDiscretization,
    _edge_geometry: &BTreeMap<EdgeIdx, CurveGeom>,
    out_verts: &mut Vec<f32>,
    out_normals: &mut Vec<f32>,
    out_indices: &mut Vec<u32>,
) {
    let axis = [cyl.axis.x, cyl.axis.y, cyl.axis.z];
    let origin = [cyl.origin.x, cyl.origin.y, cyl.origin.z];
    let inward = cyl.radius < 0.0;
    let normal_sign = if inward { -1.0_f64 } else { 1.0_f64 };

    // Collect boundary
    let boundary = collect_loop_boundary(arena, arena.faces[face_idx.0].outer_loop, disc);
    if boundary.len() < 3 {
        return;
    }

    // Check if this face has any curved edges. Polygon-clipping boolean results
    // tag faces with SurfaceGeom::Cylindrical but have only linear edge geometry
    // (polygon approximation vertices). For these faces, use planar tessellation
    // with cylindrical normals — the ring-building logic below assumes curved edges.
    let has_curved_edges = {
        let mut found = false;
        let start_he = arena.loops[arena.faces[face_idx.0].outer_loop.0].half_edge;
        let mut he = start_he;
        loop {
            let edge_idx = arena.half_edges[he.0].edge;
            if matches!(
                _edge_geometry.get(&edge_idx),
                Some(CurveGeom::Circular(_))
                    | Some(CurveGeom::Arc(_))
                    | Some(CurveGeom::Elliptical(_))
            ) {
                found = true;
                break;
            }
            he = arena.half_edges[he.0].next;
            if he == start_he {
                break;
            }
        }
        found
    };

    if !has_curved_edges {
        // Polygon-approximation face: tessellate as planar polygon with cylindrical normals
        let base_vertex = out_verts.len() as u32 / 3;
        for &vi in &boundary {
            let pos = disc.positions[vi];
            out_verts.push(pos[0] as f32);
            out_verts.push(pos[1] as f32);
            out_verts.push(pos[2] as f32);
            let dp = v3_sub(pos, origin);
            let along = v3_dot(dp, axis);
            let rad = [
                dp[0] - along * axis[0],
                dp[1] - along * axis[1],
                dp[2] - along * axis[2],
            ];
            let rlen = v3_length(rad);
            if rlen > TAU_NORMALIZE {
                out_normals.push((normal_sign * rad[0] / rlen) as f32);
                out_normals.push((normal_sign * rad[1] / rlen) as f32);
                out_normals.push((normal_sign * rad[2] / rlen) as f32);
            } else {
                out_normals.push(0.0);
                out_normals.push(0.0);
                out_normals.push(1.0);
            }
        }
        // Fan triangulation
        let n = boundary.len() as u32;
        for i in 1..n - 1 {
            if inward {
                out_indices.push(base_vertex);
                out_indices.push(base_vertex + i + 1);
                out_indices.push(base_vertex + i);
            } else {
                out_indices.push(base_vertex);
                out_indices.push(base_vertex + i);
                out_indices.push(base_vertex + i + 1);
            }
        }
        return;
    }

    let project_axial = |pos: [f64; 3]| -> f64 {
        let dp = v3_sub(pos, origin);
        v3_dot(dp, axis)
    };

    // Find axial range
    let mut t_min = f64::INFINITY;
    let mut t_max = f64::NEG_INFINITY;
    for &vi in &boundary {
        let t = project_axial(disc.positions[vi]);
        t_min = t_min.min(t);
        t_max = t_max.max(t);
    }

    // Check if this is a full cylinder (self-loop edge)
    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;
    let is_self_loop = arena.half_edges[start_he.0].next == start_he;

    if is_self_loop && boundary.len() >= circle_segments() {
        let (cx_axis, cy_axis) = make_circle_axes(&axis);

        // Check for inner loops (e.g., cyl-cyl boolean: outer ellipse + inner ellipse hole)
        let inner_loops = &arena.faces[face_idx.0].inner_loops;
        if !inner_loops.is_empty() {
            // Annular mesh between outer and inner rings on the cylinder surface.
            // Both rings are closed self-loop ellipses. We cut each ring at the
            // angle closest to 0, creating two open arcs, then stitch them into
            // an annular strip in cylindrical (θ,z) coordinates using earcut.
            let outer_ring = &boundary;

            let inner_boundary = collect_loop_boundary(arena, inner_loops[0], disc);
            if inner_boundary.len() >= 3 {
                let angle_of = |vi: usize| -> f64 {
                    let dp = v3_sub(disc.positions[vi], origin);
                    v3_dot(dp, cy_axis).atan2(v3_dot(dp, cx_axis))
                };

                // Sort both rings by angle for consistent ordering
                let mut outer_sorted: Vec<usize> = outer_ring.clone();
                outer_sorted.sort_by(|a, b| {
                    angle_of(*a)
                        .partial_cmp(&angle_of(*b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let mut inner_sorted: Vec<usize> = inner_boundary;
                inner_sorted.sort_by(|a, b| {
                    angle_of(*a)
                        .partial_cmp(&angle_of(*b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                // Build an annular polygon in (θ,z) space by cutting both rings
                // and forming a single non-self-intersecting polygon:
                //   outer[0] → outer[1] → ... → outer[N-1] → outer[0] (close outer)
                //   bridge to inner[0]
                //   inner[0] → inner[N-1] → ... → inner[1] → inner[0] (reverse inner)
                //   bridge back to outer[0]
                // This creates a "strip" polygon that earcut can triangulate.
                let mut strip_verts: Vec<usize> = Vec::new();
                let mut strip_2d: Vec<f64> = Vec::new();

                // Add outer ring (forward order) — repeat first point at end to close
                for &vi in outer_sorted.iter() {
                    strip_verts.push(vi);
                }
                strip_verts.push(outer_sorted[0]); // close outer ring

                // Bridge: add inner[0]
                strip_verts.push(inner_sorted[0]);

                // Add inner ring (reverse order) — repeat first point at end to close
                for &vi in inner_sorted.iter().rev() {
                    strip_verts.push(vi);
                }
                strip_verts.push(inner_sorted[0]); // close inner ring

                // Bridge back: add outer[0]
                strip_verts.push(outer_sorted[0]);

                // Build 2D cylindrical coordinates
                for &vi in &strip_verts {
                    let dp = v3_sub(disc.positions[vi], origin);
                    strip_2d.push(v3_dot(dp, cy_axis).atan2(v3_dot(dp, cx_axis)));
                    strip_2d.push(v3_dot(dp, axis));
                }

                // Unwrap theta
                for i in 1..strip_verts.len() {
                    let idx = i * 2;
                    while strip_2d[idx] - strip_2d[idx - 2] > std::f64::consts::PI {
                        strip_2d[idx] -= std::f64::consts::TAU;
                    }
                    while strip_2d[idx] - strip_2d[idx - 2] < -std::f64::consts::PI {
                        strip_2d[idx] += std::f64::consts::TAU;
                    }
                }

                // Emit vertices with cylindrical normals (deduplicated via index map)
                let base_vertex = out_verts.len() as u32 / 3;
                let mut vi_to_local: BTreeMap<usize, u32> = BTreeMap::new();
                let mut next_local: u32 = 0;
                let mut local_indices: Vec<u32> = Vec::with_capacity(strip_verts.len());

                for &vi in &strip_verts {
                    let local = *vi_to_local.entry(vi).or_insert_with(|| {
                        let idx = next_local;
                        next_local += 1;
                        let pos = disc.positions[vi];
                        out_verts.push(pos[0] as f32);
                        out_verts.push(pos[1] as f32);
                        out_verts.push(pos[2] as f32);
                        let dp = v3_sub(pos, origin);
                        let along = v3_dot(dp, axis);
                        let rad = [
                            dp[0] - along * axis[0],
                            dp[1] - along * axis[1],
                            dp[2] - along * axis[2],
                        ];
                        let rlen = v3_length(rad);
                        if rlen > TAU_NORMALIZE {
                            out_normals.push((normal_sign * rad[0] / rlen) as f32);
                            out_normals.push((normal_sign * rad[1] / rlen) as f32);
                            out_normals.push((normal_sign * rad[2] / rlen) as f32);
                        } else {
                            out_normals.push(0.0);
                            out_normals.push(0.0);
                            out_normals.push(1.0);
                        }
                        idx
                    });
                    local_indices.push(local);
                }

                // CDT the strip polygon (no holes — the strip IS the annulus)
                if let Ok(tri_indices) = cdt::cdt_triangulate_flat(&strip_2d, &[]) {
                    for &ti in &tri_indices {
                        let local = local_indices[ti];
                        out_indices.push(base_vertex + local);
                    }
                }
                return;
            }
        }

        // No inner loops: full cylinder tube — build quad strip from two copies of the ring
        let ring = &boundary;
        let n = ring.len();
        let base_vertex = out_verts.len() as u32 / 3;

        // Sort ring by angle for consistent winding
        let mut ring_sorted: Vec<usize> = ring.clone();
        ring_sorted.sort_by(|a, b| {
            let da = v3_sub(disc.positions[*a], origin);
            let db = v3_sub(disc.positions[*b], origin);
            let aa = v3_dot(da, cy_axis).atan2(v3_dot(da, cx_axis));
            let ab = v3_dot(db, cy_axis).atan2(v3_dot(db, cx_axis));
            aa.partial_cmp(&ab).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Helper: emit vertex at given axial position with radial from shared pool
        let emit_ring =
            |ring: &[usize], t_axial: f64, verts: &mut Vec<f32>, norms: &mut Vec<f32>| {
                for &vi in ring {
                    let orig_pos = disc.positions[vi];
                    let dp = v3_sub(orig_pos, origin);
                    let radial = [
                        dp[0] - v3_dot(dp, axis) * axis[0],
                        dp[1] - v3_dot(dp, axis) * axis[1],
                        dp[2] - v3_dot(dp, axis) * axis[2],
                    ];
                    let pos = [
                        origin[0] + t_axial * axis[0] + radial[0],
                        origin[1] + t_axial * axis[1] + radial[1],
                        origin[2] + t_axial * axis[2] + radial[2],
                    ];
                    verts.push(pos[0] as f32);
                    verts.push(pos[1] as f32);
                    verts.push(pos[2] as f32);
                    let rlen = v3_length(radial);
                    if rlen > TAU_NORMALIZE {
                        norms.push((normal_sign * radial[0] / rlen) as f32);
                        norms.push((normal_sign * radial[1] / rlen) as f32);
                        norms.push((normal_sign * radial[2] / rlen) as f32);
                    } else {
                        norms.push(0.0);
                        norms.push(0.0);
                        norms.push(1.0);
                    }
                }
            };

        // Multi-row tessellation: add intermediate axial rows to prevent
        // 3D AABB collapse where all vertices land on cap planes.
        // Ref #33 Stroud — boundary-adaptive tessellation density.
        let r_abs = {
            let dp0 = v3_sub(disc.positions[ring_sorted[0]], origin);
            let along0 = v3_dot(dp0, axis);
            let rad0 = [
                dp0[0] - along0 * axis[0],
                dp0[1] - along0 * axis[1],
                dp0[2] - along0 * axis[2],
            ];
            v3_length(rad0)
        };
        let height = (t_max - t_min).abs();
        let circumference = std::f64::consts::TAU * r_abs;
        let n_axial = if height < TAU_WORK {
            2
        } else {
            let seg_width = circumference / (n as f64);
            let aspect = height / seg_width;
            (aspect.ceil() as usize).clamp(2, 16)
        };

        for row in 0..n_axial {
            let t = t_min + (t_max - t_min) * (row as f64) / ((n_axial - 1) as f64);
            emit_ring(&ring_sorted, t, out_verts, out_normals);
        }

        let n32 = n as u32;
        for row_idx in 0..(n_axial as u32 - 1) {
            for i in 0..n32 {
                let next = (i + 1) % n32;
                let bot = base_vertex + row_idx * n32 + i;
                let bot_next = base_vertex + row_idx * n32 + next;
                let top = base_vertex + (row_idx + 1) * n32 + i;
                let top_next = base_vertex + (row_idx + 1) * n32 + next;
                if inward {
                    out_indices.push(bot);
                    out_indices.push(top);
                    out_indices.push(bot_next);
                    out_indices.push(top);
                    out_indices.push(top_next);
                    out_indices.push(bot_next);
                } else {
                    out_indices.push(bot);
                    out_indices.push(bot_next);
                    out_indices.push(top);
                    out_indices.push(top);
                    out_indices.push(bot_next);
                    out_indices.push(top_next);
                }
            }
        }
        return;
    }

    // Partial cylindrical patch: extract top/bottom rings from curved edges,
    // then either quad strip (equal rings) or cylindrical-coordinate earcut (unequal).
    let t_range = t_max - t_min;
    let mut top_ring: Vec<usize> = Vec::new();
    let mut bottom_ring: Vec<usize> = Vec::new();

    // Walk half-edges and extract curved edge vertices into rings
    let mut he2 = start_he;
    loop {
        let edge_idx = arena.half_edges[he2.0].edge;
        let is_primary = arena.edges[edge_idx.0].half_edge == he2;

        if let Some(verts) = disc.edge_verts.get(&edge_idx) {
            let is_curved = matches!(
                _edge_geometry.get(&edge_idx),
                Some(CurveGeom::Circular(_))
                    | Some(CurveGeom::Arc(_))
                    | Some(CurveGeom::Elliptical(_))
            );

            if is_curved && verts.len() > 2 {
                let sample_pos = disc.positions[verts[0]];
                let t = project_axial(sample_pos);
                let target = if t_range > TAU_NORMALIZE && (t - t_min) / t_range < 0.5 {
                    &mut bottom_ring
                } else {
                    &mut top_ring
                };

                let is_full_circle = verts.len() == circle_segments();
                if is_full_circle {
                    if is_primary {
                        target.extend_from_slice(verts);
                    } else {
                        target.extend(verts.iter().rev());
                    }
                } else if is_primary {
                    target.extend_from_slice(&verts[..verts.len() - 1]);
                } else {
                    for &v in verts.iter().rev().skip(1) {
                        target.push(v);
                    }
                }
            }
        }

        he2 = arena.half_edges[he2.0].next;
        if he2 == start_he {
            break;
        }
    }

    // Fall back to axial midpoint split if edge-walk didn't find curved edges
    if top_ring.is_empty() || bottom_ring.is_empty() {
        top_ring.clear();
        bottom_ring.clear();
        for &vi in &boundary {
            let t = project_axial(disc.positions[vi]);
            if t_range > TAU_NORMALIZE && (t - t_min) / t_range < 0.5 {
                bottom_ring.push(vi);
            } else {
                top_ring.push(vi);
            }
        }
    }

    if top_ring.is_empty() || bottom_ring.is_empty() || top_ring.len() < 3 || bottom_ring.len() < 3
    {
        // Can't form rings — fall back to polygon
        let approx_normal = [
            (normal_sign * axis[0]) as f32,
            (normal_sign * axis[1]) as f32,
            (normal_sign * axis[2]) as f32,
        ];
        // No known plane; use first boundary vertex as 2D origin fallback.
        let fallback_origin = if !boundary.is_empty() {
            disc.positions[boundary[0]]
        } else {
            [0.0; 3]
        };
        tessellate_planar_face_bounded(
            &boundary,
            &disc.positions,
            approx_normal,
            fallback_origin,
            out_verts,
            out_normals,
            out_indices,
            &[],
        );
        return;
    }

    // Sort rings by angle around the cylinder axis for consistent winding
    let (cx_axis, cy_axis) = make_circle_axes(&axis);
    let angle_of = |pos: [f64; 3]| -> f64 {
        let dp = v3_sub(pos, origin);
        v3_dot(dp, cy_axis).atan2(v3_dot(dp, cx_axis))
    };

    top_ring.sort_by(|a, b| {
        angle_of(disc.positions[*a])
            .partial_cmp(&angle_of(disc.positions[*b]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bottom_ring.sort_by(|a, b| {
        angle_of(disc.positions[*a])
            .partial_cmp(&angle_of(disc.positions[*b]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if top_ring.len() == bottom_ring.len() {
        // Equal rings → quad strip with intermediate axial rows.
        // Adding intermediate rows prevents 3D AABB collapse detection in
        // boolean results where all vertices landing on cap planes is degenerate.
        let ring_len = top_ring.len();
        let base_vertex = out_verts.len() as u32 / 3;

        // Compute n_axial from height vs segment width
        let r_est = {
            let dp0 = v3_sub(disc.positions[bottom_ring[0]], origin);
            let along0 = v3_dot(dp0, axis);
            let rad0 = [
                dp0[0] - along0 * axis[0],
                dp0[1] - along0 * axis[1],
                dp0[2] - along0 * axis[2],
            ];
            v3_length(rad0)
        };
        let height = (t_max - t_min).abs();
        let circumference = std::f64::consts::TAU * r_est;
        let seg_width = circumference / (ring_len as f64);
        let n_axial = if height < TAU_WORK || seg_width < TAU_WORK {
            2
        } else {
            let aspect = height / seg_width;
            (aspect.ceil() as usize).clamp(2, 16)
        };
        let emit_vertex_interp = |vi_bot: usize,
                                  vi_top: usize,
                                  frac: f64,
                                  verts: &mut Vec<f32>,
                                  norms: &mut Vec<f32>| {
            let bot_pos = disc.positions[vi_bot];
            let top_pos = disc.positions[vi_top];
            let pos = [
                bot_pos[0] + frac * (top_pos[0] - bot_pos[0]),
                bot_pos[1] + frac * (top_pos[1] - bot_pos[1]),
                bot_pos[2] + frac * (top_pos[2] - bot_pos[2]),
            ];
            verts.push(pos[0] as f32);
            verts.push(pos[1] as f32);
            verts.push(pos[2] as f32);
            let dp = v3_sub(pos, origin);
            let along = v3_dot(dp, axis);
            let rad = [
                dp[0] - along * axis[0],
                dp[1] - along * axis[1],
                dp[2] - along * axis[2],
            ];
            let rlen = v3_length(rad);
            if rlen > TAU_NORMALIZE {
                norms.push((normal_sign * rad[0] / rlen) as f32);
                norms.push((normal_sign * rad[1] / rlen) as f32);
                norms.push((normal_sign * rad[2] / rlen) as f32);
            } else {
                norms.push(0.0);
                norms.push(0.0);
                norms.push(1.0);
            }
        };

        for row in 0..n_axial {
            let frac = (row as f64) / ((n_axial - 1) as f64);
            for j in 0..ring_len {
                emit_vertex_interp(bottom_ring[j], top_ring[j], frac, out_verts, out_normals);
            }
        }

        let n = ring_len as u32;
        let is_full = {
            let a0 = angle_of(disc.positions[bottom_ring[0]]);
            let an = angle_of(disc.positions[bottom_ring[ring_len - 1]]);
            (an - a0).abs() > std::f64::consts::TAU - crate::units::FULL_CIRCLE_MARGIN_CONE
        };

        for row_idx in 0..(n_axial as u32 - 1) {
            for i in 0..n {
                let next = if is_full {
                    (i + 1) % n
                } else if i + 1 < n {
                    i + 1
                } else {
                    continue;
                };
                let bot = base_vertex + row_idx * n + i;
                let bot_next = base_vertex + row_idx * n + next;
                let top = base_vertex + (row_idx + 1) * n + i;
                let top_next = base_vertex + (row_idx + 1) * n + next;

                if inward {
                    out_indices.push(bot);
                    out_indices.push(top);
                    out_indices.push(bot_next);
                    out_indices.push(top);
                    out_indices.push(top_next);
                    out_indices.push(bot_next);
                } else {
                    out_indices.push(bot);
                    out_indices.push(bot_next);
                    out_indices.push(top);
                    out_indices.push(top);
                    out_indices.push(bot_next);
                    out_indices.push(top_next);
                }
            }
        }
    } else {
        // Unequal rings (cyl-cyl arc patches): use cylindrical-coordinate earcut
        // on the full boundary. This preserves all shared-pool vertices for watertightness.
        let mut thetas: Vec<f64> = Vec::with_capacity(boundary.len());
        let mut axials: Vec<f64> = Vec::with_capacity(boundary.len());
        for &vi in &boundary {
            let dp = v3_sub(disc.positions[vi], origin);
            thetas.push(v3_dot(dp, cy_axis).atan2(v3_dot(dp, cx_axis)));
            axials.push(v3_dot(dp, axis));
        }
        // Unwrap theta to avoid atan2 discontinuity
        for i in 1..thetas.len() {
            while thetas[i] - thetas[i - 1] > std::f64::consts::PI {
                thetas[i] -= std::f64::consts::TAU;
            }
            while thetas[i] - thetas[i - 1] < -std::f64::consts::PI {
                thetas[i] += std::f64::consts::TAU;
            }
        }

        let mut coords_2d: Vec<f64> = Vec::with_capacity(boundary.len() * 2);
        for i in 0..boundary.len() {
            coords_2d.push(thetas[i]);
            coords_2d.push(axials[i]);
        }

        let tri_indices = cdt::cdt_triangulate_flat(&coords_2d, &[]).unwrap_or_default();
        if tri_indices.is_empty() {
            return;
        }

        let base_vertex = out_verts.len() as u32 / 3;
        for &vi in &boundary {
            let pos = disc.positions[vi];
            out_verts.push(pos[0] as f32);
            out_verts.push(pos[1] as f32);
            out_verts.push(pos[2] as f32);
            let dp = v3_sub(pos, origin);
            let along = v3_dot(dp, axis);
            let rad = [
                dp[0] - along * axis[0],
                dp[1] - along * axis[1],
                dp[2] - along * axis[2],
            ];
            let rlen = v3_length(rad);
            if rlen > TAU_NORMALIZE {
                out_normals.push((normal_sign * rad[0] / rlen) as f32);
                out_normals.push((normal_sign * rad[1] / rlen) as f32);
                out_normals.push((normal_sign * rad[2] / rlen) as f32);
            } else {
                out_normals.push(0.0);
                out_normals.push(0.0);
                out_normals.push(1.0);
            }
        }

        for &ti in &tri_indices {
            out_indices.push(base_vertex + ti as u32);
        }
    }
}

/// Tessellate a solid using boundary-constrained (edge-first) tessellation.
/// Weld vertices that share the same position and have similar normals.
///
/// Adjacent cylindrical side faces produce separate vertex instances at shared
/// edge positions. Without welding, the GPU sees distinct vertices and renders
/// hard edges between quads. Merging vertices whose positions and normals match
/// (within tolerance) enables smooth normal interpolation across face boundaries.
fn weld_smooth_vertices(vertices: &[f32], normals: &[f32], indices: &mut [u32]) {
    if vertices.is_empty() || indices.is_empty() {
        return;
    }
    let n_verts = vertices.len() / 3;

    // Quantize positions to a grid for grouping
    let max_abs = vertices
        .iter()
        .map(|v| v.abs())
        .fold(0.0_f32, f32::max)
        .max(crate::units::MIN_FEATURE_SIZE as f32);
    let grid =
        (max_abs as f64 * crate::units::TAU_TESS_GRID_FACTOR).max(crate::units::TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    // Group vertices by quantized position
    let mut pos_groups: BTreeMap<(i64, i64, i64), Vec<usize>> = BTreeMap::new();
    for i in 0..n_verts {
        let qx = (vertices[i * 3] as f64 * inv_grid).round() as i64;
        let qy = (vertices[i * 3 + 1] as f64 * inv_grid).round() as i64;
        let qz = (vertices[i * 3 + 2] as f64 * inv_grid).round() as i64;
        pos_groups.entry((qx, qy, qz)).or_default().push(i);
    }

    // Build remap: for each group of co-located vertices with similar normals,
    // point all to the first one.
    let mut remap: Vec<u32> = (0..n_verts as u32).collect();
    let normal_tol = crate::units::COS_NORMAL_SIMILARITY;

    for group in pos_groups.values() {
        if group.len() < 2 {
            continue;
        }
        // Within each position group, cluster by normal similarity
        let mut merged = vec![false; group.len()];
        for i in 0..group.len() {
            if merged[i] {
                continue;
            }
            let vi = group[i];
            let ni = [normals[vi * 3], normals[vi * 3 + 1], normals[vi * 3 + 2]];
            for j in (i + 1)..group.len() {
                if merged[j] {
                    continue;
                }
                let vj = group[j];
                let nj = [normals[vj * 3], normals[vj * 3 + 1], normals[vj * 3 + 2]];
                // Check if normals are similar (dot product close to 1)
                let dot = ni[0] * nj[0] + ni[1] * nj[1] + ni[2] * nj[2];
                if dot > 1.0 - normal_tol {
                    remap[vj] = vi as u32;
                    merged[j] = true;
                }
            }
        }
    }

    // Apply remap to indices
    for idx in indices.iter_mut() {
        *idx = remap[*idx as usize];
    }
}

// ── PR-Y36 inverse-direction probe (INFRA-CLASS, env-gated, additive) ──
//
// Maps each F0020 final-mesh unpaired edge back to its source-face's D.1
// sub-mechanism (per PR-Y28 §1 classification). Env-gated on
// `Y36_INVERSE_PROBE=1`; output directory from `Y36_INVERSE_PROBE_DIR`.
// Default-off path is byte-identical to pre-PR-Y36 — all probe logic gated
// behind a single `bool` flag.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Y36Class {
    D1a,     // boundary.len() < 3 planar entry gate
    D1b,     // 3-vertex earcut returned empty (coincident verts)
    D1c,     // all-NMM boundary (>=90% NMM)
    D1d,     // 3-vert clean boundary lost between F.0 and F.4 (repair-pass drop)
    Other,   // none of the above patterns — pre-PR-Y37 catch-all (sub-classified in writer)
    // PR-Y37 sub-classifications of Other (computed in writer with edge-level data):
    OtherH1, // sub-grid seam dominant (≥80% boundary edges axis-aligned + grid-quantized)
    OtherH2, // NMM-pair render asymmetric dominant (≥50% NMM edges with topology-present-but-render-absent twin)
    OtherH3, // residual: Other that fits neither H1 nor H2
}

impl Y36Class {
    fn as_str(self) -> &'static str {
        match self {
            Y36Class::D1a => "D1a",
            Y36Class::D1b => "D1b",
            Y36Class::D1c => "D1c",
            Y36Class::D1d => "D1d",
            Y36Class::Other => "OTHER",
            Y36Class::OtherH1 => "OtherH1",
            Y36Class::OtherH2 => "OtherH2",
            Y36Class::OtherH3 => "OtherH3",
        }
    }
}

#[derive(Debug, Clone)]
struct Y36ProbeFaceInfo {
    kid: u64,
    face_idx: usize,
    geom: String,
    outer_he_count: usize,
    outer_nmm_count: usize,
    is_self_loop: bool,
    outer_boundary_len: usize,
    inner_loop_count: usize,
    indices_emitted: u32, // at dispatch exit (F.0-1)
    face_range_pushed: bool,
    boundary_positions: Vec<[f64; 3]>, // ordered, from disc.positions
}

type Y36QPos = (i64, i64, i64);

fn y36_probe_enabled() -> bool {
    std::env::var("Y36_INVERSE_PROBE").as_deref() == Ok("1")
}

fn y36_classify(info: &Y36ProbeFaceInfo, dropped_between_f0_and_f4: bool) -> Y36Class {
    // D.1a: boundary.len() < 3 planar entry gate (PR-Y28 §1.1)
    // self-loop or 2-HE cycle, never emits triangles in planar arm
    if info.outer_boundary_len < 3 {
        return Y36Class::D1a;
    }
    // D.1c: high-NMM boundary (PR-Y28 §1.2)
    // Threshold: >=90% NMM (PR-Y28 observed 12/12 = 100% and 4/4 = 100%)
    if info.outer_he_count > 0 {
        let nmm_frac = info.outer_nmm_count as f64 / info.outer_he_count as f64;
        if nmm_frac >= 0.9 {
            return Y36Class::D1c;
        }
    }
    // D.1b: face emitted zero indices at dispatch exit despite passing the
    // n<3 gate (earcut-empty for coincident/degenerate vertices)
    if !info.face_range_pushed && info.outer_boundary_len >= 3 {
        return Y36Class::D1b;
    }
    // D.1d: emitted at dispatch, lost in repair (F.0 -> F.4)
    if info.face_range_pushed && dropped_between_f0_and_f4 {
        return Y36Class::D1d;
    }
    Y36Class::Other
}

// ── PR-Y37 sub-classification of Y36Class::Other into H1/H2/H3 ──────────────
//
// H1 (sub-grid seam mismatch): face boundary tracks tessellation grid edges.
//     Signature: ≥80% of boundary edges are axis-aligned (exactly one of the
//     quantized-delta components is non-zero). Maps to PR-Y27 D.2 cohort.
//
// H2 (NMM-pair render asymmetry): face's NMM-edge boundary segments lack a
//     peer face in the per-face boundary inventory (the would-be twin face
//     either was never dispatched or doesn't host the same edge). Maps to
//     PR-Y27 D.3 cohort.
//
// H3 (residual): neither H1 nor H2.
//
// Precedence: geometric H1 first (cheap test), then topological H2, then H3.

const Y37_H1_THRESHOLD: f64 = 0.80;
const Y37_H2_THRESHOLD: f64 = 0.50;

/// True iff exactly one component of (b - a) is non-zero at quantization granularity.
/// "Axis-aligned" means the edge runs parallel to one of the X/Y/Z axes.
fn y37_edge_axis_aligned(a: Y36QPos, b: Y36QPos) -> bool {
    let dx = (b.0 - a.0).abs();
    let dy = (b.1 - a.1).abs();
    let dz = (b.2 - a.2).abs();
    let n_nonzero = (dx > 0) as u32 + (dy > 0) as u32 + (dz > 0) as u32;
    n_nonzero == 1
}

/// Count of boundary edges that are axis-aligned (and trivially grid-quantized
/// since the endpoints are already quantized to integer grid cells).
fn y37_count_axis_aligned_edges(
    boundary_positions: &[[f64; 3]],
    inv_grid: f64,
) -> usize {
    let n = boundary_positions.len();
    if n < 2 {
        return 0;
    }
    let mut aligned = 0usize;
    for i in 0..n {
        let p0 = boundary_positions[i];
        let p1 = boundary_positions[(i + 1) % n];
        let q0 = y36_quantize_pos(p0, inv_grid);
        let q1 = y36_quantize_pos(p1, inv_grid);
        if q0 == q1 {
            continue; // degenerate; not a useful signal either way
        }
        if y37_edge_axis_aligned(q0, q1) {
            aligned += 1;
        }
    }
    aligned
}

/// Proxy: for each NMM-incident boundary edge segment of this face, count
/// segments whose quantized edge is NOT shared by any other face in the
/// per-face boundary inventory (`face_boundary_edges`). "NMM-incident" is
/// approximated by sampling: since the face has `outer_nmm_count` NMM HEs
/// but boundary positions don't carry per-segment NMM flags, we instead use
/// the overall face NMM ratio as a weight, and count edges where the peer
/// face's boundary is missing from the inventory altogether.
///
/// Returns (asymmetric_count, total_nmm_estimate). asymmetric_count is the
/// number of boundary edge SEGMENTS for which the edge appears uniquely in
/// this face's boundary (no peer face in the inventory hosts it). Used in
/// conjunction with face-level NMM count to compute the H2 threshold.
fn y37_count_nmm_asymmetric(
    info: &Y36ProbeFaceInfo,
    inv_grid: f64,
    face_boundary_edges: &BTreeMap<(Y36QPos, Y36QPos), Vec<u64>>,
    kids_in_final: &std::collections::HashSet<u64>,
) -> usize {
    if info.outer_nmm_count == 0 {
        return 0;
    }
    let n = info.boundary_positions.len();
    if n < 2 {
        return 0;
    }
    let mut asym = 0usize;
    for i in 0..n {
        let p0 = info.boundary_positions[i];
        let p1 = info.boundary_positions[(i + 1) % n];
        let q0 = y36_quantize_pos(p0, inv_grid);
        let q1 = y36_quantize_pos(p1, inv_grid);
        if q0 == q1 {
            continue;
        }
        let edge = y36_canonical_edge(q0, q1);
        let candidates = face_boundary_edges.get(&edge);
        // "asymmetric" = this edge is unique to this face (no other face in
        // the per-face inventory has the same quantized edge), OR all peers
        // are dropped (not in final mesh). Both conditions = "topology says
        // I should have a peer, but no peer's render produced this edge."
        let has_peer_in_final = match candidates {
            Some(v) => v.iter().any(|&kid| kid != info.kid && kids_in_final.contains(&kid)),
            None => false,
        };
        if !has_peer_in_final {
            asym += 1;
        }
    }
    asym
}

/// Sub-classify a `Y36Class::Other` face into OtherH1/OtherH2/OtherH3.
/// Returns input class unchanged if the input is not `Other`.
fn y37_sub_classify(
    base: Y36Class,
    info: &Y36ProbeFaceInfo,
    inv_grid: f64,
    face_boundary_edges: &BTreeMap<(Y36QPos, Y36QPos), Vec<u64>>,
    kids_in_final: &std::collections::HashSet<u64>,
) -> (Y36Class, usize, usize) {
    if base != Y36Class::Other {
        return (base, 0, 0);
    }
    let n = info.boundary_positions.len();
    let aligned = y37_count_axis_aligned_edges(&info.boundary_positions, inv_grid);
    let asym = y37_count_nmm_asymmetric(info, inv_grid, face_boundary_edges, kids_in_final);
    // H1 precedence: geometric grid-alignment dominant.
    if n >= 2 {
        let grid_frac = aligned as f64 / n as f64;
        if grid_frac >= Y37_H1_THRESHOLD {
            return (Y36Class::OtherH1, aligned, asym);
        }
    }
    // H2: NMM-edge asymmetry. Count vs face-level NMM HE count, not vs total
    // boundary edges, since H2 is fundamentally about NMM-edges' twin loss.
    if info.outer_nmm_count > 0 {
        let asym_frac = asym as f64 / info.outer_nmm_count as f64;
        if asym_frac >= Y37_H2_THRESHOLD {
            return (Y36Class::OtherH2, aligned, asym);
        }
    }
    (Y36Class::OtherH3, aligned, asym)
}


fn y36_quantize_pos(pos: [f64; 3], inv_grid: f64) -> Y36QPos {
    (
        (pos[0] * inv_grid).round() as i64,
        (pos[1] * inv_grid).round() as i64,
        (pos[2] * inv_grid).round() as i64,
    )
}

fn y36_quantize_vert(vertices: &[f32], idx: u32, inv_grid: f64) -> Y36QPos {
    let i = idx as usize * 3;
    if i + 2 >= vertices.len() {
        return (0, 0, 0);
    }
    (
        (vertices[i] as f64 * inv_grid).round() as i64,
        (vertices[i + 1] as f64 * inv_grid).round() as i64,
        (vertices[i + 2] as f64 * inv_grid).round() as i64,
    )
}

fn y36_canonical_edge(a: Y36QPos, b: Y36QPos) -> (Y36QPos, Y36QPos) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Build the oracle-compatible quantization grid for the given f32 mesh.
fn y36_inv_grid(vertices: &[f32]) -> f64 {
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    1.0 / grid
}

/// PR-Y36 invocation counter (per-thread). Used to disambiguate per-invocation
/// TSV files within one spotlight (F0020 has 6 invocations).
std::thread_local! {
    static Y36_INVOCATION_COUNTER: std::cell::RefCell<u64> =
        const { std::cell::RefCell::new(0) };
}

fn y36_next_invocation() -> u64 {
    Y36_INVOCATION_COUNTER.with(|c| {
        let mut n = c.borrow_mut();
        *n += 1;
        *n
    })
}

/// Write inverse-attribution TSV for one `tessellate_solid_bounded` invocation.
/// Default-off when `Y36_INVERSE_PROBE != "1"`.
fn y36_write_inverse_attribution(
    faces: &[Y36ProbeFaceInfo],
    final_vertices: &[f32],
    final_indices: &[u32],
    final_face_ranges: &[FaceRange],
) {
    if !y36_probe_enabled() {
        return;
    }
    let invocation = y36_next_invocation();

    let dump_dir = match std::env::var("Y36_INVERSE_PROBE_DIR") {
        Ok(d) => d,
        Err(_) => return, // gate fires but no dir → no-op
    };
    let case = crate::boolean::yang_integration::current_case_id()
        .unwrap_or_else(|| format!("seq_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dump_dir);

    let inv_grid = y36_inv_grid(final_vertices);
    let n_tris = final_indices.len() / 3;

    // Build edge -> count map and edge -> incident triangle map for unpaired edges
    let mut edge_counts: BTreeMap<(Y36QPos, Y36QPos), usize> = BTreeMap::new();
    let mut edge_tris: BTreeMap<(Y36QPos, Y36QPos), Vec<usize>> = BTreeMap::new();
    let mut tri_qverts: Vec<[Y36QPos; 3]> = Vec::with_capacity(n_tris);
    for t in 0..n_tris {
        let base = t * 3;
        let qa = y36_quantize_vert(final_vertices, final_indices[base], inv_grid);
        let qb = y36_quantize_vert(final_vertices, final_indices[base + 1], inv_grid);
        let qc = y36_quantize_vert(final_vertices, final_indices[base + 2], inv_grid);
        tri_qverts.push([qa, qb, qc]);
        if qa == qb || qb == qc || qa == qc {
            continue;
        }
        for e in 0..3 {
            let edge = y36_canonical_edge(tri_qverts[t][e], tri_qverts[t][(e + 1) % 3]);
            *edge_counts.entry(edge).or_insert(0) += 1;
            edge_tris.entry(edge).or_insert_with(Vec::new).push(t);
        }
    }

    // Triangle -> face_id mapping from face_ranges
    let tri_face_id = |t: usize| -> u64 {
        let i3 = (t * 3) as u32;
        final_face_ranges
            .iter()
            .find(|fr| fr.start_index <= i3 && i3 < fr.end_index)
            .map(|fr| fr.face_id.0)
            .unwrap_or(0)
    };

    // Final mesh's surviving kids (for dropped-detection)
    let kids_in_final: std::collections::HashSet<u64> =
        final_face_ranges.iter().map(|fr| fr.face_id.0).collect();

    // Quantize each captured face's boundary edges into the same grid
    // and build a reverse map: edge -> kid
    let mut face_boundary_edges: BTreeMap<(Y36QPos, Y36QPos), Vec<u64>> = BTreeMap::new();
    let mut face_boundary_qedge_set: BTreeMap<u64, std::collections::HashSet<(Y36QPos, Y36QPos)>> =
        BTreeMap::new();
    for info in faces {
        let bn = info.boundary_positions.len();
        if bn < 2 {
            continue;
        }
        let mut set = std::collections::HashSet::new();
        for i in 0..bn {
            let p0 = info.boundary_positions[i];
            let p1 = info.boundary_positions[(i + 1) % bn];
            let q0 = y36_quantize_pos(p0, inv_grid);
            let q1 = y36_quantize_pos(p1, inv_grid);
            if q0 == q1 {
                continue;
            }
            let edge = y36_canonical_edge(q0, q1);
            set.insert(edge);
            face_boundary_edges
                .entry(edge)
                .or_insert_with(Vec::new)
                .push(info.kid);
        }
        face_boundary_qedge_set.insert(info.kid, set);
    }

    let face_by_kid: BTreeMap<u64, &Y36ProbeFaceInfo> =
        faces.iter().map(|f| (f.kid, f)).collect();

    // Build TSV rows for unpaired edges (count != 2)
    // Header: unpaired_edge_id, v0_x, v0_y, v0_z, v1_x, v1_y, v1_z,
    //         kept_face_id, attributed_source_face_id, classification,
    //         outer_boundary_len, outer_he_count, outer_nmm_count,
    //         nmm_pct, was_dropped_in_repair, edge_count,
    //         // PR-Y37 additions:
    //         grid_aligned_count, grid_aligned_pct, nmm_asym_count, nmm_asym_pct
    let mut rows: Vec<String> = Vec::new();
    let mut tally: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut unpaired_idx: u32 = 0;
    for (edge, &count) in edge_counts.iter() {
        if count == 2 {
            continue;
        }
        let v0 = edge.0;
        let v1 = edge.1;
        // Use grid-space coordinates for repro; convert back to model space
        let grid = 1.0 / inv_grid;
        let v0m = [v0.0 as f64 * grid, v0.1 as f64 * grid, v0.2 as f64 * grid];
        let v1m = [v1.0 as f64 * grid, v1.1 as f64 * grid, v1.2 as f64 * grid];

        // Lone incident triangle's face_id (kept side)
        let tris = edge_tris.get(edge).cloned().unwrap_or_default();
        let kept_face_id = tris.first().map(|&t| tri_face_id(t)).unwrap_or(0);

        // Attribution: prefer a DROPPED face whose boundary contains this edge.
        // If multiple candidates, pick the one NOT in final mesh.
        let candidates = face_boundary_edges.get(edge).cloned().unwrap_or_default();
        let mut attributed_kid: u64 = 0;
        let mut class: Y36Class = Y36Class::Other;
        // First pass: prefer dropped (not in final mesh) candidates
        for cand in &candidates {
            if !kids_in_final.contains(cand) {
                attributed_kid = *cand;
                if let Some(info) = face_by_kid.get(cand) {
                    let dropped_in_repair = info.face_range_pushed
                        && !kids_in_final.contains(&info.kid);
                    class = y36_classify(info, dropped_in_repair);
                }
                break;
            }
        }
        // Fallback: if no dropped candidate matched, attribute to the kept
        // face that owns this unpaired edge (means defect is on the kept
        // side — e.g., F0044 D.2 sub-grid seam mismatch)
        if attributed_kid == 0 {
            attributed_kid = kept_face_id;
            if let Some(info) = face_by_kid.get(&kept_face_id) {
                let dropped_in_repair = info.face_range_pushed
                    && !kids_in_final.contains(&info.kid);
                class = y36_classify(info, dropped_in_repair);
                // If the kept face's classification is Other but it is fully
                // present in the final mesh, leave as Other (genuine non-D.1
                // mechanism)
            }
        }

        let attr_info = face_by_kid.get(&attributed_kid);
        let (outer_boundary_len, outer_he_count, outer_nmm_count, was_dropped_repair) =
            match attr_info {
                Some(info) => {
                    let dropped = info.face_range_pushed && !kids_in_final.contains(&info.kid);
                    (
                        info.outer_boundary_len,
                        info.outer_he_count,
                        info.outer_nmm_count,
                        dropped,
                    )
                }
                None => (0, 0, 0, false),
            };
        let nmm_pct = if outer_he_count > 0 {
            outer_nmm_count as f64 / outer_he_count as f64 * 100.0
        } else {
            0.0
        };

        // PR-Y37: sub-classify Other rows into H1/H2/H3 using edge-level features.
        let (final_class, grid_aligned_cnt, nmm_asym_cnt) = match attr_info {
            Some(info) => {
                let (sub, ga, asym) = y37_sub_classify(
                    class,
                    info,
                    inv_grid,
                    &face_boundary_edges,
                    &kids_in_final,
                );
                (sub, ga, asym)
            }
            None => (class, 0usize, 0usize),
        };
        let bn = attr_info.map(|i| i.boundary_positions.len()).unwrap_or(0);
        let grid_aligned_pct = if bn > 0 {
            grid_aligned_cnt as f64 / bn as f64 * 100.0
        } else {
            0.0
        };
        let nmm_asym_pct = if outer_nmm_count > 0 {
            nmm_asym_cnt as f64 / outer_nmm_count as f64 * 100.0
        } else {
            0.0
        };

        *tally.entry(final_class.as_str()).or_insert(0) += 1;

        rows.push(format!(
            "{}\t{:.6e}\t{:.6e}\t{:.6e}\t{:.6e}\t{:.6e}\t{:.6e}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{}\t{}\t{}\t{:.1}\t{}\t{:.1}",
            unpaired_idx,
            v0m[0], v0m[1], v0m[2],
            v1m[0], v1m[1], v1m[2],
            kept_face_id,
            attributed_kid,
            final_class.as_str(),
            outer_boundary_len,
            outer_he_count,
            outer_nmm_count,
            nmm_pct,
            was_dropped_repair,
            count,
            grid_aligned_cnt,
            grid_aligned_pct,
            nmm_asym_cnt,
            nmm_asym_pct,
        ));
        unpaired_idx += 1;
    }

    // Build per-face inventory rows (for debugging methodology / Gate 5)
    let mut face_rows: Vec<String> = Vec::new();
    for info in faces {
        let in_final = kids_in_final.contains(&info.kid);
        let dropped_repair = info.face_range_pushed && !in_final;
        let base_class = y36_classify(info, dropped_repair);
        let (final_class, grid_aligned_cnt, nmm_asym_cnt) = y37_sub_classify(
            base_class,
            info,
            inv_grid,
            &face_boundary_edges,
            &kids_in_final,
        );
        let nmm_pct = if info.outer_he_count > 0 {
            info.outer_nmm_count as f64 / info.outer_he_count as f64 * 100.0
        } else {
            0.0
        };
        let bn = info.boundary_positions.len();
        let grid_aligned_pct = if bn > 0 {
            grid_aligned_cnt as f64 / bn as f64 * 100.0
        } else {
            0.0
        };
        let nmm_asym_pct = if info.outer_nmm_count > 0 {
            nmm_asym_cnt as f64 / info.outer_nmm_count as f64 * 100.0
        } else {
            0.0
        };
        face_rows.push(format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{}\t{}\t{:.1}\t{}\t{:.1}",
            info.kid,
            info.face_idx,
            info.geom,
            info.outer_he_count,
            info.outer_nmm_count,
            info.is_self_loop,
            info.outer_boundary_len,
            info.inner_loop_count,
            info.indices_emitted,
            info.face_range_pushed,
            nmm_pct,
            final_class.as_str(),
            grid_aligned_cnt,
            grid_aligned_pct,
            nmm_asym_cnt,
            nmm_asym_pct,
        ));
    }

    let unpaired_path = format!(
        "{}/{}_inv{:03}_inverse_attribution.tsv",
        dump_dir, case, invocation
    );
    let faces_path = format!(
        "{}/{}_inv{:03}_face_inventory.tsv",
        dump_dir, case, invocation
    );

    let unpaired_header =
        "unpaired_edge_id\tv0_x\tv0_y\tv0_z\tv1_x\tv1_y\tv1_z\tkept_face_id\tattributed_source_face_id\tclassification\touter_boundary_len\touter_he_count\touter_nmm_count\tnmm_pct\twas_dropped_in_repair\tedge_count\tgrid_aligned_count\tgrid_aligned_pct\tnmm_asym_count\tnmm_asym_pct";
    let faces_header = "kid\tface_idx\tgeom\touter_he_count\touter_nmm_count\tis_self_loop\touter_boundary_len\tinner_loop_count\tindices_emitted_dispatch\tface_range_pushed\tnmm_pct\tclassification\tgrid_aligned_count\tgrid_aligned_pct\tnmm_asym_count\tnmm_asym_pct";
    let _ = crate::boolean::yang_integration::dump_labels_as_csv(
        unpaired_header,
        &rows,
        &unpaired_path,
    );
    let _ = crate::boolean::yang_integration::dump_labels_as_csv(
        faces_header,
        &face_rows,
        &faces_path,
    );

    let total = rows.len();
    let d1a = tally.get("D1a").copied().unwrap_or(0);
    let d1b = tally.get("D1b").copied().unwrap_or(0);
    let d1c = tally.get("D1c").copied().unwrap_or(0);
    let d1d = tally.get("D1d").copied().unwrap_or(0);
    let other_legacy = tally.get("OTHER").copied().unwrap_or(0); // should now be 0 with sub-class
    let other_h1 = tally.get("OtherH1").copied().unwrap_or(0);
    let other_h2 = tally.get("OtherH2").copied().unwrap_or(0);
    let other_h3 = tally.get("OtherH3").copied().unwrap_or(0);
    eprintln!(
        "[y36-inverse-probe] case={} inv#{} total_unpaired={} D1a={} D1b={} D1c={} D1d={} OTHER={} OtherH1={} OtherH2={} OtherH3={} wrote={}",
        case, invocation, total, d1a, d1b, d1c, d1d, other_legacy,
        other_h1, other_h2, other_h3, unpaired_path
    );

    // PR-Y37 cross-cohort aggregator: append one row per (case, invocation) to
    // a shared TSV in dump_dir. Append-mode so multiple invocations across
    // cases/runs accumulate. Header is written only if file is empty.
    let summary_path = format!("{}/cross_cohort_summary.tsv", dump_dir);
    let summary_header = "case\tinvocation\ttotal_unpaired\tD1a\tD1b\tD1c\tD1d\tOTHER_legacy\tOtherH1\tOtherH2\tOtherH3\tD1_total\tOther_total\tH1_pct_of_other\tH2_pct_of_other\tH3_pct_of_other";
    let d1_total = d1a + d1b + d1c + d1d;
    let other_total = other_legacy + other_h1 + other_h2 + other_h3;
    let h1_pct = if other_total > 0 {
        other_h1 as f64 / other_total as f64 * 100.0
    } else {
        0.0
    };
    let h2_pct = if other_total > 0 {
        other_h2 as f64 / other_total as f64 * 100.0
    } else {
        0.0
    };
    let h3_pct = if other_total > 0 {
        other_h3 as f64 / other_total as f64 * 100.0
    } else {
        0.0
    };
    let row = format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{:.1}",
        case, invocation, total, d1a, d1b, d1c, d1d, other_legacy,
        other_h1, other_h2, other_h3, d1_total, other_total, h1_pct, h2_pct, h3_pct,
    );
    use std::io::Write;
    let header_needed = std::fs::metadata(&summary_path)
        .map(|m| m.len() == 0)
        .unwrap_or(true);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&summary_path)
    {
        if header_needed {
            let _ = writeln!(f, "{}", summary_header);
        }
        let _ = writeln!(f, "{}", row);
    }
}

// ── PR-Y41 dispatch-loop emission probe (INFRA-CLASS, env-gated, additive) ──
//
// Captures per-call data from `tessellate_planar_face_bounded` (entry +
// exit) so the parent driver can attach kid/face_id and emit a TSV. The
// load-bearing measurement is per-face `indices_emitted` and per-emitted-
// triangle quantization classification (distinct / single-vert collision /
// fully-degenerate), enabling Gate 4's 18-index accounting and Gate 5's
// degenerate-quantization check.

#[derive(Debug, Clone)]
struct Y41DispatchRecord {
    dispatch_type: &'static str,
    boundary_size: usize,
    inner_count: usize,
    indices_emitted: usize,
    distinct_quantized_tris: usize,
    degenerate_collapse_count: usize,
    single_vert_collision_count: usize,
    boundary_positions: Vec<[f64; 3]>,
}

std::thread_local! {
    static Y41_DISPATCH_BUFFER: std::cell::RefCell<Vec<Y41DispatchRecord>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static Y41_INVOCATION_COUNTER: std::cell::RefCell<u64> =
        const { std::cell::RefCell::new(0) };
}

fn y41_probe_enabled() -> bool {
    std::env::var("Y41_DISPATCH_PROBE").as_deref() == Ok("1")
}

fn y41_push_record(rec: Y41DispatchRecord) {
    Y41_DISPATCH_BUFFER.with(|b| b.borrow_mut().push(rec));
}

fn y41_take_records() -> Vec<Y41DispatchRecord> {
    Y41_DISPATCH_BUFFER.with(|b| std::mem::take(&mut *b.borrow_mut()))
}

fn y41_next_invocation() -> u64 {
    Y41_INVOCATION_COUNTER.with(|c| {
        let mut n = c.borrow_mut();
        *n += 1;
        *n
    })
}

fn y41_inv_grid_from_verts(out_verts: &[f32]) -> f64 {
    let max_abs = out_verts.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    1.0 / grid
}

fn y41_quantize_f32_vert(out_verts: &[f32], vert_idx: usize, inv_grid: f64) -> (i64, i64, i64) {
    let i = vert_idx * 3;
    if i + 2 >= out_verts.len() {
        return (0, 0, 0);
    }
    (
        (out_verts[i] as f64 * inv_grid).round() as i64,
        (out_verts[i + 1] as f64 * inv_grid).round() as i64,
        (out_verts[i + 2] as f64 * inv_grid).round() as i64,
    )
}

/// Pair each Y41 dispatch record with the kid/face_idx from the parent
/// driver's sorted_faces traversal and emit a per-invocation TSV.
fn y41_write_dispatch_tsv(records: Vec<(u64, usize, Y41DispatchRecord)>) {
    if !y41_probe_enabled() {
        return;
    }
    let invocation = y41_next_invocation();

    let dump_dir = match std::env::var("Y41_DISPATCH_PROBE_DIR") {
        Ok(d) => d,
        Err(_) => return,
    };
    let case = crate::boolean::yang_integration::current_case_id()
        .unwrap_or_else(|| format!("seq_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dump_dir);

    let path = format!(
        "{}/{}_inv{:03}_dispatch.tsv",
        dump_dir, case, invocation
    );

    let mut rows: Vec<String> = Vec::new();
    for (kid, face_idx, rec) in &records {
        rows.push(format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            kid,
            face_idx,
            rec.dispatch_type,
            rec.boundary_size,
            rec.inner_count,
            rec.indices_emitted,
            rec.distinct_quantized_tris,
            rec.degenerate_collapse_count,
            rec.single_vert_collision_count,
        ));
    }
    let header =
        "kid\tface_idx\tdispatch_type\tboundary_size\tinner_count\tindices_emitted\tdistinct_quantized_tris\tdegenerate_collapse_count\tsingle_vert_collision_count";
    let _ = crate::boolean::yang_integration::dump_labels_as_csv(header, &rows, &path);

    // F0020 18-index accounting summary: sum kids 218/232/233 indices_emitted.
    let mut d1d_sum: usize = 0;
    let mut d1d_kid_breakdown: Vec<(u64, usize, usize, usize, usize)> = Vec::new();
    for (kid, _face_idx, rec) in &records {
        if *kid == 218 || *kid == 232 || *kid == 233 {
            d1d_sum += rec.indices_emitted;
            d1d_kid_breakdown.push((
                *kid,
                rec.indices_emitted,
                rec.distinct_quantized_tris,
                rec.degenerate_collapse_count,
                rec.single_vert_collision_count,
            ));
        }
    }
    let mut sum_path = path.clone();
    sum_path = sum_path.replace("_dispatch.tsv", "_d1d_summary.tsv");
    let mut sum_rows: Vec<String> = Vec::new();
    for (kid, idx_em, distinct, degen, single) in &d1d_kid_breakdown {
        sum_rows.push(format!(
            "{}\t{}\t{}\t{}\t{}",
            kid, idx_em, distinct, degen, single
        ));
    }
    sum_rows.push(format!(
        "TOTAL_D1D_INDICES\t{}\tDISTINCT_TRIS\tDEGEN_TRIS\tSINGLE_COLL_TRIS",
        d1d_sum
    ));
    let sum_header =
        "kid\tindices_emitted\tdistinct_quantized_tris\tdegenerate_collapse_count\tsingle_vert_collision_count";
    let _ =
        crate::boolean::yang_integration::dump_labels_as_csv(sum_header, &sum_rows, &sum_path);

    eprintln!(
        "[y41-dispatch-probe] case={} inv#{} faces={} d1d_kids_present={} d1d_indices_total={} wrote={}",
        case,
        invocation,
        records.len(),
        d1d_kid_breakdown.len(),
        d1d_sum,
        path
    );
}

/// Used for boolean results where CylinderParams/RevolveParams are unavailable.
///
/// For analytical B-Rep: watertight by construction (shared vertices from
/// discretized edges). Minimal post-processing.
/// D1 Tier 2a probe — env-gated by Y47T2_INTERSECTION_PROBE. Walks each
/// boundary loop's half-edges directly (giving exact `EdgeIdx` per segment)
/// and reports which edges are flagged `is_intersection`. Default-off
/// byte-identical: only fires when the env var is set and only emits to stderr.
fn y47t2_dump_boundary_intersections(
    face_idx: FaceIdx,
    arena: &TopoArena,
    _disc: &EdgeDiscretization,
) {
    let probe_marker = EDGE_IS_INTERSECTION_PROBE.with(|c| c.borrow().clone());
    let total_marked = probe_marker.values().filter(|&&v| v).count();

    let face = &arena.faces[face_idx.0];
    let mut walked: Vec<EdgeIdx> = Vec::new();

    let walk = |start: HalfEdgeIdx, out: &mut Vec<EdgeIdx>| {
        let mut he = start;
        loop {
            out.push(arena.half_edges[he.0].edge);
            he = arena.half_edges[he.0].next;
            if he == start {
                break;
            }
        }
    };
    walk(arena.loops[face.outer_loop.0].half_edge, &mut walked);
    for &inner_loop in &face.inner_loops {
        walk(arena.loops[inner_loop.0].half_edge, &mut walked);
    }
    let walked_intersection = walked
        .iter()
        .filter(|e| probe_marker.get(e).copied().unwrap_or(false))
        .count();
    eprintln!(
        "[y47t2] face={:?} walked_edges={} walked_intersection={} arena_edges={} flagged_total={}",
        face_idx,
        walked.len(),
        walked_intersection,
        arena.edges.len(),
        total_marked,
    );
}

/// D1 Tier 2a "where do flagged edges live" deep probe — env-gated by
/// Y47T2_FLAGGED_LOCATIONS. Once per tessellation, dump every flagged
/// `EdgeIdx`: the half-edges that reference it, which face's loop those
/// half-edges belong to (if any), and the edge's origin/destination
/// vertex positions.
fn y47t2_dump_flagged_locations(arena: &TopoArena, _disc: &EdgeDiscretization) {
    let probe_marker = EDGE_IS_INTERSECTION_PROBE.with(|c| c.borrow().clone());
    let flagged: Vec<EdgeIdx> = probe_marker
        .iter()
        .filter(|(_, &v)| v)
        .map(|(&k, _)| k)
        .collect();
    if flagged.is_empty() {
        return;
    }

    // Build half-edge → loop_idx map by walking each loop and recording its half-edges.
    let mut he_to_loop: BTreeMap<HalfEdgeIdx, LoopIdx> = BTreeMap::new();
    for (loop_i, l) in arena.loops.iter().enumerate() {
        let start = l.half_edge;
        let mut he = start;
        loop {
            he_to_loop.insert(he, LoopIdx(loop_i));
            he = arena.half_edges[he.0].next;
            if he == start {
                break;
            }
        }
    }

    // Build loop_idx → face_idx map.
    let mut loop_to_face: BTreeMap<LoopIdx, FaceIdx> = BTreeMap::new();
    for (face_i, f) in arena.faces.iter().enumerate() {
        loop_to_face.insert(f.outer_loop, FaceIdx(face_i));
        for &inner_loop in &f.inner_loops {
            loop_to_face.insert(inner_loop, FaceIdx(face_i));
        }
    }

    eprintln!(
        "[y47t2-flagged] {} flagged edges in arena (total edges={}, half-edges={}, faces={}, loops={})",
        flagged.len(),
        arena.edges.len(),
        arena.half_edges.len(),
        arena.faces.len(),
        arena.loops.len(),
    );
    for &edge_idx in &flagged {
        let edge = &arena.edges[edge_idx.0];
        // Find all half-edges that reference this edge.
        let hes: Vec<HalfEdgeIdx> = arena
            .half_edges
            .iter()
            .enumerate()
            .filter(|(_, h)| h.edge == edge_idx)
            .map(|(i, _)| HalfEdgeIdx(i))
            .collect();
        let primary_he = edge.half_edge;
        let v_origin = arena.half_edges[primary_he.0].origin;
        let v_dest_opt = arena.half_edges[primary_he.0]
            .twin
            .map(|t| arena.half_edges[t.0].origin);
        // For each half-edge: which loop and face owns it?
        let mut owners: Vec<String> = Vec::new();
        for he in &hes {
            match he_to_loop.get(he) {
                Some(loop_idx) => {
                    let face = loop_to_face
                        .get(loop_idx)
                        .map(|f| format!("Face{}", f.0))
                        .unwrap_or_else(|| "(no face)".to_string());
                    owners.push(format!("he{}→loop{}→{}", he.0, loop_idx.0, face));
                }
                None => owners.push(format!("he{}→(no loop — orphan)", he.0)),
            }
        }
        eprintln!(
            "[y47t2-flagged]   edge{} origin=v{} dest=v{:?} half_edges={} owners=[{}]",
            edge_idx.0,
            v_origin.0,
            v_dest_opt.map(|v| v.0),
            hes.len(),
            owners.join(", ")
        );
    }
}

/// Walk every arena face's outer + inner loops, collect the 3D positions of
/// each origin vertex, count distinct positions (under a coarse quantization),
/// and flag faces whose entire boundary collapses to one 3D point — these are
/// zero-area faces that the deleted repair pipeline used to mask. Env-gated
/// by `Y48_FACE_DUMP=1`; default-off byte-identical.
fn y48_dump_face_boundaries(arena: &TopoArena, face_map: &BTreeMap<u64, FaceIdx>) {
    let mut idx_to_kid: BTreeMap<FaceIdx, u64> = BTreeMap::new();
    for (&kid, &fi) in face_map.iter() {
        idx_to_kid.insert(fi, kid);
    }

    let walk = |start_he: HalfEdgeIdx| -> Vec<(VertexIdx, [f64; 3])> {
        let mut out = Vec::new();
        let mut he = start_he;
        let mut steps: usize = 0;
        loop {
            let v = arena.half_edges[he.0].origin;
            let p = arena.vertices[v.0].position;
            out.push((v, p));
            he = arena.half_edges[he.0].next;
            steps += 1;
            if he == start_he {
                break;
            }
            if steps > 1_000_000 {
                break;
            }
        }
        out
    };

    let qkey = |p: [f64; 3]| -> (i64, i64, i64) {
        let s = 1.0e9_f64;
        (
            (p[0] * s).round() as i64,
            (p[1] * s).round() as i64,
            (p[2] * s).round() as i64,
        )
    };

    let mut total_zero_area = 0usize;
    eprintln!(
        "[y48-face-dump] arena_faces={} face_map_size={}",
        arena.faces.len(),
        face_map.len()
    );
    for (fi, face) in arena.faces.iter().enumerate() {
        let kid = idx_to_kid
            .get(&FaceIdx(fi))
            .map(|k| k.to_string())
            .unwrap_or_else(|| "?".to_string());

        let outer = walk(arena.loops[face.outer_loop.0].half_edge);
        let mut all_verts: Vec<(VertexIdx, [f64; 3])> = outer.clone();
        for &il in &face.inner_loops {
            let inner = walk(arena.loops[il.0].half_edge);
            all_verts.extend(inner);
        }

        let distinct_v: std::collections::BTreeSet<VertexIdx> =
            all_verts.iter().map(|(v, _)| *v).collect();
        let distinct_pos: std::collections::BTreeSet<(i64, i64, i64)> =
            all_verts.iter().map(|(_, p)| qkey(*p)).collect();
        let zero_area = distinct_pos.len() <= 1 && !all_verts.is_empty();
        if zero_area {
            total_zero_area += 1;
        }

        let flag = if zero_area { " ZERO_AREA" } else { "" };
        let first_pos = all_verts
            .first()
            .map(|(_, p)| format!("({:.6}, {:.6}, {:.6})", p[0], p[1], p[2]))
            .unwrap_or_else(|| "(empty)".to_string());
        eprintln!(
            "[y48-face-dump]   face_idx={} kid={} outer_loop_he={} outer_len={} inner_loops={} distinct_vidx={} distinct_pos={} first={}{}",
            fi,
            kid,
            arena.loops[face.outer_loop.0].half_edge.0,
            outer.len(),
            face.inner_loops.len(),
            distinct_v.len(),
            distinct_pos.len(),
            first_pos,
            flag
        );
        if zero_area {
            for (i, (v, p)) in all_verts.iter().enumerate() {
                eprintln!(
                    "[y48-face-dump]     v[{}]={} pos=({:.6}, {:.6}, {:.6})",
                    i, v.0, p[0], p[1], p[2]
                );
            }
        }
    }
    eprintln!(
        "[y48-face-dump] zero_area_faces_total={}",
        total_zero_area
    );
}

fn tessellate_solid_bounded(
    arena: &TopoArena,
    face_map: &BTreeMap<u64, FaceIdx>,
    face_geometry: &BTreeMap<FaceIdx, SurfaceGeom>,
    edge_geometry: &BTreeMap<EdgeIdx, CurveGeom>,
) -> Result<RenderMesh, KernelError> {
    let disc = discretize_edges(arena, edge_geometry);

    // D1 Tier 2a deep probe — once per solid, dump where the intersection-flagged
    // arena edges actually live (which half-edges reference them, which loops, which faces).
    if std::env::var("Y47T2_FLAGGED_LOCATIONS").is_ok() {
        y47t2_dump_flagged_locations(arena, &disc);
    }

    // Zero-area face probe — dump per-arena-face outer/inner loop vertex
    // positions, flag faces whose entire boundary collapses to one 3D point.
    // Anchors the "where are face_ids 246/250/261 born" investigation into
    // B-Rep assembly. Env-gated; default-off byte-identical.
    if std::env::var("Y48_FACE_DUMP").is_ok() {
        y48_dump_face_boundaries(arena, face_map);
    }

    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    // Sort face_map entries for deterministic tessellation order.
    let mut sorted_faces: Vec<(u64, FaceIdx)> = face_map.iter().map(|(&k, &v)| (k, v)).collect();
    sorted_faces.sort_by_key(|(k, _)| *k);

    // PR-Y36 inverse-direction probe (env-gated, default empty Vec).
    let y36_on = y36_probe_enabled();
    let mut y36_face_infos: Vec<Y36ProbeFaceInfo> = Vec::new();

    // PR-Y41 dispatch probe (env-gated, default-off).
    let y41_on = y41_probe_enabled();
    if y41_on {
        // Drain any stale records from prior invocations within this thread.
        let _ = y41_take_records();
    }
    let mut y41_attributed: Vec<(u64, usize, Y41DispatchRecord)> = Vec::new();

    for &(kid, face_idx) in sorted_faces.iter() {
        let start_index = indices.len() as u32;
        let y41_buf_before = if y41_on {
            Y41_DISPATCH_BUFFER.with(|b| b.borrow().len())
        } else {
            0
        };
        let geom = face_geometry.get(&face_idx);

        // PR-Y36: capture per-face properties BEFORE tessellate dispatch.
        // Computed only when probe is enabled; default-off path executes none
        // of this block.
        let y36_info_pre: Option<(String, Vec<usize>, usize, usize, usize, bool)> = if y36_on {
            let geom_label = match geom {
                Some(SurfaceGeom::Planar(_)) => "Planar".to_string(),
                Some(SurfaceGeom::Cylindrical(_)) => "Cylindrical".to_string(),
                Some(SurfaceGeom::Spherical(_)) => "Spherical".to_string(),
                Some(SurfaceGeom::Conical(_)) => "Conical".to_string(),
                Some(SurfaceGeom::Toroidal(_)) => "Toroidal".to_string(),
                _ => "Other".to_string(),
            };
            // Walk the outer loop to count HEs, NMM (twin=None), self-loop.
            let outer_loop_idx = arena.faces[face_idx.0].outer_loop;
            let start_he = arena.loops[outer_loop_idx.0].half_edge;
            let mut he = start_he;
            let mut he_count: usize = 0;
            let mut nmm_count: usize = 0;
            let is_self_loop = arena.half_edges[start_he.0].next == start_he;
            loop {
                he_count += 1;
                if arena.half_edges[he.0].twin.is_none() {
                    nmm_count += 1;
                }
                he = arena.half_edges[he.0].next;
                if he == start_he {
                    break;
                }
                // Defensive bound to avoid infinite loops on malformed arenas
                if he_count > 1_000_000 {
                    break;
                }
            }
            let outer_boundary = collect_loop_boundary(arena, outer_loop_idx, &disc);
            let inner_loop_count = arena.faces[face_idx.0].inner_loops.len();
            Some((
                geom_label,
                outer_boundary,
                he_count,
                nmm_count,
                inner_loop_count,
                is_self_loop,
            ))
        } else {
            None
        };

        match geom {
            Some(SurfaceGeom::Cylindrical(cyl)) => {
                tessellate_cylindrical_face_bounded(
                    arena,
                    face_idx,
                    cyl,
                    &disc,
                    edge_geometry,
                    &mut vertices,
                    &mut normals,
                    &mut indices,
                );
            }
            Some(SurfaceGeom::Planar(plane)) => {
                let normal = [
                    plane.normal.x as f32,
                    plane.normal.y as f32,
                    plane.normal.z as f32,
                ];
                let plane_origin = [plane.origin.x, plane.origin.y, plane.origin.z];
                let outer_boundary =
                    collect_loop_boundary(arena, arena.faces[face_idx.0].outer_loop, &disc);

                // Collect inner loop boundaries (holes)
                let inner_boundaries: Vec<Vec<usize>> = arena.faces[face_idx.0]
                    .inner_loops
                    .iter()
                    .map(|&inner_loop| collect_loop_boundary(arena, inner_loop, &disc))
                    .filter(|b| b.len() >= 3)
                    .collect();

                // D1 Tier 2a probe: env-gated dump of per-boundary-edge
                // intersection-curve flags. Default-off byte-identical.
                if std::env::var("Y47T2_INTERSECTION_PROBE").is_ok() {
                    y47t2_dump_boundary_intersections(face_idx, arena, &disc);
                }

                tessellate_planar_face_bounded(
                    &outer_boundary,
                    &disc.positions,
                    normal,
                    plane_origin,
                    &mut vertices,
                    &mut normals,
                    &mut indices,
                    &inner_boundaries,
                );
            }
            _ => {
                // Fallback for other surface types: collect boundary as polygon
                let boundary =
                    collect_loop_boundary(arena, arena.faces[face_idx.0].outer_loop, &disc);
                if boundary.len() >= 3 {
                    // Compute an approximate normal from the boundary polygon
                    let loop_verts: Vec<[f64; 3]> =
                        boundary.iter().map(|&i| disc.positions[i]).collect();
                    let bn = boundary.len();
                    let mut newell = [0.0f64; 3];
                    for i in 0..bn {
                        let curr = loop_verts[i];
                        let next = loop_verts[(i + 1) % bn];
                        newell[0] += (curr[1] - next[1]) * (curr[2] + next[2]);
                        newell[1] += (curr[2] - next[2]) * (curr[0] + next[0]);
                        newell[2] += (curr[0] - next[0]) * (curr[1] + next[1]);
                    }
                    let nlen = v3_length(newell);
                    let normal_f32 = if nlen > TAU_NORMALIZE {
                        [
                            (newell[0] / nlen) as f32,
                            (newell[1] / nlen) as f32,
                            (newell[2] / nlen) as f32,
                        ]
                    } else {
                        [0.0, 0.0, 1.0]
                    };
                    // No known plane in fallback path; use first boundary vertex as 2D origin.
                    let fallback_origin = if !boundary.is_empty() {
                        disc.positions[boundary[0]]
                    } else {
                        [0.0; 3]
                    };
                    tessellate_planar_face_bounded(
                        &boundary,
                        &disc.positions,
                        normal_f32,
                        fallback_origin,
                        &mut vertices,
                        &mut normals,
                        &mut indices,
                        &[],
                    );
                }
            }
        }

        let end_index = indices.len() as u32;
        let pushed = end_index > start_index;
        if pushed {
            face_ranges.push(FaceRange {
                face_id: KernelId(kid),
                start_index,
                end_index,
            });
        }

        // PR-Y41: any records pushed during this face's dispatch get
        // attributed to (kid, face_idx). Most faces emit one record; some
        // dispatch paths (e.g., cylindrical fallback into planar) may emit
        // multiple.
        if y41_on {
            let new_records: Vec<Y41DispatchRecord> = Y41_DISPATCH_BUFFER.with(|b| {
                let mut buf = b.borrow_mut();
                buf.drain(y41_buf_before..).collect()
            });
            for rec in new_records {
                y41_attributed.push((kid, face_idx.0, rec));
            }
        }

        // PR-Y36: complete the per-face capture.
        if let Some((geom_label, outer_boundary, he_count, nmm_count, inner_count, is_self_loop)) =
            y36_info_pre
        {
            let boundary_positions: Vec<[f64; 3]> = outer_boundary
                .iter()
                .map(|&i| disc.positions[i])
                .collect();
            y36_face_infos.push(Y36ProbeFaceInfo {
                kid,
                face_idx: face_idx.0,
                geom: geom_label,
                outer_he_count: he_count,
                outer_nmm_count: nmm_count,
                is_self_loop,
                outer_boundary_len: outer_boundary.len(),
                inner_loop_count: inner_count,
                indices_emitted: end_index - start_index,
                face_range_pushed: pushed,
                boundary_positions,
            });
        }
    }

    // [stage-f] F.0 baseline: after per-face dispatch loop, before fix_winding_consistency.
    // PR-VIZ-3a-fix: in-memory capture path runs even without env var (WASM).
    let probe_on = std::env::var("YANG_CONFORMAL_PROBE").as_deref() == Ok("1");
    let capture_armed = crate::boolean::yang_integration::is_yang_capture_armed();
    if probe_on {
        let unpaired = count_unpaired_in_mesh(&vertices, &indices);
        let tri_count = indices.len() / 3;
        eprintln!("[stage-f] sub=0 tri_count={tri_count} unpaired={unpaired}");
    }
    if probe_on || capture_armed {
        dump_stage_f_viz("F.0", &vertices, &indices, &face_ranges);
    }

    // D2 (2026-05-18): stage-f post-tessellation repair pipeline removed.
    // Yang §4.4.3 says watertightness is "inherited from the mesh Boolean
    // output" — F.0-F.4 sub-stages (fix_winding, remove_winding_insensitive_duplicates,
    // flip_nonmanifold_interior_diagonals, retessellate_nonmanifold_faces_with_steiner_fan,
    // remove_nonmanifold_topology_aware, remove_nonmanifold_duplicates_aggressive,
    // fix_global_orientation, weld_smooth_vertices) were all legacy S-H residue
    // masking upstream defects.

    // [stage-f] post-CDT diagnostics only — no repair.
    let probe_on = std::env::var("YANG_CONFORMAL_PROBE").as_deref() == Ok("1");
    let capture_armed = crate::boolean::yang_integration::is_yang_capture_armed();
    if probe_on {
        let unpaired = count_unpaired_in_mesh(&vertices, &indices);
        let tri_count = indices.len() / 3;
        eprintln!("[stage-f] post-cdt tri_count={tri_count} unpaired={unpaired}");
    }
    if probe_on || capture_armed {
        dump_stage_f_viz("F.4", &vertices, &indices, &face_ranges);
    }

    // [stage-f] F.4: after weld_smooth_vertices, just before return.
    let probe_on = std::env::var("YANG_CONFORMAL_PROBE").as_deref() == Ok("1");
    let capture_armed = crate::boolean::yang_integration::is_yang_capture_armed();
    if probe_on {
        let unpaired = count_unpaired_in_mesh(&vertices, &indices);
        let tri_count = indices.len() / 3;
        eprintln!("[stage-f] sub=4 tri_count={tri_count} unpaired={unpaired}");
    }
    if probe_on || capture_armed {
        dump_stage_f_viz("F.4", &vertices, &indices, &face_ranges);
    }

    // PR-Y36 inverse-direction probe: emit attribution TSV for unpaired
    // edges in the final render mesh. Default-off (gated on
    // `Y36_INVERSE_PROBE=1`).
    if y36_on {
        y36_write_inverse_attribution(&y36_face_infos, &vertices, &indices, &face_ranges);
    }

    // PR-Y41 dispatch-loop probe: emit per-face dispatch TSV with quantization
    // classification. Default-off (gated on `Y41_DISPATCH_PROBE=1`).
    if y41_on {
        y41_write_dispatch_tsv(y41_attributed);
        // Final defensive drain to keep thread-local clean for subsequent invocations.
        let _ = y41_take_records();
    }

    Ok(RenderMesh {
        vertices,
        normals,
        indices,
        face_ranges,
    })
}

// ── PR-VIZ-1: Stage F per-pass OBJ dump helper ──────────────────────────
//
// Called from each F.0–F.4 probe site under a `YANG_CONFORMAL_PROBE=1`
// guard. Inner `YANG_STAGE_DUMP=<dir>` env check makes the file-dump path
// no-op when dumps are disabled; PR-VIZ-3a's in-memory `record_stage`
// runs unconditionally (no env gate; CAPTURE_BUFFER controls the no-op).
// Spec: specs/yang_pr_viz_1_per_stage_obj_dump.md +
// specs/yang_pr_viz_3a_in_memory_capture.md
fn dump_stage_f_viz(stage_tag: &str, vertices: &[f32], indices: &[u32], face_ranges: &[FaceRange]) {
    // PR-VIZ-3a: in-memory capture. Spec §4 row 6: labels = face_id per
    // tri (mirrors the file-dump CSV's mapping logic).
    let n_tris = indices.len() / 3;
    let labels: Vec<u32> = (0..n_tris)
        .map(|i| {
            let i3 = (i * 3) as u32;
            face_ranges
                .iter()
                .find(|fr| fr.start_index <= i3 && i3 < fr.end_index)
                .map(|fr| fr.face_id.0 as u32)
                .unwrap_or(0)
        })
        .collect();
    crate::boolean::yang_integration::record_stage(stage_tag, vertices, indices, &labels);

    let dump_dir = match std::env::var("YANG_STAGE_DUMP") {
        Ok(d) => d,
        Err(_) => return,
    };
    let case_dir = crate::boolean::yang_integration::ensure_stage_dump_case_dir(&dump_dir);
    let safe_tag = crate::boolean::yang_integration::sanitize_stage_tag(stage_tag);
    let (verts, tris) =
        crate::boolean::yang_integration::pack_f32_indices_to_f64_mesh(vertices, indices);
    let _ = crate::boolean::yang_integration::dump_mesh_as_obj(
        &verts,
        &tris,
        &format!("{case_dir}/stage_{safe_tag}.obj"),
    );
    let rows: Vec<String> = (0..tris.len())
        .map(|i| {
            let i3 = (i * 3) as u32;
            let face_id = face_ranges
                .iter()
                .find(|fr| fr.start_index <= i3 && i3 < fr.end_index)
                .map(|fr| fr.face_id.0)
                .unwrap_or(0);
            format!("{i},{face_id}")
        })
        .collect();
    let _ = crate::boolean::yang_integration::dump_labels_as_csv(
        "tri_idx,face_id",
        &rows,
        &format!("{case_dir}/stage_{safe_tag}_labels.csv"),
    );
}

// ── Geometry helpers ─────────────────────────────────────────────────────

/// Derive orthogonal x/y axes from a normal vector for circle tessellation.
fn make_circle_axes(normal: &[f64; 3]) -> ([f64; 3], [f64; 3]) {
    let n = *normal;
    // Pick a vector not parallel to normal
    let up = if n[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let x = v3_cross(n, up);
    let len = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
    let x_norm = [x[0] / len, x[1] / len, x[2] / len];
    let y = v3_cross(n, x_norm);
    (x_norm, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{TAU_SNAP_FACTOR, TAU_TESS_GRID_FACTOR};

    /// Helper: build a minimal vertex + index buffer for dedup testing.
    /// Each vertex is [x, y, z] in f32. Returns (vertices_flat, indices).
    fn make_mesh(verts: &[[f32; 3]], tris: &[[u32; 3]]) -> (Vec<f32>, Vec<u32>) {
        let vertices: Vec<f32> = verts.iter().flat_map(|v| v.iter().copied()).collect();
        let indices: Vec<u32> = tris.iter().flat_map(|t| t.iter().copied()).collect();
        (vertices, indices)
    }

    // D2 (2026-05-18): dedup_* tests removed — they targeted
    // `remove_duplicate_triangles` from the deleted repair pipeline. Yang's
    // §4.4.1 CDT doesn't emit duplicates by construction.

    // ── AABB-collapse regression tests ──────────────────────────────

    use crate::traits::Kernel;
    use crate::waffle_kernel::WaffleKernel;

    /// Helper: create a cylinder solid in the given kernel.
    fn make_test_cylinder(
        kernel: &mut WaffleKernel,
        cx: f64,
        cy: f64,
        r: f64,
        depth: f64,
    ) -> crate::KernelSolidHandle {
        use crate::types::{CircleProfile, ClosedProfile};
        let mut positions = std::collections::HashMap::new();
        positions.insert(1, (cx, cy));
        let profile = ClosedProfile {
            entity_ids: vec![1],
            is_outer: true,
            vertex_ids: vec![],
            circle: Some(CircleProfile {
                center_u: cx,
                center_v: cy,
                radius: r,
            }),
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .expect("make_faces_from_profiles for cylinder");
        kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], depth)
            .expect("extrude_face for cylinder")
    }

    /// Helper: create a box solid in the given kernel.
    fn make_test_box(
        kernel: &mut WaffleKernel,
        cx: f64,
        cy: f64,
        w: f64,
        h: f64,
        depth: f64,
    ) -> crate::KernelSolidHandle {
        use crate::types::ClosedProfile;
        let mut positions = std::collections::HashMap::new();
        positions.insert(1, (cx - w / 2.0, cy - h / 2.0));
        positions.insert(2, (cx + w / 2.0, cy - h / 2.0));
        positions.insert(3, (cx + w / 2.0, cy + h / 2.0));
        positions.insert(4, (cx - w / 2.0, cy + h / 2.0));
        let profile = ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![1, 2, 3, 4],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .expect("make_faces_from_profiles for box");
        kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], depth)
            .expect("extrude_face for box")
    }

    /// Check whether all XY coordinates collapse to the AABB boundary.
    /// Signed volume of a triangle mesh via divergence theorem.
    /// Works on flat f32 vertex/index buffers (the RenderMesh layout).
    fn mesh_volume_flat(vertices: &[f32], indices: &[u32]) -> f64 {
        let mut vol = 0.0_f64;
        let n_tris = indices.len() / 3;
        for i in 0..n_tris {
            let i0 = indices[i * 3] as usize;
            let i1 = indices[i * 3 + 1] as usize;
            let i2 = indices[i * 3 + 2] as usize;
            let v0 = [
                vertices[i0 * 3] as f64,
                vertices[i0 * 3 + 1] as f64,
                vertices[i0 * 3 + 2] as f64,
            ];
            let v1 = [
                vertices[i1 * 3] as f64,
                vertices[i1 * 3 + 1] as f64,
                vertices[i1 * 3 + 2] as f64,
            ];
            let v2 = [
                vertices[i2 * 3] as f64,
                vertices[i2 * 3 + 1] as f64,
                vertices[i2 * 3 + 2] as f64,
            ];
            // Signed volume of tetrahedron formed by triangle and origin
            vol += v0[0] * (v1[1] * v2[2] - v2[1] * v1[2])
                - v1[0] * (v0[1] * v2[2] - v2[1] * v0[2])
                + v2[0] * (v0[1] * v1[2] - v1[1] * v0[2]);
        }
        (vol / 6.0).abs()
    }

    /// For extruded solids, z always matches a face (top/bottom), so we only
    /// check XY — a proper cylinder mesh should have interior XY points.
    fn is_xy_aabb_collapsed(vertices: &[f32]) -> bool {
        if vertices.len() < 3 {
            return true;
        }
        let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
        for chunk in vertices.chunks(3) {
            min_x = min_x.min(chunk[0]);
            min_y = min_y.min(chunk[1]);
            max_x = max_x.max(chunk[0]);
            max_y = max_y.max(chunk[1]);
        }
        let tol = TAU_SNAP_FACTOR as f32;
        // Check if every vertex's x and y are on AABB boundary
        for chunk in vertices.chunks(3) {
            let x_on_boundary = (chunk[0] - min_x).abs() < tol || (chunk[0] - max_x).abs() < tol;
            let y_on_boundary = (chunk[1] - min_y).abs() < tol || (chunk[1] - max_y).abs() < tol;
            // A vertex is NOT on the XY AABB boundary if neither x nor y is extreme
            if !x_on_boundary && !y_on_boundary {
                return false;
            }
        }
        true
    }

    #[test]
    fn test_cyl_cyl_union_tessellation_not_aabb_collapsed() {
        let mut kernel = WaffleKernel::new();
        // Two parallel cylinders, overlapping: r=5 at (0,0) and r=5 at (4,0), depth=10
        // Overlap distance = 2*5 - 4 = 6, so union is NOT disjoint
        let cyl_a = make_test_cylinder(&mut kernel, 0.0, 0.0, 5.0, 10.0);
        let cyl_b = make_test_cylinder(&mut kernel, 4.0, 0.0, 5.0, 10.0);
        let union_handle = kernel
            .boolean_union(&cyl_a, &cyl_b)
            .expect("cyl-cyl union should succeed");
        let mesh = kernel
            .tessellate(&union_handle, 0.1)
            .expect("tessellation should succeed");
        let vertex_count = mesh.vertices.len() / 3;
        assert!(
            vertex_count >= 3,
            "mesh should have at least 3 vertices, got {}",
            vertex_count
        );
        assert!(
            !is_xy_aabb_collapsed(&mesh.vertices),
            "cyl-cyl union XY coords should NOT all collapse to AABB faces ({} verts)",
            vertex_count
        );

        // Volume oracle: each cylinder volume = π*r²*h = π*25*10 ≈ 785.4.
        // Inclusion-exclusion: V_union = V_a + V_b - V_intersection.
        // For two r=5 cylinders offset by 4, intersection ≈ 538.
        // Expected V_union ≈ 1033 (mesh approximation may differ by up to 15%).
        let vol = mesh_volume_flat(&mesh.vertices, &mesh.indices);
        let single_cyl_vol = std::f64::consts::PI * 25.0 * 10.0;
        assert!(
            vol > single_cyl_vol,
            "cyl-cyl union volume ({:.1}) must exceed a single cylinder ({:.1})",
            vol,
            single_cyl_vol
        );
        assert!(
            vol < 1.85 * single_cyl_vol,
            "cyl-cyl union volume ({:.1}) must be less than 1.85× a single cylinder ({:.1})",
            vol,
            1.85 * single_cyl_vol
        );

        // D2: watertightness diagnostic removed (count_boundary_edges/count_nonmanifold_edges deleted with repair pipeline).
    }

    #[test]
    fn test_box_minus_enclosed_cyl_tessellation_not_aabb_collapsed() {
        let mut kernel = WaffleKernel::new();
        // Box 20x20x10 centered at (10,10), cylinder r=3 at center, fully enclosed
        let box_handle = make_test_box(&mut kernel, 10.0, 10.0, 20.0, 20.0, 10.0);
        let cyl_handle = make_test_cylinder(&mut kernel, 10.0, 10.0, 3.0, 10.0);
        let sub_handle = kernel
            .boolean_subtract(&box_handle, &cyl_handle)
            .expect("box-minus-cyl should succeed");
        let mesh = kernel
            .tessellate(&sub_handle, 0.1)
            .expect("tessellation should succeed");
        let vertex_count = mesh.vertices.len() / 3;
        assert!(
            vertex_count > 24,
            "box-minus-cyl should have more than 24 vertices (a plain box), got {}",
            vertex_count
        );
        assert!(
            !is_xy_aabb_collapsed(&mesh.vertices),
            "box-minus-cyl XY coords should NOT all collapse to AABB faces ({} verts)",
            vertex_count
        );

        // Volume oracle: box = 20*20*10 = 4000, cylinder = π*9*10 ≈ 282.7
        // Result ≈ 4000 - 282.7 = 3717.3 (mesh tolerance ~10%)
        let vol = mesh_volume_flat(&mesh.vertices, &mesh.indices);
        let box_vol = 20.0 * 20.0 * 10.0;
        let cyl_vol = std::f64::consts::PI * 9.0 * 10.0;
        let expected = box_vol - cyl_vol;
        assert!(
            vol > expected * 0.90,
            "box-minus-cyl volume ({:.1}) should be > 90% of expected ({:.1})",
            vol,
            expected
        );
        assert!(
            vol < box_vol,
            "box-minus-cyl volume ({:.1}) should be less than the box alone ({:.1})",
            vol,
            box_vol
        );

        // D2: watertightness diagnostic removed (count_boundary_edges/count_nonmanifold_edges deleted with repair pipeline).
    }

    /// Test that tessellate_solid_bounded resolves non-manifold earcut diagonals.
    ///
    /// Constructs two coplanar quadrilateral faces whose boundaries share two
    /// corner *positions* (via separate B-Rep vertices) without a B-Rep edge
    /// between those corners. Fan triangulation creates the same interior
    /// diagonal in both faces, producing 4 triangles on that edge (non-manifold).
    ///
    /// After flip_nonmanifold_interior_diagonals() is implemented, the result
    /// should have ZERO non-manifold edges and preserve all 4 triangles.
    /// Until then this test fails.
    #[test]
    fn test_edge_flip_resolves_nonmanifold_earcut_diagonal() {
        use crate::geometry::curve::Line3D;
        use crate::geometry::point::{Point3, Vector3};
        use crate::geometry::surface::Plane;

        // Build a minimal B-Rep arena with two independent coplanar quad faces.
        // Both faces are CCW (Newell normal = +z, matching the stored normal)
        // so tessellate_planar_face_bounded does NOT reverse vertex order.
        // Fan triangulation (convex quad, n<=8) fans from vertex[0].
        //
        // Face 1 (lower diamond, CCW):
        //   v0(0,0,0) → v1(1,-3,0) → v2(2,0,0) → v3(1,0.01,0)
        //   Fan: (v0,v1,v2), (v0,v2,v3) → diagonal (0,0,0)-(2,0,0)
        //
        // Face 2 (upper diamond, CCW):
        //   v4(0,0,0) → v5(1,-0.01,0) → v6(2,0,0) → v7(1,4,0)
        //   Fan: (v4,v5,v6), (v4,v6,v7) → diagonal (0,0,0)-(2,0,0)
        //
        // Shared positions: v0≡v4 at (0,0,0), v2≡v6 at (2,0,0).
        // No B-Rep edge connects v0↔v2 or v4↔v6.
        // Edge (0,0,0)-(2,0,0) appears in 4 triangles → non-manifold.

        let mut arena = TopoArena::new();

        // ── Vertices ────────────────────────────────────────────────
        // Face 1 vertices (CCW diamond: left → bottom → right → just-above-center)
        let v0 = arena.add_vertex([0.0, 0.0, 0.0]);
        let v1 = arena.add_vertex([1.0, -3.0, 0.0]);
        let v2 = arena.add_vertex([2.0, 0.0, 0.0]);
        let v3 = arena.add_vertex([1.0, 0.01, 0.0]);
        // Face 2 vertices (CCW diamond: left → just-below-center → right → top)
        // v4≡v0 at (0,0,0), v6≡v2 at (2,0,0) — same positions, different B-Rep vertices
        let v4 = arena.add_vertex([0.0, 0.0, 0.0]);
        let v5 = arena.add_vertex([1.0, -0.01, 0.0]);
        let v6 = arena.add_vertex([2.0, 0.0, 0.0]);
        let v7 = arena.add_vertex([1.0, 4.0, 0.0]);

        // ── Solid / Shell ───────────────────────────────────────────
        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);
        arena.solids[solid.0].outer_shell = shell;

        // ── Face 1 ─────────────────────────────────────────────────
        let face1 = arena.add_face(shell);
        let loop1 = arena.add_loop(face1);
        arena.faces[face1.0].outer_loop = loop1;
        arena.shells[shell.0].face = face1;

        // Build 4 edges for face 1: v0→v1, v1→v2, v2→v3, v3→v0
        let f1_verts = [v0, v1, v2, v3];
        let mut f1_he_indices = Vec::new();
        for i in 0..4 {
            let (_, he_a, he_b) = arena.add_edge();
            let next_i = (i + 1) % 4;
            arena.half_edges[he_a.0].origin = f1_verts[i];
            arena.half_edges[he_b.0].origin = f1_verts[next_i];
            arena.half_edges[he_a.0].loop_ = loop1;
            arena.half_edges[he_b.0].loop_ = loop1; // twin side: unused but needs valid loop
            f1_he_indices.push((he_a, he_b));
        }
        // Link the forward half-edges into a cycle for loop1
        for i in 0..4 {
            let next_i = (i + 1) % 4;
            arena.half_edges[f1_he_indices[i].0 .0].next = f1_he_indices[next_i].0;
            arena.half_edges[f1_he_indices[next_i].0 .0].prev = f1_he_indices[i].0;
        }
        arena.loops[loop1.0].half_edge = f1_he_indices[0].0;
        // Set vertex half_edge references
        for i in 0..4 {
            arena.vertices[f1_verts[i].0].half_edge = Some(f1_he_indices[i].0);
        }

        // ── Face 2 ─────────────────────────────────────────────────
        let face2 = arena.add_face(shell);
        let loop2 = arena.add_loop(face2);
        arena.faces[face2.0].outer_loop = loop2;

        // Build 4 edges for face 2: v4→v5, v5→v6, v6→v7, v7→v4
        let f2_verts = [v4, v5, v6, v7];
        let mut f2_he_indices = Vec::new();
        for i in 0..4 {
            let (_, he_a, he_b) = arena.add_edge();
            let next_i = (i + 1) % 4;
            arena.half_edges[he_a.0].origin = f2_verts[i];
            arena.half_edges[he_b.0].origin = f2_verts[next_i];
            arena.half_edges[he_a.0].loop_ = loop2;
            arena.half_edges[he_b.0].loop_ = loop2;
            f2_he_indices.push((he_a, he_b));
        }
        // Link the forward half-edges into a cycle for loop2
        for i in 0..4 {
            let next_i = (i + 1) % 4;
            arena.half_edges[f2_he_indices[i].0 .0].next = f2_he_indices[next_i].0;
            arena.half_edges[f2_he_indices[next_i].0 .0].prev = f2_he_indices[i].0;
        }
        arena.loops[loop2.0].half_edge = f2_he_indices[0].0;
        for i in 0..4 {
            arena.vertices[f2_verts[i].0].half_edge = Some(f2_he_indices[i].0);
        }

        // ── Geometry maps ───────────────────────────────────────────
        let z_up_normal = Plane {
            origin: Point3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            normal: Vector3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
        };

        let mut face_map: BTreeMap<u64, FaceIdx> = BTreeMap::new();
        face_map.insert(1, face1);
        face_map.insert(2, face2);

        let mut face_geometry: BTreeMap<FaceIdx, SurfaceGeom> = BTreeMap::new();
        face_geometry.insert(face1, SurfaceGeom::Planar(z_up_normal.clone()));
        face_geometry.insert(face2, SurfaceGeom::Planar(z_up_normal));

        let mut edge_geometry: BTreeMap<EdgeIdx, CurveGeom> = BTreeMap::new();
        for (idx, edge) in arena.edges.iter().enumerate() {
            let he_a = edge.half_edge;
            let v_start = arena.half_edges[he_a.0].origin;
            let v_end = arena.half_edges[arena.half_edges[he_a.0]
                .twin
                .expect("manifold-ctx: tessellation edge requires paired twin")
                .0]
                .origin;
            let p0 = arena.vertices[v_start.0].position;
            let p1 = arena.vertices[v_end.0].position;
            edge_geometry.insert(
                EdgeIdx(idx),
                CurveGeom::Linear(Line3D {
                    origin: Point3::from_array(p0),
                    direction: Vector3::from_array(v3_sub(p1, p0)),
                }),
            );
        }

        // ── Tessellate ─────────────────────────────────────────────
        let mesh = tessellate_solid_bounded(&arena, &face_map, &face_geometry, &edge_geometry)
            .expect("tessellate_solid_bounded should succeed");

        // ── Verify: no non-manifold edges ──────────────────────────
        // Count edge multiplicities using position-based quantization.
        let max_abs = mesh
            .vertices
            .iter()
            .map(|v| v.abs())
            .fold(0.0_f32, f32::max);
        let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
        let inv_grid = 1.0 / grid;
        let quantize = |idx: u32| -> (i64, i64, i64) {
            let base = idx as usize * 3;
            (
                (mesh.vertices[base] as f64 * inv_grid).round() as i64,
                (mesh.vertices[base + 1] as f64 * inv_grid).round() as i64,
                (mesh.vertices[base + 2] as f64 * inv_grid).round() as i64,
            )
        };

        let n_tris = mesh.indices.len() / 3;
        let mut edge_counts: BTreeMap<((i64, i64, i64), (i64, i64, i64)), u32> = BTreeMap::new();
        for i in 0..n_tris {
            let tri = [
                mesh.indices[i * 3],
                mesh.indices[i * 3 + 1],
                mesh.indices[i * 3 + 2],
            ];
            for j in 0..3 {
                let pa = quantize(tri[j]);
                let pb = quantize(tri[(j + 1) % 3]);
                let key = if pa <= pb { (pa, pb) } else { (pb, pa) };
                *edge_counts.entry(key).or_insert(0) += 1;
            }
        }

        let nonmanifold_edges: Vec<_> =
            edge_counts.iter().filter(|(_, &count)| count > 2).collect();

        assert!(
            nonmanifold_edges.is_empty(),
            "Expected zero non-manifold edges after edge-flip repair, \
             but found {} edges with count>2: {:?}. \
             The flip_nonmanifold_interior_diagonals() function must resolve \
             conflicting earcut diagonals between adjacent faces that share \
             corner vertex positions without a connecting B-Rep edge.",
            nonmanifold_edges.len(),
            nonmanifold_edges
                .iter()
                .take(5)
                .map(|(edge, count)| format!("edge {:?} count={}", edge, count))
                .collect::<Vec<_>>()
        );

        // Also verify the mesh is complete (no missing triangles).
        // The current removal-based non-manifold repair deletes triangles, which
        // creates boundary holes. The edge-flip approach should preserve all
        // triangles while eliminating non-manifold edges, yielding a mesh with
        // exactly 4 triangles (2 per quad face).
        assert_eq!(
            n_tris, 4,
            "Two quad faces should produce exactly 4 triangles (2 per face), \
             but got {}. The current removal-based repair deletes triangles \
             to fix non-manifold edges; flip_nonmanifold_interior_diagonals() \
             should preserve all triangles by flipping the conflicting diagonal \
             in one face instead.",
            n_tris
        );

        // For this open-surface test (two independent quads, not a closed solid),
        // boundary edges have count=1, which is expected. The important check is
        // that no edges have count>2 (already asserted above via nonmanifold_edges).
        // Also verify that the flipped diagonal exists as an internal edge (count=2).
        let internal_edges: Vec<_> = edge_counts
            .iter()
            .filter(|(_, &count)| count == 2)
            .collect();
        assert!(
            !internal_edges.is_empty(),
            "Expected at least one internal edge (count=2) from the flipped diagonal, \
             but found none. This suggests the flip did not create a valid shared edge."
        );
    }

    /// Three coplanar quad faces all sharing two vertex positions without a
    /// connecting B-Rep edge.  Earcut creates the same interior diagonal in
    /// all three faces (6 triangles on one edge).  Edge-flip alone cannot
    /// resolve this because flipping in one face may create a new conflict
    /// with the third face.  Steiner-fan re-tessellation should resolve it
    /// by giving each face a unique centroid-based fan that shares no
    /// interior diagonals.
    #[test]
    fn test_steiner_fan_resolves_three_face_shared_diagonal() {
        use crate::geometry::curve::Line3D;
        use crate::geometry::point::{Point3, Vector3};
        use crate::geometry::surface::Plane;

        let mut arena = TopoArena::new();

        // Shared positions: (0,0,0) and (2,0,0).
        // Face A: quad (0,0,0) (1,-3,0) (2,0,0) (1,-1,0) — points downward
        // Face B: quad (0,0,0) (1,1,0)  (2,0,0) (1,3,0)  — points upward
        // Face C: quad (0,0,0) (0.5,0.3,0) (2,0,0) (1.5,-0.3,0) — narrow strip
        // Each face has separate B-Rep vertices at the shared positions.

        let positions: [([f64; 3], [[f64; 3]; 4]); 3] = [
            (
                [0.0, 0.0, 1.0], // normal (unused for fan, but needed for geometry)
                [
                    [0.0, 0.0, 0.0],
                    [1.0, -3.0, 0.0],
                    [2.0, 0.0, 0.0],
                    [1.0, -1.0, 0.0],
                ],
            ),
            (
                [0.0, 0.0, 1.0],
                [
                    [0.0, 0.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [2.0, 0.0, 0.0],
                    [1.0, 3.0, 0.0],
                ],
            ),
            (
                [0.0, 0.0, 1.0],
                [
                    [0.0, 0.0, 0.0],
                    [0.5, 0.3, 0.0],
                    [2.0, 0.0, 0.0],
                    [1.5, -0.3, 0.0],
                ],
            ),
        ];

        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);
        arena.solids[solid.0].outer_shell = shell;

        let mut face_indices = Vec::new();
        let z_up = Plane {
            origin: Point3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            normal: Vector3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
        };
        let mut face_map: BTreeMap<u64, FaceIdx> = BTreeMap::new();
        let mut face_geometry: BTreeMap<FaceIdx, SurfaceGeom> = BTreeMap::new();

        for (face_id, (_normal, verts)) in positions.iter().enumerate() {
            let face = arena.add_face(shell);
            let lp = arena.add_loop(face);
            arena.faces[face.0].outer_loop = lp;
            if face_id == 0 {
                arena.shells[shell.0].face = face;
            }

            let mut v_indices = Vec::new();
            for pos in verts {
                v_indices.push(arena.add_vertex(*pos));
            }

            let mut he_pairs = Vec::new();
            for i in 0..4 {
                let (_, he_a, he_b) = arena.add_edge();
                let next_i = (i + 1) % 4;
                arena.half_edges[he_a.0].origin = v_indices[i];
                arena.half_edges[he_b.0].origin = v_indices[next_i];
                arena.half_edges[he_a.0].loop_ = lp;
                arena.half_edges[he_b.0].loop_ = lp;
                he_pairs.push((he_a, he_b));
            }
            for i in 0..4 {
                let next_i = (i + 1) % 4;
                arena.half_edges[he_pairs[i].0 .0].next = he_pairs[next_i].0;
                arena.half_edges[he_pairs[next_i].0 .0].prev = he_pairs[i].0;
            }
            arena.loops[lp.0].half_edge = he_pairs[0].0;
            for i in 0..4 {
                arena.vertices[v_indices[i].0].half_edge = Some(he_pairs[i].0);
            }

            face_map.insert(face_id as u64 + 1, face);
            face_geometry.insert(face, SurfaceGeom::Planar(z_up.clone()));
            face_indices.push(face);
        }

        // Edge geometry
        let mut edge_geometry: BTreeMap<EdgeIdx, CurveGeom> = BTreeMap::new();
        for (idx, edge) in arena.edges.iter().enumerate() {
            let he_a = edge.half_edge;
            let v_start = arena.half_edges[he_a.0].origin;
            let v_end = arena.half_edges[arena.half_edges[he_a.0]
                .twin
                .expect("manifold-ctx: tessellation edge requires paired twin")
                .0]
                .origin;
            let p0 = arena.vertices[v_start.0].position;
            let p1 = arena.vertices[v_end.0].position;
            edge_geometry.insert(
                EdgeIdx(idx),
                CurveGeom::Linear(Line3D {
                    origin: Point3::from_array(p0),
                    direction: Vector3::from_array(v3_sub(p1, p0)),
                }),
            );
        }

        let mesh = tessellate_solid_bounded(&arena, &face_map, &face_geometry, &edge_geometry)
            .expect("tessellate_solid_bounded should succeed");

        // Count non-manifold edges
        let max_abs = mesh
            .vertices
            .iter()
            .map(|v| v.abs())
            .fold(0.0_f32, f32::max);
        let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
        let inv_grid = 1.0 / grid;
        let quantize = |idx: u32| -> (i64, i64, i64) {
            let base = idx as usize * 3;
            (
                (mesh.vertices[base] as f64 * inv_grid).round() as i64,
                (mesh.vertices[base + 1] as f64 * inv_grid).round() as i64,
                (mesh.vertices[base + 2] as f64 * inv_grid).round() as i64,
            )
        };

        let n_tris = mesh.indices.len() / 3;
        let mut edge_counts: BTreeMap<((i64, i64, i64), (i64, i64, i64)), u32> = BTreeMap::new();
        for i in 0..n_tris {
            let tri = [
                mesh.indices[i * 3],
                mesh.indices[i * 3 + 1],
                mesh.indices[i * 3 + 2],
            ];
            for j in 0..3 {
                let pa = quantize(tri[j]);
                let pb = quantize(tri[(j + 1) % 3]);
                let key = if pa <= pb { (pa, pb) } else { (pb, pa) };
                *edge_counts.entry(key).or_insert(0) += 1;
            }
        }

        let nonmanifold_edges: Vec<_> =
            edge_counts.iter().filter(|(_, &count)| count > 2).collect();
        let _unpaired_edges: Vec<_> = edge_counts
            .iter()
            .filter(|(_, &count)| count == 1)
            .collect();

        assert!(
            nonmanifold_edges.is_empty(),
            "Three faces sharing vertex positions at (0,0,0) and (2,0,0) \
             should have zero non-manifold edges after Steiner-fan \
             re-tessellation, but found {} edges with count>2. \
             Steiner-fan should give each face a unique interior centroid \
             so no two faces share interior diagonals.",
            nonmanifold_edges.len()
        );

        // The key assertion: Steiner-fan must not create holes (unpaired edges
        // from triangle removal). Every face must be fully tessellated.
        // For this open surface, boundary edges are expected (count=1), but
        // the total triangle count must equal the sum of per-face triangles.
        // With earcut: 2+2+2 = 6 triangles. With Steiner-fan: 4+4+4 = 12.
        // The current aggressive removal may delete triangles, creating holes.
        // Verify that all faces contribute the expected number of triangles.
        assert!(
            n_tris >= 6,
            "Three quad faces must produce at least 6 triangles (2 per face \
             from earcut), but got {}. If triangles were removed to fix \
             non-manifold edges, Steiner-fan re-tessellation should \
             preserve all triangles instead.",
            n_tris
        );
    }

    /// Steiner-fan tessellation must produce correct triangle count:
    /// N triangles for an N-vertex polygon (vs N-2 from earcut).
    /// This tests that re-tessellated faces have the expected geometry.
    #[test]
    fn test_steiner_fan_triangle_count_for_pentagon() {
        use crate::geometry::curve::Line3D;
        use crate::geometry::point::{Point3, Vector3};
        use crate::geometry::surface::Plane;

        let mut arena = TopoArena::new();

        // Single pentagon face + a second quad face sharing 2 vertices to
        // trigger non-manifold detection → Steiner-fan re-tessellation of
        // the pentagon.
        //
        // Pentagon: V0(0,0,0) V1(2,-1,0) V2(3,1,0) V3(2,3,0) V4(0,2,0)
        // Quad:     V5(0,0,0) V6(2,-1,0) V7(1,-3,0) V8(-1,-2,0)
        // Shared positions: V0≡V5 at (0,0,0), V1≡V6 at (2,-1,0)

        let solid = arena.add_solid();
        let shell = arena.add_shell(solid);
        arena.solids[solid.0].outer_shell = shell;

        let z_up = Plane {
            origin: Point3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            normal: Vector3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
        };

        // ── Pentagon face ────────────────────
        let pent_verts: Vec<VertexIdx> = [
            [0.0, 0.0, 0.0],
            [2.0, -1.0, 0.0],
            [3.0, 1.0, 0.0],
            [2.0, 3.0, 0.0],
            [0.0, 2.0, 0.0],
        ]
        .iter()
        .map(|p| arena.add_vertex(*p))
        .collect();

        let pent_face = arena.add_face(shell);
        let pent_loop = arena.add_loop(pent_face);
        arena.faces[pent_face.0].outer_loop = pent_loop;
        arena.shells[shell.0].face = pent_face;

        let mut pent_hes = Vec::new();
        for i in 0..5 {
            let (_, he_a, he_b) = arena.add_edge();
            let next_i = (i + 1) % 5;
            arena.half_edges[he_a.0].origin = pent_verts[i];
            arena.half_edges[he_b.0].origin = pent_verts[next_i];
            arena.half_edges[he_a.0].loop_ = pent_loop;
            arena.half_edges[he_b.0].loop_ = pent_loop;
            pent_hes.push((he_a, he_b));
        }
        for i in 0..5 {
            let next_i = (i + 1) % 5;
            arena.half_edges[pent_hes[i].0 .0].next = pent_hes[next_i].0;
            arena.half_edges[pent_hes[next_i].0 .0].prev = pent_hes[i].0;
        }
        arena.loops[pent_loop.0].half_edge = pent_hes[0].0;
        for i in 0..5 {
            arena.vertices[pent_verts[i].0].half_edge = Some(pent_hes[i].0);
        }

        // ── Quad face (shares two vertex positions with pentagon) ─────
        let quad_verts: Vec<VertexIdx> = [
            [0.0, 0.0, 0.0],
            [2.0, -1.0, 0.0],
            [1.0, -3.0, 0.0],
            [-1.0, -2.0, 0.0],
        ]
        .iter()
        .map(|p| arena.add_vertex(*p))
        .collect();

        let quad_face = arena.add_face(shell);
        let quad_loop = arena.add_loop(quad_face);
        arena.faces[quad_face.0].outer_loop = quad_loop;

        let mut quad_hes = Vec::new();
        for i in 0..4 {
            let (_, he_a, he_b) = arena.add_edge();
            let next_i = (i + 1) % 4;
            arena.half_edges[he_a.0].origin = quad_verts[i];
            arena.half_edges[he_b.0].origin = quad_verts[next_i];
            arena.half_edges[he_a.0].loop_ = quad_loop;
            arena.half_edges[he_b.0].loop_ = quad_loop;
            quad_hes.push((he_a, he_b));
        }
        for i in 0..4 {
            let next_i = (i + 1) % 4;
            arena.half_edges[quad_hes[i].0 .0].next = quad_hes[next_i].0;
            arena.half_edges[quad_hes[next_i].0 .0].prev = quad_hes[i].0;
        }
        arena.loops[quad_loop.0].half_edge = quad_hes[0].0;
        for i in 0..4 {
            arena.vertices[quad_verts[i].0].half_edge = Some(quad_hes[i].0);
        }

        let mut face_map: BTreeMap<u64, FaceIdx> = BTreeMap::new();
        face_map.insert(1, pent_face);
        face_map.insert(2, quad_face);
        let mut face_geometry: BTreeMap<FaceIdx, SurfaceGeom> = BTreeMap::new();
        face_geometry.insert(pent_face, SurfaceGeom::Planar(z_up.clone()));
        face_geometry.insert(quad_face, SurfaceGeom::Planar(z_up));

        let mut edge_geometry: BTreeMap<EdgeIdx, CurveGeom> = BTreeMap::new();
        for (idx, edge) in arena.edges.iter().enumerate() {
            let he_a = edge.half_edge;
            let v_start = arena.half_edges[he_a.0].origin;
            let v_end = arena.half_edges[arena.half_edges[he_a.0]
                .twin
                .expect("manifold-ctx: tessellation edge requires paired twin")
                .0]
                .origin;
            let p0 = arena.vertices[v_start.0].position;
            let p1 = arena.vertices[v_end.0].position;
            edge_geometry.insert(
                EdgeIdx(idx),
                CurveGeom::Linear(Line3D {
                    origin: Point3::from_array(p0),
                    direction: Vector3::from_array(v3_sub(p1, p0)),
                }),
            );
        }

        let mesh = tessellate_solid_bounded(&arena, &face_map, &face_geometry, &edge_geometry)
            .expect("tessellation should succeed");

        // After Steiner-fan re-tessellation, no non-manifold edges should remain.
        let max_abs = mesh
            .vertices
            .iter()
            .map(|v| v.abs())
            .fold(0.0_f32, f32::max);
        let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
        let inv_grid = 1.0 / grid;
        let quantize = |idx: u32| -> (i64, i64, i64) {
            let base = idx as usize * 3;
            (
                (mesh.vertices[base] as f64 * inv_grid).round() as i64,
                (mesh.vertices[base + 1] as f64 * inv_grid).round() as i64,
                (mesh.vertices[base + 2] as f64 * inv_grid).round() as i64,
            )
        };

        let n_tris = mesh.indices.len() / 3;
        let mut edge_counts: BTreeMap<((i64, i64, i64), (i64, i64, i64)), u32> = BTreeMap::new();
        for i in 0..n_tris {
            let tri = [
                mesh.indices[i * 3],
                mesh.indices[i * 3 + 1],
                mesh.indices[i * 3 + 2],
            ];
            for j in 0..3 {
                let pa = quantize(tri[j]);
                let pb = quantize(tri[(j + 1) % 3]);
                let key = if pa <= pb { (pa, pb) } else { (pb, pa) };
                *edge_counts.entry(key).or_insert(0) += 1;
            }
        }

        let nonmanifold_edges: Vec<_> =
            edge_counts.iter().filter(|(_, &count)| count > 2).collect();

        assert!(
            nonmanifold_edges.is_empty(),
            "Pentagon + quad sharing (0,0,0) and (2,-1,0) should have \
             zero non-manifold edges after Steiner-fan re-tessellation, \
             but found {} edges with count>2.",
            nonmanifold_edges.len()
        );

        // Triangle count oracle: Steiner-fan produces N triangles for an N-vertex
        // polygon (centroid + N edges). Pentagon → 5 triangles. The quad face may
        // be removed during opposite-winding deduplication because its shared
        // vertices create degenerate topology. At minimum the pentagon's 5 Steiner
        // fan triangles must survive.
        assert!(
            n_tris >= 5,
            "Pentagon(5V) Steiner fan should produce at least 5 triangles, got {}",
            n_tris
        );
    }

    // ── is_smooth_edge tests ────────────────────────────────────────

    /// Build a minimal arena with two faces sharing one edge.
    /// Returns (arena, edge_idx, face_a_idx, face_b_idx).
    fn make_two_face_arena() -> (TopoArena, EdgeIdx, FaceIdx, FaceIdx) {
        use crate::topology::half_edge::*;

        let mut arena = TopoArena::default();

        // Vertices (unused by is_smooth_edge, but needed for arena)
        arena.vertices.push(Vertex {
            position: [0.0, 0.0, 0.0],
            half_edge: None,
        });
        arena.vertices.push(Vertex {
            position: [1.0, 0.0, 0.0],
            half_edge: None,
        });

        // Shells (one shell for both faces)
        arena.shells.push(Shell {
            face: FaceIdx(0),
            solid: SolidIdx(0),
        });

        // Two faces
        let face_a = FaceIdx(arena.faces.len());
        arena.faces.push(Face {
            outer_loop: LoopIdx(0),
            inner_loops: vec![],
            shell: ShellIdx(0),
        });
        let face_b = FaceIdx(arena.faces.len());
        arena.faces.push(Face {
            outer_loop: LoopIdx(1),
            inner_loops: vec![],
            shell: ShellIdx(0),
        });

        // Two loops
        arena.loops.push(Loop {
            half_edge: HalfEdgeIdx(0),
            face: face_a,
        });
        arena.loops.push(Loop {
            half_edge: HalfEdgeIdx(1),
            face: face_b,
        });

        // Two half-edges forming twins
        let he_a = HalfEdgeIdx(arena.half_edges.len());
        arena.half_edges.push(HalfEdge {
            origin: VertexIdx(0),
            edge: EdgeIdx(0),
            loop_: LoopIdx(0),
            next: he_a,
            prev: he_a,
            twin: Some(HalfEdgeIdx(1)),
        });
        let he_b = HalfEdgeIdx(arena.half_edges.len());
        arena.half_edges.push(HalfEdge {
            origin: VertexIdx(1),
            edge: EdgeIdx(0),
            loop_: LoopIdx(1),
            next: he_b,
            prev: he_b,
            twin: Some(HalfEdgeIdx(0)),
        });

        let edge_idx = EdgeIdx(arena.edges.len());
        arena.edges.push(Edge { half_edge: he_a });

        (arena, edge_idx, face_a, face_b)
    }

    #[test]
    fn is_smooth_edge_coplanar_faces_same_plane() {
        let (arena, edge_idx, face_a, face_b) = make_two_face_arena();
        let mut face_geometry = BTreeMap::new();
        let plane = SurfaceGeom::Planar(crate::geometry::surface::Plane {
            origin: crate::geometry::point::Point3::new(0.0, 0.0, 0.0),
            normal: crate::geometry::point::Vector3::new(0.0, 0.0, 1.0),
        });
        face_geometry.insert(face_a, plane.clone());
        face_geometry.insert(face_b, plane);

        assert!(
            is_smooth_edge(&arena, edge_idx, &face_geometry),
            "Co-planar faces on the same plane should be smooth"
        );
    }

    #[test]
    fn is_smooth_edge_parallel_planes_different_distance() {
        let (arena, edge_idx, face_a, face_b) = make_two_face_arena();
        let mut face_geometry = BTreeMap::new();
        face_geometry.insert(
            face_a,
            SurfaceGeom::Planar(crate::geometry::surface::Plane {
                origin: crate::geometry::point::Point3::new(0.0, 0.0, 0.0),
                normal: crate::geometry::point::Vector3::new(0.0, 0.0, 1.0),
            }),
        );
        face_geometry.insert(
            face_b,
            SurfaceGeom::Planar(crate::geometry::surface::Plane {
                origin: crate::geometry::point::Point3::new(0.0, 0.0, 1.0),
                normal: crate::geometry::point::Vector3::new(0.0, 0.0, 1.0),
            }),
        );

        assert!(
            !is_smooth_edge(&arena, edge_idx, &face_geometry),
            "Parallel planes at different distances should NOT be smooth"
        );
    }

    #[test]
    fn is_smooth_edge_perpendicular_planes() {
        let (arena, edge_idx, face_a, face_b) = make_two_face_arena();
        let mut face_geometry = BTreeMap::new();
        face_geometry.insert(
            face_a,
            SurfaceGeom::Planar(crate::geometry::surface::Plane {
                origin: crate::geometry::point::Point3::new(0.0, 0.0, 0.0),
                normal: crate::geometry::point::Vector3::new(0.0, 0.0, 1.0),
            }),
        );
        face_geometry.insert(
            face_b,
            SurfaceGeom::Planar(crate::geometry::surface::Plane {
                origin: crate::geometry::point::Point3::new(0.0, 0.0, 0.0),
                normal: crate::geometry::point::Vector3::new(1.0, 0.0, 0.0),
            }),
        );

        assert!(
            !is_smooth_edge(&arena, edge_idx, &face_geometry),
            "Perpendicular planes should NOT be smooth"
        );
    }

    #[test]
    fn tessellate_planar_face_no_collinear_degenerates() {
        // 8-vertex rectangle with collinear intermediate points on the long edges
        // (simulates Yang coplanar-merge output where intersection-plane vertices
        // remain on merged face boundaries).
        let positions: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 0.0], // 0 - corner
            [1.0, 0.0, 0.0], // 1 - intermediate (collinear with 0 and 2)
            [2.0, 0.0, 0.0], // 2 - intermediate (collinear with 1 and 3)
            [3.0, 0.0, 0.0], // 3 - corner
            [3.0, 2.0, 0.0], // 4 - corner
            [2.0, 2.0, 0.0], // 5 - intermediate (collinear with 4 and 6)
            [1.0, 2.0, 0.0], // 6 - intermediate (collinear with 5 and 7)
            [0.0, 2.0, 0.0], // 7 - corner
        ];
        let boundary: Vec<usize> = (0..8).collect();
        let normal = [0.0f32, 0.0, 1.0];
        let mut verts = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();

        tessellate_planar_face_bounded(
            &boundary,
            &positions,
            normal,
            [0.0, 0.0, 0.0], // plane origin (z=0 plane)
            &mut verts,
            &mut normals,
            &mut indices,
            &[],
        );

        // Verify no degenerate triangles
        let tri_count = indices.len() / 3;
        assert!(tri_count > 0, "should produce at least one triangle");
        for t in 0..tri_count {
            let i0 = indices[t * 3] as usize;
            let i1 = indices[t * 3 + 1] as usize;
            let i2 = indices[t * 3 + 2] as usize;
            let p0 = [
                verts[i0 * 3] as f64,
                verts[i0 * 3 + 1] as f64,
                verts[i0 * 3 + 2] as f64,
            ];
            let p1 = [
                verts[i1 * 3] as f64,
                verts[i1 * 3 + 1] as f64,
                verts[i1 * 3 + 2] as f64,
            ];
            let p2 = [
                verts[i2 * 3] as f64,
                verts[i2 * 3 + 1] as f64,
                verts[i2 * 3 + 2] as f64,
            ];
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let cross_len = ((e1[1] * e2[2] - e1[2] * e2[1]).powi(2)
                + (e1[2] * e2[0] - e1[0] * e2[2]).powi(2)
                + (e1[0] * e2[1] - e1[1] * e2[0]).powi(2))
            .sqrt();
            assert!(
                cross_len > crate::units::TAU_NORMALIZE,
                "triangle {t} is degenerate (area={cross_len})"
            );
        }
    }

}
