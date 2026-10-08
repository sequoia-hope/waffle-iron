//! P0027 (2026-10-08): a vertex claimed by two DIFFERENT exact lines is a
//! line∩line junction — two generators crossing where the cylinder×cylinder
//! crease of a previous union pierces a planar face PARALLEL to both axes,
//! the corner `{plane, cyl_A, cyl_B}`. The Stage-4 line arm STOPped on the
//! second record (`line_line_junction`, "out of scope") before the triple
//! block ever ran: the seventh junction map counting ZERO toward `n_maps`
//! (spec `yang_stage4_conic_triple_junction`, "Junction-map candidates — the
//! line pair"). The block now admits the pair; a pair it cannot resolve
//! STOPs at the residue audit with the same site text.
//!
//! Numbers are P0027's second subtract (the un-minimized seed-3 index 54
//! lineage, 5 ops) as printed by `YANG_LRR_PROBE` / `YANG_SAMETYPE_PROBE`
//! on 2026-10-08: cylinder A axis ŷ through the origin, r 2.81841; cylinder
//! B axis −ẑ through (−0.05219, 2.69286, 1.66842), r 2.97256; the pentagon
//! cut's lateral plane x = −2.64808; Stage-4 chord band d_ε = 1.0122e-1.

use super::*;
use crate::stage4_correct::tangent_plane_corridor;
use crate::stage4_relocate::{
    junction_slab_divergence, line_perp_distance, relocate_onto_implicit_triple, same_line,
    surface_value_and_normal, LineReloc,
};

const X_PLANE: f64 = -2.648076841597243;
const CYL_A_R: f64 = 2.8184146079008188;
const CYL_B_P: Point3 = Point3::new(-0.05218874481064795, 2.6928552413966136, 1.6684173559460707);
const CYL_B_R: f64 = 2.972559149650274;
/// The Stage-2 arrangement vertex v50 (probe-printed): on the plane exactly,
/// off both cylinders by their facet chords.
const V50: Point3 = Point3::new(X_PLANE, 1.2508472021310097, 0.9429216800697255);
/// P0027's Stage-4 chord band (`[triple-gate] … d_eps=1.0122e-1`).
const P0027_D_EPS: f64 = 1.0122e-1;

fn p0027_surfaces() -> [Surface; 3] {
    [
        Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 1.0, 0.0),
            radius: CYL_A_R,
        },
        // `n · p + d = 0` with n = −x̂: d = X_PLANE (as the probe printed it).
        Surface::Plane {
            normal: Vector3::new(-1.0, 0.0, 0.0),
            d: X_PLANE,
        },
        Surface::Cylinder {
            axis_point: CYL_B_P,
            axis_dir: Vector3::new(0.0, 0.0, -1.0),
            radius: CYL_B_R,
        },
    ]
}

/// The two line records v50 carried: cylinder A's generator in the plane
/// (first, `vert_line`) and cylinder B's generator in the same plane
/// (second, `vert_line_junction`), as the probe printed them.
fn p0027_lines() -> (LineReloc, LineReloc) {
    let gen_a = LineReloc {
        point: Point3::new(X_PLANE, 0.0, 0.9649611095920373),
        dir: Vector3::new(0.0, 1.0, 0.0),
        band_budget: 0.37012620849260564,
    };
    let gen_b = LineReloc {
        point: Point3::new(X_PLANE, 1.2445897660326426, 1.6684173559460707),
        dir: Vector3::new(0.0, 0.0, -1.0),
        band_budget: 0.24317105941885087,
    };
    (gen_a, gen_b)
}

/// The closed-form crossing of the two coplanar generators — the exact
/// corner, independent of the Newton.
fn p0027_crossing() -> Point3 {
    let (gen_a, gen_b) = p0027_lines();
    Point3::new(X_PLANE, gen_b.point.y(), gen_a.point.z())
}

