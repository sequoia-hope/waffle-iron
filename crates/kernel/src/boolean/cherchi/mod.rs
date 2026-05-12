//! Cherchi mesh arrangement — 1:1 Rust port of FastAndRobustMeshArrangements.
//!
//! MIT License — Copyright (c) 2020 Gianmarco Cherchi, Marco Livesu,
//! Riccardo Scateni e Marco Attene.
//!
//! Ported from: github.com/gcherchi/FastAndRobustMeshArrangements
//!
//! Paper lineage (see specs/cherchi_indirect_predicates.md "Paper Lineage and
//! Codebase Map" for full details):
//!   - Cherchi 2020 [#9] "Fast and Robust Mesh Arrangements" — the arrangement
//!     algorithm (§5) and indirect predicates (§4) implemented here.
//!   - Cherchi 2022 "Interactive and Robust Mesh Booleans" — speed improvements
//!     (cached `orient3d`, parallelism, octree refinements) on top of the 2020
//!     arrangement; introduces Algorithm 1 ray-cast in/out (§5), implemented
//!     in `boolean/exact_mesh.rs::label_sub_tri_raycast`. Several files in this
//!     module (processing.rs, aux_structure.rs, intersection_class.rs,
//!     triangulation.rs) were ported from the InteractiveAndRobustMeshBooleans
//!     codebase that combines both papers.
//!   - Livesu et al. 2021 "Deterministic Linear Time Constrained Triangulation
//!     Using Simplified Earcut" — the linear-time CDT used by Cherchi 2022 §4
//!     for segment insertion (`triangulation.rs::earcut_linear`).

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
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

// PR10 Path A-refined: re-export the cosurface orientation enum so consumers
// (e.g. `exact_mesh::SubTriangle`) can plumb it through. Cherchi 2020 §5.4 /
// Hoffmann 1989 §5.3.
pub(crate) use processing::Orientation;

// PR2 telemetry: count `solve_intersections` calls where any output triangle
// references a jolly vertex. Jolly points are a Cherchi 2020 §5.4 algorithmic
// construct (5 fixed utility points) — informational only, not a hack.
pub(crate) static JOLLY_POINT_CREATIONS: AtomicUsize = AtomicUsize::new(0);

// Y33_PROBE: per-`solve_intersections`-call counter so multi-invocation cases
// (e.g. F0020 = 3 extrudes → 2 boolean invocations) dump into distinct
// per-invocation subdirectories (`inv0/`, `inv1/`, ...). Only matters when
// `Y33_PROBE=1`; otherwise the counter still increments but no dumps happen.
static Y33_INVOCATION_COUNTER: AtomicU32 = AtomicU32::new(0);

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
    /// PR10 Path A-refined: per-output-triangle cosurface orientation (parallel
    /// to `labels`). `Some(Parallel)` / `Some(AntiParallel)` when the parent
    /// preprocessed triangle was a STAGE2 cosurface merge; `None` otherwise.
    /// Cherchi 2020 §5.4 / Hoffmann 1989 §5.3.
    pub cosurface_orientation: Vec<Option<Orientation>>,
}

/// Y33_PROBE: per-stage dump utilities. Gated entirely behind `Y33_PROBE=1`
/// env var; output directory from `Y33_PROBE_DIR` (default
/// `/tmp/y33-canary/waffle`). Output formats are intentionally simple text —
/// designed for line-by-line diff against matching dumps from the Cherchi 2022
/// C++ reference patched at
/// `~/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/solve_intersections.cpp`.
mod y33_probe {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    pub(super) fn dir_for(inv: u32) -> Option<PathBuf> {
        if std::env::var("Y33_PROBE").as_deref() != Ok("1") {
            return None;
        }
        let base =
            std::env::var("Y33_PROBE_DIR").unwrap_or_else(|_| "/tmp/y33-canary/waffle".to_string());
        let inv_dir = PathBuf::from(base).join(format!("inv{}", inv));
        std::fs::create_dir_all(&inv_dir).ok()?;
        Some(inv_dir)
    }

