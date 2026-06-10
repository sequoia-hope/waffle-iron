//! Verification oracles — pure functions returning pass/fail verdicts.
//!
//! Each oracle returns an `OracleVerdict` with diagnostic detail, not panics.
//! This lets agents collect all failures in one pass.

use std::collections::HashMap;

use kernel::types::RenderMesh;
use kernel::units::{
    TAU_COINCIDENT, TAU_NORMALIZE_SQ, TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN, TAU_WELD_MAX,
};
use kernel::{KernelIntrospect, KernelSolidHandle};
use modeling_ops::types::OpResult;
use waffle_types::Role;

/// The result of a single oracle check.
#[derive(Debug, Clone)]
pub struct OracleVerdict {
    pub oracle_name: String,
    pub passed: bool,
    pub detail: String,
    pub value: Option<f64>,
}

impl OracleVerdict {
    fn pass(name: &str, detail: String) -> Self {
        Self {
            oracle_name: name.to_string(),
            passed: true,
            detail,
            value: None,
        }
    }

    fn pass_val(name: &str, detail: String, value: f64) -> Self {
        Self {
            oracle_name: name.to_string(),
            passed: true,
            detail,
            value: Some(value),
        }
    }

    fn fail(name: &str, detail: String) -> Self {
        Self {
            oracle_name: name.to_string(),
            passed: false,
            detail,
            value: None,
        }
    }

    fn fail_val(name: &str, detail: String, value: f64) -> Self {
        Self {
            oracle_name: name.to_string(),
            passed: false,
            detail,
            value: Some(value),
        }
    }
}

// ── Topology Oracles ────────────────────────────────────────────────────────

/// Check Euler's formula: V - E + F = 2 (for genus-0 solids).
pub fn check_euler_formula(
    introspect: &dyn KernelIntrospect,
    solid: &KernelSolidHandle,
) -> OracleVerdict {
    let v = introspect.list_vertices(solid).len() as i64;
    let e = introspect.list_edges(solid).len() as i64;
    let f = introspect.list_faces(solid).len() as i64;
    let euler = v - e + f;

    if euler == 2 {
        OracleVerdict::pass_val(
            "euler_formula",
            format!("V({}) - E({}) + F({}) = 2", v, e, f),
            euler as f64,
        )
    } else {
        OracleVerdict::fail_val(
            "euler_formula",
            format!("V({}) - E({}) + F({}) = {} (expected 2)", v, e, f, euler),
            euler as f64,
        )
    }
}

/// Check that every edge has exactly 2 adjacent faces (manifold condition).
pub fn check_manifold_edges(
    introspect: &dyn KernelIntrospect,
    solid: &KernelSolidHandle,
) -> OracleVerdict {
    let edges = introspect.list_edges(solid);
    let mut non_manifold = Vec::new();

    for &edge in &edges {
        let face_count = introspect.edge_faces(edge).len();
        if face_count != 2 {
            non_manifold.push((edge, face_count));
        }
    }

    if non_manifold.is_empty() {
        OracleVerdict::pass(
            "manifold_edges",
            format!("all {} edges have exactly 2 faces", edges.len()),
        )
    } else {
        OracleVerdict::fail(
            "manifold_edges",
            format!(
                "{} non-manifold edges: {:?}",
                non_manifold.len(),
                &non_manifold[..non_manifold.len().min(5)]
            ),
        )
    }
}

/// Check that every face has at least 3 edges.
pub fn check_face_validity(
    introspect: &dyn KernelIntrospect,
    solid: &KernelSolidHandle,
) -> OracleVerdict {
    let faces = introspect.list_faces(solid);
    let mut invalid = Vec::new();

    for &face in &faces {
        let edge_count = introspect.face_edges(face).len();
        if edge_count < 3 {
            invalid.push((face, edge_count));
        }
    }

    if invalid.is_empty() {
        OracleVerdict::pass(
            "face_validity",
            format!("all {} faces have >= 3 edges", faces.len()),
        )
    } else {
        OracleVerdict::fail(
            "face_validity",
            format!(
                "{} invalid faces (< 3 edges): {:?}",
                invalid.len(),
                &invalid[..invalid.len().min(5)]
            ),
        )
    }
}

/// Check exact vertex/edge/face counts.
pub fn check_topology_counts(
    introspect: &dyn KernelIntrospect,
    solid: &KernelSolidHandle,
    expected_v: usize,
    expected_e: usize,
    expected_f: usize,
) -> OracleVerdict {
    let v = introspect.list_vertices(solid).len();
    let e = introspect.list_edges(solid).len();
    let f = introspect.list_faces(solid).len();

    if v == expected_v && e == expected_e && f == expected_f {
        OracleVerdict::pass("topology_counts", format!("V={} E={} F={}", v, e, f))
    } else {
        OracleVerdict::fail(
            "topology_counts",
            format!(
                "expected V={} E={} F={}, got V={} E={} F={}",
                expected_v, expected_e, expected_f, v, e, f
            ),
        )
    }
}

// ── Mesh Oracles ────────────────────────────────────────────────────────────

/// Check that the mesh is watertight: every triangle edge shared by exactly 2 triangles.
///
/// Uses position-based edge matching (quantized to 1e-4) to handle meshes with
/// per-face vertices (non-shared vertex indices but shared positions).
pub fn check_watertight_mesh(mesh: &RenderMesh) -> OracleVerdict {
    // Compute scale-adaptive quantization: the grid must be above f32 noise
    // (~magnitude * 1.2e-7) but small enough to resolve geometry features.
    // Use max_abs * 2e-6 (17x safety margin above f32 noise) with a small
    // absolute floor for near-zero coordinates. No large floor — previously
    // 1e-4 caused geometry collapse for models at scale ~1e-4.
    let max_abs = mesh
        .vertices
        .iter()
        .map(|v| v.abs())
        .fold(0.0_f32, f32::max);
    let grid_size = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid_size;

    // Quantize vertex positions to allow position-based matching
    let quantize = |v: f32| -> i64 { (v as f64 * inv_grid).round() as i64 };

    let vert_key = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize * 3;
        (
            quantize(mesh.vertices[i]),
            quantize(mesh.vertices[i + 1]),
            quantize(mesh.vertices[i + 2]),
        )
    };

    type PosEdge = ((i64, i64, i64), (i64, i64, i64));

    fn make_edge(a: (i64, i64, i64), b: (i64, i64, i64)) -> PosEdge {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    let mut edge_counts: HashMap<PosEdge, usize> = HashMap::new();

    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let va = vert_key(tri[0]);
        let vb = vert_key(tri[1]);
        let vc = vert_key(tri[2]);

        *edge_counts.entry(make_edge(va, vb)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(vb, vc)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(vc, va)).or_insert(0) += 1;
    }

    let non_paired: Vec<_> = edge_counts.iter().filter(|(_, &c)| c != 2).collect();

    // ── PR-Y38 grid-sensitivity probe (INFRA-CLASS, env-gated, additive) ──
    //
    // Re-runs edge pairing at multiple TAU_TESS_GRID_FACTOR multipliers and
    // performs a 27-neighbor near-pair scan at the default 1× grid. Default-
    // off path is byte-identical — all probe logic gated behind a single env
    // check. Output: per-invocation TSV at $Y38_GRID_PROBE_DIR.
    if std::env::var("Y38_GRID_PROBE").as_deref() == Ok("1") {
        y38_grid_sensitivity_probe(mesh, max_abs, &edge_counts, &non_paired);
    }

    if non_paired.is_empty() {
        OracleVerdict::pass(
            "watertight_mesh",
            format!("all {} edges paired", edge_counts.len()),
        )
    } else {
        // Count by edge multiplicity for diagnostics
        let count_1 = non_paired.iter().filter(|(_, &c)| c == 1).count();
        let count_3plus = non_paired.iter().filter(|(_, &c)| c >= 3).count();
        let detail = if count_3plus > 0 {
            format!(
                "{} unpaired edges out of {} total ({} boundary, {} non-manifold)",
                non_paired.len(),
                edge_counts.len(),
                count_1,
                count_3plus
            )
        } else {
            format!(
                "{} unpaired edges out of {} total",
                non_paired.len(),
                edge_counts.len()
            )
        };
        OracleVerdict::fail("watertight_mesh", detail)
    }
}

// PR-Y38 probe helpers (env-gated; only invoked under Y38_GRID_PROBE=1).
//
// Per-invocation monotonic counter so each `check_watertight_mesh` call
// produces a distinct TSV filename. The canary maps invocation number ↔
// case by spotlight run ordering (documented in pr_y38_canary.md §3).
static Y38_INVOCATION_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

type Y38PosEdge = ((i64, i64, i64), (i64, i64, i64));

fn y38_make_edge(a: (i64, i64, i64), b: (i64, i64, i64)) -> Y38PosEdge {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Re-count non-paired edges at a given grid multiplier `m`.
fn y38_count_non_paired_at_multiplier(mesh: &RenderMesh, max_abs: f32, m: f64) -> (usize, usize) {
    let grid_size_m = (max_abs as f64 * TAU_TESS_GRID_FACTOR * m).max(TAU_TESS_GRID_MIN * m);
    let inv_grid_m = 1.0 / grid_size_m;
    let q = |v: f32| -> i64 { (v as f64 * inv_grid_m).round() as i64 };
    let key = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize * 3;
        (
            q(mesh.vertices[i]),
            q(mesh.vertices[i + 1]),
            q(mesh.vertices[i + 2]),
        )
    };
    let mut ec: HashMap<Y38PosEdge, usize> = HashMap::new();
    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let va = key(tri[0]);
        let vb = key(tri[1]);
        let vc = key(tri[2]);
        *ec.entry(y38_make_edge(va, vb)).or_insert(0) += 1;
        *ec.entry(y38_make_edge(vb, vc)).or_insert(0) += 1;
        *ec.entry(y38_make_edge(vc, va)).or_insert(0) += 1;
    }
    let non_paired = ec.values().filter(|&&c| c != 2).count();
    (non_paired, ec.len())
}

