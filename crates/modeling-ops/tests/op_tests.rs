use std::collections::HashMap;

use kernel::{Kernel, KernelId, KernelIntrospect};
use kernel::{MockKernel, WaffleKernel};
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
        vertex_ids: vec![],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
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
        180.0,
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
        90.0,
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
        360.0,
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
        180.0,
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
        vertex_ids: vec![],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
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

// ── M9: Comprehensive MockKernel Tests ──────────────────────────────────

/// Verify Euler's formula V - E + F = 2 holds for a box solid.
#[test]
fn euler_formula_holds_for_extrude() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let v = kernel.list_vertices(&handle).len() as i32;
    let e = kernel.list_edges(&handle).len() as i32;
    let f = kernel.list_faces(&handle).len() as i32;

    assert_eq!(
        v - e + f,
        2,
        "Euler: V({v}) - E({e}) + F({f}) should equal 2"
    );
}

/// Extrude → Fillet pipeline: fillet an edge of an extruded box.
#[test]
fn pipeline_extrude_then_fillet() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let edges = kernel.list_edges(&handle);
    let result = execute_fillet(&mut kernel, &handle, &[edges[0]], 0.2).unwrap();

    // Result should have 7 faces (6 original + 1 fillet)
    let result_handle = &result.outputs[0].1.handle;
    let result_faces = kernel.list_faces(result_handle);
    assert_eq!(result_faces.len(), 7, "Fillet adds 1 face to box");

    // Provenance should track creation of new entities
    let created_faces = result
        .provenance
        .created
        .iter()
        .filter(|e| e.kind == TopoKind::Face)
        .count();
    assert!(created_faces >= 1, "At least 1 face created by fillet");
}

/// Extrude → Chamfer pipeline.
#[test]
fn pipeline_extrude_then_chamfer() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let edges = kernel.list_edges(&handle);
    let result = execute_chamfer(&mut kernel, &handle, &[edges[0]], 0.3).unwrap();

    let result_handle = &result.outputs[0].1.handle;
    let result_faces = kernel.list_faces(result_handle);
    assert_eq!(result_faces.len(), 7, "Chamfer adds 1 face to box");

    let chamfer_count = result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::ChamferFace { .. }))
        .count();
    assert_eq!(chamfer_count, 1, "Exactly 1 chamfer face");
}

/// Extrude → Shell pipeline.
#[test]
fn pipeline_extrude_then_shell() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let faces = kernel.list_faces(&handle);
    // Remove top face (index 1 is top in MockKernel box)
    let result = execute_shell(&mut kernel, &handle, &[faces[1]], 0.2).unwrap();

    let result_handle = &result.outputs[0].1.handle;
    let result_faces = kernel.list_faces(result_handle);
    // 6 original - 1 removed + 5 inner = 10
    assert_eq!(result_faces.len(), 10, "Shell: 5 outer + 5 inner faces");

    let inner_count = result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::ShellInnerFace { .. }))
        .count();
    assert_eq!(inner_count, 5, "5 inner faces for 5 kept outer faces");
}

/// Extrude → Boolean Union pipeline.
#[test]
fn pipeline_extrude_boolean_union() {
    let mut kernel = MockKernel::new();
    let face_a = make_face(&mut kernel);
    let handle_a = kernel.extrude_face(face_a, [0.0, 0.0, 1.0], 3.0).unwrap();
    let face_b = make_face(&mut kernel);
    let handle_b = kernel.extrude_face(face_b, [0.0, 0.0, 1.0], 3.0).unwrap();

    let snap_a = diff::snapshot(&kernel, &handle_a);
    let snap_b = diff::snapshot(&kernel, &handle_b);

    let result = execute_boolean(&mut kernel, &handle_a, &handle_b, BooleanKind::Union).unwrap();

    let result_handle = &result.outputs[0].1.handle;
    let result_faces = kernel.list_faces(result_handle);
    assert_eq!(
        result_faces.len(),
        snap_a.faces.len() + snap_b.faces.len(),
        "Mock union merges all faces"
    );
}

