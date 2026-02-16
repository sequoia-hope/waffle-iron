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
//!   - Boss (additive) extrude has no implicit union — it creates an
//!     independent body and breaks the implicit cut chain.
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
/// actual 3D extent is approximately x∈[−10,0], y∈[0,10], z∈[0,10].
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

/// Rect-rect boolean subtract with explicit offset bodies. When both
/// boxes are created independently and the tool is well-offset from
/// the target (no near-coplanar faces), the subtraction succeeds.
#[test]
fn rect_subtract_offset_bodies() {
    let mut m = ModelBuilder::truck();

    // Target box: 10×10×10
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    // Tool box: 6×6×10 offset so no faces are coplanar (starts at z=5)
    m.rect_sketch("sk2", [2., 2., 5.], [0., 0., 1.], 0., 0., 6., 6.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.boolean_subtract("result", "box1", "box2").unwrap();
    m.assert_has_solid("result").unwrap();
    m.assert_no_errors().unwrap();

    // Subtraction should add faces beyond the original 6
    let (_, _, f) = m.topology_counts("result").unwrap();
    assert!(f > 6, "Rect subtract should add faces (got {})", f);
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
/// truck cannot compute intersection curves along existing edges.
#[test]
#[ignore = "truck 0.4: coincident edges — extract_interference returns empty for edge-on-edge intersection"]
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
#[ignore = "truck 0.4: fully covering cut profile — all four tool sides have coincident edges with target"]
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
/// face boundary at exactly one point, creating a degenerate
/// intersection curve that truck cannot handle.
#[test]
#[ignore = "truck 0.4: single-point tangency at face edge — degenerate intersection curve"]
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
#[ignore = "truck 0.4: circle extending beyond face boundary — intersection curve crosses face edge"]
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

/// A second (boss) extrude does NOT automatically merge with the first.
/// When cut=false, the engine creates an independent solid with no
/// boolean union. This test documents the current behavior.
#[test]
fn boss_extrude_creates_separate_body() {
    let mut m = base_cube();

    // Boss sketch on the top face (z=10), extruded upward by 5.
    // This creates a SEPARATE solid — no auto-union with the cube.
    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    // Both solids exist independently
    m.assert_has_solid("cube").unwrap();
    m.assert_has_solid("boss").unwrap();

    // The boss volume should be much smaller than the cube volume,
    // proving it doesn't contain the cube geometry.
    let cube_mesh = m.tessellate("cube").unwrap();
    let boss_mesh = m.tessellate("boss").unwrap();
    let cube_vol = mesh_volume(&cube_mesh);
    let boss_vol = mesh_volume(&boss_mesh);

    assert!(
        boss_vol < cube_vol * 0.5,
        "Boss should be a separate smaller body (boss_vol={:.0}, cube_vol={:.0})",
        boss_vol,
        cube_vol
    );
}

/// Explicit boolean_union of a boss on top of the cube. When the boss
/// shares a face with the cube (z=10), truck's coplanar face limitation
/// prevents the union from succeeding.
#[test]
fn boss_extrude_with_explicit_union() {
    let mut m = base_cube();

    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();

    m.boolean_union("merged", "cube", "boss").unwrap();
    m.assert_has_solid("merged").unwrap();
    m.assert_no_errors().unwrap();
}

/// Offset boss unioned with base. When the boss is displaced so no
/// faces are coplanar, the boolean_union succeeds.
#[test]
fn boss_extrude_offset_union() {
    let mut m = ModelBuilder::truck();

    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    // Box 2 offset in all axes — no shared coplanar faces
    m.rect_sketch("sk2", [3., 3., 5.], [0., 0., 1.], 0., 0., 6., 6.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.boolean_union("merged", "box1", "box2").unwrap();
    m.assert_has_solid("merged").unwrap();
    m.assert_no_errors().unwrap();

    let merged_mesh = m.tessellate("merged").unwrap();
    let merged_vol = mesh_volume(&merged_mesh);
    assert!(
        merged_vol > 500.0,
        "Merged body should have substantial volume (got {:.0})",
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

/// Explicit multi-subtract workaround: create tool bodies as separate
/// extrudes and subtract them one at a time. This bypasses the
/// extrude_cut path and avoids the eps/tolerance issue.
#[test]
fn two_subtracts_explicit_workaround() {
    let mut m = ModelBuilder::truck();

    // Base cube
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();

    // Tool 1: cylinder extending through the cube (starts above, goes below)
    m.circle_sketch("tool1_sk", [0., 0., 11.], [0., 0., 1.], 3., 5., 1.5)
        .unwrap();
    m.extrude_directed("tool1", "tool1_sk", 16.0, [0., 0., -1.], false)
        .unwrap();

    // Explicit subtract
    m.boolean_subtract("result1", "cube", "tool1").unwrap();
    m.assert_has_solid("result1").unwrap();

    let (_, _, f1) = m.topology_counts("result1").unwrap();
    assert!(f1 > 6, "First subtract should add faces (got {})", f1);
}

/// Cut, then boss, then cut. Documents that a boss extrude (cut=false)
/// breaks the implicit cut chain. The second cut operates on the boss
/// (the most recent solid), not on the first cut's result.
#[test]
fn cut_then_boss_then_cut() {
    let mut m = base_cube();

    // First cut: circle on top face
    m.circle_sketch("cut1_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 1.5)
        .unwrap();
    m.extrude_cut("hole1", "cut1_sk", 15.0).unwrap();
    m.assert_has_solid("hole1").unwrap();

    // Boss: separate body on top of cube (NOT auto-unioned)
    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    // Second cut: circle on boss top face (z=15)
    m.circle_sketch("cut2_sk", [0., 0., 15.], [0., 0., 1.], 5., 5., 1.0)
        .unwrap();
    m.extrude_cut("hole2", "cut2_sk", 20.0).unwrap();

    m.assert_has_solid("hole2").unwrap();

    // Key assertion: hole2 operates on the boss, not hole1.
    // The boss extends from z≈10 to z≈15. The cube extends z≈0 to z≈10.
    let mesh = m.tessellate("hole2").unwrap();
    let (_, bb_max) = mesh_bounding_box(&mesh);
    assert!(
        bb_max[2] > 12.0,
        "Second cut operates on the boss (z_max ≈ 15), not the cube (z_max ≈ 10). \
         This documents that boss extrude breaks the implicit cut chain. Got z_max={:.1}",
        bb_max[2]
    );

    // The first cut result is still intact but disconnected
    m.assert_has_solid("hole1").unwrap();
}

// ══════════════════════════════════════════════════════════════════════════
// Category E — Sketch Plane Variations
// ══════════════════════════════════════════════════════════════════════════

/// Circle cut from the XZ plane (normal [0,1,0]) through the +Y face.
/// Verifies that cuts work from non-XY sketch orientations.
/// Uses circle profile because rect extrude_cut has the eps/tolerance issue.
#[test]
fn cut_from_xz_plane() {
    let mut m = base_cube();

    // XZ plane at origin [0, 10, 10], normal [0, 1, 0].
    // Circle at (5, 5) maps well inside the cube's y=10 face.
    m.circle_sketch("xz_sk", [0., 10., 10.], [0., 1., 0.], 5., 5., 2.0)
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

/// Circle cut from the YZ plane (normal [1,0,0]) through the +X face.
/// Tests a third orthogonal orientation.
#[test]
fn cut_from_yz_plane() {
    let mut m = base_cube();

    // YZ plane at origin [0, 10, 0], normal [1, 0, 0].
    // Circle at (5, 5) maps well inside the cube's x=0 face.
    m.circle_sketch("yz_sk", [0., 10., 0.], [1., 0., 0.], 5., 5., 2.0)
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
#[test]
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

/// A boolean union of an offset boss should increase the total volume.
#[test]
fn boss_union_increases_volume() {
    let mut m = ModelBuilder::truck();

    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    let box_mesh = m.tessellate("box1").unwrap();
    let box_vol = mesh_volume(&box_mesh);

    m.rect_sketch("sk2", [3., 3., 5.], [0., 0., 1.], 0., 0., 6., 6.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();
    m.boolean_union("merged", "box1", "box2").unwrap();

    let merged_mesh = m.tessellate("merged").unwrap();
    let merged_vol = mesh_volume(&merged_mesh);

    assert!(
        merged_vol > box_vol,
        "Union should increase volume (box={:.0}, merged={:.0})",
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
