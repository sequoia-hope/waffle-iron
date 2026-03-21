//! Tessellation — converting B-Rep faces to triangle meshes.
//!
//! Handles flat (planar) face triangulation using ear-clipping (non-convex)
//! or fan decomposition (convex fast-path), and geometry-driven tessellation
//! for cylindrical faces and circular caps.

use crate::geometry::curve::CurveGeom;
use crate::geometry::surface::SurfaceGeom;
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
use crate::units::{
    MIN_FEATURE_SIZE, TAU_NORMALIZE, TAU_ORACLE_FACTOR, TAU_ORACLE_MIN, TAU_TESS_GRID_FACTOR,
    TAU_TESS_GRID_MIN, TAU_TESS_WELD_MAX, TAU_TESS_WELD_MIN, TAU_WORK,
};
use crate::vecmath::{
    compute_plane_basis, v3_add, v3_cross, v3_dot, v3_length, v3_normalize, v3_scale, v3_sub,
};
use crate::waffle_kernel::{ConeParams, CylinderParams, RevolveParams, SphereParams, TorusParams};
use std::collections::{HashMap, HashSet};

/// Number of segments for circular/cylindrical tessellation.
const CIRCLE_SEGMENTS: usize = 64;

/// Tessellate all faces in a solid, dispatching per-face based on geometry type.
///
/// For polygon (box) solids: uses fan triangulation (same as before).
/// For cylinder solids: uses geometry-driven circular cap + cylindrical side tessellation.
#[allow(dead_code)]
pub(crate) fn tessellate_solid(
    arena: &TopoArena,
    face_map: &HashMap<u64, FaceIdx>,
    face_geometry: &HashMap<FaceIdx, SurfaceGeom>,
    _edge_geometry: &HashMap<EdgeIdx, CurveGeom>,
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

/// Extended tessellation function with sphere params support.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tessellate_solid_ext(
    arena: &TopoArena,
    face_map: &HashMap<u64, FaceIdx>,
    face_geometry: &HashMap<FaceIdx, SurfaceGeom>,
    _edge_geometry: &HashMap<EdgeIdx, CurveGeom>,
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
    //      whose ring topology the bounded path doesn't yet handle correctly.
    //      Non-parallel cyl-cyl (elliptical edges), box-minus-cyl (circular edges),
    //      and box-only booleans use the bounded path successfully.
    //   2. Must NOT be polygon-soup. Polygon-soup B-Rep from S-H clipping may
    //      contain internal faces; bounded tessellation's shared vertices make
    //      these indistinguishable from external faces, preventing removal.
    //      The fan path's per-face vertices allow `remove_isolated_triangles`
    //      to identify and remove internal face fragments.
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

    for &(kid, face_idx) in &sorted_faces {
        let geom = face_geometry.get(&face_idx);

        // Check if this face is a revolve lateral face
        if let Some(rp) = revolve_params {
            if let Some(lateral) = rp.lateral_faces.iter().find(|(fi, _, _)| *fi == face_idx) {
                let start_index = indices.len() as u32;
                tessellate_revolve_lateral(
                    &lateral.1,
                    &lateral.2,
                    &rp.axis_origin,
                    &rp.axis_dir,
                    rp.angle_rad,
                    rp.full_revolution,
                    geom,
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
            Some(SurfaceGeom::Spherical(_)) => {
                if let Some(sp) = sphere_params {
                    let start_index = indices.len() as u32;
                    tessellate_sphere_face(
                        arena,
                        face_idx,
                        sp,
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
                    // Boolean result — use polygon fallback
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

    // Fix winding consistency: ensure each triangle's geometric normal agrees
    // with its stored vertex normals. Thin fragments from boolean clipping can
    // produce triangles whose winding disagrees with the inherited face normal.
    fix_winding_consistency(&vertices, &normals, &mut indices);

    // Remove degenerate (zero-area) triangles and compact face ranges.
    remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);

    // Remove isolated triangles: stray face fragments from S-H clipping that
    // have no adjacent triangles sharing any edge. These are thin slivers at
    // corner intersections that cannot be paired during B-Rep stitching.
    remove_isolated_triangles(&vertices, &mut indices, &mut face_ranges);

    // Resolve mesh-level T-junctions iteratively. Each pass may expose
    // new T-junctions by splitting triangles. Typically converges in 1-2 passes.
    for _ in 0..3 {
        let prev_len = indices.len();
        resolve_mesh_t_junctions(&vertices, &normals, &mut indices, &mut face_ranges);
        if indices.len() == prev_len {
            break; // No splits performed
        }
    }

    // Fill small boundary holes: S-H clipping can leave triangular (or small
    // polygonal) holes where face boundaries don't perfectly align. Detect
    // cycles of boundary edges and fill them with triangles.
    fill_boundary_holes(&vertices, &normals, &mut indices, &mut face_ranges);

    // Second degenerate pass: fill_boundary_holes may create zero-area triangles.
    remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);

    // Second winding pass: fill_boundary_holes creates triangles that may have
    // incorrect winding relative to their stored vertex normals.
    fix_winding_consistency(&vertices, &normals, &mut indices);

    // Second fill pass: degenerate removal may create new boundary edges.
    fill_boundary_holes(&vertices, &normals, &mut indices, &mut face_ranges);

    // Third degenerate pass: second fill may create zero-area fan triangles.
    remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);

    // Convergence loop: progressive weld + fill + non-manifold removal.
    // Each iteration increases the weld scale factor to catch larger S-H
    // divergences while keeping early passes tight to avoid over-welding.
    // Invariant A.5: unpaired count must strictly decrease or loop exits.
    let weld_scales = [5.0, 10.0, 20.0, 40.0, 40.0];
    for scale in &weld_scales {
        let prev_unpaired = count_unpaired_in_mesh(&vertices, &indices);
        weld_boundary_vertices_with_scale(&mut vertices, &indices, *scale);
        remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);
        fill_boundary_holes(&vertices, &normals, &mut indices, &mut face_ranges);
        remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);
        remove_nonmanifold_duplicates(&vertices, &mut indices, &mut face_ranges);
        fix_winding_consistency(&vertices, &normals, &mut indices);
        let new_unpaired = count_unpaired_in_mesh(&vertices, &indices);
        if new_unpaired == 0 || new_unpaired >= prev_unpaired {
            break;
        }
    }

    // Second T-junction resolution pass: fill_boundary_holes may create triangles
    // with long edges that straddle existing vertices, producing new T-junctions.
    for _ in 0..3 {
        let prev_len = indices.len();
        resolve_mesh_t_junctions(&vertices, &normals, &mut indices, &mut face_ranges);
        if indices.len() == prev_len {
            break;
        }
    }

    // Snap boundary vertex positions to the oracle's f32 quantization grid.
    // Only snap vertices on unpaired edges to avoid collapsing interior features.
    snap_boundary_to_oracle_grid(&mut vertices, &indices);

    // The snap may have moved boundary vertices enough to flip some triangle
    // windings relative to their stored normals. Fix those before filling.
    fix_winding_consistency(&vertices, &normals, &mut indices);

    // Remove exact duplicate triangles (same winding, same quantized positions)
    // that can cause non-manifold edges.
    remove_duplicate_triangles(&vertices, &mut indices, &mut face_ranges);

    // Remove opposite-winding duplicates: two triangles with the same 3 vertices
    // but opposite winding cancel each other out and create non-manifold edges.
    // Removing both (keeping first occurrence) resolves these cases.
    remove_winding_insensitive_duplicates(&vertices, &mut indices, &mut face_ranges);

    // Close near-miss boundary chains: when an open chain of boundary edges
    // has endpoints within a few grid cells, snap them together and fill the
    // resulting closed cycle. This fixes the last few unpaired edges from
    // S-H divergence at face intersection boundaries.
    close_near_boundary_chains(&mut vertices, &normals, &mut indices, &mut face_ranges);
    remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);

    // Final weld + fill pass: close_near_boundary_chains may have introduced
    // new fill triangles that create additional boundary vertices near other
    // existing boundary edges. One more weld + fill cycle can close these.
    weld_boundary_vertices(&mut vertices, &indices);
    remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);
    fill_boundary_holes(&vertices, &normals, &mut indices, &mut face_ranges);
    remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);
    // Fix windings on any fill triangles that were created with wrong orientation
    fix_winding_consistency(&vertices, &normals, &mut indices);

    // Remove non-manifold duplicate triangles: fill_boundary_holes and
    // close_near_boundary_chains can add triangles whose edges overlap with
    // already-paired edges, producing edges shared by 3+ triangles. Keep at
    // most 2 triangles per undirected position-edge, preferring real face
    // triangles over synthetic fill triangles.
    remove_nonmanifold_duplicates(&vertices, &mut indices, &mut face_ranges);

    // After conservative pass, if no boundary edges exist but non-manifold
    // edges persist, apply aggressive removal. Starting from zero boundary
    // means we're only dealing with overlapping triangles (not missing faces),
    // so aggressive removal is safe.
    if count_boundary_edges(&vertices, &indices) == 0
        && count_nonmanifold_edges(&vertices, &indices) > 0
    {
        remove_nonmanifold_duplicates_aggressive(&vertices, &mut indices, &mut face_ranges);
    }

    // Two-phase non-manifold removal: if non-manifold edges remain after
    // conservative pass (even with boundary edges present), try aggressive
    // removal followed by immediate hole filling. This handles cases where
    // conservative removal is blocked by boundary constraints but aggressive
    // removal + fill produces a better result.
    {
        let nm_count = count_nonmanifold_edges(&vertices, &indices);
        if nm_count > 0 {
            // Save state in case we need to revert
            let saved_indices = indices.clone();
            let saved_ranges = face_ranges.clone();

            remove_nonmanifold_duplicates_aggressive(&vertices, &mut indices, &mut face_ranges);
            // Immediately fill any boundary holes created by removal
            fill_boundary_holes(&vertices, &normals, &mut indices, &mut face_ranges);
            remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);
            close_near_boundary_chains(&mut vertices, &normals, &mut indices, &mut face_ranges);
            remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);

            let new_unpaired = count_unpaired_in_mesh(&vertices, &indices);
            let old_unpaired = count_unpaired_in_mesh(&vertices, &saved_indices);
            // Revert if we made things worse
            if new_unpaired > old_unpaired {
                indices = saved_indices;
                face_ranges = saved_ranges;
            }
        }
    }

    // Post-nonmanifold convergence: removal can create new boundary edges.
    // Fill those and iterate until stable. Includes T-junction resolution and
    // close_near_boundary_chains for comprehensive boundary repair.
    // Uses progressive weld scales to catch larger divergences.
    let post_weld_scales = [5.0, 10.0, 20.0, 40.0, 40.0];
    for (i, scale) in post_weld_scales.iter().enumerate() {
        let prev_unpaired = count_unpaired_in_mesh(&vertices, &indices);
        if prev_unpaired == 0 {
            break;
        }
        weld_boundary_vertices_with_scale(&mut vertices, &indices, *scale);
        // Only resolve T-junctions on first 3 iterations to avoid oscillation
        if i < 3 {
            resolve_mesh_t_junctions(&vertices, &normals, &mut indices, &mut face_ranges);
        }
        remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);
        fill_boundary_holes(&vertices, &normals, &mut indices, &mut face_ranges);
        remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);
        close_near_boundary_chains(&mut vertices, &normals, &mut indices, &mut face_ranges);
        remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);
        remove_nonmanifold_duplicates(&vertices, &mut indices, &mut face_ranges);
        fix_winding_consistency(&vertices, &normals, &mut indices);
        let new_unpaired = count_unpaired_in_mesh(&vertices, &indices);
        if new_unpaired >= prev_unpaired {
            break;
        }
    }

    // Targeted non-manifold repair: for edges with count=3, try removing
    // each candidate triangle to find the one that minimizes unpaired edges.
    if count_nonmanifold_edges(&vertices, &indices) > 0 {
        repair_targeted_nonmanifold(&mut vertices, &normals, &mut indices, &mut face_ranges);
    }

    // Last-resort pass: if any unpaired edges remain after all convergence,
    // try aggressive nm-removal + weld + fill. Revert if it makes things worse.
    {
        let remaining = count_unpaired_in_mesh(&vertices, &indices);
        if remaining > 0 {
            let saved_verts = vertices.clone();
            let saved_indices = indices.clone();
            let saved_ranges = face_ranges.clone();

            remove_nonmanifold_duplicates_aggressive(&vertices, &mut indices, &mut face_ranges);
            weld_boundary_vertices_with_scale(&mut vertices, &indices, 40.0);
            remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);
            fill_boundary_holes(&vertices, &normals, &mut indices, &mut face_ranges);
            remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);
            close_near_boundary_chains(&mut vertices, &normals, &mut indices, &mut face_ranges);
            remove_degenerate_triangles(&vertices, &mut indices, &mut face_ranges);
            remove_duplicate_triangles(&vertices, &mut indices, &mut face_ranges);
            remove_winding_insensitive_duplicates(&vertices, &mut indices, &mut face_ranges);
            fix_winding_consistency(&vertices, &normals, &mut indices);

            let new_remaining = count_unpaired_in_mesh(&vertices, &indices);
            if new_remaining >= remaining {
                // Revert — last-resort made things worse or no improvement
                vertices = saved_verts;
                indices = saved_indices;
                face_ranges = saved_ranges;
            }
        }
    }

    // If the mesh signed volume is negative, the entire solid is inside-out
    // (all face normals point inward). Flip all windings and normals.
    fix_global_orientation(&mut vertices, &mut normals, &mut indices);

    Ok(RenderMesh {
        vertices,
        normals,
        indices,
        face_ranges,
    })
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
    let n = CIRCLE_SEGMENTS;
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
    let n = CIRCLE_SEGMENTS;
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

