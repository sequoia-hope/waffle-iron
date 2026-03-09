//! Tessellation — converting B-Rep faces to triangle meshes.
//!
//! Handles flat (planar) face triangulation using fan decomposition,
//! and geometry-driven tessellation for cylindrical faces and circular caps.

use crate::geometry::curve::CurveGeom;
use crate::geometry::surface::SurfaceGeom;
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
use crate::waffle_kernel::CylinderParams;
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
) -> Result<RenderMesh, KernelError> {
    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    for (&kid, &face_idx) in face_map {
        let geom = face_geometry.get(&face_idx);

        match geom {
            Some(SurfaceGeom::Cylindrical(cyl)) => {
                // Cylindrical side face — use parametric tessellation
                let cp = cylinder_params.ok_or_else(|| KernelError::Other {
                    message: "Cylindrical face without cylinder params".to_string(),
                })?;
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
            }
            Some(SurfaceGeom::Planar(plane)) => {
                // Check if this is a circular cap (self-loop face in cylinder)
                if let Some(cp) = cylinder_params {
                    let loop_idx = arena.faces[face_idx.0].outer_loop;
                    let start_he = arena.loops[loop_idx.0].half_edge;
                    let is_self_loop = arena.half_edges[start_he.0].next == start_he;

                    if is_self_loop {
                        // Circular cap — use fan triangulation with parametric vertices
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
