//! Junction-LINE displacement metric, the LINE-CURVE arm (spec
//! `yang_stage4_conic_triple_junction.md`, "Junction-line amendment — the
//! line-curve carriers"; `stage4_relocate::junction_line_curve_divergence`).
//!
//! Pinned on the R0070 op-3 measurement (2026-09-17, `YANG_LRR_PROBE`
//! `[triple-gate]`): a cylinder (r 6.869e-3) drilled into a 152° revolve
//! boss has its axis PARALLEL to the boss's annular cap plane, so cap ∩ cut
//! cylinder is a pair of GENERATOR lines (`Curve` Line, `vert_line`). Where a
//! generator reaches the boss's rim it pierces the boss lateral (r 2.276e-2)
//! at |L̂·n| = 0.3454. The Stage-2 crossing vertex v88 sits EXACTLY on the
//! generator (cap residual −3.5e-18, cut-cylinder residual 0) and 6.116e-4
//! inside the boss lateral — within the Stage-4 chord band d_ε = 7.3383e-4 —
//! and the exact junction lies 2.0014e-3 along the generator, a move with no
//! off-line component. The surface-pair corridor `2·d_ε/sin(n_cap, n_boss)`
//! (sin θ = 1: 1.4677e-3) refused it; the line corridor `2·d_ε/|L̂·n_boss|`
//! (4.249e-3) admits it. The two-PLANE arm (`junction_line_divergence`) sees
//! one plane and declines, which is why R0070 needed this arm.

use super::*;
use crate::stage4_correct::tangent_plane_corridor;
use crate::stage4_relocate::{
    junction_line_curve_divergence, junction_line_divergence, relocate_onto_implicit_triple,
    surface_value_and_normal,
};

/// R0070 op 3's Stage-4 chord band (`stage4_chord_band`, probe-printed).
const R0070_D_EPS: f64 = 7.3383e-4;

/// The revolve boss's annular cap (normal = the revolve axis direction).
fn boss_cap() -> Surface {
    Surface::Plane {
        normal: Vector3::new(
            0.7056587190847249,
            0.7085518839010345,
            -1.935865331115961e-16,
        ),
        d: -0.009402977445675378,
    }
}

/// The revolve boss's outer lateral.
fn boss_lateral() -> Surface {
    Surface::Cylinder {
        axis_point: Point3::new(
            -0.0021421299430065296,
            0.006916332380305597,
            -0.000766000894386696,
        ),
        axis_dir: Vector3::new(-0.7056587190847247, -0.7085518839010351, 0.0),
        radius: 0.02276298919442843,
    }
}

/// The cut cylinder (extruded along the sketch normal, which lies IN the cap
/// plane — hence the generator).
fn cut_lateral() -> Surface {
    Surface::Cylinder {
        axis_point: Point3::new(
            0.0012468834683532874,
            0.0035411570160761713,
            0.01663048156680397,
        ),
        axis_dir: Vector3::new(-0.6831997770107031, 0.680410130123483, 0.2651039786911369),
        radius: 0.006869464164596666,
    }
}

/// The Stage-2 crossing vertex v88, as Stage 4 found it.
const V88: Point3 = Point3::new(
    0.0021097767480882483,
    0.011169535030366369,
    0.021385414037378227,
);

