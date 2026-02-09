use std::collections::HashMap;

use kernel_fork::{Kernel, KernelId};
use kernel_fork::{KernelIntrospect, MockKernel};
use modeling_ops::boolean::{execute_boolean, BooleanKind};
use modeling_ops::chamfer::execute_chamfer;
use modeling_ops::diff::{self, signature_similarity};
use modeling_ops::extrude::{execute_extrude, execute_symmetric_extrude};
use modeling_ops::fillet::execute_fillet;
use modeling_ops::revolve::execute_revolve;
use modeling_ops::shell::execute_shell;
use modeling_ops::types::OpError;
use waffle_types::{ClosedProfile, OutputKey, Role, TopoKind, TopoSignature};

/// Helper: create a face from a rectangular profile.
fn make_face(kernel: &mut MockKernel) -> KernelId {
    let profile = ClosedProfile {
        entity_ids: vec![1, 2, 3, 4],
        is_outer: true,
    };
    let mut positions = HashMap::new();
    positions.insert(1, (0.0, 0.0));
    positions.insert(2, (2.0, 0.0));
    positions.insert(3, (2.0, 3.0));
    positions.insert(4, (0.0, 3.0));

    let face_ids = kernel
        .make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .unwrap();
    face_ids[0]
}

// ── Topology Diff Tests ────────────────────────────────────────────────────

#[test]
fn diff_empty_before_all_created() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let empty = diff::TopoSnapshot {
        faces: Vec::new(),
        edges: Vec::new(),
        vertices: Vec::new(),
    };
    let after = diff::snapshot(&kernel, &handle);
    let result = diff::diff(&empty, &after);

    assert!(!result.created.is_empty(), "Should have created entities");
    assert!(result.deleted.is_empty(), "Should have no deleted entities");
    let face_count = result
        .created
        .iter()
        .filter(|e| e.kind == TopoKind::Face)
        .count();
    assert_eq!(face_count, 6, "Should create 6 faces for a box");
}

#[test]
fn diff_identical_snapshots_no_changes() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let snap = diff::snapshot(&kernel, &handle);
    let result = diff::diff(&snap, &snap);

    assert!(result.created.is_empty(), "No created entities");
    assert!(result.deleted.is_empty(), "No deleted entities");
    assert!(!result.survived.is_empty(), "All entities survived");
}

#[test]
fn snapshot_captures_correct_entity_counts() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let snap = diff::snapshot(&kernel, &handle);
    assert_eq!(snap.faces.len(), 6, "Box has 6 faces");
    assert_eq!(snap.edges.len(), 12, "Box has 12 edges");
    assert_eq!(snap.vertices.len(), 8, "Box has 8 vertices");
}

#[test]
fn signature_similarity_identical_is_1() {
    let sig = TopoSignature {
        surface_type: Some("planar".to_string()),
        area: Some(4.0),
        centroid: Some([1.0, 1.5, 0.0]),
        normal: Some([0.0, 0.0, 1.0]),
        bbox: None,
        adjacency_hash: None,
        length: None,
    };
    let sim = signature_similarity(&sig, &sig);
    assert!(
        (sim - 1.0).abs() < 1e-6,
        "Identical signatures should have similarity 1.0, got {}",
        sim
    );
}

#[test]
fn signature_similarity_different_type_is_low() {
    let sig_a = TopoSignature {
        surface_type: Some("planar".to_string()),
        area: Some(4.0),
        centroid: Some([0.0, 0.0, 0.0]),
        normal: Some([0.0, 0.0, 1.0]),
        bbox: None,
        adjacency_hash: None,
        length: None,
    };
    let sig_b = TopoSignature {
        surface_type: Some("cylindrical".to_string()),
        area: Some(4.0),
        centroid: Some([0.0, 0.0, 0.0]),
        normal: Some([0.0, 0.0, 1.0]),
        bbox: None,
        adjacency_hash: None,
        length: None,
    };
    let sim = signature_similarity(&sig_a, &sig_b);
    assert!(
        sim < 0.9,
        "Different surface types should lower similarity, got {}",
        sim
    );
}

