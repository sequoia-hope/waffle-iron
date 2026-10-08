//! P0025 (2026-10-08): an ellipse∩ellipse junction whose two ellipses lie in
//! the SAME cutting plane on two DIFFERENT cylinders is a THREE-surface
//! corner — the cylinder×cylinder crease of one operand (two bosses) pierced
//! by a planar face of the other, `{plane, cyl_A, cyl_B}` — not the PR-KV9
//! box-edge / Steinmetz crossing (two planes, ONE cylinder). Stage 4 demoted
//! it into `vert_ell_junction`, whose closed form `(plane₁ ∩ plane₂) ∩
//! cylinder` STOPped `LocalRefinementRequired` at `|n₁ × n₂| <
//! MIN_FEATURE_SIZE` (14 of 32 seed-3 ERROR rows), while the triple block
//! never scanned that map: the sixth junction map counting ZERO toward
//! `n_maps` (spec `yang_stage4_conic_triple_junction`, "Junction-map
//! candidates — the coplanar ellipse pair"). The block now admits the
//! coplanar pair; the non-coplanar pair keeps its closed form.
//!
//! Numbers are P0025's intersect as printed by `YANG_V_PROBE=3` /
//! `YANG_LRR_PROBE` on 2026-10-08 (scale 200): cylinder A axis ŷ through
//! (−10, 20, 20), r 30; cylinder B axis (−0.2914, 0.8742, −0.3885) through
//! (−30, 40, 90), r 60; the prism's lateral plane n = (0.5269, −0.0456,
//! −0.8487), d = 44.697; Stage-4 chord band d_ε = 3.4604.

use super::*;
use crate::stage4_correct::tangent_plane_corridor;
use crate::stage4_relocate::{
    ellipse_pair_coplanar, junction_slab_divergence, relocate_onto_implicit_triple,
    surface_value_and_normal, EllipseReloc,
};

const CYL_A_P: Point3 = Point3::new(-10.0, 20.0, 20.0);
const CYL_A_D: Vector3 = Vector3::new(0.0, 1.0, 0.0);
const CYL_A_R: f64 = 30.0;
const CYL_B_P: Point3 = Point3::new(-30.0, 40.0, 90.0);
const CYL_B_D: Vector3 = Vector3::new(-0.2913857587071793, 0.8741572761215378, -0.3885143449429057);
const CYL_B_R: f64 = 60.0;
const PLANE_N: Vector3 = Vector3::new(
    0.5269019100468014,
    -0.04557792761920944,
    -0.8487031458071607,
);
const PLANE_D: f64 = 44.69712644310505;
/// The Stage-2 arrangement vertex v3 (probe-printed).
const V3: Point3 = Point3::new(-39.28308646716163, 35.583755586597675, 26.366061472762294);
/// P0025's Stage-4 chord band (`[triple-gate] … d_eps=3.4604e0`).
const P0025_D_EPS: f64 = 3.4604;

fn p0025_surfaces() -> [Surface; 3] {
    [
        Surface::Cylinder {
            axis_point: CYL_A_P,
            axis_dir: CYL_A_D,
            radius: CYL_A_R,
        },
        Surface::Cylinder {
            axis_point: CYL_B_P,
            axis_dir: CYL_B_D,
            radius: CYL_B_R,
        },
        Surface::Plane {
            normal: PLANE_N,
            d: PLANE_D,
        },
    ]
}

