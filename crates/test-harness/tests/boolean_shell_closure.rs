//! Tests for boolean shell closure: verifying that the targeted re-weld
//! and deterministic BTreeMap iteration correctly close shells after
//! complex multi-boolean operations.

use test_harness::helpers::mesh_volume;
use test_harness::ModelBuilder;

/// Boss→cut pattern in isolation: verifies the simplest failure case (K1 pattern).
/// A boss auto-unions onto a cube, then a cut is applied to the merged body.
#[test]
fn shell_closure_boss_then_cut() {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();
    let v0 = mesh_volume(&m.tessellate("cube").unwrap());

    // Boss on top face
    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();
    m.assert_has_solid("boss").unwrap();
    let v1 = mesh_volume(&m.tessellate("boss").unwrap());
    assert!(v1 > v0, "Boss should increase volume");

    // Cut on the merged body
    m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 0.5, 0.5, 3., 3.)
        .unwrap();
    m.extrude_cut("cut", "cut_sk", 5.0).unwrap();
    m.assert_has_solid("cut").unwrap();
    let v2 = mesh_volume(&m.tessellate("cut").unwrap());
    assert!(
        v2 < v1,
        "Cut on boss body should reduce volume (boss={:.0}, cut={:.0})",
        v1,
        v2
    );
}

/// Two bosses then a cut: verifies more complex multi-union topology.
#[test]
fn shell_closure_two_bosses_then_cut() {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();

    // Two bosses
    m.rect_sketch("b1_sk", [0., 0., 10.], [0., 0., 1.], 0.5, 0.5, 3., 3.)
        .unwrap();
    m.extrude("b1", "b1_sk", 3.0).unwrap();
    m.assert_has_solid("b1").unwrap();

    m.rect_sketch("b2_sk", [0., 0., 10.], [0., 0., 1.], 6., 6., 3., 3.)
        .unwrap();
    m.extrude("b2", "b2_sk", 4.0).unwrap();
    m.assert_has_solid("b2").unwrap();
    let v_bosses = mesh_volume(&m.tessellate("b2").unwrap());

    // Cut
    m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 4., 0.5, 3., 3.)
        .unwrap();
    m.extrude_cut("cut", "cut_sk", 5.0).unwrap();
    m.assert_has_solid("cut").unwrap();
    let v_cut = mesh_volume(&m.tessellate("cut").unwrap());
    assert!(
        v_cut < v_bosses,
        "Cut after two bosses should reduce volume"
    );
}

/// Boss→cut→boss (full K1 pattern) run 3 times for repeatability.
#[test]
fn shell_closure_boss_cut_boss_repeatable() {
    for run in 0..3 {
        let mut m = ModelBuilder::kernel_v2();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();
        m.assert_has_solid("cube").unwrap();

        m.rect_sketch("boss1_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 3., 3.)
            .unwrap();
        m.extrude("boss1", "boss1_sk", 4.0).unwrap();
        assert!(
            m.assert_has_solid("boss1").is_ok(),
            "Run {run}: boss1 should succeed"
        );

        m.rect_sketch("cut1_sk", [0., 0., 10.], [0., 0., 1.], 0.5, 0.5, 3., 3.)
            .unwrap();
        m.extrude_cut("cut1", "cut1_sk", 5.0).unwrap();
        assert!(
            m.assert_has_solid("cut1").is_ok(),
            "Run {run}: cut1 should succeed"
        );

        m.rect_sketch("boss2_sk", [0., 0., 10.], [0., 0., 1.], 6., 6., 3., 3.)
            .unwrap();
        m.extrude("boss2", "boss2_sk", 3.0).unwrap();
        assert!(
            m.assert_has_solid("boss2").is_ok(),
            "Run {run}: boss2 should succeed"
        );
    }
}

/// Overlapping cuts (M6 pattern) — verifies coplanar pocket handling.
/// Sprint 33: topology-driven edge canonicalization (multi-point curve
/// matching) improved truck-level tests, but harness-level test still fails
/// due to perturbation exhaustion on 3rd overlapping cut (14-face shell).
#[test]
#[ignore = "kernel-v2: overlapping cuts hit the coplanar wall mid-chain, NotSupported until Yang Stage 0 (roadmap M8)"]
fn shell_closure_overlapping_cuts() {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();
    let v0 = mesh_volume(&m.tessellate("cube").unwrap());

    // Three overlapping 3x3 cuts
    m.rect_sketch("c1_sk", [0., 0., 10.], [0., 0., 1.], 1., 1., 3., 3.)
        .unwrap();
    m.extrude_cut("c1", "c1_sk", 4.0).unwrap();
    m.assert_has_solid("c1").unwrap();
    let v1 = mesh_volume(&m.tessellate("c1").unwrap());
    assert!(v1 < v0);

    m.rect_sketch("c2_sk", [0., 0., 10.], [0., 0., 1.], 3., 1., 3., 3.)
        .unwrap();
    m.extrude_cut("c2", "c2_sk", 4.0).unwrap();
    m.assert_has_solid("c2").unwrap();
    let v2 = mesh_volume(&m.tessellate("c2").unwrap());
    assert!(v2 < v1);

    m.rect_sketch("c3_sk", [0., 0., 10.], [0., 0., 1.], 2., 3., 3., 3.)
        .unwrap();
    m.extrude_cut("c3", "c3_sk", 4.0).unwrap();
    m.assert_has_solid("c3").unwrap();
    let v3 = mesh_volume(&m.tessellate("c3").unwrap());
    assert!(v3 < v2, "Third overlapping cut should reduce volume");
}
