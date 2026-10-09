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

// ── The arm's LOCAL form (spec §5k, P0029) ───────────────────────────
//
// P0029's measured site (seed-3 index 109, op 4's subtract, as printed by
// `NONMANIFOLD_SITE_PROBE s4-dc-attr` on 2026-10-09): the cut cylinder's
// bottom cap rim grazes the prism's own bottom cap plane by 1.455e-2 on a
// radius of 24.1289 — a relative 6.0e-4, so the lens sits BELOW the
// render-observability line and the body-wide arm refuses it.

/// B's bottom cap rim: center = the cylinder's axis point (the rim plane
/// passes through it), normal = the cylinder axis, r = 24.1289.
const P0029_RIM: ([f64; 3], [f64; 3], f64) = (
    [-0.8717941718665827, -23.631148712246116, -5.138305924828366],
    [0.2022032143054565, 0.45581911860854896, 0.8668003179714849],
    24.128865097509188,
);
/// A's bottom cap plane, `n·p + d = 0` with n = −ẑ.
const P0029_PLANE: ([f64; 3], f64) = ([0.0, 0.0, -1.0], -17.155745093009863);
/// The two exact {A-plane, B-cap-plane, B-cylinder} corners the arrangement
/// already carries as ADJACENT rim vertices — which is what flattens the
/// crossing into a tangency: a straight edge between two points that lie in
/// BOTH planes lies in both planes.
const P0029_V18: [f64; 3] = [8.683234895040057, -5.017049049264198, -17.155745093009863];
const P0029_V19: [f64; 3] = [6.514525822650038, -4.055000887113145, -17.155745093009863];

fn signed_to_plane(p: [f64; 3], (m, d): ([f64; 3], f64)) -> f64 {
    let mlen = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
    (m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + d) / mlen
}

fn rim_point((c, n, r): ([f64; 3], [f64; 3], f64), t: f64) -> [f64; 3] {
    let axis = canonical_axis(n);
    let (e1, e2) = ortho_basis(Vector3::new(axis[0], axis[1], axis[2]));
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let (s, co) = t.sin_cos();
    [
        c[0] + r * (co * e1[0] + s * e2[0]),
        c[1] + r * (co * e1[1] + s * e2[1]),
        c[2] + r * (co * e1[2] + s * e2[2]),
    ]
}

/// The two arms' scopes are COMPLEMENTARY at the render line: P0029's lens
/// derives no body-wide N (so every case converting on the rim-N floor
/// today is untouched) while the sagitta demand behind it is real — 128
/// against a natural 9, the floor the corpus has twice refused.
#[test]
pub(crate) fn rim_plane_p0029_is_sub_render_with_a_real_demand() {
    let r = P0029_RIM.2;
    let lens = rim_plane_lens(P0029_RIM, P0029_PLANE).expect("the pair grazes");
    assert!(
        (lens.depth - 1.45497282900e-2).abs() < 1e-12,
        "measured lens depth, got {}",
        lens.depth
    );
    assert!(
        lens.depth <= RIM_PLANE_RENDER_LINE * r,
        "the lens must be BELOW the render line (that is why §5k exists)"
    );
    assert_eq!(
        rim_plane_graze_n(P0029_RIM, P0029_PLANE),
        None,
        "the body-wide arm must keep refusing it"
    );
    assert_eq!(
        rim_sag_demand(r, lens.depth),
        Some(128),
        "the sagitta demand behind the refusal"
    );
}

/// The lens's azimuth geometry IS the site: the apex stands the full depth
/// clear of the plane, and `apex ± half_span` are the two exact crossings
/// the probe found already seated as adjacent rim vertices.
#[test]
pub(crate) fn rim_plane_p0029_apex_and_crossings_are_the_measured_corners() {
    let lens = rim_plane_lens(P0029_RIM, P0029_PLANE).expect("the pair grazes");
    let apex = rim_point(P0029_RIM, lens.apex);
    assert!(
        (signed_to_plane(apex, P0029_PLANE) - lens.depth).abs() < 1e-12,
        "the apex stands `depth` clear of the plane, got {}",
        signed_to_plane(apex, P0029_PLANE)
    );
    for t in [lens.apex - lens.half_span, lens.apex + lens.half_span] {
        let p = rim_point(P0029_RIM, t);
        assert!(
            signed_to_plane(p, P0029_PLANE).abs() < 1e-12,
            "a crossing lies ON the plane, got {}",
            signed_to_plane(p, P0029_PLANE)
        );
        let near = |q: [f64; 3]| {
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt() < 1e-9
        };
        assert!(
            near(P0029_V18) || near(P0029_V19),
            "a crossing must be one of the two probe-printed corners, got {p:?}"
        );
    }
}

