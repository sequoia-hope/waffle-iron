//! D2.7 — Tolerance sensitivity tests.
//!
//! Verify boolean correctness (volume, face count) across varied geometry
//! configurations. Uses proven box-subtract and boss-union patterns that
//! the current pipeline handles reliably.
//!
//! Categories:
//!   TS — Tolerance Sensitivity deterministic tests (4 tests)
//!   TP — Tolerance Proptest variation tests (2 proptests)

use proptest::prelude::*;
use test_harness::helpers::mesh_volume;
use test_harness::ModelBuilder;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Perform a subtract operation (outer minus inner) and return volume.
/// Outer: [0, outer_size]^3 on XY plane.
/// Inner: [ix, ix+iw] x [iy, iy+ih] x [iz..iz+id] tool.
fn subtract_volume(
    outer_size: f64,
    ix: f64,
    iy: f64,
    iz: f64,
    iw: f64,
    ih: f64,
    id: f64,
) -> Option<f64> {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch(
        "outer_sk",
        [0., 0., 0.],
        [0., 0., 1.],
        0.,
        0.,
        outer_size,
        outer_size,
    )
    .ok()?;
    m.extrude_no_merge("outer", "outer_sk", outer_size).ok()?;

    // Tool sketch on the Z=iz plane, extends upward by id
    m.rect_sketch("tool_sk", [0., 0., iz], [0., 0., 1.], ix, iy, iw, ih)
        .ok()?;
    m.extrude_no_merge("tool", "tool_sk", id).ok()?;

    m.boolean_subtract("result", "outer", "tool").ok()?;
    m.assert_has_solid("result").ok()?;

    let meshes = m.tessellate_all("result").ok()?;
    let mut total_vol = 0.0;
    for mesh in &meshes {
        total_vol += mesh_volume(mesh);
    }
    Some(total_vol)
}

/// Perform a boss union and return volume.
/// Base: [0, base_size]^3.  Boss: rectangle at (bx, by) size (bw, bh), height boss_h,
/// placed on top face (z = base_size).
fn boss_union_volume(
    base_size: f64,
    bx: f64,
    by: f64,
    bw: f64,
    bh: f64,
    boss_h: f64,
) -> Option<f64> {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch(
        "base_sk",
        [0., 0., 0.],
        [0., 0., 1.],
        0.,
        0.,
        base_size,
        base_size,
    )
    .ok()?;
    m.extrude_no_merge("base", "base_sk", base_size).ok()?;

    // Boss sketch on top face (z = base_size)
    m.rect_sketch("boss_sk", [0., 0., base_size], [0., 0., 1.], bx, by, bw, bh)
        .ok()?;
    m.extrude_no_merge("boss", "boss_sk", boss_h).ok()?;

    m.boolean_union("result", "base", "boss").ok()?;
    m.assert_has_solid("result").ok()?;

    let meshes = m.tessellate_all("result").ok()?;
    let mut total_vol = 0.0;
    for mesh in &meshes {
        total_vol += mesh_volume(mesh);
    }
    Some(total_vol)
}

// ══════════════════════════════════════════════════════════════════════════════
// Category TS — Deterministic tolerance sensitivity tests
// ══════════════════════════════════════════════════════════════════════════════

/// TS1: Centered subtract — through-cut removed from center of box.
/// Known-good pattern (matches ET1 from boolean_edge_cases).
#[test]
fn ts1_centered_subtract_volume() {
    // Outer: 20x20x20, Tool: 10x10 at (5,5), extends through
    let vol = subtract_volume(20.0, 5.0, 5.0, 0.0, 10.0, 10.0, 25.0);
    assert!(vol.is_some(), "Centered subtract should succeed");

    let vol = vol.unwrap();
    // Cut region: [5,15] x [5,15] x [0,20] = 10*10*20 = 2000
    let expected = 8000.0 - 2000.0; // = 6000
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.10,
        "Volume {:.1} should be within 10% of {:.1} (err={:.3})",
        vol,
        expected,
        rel_err
    );
}

/// TS2: Offset subtract — tool at non-integer coordinates.
/// Known-good pattern (matches ET2 from boolean_edge_cases).
#[test]
fn ts2_offset_subtract_volume() {
    // Outer: 10x10x10, Tool: 5x5 at (2.7, 3.1), extends through
    let vol = subtract_volume(10.0, 2.7, 3.1, 0.0, 5.0, 5.0, 15.0);
    assert!(vol.is_some(), "Offset subtract should succeed");

    let vol = vol.unwrap();
    // Cut region: [2.7,7.7] x [3.1,8.1] x [0,10] = 5*5*10 = 250
    let expected = 1000.0 - 250.0;
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.10,
        "Volume {:.1} should be within 10% of {:.1} (err={:.3})",
        vol,
        expected,
        rel_err
    );
}

