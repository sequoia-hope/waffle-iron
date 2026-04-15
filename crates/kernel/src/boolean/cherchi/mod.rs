//! Cherchi mesh arrangement — 1:1 Rust port of FastAndRobustMeshArrangements.
//!
//! MIT License — Copyright (c) 2020 Gianmarco Cherchi, Marco Livesu,
//! Riccardo Scateni e Marco Attene.
//!
//! Ported from: github.com/gcherchi/FastAndRobustMeshArrangements
//! Paper: Cherchi et al. 2020 "Fast and Robust Mesh Arrangements"
//! Paper: Cherchi et al. 2022 "Interactive and Robust Mesh Booleans"

pub(crate) mod common;
pub(crate) mod fast_trimesh;
pub(crate) mod tree;

pub(crate) mod aux_structure;
pub(crate) mod intersection_class;
pub(crate) mod processing;
pub(crate) mod triangle_soup;
pub(crate) mod triangulation;

use self::aux_structure::AuxiliaryStructure;
use self::intersection_class::{classify_intersections, detect_intersections};
use self::processing::{
    compute_approximate_coordinates, compute_multiplier_flat, merge_duplicated_vertices_flat,
    remove_degenerate_and_duplicated_triangles,
};
use self::triangle_soup::TriangleSoup;
use self::triangulation::triangulation_with_parents;

/// Result of the mesh arrangement pipeline.
#[allow(dead_code)]
pub(crate) struct SolveResult {
    /// Output vertex coordinates.
    pub coords: Vec<[f64; 3]>,
    /// Output triangles (vertex index triples).
    pub tris: Vec<[usize; 3]>,
    /// Per-triangle labels preserved from input.
    pub labels: Vec<u32>,
    /// Per-output-triangle parent: index of the *preprocessed* triangle that produced it.
    pub parent_tris: Vec<usize>,
    /// Mapping from preprocessed triangle index → original input triangle index.
    /// Use this to convert `parent_tris` back to original indices.
    pub clean_to_orig: Vec<usize>,
}