/// For each unpaired edge at the default 1× grid, scan the ±1 i64-cell
/// neighborhood of both endpoints (27³ candidate pairs at most, minus self),
/// and classify by minimum Chebyshev distance to a paired/known edge.
/// Returns (dist1, dist2, isolated) counts. We restrict to ±1 in each axis,
/// so the maximum observable Chebyshev distance is 1 — but we still report
/// dist2/isolated buckets in the TSV header per the plan, with `dist2=0`
/// always (per the brief's recommended ±1 scan). See memo §2 for rationale.
fn y38_near_pair_scan(
    edge_counts: &HashMap<Y38PosEdge, usize>,
    non_paired: &[(&Y38PosEdge, &usize)],
) -> (usize, usize, usize) {
    let mut dist1 = 0usize;
    let mut dist2 = 0usize;
    let mut isolated = 0usize;
    let neighbors: Vec<(i64, i64, i64)> = (-1..=1)
        .flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| (dx, dy, dz))))
        .collect();
    for ((va, vb), _) in non_paired {
        let mut min_dist: Option<i64> = None;
        for &(dax, day, daz) in &neighbors {
            for &(dbx, dby, dbz) in &neighbors {
                // Skip the original edge itself.
                if dax == 0 && day == 0 && daz == 0 && dbx == 0 && dby == 0 && dbz == 0 {
                    continue;
                }
                let va2 = (va.0 + dax, va.1 + day, va.2 + daz);
                let vb2 = (vb.0 + dbx, vb.1 + dby, vb.2 + dbz);
                if va2 == vb2 {
                    continue; // degenerate
                }
                let cand = y38_make_edge(va2, vb2);
                if let Some(&c) = edge_counts.get(&cand) {
                    if c >= 1 {
                        // Chebyshev distance is max over all 6 perturbations.
                        let d = dax
                            .abs()
                            .max(day.abs())
                            .max(daz.abs())
                            .max(dbx.abs())
                            .max(dby.abs())
                            .max(dbz.abs());
                        min_dist = Some(min_dist.map_or(d, |m| m.min(d)));
                    }
                }
            }
        }
        match min_dist {
            Some(1) => dist1 += 1,
            Some(d) if d >= 2 => dist2 += 1,
            Some(_) => unreachable!("Chebyshev distance over +/-1 scan is in 0 or 1"),
            None => isolated += 1,
        }
    }
    (dist1, dist2, isolated)
}

fn y38_grid_sensitivity_probe(
    mesh: &RenderMesh,
    max_abs: f32,
    edge_counts: &HashMap<Y38PosEdge, usize>,
    non_paired: &[(&Y38PosEdge, &usize)],
) {
    let dir = match std::env::var("Y38_GRID_PROBE_DIR") {
        Ok(d) => d,
        Err(_) => {
            eprintln!("[Y38_GRID_PROBE] Y38_GRID_PROBE_DIR not set; skipping write");
            return;
        }
    };
    if std::fs::create_dir_all(&dir).is_err() {
        eprintln!("[Y38_GRID_PROBE] failed to create dir {}; skipping", dir);
        return;
    }
    let inv_n = Y38_INVOCATION_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    let multipliers: [(f64, &str); 6] = [
        (0.5, "05x"),
        (1.0, "1x"),
        (2.0, "2x"),
        (4.0, "4x"),
        (10.0, "10x"),
        (100.0, "100x"),
    ];
    let mut grid_counts: Vec<(usize, usize)> = Vec::with_capacity(multipliers.len());
    for (m, _label) in &multipliers {
        grid_counts.push(y38_count_non_paired_at_multiplier(mesh, max_abs, *m));
    }

    let (dist1, dist2, isolated) = y38_near_pair_scan(edge_counts, non_paired);

    let header = "case\ttotal_edges\tunpaired_at_05x\tunpaired_at_1x\tunpaired_at_2x\tunpaired_at_4x\tunpaired_at_10x\tunpaired_at_100x\tnear_pair_dist1\tnear_pair_dist2\tisolated\tnon_paired_at_1x_oracle\n";
    let case_label = std::env::var("Y38_PROBE_CASE_NAME").unwrap_or_else(|_| "unknown".into());
    let row = format!(
        "{}_inv{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        case_label,
        inv_n,
        grid_counts[1].1, // total_edges at 1×
        grid_counts[0].0,
        grid_counts[1].0,
        grid_counts[2].0,
        grid_counts[3].0,
        grid_counts[4].0,
        grid_counts[5].0,
        dist1,
        dist2,
        isolated,
        non_paired.len(),
    );

    let path = format!("{}/Y38_inv{:04}_grid_sensitivity.tsv", dir, inv_n);
    if let Err(e) = std::fs::write(&path, format!("{}{}", header, row)) {
        eprintln!("[Y38_GRID_PROBE] write {} failed: {}", path, e);
    }
}

/// Check that stored normals are consistent with geometric winding.
pub fn check_consistent_normals(mesh: &RenderMesh) -> OracleVerdict {
    let verts = &mesh.vertices;
    let norms = &mesh.normals;
    let mut inconsistent = 0usize;
    let total = mesh.indices.len() / 3;

    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let i0 = tri[0] as usize * 3;
        let i1 = tri[1] as usize * 3;
        let i2 = tri[2] as usize * 3;

        if i0 + 2 >= verts.len() || i1 + 2 >= verts.len() || i2 + 2 >= verts.len() {
            continue;
        }

        // Geometric normal from cross product (f64 for precision)
        let ax = (verts[i1] - verts[i0]) as f64;
        let ay = (verts[i1 + 1] - verts[i0 + 1]) as f64;
        let az = (verts[i1 + 2] - verts[i0 + 2]) as f64;
        let bx = (verts[i2] - verts[i0]) as f64;
        let by = (verts[i2 + 1] - verts[i0 + 1]) as f64;
        let bz = (verts[i2 + 2] - verts[i0 + 2]) as f64;
        let gnx = ay * bz - az * by;
        let gny = az * bx - ax * bz;
        let gnz = ax * by - ay * bx;

        // Skip degenerate triangles — cross product unreliable for tiny areas
        let area_sq = gnx * gnx + gny * gny + gnz * gnz;
        if area_sq < 1e-20 {
            continue;
        }

        // Average stored normal for the triangle's vertices
        if i0 + 2 >= norms.len() || i1 + 2 >= norms.len() || i2 + 2 >= norms.len() {
            continue;
        }
        let snx = (norms[i0] as f64 + norms[i1] as f64 + norms[i2] as f64) / 3.0;
        let sny = (norms[i0 + 1] as f64 + norms[i1 + 1] as f64 + norms[i2 + 1] as f64) / 3.0;
        let snz = (norms[i0 + 2] as f64 + norms[i1 + 2] as f64 + norms[i2 + 2] as f64) / 3.0;

        let dot = gnx * snx + gny * sny + gnz * snz;
        if dot < 0.0 {
            inconsistent += 1;
        }
    }

    // Allow a tiny tolerance for near-degenerate triangles that escape the area
    // threshold but have unreliable winding due to numerical noise. Require ≥99%
    // consistent to pass.
    let ratio = (total - inconsistent) as f64 / total as f64;
    if ratio >= 0.99 {
        OracleVerdict::pass(
            "consistent_normals",
            format!(
                "{} of {} triangles have consistent winding ({}%)",
                total - inconsistent,
                total,
                (ratio * 100.0).round()
            ),
        )
    } else {
        OracleVerdict::fail(
            "consistent_normals",
            format!(
                "{} of {} triangles have reversed normals",
                inconsistent, total
            ),
        )
    }
}

/// Check that no triangles have zero area (degenerate).
pub fn check_no_degenerate_triangles(mesh: &RenderMesh) -> OracleVerdict {
    let verts = &mesh.vertices;
    let mut degenerate = 0usize;
    let total = mesh.indices.len() / 3;

    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let i0 = tri[0] as usize * 3;
        let i1 = tri[1] as usize * 3;
        let i2 = tri[2] as usize * 3;

        if i0 + 2 >= verts.len() || i1 + 2 >= verts.len() || i2 + 2 >= verts.len() {
            continue;
        }

        let ax = verts[i1] - verts[i0];
        let ay = verts[i1 + 1] - verts[i0 + 1];
        let az = verts[i1 + 2] - verts[i0 + 2];
        let bx = verts[i2] - verts[i0];
        let by = verts[i2 + 1] - verts[i0 + 1];
        let bz = verts[i2 + 2] - verts[i0 + 2];

        let cx = ay * bz - az * by;
        let cy = az * bx - ax * bz;
        let cz = ax * by - ay * bx;
        let area = (cx * cx + cy * cy + cz * cz).sqrt() / 2.0;

        if area < 1e-12 {
            degenerate += 1;
        }
    }

    if degenerate == 0 {
        OracleVerdict::pass(
            "no_degenerate_triangles",
            format!("all {} triangles have non-zero area", total),
        )
    } else {
        OracleVerdict::fail(
            "no_degenerate_triangles",
            format!("{} of {} triangles are degenerate", degenerate, total),
        )
    }
}

/// Check that all stored normals have approximately unit length.
pub fn check_unit_normals(mesh: &RenderMesh) -> OracleVerdict {
    let norms = &mesh.normals;
    let vertex_count = norms.len() / 3;
    let mut bad = 0usize;

    for chunk in norms.chunks(3) {
        if chunk.len() < 3 {
            continue;
        }
        let len = (chunk[0] * chunk[0] + chunk[1] * chunk[1] + chunk[2] * chunk[2]).sqrt();
        if (len - 1.0).abs() > 0.01 {
            bad += 1;
        }
    }

    if bad == 0 {
        OracleVerdict::pass(
            "unit_normals",
            format!("all {} normals are unit length", vertex_count),
        )
    } else {
        OracleVerdict::fail(
            "unit_normals",
            format!("{} of {} normals are not unit length", bad, vertex_count),
        )
    }
}

