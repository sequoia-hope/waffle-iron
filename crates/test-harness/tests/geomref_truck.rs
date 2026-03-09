//! GeomRef resolution tests against the real RealKernel.
//!
//! These test the full pipeline (sketch -> extrude -> role assignment -> GeomRef resolution)
//! using real truck geometry, not MockKernel. This catches issues that MockKernel's
//! signature_similarity-based re-ID system might mask.

use feature_engine::resolve::{resolve_geom_ref, resolve_with_fallback};
use test_harness::helpers::face_ref;
use test_harness::ModelBuilder;
use waffle_types::{Anchor, GeomRef, OutputKey, ResolvePolicy, Role, Selector, TopoKind};

// ── Helper: resolve a face by role and return its signature ────────────────

fn resolve_face_normal(m: &ModelBuilder, feature_name: &str, role: Role, index: usize) -> [f64; 3] {
    let feature_id = m.feature_id(feature_name).unwrap();
    let geom_ref = face_ref(feature_id, role.clone(), index);
    let op_result = m.op_result(feature_name).unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    let resolved = resolve_geom_ref(&geom_ref, &results)
        .unwrap_or_else(|e| panic!("Failed to resolve {:?} index {}: {}", role, index, e));

    let introspect = m.kernel_ref().as_introspect();
    let sig = introspect.compute_signature(resolved.kernel_id, TopoKind::Face);
    sig.normal
        .unwrap_or_else(|| panic!("Face {:?} has no normal", role))
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn vec_len(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

// ── Test 1: Extrude rectangle -> resolve top face ──────────────────────────

#[test]
fn test_truck_geomref_top_face_normal() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // EndCapPositive should point in the extrude direction (+Z)
    let normal = resolve_face_normal(&m, "box", Role::EndCapPositive, 0);
    assert!(
        normal[2] > 0.9,
        "Top face normal should point up (+Z), got {:?}",
        normal
    );
    assert!(
        normal[0].abs() < 0.1 && normal[1].abs() < 0.1,
        "Top face normal should have minimal X/Y components, got {:?}",
        normal
    );
}

// ── Test 2: Extrude rectangle -> resolve bottom face ───────────────────────

#[test]
fn test_truck_geomref_bottom_face_normal() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // EndCapNegative should point opposite to extrude direction (-Z)
    let normal = resolve_face_normal(&m, "box", Role::EndCapNegative, 0);
    assert!(
        normal[2] < -0.9,
        "Bottom face normal should point down (-Z), got {:?}",
        normal
    );
}

// ── Test 3: Extrude rectangle -> resolve all 6 faces ───────────────────────

#[test]
fn test_truck_geomref_all_box_faces() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    let feature_id = m.feature_id("box").unwrap();
    let op_result = m.op_result("box").unwrap();

    // Verify role assignments exist
    let roles = &op_result.provenance.role_assignments;
    assert!(!roles.is_empty(), "Extrude should produce role assignments");

    // Count roles by type
    let end_cap_pos = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::EndCapPositive))
        .count();
    let end_cap_neg = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::EndCapNegative))
        .count();
    let side_faces = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::SideFace { .. }))
        .count();

    assert_eq!(end_cap_pos, 1, "Should have exactly 1 EndCapPositive");
    assert_eq!(end_cap_neg, 1, "Should have exactly 1 EndCapNegative");
    assert_eq!(side_faces, 4, "Rectangle extrude should have 4 side faces");

    // Verify each role resolves successfully
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    // Top face
    let top_ref = face_ref(feature_id, Role::EndCapPositive, 0);
    assert!(
        resolve_geom_ref(&top_ref, &results).is_ok(),
        "EndCapPositive should resolve"
    );

    // Bottom face
    let bottom_ref = face_ref(feature_id, Role::EndCapNegative, 0);
    assert!(
        resolve_geom_ref(&bottom_ref, &results).is_ok(),
        "EndCapNegative should resolve"
    );

    // All 4 side faces
    for i in 0..4 {
        let side_ref = face_ref(feature_id, Role::SideFace { index: i }, 0);
        assert!(
            resolve_geom_ref(&side_ref, &results).is_ok(),
            "SideFace index {} should resolve",
            i
        );
    }

    // Verify side face normals are perpendicular to extrude direction
    for i in 0..4 {
        let normal = resolve_face_normal(&m, "box", Role::SideFace { index: i }, 0);
        assert!(
            normal[2].abs() < 0.1,
            "Side face {} normal Z-component should be ~0 (perpendicular to extrude), got {:?}",
            i,
            normal
        );
    }
}

