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
use crate::units::TAU_NORMALIZE;
use crate::vecmath::{compute_plane_basis, v3_cross, v3_dot, v3_length, v3_sub};
use crate::waffle_kernel::{CylinderParams, RevolveParams};
use std::collections::HashMap;

/// Number of segments for circular/cylindrical tessellation.
const CIRCLE_SEGMENTS: usize = 64;

/// Tessellate all faces in a solid, dispatching per-face based on geometry type.
///
/// For polygon (box) solids: uses fan triangulation (same as before).
/// For cylinder solids: uses geometry-driven circular cap + cylindrical side tessellation.
pub(crate) fn tessellate_solid(
    arena: &TopoArena,
    face_map: &HashMap<u64, FaceIdx>,
    face_geometry: &HashMap<FaceIdx, SurfaceGeom>,
    _edge_geometry: &HashMap<EdgeIdx, CurveGeom>,
    cylinder_params: Option<&CylinderParams>,
    revolve_params: Option<&RevolveParams>,
) -> Result<RenderMesh, KernelError> {
    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    for (&kid, &face_idx) in face_map {
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
/// Generates a grid of (N+1) x 2 vertices by rotating two profile edge endpoints
/// through the revolution angle in N steps, producing 2N triangles.
#[allow(clippy::too_many_arguments)]
fn tessellate_revolve_lateral(
    start_v0: &[f64; 3],
    start_v1: &[f64; 3],
    axis_origin: &[f64; 3],
    axis_dir: &[f64; 3],
    angle_rad: f64,
    geom: Option<&SurfaceGeom>,
    vertices: &mut Vec<f32>,
    normals: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    let n = CIRCLE_SEGMENTS;
    let base_vertex = vertices.len() as u32 / 3;

    // Generate (N+1) x 2 vertex grid
    for i in 0..=n {
        let theta = angle_rad * (i as f64) / (n as f64);
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        // Rotate both vertices around axis using Rodrigues
        for sv in &[start_v0, start_v1] {
            let v = v3_sub(**sv, *axis_origin);
            let k_dot_v = v3_dot(*axis_dir, v);
            let k_cross_v = v3_cross(*axis_dir, v);
            let rotated = [
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
            vertices.push(rotated[0] as f32);
            vertices.push(rotated[1] as f32);
            vertices.push(rotated[2] as f32);

            // Compute normal based on face geometry type
            match geom {
                Some(SurfaceGeom::Cylindrical(_)) => {
                    // Radially outward normal
                    let proj = [
                        axis_origin[0] + axis_dir[0] * k_dot_v,
                        axis_origin[1] + axis_dir[1] * k_dot_v,
                        axis_origin[2] + axis_dir[2] * k_dot_v,
                    ];
                    let radial = v3_sub(rotated, proj);
                    let len =
                        (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2])
                            .sqrt();
                    if len > TAU_NORMALIZE {
                        normals.push((radial[0] / len) as f32);
                        normals.push((radial[1] / len) as f32);
                        normals.push((radial[2] / len) as f32);
                    } else {
                        normals.push(0.0);
                        normals.push(0.0);
                        normals.push(1.0);
                    }
                }
                Some(SurfaceGeom::Planar(plane)) => {
                    normals.push(plane.normal.x as f32);
                    normals.push(plane.normal.y as f32);
                    normals.push(plane.normal.z as f32);
                }
                _ => {
                    normals.push(0.0);
                    normals.push(0.0);
                    normals.push(1.0);
                }
            }
        }
    }

    // Generate quads: each step i produces a quad from vertices [i*2, i*2+1, (i+1)*2, (i+1)*2+1]
    for i in 0..n as u32 {
        let v00 = base_vertex + i * 2;
        let v01 = base_vertex + i * 2 + 1;
        let v10 = base_vertex + (i + 1) * 2;
        let v11 = base_vertex + (i + 1) * 2 + 1;

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
            let n_verts = (n + 1) * 2;
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
                v3_dot(cr, cr) > 1e-20
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
        if v3_length(tri_normal) > 1e-12 {
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
        let should_flip = if v3_length(tri_normal) > 1e-12 {
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
                v3_dot(cr, cr) > 1e-20
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
        if v3_length(tri_normal) > 1e-12 {
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
        let should_flip = if v3_length(tri_normal) > 1e-12 {
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

        for row in 0..2 {
            let t = if row == 0 { t_min } else { t_max };
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
        for i in 0..n32 {
            let next = (i + 1) % n32;
            let bot = base_vertex + i;
            let bot_next = base_vertex + next;
            let top = base_vertex + n32 + i;
            let top_next = base_vertex + n32 + next;
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

        for row in 0..2 {
            let t = if row == 0 { t_min } else { t_max };
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
        for i in 0..n as u32 {
            let bot = base_vertex + i;
            let bot_next = base_vertex + i + 1;
            let top = base_vertex + m + i;
            let top_next = base_vertex + m + i + 1;
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
        if cr_len > 1e-12 {
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
        let should_reverse = if cr_len > 1e-12 {
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
                let origin_is_start = dist_sq_3d(&origin_pos, &arc_start_pt) < 1e-6;
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
        if geo_len < 1e-12 {
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
            if area >= 1e-12_f32 {
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

    // Use 1e-5 relative to diagonal, clamped for safety.
    // f32 has ~7 digits precision, so 1e-5 relative is well above noise floor.
    let tau = (diag * 1e-5).max(1e-8).min(1e-2);
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
