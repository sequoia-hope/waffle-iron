//! Complex workflow regression tests against MockKernel.
//!
//! These scenarios test the full dispatch pipeline through ModelBuilder,
//! validating topology, mesh quality, and oracle results at each step.

use std::collections::HashMap;
use test_harness::ModelBuilder;
use waffle_types::{ClosedProfile, Role};

// ── Scenario 1: Basic box extrude ───────────────────────────────────────

#[test]
fn test_box_extrude_basic() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    m.assert_has_solid("box").unwrap();
    m.assert_no_errors().unwrap();

    let (v, e, f) = m.topology_counts("box").unwrap();
    assert_eq!((v, e, f), (8, 12, 6), "Box: V=8 E=12 F=6");

    let mesh = m.tessellate("box").unwrap();
    assert_eq!(mesh.indices.len() / 3, 12, "Box: 12 triangles");
    assert_eq!(mesh.face_ranges.len(), 6, "Box: 6 face ranges");
}

// ── Scenario 2: Box with hole (multi-profile) ──────────────────────────

#[test]
fn test_box_with_hole() {
    let mut m = ModelBuilder::mock();

    // Sketch with outer rect + inner circle
    m.begin_sketch([0., 0., 0.], [0., 0., 1.]);
    // Outer rect
    m.add_point(1, 0., 0.)
        .add_point(2, 100., 0.)
        .add_point(3, 100., 50.)
        .add_point(4, 0., 50.);
    m.add_line(10, 1, 2)
        .add_line(11, 2, 3)
        .add_line(12, 3, 4)
        .add_line(13, 4, 1);
    // Inner circle approximated as point (just for testing multi-entity sketches)
    m.add_point(5, 50., 25.);
    m.add_circle_entity(20, 5, 10.0);

    let mut positions = HashMap::new();
    positions.insert(1, (0.0, 0.0));
    positions.insert(2, (100.0, 0.0));
    positions.insert(3, (100.0, 50.0));
    positions.insert(4, (0.0, 50.0));
    positions.insert(5, (50.0, 25.0));

    let profiles = vec![
        ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        },
        ClosedProfile {
            entity_ids: vec![5],
            is_outer: false,
        },
    ];

    m.finish_sketch_manual("sk", positions, profiles, [0., 0., 0.], [0., 0., 1.])
        .unwrap();

    m.extrude("box", "sk", 20.0).unwrap();
    m.assert_has_solid("box").unwrap();
    // Feature tree should have Sketch + Extrude
    m.assert_feature_count(2).unwrap();
}

// ── Scenario 3: Sketch on face ──────────────────────────────────────────

#[test]
fn test_sketch_on_face() {
    let mut m = ModelBuilder::mock();

    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("base_box", "base_sk", 10.0).unwrap();
    m.assert_has_solid("base_box").unwrap();

    // Create a sketch on the "top face" (z=10, normal up)
    m.rect_sketch("face_sk", [0., 0., 10.], [0., 0., 1.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss", "face_sk", 5.0).unwrap();

    m.assert_has_solid("boss").unwrap();
    m.assert_feature_count(4).unwrap(); // 2 sketches + 2 extrudes
}

// ── Scenario 4: Revolve 360 ────────────────────────────────────────────

#[test]
fn test_revolve_360() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("cyl", "sk", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();

    m.assert_has_solid("cyl").unwrap();
    let mesh = m.tessellate("cyl").unwrap();
    assert!(!mesh.indices.is_empty(), "Revolve should produce mesh");
}

// ── Scenario 5: Fillet box ─────────────────────────────────────────────

#[test]
fn test_fillet_box() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let (_, _, f_before) = m.topology_counts("box").unwrap();

    m.fillet("fillet", "box", 0.5).unwrap();
    m.assert_has_solid("fillet").unwrap();

    let (_, _, f_after) = m.topology_counts("fillet").unwrap();
    assert!(
        f_after > f_before,
        "Fillet should increase face count: {} > {}",
        f_after,
        f_before
    );
}

// ── Scenario 6: Chamfer box ────────────────────────────────────────────

#[test]
fn test_chamfer_box() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    m.chamfer("cham", "box", 1.0).unwrap();
    m.assert_has_solid("cham").unwrap();

    // Chamfer should change topology
    let (_, _, f) = m.topology_counts("cham").unwrap();
    assert!(f > 6, "Chamfer should add faces beyond original 6");
}

