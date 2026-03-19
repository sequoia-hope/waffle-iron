use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A lightweight triangle mesh for preview/thumbnail rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewMesh {
    pub vertices: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
}

/// Decimate a triangle mesh using vertex clustering.
///
/// If the input triangle count is already <= `max_tris`, the input is returned as-is.
/// Otherwise, vertices are clustered into a uniform grid (AABB / 32 cells per axis),
/// averaged within each cell, and degenerate triangles are removed.
pub fn decimate_mesh(
    vertices: &[f32],
    normals: &[f32],
    indices: &[u32],
    max_tris: u32,
) -> PreviewMesh {
    let num_tris = indices.len() / 3;

    // Pass through if already small enough or empty
    if num_tris as u32 <= max_tris || vertices.is_empty() || indices.is_empty() {
        return PreviewMesh {
            vertices: vertices.to_vec(),
            normals: normals.to_vec(),
            indices: indices.to_vec(),
        };
    }

    let num_verts = vertices.len() / 3;

    // Compute AABB
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for i in 0..num_verts {
        for axis in 0..3 {
            let v = vertices[i * 3 + axis];
            if v < min[axis] {
                min[axis] = v;
            }
            if v > max[axis] {
                max[axis] = v;
            }
        }
    }

    let diagonal =
        ((max[0] - min[0]).powi(2) + (max[1] - min[1]).powi(2) + (max[2] - min[2]).powi(2)).sqrt();

    // Avoid division by zero for degenerate meshes
    if diagonal < f32::EPSILON {
        return PreviewMesh {
            vertices: vertices.to_vec(),
            normals: normals.to_vec(),
            indices: indices.to_vec(),
        };
    }

    let cell_size = diagonal / 32.0;
    let inv_cell = 1.0 / cell_size;

    // Map each vertex to a grid cell
    let mut vertex_to_cell: Vec<(i32, i32, i32)> = Vec::with_capacity(num_verts);
    for i in 0..num_verts {
        let ci = ((vertices[i * 3] - min[0]) * inv_cell) as i32;
        let cj = ((vertices[i * 3 + 1] - min[1]) * inv_cell) as i32;
        let ck = ((vertices[i * 3 + 2] - min[2]) * inv_cell) as i32;
        vertex_to_cell.push((ci, cj, ck));
    }

    // Accumulate positions and normals per cell
    struct CellAccum {
        pos_sum: [f64; 3],
        nrm_sum: [f64; 3],
        count: u32,
        new_index: u32,
    }

    let mut cells: HashMap<(i32, i32, i32), CellAccum> = HashMap::new();
    for i in 0..num_verts {
        let key = vertex_to_cell[i];
        let entry = cells.entry(key).or_insert(CellAccum {
            pos_sum: [0.0; 3],
            nrm_sum: [0.0; 3],
            count: 0,
            new_index: 0,
        });
        for axis in 0..3 {
            entry.pos_sum[axis] += vertices[i * 3 + axis] as f64;
            if i * 3 + axis < normals.len() {
                entry.nrm_sum[axis] += normals[i * 3 + axis] as f64;
            }
        }
        entry.count += 1;
    }

    // Assign new indices and build output vertices/normals
    let mut out_vertices: Vec<f32> = Vec::with_capacity(cells.len() * 3);
    let mut out_normals: Vec<f32> = Vec::with_capacity(cells.len() * 3);
    for (next_index, accum) in cells.values_mut().enumerate() {
        accum.new_index = next_index as u32;
        let c = accum.count as f64;
        for axis in 0..3 {
            out_vertices.push((accum.pos_sum[axis] / c) as f32);
        }
        // Normalize the averaged normal
        let nx = accum.nrm_sum[0] / c;
        let ny = accum.nrm_sum[1] / c;
        let nz = accum.nrm_sum[2] / c;
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len > 1e-12 {
            out_normals.push((nx / len) as f32);
            out_normals.push((ny / len) as f32);
            out_normals.push((nz / len) as f32);
        } else {
            out_normals.push(0.0);
            out_normals.push(0.0);
            out_normals.push(1.0);
        }
    }

    // Build vertex merge map: old index -> new index
    let merge_map: Vec<u32> = vertex_to_cell
        .iter()
        .map(|key| cells[key].new_index)
        .collect();

    // Remap triangles, removing degenerates
    let mut out_indices: Vec<u32> = Vec::with_capacity(indices.len());
    for tri in indices.chunks(3) {
        if tri.len() < 3 {
            break;
        }
        let a = merge_map[tri[0] as usize];
        let b = merge_map[tri[1] as usize];
        let c = merge_map[tri[2] as usize];
        // Skip degenerate triangles
        if a == b || b == c || a == c {
            continue;
        }
        out_indices.push(a);
        out_indices.push(b);
        out_indices.push(c);
    }

    PreviewMesh {
        vertices: out_vertices,
        normals: out_normals,
        indices: out_indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_mesh_passes_through() {
        // A single triangle — well under any reasonable max_tris
        let vertices = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let normals = vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        let indices = vec![0, 1, 2];

        let result = decimate_mesh(&vertices, &normals, &indices, 10);
        assert_eq!(result.vertices, vertices);
        assert_eq!(result.normals, normals);
        assert_eq!(result.indices, indices);
    }

    #[test]
    fn empty_input_returns_empty() {
        let result = decimate_mesh(&[], &[], &[], 10);
        assert!(result.vertices.is_empty());
        assert!(result.normals.is_empty());
        assert!(result.indices.is_empty());
    }

    #[test]
    fn larger_mesh_gets_reduced() {
        // Create a dense grid where vertex spacing << cell size, causing merges.
        // 50x50 quads = 5000 triangles. Extent ~0.05, diagonal ~0.07,
        // cell_size ~0.002. Step 0.001 means ~2 grid positions per cell,
        // so many triangle vertices merge → degenerate triangles removed.
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();

        let grid = 50;
        let step = 0.001_f32;
        let mut vi = 0u32;
        for i in 0..grid {
            for j in 0..grid {
                let x = i as f32 * step;
                let z = j as f32 * step;
                for &(dx, dz) in &[(0.0, 0.0), (step, 0.0), (step, step), (0.0, step)] {
                    vertices.extend_from_slice(&[x + dx, 0.0, z + dz]);
                    normals.extend_from_slice(&[0.0, 1.0, 0.0]);
                }
                indices.extend_from_slice(&[vi, vi + 1, vi + 2, vi, vi + 2, vi + 3]);
                vi += 4;
            }
        }

        let original_tris = indices.len() / 3;
        assert_eq!(original_tris, 5000);

        // Decimate to max 100 triangles
        let result = decimate_mesh(&vertices, &normals, &indices, 100);
        let result_tris = result.indices.len() / 3;

        // Should have fewer triangles than original
        assert!(
            result_tris < original_tris,
            "Expected fewer triangles after decimation: got {} from {}",
            result_tris,
            original_tris
        );
        // Should have fewer vertices
        assert!(result.vertices.len() < vertices.len());
    }

    #[test]
    fn degenerate_triangles_removed() {
        // Create vertices where some will merge into the same cell
        // Two vertices very close together (same cell), one far away
        let vertices = vec![
            0.0, 0.0, 0.0, // v0
            0.001, 0.001, 0.001, // v1 — same cell as v0
            1.0, 0.0, 0.0, // v2 — different cell
            0.002, 0.0, 0.0, // v3 — same cell as v0
            1.0, 1.0, 0.0, // v4 — different cell
        ];
        let normals = vec![
            0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
        ];
        // Triangle (v0, v1, v2): v0 and v1 merge → degenerate
        // Triangle (v0, v2, v4): all in different cells → survives
        let indices = vec![0, 1, 2, 0, 2, 4];

        let result = decimate_mesh(&vertices, &normals, &indices, 1);
        // The first triangle should be removed (degenerate after merge)
        // The second should survive
        // At most 1 triangle should remain
        let result_tris = result.indices.len() / 3;
        assert!(
            result_tris <= 1,
            "Expected at most 1 non-degenerate triangle, got {}",
            result_tris
        );
        // All indices should reference valid vertices
        let num_out_verts = result.vertices.len() / 3;
        for &idx in &result.indices {
            assert!((idx as usize) < num_out_verts);
        }
    }

    #[test]
    fn passthrough_at_exact_limit() {
        // Exactly max_tris triangles should pass through
        let vertices = vec![
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
            1.0,
        ];
        let normals = vec![
            0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0,
            1.0,
        ];
        let indices = vec![0, 1, 2, 3, 4, 5];

        let result = decimate_mesh(&vertices, &normals, &indices, 2);
        assert_eq!(result.indices, indices);
    }
}
