//! TruckKernel scenario tests.
//!
//! These test against real truck geometry. Some operations are known
//! to fail or be unsupported — those tests are #[ignore]d.

use test_harness::helpers::mesh_bounding_box;
use test_harness::oracle;
use test_harness::ModelBuilder;

#[test]
fn test_truck_box_extrude() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    let (v, e, f) = m.topology_counts("box").unwrap();
    assert!(v > 0, "Truck box should have vertices");
    assert!(e > 0, "Truck box should have edges");
    assert!(f > 0, "Truck box should have faces");
}

#[test]
fn test_truck_revolve() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("rev", "sk", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("rev").unwrap();
}

#[test]
fn test_truck_tessellate_stl() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let mesh = m.tessellate("box").unwrap();
    assert!(!mesh.indices.is_empty(), "Truck mesh should have triangles");

    let stl = m.export_stl("box").unwrap();
    assert!(stl.len() > 84, "STL should have content");
}

#[test]
fn test_truck_boolean_offset() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [5., 5., 5.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    // Offset boxes (not coplanar) should work in truck
    m.boolean_union("merged", "box1", "box2").unwrap();
    m.assert_has_solid("merged").unwrap();
}

#[test]
#[ignore = "truck 0.4: coplanar boolean faces fail"]
fn test_truck_boolean_coplanar() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    // Same Z plane = coplanar faces
    m.rect_sketch("sk2", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.boolean_union("merged", "box1", "box2").unwrap();
    m.assert_has_solid("merged").unwrap();
}

#[test]
#[ignore = "TruckKernel fillet returns NotSupported"]
fn test_truck_fillet() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.fillet("fillet", "box", 1.0).unwrap();
    m.assert_has_solid("fillet").unwrap();
}

#[test]
#[ignore = "TruckKernel chamfer returns NotSupported"]
fn test_truck_chamfer() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.chamfer("cham", "box", 1.0).unwrap();
    m.assert_has_solid("cham").unwrap();
}

// ── Additional TruckKernel tests ────────────────────────────────────────────

#[test]
#[ignore = "TruckKernel shell returns NotSupported"]
fn test_truck_shell() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.shell("shell", "box", 1.0).unwrap();
    m.assert_has_solid("shell").unwrap();
}

#[test]
fn test_truck_boolean_subtract_offset() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    // Offset box (not coplanar) for subtraction
    m.rect_sketch("sk2", [2., 2., 5.], [0., 0., 1.], 0., 0., 6., 6.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.boolean_subtract("result", "box1", "box2").unwrap();
    m.assert_has_solid("result").unwrap();
}

#[test]
fn test_truck_revolve_oracle() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("rev", "sk", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("rev").unwrap();

    // Tessellate and run mesh oracle suite
    let mesh = m.tessellate("rev").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "Revolve mesh should have triangles"
    );

    let verdicts = oracle::run_all_mesh_checks(&mesh);
    // Truck revolve tessellation has known quality issues (unpaired edges,
    // degenerate triangles at poles). Verify structural checks pass.
    let known_truck_issues = ["watertight_mesh", "no_degenerate_triangles"];
    for v in &verdicts {
        if known_truck_issues.contains(&v.oracle_name.as_str()) {
            continue;
        }
        assert!(
            v.passed,
            "Mesh oracle '{}' failed: {}",
            v.oracle_name, v.detail
        );
    }
}

#[test]
fn test_truck_save_load_roundtrip() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    let json = m.save().unwrap();

    let mut m2 = ModelBuilder::truck();
    m2.load(&json).unwrap();
    assert_eq!(m2.feature_count(), 2, "Loaded model should have 2 features");
    m2.assert_no_errors().unwrap();
}

#[test]
fn test_truck_circle_extrude() {
    let mut m = ModelBuilder::truck();
    m.circle_sketch("circle", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("cyl", "circle", 10.0).unwrap();

    m.assert_has_solid("cyl").unwrap();

    let mesh = m.tessellate("cyl").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "Circle extrude should produce mesh"
    );
}

