//! Junction displacement metric, the THREE-SLAB arm (spec
//! `yang_stage4_conic_triple_junction.md`, "Junction-line amendment — the
//! three-slab bound"; `stage4_relocate::junction_slab_divergence`).
//!
//! Pinned on the R0050 op-3 measurement (2026-09-22, `YANG_TORUS_PROBE` at
//! the §4.5.2 ladder's d_ε/4 rung, v547): the Revolve-3 boss torus B pierces
//! operand A's edge — the parallel circle where A's cut torus meets the
//! rectangle revolve's cap plane — at |t̂·n_B| = 0.0347 (2.0°). The Stage-2
//! crossing vertex sits ON the cap plane, 1.98e-2 off A's torus and 6.2e-3
//! off B's (all within d_ε = 8.18e-2), and the exact junction — converged on
//! all three surfaces, the only crossing of that circle within 6.7 units of
//! arc — lies 0.332 away, 0.327 of it ALONG the circle. The surface-pair
//! corridor `2·d_ε/sin θ` (θ = 34.5° between A's torus and its cap) refused
//! it at 0.289; the three-slab bound admits it. Every rung of the ladder
//! (d_ε/3 … d_ε/16) refused the same site with the same metric.

use super::*;
use crate::stage4_correct::tangent_plane_corridor;
use crate::stage4_relocate::{
    junction_line_divergence, junction_slab_divergence, relocate_onto_implicit_triple,
    surface_value_and_normal,
};

/// R0050 op 3's Stage-4 chord band at the d_ε/4 rung (probe-printed).
const R0050_D_EPS_RUNG4: f64 = 8.1785e-2;

/// The shared revolve axis of the two tori and the cap plane's normal.
fn axis() -> Vector3 {
    Vector3::new(0.8095934154839622, 0.5869910575170738, 0.0)
}

/// Operand A's cut torus (op 2, the 115° circle revolve).
fn torus_a() -> Surface {
    Surface::Torus {
        center: Point3::new(5.518820044765265, -7.827431316573897, 7.781175252265069),
        axis_dir: axis(),
        major_radius: 3.9508518457613926,
        minor_radius: 2.6339012305075946,
    }
}

/// Operand A's cap plane (op 1, the rectangle revolve's annulus, ⊥ the axis).
fn cap_plane() -> Surface {
    Surface::Plane {
        normal: Vector3::new(-0.8095934154839622, -0.5869910575170738, 0.0),
        d: -2.296571595893922,
    }
}

/// Operand B's boss torus (op 3, the 330° circle revolve).
fn torus_b() -> Surface {
    Surface::Torus {
        center: Point3::new(5.565900005122726, -7.892365228970914, 7.9366274781205455),
        axis_dir: axis(),
        major_radius: 3.7759280618729063,
        minor_radius: 2.517285374581937,
    }
}

/// The Stage-2 crossing vertex v547 at the d_ε/4 rung, as Stage 4 found it.
const V547: Point3 = Point3::new(4.7324207538263385, -10.439525779850797, 6.009230095557655);

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

