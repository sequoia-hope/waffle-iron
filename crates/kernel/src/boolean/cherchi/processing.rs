// MIT License
//
// Copyright (c) 2022 G. Cherchi, M. Livesu, R. Scateni, M. Attene and F. Pellacini
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Preprocessing utilities for Cherchi mesh arrangement.
//!
//! Spatial dedup of vertices, degenerate triangle removal, coordinate scaling.
//!
//! Ported from Cherchi processing.h + processing.cpp
//! MIT License (c) 2022 Cherchi, Livesu, Scateni, Attene, Pellacini

use std::collections::HashMap;

/// Compute the multiplier (power-of-2 scaling factor) for predicate stability.
///
/// Scales coordinates so the max absolute coordinate is near
/// `R = 11259470696.0` (avg_max_coord * old_multiplier), then rounds to the
/// nearest power of 2.
///
/// Ported from processing.cpp:47-64
#[allow(dead_code)]
pub(crate) fn compute_multiplier(coords: &[[f64; 3]]) -> f64 {
    const R: f64 = 11_259_470_696.0; // avg_max_coord (167.78) * old_multiplier (67108864.0)

    let mut abs_max: f64 = 0.0;
    for c in coords {
        for &v in c {
            let a = v.abs();
            if a > abs_max {
                abs_max = a;
            }
        }
    }

    if abs_max == 0.0 {
        return 1.0;
    }

    let div = R / abs_max;

    // Closest power of 2
    let e = div.log2().round() as i32;
    let multiplier = if e >= 0 {
        (1u64 << e.min(62)) as f64
    } else {
        1.0 / ((1u64 << (-e).min(62)) as f64)
    };

    if multiplier < 0.0 {
        1.0 // temporary fix, matching C++
    } else {
        multiplier
    }
}

/// Compute the multiplier from flat coordinate slice `[x0, y0, z0, x1, y1, z1, ...]`.
///
/// Convenience wrapper matching the C++ signature that takes `vector<double>`.
///
/// Ported from processing.cpp:47-64
#[allow(dead_code)]
pub(crate) fn compute_multiplier_flat(coords: &[f64]) -> f64 {
    const R: f64 = 11_259_470_696.0;

    if coords.is_empty() {
        return 1.0;
    }

    let max_coord = coords.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min_coord = coords.iter().copied().fold(f64::INFINITY, f64::min);

    let abs_max = max_coord.abs().max(min_coord.abs());

    if abs_max == 0.0 {
        return 1.0;
    }

    let div = R / abs_max;

    let e = div.log2().round() as i32;
    let multiplier = if e >= 0 {
        (1u64 << e.min(62)) as f64
    } else {
        1.0 / ((1u64 << (-e).min(62)) as f64)
    };

    if multiplier < 0.0 {
        1.0
    } else {
        multiplier
    }
}

/// Merge duplicated vertices in the input mesh.
///
/// Takes flat coordinates and triangle indices, produces deduplicated vertex
/// list and remapped triangle indices.
///
/// Ported from processing.cpp:68-120 (sequential path)
#[allow(dead_code)]
pub(crate) fn merge_duplicated_vertices(
    in_coords: &[[f64; 3]],
    in_tris: &[usize],
) -> (Vec<[f64; 3]>, Vec<usize>) {
    let mut verts: Vec<[f64; 3]> = Vec::with_capacity(in_coords.len());
    let mut tris: Vec<usize> = Vec::with_capacity(in_tris.len());

    // Use a HashMap keyed on bitwise-exact f64 triples for dedup.
    // We convert [f64; 3] to [u64; 3] via to_bits() for exact hashing.
    let mut v_map: HashMap<[u64; 3], usize> = HashMap::with_capacity(in_tris.len() * 3);

    for &v_id in in_tris {
        let v = in_coords[v_id];
        let key = [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()];

        let next_id = verts.len();
        let entry = v_map.entry(key).or_insert_with(|| {
            verts.push(v);
            next_id
        });
        tris.push(*entry);
    }

    (verts, tris)
}

/// Merge duplicated vertices from flat coordinate data.
///
/// Takes `[x0, y0, z0, x1, ...]` and triangle indices, returns deduplicated
/// `Vec<[f64; 3]>` and remapped triangle indices.
///
/// Ported from processing.cpp:68-120 (sequential path, flat coords variant)
#[allow(dead_code)]
pub(crate) fn merge_duplicated_vertices_flat(
    in_coords: &[f64],
    in_tris: &[usize],
) -> (Vec<[f64; 3]>, Vec<usize>) {
    let mut verts: Vec<[f64; 3]> = Vec::with_capacity(in_coords.len() / 3);
    let mut tris: Vec<usize> = Vec::with_capacity(in_tris.len());

    let mut v_map: HashMap<[u64; 3], usize> = HashMap::with_capacity(in_tris.len() * 3);

    for &v_id in in_tris {
        let v = [
            in_coords[3 * v_id],
            in_coords[3 * v_id + 1],
            in_coords[3 * v_id + 2],
        ];
        let key = [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()];

        let next_id = verts.len();
        let entry = v_map.entry(key).or_insert_with(|| {
            verts.push(v);
            next_id
        });
        tris.push(*entry);
    }

    (verts, tris)
}

