//! `DepthMode::ThroughAll` on real geometry (kernel-v2), at three authoring
//! scales — the pins for assay **P0012**.
//!
//! A through-all extrude is measured along the direction it ACTUALLY sweeps,
//! and its overshoot past the material is a FRACTION of that material's own
//! extent. Before the P0012 fix the depth was measured along the unreversed
//! sketch normal and padded by an absolute 1 m, so:
//!
//! - a cut whose body lay entirely behind its sketch plane got a 1 m cutter
//!   that never reached the body and silently removed NOTHING (P0012: the
//!   kernel read 72016 = 72000 + 16 exactly), and
//! - every through-all extrude carried a 1 m overshoot — invisible at ×1e-3,
//!   a metre of spurious geometry at ×1e3.
//!
//! Each test therefore runs at ×1e-3, ×1 and ×1e3: the measured quantity
//! divided by the scale must be the SAME number at every scale. That is the
//! property an absolute margin cannot have, and it is how P0012 hid — it read
//! CORRECT at ×1e-3 because 1 m dwarfed the whole part.

use test_harness::helpers::{mesh_bounding_box, mesh_volume};
use test_harness::ModelBuilder;

const SCALES: [f64; 3] = [1e-3, 1.0, 1e3];

/// A `10 s` cube at the origin, spanning `z ∈ [0, 10 s]`.
fn cube(s: f64) -> ModelBuilder {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch(
        "base_sk",
        [0., 0., 0.],
        [0., 0., 1.],
        0.,
        0.,
        10. * s,
        10. * s,
    )
    .unwrap();
    m.extrude("cube", "base_sk", 10. * s).unwrap();
    m.assert_has_solid("cube").unwrap();
    m
}

/// P0012's shape: the cut's sketch plane sits BEYOND the body with its normal
/// pointing away, so the sweep reverses — and the depth must be measured along
/// that reversed sweep, or the cutter misses the body entirely.
#[test]
fn through_all_cut_reaches_a_body_entirely_behind_its_sketch_plane() {
    for s in SCALES {
        let mut m = cube(s);
        // Plane at z = 20 s, normal +z: the whole cube is behind it.
        m.rect_sketch(
            "win_sk",
            [0., 0., 20. * s],
            [0., 0., 1.],
            2. * s,
            2. * s,
            6. * s,
            6. * s,
        )
        .unwrap();
        m.extrude_through_all("win", "win_sk", true).unwrap();
        m.assert_has_solid("win").unwrap();
        m.assert_no_errors().unwrap();

        let vol = mesh_volume(&m.tessellate("win").unwrap());
        // The 6 s × 6 s window is swept through the cube's full 10 s height:
        // 1000 s³ − 360 s³.
        let expect = 640. * s * s * s;
        assert!(
            (vol - expect).abs() <= 1e-5 * expect,
            "scale {s}: through-all cut left {vol:.6e}, expected {expect:.6e} \
             (an unreached body would leave the full 1000 s³ = {:.6e})",
            1000. * s * s * s
        );
    }
}

/// The everyday case — a hole cut from the body's own top face — keeps
/// working, at every scale.
#[test]
fn through_all_cut_from_the_top_face_pierces_the_body() {
    for s in SCALES {
        let mut m = cube(s);
        m.rect_sketch(
            "hole_sk",
            [0., 0., 10. * s],
            [0., 0., 1.],
            2. * s,
            2. * s,
            6. * s,
            6. * s,
        )
        .unwrap();
        m.extrude_through_all("hole", "hole_sk", true).unwrap();
        m.assert_has_solid("hole").unwrap();
        m.assert_no_errors().unwrap();

        let vol = mesh_volume(&m.tessellate("hole").unwrap());
        let expect = 640. * s * s * s;
        assert!(
            (vol - expect).abs() <= 1e-5 * expect,
            "scale {s}: top-face through-all cut left {vol:.6e}, expected {expect:.6e}"
        );
    }
}

/// The overshoot past the material is RELATIVE, so a through-all BOSS ends at
/// the same multiple of the model's size at every scale. The absolute 1 m
/// margin put the end face at 10 s + 1 m — off by 1000× between scales.
#[test]
fn through_all_boss_overshoots_by_a_relative_margin() {
    for s in SCALES {
        let mut m = cube(s);
        // A column sketched BELOW the cube, sweeping up through it: the
        // material lies at [5 s, 15 s] along the sweep, so the depth is
        // 15 s + 1e-2 · max(span, far) = 15.15 s and the column ends at
        // z = 10.15 s.
        m.rect_sketch(
            "col_sk",
            [0., 0., -5. * s],
            [0., 0., 1.],
            2. * s,
            2. * s,
            6. * s,
            6. * s,
        )
        .unwrap();
        m.extrude_through_all("col", "col_sk", false).unwrap();
        m.assert_has_solid("col").unwrap();
        m.assert_no_errors().unwrap();

        let mesh = m.tessellate("col").unwrap();
        let (lo, hi) = mesh_bounding_box(&mesh);
        // The mesh carries f32 positions, so judge at 1e-5 relative — three
        // orders tighter than the quantity under test (the absolute margin
        // put this end face at 10 s + 1 m: 1.000e3 s at ×1e-3).
        let (lo_z, hi_z) = (f64::from(lo[2]) / s, f64::from(hi[2]) / s);
        assert!(
            (lo_z + 5.0).abs() < 1e-5,
            "scale {s}: column starts at {lo_z} s (expected −5 s)"
        );
        assert!(
            (hi_z - 10.15).abs() < 1e-5,
            "scale {s}: column ends at {hi_z} s, expected the scale-invariant 10.15 s"
        );
        // The column unions into the cube, so the body is 1000 s³ (cube)
        // + 180 s³ (the 6 s × 6 s column below it) + 5.4 s³ (36 s² × the
        // 0.15 s overshoot). That last term is the whole point: an absolute
        // 1 m margin would make it 36 s² × 1 m instead — 36 000 s³ at ×1e-3.
        let vol = mesh_volume(&mesh);
        let expect = (1000. + 180. + 5.4) * s * s * s;
        assert!(
            (vol - expect).abs() <= 1e-5 * expect,
            "scale {s}: column volume {vol:.6e}, expected {expect:.6e}"
        );
    }
}