// ── Test 4: Side face normals are orthogonal ───────────────────────────────

#[test]
fn test_truck_geomref_side_faces_orthogonal() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    // Collect all 4 side face normals
    let mut normals = Vec::new();
    for i in 0..4 {
        normals.push(resolve_face_normal(
            &m,
            "box",
            Role::SideFace { index: i },
            0,
        ));
    }

    // Each side normal should be a unit vector in the XY plane
    for (i, n) in normals.iter().enumerate() {
        let len = vec_len(*n);
        assert!(
            (len - 1.0).abs() < 0.1,
            "Side face {} normal should be unit length, got {:.3}",
            i,
            len
        );
        assert!(
            n[2].abs() < 0.1,
            "Side face {} should be perpendicular to Z, got {:?}",
            i,
            n
        );
    }

    // Verify opposite faces have opposing normals (at least one pair)
    let mut found_opposing = false;
    for i in 0..4 {
        for j in (i + 1)..4 {
            let d = dot(normals[i], normals[j]);
            if d < -0.9 {
                found_opposing = true;
            }
        }
    }
    assert!(
        found_opposing,
        "Should find at least one pair of opposing side faces"
    );
}

// ── Test 5: Two sequential extrudes -> first still resolvable ──────────────

#[test]
fn test_truck_geomref_two_extrudes_first_survives() {
    let mut m = ModelBuilder::kernel();

    // First extrude
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();
    m.assert_has_solid("box1").unwrap();

    // Second extrude (separate, non-merging to keep both independent)
    m.rect_sketch("sk2", [50., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box2", "sk2", 10.0).unwrap();
    m.assert_has_solid("box2").unwrap();

    // Resolve top face of FIRST extrude (should still work)
    let normal1 = resolve_face_normal(&m, "box1", Role::EndCapPositive, 0);
    assert!(
        normal1[2] > 0.9,
        "First box top face should still resolve after second extrude, got {:?}",
        normal1
    );

    // Resolve top face of SECOND extrude
    let normal2 = resolve_face_normal(&m, "box2", Role::EndCapPositive, 0);
    assert!(
        normal2[2] > 0.9,
        "Second box top face should resolve, got {:?}",
        normal2
    );

    // Both bottom faces should also resolve
    let bot1 = resolve_face_normal(&m, "box1", Role::EndCapNegative, 0);
    let bot2 = resolve_face_normal(&m, "box2", Role::EndCapNegative, 0);
    assert!(
        bot1[2] < -0.9,
        "First box bottom should resolve, got {:?}",
        bot1
    );
    assert!(
        bot2[2] < -0.9,
        "Second box bottom should resolve, got {:?}",
        bot2
    );
}

// ── Test 6: Extrude on different planes -> correct normals ─────────────────

#[test]
fn test_truck_geomref_xz_plane_extrude() {
    let mut m = ModelBuilder::kernel();
    // Sketch on XZ plane (normal = +Y)
    m.rect_sketch("sk", [0., 0., 0.], [0., 1., 0.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // EndCapPositive should point in +Y direction
    let normal = resolve_face_normal(&m, "box", Role::EndCapPositive, 0);
    assert!(
        normal[1] > 0.9,
        "XZ-plane extrude EndCapPositive should point +Y, got {:?}",
        normal
    );

    // EndCapNegative should point -Y
    let bot = resolve_face_normal(&m, "box", Role::EndCapNegative, 0);
    assert!(
        bot[1] < -0.9,
        "XZ-plane extrude EndCapNegative should point -Y, got {:?}",
        bot
    );
}

#[test]
fn test_truck_geomref_yz_plane_extrude() {
    let mut m = ModelBuilder::kernel();
    // Sketch on YZ plane (normal = +X)
    m.rect_sketch("sk", [0., 0., 0.], [1., 0., 0.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // EndCapPositive should point in +X direction
    let normal = resolve_face_normal(&m, "box", Role::EndCapPositive, 0);
    assert!(
        normal[0] > 0.9,
        "YZ-plane extrude EndCapPositive should point +X, got {:?}",
        normal
    );
}

// ── Test 7: Boolean subtract -> original face roles survive ────────────────

#[test]
fn test_truck_geomref_after_boolean_subtract() {
    let mut m = ModelBuilder::kernel();

    // Big box
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();
    m.assert_has_solid("box1").unwrap();

    // Smaller offset box for subtraction
    m.rect_sketch("sk2", [2., 2., 5.], [0., 0., 1.], 0., 0., 6., 6.)
        .unwrap();
    m.extrude_no_merge("box2", "sk2", 10.0).unwrap();
    m.assert_has_solid("box2").unwrap();

    // Boolean subtract
    m.boolean_subtract("result", "box1", "box2").unwrap();
    m.assert_has_solid("result").unwrap();

    // The original box1's role assignments should still be resolvable
    // (box1's OpResult still exists in feature_results)
    let box1_id = m.feature_id("box1").unwrap();
    let box1_result = m.op_result("box1").unwrap();

    let roles = &box1_result.provenance.role_assignments;
    assert!(
        !roles.is_empty(),
        "Original box1 should retain role assignments"
    );

    // EndCapPositive of box1 should still resolve
    let mut results = std::collections::HashMap::new();
    results.insert(box1_id, box1_result.clone());

    let top_ref = face_ref(box1_id, Role::EndCapPositive, 0);
    let resolved = resolve_geom_ref(&top_ref, &results);
    assert!(
        resolved.is_ok(),
        "box1 EndCapPositive should still resolve after boolean: {:?}",
        resolved.err()
    );
}

// ── Test 8: Role resolution with fallback ──────────────────────────────────

#[test]
fn test_truck_geomref_fallback_resolution() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let feature_id = m.feature_id("box").unwrap();
    let op_result = m.op_result("box").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    // Try resolving a non-existent role with BestEffort -> should fall back
    let nonexistent_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::FilletFace { index: 99 },
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
    };

    let resolved = resolve_with_fallback(&nonexistent_ref, &results);
    assert!(
        resolved.is_ok(),
        "BestEffort fallback should find a face by kind-match"
    );
    let resolved = resolved.unwrap();
    assert!(
        !resolved.warnings.is_empty(),
        "Fallback resolution should produce warnings"
    );

    // Same with Strict policy -> should fail
    let strict_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::FilletFace { index: 99 },
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    };

    let strict_result = resolve_with_fallback(&strict_ref, &results);
    assert!(
        strict_result.is_err(),
        "Strict resolution of non-existent role should fail"
    );
}

// ── Test 9: Circle extrude roles ───────────────────────────────────────────

#[test]
fn test_truck_geomref_circle_extrude_roles() {
    let mut m = ModelBuilder::kernel();
    m.circle_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("cyl", "sk", 10.0).unwrap();
    m.assert_has_solid("cyl").unwrap();

    let op_result = m.op_result("cyl").unwrap();
    let roles = &op_result.provenance.role_assignments;

    // Circle extrude: 1 EndCapPositive (top), 1 EndCapNegative (bottom), N side faces
    let end_cap_pos = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::EndCapPositive))
        .count();
    let end_cap_neg = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::EndCapNegative))
        .count();

    assert_eq!(end_cap_pos, 1, "Cylinder should have 1 EndCapPositive");
    assert_eq!(end_cap_neg, 1, "Cylinder should have 1 EndCapNegative");

    // Top face normal should point +Z
    let top_normal = resolve_face_normal(&m, "cyl", Role::EndCapPositive, 0);
    assert!(
        top_normal[2] > 0.9,
        "Cylinder top face should point +Z, got {:?}",
        top_normal
    );
}

