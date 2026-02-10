//! Tests for verification oracles.

use kernel_fork::types::{FaceRange, RenderMesh};
use kernel_fork::KernelId;
use test_harness::oracle::*;
use test_harness::ModelBuilder;

/// Build a MockKernel box and get its solid handle + mesh for testing.
fn build_mock_box() -> (ModelBuilder, String) {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    (m, "box".to_string())
}

// ── Topology Oracle Tests ───────────────────────────────────────────────

#[test]
fn euler_formula_passes_for_box() {
    let (m, name) = build_mock_box();
    let handle = m.solid_handle(&name).unwrap();
    let result = check_euler_formula(m.kernel().as_introspect(), &handle);
    assert!(
        result.passed,
        "Box should satisfy Euler's formula: {}",
        result.detail
    );
}

#[test]
fn manifold_edges_passes_for_box() {
    let (m, name) = build_mock_box();
    let handle = m.solid_handle(&name).unwrap();
    let result = check_manifold_edges(m.kernel().as_introspect(), &handle);
    assert!(
        result.passed,
        "Box edges should be manifold: {}",
        result.detail
    );
}

#[test]
fn face_validity_passes_for_box() {
    let (m, name) = build_mock_box();
    let handle = m.solid_handle(&name).unwrap();
    let result = check_face_validity(m.kernel().as_introspect(), &handle);
    assert!(
        result.passed,
        "Box faces should be valid: {}",
        result.detail
    );
}

#[test]
fn topology_counts_correct_for_box() {
    let (m, name) = build_mock_box();
    let handle = m.solid_handle(&name).unwrap();
    // MockKernel box: V=8 E=12 F=6
    let result = check_topology_counts(m.kernel().as_introspect(), &handle, 8, 12, 6);
    assert!(result.passed, "Box V=8 E=12 F=6: {}", result.detail);
}

#[test]
fn topology_counts_fails_with_wrong_values() {
    let (m, name) = build_mock_box();
    let handle = m.solid_handle(&name).unwrap();
    let result = check_topology_counts(m.kernel().as_introspect(), &handle, 99, 99, 99);
    assert!(!result.passed, "Should fail with wrong counts");
}

// ── Mesh Oracle Tests ───────────────────────────────────────────────────

fn mock_box_mesh() -> RenderMesh {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.tessellate("box").unwrap()
}

#[test]
fn watertight_mesh_passes_for_mock_box() {
    let mesh = mock_box_mesh();
    let result = check_watertight_mesh(&mesh);
    assert!(
        result.passed,
        "MockKernel box mesh should be watertight: {}",
        result.detail
    );
}

#[test]
fn consistent_normals_passes_for_mock_box() {
    let mesh = mock_box_mesh();
    let result = check_consistent_normals(&mesh);
    assert!(
        result.passed,
        "Box normals should be consistent: {}",
        result.detail
    );
}

#[test]
fn no_degenerate_triangles_passes_for_mock_box() {
    let mesh = mock_box_mesh();
    let result = check_no_degenerate_triangles(&mesh);
    assert!(
        result.passed,
        "Box should have no degenerate triangles: {}",
        result.detail
    );
}

#[test]
fn unit_normals_passes_for_mock_box() {
    let mesh = mock_box_mesh();
    let result = check_unit_normals(&mesh);
    assert!(
        result.passed,
        "Box normals should be unit length: {}",
        result.detail
    );
}

#[test]
fn face_range_coverage_passes_for_mock_box() {
    let mesh = mock_box_mesh();
    let result = check_face_range_coverage(&mesh);
    assert!(
        result.passed,
        "Box face ranges should cover all indices: {}",
        result.detail
    );
}

#[test]
fn valid_indices_passes_for_mock_box() {
    let mesh = mock_box_mesh();
    let result = check_valid_indices(&mesh);
    assert!(
        result.passed,
        "Box indices should be valid: {}",
        result.detail
    );
}

// ── Failing Oracle Tests (deliberately broken meshes) ───────────────────

#[test]
fn watertight_fails_for_open_mesh() {
    let mesh = RenderMesh {
        vertices: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 2], // Single triangle — all edges are boundary
        face_ranges: vec![FaceRange {
            face_id: KernelId(1),
            start_index: 0,
            end_index: 3,
        }],
    };
    let result = check_watertight_mesh(&mesh);
    assert!(!result.passed, "Single triangle should not be watertight");
}

#[test]
fn valid_indices_fails_for_out_of_bounds() {
    let mesh = RenderMesh {
        vertices: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 99], // Index 99 is out of bounds
        face_ranges: vec![],
    };
    let result = check_valid_indices(&mesh);
    assert!(!result.passed, "Out-of-bounds index should fail");
}

#[test]
fn face_range_coverage_fails_with_gap() {
    let mesh = RenderMesh {
        vertices: vec![0.0; 18],
        normals: vec![0.0; 18],
        indices: vec![0, 1, 2, 3, 4, 5],
        face_ranges: vec![
            FaceRange {
                face_id: KernelId(1),
                start_index: 0,
                end_index: 3,
            },
            // Gap: missing indices 3..6
        ],
    };
    let result = check_face_range_coverage(&mesh);
    assert!(!result.passed, "Gap in face ranges should fail");
}

#[test]
fn run_all_mesh_checks_returns_multiple_verdicts() {
    let mesh = mock_box_mesh();
    let results = run_all_mesh_checks(&mesh);
    assert!(results.len() >= 6, "Should have at least 6 mesh checks");
    // All should pass for a valid mock box
    for v in &results {
        assert!(
            v.passed,
            "All mesh checks should pass for mock box: {} — {}",
            v.oracle_name, v.detail
        );
    }
}