/// The generator direction is the cut cylinder's axis.
fn generator_dir() -> Vector3 {
    Vector3::new(-0.6831997770107031, 0.680410130123483, 0.2651039786911369)
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// The measured R0070 pierce: v88 is on the generator, inside the boss by
/// 6.116e-4, the exact junction 2.0014e-3 along the generator; the curve
/// corridor refuses it, the line-curve corridor admits it.
#[test]
fn r0070_v88_generator_pierce_takes_the_line_curve_corridor() {
    let (cap, boss, cut) = (boss_cap(), boss_lateral(), cut_lateral());
    let pa = V88.as_array();
    // The certificate's premise: v88 is exact on both carriers and inside the
    // boss lateral within the chord band.
    let (cap_res, _) = surface_value_and_normal(cap, pa).unwrap();
    let (cut_res, _) = surface_value_and_normal(cut, pa).unwrap();
    let (boss_res, _) = surface_value_and_normal(boss, pa).unwrap();
    assert!(cap_res.abs() < 1e-15, "cap residual {cap_res:e}");
    assert!(cut_res.abs() < 1e-15, "cut residual {cut_res:e}");
    assert!(
        (boss_res.abs() - 6.116e-4).abs() < 1e-6,
        "boss offset {boss_res:e}"
    );
    assert!(boss_res.abs() <= R0070_D_EPS);

    let q = relocate_onto_implicit_triple(V88, cap, boss, cut).expect("triple Newton converges");
    let qa = q.as_array();
    for s in [cap, boss, cut] {
        let (res, _) = surface_value_and_normal(s, qa).unwrap();
        assert!(res.abs() < 1e-12, "q off {s:?} by {res:e}");
    }
    let disp = sub(qa, pa);
    let rho = len(disp);
    assert!((rho - 2.0014e-3).abs() < 1e-6, "ρ = {rho:e}");
    // The move is ALONG the generator: no off-line component.
    let l = normalize3(generator_dir().as_array());
    let along = dot(disp, l);
    assert!(len(sub(disp, [l[0] * along, l[1] * along, l[2] * along])) < 1e-12);

    // The curve corridor (θ between the cap normal and the boss normal at q —
    // two surfaces the move slides within NEITHER of) refuses the exact
    // junction — the measured R0070 wall.
    let (_, n_cap) = surface_value_and_normal(cap, qa).unwrap();
    let (_, n_boss) = surface_value_and_normal(boss, qa).unwrap();
    let sin_theta = len(cross(n_cap, n_boss));
    assert!((sin_theta - 1.0).abs() < 1e-9);
    let curve_gate = tangent_plane_corridor(R0070_D_EPS, sin_theta);
    assert!((curve_gate - 1.4677e-3).abs() < 1e-6);
    assert!(
        rho > curve_gate,
        "curve corridor {curve_gate} did not refuse ρ = {rho}"
    );

    // The two-plane arm declines (one plane only) — the gap R0070 fell through.
    assert_eq!(junction_line_divergence([cap, boss, cut], qa), None);

    // The line-curve arm: divergence |L̂·n_boss| = 0.3454, any surface order.
    let line = (V88, generator_dir());
    let div =
        junction_line_curve_divergence(line, pa, [cap, boss, cut], qa).expect("generator junction");
    assert!((div - dot(l, n_boss).abs()).abs() < 1e-12);
    assert!((div - 0.3454).abs() < 1e-4, "div {div}");
    assert_eq!(
        Some(div),
        junction_line_curve_divergence(line, pa, [boss, cut, cap], qa)
    );
    assert_eq!(
        Some(div),
        junction_line_curve_divergence(line, pa, [cut, cap, boss], qa)
    );
    let line_gate = tangent_plane_corridor(R0070_D_EPS, div);
    assert!((line_gate - 4.249e-3).abs() < 1e-5);
    assert!(
        rho <= line_gate,
        "line corridor {line_gate} refused ρ = {rho}"
    );
    // First-order bound: the move closes the boss offset along the line.
    assert!(rho * div <= 2.0 * R0070_D_EPS);
    // Never below the curve corridor measured against either carrier.
    let (_, n_cut) = surface_value_and_normal(cut, qa).unwrap();
    assert!(div <= len(cross(n_cap, n_boss)) + 1e-12);
    assert!(div <= len(cross(n_cut, n_boss)) + 1e-12);
}

/// The certificate refuses a vertex that is OFF the line (its carriers are
/// not exact there): the caller keeps the curve corridor.
#[test]
fn a_vertex_off_the_line_yields_no_line_curve_divergence() {
    let (cap, boss, cut) = (boss_cap(), boss_lateral(), cut_lateral());
    let q = relocate_onto_implicit_triple(V88, cap, boss, cut)
        .unwrap()
        .as_array();
    // Push v88 1e-6 off the generator, perpendicular to it (along the cap
    // normal — off the cut cylinder, still on the cap).
    let n_cap = normalize3([0.7056587190847249, 0.7085518839010345, 0.0]);
    let pa = V88.as_array();
    let off = [
        pa[0] + 1e-6 * n_cap[0],
        pa[1] + 1e-6 * n_cap[1],
        pa[2] + 1e-6 * n_cap[2],
    ];
    assert_eq!(
        junction_line_curve_divergence((V88, generator_dir()), off, [cap, boss, cut], q),
        None
    );
    // At 1e-12 (inside the on-line band) it is still the same junction.
    let near = [
        pa[0] + 1e-12 * n_cap[0],
        pa[1] + 1e-12 * n_cap[1],
        pa[2] + 1e-12 * n_cap[2],
    ];
    assert!(
        junction_line_curve_divergence((V88, generator_dir()), near, [cap, boss, cut], q).is_some()
    );
}

/// Not a line junction: a line that lies in only ONE of the three surfaces,
/// or that pierces two of them, yields `None`.
#[test]
fn wrong_carrier_counts_yield_no_line_curve_divergence() {
    let (cap, boss, cut) = (boss_cap(), boss_lateral(), cut_lateral());
    let q = relocate_onto_implicit_triple(V88, cap, boss, cut)
        .unwrap()
        .as_array();
    let pa = V88.as_array();
    // A direction in the cap plane but NOT along the cut cylinder's axis:
    // the cap carries it, the two cylinders are both pierced (2 transversal).
    let in_cap = normalize3(cross(
        [0.7056587190847249, 0.7085518839010345, 0.0],
        [0.0, 0.0, 1.0],
    ));
    assert_eq!(
        junction_line_curve_divergence(
            (V88, Vector3::new(in_cap[0], in_cap[1], in_cap[2])),
            pa,
            [cap, boss, cut],
            q
        ),
        None
    );
    // Three surfaces that all carry the line (a second copy of the cap in
    // place of the boss): no pierced surface.
    assert_eq!(
        junction_line_curve_divergence((V88, generator_dir()), pa, [cap, cut, cap], q),
        None
    );
}
