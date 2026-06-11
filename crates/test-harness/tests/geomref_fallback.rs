//! GeomRef resolution fallback tests.
//!
//! Tests the fallback behavior of GeomRef resolution when the primary selector
//! (Role or Signature) fails:
//!   - BestEffort policy falls back to kind-matching
//!   - Strict policy fails cleanly
//!   - Signature-based resolution finds closest match
//!   - Query-based resolution with filters and tie-breaking
//!   - Resolution survives across boolean operations
//!   - Save/load round-trip preserves role assignments
//!
//! Uses MockKernel for fast, deterministic tests unless real geometry is needed.

use feature_engine::resolve::{resolve_geom_ref, resolve_with_fallback};
use test_harness::helpers::face_ref;
use test_harness::ModelBuilder;
use waffle_types::{Anchor, GeomRef, OutputKey, ResolvePolicy, Role, Selector, TopoKind};

// ── GF1: Role resolution returns same entity as Signature resolution ────────

/// Verify that resolving a face by Role and then by its computed Signature
/// returns the same KernelId.
#[test]
fn gf1_role_and_signature_resolve_same_entity() {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    let feature_id = m.feature_id("box").unwrap();
    let op_result = m.op_result("box").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    // Resolve top face by Role
    let role_ref = face_ref(feature_id, Role::EndCapPositive, 0);
    let role_resolved = resolve_geom_ref(&role_ref, &results).unwrap();

    // Get the signature of the resolved face from introspection
    let introspect = m.kernel_ref().as_introspect();
    let sig = introspect.compute_signature(role_resolved.kernel_id, TopoKind::Face);

    // Resolve by Signature using the computed signature
    let sig_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Signature { signature: sig },
        policy: ResolvePolicy::Strict,
    };

    let sig_resolved = resolve_geom_ref(&sig_ref, &results).unwrap();

    assert_eq!(
        role_resolved.kernel_id, sig_resolved.kernel_id,
        "Role and Signature resolution should return the same KernelId"
    );
    assert!(
        sig_resolved.warnings.is_empty(),
        "Exact signature match should produce no warnings"
    );
}

// ── GF2: BestEffort falls back to kind-match on missing role ────────────────

/// When the requested Role doesn't exist, BestEffort policy should fall back
/// to returning any face of the same kind, with a warning.
#[test]
fn gf2_best_effort_falls_back_on_missing_role() {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let feature_id = m.feature_id("box").unwrap();
    let op_result = m.op_result("box").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    // Try resolving a non-existent role with BestEffort
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
        "BestEffort should succeed via kind-match fallback"
    );
    let resolved = resolved.unwrap();
    assert!(
        !resolved.warnings.is_empty(),
        "Fallback resolution should produce warnings"
    );
}

// ── GF3: Strict policy fails cleanly on missing role ────────────────────────

/// When the requested Role doesn't exist, Strict policy should fail with
/// a clear error message.
#[test]
fn gf3_strict_fails_on_missing_role() {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let feature_id = m.feature_id("box").unwrap();
    let op_result = m.op_result("box").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    // Try resolving a non-existent role with Strict
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

    let result = resolve_with_fallback(&strict_ref, &results);
    assert!(
        result.is_err(),
        "Strict policy should fail on non-existent role"
    );
}

// ── GF4: Role resolution on post-boolean geometry ───────────────────────────

/// After a boolean union, the original feature's role assignments should
/// still resolve, and the boolean result should have its own provenance.
#[test]
fn gf4_role_resolution_after_boolean_union() {
    let mut m = ModelBuilder::kernel_v2();

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

    // Original box_a roles should still resolve against box_a's OpResult
    let box_a_id = m.feature_id("box_a").unwrap();
    let box_a_result = m.op_result("box_a").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(box_a_id, box_a_result.clone());

    let top_ref = face_ref(box_a_id, Role::EndCapPositive, 0);
    let resolved = resolve_geom_ref(&top_ref, &results);
    assert!(
        resolved.is_ok(),
        "box_a EndCapPositive should resolve after boolean: {:?}",
        resolved.err()
    );

    // Box B roles also survive
    let box_b_id = m.feature_id("box_b").unwrap();
    let box_b_result = m.op_result("box_b").unwrap();
    results.insert(box_b_id, box_b_result.clone());

    let top_ref_b = face_ref(box_b_id, Role::EndCapPositive, 0);
    let resolved_b = resolve_geom_ref(&top_ref_b, &results);
    assert!(
        resolved_b.is_ok(),
        "box_b EndCapPositive should resolve after boolean: {:?}",
        resolved_b.err()
    );
}

// ── GF5: Signature resolution with perturbed signature ──────────────────────

