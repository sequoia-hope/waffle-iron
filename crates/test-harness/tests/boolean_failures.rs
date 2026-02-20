//! Boolean failure diagnostic tests for TruckKernel.
//!
//! These tests systematically probe the boundary conditions of truck 0.4's
//! boolean operations. They document known failures and verify workarounds.
//!
//! Key findings documented by these tests:
//!   - With coplanar face handling in truck-shapeops, eps=0.0 works for
//!     extrude_cut (no offset needed). Both rect and circle cuts succeed.
//!   - Chained booleans on boolean results often produce degenerate
//!     0-face solids in truck 0.4.
//!   - Boss (additive) extrude auto-unions with the most recent solid.
//!     If the union fails, the boss falls back to a standalone body.
//!
//! Categories:
//!   A — Rect-on-rect boolean (4 tests)
//!   B — Circle-on-box edge cases (3 tests)
//!   C — Boss (additive) extrude (3 tests)
//!   D — Multi-cut chains (3 tests)
//!   E — Sketch plane variations (3 tests)
//!   F — Volume verification (3 tests)

use test_harness::helpers::{mesh_bounding_box, mesh_volume};
use test_harness::ModelBuilder;

// ── Helper ─────────────────────────────────────────────────────────────────

/// Create a standard 10×10×10 base cube and return the builder.
///
/// Cube is created from a rect sketch at origin [0,0,0] on the XY plane,
/// extruded 10 units in +Z. Due to the tangent frame computation, the
/// actual 3D extent is approximately x∈[0,10], y∈[−10,0], z∈[0,10].
fn base_cube() -> ModelBuilder {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();
    m
}

// ══════════════════════════════════════════════════════════════════════════
// Category A — Rect-on-Rect Boolean
// ══════════════════════════════════════════════════════════════════════════

/// Two offset extrudes auto-union into a single merged body.
/// The second extrude (offset from the first) automatically merges.
#[test]
fn rect_offset_extrudes_auto_union() {
    let mut m = ModelBuilder::truck();

    // First box: 10×10×10
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    let box1_mesh = m.tessellate("box1").unwrap();
    let box1_vol = mesh_volume(&box1_mesh);

    // Second box: 6×6×10 offset (starts at z=5), auto-unions with box1
    m.rect_sketch("sk2", [2., 2., 5.], [0., 0., 1.], 0., 0., 6., 6.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();
    m.assert_has_solid("box2").unwrap();

    // The auto-union should produce more faces and more volume
    let merged_mesh = m.tessellate("box2").unwrap();
    let merged_vol = mesh_volume(&merged_mesh);
    let (_, _, f) = m.topology_counts("box2").unwrap();

    assert!(
        f > 6,
        "Auto-union should produce more than 6 faces (got {})",
        f
    );
    assert!(
        merged_vol > box1_vol,
        "Auto-union should increase volume (box1={:.0}, merged={:.0})",
        box1_vol,
        merged_vol
    );
}

/// Rect extrude_cut via the engine's cut path. truck 0.4's box-box boolean
/// at 10x scale with tol=0.05 returns the unchanged cube regardless of eps
/// offset. This is a fundamental truck limitation at this geometry scale.
#[test]
fn rect_cut_via_extrude_cut() {
    let mut m = base_cube();

    m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude_cut("slot", "cut_sk", 15.0).unwrap();
    m.assert_has_solid("slot").unwrap();
    m.assert_no_errors().unwrap();

    let (_, _, f) = m.topology_counts("slot").unwrap();
    assert!(f > 6, "Rect cut should add faces (got {})", f);
}

/// Rect cut where profile edges coincide with box edges.
/// Fixed via coplanar perturbation retry in boolean ops.
#[test]
#[ignore = "truck: coplanar edge coincidence — NotSimpleWire from divide_one_face"]
fn rect_cut_coplanar_edges() {
    let mut m = base_cube();

    m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 0., 2., 10., 6.)
        .unwrap();
    m.extrude_cut("slot", "cut_sk", 15.0).unwrap();
    m.assert_has_solid("slot").unwrap();
    m.assert_no_errors().unwrap();
}

/// Rect cut profile exactly matches the box top face.
/// Creates fully coplanar faces on all four sides of the tool.
#[test]
fn rect_cut_at_box_boundary() {
    let mut m = base_cube();

    m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_cut("slot", "cut_sk", 15.0).unwrap();
    m.assert_has_solid("slot").unwrap();
    m.assert_no_errors().unwrap();
}

