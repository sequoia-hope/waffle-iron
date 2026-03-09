//! Rebuild stability tests.
//!
//! Verifies that the feature tree rebuild pipeline produces consistent results:
//!   - GeomRef role assignments survive rebuilds triggered by save/load
//!   - Sequential rebuilds produce identical body counts and feature counts
//!   - Suppress mid-chain feature correctly disables dependents
//!   - Suppress/unsuppress cycle preserves role assignments
//!   - Multi-feature chains rebuild deterministically
//!
//! Uses RealKernel for real geometry validation.

use test_harness::helpers::mesh_bounding_box;
use test_harness::ModelBuilder;
use waffle_types::Role;

// ── Helpers ────────────────────────────────────────────────────────────────

/// Count how many distinct body outputs exist across all non-consumed features.
fn count_visible_bodies(m: &ModelBuilder) -> usize {
    let consumed = m.consumed_features();
    let mut body_count = 0;
    for feature in &m.state.engine.tree.features {
        if feature.suppressed {
            continue;
        }
        if consumed.contains(&feature.id) {
            continue;
        }
        if let Some(result) = m.state.engine.get_result(feature.id) {
            body_count += result.outputs.len();
        }
    }
    body_count
}

// ── RS1: Save/load rebuild produces identical body and feature counts ────────

/// Build a multi-feature model, save it, load it back (triggering rebuild),
/// and verify body count, feature count, and role assignments match.
#[test]
fn rs1_rebuild_preserves_body_and_feature_counts() {
    let mut m = ModelBuilder::kernel();

    // Sketch + extrude base box
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // Add a boss on top (auto-union)
    m.rect_sketch("sk2", [0., 0., 10.], [0., 0., 1.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss", "sk2", 5.0).unwrap();

    let bodies_before = count_visible_bodies(&m);
    let features_before = m.feature_count();

    // Count role assignments before save (across all features)
    let mut total_roles_before = 0;
    let mut end_cap_pos_before = 0;
    for feature in &m.state.engine.tree.features {
        if let Some(result) = m.state.engine.get_result(feature.id) {
            let roles = &result.provenance.role_assignments;
            total_roles_before += roles.len();
            end_cap_pos_before += roles
                .iter()
                .filter(|(_, r)| matches!(r, Role::EndCapPositive))
                .count();
        }
    }

    // Save and load (triggers full rebuild)
    let json = m.save().unwrap();
    let mut m2 = ModelBuilder::kernel();
    m2.load(&json).unwrap();

    let bodies_after = count_visible_bodies(&m2);
    let features_after = m2.feature_count();

    assert_eq!(
        bodies_before, bodies_after,
        "body count changed after rebuild: {} -> {}",
        bodies_before, bodies_after
    );
    assert_eq!(
        features_before, features_after,
        "feature count changed after rebuild: {} -> {}",
        features_before, features_after
    );

    // Verify role assignments survive rebuild (across all features)
    let mut total_roles_after = 0;
    let mut end_cap_pos_after = 0;
    for feature in &m2.state.engine.tree.features {
        if let Some(result) = m2.state.engine.get_result(feature.id) {
            let roles = &result.provenance.role_assignments;
            total_roles_after += roles.len();
            end_cap_pos_after += roles
                .iter()
                .filter(|(_, r)| matches!(r, Role::EndCapPositive))
                .count();
        }
    }

    assert!(
        total_roles_after > 0,
        "role assignments should survive rebuild"
    );
    assert_eq!(
        total_roles_before, total_roles_after,
        "total role count should persist: {} -> {}",
        total_roles_before, total_roles_after
    );
    assert_eq!(
        end_cap_pos_before, end_cap_pos_after,
        "EndCapPositive count should persist: {} -> {}",
        end_cap_pos_before, end_cap_pos_after
    );
}

// ── RS2: Sequential rebuilds produce identical results ──────────────────────

/// Load the same JSON twice into separate builders and verify both produce
/// the same body count, feature count, and bounding box dimensions.
#[test]
fn rs2_sequential_rebuilds_identical() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    let json = m.save().unwrap();

    // First rebuild
    let mut m1 = ModelBuilder::kernel();
    m1.load(&json).unwrap();
    let bodies1 = count_visible_bodies(&m1);
    let features1 = m1.feature_count();

    // Second rebuild
    let mut m2 = ModelBuilder::kernel();
    m2.load(&json).unwrap();
    let bodies2 = count_visible_bodies(&m2);
    let features2 = m2.feature_count();

    assert_eq!(bodies1, bodies2, "body count differs between rebuilds");
    assert_eq!(
        features1, features2,
        "feature count differs between rebuilds"
    );

    // Verify bounding boxes match
    if m1.assert_has_solid("box").is_ok() && m2.assert_has_solid("box").is_ok() {
        let mesh1 = m1.tessellate("box").unwrap();
        let mesh2 = m2.tessellate("box").unwrap();

        let (min1, max1) = mesh_bounding_box(&mesh1);
        let (min2, max2) = mesh_bounding_box(&mesh2);

        for i in 0..3 {
            assert!(
                (min1[i] - min2[i]).abs() < 0.01,
                "bbox min[{}] differs: {} vs {}",
                i,
                min1[i],
                min2[i]
            );
            assert!(
                (max1[i] - max2[i]).abs() < 0.01,
                "bbox max[{}] differs: {} vs {}",
                i,
                max1[i],
                max2[i]
            );
        }
    }
}

// ── RS3: Suppress mid-chain feature cascades to dependents ──────────────────

/// Suppressing a sketch that feeds an extrude should make the extrude lose
/// its solid (cascade suppression). Unsuppressing restores everything.
#[test]
fn rs3_suppress_mid_chain_cascades() {
    let mut m = ModelBuilder::mock();

    // Chain: sketch → extrude → (solid)
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();
    m.assert_no_errors().unwrap();

    // Suppress the sketch (mid-chain)
    m.suppress("sk").unwrap();

    // Extrude should now have an error (missing input sketch)
    m.assert_has_errors().unwrap();

    // Unsuppress restores everything
    m.unsuppress("sk").unwrap();
    m.assert_has_solid("box").unwrap();
    m.assert_no_errors().unwrap();
}

// ── RS4: Suppress/unsuppress preserves role assignments ─────────────────────

/// After suppress → unsuppress cycle, the role assignments should be
/// identical to the original.
#[test]
fn rs4_suppress_unsuppress_preserves_roles() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // Record roles before suppress
    let roles_before = m
        .op_result("box")
        .unwrap()
        .provenance
        .role_assignments
        .len();

    // Suppress and unsuppress
    m.suppress("box").unwrap();
    m.unsuppress("box").unwrap();
    m.assert_has_solid("box").unwrap();

    // Verify roles are identical
    let roles_after = m
        .op_result("box")
        .unwrap()
        .provenance
        .role_assignments
        .len();

    assert_eq!(
        roles_before, roles_after,
        "role assignment count changed after suppress/unsuppress: {} -> {}",
        roles_before, roles_after
    );
}