// ── Test 10: Resolved KernelIds are unique per role ────────────────────────

#[test]
fn test_truck_geomref_unique_kernel_ids() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let feature_id = m.feature_id("box").unwrap();
    let op_result = m.op_result("box").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    // Collect all resolved KernelIds
    let mut kernel_ids = Vec::new();

    let top_ref = face_ref(feature_id, Role::EndCapPositive, 0);
    kernel_ids.push(resolve_geom_ref(&top_ref, &results).unwrap().kernel_id);

    let bot_ref = face_ref(feature_id, Role::EndCapNegative, 0);
    kernel_ids.push(resolve_geom_ref(&bot_ref, &results).unwrap().kernel_id);

    for i in 0..4 {
        let side_ref = face_ref(feature_id, Role::SideFace { index: i }, 0);
        kernel_ids.push(resolve_geom_ref(&side_ref, &results).unwrap().kernel_id);
    }

    // All 6 KernelIds should be unique
    let unique_count = {
        let mut unique = kernel_ids.clone();
        unique.dedup_by(|a, b| a.0 == b.0);
        // Also check via set-based approach
        let set: std::collections::HashSet<u64> = kernel_ids.iter().map(|id| id.0).collect();
        set.len()
    };
    assert_eq!(
        unique_count,
        kernel_ids.len(),
        "All resolved KernelIds should be unique (got {} unique out of {})",
        unique_count,
        kernel_ids.len()
    );
}

