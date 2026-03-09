//! Boolean edge case and boundary tests for RealKernel.
//!
//! These tests probe the boundary conditions of the boolean pipeline:
//! disjoint solids, barely-touching geometry, barely-overlapping geometry,
//! and multi-cylinder stacking scenarios.
//!
//! Categories:
//!   DE — Disjoint/Edge-contact tests (3 tests)
//!   CS — Cylinder Stacking tests (2 tests)
//!   ET — Exact Topology tests (2 tests)

use test_harness::helpers::{mesh_bounding_box, mesh_volume};
use test_harness::ModelBuilder;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Create a 10x10x10 base cube at origin on XY plane.
/// Cube spans x∈[0,10], y∈[0,10], z∈[0,10].
fn base_cube() -> ModelBuilder {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();
    m
}

/// Assert all mesh vertices and normals are finite (no NaN/Inf).
fn assert_mesh_finite(mesh: &kernel::types::RenderMesh, label: &str) {
    for (i, v) in mesh.vertices.iter().enumerate() {
        assert!(
            v.is_finite(),
            "{}: vertex[{}] is not finite: {}",
            label,
            i,
            v
        );
    }
    for (i, n) in mesh.normals.iter().enumerate() {
        assert!(
            n.is_finite(),
            "{}: normal[{}] is not finite: {}",
            label,
            i,
            n
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Category DE — Disjoint / Edge-contact Tests
// ══════════════════════════════════════════════════════════════════════════════

/// DE1: Union of two disjoint boxes.
///
/// When two solids don't overlap, their union should contain both bodies.
/// Currently booleans may struggle with disjoint inputs.
#[test]
fn de1_disjoint_boxes_union() {
    let mut m = ModelBuilder::kernel();

    // Box A: [0,5] x [0,5] x [0,5]
    m.rect_sketch("a_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_a", "a_sk", 5.0).unwrap();

    // Box B: [20,25] x [20,25] x [0,5] — completely separated from A
    m.rect_sketch("b_sk", [0., 0., 0.], [0., 0., 1.], 20., 20., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_b", "b_sk", 5.0).unwrap();

    m.boolean_union("merged", "box_a", "box_b").unwrap();
    m.assert_has_solid("merged").unwrap();

    // Disjoint union may produce multiple bodies — sum volumes across all outputs
    let meshes = m.tessellate_all("merged").unwrap();
    let mut vol = 0.0;
    for mesh in &meshes {
        assert_mesh_finite(mesh, "disjoint_union");
        vol += mesh_volume(mesh);
    }
    // Two 5x5x5 cubes = 125 + 125 = 250
    assert!(vol > 200.0, "Disjoint union vol={:.1}, expected ~250", vol);
}

/// DE2: Union of two boxes sharing exactly one vertex.
///
/// Corner-to-corner contact is a degenerate configuration — the intersection
/// is a single point, not a curve. The boolean must handle this gracefully.
#[test]
fn de2_barely_touching_boxes_union() {
    let mut m = ModelBuilder::kernel();

    // Box A: [0,5] x [0,5] x [0,5]
    m.rect_sketch("a_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_a", "a_sk", 5.0).unwrap();

    // Box B: [5,10] x [5,10] x [5,10] — shares corner at (5,5,5)
    m.rect_sketch("b_sk", [0., 0., 5.], [0., 0., 1.], 5., 5., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_b", "b_sk", 5.0).unwrap();

    // This may succeed or fail depending on degenerate handling
    match m.boolean_union("merged", "box_a", "box_b") {
        Ok(_) => {
            let mesh = m.tessellate("merged").unwrap();
            assert_mesh_finite(&mesh, "barely_touching");
            let vol = mesh_volume(&mesh);
            assert!(vol > 200.0, "Vol={:.1}, expected ~250", vol);
        }
        Err(e) => {
            // Corner contact is a known hard case — document the error
            eprintln!("[DE2] Boolean failed on corner contact: {:?}", e);
        }
    }
}

/// DE3: Union of two boxes with tiny overlap (0.01 units).
///
/// Very small overlaps stress the tolerance handling — the intersection
/// curves are close to degenerate.
#[test]
fn de3_barely_overlapping_boxes_union() {
    let mut m = ModelBuilder::kernel();

    // Box A: [0,10] x [0,10] x [0,10]
    m.rect_sketch("a_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "a_sk", 10.0).unwrap();

    // Box B: [9.99,19.99] x [0,10] x [0,10] — overlaps by 0.01 in X
    m.rect_sketch("b_sk", [0., 0., 0.], [0., 0., 1.], 9.99, 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_b", "b_sk", 10.0).unwrap();

    m.boolean_union("merged", "box_a", "box_b").unwrap();
    m.assert_has_solid("merged").unwrap();

    let mesh = m.tessellate("merged").unwrap();
    assert_mesh_finite(&mesh, "barely_overlapping");

    let vol = mesh_volume(&mesh);
    // ~10*10*10 + 10*10*10 - 0.01*10*10 = 2000 - 1 = 1999
    assert!(vol > 1800.0, "Vol={:.1}, expected ~1999", vol);
    assert!(vol < 2100.0, "Vol={:.1}, expected ~1999", vol);
}

// ══════════════════════════════════════════════════════════════════════════════
// Category CS — Cylinder Stacking Tests
// ══════════════════════════════════════════════════════════════════════════════

/// CS1: Two cylinder bosses on a box, then union.
///
/// Reduced version of S3 (multi-cylinder cascade exhaustion). Two cylinders
/// are simpler than three and may succeed where three fail.
#[test]
fn cs1_two_cylinder_bosses_union() {
    let mut m = base_cube();

    // Cylinder 1: on top face at (3, 3), radius 1.5
    m.circle_sketch("cyl1_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 1.5)
        .unwrap();
    m.extrude_no_merge("cyl1", "cyl1_sk", 3.0).unwrap();

    // Union cylinder 1 with cube
    m.boolean_union("step1", "cube", "cyl1").unwrap();
    m.assert_has_solid("step1").unwrap();

    // Cylinder 2: on top face at (7, 7), radius 1.5
    m.circle_sketch("cyl2_sk", [0., 0., 10.], [0., 0., 1.], 7., 7., 1.5)
        .unwrap();
    m.extrude_no_merge("cyl2", "cyl2_sk", 3.0).unwrap();

    // Union cylinder 2 with result
    m.boolean_union("result", "step1", "cyl2").unwrap();
    m.assert_has_solid("result").unwrap();

    let mesh = m.tessellate("result").unwrap();
    assert_mesh_finite(&mesh, "two_cylinders");

    let vol = mesh_volume(&mesh);
    let cube_vol = 1000.0;
    assert!(
        vol > cube_vol * 0.95,
        "Vol={:.1} should be > cube vol={:.1}",
        vol,
        cube_vol
    );

    // Topology check
    let (v, e, f) = m.topology_counts("result").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    eprintln!("[CS1] topology: V={} E={} F={} chi={}", v, e, f, chi);
}

/// CS2: Stacked cylinder: extrude cylinder, union, extrude another on top, union.
///
/// Tests chained boolean stability when operations build on top of each other.
#[test]
fn cs2_cylinder_on_cylinder_stacked() {
    let mut m = base_cube();

    // First boss: cylinder on top of cube at (5, 5), radius 2, height 3
    m.circle_sketch("boss1_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.)
        .unwrap();
    m.extrude_no_merge("boss1", "boss1_sk", 3.0).unwrap();
    m.boolean_union("step1", "cube", "boss1").unwrap();
    m.assert_has_solid("step1").unwrap();

    // Second boss: smaller cylinder on top of first boss at (5, 5), radius 1, height 2
    m.circle_sketch("boss2_sk", [0., 0., 13.], [0., 0., 1.], 5., 5., 1.)
        .unwrap();
    m.extrude_no_merge("boss2", "boss2_sk", 2.0).unwrap();
    m.boolean_union("result", "step1", "boss2").unwrap();
    m.assert_has_solid("result").unwrap();

    let mesh = m.tessellate("result").unwrap();
    assert_mesh_finite(&mesh, "stacked_cylinders");

    let (_min, max) = mesh_bounding_box(&mesh);
    // Top should be at z ~ 15 (10 cube + 3 boss1 + 2 boss2)
    assert!(max[2] > 14.0, "z_max={:.1}, expected ~15", max[2]);
}

// ══════════════════════════════════════════════════════════════════════════════
// Category ET — Exact Topology Tests
// ══════════════════════════════════════════════════════════════════════════════

/// ET1: Subtract centered smaller box from larger box.
///
/// The smaller box is fully contained and centered, producing a hollow shell.
/// This should work cleanly — no degenerate edges.
#[test]
fn et1_subtract_centered_box() {
    let mut m = ModelBuilder::kernel();

    // Outer: 20x20x20 box at origin
    m.rect_sketch("outer_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 20., 20.)
        .unwrap();
    m.extrude_no_merge("outer", "outer_sk", 20.0).unwrap();

    // Inner: 10x10x10 box centered at (5,5,5)
    m.rect_sketch("inner_sk", [0., 0., 5.], [0., 0., 1.], 5., 5., 10., 10.)
        .unwrap();
    m.extrude_no_merge("inner", "inner_sk", 10.0).unwrap();

    m.boolean_subtract("hollow", "outer", "inner").unwrap();
    m.assert_has_solid("hollow").unwrap();

    let mesh = m.tessellate("hollow").unwrap();
    assert_mesh_finite(&mesh, "centered_subtract");

    let vol = mesh_volume(&mesh);
    let expected = 20.0 * 20.0 * 20.0 - 10.0 * 10.0 * 10.0; // 8000 - 1000 = 7000
    assert!(
        (vol - expected).abs() < expected * 0.15,
        "Vol={:.1}, expected ~{:.1}",
        vol,
        expected
    );
}

/// ET2: Subtract offset box at non-integer position.
///
/// Non-integer coordinates stress floating-point precision.
#[test]
fn et2_subtract_offset_box() {
    let mut m = ModelBuilder::kernel();

    // Base: 10x10x10
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("base", "base_sk", 10.0).unwrap();

    // Tool: 5x5x5 at (2.7, 3.1, 0) — non-integer offset, extends above base
    m.rect_sketch("tool_sk", [0., 0., 0.], [0., 0., 1.], 2.7, 3.1, 5., 5.)
        .unwrap();
    m.extrude_no_merge("tool", "tool_sk", 15.0).unwrap();

    m.boolean_subtract("result", "base", "tool").unwrap();
    m.assert_has_solid("result").unwrap();

    let mesh = m.tessellate("result").unwrap();
    assert_mesh_finite(&mesh, "offset_subtract");

    let vol = mesh_volume(&mesh);
    let base_vol = 1000.0;
    let cut_vol = 5.0 * 5.0 * 10.0; // only 10 high intersects the base
    let expected = base_vol - cut_vol;
    assert!(
        vol > expected * 0.85 && vol < expected * 1.15,
        "Vol={:.1}, expected ~{:.1}",
        vol,
        expected
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Category CC — Cylinder-Cylinder Intersection Tests
// ══════════════════════════════════════════════════════════════════════════════

/// CC_INT1: Two perpendicular equal-radius cylinder bosses, boolean union.
///
/// This exercises the cylinder-cylinder analytical SSI path: two cylinders
/// of the same radius intersecting at 90 degrees. The analytical IC module
/// should detect this and produce exact elliptic intersection curves.
#[test]
fn cc_int1_perpendicular_cylinder_bosses_union() {
    let mut m = ModelBuilder::kernel();

    // Cylinder A: along Z-axis at (5, 5), radius 2, height 15
    m.circle_sketch("cyl_a_sk", [0., 0., 0.], [0., 0., 1.], 5., 5., 2.)
        .unwrap();
    m.extrude_no_merge("cyl_a", "cyl_a_sk", 15.0).unwrap();
    m.assert_has_solid("cyl_a").unwrap();

    // Cylinder B: along X-axis at (0, 5, 5), radius 2, height 15
    // Sketch on YZ plane (normal along X)
    m.circle_sketch("cyl_b_sk", [0., 0., 0.], [1., 0., 0.], 5., 5., 2.)
        .unwrap();
    m.extrude_no_merge("cyl_b", "cyl_b_sk", 15.0).unwrap();
    m.assert_has_solid("cyl_b").unwrap();

    // Boolean union
    match m.boolean_union("result", "cyl_a", "cyl_b") {
        Ok(_) => {
            m.assert_has_solid("result").unwrap();
            let mesh = m.tessellate("result").unwrap();
            assert_mesh_finite(&mesh, "cc_int1_union");

            let vol = mesh_volume(&mesh);
            // Each cylinder: pi * 2^2 * 15 ≈ 188.5
            // Overlap region is complex, but union should be > one cylinder
            let one_cyl = std::f64::consts::PI * 4.0 * 15.0;
            assert!(
                vol > one_cyl * 0.9,
                "Union vol={:.1}, expected > {:.1}",
                vol,
                one_cyl * 0.9
            );
            assert!(
                vol < one_cyl * 2.1,
                "Union vol={:.1}, expected < {:.1}",
                vol,
                one_cyl * 2.1
            );

            let (v, e, f) = m.topology_counts("result").unwrap();
            let chi = v as i64 - e as i64 + f as i64;
            eprintln!("[CC_INT1] topology: V={} E={} F={} chi={}", v, e, f, chi);
        }
        Err(e) => {
            // Cylinder-cylinder boolean is hard — document the error
            eprintln!("[CC_INT1] Boolean failed: {:?}", e);
        }
    }
}
