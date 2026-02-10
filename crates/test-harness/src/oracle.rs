//! Verification oracles — pure functions returning pass/fail verdicts.
//!
//! Each oracle returns an `OracleVerdict` with diagnostic detail, not panics.
//! This lets agents collect all failures in one pass.

use std::collections::HashMap;

use kernel_fork::types::RenderMesh;
use kernel_fork::{KernelIntrospect, KernelSolidHandle};
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
    // Quantize vertex positions to allow position-based matching
    fn quantize(v: f32) -> i64 {
        (v as f64 * 10000.0).round() as i64
    }

    fn vert_key(mesh: &RenderMesh, idx: u32) -> (i64, i64, i64) {
        let i = idx as usize * 3;
        (
            quantize(mesh.vertices[i]),
            quantize(mesh.vertices[i + 1]),
            quantize(mesh.vertices[i + 2]),
        )
    }

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
        let va = vert_key(mesh, tri[0]);
        let vb = vert_key(mesh, tri[1]);
        let vc = vert_key(mesh, tri[2]);

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

        // Geometric normal from cross product
        let ax = verts[i1] - verts[i0];
        let ay = verts[i1 + 1] - verts[i0 + 1];
        let az = verts[i1 + 2] - verts[i0 + 2];
        let bx = verts[i2] - verts[i0];
        let by = verts[i2 + 1] - verts[i0 + 1];
        let bz = verts[i2 + 2] - verts[i0 + 2];
        let gnx = ay * bz - az * by;
        let gny = az * bx - ax * bz;
        let gnz = ax * by - ay * bx;

        // Average stored normal for the triangle's vertices
        if i0 + 2 >= norms.len() || i1 + 2 >= norms.len() || i2 + 2 >= norms.len() {
            continue;
        }
        let snx = (norms[i0] + norms[i1] + norms[i2]) / 3.0;
        let sny = (norms[i0 + 1] + norms[i1 + 1] + norms[i2 + 1]) / 3.0;
        let snz = (norms[i0 + 2] + norms[i1 + 2] + norms[i2 + 2]) / 3.0;

        let dot = gnx * snx + gny * sny + gnz * snz;
        if dot < 0.0 {
            inconsistent += 1;
        }
    }

    if inconsistent == 0 {
        OracleVerdict::pass(
            "consistent_normals",
            format!("all {} triangles have consistent winding", total),
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
