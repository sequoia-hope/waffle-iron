//! Advanced test scenarios for MockKernel.
//!
//! Categories:
//! 1. Error/Edge Cases — undo/redo boundaries, cascade errors, suppress/delete
//! 2. Feature Combinations — chained ops, stacked modifications
//! 3. Sketch Varieties — circle, L-shape, non-XY planes
//! 4. Advanced Workflows — deep trees, save/load complex, explicit direction, partial revolve
//! 5. Role/Provenance Verification — semantic role checks via oracle
//! 6. Mesh Quality Verification — volume, surface area, bounding box

use std::collections::HashMap;
use test_harness::helpers::{mesh_bounding_box, mesh_surface_area, mesh_volume};
use test_harness::oracle;
use test_harness::ModelBuilder;
use waffle_types::{ClosedProfile, Role};

// ══════════════════════════════════════════════════════════════════════════════
// Category 1: Error/Edge Cases
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_undo_past_beginning() {
    let mut m = ModelBuilder::mock();
    // Nothing to undo on a fresh engine
    assert!(m.undo().is_err(), "undo on empty engine should return Err");
}

#[test]
fn test_redo_past_end() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    // No undone ops to redo
    assert!(
        m.redo().is_err(),
        "redo with no undone ops should return Err"
    );
}

#[test]
fn test_delete_base_sketch_cascade_error() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_no_errors().unwrap();

    // Delete the sketch — extrude loses its sketch_id reference
    m.delete_feature("sk").unwrap();

    // add_feature never returns Err — errors accumulate in engine.errors
    m.assert_has_errors().unwrap();
}

#[test]
fn test_suppress_base_sketch_cascade() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_no_errors().unwrap();

    // Suppress sketch → extrude should accumulate errors
    m.suppress("sk").unwrap();
    m.assert_has_errors().unwrap();

    // Unsuppress → should recover
    m.unsuppress("sk").unwrap();
    m.assert_no_errors().unwrap();
}

#[test]
fn test_reorder_extrude_before_sketch() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_no_errors().unwrap();

    // Reorder extrude to position 0 (before its sketch) → forward reference
    m.reorder("box", 0).unwrap();
    m.assert_has_errors().unwrap();
}

#[test]
fn test_delete_middle_feature_cascade() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.fillet("fillet", "box", 0.5).unwrap();
    m.assert_no_errors().unwrap();

    // Delete the extrude (middle of sketch→extrude→fillet) → fillet should error
    m.delete_feature("box").unwrap();
    m.assert_has_errors().unwrap();
}

#[test]
fn test_multiple_undo_redo_cycles() {
    let mut m = ModelBuilder::mock();
    // Build 3 features: sketch + extrude + fillet
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.fillet("fillet", "box", 0.5).unwrap();
    assert_eq!(m.feature_count(), 3);

    // Undo all 3
    m.undo().unwrap(); // undo fillet
    assert_eq!(m.feature_count(), 2);
    m.undo().unwrap(); // undo extrude
    assert_eq!(m.feature_count(), 1);
    m.undo().unwrap(); // undo sketch
    assert_eq!(m.feature_count(), 0);

    // Redo 2
    m.redo().unwrap(); // redo sketch
    assert_eq!(m.feature_count(), 1);
    m.redo().unwrap(); // redo extrude
    assert_eq!(m.feature_count(), 2);

    // Undo 1
    m.undo().unwrap(); // undo extrude
    assert_eq!(m.feature_count(), 1);

    // Verify final state: only the sketch remains
    assert_eq!(m.feature_count(), 1);
}

// ══════════════════════════════════════════════════════════════════════════════
// Category 2: Feature Combinations
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_extrude_fillet_chamfer_chain() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    let (_, _, f_box) = m.topology_counts("box").unwrap();

    m.fillet("fillet", "box", 0.5).unwrap();
    let (_, _, f_fillet) = m.topology_counts("fillet").unwrap();
    assert!(
        f_fillet > f_box,
        "Fillet should increase face count: {} > {}",
        f_fillet,
        f_box
    );

    m.chamfer("chamfer", "fillet", 0.3).unwrap();
    let (_, _, f_chamfer) = m.topology_counts("chamfer").unwrap();
    assert!(
        f_chamfer > f_fillet,
        "Chamfer should further increase face count: {} > {}",
        f_chamfer,
        f_fillet
    );

    m.assert_no_errors().unwrap();
}

