//! Verification oracles — pure functions returning pass/fail verdicts.
//!
//! Each oracle returns an `OracleVerdict` with diagnostic detail, not panics.
//! This lets agents collect all failures in one pass.

use std::collections::HashMap;

use kernel::types::RenderMesh;
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
    let grid_size = (max_abs as f64 * 1e-5).max(1e-10);
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

    if non_paired.is_empty() {
        OracleVerdict::pass(
            "watertight_mesh",
            format!("all {} edges paired", edge_counts.len()),
        )
    } else {
        OracleVerdict::fail(
            "watertight_mesh",
            format!(
                "{} unpaired edges out of {} total",
                non_paired.len(),
                edge_counts.len()
            ),
        )
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
        OracleVerdict::fail(
            "aabb_collapse",
            format!(
                "all {} unique vertices lie on AABB faces — mesh collapsed to bounding box",
                total_unique
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
}
