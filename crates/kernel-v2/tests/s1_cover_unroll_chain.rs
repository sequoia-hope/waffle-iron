//! Stage-1 UNIVERSAL-COVER unroll of a sheared lateral patch (P0038's shape,
//! 2026-10-12; spec `yang_stage1_curved_holed_patch` §"Slice I").
//!
//! Three bosses — a diamond prism on z = 0, a diamond prism on an oblique
//! plane, and a unit cylinder along +x — are unioned, then a diamond prism
//! on a second oblique plane is cut. The union's cylinder lateral (P0038's
//! face 15) is bounded by its two rim arcs and the oblique prisms' section
//! ellipses: its bottom rim spans −51° … 295° and its top rim 54° … 382°,
//! so the complement wedge rotates ~290° from one rim to the other and no
//! generator meridian misses the face, although every circle of latitude
//! meets it in less than a full turn. The outer loop nets zero winding but
//! unwraps to 7.5551 rad ≥ 2π, and the single-sheet chart STOPped the cut:
//! `malformed B-Rep topology: face 15: bounded lateral patch spans 7.5551
//! rad of azimuth unwrapped (≥ 2π): no seam wedge for a single-sheet
//! unroll`.
//!
//! An embedded disc bounded by a null-homotopic loop lifts homeomorphically
//! to the cylinder's universal cover, so the loop's unwrapped polyline is a
//! simple chart polygon and the CDT runs there. RED with
//! `YANG_S1_COVER_UNROLL=0` (the P0038 STOP, verbatim); GREEN with the arm:
//! the cut validates.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh};

/// The engine's sketch frame for a plane normal (`tangent_x_from_normal`):
/// `x = ref × n` with `ref = ẑ` unless |n_z| ≥ 0.99 (then `x̂`), `y = n × x`.
fn engine_frame(n: [f64; 3]) -> (Vector3, Vector3, Vector3) {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let n = [n[0] / len, n[1] / len, n[2] / len];
    let r = if n[2].abs() < 0.99 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let x = cross(r, n);
    let lx = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
    let x = [x[0] / lx, x[1] / lx, x[2] / lx];
    let y = cross(n, x);
    (
        Vector3::new(n[0], n[1], n[2]),
        Vector3::new(x[0], x[1], x[2]),
        Vector3::new(y[0], y[1], y[2]),
    )
}

fn diamond(origin: [f64; 3], normal: [f64; 3], r: f64) -> Profile {
    let (_, u, v) = engine_frame(normal);
    Profile::new(
        Point3::new(origin[0], origin[1], origin[2]),
        u,
        v,
        vec![
            Point2::new(r, 0.0),
            Point2::new(0.0, r),
            Point2::new(-r, 0.0),
            Point2::new(0.0, -r),
        ],
        vec![],
    )
    .expect("diamond profile")
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
fn a_cylinder_lateral_sheared_past_a_full_turn_re_enters_a_cut() {
    let mut a = BrepArena::new();
    // Op 1: diamond (r 0.7) on z = 0, extruded 0.8 along +z.
    let (n1, _, _) = engine_frame([0.0, 0.0, 1.0]);
    let b1 = extrude(
        &mut a,
        &diamond([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.7),
        n1,
        0.8,
    )
    .expect("boss 1")
    .solid;
    // Op 2: diamond (r 0.8) on the oblique plane through (0.3, −0.4, 0.7),
    // normal (−0.03, 0.6, 0.8), extruded 2.0 along the normal.
    let (n2, _, _) = engine_frame([-0.03, 0.6, 0.8]);
    let b2 = extrude(
        &mut a,
        &diamond([0.3, -0.4, 0.7], [-0.03, 0.6, 0.8], 0.8),
        n2,
        2.0,
    )
    .expect("boss 2")
    .solid;
    let body = boolean_op(&mut a, b1, b2, BoolOp::Union).expect("boss 1 ∪ boss 2");
    // Op 3: unit circle on the plane x = −0.6 through (−0.6, −0.53, 0.92),
    // extruded 1.4 along +x.
    let (n3, u3, v3) = engine_frame([1.0, 0.0, 0.0]);
    let circle = Profile::circle(
        Point3::new(-0.6, -0.53, 0.92),
        u3,
        v3,
        Point2::new(0.0, 0.0),
        1.0,
    )
    .expect("circle profile");
    let b3 = extrude(&mut a, &circle, n3, 1.4).expect("boss 3").solid;
    let body = boolean_op(&mut a, body, b3, BoolOp::Union).expect("body ∪ cylinder");
    let v_body = mesh_signed_volume(&tessellate(&a, body).expect("body tessellates"));
    assert!(v_body > 0.0);

    // Op 4: diamond (r 1.0) on the oblique plane through (−0.6, −1.0, 0.3),
    // normal (−0.2, −1, −0.1); a blind cut with no direction runs INTO the
    // material, against the normal.
    let (n4, _, _) = engine_frame([-0.2, -1.0, -0.1]);
    let into = Vector3::new(-n4.x(), -n4.y(), -n4.z());
    let cutter = extrude(
        &mut a,
        &diamond([-0.6, -1.0, 0.3], [-0.2, -1.0, -0.1], 1.0),
        into,
        2.0,
    )
    .expect("cutter")
    .solid;
    let out = boolean_op(&mut a, body, cutter, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("body − cutter failed: {e:?}"));
    let report = validate_solid(&a, out).expect("cut validates");
    assert_eq!(report.euler_lhs, report.euler_rhs);
    let v_out = mesh_signed_volume(&tessellate(&a, out).expect("cut tessellates"));
    assert!(
        v_out > 0.0 && v_out < v_body,
        "the cut removes a proper part of the body: {v_body} → {v_out}"
    );
}