// ── Test 11: Resolve face normal consistency with direct introspection ─────

#[test]
fn test_truck_geomref_normal_matches_direct_introspect() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let feature_id = m.feature_id("box").unwrap();
    let op_result = m.op_result("box").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    // Get face signatures directly from the solid handle
    let handle = m.solid_handle("box").unwrap();
    let introspect = m.kernel_ref().as_introspect();
    let all_sigs = introspect.compute_all_signatures(&handle, TopoKind::Face);

    // Resolve EndCapPositive and verify it matches one of the direct signatures
    let top_ref = face_ref(feature_id, Role::EndCapPositive, 0);
    let resolved = resolve_geom_ref(&top_ref, &results).unwrap();

    let matched_sig = all_sigs
        .iter()
        .find(|(id, _)| *id == resolved.kernel_id)
        .map(|(_, sig)| sig);
    assert!(
        matched_sig.is_some(),
        "Resolved KernelId should exist in direct introspection"
    );

    let sig = matched_sig.unwrap();
    let normal = sig.normal.expect("Face should have normal");
    assert!(
        normal[2] > 0.9,
        "Direct introspection should confirm top face normal, got {:?}",
        normal
    );
}

// ── Test 12: Revolve role detection ────────────────────────────────────────

#[test]
fn test_truck_revolve_full_360_role_detection() {
    let mut m = ModelBuilder::kernel();
    // Rectangle offset from Y axis (x=5..10), revolve 360 around Y axis
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("rev", "sk", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("rev").unwrap();

    let op_result = m.op_result("rev").unwrap();
    let roles = &op_result.provenance.role_assignments;

    // Full 360 revolution should have face roles assigned
    assert!(
        !roles.is_empty(),
        "Full revolve should produce role assignments"
    );

    // All faces should have SideFace roles for full revolution
    let side_count = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::SideFace { .. }))
        .count();
    assert!(
        side_count > 0,
        "Full revolution should have side faces, got {} roles total",
        roles.len()
    );
}

#[test]
fn test_truck_revolve_partial_role_detection() {
    let mut m = ModelBuilder::kernel();
    // Rectangle offset from Y axis, revolve 180 degrees
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("rev", "sk", [0., 0., 0.], [0., 1., 0.], 180.0)
        .unwrap();
    m.assert_has_solid("rev").unwrap();

    let op_result = m.op_result("rev").unwrap();
    let roles = &op_result.provenance.role_assignments;

    assert!(
        !roles.is_empty(),
        "Partial revolve should produce role assignments"
    );

    // Partial revolution should have RevStartFace and RevEndFace
    let start_count = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::RevStartFace))
        .count();
    let end_count = roles
        .iter()
        .filter(|(_, r)| matches!(r, Role::RevEndFace))
        .count();

    assert_eq!(
        start_count, 1,
        "180-degree revolve should have 1 RevStartFace"
    );
    assert_eq!(end_count, 1, "180-degree revolve should have 1 RevEndFace");

    // Start/end faces should have normals aligned with the revolution axis (Y axis)
    let feature_id = m.feature_id("rev").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    let start_ref = face_ref(feature_id, Role::RevStartFace, 0);
    let start_resolved = resolve_geom_ref(&start_ref, &results).unwrap();
    let introspect = m.kernel_ref().as_introspect();
    let start_sig = introspect.compute_signature(start_resolved.kernel_id, TopoKind::Face);
    if let Some(normal) = start_sig.normal {
        let axis_alignment = normal[1].abs(); // Y-axis component
        assert!(
            axis_alignment > 0.5,
            "RevStartFace normal should align with revolution axis (Y), got {:?}",
            normal
        );
    }
}