    /// Remap a Rust vertex id to the C++ vertex-id space: Rust has jolly
    /// points at `[num_orig..num_orig+5)` and implicit verts at
    /// `[num_orig+5..)`; C++ doesn't insert jolly points until
    /// `appendJollyPoints()` (called AFTER triangulation in
    /// `solveIntersections.cpp:66`), so C++ implicit verts are at
    /// `[num_orig..)`. To make dumps byte-comparable, the Rust dump skips the
    /// 5 jolly slots and renumbers implicits down by 5.
    fn remap_vid(v_id: usize, n_orig: usize) -> Option<usize> {
        if v_id < n_orig {
            Some(v_id)
        } else if v_id < n_orig + 5 {
            None // jolly — filtered out for C++ parity at this stage
        } else {
            Some(v_id - 5)
        }
    }

    pub(super) fn dump_stage3(dir: &PathBuf, ts: &TriangleSoup, multiplier: f64) {
        let inv_mul = if multiplier != 0.0 {
            1.0 / multiplier
        } else {
            1.0
        };
        let n_orig = ts.num_orig_verts();
        if let Ok(mut f) = File::create(dir.join("stage3_verts.txt")) {
            // Emit originals only (STAGE3 has no implicits yet; jolly skipped).
            for v_id in 0..n_orig {
                let p = ts.implicit_point(v_id).materialize().unwrap_or([0.0; 3]);
                let _ = writeln!(
                    f,
                    "{} O {:.15e} {:.15e} {:.15e}",
                    v_id,
                    p[0] * inv_mul,
                    p[1] * inv_mul,
                    p[2] * inv_mul
                );
            }
        }
        if let Ok(mut f) = File::create(dir.join("stage3_jolly.txt")) {
            // Jolly points emitted separately (informational; never in F0020 output).
            for j in 0..5 {
                let p = ts
                    .implicit_point(n_orig + j)
                    .materialize()
                    .unwrap_or([0.0; 3]);
                let _ = writeln!(
                    f,
                    "{} J {:.15e} {:.15e} {:.15e}",
                    j,
                    p[0] * inv_mul,
                    p[1] * inv_mul,
                    p[2] * inv_mul
                );
            }
        }
        if let Ok(mut f) = File::create(dir.join("stage3_tris.txt")) {
            for t_id in 0..ts.num_tris() {
                let t = ts.tri(t_id);
                let v0 = remap_vid(t[0], n_orig).unwrap_or(usize::MAX);
                let v1 = remap_vid(t[1], n_orig).unwrap_or(usize::MAX);
                let v2 = remap_vid(t[2], n_orig).unwrap_or(usize::MAX);
                let _ = writeln!(f, "{} {} {} {} {}", t_id, v0, v1, v2, ts.tri_label(t_id));
            }
        }
        if let Ok(mut f) = File::create(dir.join("stage3_edges.txt")) {
            for e_id in 0..ts.num_edges() {
                let (a, b) = ts.edge_verts(e_id);
                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                let lo = remap_vid(lo, n_orig).unwrap_or(usize::MAX);
                let hi = remap_vid(hi, n_orig).unwrap_or(usize::MAX);
                let _ = writeln!(f, "{} {} {}", e_id, lo, hi);
            }
        }
    }

    pub(super) fn dump_stage4(dir: &PathBuf, aux: &AuxiliaryStructure) {
        let mut pairs: Vec<(usize, usize)> = aux
            .intersection_list()
            .iter()
            .map(|&(a, b)| if a < b { (a, b) } else { (b, a) })
            .collect();
        pairs.sort();
        if let Ok(mut f) = File::create(dir.join("stage4_pairs.txt")) {
            for (a, b) in pairs {
                let _ = writeln!(f, "{} {}", a, b);
            }
        }
    }