/// When a signature is slightly perturbed (e.g., centroid shifted), the
/// Signature selector should still find the closest match with BestEffort.
#[test]
fn gf5_signature_resolution_with_perturbed_values() {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    let feature_id = m.feature_id("box").unwrap();
    let op_result = m.op_result("box").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    // Get the actual top face signature
    let role_ref = face_ref(feature_id, Role::EndCapPositive, 0);
    let role_resolved = resolve_geom_ref(&role_ref, &results).unwrap();
    let introspect = m.kernel_ref().as_introspect();
    let mut sig = introspect.compute_signature(role_resolved.kernel_id, TopoKind::Face);

    // Perturb the centroid slightly (simulate geometry changes after edit)
    if let Some(ref mut centroid) = sig.centroid {
        centroid[0] += 0.01;
        centroid[1] += 0.01;
    }

    // Resolve with perturbed signature using BestEffort
    let perturbed_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Signature {
            signature: sig.clone(),
        },
        policy: ResolvePolicy::BestEffort,
    };

    let resolved = resolve_geom_ref(&perturbed_ref, &results).unwrap();
    // Should still find the same top face (closest signature match)
    assert_eq!(
        role_resolved.kernel_id, resolved.kernel_id,
        "Slightly perturbed signature should still match the same face"
    );
}

// ── GF6: Query-based resolution with NormalDirection filter ─────────────────

/// The Query selector with a NormalDirection filter should find faces
/// pointing in the requested direction.
#[test]
fn gf6_query_resolution_by_normal_direction() {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    let feature_id = m.feature_id("box").unwrap();
    let op_result = m.op_result("box").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    // Query for the face pointing +Z (should be EndCapPositive)
    let query_ref = m.select_face_by_normal("box", [0., 0., 1.], 0.2).unwrap();
    let query_resolved = resolve_geom_ref(&query_ref, &results).unwrap();

    // Also resolve by role
    let role_ref = face_ref(feature_id, Role::EndCapPositive, 0);
    let role_resolved = resolve_geom_ref(&role_ref, &results).unwrap();

    assert_eq!(
        query_resolved.kernel_id, role_resolved.kernel_id,
        "Query by +Z normal should find the same face as EndCapPositive role"
    );
}

// ── GF7: BestEffort index clamping on out-of-range index ────────────────────

/// When a Role selector has an out-of-range index, BestEffort should clamp
/// to the last available match.
#[test]
fn gf7_best_effort_clamps_out_of_range_index() {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let feature_id = m.feature_id("box").unwrap();
    let op_result = m.op_result("box").unwrap();
    let mut results = std::collections::HashMap::new();
    results.insert(feature_id, op_result.clone());

    // EndCapPositive exists at index 0, but ask for index 5 (out of range)
    let oob_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 5,
        },
        policy: ResolvePolicy::BestEffort,
    };

    let resolved = resolve_geom_ref(&oob_ref, &results);
    assert!(
        resolved.is_ok(),
        "BestEffort should clamp out-of-range index"
    );
    let resolved = resolved.unwrap();
    assert!(
        !resolved.warnings.is_empty(),
        "Clamped resolution should produce warnings"
    );

    // Same with Strict should fail
    let strict_oob_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 5,
        },
        policy: ResolvePolicy::Strict,
    };

    let strict_result = resolve_geom_ref(&strict_oob_ref, &results);
    assert!(
        strict_result.is_err(),
        "Strict should fail on out-of-range index"
    );
}

// ── GF8: Save/load round-trip preserves role assignments ────────────────────

/// After saving and loading a model, role assignments should persist and
/// resolve to geometrically equivalent faces. Uses feature tree iteration
/// since load() remaps names from the feature tree (not ModelBuilder names).
#[test]
fn gf8_save_load_preserves_role_resolution() {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // Collect role stats before save by scanning all features with results
    let mut total_roles_before = 0;
    let mut end_cap_pos_before = 0;
    let mut side_faces_before = 0;
    for feature in &m.state.engine.tree.features {
        if let Some(result) = m.state.engine.get_result(feature.id) {
            let roles = &result.provenance.role_assignments;
            total_roles_before += roles.len();
            end_cap_pos_before += roles
                .iter()
                .filter(|(_, r)| matches!(r, Role::EndCapPositive))
                .count();
            side_faces_before += roles
                .iter()
                .filter(|(_, r)| matches!(r, Role::SideFace { .. }))
                .count();
        }
    }
    assert!(
        total_roles_before > 0,
        "should have role assignments before save"
    );

    let features_before = m.feature_count();

    // Save
    let json = m.save().unwrap();

    // Load into fresh builder (triggers rebuild)
    let mut m2 = ModelBuilder::kernel_v2();
    m2.load(&json).unwrap();

    assert_eq!(
        features_before,
        m2.feature_count(),
        "feature count should match after load"
    );

    // Collect role stats after load
    let mut total_roles_after = 0;
    let mut end_cap_pos_after = 0;
    let mut side_faces_after = 0;
    for feature in &m2.state.engine.tree.features {
        if let Some(result) = m2.state.engine.get_result(feature.id) {
            let roles = &result.provenance.role_assignments;
            total_roles_after += roles.len();
            end_cap_pos_after += roles
                .iter()
                .filter(|(_, r)| matches!(r, Role::EndCapPositive))
                .count();
            side_faces_after += roles
                .iter()
                .filter(|(_, r)| matches!(r, Role::SideFace { .. }))
                .count();
        }
    }

    assert_eq!(
        total_roles_before, total_roles_after,
        "total role assignment count should persist across save/load: {} -> {}",
        total_roles_before, total_roles_after
    );
    assert_eq!(
        end_cap_pos_before, end_cap_pos_after,
        "EndCapPositive count should persist"
    );
    assert_eq!(
        side_faces_before, side_faces_after,
        "SideFace count should persist"
    );
}