// ══════════════════════════════════════════════════════════════════════════
// Category B — Circle-on-Box Cut Edge Cases
// ══════════════════════════════════════════════════════════════════════════

/// Circle positioned tangent to a box edge. The circle touches the
/// face boundary at exactly one point. Fixed via coplanar perturbation retry.
#[test]
#[ignore = "truck 0.4: circle tangent to box edge — NoSolid from boolean"]
fn circle_cut_tangent_to_box_edge() {
    let mut m = base_cube();

    m.circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 2.5, 2.5)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 15.0).unwrap();
    m.assert_has_solid("hole").unwrap();
    m.assert_no_errors().unwrap();
}

/// Circle partially extends beyond the box face boundary.
/// The tool body protrudes past a box edge, requiring truck to
/// compute a partial intersection curve on the face.
#[test]
#[ignore = "truck 0.4: circle extending beyond face boundary — NoSolid despite cardinal perturbation"]
fn circle_cut_crossing_box_edge() {
    let mut m = base_cube();

    m.circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 1., 2.5)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 15.0).unwrap();
    m.assert_has_solid("hole").unwrap();
    m.assert_no_errors().unwrap();
}

/// Circle centered near a box corner, intersecting two adjacent edges.
/// Despite the double-edge intersection, truck handles this case because
/// the curved intersection curves remain well-defined.
#[test]
fn circle_cut_at_box_corner() {
    let mut m = base_cube();

    m.circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 8., 8., 3.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 15.0).unwrap();
    m.assert_has_solid("hole").unwrap();
    m.assert_no_errors().unwrap();

    let (_, _, f) = m.topology_counts("hole").unwrap();
    assert!(
        f > 6,
        "Corner circle cut should add faces beyond original 6 (got {})",
        f
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Category C — Boss (Additive) Extrude
// ══════════════════════════════════════════════════════════════════════════

/// A second (boss) extrude automatically merges with the first via
/// boolean union. The engine auto-unions non-cut extrudes with the
/// most recent solid.
#[test]
fn boss_extrude_auto_unions_with_existing_body() {
    let mut m = base_cube();

    let cube_mesh = m.tessellate("cube").unwrap();
    let cube_vol = mesh_volume(&cube_mesh);

    // Boss sketch on the top face (z=10), extruded upward by 5.
    // Auto-union merges this with the cube into a single body.
    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    // The merged body should have more volume than the original cube
    let boss_mesh = m.tessellate("boss").unwrap();
    let boss_vol = mesh_volume(&boss_mesh);

    assert!(
        boss_vol > cube_vol,
        "Boss auto-union should increase volume (boss_vol={:.0}, cube_vol={:.0})",
        boss_vol,
        cube_vol
    );
}

/// Boss extrude on top of cube auto-unions via coplanar perturbation.
/// The boss shares a face with the cube (z=10). The auto-union handles
/// this via the perturbation retry in boolean ops.
#[test]
fn boss_extrude_coplanar_auto_union() {
    let mut m = base_cube();

    let cube_mesh = m.tessellate("cube").unwrap();
    let cube_vol = mesh_volume(&cube_mesh);

    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    // Auto-union should produce a merged body extending above the cube
    let merged_mesh = m.tessellate("boss").unwrap();
    let merged_vol = mesh_volume(&merged_mesh);
    assert!(
        merged_vol > cube_vol,
        "Auto-union should increase volume (cube={:.0}, merged={:.0})",
        cube_vol,
        merged_vol
    );
}

/// Offset boss auto-union. When the boss is displaced so no
/// faces are coplanar, the auto-union succeeds directly.
#[test]
fn boss_extrude_offset_auto_union() {
    let mut m = ModelBuilder::truck();

    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    let box1_mesh = m.tessellate("box1").unwrap();
    let box1_vol = mesh_volume(&box1_mesh);

    // Box 2 offset in all axes — auto-unions with box1
    m.rect_sketch("sk2", [3., 3., 5.], [0., 0., 1.], 0., 0., 6., 6.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();
    m.assert_has_solid("box2").unwrap();

    let merged_mesh = m.tessellate("box2").unwrap();
    let merged_vol = mesh_volume(&merged_mesh);
    assert!(
        merged_vol > box1_vol,
        "Auto-union should increase volume (box1={:.0}, merged={:.0})",
        box1_vol,
        merged_vol
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Category D — Multi-Cut Chains
// ══════════════════════════════════════════════════════════════════════════

/// Two sequential circle cuts using extrude_cut. The second boolean
/// operates on the result of the first. truck 0.4 often produces
/// degenerate 0-face solids when chaining booleans on boolean results.
#[test]
fn two_circle_cuts_chained() {
    let mut m = base_cube();

    m.circle_sketch("cut1_sk", [0., 0., 10.], [0., 0., 1.], 3., 5., 1.5)
        .unwrap();
    m.extrude_cut("hole1", "cut1_sk", 15.0).unwrap();
    m.assert_has_solid("hole1").unwrap();

    let (_, _, f1) = m.topology_counts("hole1").unwrap();

    m.circle_sketch("cut2_sk", [0., 0., 10.], [0., 0., 1.], 7., 5., 1.5)
        .unwrap();
    m.extrude_cut("hole2", "cut2_sk", 15.0).unwrap();
    m.assert_has_solid("hole2").unwrap();
    m.assert_no_errors().unwrap();

    let (_, _, f2) = m.topology_counts("hole2").unwrap();
    assert!(
        f2 > f1,
        "Two-hole body should have more faces than one-hole (got {} vs {})",
        f2,
        f1
    );
}

/// Directed extrude auto-unions with existing body. A directed
/// non-cut extrude in the -Z direction auto-unions with the cube.
#[test]
fn directed_extrude_auto_unions() {
    let mut m = base_cube();

    let cube_mesh = m.tessellate("cube").unwrap();
    let cube_vol = mesh_volume(&cube_mesh);

    // Cylinder extending downward from above the cube, auto-unions
    m.circle_sketch("cyl_sk", [0., 0., 11.], [0., 0., 1.], 3., 5., 1.5)
        .unwrap();
    m.extrude_directed("cyl", "cyl_sk", 16.0, [0., 0., -1.], false)
        .unwrap();
    m.assert_has_solid("cyl").unwrap();

    // Auto-union should increase volume
    let merged_mesh = m.tessellate("cyl").unwrap();
    let merged_vol = mesh_volume(&merged_mesh);
    assert!(
        merged_vol > cube_vol,
        "Auto-union should increase volume (cube={:.0}, merged={:.0})",
        cube_vol,
        merged_vol
    );
}

/// Cut, then boss, then cut. The boss auto-unions with the cut result,
/// so the second cut operates on the merged body (cube+boss with hole).
#[test]
fn cut_then_boss_then_cut() {
    let mut m = base_cube();

    // First cut: circle on top face
    m.circle_sketch("cut1_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 1.5)
        .unwrap();
    m.extrude_cut("hole1", "cut1_sk", 15.0).unwrap();
    m.assert_has_solid("hole1").unwrap();

    // Boss: auto-unions with hole1 result
    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    // The boss merged body extends above the cube
    let boss_mesh = m.tessellate("boss").unwrap();
    let (_, bb_max) = mesh_bounding_box(&boss_mesh);
    assert!(
        bb_max[2] > 12.0,
        "Boss auto-union should extend above cube (z_max ≈ 15). Got z_max={:.1}",
        bb_max[2]
    );

    // Second cut: circle on boss top face (z=15)
    m.circle_sketch("cut2_sk", [0., 0., 15.], [0., 0., 1.], 5., 5., 1.0)
        .unwrap();
    m.extrude_cut("hole2", "cut2_sk", 20.0).unwrap();
    m.assert_has_solid("hole2").unwrap();
}

// ══════════════════════════════════════════════════════════════════════════
// Category E — Sketch Plane Variations
// ══════════════════════════════════════════════════════════════════════════

/// Circle cut from the XZ plane (normal [0,1,0]) through the y=0 face.
/// Verifies that cuts work from non-XY sketch orientations.
/// Uses circle profile because rect extrude_cut has the eps/tolerance issue.
///
/// tangent_x_from_normal([0,1,0]) = [-1,0,0], yAxis = [0,0,1].
/// Sketch (0,0) at origin [5,0,5] maps to world (5, 0, 5) — inside cube's y=0 face.
/// Extrude_cut goes -Y through the cube (y∈[-10,0]).
#[test]
fn cut_from_xz_plane() {
    let mut m = base_cube();

    m.circle_sketch("xz_sk", [5., 0., 5.], [0., 1., 0.], 0., 0., 2.0)
        .unwrap();
    m.extrude_cut("xz_cut", "xz_sk", 15.0).unwrap();
    m.assert_has_solid("xz_cut").unwrap();
    m.assert_no_errors().unwrap();

    let (_, _, f) = m.topology_counts("xz_cut").unwrap();
    assert!(
        f > 6,
        "XZ plane cut should add faces beyond original 6 (got {})",
        f
    );
}

/// Circle cut from the YZ plane (normal [1,0,0]) through the x=10 face.
/// Tests a third orthogonal orientation.
///
/// tangent_x_from_normal([1,0,0]) = [0,1,0], yAxis = [0,0,1].
/// Sketch (0,0) at origin [10,-5,5] maps to world (10, -5, 5) — inside cube's x=10 face.
/// Extrude_cut goes -X through the cube (x∈[0,10]).
#[test]
fn cut_from_yz_plane() {
    let mut m = base_cube();

    m.circle_sketch("yz_sk", [10., -5., 5.], [1., 0., 0.], 0., 0., 2.0)
        .unwrap();
    m.extrude_cut("yz_cut", "yz_sk", 15.0).unwrap();
    m.assert_has_solid("yz_cut").unwrap();
    m.assert_no_errors().unwrap();

    let (_, _, f) = m.topology_counts("yz_cut").unwrap();
    assert!(
        f > 6,
        "YZ plane cut should add faces beyond original 6 (got {})",
        f
    );
}

/// Cut extrude in a non-axis-aligned direction ([1,0,1] at 45°).
/// The angled tool creates no coplanar faces with the axis-aligned box,
/// so the boolean should succeed.
/// NOTE: Fails because angled extrude_directed produces a cylinder boolean
/// that truck cannot handle (known limitation).
#[test]
#[ignore]
fn cut_from_angled_direction() {
    let mut m = base_cube();

    // Circle on top face, cut in angled direction [1, 0, 1].
    m.circle_sketch("ang_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.0)
        .unwrap();
    m.extrude_directed("ang_cut", "ang_sk", 20.0, [1., 0., 1.], true)
        .unwrap();
    m.assert_has_solid("ang_cut").unwrap();
    m.assert_no_errors().unwrap();

    let (_, _, f) = m.topology_counts("ang_cut").unwrap();
    assert!(
        f > 6,
        "Angled cut should add faces beyond original 6 (got {})",
        f
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Category F — Volume Verification
// ══════════════════════════════════════════════════════════════════════════

/// A circular cut should reduce the volume of the body.
#[test]
fn cut_reduces_volume() {
    let mut m = base_cube();

    let cube_mesh = m.tessellate("cube").unwrap();
    let cube_vol = mesh_volume(&cube_mesh);
    assert!(cube_vol > 100.0, "Cube should have substantial volume");

    m.circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.5)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 15.0).unwrap();

    let cut_mesh = m.tessellate("hole").unwrap();
    let cut_vol = mesh_volume(&cut_mesh);

    assert!(
        cut_vol < cube_vol,
        "Cut should reduce volume (before={:.0}, after={:.0})",
        cube_vol,
        cut_vol
    );

    let removed = cube_vol - cut_vol;
    assert!(
        removed > 50.0,
        "Should remove substantial material (removed only {:.0})",
        removed
    );
}

/// An auto-union of an offset boss should increase the total volume.
#[test]
fn boss_auto_union_increases_volume() {
    let mut m = ModelBuilder::truck();

    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    let box_mesh = m.tessellate("box1").unwrap();
    let box_vol = mesh_volume(&box_mesh);

    // Second extrude auto-unions with box1
    m.rect_sketch("sk2", [3., 3., 5.], [0., 0., 1.], 0., 0., 6., 6.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    let merged_mesh = m.tessellate("box2").unwrap();
    let merged_vol = mesh_volume(&merged_mesh);

    assert!(
        merged_vol > box_vol,
        "Auto-union should increase volume (box={:.0}, merged={:.0})",
        box_vol,
        merged_vol
    );
}

/// The bounding box of a cut body should not exceed the original.
#[test]
fn cut_bbox_unchanged() {
    let mut m = base_cube();

    let cube_mesh = m.tessellate("cube").unwrap();
    let (cube_min, cube_max) = mesh_bounding_box(&cube_mesh);

    m.circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.5)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 15.0).unwrap();

    let cut_mesh = m.tessellate("hole").unwrap();
    let (cut_min, cut_max) = mesh_bounding_box(&cut_mesh);

    let tol = 0.5;
    for i in 0..3 {
        assert!(
            cut_min[i] >= cube_min[i] - tol,
            "Cut min[{}]={:.2} should not be less than cube min[{}]={:.2}",
            i,
            cut_min[i],
            i,
            cube_min[i]
        );
        assert!(
            cut_max[i] <= cube_max[i] + tol,
            "Cut max[{}]={:.2} should not exceed cube max[{}]={:.2}",
            i,
            cut_max[i],
            i,
            cube_max[i]
        );
    }
}