    pub(super) fn dump_stage5(dir: &PathBuf, ts: &TriangleSoup, aux: &AuxiliaryStructure) {
        let n_orig = ts.num_orig_verts();
        let remap = |v: usize| remap_vid(v, n_orig).unwrap_or(usize::MAX);
        if let Ok(mut f) = File::create(dir.join("stage5_int_tris.txt")) {
            for t_id in 0..ts.num_tris() {
                if aux.triangle_has_intersections(t_id) {
                    let _ = writeln!(f, "{}", t_id);
                }
            }
        }
        if let Ok(mut f) = File::create(dir.join("stage5_cop_tris.txt")) {
            for t_id in 0..ts.num_tris() {
                if aux.triangle_has_coplanars(t_id) {
                    let mut cop: Vec<usize> = aux.coplanar_triangles(t_id).to_vec();
                    cop.sort();
                    let _ = writeln!(f, "{} {:?}", t_id, cop);
                }
            }
        }
        if let Ok(mut f) = File::create(dir.join("stage5_segs.txt")) {
            for t_id in 0..ts.num_tris() {
                let segs = aux.triangle_segments_list(t_id);
                if !segs.is_empty() {
                    let mut canon: Vec<(usize, usize)> = segs
                        .iter()
                        .map(|&(a, b)| {
                            let ra = remap(a);
                            let rb = remap(b);
                            if ra < rb {
                                (ra, rb)
                            } else {
                                (rb, ra)
                            }
                        })
                        .collect();
                    canon.sort();
                    for (a, b) in canon {
                        let _ = writeln!(f, "{} {} {}", t_id, a, b);
                    }
                }
            }
        }
        if let Ok(mut f) = File::create(dir.join("stage5_tri2pts.txt")) {
            for t_id in 0..ts.num_tris() {
                let pts = aux.triangle_points_list(t_id);
                if !pts.is_empty() {
                    let mut p: Vec<usize> = pts.iter().map(|&v| remap(v)).collect();
                    p.sort();
                    let _ = writeln!(f, "{} {:?}", t_id, p);
                }
            }
        }
    }

