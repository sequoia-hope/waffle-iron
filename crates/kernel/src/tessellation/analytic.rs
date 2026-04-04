//! Analytic solid tessellation — sphere, cone, and torus.
//!
//! These tessellators produce triangle meshes directly from analytic surface
//! parameters, bypassing the generic face-by-face tessellation path. They share
//! vertices at edges and corners for watertight output.

use crate::geometry::surface::SurfaceGeom;
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::FaceIdx;
use crate::types::{FaceRange, KernelError, KernelId, RenderMesh};
use crate::units::TAU_COINCIDENT;
use crate::vecmath::{v3_cross, v3_normalize};
use crate::waffle_kernel::{ConeParams, SphereParams, TorusParams};
use std::collections::BTreeMap;

use super::circle_segments;

/// Tessellate a complete sphere solid with shared vertices.
///
/// Builds an icosphere-style mesh from the octahedral B-Rep: each of the 8
/// octahedral triangles is subdivided, all vertices are projected onto the sphere,
/// and shared vertices on edges/corners are welded for watertightness.
pub(super) fn tessellate_sphere_solid(
    arena: &TopoArena,
    face_map: &BTreeMap<u64, FaceIdx>,
    sp: &SphereParams,
) -> Result<RenderMesh, KernelError> {
    let center = sp.center;
    let radius = sp.radius;
    let n = circle_segments() / 4; // subdivision level per edge

    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    // Use a position-based vertex map for sharing: round positions to avoid
    // floating-point mismatch on shared edges.
    let mut vertex_cache: BTreeMap<[i64; 3], u32> = BTreeMap::new();

    // Quantization: snap sphere-surface positions to a grid fine enough for
    // the subdivision but coarse enough for f32 fidelity.
    // Use relative precision: grid step = radius * TAU_COINCIDENT
    let quant = radius * TAU_COINCIDENT;
    let quant_inv = 1.0 / quant;

    let quantize = |pos: [f64; 3]| -> [i64; 3] {
        [
            (pos[0] * quant_inv).round() as i64,
            (pos[1] * quant_inv).round() as i64,
            (pos[2] * quant_inv).round() as i64,
        ]
    };

    // Sort face_map entries for deterministic tessellation order.
    let mut sorted_faces: Vec<(u64, FaceIdx)> = face_map.iter().map(|(&k, &v)| (k, v)).collect();
    sorted_faces.sort_by_key(|(k, _)| *k);

    // Detect if this is a cavity (inner shell) by checking the B-Rep winding
    // direction of the first face against the sphere outward normal.
    // All faces of a sphere share the same orientation, so checking one suffices.
    // Ref: [#33] Stroud — inner shell face orientation is inverted.
    let flip = if let Some(&(_, first_face)) = sorted_faces.first() {
        let loop_idx = arena.faces[first_face.0].outer_loop;
        let start_he = arena.loops[loop_idx.0].half_edge;
        let mut fv = Vec::new();
        let mut he = start_he;
        loop {
            let v = arena.half_edges[he.0].origin;
            fv.push(arena.vertices[v.0].position);
            he = arena.half_edges[he.0].next;
            if he == start_he {
                break;
            }
        }
        if fv.len() >= 3 {
            let ea = [
                fv[1][0] - fv[0][0],
                fv[1][1] - fv[0][1],
                fv[1][2] - fv[0][2],
            ];
            let eb = [
                fv[2][0] - fv[0][0],
                fv[2][1] - fv[0][1],
                fv[2][2] - fv[0][2],
            ];
            let cross = [
                ea[1] * eb[2] - ea[2] * eb[1],
                ea[2] * eb[0] - ea[0] * eb[2],
                ea[0] * eb[1] - ea[1] * eb[0],
            ];
            let centroid = [
                (fv[0][0] + fv[1][0] + fv[2][0]) / 3.0,
                (fv[0][1] + fv[1][1] + fv[2][1]) / 3.0,
                (fv[0][2] + fv[1][2] + fv[2][2]) / 3.0,
            ];
            let outward = [
                centroid[0] - center[0],
                centroid[1] - center[1],
                centroid[2] - center[2],
            ];
            cross[0] * outward[0] + cross[1] * outward[1] + cross[2] * outward[2] < 0.0
        } else {
            false
        }
    } else {
        false
    };

    let normal_sign: f64 = if flip { -1.0 } else { 1.0 };

    // Modified add_vertex to support normal flipping
    let add_vertex = |pos: [f64; 3],
                      vertex_cache: &mut BTreeMap<[i64; 3], u32>,
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

        let nx = normal_sign * dx / len;
        let ny = normal_sign * dy / len;
        let nz = normal_sign * dz / len;

        vertices.push(sx as f32);
        vertices.push(sy as f32);
        vertices.push(sz as f32);
        normals.push(nx as f32);
        normals.push(ny as f32);
        normals.push(nz as f32);

        vertex_cache.insert(key, idx);
        idx
    };

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

        // Generate triangle indices — flip winding for cavity faces
        for i in 0..n {
            let row_len = grid[i].len();
            for j in 0..(row_len - 1) {
                if flip {
                    // Reversed winding for inner shell
                    indices.push(grid[i][j]);
                    indices.push(grid[i + 1][j]);
                    indices.push(grid[i][j + 1]);
                    if j + 1 < grid[i + 1].len() {
                        indices.push(grid[i][j + 1]);
                        indices.push(grid[i + 1][j]);
                        indices.push(grid[i + 1][j + 1]);
                    }
                } else {
                    // Normal outward winding
                    indices.push(grid[i][j]);
                    indices.push(grid[i][j + 1]);
                    indices.push(grid[i + 1][j]);
                    if j + 1 < grid[i + 1].len() {
                        indices.push(grid[i][j + 1]);
                        indices.push(grid[i + 1][j + 1]);
                        indices.push(grid[i + 1][j]);
                    }
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
pub(super) fn tessellate_sphere_face(
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

    // Detect normal direction: compute B-Rep face normal from winding,
    // compare with sphere outward normal (centroid - center). If they
    // disagree, the face is an inner shell (cavity) — flip normals and
    // winding to produce inward-facing tessellation.
    // Ref: [#33] Stroud — inner shells have reversed face orientation.
    let edge_a = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let edge_b = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let cross = [
        edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1],
        edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2],
        edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0],
    ];
    let centroid = [
        (p0[0] + p1[0] + p2[0]) / 3.0,
        (p0[1] + p1[1] + p2[1]) / 3.0,
        (p0[2] + p1[2] + p2[2]) / 3.0,
    ];
    let outward = [
        centroid[0] - center[0],
        centroid[1] - center[1],
        centroid[2] - center[2],
    ];
    let dot = cross[0] * outward[0] + cross[1] * outward[1] + cross[2] * outward[2];
    let flip = dot < 0.0; // Negative dot means B-Rep winding disagrees with outward normal

    // Subdivision level: circle_segments() / 4
    let n = circle_segments() / 4;

    // Normal sign: +1 for outward-facing, -1 for inward-facing (cavity)
    let normal_sign: f64 = if flip { -1.0 } else { 1.0 };

    // Generate (n+1)*(n+2)/2 vertices on the sphere by barycentric subdivision
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

            // Normal = ±normalize(v - center), flipped for cavity faces
            let nx = normal_sign * dx / len;
            let ny = normal_sign * dy / len;
            let nz = normal_sign * dz / len;

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
            if flip {
                // Reversed winding for cavity faces (signed volume contribution negative)
                indices.push(vertex_index(i, j));
                indices.push(vertex_index(i + 1, j));
                indices.push(vertex_index(i, j + 1));

                if j + 1 < row_len - 1 {
                    indices.push(vertex_index(i, j + 1));
                    indices.push(vertex_index(i + 1, j));
                    indices.push(vertex_index(i + 1, j + 1));
                }
            } else {
                // Normal winding for outward-facing faces
                indices.push(vertex_index(i, j));
                indices.push(vertex_index(i, j + 1));
                indices.push(vertex_index(i + 1, j));

                if j + 1 < row_len - 1 {
                    indices.push(vertex_index(i, j + 1));
                    indices.push(vertex_index(i + 1, j + 1));
                    indices.push(vertex_index(i + 1, j));
                }
            }
        }
    }
}

