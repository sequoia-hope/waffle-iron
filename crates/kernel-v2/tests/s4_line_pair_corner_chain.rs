//! Stage-4 LINE-PAIR CORNER on a chained boolean (P0027's shape; spec
//! `yang_stage4_conic_triple_junction.md`, "Junction-map candidates — the
//! line pair", 2026-10-08).
//!
//! Two cylindrical bosses with perpendicular axes are unioned: A1 gains a
//! cylinder×cylinder CREASE. Then a box whose near face is a plane PARALLEL
//! to BOTH axes is subtracted: that face meets cylinder A in two GENERATORS
//! (ruling lines) and cylinder B in two more, all four in the same plane,
//! and an A-generator crosses a B-generator exactly where the crease pierces
//! the face — the corner `{plane, cyl_A, cyl_B}`.
//!
//! Before the fix that vertex carried two DIFFERENT exact-line records and
//! the Stage-4 line arm STOPped on the second one (`line_line_junction`,
//! "out of scope — loud STOP rather than silently overwriting") before the
//! triple block ever ran. RED without the `line_pair_corner` admission
//! (`YANG_LINE_PAIR_CORNER=0`, mutation-checked 2026-10-08), GREEN with it:
//! every corner is an exact output vertex on all three surfaces.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh, SolidId,
};

/// Cylinder A: axis ŷ through the origin, r 3, y ∈ [0, 10].
const A_R: f64 = 3.0;
const A_H: f64 = 10.0;
/// Cylinder B: axis ẑ through (−0.5, 5, ·), r 4, z ∈ [−12, 12] — clear
/// through A.
const B_R: f64 = 4.0;
const B_X: f64 = -0.5;
const B_Y: f64 = 5.0;
const B_HALF: f64 = 12.0;
/// The cutter: a box `x ∈ [X_CUT, 20]` spanning both bosses in y and z. Its
/// face `x = X_CUT` is parallel to ŷ AND ẑ, so it meets each cylinder in
/// generators; the crease crosses it at four corners.
const X_CUT: f64 = -2.6;

fn cyl_a(a: &mut BrepArena) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        Point2::new(0.0, 0.0),
        A_R,
    )
    .expect("cylinder A profile");
    extrude(a, &p, Vector3::new(0.0, 1.0, 0.0), A_H)
        .expect("cylinder A extrude")
        .solid
}

fn cyl_b(a: &mut BrepArena) -> SolidId {
    let p = Profile::circle(
        Point3::new(B_X, B_Y, -B_HALF),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        B_R,
    )
    .expect("cylinder B profile");
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), 2.0 * B_HALF)
        .expect("cylinder B extrude")
        .solid
}

fn cutter(a: &mut BrepArena) -> SolidId {
    // Profile in the plane x = X_CUT (u = ŷ, v = ẑ), extruded along +x̂.
    let p = Profile::new(
        Point3::new(X_CUT, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        vec![
            Point2::new(-2.0, -14.0),
            Point2::new(12.0, -14.0),
            Point2::new(12.0, 14.0),
            Point2::new(-2.0, 14.0),
        ],
        vec![],
    )
    .expect("cutter profile");
    extrude(a, &p, Vector3::new(1.0, 0.0, 0.0), 20.0 - X_CUT)
        .expect("cutter extrude")
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

/// Radial residual against cylinder A: `sqrt(x² + z²) − r_A`.
fn cyl_a_residual(p: [f64; 3]) -> f64 {
    (p[0] * p[0] + p[2] * p[2]).sqrt() - A_R
}

/// Radial residual against cylinder B: `sqrt((x − B_X)² + (y − B_Y)²) − r_B`.
fn cyl_b_residual(p: [f64; 3]) -> f64 {
    ((p[0] - B_X).powi(2) + (p[1] - B_Y).powi(2)).sqrt() - B_R
}

#[test]
fn a_box_face_parallel_to_both_axes_through_a_two_boss_crease_resolves_line_pair_corners() {
    let mut a = BrepArena::new();
    let ca = cyl_a(&mut a);
    let cb = cyl_b(&mut a);
    let bosses = boolean_op(&mut a, ca, cb, BoolOp::Union).expect("A ∪ B");
    validate_solid(&a, bosses).expect("two-boss union validates");
    let v_bosses = mesh_signed_volume(&tessellate(&a, bosses).expect("union tessellates"));

    let c = cutter(&mut a);
    let out = boolean_op(&mut a, bosses, c, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("(A ∪ B) − box failed: {e:?}"));
    let report = validate_solid(&a, out).expect("output validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.euler_lhs, report.euler_rhs);

    let mesh = tessellate(&a, out).expect("output tessellates");
    let vol = mesh_signed_volume(&mesh);
    assert!(
        vol > 0.0 && vol < v_bosses,
        "the cut keeps a proper part of the bosses: {v_bosses} → {vol}"
    );

    // The four exact corners: the crease `{x² + z² = r_A², (x − B_X)² +
    // (y − B_Y)² = r_B²}` on the plane x = X_CUT.
    let za = (A_R * A_R - X_CUT * X_CUT).sqrt();
    let yb = (B_R * B_R - (X_CUT - B_X).powi(2)).sqrt();
    let expected = [
        [X_CUT, B_Y - yb, -za],
        [X_CUT, B_Y - yb, za],
        [X_CUT, B_Y + yb, -za],
        [X_CUT, B_Y + yb, za],
    ];

    // Every output vertex on the cut face that is exactly on ONE cylinder and
    // chord-close to the OTHER is a crease corner: it must lie EXACTLY on
    // both (the triple Newton's exactness, not a chord vertex relocated onto
    // one generator's foot and left off the other cylinder by the sagitta).
    // The near-band 1e-3 is far below the generator spacing, so no interior
    // sample qualifies; all four corners must be present.
    let tol = 1e-9;
    let near = 1e-3;
    let mut found = [false; 4];
    for p in mesh.positions.chunks_exact(3) {
        let p = [p[0], p[1], p[2]];
        if (p[0] - X_CUT).abs() > tol {
            continue;
        }
        let ra = cyl_a_residual(p);
        let rb = cyl_b_residual(p);
        let on_a_near_b = ra.abs() <= tol && rb.abs() <= near;
        let on_b_near_a = rb.abs() <= tol && ra.abs() <= near;
        if !(on_a_near_b || on_b_near_a) {
            continue;
        }
        assert!(
            ra.abs() <= tol && rb.abs() <= tol,
            "corner {p:?} is on the cut face but off cylinder A by {ra:.3e} / cylinder B by {rb:.3e}"
        );
        for (k, e) in expected.iter().enumerate() {
            if (p[1] - e[1]).abs() <= tol && (p[2] - e[2]).abs() <= tol {
                found[k] = true;
            }
        }
    }
    assert_eq!(
        found,
        [true; 4],
        "all four crease corners on the cut face are exact output vertices: {found:?} (expected {expected:?})"
    );
}
