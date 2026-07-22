#[allow(unused_imports)]
use super::*;

// ── Rim×plane graze arm (spec `yang_195_seal_neighborhood_self_overlap`
//    §5, #195 inc-2) ─────────────────────────────────────────────────

fn sag(r: f64, n: usize) -> f64 {
    r * (1.0 - (std::f64::consts::PI / n as f64).cos())
}

/// The F0082 analog: the tube cap rim (r = 0.2123, rim plane nearly
/// parallel to the wall's normal ⇒ k ≈ 1) crossing the wall plane at the
/// measured extent 1.25e-3. The demand is the MINIMAL N clearing the
/// factor-2 sagitta margin (measured Phase-0: floor 32 = sag < depth but
/// no margin → silent WRONG χ=1; the derived N = 41 → CORRECT).
#[test]
pub(crate) fn rim_plane_f0082_analog_derives_minimal_n() {
    let r = 0.2123;
    let depth = 1.25e-3;
    // Rim in the yz-plane-ish (normal = z), wall plane normal = x with the
    // rim center offset so the shallow-side extent is exactly `depth`:
    // s = m̂·c + d̂ = r·k − depth, k = 1 (n ⊥ m̂).
    let rim = ([r - depth, 0.0, 0.0], [0.0, 0.0, 1.0], r);
    let n = rim_plane_graze_n(rim, ([1.0, 0.0, 0.0], 0.0)).expect("expected a demand");
    assert!(
        sag(r, n) <= depth / 2.0,
        "derived N={n} must clear the depth with the factor-2 margin"
    );
    assert!(
        sag(r, n - 1) > depth / 2.0,
        "derived N={n} must be MINIMAL (no over-refinement)"
    );
    assert_eq!(n, 41, "the F0082 pair derives the measured-green N");
}

/// Deep crossings derive a tiny N absorbed by the natural-N gate at the
/// scan level; no crossing (plane clear of the circle) and the rim lying
/// IN the partner plane (k → 0, the M8/Stage-0 coplanar remit) are
/// silent.
#[test]
pub(crate) fn rim_plane_deep_disjoint_inplane() {
    let r = 0.5;
    // Deep: extent 0.3 ⇒ minimal N with sag ≤ 0.15.
    let n = rim_plane_graze_n(
        ([0.2, 0.0, 0.0], [0.0, 0.0, 1.0], r),
        ([1.0, 0.0, 0.0], 0.0),
    )
    .expect("deep crossing still yields a (tiny) demand");
    assert!(n <= 6, "deep crossing derives a tiny N, got {n}");
    // No crossing: plane 0.1 beyond the rim's reach.
    assert_eq!(
        rim_plane_graze_n(
            ([0.6, 0.0, 0.0], [0.0, 0.0, 1.0], r),
            ([1.0, 0.0, 0.0], 0.0)
        ),
        None
    );
    // Rim lying in the partner plane: k = 0, depth < 0 — the coplanar
    // machinery's remit, never a boost.
    assert_eq!(
        rim_plane_graze_n(
            ([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], r),
            ([0.0, 0.0, 1.0], 0.0)
        ),
        None
    );
}

/// Scope lines (spec §5c): authored-coincidence residue at or below the
/// #178-calibrated noise line and sub-render lenses at or below
/// `2·10⁻³·r` demand nothing; just above the render line the demand
/// appears, bounded ≈ 71.
#[test]
pub(crate) fn rim_plane_noise_and_render_lines() {
    let r = 0.5;
    let place = |depth: f64| ([r - depth, 0.0, 0.0], [0.0, 0.0, 1.0], r);
    // Authored flush contact: depth 1e-12 ≪ noise.
    assert_eq!(
        rim_plane_graze_n(place(1.0e-12), ([1.0, 0.0, 0.0], 0.0)),
        None
    );
    // Sub-render lens: depth just below 2e-3·r = 1e-3.
    assert_eq!(
        rim_plane_graze_n(place(0.9e-3), ([1.0, 0.0, 0.0], 0.0)),
        None
    );
    // Just above the render line: bounded demand.
    let n = rim_plane_graze_n(place(1.1e-3), ([1.0, 0.0, 0.0], 0.0)).expect("above the line");
    assert!(n <= 75, "render line bounds the derived N ≈ 71, got {n}");
}

/// A tilted rim (k < 1) reduces the crossing extent through the same
/// formula: the depth is measured on the circle's signed-distance span,
/// not the raw center offset.
#[test]
pub(crate) fn rim_plane_tilted_k_scales_reach() {
    let r = 0.5;
    // Rim normal tilted 60° from the plane normal ⇒ k = sin(60°) ≈ 0.866;
    // reach = r·k ≈ 0.433. Center offset 0.44 ⇒ no crossing.
    let tilt = [(60f64).to_radians().cos(), 0.0, (60f64).to_radians().sin()];
    assert_eq!(
        rim_plane_graze_n(([0.44, 0.0, 0.0], tilt, r), ([1.0, 0.0, 0.0], 0.0)),
        None
    );
    // Center offset reach − 5e-3 ⇒ shallow crossing, demand present.
    let reach = r * (60f64).to_radians().sin();
    assert!(rim_plane_graze_n(
        ([reach - 5.0e-3, 0.0, 0.0], tilt, r),
        ([1.0, 0.0, 0.0], 0.0)
    )
    .is_some());
}