/// The measured R0050 pierce: the move is along A's edge curve, explained
/// by B's chord offset through the 2° pierce angle; the pair corridor
/// refuses it, the three-slab bound admits it.
#[test]
fn r0050_v547_grazing_torus_pierce_takes_the_slab_bound() {
    let (ta, cap, tb) = (torus_a(), cap_plane(), torus_b());
    let pa = V547.as_array();
    let d_eps = R0050_D_EPS_RUNG4;
    // Premise: the chord vertex is within d_ε of all three surfaces (exact on
    // the plane, off each torus by its chord sag).
    let (cap_res, _) = surface_value_and_normal(cap, pa).unwrap();
    let (ta_res, _) = surface_value_and_normal(ta, pa).unwrap();
    let (tb_res, _) = surface_value_and_normal(tb, pa).unwrap();
    assert!(cap_res.abs() < 1e-14, "cap residual {cap_res:e}");
    assert!(
        (ta_res - 1.976e-2).abs() < 1e-4,
        "torus A offset {ta_res:e}"
    );
    assert!((tb_res - 6.21e-3).abs() < 1e-4, "torus B offset {tb_res:e}");
    assert!(ta_res.abs() <= d_eps && tb_res.abs() <= d_eps);

    let q = relocate_onto_implicit_triple(V547, ta, cap, tb).expect("triple Newton converges");
    let qa = q.as_array();
    for s in [ta, cap, tb] {
        let (res, _) = surface_value_and_normal(s, qa).unwrap();
        assert!(res.abs() < 1e-12, "q off {s:?} by {res:e}");
    }
    let disp = sub(qa, pa);
    let rho = len(disp);
    assert!((rho - 0.3323).abs() < 1e-3, "ρ = {rho}");

    // The move is ALONG A's edge curve: t̂ = n_A × n_cap at q.
    let (_, n_a) = surface_value_and_normal(ta, qa).unwrap();
    let (_, n_cap) = surface_value_and_normal(cap, qa).unwrap();
    let (_, n_b) = surface_value_and_normal(tb, qa).unwrap();
    let t = normalize3(cross(n_a, n_cap));
    let along = dot(disp, t).abs();
    assert!((along - 0.3274).abs() < 1e-3, "along-curve {along}");
    // …and B pierces that curve at 2.0°.
    let pierce = dot(t, n_b).abs();
    assert!((pierce - 0.0347).abs() < 1e-3, "|t̂·n_B| = {pierce}");

    // The pair corridor (θ between A's torus and its cap) refuses the exact
    // junction — the measured R0050 wall at every rung.
    let pair_sin = len(cross(n_a, n_cap));
    assert!((pair_sin - 0.5668).abs() < 1e-3);
    let pair_gate = tangent_plane_corridor(d_eps, pair_sin);
    assert!((pair_gate - 0.2886).abs() < 1e-3);
    assert!(
        rho > pair_gate,
        "pair corridor {pair_gate} did not refuse ρ = {rho}"
    );

    // No line arm applies (one plane, no line curve).
    assert_eq!(junction_line_divergence([ta, cap, tb], qa), None);

    // The three-slab bound: order-independent, below the pair divergence,
    // and it admits the move.
    let div = junction_slab_divergence([ta, cap, tb], qa).expect("full-rank junction");
    for perm in [[cap, tb, ta], [tb, ta, cap]] {
        let other = junction_slab_divergence(perm, qa).unwrap();
        assert!((other - div).abs() < 1e-12 * div, "{other} vs {div}");
    }
    assert!(
        div < pair_sin,
        "slab divergence {div} not below pair {pair_sin}"
    );
    let slab_gate = tangent_plane_corridor(d_eps, div);
    assert!(rho <= slab_gate, "slab bound {slab_gate} refused ρ = {rho}");
    // It is a bound of the pierce angle's order — not vacuous: the
    // parallelepiped's reach is at most the along-curve reach plus the
    // across-curve corridor, so the divergence is at least a third of the
    // pierce cosine.
    assert!(
        div > pierce / 3.0,
        "slab divergence {div} vs pierce {pierce}"
    );
    assert!(div < pierce, "slab divergence {div} vs pierce {pierce}");
}

/// Three mutually orthogonal planes: the parallelepiped is the cube of
/// half-width d_ε, its far vertex √3·d_ε away — divergence 1/√3, gate
/// 2√3·d_ε. Above the pair corridor there (sin θ = 1), so the caller keeps
/// the pair metric for such a junction.
#[test]
fn orthogonal_triple_is_the_cube_diagonal() {
    let px = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: 0.0,
    };
    let py = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: 0.0,
    };
    let pz = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let div = junction_slab_divergence([px, py, pz], [0.0, 0.0, 0.0]).unwrap();
    assert!((div - 1.0 / 3f64.sqrt()).abs() < 1e-15, "div {div}");
}

/// A grazing third plane: two orthogonal planes carry the line `z`, the
/// third has normal at angle `α` to that line. The far vertex reaches
/// `1/sin α` along the line, so the divergence approaches `sin α` — the
/// line arm's `|L̂·n₃|` — for small α, and never exceeds it.
#[test]
fn grazing_third_plane_recovers_the_line_metric() {
    let px = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: 0.0,
    };
    let py = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: 0.0,
    };
    for alpha in [0.5f64, 0.1, 0.02, 0.005] {
        let (s, c) = alpha.sin_cos();
        let third = Surface::Plane {
            normal: Vector3::new(c, 0.0, s),
            d: 0.0,
        };
        let div = junction_slab_divergence([px, py, third], [0.0, 0.0, 0.0]).unwrap();
        // The line arm itself declines three planes (it wants exactly two);
        // its metric for this configuration is |L̂·n₃| = sin α by definition.
        let line = s;
        assert!(
            div <= line + 1e-15,
            "α={alpha}: slab {div} above line {line}"
        );
        // The slab bound also pays the carriers' own d_ε slabs, so it sits
        // within a factor (1 + 1/tan α)·√… of the line metric — check the
        // ratio tightens toward 1 as the pierce grazes.
        let ratio = line / div;
        assert!((1.0..3.0).contains(&ratio), "α={alpha}: ratio {ratio}");
    }
}

/// Rank-deficient normals (two coincident planes) decline — the caller
/// keeps its metric and the triple Newton STOPs there anyway.
#[test]
fn rank_deficient_triple_declines() {
    let px = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: 0.0,
    };
    let px2 = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: 1.0,
    };
    let py = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: 0.0,
    };
    assert_eq!(
        junction_slab_divergence([px, px2, py], [0.0, 0.0, 0.0]),
        None
    );
}
