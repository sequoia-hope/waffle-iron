//! Stage-1 operand self-contact guard, CHART-bound channel (P0037's shape,
//! 2026-10-12; spec `yang_stage1_self_contact_guard` §2 "channel").
//!
//! A cylinder boss (r 36.1 along +y, 64 tall) is cut by an equal-radius
//! cylinder whose axis runs along x at (y 31.2, z 37.2): the cut's start
//! cap plane x = 36 passes 0.1 inside the boss, leaving a lens of boss
//! material 0.1 thick at most between the plane and the boss wall, and a
//! sliver of cap between the boss rim (z = ±2.685 at x = 36) and the cutter
//! circle (z ≥ 1.1). The body's boss lateral and cutter lateral are both
//! on Stage 1's seeded chart path, whose interior census enforces the
//! OPERAND budget (the rim AABB × 1e-2 = 1.496 on a 36.1 radius). A chord
//! leaving the lens corner for a grid row more than 8.54° away dips below
//! x = 36 and through the cutter lateral: the arrangement's exact tri–tri
//! classification reports the body's own mesh as self-intersecting.
//!
//! The guard's rim-N channel raised N 11 → 47 over four rounds without
//! touching the census bound, so the corner chord kept spanning 8.9°–15°
//! (the grid's phase against the corner) and the next boolean STOPped:
//! `Stage-1 mesh of one operand self-intersects: 17 improper triangle
//! contact(s) … after 4 refinement round(s)`. A cylinder lateral now also
//! halves its per-face CHART bound — the bound the seeded path reads — so
//! the halving converges. RED with `YANG_S1_SELF_CONTACT_CHART=0` (the
//! P0037 STOP, verbatim); GREEN with the channel: the union validates.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh, SolidId,
};

const R: f64 = 36.1;
const BOSS_H: f64 = 64.0;
/// The cutter's axis (y, z) and its x-extent [9, 36].
const CUT_Y: f64 = 31.2;
const CUT_Z: f64 = 37.2;
const CUT_X0: f64 = 9.0;
const CUT_DEPTH: f64 = 27.0;
/// The third operation: a 20-radius diamond on the plane y = 60 at
/// (8, 60, −40), extruded 60 along +y.
const BOX_O: [f64; 3] = [8.0, 60.0, -40.0];
const BOX_R: f64 = 20.0;
const BOX_H: f64 = 60.0;

/// A +y plane's frame: u = ẑ, v = x̂ (u × v = +ŷ).
fn plus_y_frame() -> (Vector3, Vector3) {
    (Vector3::new(0.0, 0.0, 1.0), Vector3::new(1.0, 0.0, 0.0))
}

fn boss(a: &mut BrepArena) -> SolidId {
    let (u, v) = plus_y_frame();
    let p = Profile::circle(Point3::new(0.0, 0.0, 0.0), u, v, Point2::new(0.0, 0.0), R)
        .expect("boss profile");
    extrude(a, &p, Vector3::new(0.0, 1.0, 0.0), BOSS_H)
        .expect("boss extrude")
        .solid
}

/// The cutter as the engine resolves the sketch's blind cut: the cylinder
/// x ∈ [9, 36] about the axis (y 31.2, z 37.2). Built from its far end so
/// the extrusion runs along the plane normal (+x).
fn cutter(a: &mut BrepArena) -> SolidId {
    let p = Profile::circle(
        Point3::new(CUT_X0, CUT_Y, CUT_Z),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        Point2::new(0.0, 0.0),
        R,
    )
    .expect("cutter profile");
    extrude(a, &p, Vector3::new(1.0, 0.0, 0.0), CUT_DEPTH)
        .expect("cutter extrude")
        .solid
}

fn diamond_prism(a: &mut BrepArena) -> SolidId {
    let (u, v) = plus_y_frame();
    let p = Profile::new(
        Point3::new(BOX_O[0], BOX_O[1], BOX_O[2]),
        u,
        v,
        vec![
            Point2::new(BOX_R, 0.0),
            Point2::new(0.0, BOX_R),
            Point2::new(-BOX_R, 0.0),
            Point2::new(0.0, -BOX_R),
        ],
        vec![],
    )
    .expect("diamond profile");
    extrude(a, &p, Vector3::new(0.0, 1.0, 0.0), BOX_H)
        .expect("prism extrude")
        .solid
}

fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let mut six_v = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

#[test]
fn a_body_with_a_sub_chord_lens_re_enters_a_union_through_the_chart_channel() {
    let mut a = BrepArena::new();
    let b = boss(&mut a);
    let v_boss = mesh_signed_volume(&tessellate(&a, b).expect("boss tessellates"));
    let c = cutter(&mut a);
    let body = boolean_op(&mut a, b, c, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("boss − cutter failed: {e:?}"));
    let v_body = mesh_signed_volume(&tessellate(&a, body).expect("body tessellates"));
    assert!(
        v_body > 0.0 && v_body < v_boss,
        "the cut keeps a proper part of the boss: {v_boss} → {v_body}"
    );
    // The lens survives the cut: boss material beyond the cap plane x = 36.
    let body_mesh = tessellate(&a, body).expect("body tessellates");
    let max_x = body_mesh
        .positions
        .chunks_exact(3)
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        max_x > CUT_X0 + CUT_DEPTH + 0.05,
        "the 0.1 lens beyond x = 36 is part of the body: max x {max_x}"
    );

    // P0037's STOP: the body's Stage-1 mesh self-intersected across the
    // lens and the union was refused.
    let prism = diamond_prism(&mut a);
    let v_prism = mesh_signed_volume(&tessellate(&a, prism).expect("prism tessellates"));
    let out = boolean_op(&mut a, body, prism, BoolOp::Union)
        .unwrap_or_else(|e| panic!("body ∪ prism failed: {e:?}"));
    let report = validate_solid(&a, out).expect("union validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.euler_lhs, report.euler_rhs);
    let v_out = mesh_signed_volume(&tessellate(&a, out).expect("union tessellates"));
    assert!(
        v_out > v_body && v_out < v_body + v_prism,
        "the union adds the prism's part outside the body: {v_body} + {v_prism} → {v_out}"
    );
}