/// Remove degenerate (collinear) and duplicated triangles from the mesh.
///
/// Degenerate triangles have collinear vertices. Duplicated triangles (same sorted
/// vertex triple) get their labels merged via bitwise OR.
///
/// Returns the filtered (tris, labels).
///
/// Ported from processing.cpp:125-173
#[allow(dead_code)]
pub(crate) fn remove_degenerate_and_duplicated_triangles(
    verts: &[[f64; 3]],
    in_tris: &[usize],
    in_labels: &[u32],
) -> (Vec<usize>, Vec<u32>) {
    let num_orig_tris = in_tris.len() / 3;

    let mut tris = Vec::with_capacity(in_tris.len());
    let mut labels = Vec::with_capacity(num_orig_tris);

    // Map from sorted vertex triple → index in output labels
    let mut tris_map: HashMap<[usize; 3], usize> = HashMap::with_capacity(num_orig_tris);

    for t_id in 0..num_orig_tris {
        let v0_id = in_tris[3 * t_id];
        let v1_id = in_tris[3 * t_id + 1];
        let v2_id = in_tris[3 * t_id + 2];
        let l = in_labels[t_id];

        // Check for degenerate (collinear) triangle
        if points_are_collinear_3d(&verts[v0_id], &verts[v1_id], &verts[v2_id]) {
            continue;
        }

        // Sorted triple for dedup
        let mut tri_key = [v0_id, v1_id, v2_id];
        tri_key.sort();

        match tris_map.entry(tri_key) {
            std::collections::hash_map::Entry::Vacant(e) => {
                let label_idx = labels.len();
                e.insert(label_idx);
                labels.push(l);
                tris.push(v0_id);
                tris.push(v1_id);
                tris.push(v2_id);
            }
            std::collections::hash_map::Entry::Occupied(e) => {
                // Merge labels for duplicate triangle
                let pos = *e.get();
                labels[pos] |= l;
            }
        }
    }

    (tris, labels)
}

/// Compute approximate coordinates from the vertex list, dividing by the multiplier.
///
/// Materializes each ImplicitPoint and divides by the multiplier.
/// The last 5 vertices are jolly points and are excluded from output.
///
/// Ported from processing.cpp:186-210
#[allow(dead_code)]
pub(crate) fn compute_approximate_coordinates(
    vertices: &[crate::boolean::indirect_predicates::ImplicitPoint],
    multiplier: f64,
) -> Vec<[f64; 3]> {
    if multiplier == 0.0 {
        let mut out = Vec::with_capacity(vertices.len());
        for v in vertices {
            out.push(v.materialize().unwrap_or([0.0, 0.0, 0.0]));
        }
        return out;
    }

    // Exclude last 5 jolly points
    let n = if vertices.len() >= 5 {
        vertices.len() - 5
    } else {
        vertices.len()
    };

    let mut out = Vec::with_capacity(n);
    for v in &vertices[..n] {
        let coords = v.materialize().unwrap_or([0.0, 0.0, 0.0]);
        out.push([
            coords[0] / multiplier,
            coords[1] / multiplier,
            coords[2] / multiplier,
        ]);
    }
    out
}