// ── Scenario 7: Shell box ──────────────────────────────────────────────

#[test]
fn test_shell_box() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    m.shell("shell", "box", 1.0).unwrap();
    m.assert_has_solid("shell").unwrap();

    let (_, _, f) = m.topology_counts("shell").unwrap();
    assert!(f > 6, "Shell should add inner faces");
}

// ── Scenario 8: Boolean union ──────────────────────────────────────────

#[test]
fn test_boolean_union() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.boolean_union("merged", "box1", "box2").unwrap();
    m.assert_has_solid("merged").unwrap();
}

// ── Scenario 9: Boolean subtract ───────────────────────────────────────

#[test]
fn test_boolean_subtract() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.boolean_subtract("result", "box1", "box2").unwrap();
    m.assert_has_solid("result").unwrap();
}

// ── Scenario 10: Multi-body without boolean ────────────────────────────

#[test]
fn test_multi_body_no_boolean() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [50., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    // Both should exist independently
    m.assert_has_solid("box1").unwrap();
    m.assert_has_solid("box2").unwrap();
    m.assert_feature_count(4).unwrap();
}

// ── Scenario 11: Undo/redo preserves topology ──────────────────────────

#[test]
fn test_undo_redo_preserves_topology() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let (v1, e1, f1) = m.topology_counts("box").unwrap();

    // Undo all
    m.undo().unwrap(); // undo extrude
    m.undo().unwrap(); // undo sketch
    assert_eq!(m.feature_count(), 0);

    // Redo all
    m.redo().unwrap(); // redo sketch
    m.redo().unwrap(); // redo extrude
    assert_eq!(m.feature_count(), 2);

    // Get topology of the re-done extrude (may have different UUID)
    let redo_extrude_id = m.state.engine.tree.features[1].id;
    let result = m.state.engine.get_result(redo_extrude_id);
    assert!(result.is_some(), "Redo'd extrude should have result");
    let handle = &result.unwrap().outputs[0].1.handle;
    let introspect = m.kernel().as_introspect();
    let v2 = introspect.list_vertices(handle).len();
    let e2 = introspect.list_edges(handle).len();
    let f2 = introspect.list_faces(handle).len();
    assert_eq!(
        (v1, e1, f1),
        (v2, e2, f2),
        "Topology should match after undo/redo"
    );
}

// ── Scenario 12: Save/load roundtrip ───────────────────────────────────

#[test]
fn test_save_load_roundtrip() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let json = m.save().unwrap();

    let mut m2 = ModelBuilder::mock();
    m2.load(&json).unwrap();
    assert_eq!(m2.feature_count(), 2);

    // Verify the loaded extrude has a solid
    let extrude_id = m2.state.engine.tree.features[1].id;
    let result = m2.state.engine.get_result(extrude_id);
    assert!(result.is_some(), "Loaded extrude should have result");
    assert!(
        !result.unwrap().outputs.is_empty(),
        "Loaded extrude should have outputs"
    );
}

// ── Scenario 13: Suppress/unsuppress ───────────────────────────────────

#[test]
fn test_suppress_unsuppress() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // Suppress
    m.suppress("box").unwrap();
    assert!(
        m.solid_handle("box").is_err(),
        "Suppressed feature should have no solid"
    );

    // Unsuppress
    m.unsuppress("box").unwrap();
    m.assert_has_solid("box").unwrap();
}

// ── Scenario 14: STL export ────────────────────────────────────────────

#[test]
fn test_stl_export_valid() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let stl = m.export_stl("box").unwrap();
    assert!(stl.len() > 84, "STL should be more than header");
    let tri_count = u32::from_le_bytes([stl[80], stl[81], stl[82], stl[83]]);
    assert_eq!(tri_count, 12, "Box should have 12 triangles in STL");
}

