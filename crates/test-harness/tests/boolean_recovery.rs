//! Recovery branch and perturbation cascade tests for TruckKernel booleans.
//!
//! These tests exercise the finalize_boolean_shell recovery levels (0-6) in
//! `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` and the
//! perturbation cascade in `crates/kernel-fork/src/healing.rs`.
//!
//! Categories:
//!   R — Recovery Level Tests (exercise finalize_boolean_shell branches)
//!   S — Perturbation Strategy Tests (exercise healing.rs cascade strategies)
//!   T — Cascade Behavior Tests (verify cascade mechanics)

use test_harness::helpers::{mesh_bounding_box, mesh_volume};
use test_harness::ModelBuilder;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Create a standard 10×10×10 base cube on the XY plane.
/// Cube spans x∈[0,10], y∈[0,10], z∈[0,10].
fn base_cube() -> ModelBuilder {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();
    m
}

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

// ══════════════════════════════════════════════════════════════════════════════
// Category R — Recovery Level Tests
// ══════════════════════════════════════════════════════════════════════════════

/// R1: Clean union of two well-separated boxes — no recovery needed (level 0).
///
/// Two boxes with no shared faces, edges, or vertices. The boolean should
/// succeed on the first Solid::try_new after weld, exercising only recovery
/// level 0 (standard weld). Verifies the happy path through finalize_boolean_shell.
#[test]
fn r1_recovery_level_0_clean_union() {
    let mut m = ModelBuilder::truck();

    // Box A: x∈[0,5], y∈[0,5], z∈[0,5]
    m.rect_sketch("a_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_a", "a_sk", 5.0).unwrap();
    m.assert_has_solid("box_a").unwrap();

    // Box B: x∈[7,12], y∈[7,12], z∈[0,5] — clearly separated from A
    m.rect_sketch("b_sk", [0., 0., 0.], [0., 0., 1.], 7., 7., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_b", "b_sk", 5.0).unwrap();
    m.assert_has_solid("box_b").unwrap();

    m.boolean_union("merged", "box_a", "box_b").unwrap();
    m.assert_has_solid("merged").unwrap();

    let mesh = m.tessellate("merged").unwrap();
    let vol = mesh_volume(&mesh);
    // Two 5×5×5 boxes = 125 + 125 = 250
    assert!(
        vol > 200.0,
        "Union of two separated boxes should have volume ~250 (got {:.1})",
        vol
    );
}

/// R2: Union of two overlapping boxes — exercises standard weld recovery.
///
/// Boxes overlap by 2 units along X, creating intersection curves that
/// need welding. Standard weld (level 0-1) should close the shell.
#[test]
fn r2_recovery_level_weld_overlapping_boxes() {
    let mut m = ModelBuilder::truck();

    // Box A: x∈[0,6], y∈[0,5], z∈[0,5]
    m.rect_sketch("a_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 6., 5.)
        .unwrap();
    m.extrude_no_merge("box_a", "a_sk", 5.0).unwrap();
    m.assert_has_solid("box_a").unwrap();

    // Box B: x∈[4,10], y∈[0,5], z∈[0,5] — overlaps A by 2 units in X
    m.rect_sketch("b_sk", [0., 0., 0.], [0., 0., 1.], 4., 0., 6., 5.)
        .unwrap();
    m.extrude_no_merge("box_b", "b_sk", 5.0).unwrap();
    m.assert_has_solid("box_b").unwrap();

    m.boolean_union("merged", "box_a", "box_b").unwrap();
    m.assert_has_solid("merged").unwrap();

    let mesh = m.tessellate("merged").unwrap();
    let vol = mesh_volume(&mesh);
    // Union: 6×5×5 + 6×5×5 - 2×5×5 overlap = 150+150-50 = 250
    let expected = 250.0;
    let tol = expected * 0.15;
    assert!(
        (vol - expected).abs() < tol,
        "Overlapping union volume should be ~{:.0} (got {:.1})",
        expected,
        vol
    );
}

/// R3: Union of two abutting boxes (shared face) — exercises wider weld or
/// coplanar recovery.
///
/// Boxes share a face at x=5. The coplanar face creates a degenerate
/// intersection that may require wider weld tolerance or perturbation
/// cascade to resolve. This tests the recovery levels 1-2 (wider weld).
///
/// Previously failed due to sketch coordinate mapping bug (rect offsets
/// instead of plane_origin offset). Fixed in Sprint 42.
#[test]
// Previously ignored: test had sketch coordinate mapping bug (rect offsets
// instead of plane_origin offset). Fixed Sprint 42 — coplanar union works.
fn r3_recovery_wider_weld_abutting_boxes() {
    let mut m = ModelBuilder::truck();

    // Box A: 5×5×5 at origin. plane_origin offset approach (same as EC1)
    // to ensure correct sketch → world coordinate mapping.
    m.rect_sketch("a_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_a", "a_sk", 5.0).unwrap();
    m.assert_has_solid("box_a").unwrap();

    // Box B: 5×5×5 abutting box A. Use plane_origin=[5,0,0] to offset
    // the sketch plane (matching EC1 pattern) rather than sketch-coord
    // offset which creates geometry at the wrong world position.
    m.rect_sketch("b_sk", [5., 0., 0.], [0., 0., 1.], 0., 0., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_b", "b_sk", 5.0).unwrap();
    m.assert_has_solid("box_b").unwrap();

    m.boolean_union("merged", "box_a", "box_b").unwrap();
    m.assert_has_solid("merged").unwrap();

    let mesh = m.tessellate("merged").unwrap();
    let vol = mesh_volume(&mesh);
    // Two abutting 5×5×5 boxes = 250 total
    let expected = 250.0;
    let tol = expected * 0.15;
    assert!(
        (vol - expected).abs() < tol,
        "Abutting union volume should be ~{:.0} (got {:.1})",
        expected,
        vol
    );

    // Bounding box should span x∈[0,10]
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    assert!(
        bb_max[0] > 9.5,
        "Abutting union should extend to x≈10 (got {:.1})",
        bb_max[0]
    );
    assert!(
        bb_min[0] < 0.5,
        "Abutting union should start at x≈0 (got {:.1})",
        bb_min[0]
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Category S — Perturbation Strategy Tests
// ══════════════════════════════════════════════════════════════════════════════

/// S1: Coplanar face perturbation — two boxes sharing a face with
/// different-sized cross sections.
///
/// Box A (10×10×10) and Box B (5×5×5) share face at z=10, but B is
/// centered on A's top face. The coplanar face triggers detect_all_coplanar_directions
/// in the perturbation cascade, and the cascade should try coplanar-dir strategies.
#[test]
fn s1_coplanar_perturbation_boss_on_face() {
    let mut m = base_cube();

    // Boss on top face (z=10), 5×5 centered at (2.5, 2.5)
    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 2.5, 2.5, 5., 5.)
        .unwrap();
    m.extrude_no_merge("boss", "boss_sk", 5.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    m.boolean_union("merged", "cube", "boss").unwrap();
    m.assert_has_solid("merged").unwrap();

    let mesh = m.tessellate("merged").unwrap();
    let vol = mesh_volume(&mesh);
    // 10×10×10 + 5×5×5 = 1000 + 125 = 1125
    assert!(
        vol > 1050.0,
        "Boss union should have volume > 1050 (got {:.1})",
        vol
    );

    let (_, bb_max) = mesh_bounding_box(&mesh);
    assert!(
        bb_max[2] > 14.0,
        "Boss should extend to z≈15 (got {:.1})",
        bb_max[2]
    );
}

/// S2: Coplanar cut — rectangular cut exactly aligned with a face.
///
/// A 4×4 cut on the z=10 face of a 10×10×10 cube. The cut tool's top
/// face is coplanar with the cube's top face, triggering coplanar
/// detection and perturbation strategies.
#[test]
fn s2_coplanar_cut_aligned_face() {
    let mut m = base_cube();

    // 4×4 cut centered at (3,3) on top face, depth 5
    m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude_cut("cut", "cut_sk", 5.0).unwrap();
    m.assert_has_solid("cut").unwrap();

    let mesh = m.tessellate("cut").unwrap();
    let vol = mesh_volume(&mesh);
    // 1000 - 4×4×5 = 1000 - 80 = 920
    let expected = 920.0;
    let tol = expected * 0.10;
    assert!(
        (vol - expected).abs() < tol,
        "Cut volume should be ~{:.0} (got {:.1})",
        expected,
        vol
    );
}

/// S3: Scale-expand strategy for complex shell — multiple bosses creating
/// >30 faces, then a cut that triggers scale-expand-first perturbation.
///
/// The cascade logic uses scale-expand FIRST for complex shells (>30 faces)
/// with corner-coplanar geometry. This test creates a sufficiently complex
/// shell to trigger that path.
#[test]
#[ignore = "Non-deterministic (~50% pass rate). Sprint 42 cascade gate relaxation helps but truck pointer-derived ordering causes variable IC quality. When it fails: 47 attempts exhausted, 36 open edges."]
fn s3_scale_expand_complex_shell() {
    let mut m = base_cube();

    // Add 3 circle bosses on top face to increase face count
    m.circle_sketch("boss1_sk", [0., 0., 10.], [0., 0., 1.], 2., 2., 1.5)
        .unwrap();
    m.extrude_no_merge("boss1", "boss1_sk", 3.0).unwrap();
    m.boolean_union("m1", "cube", "boss1").unwrap();
    m.assert_has_solid("m1").unwrap();

    m.circle_sketch("boss2_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 1.5)
        .unwrap();
    m.extrude_no_merge("boss2", "boss2_sk", 3.0).unwrap();
    m.boolean_union("m2", "m1", "boss2").unwrap();
    m.assert_has_solid("m2").unwrap();

    m.circle_sketch("boss3_sk", [0., 0., 10.], [0., 0., 1.], 8., 8., 1.5)
        .unwrap();
    m.extrude_no_merge("boss3", "boss3_sk", 3.0).unwrap();
    m.boolean_union("m3", "m2", "boss3").unwrap();
    m.assert_has_solid("m3").unwrap();

    // Now cut into the complex shell — this creates a shell with many faces
    // where scale-expand should be tried first
    m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 1., 1., 8., 8.)
        .unwrap();
    m.extrude_cut("final_cut", "cut_sk", 5.0).unwrap();
    m.assert_has_solid("final_cut").unwrap();

    let mesh = m.tessellate("final_cut").unwrap();
    let vol = mesh_volume(&mesh);
    // Volume should be less than original cube (1000) minus the cut
    assert!(
        vol < 1000.0,
        "Cut should reduce volume below 1000 (got {:.1})",
        vol
    );
}

/// S4: Corner-coplanar geometry — two boxes sharing edges at 90° angle.
///
/// Creates L-shaped geometry where two coplanar face normals exist at 90°.
/// The detect_corner_coplanar function should find the cross-product
/// direction and use corner-coplanar perturbation strategy.
#[test]
fn s4_corner_coplanar_l_shaped() {
    let mut m = ModelBuilder::truck();

    // Base: 10×10×5 box (lower half)
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("base", "base_sk", 5.0).unwrap();
    m.assert_has_solid("base").unwrap();

    // Right wing: 5×10×5 box sharing face at x=10
    m.rect_sketch("wing_sk", [0., 0., 0.], [0., 0., 1.], 10., 0., 5., 10.)
        .unwrap();
    m.extrude_no_merge("wing", "wing_sk", 5.0).unwrap();
    m.assert_has_solid("wing").unwrap();

    m.boolean_union("l_shape", "base", "wing").unwrap();
    m.assert_has_solid("l_shape").unwrap();

    let mesh = m.tessellate("l_shape").unwrap();
    let vol = mesh_volume(&mesh);
    // 10×10×5 + 5×10×5 = 500 + 250 = 750
    let expected = 750.0;
    let tol = expected * 0.15;
    assert!(
        (vol - expected).abs() < tol,
        "L-shaped union volume should be ~{:.0} (got {:.1})",
        expected,
        vol
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Category T — Cascade Behavior Tests
// ══════════════════════════════════════════════════════════════════════════════

/// T1: Direct strategy success — simple overlapping boxes where the first
/// attempt (direct, no perturbation) should succeed.
///
/// Two boxes overlapping by 3 units in X with no shared faces. The
/// boolean should succeed on attempt #1 ("direct") without any
/// perturbation cascade.
#[test]
fn t1_cascade_direct_strategy_succeeds() {
    let mut m = ModelBuilder::truck();

    // Box A: x∈[0,8], y∈[0,6], z∈[0,6]
    m.rect_sketch("a_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 8., 6.)
        .unwrap();
    m.extrude_no_merge("box_a", "a_sk", 6.0).unwrap();
    m.assert_has_solid("box_a").unwrap();

    // Box B: x∈[5,13], y∈[1,5], z∈[1,5] — overlaps A, no shared faces
    m.rect_sketch("b_sk", [0., 0., 0.], [0., 0., 1.], 5., 1., 8., 4.)
        .unwrap();
    m.extrude_no_merge("box_b", "b_sk", 4.0).unwrap();
    m.assert_has_solid("box_b").unwrap();

    m.boolean_union("merged", "box_a", "box_b").unwrap();
    m.assert_has_solid("merged").unwrap();

    let mesh = m.tessellate("merged").unwrap();
    let vol = mesh_volume(&mesh);
    // 8×6×6 = 288, 8×4×4 = 128, overlap = 3×4×4 = 48
    // union = 288 + 128 - 48 = 368
    let expected = 368.0;
    let tol = expected * 0.15;
    assert!(
        (vol - expected).abs() < tol,
        "Direct union volume should be ~{:.0} (got {:.1})",
        expected,
        vol
    );
}

/// T2: Cascade with coplanar detection — boss on face exercises
/// coplanar direction detection in the cascade.
///
/// Creates a cube + boss configuration where the shared face at z=10
/// triggers coplanar detection. The cascade should detect the coplanar
/// direction and use it for perturbation if the direct attempt fails.
#[test]
fn t2_cascade_coplanar_detection() {
    let mut m = base_cube();

    // Circle boss on z=10 top face — coplanar face triggers detection
    m.circle_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 3.)
        .unwrap();
    m.extrude_no_merge("boss", "boss_sk", 5.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    m.boolean_union("merged", "cube", "boss").unwrap();
    m.assert_has_solid("merged").unwrap();

    let mesh = m.tessellate("merged").unwrap();
    let vol = mesh_volume(&mesh);
    assert!(
        vol > 1000.0,
        "Boss union should exceed cube volume 1000 (got {:.1})",
        vol
    );
}

/// T3: Chained booleans exercise healing pre-heal vertex unification.
///
/// After a boolean, result edges carry IntersectionCurve geometry. The
/// next boolean triggers pre-heal vertex unification in the cascade
/// (healing.rs lines 1236-1305). Two sequential unions verify this path.
#[test]
fn t3_chained_booleans_preheal() {
    let mut m = base_cube();

    // First boss on z=10 top face
    m.rect_sketch("boss1_sk", [0., 0., 10.], [0., 0., 1.], 1., 1., 3., 3.)
        .unwrap();
    m.extrude_no_merge("boss1", "boss1_sk", 4.0).unwrap();
    m.assert_has_solid("boss1").unwrap();
    m.boolean_union("m1", "cube", "boss1").unwrap();
    m.assert_has_solid("m1").unwrap();

    // Second boss on z=10 top face, different position
    m.rect_sketch("boss2_sk", [0., 0., 10.], [0., 0., 1.], 6., 6., 3., 3.)
        .unwrap();
    m.extrude_no_merge("boss2", "boss2_sk", 4.0).unwrap();
    m.assert_has_solid("boss2").unwrap();
    m.boolean_union("m2", "m1", "boss2").unwrap();
    m.assert_has_solid("m2").unwrap();

    let mesh = m.tessellate("m2").unwrap();
    let vol = mesh_volume(&mesh);
    // 1000 + 3×3×4 + 3×3×4 = 1000 + 36 + 36 = 1072
    let expected = 1072.0;
    let tol = expected * 0.15;
    assert!(
        (vol - expected).abs() < tol,
        "Chained union volume should be ~{:.0} (got {:.1})",
        expected,
        vol
    );
}

/// T4: Cut then boss exercises cascade with healed IntersectionCurve edges.
///
/// After a cut, the result has IntersectionCurve edges that are healed
/// (replaced with BSpline/Line). A subsequent boss union must handle
/// these healed edges correctly in the perturbation cascade.
#[test]
fn t4_cut_then_boss_healing_cascade() {
    let mut m = base_cube();

    // Cut on top face
    m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude_cut("cut", "cut_sk", 5.0).unwrap();
    m.assert_has_solid("cut").unwrap();

    // Boss on side face (y=10)
    m.rect_sketch("boss_sk", [0., 10., 0.], [0., 1., 0.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude_no_merge("boss", "boss_sk", 4.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    m.boolean_union("merged", "cut", "boss").unwrap();
    m.assert_has_solid("merged").unwrap();

    let mesh = m.tessellate("merged").unwrap();
    let vol = mesh_volume(&mesh);
    // (1000 - 4×4×5) + 6×6×4 = 920 + 144 = 1064
    assert!(
        vol > 900.0,
        "Cut+boss volume should exceed 900 (got {:.1})",
        vol
    );
}

/// T5: Subtract produces single body — verify body_count == 1.
///
/// A simple subtraction should produce exactly one body (not split into
/// multiple disconnected components). This exercises the finalize path
/// where Solid::try_new must succeed with a single boundary shell.
///
/// KNOWN LIMITATION: Explicit boolean_subtract at cube corner produces
/// 2 visible bodies instead of 1. The subtraction succeeds geometrically
/// but the feature engine exposes both input and result bodies. Related
/// to the q5 multi-body diagnosis (Sprint 38 Phase 0).
#[test]
fn t5_subtract_single_body() {
    let mut m = base_cube();

    // Small box to subtract from corner
    m.rect_sketch("sub_sk", [0., 0., 10.], [0., 0., 1.], 0., 0., 3., 3.)
        .unwrap();
    m.extrude_no_merge("sub_box", "sub_sk", 3.0).unwrap();
    m.assert_has_solid("sub_box").unwrap();

    m.boolean_subtract("result", "cube", "sub_box").unwrap();
    m.assert_has_solid("result").unwrap();

    // Verify single body output
    assert_eq!(
        count_visible_bodies(&m),
        1,
        "Subtraction should produce exactly 1 visible body"
    );

    let mesh = m.tessellate("result").unwrap();
    let vol = mesh_volume(&mesh);
    // 1000 - 3×3×3 = 1000 - 27 = 973
    let expected = 973.0;
    let tol = expected * 0.10;
    assert!(
        (vol - expected).abs() < tol,
        "Subtract volume should be ~{:.0} (got {:.1})",
        expected,
        vol
    );
}

/// T6: Intersect operation exercises finalize_boolean_shell AND path.
///
/// Boolean AND (intersection) uses a different codepath in integrate/mod.rs
/// (and_result_with_tol → finalize_boolean_shell). Test with overlapping
/// boxes to verify the AND recovery path works.
#[test]
fn t6_intersect_recovery_path() {
    let mut m = ModelBuilder::truck();

    // Box A: x∈[0,8], y∈[0,8], z∈[0,8]
    m.rect_sketch("a_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 8., 8.)
        .unwrap();
    m.extrude_no_merge("box_a", "a_sk", 8.0).unwrap();

    // Box B: x∈[3,11], y∈[3,11], z∈[3,11]
    m.rect_sketch("b_sk", [0., 0., 3.], [0., 0., 1.], 3., 3., 8., 8.)
        .unwrap();
    m.extrude_no_merge("box_b", "b_sk", 8.0).unwrap();

    m.boolean_intersect("result", "box_a", "box_b").unwrap();
    m.assert_has_solid("result").unwrap();

    let mesh = m.tessellate("result").unwrap();
    let vol = mesh_volume(&mesh);
    // Intersection: x∈[3,8], y∈[3,8], z∈[3,8] → 5×5×5 = 125
    let expected = 125.0;
    let tol = expected * 0.20;
    assert!(
        (vol - expected).abs() < tol,
        "Intersect volume should be ~{:.0} (got {:.1})",
        expected,
        vol
    );
}

/// T7: Sequential cuts exercise cumulative recovery.
///
/// Multiple cuts on the same body exercise the cascade's pre-heal
/// (vertex unification) after each boolean, since each cut introduces
/// IntersectionCurve edges that must be healed for subsequent operations.
#[test]
fn t7_sequential_cuts_cumulative_recovery() {
    let mut m = base_cube();

    // Cut 1: 3×3 at (0.5, 0.5), depth 5
    m.rect_sketch("c1_sk", [0., 0., 10.], [0., 0., 1.], 0.5, 0.5, 3., 3.)
        .unwrap();
    m.extrude_cut("c1", "c1_sk", 5.0).unwrap();
    m.assert_has_solid("c1").unwrap();

    // Cut 2: 3×3 at (6, 6), depth 5
    m.rect_sketch("c2_sk", [0., 0., 10.], [0., 0., 1.], 6., 6., 3., 3.)
        .unwrap();
    m.extrude_cut("c2", "c2_sk", 5.0).unwrap();
    m.assert_has_solid("c2").unwrap();

    // Cut 3: 3×3 at (0.5, 6), depth 3
    m.rect_sketch("c3_sk", [0., 0., 10.], [0., 0., 1.], 0.5, 6., 3., 3.)
        .unwrap();
    m.extrude_cut("c3", "c3_sk", 3.0).unwrap();
    m.assert_has_solid("c3").unwrap();

    let mesh = m.tessellate("c3").unwrap();
    let vol = mesh_volume(&mesh);
    // 1000 - 3×3×5 - 3×3×5 - 3×3×3 = 1000 - 45 - 45 - 27 = 883
    let expected = 883.0;
    let tol = expected * 0.10;
    assert!(
        (vol - expected).abs() < tol,
        "Triple cut volume should be ~{:.0} (got {:.1})",
        expected,
        vol
    );
}

/// T8: Extrude-cut (auto-merge) exercises the full cascade path through
/// the feature engine, including healing.rs pre-heal and
/// try_boolean_with_perturbation.
///
/// Unlike explicit boolean_subtract, extrude_cut goes through the feature
/// engine's auto-merge logic which invokes the cascade differently.
#[test]
fn t8_extrude_cut_auto_merge_cascade() {
    let mut m = base_cube();

    // Circle cut on top face — exercises plane-cylinder IC healing
    m.circle_sketch("ccut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.)
        .unwrap();
    m.extrude_cut("ccut", "ccut_sk", 7.0).unwrap();
    m.assert_has_solid("ccut").unwrap();

    let mesh = m.tessellate("ccut").unwrap();
    let vol = mesh_volume(&mesh);
    // cylinder_vol ≈ π×2²×7 ≈ 87.96, but 16-segment polygon approx
    let n = 16.0_f64;
    let approx_cyl = 2.0 * 2.0 * n * (2.0 * std::f64::consts::PI / n).sin() / 2.0 * 7.0;
    let expected = 1000.0 - approx_cyl;
    let tol = expected * 0.10;
    assert!(
        (vol - expected).abs() < tol,
        "Circle cut volume should be ~{:.0} (got {:.1})",
        expected,
        vol
    );
}