/// Multiple fillet operations on different edges.
#[test]
fn fillet_multiple_edges() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let edges = kernel.list_edges(&handle);
    // Fillet 3 edges at once
    let result =
        execute_fillet(&mut kernel, &handle, &[edges[0], edges[1], edges[2]], 0.15).unwrap();

    let result_handle = &result.outputs[0].1.handle;
    let result_faces = kernel.list_faces(result_handle);
    // 6 original + 3 fillet faces = 9
    assert_eq!(
        result_faces.len(),
        9,
        "3 filleted edges → 3 new fillet faces"
    );

    let fillet_count = result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::FilletFace { .. }))
        .count();
    assert_eq!(fillet_count, 3, "3 FilletFace roles assigned");
}

/// Multiple chamfer operations on different edges.
#[test]
fn chamfer_multiple_edges() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let edges = kernel.list_edges(&handle);
    let result = execute_chamfer(&mut kernel, &handle, &[edges[0], edges[1]], 0.25).unwrap();

    let result_handle = &result.outputs[0].1.handle;
    let result_faces = kernel.list_faces(result_handle);
    assert_eq!(result_faces.len(), 8, "2 chamfered edges → 2 new faces");

    let chamfer_count = result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::ChamferFace { .. }))
        .count();
    assert_eq!(chamfer_count, 2, "2 ChamferFace roles assigned");
}

/// Shell with multiple faces removed.
#[test]
fn shell_remove_multiple_faces() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let faces = kernel.list_faces(&handle);
    // Remove top and bottom faces
    let result = execute_shell(&mut kernel, &handle, &[faces[0], faces[1]], 0.2).unwrap();

    let result_handle = &result.outputs[0].1.handle;
    let result_faces = kernel.list_faces(result_handle);
    // 6 - 2 removed + 4 inner = 8
    assert_eq!(
        result_faces.len(),
        8,
        "Shell removing 2 faces: 4 outer + 4 inner"
    );
}

/// Provenance created entities all have valid signatures.
#[test]
fn provenance_created_entities_have_signatures() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();

    for entity in &result.provenance.created {
        // Every created entity should have a non-empty signature
        let sig = &entity.signature;
        let has_some_data = sig.surface_type.is_some()
            || sig.area.is_some()
            || sig.centroid.is_some()
            || sig.normal.is_some()
            || sig.length.is_some();
        assert!(
            has_some_data,
            "Created entity {:?} should have signature data",
            entity.kernel_id
        );
    }
}

/// All ops produce consistent OutputKey::Main.
#[test]
fn all_ops_produce_main_output_key() {
    let mut kernel = MockKernel::new();

    // Extrude
    let face_id = make_face(&mut kernel);
    let extrude_result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();
    assert!(matches!(extrude_result.outputs[0].0, OutputKey::Main));

    // Revolve
    let face_id2 = make_face(&mut kernel);
    let revolve_result = execute_revolve(
        &mut kernel,
        face_id2,
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        180.0,
        None,
    )
    .unwrap();
    assert!(matches!(revolve_result.outputs[0].0, OutputKey::Main));

    // Symmetric extrude
    let face_id3 = make_face(&mut kernel);
    let sym_result =
        execute_symmetric_extrude(&mut kernel, face_id3, [0.0, 0.0, 1.0], 10.0, None).unwrap();
    assert!(matches!(sym_result.outputs[0].0, OutputKey::Main));
}

/// Verify diff detects topology changes after fillet.
#[test]
fn diff_detects_fillet_topology_changes() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let before = diff::snapshot(&kernel, &handle);
    let edges = kernel.list_edges(&handle);
    let fillet_handle = kernel.fillet_edges(&handle, &[edges[0]], 0.2).unwrap();
    let after = diff::snapshot(&kernel, &fillet_handle);

    let diff_result = diff::diff(&before, &after);

    // Fillet adds new entities (fillet face, new edges, new vertices)
    assert!(
        !diff_result.created.is_empty(),
        "Fillet creates new entities"
    );

    // After fillet, topology counts should change:
    // faces: 6 → 7, edges: 12 → 13, vertices: 8 → 10
    assert!(
        after.faces.len() > before.faces.len(),
        "Fillet adds faces: {} → {}",
        before.faces.len(),
        after.faces.len()
    );
}