/// Tessellate a complete cone solid with shared vertices.
///
/// Generates the cone mesh directly from ConeParams (not per-B-Rep-face),
/// ensuring all shared edges have identical vertex positions for watertightness.
/// The base is a fan from center, lateral surface is rings from apex to base.
pub(super) fn tessellate_cone_solid(
    _arena: &TopoArena,
    face_map: &BTreeMap<u64, FaceIdx>,
    face_geometry: &BTreeMap<FaceIdx, SurfaceGeom>,
    cp: &ConeParams,
) -> Result<RenderMesh, KernelError> {
    let base_center = cp.base_center;
    let apex = cp.apex;
    let axis = cp.axis;
    let radius = cp.radius;
    let height = cp.height;
    let nseg = circle_segments(); // segments around full circle

    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    // Build orthonormal basis for base circle
    let u_axis = {
        let trial = if axis[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
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
        let nrings = circle_segments() / 4; // subdivision rings from apex to base
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
pub(super) fn tessellate_torus_solid(
    face_map: &BTreeMap<u64, FaceIdx>,
    tp: &TorusParams,
) -> Result<RenderMesh, KernelError> {
    let center = tp.center;
    let axis = tp.axis;
    let big_r = tp.major_radius;
    let small_r = tp.minor_radius;

    // Resolution: use circle_segments() for major, circle_segments()/2 for minor
    let n_u = circle_segments(); // major (around the ring)
    let n_v = circle_segments() / 2; // minor (around the tube cross-section)

    // Build orthonormal basis
    let e1 = {
        let trial = if axis[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
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