/// Check if three 3D points are collinear (degenerate triangle test).
///
/// Uses cross-product magnitude. Replaces `cinolib::points_are_colinear_3d`.
///
/// Ported from processing.cpp:144-146 (cinolib dependency replaced)
fn points_are_collinear_3d(a: &[f64; 3], b: &[f64; 3], c: &[f64; 3]) -> bool {
    // Cross product of (b - a) × (c - a)
    let ux = b[0] - a[0];
    let uy = b[1] - a[1];
    let uz = b[2] - a[2];
    let vx = c[0] - a[0];
    let vy = c[1] - a[1];
    let vz = c[2] - a[2];

    let cx = uy * vz - uz * vy;
    let cy = uz * vx - ux * vz;
    let cz = ux * vy - uy * vx;

    // If cross product is zero, points are collinear
    cx == 0.0 && cy == 0.0 && cz == 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_multiplier() {
        // Coordinates around magnitude 1.0
        let coords = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let m = compute_multiplier(&coords);
        // Should be a power of 2 near R/1.0 = 11259470696
        assert!(m > 0.0);
        // Check it's a power of 2
        assert!((m.log2() - m.log2().round()).abs() < 1e-10);
    }

    #[test]
    fn test_compute_multiplier_flat() {
        let coords = vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let m = compute_multiplier_flat(&coords);
        assert!(m > 0.0);
        assert!((m.log2() - m.log2().round()).abs() < 1e-10);
    }

    #[test]
    fn test_compute_multiplier_zero() {
        let coords: Vec<[f64; 3]> = vec![[0.0, 0.0, 0.0]];
        assert_eq!(compute_multiplier(&coords), 1.0);
    }

    #[test]
    fn test_processing_dedup() {
        // 4 coords, but v0 and v3 are at the same position
        let coords = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0], // duplicate of v0
        ];
        // tri0: v0,v1,v2   tri1: v3,v1,v2 (v3 == v0 in position)
        let tris = vec![0, 1, 2, 3, 1, 2];
        let (verts, new_tris) = merge_duplicated_vertices(&coords, &tris);

        // Should have 3 unique vertices
        assert_eq!(verts.len(), 3);
        // tri0 and tri1 should map v0 and v3 to the same ID
        assert_eq!(new_tris[0], new_tris[3]); // v0 and v3 merged
    }

    #[test]
    fn test_processing_dedup_flat() {
        let coords = vec![
            0.0, 0.0, 0.0, // v0
            1.0, 0.0, 0.0, // v1
            0.0, 1.0, 0.0, // v2
            0.0, 0.0, 0.0, // v3 = v0
        ];
        let tris = vec![0, 1, 2, 3, 1, 2];
        let (verts, new_tris) = merge_duplicated_vertices_flat(&coords, &tris);
        assert_eq!(verts.len(), 3);
        assert_eq!(new_tris[0], new_tris[3]);
    }

    #[test]
    fn test_remove_degenerate() {
        let verts = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, 0.0, 0.0], // collinear with v0,v1
        ];
        // tri0: good triangle  tri1: degenerate (v0,v1,v3 are collinear)
        let tris = vec![0, 1, 2, 0, 1, 3];
        let labels = vec![1, 2];
        let (new_tris, new_labels) =
            remove_degenerate_and_duplicated_triangles(&verts, &tris, &labels);

        // Only tri0 survives
        assert_eq!(new_tris.len(), 3);
        assert_eq!(new_labels.len(), 1);
        assert_eq!(new_labels[0], 1);
    }

    #[test]
    fn test_remove_duplicate_triangles() {
        let verts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // Same triangle twice with different labels
        let tris = vec![0, 1, 2, 0, 2, 1]; // reversed winding but same sorted triple
        let labels = vec![1, 2];
        let (new_tris, new_labels) =
            remove_degenerate_and_duplicated_triangles(&verts, &tris, &labels);

        // One triangle with merged label (1 | 2 = 3)
        assert_eq!(new_tris.len(), 3);
        assert_eq!(new_labels.len(), 1);
        assert_eq!(new_labels[0], 3);
    }

    #[test]
    fn test_compute_approximate_coordinates() {
        use crate::boolean::indirect_predicates::ImplicitPoint;
        let verts = vec![
            ImplicitPoint::Explicit([2.0, 4.0, 6.0]),
            ImplicitPoint::Explicit([8.0, 10.0, 12.0]),
            // 5 jolly points
            ImplicitPoint::Explicit([0.0; 3]),
            ImplicitPoint::Explicit([0.0; 3]),
            ImplicitPoint::Explicit([0.0; 3]),
            ImplicitPoint::Explicit([0.0; 3]),
            ImplicitPoint::Explicit([0.0; 3]),
        ];
        let result = compute_approximate_coordinates(&verts, 2.0);
        assert_eq!(result.len(), 2);
        assert!((result[0][0] - 1.0).abs() < 1e-10);
        assert!((result[0][1] - 2.0).abs() < 1e-10);
        assert!((result[0][2] - 3.0).abs() < 1e-10);
        assert!((result[1][0] - 4.0).abs() < 1e-10);
    }

    #[test]
    fn test_points_are_collinear() {
        // Collinear points
        assert!(points_are_collinear_3d(
            &[0.0, 0.0, 0.0],
            &[1.0, 0.0, 0.0],
            &[2.0, 0.0, 0.0],
        ));
        // Non-collinear points
        assert!(!points_are_collinear_3d(
            &[0.0, 0.0, 0.0],
            &[1.0, 0.0, 0.0],
            &[0.0, 1.0, 0.0],
        ));
    }
}
