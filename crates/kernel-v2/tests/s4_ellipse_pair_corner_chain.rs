//! Stage-4 COPLANAR ellipse-pair CORNER on a chained boolean (P0025's shape;
//! spec `yang_stage4_conic_triple_junction.md`, "Junction-map candidates —
//! the coplanar ellipse pair", 2026-10-08).
//!
//! Two cylindrical bosses of DIFFERENT radii and non-parallel axes are
//! unioned: A1 gains a cylinder×cylinder CREASE (a degree-4 curve, not a
//! conic). Then a slab whose two faces are oblique to BOTH axes is
//! intersected with A1: each slab face meets cylinder A in one ELLIPSE and
//! cylinder B in another ELLIPSE, in the SAME plane, and the two ellipses
//! cross exactly where the crease pierces that face — the corner
//! `{plane, cyl_A, cyl_B}`.
//!
//! Before the fix that vertex carried two DIFFERENT ellipse records, so
//! Stage 4 demoted it into the PR-KV9 ellipse×ellipse junction map, whose
//! closed form is `(plane₁ ∩ plane₂) ∩ cylinder` — and with ONE plane the
//! plane-pair line does not exist: `LocalRefinementRequired` at
//! `|n₁ × n₂| < MIN_FEATURE_SIZE` (P0025 v3; 14 of 32 seed-3 ERROR rows).
//! The triple block never scanned that map. RED without the
//! `ell_pair_corner` admission (`YANG_ELL_PAIR_CORNER=0`, mutation-checked
//! 2026-10-08), GREEN with it: every corner is an exact output vertex on
//! all three surfaces.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh, SolidId,
};

/// Cylinder A: axis ŷ through the origin, r 3, y ∈ [0, 10].
const A_R: f64 = 3.0;
const A_H: f64 = 10.0;
/// Cylinder B: axis d̂ = norm(0.5, 1, 0.3) through (2, 5, 1), r 4, extruded
/// ±12 along the axis so it runs clear through A.
const B_R: f64 = 4.0;
const B_D: [f64; 3] = [0.5, 1.0, 0.3];
const B_C: [f64; 3] = [2.0, 5.0, 1.0];
const B_HALF: f64 = 12.0;
/// The slab: `û · p ∈ [C − W, C + W]`, û oblique to both axes, filling the
/// bosses' extent across the other two directions.
const SLAB_U: [f64; 3] = [0.53, -0.25, -0.81];
const SLAB_C: f64 = 0.4;
const SLAB_W: f64 = 1.2;