    pub(super) fn dump_stage6(
        dir: &PathBuf,
        ts: &TriangleSoup,
        new_tris_flat: &[usize],
        multiplier: f64,
    ) {
        let n_orig = ts.num_orig_verts();
        let approx = compute_approximate_coordinates(&ts.vertices, multiplier);
        // Emit non-jolly verts (originals + implicits) renumbered to C++ ID space.
        // Note: `compute_approximate_coordinates` excludes jolly points internally
        // by truncating at num_non_jolly (per processing.rs:338-365).
        if let Ok(mut f) = File::create(dir.join("stage6_verts.txt")) {
            for (v_id, p) in approx.iter().enumerate() {
                // approx is indexed [0..num_orig + n_implicit) — already excludes jollies.
                // But the original-id-to-emit mapping is identity for [0..n_orig)
                // and (v_id - 0) for [n_orig..) because compute_approximate skips
                // 5 jolly slots between them. Verify by checking len.
                let _ = writeln!(f, "{} {:.15e} {:.15e} {:.15e}", v_id, p[0], p[1], p[2]);
            }
        }
        // Triangle verts may still reference Rust's pre-renumbering ID space
        // (with jolly slots). Apply remap_vid to convert to C++ ID space.
        if let Ok(mut f) = File::create(dir.join("stage6_tris.txt")) {
            let n = new_tris_flat.len() / 3;
            for t in 0..n {
                let v0 = remap_vid(new_tris_flat[3 * t], n_orig).unwrap_or(usize::MAX);
                let v1 = remap_vid(new_tris_flat[3 * t + 1], n_orig).unwrap_or(usize::MAX);
                let v2 = remap_vid(new_tris_flat[3 * t + 2], n_orig).unwrap_or(usize::MAX);
                let _ = writeln!(f, "{} {} {} {}", t, v0, v1, v2);
            }
        }
    }
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
    d_epsilon: f64,
) -> Result<SolveResult, String> {
    if in_tris.is_empty() {
        return Ok(SolveResult {
            coords: Vec::new(),
            tris: Vec::new(),
            labels: Vec::new(),
            parent_tris: Vec::new(),
            clean_to_orig: Vec::new(),
            cosurface_orientation: Vec::new(),
        });
    }

    // PR2 telemetry: snapshot jolly counter at entry; emit per-call delta after
    // STAGE6. Tracks how often Cherchi 2020 §5.4 coplanar-disambiguation fires.
    let jolly_before = JOLLY_POINT_CREATIONS.load(Ordering::Relaxed);

    // Y33_PROBE: which invocation are we on this corpus run.
    let y33_inv = Y33_INVOCATION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let y33_dir = y33_probe::dir_for(y33_inv);

    // Step 1: Compute multiplier for predicate stability
    let multiplier = compute_multiplier_flat(in_coords);

    // Step 2: Merge duplicated vertices
    let (deduped_verts, deduped_tris) = merge_duplicated_vertices_flat(in_coords, in_tris);
    eprintln!(
        "[cherchi-trace] STAGE1 merge: {} verts, {} tris",
        deduped_verts.len(),
        deduped_tris.len() / 3
    );

    // Step 3: Remove degenerate and duplicated triangles
    let (clean_tris, clean_labels, clean_to_orig, clean_orientations) =
        remove_degenerate_and_duplicated_triangles(&deduped_verts, &deduped_tris, in_labels);
    eprintln!(
        "[cherchi-trace] STAGE2 degenerate: {} tris",
        clean_tris.len() / 3
    );

    if clean_tris.is_empty() {
        return Ok(SolveResult {
            coords: Vec::new(),
            tris: Vec::new(),
            labels: Vec::new(),
            parent_tris: Vec::new(),
            clean_to_orig: Vec::new(),
            cosurface_orientation: Vec::new(),
        });
    }

    // Step 4: Create TriangleSoup (scales vertices by multiplier, adds jolly points)
    let mut ts = TriangleSoup::new(deduped_verts, clean_tris, clean_labels, multiplier);
    eprintln!(
        "[cherchi-trace] STAGE3 soup: {} verts, {} edges, {} tris",
        ts.num_verts(),
        ts.num_edges(),
        ts.num_tris()
    );
    if let Some(d) = y33_dir.as_ref() {
        y33_probe::dump_stage3(d, &ts, multiplier);
    }

    // Step 5: Detect intersecting triangle pairs (broad-phase BVH + exact predicates)
    let mut aux = AuxiliaryStructure::new();
    aux.init_from_triangle_soup(&ts);
    detect_intersections(&ts, &mut aux, d_epsilon);
    eprintln!(
        "[cherchi-trace] STAGE4 pairs: {}",
        aux.intersection_list().len()
    );
    if let Some(d) = y33_dir.as_ref() {
        y33_probe::dump_stage4(d, &aux);
    }

    // Step 6: Classify intersections — populate edge2pts, tri2pts, tri2segs
    classify_intersections(&mut ts, &mut aux);
    let tris_with_int = (0..ts.num_tris())
        .filter(|&t| aux.triangle_has_intersections(t))
        .count();
    let tris_with_cop = (0..ts.num_tris())
        .filter(|&t| aux.triangle_has_coplanars(t))
        .count();
    eprintln!(
        "[cherchi-trace] STAGE5 classify: {} with_intersections, {} with_coplanars",
        tris_with_int, tris_with_cop
    );
    if let Some(d) = y33_dir.as_ref() {
        y33_probe::dump_stage5(d, &ts, &aux);
    }

    // Step 7: Triangulate — subdivide intersected triangles. PR10:
    // `clean_orientations` is keyed by preprocessed-triangle index and is
    // propagated into a parallel-to-`new_labels` vec by STAGE6.
    let (new_tris_flat, new_labels, parent_tris, new_cosurface_orientation) =
        triangulation_with_parents(&mut ts, &mut aux, &clean_orientations);
    eprintln!(
        "[cherchi-trace] STAGE6 triangulation: {} tris",
        new_tris_flat.len() / 3
    );
    if let Some(d) = y33_dir.as_ref() {
        y33_probe::dump_stage6(d, &ts, &new_tris_flat, multiplier);
    }

    // PR2 telemetry: emit per-call jolly delta. Tracks how often Cherchi 2020
    // §5.4 coplanar-disambiguation fires across the assay corpus.
    let jolly_after = JOLLY_POINT_CREATIONS.load(Ordering::Relaxed);
    let jolly_d = jolly_after - jolly_before;
    eprintln!("[cherchi-tele] jolly_creations: {}", jolly_d);

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
            // PR2 telemetry: count once per solve_intersections call where any
            // jolly is actually needed (false→true transition).
            if !needs_jolly {
                JOLLY_POINT_CREATIONS.fetch_add(1, Ordering::Relaxed);
                needs_jolly = true;
            }
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
        cosurface_orientation: new_cosurface_orientation,
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

        let result = solve_intersections(&coords, &tris, &labels, 0.0);
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

        let result = solve_intersections(&coords, &tris, &labels, 0.0);
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

        let result = solve_intersections(&coords, &tris, &labels, 0.0);
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

        let result = solve_intersections(&coords, &tris, &labels, 0.0);
        assert!(result.is_ok(), "should not panic: {:?}", result.err());
    }
}