/// The two ellipse records v3 carried (centre / axes as the probe printed
/// them; only the plane and cylinder fields matter to the predicate).
fn p0025_ellipses() -> (EllipseReloc, EllipseReloc) {
    let e_a = EllipseReloc {
        axis_point: CYL_A_P,
        axis_dir: CYL_A_D,
        radius: CYL_A_R,
        plane_n: PLANE_N,
        plane_d: PLANE_D,
        center: Point3::new(-10.0, 492.6517197993501, 20.0),
        normal: PLANE_N,
        major_axis: Vector3::new(
            0.02404007989953279,
            0.9989607862743853,
            -0.03872237136960977,
        ),
        major_radius: 658.213340690727,
        minor_radius: 30.0,
        second_cyl: None,
    };
    let e_b = EllipseReloc {
        axis_point: CYL_B_P,
        axis_dir: CYL_B_D,
        radius: CYL_B_R,
        center: Point3::new(-135.3838686302533, 356.1516058907598, -50.51182484033771),
        major_axis: Vector3::new(-0.36665856062465174, 0.888672956205461, -0.2753577252043332),
        major_radius: 440.0138089582255,
        minor_radius: 60.0,
        ..e_a
    };
    (e_a, e_b)
}

#[test]
fn p0025_ellipse_pair_is_coplanar_and_on_two_cylinders() {
    let (e_a, e_b) = p0025_ellipses();
    assert!(ellipse_pair_coplanar(&e_a, &e_b));
    assert!(ellipse_pair_coplanar(&e_b, &e_a));
    // Not the same ellipse: the two cylinders differ.
    assert_ne!(e_a.axis_point.as_array(), e_b.axis_point.as_array());
    assert_ne!(e_a.radius, e_b.radius);
}

#[test]
fn the_predicate_is_orientation_free() {
    // `(−n, −d)` names the same plane.
    let (e_a, mut e_b) = p0025_ellipses();
    e_b.plane_n = Vector3::new(-PLANE_N.x(), -PLANE_N.y(), -PLANE_N.z());
    e_b.plane_d = -PLANE_D;
    assert!(ellipse_pair_coplanar(&e_a, &e_b));
}

#[test]
fn a_box_edge_pair_is_not_coplanar() {
    // The PR-KV9 customer: two DISTINCT planes cutting one cylinder (two
    // lateral faces of a box pierced by the cylinder). Its closed form stays
    // the owner — the predicate must decline it.
    let (e_a, mut e_b) = p0025_ellipses();
    e_b.axis_point = e_a.axis_point;
    e_b.axis_dir = e_a.axis_dir;
    e_b.radius = e_a.radius;
    e_b.plane_n = Vector3::new(0.0, 0.0, 1.0);
    e_b.plane_d = -5.0;
    assert!(!ellipse_pair_coplanar(&e_a, &e_b));
    // Parallel but OFFSET planes (two parallel faces) are not the same plane
    // either: the offset check, not just the normals' cross product.
    e_b.plane_n = e_a.plane_n;
    e_b.plane_d = e_a.plane_d + 1.0;
    // Put the centre on the offset plane so the test exercises the offset
    // term honestly.
    let c = e_b.center.as_array();
    let n = PLANE_N.as_array();
    e_b.center = Point3::new(c[0] - n[0], c[1] - n[1], c[2] - n[2]);
    assert!(!ellipse_pair_coplanar(&e_a, &e_b));
}

#[test]
fn the_triple_newton_lands_v3_on_all_three_surfaces_within_the_slab_corridor() {
    let surfs = p0025_surfaces();
    let q = relocate_onto_implicit_triple(V3, surfs[0], surfs[1], surfs[2])
        .expect("the {cyl_A, cyl_B, plane} Newton converges from the chord vertex");
    let qa = q.as_array();
    for s in surfs {
        let (val, _) = surface_value_and_normal(s, qa).expect("evaluates");
        assert!(val.abs() <= 1e-9, "residual {val:.3e} on {s:?}");
    }
    // The move is the chord sagitta (probe: ρ 7.3033e-2) and the block's
    // three-slab metric admits it with room (gate 6.0690e1 at sin θ 0.11403).
    let pa = V3.as_array();
    let rho = ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2)).sqrt();
    assert!((rho - 7.3033e-2).abs() < 1e-5, "rho {rho:.6e}");
    let sin_theta = junction_slab_divergence(surfs, qa).expect("slab metric defined");
    let gate = tangent_plane_corridor(P0025_D_EPS, sin_theta);
    assert!(rho <= gate, "rho {rho:.3e} vs gate {gate:.3e}");
}