fn norm(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
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

/// An orthonormal pair spanning the plane ⊥ `d̂`.
fn frame(d: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let seed = if d[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = norm(cross(seed, d));
    let v = cross(d, u);
    (u, v)
}

fn v3(a: [f64; 3]) -> Vector3 {
    Vector3::new(a[0], a[1], a[2])
}

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
    let d = norm(B_D);
    let (u, v) = frame(d);
    let base = [
        B_C[0] - B_HALF * d[0],
        B_C[1] - B_HALF * d[1],
        B_C[2] - B_HALF * d[2],
    ];
    let p = Profile::circle(
        Point3::new(base[0], base[1], base[2]),
        v3(u),
        v3(v),
        Point2::new(0.0, 0.0),
        B_R,
    )
    .expect("cylinder B profile");
    extrude(a, &p, v3(d), 2.0 * B_HALF)
        .expect("cylinder B extrude")
        .solid
}

fn slab(a: &mut BrepArena) -> SolidId {
    let u = norm(SLAB_U);
    let (s, t) = frame(u);
    let o = [
        (SLAB_C - SLAB_W) * u[0],
        (SLAB_C - SLAB_W) * u[1],
        (SLAB_C - SLAB_W) * u[2],
    ];
    let p = Profile::new(
        Point3::new(o[0], o[1], o[2]),
        v3(s),
        v3(t),
        vec![
            Point2::new(-40.0, -40.0),
            Point2::new(40.0, -40.0),
            Point2::new(40.0, 40.0),
            Point2::new(-40.0, 40.0),
        ],
        vec![],
    )
    .expect("slab profile");
    extrude(a, &p, v3(u), 2.0 * SLAB_W)
        .expect("slab extrude")
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

/// Radial residual against cylinder B.
fn cyl_b_residual(p: [f64; 3]) -> f64 {
    let d = norm(B_D);
    let w = [p[0] - B_C[0], p[1] - B_C[1], p[2] - B_C[2]];
    let h = dot(w, d);
    let r = [w[0] - h * d[0], w[1] - h * d[1], w[2] - h * d[2]];
    (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt() - B_R
}

/// Signed distances to the slab's two faces.
fn slab_dists(p: [f64; 3]) -> [f64; 2] {
    let u = norm(SLAB_U);
    [dot(u, p) - (SLAB_C - SLAB_W), dot(u, p) - (SLAB_C + SLAB_W)]
}

#[test]
fn an_oblique_slab_through_a_two_boss_crease_resolves_coplanar_ellipse_corners() {
    let mut a = BrepArena::new();
    let ca = cyl_a(&mut a);
    let cb = cyl_b(&mut a);
    let bosses = boolean_op(&mut a, ca, cb, BoolOp::Union).expect("A ∪ B");
    validate_solid(&a, bosses).expect("two-boss union validates");
    let v_bosses = mesh_signed_volume(&tessellate(&a, bosses).expect("union tessellates"));

    let s = slab(&mut a);
    let out = boolean_op(&mut a, bosses, s, BoolOp::Intersect)
        .unwrap_or_else(|e| panic!("(A ∪ B) ∩ slab failed: {e:?}"));
    let report = validate_solid(&a, out).expect("output validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.euler_lhs, report.euler_rhs);

    let mesh = tessellate(&a, out).expect("output tessellates");
    let vol = mesh_signed_volume(&mesh);
    assert!(
        vol > 0.0 && vol < v_bosses,
        "the slab keeps a proper part of the bosses: {v_bosses} → {vol}"
    );

    // Every output vertex on a slab face that is exactly on ONE cylinder and
    // chord-close to the OTHER is a crease corner: it must lie EXACTLY on
    // both (all three surfaces — the triple Newton's exactness, not a chord
    // vertex relocated onto one ellipse and left off the other by the
    // sagitta). The near-band 1e-3 is far below the ellipse chord spacing at
    // r 3–4, so no interior ellipse sample qualifies; the crease is OPEN
    // (it runs off A's caps), so its crossing count with the two faces need
    // not be even — the fixture is authored so at least one exists.
    let tol = 1e-9;
    let near = 1e-3;
    let mut corners = 0;
    let mut seen: Vec<[f64; 3]> = Vec::new();
    for p in mesh.positions.chunks_exact(3) {
        let p = [p[0], p[1], p[2]];
        let [d_lo, d_hi] = slab_dists(p);
        if d_lo.abs() > tol && d_hi.abs() > tol {
            continue;
        }
        let ra = cyl_a_residual(p);
        let rb = cyl_b_residual(p);
        let on_a_near_b = ra.abs() <= tol && rb.abs() <= near;
        let on_b_near_a = rb.abs() <= tol && ra.abs() <= near;
        if !(on_a_near_b || on_b_near_a) {
            continue;
        }
        if seen.iter().any(|q| {
            (q[0] - p[0]).abs() <= tol && (q[1] - p[1]).abs() <= tol && (q[2] - p[2]).abs() <= tol
        }) {
            continue;
        }
        seen.push(p);
        corners += 1;
        assert!(
            ra.abs() <= tol && rb.abs() <= tol,
            "corner {p:?} is on a slab face but off cylinder A by {ra:.3e} / cylinder B by {rb:.3e}"
        );
    }
    assert!(
        corners >= 1,
        "the fixture is authored with a crease corner on a slab face; found none"
    );
}
