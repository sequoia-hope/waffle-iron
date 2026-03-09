//! Tessellation — converting B-Rep faces to triangle meshes.
//!
//! Handles flat (planar) face triangulation using fan decomposition,
//! and geometry-driven tessellation for cylindrical faces and circular caps.

use crate::geometry::curve::CurveGeom;
use crate::geometry::surface::SurfaceGeom;
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
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
    let cross = cross3f(v1, v2);
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
            let v = sub3(**sv, *axis_origin);
            let k_dot_v = dot3(*axis_dir, v);
            let k_cross_v = cross3(*axis_dir, v);
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
                    let radial = sub3(rotated, proj);
                    let len =
                        (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2])
                            .sqrt();
                    if len > 1e-15 {
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

    for i in 1..loop_verts.len() as u32 - 1 {
        indices.push(base_vertex);
        indices.push(base_vertex + i);
        indices.push(base_vertex + i + 1);
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

    let ab = sub3(loop_verts[1], loop_verts[0]);
    let ac = sub3(loop_verts[2], loop_verts[0]);
    let n = cross3(ab, ac);
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let normal = if len < 1e-15 {
        [0.0f32, 0.0, 1.0]
    } else {
        [
            (n[0] / len) as f32,
            (n[1] / len) as f32,
            (n[2] / len) as f32,
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

    for i in 1..loop_verts.len() as u32 - 1 {
        indices.push(base_vertex);
        indices.push(base_vertex + i);
        indices.push(base_vertex + i + 1);
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
                let radial = sub3(start_pt, center);
                let len =
                    (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
                let x_axis = [radial[0] / len, radial[1] / len, radial[2] / len];
                let y_axis = cross3(normal, x_axis);

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
    let center = [cyl.origin.x, cyl.origin.y];
    // Negative radius signals inward-facing normals (hole surface)
    let inward = cyl.radius < 0.0;
    let r = cyl.radius.abs();
    let axis = [cyl.axis.x, cyl.axis.y, cyl.axis.z];

    // Walk face boundary to find Z range and angular range
    let loop_idx = arena.faces[face_idx.0].outer_loop;
    let start_he = arena.loops[loop_idx.0].half_edge;

    let mut z_min = f64::INFINITY;
    let mut z_max = f64::NEG_INFINITY;
    let mut angle_start: Option<f64> = None;
    let mut total_sweep = 0.0_f64;

    // Check if this is a full cylinder (self-loop with seam) or partial patch
    let mut he = start_he;
    let mut has_circular_edge = false;
    let mut has_arc_edge = false;
    let mut first_arc: Option<(f64, f64)> = None; // (start_angle, sweep_angle) from Arc geometry
    loop {
        let v = arena.half_edges[he.0].origin;
        let pos = arena.vertices[v.0].position;
        z_min = z_min.min(pos[2]);
        z_max = z_max.max(pos[2]);

        let edge = arena.half_edges[he.0].edge;
        if let Some(CurveGeom::Arc(ref arc)) = edge_geometry.get(&edge) {
            if first_arc.is_none() {
                // Use the arc's own start_angle (from start_point) and sweep
                let arc_start =
                    (arc.start_point.y - center[1]).atan2(arc.start_point.x - center[0]);
                first_arc = Some((arc_start, arc.sweep_angle.abs()));
            }
            total_sweep += arc.sweep_angle.abs();
            let a = (pos[1] - center[1]).atan2(pos[0] - center[0]);
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

    // Full cylinder: has Circular edges (not Arc), or total sweep close to 2π
    // Partial patch from cyl-cyl booleans: has Arc edges, use single arc's sweep
    let is_full = has_circular_edge || (total_sweep > std::f64::consts::TAU - 0.1 && !has_arc_edge);

    if is_full || angle_start.is_none() {
        // Full cylinder: tessellate like tessellate_cylindrical_face
        let (cx_axis, cy_axis) = make_circle_axes(&axis);
        let n = CIRCLE_SEGMENTS;
        let base_vertex = vertices.len() as u32 / 3;

        let normal_sign = if inward { -1.0_f64 } else { 1.0_f64 };

        for row in 0..2 {
            let z = if row == 0 { z_min } else { z_max };
            for i in 0..n {
                let theta = std::f64::consts::TAU * (i as f64) / (n as f64);
                let cos_t = theta.cos();
                let sin_t = theta.sin();
                let px = center[0] + r * (cos_t * cx_axis[0] + sin_t * cy_axis[0]);
                let py = center[1] + r * (cos_t * cx_axis[1] + sin_t * cy_axis[1]);
                vertices.push(px as f32);
                vertices.push(py as f32);
                vertices.push(z as f32);
                let nx = normal_sign * (cos_t * cx_axis[0] + sin_t * cy_axis[0]);
                let ny = normal_sign * (cos_t * cx_axis[1] + sin_t * cy_axis[1]);
                let nz = 0.0;
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
                // Reverse winding for inward-facing normals
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
        // Partial cylinder patch: use angular range from the arc edge geometry
        // (top and bottom arcs have the same range, so use the first one's start_angle + sweep)
        let (a_start, sweep) =
            first_arc.unwrap_or((angle_start.unwrap_or(0.0), std::f64::consts::TAU));
        let normal_sign = if inward { -1.0_f64 } else { 1.0_f64 };

        let n = ((CIRCLE_SEGMENTS as f64) * sweep / std::f64::consts::TAU)
            .ceil()
            .max(4.0) as usize;
        let base_vertex = vertices.len() as u32 / 3;

        for row in 0..2 {
            let z = if row == 0 { z_min } else { z_max };
            for i in 0..=n {
                let theta = a_start + sweep * (i as f64) / (n as f64);
                let cos_t = theta.cos();
                let sin_t = theta.sin();
                let px = center[0] + r * cos_t;
                let py = center[1] + r * sin_t;
                vertices.push(px as f32);
                vertices.push(py as f32);
                vertices.push(z as f32);
                normals.push((normal_sign * cos_t) as f32);
                normals.push((normal_sign * sin_t) as f32);
                normals.push(0.0);
            }
        }

        let m = (n + 1) as u32;
        for i in 0..n as u32 {
            let bot = base_vertex + i;
            let bot_next = base_vertex + i + 1;
            let top = base_vertex + m + i;
            let top_next = base_vertex + m + i + 1;
            if inward {
                // Reverse winding for inward-facing normals
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
    let outer_verts = collect_loop_verts(arena, outer_loop_idx);

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

    // Check winding of first triangle against face normal and reverse if needed
    let reverse = if let Some(&(a, b, c)) = tri_buf.first() {
        let get_pos = |idx: u32| -> [f64; 3] {
            let i = idx as usize;
            if i < n_outer {
                outer_verts[i]
            } else {
                inner_verts[i - n_outer]
            }
        };
        let pa = get_pos(a - base);
        let pb = get_pos(b - base);
        let pc = get_pos(c - base);
        let ab = sub3(pb, pa);
        let ac = sub3(pc, pa);
        let cr = cross3(ab, ac);
        let dot = cr[0] * plane.normal.x + cr[1] * plane.normal.y + cr[2] * plane.normal.z;
        dot < 0.0
    } else {
        false
    };

    for (a, b, c) in tri_buf {
        if reverse {
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

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn cross3f(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    cross3(a, b)
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Derive orthogonal x/y axes from a normal vector for circle tessellation.
fn make_circle_axes(normal: &[f64; 3]) -> ([f64; 3], [f64; 3]) {
    let n = *normal;
    // Pick a vector not parallel to normal
    let up = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let x = cross3(n, up);
    let len = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
    let x_norm = [x[0] / len, x[1] / len, x[2] / len];
    let y = cross3(n, x_norm);
    (x_norm, y)
}
