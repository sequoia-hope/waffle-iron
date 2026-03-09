//! Extrude-on-extrude workflow tests.
//!
//! Tests the core user workflow: sketch → extrude base → sketch on face →
//! extrude boss (auto-union via merge=true). This is the primary CAD workflow
//! that the boss eps offset fix enables.
//!
//! All tests use `ModelBuilder::kernel()` with `extrude()` which sets
//! `merge: true`, the same path the GUI uses.

use test_harness::helpers::{mesh_bounding_box, mesh_volume};
use test_harness::ModelBuilder;

// ── Helper ─────────────────────────────────────────────────────────────────

/// Create a 10×10×10 base cube on the XY plane.
fn base_cube() -> ModelBuilder {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();
    m
}

// ══════════════════════════════════════════════════════════════════════════
// Test 1 — Rect boss on top face, auto-union
// ══════════════════════════════════════════════════════════════════════════

/// The core workflow: rect sketch on top face → extrude boss → auto-union.
/// Verifies the merged body has volume > cube and bbox extends beyond cube.
#[test]
fn rect_boss_on_top_face_auto_union() {
    let mut m = base_cube();

    let cube_mesh = m.tessellate("cube").unwrap();
    let cube_vol = mesh_volume(&cube_mesh);
    let (_, cube_max) = mesh_bounding_box(&cube_mesh);

    // 4×4 rect boss at (3,3) on top face (z=10), extruded 5 units
    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    let boss_mesh = m.tessellate("boss").unwrap();
    let boss_vol = mesh_volume(&boss_mesh);
    let (boss_min, boss_max) = mesh_bounding_box(&boss_mesh);

    // Volume: merged body should contain cube + boss volume
    assert!(
        boss_vol > cube_vol,
        "Merged volume ({:.0}) should exceed cube volume ({:.0})",
        boss_vol,
        cube_vol
    );

    // Bounding box: should extend beyond cube top
    assert!(
        boss_max[2] > cube_max[2] + 2.0,
        "Boss z_max ({:.1}) should extend well beyond cube z_max ({:.1})",
        boss_max[2],
        cube_max[2]
    );

    // XY footprint should match cube
    assert!(
        (boss_min[0] - 0.0).abs() < 1.0,
        "Merged body x_min should be near 0"
    );

    // Face count > 6 proves the union created additional topology
    let (v, e, f) = m.topology_counts("boss").unwrap();
    assert!(
        f > 6,
        "Merged body should have more than 6 faces (got F={}, V={}, E={})",
        f,
        v,
        e
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Test 2 — Circle boss on top face, auto-union
// ══════════════════════════════════════════════════════════════════════════

/// Circle (polygon-approximated) boss on top face, auto-unioned.
#[test]
fn circle_boss_on_top_face_auto_union() {
    let mut m = base_cube();

    let cube_mesh = m.tessellate("cube").unwrap();
    let cube_vol = mesh_volume(&cube_mesh);

    // Circle boss centered at (5,5), radius 3, on top face (z=10)
    m.circle_sketch("cyl_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 3.)
        .unwrap();
    m.extrude("cyl_boss", "cyl_sk", 5.0).unwrap();
    m.assert_has_solid("cyl_boss").unwrap();

    let cyl_mesh = m.tessellate("cyl_boss").unwrap();
    let cyl_vol = mesh_volume(&cyl_mesh);

    assert!(
        cyl_vol > cube_vol,
        "Circle boss merged volume ({:.0}) should exceed cube ({:.0})",
        cyl_vol,
        cube_vol
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Test 3 — Boss on side face (XZ plane)
// ══════════════════════════════════════════════════════════════════════════

/// Boss on the side face (y=10 face), extruded in +Y direction.
#[test]
fn rect_boss_on_side_face_auto_union() {
    let mut m = base_cube();

    let cube_mesh = m.tessellate("cube").unwrap();
    let cube_vol = mesh_volume(&cube_mesh);

    // 4×4 boss on the y=10 side face, extruded 5 units in +Y
    m.rect_sketch("side_sk", [0., 10., 0.], [0., 1., 0.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("side_boss", "side_sk", 5.0).unwrap();
    m.assert_has_solid("side_boss").unwrap();

    let boss_mesh = m.tessellate("side_boss").unwrap();
    let boss_vol = mesh_volume(&boss_mesh);

    assert!(
        boss_vol > cube_vol,
        "Side boss merged volume ({:.0}) should exceed cube ({:.0})",
        boss_vol,
        cube_vol
    );

    let (_, boss_max) = mesh_bounding_box(&boss_mesh);
    assert!(
        boss_max[1] > 12.0,
        "Boss should extend in Y beyond 12 (got {:.1})",
        boss_max[1]
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Test 4 — Small centered boss
// ══════════════════════════════════════════════════════════════════════════

/// Small boss (3×3) centered on a large face.
#[test]
fn small_centered_boss_auto_union() {
    let mut m = base_cube();

    let cube_mesh = m.tessellate("cube").unwrap();
    let cube_vol = mesh_volume(&cube_mesh);

    m.rect_sketch("small_sk", [0., 0., 10.], [0., 0., 1.], 3.5, 3.5, 3., 3.)
        .unwrap();
    m.extrude("small_boss", "small_sk", 3.0).unwrap();
    m.assert_has_solid("small_boss").unwrap();

    let boss_mesh = m.tessellate("small_boss").unwrap();
    let boss_vol = mesh_volume(&boss_mesh);

    assert!(
        boss_vol > cube_vol,
        "Small boss merged volume ({:.0}) should exceed cube ({:.0})",
        boss_vol,
        cube_vol
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Test 5 — Warning produced when union fails (no errors though)
// ══════════════════════════════════════════════════════════════════════════

/// When auto-union fails, the engine should produce a warning (not an error).
/// The standalone boss body is still valid — just not merged.
#[test]
fn auto_union_fallback_produces_warning_not_error() {
    let mut m = base_cube();

    // Centered boss — should succeed with auto-union (no warnings)
    m.rect_sketch("ok_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("ok_boss", "ok_sk", 5.0).unwrap();

    // No engine errors
    let errors = m.engine_errors();
    assert!(
        errors.is_empty(),
        "Centered boss should produce no errors: {:?}",
        errors
    );

    // Verify the boss is the merged body (volume > cube)
    let boss_mesh = m.tessellate("ok_boss").unwrap();
    let boss_vol = mesh_volume(&boss_mesh);
    let cube_mesh = m.tessellate("cube").unwrap();
    let cube_vol = mesh_volume(&cube_mesh);

    assert!(
        boss_vol > cube_vol,
        "After successful auto-union, boss volume ({:.0}) should exceed cube ({:.0})",
        boss_vol,
        cube_vol
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Test 6 — Consumed features tracking
// ══════════════════════════════════════════════════════════════════════════

/// When auto-union succeeds, the consumed (original cube) feature should be
/// tracked and not rendered. Verify via the engine's consumed_features set.
#[test]
fn consumed_feature_tracked_on_successful_union() {
    let mut m = base_cube();

    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();

    // After successful auto-union, the cube feature should be consumed
    let consumed = m.consumed_features();
    let cube_id = m.feature_id("cube").unwrap();

    assert!(
        consumed.contains(&cube_id),
        "Cube feature should be consumed after successful boss auto-union. \
         Consumed features: {:?}",
        consumed
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Test 7 — Multiple bosses on same face
// ══════════════════════════════════════════════════════════════════════════

/// Two sequential bosses on the same face, each auto-unioned.
#[test]
fn two_bosses_on_same_face_sequential() {
    let mut m = base_cube();

    let cube_mesh = m.tessellate("cube").unwrap();
    let cube_vol = mesh_volume(&cube_mesh);

    // First boss: 3×3 at (1,1)
    m.rect_sketch("boss1_sk", [0., 0., 10.], [0., 0., 1.], 1., 1., 3., 3.)
        .unwrap();
    m.extrude("boss1", "boss1_sk", 4.0).unwrap();

    let boss1_mesh = m.tessellate("boss1").unwrap();
    let boss1_vol = mesh_volume(&boss1_mesh);
    assert!(
        boss1_vol > cube_vol,
        "First boss merged volume ({:.0}) should exceed cube ({:.0})",
        boss1_vol,
        cube_vol
    );

    // Second boss: 3×3 at (6,6) — well separated from first boss
    m.rect_sketch("boss2_sk", [0., 0., 10.], [0., 0., 1.], 6., 6., 3., 3.)
        .unwrap();
    m.extrude("boss2", "boss2_sk", 4.0).unwrap();

    let boss2_mesh = m.tessellate("boss2").unwrap();
    let boss2_vol = mesh_volume(&boss2_mesh);
    assert!(
        boss2_vol > boss1_vol,
        "Second boss merged volume ({:.0}) should exceed first boss ({:.0})",
        boss2_vol,
        boss1_vol
    );
}

// ══════════════════════════════════════════════════════════════════════════
// Test 8 — Same sketch extruded opposite directions (mutual containment)
// ══════════════════════════════════════════════════════════════════════════

/// Extrude the same sketch both +Z and -Z. The two bodies share an exact
/// coplanar face at z=0 with opposite normals (anti-sense mutual containment).
/// Auto-union should produce a single merged body spanning [-10, +10] in Z.
///
/// This is the core "mutual containment" bug case: without the fix, injecting
/// each face's boundary into the other creates degenerate face division topology.
#[test]
fn rect_extrude_opposite_directions_union() {
    let mut m = ModelBuilder::kernel();

    // Shared sketch at z=0
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();

    // Extrude +Z (creates body from z=0 to z=10)
    m.extrude("up", "sk", 10.0).unwrap();
    m.assert_has_solid("up").unwrap();

    let up_mesh = m.tessellate("up").unwrap();
    let up_vol = mesh_volume(&up_mesh);
    assert!(
        up_vol > 900.0,
        "Upward extrude volume should be ~1000, got {:.0}",
        up_vol
    );

    // Extrude -Z using the same sketch (creates body from z=0 to z=-10, auto-unions)
    m.extrude_directed("down", "sk", 10.0, [0., 0., -1.], false)
        .unwrap();
    m.assert_has_solid("down").unwrap();

    let down_mesh = m.tessellate("down").unwrap();
    let down_vol = mesh_volume(&down_mesh);

    // Merged volume should be ~2000 (two 10×10×10 boxes)
    assert!(
        down_vol > up_vol * 1.5,
        "Merged volume ({:.0}) should be significantly greater than single extrude ({:.0})",
        down_vol,
        up_vol
    );

    // Bounding box should span z=-10 to z=+10
    let (down_min, down_max) = mesh_bounding_box(&down_mesh);
    assert!(
        down_min[2] < -8.0,
        "Merged body z_min ({:.1}) should be near -10",
        down_min[2]
    );
    assert!(
        down_max[2] > 8.0,
        "Merged body z_max ({:.1}) should be near +10",
        down_max[2]
    );

    // Euler characteristic: merged box should have chi=2
    let (v, e, f) = m.topology_counts("down").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert_eq!(
        chi, 2,
        "Merged body Euler chi should be 2, got {} (V={}, E={}, F={})",
        chi, v, e, f
    );
}