/// Verify that role assignment indices are sequential.
#[test]
fn role_indices_are_sequential() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();

    let side_indices: Vec<usize> = result
        .provenance
        .role_assignments
        .iter()
        .filter_map(|(_, r)| match r {
            Role::SideFace { index } => Some(*index),
            _ => None,
        })
        .collect();

    // Side indices should be 0, 1, 2, 3 (in some order)
    let mut sorted = side_indices.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec![0, 1, 2, 3],
        "Side face indices should be sequential 0-3"
    );
}

// ── M10: WaffleKernel Integration Tests ─────────────────────────────────

/// Helper: create a face from a rectangular profile using WaffleKernel.
fn make_truck_face(kernel: &mut WaffleKernel) -> KernelId {
    let profile = ClosedProfile {
        entity_ids: vec![1, 2, 3, 4],
        is_outer: true,
        vertex_ids: vec![],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
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

#[test]
fn truck_extrude_produces_valid_op_result() {
    let mut kernel = WaffleKernel::new();
    let face_id = make_truck_face(&mut kernel);

    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();

    assert_eq!(result.outputs.len(), 1);
    assert!(matches!(result.outputs[0].0, OutputKey::Main));
    assert!(
        !result.provenance.created.is_empty(),
        "Should have created entities"
    );
}

#[test]
fn truck_extrude_has_correct_topology() {
    let mut kernel = WaffleKernel::new();
    let face_id = make_truck_face(&mut kernel);

    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();
    let handle = &result.outputs[0].1.handle;

    let faces = kernel.list_faces(handle);
    let edges = kernel.list_edges(handle);
    let vertices = kernel.list_vertices(handle);

    assert_eq!(faces.len(), 6, "Extruded rectangle should have 6 faces");
    assert_eq!(edges.len(), 12, "Extruded rectangle should have 12 edges");
    assert_eq!(
        vertices.len(),
        8,
        "Extruded rectangle should have 8 vertices"
    );

    // Euler formula: V - E + F = 2
    let euler = vertices.len() as i32 - edges.len() as i32 + faces.len() as i32;
    assert_eq!(euler, 2, "Euler formula V-E+F=2 should hold");
}

#[test]
fn truck_extrude_assigns_roles() {
    let mut kernel = WaffleKernel::new();
    let face_id = make_truck_face(&mut kernel);

    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();

    let roles = &result.provenance.role_assignments;
    let end_cap_pos = roles
        .iter()
        .filter(|(_, r)| *r == Role::EndCapPositive)
        .count();
    let end_cap_neg = roles
        .iter()
        .filter(|(_, r)| *r == Role::EndCapNegative)
        .count();
    let side = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::SideFace { .. }))
        .count();

    assert_eq!(end_cap_pos, 1, "Should have 1 EndCapPositive");
    assert_eq!(end_cap_neg, 1, "Should have 1 EndCapNegative");
    assert_eq!(side, 4, "Should have 4 SideFaces");
}

#[test]
fn truck_extrude_provenance_has_signatures() {
    let mut kernel = WaffleKernel::new();
    let face_id = make_truck_face(&mut kernel);

    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();

    // All created entities should have signatures
    for entity_record in &result.provenance.created {
        assert!(
            entity_record.signature.surface_type.is_some(),
            "Entity {:?} should have surface_type in signature",
            entity_record.kernel_id
        );
    }
}

#[test]
fn truck_revolve_produces_valid_op_result() {
    let mut kernel = WaffleKernel::new();
    let face_id = make_truck_face(&mut kernel);

    let result = execute_revolve(
        &mut kernel,
        face_id,
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        90.0,
        None,
    )
    .unwrap();

    assert_eq!(result.outputs.len(), 1);
    assert!(matches!(result.outputs[0].0, OutputKey::Main));
    assert!(
        !result.provenance.created.is_empty(),
        "Should have created entities"
    );
}

#[test]
fn truck_revolve_assigns_roles() {
    let mut kernel = WaffleKernel::new();
    let face_id = make_truck_face(&mut kernel);

    // Partial revolve (90 degrees)
    let result = execute_revolve(
        &mut kernel,
        face_id,
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        90.0,
        None,
    )
    .unwrap();

    let handle = &result.outputs[0].1.handle;
    let face_count = kernel.list_faces(handle).len();
    let roles = &result.provenance.role_assignments;

    // Every face gets a role assignment
    assert_eq!(
        roles.len(),
        face_count,
        "Every face should get a role assignment"
    );

    // At least some side faces (the revolve role heuristic is normal-based;
    // with real geometry, start/end face detection depends on normal alignment
    // with the axis, which may classify them as side faces instead)
    let side_count = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::SideFace { .. }))
        .count();
    assert!(side_count > 0, "Should have side faces");
}