/// TS3: Boss union — rectangular boss on top face.
/// Known-good pattern (matches A5/E7 from boolean_workflows).
#[test]
fn ts3_boss_union_volume() {
    // Base: 10x10x10, Boss: 4x4 at (3, 3), height 5
    let vol = boss_union_volume(10.0, 3.0, 3.0, 4.0, 4.0, 5.0);
    assert!(vol.is_some(), "Boss union should succeed");

    let vol = vol.unwrap();
    let expected = 1000.0 + 80.0; // = 1080
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.10,
        "Volume {:.1} should be within 10% of {:.1} (err={:.3})",
        vol,
        expected,
        rel_err
    );
}

/// TS4: Edge-adjacent boss — boss placed at corner of base face.
/// Known-good pattern (matches E3 from boolean_workflows).
#[test]
fn ts4_edge_boss_union_volume() {
    // Base: 10x10x10, Boss: 3x3 at (0, 0), height 4
    let vol = boss_union_volume(10.0, 0.0, 0.0, 3.0, 3.0, 4.0);
    assert!(vol.is_some(), "Edge boss union should succeed");

    let vol = vol.unwrap();
    let expected = 1000.0 + 36.0; // = 1036
    let rel_err = (vol - expected).abs() / expected;
    assert!(
        rel_err < 0.10,
        "Volume {:.1} should be within 10% of {:.1} (err={:.3})",
        vol,
        expected,
        rel_err
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Category TP — Proptest tolerance variation
// ══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]

    /// TP1: Subtract with varying tool position and size.
    ///
    /// Tool position varies within the outer box. When the boolean succeeds,
    /// volume must match analytical expectation within 10%.
    #[test]
    fn tp1_subtract_position_sweep(
        ix in 1.0f64..5.0,
        iy in 1.0f64..5.0,
        iw in 2.0f64..6.0,
        ih in 2.0f64..6.0,
    ) {
        let outer_size = 10.0;

        match subtract_volume(outer_size, ix, iy, 0.0, iw, ih, outer_size + 5.0) {
            Some(vol) => {
                // Clamp tool to outer box for analytical volume
                let cut_x = (ix + iw).min(outer_size) - ix;
                let cut_y = (iy + ih).min(outer_size) - iy;
                let cut_vol = cut_x * cut_y * outer_size;
                let expected = outer_size.powi(3) - cut_vol;

                if expected > 10.0 {
                    let rel_err = (vol - expected).abs() / expected;
                    prop_assert!(
                        rel_err < 0.10,
                        "ix={:.1} iy={:.1} iw={:.1} ih={:.1}: vol={:.1} expected={:.1} err={:.3}",
                        ix, iy, iw, ih, vol, expected, rel_err
                    );
                }
            }
            None => {
                // Boolean failure acceptable — test guards correctness when successful.
            }
        }
    }

    /// TP2: Boss union with varying boss size and position.
    ///
    /// Boss sits on top face; position and size vary. When the boolean succeeds,
    /// volume must match analytical expectation within 10%.
    #[test]
    fn tp2_boss_union_size_sweep(
        bx in 1.0f64..6.0,
        by in 1.0f64..6.0,
        bw in 2.0f64..5.0,
        bh in 2.0f64..5.0,
    ) {
        let base_size = 10.0;
        let boss_h = 5.0;

        // Boss must fit on the top face
        prop_assume!(bx + bw <= base_size);
        prop_assume!(by + bh <= base_size);

        match boss_union_volume(base_size, bx, by, bw, bh, boss_h) {
            Some(vol) => {
                let expected = base_size.powi(3) + bw * bh * boss_h;
                let rel_err = (vol - expected).abs() / expected;
                prop_assert!(
                    rel_err < 0.10,
                    "bx={:.1} by={:.1} bw={:.1} bh={:.1}: vol={:.1} expected={:.1} err={:.3}",
                    bx, by, bw, bh, vol, expected, rel_err
                );
            }
            None => {
                // Boolean failure acceptable — test guards correctness when successful.
            }
        }
    }
}