/// Check that face ranges cover all indices without gaps or overlaps.
pub fn check_face_range_coverage(mesh: &RenderMesh) -> OracleVerdict {
    let ranges = &mesh.face_ranges;
    let total_indices = mesh.indices.len() as u32;

    if ranges.is_empty() {
        return OracleVerdict::fail("face_range_coverage", "no face ranges defined".to_string());
    }

    let mut expected_start = 0u32;
    for (i, fr) in ranges.iter().enumerate() {
        if fr.start_index != expected_start {
            return OracleVerdict::fail(
                "face_range_coverage",
                format!(
                    "gap/overlap at range {}: expected start={}, got start={}",
                    i, expected_start, fr.start_index
                ),
            );
        }
        if fr.end_index <= fr.start_index {
            return OracleVerdict::fail(
                "face_range_coverage",
                format!("empty range at index {}", i),
            );
        }
        expected_start = fr.end_index;
    }

    if expected_start != total_indices {
        return OracleVerdict::fail(
            "face_range_coverage",
            format!(
                "ranges end at {} but mesh has {} indices",
                expected_start, total_indices
            ),
        );
    }

    OracleVerdict::pass(
        "face_range_coverage",
        format!("{} ranges, no gaps", ranges.len()),
    )
}

/// Check that stored normals point outward from the solid.
///
/// Computes the mesh centroid, then for each triangle checks that the stored
/// normal has a positive dot product with the vector from centroid to triangle
/// center. The `convexity_threshold` (0.0–1.0) controls the required fraction
/// of triangles that must pass — set below 1.0 to tolerate minor non-convexity.
pub fn check_outward_normals(mesh: &RenderMesh, convexity_threshold: f64) -> OracleVerdict {
    let verts = &mesh.vertices;
    let norms = &mesh.normals;
    let vertex_count = verts.len() / 3;

    if vertex_count == 0 {
        return OracleVerdict::fail("outward_normals", "empty mesh".to_string());
    }

    // Two-pass approach that works for non-convex solids:
    //
    // 1. Check that geometric normals (cross product) agree with stored normals.
    //    This verifies per-triangle winding consistency.
    //
    // 2. Check that the mesh signed volume is positive. For a closed mesh with
    //    CCW winding convention, positive signed volume means normals point outward.
    //
    // Combined: if all triangles are winding-consistent AND total volume is positive,
    // then all normals point outward. This replaces the centroid-based check which
    // fails for non-convex shapes (e.g., box with tall cylinder boss).

    let mut consistent = 0usize;
    let mut total = 0usize;

    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let i0 = tri[0] as usize * 3;
        let i1 = tri[1] as usize * 3;
        let i2 = tri[2] as usize * 3;

        if i0 + 2 >= verts.len() || i1 + 2 >= verts.len() || i2 + 2 >= verts.len() {
            continue;
        }
        if i0 + 2 >= norms.len() || i1 + 2 >= norms.len() || i2 + 2 >= norms.len() {
            continue;
        }

        // Geometric normal from cross product
        let ax = (verts[i1] - verts[i0]) as f64;
        let ay = (verts[i1 + 1] - verts[i0 + 1]) as f64;
        let az = (verts[i1 + 2] - verts[i0 + 2]) as f64;
        let bx = (verts[i2] - verts[i0]) as f64;
        let by = (verts[i2 + 1] - verts[i0 + 1]) as f64;
        let bz = (verts[i2 + 2] - verts[i0 + 2]) as f64;
        let gnx = ay * bz - az * by;
        let gny = az * bx - ax * bz;
        let gnz = ax * by - ay * bx;

        // Skip degenerate triangles — cross product unreliable for tiny areas
        let area_sq = gnx * gnx + gny * gny + gnz * gnz;
        if area_sq < 1e-20 {
            continue;
        }

        // Average stored normal for the triangle
        let snx = (norms[i0] as f64 + norms[i1] as f64 + norms[i2] as f64) / 3.0;
        let sny = (norms[i0 + 1] as f64 + norms[i1 + 1] as f64 + norms[i2 + 1] as f64) / 3.0;
        let snz = (norms[i0 + 2] as f64 + norms[i1 + 2] as f64 + norms[i2 + 2] as f64) / 3.0;

        let dot = gnx * snx + gny * sny + gnz * snz;
        total += 1;
        if dot > 0.0 {
            consistent += 1;
        }
    }

    if total == 0 {
        return OracleVerdict::fail("outward_normals", "no valid triangles".to_string());
    }

    // Check signed volume: positive means outward normals (CCW convention)
    let signed_vol = crate::helpers::mesh_signed_volume(mesh);

    let outward = if signed_vol > 0.0 {
        // Positive volume: triangles with consistent winding have outward normals
        consistent
    } else if signed_vol < 0.0 {
        // Negative volume: triangles with INCONSISTENT winding have outward normals
        total - consistent
    } else {
        // Zero volume (degenerate): fall back to counting consistent
        consistent
    };

    let ratio = outward as f64 / total as f64;
    if ratio >= convexity_threshold {
        OracleVerdict::pass_val(
            "outward_normals",
            format!(
                "{} of {} triangles ({:.1}%) have outward normals",
                outward,
                total,
                ratio * 100.0
            ),
            ratio,
        )
    } else {
        OracleVerdict::fail_val(
            "outward_normals",
            format!(
                "only {} of {} triangles ({:.1}%) have outward normals (need {:.0}%)",
                outward,
                total,
                ratio * 100.0,
                convexity_threshold * 100.0,
            ),
            ratio,
        )
    }
}

/// Check that all index values are within bounds.
pub fn check_valid_indices(mesh: &RenderMesh) -> OracleVerdict {
    let vertex_count = mesh.vertices.len() / 3;
    let mut bad = Vec::new();

    for (i, &idx) in mesh.indices.iter().enumerate() {
        if idx as usize >= vertex_count {
            bad.push((i, idx));
        }
    }

    if bad.is_empty() {
        OracleVerdict::pass("valid_indices", format!("all indices < {}", vertex_count))
    } else {
        OracleVerdict::fail(
            "valid_indices",
            format!(
                "{} out-of-bounds indices (vertex_count={}): {:?}",
                bad.len(),
                vertex_count,
                &bad[..bad.len().min(5)]
            ),
        )
    }
}

/// Check that the mesh bounding box falls within expected bounds.
pub fn check_bounding_box(
    mesh: &RenderMesh,
    expected_min: [f32; 3],
    expected_max: [f32; 3],
    tolerance: f32,
) -> OracleVerdict {
    let (actual_min, actual_max) = crate::helpers::mesh_bounding_box(mesh);

    for i in 0..3 {
        if (actual_min[i] - expected_min[i]).abs() > tolerance {
            return OracleVerdict::fail(
                "bounding_box",
                format!(
                    "min[{}]: expected {:.3}, got {:.3} (tol={})",
                    i, expected_min[i], actual_min[i], tolerance
                ),
            );
        }
        if (actual_max[i] - expected_max[i]).abs() > tolerance {
            return OracleVerdict::fail(
                "bounding_box",
                format!(
                    "max[{}]: expected {:.3}, got {:.3} (tol={})",
                    i, expected_max[i], actual_max[i], tolerance
                ),
            );
        }
    }

    OracleVerdict::pass(
        "bounding_box",
        format!(
            "({:.1},{:.1},{:.1}) -> ({:.1},{:.1},{:.1})",
            actual_min[0],
            actual_min[1],
            actual_min[2],
            actual_max[0],
            actual_max[1],
            actual_max[2],
        ),
    )
}

// ── Provenance Oracles ──────────────────────────────────────────────────────

/// Check that a specific role exists in the OpResult provenance with at least min_count entries.
pub fn check_role_exists(op: &OpResult, role: &Role, min_count: usize) -> OracleVerdict {
    let matching: Vec<_> = op
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| r == role)
        .collect();

    if matching.len() >= min_count {
        OracleVerdict::pass(
            "role_exists",
            format!(
                "role {:?} found {} times (need >= {})",
                role,
                matching.len(),
                min_count
            ),
        )
    } else {
        OracleVerdict::fail(
            "role_exists",
            format!(
                "role {:?} found {} times, need >= {}. Available roles: {:?}",
                role,
                matching.len(),
                min_count,
                op.provenance
                    .role_assignments
                    .iter()
                    .map(|(_, r)| format!("{:?}", r))
                    .collect::<Vec<_>>()
            ),
        )
    }
}

/// Check that the mesh has positive signed volume (correct outward winding).
///
/// A correctly-oriented closed mesh with outward normals produces positive
/// signed volume via the divergence theorem. Negative signed volume indicates
/// inverted normals or inside-out winding.
pub fn check_positive_signed_volume(mesh: &RenderMesh) -> OracleVerdict {
    let signed_vol = crate::helpers::mesh_signed_volume(mesh);
    if signed_vol > 0.0 {
        OracleVerdict::pass_val(
            "positive_signed_volume",
            format!("signed volume = {:.6e}", signed_vol),
            signed_vol,
        )
    } else {
        OracleVerdict::fail_val(
            "positive_signed_volume",
            format!("signed volume = {:.6e} (should be > 0)", signed_vol),
            signed_vol,
        )
    }
}

// ── Shape Oracles ─────────────────────────────────────────────────────────