/// The sample set: apex-centred, every sample STRICTLY inside the arc (so
/// neither crossing is re-minted — the band-merge hazard §5k names), and
/// three of them for P0029.
#[test]
pub(crate) fn rim_plane_p0029_samples_are_apex_centred_and_submerged() {
    let r = P0029_RIM.2;
    let lens = rim_plane_lens(P0029_RIM, P0029_PLANE).expect("the pair grazes");
    let n = rim_sag_demand(r, lens.depth).expect("a demand");
    let step = 2.0 * std::f64::consts::PI / n as f64;
    let angles = submerged_arc_samples(lens.apex, lens.half_span, step).expect("samples");
    assert_eq!(angles.len(), 3, "apex plus one neighbour each way");
    assert_eq!(angles[0], lens.apex, "the apex leads");
    for &t in &angles {
        assert!(
            (t - lens.apex).abs() < lens.half_span,
            "every sample is strictly inside the arc — never a crossing"
        );
        let p = rim_point(P0029_RIM, t);
        assert!(
            signed_to_plane(p, P0029_PLANE) > 0.0,
            "every sample is on the submerged side, so the polyline dips past \
             the plane as the exact rim does"
        );
    }
}

/// The guarantee the sample set rests on, over the whole family rather than
/// one case: whatever the lens, the apex sample is submerged by the FULL
/// depth, the set is symmetric about it (so a coaxial closure gets one
/// shared azimuth set and keeps equal rings), and no sample reaches a
/// crossing. Swept over depth and rim tilt.
#[test]
pub(crate) fn rim_plane_local_samples_hold_the_guarantee_across_the_family() {
    for &r in &[0.5f64, 24.128_865_097_509_188, 1.0e3] {
        for &rel in &[1.0e-5f64, 1.0e-4, 5.0e-4, 1.9e-3] {
            for &deg in &[0.0f64, 30.0, 60.0, 85.0] {
                let depth = rel * r;
                let (c, s) = deg.to_radians().sin_cos();
                // Rim normal tilted `deg` off the plane normal ⇒ k = cos(deg)…
                // place the center so the shallow-side extent is `depth`.
                let n_rim = [c, 0.0, s];
                let k = (1.0 - (-n_rim[2]).powi(2)).max(0.0).sqrt();
                if r * k <= depth {
                    continue; // no crossing at this tilt
                }
                let rim = ([0.0, 0.0, -(r * k - depth)], n_rim, r);
                let plane = ([0.0, 0.0, -1.0], 0.0);
                let lens = rim_plane_lens(rim, plane).expect("a crossing");
                assert!(
                    (lens.depth - depth).abs() <= 1e-9 * r.max(1.0),
                    "constructed depth {depth} vs derived {}",
                    lens.depth
                );
                let Some(n) = rim_sag_demand(r, lens.depth) else {
                    continue;
                };
                let step = 2.0 * std::f64::consts::PI / n as f64;
                let angles =
                    submerged_arc_samples(lens.apex, lens.half_span, step).expect("samples");
                assert!(!angles.is_empty(), "the apex is always derived");
                let apex_d = signed_to_plane(rim_point(rim, lens.apex), plane);
                assert!(
                    (apex_d.abs() - lens.depth).abs() <= 1e-9 * r.max(1.0),
                    "apex depth {apex_d} vs lens depth {}",
                    lens.depth
                );
                let mut offs: Vec<f64> = angles.iter().map(|t| t - lens.apex).collect();
                offs.sort_by(f64::total_cmp);
                for w in offs.windows(2) {
                    assert!(
                        w[1] - w[0] <= step * (1.0 + 1e-12),
                        "consecutive samples never exceed the demanded step"
                    );
                }
                for &o in &offs {
                    assert!(
                        o.abs() < lens.half_span,
                        "no sample reaches a crossing (r={r} rel={rel} deg={deg})"
                    );
                    assert!(
                        offs.iter().any(|&p| (p + o).abs() < 1e-12),
                        "the set is symmetric about the apex, so every rim of a \
                         closure gets the same azimuths"
                    );
                }
            }
        }
    }
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
