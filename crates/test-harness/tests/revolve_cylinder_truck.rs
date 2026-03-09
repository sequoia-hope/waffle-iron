//! RealKernel integration tests for revolve operations and cylinder geometry.
//!
//! Tests partial revolves, cylinder topology, revolve+boolean combinations,
//! and save/load roundtrips with complex feature trees.

use test_harness::helpers::{mesh_bounding_box, mesh_volume};
use test_harness::ModelBuilder;

// ── Partial Revolve Tests ────────────────────────────────────────────────────

#[test]
fn test_truck_partial_revolve_180() {
    let mut m = ModelBuilder::kernel();
    // Rectangle at x=5..10, y=0..5 on XY plane
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    // Revolve 180° around Y axis → sweeps from +X through +Z to -X
    m.revolve("half", "sk", [0., 0., 0.], [0., 1., 0.], 180.0)
        .unwrap();
    m.assert_has_solid("half").unwrap();

    let mesh = m.tessellate("half").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "180° revolve should produce non-empty mesh"
    );

    // Bounding box: revolving x=5..10 through 180° around Y sweeps to x=-10..10
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    assert!(
        bb_min[0] < -4.0,
        "180° revolve should extend into -X (min_x={:.1})",
        bb_min[0]
    );
    assert!(
        bb_max[0] > 4.0,
        "180° revolve should extend into +X (max_x={:.1})",
        bb_max[0]
    );
}

#[test]
fn test_truck_partial_revolve_90() {
    let mut m = ModelBuilder::kernel();
    // Rectangle at x=5..10, y=0..5 on XY plane
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    // Revolve 90° around Y axis → sweeps from +X to +Z quadrant only
    m.revolve("quarter", "sk", [0., 0., 0.], [0., 1., 0.], 90.0)
        .unwrap();
    m.assert_has_solid("quarter").unwrap();

    let mesh = m.tessellate("quarter").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "90° revolve should produce non-empty mesh"
    );

    // Quarter revolve sweeps from +X into either +Z or -Z (depends on truck convention).
    // The solid should have significant extent along the Z axis in one direction.
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    let z_extent = bb_max[2] - bb_min[2];
    assert!(
        z_extent > 4.0 || bb_min[2] < -4.0 || bb_max[2] > 4.0,
        "90° revolve should extend along Z axis (min_z={:.1}, max_z={:.1})",
        bb_min[2],
        bb_max[2]
    );
}

// ── Partial Revolve Role Assignment Tests ───────────────────────────────────

#[test]
fn test_partial_revolve_90_roles() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("rev", "sk", [0., 0., 0.], [0., 1., 0.], 90.0)
        .unwrap();
    m.assert_has_solid("rev").unwrap();

    // A 90° revolve should assign start/end cap roles
    let op = m.op_result("rev").unwrap();
    test_harness::assertions::assert_role_assigned(
        op,
        &waffle_types::Role::RevStartFace,
        "90° revolve start face",
    )
    .unwrap();
    test_harness::assertions::assert_role_assigned(
        op,
        &waffle_types::Role::RevEndFace,
        "90° revolve end face",
    )
    .unwrap();
}

#[test]
fn test_partial_revolve_180_roles() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("rev", "sk", [0., 0., 0.], [0., 1., 0.], 180.0)
        .unwrap();
    m.assert_has_solid("rev").unwrap();

    // A 180° revolve should assign start/end cap roles
    let op = m.op_result("rev").unwrap();
    test_harness::assertions::assert_role_assigned(
        op,
        &waffle_types::Role::RevStartFace,
        "180° revolve start face",
    )
    .unwrap();
    test_harness::assertions::assert_role_assigned(
        op,
        &waffle_types::Role::RevEndFace,
        "180° revolve end face",
    )
    .unwrap();
}

// ── Revolve + Boolean Tests ──────────────────────────────────────────────────