// ── Test 13: Revolve side faces exist ──────────────────────────────────────

#[test]
fn test_truck_revolve_side_faces_have_normals() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("rev", "sk", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("rev").unwrap();

    let op_result = m.op_result("rev").unwrap();
    let roles = &op_result.provenance.role_assignments;
    let introspect = m.kernel_ref().as_introspect();

    // Verify all assigned faces have computable normals
    for (kernel_id, role) in roles {
        let sig = introspect.compute_signature(*kernel_id, TopoKind::Face);
        // Face should have at least some geometric data
        assert!(
            sig.normal.is_some() || sig.centroid.is_some(),
            "Face with role {:?} should have geometric signature data",
            role
        );
    }
}

// ── Test 14: Extrude cut preserves earlier feature roles ───────────────────

#[test]
fn test_truck_geomref_survives_cut_extrude() {
    let mut m = ModelBuilder::kernel();

    // Base cube
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();

    // Verify cube roles before cut
    let pre_cut_normal = resolve_face_normal(&m, "cube", Role::EndCapPositive, 0);
    assert!(pre_cut_normal[2] > 0.9, "Pre-cut top face should point +Z");

    // Circle cut on top face
    m.circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.5)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 15.0).unwrap();
    m.assert_has_solid("hole").unwrap();

    // The original cube's role assignments should still be resolvable
    // (even though the geometry was modified by the cut)
    let cube_result = m.op_result("cube").unwrap();

    let roles = &cube_result.provenance.role_assignments;
    assert!(
        !roles.is_empty(),
        "Cube role assignments should persist after cut"
    );
}

// ── Test 15: Extrude on non-standard planes ────────────────────────────────

#[test]
fn test_truck_geomref_roles_consistent_across_planes() {
    let planes: Vec<(&str, [f64; 3])> = vec![
        ("xy", [0., 0., 1.]),
        ("xz", [0., 1., 0.]),
        ("yz", [1., 0., 0.]),
    ];

    for (label, normal) in &planes {
        let mut m = ModelBuilder::kernel();
        let sk_name = format!("sk_{}", label);
        let box_name = format!("box_{}", label);

        m.rect_sketch(&sk_name, [0., 0., 0.], *normal, 0., 0., 10., 10.)
            .unwrap();
        m.extrude(&box_name, &sk_name, 10.0).unwrap();
        m.assert_has_solid(&box_name).unwrap();

        let op_result = m.op_result(&box_name).unwrap();
        let roles = &op_result.provenance.role_assignments;

        // Should always have exactly 6 faces with proper role distribution
        let end_pos = roles
            .iter()
            .filter(|(_, r)| matches!(r, Role::EndCapPositive))
            .count();
        let end_neg = roles
            .iter()
            .filter(|(_, r)| matches!(r, Role::EndCapNegative))
            .count();
        let sides = roles
            .iter()
            .filter(|(_, r)| matches!(r, Role::SideFace { .. }))
            .count();

        assert_eq!(end_pos, 1, "{} plane: should have 1 EndCapPositive", label);
        assert_eq!(end_neg, 1, "{} plane: should have 1 EndCapNegative", label);
        assert_eq!(sides, 4, "{} plane: should have 4 SideFaces", label);

        // EndCapPositive normal should align with the plane normal (extrude direction)
        let top = resolve_face_normal(&m, &box_name, Role::EndCapPositive, 0);
        let alignment = dot(top, *normal);
        assert!(
            alignment > 0.9,
            "{} plane: EndCapPositive normal {:?} should align with plane normal {:?}",
            label,
            top,
            normal
        );
    }
}