/// Top-level mesh arrangement pipeline.
///
/// Takes flat coordinate + triangle arrays with per-triangle mesh labels.
/// Returns the subdivided mesh where all intersections are resolved into
/// explicit edges, with watertight conformal output guaranteed.
///
/// Ported from Cherchi solve_intersections.cpp:44-71 (meshArrangementPipeline)
#[allow(dead_code)]
pub(crate) fn solve_intersections(
    in_coords: &[f64],
    in_tris: &[usize],
    in_labels: &[u32],
) -> Result<SolveResult, String> {
    if in_tris.is_empty() {
        return Ok(SolveResult {
            coords: Vec::new(),
            tris: Vec::new(),
            labels: Vec::new(),
            parent_tris: Vec::new(),
            clean_to_orig: Vec::new(),
        });
    }

    // Step 1: Compute multiplier for predicate stability
    let multiplier = compute_multiplier_flat(in_coords);

    // Step 2: Merge duplicated vertices
    let (deduped_verts, deduped_tris) = merge_duplicated_vertices_flat(in_coords, in_tris);

    // Step 3: Remove degenerate and duplicated triangles
    let (clean_tris, clean_labels, clean_to_orig) =
        remove_degenerate_and_duplicated_triangles(&deduped_verts, &deduped_tris, in_labels);

    if clean_tris.is_empty() {
        return Ok(SolveResult {
            coords: Vec::new(),
            tris: Vec::new(),
            labels: Vec::new(),
            parent_tris: Vec::new(),
            clean_to_orig: Vec::new(),
        });
    }

    // Step 4: Create TriangleSoup (scales vertices by multiplier, adds jolly points)
    let mut ts = TriangleSoup::new(deduped_verts, clean_tris, clean_labels, multiplier);

    // Step 5: Detect intersecting triangle pairs (broad-phase BVH + exact predicates)
    let mut aux = AuxiliaryStructure::new();
    aux.init_from_triangle_soup(&ts);
    detect_intersections(&ts, &mut aux);

    // Step 6: Classify intersections — populate edge2pts, tri2pts, tri2segs
    classify_intersections(&mut ts, &mut aux);

    // Step 7: Triangulate — subdivide intersected triangles
    let (new_tris_flat, new_labels, parent_tris) = triangulation_with_parents(&mut ts, &mut aux);

    // Step 8: Compute approximate coordinates (inverse scale by multiplier,
    // exclude jolly points)
    let mut out_coords = compute_approximate_coordinates(&ts.vertices, multiplier);

    // Include jolly point coordinates in output if any output triangles
    // reference them. With exact indirect predicates (C++ reference) jolly
    // points never appear in output triangles, but our materialize-fallback
    // orient2d can produce triangles that reference them.
    let num_non_jolly = out_coords.len();
    let num_all_verts = ts.vertices.len();

    // Convert flat tri indices to [usize; 3] triples
    let num_out_tris = new_tris_flat.len() / 3;
    let mut out_tris = Vec::with_capacity(num_out_tris);
    let mut needs_jolly = false;
    for i in 0..num_out_tris {
        let v0 = new_tris_flat[3 * i];
        let v1 = new_tris_flat[3 * i + 1];
        let v2 = new_tris_flat[3 * i + 2];
        if v0 >= num_non_jolly || v1 >= num_non_jolly || v2 >= num_non_jolly {
            needs_jolly = true;
        }
        out_tris.push([v0, v1, v2]);
    }

    // Append jolly point coordinates if needed
    if needs_jolly {
        for v in &ts.vertices[num_non_jolly..num_all_verts] {
            let coords = v.materialize().unwrap_or([0.0, 0.0, 0.0]);
            let inv = if multiplier != 0.0 {
                1.0 / multiplier
            } else {
                1.0
            };
            out_coords.push([coords[0] * inv, coords[1] * inv, coords[2] * inv]);
        }
    }

    Ok(SolveResult {
        coords: out_coords,
        tris: out_tris,
        labels: new_labels,
        parent_tris,
        clean_to_orig,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_solve_intersections_three_cubes() {
        // 24 vertices, 36 triangles from three_cubes.stl (3 overlapping unit cubes)
        let coords: Vec<f64> = vec![
            1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0,
            -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 0.087676, 2.016374, 1.898318,
            0.087676, 2.016374, -0.101682, 0.087676, 0.016374, 1.898318, 0.087676, 0.016374,
            -0.101682, 2.087676, 0.016374, 1.898318, 2.087676, 0.016374, -0.101682, 2.087676,
            2.016374, 1.898318, 2.087676, 2.016374, -0.101682, -1.241614, 2.682978, 2.336984,
            -1.241614, 2.682978, 0.336983, -1.241614, 0.682978, 2.336984, -1.241614, 0.682978,
            0.336983, 0.758387, 0.682978, 2.336984, 0.758387, 0.682978, 0.336983, 0.758387,
            2.682978, 2.336984, 0.758387, 2.682978, 0.336983,
        ];
        let tris: Vec<usize> = vec![
            0, 1, 2, 3, 1, 0, 4, 5, 6, 7, 5, 4, 2, 7, 4, 1, 7, 2, 6, 3, 0, 5, 3, 6, 4, 0, 2, 6, 0,
            4, 5, 1, 3, 7, 1, 5, 8, 9, 10, 10, 9, 11, 12, 13, 14, 14, 13, 15, 10, 11, 12, 12, 11,
            13, 9, 8, 15, 15, 8, 14, 8, 10, 14, 14, 10, 12, 11, 9, 13, 13, 9, 15, 16, 17, 18, 18,
            17, 19, 20, 21, 22, 22, 21, 23, 18, 19, 20, 20, 19, 21, 17, 16, 23, 23, 16, 22, 16, 18,
            22, 22, 18, 20, 19, 17, 21, 21, 17, 23,
        ];
        let labels: Vec<u32> = vec![0; 36];

        let result = solve_intersections(&coords, &tris, &labels);
        assert!(result.is_ok(), "should not panic: {:?}", result.err());

        let r = result.unwrap();
        eprintln!(
            "three_cubes result: {} tris, {} coords",
            r.tris.len(),
            r.coords.len()
        );
        // C++ reference produces 212 tris. Match exactly.
        assert_eq!(
            r.tris.len(),
            212,
            "expected 212 output tris (matching C++ reference), got {}",
            r.tris.len()
        );

        // Check conformality: every directed edge should have its reverse
        let mut edge_count: HashMap<(usize, usize), usize> = HashMap::new();
        for tri in &r.tris {
            for i in 0..3 {
                *edge_count.entry((tri[i], tri[(i + 1) % 3])).or_default() += 1;
            }
        }
        let non_conformal: Vec<_> = edge_count
            .keys()
            .filter(|&&(a, b)| !edge_count.contains_key(&(b, a)))
            .collect();

        assert!(
            non_conformal.is_empty(),
            "non-conformal edges: {} (expected 0)",
            non_conformal.len()
        );
    }

    fn make_box_flat(
        x0: f64,
        y0: f64,
        z0: f64,
        x1: f64,
        y1: f64,
        z1: f64,
    ) -> (Vec<f64>, Vec<usize>) {
        let coords = vec![
            x0, y0, z0, x1, y0, z0, x1, y1, z0, x0, y1, z0, x0, y0, z1, x1, y0, z1, x1, y1, z1, x0,
            y1, z1,
        ];
        let tris = vec![
            0, 2, 1, 0, 3, 2, // -Z
            4, 5, 6, 4, 6, 7, // +Z
            0, 1, 5, 0, 5, 4, // -Y
            2, 3, 7, 2, 7, 6, // +Y
            0, 4, 7, 0, 7, 3, // -X
            1, 2, 6, 1, 6, 5, // +X
        ];
        (coords, tris)
    }

    /// Apply rotation around Y axis by angle (radians) to a flat coordinate list.
    fn rotate_y(coords: &mut [f64], angle: f64) {
        let c = angle.cos();
        let s = angle.sin();
        let n = coords.len() / 3;
        for i in 0..n {
            let x = coords[3 * i];
            let z = coords[3 * i + 2];
            coords[3 * i] = x * c + z * s;
            coords[3 * i + 2] = -x * s + z * c;
        }
    }

    #[test]
    fn test_cherchi_two_overlapping_boxes_rotated() {
        let (mut coords_a, tris_a) = make_box_flat(0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
        let (mut coords_b, tris_b) = make_box_flat(1.0, 0.0, 0.0, 3.0, 2.0, 2.0);
        // Rotate both meshes by 37 degrees around Y
        rotate_y(&mut coords_a, 37.0_f64.to_radians());
        rotate_y(&mut coords_b, 37.0_f64.to_radians());

        let offset = coords_a.len() / 3;
        let mut coords = coords_a;
        coords.extend_from_slice(&coords_b);
        let num_tris_a = tris_a.len() / 3;
        let mut tris: Vec<usize> = tris_a;
        for t in &tris_b {
            tris.push(t + offset);
        }
        let mut labels = vec![0u32; num_tris_a];
        labels.extend(vec![1u32; tris_b.len() / 3]);

        let result = solve_intersections(&coords, &tris, &labels);
        assert!(result.is_ok(), "should not panic: {:?}", result.err());
    }

    #[test]
    fn test_cherchi_two_boxes_enclosed() {
        // Box A: [-0.003,-0.003,0] → [0.003,0.003,0.004]
        // Box B: [-0.001,-0.001,0] → [0.001,0.001,0.004] (enclosed in A)
        // C++ reference: 16 verts, 36 edges, 24 tris → 36 intersecting pairs → 36 output tris, 0 NC
        let (ca, ta) = make_box_flat(-0.003, -0.003, 0.0, 0.003, 0.003, 0.004);
        let (cb, tb) = make_box_flat(-0.001, -0.001, 0.0, 0.001, 0.001, 0.004);

        let offset = ca.len() / 3;
        let mut coords = ca;
        coords.extend_from_slice(&cb);
        let mut tris: Vec<usize> = ta;
        for &t in &tb {
            tris.push(t + offset);
        }
        let mut labels = vec![1u32; 12]; // box A
        labels.extend(vec![2u32; 12]); // box B

        let result = solve_intersections(&coords, &tris, &labels);
        assert!(result.is_ok(), "should not panic: {:?}", result.err());
        let r = result.unwrap();

        // C++ produces 36 tris, 0 NC
        let mut edge_count: HashMap<(usize, usize), usize> = HashMap::new();
        for tri in &r.tris {
            for i in 0..3 {
                *edge_count.entry((tri[i], tri[(i + 1) % 3])).or_default() += 1;
            }
        }
        let nc: Vec<_> = edge_count
            .keys()
            .filter(|&&(a, b)| !edge_count.contains_key(&(b, a)))
            .collect();

        eprintln!(
            "F0002 result: {} tris, {} verts, {} NC",
            r.tris.len(),
            r.coords.len(),
            nc.len()
        );
        assert_eq!(
            nc.len(),
            0,
            "should have 0 non-conformal edges, got {}",
            nc.len()
        );
        assert_eq!(
            r.tris.len(),
            36,
            "C++ produces 36 tris, got {}",
            r.tris.len()
        );
    }

    #[test]
    fn test_cherchi_two_overlapping_boxes() {
        let (coords_a, tris_a) = make_box_flat(0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
        let (coords_b, tris_b) = make_box_flat(1.0, 0.0, 0.0, 3.0, 2.0, 2.0);

        let offset = coords_a.len() / 3;
        let mut coords = coords_a;
        coords.extend_from_slice(&coords_b);
        let num_tris_a = tris_a.len() / 3;
        let mut tris: Vec<usize> = tris_a;
        for t in &tris_b {
            tris.push(t + offset);
        }
        let mut labels = vec![0u32; num_tris_a];
        labels.extend(vec![1u32; tris_b.len() / 3]);

        let result = solve_intersections(&coords, &tris, &labels);
        assert!(result.is_ok(), "should not panic: {:?}", result.err());
    }
}