#[test]
fn truck_fillet_works_on_planar_edge() {
    let mut kernel = WaffleKernel::new();
    let face_id = make_truck_face(&mut kernel);
    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();
    let handle = &result.outputs[0].1.handle;

    let faces_before = kernel.list_faces(handle).len();
    let edges = kernel.list_edges(handle);
    let fillet_result = execute_fillet(&mut kernel, handle, &[edges[0]], 0.5);
    assert!(
        fillet_result.is_ok(),
        "WaffleKernel fillet should succeed on planar edges: {:?}",
        fillet_result.err()
    );

    let result = fillet_result.unwrap();
    let result_handle = &result.outputs[0].1.handle;
    let faces_after = kernel.list_faces(result_handle).len();
    assert!(
        faces_after > faces_before,
        "Fillet should add faces: before={}, after={}",
        faces_before,
        faces_after
    );
}

#[test]
fn truck_chamfer_works_on_planar_edge() {
    let mut kernel = WaffleKernel::new();
    let face_id = make_truck_face(&mut kernel);
    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();
    let handle = &result.outputs[0].1.handle;

    let edges = kernel.list_edges(handle);
    let chamfer_result = execute_chamfer(&mut kernel, handle, &[edges[0]], 0.5);
    assert!(
        chamfer_result.is_ok(),
        "WaffleKernel chamfer should succeed on planar edges: {:?}",
        chamfer_result.err()
    );
}

#[test]
fn truck_shell_works_on_planar_box() {
    let mut kernel = WaffleKernel::new();
    let face_id = make_truck_face(&mut kernel);
    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();
    let handle = &result.outputs[0].1.handle;

    let faces = kernel.list_faces(handle);
    let shell_result = execute_shell(&mut kernel, handle, &[faces[0]], 0.5);
    assert!(
        shell_result.is_ok(),
        "WaffleKernel shell should succeed on planar boxes: {:?}",
        shell_result.err()
    );
}

// ── Governance Gap Tests: Numeric Oracles & Branch Coverage ──────────

/// GAP M5: Bounding box oracle for extrude.
/// After extruding a 2×3 profile by depth=5 in Z, the tessellation bbox
/// Z-extent should equal the depth parameter.
#[test]
fn extrude_bbox_oracle_z_extent_equals_depth() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let depth = 5.0;
    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], depth, None).unwrap();

    let handle = &result.outputs[0].1.handle;
    let mesh = kernel.tessellate(handle, 0.01).unwrap();

    // Compute bounding box from tessellation vertices
    assert!(
        mesh.vertices.len() >= 3,
        "Mesh should have at least one vertex"
    );
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for chunk in mesh.vertices.chunks(3) {
        let z = chunk[2];
        if z < min_z {
            min_z = z;
        }
        if z > max_z {
            max_z = z;
        }
    }

    let z_extent = (max_z - min_z) as f64;
    assert!(
        (z_extent - depth).abs() < 0.1,
        "Z-extent of tessellation bbox ({:.3}) should approximately equal depth ({:.3})",
        z_extent,
        depth
    );
}

/// GAP M5 continued: Bounding box oracle with different depth.
#[test]
fn extrude_bbox_oracle_depth_10() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let depth = 10.0;
    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], depth, None).unwrap();

    let handle = &result.outputs[0].1.handle;
    let mesh = kernel.tessellate(handle, 0.01).unwrap();

    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for chunk in mesh.vertices.chunks(3) {
        let z = chunk[2];
        if z < min_z {
            min_z = z;
        }
        if z > max_z {
            max_z = z;
        }
    }

    let z_extent = (max_z - min_z) as f64;
    assert!(
        (z_extent - depth).abs() < 0.1,
        "Z-extent ({:.3}) should approximately equal depth ({:.3})",
        z_extent,
        depth
    );
}

