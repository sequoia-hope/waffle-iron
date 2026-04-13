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
    /// Per-output-triangle parent: index of the input triangle that produced it.
    pub parent_tris: Vec<usize>,
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
        });
    }

    // Step 1: Compute multiplier for predicate stability
    let multiplier = compute_multiplier_flat(in_coords);

    // Step 2: Merge duplicated vertices
    let (deduped_verts, deduped_tris) = merge_duplicated_vertices_flat(in_coords, in_tris);

    // Step 3: Remove degenerate and duplicated triangles
    let (clean_tris, clean_labels) =
        remove_degenerate_and_duplicated_triangles(&deduped_verts, &deduped_tris, in_labels);

    if clean_tris.is_empty() {
        return Ok(SolveResult {
            coords: Vec::new(),
            tris: Vec::new(),
            labels: Vec::new(),
            parent_tris: Vec::new(),
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
    let out_coords = compute_approximate_coordinates(&ts.vertices, multiplier);

    // Convert flat tri indices to [usize; 3] triples
    let num_out_tris = new_tris_flat.len() / 3;
    let mut out_tris = Vec::with_capacity(num_out_tris);
    for i in 0..num_out_tris {
        out_tris.push([
            new_tris_flat[3 * i],
            new_tris_flat[3 * i + 1],
            new_tris_flat[3 * i + 2],
        ]);
    }

    Ok(SolveResult {
        coords: out_coords,
        tris: out_tris,
        labels: new_labels,
        parent_tris,
    })
}