#[test]
fn signature_similarity_empty_is_0() {
    let empty = TopoSignature::empty();
    let sim = signature_similarity(&empty, &empty);
    assert_eq!(
        sim, 0.0,
        "Empty signatures should have 0 similarity (no fields to compare)"
    );
}

// ── Extrude Tests ──────────────────────────────────────────────────────────

#[test]
fn extrude_produces_valid_op_result() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();

    assert_eq!(result.outputs.len(), 1);
    assert!(matches!(result.outputs[0].0, OutputKey::Main));
    assert!(!result.provenance.created.is_empty());
}

#[test]
fn extrude_assigns_end_cap_roles() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();

    let roles = &result.provenance.role_assignments;
    let has_pos_cap = roles.iter().any(|(_, r)| *r == Role::EndCapPositive);
    let has_neg_cap = roles.iter().any(|(_, r)| *r == Role::EndCapNegative);
    let side_count = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::SideFace { .. }))
        .count();

    assert!(has_pos_cap, "Should assign EndCapPositive role");
    assert!(has_neg_cap, "Should assign EndCapNegative role");
    assert_eq!(side_count, 4, "Box extrude should have 4 side faces");
}

#[test]
fn extrude_provenance_tracks_face_count() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();

    assert_eq!(
        result.provenance.role_assignments.len(),
        6,
        "All 6 box faces should get roles"
    );
}

#[test]
fn extrude_with_different_directions() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_extrude(&mut kernel, face_id, [1.0, 0.0, 0.0], 3.0, None).unwrap();

    let roles = &result.provenance.role_assignments;
    let has_pos_cap = roles.iter().any(|(_, r)| *r == Role::EndCapPositive);
    assert!(
        has_pos_cap,
        "Should assign EndCapPositive for X-direction extrude"
    );
}

#[test]
fn extrude_invalid_face_returns_error() {
    let mut kernel = MockKernel::new();
    let result = execute_extrude(&mut kernel, KernelId(999), [0.0, 0.0, 1.0], 5.0, None);
    assert!(matches!(result, Err(OpError::Kernel(_))));
}

// ── Revolve Tests ──────────────────────────────────────────────────────────

#[test]
fn revolve_produces_valid_op_result() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_revolve(
        &mut kernel,
        face_id,
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        std::f64::consts::PI,
        None,
    )
    .unwrap();

    assert_eq!(result.outputs.len(), 1);
    assert!(matches!(result.outputs[0].0, OutputKey::Main));
    assert!(!result.provenance.created.is_empty());
}

#[test]
fn revolve_partial_assigns_roles() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_revolve(
        &mut kernel,
        face_id,
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        std::f64::consts::FRAC_PI_2,
        None,
    )
    .unwrap();

    let roles = &result.provenance.role_assignments;
    assert!(!roles.is_empty(), "Should assign roles");
}

#[test]
fn revolve_full_assigns_side_faces() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_revolve(
        &mut kernel,
        face_id,
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        std::f64::consts::TAU,
        None,
    )
    .unwrap();

    let roles = &result.provenance.role_assignments;
    for (_, role) in roles {
        assert!(
            matches!(role, Role::SideFace { .. }),
            "Full revolution should only have SideFace roles, got {:?}",
            role
        );
    }
}

#[test]
fn revolve_invalid_face_returns_error() {
    let mut kernel = MockKernel::new();
    let result = execute_revolve(
        &mut kernel,
        KernelId(999),
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        std::f64::consts::PI,
        None,
    );
    assert!(matches!(result, Err(OpError::Kernel(_))));
}

// ── Boolean Tests ──────────────────────────────────────────────────────────

#[test]
fn boolean_union_produces_combined_result() {
    let mut kernel = MockKernel::new();

    let face_a = make_face(&mut kernel);
    let handle_a = kernel.extrude_face(face_a, [0.0, 0.0, 1.0], 2.0).unwrap();
    let face_b = make_face(&mut kernel);
    let handle_b = kernel.extrude_face(face_b, [0.0, 0.0, 1.0], 2.0).unwrap();

    let result = execute_boolean(&mut kernel, &handle_a, &handle_b, BooleanKind::Union).unwrap();

    assert_eq!(result.outputs.len(), 1);
    assert!(matches!(result.outputs[0].0, OutputKey::Main));

    let result_faces = kernel.list_faces(&result.outputs[0].1.handle);
    assert_eq!(result_faces.len(), 12, "Mock union merges all faces");
}