#[test]
#[ignore] // Known truck limitation: boolean union of curved (revolve) + planar (extrude) solids
fn test_truck_revolve_boolean_union() {
    let mut m = ModelBuilder::kernel();

    // Revolve a ring: rect at x=5..7, y=0..5 → thin torus
    m.rect_sketch("sk_ring", [5., 0., 0.], [0., 0., 1.], 5., 0., 2., 5.)
        .unwrap();
    m.revolve("ring", "sk_ring", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("ring").unwrap();

    // Extrude a box
    m.rect_sketch("sk_box", [0., 0., 0.], [0., 0., 1.], -5., -5., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box", "sk_box", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // Boolean union — likely fails due to curved+planar intersection
    m.boolean_union("merged", "ring", "box").unwrap();
    m.assert_has_solid("merged").unwrap();
}

// ── Cylinder Topology & Volume ───────────────────────────────────────────────

#[test]
fn test_truck_cylinder_topology() {
    let mut m = ModelBuilder::kernel();
    m.circle_sketch("sk_cyl", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.0)
        .unwrap();
    m.extrude("cyl", "sk_cyl", 10.0).unwrap();
    m.assert_has_solid("cyl").unwrap();

    // Topology: Euler's formula V - E + F = 2
    let (v, e, f) = m.topology_counts("cyl").unwrap();
    let euler = v as i64 - e as i64 + f as i64;
    assert_eq!(
        euler, 2,
        "Cylinder should satisfy V-E+F=2 (got V={} E={} F={}, χ={})",
        v, e, f, euler
    );

    // Cylinder has at minimum: top cap, bottom cap, and lateral surface
    assert!(
        f > 2,
        "Cylinder should have more than 2 faces (got F={})",
        f
    );

    // Volume check: pi * r^2 * h = pi * 25 * 10 ≈ 785.4
    let mesh = m.tessellate("cyl").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "Cylinder mesh should have triangles"
    );

    let volume = mesh_volume(&mesh);
    let expected = std::f64::consts::PI * 25.0 * 10.0;
    let rel_err = (volume - expected).abs() / expected;
    assert!(
        rel_err < 0.10,
        "Cylinder volume should be ~{:.1} (got {:.1}, err={:.1}%)",
        expected,
        volume,
        rel_err * 100.0
    );
}

// ── Save/Load Complex Feature Tree ───────────────────────────────────────────

#[test]
fn test_truck_save_load_complex_tree() {
    let mut m = ModelBuilder::kernel();

    // 1. Base plate 20x20x10
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 20., 20.)
        .unwrap();
    m.extrude("base", "base_sk", 10.0).unwrap();

    // 2. Pocket cut on top face
    m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 10., 10.)
        .unwrap();
    m.extrude_cut("pocket", "cut_sk", 5.0).unwrap();

    // 3. Side boss
    m.rect_sketch("side_sk", [0., 0., 0.], [0., 0., 1.], -5., 5., 5., 10.)
        .unwrap();
    m.extrude("boss", "side_sk", 15.0).unwrap();

    m.assert_feature_count(6).unwrap(); // 3 sketches + 3 operations

    // Save
    let json = m.save().unwrap();
    assert!(!json.is_empty(), "Saved JSON should not be empty");

    // Load into fresh builder
    let mut m2 = ModelBuilder::kernel();
    m2.load(&json).unwrap();
    m2.assert_feature_count(6).unwrap();
    m2.assert_no_errors().unwrap();
}

// ── Revolve + Cut ────────────────────────────────────────────────────────────

#[test]
#[ignore] // Known truck limitation: boolean cut of cylindrical hole through curved (revolve) solid
fn test_truck_revolve_with_cut() {
    let mut m = ModelBuilder::kernel();

    // Revolve a ring: rect at x=5..7, y=0..5 → thin torus
    m.rect_sketch("sk_ring", [5., 0., 0.], [0., 0., 1.], 5., 0., 2., 5.)
        .unwrap();
    m.revolve("ring", "sk_ring", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("ring").unwrap();

    // Cut through ring with a cylindrical hole
    m.circle_sketch("sk_hole", [0., 5., 0.], [0., 1., 0.], 0., 0., 3.0)
        .unwrap();
    m.extrude_cut("hole", "sk_hole", 20.0).unwrap();
    m.assert_has_solid("hole").unwrap();
}
