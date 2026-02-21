//! Multi-body workflow tests.
//!
//! Tests for independent extrudes (no auto-merge), boolean combinations,
//! suppress/undo interactions with multiple bodies, and TruckKernel variants.

use test_harness::ModelBuilder;

// ── Two independent extrudes ─────────────────────────────────────────────

/// Two independent extrudes (no auto-merge) should both produce solids.
#[test]
fn test_two_independent_extrudes_mock() {
    let mut m = ModelBuilder::mock();

    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [20., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box2", "sk2", 10.0).unwrap();

    m.assert_has_solid("box1").unwrap();
    m.assert_has_solid("box2").unwrap();
    m.assert_feature_count(4).unwrap(); // 2 sketches + 2 extrudes
    m.assert_no_errors().unwrap();
}

// ── Three independent extrudes ───────────────────────────────────────────

/// Three independent extrudes should all produce solids.
#[test]
fn test_three_independent_extrudes_mock() {
    let mut m = ModelBuilder::mock();

    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [20., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box2", "sk2", 10.0).unwrap();

    m.rect_sketch("sk3", [40., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box3", "sk3", 10.0).unwrap();

    m.assert_has_solid("box1").unwrap();
    m.assert_has_solid("box2").unwrap();
    m.assert_has_solid("box3").unwrap();
    m.assert_feature_count(6).unwrap(); // 3 sketches + 3 extrudes
    m.assert_no_errors().unwrap();
}

// ── Independent extrudes then boolean union ──────────────────────────────

/// Independent extrudes then boolean union should merge them.
#[test]
fn test_independent_extrudes_then_union_mock() {
    let mut m = ModelBuilder::mock();

    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box2", "sk2", 10.0).unwrap();

    m.boolean_union("merged", "box1", "box2").unwrap();
    m.assert_has_solid("merged").unwrap();
    m.assert_feature_count(5).unwrap(); // 2 sketches + 2 extrudes + 1 boolean
    m.assert_no_errors().unwrap();
}

// ── Suppress one of two bodies ───────────────────────────────────────────

/// Suppress one of two independent bodies — the other should remain.
#[test]
fn test_suppress_one_of_two_bodies_mock() {
    let mut m = ModelBuilder::mock();

    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [20., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box2", "sk2", 10.0).unwrap();

    m.suppress("box1").unwrap();
    // box2 is independent, should still have solid
    m.assert_has_solid("box2").unwrap();
    // Feature count unchanged — suppressed features still exist in tree
    m.assert_feature_count(4).unwrap();
}

// ── Undo after boolean union ─────────────────────────────────────────────

/// Undo after boolean union should restore two separate bodies.
#[test]
fn test_undo_after_boolean_union_mock() {
    let mut m = ModelBuilder::mock();

    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box2", "sk2", 10.0).unwrap();

    m.boolean_union("merged", "box1", "box2").unwrap();
    m.assert_feature_count(5).unwrap();

    m.undo().unwrap();
    m.assert_feature_count(4).unwrap(); // Back to 2 sketches + 2 extrudes
    m.assert_has_solid("box1").unwrap();
    m.assert_has_solid("box2").unwrap();
}

// ── TruckKernel: two independent extrudes ────────────────────────────────

/// Two independent extrudes with TruckKernel — both should produce meshes.
#[test]
fn test_two_independent_extrudes_truck() {
    let mut m = ModelBuilder::truck();

    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [20., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box2", "sk2", 10.0).unwrap();

    m.assert_has_solid("box1").unwrap();
    m.assert_has_solid("box2").unwrap();

    // Both should tessellate
    let mesh1 = m.tessellate("box1").unwrap();
    let mesh2 = m.tessellate("box2").unwrap();
    assert!(!mesh1.indices.is_empty(), "box1 should have mesh");
    assert!(!mesh2.indices.is_empty(), "box2 should have mesh");
}