#[test]
fn test_truck_circular_cut_through_cube() {
    let mut m = ModelBuilder::truck();

    // Step 1: Create 10x10x10 base cube
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();

    let (v, e, f) = m.topology_counts("cube").unwrap();
    assert_eq!((v, e, f), (8, 12, 6), "Cube should have box topology");

    // Step 2: Circle sketch on top face (z=10 plane), centered at (5,5), radius 2.5
    // Circle is well inside the 10x10 face to avoid edge intersection issues
    m.circle_sketch("hole_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.5)
        .unwrap();

    // Step 3: Cut extrude through the cube (depth 15 > cube height 10 ensures full penetration)
    m.extrude_cut("hole", "hole_sk", 15.0).unwrap();
    m.assert_has_solid("hole").unwrap();
    m.assert_no_errors().unwrap();

    // Step 4: Topology checks — hole adds cylindrical wall + modifies top/bottom
    let (_, _, f_cut) = m.topology_counts("hole").unwrap();
    assert!(
        f_cut > 6,
        "Cut should add faces beyond the original 6 (got {})",
        f_cut
    );

    // Euler formula and manifold checks
    let topo_verdicts = m.check_topology("hole").unwrap();
    for v in &topo_verdicts {
        assert!(
            v.passed,
            "Topology oracle '{}' failed: {}",
            v.oracle_name, v.detail
        );
    }

    // Tessellation checks
    let mesh = m.tessellate("hole").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "Cut result should have mesh triangles"
    );

    let mesh_verdicts = m.check_mesh("hole").unwrap();
    let known_truck_issues = ["watertight_mesh", "no_degenerate_triangles"];
    for v in &mesh_verdicts {
        if known_truck_issues.contains(&v.oracle_name.as_str()) {
            continue;
        }
        assert!(
            v.passed,
            "Mesh oracle '{}' failed: {}",
            v.oracle_name, v.detail
        );
    }

    // Verify bounding box is finite and reasonable (cut only removes material)
    let (bb_min, bb_max) = test_harness::helpers::mesh_bounding_box(&mesh);
    for i in 0..3 {
        assert!(
            (bb_max[i] - bb_min[i]) > 0.0 && (bb_max[i] - bb_min[i]) < 100.0,
            "Bounding box axis {} should be reasonable: min={}, max={}",
            i,
            bb_min[i],
            bb_max[i]
        );
    }
}

// ── Advanced Extrude: Depth Modes & Bidirectional ─────────────────────────────

#[test]
fn test_truck_extrude_through_all_cut() {
    // ThroughAll cut should punch a hole through the entire cube
    let mut m = ModelBuilder::truck();

    // Base 10x10x10 cube
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();

    // Circle on top face, ThroughAll cut
    m.circle_sketch("hole_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.5)
        .unwrap();
    m.extrude_through_all("hole", "hole_sk", true).unwrap();
    m.assert_has_solid("hole").unwrap();
    m.assert_no_errors().unwrap();

    // Should have more faces than the original 6
    let (_, _, f_cut) = m.topology_counts("hole").unwrap();
    assert!(
        f_cut > 6,
        "ThroughAll cut should add faces beyond 6 (got {})",
        f_cut
    );

    // Topology checks
    let topo_verdicts = m.check_topology("hole").unwrap();
    for v in &topo_verdicts {
        assert!(
            v.passed,
            "Topology oracle '{}' failed: {}",
            v.oracle_name, v.detail
        );
    }
}

#[test]
fn test_truck_extrude_explicit_direction() {
    // Extrude in a non-normal direction
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_directed("angled", "sk", 10.0, [1., 0., 1.], false)
        .unwrap();
    m.assert_has_solid("angled").unwrap();
    m.assert_no_errors().unwrap();

    let mesh = m.tessellate("angled").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "Angled extrude should produce mesh"
    );

    // Bounding box should reflect the angled direction
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    // X extent should be > 0 since we extruded in [1,0,1]
    assert!(
        bb_max[0] - bb_min[0] > 5.0,
        "Angled extrude should have X extent > 5 (got {})",
        bb_max[0] - bb_min[0]
    );
}