#[test]
fn p0027_lines_are_different_generators_of_two_cylinders_in_one_plane() {
    let (gen_a, gen_b) = p0027_lines();
    assert!(!same_line(&gen_a, &gen_b));
    assert!(!same_line(&gen_b, &gen_a));
    // Identity is sense-free and point-free: the same line named from
    // another point with the reversed direction is the SAME record.
    let gen_a_again = LineReloc {
        point: Point3::new(X_PLANE, 7.0, gen_a.point.z()),
        dir: Vector3::new(0.0, -1.0, 0.0),
        band_budget: 1.0,
    };
    assert!(same_line(&gen_a, &gen_a_again));
    // Each generator lies in the plane and on its own cylinder (sampled at
    // two points), so the pair is `{plane, cyl_A, cyl_B}`.
    let [cyl_a, plane, cyl_b] = p0027_surfaces();
    for (line, cyl) in [(gen_a, cyl_a), (gen_b, cyl_b)] {
        for t in [-1.0, 2.5] {
            let p = line.point.as_array();
            let d = line.dir.as_array();
            let q = [p[0] + t * d[0], p[1] + t * d[1], p[2] + t * d[2]];
            let (on_plane, _) = surface_value_and_normal(plane, q).expect("plane evaluates");
            let (on_cyl, _) = surface_value_and_normal(cyl, q).expect("cylinder evaluates");
            assert!(
                on_plane.abs() <= 1e-12,
                "generator off the plane by {on_plane:.3e}"
            );
            assert!(
                on_cyl.abs() <= 1e-9,
                "generator off its cylinder by {on_cyl:.3e}"
            );
        }
    }
}

#[test]
fn the_crossing_is_on_all_three_surfaces_and_v50_is_off_both_cylinders() {
    let x = p0027_crossing();
    let (gen_a, gen_b) = p0027_lines();
    assert!(line_perp_distance(x, gen_a.point, gen_a.dir) <= 1e-12);
    assert!(line_perp_distance(x, gen_b.point, gen_b.dir) <= 1e-12);
    for s in p0027_surfaces() {
        let (val, _) = surface_value_and_normal(s, x.as_array()).expect("evaluates");
        assert!(val.abs() <= 1e-9, "crossing residual {val:.3e} on {s:?}");
    }
    // The chord vertex sits on neither generator: relocating it onto ONE
    // line's foot (the pre-admission single-line arm) leaves it off the other
    // cylinder by the chord — which is why the pair needs the triple block.
    let [cyl_a, _, cyl_b] = p0027_surfaces();
    let (ra, _) = surface_value_and_normal(cyl_a, V50.as_array()).expect("evaluates");
    let (rb, _) = surface_value_and_normal(cyl_b, V50.as_array()).expect("evaluates");
    assert!(
        ra.abs() > 1e-4,
        "v50 is a chord vertex off cylinder A: {ra:.3e}"
    );
    assert!(
        rb.abs() > 1e-4,
        "v50 is a chord vertex off cylinder B: {rb:.3e}"
    );
}

#[test]
fn the_triple_newton_lands_v50_on_the_crossing_within_the_slab_corridor() {
    let surfs = p0027_surfaces();
    let q = relocate_onto_implicit_triple(V50, surfs[0], surfs[1], surfs[2])
        .expect("the {cyl_A, plane, cyl_B} Newton converges from the chord vertex");
    let qa = q.as_array();
    let xa = p0027_crossing().as_array();
    for i in 0..3 {
        assert!(
            (qa[i] - xa[i]).abs() <= 1e-9,
            "Newton {qa:?} vs closed-form crossing {xa:?}"
        );
    }
    // The move is the chord sagitta (probe: ρ 2.2911e-2) and the block's
    // three-slab metric admits it with room (gate 1.4008 at sin θ 0.14453).
    let pa = V50.as_array();
    let rho = ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2)).sqrt();
    assert!((rho - 2.2911e-2).abs() < 1e-5, "rho {rho:.6e}");
    let sin_theta = junction_slab_divergence(surfs, qa).expect("slab metric defined");
    assert!(
        (sin_theta - 0.14453).abs() < 1e-4,
        "sin_theta {sin_theta:.6e}"
    );
    let gate = tangent_plane_corridor(P0027_D_EPS, sin_theta);
    assert!(rho <= gate, "rho {rho:.3e} vs gate {gate:.3e}");
}