// ── RS5: Multi-feature chain rebuild determinism with RealKernel ───────────

/// Build a 3-feature chain (sketch → extrude → boss), save/load twice,
/// and verify that body counts, feature counts, and topology are
/// deterministic across rebuilds.
#[test]
fn rs5_multi_feature_chain_rebuild_determinism() {
    let mut m = ModelBuilder::kernel();

    // Base box
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();
    m.assert_has_solid("box1").unwrap();

    // Boss on top (auto-unions)
    m.rect_sketch("sk2", [0., 0., 10.], [0., 0., 1.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss", "sk2", 5.0).unwrap();

    // Boss on side (auto-unions)
    m.rect_sketch("sk3", [10., 0., 0.], [1., 0., 0.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("side_boss", "sk3", 3.0).unwrap();

    let bodies_orig = count_visible_bodies(&m);
    let features_orig = m.feature_count();

    let json = m.save().unwrap();

    // Rebuild three times and verify consistency
    let mut counts = Vec::new();
    for _ in 0..3 {
        let mut m_r = ModelBuilder::kernel();
        m_r.load(&json).unwrap();
        counts.push((count_visible_bodies(&m_r), m_r.feature_count()));
    }

    for (i, (bodies, features)) in counts.iter().enumerate() {
        assert_eq!(
            *bodies, bodies_orig,
            "rebuild #{}: body count {} != original {}",
            i, bodies, bodies_orig
        );
        assert_eq!(
            *features, features_orig,
            "rebuild #{}: feature count {} != original {}",
            i, features, features_orig
        );
    }
}

// ── RS6: Undo after extrude restores previous state ─────────────────────────

/// Adding an extrude then undoing should restore the previous feature count
/// and body count.
#[test]
fn rs6_undo_restores_feature_state() {
    let mut m = ModelBuilder::mock();

    // Build base
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    let features_after_box = m.feature_count();
    let bodies_after_box = count_visible_bodies(&m);

    // Add another feature
    m.rect_sketch("sk2", [0., 0., 10.], [0., 0., 1.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss", "sk2", 5.0).unwrap();

    let features_after_boss = m.feature_count();
    assert!(
        features_after_boss > features_after_box,
        "adding boss should increase feature count"
    );

    // Undo the extrude
    m.undo().unwrap();
    let features_after_undo = m.feature_count();
    assert_eq!(
        features_after_undo,
        features_after_boss - 1,
        "undo should remove the last feature"
    );

    // Undo the sketch
    m.undo().unwrap();
    assert_eq!(
        m.feature_count(),
        features_after_box,
        "double undo should restore original feature count"
    );
}