#[test]
fn test_truck_extrude_symmetric() {
    // Symmetric extrude: equal depth in both directions from XY plane
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_symmetric("sym", "sk", 20.0).unwrap();
    m.assert_has_solid("sym").unwrap();
    m.assert_no_errors().unwrap();

    let mesh = m.tessellate("sym").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "Symmetric extrude should produce mesh"
    );

    // Bounding box should extend in both Z directions from the sketch plane (z=0)
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    // Should extend roughly -10 to +10 in Z (symmetric 20 total = 10 each way)
    assert!(
        bb_min[2] < -5.0,
        "Symmetric extrude should extend below sketch plane (min_z={:.1})",
        bb_min[2]
    );
    assert!(
        bb_max[2] > 5.0,
        "Symmetric extrude should extend above sketch plane (max_z={:.1})",
        bb_max[2]
    );

    // Topology checks
    let topo_verdicts = m.check_topology("sym").unwrap();
    for v in &topo_verdicts {
        assert!(
            v.passed,
            "Topology oracle '{}' failed: {}",
            v.oracle_name, v.detail
        );
    }
}

#[test]
fn test_truck_extrude_bidirectional() {
    // Asymmetric bidirectional: 15 up, 5 down
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_bidirectional("bidir", "sk", 15.0, 5.0).unwrap();
    m.assert_has_solid("bidir").unwrap();
    m.assert_no_errors().unwrap();

    let mesh = m.tessellate("bidir").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "Bidirectional extrude should produce mesh"
    );

    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    // Primary: +15 in Z, secondary: -5 in Z
    assert!(
        bb_max[2] > 10.0,
        "Bidirectional should extend significantly in +Z (max_z={:.1})",
        bb_max[2]
    );
    assert!(
        bb_min[2] < -2.0,
        "Bidirectional should extend in -Z (min_z={:.1})",
        bb_min[2]
    );
}

#[test]
fn test_truck_cut_direction_selection() {
    // Cut with explicit direction vector pointing downward
    let mut m = ModelBuilder::truck();

    // Base cube
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();

    // Circle on top face, cut with direction matching sketch normal (code reverses for cut)
    m.circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.5)
        .unwrap();
    m.extrude_directed("cut", "cut_sk", 15.0, [0., 0., 1.], true)
        .unwrap();
    m.assert_has_solid("cut").unwrap();
    m.assert_no_errors().unwrap();

    let (_, _, f) = m.topology_counts("cut").unwrap();
    assert!(f > 6, "Directed cut should add faces beyond 6 (got {})", f);
}

#[test]
fn test_truck_extrude_through_all_no_target() {
    // ThroughAll without existing body: uses generous fallback depth
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_through_all("ext", "sk", false).unwrap();
    m.assert_has_solid("ext").unwrap();
    m.assert_no_errors().unwrap();

    let mesh = m.tessellate("ext").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "ThroughAll fallback should produce mesh"
    );

    // Should have a large Z extent (fallback = 100)
    let (_, bb_max) = mesh_bounding_box(&mesh);
    assert!(
        bb_max[2] > 50.0,
        "ThroughAll fallback should produce tall extrude (max_z={:.1})",
        bb_max[2]
    );
}

#[test]
fn test_truck_extrude_symmetric_topology() {
    // Verify symmetric extrude produces valid manifold topology
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 5.], [0., 0., 1.], 0., 0., 8., 8.)
        .unwrap();
    m.extrude_symmetric("sym", "sk", 10.0).unwrap();
    m.assert_has_solid("sym").unwrap();

    // Topology checks (Euler formula, manifold edges)
    let topo_verdicts = m.check_topology("sym").unwrap();
    for v in &topo_verdicts {
        assert!(
            v.passed,
            "Topology oracle '{}' failed: {}",
            v.oracle_name, v.detail
        );
    }

    // Mesh quality checks
    let mesh_verdicts = m.check_mesh("sym").unwrap();
    let known_truck_issues = ["watertight_mesh", "no_degenerate_triangles"];
    for v in &mesh_verdicts {
        if known_truck_issues.contains(&v.oracle_name.as_str()) {
            continue;
        }
        assert!(
            v.passed,
            "Mesh oracle '{}' failed: {}",
            v.oracle_name, v.detail
        );
    }
}