#[test]
fn boolean_subtract_produces_result() {
    let mut kernel = MockKernel::new();

    let face_a = make_face(&mut kernel);
    let handle_a = kernel.extrude_face(face_a, [0.0, 0.0, 1.0], 2.0).unwrap();
    let face_b = make_face(&mut kernel);
    let handle_b = kernel.extrude_face(face_b, [0.0, 0.0, 1.0], 1.0).unwrap();

    let result = execute_boolean(&mut kernel, &handle_a, &handle_b, BooleanKind::Subtract).unwrap();

    assert_eq!(result.outputs.len(), 1);
    let result_faces = kernel.list_faces(&result.outputs[0].1.handle);
    assert_eq!(result_faces.len(), 6);
}

#[test]
fn boolean_intersect_produces_result() {
    let mut kernel = MockKernel::new();

    let face_a = make_face(&mut kernel);
    let handle_a = kernel.extrude_face(face_a, [0.0, 0.0, 1.0], 2.0).unwrap();
    let face_b = make_face(&mut kernel);
    let handle_b = kernel.extrude_face(face_b, [0.0, 0.0, 1.0], 2.0).unwrap();

    let result =
        execute_boolean(&mut kernel, &handle_a, &handle_b, BooleanKind::Intersect).unwrap();

    assert_eq!(result.outputs.len(), 1);
    let result_faces = kernel.list_faces(&result.outputs[0].1.handle);
    assert_eq!(
        result_faces.len(),
        6,
        "Intersect in MockKernel produces a box"
    );
}

#[test]
fn boolean_assigns_body_a_b_roles() {
    let mut kernel = MockKernel::new();

    let face_a = make_face(&mut kernel);
    let handle_a = kernel.extrude_face(face_a, [0.0, 0.0, 1.0], 2.0).unwrap();

    // Use a different-sized profile for body B so signatures differ
    let profile_b = ClosedProfile {
        entity_ids: vec![10, 11, 12, 13],
        is_outer: true,
    };
    let mut positions_b = HashMap::new();
    positions_b.insert(10, (0.0, 0.0));
    positions_b.insert(11, (5.0, 0.0));
    positions_b.insert(12, (5.0, 5.0));
    positions_b.insert(13, (0.0, 5.0));
    let face_b_ids = kernel
        .make_faces_from_profiles(
            &[profile_b],
            [10.0, 10.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions_b,
        )
        .unwrap();
    let handle_b = kernel
        .extrude_face(face_b_ids[0], [0.0, 0.0, 1.0], 8.0)
        .unwrap();

    let result = execute_boolean(&mut kernel, &handle_a, &handle_b, BooleanKind::Union).unwrap();

    let roles = &result.provenance.role_assignments;
    let a_count = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::BooleanBodyAFace { .. }))
        .count();
    let b_count = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::BooleanBodyBFace { .. }))
        .count();

    assert!(a_count > 0, "Should have BooleanBodyAFace roles");
    assert!(b_count > 0, "Should have BooleanBodyBFace roles");
    assert_eq!(
        a_count + b_count,
        12,
        "All 12 union faces should get body A or B roles"
    );
}

// ── Fillet Tests ──────────────────────────────────────────────────────────

#[test]
fn fillet_produces_valid_op_result() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    // Fillet one edge
    let edges = kernel.list_edges(&handle);
    let result = execute_fillet(&mut kernel, &handle, &[edges[0]], 0.2).unwrap();

    assert_eq!(result.outputs.len(), 1);
    assert!(matches!(result.outputs[0].0, OutputKey::Main));
}

#[test]
fn fillet_assigns_fillet_face_roles() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let edges = kernel.list_edges(&handle);
    let result = execute_fillet(&mut kernel, &handle, &[edges[0]], 0.2).unwrap();

    let fillet_roles: Vec<_> = result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::FilletFace { .. }))
        .collect();

    assert!(
        !fillet_roles.is_empty(),
        "Should assign FilletFace roles to new faces"
    );
}