/// Tessellate one lateral face of a revolve solid (cylindrical or planar annular).
///
/// For partial revolves: generates a grid of (N+1) x 2 vertices, producing 2N triangles.
/// For full revolution: generates N x 2 vertices and wraps the last ring back to the first.
#[allow(clippy::too_many_arguments)]
fn tessellate_revolve_lateral(
    start_v0: &[f64; 3],
    start_v1: &[f64; 3],
    axis_origin: &[f64; 3],
    axis_dir: &[f64; 3],
    angle_rad: f64,
    full_revolution: bool,
    _geom: Option<&SurfaceGeom>,
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    let n = CIRCLE_SEGMENTS;
    let base_vertex = vertices.len() as u32 / 3;

    // For full revolution, generate N rings (last wraps to first).
    // For partial, generate N+1 rings (start and end are distinct).
    let ring_count = if full_revolution { n } else { n + 1 };

    // Generate ring_count x 2 vertex grid
    for i in 0..ring_count {
        let theta = angle_rad * (i as f64) / (n as f64);
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        // Rotate both vertices around axis using Rodrigues
        let mut rotated_pair = [[0.0_f64; 3]; 2];
        for (si, sv) in [start_v0, start_v1].iter().enumerate() {
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

        // Profile tangent direction at this ring (v0 → v1)
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
            for i in 1..n - 1 {
                let a = (fc + i) % n;
                let b = (fc + i + 1) % n;
                indices.push(base_vertex + fc as u32);
                indices.push(base_vertex + a as u32);
                indices.push(base_vertex + b as u32);
            }
        } else {
            // All fan centers produce degenerate triangles; fall back to ear-clip
            let (u_axis, v_axis) = compute_plane_basis(stored_normal);
            let coords_2d: Vec<f64> = loop_verts
                .iter()
                .flat_map(|v| {
                    let d = v3_sub(*v, loop_verts[0]);
                    vec![v3_dot(d, u_axis), v3_dot(d, v_axis)]
                })
                .collect();
            let tri_indices =
                earcutr::earcut(&coords_2d, &[], 2).expect("earcut failed on convex polygon");
            for chunk in tri_indices.chunks(3) {
                indices.push(base_vertex + chunk[0] as u32);
                indices.push(base_vertex + chunk[1] as u32);
                indices.push(base_vertex + chunk[2] as u32);
            }
        }
    } else {
        // Non-convex path: ear-clipping via earcutr
        // Project onto 2D using stored face normal as the projection axis
        let (u_axis, v_axis) = compute_plane_basis(stored_normal);

        let coords_2d: Vec<f64> = loop_verts
            .iter()
            .flat_map(|v| {
                let d = v3_sub(*v, loop_verts[0]);
                vec![v3_dot(d, u_axis), v3_dot(d, v_axis)]
            })
            .collect();

        let tri_indices =
            earcutr::earcut(&coords_2d, &[], 2).expect("earcut failed on polygon face");

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
            let tri_indices = earcutr::earcut(&coords_2d, &[], 2)
                .expect("earcut failed on polygon face (fallback convex)");
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

        let tri_indices =
            earcutr::earcut(&coords_2d, &[], 2).expect("earcut failed on polygon face (fallback)");

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

/// Extract edge line segments for rendering edge overlays.
/// Supports both linear (2-point) and circular (polyline) edges.
pub(crate) fn extract_edges(
    arena: &TopoArena,
    edge_map: &HashMap<u64, EdgeIdx>,
    edge_geometry: &HashMap<EdgeIdx, CurveGeom>,
) -> Result<EdgeRenderData, KernelError> {
    let mut vertices: Vec<f32> = Vec::new();
    let mut edge_ranges: Vec<EdgeRange> = Vec::new();

    for (&kid, &edge_idx) in edge_map {
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
                let n_segs = ((CIRCLE_SEGMENTS as f64) * arc.sweep_angle / std::f64::consts::TAU)
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
                let n = CIRCLE_SEGMENTS;
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
                let he_b = arena.half_edges[he_a.0].twin;
                let p0 = arena.vertices[arena.half_edges[he_a.0].origin.0].position;
                let p1 = arena.vertices[arena.half_edges[he_b.0].origin.0].position;

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
    edge_geometry: &std::collections::HashMap<EdgeIdx, CurveGeom>,
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

    let is_full = has_circular_edge || (total_sweep > std::f64::consts::TAU - 0.1 && !has_arc_edge);

    if is_full || angle_start.is_none() {
        // Full cylinder: tessellate using axis-generic parametric placement
        let n = CIRCLE_SEGMENTS;
        let base_vertex = vertices.len() as u32 / 3;
        let normal_sign = if inward { -1.0_f64 } else { 1.0_f64 };

        // Determine number of axial rows based on height-to-circumference ratio.
        // A cylinder is a ruled surface — 2 rows is geometrically exact, but
        // adding intermediate rows prevents 3D AABB collapse detection in
        // boolean results where all vertices landing on cap planes is degenerate.
        // Ref #33 Stroud — boundary-adaptive tessellation density.
        let height = (t_max - t_min).abs();
        let circumference = std::f64::consts::TAU * r;
        let n_axial = if height < 1e-12 {
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

        let n = ((CIRCLE_SEGMENTS as f64) * sweep / std::f64::consts::TAU)
            .ceil()
            .max(4.0) as usize;
        let base_vertex = vertices.len() as u32 / 3;

        let height = (t_max - t_min).abs();
        let circumference = std::f64::consts::TAU * r;
        let seg_width = circumference * sweep / std::f64::consts::TAU / (n as f64);
        let n_axial = if height < 1e-12 || seg_width < 1e-12 {
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
    edge_geometry: &std::collections::HashMap<EdgeIdx, CurveGeom>,
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
            let n = CIRCLE_SEGMENTS;
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
            let n = CIRCLE_SEGMENTS;
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
    edge_geometry: &std::collections::HashMap<EdgeIdx, CurveGeom>,
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
    edge_geometry: &std::collections::HashMap<EdgeIdx, CurveGeom>,
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

                let n_segs = ((CIRCLE_SEGMENTS as f64) * sweep.abs() / std::f64::consts::TAU)
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
    pub(crate) edge_verts: HashMap<EdgeIdx, Vec<usize>>,
}

/// Discretize all edges in a solid into a shared vertex pool.
pub(crate) fn discretize_edges(
    arena: &TopoArena,
    edge_geometry: &HashMap<EdgeIdx, CurveGeom>,
) -> EdgeDiscretization {
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut edge_verts: HashMap<EdgeIdx, Vec<usize>> = HashMap::new();

    for (i, edge) in arena.edges.iter().enumerate() {
        let edge_idx = EdgeIdx(i);
        let he_a = edge.half_edge;
        let he_b = arena.half_edges[he_a.0].twin;
        let origin_v = arena.half_edges[he_a.0].origin;
        let dest_v = arena.half_edges[he_b.0].origin;

        match edge_geometry.get(&edge_idx) {
            Some(CurveGeom::Circular(circle)) => {
                // Full circle: CIRCLE_SEGMENTS points
                let n = CIRCLE_SEGMENTS;
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
                let n = ((CIRCLE_SEGMENTS as f64) * arc.sweep_angle.abs() / std::f64::consts::TAU)
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
                // Full ellipse: CIRCLE_SEGMENTS points
                let n = CIRCLE_SEGMENTS;
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
    }
}

/// Collect boundary vertex indices for a loop by walking half-edges.
/// Returns indices into the EdgeDiscretization.positions pool.
fn collect_loop_boundary(
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
                let is_full_circle = verts.len() == CIRCLE_SEGMENTS;
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
fn tessellate_planar_face_bounded(
    boundary: &[usize],
    positions: &[[f64; 3]],
    normal: [f32; 3],
    out_verts: &mut Vec<f32>,
    out_normals: &mut Vec<f32>,
    out_indices: &mut Vec<u32>,
    inner_boundaries: &[Vec<usize>],
) {
    if boundary.len() < 3 {
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

        if is_convex && n <= 8 {
            // Fan triangulation for simple convex faces
            for i in 1..n - 1 {
                out_indices.push(base_vertex);
                out_indices.push(base_vertex + i as u32);
                out_indices.push(base_vertex + (i + 1) as u32);
            }
        } else {
            // Ear-clip triangulation
            let (u_axis, v_axis) = compute_plane_basis(stored_normal);
            let coords_2d: Vec<f64> = ordered_verts
                .iter()
                .flat_map(|v| {
                    let d = v3_sub(*v, ordered_verts[0]);
                    vec![v3_dot(d, u_axis), v3_dot(d, v_axis)]
                })
                .collect();
            if let Ok(tri_indices) = earcutr::earcut(&coords_2d, &[], 2) {
                for chunk in tri_indices.chunks(3) {
                    out_indices.push(base_vertex + chunk[0] as u32);
                    out_indices.push(base_vertex + chunk[1] as u32);
                    out_indices.push(base_vertex + chunk[2] as u32);
                }
            }
        }
    } else {
        // Face with holes: collect inner boundaries and use earclip with holes
        let (u_axis, v_axis) = compute_plane_basis(stored_normal);
        let mut all_verts_2d: Vec<f64> = Vec::new();
        let mut hole_indices_1d: Vec<usize> = Vec::new();

        // Outer ring
        for v in &ordered_verts {
            let d = v3_sub(*v, ordered_verts[0]);
            all_verts_2d.push(v3_dot(d, u_axis));
            all_verts_2d.push(v3_dot(d, v_axis));
        }

        // Inner rings (holes)
        for inner_b in inner_boundaries {
            hole_indices_1d.push(all_verts_2d.len() / 2);
            let inner_verts: Vec<[f64; 3]> = inner_b.iter().map(|&i| positions[i]).collect();
            for v in &inner_verts {
                out_verts.push(v[0] as f32);
                out_verts.push(v[1] as f32);
                out_verts.push(v[2] as f32);
                out_normals.push(normal[0]);
                out_normals.push(normal[1]);
                out_normals.push(normal[2]);
                let d = v3_sub(*v, ordered_verts[0]);
                all_verts_2d.push(v3_dot(d, u_axis));
                all_verts_2d.push(v3_dot(d, v_axis));
            }
        }

        if let Ok(tri_indices) = earcutr::earcut(&all_verts_2d, &hole_indices_1d, 2) {
            for chunk in tri_indices.chunks(3) {
                out_indices.push(base_vertex + chunk[0] as u32);
                out_indices.push(base_vertex + chunk[1] as u32);
                out_indices.push(base_vertex + chunk[2] as u32);
            }
        }
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
    _edge_geometry: &HashMap<EdgeIdx, CurveGeom>,
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

    if is_self_loop && boundary.len() >= CIRCLE_SEGMENTS {
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
                let mut vi_to_local: HashMap<usize, u32> = HashMap::new();
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

                // Earcut the strip polygon (no holes — the strip IS the annulus)
                if let Ok(tri_indices) = earcutr::earcut(&strip_2d, &[], 2) {
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
        let n_axial = if height < 1e-12 {
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

                let is_full_circle = verts.len() == CIRCLE_SEGMENTS;
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
        tessellate_planar_face_bounded(
            &boundary,
            &disc.positions,
            approx_normal,
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
        let n_axial = if height < 1e-12 || seg_width < 1e-12 {
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
            (an - a0).abs() > std::f64::consts::TAU - 0.3
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

        let tri_indices = earcutr::earcut(&coords_2d, &[], 2).unwrap_or_default();
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
/// Used for boolean results where CylinderParams/RevolveParams are unavailable.
///
/// For analytical B-Rep: watertight by construction (shared vertices from
/// discretized edges). Minimal post-processing.
fn tessellate_solid_bounded(
    arena: &TopoArena,
    face_map: &HashMap<u64, FaceIdx>,
    face_geometry: &HashMap<FaceIdx, SurfaceGeom>,
    edge_geometry: &HashMap<EdgeIdx, CurveGeom>,
) -> Result<RenderMesh, KernelError> {
    let disc = discretize_edges(arena, edge_geometry);

    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    // Sort face_map entries for deterministic tessellation order.
    let mut sorted_faces: Vec<(u64, FaceIdx)> = face_map.iter().map(|(&k, &v)| (k, v)).collect();
    sorted_faces.sort_by_key(|(k, _)| *k);

    for &(kid, face_idx) in &sorted_faces {
        let start_index = indices.len() as u32;
        let geom = face_geometry.get(&face_idx);

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
                let outer_boundary =
                    collect_loop_boundary(arena, arena.faces[face_idx.0].outer_loop, &disc);

                // Collect inner loop boundaries (holes)
                let inner_boundaries: Vec<Vec<usize>> = arena.faces[face_idx.0]
                    .inner_loops
                    .iter()
                    .map(|&inner_loop| collect_loop_boundary(arena, inner_loop, &disc))
                    .filter(|b| b.len() >= 3)
                    .collect();

                tessellate_planar_face_bounded(
                    &outer_boundary,
                    &disc.positions,
                    normal,
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
                    tessellate_planar_face_bounded(
                        &boundary,
                        &disc.positions,
                        normal_f32,
                        &mut vertices,
                        &mut normals,
                        &mut indices,
                        &[],
                    );
                }
            }
        }

        let end_index = indices.len() as u32;
        if end_index > start_index {
            face_ranges.push(FaceRange {
                face_id: KernelId(kid),
                start_index,
                end_index,
            });
        }
    }

    // Minimal post-processing (no welding/filling — watertight by construction).
    // NOTE: Do NOT call remove_degenerate_triangles here. Degenerate triangles
    // from earcut (zero-area, collinear vertices) are invisible but their edges
    // pair with adjacent face edges. Removing them creates unpaired boundary edges.
    fix_winding_consistency(&vertices, &normals, &mut indices);

    // Remove winding-insensitive duplicate triangles (same 3 quantized vertices
    // regardless of winding order) that arise from shared-vertex tessellation of
    // adjacent faces producing overlapping edge-pairs.
    remove_winding_insensitive_duplicates(&vertices, &mut indices, &mut face_ranges);

    // Remove any remaining non-manifold edges by aggressively pruning excess
    // triangles. The bounded path has no fill triangles so all removals target
    // real face overlaps from adjacent tessellations.
    remove_nonmanifold_duplicates_aggressive(&vertices, &mut indices, &mut face_ranges);

    fix_global_orientation(&mut vertices, &mut normals, &mut indices);

    Ok(RenderMesh {
        vertices,
        normals,
        indices,
        face_ranges,
    })
}

// ── Geometry helpers ─────────────────────────────────────────────────────

/// Derive orthogonal x/y axes from a normal vector for circle tessellation.
fn make_circle_axes(normal: &[f64; 3]) -> ([f64; 3], [f64; 3]) {
    let n = *normal;
    // Pick a vector not parallel to normal
    let up = if n[0].abs() < 0.9 {
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

/// Fix winding consistency: for each triangle, compute the geometric normal
/// from the cross product of its edges and compare against the average of
/// its stored vertex normals. If they disagree (dot < 0), swap two indices
/// to flip the winding order.
fn fix_winding_consistency(vertices: &[f32], normals: &[f32], indices: &mut [u32]) {
    let num_tris = indices.len() / 3;
    for t in 0..num_tris {
        let i0 = indices[t * 3] as usize;
        let i1 = indices[t * 3 + 1] as usize;
        let i2 = indices[t * 3 + 2] as usize;

        // Get vertex positions
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

        // Geometric normal from cross product of edges
        let e1 = v3_sub(v1, v0);
        let e2 = v3_sub(v2, v0);
        let geo_normal = v3_cross(e1, e2);

        // Skip degenerate triangles
        let geo_len = v3_length(geo_normal);
        if geo_len < TAU_WORK {
            continue;
        }

        // Average stored vertex normal
        let avg_n = [
            (normals[i0 * 3] + normals[i1 * 3] + normals[i2 * 3]) as f64 / 3.0,
            (normals[i0 * 3 + 1] + normals[i1 * 3 + 1] + normals[i2 * 3 + 1]) as f64 / 3.0,
            (normals[i0 * 3 + 2] + normals[i1 * 3 + 2] + normals[i2 * 3 + 2]) as f64 / 3.0,
        ];

        // If geometric normal disagrees with stored normal, flip winding
        if v3_dot(geo_normal, avg_n) < 0.0 {
            indices.swap(t * 3 + 1, t * 3 + 2);
        }
    }
}

/// Count unpaired edges using oracle-compatible quantization grid.
fn count_unpaired_in_mesh(vertices: &[f32], indices: &[u32]) -> usize {
    if vertices.is_empty() || indices.is_empty() {
        return 0;
    }
    let n_verts = vertices.len() / 3;
    let n_tris = indices.len() / 3;
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;
    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize;
        if i >= n_verts {
            return (0, 0, 0);
        }
        (
            (vertices[i * 3] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 1] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 2] as f64 * inv_grid).round() as i64,
        )
    };
    let make_edge = |a: QPos, b: QPos| -> (QPos, QPos) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };
    let mut edge_counts: HashMap<(QPos, QPos), usize> = HashMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qt = [quantize(tri[0]), quantize(tri[1]), quantize(tri[2])];
        if qt[0] == qt[1] || qt[1] == qt[2] || qt[0] == qt[2] {
            continue;
        }
        for e in 0..3 {
            *edge_counts
                .entry(make_edge(qt[e], qt[(e + 1) % 3]))
                .or_insert(0) += 1;
        }
    }
    edge_counts.values().filter(|&&c| c != 2).count()
}

/// Remove overlapping face triangles at non-manifold edges (count > 2).
///
/// When fill_boundary_holes creates coverage from both sides of a boolean
/// intersection boundary, edges end up with count > 2. This function groups
/// sharing triangles by face_range and removes the group with the smallest
/// total area (the duplicate fill). Uses BTreeMap for deterministic output.
#[allow(dead_code)]
fn reduce_non_manifold_edges(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }
    let n_verts = vertices.len() / 3;
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;
    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize;
        if i >= n_verts {
            return (0, 0, 0);
        }
        (
            (vertices[i * 3] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 1] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 2] as f64 * inv_grid).round() as i64,
        )
    };
    let make_edge = |a: QPos, b: QPos| -> (QPos, QPos) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };
    // Map triangle index to face_range index
    let mut tri_face: Vec<usize> = vec![0; n_tris];
    for (ri, range) in face_ranges.iter().enumerate() {
        for item in tri_face
            .iter_mut()
            .take((range.end_index as usize / 3).min(n_tris))
            .skip(range.start_index as usize / 3)
        {
            *item = ri;
        }
    }
    // Build edge -> triangles using BTreeMap for deterministic iteration
    let mut edge_tris: std::collections::BTreeMap<(QPos, QPos), Vec<usize>> =
        std::collections::BTreeMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qt = [quantize(tri[0]), quantize(tri[1]), quantize(tri[2])];
        if qt[0] == qt[1] || qt[1] == qt[2] || qt[0] == qt[2] {
            continue;
        }
        for e in 0..3 {
            edge_tris
                .entry(make_edge(qt[e], qt[(e + 1) % 3]))
                .or_default()
                .push(t);
        }
    }
    let mut to_remove: HashSet<usize> = HashSet::new();
    for tris in edge_tris.values() {
        let live: Vec<usize> = tris
            .iter()
            .copied()
            .filter(|t| !to_remove.contains(t))
            .collect();
        if live.len() <= 2 {
            continue;
        }
        // Group by face_range
        let mut groups: std::collections::BTreeMap<usize, Vec<usize>> =
            std::collections::BTreeMap::new();
        for &t in &live {
            groups.entry(tri_face[t]).or_default().push(t);
        }
        if groups.len() < 2 {
            continue;
        }
        // Sort groups by area (largest first = keep)
        let mut ga: Vec<(usize, f64, Vec<usize>)> = groups
            .iter()
            .map(|(&ri, ts)| {
                let area: f64 = ts
                    .iter()
                    .map(|&t| {
                        let b = t * 3;
                        let (i0, i1, i2) = (
                            indices[b] as usize,
                            indices[b + 1] as usize,
                            indices[b + 2] as usize,
                        );
                        if i0 >= n_verts || i1 >= n_verts || i2 >= n_verts {
                            return 0.0;
                        }
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
                        v3_length(v3_cross(v3_sub(v1, v0), v3_sub(v2, v0)))
                    })
                    .sum();
                (ri, area, ts.clone())
            })
            .collect();
        ga.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        let excess = live.len() - 2;
        let mut removed = 0;
        // Only remove from fill face ranges (face_id >= u64::MAX - 1) to avoid
        // removing real face triangles. If no fill triangles, skip this edge.
        for g in ga.iter().skip(1) {
            let is_fill = g.0 < face_ranges.len() && face_ranges[g.0].face_id.0 >= u64::MAX - 1;
            if !is_fill {
                continue;
            }
            for &t in &g.2 {
                if removed >= excess {
                    break;
                }
                to_remove.insert(t);
                removed += 1;
            }
            if removed >= excess {
                break;
            }
        }
    }
    if to_remove.is_empty() {
        return;
    }
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();
    for range in face_ranges.iter() {
        let rs = new_indices.len() as u32;
        for t in (range.start_index as usize / 3)..(range.end_index as usize / 3).min(n_tris) {
            if to_remove.contains(&t) {
                continue;
            }
            let b = t * 3;
            new_indices.extend_from_slice(&[indices[b], indices[b + 1], indices[b + 2]]);
        }
        let re = new_indices.len() as u32;
        if re > rs {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: rs,
                end_index: re,
            });
        }
    }
    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Weld boundary vertices that are close enough to match in the oracle grid.
///
/// The boolean pipeline can produce seam vertices that are very close but
/// not exactly coincident, causing oracle edge matching to report "unpaired"
/// edges. This function identifies boundary (unpaired-edge) vertices, then
/// uses union-find to cluster those within distance `grid * 1.5` of each
/// other. Each cluster is replaced by its centroid, ensuring all seam
/// vertices match in the oracle quantization.
fn weld_boundary_vertices(vertices: &mut [f32], indices: &[u32]) {
    weld_boundary_vertices_with_scale(vertices, indices, 5.0);
}

/// Progressive boundary vertex welding with configurable scale factor.
///
/// Clusters boundary vertices (endpoints of unpaired edges) within
/// `scale_factor × grid` distance and snaps each cluster to its centroid.
/// Higher scale factors capture larger S-H clipping divergences but risk
/// merging genuinely distinct vertices. Used in the convergence loop at
/// progressively increasing scales (5, 10, 20, 40).
fn weld_boundary_vertices_with_scale(vertices: &mut [f32], indices: &[u32], scale_factor: f64) {
    if vertices.is_empty() || indices.is_empty() {
        return;
    }
    let n_verts = vertices.len() / 3;
    let n_tris = indices.len() / 3;

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    // Quantize helper
    let quantize = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize;
        if i >= n_verts {
            return (0, 0, 0);
        }
        let x = (vertices[i * 3] as f64 * inv_grid).round() as i64;
        let y = (vertices[i * 3 + 1] as f64 * inv_grid).round() as i64;
        let z = (vertices[i * 3 + 2] as f64 * inv_grid).round() as i64;
        (x, y, z)
    };

    // Build undirected edge counts
    type QPos = (i64, i64, i64);
    let mut edge_counts: HashMap<(QPos, QPos), usize> = HashMap::new();
    let make_edge = |a: QPos, b: QPos| -> (QPos, QPos) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };

    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qt = [quantize(tri[0]), quantize(tri[1]), quantize(tri[2])];
        for e in 0..3 {
            let edge = make_edge(qt[e], qt[(e + 1) % 3]);
            if edge.0 != edge.1 {
                *edge_counts.entry(edge).or_insert(0) += 1;
            }
        }
    }

    // Collect boundary vertex indices (endpoints of unpaired edges)
    let mut boundary_verts: Vec<u32> = Vec::new();
    let mut is_boundary: HashSet<u32> = HashSet::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qt = [quantize(tri[0]), quantize(tri[1]), quantize(tri[2])];
        for e in 0..3 {
            let edge = make_edge(qt[e], qt[(e + 1) % 3]);
            if edge.0 != edge.1 {
                if let Some(&count) = edge_counts.get(&edge) {
                    if count != 2 {
                        for &vi in &[tri[e], tri[(e + 1) % 3]] {
                            if is_boundary.insert(vi) {
                                boundary_verts.push(vi);
                            }
                        }
                    }
                }
            }
        }
    }

    if boundary_verts.is_empty() {
        return;
    }

    // Union-find for clustering close boundary vertices
    let n = boundary_verts.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }

    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[rb] = ra;
        }
    }

    // Weld threshold uses configurable scale factor to capture near-miss seam
    // vertices that diverge due to Sutherland-Hodgman clipping at intersection
    // boundaries. Progressive scales (5→10→20→40) catch divergences at
    // different magnitudes without over-welding in a single pass.
    let weld_dist_sq = (grid * scale_factor) * (grid * scale_factor);

    // O(N²) pairwise check — N is small (boundary vertices only)
    for (i, &bv_i) in boundary_verts.iter().enumerate() {
        let ai = bv_i as usize;
        let ax = vertices[ai * 3] as f64;
        let ay = vertices[ai * 3 + 1] as f64;
        let az = vertices[ai * 3 + 2] as f64;
        for (j, &bv_j) in boundary_verts.iter().enumerate().skip(i + 1) {
            let bj = bv_j as usize;
            let bx = vertices[bj * 3] as f64;
            let by = vertices[bj * 3 + 1] as f64;
            let bz = vertices[bj * 3 + 2] as f64;
            let dx = ax - bx;
            let dy = ay - by;
            let dz = az - bz;
            if dx * dx + dy * dy + dz * dz < weld_dist_sq {
                union(&mut parent, i, j);
            }
        }
    }

    // Compute centroid for each cluster and assign
    let mut cluster_sum: HashMap<usize, [f64; 3]> = HashMap::new();
    let mut cluster_count: HashMap<usize, usize> = HashMap::new();

    for (i, &bv_i) in boundary_verts.iter().enumerate() {
        let root = find(&mut parent, i);
        let vi = bv_i as usize;
        let entry = cluster_sum.entry(root).or_insert([0.0; 3]);
        entry[0] += vertices[vi * 3] as f64;
        entry[1] += vertices[vi * 3 + 1] as f64;
        entry[2] += vertices[vi * 3 + 2] as f64;
        *cluster_count.entry(root).or_insert(0) += 1;
    }

    // Only weld clusters with >1 vertex (actual merges)
    for (i, &bv_i) in boundary_verts.iter().enumerate() {
        let root = find(&mut parent, i);
        let count = cluster_count[&root];
        if count <= 1 {
            continue;
        }
        let sum = cluster_sum[&root];
        let vi = bv_i as usize;
        vertices[vi * 3] = (sum[0] / count as f64) as f32;
        vertices[vi * 3 + 1] = (sum[1] / count as f64) as f32;
        vertices[vi * 3 + 2] = (sum[2] / count as f64) as f32;
    }
}