/// Check whether a mesh has collapsed to its AABB (axis-aligned bounding box).
///
/// When the kernel reconstructs a non-rectangular operand from its AABB, the
/// resulting mesh has all vertices on the 6 bounding-box faces. This oracle
/// detects that degeneration: if every unique vertex lies on an AABB face and
/// the mesh has more than 24 unique positions (ruling out legitimate small
/// boxes), it fails.
pub fn check_aabb_collapse(mesh: &RenderMesh) -> OracleVerdict {
    use std::collections::HashSet;

    if mesh.vertices.is_empty() {
        return OracleVerdict::pass("aabb_collapse", "empty mesh — skipped".to_string());
    }

    let (bb_min, bb_max) = crate::helpers::mesh_bounding_box(mesh);

    // Scale-adaptive tolerance: max_abs * 1e-4, floor 1e-8
    let max_abs = bb_min
        .iter()
        .chain(bb_max.iter())
        .map(|v| v.abs())
        .fold(0.0_f32, f32::max);
    let tol = (max_abs * 1e-4).max(1e-8);

    // Collect unique vertex positions (quantized to tolerance)
    let inv = 1.0 / tol;
    let quantize = |v: f32| -> i64 { (v as f64 * inv as f64).round() as i64 };

    let mut unique_positions: HashSet<(i64, i64, i64)> = HashSet::new();
    for chunk in mesh.vertices.chunks(3) {
        if chunk.len() < 3 {
            continue;
        }
        unique_positions.insert((quantize(chunk[0]), quantize(chunk[1]), quantize(chunk[2])));
    }

    let total_unique = unique_positions.len();

    // Small meshes (≤24 unique positions) could be legitimate tessellated boxes
    if total_unique <= 24 {
        return OracleVerdict::pass(
            "aabb_collapse",
            format!(
                "{} unique positions ≤ 24 — too small to detect collapse",
                total_unique
            ),
        );
    }

    // Check how many unique positions are NOT on any AABB face
    let on_face = |x: f32, y: f32, z: f32| -> bool {
        (x - bb_min[0]).abs() < tol
            || (x - bb_max[0]).abs() < tol
            || (y - bb_min[1]).abs() < tol
            || (y - bb_max[1]).abs() < tol
            || (z - bb_min[2]).abs() < tol
            || (z - bb_max[2]).abs() < tol
    };

    let mut non_aabb_count = 0usize;
    for chunk in mesh.vertices.chunks(3) {
        if chunk.len() < 3 {
            continue;
        }
        let key = (quantize(chunk[0]), quantize(chunk[1]), quantize(chunk[2]));
        // Only count each unique position once — remove after checking
        if unique_positions.remove(&key) && !on_face(chunk[0], chunk[1], chunk[2]) {
            non_aabb_count += 1;
        }
    }

    if non_aabb_count == 0 {
        // All vertices are on AABB faces, but this can be legitimate for
        // prismatic through-extrusions where all z-values are at z_min/z_max
        // and some vertices happen to touch x/y AABB faces (e.g., gear tips).
        //
        // Distinguish real AABB collapse from legitimate geometry by counting
        // how many AABB faces each vertex touches. A real AABB-collapsed box
        // has every vertex on ≥2 AABB faces simultaneously (edges and corners
        // of the box). A prismatic solid has most vertices on only 1 AABB face
        // (a z-face), with few vertices touching x/y faces.
        let mut multi_face_count = 0usize;
        for chunk in mesh.vertices.chunks(3) {
            if chunk.len() < 3 {
                continue;
            }
            let mut face_count = 0u32;
            if (chunk[0] - bb_min[0]).abs() < tol {
                face_count += 1;
            }
            if (chunk[0] - bb_max[0]).abs() < tol {
                face_count += 1;
            }
            if (chunk[1] - bb_min[1]).abs() < tol {
                face_count += 1;
            }
            if (chunk[1] - bb_max[1]).abs() < tol {
                face_count += 1;
            }
            if (chunk[2] - bb_min[2]).abs() < tol {
                face_count += 1;
            }
            if (chunk[2] - bb_max[2]).abs() < tol {
                face_count += 1;
            }
            if face_count >= 2 {
                multi_face_count += 1;
            }
        }
        let total_verts = mesh.vertices.len() / 3;
        // A true AABB-collapse has >50% of vertices on ≥2 AABB faces.
        // A prismatic solid (gear extrusion) has most vertices on only 1 face.
        let multi_face_ratio = if total_verts > 0 {
            multi_face_count as f64 / total_verts as f64
        } else {
            0.0
        };
        if multi_face_ratio <= 0.5 {
            return OracleVerdict::pass(
                "aabb_collapse",
                format!(
                    "all {} unique vertices on AABB faces but only {:.0}% on ≥2 faces — prismatic solid, not collapse",
                    total_unique, multi_face_ratio * 100.0
                ),
            );
        }
        OracleVerdict::fail(
            "aabb_collapse",
            format!(
                "all {} unique vertices lie on AABB faces ({:.0}% on ≥2 faces) — mesh collapsed to bounding box",
                total_unique, multi_face_ratio * 100.0
            ),
        )
    } else {
        OracleVerdict::pass(
            "aabb_collapse",
            format!(
                "{} of {} unique vertices are interior to AABB",
                non_aabb_count, total_unique
            ),
        )
    }
}

// ── Assay Oracles ─────────────────────────────────────────────────────────

/// Check that the mesh has at least the expected minimum number of triangles
/// based on operation and profile types.
///
/// Minimum triangle counts by profile:
/// - rectangle: 12 (6 faces × 2 tris)
/// - circle: 32 (polygon approximation needs many tris)
/// - gear: 96 (many teeth → many faces)
///
/// Revolve multiplier: 3× (rotational sweep adds lateral faces).
/// Takes the max across all operations (booleans don't reduce complexity).
pub fn check_minimum_triangle_count(
    mesh: &RenderMesh,
    operations: &[(String, String)], // (kind, profile_type) pairs
) -> OracleVerdict {
    let tri_count = mesh.indices.len() / 3;

    // For multi-operation cases, subsequent boolean operations (especially cuts)
    // can remove significant geometry. The safe minimum is the FIRST operation's
    // base count — the initial boss creates the base solid, and subsequent
    // booleans can only reduce it. For single-operation cases, use that op's count.
    let per_op_mins: Vec<usize> = operations
        .iter()
        .map(|(kind, profile)| {
            let base = match profile.as_str() {
                "rectangle" => 12,
                "circle" => 32,
                "gear" => 96,
                _ => 12,
            };
            let multiplier = if kind == "revolve" { 3 } else { 1 };
            base * multiplier
        })
        .collect();

    let expected_min: usize = if per_op_mins.len() <= 1 {
        // Single operation: use its full minimum
        per_op_mins.first().copied().unwrap_or(12)
    } else {
        // Multi-operation: use the first operation's minimum (boss creates base
        // solid; subsequent cuts/booleans can reduce geometry significantly)
        per_op_mins[0]
    };

    if tri_count >= expected_min {
        OracleVerdict::pass(
            "minimum_triangle_count",
            format!("{} triangles >= minimum {}", tri_count, expected_min),
        )
    } else {
        OracleVerdict::fail(
            "minimum_triangle_count",
            format!(
                "{} triangles < expected minimum {} for operations {:?}",
                tri_count, expected_min, operations
            ),
        )
    }
}

/// Check that the mesh volume is within reasonable magnitude bounds given the scale.
///
/// Uses very loose bounds (8 orders of magnitude) to only catch extreme degeneration:
/// - min_vol = scale³ × 1e-8
/// - max_vol = scale³ × 1e8
///
/// This won't false-positive on unusual aspect ratios but catches things like
/// a volume of 1e-20 for a 100m-scale object.
pub fn check_volume_magnitude(mesh: &RenderMesh, scale: f64) -> OracleVerdict {
    let vol = crate::helpers::mesh_signed_volume(mesh).abs();
    let scale_cubed = scale * scale * scale;
    let min_vol = scale_cubed * 1e-8;
    let max_vol = scale_cubed * 1e8;

    if vol >= min_vol && vol <= max_vol {
        OracleVerdict::pass(
            "volume_magnitude",
            format!(
                "volume {:.6e} within [{:.6e}, {:.6e}]",
                vol, min_vol, max_vol
            ),
        )
    } else {
        OracleVerdict::fail(
            "volume_magnitude",
            format!(
                "volume {:.6e} outside [{:.6e}, {:.6e}] for scale {:.6e}",
                vol, min_vol, max_vol, scale
            ),
        )
    }
}

// ── Mesh Euler Characteristic ────────────────────────────────────────────────

/// Check the mesh Euler characteristic χ = V - E + F against an expected value.
///
/// Uses position-quantized vertex/edge counting on the triangle mesh (same
/// quantization as `check_watertight_mesh`). For a genus-g closed surface,
/// χ = 2 - 2g. A simple solid has χ=2; a solid with one through-hole has χ=0.
///
/// Interior/residual faces from incomplete boolean operations shift χ away
/// from the expected value, making this oracle effective at catching them.
pub fn check_mesh_euler_characteristic(mesh: &RenderMesh, expected_chi: i64) -> OracleVerdict {
    use std::collections::HashSet;

    if mesh.vertices.is_empty() || mesh.indices.is_empty() {
        return OracleVerdict::pass(
            "mesh_euler_characteristic",
            "skipped: empty mesh".to_string(),
        );
    }

    // Scale-adaptive quantization (matches check_watertight_mesh)
    let max_abs = mesh
        .vertices
        .iter()
        .map(|v| v.abs())
        .fold(0.0_f32, f32::max);
    let grid_size = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid_size;

    let quantize = |v: f32| -> i64 { (v as f64 * inv_grid).round() as i64 };

    let vert_key = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize * 3;
        (
            quantize(mesh.vertices[i]),
            quantize(mesh.vertices[i + 1]),
            quantize(mesh.vertices[i + 2]),
        )
    };

    // Collect unique vertices
    let mut unique_verts: HashSet<(i64, i64, i64)> = HashSet::new();
    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        unique_verts.insert(vert_key(tri[0]));
        unique_verts.insert(vert_key(tri[1]));
        unique_verts.insert(vert_key(tri[2]));
    }

    // Collect unique edges (sorted pairs)
    type PosEdge = ((i64, i64, i64), (i64, i64, i64));
    fn make_edge(a: (i64, i64, i64), b: (i64, i64, i64)) -> PosEdge {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    let mut unique_edges: HashSet<PosEdge> = HashSet::new();
    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let va = vert_key(tri[0]);
        let vb = vert_key(tri[1]);
        let vc = vert_key(tri[2]);
        unique_edges.insert(make_edge(va, vb));
        unique_edges.insert(make_edge(vb, vc));
        unique_edges.insert(make_edge(vc, va));
    }

    let v = unique_verts.len() as i64;
    let e = unique_edges.len() as i64;
    let f = (mesh.indices.len() / 3) as i64;
    let chi = v - e + f;

    if chi == expected_chi {
        OracleVerdict::pass_val(
            "mesh_euler_characteristic",
            format!(
                "V({}) - E({}) + F({}) = {} (expected {})",
                v, e, f, chi, expected_chi
            ),
            chi as f64,
        )
    } else {
        OracleVerdict::fail_val(
            "mesh_euler_characteristic",
            format!(
                "V({}) - E({}) + F({}) = {} (expected {})",
                v, e, f, chi, expected_chi
            ),
            chi as f64,
        )
    }
}

