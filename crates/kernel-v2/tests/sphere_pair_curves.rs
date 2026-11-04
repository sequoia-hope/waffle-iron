//! A sphere meeting a cylinder off its centre: `ssi_rs` returns the
//! intersection as a surface-pair curve, and Stage 3 must recognise the mesh
//! edges that lie on it. Its membership test once read each surface's
//! distance through a per-kind closure that had no sphere arm, so every
//! sphere × cylinder / cone / torus boolean stopped loudly with
//! `AmbiguousCurve { candidates: 1, matched: 0 }` (kernel 1.0 gate-3 pair
//! matrix, 2026-11-03). These pins hold the three operations.

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, Profile, SolidId,
};

const R: f64 = 1.0;
const CX: f64 = 5.0;
/// Cylinder radius and its offset from the sphere's centre: the axis misses
/// the centre, so the section is a genuine space quartic, not a circle.
const RC: f64 = 0.35;

fn sphere(arena: &mut BrepArena) -> SolidId {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(CX, 0.0),
        R,
    )
    .expect("on-axis circle");
    revolve(
        arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("closed sphere")
    .solid
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / n, v[1] / n, v[2] / n]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// A cylinder of radius `RC` on an oblique axis through
/// `(CX + 0.3, 0.2, 0)`, long enough to pass right through the sphere.
fn oblique_cylinder(arena: &mut BrepArena) -> SolidId {
    let n = unit([0.3, 0.2, 1.0]);
    let u = unit(cross(n, [0.0, 1.0, 0.0]));
    let v = cross(n, u);
    let through = [CX + 0.3, 0.2, 0.0];
    let half = 2.5;
    let origin = [
        through[0] - half * n[0],
        through[1] - half * n[1],
        through[2] - half * n[2],
    ];
    let profile = Profile::circle(
        Point3::new(origin[0], origin[1], origin[2]),
        Vector3::new(u[0], u[1], u[2]),
        Vector3::new(v[0], v[1], v[2]),
        Point2::new(0.0, 0.0),
        RC,
    )
    .expect("tilted circle profile");
    extrude(arena, &profile, Vector3::new(n[0], n[1], n[2]), 2.0 * half)
        .expect("oblique cylinder")
        .solid
}

fn mesh_volume(arena: &BrepArena, solid: SolidId) -> f64 {
    let mesh = tessellate(arena, solid).expect("result tessellates");
    let mut v = 0.0;
    for t in mesh.indices.chunks(3) {
        let p = |i: u32| {
            let k = 3 * i as usize;
            [
                mesh.positions[k],
                mesh.positions[k + 1],
                mesh.positions[k + 2],
            ]
        };
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    v / 6.0
}

/// The part of the cylinder inside the sphere lies between 0 and the volume
/// of a cylinder of radius `RC` and length `2R` (a chord is at most a
/// diameter), so each result has a band it must sit in.
fn bounds() -> (f64, f64) {
    (4.0 / 3.0 * PI * R * R * R, PI * RC * RC * 2.0 * R)
}

fn run(op: BoolOp) -> f64 {
    let mut arena = BrepArena::new();
    let s = sphere(&mut arena);
    let c = oblique_cylinder(&mut arena);
    let out = boolean_op(&mut arena, s, c, op)
        .unwrap_or_else(|e| panic!("sphere {op:?} oblique cylinder failed: {e:?}"));
    let report = validate_solid(&arena, out).expect("result validates");
    assert_eq!(report.euler_lhs, report.euler_rhs, "{op:?}: Euler holds");
    mesh_volume(&arena, out)
}

#[test]
fn a_sphere_minus_an_off_centre_cylinder_is_a_drilled_ball() {
    let (ball, plug_max) = bounds();
    let v = run(BoolOp::Subtract);
    assert!(
        v < ball && v > ball - plug_max,
        "drilled ball volume {v} outside ({}, {ball})",
        ball - plug_max
    );
}

#[test]
fn a_sphere_intersect_an_off_centre_cylinder_is_the_plug() {
    let (_, plug_max) = bounds();
    let v = run(BoolOp::Intersect);
    assert!(
        v > 0.0 && v < plug_max,
        "plug volume {v} outside (0, {plug_max})"
    );
}

#[test]
fn a_sphere_union_an_off_centre_cylinder_grows_the_ball() {
    let (ball, _) = bounds();
    let v = run(BoolOp::Union);
    assert!(v > ball, "union volume {v} not above the ball's {ball}");
}
