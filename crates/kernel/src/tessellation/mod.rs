//! Tessellation — converting B-Rep faces to triangle meshes.
//!
//! Handles flat (planar) face triangulation using fan decomposition.
//! Curved face tessellation (adaptive subdivision) comes later.

use crate::geometry::surface::SurfaceGeom;
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::*;
use crate::types::*;
use std::collections::HashMap;

/// Tessellate all faces in the arena using fan triangulation (valid for convex planar faces).
///
/// Each face gets its own set of vertices (not shared across faces) so normals
/// are per-face. For a quad: 4 vertices, 2 triangles, 6 indices.
pub fn tessellate_flat_faces(
    arena: &TopoArena,
    face_map: &HashMap<u64, FaceIdx>,
    face_geometry: &HashMap<FaceIdx, SurfaceGeom>,
) -> Result<RenderMesh, KernelError> {
    let mut vertices: Vec<f32> = Vec::new();
    let mut normals: Vec<f32> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut face_ranges: Vec<FaceRange> = Vec::new();

    for (&kid, &face_idx) in face_map {
        let loop_idx = arena.faces[face_idx.0].outer_loop;
        let start_he = arena.loops[loop_idx.0].half_edge;

        // Collect ordered vertex positions around the face loop
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
            continue;
        }

        // Compute face normal from geometry, or from cross product
        let normal = match face_geometry.get(&face_idx) {
            Some(SurfaceGeom::Planar(plane)) => [
                plane.normal.x as f32,
                plane.normal.y as f32,
                plane.normal.z as f32,
            ],
            _ => {
                // Fallback: compute from first triangle
                let ab = sub3(loop_verts[1], loop_verts[0]);
                let ac = sub3(loop_verts[2], loop_verts[0]);
                let n = cross3(ab, ac);
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                if len < 1e-15 {
                    [0.0f32, 0.0, 1.0]
                } else {
                    [
                        (n[0] / len) as f32,
                        (n[1] / len) as f32,
                        (n[2] / len) as f32,
                    ]
                }
            }
        };

        let base_vertex = vertices.len() as u32 / 3;
        let start_index = indices.len() as u32;

        // Emit per-face vertices (not shared across faces)
        for v in &loop_verts {
            vertices.push(v[0] as f32);
            vertices.push(v[1] as f32);
            vertices.push(v[2] as f32);
            normals.push(normal[0]);
            normals.push(normal[1]);
            normals.push(normal[2]);
        }

        // Fan triangulation: v0, v1, v2; v0, v2, v3; ...
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

    Ok(RenderMesh {
        vertices,
        normals,
        indices,
        face_ranges,
    })
}

/// Extract edge line segments for rendering edge overlays.
pub fn extract_edges(
    arena: &TopoArena,
    edge_map: &HashMap<u64, EdgeIdx>,
) -> Result<EdgeRenderData, KernelError> {
    let mut vertices: Vec<f32> = Vec::new();
    let mut edge_ranges: Vec<EdgeRange> = Vec::new();

    for (&kid, &edge_idx) in edge_map {
        let he_a = arena.edges[edge_idx.0].half_edge;
        let he_b = arena.half_edges[he_a.0].twin;
        let p0 = arena.vertices[arena.half_edges[he_a.0].origin.0].position;
        let p1 = arena.vertices[arena.half_edges[he_b.0].origin.0].position;

        let start_vertex = vertices.len() as u32 / 3;

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