#[test]
fn fillet_provenance_tracks_created() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let edges = kernel.list_edges(&handle);
    let result = execute_fillet(&mut kernel, &handle, &[edges[0]], 0.2).unwrap();

    assert!(
        !result.provenance.created.is_empty(),
        "Fillet should create new entities"
    );
}

#[test]
fn fillet_invalid_radius_returns_error() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let result = execute_fillet(&mut kernel, &handle, &[], -0.1);
    assert!(matches!(result, Err(OpError::InvalidParameter { .. })));
}

// ── Chamfer Tests ─────────────────────────────────────────────────────────

#[test]
fn chamfer_produces_valid_op_result() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let edges = kernel.list_edges(&handle);
    let result = execute_chamfer(&mut kernel, &handle, &[edges[0]], 0.3).unwrap();

    assert_eq!(result.outputs.len(), 1);
    assert!(matches!(result.outputs[0].0, OutputKey::Main));
}

#[test]
fn chamfer_assigns_chamfer_face_roles() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let edges = kernel.list_edges(&handle);
    let result = execute_chamfer(&mut kernel, &handle, &[edges[0]], 0.3).unwrap();

    let chamfer_roles: Vec<_> = result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::ChamferFace { .. }))
        .collect();

    assert!(
        !chamfer_roles.is_empty(),
        "Should assign ChamferFace roles to new faces"
    );
}

#[test]
fn chamfer_invalid_distance_returns_error() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let result = execute_chamfer(&mut kernel, &handle, &[], -0.1);
    assert!(matches!(result, Err(OpError::InvalidParameter { .. })));
}

// ── Shell Tests ───────────────────────────────────────────────────────────

#[test]
fn shell_produces_valid_op_result() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let faces = kernel.list_faces(&handle);
    // Remove one face (top face)
    let result = execute_shell(&mut kernel, &handle, &[faces[1]], 0.2).unwrap();

    assert_eq!(result.outputs.len(), 1);
    assert!(matches!(result.outputs[0].0, OutputKey::Main));
}

#[test]
fn shell_assigns_inner_face_roles() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let faces = kernel.list_faces(&handle);
    let result = execute_shell(&mut kernel, &handle, &[faces[1]], 0.2).unwrap();

    let inner_roles: Vec<_> = result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::ShellInnerFace { .. }))
        .collect();

    assert!(
        !inner_roles.is_empty(),
        "Should assign ShellInnerFace roles to inner faces"
    );
}

#[test]
fn shell_invalid_thickness_returns_error() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let result = execute_shell(&mut kernel, &handle, &[], -0.1);
    assert!(matches!(result, Err(OpError::InvalidParameter { .. })));
}

// ── Symmetric Extrude Tests ──────────────────────────────────────────────

#[test]
fn symmetric_extrude_produces_valid_result() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result =
        execute_symmetric_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 10.0, None).unwrap();

    assert_eq!(result.outputs.len(), 1);
    assert!(matches!(result.outputs[0].0, OutputKey::Main));
    assert!(!result.provenance.created.is_empty());
}

#[test]
fn symmetric_extrude_assigns_end_cap_roles() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result =
        execute_symmetric_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 10.0, None).unwrap();

    let roles = &result.provenance.role_assignments;
    let has_pos_cap = roles.iter().any(|(_, r)| *r == Role::EndCapPositive);
    let has_neg_cap = roles.iter().any(|(_, r)| *r == Role::EndCapNegative);

    assert!(has_pos_cap, "Symmetric extrude should have EndCapPositive");
    assert!(has_neg_cap, "Symmetric extrude should have EndCapNegative");
}

#[test]
fn symmetric_extrude_has_diagnostic_warning() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result =
        execute_symmetric_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 10.0, None).unwrap();

    assert!(
        !result.diagnostics.warnings.is_empty(),
        "Should include symmetric extrude diagnostic"
    );
}

#[test]
fn symmetric_extrude_invalid_depth_returns_error() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_symmetric_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], -5.0, None);
    assert!(matches!(result, Err(OpError::InvalidParameter { .. })));
}