#[test]
fn test_extrude_fillet_boolean_chain() {
    let mut m = ModelBuilder::mock();
    // Body A: box + fillet
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();
    m.fillet("fillet1", "box1", 0.5).unwrap();
    m.assert_has_solid("fillet1").unwrap();

    let (_, _, f_fillet) = m.topology_counts("fillet1").unwrap();
    assert!(f_fillet > 6, "Fillet should increase face count beyond 6");

    // Body B: box + shell (independent)
    m.rect_sketch("sk2", [20., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();
    m.shell("shell2", "box2", 1.0).unwrap();
    m.assert_has_solid("shell2").unwrap();

    // Verify both bodies produced valid topology
    m.assert_no_errors().unwrap();
    assert_eq!(m.feature_count(), 6); // 2 sketches + 2 extrudes + fillet + shell
}

#[test]
fn test_extrude_fillet_boolean_union() {
    let mut m = ModelBuilder::mock();
    // Body A: filleted box
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();
    m.fillet("fillet1", "box1", 0.5).unwrap();

    // Body B: plain box
    m.rect_sketch("sk2", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    // Union of filleted body with plain body
    m.boolean_union("merged", "fillet1", "box2").unwrap();
    m.assert_has_solid("merged").unwrap();
    m.assert_no_errors().unwrap();
}

#[test]
fn test_chained_booleans() {
    let mut m = ModelBuilder::mock();
    // 3 boxes
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box_a", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box_b", "sk2", 10.0).unwrap();

    m.rect_sketch("sk3", [3., 3., 0.], [0., 0., 1.], 0., 0., 4., 4.)
        .unwrap();
    m.extrude("box_c", "sk3", 10.0).unwrap();

    // union(A, B) → subtract(AB, C)
    m.boolean_union("ab", "box_a", "box_b").unwrap();
    m.boolean_subtract("result", "ab", "box_c").unwrap();
    m.assert_has_solid("result").unwrap();
    m.assert_no_errors().unwrap();

    // 3 sketches + 3 extrudes + 2 booleans = 8 features
    m.assert_feature_count(8).unwrap();
}

#[test]
fn test_boolean_intersect_overlap() {
    let mut m = ModelBuilder::mock();
    // Two 10x10x10 boxes overlapping by 5 units in X and Y
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.boolean_intersect("inter", "box1", "box2").unwrap();
    m.assert_has_solid("inter").unwrap();

    // MockKernel intersect still produces a box-like topology
    let (v, e, f) = m.topology_counts("inter").unwrap();
    assert_eq!((v, e, f), (8, 12, 6), "Intersect result: V=8 E=12 F=6");
    m.assert_no_errors().unwrap();
}

#[test]
fn test_revolve_boolean_subtract() {
    let mut m = ModelBuilder::mock();
    // Body A: revolve (cylinder-like)
    m.rect_sketch("sk_rev", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("cyl", "sk_rev", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();

    // Body B: box
    m.rect_sketch("sk_box", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk_box", 10.0).unwrap();

    // Cross-type boolean: subtract box from cylinder
    m.boolean_subtract("result", "cyl", "box").unwrap();
    m.assert_has_solid("result").unwrap();
    m.assert_no_errors().unwrap();
}

// ══════════════════════════════════════════════════════════════════════════════
// Category 3: Sketch Varieties
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_circle_sketch_extrude() {
    let mut m = ModelBuilder::mock();
    m.circle_sketch("circle", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("cyl", "circle", 10.0).unwrap();

    m.assert_has_solid("cyl").unwrap();
    m.assert_no_errors().unwrap();

    let mesh = m.tessellate("cyl").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "Cylinder should produce triangles"
    );
    assert!(
        !mesh.face_ranges.is_empty(),
        "Cylinder should have face ranges"
    );
}

#[test]
fn test_l_shaped_manual_sketch() {
    let mut m = ModelBuilder::mock();

    // L-shaped profile: 6 points forming an L
    //  4---3
    //  |   |
    //  5-6 |
    //    | |
    //    1-2
    m.begin_sketch([0., 0., 0.], [0., 0., 1.]);
    m.add_point(1, 0., 0.)
        .add_point(2, 10., 0.)
        .add_point(3, 10., 20.)
        .add_point(4, 0., 20.)
        .add_point(5, 0., 10.)
        .add_point(6, 5., 10.);
    m.add_line(10, 1, 2)
        .add_line(11, 2, 3)
        .add_line(12, 3, 4)
        .add_line(13, 4, 5)
        .add_line(14, 5, 6)
        .add_line(15, 6, 1);

    let mut positions = HashMap::new();
    positions.insert(1, (0.0, 0.0));
    positions.insert(2, (10.0, 0.0));
    positions.insert(3, (10.0, 20.0));
    positions.insert(4, (0.0, 20.0));
    positions.insert(5, (0.0, 10.0));
    positions.insert(6, (5.0, 10.0));

    let profiles = vec![ClosedProfile {
        entity_ids: vec![1, 2, 3, 4, 5, 6],
        is_outer: true,
    }];

    m.finish_sketch_manual("l_sketch", positions, profiles, [0., 0., 0.], [0., 0., 1.])
        .unwrap();

    m.extrude("l_box", "l_sketch", 5.0).unwrap();
    m.assert_has_solid("l_box").unwrap();
    m.assert_no_errors().unwrap();

    let mesh = m.tessellate("l_box").unwrap();
    assert!(!mesh.indices.is_empty(), "L-shape should produce mesh");
}

#[test]
fn test_sketch_on_xz_and_yz_planes() {
    let mut m = ModelBuilder::mock();

    // Sketch on XZ plane (normal = [0, 1, 0])
    m.rect_sketch("sk_xz", [0., 0., 0.], [0., 1., 0.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box_xz", "sk_xz", 10.0).unwrap();

    let (v1, e1, f1) = m.topology_counts("box_xz").unwrap();
    assert_eq!((v1, e1, f1), (8, 12, 6), "XZ plane box: V=8 E=12 F=6");

    // Sketch on YZ plane (normal = [1, 0, 0])
    m.rect_sketch("sk_yz", [0., 0., 0.], [1., 0., 0.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box_yz", "sk_yz", 10.0).unwrap();

    let (v2, e2, f2) = m.topology_counts("box_yz").unwrap();
    assert_eq!((v2, e2, f2), (8, 12, 6), "YZ plane box: V=8 E=12 F=6");

    m.assert_no_errors().unwrap();
}

// ══════════════════════════════════════════════════════════════════════════════
// Category 4: Advanced Workflows
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_deep_feature_tree() {
    let mut m = ModelBuilder::mock();

    // 4 sketch+extrude pairs
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [20., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.rect_sketch("sk3", [40., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box3", "sk3", 10.0).unwrap();

    m.rect_sketch("sk4", [60., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box4", "sk4", 10.0).unwrap();

    // Fillet + chamfer on box1
    m.fillet("fillet1", "box1", 0.5).unwrap();
    m.chamfer("chamfer1", "fillet1", 0.3).unwrap();

    // Boolean union of box2 and box3
    m.boolean_union("merged", "box2", "box3").unwrap();

    // 4 sketches + 4 extrudes + fillet + chamfer + boolean = 11
    assert!(
        m.feature_count() >= 11,
        "Should have at least 11 features, got {}",
        m.feature_count()
    );
    m.assert_no_errors().unwrap();
}

#[test]
fn test_save_load_complex_model() {
    let mut m = ModelBuilder::mock();

    // Build a multi-feature model
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();
    m.fillet("fillet", "box1", 0.5).unwrap();

    m.rect_sketch("sk2", [20., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.boolean_union("merged", "fillet", "box2").unwrap();
    let original_count = m.feature_count();
    m.assert_no_errors().unwrap();

    // Save
    let json = m.save().unwrap();

    // Load into fresh builder
    let mut m2 = ModelBuilder::mock();
    m2.load(&json).unwrap();

    assert_eq!(
        m2.feature_count(),
        original_count,
        "Loaded model should have same feature count"
    );
    m2.assert_no_errors().unwrap();
}

#[test]
fn test_extrude_explicit_direction() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();

    // Extrude in a non-standard direction [1, 0, 1]
    m.extrude_on_face("angled", "sk", 10.0, [1., 0., 1.])
        .unwrap();
    m.assert_has_solid("angled").unwrap();
    m.assert_no_errors().unwrap();

    let mesh = m.tessellate("angled").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "Angled extrude should produce mesh"
    );
}

#[test]
fn test_revolve_partial_90() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("quarter", "sk", [0., 0., 0.], [0., 1., 0.], 90.0)
        .unwrap();

    m.assert_has_solid("quarter").unwrap();
    m.assert_no_errors().unwrap();

    let mesh = m.tessellate("quarter").unwrap();
    assert!(!mesh.indices.is_empty(), "90° revolve should produce mesh");
}

#[test]
fn test_revolve_partial_180() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("half", "sk", [0., 0., 0.], [0., 1., 0.], 180.0)
        .unwrap();

    m.assert_has_solid("half").unwrap();
    m.assert_no_errors().unwrap();

    let mesh = m.tessellate("half").unwrap();
    assert!(!mesh.indices.is_empty(), "180° revolve should produce mesh");
}

// ══════════════════════════════════════════════════════════════════════════════
// Category 5: Role/Provenance Verification
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_extrude_roles_assigned() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let op = m.op_result("box").unwrap();
    let v_pos = oracle::check_role_exists(op, &Role::EndCapPositive, 1);
    assert!(v_pos.passed, "EndCapPositive: {}", v_pos.detail);

    let v_neg = oracle::check_role_exists(op, &Role::EndCapNegative, 1);
    assert!(v_neg.passed, "EndCapNegative: {}", v_neg.detail);

    let v_side = oracle::check_role_exists(op, &Role::SideFace { index: 0 }, 1);
    assert!(v_side.passed, "SideFace{{0}}: {}", v_side.detail);
}

#[test]
fn test_fillet_roles_assigned() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.fillet("fillet", "box", 0.5).unwrap();

    let op = m.op_result("fillet").unwrap();
    let v = oracle::check_role_exists(op, &Role::FilletFace { index: 0 }, 1);
    assert!(v.passed, "FilletFace{{0}}: {}", v.detail);
}

#[test]
fn test_chamfer_roles_assigned() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.chamfer("chamfer", "box", 1.0).unwrap();

    let op = m.op_result("chamfer").unwrap();
    let v = oracle::check_role_exists(op, &Role::ChamferFace { index: 0 }, 1);
    assert!(v.passed, "ChamferFace{{0}}: {}", v.detail);
}

#[test]
fn test_shell_roles_assigned() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.shell("shell", "box", 1.0).unwrap();

    let op = m.op_result("shell").unwrap();
    let v = oracle::check_role_exists(op, &Role::ShellInnerFace { index: 0 }, 1);
    assert!(v.passed, "ShellInnerFace{{0}}: {}", v.detail);
}

#[test]
fn test_boolean_roles_assigned() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.boolean_union("merged", "box1", "box2").unwrap();

    let op = m.op_result("merged").unwrap();
    // MockKernel assigns all merged faces as BooleanBodyAFace (body-A-centric)
    let va = oracle::check_role_exists(op, &Role::BooleanBodyAFace { index: 0 }, 1);
    assert!(va.passed, "BooleanBodyAFace{{0}}: {}", va.detail);

    // Verify provenance has role assignments at all (at least 6 faces from a box)
    let total_roles = op.provenance.role_assignments.len();
    assert!(
        total_roles >= 6,
        "Boolean should assign roles to at least 6 faces, got {}",
        total_roles
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Category 6: Mesh Quality Verification
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_mesh_volume_box_sanity() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let mesh = m.tessellate("box").unwrap();
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 1000.0).abs() < 50.0,
        "10x10x10 box volume should be ~1000, got {}",
        vol
    );
}

#[test]
fn test_mesh_surface_area_box_sanity() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let mesh = m.tessellate("box").unwrap();
    let area = mesh_surface_area(&mesh);
    assert!(
        (area - 600.0).abs() < 50.0,
        "10x10x10 box surface area should be ~600, got {}",
        area
    );
}

#[test]
fn test_mesh_bounding_box_matches() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let mesh = m.tessellate("box").unwrap();
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);

    let tol = 0.5;
    for i in 0..3 {
        assert!(
            bb_min[i].abs() < tol,
            "min[{}] should be ~0, got {}",
            i,
            bb_min[i]
        );
        assert!(
            (bb_max[i] - 10.0).abs() < tol,
            "max[{}] should be ~10, got {}",
            i,
            bb_max[i]
        );
    }
}