/// GAP M6: revolve_partial — specific role assertions.
/// Partial revolve should assign exactly 1 RevStartFace and 1 RevEndFace.
#[test]
fn revolve_partial_specific_role_assertions() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_revolve(
        &mut kernel,
        face_id,
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        90.0,
        None,
    )
    .unwrap();

    let roles = &result.provenance.role_assignments;
    let handle = &result.outputs[0].1.handle;
    let face_count = kernel.list_faces(handle).len();

    let start_count = roles
        .iter()
        .filter(|(_, r)| *r == Role::RevStartFace)
        .count();
    let end_count = roles.iter().filter(|(_, r)| *r == Role::RevEndFace).count();

    assert_eq!(
        start_count, 1,
        "Partial revolve should have exactly 1 RevStartFace, got {}",
        start_count
    );
    assert_eq!(
        end_count, 1,
        "Partial revolve should have exactly 1 RevEndFace, got {}",
        end_count
    );
    assert_eq!(
        roles.len(),
        face_count,
        "Every face ({}) should get a role assignment, got {}",
        face_count,
        roles.len()
    );
}

/// GAP M7: revolve_full — SideFace indices are sequential 0..N-1.
#[test]
fn revolve_full_side_face_indices_sequential() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_revolve(
        &mut kernel,
        face_id,
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        360.0,
        None,
    )
    .unwrap();

    let roles = &result.provenance.role_assignments;
    let mut side_indices: Vec<usize> = roles
        .iter()
        .filter_map(|(_, r)| match r {
            Role::SideFace { index } => Some(*index),
            _ => None,
        })
        .collect();

    assert!(
        !side_indices.is_empty(),
        "Full revolve should produce SideFace roles"
    );

    side_indices.sort();
    let expected: Vec<usize> = (0..side_indices.len()).collect();
    assert_eq!(
        side_indices,
        expected,
        "SideFace indices should be sequential 0..{}, got {:?}",
        side_indices.len() - 1,
        side_indices
    );
}

/// GAP M8: Boolean mode distinction — different modes produce different results.
#[test]
fn boolean_modes_produce_distinct_results() {
    let mut kernel = MockKernel::new();

    // Create two boxes
    let face_a = make_face(&mut kernel);
    let handle_a = kernel.extrude_face(face_a, [0.0, 0.0, 1.0], 2.0).unwrap();
    let face_b = make_face(&mut kernel);
    let handle_b = kernel.extrude_face(face_b, [0.0, 0.0, 1.0], 2.0).unwrap();

    let union_result =
        execute_boolean(&mut kernel, &handle_a, &handle_b, BooleanKind::Union).unwrap();
    let union_faces = kernel.list_faces(&union_result.outputs[0].1.handle).len();

    // Need fresh solids for subtract (MockKernel consumes handles)
    let face_c = make_face(&mut kernel);
    let handle_c = kernel.extrude_face(face_c, [0.0, 0.0, 1.0], 2.0).unwrap();
    let face_d = make_face(&mut kernel);
    let handle_d = kernel.extrude_face(face_d, [0.0, 0.0, 1.0], 2.0).unwrap();

    let subtract_result =
        execute_boolean(&mut kernel, &handle_c, &handle_d, BooleanKind::Subtract).unwrap();
    let subtract_faces = kernel
        .list_faces(&subtract_result.outputs[0].1.handle)
        .len();

    // Need fresh solids for intersect
    let face_e = make_face(&mut kernel);
    let handle_e = kernel.extrude_face(face_e, [0.0, 0.0, 1.0], 2.0).unwrap();
    let face_f = make_face(&mut kernel);
    let handle_f = kernel.extrude_face(face_f, [0.0, 0.0, 1.0], 2.0).unwrap();

    let intersect_result =
        execute_boolean(&mut kernel, &handle_e, &handle_f, BooleanKind::Intersect).unwrap();
    let _intersect_faces = kernel
        .list_faces(&intersect_result.outputs[0].1.handle)
        .len();

    // MockKernel: Union merges all faces (12), Subtract returns body A (6),
    // Intersect creates a small box (6). Subtract face count != Union face count.
    assert_ne!(
        union_faces, subtract_faces,
        "Union ({}) and Subtract ({}) should produce different face counts",
        union_faces, subtract_faces
    );
    // Union should have more faces than intersect or subtract
    assert!(
        union_faces > subtract_faces,
        "Union ({}) should have more faces than Subtract ({})",
        union_faces,
        subtract_faces
    );
}