/// If the mesh signed volume is negative, the entire solid is inside-out.
/// Flip all triangle windings and negate all normals to fix orientation.
fn fix_global_orientation(vertices: &mut [f32], normals: &mut [f32], indices: &mut [u32]) {
    let num_tris = indices.len() / 3;
    if num_tris == 0 {
        return;
    }

    // Compute signed volume using divergence theorem
    let mut vol = 0.0f64;
    for t in 0..num_tris {
        let i0 = indices[t * 3] as usize;
        let i1 = indices[t * 3 + 1] as usize;
        let i2 = indices[t * 3 + 2] as usize;
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
        vol += v0[0] * (v1[1] * v2[2] - v1[2] * v2[1])
            + v1[0] * (v2[1] * v0[2] - v2[2] * v0[1])
            + v2[0] * (v0[1] * v1[2] - v0[2] * v1[1]);
    }
    vol /= 6.0;

    if vol < 0.0 {
        // Flip all triangle windings
        for t in 0..num_tris {
            indices.swap(t * 3 + 1, t * 3 + 2);
        }
        // Negate all normals
        for n in normals.iter_mut() {
            *n = -*n;
        }
        // Also flip the unused vertices' normals? No — only normals array matters
    }
}

/// Remove degenerate (zero-area) triangles from the mesh and compact face ranges.
/// Degenerate triangles arise from ear-clipping on very thin boolean fragments
/// or collinear vertices in polygon faces. Removing them prevents oracle failures.
// Old reduce_non_manifold_edges removed; replaced by the face-group-aware
// version above count_unpaired_in_mesh.
#[allow(dead_code)]
fn _reduce_non_manifold_edges_old(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize_pos = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Count directed edges across all triangles
    let mut directed_counts: HashMap<(QPos, QPos), usize> = HashMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [
            quantize_pos(indices[base]),
            quantize_pos(indices[base + 1]),
            quantize_pos(indices[base + 2]),
        ];
        for j in 0..3 {
            *directed_counts
                .entry((tri[j], tri[(j + 1) % 3]))
                .or_insert(0) += 1;
        }
    }

    // Find triangles to remove: all 3 directed edges appear 2+ times AND
    // all 3 reverse edges exist.
    let mut remove = vec![false; n_tris];
    for (t, should_remove) in remove.iter_mut().enumerate().take(n_tris) {
        let base = t * 3;
        let tri = [
            quantize_pos(indices[base]),
            quantize_pos(indices[base + 1]),
            quantize_pos(indices[base + 2]),
        ];

        let mut all_dup = true;
        let mut all_rev = true;
        for j in 0..3 {
            let fwd = (tri[j], tri[(j + 1) % 3]);
            let rev = (tri[(j + 1) % 3], tri[j]);
            if directed_counts.get(&fwd).copied().unwrap_or(0) < 2 {
                all_dup = false;
                break;
            }
            if directed_counts.get(&rev).copied().unwrap_or(0) < 1 {
                all_rev = false;
                break;
            }
        }

        if all_dup && all_rev {
            // Mark for removal and decrement edge counts
            *should_remove = true;
            for j in 0..3 {
                let fwd = (tri[j], tri[(j + 1) % 3]);
                if let Some(c) = directed_counts.get_mut(&fwd) {
                    *c -= 1;
                }
            }
        }
    }

    let removed: usize = remove.iter().filter(|&&r| r).count();
    if removed == 0 {
        return;
    }

    // Rebuild indices and face_ranges excluding removed triangles
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();

    for range in face_ranges.iter() {
        let start = range.start_index as usize;
        let end = range.end_index as usize;
        let range_start = new_indices.len() as u32;
        let range_tris = (end - start) / 3;
        let tri_offset = start / 3;

        for t in 0..range_tris {
            let global_t = tri_offset + t;
            if global_t < n_tris && !remove[global_t] {
                let base = global_t * 3;
                new_indices.push(indices[base]);
                new_indices.push(indices[base + 1]);
                new_indices.push(indices[base + 2]);
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

fn remove_degenerate_triangles(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end {
            let base = t * 3;
            if base + 2 >= indices.len() {
                break;
            }
            let i0 = indices[base] as usize;
            let i1 = indices[base + 1] as usize;
            let i2 = indices[base + 2] as usize;

            if i0 * 3 + 2 >= vertices.len()
                || i1 * 3 + 2 >= vertices.len()
                || i2 * 3 + 2 >= vertices.len()
            {
                continue;
            }

            // Match oracle computation exactly: f32 arithmetic, area = |cross|/2
            let ax = vertices[i1 * 3] - vertices[i0 * 3];
            let ay = vertices[i1 * 3 + 1] - vertices[i0 * 3 + 1];
            let az = vertices[i1 * 3 + 2] - vertices[i0 * 3 + 2];
            let bx = vertices[i2 * 3] - vertices[i0 * 3];
            let by = vertices[i2 * 3 + 1] - vertices[i0 * 3 + 1];
            let bz = vertices[i2 * 3 + 2] - vertices[i0 * 3 + 2];
            let cx = ay * bz - az * by;
            let cy = az * bx - ax * bz;
            let cz = ax * by - ay * bx;
            let area = (cx * cx + cy * cy + cz * cz).sqrt() / 2.0;

            // Keep non-degenerate triangles (matches oracle threshold)
            if area >= TAU_WORK as f32 {
                new_indices.push(indices[base]);
                new_indices.push(indices[base + 1]);
                new_indices.push(indices[base + 2]);
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Remove exact duplicate triangles (same winding, same quantized positions).
///
/// When the boolean produces duplicate face fragments (same 3 vertices in
/// same cyclic order), keep only one copy. This is conservative — it only
/// removes triangles that are exact duplicates, not triangles that merely
/// share edges.
fn remove_duplicate_triangles(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Canonical form: rotate to minimum vertex, preserving winding direction.
    let tri_key = |a: QPos, b: QPos, c: QPos| -> [QPos; 3] {
        if a <= b && a <= c {
            [a, b, c]
        } else if b <= a && b <= c {
            [b, c, a]
        } else {
            [c, a, b]
        }
    };

    let mut seen: HashSet<[QPos; 3]> = HashSet::new();
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end {
            let base = t * 3;
            if base + 2 >= indices.len() {
                break;
            }
            let qa = quantize(indices[base]);
            let qb = quantize(indices[base + 1]);
            let qc = quantize(indices[base + 2]);
            let key = tri_key(qa, qb, qc);

            if seen.insert(key) {
                new_indices.push(indices[base]);
                new_indices.push(indices[base + 1]);
                new_indices.push(indices[base + 2]);
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Remove winding-insensitive duplicate triangles.
///
/// Two triangles with the same 3 quantized vertex positions (regardless of
/// winding order) are duplicates. The first occurrence is kept; subsequent
/// occurrences are removed. This catches opposite-winding duplicates that
/// `remove_duplicate_triangles` (winding-sensitive) misses.
fn remove_winding_insensitive_duplicates(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Canonical form: sort the 3 vertices (winding-insensitive).
    let tri_key = |a: QPos, b: QPos, c: QPos| -> [QPos; 3] {
        let mut arr = [a, b, c];
        arr.sort();
        arr
    };

    let mut seen: HashSet<[QPos; 3]> = HashSet::new();
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end {
            let base = t * 3;
            if base + 2 >= indices.len() {
                break;
            }
            let qa = quantize(indices[base]);
            let qb = quantize(indices[base + 1]);
            let qc = quantize(indices[base + 2]);
            let key = tri_key(qa, qb, qc);

            if seen.insert(key) {
                new_indices.push(indices[base]);
                new_indices.push(indices[base + 1]);
                new_indices.push(indices[base + 2]);
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Core non-manifold removal logic shared by both aggressive and conservative modes.
///
/// For each non-manifold edge (shared by 3+ triangles), removes excess triangles
/// to bring the count down to 2. Processes edges in sorted order for determinism.
///
/// In `conservative` mode, fill triangles are only removed if at least 2 of their
/// 3 edges have count >= 3, and real triangles are only removed if all 3 edges
/// have count >= 3. This prevents creating new unpaired (boundary) edges.
///
/// In aggressive mode (`conservative = false`), all excess triangles are removed
/// with no safety check.
fn remove_nonmanifold_duplicates_inner(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
    conservative: bool,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    // Build quantization grid matching the test oracle exactly.
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Build a map from triangle index to its face_id.
    let mut tri_face_id: Vec<KernelId> = vec![KernelId(0); n_tris];
    for range in face_ranges.iter() {
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;
        for item in tri_face_id
            .iter_mut()
            .take(tri_end.min(n_tris))
            .skip(tri_start)
        {
            *item = range.face_id;
        }
    }

    // Compute triangle area for each triangle (for removal priority).
    let tri_area: Vec<f64> = (0..n_tris)
        .map(|t| {
            let base = t * 3;
            let i0 = indices[base] as usize * 3;
            let i1 = indices[base + 1] as usize * 3;
            let i2 = indices[base + 2] as usize * 3;
            if i0 + 2 >= vertices.len() || i1 + 2 >= vertices.len() || i2 + 2 >= vertices.len() {
                return 0.0;
            }
            let v0 = [
                vertices[i0] as f64,
                vertices[i0 + 1] as f64,
                vertices[i0 + 2] as f64,
            ];
            let v1 = [
                vertices[i1] as f64,
                vertices[i1 + 1] as f64,
                vertices[i1 + 2] as f64,
            ];
            let v2 = [
                vertices[i2] as f64,
                vertices[i2 + 1] as f64,
                vertices[i2 + 2] as f64,
            ];
            let e1 = v3_sub(v1, v0);
            let e2 = v3_sub(v2, v0);
            v3_length(v3_cross(e1, e2))
        })
        .collect();

    // Iterate: batch-remove, then re-check. Converges because each iteration
    // removes at least one triangle.
    let mut remove_set: HashSet<usize> = HashSet::new();

    for _pass in 0..10 {
        // Build edge -> list of triangle indices (excluding already removed).
        type UEdge = (QPos, QPos);
        let mut edge_tris: HashMap<UEdge, Vec<usize>> = HashMap::new();
        for t in 0..n_tris {
            if remove_set.contains(&t) {
                continue;
            }
            let base = t * 3;
            let tri = [indices[base], indices[base + 1], indices[base + 2]];
            for j in 0..3 {
                let pa = quantize(tri[j]);
                let pb = quantize(tri[(j + 1) % 3]);
                let key: UEdge = if pa <= pb { (pa, pb) } else { (pb, pa) };
                edge_tris.entry(key).or_default().push(t);
            }
        }

        // Collect non-manifold edges and sort for deterministic processing.
        let mut nm_edges: Vec<(UEdge, Vec<usize>)> = edge_tris
            .iter()
            .filter(|(_, tris)| tris.len() >= 3)
            .map(|(e, t)| (*e, t.clone()))
            .collect();

        if nm_edges.is_empty() {
            break;
        }

        nm_edges.sort_by_key(|(edge, _)| *edge);

        let prev_remove_count = remove_set.len();

        // Build per-triangle edge lists and effective counts for safety checks.
        let tri_edge_keys: Vec<[UEdge; 3]> = (0..n_tris)
            .map(|t| {
                if remove_set.contains(&t) {
                    return [((0, 0, 0), (0, 0, 0)); 3];
                }
                let base = t * 3;
                let tri = [indices[base], indices[base + 1], indices[base + 2]];
                let mut edges = [((0, 0, 0), (0, 0, 0)); 3];
                for j in 0..3 {
                    let pa = quantize(tri[j]);
                    let pb = quantize(tri[(j + 1) % 3]);
                    edges[j] = if pa <= pb { (pa, pb) } else { (pb, pa) };
                }
                edges
            })
            .collect();

        let mut eff_edge_count: HashMap<UEdge, usize> = HashMap::new();
        for (e, tris) in edge_tris.iter() {
            eff_edge_count.insert(*e, tris.len());
        }

        for (_nm_edge, tris) in &nm_edges {
            let mut live: Vec<usize> = tris
                .iter()
                .copied()
                .filter(|t| !remove_set.contains(t))
                .collect();
            live.sort_unstable();
            live.dedup();

            if live.len() <= 2 {
                continue;
            }

            // Sort by removal priority: fill first, then smaller area, then higher index.
            live.sort_by(|&a, &b| {
                let a_fill = tri_face_id[a].0 >= u64::MAX - 1;
                let b_fill = tri_face_id[b].0 >= u64::MAX - 1;
                b_fill
                    .cmp(&a_fill)
                    .then_with(|| {
                        tri_area[a]
                            .partial_cmp(&tri_area[b])
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| b.cmp(&a))
            });

            let target_removals = live.len() - 2;
            let mut removed_count = 0;
            for &t in &live {
                if removed_count >= target_removals {
                    break;
                }

                // Safety check: removing this triangle must not drop any of its
                // edges below count 2 (which would create new boundary edges).
                let safe = if conservative {
                    tri_edge_keys[t]
                        .iter()
                        .all(|e| eff_edge_count.get(e).copied().unwrap_or(0) >= 3)
                } else {
                    true
                };

                if safe {
                    remove_set.insert(t);
                    removed_count += 1;
                    for e in &tri_edge_keys[t] {
                        if let Some(c) = eff_edge_count.get_mut(e) {
                            *c = c.saturating_sub(1);
                        }
                    }
                }
            }

            // If conservative mode couldn't remove enough (other edges have count=2),
            // try paired removal: find two triangles that share TWO edges (the NM edge
            // + one other). Removing both simultaneously drops the NM edge by 2 and
            // the shared edge by 2 (from 2→0), so we must also check that the partner
            // edge itself has count >= 4 or that the pair forms opposite-winding
            // duplicates (canceling faces). Instead, find triangles that share the NM
            // edge AND have a mutual second edge — removing both preserves the mutual
            // edge at count 0 but the third edges each drop by 1.
            //
            // Safer approach: for count=4 edges, check if two triangles share the
            // SAME 3 quantized vertices (winding-insensitive duplicates). If so,
            // they are coplanar duplicates and one can be safely removed.
            if conservative && removed_count < target_removals && live.len() == 4 {
                let remaining: Vec<usize> = live
                    .iter()
                    .copied()
                    .filter(|t| !remove_set.contains(t))
                    .collect();
                // Check for winding-insensitive duplicate pairs
                for i in 0..remaining.len() {
                    if removed_count >= target_removals {
                        break;
                    }
                    let ti = remaining[i];
                    if remove_set.contains(&ti) {
                        continue;
                    }
                    let mut ki = tri_edge_keys[ti];
                    ki.sort();
                    for &tj in &remaining[(i + 1)..] {
                        if removed_count >= target_removals {
                            break;
                        }
                        if remove_set.contains(&tj) {
                            continue;
                        }
                        let mut kj = tri_edge_keys[tj];
                        kj.sort();
                        // Same 3 edges = same triangle (possibly different winding)
                        if ki == kj {
                            // Remove the one with smaller area (likely the degenerate one)
                            let victim = if tri_area[ti] <= tri_area[tj] { ti } else { tj };
                            remove_set.insert(victim);
                            removed_count += 1;
                            for e in &tri_edge_keys[victim] {
                                if let Some(c) = eff_edge_count.get_mut(e) {
                                    *c = c.saturating_sub(1);
                                }
                            }
                        }
                    }
                }
            }
        }

        if remove_set.len() == prev_remove_count {
            break;
        }
    }

    if remove_set.is_empty() {
        return;
    }

    // Rebuild indices and face_ranges, skipping removed triangles.
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end.min(n_tris) {
            if remove_set.contains(&t) {
                continue;
            }
            let base = t * 3;
            new_indices.push(indices[base]);
            new_indices.push(indices[base + 1]);
            new_indices.push(indices[base + 2]);
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Conservative non-manifold removal: only removes fill triangles (with safety)
/// and fully-redundant real triangles. Used in the fan-path pipeline where
/// fill triangles may be needed for watertightness.
fn remove_nonmanifold_duplicates(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    remove_nonmanifold_duplicates_inner(vertices, indices, face_ranges, true);
}

/// Aggressive non-manifold removal: removes all excess triangles without safety
/// checks. Used in the bounded-path pipeline where all triangles are real face
/// tessellations and non-manifold edges come from overlapping adjacent faces.
fn remove_nonmanifold_duplicates_aggressive(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    remove_nonmanifold_duplicates_inner(vertices, indices, face_ranges, false);
}

/// Targeted non-manifold edge repair: for each non-manifold edge (count=3),
/// try removing each candidate triangle and check if the overall unpaired
/// count improves. Keep the best removal. This handles cases where
/// conservative removal is blocked (other edges have count=2) but removing
/// a specific triangle results in fillable boundary holes.
///
/// Only processes edges with count exactly 3 (the most common case after
/// all other repair). Higher counts are left to aggressive removal.
fn repair_targeted_nonmanifold(
    vertices: &mut [f32],
    normals: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 3 {
        return;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32, verts: &[f32]| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= verts.len() {
            return (0, 0, 0);
        }
        (
            (verts[i] as f64 * inv_grid).round() as i64,
            (verts[i + 1] as f64 * inv_grid).round() as i64,
            (verts[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Build edge → triangle list
    type UEdge = (QPos, QPos);
    let mut edge_tris: HashMap<UEdge, Vec<usize>> = HashMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        for j in 0..3 {
            let pa = quantize(tri[j], vertices);
            let pb = quantize(tri[(j + 1) % 3], vertices);
            let key: UEdge = if pa <= pb { (pa, pb) } else { (pb, pa) };
            edge_tris.entry(key).or_default().push(t);
        }
    }

    // Find edges with exactly 3 sharing triangles
    let nm3_edges: Vec<(UEdge, Vec<usize>)> = edge_tris
        .iter()
        .filter(|(_, tris)| tris.len() == 3)
        .map(|(e, t)| (*e, t.clone()))
        .collect();

    if nm3_edges.is_empty() {
        return;
    }

    let baseline = count_unpaired_in_mesh(vertices, indices);

    // For each nm3 edge, try removing each of the 3 triangles.
    // After removal, simulate fill_boundary_holes to see if the resulting
    // boundary holes are fillable. Pick the removal that yields the best
    // post-fill unpaired count.
    let mut best_removal: Option<usize> = None;
    let mut best_score = baseline;

    // Build temporary face ranges for trial runs
    let trial_face_range = |trial_idx: &[u32]| -> Vec<FaceRange> {
        vec![FaceRange {
            face_id: KernelId(u64::MAX),
            start_index: 0,
            end_index: trial_idx.len() as u32,
        }]
    };

    for (_, tris) in &nm3_edges {
        for &t in tris {
            // Build trial index buffer without triangle t
            let mut trial_indices: Vec<u32> = (0..n_tris)
                .filter(|&i| i != t)
                .flat_map(|i| {
                    let b = i * 3;
                    [indices[b], indices[b + 1], indices[b + 2]]
                })
                .collect();

            // Simulate fill on the trial buffer
            let mut trial_ranges = trial_face_range(&trial_indices);
            fill_boundary_holes(vertices, normals, &mut trial_indices, &mut trial_ranges);

            let score = count_unpaired_in_mesh(vertices, &trial_indices);
            if score < best_score {
                best_score = score;
                best_removal = Some(t);
            }
        }
    }

    if let Some(remove_tri) = best_removal {
        // Apply the best removal
        let mut new_indices = Vec::with_capacity(indices.len() - 3);
        let mut new_ranges = Vec::new();

        for range in face_ranges.iter() {
            let range_start = new_indices.len() as u32;
            let tri_start = range.start_index as usize / 3;
            let tri_end = range.end_index as usize / 3;

            for t in tri_start..tri_end {
                if t == remove_tri {
                    continue;
                }
                let base = t * 3;
                if base + 2 >= indices.len() {
                    break;
                }
                new_indices.push(indices[base]);
                new_indices.push(indices[base + 1]);
                new_indices.push(indices[base + 2]);
            }

            let range_end = new_indices.len() as u32;
            if range_end > range_start {
                new_ranges.push(FaceRange {
                    face_id: range.face_id,
                    start_index: range_start,
                    end_index: range_end,
                });
            }
        }

        *indices = new_indices;
        *face_ranges = new_ranges;

        // Fill any boundary holes created by the removal
        fill_boundary_holes(vertices, normals, indices, face_ranges);
        remove_degenerate_triangles(vertices, indices, face_ranges);

        // Recurse to handle remaining nm3 edges (up to 10 depth)
        if count_nonmanifold_edges(vertices, indices) > 0
            && count_unpaired_in_mesh(vertices, indices) < baseline
        {
            repair_targeted_nonmanifold(vertices, normals, indices, face_ranges);
        }
    }
}

/// Count boundary edges (edges shared by exactly 1 triangle) in the mesh.
/// Uses the same quantization grid as the watertightness oracle.
fn count_boundary_edges(vertices: &[f32], indices: &[u32]) -> usize {
    let n_tris = indices.len() / 3;
    if n_tris == 0 {
        return 0;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    let mut edge_counts: HashMap<(QPos, QPos), u32> = HashMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        for j in 0..3 {
            let pa = quantize(tri[j]);
            let pb = quantize(tri[(j + 1) % 3]);
            let key = if pa <= pb { (pa, pb) } else { (pb, pa) };
            *edge_counts.entry(key).or_insert(0) += 1;
        }
    }

    edge_counts.values().filter(|&&c| c == 1).count()
}

/// Count non-manifold edges (edges shared by 3+ triangles).
fn count_nonmanifold_edges(vertices: &[f32], indices: &[u32]) -> usize {
    let n_tris = indices.len() / 3;
    if n_tris == 0 {
        return 0;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    let mut edge_counts: HashMap<(QPos, QPos), u32> = HashMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        for j in 0..3 {
            let pa = quantize(tri[j]);
            let pb = quantize(tri[(j + 1) % 3]);
            let key = if pa <= pb { (pa, pb) } else { (pb, pa) };
            *edge_counts.entry(key).or_insert(0) += 1;
        }
    }

    edge_counts.values().filter(|&&c| c >= 3).count()
}

/// Split "chord" triangles on non-manifold edges where a boundary chain
/// connects the same two endpoints through intermediate vertices.
///
/// When face tessellation creates a straight chord edge A→D while the
/// adjacent face tessellates through intermediate boundary vertices A→B→C→D,
/// the chord edge becomes non-manifold (count 3+) and the chain edges become
/// boundary (count 1). Fix by replacing the chord triangle with fan triangles
/// that follow the boundary chain.
#[allow(dead_code)]
fn split_nonmanifold_chord_triangles(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    type UEdge = (QPos, QPos);
    let make_edge = |a: QPos, b: QPos| -> UEdge {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };

    // Build edge counts
    let mut edge_counts: HashMap<UEdge, u32> = HashMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        for j in 0..3 {
            let pa = quantize(tri[j]);
            let pb = quantize(tri[(j + 1) % 3]);
            let key = make_edge(pa, pb);
            *edge_counts.entry(key).or_insert(0) += 1;
        }
    }

    // Find non-manifold edges (count >= 3)
    let nm_edges: Vec<UEdge> = edge_counts
        .iter()
        .filter(|(_, &c)| c >= 3)
        .map(|(&e, _)| e)
        .collect();

    if nm_edges.is_empty() {
        return;
    }

    // Build boundary adjacency: directed boundary edges (count 1 with no reverse)
    let mut directed_counts: HashMap<(QPos, QPos), u32> = HashMap::new();
    let mut pos_to_idx: HashMap<QPos, u32> = HashMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        for j in 0..3 {
            let pa = quantize(tri[j]);
            let pb = quantize(tri[(j + 1) % 3]);
            *directed_counts.entry((pa, pb)).or_insert(0) += 1;
            pos_to_idx.entry(pa).or_insert(tri[j]);
        }
    }

    let mut boundary_next: HashMap<QPos, Vec<QPos>> = HashMap::new();
    for (&(a, b), &count) in &directed_counts {
        if count == 1 && directed_counts.get(&(b, a)).copied().unwrap_or(0) == 0 {
            boundary_next.entry(a).or_default().push(b);
        }
    }

    // For each non-manifold edge, find chord triangles and boundary chains.
    let mut replacements: HashMap<usize, Vec<[u32; 3]>> = HashMap::new();

    for &(ea, eb) in &nm_edges {
        // Find triangles on this edge where the OTHER 2 edges have count == 2.
        // These are "chord" triangles.
        for t in 0..n_tris {
            if replacements.contains_key(&t) {
                continue;
            }
            let base = t * 3;
            let tri = [indices[base], indices[base + 1], indices[base + 2]];
            let qt = [quantize(tri[0]), quantize(tri[1]), quantize(tri[2])];

            // Find which local edge matches the nm edge
            let mut nm_local_edge: Option<usize> = None;
            for j in 0..3 {
                let key = make_edge(qt[j], qt[(j + 1) % 3]);
                if key == (ea, eb) || key == (eb, ea) {
                    nm_local_edge = Some(j);
                    break;
                }
            }
            let nm_local = match nm_local_edge {
                Some(e) => e,
                None => continue,
            };

            // Check other 2 edges have count == 2
            let e1 = make_edge(qt[(nm_local + 1) % 3], qt[(nm_local + 2) % 3]);
            let e2 = make_edge(qt[(nm_local + 2) % 3], qt[nm_local]);
            let c1 = edge_counts.get(&e1).copied().unwrap_or(0);
            let c2 = edge_counts.get(&e2).copied().unwrap_or(0);
            if c1 != 2 || c2 != 2 {
                continue;
            }

            // This is a chord triangle. The nm edge goes from qt[nm_local]
            // to qt[(nm_local+1)%3]. The opposite vertex is qt[(nm_local+2)%3].
            let start_q = qt[nm_local];
            let end_q = qt[(nm_local + 1) % 3];
            let opp_idx = tri[(nm_local + 2) % 3];

            // Try both directions for the boundary chain.
            let directions = [(start_q, end_q), (end_q, start_q)];
            let mut best_chain: Option<Vec<QPos>> = None;
            for &(chain_start, chain_end) in &directions {
                let mut chain: Vec<QPos> = vec![chain_start];
                let mut current = chain_start;
                let mut found = false;
                for _ in 0..16 {
                    if let Some(nexts) = boundary_next.get(&current) {
                        let next = nexts.iter().copied().find(|&n| {
                            n == chain_end
                                || (!chain.contains(&n) && boundary_next.contains_key(&n))
                        });
                        match next {
                            Some(n) => {
                                chain.push(n);
                                if n == chain_end {
                                    found = true;
                                    break;
                                }
                                current = n;
                            }
                            None => break,
                        }
                    } else {
                        break;
                    }
                }
                if found && chain.len() >= 3 {
                    best_chain = Some(chain);
                    break;
                }
            }

            let chain = match best_chain {
                Some(c) => c,
                None => continue,
            };

            // Replace the chord triangle with fan triangles following the
            // boundary chain. The fan vertex is the opposite vertex (opp_idx).
            // Chain vertices are mapped to actual vertex indices via pos_to_idx.
            //
            // The chord triangle has directed edge start_q → end_q.
            // If chain goes start→end, fan triangles are (chain[i], chain[i+1], opp).
            // If chain goes end→start, fan triangles are (chain[i+1], chain[i], opp)
            // to preserve the original winding direction.
            // The boundary chain edges are directed. The fan triangles need
            // to produce the REVERSE directed edges to pair with them.
            // If chain goes start→end (same as nm directed edge), fan edges
            // should go end→start direction, so use (chain[i+1], chain[i], opp).
            // If chain goes end→start (reversed), fan edges should go
            // start→end direction, so use (chain[i], chain[i+1], opp).
            let chain_follows_original = chain[0] == start_q;
            let mut new_tris = Vec::new();
            for i in 0..chain.len() - 1 {
                let p1 = chain[i];
                let p2 = chain[i + 1];
                if let (Some(&i1), Some(&i2)) = (pos_to_idx.get(&p1), pos_to_idx.get(&p2)) {
                    if chain_follows_original {
                        // Chain follows nm edge direction → fan must reverse
                        new_tris.push([i2, i1, opp_idx]);
                    } else {
                        // Chain reversed → fan follows original direction
                        new_tris.push([i1, i2, opp_idx]);
                    }
                }
            }

            if !new_tris.is_empty() {
                replacements.insert(t, new_tris);
            }
        }
    }

    if replacements.is_empty() {
        return;
    }

    // Rebuild indices with replacements.
    let mut new_indices = Vec::with_capacity(indices.len() + replacements.len() * 6);
    let mut new_ranges = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end.min(n_tris) {
            let base = t * 3;
            if let Some(new_tris) = replacements.get(&t) {
                for tri in new_tris {
                    new_indices.extend_from_slice(&[tri[0], tri[1], tri[2]]);
                }
            } else {
                new_indices.push(indices[base]);
                new_indices.push(indices[base + 1]);
                new_indices.push(indices[base + 2]);
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Resolve mesh-level T-junctions.
///
/// A T-junction occurs when triangle T1 has an edge A→B, while adjacent
/// triangles T2, T3 have edges A→C and C→B (vertex C lies on the interior
/// of edge AB). This makes edges {A,B}, {A,C}, and {C,B} all appear with
/// count 1 (unpaired) in the oracle.
///
/// Fix: find boundary edges where a boundary vertex lies on the edge interior,
/// and split the triangle into two triangles at that vertex.
fn resolve_mesh_t_junctions(
    vertices: &[f32],
    _normals: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    // Compute oracle-compatible quantization grid
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize_pos = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Build undirected edge counts (oracle-style)
    let mut edge_counts: HashMap<(QPos, QPos), usize> = HashMap::new();
    let make_edge = |a: QPos, b: QPos| -> (QPos, QPos) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };

    for t in 0..n_tris {
        let base = t * 3;
        let qa = quantize_pos(indices[base]);
        let qb = quantize_pos(indices[base + 1]);
        let qc = quantize_pos(indices[base + 2]);
        *edge_counts.entry(make_edge(qa, qb)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(qb, qc)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(qc, qa)).or_insert(0) += 1;
    }

    // Collect boundary edges (undirected count != 2), sorted for determinism.
    let mut boundary_edges_vec: Vec<(QPos, QPos)> = edge_counts
        .iter()
        .filter(|(_, &c)| c != 2)
        .map(|(&e, _)| e)
        .collect();
    boundary_edges_vec.sort();
    let boundary_edges: std::collections::HashSet<(QPos, QPos)> =
        boundary_edges_vec.iter().copied().collect();

    if boundary_edges.is_empty() {
        return;
    }

    // Collect ONLY vertices that are endpoints of boundary edges (T-junction
    // candidates must themselves be on the boundary manifold).
    let mut boundary_verts: HashMap<QPos, u32> = HashMap::new();
    for &(qa, qb) in &boundary_edges_vec {
        // Find a vertex index for each quantized position
        for t in 0..n_tris {
            let base = t * 3;
            for k in 0..3 {
                let idx = indices[base + k];
                let qp = quantize_pos(idx);
                if qp == qa {
                    boundary_verts.entry(qa).or_insert(idx);
                }
                if qp == qb {
                    boundary_verts.entry(qb).or_insert(idx);
                }
            }
        }
    }

    // For each boundary edge, check if a BOUNDARY vertex lies on its interior.
    // Build map: triangle_index → list of (edge_local_idx, split_vertex_idx)
    let mut splits: HashMap<usize, Vec<(usize, u32)>> = HashMap::new();

    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qtri = [
            quantize_pos(tri[0]),
            quantize_pos(tri[1]),
            quantize_pos(tri[2]),
        ];

        for local_e in 0..3 {
            let qa = qtri[local_e];
            let qb = qtri[(local_e + 1) % 3];
            let edge_key = make_edge(qa, qb);

            if !boundary_edges.contains(&edge_key) {
                continue;
            }

            // Get f64 positions for the edge endpoints
            let ai = tri[local_e] as usize * 3;
            let bi = tri[(local_e + 1) % 3] as usize * 3;
            let ax = vertices[ai] as f64;
            let ay = vertices[ai + 1] as f64;
            let az = vertices[ai + 2] as f64;
            let bx = vertices[bi] as f64;
            let by = vertices[bi + 1] as f64;
            let bz = vertices[bi + 2] as f64;
            let dx = bx - ax;
            let dy = by - ay;
            let dz = bz - az;
            let edge_len_sq = dx * dx + dy * dy + dz * dz;
            if edge_len_sq < TAU_TESS_GRID_MIN * TAU_TESS_GRID_MIN {
                continue;
            }

            // Only check boundary vertices (not all mesh vertices)
            let mut best: Option<(f64, u32)> = None;
            // Sort boundary vertex candidates for deterministic tiebreaking.
            let mut bv_sorted: Vec<(QPos, u32)> =
                boundary_verts.iter().map(|(&k, &v)| (k, v)).collect();
            bv_sorted.sort();
            for &(qp, vidx) in &bv_sorted {
                if qp == qa || qp == qb {
                    continue;
                }

                let vi = vidx as usize * 3;
                let vx = vertices[vi] as f64;
                let vy = vertices[vi + 1] as f64;
                let vz = vertices[vi + 2] as f64;

                // Parametric position along edge
                let avx = vx - ax;
                let avy = vy - ay;
                let avz = vz - az;
                let t_param = (avx * dx + avy * dy + avz * dz) / edge_len_sq;
                if t_param <= 0.05 || t_param >= 0.95 {
                    continue; // not clearly in interior
                }

                // Distance from line
                let px = ax + dx * t_param;
                let py = ay + dy * t_param;
                let pz = az + dz * t_param;
                let dist_sq = (vx - px) * (vx - px) + (vy - py) * (vy - py) + (vz - pz) * (vz - pz);
                // Tight tolerance: slightly more than half oracle grid cell
                let tol = grid * 0.6;
                if dist_sq < tol * tol {
                    // Pick the closest candidate (lowest dist_sq, tiebreak by QPos order)
                    if best.is_none() || dist_sq < best.unwrap().0 {
                        best = Some((dist_sq, vidx));
                    }
                }
            }

            if let Some((_, split_v)) = best {
                // Verify split produces non-degenerate triangles.
                // The third vertex (opposite the split edge) must not be collinear.
                let opp_idx = tri[(local_e + 2) % 3];
                let oi = opp_idx as usize * 3;
                let sv = split_v as usize * 3;
                if oi + 2 < vertices.len() && sv + 2 < vertices.len() {
                    let ox = vertices[oi] as f64;
                    let oy = vertices[oi + 1] as f64;
                    let oz = vertices[oi + 2] as f64;
                    let svx = vertices[sv] as f64;
                    let svy = vertices[sv + 1] as f64;
                    let svz = vertices[sv + 2] as f64;
                    // Check triangle (A, V, Opp): area = |cross(AV, AOpp)| / 2
                    let av = [svx - ax, svy - ay, svz - az];
                    let ao = [ox - ax, oy - ay, oz - az];
                    let c1x = av[1] * ao[2] - av[2] * ao[1];
                    let c1y = av[2] * ao[0] - av[0] * ao[2];
                    let c1z = av[0] * ao[1] - av[1] * ao[0];
                    let area1 = (c1x * c1x + c1y * c1y + c1z * c1z).sqrt() / 2.0;

                    // Check triangle (V, B, Opp): area = |cross(VB, VOpp)| / 2
                    let vb = [bx - svx, by - svy, bz - svz];
                    let vo = [ox - svx, oy - svy, oz - svz];
                    let c2x = vb[1] * vo[2] - vb[2] * vo[1];
                    let c2y = vb[2] * vo[0] - vb[0] * vo[2];
                    let c2z = vb[0] * vo[1] - vb[1] * vo[0];
                    let area2 = (c2x * c2x + c2y * c2y + c2z * c2z).sqrt() / 2.0;

                    // Only split if both triangles are non-degenerate
                    if area1 > TAU_TESS_GRID_MIN * 0.1 && area2 > TAU_TESS_GRID_MIN * 0.1 {
                        splits.entry(t).or_default().push((local_e, split_v));
                    }
                }
            }
        }
    }

    if splits.is_empty() {
        return;
    }

    // Rebuild index buffer, splitting triangles with T-junctions.
    let mut new_indices: Vec<u32> = Vec::with_capacity(indices.len() + splits.len() * 3);
    let mut new_ranges: Vec<FaceRange> = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end {
            let base = t * 3;
            if base + 2 >= indices.len() {
                break;
            }

            if let Some(tri_splits) = splits.get(&t) {
                let i0 = indices[base];
                let i1 = indices[base + 1];
                let i2 = indices[base + 2];

                // Apply first split only
                let (local_e, split_v) = tri_splits[0];
                match local_e {
                    0 => {
                        // Split edge 0→1: [0,V,2] + [V,1,2]
                        new_indices.extend_from_slice(&[i0, split_v, i2]);
                        new_indices.extend_from_slice(&[split_v, i1, i2]);
                    }
                    1 => {
                        // Split edge 1→2: [0,1,V] + [0,V,2]
                        new_indices.extend_from_slice(&[i0, i1, split_v]);
                        new_indices.extend_from_slice(&[i0, split_v, i2]);
                    }
                    2 => {
                        // Split edge 2→0: [V,1,2] + [0,1,V]
                        new_indices.extend_from_slice(&[split_v, i1, i2]);
                        new_indices.extend_from_slice(&[i0, i1, split_v]);
                    }
                    _ => {
                        new_indices.extend_from_slice(&[i0, i1, i2]);
                    }
                }
            } else {
                new_indices.extend_from_slice(&[
                    indices[base],
                    indices[base + 1],
                    indices[base + 2],
                ]);
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Fill small boundary holes in the mesh.
///
/// After boolean operations, S-H clipping can leave small holes where face
/// boundaries don't perfectly align. This function detects cycles of boundary
/// edges (edges that appear exactly once) and fills them with triangles.
///
/// Only fills holes with ≤ 128 edges (small to medium polygonal holes).
/// Larger holes indicate structural issues that shouldn't be auto-filled.
fn fill_boundary_holes(
    vertices: &[f32],
    _normals: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    // Compute oracle-compatible quantization grid
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize_pos = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Build directed edge → count and vertex index mapping
    let mut directed_counts: HashMap<(QPos, QPos), usize> = HashMap::new();
    // Map quantized position → vertex index (first seen)
    let mut pos_to_idx: HashMap<QPos, u32> = HashMap::new();

    for t in 0..n_tris {
        let base = t * 3;
        let tri_indices = [indices[base], indices[base + 1], indices[base + 2]];
        let tri_pos: Vec<QPos> = tri_indices.iter().map(|&i| quantize_pos(i)).collect();

        for j in 0..3 {
            let a = tri_pos[j];
            let b = tri_pos[(j + 1) % 3];
            *directed_counts.entry((a, b)).or_insert(0) += 1;
            pos_to_idx.entry(a).or_insert(tri_indices[j]);
        }
    }

    // Find boundary edges: directed edges that appear once and have no reverse
    let mut boundary_edges: Vec<(QPos, QPos)> = Vec::new();
    for (&(a, b), &count) in &directed_counts {
        if count == 1 {
            let rev_count = directed_counts.get(&(b, a)).copied().unwrap_or(0);
            if rev_count == 0 {
                boundary_edges.push((a, b));
            }
        }
    }

    if boundary_edges.is_empty() {
        return;
    }

    // Sort for deterministic cycle detection (eliminates HashMap ordering nondeterminism)
    boundary_edges.sort();

    // Build adjacency: for each boundary vertex, what are the next vertices?
    // Use Vec to handle branching (vertex with multiple outgoing boundary edges).
    let mut next_vertices: HashMap<QPos, Vec<QPos>> = HashMap::new();
    for &(a, b) in &boundary_edges {
        next_vertices.entry(a).or_default().push(b);
    }

    // Find cycles of boundary edges (max length 20 to cover medium holes)
    let mut used_edges = std::collections::HashSet::new();
    let mut fill_triangles: Vec<[u32; 3]> = Vec::new();

    for &(start, start_next) in &boundary_edges {
        if used_edges.contains(&(start, start_next)) {
            continue;
        }

        // Trace the cycle starting with this specific edge
        let mut cycle: Vec<QPos> = vec![start];
        let mut current = start_next;
        let mut found_cycle = false;

        for _ in 0..128 {
            if current == start && cycle.len() >= 3 {
                found_cycle = true;
                break;
            }
            cycle.push(current);
            // Pick the next vertex that isn't already in the cycle (avoid infinite loops)
            let next = next_vertices
                .get(&current)
                .and_then(|nexts| {
                    nexts.iter().find(|&&n| {
                        !used_edges.contains(&(current, n)) && (n == start || !cycle.contains(&n))
                    })
                })
                .copied();
            if let Some(n) = next {
                current = n;
            } else {
                break;
            }
        }

        if !found_cycle || cycle.len() > 128 {
            continue;
        }

        // Mark edges as used
        for i in 0..cycle.len() {
            let a = cycle[i];
            let b = cycle[(i + 1) % cycle.len()];
            used_edges.insert((a, b));
        }

        // Fan-triangulate the cycle to fill the hole
        let cycle_indices: Vec<u32> = cycle
            .iter()
            .filter_map(|q| pos_to_idx.get(q).copied())
            .collect();

        if cycle_indices.len() != cycle.len() {
            continue;
        }

        // Fan-triangulate, skipping degenerate triangles (collinear vertices)
        for i in 1..cycle_indices.len() - 1 {
            let ia = cycle_indices[0] as usize * 3;
            let ib = cycle_indices[i] as usize * 3;
            let ic = cycle_indices[i + 1] as usize * 3;
            if ia + 2 >= vertices.len() || ib + 2 >= vertices.len() || ic + 2 >= vertices.len() {
                continue;
            }
            let ax = vertices[ib] - vertices[ia];
            let ay = vertices[ib + 1] - vertices[ia + 1];
            let az = vertices[ib + 2] - vertices[ia + 2];
            let bx = vertices[ic] - vertices[ia];
            let by = vertices[ic + 1] - vertices[ia + 1];
            let bz = vertices[ic + 2] - vertices[ia + 2];
            let cx = ay * bz - az * by;
            let cy = az * bx - ax * bz;
            let cz = ax * by - ay * bx;
            let area = (cx * cx + cy * cy + cz * cz).sqrt() / 2.0;
            if area >= TAU_WORK as f32 {
                fill_triangles.push([cycle_indices[0], cycle_indices[i], cycle_indices[i + 1]]);
            }
        }
    }

    if fill_triangles.is_empty() {
        return;
    }

    // Add fill triangles as a new face range (or append to the last face range)
    let fill_start = indices.len() as u32;
    for tri in &fill_triangles {
        indices.push(tri[0]);
        indices.push(tri[1]);
        indices.push(tri[2]);
    }
    let fill_end = indices.len() as u32;

    // Add as a new face range with a synthetic face ID
    if fill_end > fill_start {
        face_ranges.push(FaceRange {
            face_id: KernelId(u64::MAX), // synthetic fill face
            start_index: fill_start,
            end_index: fill_end,
        });
    }
}

/// Close near-miss boundary chains by snapping close chain endpoints together.
///
/// After all other post-processing, some boundary edges form short open chains
/// where the start and end vertices are very close (within a few oracle grid
/// cells) but not identical. This happens when S-H clipping produces slightly
/// different intersection coordinates on adjacent faces.
///
/// This function finds such chains, snaps the endpoint vertex positions to
/// match the start vertex, and fills the resulting closed cycle with triangles.
fn close_near_boundary_chains(
    vertices: &mut [f32],
    normals: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    // Strategy: find small groups of unpaired boundary edges that share vertices
    // and can be "healed" by adding fill triangles with the correct winding.
    //
    // For a manifold mesh, every directed half-edge A→B must have a matching B→A.
    // Unpaired edges (A→B exists but B→A doesn't) indicate missing faces.
    // When N unpaired edges share exactly N vertices (forming a polygon hole),
    // we can fill it with a fan of triangles.

    let n_tris = indices.len() / 3;
    if n_tris < 4 {
        return;
    }

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    type QPos = (i64, i64, i64);
    let quantize_pos = |idx: u32| -> QPos {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    // Build directed edge counts
    let mut directed_counts: HashMap<(QPos, QPos), usize> = HashMap::new();
    let mut pos_to_idx: HashMap<QPos, u32> = HashMap::new();

    for t in 0..n_tris {
        let base = t * 3;
        let tri_indices = [indices[base], indices[base + 1], indices[base + 2]];
        let tri_pos: Vec<QPos> = tri_indices.iter().map(|&i| quantize_pos(i)).collect();

        for j in 0..3 {
            let a = tri_pos[j];
            let b = tri_pos[(j + 1) % 3];
            *directed_counts.entry((a, b)).or_insert(0) += 1;
            pos_to_idx.entry(a).or_insert(tri_indices[j]);
        }
    }

    // Find boundary edges: directed half-edges with no matching reverse
    let mut boundary_edges: Vec<(QPos, QPos)> = Vec::new();
    for (&(a, b), &count) in &directed_counts {
        if count == 1 && directed_counts.get(&(b, a)).copied().unwrap_or(0) == 0 {
            boundary_edges.push((a, b));
        }
    }

    if boundary_edges.is_empty() {
        return;
    }

    // Sort for deterministic processing.
    boundary_edges.sort();

    // Collect boundary vertex adjacency (undirected) using union-find-like component detection
    let mut boundary_verts: HashSet<QPos> = HashSet::new();
    let mut vert_adj: HashMap<QPos, HashSet<QPos>> = HashMap::new();
    for &(a, b) in &boundary_edges {
        boundary_verts.insert(a);
        boundary_verts.insert(b);
        vert_adj.entry(a).or_default().insert(b);
        vert_adj.entry(b).or_default().insert(a);
    }

    // Find connected components of boundary vertices
    let mut visited: HashSet<QPos> = HashSet::new();
    let mut fill_triangles: Vec<[u32; 3]> = Vec::new();
    let boundary_edge_set: HashSet<(QPos, QPos)> = boundary_edges.iter().copied().collect();

    // Sort boundary vertices for deterministic component discovery.
    let mut sorted_boundary_verts: Vec<QPos> = boundary_verts.iter().copied().collect();
    sorted_boundary_verts.sort();

    for &start in &sorted_boundary_verts {
        if visited.contains(&start) {
            continue;
        }

        // BFS to find connected component (sort neighbors for determinism)
        let mut component: Vec<QPos> = Vec::new();
        let mut queue = vec![start];
        while let Some(v) = queue.pop() {
            if visited.contains(&v) {
                continue;
            }
            visited.insert(v);
            component.push(v);
            if let Some(neighbors) = vert_adj.get(&v) {
                let mut sorted_neighbors: Vec<QPos> = neighbors.iter().copied().collect();
                sorted_neighbors.sort();
                for &n in &sorted_neighbors {
                    if !visited.contains(&n) {
                        queue.push(n);
                    }
                }
            }
        }

        // Handle boundary components up to 64 vertices (raised from 32 to fill
        // larger boundary holes at complex boolean intersections, e.g.
        // cylinder-cylinder saddle curves with high tessellation resolution).
        if component.len() < 3 || component.len() > 64 {
            continue;
        }

        // Count boundary edges in this component
        let comp_set: HashSet<QPos> = component.iter().copied().collect();
        let comp_edges: Vec<(QPos, QPos)> = boundary_edges
            .iter()
            .filter(|(a, b)| comp_set.contains(a) && comp_set.contains(b))
            .copied()
            .collect();

        // For a triangle hole (3 edges, 3 vertices): add ONE triangle
        // that produces the 3 REVERSE edges needed to pair the boundary
        if component.len() == 3 && comp_edges.len() == 3 {
            let a = component[0];
            let b = component[1];
            let c = component[2];

            // We need to find a winding (a,b,c) such that the 3 half-edges
            // a→b, b→c, c→a are exactly the reverses of the 3 boundary edges.
            // Try both windings and pick the one that produces more reverse matches.
            let winding_abc = [
                boundary_edge_set.contains(&(b, a)),
                boundary_edge_set.contains(&(c, b)),
                boundary_edge_set.contains(&(a, c)),
            ];
            let winding_acb = [
                boundary_edge_set.contains(&(c, a)),
                boundary_edge_set.contains(&(b, c)),
                boundary_edge_set.contains(&(a, b)),
            ];

            let abc_matches: usize = winding_abc.iter().filter(|&&x| x).count();
            let acb_matches: usize = winding_acb.iter().filter(|&&x| x).count();

            if let (Some(&ia), Some(&ib), Some(&ic)) =
                (pos_to_idx.get(&a), pos_to_idx.get(&b), pos_to_idx.get(&c))
            {
                // Check area is non-degenerate
                let ai = ia as usize * 3;
                let bi = ib as usize * 3;
                let ci = ic as usize * 3;
                if ai + 2 < vertices.len() && bi + 2 < vertices.len() && ci + 2 < vertices.len() {
                    let ax = vertices[bi] - vertices[ai];
                    let ay = vertices[bi + 1] - vertices[ai + 1];
                    let az = vertices[bi + 2] - vertices[ai + 2];
                    let bx = vertices[ci] - vertices[ai];
                    let by = vertices[ci + 1] - vertices[ai + 1];
                    let bz = vertices[ci + 2] - vertices[ai + 2];
                    let cx_n = ay * bz - az * by;
                    let cy_n = az * bx - ax * bz;
                    let cz_n = ax * by - ay * bx;
                    let area = (cx_n * cx_n + cy_n * cy_n + cz_n * cz_n).sqrt() / 2.0;
                    if area >= TAU_WORK as f32 {
                        // Also consider stored vertex normals: the geometric
                        // normal of the fill triangle should agree with the
                        // average stored normal at its vertices.
                        let mut use_abc = abc_matches >= acb_matches;

                        // If edge matching is tied, use normals as tiebreaker.
                        // Also verify the edge-based choice against normals.
                        if ai + 2 < normals.len()
                            && bi + 2 < normals.len()
                            && ci + 2 < normals.len()
                        {
                            let snx = (normals[ai] + normals[bi] + normals[ci]) as f64 / 3.0;
                            let sny =
                                (normals[ai + 1] + normals[bi + 1] + normals[ci + 1]) as f64 / 3.0;
                            let snz =
                                (normals[ai + 2] + normals[bi + 2] + normals[ci + 2]) as f64 / 3.0;
                            // cx_n, cy_n, cz_n is the geometric normal for ABC winding
                            let dot = cx_n as f64 * snx + cy_n as f64 * sny + cz_n as f64 * snz;
                            // If normals disagree with edge-based choice, flip
                            if abc_matches == acb_matches {
                                use_abc = dot >= 0.0;
                            } else if (use_abc && dot < 0.0) || (!use_abc && dot > 0.0) {
                                // Edge matching and normals disagree — trust normals
                                use_abc = dot >= 0.0;
                            }
                        }

                        if use_abc {
                            fill_triangles.push([ia, ib, ic]);
                        } else {
                            fill_triangles.push([ia, ic, ib]);
                        }
                    }
                }
            }
        }

        // For a polygon hole (4+ edges, same number of vertices): trace the
        // boundary loop, determine winding, and fan-triangulate.
        // Upper bound raised to 32 to match the component limit above.
        if component.len() >= 4 && component.len() <= 32 && comp_edges.len() == component.len() {
            // Order vertices by tracing through the boundary edges (undirected)
            let target_len = component.len();
            let mut ordered: Vec<QPos> = vec![component[0]];
            let mut remaining: Vec<QPos> = component[1..].to_vec();
            remaining.sort();
            while !remaining.is_empty() && ordered.len() < target_len {
                let last = *ordered.last().unwrap();
                if let Some(pos) = remaining.iter().position(|&v| {
                    boundary_edge_set.contains(&(last, v)) || boundary_edge_set.contains(&(v, last))
                }) {
                    ordered.push(remaining.remove(pos));
                } else {
                    break;
                }
            }

            if ordered.len() == target_len {
                // Determine winding: count how many consecutive pairs (ordered[i], ordered[i+1])
                // match the REVERSE of a boundary edge (meaning our fill polygon's edge would
                // pair the boundary edge).
                let fwd_matches: usize = (0..target_len)
                    .filter(|&i| {
                        let a = ordered[i];
                        let b = ordered[(i + 1) % target_len];
                        boundary_edge_set.contains(&(b, a))
                    })
                    .count();
                let rev_matches: usize = (0..target_len)
                    .filter(|&i| {
                        let a = ordered[i];
                        let b = ordered[(i + 1) % target_len];
                        boundary_edge_set.contains(&(a, b))
                    })
                    .count();

                let reverse_winding = rev_matches > fwd_matches;

                // Resolve vertex indices
                let vert_indices: Vec<Option<u32>> =
                    ordered.iter().map(|q| pos_to_idx.get(q).copied()).collect();
                if vert_indices.iter().all(|v| v.is_some()) {
                    let vidx: Vec<u32> = vert_indices.into_iter().map(|v| v.unwrap()).collect();

                    // Fan triangulation from vertex 0
                    for j in 1..(target_len - 1) {
                        if reverse_winding {
                            fill_triangles.push([vidx[0], vidx[j + 1], vidx[j]]);
                        } else {
                            fill_triangles.push([vidx[0], vidx[j], vidx[j + 1]]);
                        }
                    }
                }
            }
        }

        // Open chain closure: when boundary edges form an open chain (not a
        // complete cycle), check if the chain endpoints are within 10× grid.
        // If so, snap them together and fill with fan triangles.
        // This handles S-H clipping divergence at cylinder-box intersection
        // boundaries where the tessellation produces almost-closed chains.
        // Only for chains up to 32 vertices to avoid filling large boundaries.
        if component.len() >= 3
            && component.len() <= 32
            && !comp_edges.is_empty()
            && comp_edges.len() < component.len()
        {
            // Build directed adjacency from boundary edges within this component
            let mut fwd: HashMap<QPos, QPos> = HashMap::new();
            let mut rev_map: HashMap<QPos, QPos> = HashMap::new();
            for &(a, b) in &comp_edges {
                fwd.insert(a, b);
                rev_map.insert(b, a);
            }

            // Find chain start: a vertex that has an outgoing boundary edge
            // but no incoming boundary edge within this component
            let chain_starts: Vec<QPos> = comp_edges
                .iter()
                .map(|&(a, _)| a)
                .filter(|a| !rev_map.contains_key(a))
                .collect();

            // We need exactly one chain start for a single open chain
            if chain_starts.len() == 1 {
                let chain_start = chain_starts[0];
                let mut chain: Vec<QPos> = vec![chain_start];
                let mut cur = chain_start;
                while let Some(&next) = fwd.get(&cur) {
                    chain.push(next);
                    cur = next;
                    if chain.len() > component.len() + 1 {
                        break; // safety valve
                    }
                }

                // Check if chain endpoints are within 10× grid distance
                let chain_end = *chain.last().unwrap();
                if chain.len() >= 3 && chain_start != chain_end {
                    if let (Some(&start_idx), Some(&end_idx)) =
                        (pos_to_idx.get(&chain_start), pos_to_idx.get(&chain_end))
                    {
                        let si = start_idx as usize * 3;
                        let ei = end_idx as usize * 3;
                        if si + 2 < vertices.len() && ei + 2 < vertices.len() {
                            let dx = (vertices[si] - vertices[ei]) as f64;
                            let dy = (vertices[si + 1] - vertices[ei + 1]) as f64;
                            let dz = (vertices[si + 2] - vertices[ei + 2]) as f64;
                            let dist_sq = dx * dx + dy * dy + dz * dz;
                            let snap_threshold = grid * 10.0;
                            let snap_threshold_sq = snap_threshold * snap_threshold;

                            if dist_sq <= snap_threshold_sq {
                                // Snap chain end to chain start position
                                vertices[ei] = vertices[si];
                                vertices[ei + 1] = vertices[si + 1];
                                vertices[ei + 2] = vertices[si + 2];

                                // Now the chain forms a closed loop — fill with fan triangles.
                                // Determine winding from boundary edges.
                                let fwd_matches: usize = (0..chain.len() - 1)
                                    .filter(|&i| {
                                        let a = chain[i];
                                        let b = chain[i + 1];
                                        boundary_edge_set.contains(&(b, a))
                                    })
                                    .count();
                                let rev_matches: usize = (0..chain.len() - 1)
                                    .filter(|&i| {
                                        let a = chain[i];
                                        let b = chain[i + 1];
                                        boundary_edge_set.contains(&(a, b))
                                    })
                                    .count();

                                let reverse_winding = rev_matches > fwd_matches;

                                // Use chain without duplicate end (it's snapped to start)
                                let loop_verts = &chain[..chain.len() - 1];
                                let vert_indices: Vec<Option<u32>> = loop_verts
                                    .iter()
                                    .map(|q| pos_to_idx.get(q).copied())
                                    .collect();
                                if vert_indices.iter().all(|v| v.is_some()) {
                                    let vidx: Vec<u32> =
                                        vert_indices.into_iter().map(|v| v.unwrap()).collect();
                                    let n = vidx.len();
                                    for j in 1..(n - 1) {
                                        if reverse_winding {
                                            fill_triangles.push([vidx[0], vidx[j + 1], vidx[j]]);
                                        } else {
                                            fill_triangles.push([vidx[0], vidx[j], vidx[j + 1]]);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if fill_triangles.is_empty() {
        return;
    }

    let fill_start = indices.len() as u32;
    for tri in &fill_triangles {
        indices.push(tri[0]);
        indices.push(tri[1]);
        indices.push(tri[2]);
    }
    let fill_end = indices.len() as u32;

    if fill_end > fill_start {
        face_ranges.push(FaceRange {
            face_id: KernelId(u64::MAX - 1), // synthetic boundary fill
            start_index: fill_start,
            end_index: fill_end,
        });
    }
}

/// Remove isolated triangles from the mesh.
///
/// An isolated triangle has ALL 3 edges appearing exactly once (no other
/// triangle shares any of its edges). These arise from stray face fragments
/// produced by Sutherland-Hodgman clipping at corner intersections — thin
/// slivers that the B-Rep stitching can't pair because no adjacent face
/// has matching edges.
///
/// Removal is safe because isolated triangles don't share edges with any
/// other triangle, so removing them doesn't break any existing edge pairings.
fn remove_isolated_triangles(
    vertices: &[f32],
    indices: &mut Vec<u32>,
    face_ranges: &mut Vec<FaceRange>,
) {
    let n_tris = indices.len() / 3;
    if n_tris < 2 {
        return;
    }

    // Compute oracle-compatible quantization grid for edge matching
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;
    let quantize = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize * 3;
        if i + 2 >= vertices.len() {
            return (0, 0, 0);
        }
        (
            (vertices[i] as f64 * inv_grid).round() as i64,
            (vertices[i + 1] as f64 * inv_grid).round() as i64,
            (vertices[i + 2] as f64 * inv_grid).round() as i64,
        )
    };

    type PosEdge = ((i64, i64, i64), (i64, i64, i64));
    fn make_edge(a: (i64, i64, i64), b: (i64, i64, i64)) -> PosEdge {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    // Build edge count map
    let mut edge_counts: HashMap<PosEdge, usize> = HashMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let va = quantize(indices[base]);
        let vb = quantize(indices[base + 1]);
        let vc = quantize(indices[base + 2]);
        *edge_counts.entry(make_edge(va, vb)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(vb, vc)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(vc, va)).or_insert(0) += 1;
    }

    // Mark triangles where ALL 3 edges are unpaired (count != 2)
    let mut keep = vec![true; n_tris];
    for (t, should_keep) in keep.iter_mut().enumerate().take(n_tris) {
        let base = t * 3;
        let va = quantize(indices[base]);
        let vb = quantize(indices[base + 1]);
        let vc = quantize(indices[base + 2]);
        let e1 = edge_counts.get(&make_edge(va, vb)).copied().unwrap_or(0);
        let e2 = edge_counts.get(&make_edge(vb, vc)).copied().unwrap_or(0);
        let e3 = edge_counts.get(&make_edge(vc, va)).copied().unwrap_or(0);
        if e1 != 2 && e2 != 2 && e3 != 2 {
            *should_keep = false;
        }
    }

    let removed = keep.iter().filter(|&&k| !k).count();
    #[cfg(test)]
    {
        let unpaired = edge_counts.values().filter(|&&c| c != 2).count();
        if unpaired > 0 || removed > 0 {
            eprintln!(
                "remove_isolated_triangles: n_tris={}, unpaired_edges={}, isolated_removed={}",
                n_tris, unpaired, removed
            );
            // Show triangles with any unpaired edges
            for t in 0..n_tris {
                let base = t * 3;
                let va = quantize(indices[base]);
                let vb = quantize(indices[base + 1]);
                let vc = quantize(indices[base + 2]);
                let e1 = edge_counts.get(&make_edge(va, vb)).copied().unwrap_or(0);
                let e2 = edge_counts.get(&make_edge(vb, vc)).copied().unwrap_or(0);
                let e3 = edge_counts.get(&make_edge(vc, va)).copied().unwrap_or(0);
                if e1 != 2 || e2 != 2 || e3 != 2 {
                    let i0 = indices[base] as usize;
                    let i1 = indices[base + 1] as usize;
                    let i2 = indices[base + 2] as usize;
                    eprintln!(
                        "  tri[{}]: edge_counts=({},{},{}) v0=({:.4},{:.4},{:.4}) v1=({:.4},{:.4},{:.4}) v2=({:.4},{:.4},{:.4})",
                        t, e1, e2, e3,
                        vertices[i0*3], vertices[i0*3+1], vertices[i0*3+2],
                        vertices[i1*3], vertices[i1*3+1], vertices[i1*3+2],
                        vertices[i2*3], vertices[i2*3+1], vertices[i2*3+2],
                    );
                }
            }
        }
    }
    if removed == 0 {
        return;
    }

    // Rebuild indices and face ranges without isolated triangles
    let mut new_indices = Vec::with_capacity(indices.len());
    let mut new_ranges = Vec::new();

    for range in face_ranges.iter() {
        let range_start = new_indices.len() as u32;
        let tri_start = range.start_index as usize / 3;
        let tri_end = range.end_index as usize / 3;

        for t in tri_start..tri_end {
            if t < n_tris && keep[t] {
                new_indices.push(indices[t * 3]);
                new_indices.push(indices[t * 3 + 1]);
                new_indices.push(indices[t * 3 + 2]);
            }
        }

        let range_end = new_indices.len() as u32;
        if range_end > range_start {
            new_ranges.push(FaceRange {
                face_id: range.face_id,
                start_index: range_start,
                end_index: range_end,
            });
        }
    }

    *indices = new_indices;
    *face_ranges = new_ranges;
}

/// Snap all vertex positions to the oracle's quantization grid.
///
/// The oracle uses grid = max(TAU_ORACLE_MIN, max_abs * TAU_ORACLE_FACTOR) to quantize vertex positions
/// for edge matching. Two vertices at positions P1 and P2 with |P1-P2| < grid/2
/// can still fall in adjacent grid cells, causing the oracle to see them as
/// different positions. By snapping all vertices to grid centers, we guarantee
/// that vertices within grid/2 of each other become exactly the same position.
///
/// Max position change: grid/2 ≈ 5e-5 at unit scale (0.05mm), well within
/// manufacturing tolerance and f32 visual precision.
#[allow(dead_code)]
/// Snap boundary vertex positions to the oracle's quantization grid.
/// Only vertices on unpaired edges are snapped, preserving interior mesh quality.
fn snap_boundary_to_oracle_grid(vertices: &mut [f32], indices: &[u32]) {
    if vertices.is_empty() || indices.is_empty() {
        return;
    }
    let n_verts = vertices.len() / 3;
    let n_tris = indices.len() / 3;

    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;

    let quantize = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize;
        if i >= n_verts {
            return (0, 0, 0);
        }
        (
            (vertices[i * 3] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 1] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 2] as f64 * inv_grid).round() as i64,
        )
    };

    type QPos = (i64, i64, i64);
    let make_edge = |a: QPos, b: QPos| -> (QPos, QPos) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };

    let mut edge_counts: HashMap<(QPos, QPos), usize> = HashMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let qt = [
            quantize(indices[base]),
            quantize(indices[base + 1]),
            quantize(indices[base + 2]),
        ];
        for e in 0..3 {
            let edge = make_edge(qt[e], qt[(e + 1) % 3]);
            if edge.0 != edge.1 {
                *edge_counts.entry(edge).or_insert(0) += 1;
            }
        }
    }

    // Collect boundary vertex indices
    let mut is_boundary = HashSet::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qt = [quantize(tri[0]), quantize(tri[1]), quantize(tri[2])];
        for e in 0..3 {
            let edge = make_edge(qt[e], qt[(e + 1) % 3]);
            if edge.0 != edge.1 && edge_counts.get(&edge).copied().unwrap_or(0) != 2 {
                is_boundary.insert(tri[e] as usize);
                is_boundary.insert(tri[(e + 1) % 3] as usize);
            }
        }
    }

    // Snap only boundary vertices to the grid
    for &vi in &is_boundary {
        if vi < n_verts {
            for j in 0..3 {
                let idx = vi * 3 + j;
                vertices[idx] = ((vertices[idx] as f64 * inv_grid).round() * grid) as f32;
            }
        }
    }
}

/// Heal boundary edges in the tessellated mesh.
///
/// After boolean polygon clipping, the B-Rep may have unpaired half-edges
/// where adjacent faces from different solids have slightly different vertex
/// positions at their shared boundary. This creates "cracks" in the mesh —
/// boundary edges (shared by only 1 triangle) that should be paired.
///
/// This function finds pairs of boundary edges going in opposite directions
/// with close midpoints, and snaps one edge's vertices to match the other's.
/// This is done at the mesh (f32) level, so the vertex merging is precise
/// (no f64→f32 round-trip issues).
#[allow(dead_code)]
fn heal_boundary_edges(vertices: &mut [f32], indices: &[u32]) {
    use std::collections::HashMap;

    let num_tris = indices.len() / 3;
    if num_tris == 0 {
        return;
    }

    // Compute scale-adaptive grid (same as oracle)
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = TAU_ORACLE_MIN.max(max_abs * TAU_ORACLE_FACTOR);
    let inv_grid = 1.0 / grid;

    // Quantize vertex positions (same as oracle)
    let qv = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize * 3;
        (
            (vertices[i] as f64 * inv_grid as f64).round() as i64,
            (vertices[i + 1] as f64 * inv_grid as f64).round() as i64,
            (vertices[i + 2] as f64 * inv_grid as f64).round() as i64,
        )
    };

    type QPos = (i64, i64, i64);
    type QEdge = (QPos, QPos);

    fn make_edge(a: QPos, b: QPos) -> QEdge {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    // Count edge occurrences (position-based)
    let mut edge_counts: HashMap<QEdge, usize> = HashMap::new();
    for tri in indices.chunks(3) {
        let va = qv(tri[0]);
        let vb = qv(tri[1]);
        let vc = qv(tri[2]);
        *edge_counts.entry(make_edge(va, vb)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(vb, vc)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(vc, va)).or_insert(0) += 1;
    }

    // Collect boundary edges: count != 2, with vertex indices
    let boundary_edges: std::collections::HashSet<QEdge> = edge_counts
        .iter()
        .filter(|(_, &c)| c != 2)
        .map(|(e, _)| *e)
        .collect();

    if boundary_edges.is_empty() {
        return; // Already watertight
    }

    // Build directed boundary edge list with actual vertex indices
    // For each boundary edge, we need the raw vertex indices to snap positions
    struct BoundaryHE {
        v0: u32,
        v1: u32,
        qv0: QPos,
        qv1: QPos,
    }
    let mut boundary_hes: Vec<BoundaryHE> = Vec::new();

    for tri in indices.chunks(3) {
        let edges = [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])];
        for (v0, v1) in edges {
            let q0 = qv(v0);
            let q1 = qv(v1);
            if boundary_edges.contains(&make_edge(q0, q1)) {
                boundary_hes.push(BoundaryHE {
                    v0,
                    v1,
                    qv0: q0,
                    qv1: q1,
                });
            }
        }
    }

    // For each boundary half-edge, look for a reverse-direction boundary half-edge
    // whose quantized endpoints are CLOSE (within 1 grid cell) but not identical
    // (identical ones would already be paired by the edge_counts check).
    //
    // Use a spatial hash on the midpoint for fast lookup.
    let mid_grid = grid * 5.0; // Coarser grid for midpoint matching
    let inv_mid = 1.0 / mid_grid as f64;

    // Compute midpoints eagerly to avoid borrowing vertices during snap
    let midpoints: Vec<(f32, f32, f32)> = boundary_hes
        .iter()
        .map(|he| {
            let i0 = he.v0 as usize * 3;
            let i1 = he.v1 as usize * 3;
            (
                (vertices[i0] + vertices[i1]) * 0.5,
                (vertices[i0 + 1] + vertices[i1 + 1]) * 0.5,
                (vertices[i0 + 2] + vertices[i1 + 2]) * 0.5,
            )
        })
        .collect();

    let qmid = |mp: &(f32, f32, f32)| -> (i64, i64, i64) {
        (
            (mp.0 as f64 * inv_mid).round() as i64,
            (mp.1 as f64 * inv_mid).round() as i64,
            (mp.2 as f64 * inv_mid).round() as i64,
        )
    };

    // Group boundary HEs by quantized midpoint
    let mut mid_map: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (idx, mp) in midpoints.iter().enumerate() {
        let qm = qmid(mp);
        mid_map.entry(qm).or_default().push(idx);
    }

    // Track which vertex indices have been snapped (avoid double-snap)
    let mut snapped: std::collections::HashSet<u32> = std::collections::HashSet::new();

    // For each boundary HE, find a matching reverse-direction HE
    for i in 0..boundary_hes.len() {
        let he_a = &boundary_hes[i];
        if snapped.contains(&he_a.v0) || snapped.contains(&he_a.v1) {
            continue;
        }
        let qm = qmid(&midpoints[i]);

        // Check 3x3x3 neighborhood
        for dx in -1..=1_i64 {
            for dy in -1..=1_i64 {
                for dz in -1..=1_i64 {
                    let key = (qm.0 + dx, qm.1 + dy, qm.2 + dz);
                    if let Some(candidates) = mid_map.get(&key) {
                        for &j in candidates {
                            if j <= i {
                                continue;
                            }
                            let he_b = &boundary_hes[j];
                            if snapped.contains(&he_b.v0) || snapped.contains(&he_b.v1) {
                                continue;
                            }

                            // Already matched by oracle grid? Skip (already paired).
                            if he_a.qv0 == he_b.qv1 && he_a.qv1 == he_b.qv0 {
                                continue;
                            }

                            // Check reverse direction using actual f32 distances.
                            // A goes v0→v1, B should go ~v1→~v0.
                            let a0 = he_a.v0 as usize * 3;
                            let a1 = he_a.v1 as usize * 3;
                            let b0 = he_b.v0 as usize * 3;
                            let b1 = he_b.v1 as usize * 3;

                            // Compute edge length of A
                            let edge_len_sq = (vertices[a1] - vertices[a0]).powi(2)
                                + (vertices[a1 + 1] - vertices[a0 + 1]).powi(2)
                                + (vertices[a1 + 2] - vertices[a0 + 2]).powi(2);

                            // Vertex distances: A.v0↔B.v1, A.v1↔B.v0
                            let d01_sq = (vertices[a0] - vertices[b1]).powi(2)
                                + (vertices[a0 + 1] - vertices[b1 + 1]).powi(2)
                                + (vertices[a0 + 2] - vertices[b1 + 2]).powi(2);
                            let d10_sq = (vertices[a1] - vertices[b0]).powi(2)
                                + (vertices[a1 + 1] - vertices[b0 + 1]).powi(2)
                                + (vertices[a1 + 2] - vertices[b0 + 2]).powi(2);

                            // Each vertex mismatch must be < 10% of edge length
                            // and < 5 oracle grid cells (absolute limit)
                            let rel_tol_sq = edge_len_sq * 0.01; // 10% squared
                            let abs_tol = grid * 5.0;
                            let abs_tol_sq = abs_tol * abs_tol;
                            let tol_sq = rel_tol_sq.min(abs_tol_sq);

                            if d01_sq > tol_sq || d10_sq > tol_sq {
                                continue;
                            }

                            // Snap B's vertex positions to A's positions
                            // B.v0 ≈ A.v1, B.v1 ≈ A.v0 (reverse direction)
                            vertices[b0] = vertices[a1];
                            vertices[b0 + 1] = vertices[a1 + 1];
                            vertices[b0 + 2] = vertices[a1 + 2];
                            vertices[b1] = vertices[a0];
                            vertices[b1 + 1] = vertices[a0 + 1];
                            vertices[b1 + 2] = vertices[a0 + 2];

                            snapped.insert(he_b.v0);
                            snapped.insert(he_b.v1);
                        }
                    }
                }
            }
        }
    }
}

/// Snap nearby vertex positions so coincident vertices have identical f32 values.
///
/// Per-face tessellation creates separate vertices at shared B-Rep edges.
/// After boolean operations, independent polygon clipping can produce tiny
/// position differences at shared edges. This function groups vertices within
/// a scale-adaptive tolerance and sets all vertices in each group to the same
/// position (the first vertex encountered in that group).
///
/// Normals and indices are NOT modified — only positions are snapped.
/// This preserves sharp-edge normals while ensuring the oracle's watertight
/// check sees matching edge endpoints.
#[allow(dead_code)]
fn snap_close_positions(vertices: &mut [f32]) {
    use std::collections::HashMap;

    let num_verts = vertices.len() / 3;
    if num_verts < 2 {
        return;
    }

    // Compute bounding box diagonal for scale-adaptive tolerance
    let mut bbox_min = [f32::INFINITY; 3];
    let mut bbox_max = [f32::NEG_INFINITY; 3];
    for i in 0..num_verts {
        for j in 0..3 {
            let v = vertices[i * 3 + j];
            if v < bbox_min[j] {
                bbox_min[j] = v;
            }
            if v > bbox_max[j] {
                bbox_max[j] = v;
            }
        }
    }
    let diag = ((bbox_max[0] - bbox_min[0]).powi(2)
        + (bbox_max[1] - bbox_min[1]).powi(2)
        + (bbox_max[2] - bbox_min[2]).powi(2))
    .sqrt();

    if diag < TAU_WORK as f32 {
        return;
    }

    // Oracle-safe tolerance: 1% of the oracle's quantization grid.
    // Oracle grid = max(TAU_ORACLE_MIN, max_abs * TAU_ORACLE_FACTOR). We snap at 1% of this
    // to ensure vertices never move across oracle grid boundaries.
    let max_abs = bbox_max[0]
        .abs()
        .max(bbox_max[1].abs())
        .max(bbox_max[2].abs())
        .max(bbox_min[0].abs())
        .max(bbox_min[1].abs())
        .max(bbox_min[2].abs());
    let oracle_grid = TAU_ORACLE_MIN.max(max_abs * TAU_ORACLE_FACTOR);
    let tolerance = oracle_grid * 0.01;
    let tol_sq = tolerance * tolerance;
    let inv_cell = 1.0 / tolerance;

    // Spatial hash: cell -> list of vertex indices
    let mut cells: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
    for i in 0..num_verts {
        let cx = (vertices[i * 3] * inv_cell).floor() as i32;
        let cy = (vertices[i * 3 + 1] * inv_cell).floor() as i32;
        let cz = (vertices[i * 3 + 2] * inv_cell).floor() as i32;
        cells.entry((cx, cy, cz)).or_default().push(i);
    }

    // For each vertex, find the earliest vertex in the neighborhood that's close enough.
    // Use union-find style: snap to the canonical (lowest-index) vertex in each cluster.
    let mut snap_target: Vec<usize> = (0..num_verts).collect();

    for i in 0..num_verts {
        if snap_target[i] != i {
            continue; // Already snapped to an earlier vertex
        }
        let px = vertices[i * 3];
        let py = vertices[i * 3 + 1];
        let pz = vertices[i * 3 + 2];
        let cx = (px * inv_cell).floor() as i32;
        let cy = (py * inv_cell).floor() as i32;
        let cz = (pz * inv_cell).floor() as i32;

        // Check 3x3x3 neighborhood for later vertices to snap to this one
        for dx in -1..=1_i32 {
            for dy in -1..=1_i32 {
                for dz in -1..=1_i32 {
                    if let Some(neighbors) = cells.get(&(cx + dx, cy + dy, cz + dz)) {
                        for &j in neighbors {
                            if j <= i || snap_target[j] != j {
                                continue;
                            }
                            let dist_sq = (vertices[j * 3] - px).powi(2)
                                + (vertices[j * 3 + 1] - py).powi(2)
                                + (vertices[j * 3 + 2] - pz).powi(2);
                            if dist_sq < tol_sq {
                                snap_target[j] = i;
                            }
                        }
                    }
                }
            }
        }
    }

    // Apply: set each snapped vertex's position to its target's position
    for i in 0..num_verts {
        let t = snap_target[i];
        if t != i {
            vertices[i * 3] = vertices[t * 3];
            vertices[i * 3 + 1] = vertices[t * 3 + 1];
            vertices[i * 3 + 2] = vertices[t * 3 + 2];
        }
    }
}

/// Weld mesh vertices by merging nearby positions.
///
/// Per-face tessellation creates separate vertices for each face's edges.
/// Adjacent faces produce duplicate vertices at shared edges. This function
/// merges vertices within a quantization tolerance, remapping triangle indices
/// to produce a mesh where adjacent triangles share vertex indices.
///
/// This is critical for watertight mesh output from boolean result B-Reps
/// that have minor gaps from polygon-clipping artifacts.
#[allow(dead_code)]
fn weld_mesh_vertices(
    vertices: Vec<f32>,
    normals: Vec<f32>,
    indices: Vec<u32>,
    face_ranges: Vec<FaceRange>,
) -> RenderMesh {
    use std::collections::HashMap;

    let num_verts = vertices.len() / 3;
    if num_verts == 0 {
        return RenderMesh {
            vertices,
            normals,
            indices,
            face_ranges,
        };
    }

    // Compute bounding box to determine adaptive quantization tolerance
    let mut bbox_min = [f64::INFINITY; 3];
    let mut bbox_max = [f64::NEG_INFINITY; 3];
    for i in 0..num_verts {
        for j in 0..3 {
            let v = vertices[i * 3 + j] as f64;
            if v < bbox_min[j] {
                bbox_min[j] = v;
            }
            if v > bbox_max[j] {
                bbox_max[j] = v;
            }
        }
    }

    let diag = ((bbox_max[0] - bbox_min[0]).powi(2)
        + (bbox_max[1] - bbox_min[1]).powi(2)
        + (bbox_max[2] - bbox_min[2]).powi(2))
    .sqrt();

    // Use TAU_TESS_GRID_FACTOR relative to diagonal, clamped for safety.
    // f32 has ~7 digits precision, so 1e-5 relative is well above noise floor.
    let tau = (diag * TAU_TESS_GRID_FACTOR).clamp(TAU_TESS_WELD_MIN, TAU_TESS_WELD_MAX);
    let inv_tau = 1.0 / tau;

    // Build vertex welding map: quantize (position + normal) → canonical vertex index.
    // Vertices are only merged when they have BOTH the same position AND
    // the same normal (within tolerance). This preserves per-face normals
    // at sharp edges while merging duplicate vertices at shared edges.
    let normal_quant = 100.0; // Quantize normal components to 0.01 resolution
    let mut pn_to_new_idx: HashMap<(i64, i64, i64, i64, i64, i64), u32> = HashMap::new();
    let mut old_to_new: Vec<u32> = Vec::with_capacity(num_verts);
    let mut new_vertices: Vec<f32> = Vec::new();
    let mut new_normals: Vec<f32> = Vec::new();

    for i in 0..num_verts {
        let px = vertices[i * 3] as f64;
        let py = vertices[i * 3 + 1] as f64;
        let pz = vertices[i * 3 + 2] as f64;
        let nx = normals[i * 3] as f64;
        let ny = normals[i * 3 + 1] as f64;
        let nz = normals[i * 3 + 2] as f64;
        let key = (
            (px * inv_tau).round() as i64,
            (py * inv_tau).round() as i64,
            (pz * inv_tau).round() as i64,
            (nx * normal_quant).round() as i64,
            (ny * normal_quant).round() as i64,
            (nz * normal_quant).round() as i64,
        );

        let new_idx = pn_to_new_idx.entry(key).or_insert_with(|| {
            let idx = new_vertices.len() as u32 / 3;
            new_vertices.push(vertices[i * 3]);
            new_vertices.push(vertices[i * 3 + 1]);
            new_vertices.push(vertices[i * 3 + 2]);
            new_normals.push(normals[i * 3]);
            new_normals.push(normals[i * 3 + 1]);
            new_normals.push(normals[i * 3 + 2]);
            idx
        });
        old_to_new.push(*new_idx);
    }

    // Remap indices
    let new_indices: Vec<u32> = indices
        .iter()
        .map(|&idx| old_to_new[idx as usize])
        .collect();

    // Remove degenerate triangles (where two or more vertices merged to the same index)
    let mut final_indices: Vec<u32> = Vec::with_capacity(new_indices.len());
    let mut new_face_ranges: Vec<FaceRange> = Vec::new();

    for fr in &face_ranges {
        let start = final_indices.len() as u32;
        let tri_start = fr.start_index as usize / 3;
        let tri_end = fr.end_index as usize / 3;
        for t in tri_start..tri_end {
            let a = new_indices[t * 3];
            let b = new_indices[t * 3 + 1];
            let c = new_indices[t * 3 + 2];
            if a != b && b != c && a != c {
                final_indices.push(a);
                final_indices.push(b);
                final_indices.push(c);
            }
        }
        let end = final_indices.len() as u32;
        new_face_ranges.push(FaceRange {
            face_id: fr.face_id,
            start_index: start,
            end_index: end,
        });
    }

    RenderMesh {
        vertices: new_vertices,
        normals: new_normals,
        indices: final_indices,
        face_ranges: new_face_ranges,
    }
}

/// Tessellate a complete sphere solid with shared vertices.
///
/// Builds an icosphere-style mesh from the octahedral B-Rep: each of the 8
/// octahedral triangles is subdivided, all vertices are projected onto the sphere,
/// and shared vertices on edges/corners are welded for watertightness.
fn tessellate_sphere_solid(
    arena: &TopoArena,
    face_map: &HashMap<u64, FaceIdx>,
    sp: &SphereParams,
) -> Result<RenderMesh, KernelError> {
    let center = sp.center;
    let radius = sp.radius;
    let n = CIRCLE_SEGMENTS / 4; // subdivision level per edge

    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    // Use a position-based vertex map for sharing: round positions to avoid
    // floating-point mismatch on shared edges.
    let mut vertex_cache: HashMap<[i64; 3], u32> = HashMap::new();

    // Quantization: snap sphere-surface positions to a grid fine enough for
    // the subdivision but coarse enough for f32 fidelity.
    // Use relative precision: grid step = radius * 1e-9
    let quant = radius * 1e-9;
    let quant_inv = 1.0 / quant;

    let quantize = |pos: [f64; 3]| -> [i64; 3] {
        [
            (pos[0] * quant_inv).round() as i64,
            (pos[1] * quant_inv).round() as i64,
            (pos[2] * quant_inv).round() as i64,
        ]
    };

    let add_vertex = |pos: [f64; 3],
                      vertex_cache: &mut HashMap<[i64; 3], u32>,
                      vertices: &mut Vec<f32>,
                      normals: &mut Vec<f32>|
     -> u32 {
        let key = quantize(pos);
        if let Some(&idx) = vertex_cache.get(&key) {
            return idx;
        }
        let idx = vertices.len() as u32 / 3;

        // Project onto sphere
        let dx = pos[0] - center[0];
        let dy = pos[1] - center[1];
        let dz = pos[2] - center[2];
        let len = (dx * dx + dy * dy + dz * dz).sqrt();
        let scale = radius / len;

        let sx = center[0] + dx * scale;
        let sy = center[1] + dy * scale;
        let sz = center[2] + dz * scale;

        let nx = dx / len;
        let ny = dy / len;
        let nz = dz / len;

        vertices.push(sx as f32);
        vertices.push(sy as f32);
        vertices.push(sz as f32);
        normals.push(nx as f32);
        normals.push(ny as f32);
        normals.push(nz as f32);

        vertex_cache.insert(key, idx);
        idx
    };

    // Sort face_map entries for deterministic tessellation order.
    let mut sorted_faces: Vec<(u64, FaceIdx)> = face_map.iter().map(|(&k, &v)| (k, v)).collect();
    sorted_faces.sort_by_key(|(k, _)| *k);

    for &(kid, face_idx) in &sorted_faces {
        // Collect the 3 vertices of this triangular face
        let loop_idx = arena.faces[face_idx.0].outer_loop;
        let start_he = arena.loops[loop_idx.0].half_edge;
        let mut face_verts = Vec::new();
        let mut he = start_he;
        loop {
            let v = arena.half_edges[he.0].origin;
            face_verts.push(arena.vertices[v.0].position);
            he = arena.half_edges[he.0].next;
            if he == start_he {
                break;
            }
        }

        if face_verts.len() != 3 {
            continue;
        }

        let p0 = face_verts[0];
        let p1 = face_verts[1];
        let p2 = face_verts[2];

        let start_index = indices.len() as u32;

        // Build vertex grid for this face using barycentric subdivision
        // Row i (0..=n) has (n-i+1) vertices.
        // We store vertex indices in a flat array for indexing.
        let mut grid: Vec<Vec<u32>> = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let mut row = Vec::with_capacity(n - i + 1);
            for j in 0..=(n - i) {
                let k = n - i - j;
                let u = k as f64 / n as f64;
                let v_bc = j as f64 / n as f64;
                let w = i as f64 / n as f64;

                let px = u * p0[0] + v_bc * p1[0] + w * p2[0];
                let py = u * p0[1] + v_bc * p1[1] + w * p2[1];
                let pz = u * p0[2] + v_bc * p1[2] + w * p2[2];

                let idx = add_vertex([px, py, pz], &mut vertex_cache, &mut vertices, &mut normals);
                row.push(idx);
            }
            grid.push(row);
        }

        // Generate triangle indices
        for i in 0..n {
            let row_len = grid[i].len();
            for j in 0..(row_len - 1) {
                // Upper triangle
                indices.push(grid[i][j]);
                indices.push(grid[i][j + 1]);
                indices.push(grid[i + 1][j]);

                // Lower triangle (if exists)
                if j + 1 < grid[i + 1].len() {
                    indices.push(grid[i][j + 1]);
                    indices.push(grid[i + 1][j + 1]);
                    indices.push(grid[i + 1][j]);
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

    Ok(RenderMesh {
        vertices,
        normals,
        indices,
        face_ranges,
    })
}

/// Tessellate a single spherical face of an octahedral sphere decomposition.
///
/// Each face is a triangular patch on the sphere (3 octahedral vertices).
/// We subdivide the triangle using barycentric coordinates, project each
/// point onto the sphere surface, and generate a triangle mesh.
fn tessellate_sphere_face(
    arena: &TopoArena,
    face_idx: FaceIdx,
    sp: &SphereParams,
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    // Collect the 3 vertices of this triangular face
    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;
    let mut face_verts = Vec::new();
    let mut he = start_he;
    loop {
        let v = arena.half_edges[he.0].origin;
        face_verts.push(arena.vertices[v.0].position);
        he = arena.half_edges[he.0].next;
        if he == start_he {
            break;
        }
    }

    if face_verts.len() != 3 {
        return; // Should not happen for octahedral sphere
    }

    let p0 = face_verts[0];
    let p1 = face_verts[1];
    let p2 = face_verts[2];
    let center = sp.center;
    let radius = sp.radius;

    // Subdivision level: CIRCLE_SEGMENTS / 4
    let n = CIRCLE_SEGMENTS / 4;

    // Generate (n+1)*(n+2)/2 vertices on the sphere by barycentric subdivision
    // For row i (0..=n), we have (n-i+1) points.
    // Barycentric coords: (1 - i/n - j/n, j/n, i/n) for j in 0..=(n-i)
    let base_vertex = vertices.len() as u32 / 3;
    for i in 0..=n {
        for j in 0..=(n - i) {
            let k = n - i - j;
            let u = k as f64 / n as f64;
            let v_bc = j as f64 / n as f64;
            let w = i as f64 / n as f64;

            // Interpolate in 3D
            let px = u * p0[0] + v_bc * p1[0] + w * p2[0];
            let py = u * p0[1] + v_bc * p1[1] + w * p2[1];
            let pz = u * p0[2] + v_bc * p1[2] + w * p2[2];

            // Project onto sphere: v' = center + r * normalize(v - center)
            let dx = px - center[0];
            let dy = py - center[1];
            let dz = pz - center[2];
            let len = (dx * dx + dy * dy + dz * dz).sqrt();
            let scale = radius / len;

            let sx = center[0] + dx * scale;
            let sy = center[1] + dy * scale;
            let sz = center[2] + dz * scale;

            // Normal = normalize(v - center) = (dx, dy, dz) / len
            let nx = dx / len;
            let ny = dy / len;
            let nz = dz / len;

            vertices.push(sx as f32);
            vertices.push(sy as f32);
            vertices.push(sz as f32);
            normals.push(nx as f32);
            normals.push(ny as f32);
            normals.push(nz as f32);
        }
    }

    // Generate triangle indices.
    // Row i has (n-i+1) vertices. Vertex (i,j) index = sum of row lengths before i, plus j.
    let vertex_index = |i: usize, j: usize| -> u32 {
        let mut idx = 0usize;
        for r in 0..i {
            idx += n - r + 1;
        }
        base_vertex + (idx + j) as u32
    };

    for i in 0..n {
        let row_len = n - i + 1;
        for j in 0..(row_len - 1) {
            // Upper triangle: (i,j), (i,j+1), (i+1,j)
            indices.push(vertex_index(i, j));
            indices.push(vertex_index(i, j + 1));
            indices.push(vertex_index(i + 1, j));

            // Lower triangle (if it exists): (i,j+1), (i+1,j+1), (i+1,j)
            if j + 1 < row_len - 1 {
                indices.push(vertex_index(i, j + 1));
                indices.push(vertex_index(i + 1, j + 1));
                indices.push(vertex_index(i + 1, j));
            }
        }
    }
}

/// Tessellate a complete cone solid with shared vertices.
///
/// Generates the cone mesh directly from ConeParams (not per-B-Rep-face),
/// ensuring all shared edges have identical vertex positions for watertightness.
/// The base is a fan from center, lateral surface is rings from apex to base.
fn tessellate_cone_solid(
    _arena: &TopoArena,
    face_map: &HashMap<u64, FaceIdx>,
    face_geometry: &HashMap<FaceIdx, SurfaceGeom>,
    cp: &ConeParams,
) -> Result<RenderMesh, KernelError> {
    let base_center = cp.base_center;
    let apex = cp.apex;
    let axis = cp.axis;
    let radius = cp.radius;
    let height = cp.height;
    let nseg = CIRCLE_SEGMENTS; // segments around full circle

    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    // Build orthonormal basis for base circle
    let u_axis = {
        let trial = if axis[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let cross = v3_cross(axis, trial);
        v3_normalize(cross)
    };
    let v_axis = v3_cross(axis, u_axis);

    let half_angle = radius.atan2(height);
    let sin_ha = half_angle.sin();
    let cos_ha = half_angle.cos();

    // Precompute base circle positions
    let mut base_pts: Vec<[f64; 3]> = Vec::with_capacity(nseg);
    let mut base_norms_lateral: Vec<[f64; 3]> = Vec::with_capacity(nseg);
    for i in 0..nseg {
        let theta = 2.0 * std::f64::consts::PI * i as f64 / nseg as f64;
        let ct = theta.cos();
        let st = theta.sin();
        base_pts.push([
            base_center[0] + u_axis[0] * radius * ct + v_axis[0] * radius * st,
            base_center[1] + u_axis[1] * radius * ct + v_axis[1] * radius * st,
            base_center[2] + u_axis[2] * radius * ct + v_axis[2] * radius * st,
        ]);
        // Outward normal on cone surface
        let radial = [
            u_axis[0] * ct + v_axis[0] * st,
            u_axis[1] * ct + v_axis[1] * st,
            u_axis[2] * ct + v_axis[2] * st,
        ];
        base_norms_lateral.push([
            radial[0] * cos_ha + axis[0] * sin_ha,
            radial[1] * cos_ha + axis[1] * sin_ha,
            radial[2] * cos_ha + axis[2] * sin_ha,
        ]);
    }

    let push_vert =
        |pos: [f64; 3], norm: [f64; 3], verts: &mut Vec<f32>, norms: &mut Vec<f32>| -> u32 {
            let idx = verts.len() as u32 / 3;
            verts.push(pos[0] as f32);
            verts.push(pos[1] as f32);
            verts.push(pos[2] as f32);
            norms.push(norm[0] as f32);
            norms.push(norm[1] as f32);
            norms.push(norm[2] as f32);
            idx
        };

    // Find the face IDs for base (planar) and lateral (conical) faces
    let mut base_face_kid: Option<u64> = None;
    let mut lateral_face_kids: Vec<u64> = Vec::new();
    // Sort face_map entries for deterministic iteration.
    let mut sorted_faces_pre: Vec<(u64, FaceIdx)> =
        face_map.iter().map(|(&k, &v)| (k, v)).collect();
    sorted_faces_pre.sort_by_key(|(k, _)| *k);

    for &(kid, face_idx) in &sorted_faces_pre {
        let geom = face_geometry.get(&face_idx);
        if matches!(geom, Some(SurfaceGeom::Planar(_))) {
            base_face_kid = Some(kid);
        } else {
            lateral_face_kids.push(kid);
        }
    }

    // === Lateral surface: ring strips from apex to base ===
    // Use shared vertices between lateral faces (all part of the same cone surface).
    // Assign triangles to lateral face_ids by quadrant.
    {
        // Apex vertex (single shared vertex with averaged normal = axis direction)
        let apex_idx = push_vert(apex, axis, &mut vertices, &mut normals);

        // Ring vertices at multiple heights
        let nrings = CIRCLE_SEGMENTS / 4; // subdivision rings from apex to base
        let mut rings: Vec<Vec<u32>> = Vec::with_capacity(nrings);

        for ring in 1..=nrings {
            let t = ring as f64 / nrings as f64; // 0 = apex, 1 = base
            let r = radius * t;
            let h = height * (1.0 - t);
            let mut ring_verts = Vec::with_capacity(nseg);

            for (i, norm) in base_norms_lateral.iter().enumerate() {
                let theta = 2.0 * std::f64::consts::PI * i as f64 / nseg as f64;
                let ct = theta.cos();
                let st = theta.sin();
                let pos = [
                    base_center[0] + u_axis[0] * r * ct + v_axis[0] * r * st + axis[0] * h,
                    base_center[1] + u_axis[1] * r * ct + v_axis[1] * r * st + axis[1] * h,
                    base_center[2] + u_axis[2] * r * ct + v_axis[2] * r * st + axis[2] * h,
                ];
                let idx = push_vert(pos, *norm, &mut vertices, &mut normals);
                ring_verts.push(idx);
            }
            rings.push(ring_verts);
        }

        // Track triangles per quadrant for face_ranges
        let segs_per_quad = nseg / 4;
        let mut quad_starts = [indices.len() as u32; 4];

        // First ring: fan from apex
        for i in 0..nseg {
            let j = (i + 1) % nseg;
            let quad = i / segs_per_quad;
            if quad < 4 && indices.len() as u32 > quad_starts[quad] {
                // Already started
            } else if quad < 4 {
                quad_starts[quad] = indices.len() as u32;
            }
            indices.push(apex_idx);
            indices.push(rings[0][i]);
            indices.push(rings[0][j]);
        }

        // Subsequent rings: quad strips
        for ring in 1..nrings {
            for i in 0..nseg {
                let j = (i + 1) % nseg;
                // Two triangles per quad
                indices.push(rings[ring - 1][i]);
                indices.push(rings[ring][i]);
                indices.push(rings[ring][j]);

                indices.push(rings[ring - 1][i]);
                indices.push(rings[ring][j]);
                indices.push(rings[ring - 1][j]);
            }
        }

        // Assign all lateral triangles to the lateral face IDs (distribute by quadrant)
        let total_lateral_indices = indices.len() as u32;
        if !lateral_face_kids.is_empty() {
            // Simple: assign all lateral triangles to the first lateral face
            // (face_ranges just need valid mappings for picking)
            let tris_per_face = (total_lateral_indices / 3) / lateral_face_kids.len() as u32;
            let mut start = 0u32;
            for (fi, &kid) in lateral_face_kids.iter().enumerate() {
                let end = if fi == lateral_face_kids.len() - 1 {
                    total_lateral_indices
                } else {
                    // Round to triangle boundary
                    ((fi as u32 + 1) * tris_per_face) * 3
                };
                face_ranges.push(FaceRange {
                    face_id: KernelId(kid),
                    start_index: start,
                    end_index: end,
                });
                start = end;
            }
        }
    }

    // === Base face: fan from center ===
    if let Some(base_kid) = base_face_kid {
        let base_norm = [-axis[0], -axis[1], -axis[2]]; // outward (away from interior)
        let center_idx = push_vert(base_center, base_norm, &mut vertices, &mut normals);

        let base_start = indices.len() as u32;

        // Create base circle vertices (separate from lateral for correct normals)
        let mut base_ring: Vec<u32> = Vec::with_capacity(nseg);
        for pt in &base_pts {
            let idx = push_vert(*pt, base_norm, &mut vertices, &mut normals);
            base_ring.push(idx);
        }

        for i in 0..nseg {
            let j = (i + 1) % nseg;
            // Winding: center → i → j gives outward normal matching base_norm
            // when base_norm = -axis (pointing down for axis=+Z)
            indices.push(center_idx);
            indices.push(base_ring[j]);
            indices.push(base_ring[i]);
        }

        let base_end = indices.len() as u32;
        face_ranges.push(FaceRange {
            face_id: KernelId(base_kid),
            start_index: base_start,
            end_index: base_end,
        });
    }

    Ok(RenderMesh {
        vertices,
        normals,
        indices,
        face_ranges,
    })
}

/// Tessellate a torus solid using parametric (u,v) grid evaluation.
///
/// Generates a shared-vertex mesh with `n_u × n_v` quads (each split into 2 triangles).
/// Normals point outward from the tube surface.
fn tessellate_torus_solid(
    face_map: &HashMap<u64, FaceIdx>,
    tp: &TorusParams,
) -> Result<RenderMesh, KernelError> {
    let center = tp.center;
    let axis = tp.axis;
    let big_r = tp.major_radius;
    let small_r = tp.minor_radius;

    // Resolution: use CIRCLE_SEGMENTS for major, CIRCLE_SEGMENTS/2 for minor
    let n_u = CIRCLE_SEGMENTS; // major (around the ring)
    let n_v = CIRCLE_SEGMENTS / 2; // minor (around the tube cross-section)

    // Build orthonormal basis
    let e1 = {
        let trial = if axis[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let cross = v3_cross(axis, trial);
        v3_normalize(cross)
    };
    let e2 = v3_cross(axis, e1);

    let mut vertices: Vec<f32> = Vec::with_capacity(n_u * n_v * 3);
    let mut normals: Vec<f32> = Vec::with_capacity(n_u * n_v * 3);
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    // Generate vertices on the torus surface
    // Vertex (i, j) is at parameter (u, v) where u = 2π*i/n_u, v = 2π*j/n_v
    for i in 0..n_u {
        let u = 2.0 * std::f64::consts::PI * i as f64 / n_u as f64;
        let cos_u = u.cos();
        let sin_u = u.sin();
        for j in 0..n_v {
            let v = 2.0 * std::f64::consts::PI * j as f64 / n_v as f64;
            let cos_v = v.cos();
            let sin_v = v.sin();

            let r = big_r + small_r * cos_v;

            // Position on torus
            let px = center[0] + r * (cos_u * e1[0] + sin_u * e2[0]) + small_r * sin_v * axis[0];
            let py = center[1] + r * (cos_u * e1[1] + sin_u * e2[1]) + small_r * sin_v * axis[1];
            let pz = center[2] + r * (cos_u * e1[2] + sin_u * e2[2]) + small_r * sin_v * axis[2];

            // Normal: direction from tube center circle to surface point
            // Tube center at this u: center + R*(cos(u)*e1 + sin(u)*e2)
            let tube_cx = center[0] + big_r * (cos_u * e1[0] + sin_u * e2[0]);
            let tube_cy = center[1] + big_r * (cos_u * e1[1] + sin_u * e2[1]);
            let tube_cz = center[2] + big_r * (cos_u * e1[2] + sin_u * e2[2]);

            let nx = px - tube_cx;
            let ny = py - tube_cy;
            let nz = pz - tube_cz;
            let nlen = (nx * nx + ny * ny + nz * nz).sqrt();

            vertices.push(px as f32);
            vertices.push(py as f32);
            vertices.push(pz as f32);
            normals.push((nx / nlen) as f32);
            normals.push((ny / nlen) as f32);
            normals.push((nz / nlen) as f32);
        }
    }

    // Generate indices: quads split into 2 triangles each
    // Distribute triangles across face IDs for face_ranges
    let face_kids: Vec<u64> = face_map.keys().copied().collect();
    let total_quads = n_u * n_v;
    let quads_per_face = if face_kids.is_empty() {
        total_quads
    } else {
        total_quads / face_kids.len()
    };

    let mut quad_count = 0_usize;
    let mut current_face_idx = 0_usize;
    let mut current_start = 0u32;

    for i in 0..n_u {
        let i_next = (i + 1) % n_u;
        for j in 0..n_v {
            let j_next = (j + 1) % n_v;

            let v00 = (i * n_v + j) as u32;
            let v01 = (i * n_v + j_next) as u32;
            let v10 = (i_next * n_v + j) as u32;
            let v11 = (i_next * n_v + j_next) as u32;

            // Two triangles per quad, wound CCW when viewed from outside
            indices.push(v00);
            indices.push(v10);
            indices.push(v11);

            indices.push(v00);
            indices.push(v11);
            indices.push(v01);

            quad_count += 1;

            // Check if we should close the current face range
            if !face_kids.is_empty()
                && current_face_idx < face_kids.len() - 1
                && quad_count >= quads_per_face * (current_face_idx + 1)
            {
                let end = indices.len() as u32;
                face_ranges.push(FaceRange {
                    face_id: KernelId(face_kids[current_face_idx]),
                    start_index: current_start,
                    end_index: end,
                });
                current_start = end;
                current_face_idx += 1;
            }
        }
    }

    // Close last face range
    if !face_kids.is_empty() {
        let end = indices.len() as u32;
        face_ranges.push(FaceRange {
            face_id: KernelId(face_kids[current_face_idx]),
            start_index: current_start,
            end_index: end,
        });
    }

    Ok(RenderMesh {
        vertices,
        normals,
        indices,
        face_ranges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a minimal vertex + index buffer for dedup testing.
    /// Each vertex is [x, y, z] in f32. Returns (vertices_flat, indices).
    fn make_mesh(verts: &[[f32; 3]], tris: &[[u32; 3]]) -> (Vec<f32>, Vec<u32>) {
        let vertices: Vec<f32> = verts.iter().flat_map(|v| v.iter().copied()).collect();
        let indices: Vec<u32> = tris.iter().flat_map(|t| t.iter().copied()).collect();
        (vertices, indices)
    }

    #[test]
    fn dedup_preserves_opposite_winding() {
        // Winding-preserving dedup treats [0,1,2] and [0,2,1] as distinct keys.
        // Opposite-winding removal requires a different mechanism (e.g., position-based
        // twin pairing per boolean_vertex_welding_fix.md).
        let (vertices, mut indices) = make_mesh(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2], [0, 2, 1]],
        );
        let mut face_ranges = vec![FaceRange {
            face_id: KernelId(1),
            start_index: 0,
            end_index: 6,
        }];

        remove_duplicate_triangles(&vertices, &mut indices, &mut face_ranges);

        // Both survive — they have different winding keys
        assert_eq!(indices.len(), 6, "opposite-winding tris are distinct");
    }

    #[test]
    fn dedup_removes_same_winding_duplicate() {
        // Two identical triangles (same winding) — should dedup
        let (vertices, mut indices) = make_mesh(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2], [0, 1, 2]],
        );
        let mut face_ranges = vec![FaceRange {
            face_id: KernelId(1),
            start_index: 0,
            end_index: 6,
        }];

        remove_duplicate_triangles(&vertices, &mut indices, &mut face_ranges);

        assert_eq!(indices.len(), 3, "expected 1 triangle after dedup");
    }

    #[test]
    fn dedup_removes_same_winding_rotated() {
        // Same winding, rotated indices: [0,1,2] and [1,2,0] are the same triangle
        let (vertices, mut indices) = make_mesh(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2], [1, 2, 0]],
        );
        let mut face_ranges = vec![FaceRange {
            face_id: KernelId(1),
            start_index: 0,
            end_index: 6,
        }];

        remove_duplicate_triangles(&vertices, &mut indices, &mut face_ranges);

        assert_eq!(indices.len(), 3, "rotated same-winding duplicate removed");
    }

    #[test]
    fn dedup_preserves_distinct_triangles() {
        // Two distinct triangles sharing an edge — should NOT be deduped
        let (vertices, mut indices) = make_mesh(
            &[
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            &[[0, 1, 2], [1, 3, 2]],
        );
        let mut face_ranges = vec![FaceRange {
            face_id: KernelId(1),
            start_index: 0,
            end_index: 6,
        }];

        remove_duplicate_triangles(&vertices, &mut indices, &mut face_ranges);

        assert_eq!(indices.len(), 6, "distinct triangles should be preserved");
    }

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
        let tol = 1e-4;
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
        // Two parallel cylinders, overlapping
        let cyl_a = make_test_cylinder(&mut kernel, 0.0, 0.0, 5.0, 10.0);
        let cyl_b = make_test_cylinder(&mut kernel, 4.0, 0.0, 5.0, 10.0);
        let union_handle = kernel
            .boolean_union(&cyl_a, &cyl_b)
            .expect("cyl-cyl union should succeed");
        let mesh = kernel
            .tessellate(&union_handle, 0.1)
            .expect("tessellation should succeed");
        assert!(
            mesh.vertices.len() >= 3 * 3,
            "mesh should have at least 3 vertices, got {}",
            mesh.vertices.len() / 3
        );
        assert!(
            !is_xy_aabb_collapsed(&mesh.vertices),
            "cyl-cyl union XY coords should NOT all collapse to AABB faces ({} verts)",
            mesh.vertices.len() / 3
        );
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
    }
}