// ── Scenario 15: Full workflow (the big one) ───────────────────────────

#[test]
fn test_full_workflow() {
    let mut m = ModelBuilder::mock();

    // 1. Sketch on XY plane: box with offset hole
    m.begin_sketch([0., 0., 0.], [0., 0., 1.]);
    // Outer rectangle 100x50
    m.add_point(1, 0., 0.)
        .add_point(2, 100., 0.)
        .add_point(3, 100., 50.)
        .add_point(4, 0., 50.);
    m.add_line(10, 1, 2)
        .add_line(11, 2, 3)
        .add_line(12, 3, 4)
        .add_line(13, 4, 1);
    // Inner circle (hole) at offset position
    m.add_point(5, 70., 25.);
    m.add_circle_entity(20, 5, 10.0);

    let mut positions = HashMap::new();
    positions.insert(1, (0.0, 0.0));
    positions.insert(2, (100.0, 0.0));
    positions.insert(3, (100.0, 50.0));
    positions.insert(4, (0.0, 50.0));
    positions.insert(5, (70.0, 25.0));

    let profiles = vec![
        ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        },
        ClosedProfile {
            entity_ids: vec![5],
            is_outer: false,
        },
    ];

    m.finish_sketch_manual(
        "base_sketch",
        positions,
        profiles,
        [0., 0., 0.],
        [0., 0., 1.],
    )
    .unwrap();

    // 2. Extrude the base
    m.extrude("base_box", "base_sketch", 20.0).unwrap();
    m.assert_has_solid("base_box").unwrap();
    let (v, e, f) = m.topology_counts("base_box").unwrap();
    assert!(v > 0 && e > 0 && f > 0, "Base box should have topology");

    // 3. Select top face, sketch on it
    let _top_face = m
        .select_face_by_role("base_box", Role::EndCapPositive, 0)
        .unwrap();
    m.rect_sketch(
        "face_sketch",
        [0., 0., 20.],
        [0., 0., 1.],
        10.,
        10.,
        30.,
        30.,
    )
    .unwrap();

    // 4. Extrude from face
    m.extrude("boss", "face_sketch", 10.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    // 5. New part in same workspace
    m.rect_sketch(
        "part2_sketch",
        [200., 0., 0.],
        [0., 0., 1.],
        0.,
        0.,
        50.,
        50.,
    )
    .unwrap();
    m.extrude("part2", "part2_sketch", 30.0).unwrap();
    m.assert_has_solid("part2").unwrap();

    // 6. Boolean subtract
    m.boolean_subtract("final", "base_box", "part2").unwrap();
    m.assert_has_solid("final").unwrap();

    // 7. Export STL
    let stl = m.export_stl("final").unwrap();
    assert!(stl.len() > 84, "STL should have content beyond header");

    // 8. Full report
    let report = m.report().unwrap();
    let text = report.to_text();

    // Verify report content
    assert!(
        text.contains("Feature Tree"),
        "Report should have feature tree"
    );
    assert!(
        text.contains("Mesh Summary"),
        "Report should have mesh summary"
    );
    assert!(
        text.contains("Oracle Results"),
        "Report should have oracle results"
    );
    assert!(
        text.contains("base_box") || text.contains("Extrude"),
        "Report should mention features"
    );

    // Check that all topology-passed oracles pass
    let topo_oracles: Vec<_> = report
        .oracle_results
        .iter()
        .filter(|v| v.oracle_name.contains("euler") || v.oracle_name.contains("manifold"))
        .collect();
    for v in &topo_oracles {
        assert!(
            v.passed,
            "Topology oracle {} should pass: {}",
            v.oracle_name, v.detail
        );
    }

    // Verify we built the full feature tree
    assert!(
        m.feature_count() >= 7,
        "Should have at least 7 features (sketches + extrudes + boolean), got {}",
        m.feature_count()
    );

    println!("=== FULL WORKFLOW REPORT ===\n{}", text);
}