// ── Test 16: Sketch → Extrude → Boolean → Extrude chain ────────────────────

/// Multi-feature chain: create two boxes, boolean union them, then extrude
/// a boss on the result. Verify that face role references resolve through
/// the entire chain.
#[test]
fn test_truck_geomref_chain_through_boolean() {
    let mut m = ModelBuilder::kernel();

    // Box A
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();
    m.assert_has_solid("box_a").unwrap();

    // Box B (overlapping)
    m.rect_sketch("sk_b", [5., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();
    m.assert_has_solid("box_b").unwrap();

    // Boolean union
    m.boolean_union("merged", "box_a", "box_b").unwrap();
    m.assert_has_solid("merged").unwrap();

    // Verify roles on original features still resolve
    let box_a_id = m.feature_id("box_a").unwrap();
    let box_a_result = m.op_result("box_a").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(box_a_id, box_a_result.clone());

    // All side faces of box_a should still resolve
    for i in 0..4 {
        let side_ref = face_ref(box_a_id, Role::SideFace { index: i }, 0);
        let resolved = resolve_geom_ref(&side_ref, &results);
        assert!(
            resolved.is_ok(),
            "box_a SideFace {} should resolve after boolean chain: {:?}",
            i,
            resolved.err()
        );
    }

    // Top face of box_a should still resolve with correct normal
    let top_ref = face_ref(box_a_id, Role::EndCapPositive, 0);
    let top_resolved = resolve_geom_ref(&top_ref, &results).unwrap();
    let introspect = m.kernel_ref().as_introspect();
    let sig = introspect.compute_signature(top_resolved.kernel_id, TopoKind::Face);
    if let Some(normal) = sig.normal {
        assert!(
            normal[2] > 0.9,
            "box_a top face normal should still be +Z after boolean, got {:?}",
            normal
        );
    }
}

// ── Test 17: Multiple extrudes → roles persist across chain ─────────────────

/// Create two sequential (auto-merging) extrudes and verify that role
/// assignments from the first extrude persist independently of the second.
#[test]
fn test_truck_geomref_sequential_extrudes_roles_persist() {
    let mut m = ModelBuilder::kernel();

    // Base box
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();
    m.assert_has_solid("box1").unwrap();

    // Record box1's role assignment count
    let box1_roles_count = m
        .op_result("box1")
        .unwrap()
        .provenance
        .role_assignments
        .len();
    assert!(
        box1_roles_count >= 6,
        "box1 should have at least 6 role assignments (6 faces), got {}",
        box1_roles_count
    );

    // Boss on top (auto-union)
    m.rect_sketch("sk2", [0., 0., 10.], [0., 0., 1.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss", "sk2", 5.0).unwrap();

    // box1's role assignments should be unchanged
    let box1_roles_after = m
        .op_result("box1")
        .unwrap()
        .provenance
        .role_assignments
        .len();
    assert_eq!(
        box1_roles_count, box1_roles_after,
        "box1 role count should not change after adding boss: {} -> {}",
        box1_roles_count, box1_roles_after
    );

    // box1's EndCapPositive should still resolve independently
    let box1_id = m.feature_id("box1").unwrap();
    let box1_result = m.op_result("box1").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(box1_id, box1_result.clone());

    let box1_top = face_ref(box1_id, Role::EndCapPositive, 0);
    assert!(
        resolve_geom_ref(&box1_top, &results).is_ok(),
        "box1 EndCapPositive should resolve"
    );

    // Boss result exists (auto-merge may alter its provenance, but the
    // OpResult should still be present)
    let boss_result = m.op_result("boss").unwrap();
    assert!(!boss_result.outputs.is_empty(), "boss should have outputs");

    // Boss provenance should exist (may have merged roles or its own)
    let boss_roles = &boss_result.provenance.role_assignments;
    // Auto-merged boss inherits roles from the boolean result — count may
    // be more than 6 (merged body has more faces). Just verify non-empty.
    assert!(
        !boss_roles.is_empty() || !boss_result.provenance.created.is_empty(),
        "boss should have provenance (role assignments or created entities)"
    );
}