// ── Inter-Face Self-Intersection Oracle ──────────────────────────────────────

/// Check that triangles from different B-Rep faces do not penetrate each other.
///
/// Uses Möller's separating-axis triangle-triangle intersection test (1997)
/// with AABB broad-phase per face and a penetration depth threshold to reject
/// grazing contacts at shared boundaries. See specs/inter_face_self_intersection_oracle.md.
pub fn check_no_self_intersection(mesh: &RenderMesh) -> OracleVerdict {
    if mesh.face_ranges.len() <= 1 || mesh.indices.is_empty() {
        return OracleVerdict::pass("no_self_intersection", "skipped: ≤1 face range".to_string());
    }

    // Scale-adaptive quantization for shared-vertex detection
    let max_abs = mesh
        .vertices
        .iter()
        .map(|v| v.abs())
        .fold(0.0_f32, f32::max);
    let grid_size = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid_size;
    let quantize = |v: f32| -> i64 { (v as f64 * inv_grid).round() as i64 };

    let vert_pos = |idx: u32| -> [f64; 3] {
        let i = idx as usize * 3;
        [
            mesh.vertices[i] as f64,
            mesh.vertices[i + 1] as f64,
            mesh.vertices[i + 2] as f64,
        ]
    };

    let vert_quant = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize * 3;
        (
            quantize(mesh.vertices[i]),
            quantize(mesh.vertices[i + 1]),
            quantize(mesh.vertices[i + 2]),
        )
    };

    // Penetration depth threshold for grazing rejection
    let depth_threshold = (max_abs as f64 * TAU_WELD_MAX).max(TAU_COINCIDENT);

    // Partition triangles into per-face groups and compute AABBs
    struct FaceGroup {
        tri_indices: Vec<[u32; 3]>, // index triples
        aabb_min: [f64; 3],
        aabb_max: [f64; 3],
    }

    let mut faces: Vec<FaceGroup> = Vec::with_capacity(mesh.face_ranges.len());

    for fr in &mesh.face_ranges {
        let start = fr.start_index as usize;
        let end = fr.end_index as usize;
        let mut aabb_min = [f64::MAX; 3];
        let mut aabb_max = [f64::MIN; 3];
        let mut tris = Vec::new();

        for tri in mesh.indices[start..end].chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            tris.push([tri[0], tri[1], tri[2]]);
            for &idx in tri {
                let p = vert_pos(idx);
                for d in 0..3 {
                    aabb_min[d] = aabb_min[d].min(p[d]);
                    aabb_max[d] = aabb_max[d].max(p[d]);
                }
            }
        }

        faces.push(FaceGroup {
            tri_indices: tris,
            aabb_min,
            aabb_max,
        });
    }

    let aabbs_overlap = |a: &FaceGroup, b: &FaceGroup| -> bool {
        for d in 0..3 {
            if a.aabb_max[d] < b.aabb_min[d] || b.aabb_max[d] < a.aabb_min[d] {
                return false;
            }
        }
        true
    };

    let mut violations = 0usize;
    let mut violation_pairs: Vec<(usize, usize)> = Vec::new();
    const MAX_VIOLATIONS: usize = 10;

    'outer: for i in 0..faces.len() {
        for j in (i + 1)..faces.len() {
            if !aabbs_overlap(&faces[i], &faces[j]) {
                continue;
            }

            // Test all triangle pairs between face i and face j
            for tri_a in &faces[i].tri_indices {
                for tri_b in &faces[j].tri_indices {
                    // Skip if shared edge (≥2 common quantized vertices)
                    let qa: [(i64, i64, i64); 3] = [
                        vert_quant(tri_a[0]),
                        vert_quant(tri_a[1]),
                        vert_quant(tri_a[2]),
                    ];
                    let qb: [(i64, i64, i64); 3] = [
                        vert_quant(tri_b[0]),
                        vert_quant(tri_b[1]),
                        vert_quant(tri_b[2]),
                    ];
                    let shared = qa.iter().filter(|v| qb.contains(v)).count();
                    if shared >= 2 {
                        continue;
                    }

                    let pa: [[f64; 3]; 3] =
                        [vert_pos(tri_a[0]), vert_pos(tri_a[1]), vert_pos(tri_a[2])];
                    let pb: [[f64; 3]; 3] =
                        [vert_pos(tri_b[0]), vert_pos(tri_b[1]), vert_pos(tri_b[2])];

                    if triangles_intersect(&pa, &pb, depth_threshold) {
                        violations += 1;
                        if violation_pairs.len() < MAX_VIOLATIONS {
                            violation_pairs.push((i, j));
                        }
                        if violations >= MAX_VIOLATIONS {
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    if violations == 0 {
        OracleVerdict::pass(
            "no_self_intersection",
            format!(
                "no inter-face triangle penetrations ({} face pairs tested)",
                faces.len() * (faces.len() - 1) / 2
            ),
        )
    } else {
        let pairs_str: Vec<String> = violation_pairs
            .iter()
            .take(5)
            .map(|(a, b)| format!("({},{})", a, b))
            .collect();
        OracleVerdict::fail(
            "no_self_intersection",
            format!(
                "{} inter-face triangle penetrations, face pairs: {}{}",
                violations,
                pairs_str.join(", "),
                if violations > 5 { ", ..." } else { "" }
            ),
        )
    }
}

/// Möller-style triangle-triangle intersection test using separating axes.
///
/// Returns true if the two triangles geometrically penetrate each other
/// beyond `depth_threshold`. Coplanar triangles are treated as non-intersecting.
///
/// Reference: Möller, "A Fast Triangle-Triangle Intersection Test", JGT 2(2), 1997.
fn triangles_intersect(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3], depth_threshold: f64) -> bool {
    // Compute plane of triangle A: normal = (a1-a0) × (a2-a0)
    let cross = |u: [f64; 3], v: [f64; 3]| -> [f64; 3] {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let sub = |p: [f64; 3], q: [f64; 3]| -> [f64; 3] { [p[0] - q[0], p[1] - q[1], p[2] - q[2]] };
    let dot = |u: [f64; 3], v: [f64; 3]| -> f64 { u[0] * v[0] + u[1] * v[1] + u[2] * v[2] };

    let na = cross(sub(a[1], a[0]), sub(a[2], a[0]));
    let da = dot(na, a[0]);

    // Signed distances of B's vertices from A's plane
    let db: [f64; 3] = [dot(na, b[0]) - da, dot(na, b[1]) - da, dot(na, b[2]) - da];

    // All B on one side of A's plane?
    if (db[0] > depth_threshold && db[1] > depth_threshold && db[2] > depth_threshold)
        || (db[0] < -depth_threshold && db[1] < -depth_threshold && db[2] < -depth_threshold)
    {
        return false;
    }

    let nb = cross(sub(b[1], b[0]), sub(b[2], b[0]));
    let d_b_plane = dot(nb, b[0]);

    // Signed distances of A's vertices from B's plane
    let d_a: [f64; 3] = [
        dot(nb, a[0]) - d_b_plane,
        dot(nb, a[1]) - d_b_plane,
        dot(nb, a[2]) - d_b_plane,
    ];

    // All A on one side of B's plane?
    if (d_a[0] > depth_threshold && d_a[1] > depth_threshold && d_a[2] > depth_threshold)
        || (d_a[0] < -depth_threshold && d_a[1] < -depth_threshold && d_a[2] < -depth_threshold)
    {
        return false;
    }

    // Intersection line direction
    let dir = cross(na, nb);
    let dir_len_sq = dot(dir, dir);
    if dir_len_sq < TAU_NORMALIZE_SQ {
        // Planes are (near-)parallel / coplanar — treat as non-intersecting
        return false;
    }

    // Project vertices onto the intersection line and compute intervals
    // Pick the axis with largest |dir| component for projection
    let abs_dir = [dir[0].abs(), dir[1].abs(), dir[2].abs()];
    let axis = if abs_dir[0] >= abs_dir[1] && abs_dir[0] >= abs_dir[2] {
        0
    } else if abs_dir[1] >= abs_dir[2] {
        1
    } else {
        2
    };

    let proj_a: [f64; 3] = [a[0][axis], a[1][axis], a[2][axis]];
    let proj_b: [f64; 3] = [b[0][axis], b[1][axis], b[2][axis]];

    // Compute interval for triangle A on the intersection line
    // Find the edge(s) that cross B's plane (d_a sign changes)
    let interval_a = compute_interval(&proj_a, &d_a);
    let interval_b = compute_interval(&proj_b, &db);

    match (interval_a, interval_b) {
        (Some((a_min, a_max)), Some((b_min, b_max))) => {
            // Check if intervals overlap
            a_min < b_max && b_min < a_max
        }
        _ => false,
    }
}

/// Compute the interval [t_min, t_max] where a triangle's edges cross the
/// opposing plane. `proj` are vertex projections onto the intersection line axis,
/// `dists` are signed distances from the opposing plane.
fn compute_interval(proj: &[f64; 3], dists: &[f64; 3]) -> Option<(f64, f64)> {
    let mut ts = Vec::with_capacity(2);

    // For each edge, if the endpoints straddle (or touch) the plane, compute crossing parameter
    for &(i, j) in &[(0usize, 1usize), (1, 2), (2, 0)] {
        let di = dists[i];
        let dj = dists[j];
        if (di > 0.0 && dj < 0.0) || (di < 0.0 && dj > 0.0) {
            // Edge crosses the plane
            let t = proj[i] + (proj[j] - proj[i]) * di / (di - dj);
            ts.push(t);
        } else if di.abs() < TAU_NORMALIZE_SQ {
            // Vertex i is on the plane
            ts.push(proj[i]);
        }
    }
    // Also check if vertex 2 is on the plane (handled in edge (2,0) start but
    // not if vertex 0 was also zero)
    if ts.len() < 2
        && dists[2].abs() < TAU_NORMALIZE_SQ
        && (ts.is_empty() || (ts[0] - proj[2]).abs() > TAU_NORMALIZE_SQ)
    {
        ts.push(proj[2]);
    }

    if ts.len() >= 2 {
        let (a, b) = (ts[0], ts[1]);
        Some(if a <= b { (a, b) } else { (b, a) })
    } else if ts.len() == 1 {
        // Degenerate: single point contact — not a real intersection
        None
    } else {
        None
    }
}

// ── Composite ───────────────────────────────────────────────────────────────

/// Run all applicable checks on a solid + mesh + op_result combination.
pub fn run_all_mesh_checks(mesh: &RenderMesh) -> Vec<OracleVerdict> {
    vec![
        check_watertight_mesh(mesh),
        check_consistent_normals(mesh),
        check_no_degenerate_triangles(mesh),
        check_unit_normals(mesh),
        check_face_range_coverage(mesh),
        check_valid_indices(mesh),
        check_outward_normals(mesh, 0.95),
        check_positive_signed_volume(mesh),
        check_no_self_intersection(mesh),
    ]
}

/// Run topology checks on a solid.
pub fn run_topology_checks(
    introspect: &dyn KernelIntrospect,
    solid: &KernelSolidHandle,
) -> Vec<OracleVerdict> {
    vec![
        check_euler_formula(introspect, solid),
        check_manifold_edges(introspect, solid),
        check_face_validity(introspect, solid),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::types::{FaceRange, RenderMesh};
    use kernel::KernelId;

    /// Build a unit cube mesh (8 corners, 12 triangles, per-face vertices).
    /// All vertices lie exactly on the AABB faces [0,0,0]→[1,1,1].
    fn make_unit_cube_mesh() -> RenderMesh {
        // 6 faces × 4 verts = 24 verts, 6 faces × 2 tris = 12 tris
        // But we need >24 unique positions to trigger the oracle.
        // Use a denser tessellation: split each face into a 2×2 grid (9 verts, 8 tris per face).
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        let mut face_ranges = Vec::new();

        // Helper: add a face as a 2×2 grid of quads (each quad = 2 tris)
        let mut add_face = |corners: [[f32; 3]; 4], normal: [f32; 3]| {
            // corners: [bl, br, tr, tl] — bottom-left, bottom-right, top-right, top-left
            let start_idx = (vertices.len() / 3) as u32;
            let idx_start = indices.len() as u32;

            // 3×3 grid of vertices
            for iy in 0..3 {
                for ix in 0..3 {
                    let u = ix as f32 / 2.0;
                    let v = iy as f32 / 2.0;
                    // Bilinear interpolation
                    let x = corners[0][0] * (1.0 - u) * (1.0 - v)
                        + corners[1][0] * u * (1.0 - v)
                        + corners[2][0] * u * v
                        + corners[3][0] * (1.0 - u) * v;
                    let y = corners[0][1] * (1.0 - u) * (1.0 - v)
                        + corners[1][1] * u * (1.0 - v)
                        + corners[2][1] * u * v
                        + corners[3][1] * (1.0 - u) * v;
                    let z = corners[0][2] * (1.0 - u) * (1.0 - v)
                        + corners[1][2] * u * (1.0 - v)
                        + corners[2][2] * u * v
                        + corners[3][2] * (1.0 - u) * v;
                    vertices.extend_from_slice(&[x, y, z]);
                    normals.extend_from_slice(&normal);
                }
            }

            // 2×2 grid of quads → 8 triangles
            for iy in 0..2u32 {
                for ix in 0..2u32 {
                    let bl = start_idx + iy * 3 + ix;
                    let br = bl + 1;
                    let tl = bl + 3;
                    let tr = tl + 1;
                    indices.extend_from_slice(&[bl, br, tr]);
                    indices.extend_from_slice(&[bl, tr, tl]);
                }
            }

            let idx_end = indices.len() as u32;
            face_ranges.push(FaceRange {
                face_id: KernelId(face_ranges.len() as u64),
                start_index: idx_start,
                end_index: idx_end,
            });
        };

        // 6 faces of unit cube [0,0,0]→[1,1,1]
        // Front (z=1)
        add_face(
            [
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
            [0.0, 0.0, 1.0],
        );
        // Back (z=0)
        add_face(
            [
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            [0.0, 0.0, -1.0],
        );
        // Right (x=1)
        add_face(
            [
                [1.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, 1.0, 1.0],
            ],
            [1.0, 0.0, 0.0],
        );
        // Left (x=0)
        add_face(
            [
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
                [0.0, 1.0, 0.0],
            ],
            [-1.0, 0.0, 0.0],
        );
        // Top (y=1)
        add_face(
            [
                [0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            [0.0, 1.0, 0.0],
        );
        // Bottom (y=0)
        add_face(
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
            [0.0, -1.0, 0.0],
        );

        RenderMesh {
            vertices,
            normals,
            indices,
            face_ranges,
        }
    }

    #[test]
    fn aabb_collapse_detects_pure_box() {
        let mesh = make_unit_cube_mesh();
        let unique_count = {
            let mut s = std::collections::HashSet::new();
            for c in mesh.vertices.chunks(3) {
                s.insert((
                    (c[0] * 1e6) as i64,
                    (c[1] * 1e6) as i64,
                    (c[2] * 1e6) as i64,
                ));
            }
            s.len()
        };
        // 3×3 grid per face × 6 faces, but some shared on edges/corners
        // Should be >24 unique positions
        assert!(
            unique_count > 24,
            "test mesh needs >24 unique positions, got {}",
            unique_count
        );

        let verdict = check_aabb_collapse(&mesh);
        assert!(
            !verdict.passed,
            "should detect AABB collapse: {}",
            verdict.detail
        );
    }

    #[test]
    fn aabb_collapse_passes_non_box() {
        // Start with a unit cube mesh, then add a vertex interior to the AABB
        let mut mesh = make_unit_cube_mesh();
        // Add an extra triangle with a vertex at (0.5, 0.5, 0.5) — interior
        let base = (mesh.vertices.len() / 3) as u32;
        mesh.vertices
            .extend_from_slice(&[0.5, 0.5, 0.5, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        mesh.normals
            .extend_from_slice(&[0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]);
        let idx_start = mesh.indices.len() as u32;
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
        mesh.face_ranges.push(FaceRange {
            face_id: KernelId(mesh.face_ranges.len() as u64),
            start_index: idx_start,
            end_index: mesh.indices.len() as u32,
        });

        let verdict = check_aabb_collapse(&mesh);
        assert!(
            verdict.passed,
            "should pass with interior vertex: {}",
            verdict.detail
        );
    }

    #[test]
    fn aabb_collapse_skips_small_mesh() {
        // Simple box with 8 vertices (≤24 unique) — should pass even if all on AABB
        let vertices = vec![
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0,
            1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0,
        ];
        let normals = vec![0.0; vertices.len()]; // dummy normals
        let indices = vec![
            0, 1, 2, 0, 2, 3, // front
            4, 5, 6, 4, 6, 7, // back
            0, 1, 5, 0, 5, 4, // bottom
            3, 2, 6, 3, 6, 7, // top
            0, 3, 7, 0, 7, 4, // left
            1, 2, 6, 1, 6, 5, // right
        ];
        let mesh = RenderMesh {
            vertices,
            normals,
            indices,
            face_ranges: vec![FaceRange {
                face_id: KernelId(0),
                start_index: 0,
                end_index: 36,
            }],
        };

        let verdict = check_aabb_collapse(&mesh);
        assert!(
            verdict.passed,
            "small mesh (≤24 unique verts) should pass: {}",
            verdict.detail
        );
    }

    /// Helper: make a simple mesh with N triangles (degenerate but with valid indices).
    fn make_mesh_with_n_tris(n: usize) -> RenderMesh {
        // Each triangle gets 3 unique vertices to reach the desired count
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        for i in 0..n {
            let base = (i * 3) as u32;
            let z = i as f32 * 0.1;
            vertices.extend_from_slice(&[0.0, 0.0, z, 1.0, 0.0, z, 0.0, 1.0, z]);
            normals.extend_from_slice(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
            indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
        RenderMesh {
            vertices,
            normals,
            indices,
            face_ranges: vec![FaceRange {
                face_id: KernelId(0),
                start_index: 0,
                end_index: (n * 3) as u32,
            }],
        }
    }

    #[test]
    fn minimum_triangle_count_pass_rect_extrude() {
        let mesh = make_mesh_with_n_tris(48);
        let ops = vec![("extrude".to_string(), "rectangle".to_string())];
        let verdict = check_minimum_triangle_count(&mesh, &ops);
        assert!(
            verdict.passed,
            "48 tris should pass rect extrude (min 12): {}",
            verdict.detail
        );
    }

    #[test]
    fn minimum_triangle_count_fail_circle_revolve() {
        let mesh = make_mesh_with_n_tris(12);
        let ops = vec![("revolve".to_string(), "circle".to_string())];
        let verdict = check_minimum_triangle_count(&mesh, &ops);
        assert!(
            !verdict.passed,
            "12 tris should fail circle revolve (min 96): {}",
            verdict.detail
        );
    }

    #[test]
    fn minimum_triangle_count_uses_first_op_for_multi_op() {
        let mesh = make_mesh_with_n_tris(50);
        // rect extrude (min 12) + gear extrude (min 96)
        // Multi-op: use first op's minimum (12), not max (96).
        // Subsequent booleans (cuts) can reduce geometry below the second op's base.
        let ops = vec![
            ("extrude".to_string(), "rectangle".to_string()),
            ("extrude".to_string(), "gear".to_string()),
        ];
        let verdict = check_minimum_triangle_count(&mesh, &ops);
        assert!(
            verdict.passed,
            "50 tris should pass multi-op (first op min 12): {}",
            verdict.detail
        );
    }

    #[test]
    fn volume_magnitude_pass_unit_cube() {
        // Unit cube at scale 1: volume ~ 0.5 (from simple mesh), well within bounds
        let mesh = make_mesh_with_n_tris(12);
        // Volume of this mesh is non-trivial positive; scale=1 → bounds [1e-8, 1e8]
        let verdict = check_volume_magnitude(&mesh, 1.0);
        // The mesh_signed_volume of make_mesh_with_n_tris is ~0.5 per tri pair
        // At scale 1, bounds are [1e-8, 1e8], so any reasonable volume passes
        assert!(
            verdict.passed,
            "unit-scale mesh should pass magnitude check: {}",
            verdict.detail
        );
    }

    #[test]
    fn volume_magnitude_fail_tiny_at_large_scale() {
        // Tiny mesh (volume ~ 0.5) at scale 100 → expected vol ~ 1e6, actual ~ 0.5
        // bounds: [100³ × 1e-8, 100³ × 1e8] = [1e-2, 1e14]
        // 0.5 < 1e-2 should barely pass... let's use a really extreme case
        // Scale 1e4 → bounds [1e4³ × 1e-8, ...] = [1e4, ...]
        // Our mesh vol is ~0.5, which is < 1e4 → fail
        let mesh = make_mesh_with_n_tris(12);
        let verdict = check_volume_magnitude(&mesh, 1e4);
        assert!(
            !verdict.passed,
            "tiny mesh at scale 1e4 should fail: {}",
            verdict.detail
        );
    }

    /// Test 2: Cut operations should have a reduced minimum triangle count.
    ///
    /// A cut (subtract) operation removes material from a boss, so the resulting
    /// mesh can have fewer triangles than the profile's base minimum. For example,
    /// a rectangle-profile cut from a rectangle-profile boss yields a solid whose
    /// triangle count depends on the intersection geometry, not the cut profile.
    ///
    /// The oracle should accept a lower triangle count when the last operation is
    /// a cut. Currently, `check_minimum_triangle_count` does not receive cut info
    /// and applies the full base minimum regardless.
    #[test]
    fn minimum_triangle_count_reduced_for_cut_operations() {
        // Scenario: boss extrude (rectangle, min=12) then cut extrude (gear, min=96)
        // After the cut, the solid may have only ~20 triangles if the gear cut
        // removes most of the boss. The oracle should NOT demand 96 triangles
        // when the gear operation is a cut — cuts reduce geometry.
        //
        // Current behavior: ops = [("extrude", "rectangle"), ("extrude", "gear")]
        // takes max(12, 96) = 96 regardless of cut/boss distinction.
        //
        // Expected behavior after fix: the oracle receives cut info and applies
        // a reduced minimum (e.g., base/2 or the boss's minimum) for cut ops.
        let mesh = make_mesh_with_n_tris(20);

        // When the second operation is a cut, 20 triangles should be acceptable.
        // The oracle needs to know that "gear" here is a cut, not a boss.
        // After the fix, this function signature will accept (kind, profile, is_cut)
        // tuples and reduce the minimum for cuts.
        //
        // For now, we test with the current (kind, profile) signature:
        // the test SHOULD pass (20 >= reduced minimum for a cut), but currently
        // it FAILS because the oracle demands 96 for gear regardless.
        let ops = vec![
            ("extrude".to_string(), "rectangle".to_string()),
            ("extrude".to_string(), "gear".to_string()),
        ];
        let verdict = check_minimum_triangle_count(&mesh, &ops);

        // This assertion currently fails: oracle requires 96, but we have 20.
        // After the fix, cut operations should reduce the minimum, making this pass.
        assert!(
            verdict.passed,
            "20 triangles should pass when the gear operation is a cut (reduces geometry), \
             but oracle currently requires {} regardless of cut/boss: {}",
            96, verdict.detail
        );
    }

    // ── Mesh Euler Characteristic Tests ──────────────────────────────────────

    #[test]
    fn mesh_euler_characteristic_cube_chi_2() {
        let mesh = make_unit_cube_mesh();
        let verdict = check_mesh_euler_characteristic(&mesh, 2);
        assert!(
            verdict.passed,
            "unit cube should have χ=2: {}",
            verdict.detail
        );
        assert_eq!(verdict.value, Some(2.0));
    }

    #[test]
    fn mesh_euler_characteristic_wrong_expectation_fails() {
        let mesh = make_unit_cube_mesh();
        let verdict = check_mesh_euler_characteristic(&mesh, 0);
        assert!(!verdict.passed, "cube with expected χ=0 should fail");
    }

    #[test]
    fn mesh_euler_characteristic_empty_mesh_passes() {
        let mesh = RenderMesh {
            vertices: vec![],
            normals: vec![],
            indices: vec![],
            face_ranges: vec![],
        };
        let verdict = check_mesh_euler_characteristic(&mesh, 2);
        assert!(
            verdict.passed,
            "empty mesh should be skipped: {}",
            verdict.detail
        );
    }

    /// Build a torus-like mesh (genus 1, χ=0) from a 4×4 grid wrapped in both directions.
    fn make_torus_mesh() -> RenderMesh {
        // A torus can be parameterized on a grid [0..N) × [0..M) with both
        // directions wrapped. We use N=M=4 for a minimal triangulation.
        let n = 4usize;
        let m = 4usize;
        let major_r = 1.0f32;
        let minor_r = 0.3f32;

        let mut vertices = Vec::new();
        let mut normals = Vec::new();

        // Generate vertices on torus surface
        for i in 0..n {
            let theta = std::f32::consts::TAU * i as f32 / n as f32;
            for j in 0..m {
                let phi = std::f32::consts::TAU * j as f32 / m as f32;
                let x = (major_r + minor_r * phi.cos()) * theta.cos();
                let y = (major_r + minor_r * phi.cos()) * theta.sin();
                let z = minor_r * phi.sin();
                vertices.extend_from_slice(&[x, y, z]);
                // Normal = position on minor circle, normalized
                let nx = phi.cos() * theta.cos();
                let ny = phi.cos() * theta.sin();
                let nz = phi.sin();
                normals.extend_from_slice(&[nx, ny, nz]);
            }
        }

        // Triangulate: each quad (i,j)→(i+1,j)→(i+1,j+1)→(i,j+1) becomes 2 tris
        let mut indices = Vec::new();
        for i in 0..n {
            for j in 0..m {
                let v00 = (i * m + j) as u32;
                let v10 = (((i + 1) % n) * m + j) as u32;
                let v01 = (i * m + (j + 1) % m) as u32;
                let v11 = (((i + 1) % n) * m + (j + 1) % m) as u32;
                indices.extend_from_slice(&[v00, v10, v11]);
                indices.extend_from_slice(&[v00, v11, v01]);
            }
        }

        RenderMesh {
            vertices,
            normals,
            indices,
            face_ranges: vec![FaceRange {
                face_id: KernelId(0),
                start_index: 0,
                end_index: (n * m * 6) as u32,
            }],
        }
    }

    #[test]
    fn mesh_euler_characteristic_torus_chi_0() {
        let mesh = make_torus_mesh();
        let verdict = check_mesh_euler_characteristic(&mesh, 0);
        assert!(verdict.passed, "torus should have χ=0: {}", verdict.detail);
        assert_eq!(verdict.value, Some(0.0));
    }

    // ── Self-Intersection Oracle Tests ──────────────────────────────────────

    #[test]
    fn self_intersection_passes_clean_cube() {
        let mesh = make_unit_cube_mesh();
        let verdict = check_no_self_intersection(&mesh);
        assert!(
            verdict.passed,
            "clean cube should pass self-intersection check: {}",
            verdict.detail
        );
    }

    #[test]
    fn self_intersection_catches_penetrating_faces() {
        // Build a mesh with two face groups whose triangles cross each other.
        // Face 0: a large triangle in the XY plane at z=0
        // Face 1: a large triangle in the XZ plane at y=0 — they intersect
        // along the x-axis.
        let vertices = vec![
            // Face 0: XY plane triangle (z=0), spanning [-1,1] in x and y
            -1.0f32, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0,
            // Face 1: XZ plane triangle (y=0), spanning [-1,1] in x, [-1,1] in z
            -1.0, 0.0, -1.0, 1.0, 0.0, -1.0, 0.0, 0.0, 1.0,
        ];
        let normals = vec![
            0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
            0.0,
        ];
        let indices = vec![0, 1, 2, 3, 4, 5];
        let face_ranges = vec![
            FaceRange {
                face_id: KernelId(0),
                start_index: 0,
                end_index: 3,
            },
            FaceRange {
                face_id: KernelId(1),
                start_index: 3,
                end_index: 6,
            },
        ];

        let mesh = RenderMesh {
            vertices,
            normals,
            indices,
            face_ranges,
        };

        let verdict = check_no_self_intersection(&mesh);
        assert!(
            !verdict.passed,
            "penetrating faces should fail: {}",
            verdict.detail
        );
    }

    #[test]
    fn self_intersection_allows_shared_edges() {
        // Two face groups sharing an edge — should NOT be flagged.
        // Face 0: triangle (0,0,0)-(1,0,0)-(0,1,0)
        // Face 1: triangle (0,0,0)-(1,0,0)-(0,0,1) — shares edge (0,0,0)-(1,0,0)
        let vertices = vec![
            // Face 0
            0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, // Face 1
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let normals = vec![
            0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0, -1.0,
            0.0,
        ];
        let indices = vec![0, 1, 2, 3, 4, 5];
        let face_ranges = vec![
            FaceRange {
                face_id: KernelId(0),
                start_index: 0,
                end_index: 3,
            },
            FaceRange {
                face_id: KernelId(1),
                start_index: 3,
                end_index: 6,
            },
        ];

        let mesh = RenderMesh {
            vertices,
            normals,
            indices,
            face_ranges,
        };

        let verdict = check_no_self_intersection(&mesh);
        assert!(
            verdict.passed,
            "adjacent faces sharing an edge should pass: {}",
            verdict.detail
        );
    }
    // ── PR-TH1: T-junction-aware watertight/χ + normalized penetration ──────

    /// Push one triangle with per-triangle vertices and dummy unit normals.
    fn push_tri(mesh: &mut RenderMesh, p0: [f32; 3], p1: [f32; 3], p2: [f32; 3]) {
        let base = (mesh.vertices.len() / 3) as u32;
        for p in [p0, p1, p2] {
            mesh.vertices.extend_from_slice(&p);
            mesh.normals.extend_from_slice(&[0.0, 0.0, 1.0]);
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    fn finish_single_range(mesh: &mut RenderMesh) {
        mesh.face_ranges = vec![FaceRange {
            face_id: KernelId(0),
            start_index: 0,
            end_index: mesh.indices.len() as u32,
        }];
    }

    fn empty_mesh() -> RenderMesh {
        RenderMesh {
            vertices: vec![],
            normals: vec![],
            indices: vec![],
            face_ranges: vec![],
        }
    }

    /// Unit cube where five faces are 2-triangle quads but the x=1 face is a
    /// 3-triangle fan around m=(1, 0.5, 0) — the exact midpoint of the cube
    /// edge A=(1,0,0)→B=(1,1,0). The z=0 face emits the full edge [A,B] while
    /// the x=1 face emits [A,m]+[m,B]: a T-junction that CLOSES under
    /// subdivision. This is the shape kernel-v2's render tessellation
    /// legitimately emits (collinear chain vertex kept on one face only).
    fn make_t_junction_cube_mesh() -> RenderMesh {
        let mut mesh = empty_mesh();
        let mut quad = |c: [[f32; 3]; 4]| {
            push_tri(&mut mesh, c[0], c[1], c[2]);
            push_tri(&mut mesh, c[0], c[2], c[3]);
        };
        // z=0, z=1, x=0, y=0, y=1 as plain quads
        quad([
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ]);
        quad([
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ]);
        quad([
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
        ]);
        quad([
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ]);
        quad([
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ]);
        // x=1 face: fan around the A–B midpoint m
        let a = [1.0, 0.0, 0.0];
        let b = [1.0, 1.0, 0.0];
        let c = [1.0, 1.0, 1.0];
        let d = [1.0, 0.0, 1.0];
        let m = [1.0, 0.5, 0.0];
        push_tri(&mut mesh, m, b, c);
        push_tri(&mut mesh, m, c, d);
        push_tri(&mut mesh, m, d, a);
        finish_single_range(&mut mesh);
        mesh
    }

    #[test]
    fn watertight_passes_t_junction_that_closes_under_subdivision() {
        let mesh = make_t_junction_cube_mesh();
        let verdict = check_watertight_mesh(&mesh);
        assert!(
            verdict.passed,
            "T-junction mesh that closes under subdivision must be watertight: {}",
            verdict.detail
        );
    }

    #[test]
    fn euler_characteristic_t_junction_cube_chi_2() {
        let mesh = make_t_junction_cube_mesh();
        let verdict = check_mesh_euler_characteristic(&mesh, 2);
        assert!(
            verdict.passed,
            "T-junction cube must have χ=2 on the subdivided complex: {}",
            verdict.detail
        );
        assert_eq!(verdict.value, Some(2.0));
    }

    #[test]
    fn watertight_fails_real_hole_even_with_subdivision() {
        // Same T-junction cube but with the last fan triangle removed — a
        // REAL hole. Its boundary edges do not close under subdivision, so
        // the oracle must stay strict and fail.
        let mut mesh = make_t_junction_cube_mesh();
        mesh.indices.truncate(mesh.indices.len() - 3);
        finish_single_range(&mut mesh);
        let verdict = check_watertight_mesh(&mesh);
        assert!(
            !verdict.passed,
            "a real hole must fail even with T-junction subdivision: {}",
            verdict.detail
        );
    }

    #[test]
    fn watertight_does_not_split_at_vertex_off_the_edge() {
        // Cube whose x=1 face is fanned from m'=(1, 0.5, 0.1) — 0.1 off the
        // A–B edge line (=10⁴ lattice cells at unit scale) — and missing the
        // (m',A,B) triangle, leaving a real slit. m' must NOT split [A,B]:
        // the mesh stays non-watertight.
        let mut mesh = empty_mesh();
        let mut quad = |c: [[f32; 3]; 4]| {
            push_tri(&mut mesh, c[0], c[1], c[2]);
            push_tri(&mut mesh, c[0], c[2], c[3]);
        };
        quad([
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ]);
        quad([
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ]);
        quad([
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
        ]);
        quad([
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ]);
        quad([
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ]);
        let a = [1.0, 0.0, 0.0];
        let b = [1.0, 1.0, 0.0];
        let c = [1.0, 1.0, 1.0];
        let d = [1.0, 0.0, 1.0];
        let m = [1.0, 0.5, 0.1]; // NOT on [A,B]
        push_tri(&mut mesh, m, b, c);
        push_tri(&mut mesh, m, c, d);
        push_tri(&mut mesh, m, d, a);
        finish_single_range(&mut mesh);
        let verdict = check_watertight_mesh(&mesh);
        assert!(
            !verdict.passed,
            "a vertex off the edge line must not split it: {}",
            verdict.detail
        );
    }

    /// Plain 12-triangle cube spanning [origin, origin+1]³.
    fn push_plain_cube(mesh: &mut RenderMesh, o: [f32; 3]) {
        let p = |dx: f32, dy: f32, dz: f32| -> [f32; 3] { [o[0] + dx, o[1] + dy, o[2] + dz] };
        let quads = [
            [p(0., 0., 0.), p(0., 1., 0.), p(1., 1., 0.), p(1., 0., 0.)],
            [p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)],
            [p(0., 0., 0.), p(0., 0., 1.), p(0., 1., 1.), p(0., 1., 0.)],
            [p(1., 0., 0.), p(1., 1., 0.), p(1., 1., 1.), p(1., 0., 1.)],
            [p(0., 0., 0.), p(1., 0., 0.), p(1., 0., 1.), p(0., 0., 1.)],
            [p(0., 1., 0.), p(0., 1., 1.), p(1., 1., 1.), p(1., 1., 0.)],
        ];
        for q in quads {
            push_tri(mesh, q[0], q[1], q[2]);
            push_tri(mesh, q[0], q[2], q[3]);
        }
    }

    #[test]
    fn euler_characteristic_two_shell_mesh_expects_2_per_shell() {
        // KV4-F4: a disjoint-union output is ONE solid with TWO closed
        // shells; total χ = 4, not 2. The oracle derives the shell count
        // from the welded complex and adjusts the expectation.
        let mut mesh = empty_mesh();
        push_plain_cube(&mut mesh, [0.0, 0.0, 0.0]);
        push_plain_cube(&mut mesh, [3.0, 0.0, 0.0]);
        finish_single_range(&mut mesh);
        let verdict = check_mesh_euler_characteristic(&mesh, 2);
        assert!(
            verdict.passed,
            "2-shell complex must be scored as χ=2 per shell: {}",
            verdict.detail
        );
        assert_eq!(verdict.value, Some(4.0));
        // Both shells must also be watertight.
        let wt = check_watertight_mesh(&mesh);
        assert!(wt.passed, "two clean shells: {}", wt.detail);
    }

    #[test]
    fn euler_characteristic_two_shell_mesh_with_defect_still_fails() {
        // Strictness: a 2-shell mesh whose χ is NOT 2 per shell still fails.
        let mut mesh = empty_mesh();
        push_plain_cube(&mut mesh, [0.0, 0.0, 0.0]);
        push_plain_cube(&mut mesh, [3.0, 0.0, 0.0]);
        mesh.indices.truncate(mesh.indices.len() - 3); // hole in shell 2
        finish_single_range(&mut mesh);
        let verdict = check_mesh_euler_characteristic(&mesh, 2);
        assert!(
            !verdict.passed,
            "defective 2-shell mesh must still fail: {}",
            verdict.detail
        );
    }

    /// Two-face-range mesh: a LARGE triangle in the z=0 plane (unnormalized
    /// plane normal |n| ≈ 8e4) plus a second triangle positioned by `dip`.
    fn make_large_plane_pair(dip_z: f32) -> RenderMesh {
        let mut mesh = empty_mesh();
        // Face 0: large triangle, max_abs = 100 → depth_threshold = 1e-2
        push_tri(
            &mut mesh,
            [-100.0, -100.0, 0.0],
            [100.0, -100.0, 0.0],
            [0.0, 100.0, 0.0],
        );
        // Face 1: triangle touching/penetrating the plane near the origin
        push_tri(
            &mut mesh,
            [-1.0, 0.0, dip_z],
            [1.0, 0.0, dip_z],
            [0.0, 1.0, 50.0],
        );
        mesh.face_ranges = vec![
            FaceRange {
                face_id: KernelId(0),
                start_index: 0,
                end_index: 3,
            },
            FaceRange {
                face_id: KernelId(1),
                start_index: 3,
                end_index: 6,
            },
        ];
        mesh
    }

    #[test]
    fn self_intersection_ignores_grazing_contact_with_large_normal() {
        // f32-noise-scale dip (1e-5 ≪ depth_threshold 1e-2): a grazing
        // contact, NOT a penetration. Before normalization the |n|≈8e4 plane
        // scaled the 1e-5 dip to 0.8 ≫ threshold → false positive.
        let mesh = make_large_plane_pair(-1e-5);
        let verdict = check_no_self_intersection(&mesh);
        assert!(
            verdict.passed,
            "grazing contact at f32 noise must not be a penetration: {}",
            verdict.detail
        );
    }

    #[test]
    fn self_intersection_catches_real_penetration_with_large_normal() {
        // Deep crossing (50 units below the plane) must fail before AND
        // after normalization — the guard is normalized, not widened.
        let mesh = make_large_plane_pair(-50.0);
        let verdict = check_no_self_intersection(&mesh);
        assert!(
            !verdict.passed,
            "a real penetration must still fail: {}",
            verdict.detail
        );
    }
}