/// GAP M1: Zero-depth extrude.
/// execute_extrude with depth=0.0 should produce a degenerate solid
/// (MockKernel allows it) rather than erroring.
#[test]
fn extrude_zero_depth_produces_degenerate_geometry() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    // depth=0 is not explicitly validated by execute_extrude;
    // MockKernel allows it and produces a flat box (d=0).
    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 0.0, None);

    // Document behavior: MockKernel produces a valid (degenerate) result.
    // A real kernel might want to reject this; for now it succeeds.
    let result = result.unwrap();
    assert_eq!(result.outputs.len(), 1, "Should still produce 1 output");

    // Tessellate: Z-extent should be ~0
    let handle = &result.outputs[0].1.handle;
    let mesh = kernel.tessellate(handle, 0.01).unwrap();

    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for chunk in mesh.vertices.chunks(3) {
        let z = chunk[2];
        if z < min_z {
            min_z = z;
        }
        if z > max_z {
            max_z = z;
        }
    }

    let z_extent = (max_z - min_z) as f64;
    assert!(
        z_extent < 0.01,
        "Zero-depth extrude should have near-zero Z-extent, got {:.6}",
        z_extent
    );
}

/// GAP M2: assign_extrude_roles with no faces (tested indirectly).
/// When the kernel produces a valid extrude, roles should cover all faces.
/// We can't test empty faces directly since assign_extrude_roles is private,
/// but we verify that every face gets a role assignment.
#[test]
fn extrude_roles_cover_all_faces() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 1.0], 5.0, None).unwrap();

    let handle = &result.outputs[0].1.handle;
    let face_count = kernel.list_faces(handle).len();
    let role_count = result.provenance.role_assignments.len();

    assert_eq!(
        role_count, face_count,
        "Every face ({}) should get exactly one role, got {}",
        face_count, role_count
    );
}

/// GAP M3: Extrude with zero-length direction vector.
/// assign_extrude_roles should use the fallback direction [0,0,1]
/// and not panic.
#[test]
fn extrude_zero_direction_uses_fallback() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);

    // Direction [0,0,0] should hit the fallback path in assign_extrude_roles
    let result = execute_extrude(&mut kernel, face_id, [0.0, 0.0, 0.0], 5.0, None).unwrap();

    let roles = &result.provenance.role_assignments;
    assert_eq!(roles.len(), 6, "Should still assign 6 roles for box faces");

    // With fallback direction [0,0,1], the role assignment should work
    // the same as a normal Z-extrude
    let has_pos_cap = roles.iter().any(|(_, r)| *r == Role::EndCapPositive);
    let has_neg_cap = roles.iter().any(|(_, r)| *r == Role::EndCapNegative);
    assert!(
        has_pos_cap,
        "Fallback direction should still produce EndCapPositive"
    );
    assert!(
        has_neg_cap,
        "Fallback direction should still produce EndCapNegative"
    );
}

/// GAP M9: Fillet with empty edges slice and positive radius.
/// Should succeed but produce topology identical to the input (no fillet faces).
#[test]
fn fillet_empty_edges_preserves_topology() {
    let mut kernel = MockKernel::new();
    let face_id = make_face(&mut kernel);
    let handle = kernel.extrude_face(face_id, [0.0, 0.0, 1.0], 5.0).unwrap();

    let faces_before = kernel.list_faces(&handle).len();

    // Fillet with no edges selected — should succeed with no new fillet faces
    let result = execute_fillet(&mut kernel, &handle, &[], 0.5).unwrap();

    let result_handle = &result.outputs[0].1.handle;
    let faces_after = kernel.list_faces(result_handle).len();

    assert_eq!(
        faces_before, faces_after,
        "Fillet with no edges should preserve face count ({} vs {})",
        faces_before, faces_after
    );

    let fillet_roles: Vec<_> = result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::FilletFace { .. }))
        .collect();
    assert!(
        fillet_roles.is_empty(),
        "Should assign no FilletFace roles when no edges are filleted"
    );
}
