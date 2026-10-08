//! Stage-4 §4.5.3 reversal at a SOLID-EDGE corner (P0028's shape, 2026-10-08).
//!
//! A diamond prism (a square rotated 45°, "radius" 21, height 34 along +z)
//! is cut by a tilted cylinder (r 24) whose wall pierces the prism's own
//! top edge — the edge where the cap z = 34 meets the side face
//! −x + y = 21. The intersection curve on the cylinder is a cap-ellipse arc
//! and a side-ellipse arc meeting at the corner J = {cap, side, cylinder},
//! which Stage 1 mints exactly and Stage 4 relocates the arrangement's
//! edge-crossing vertex onto in closed form.
//!
//! The cap ellipse's last chord vertex before J is relocated by its nearest
//! point on the ellipse, which lies PAST J (outside the side plane): the
//! sequence (p_b, p_r, J) reverses at p_r — Yang §4.5.3 — and the sweep
//! must remove the overshooting p_r. Before the fix the sweep collapsed J
//! into p_r instead (J's far edge on the cylinder loop reads as a DIFFERENT
//! curve only by payload equality, which the junction-protected direction
//! rule compares; the solid-edge corner is a junction by INCIDENCE), so
//! the output cap loop carried a vertex 3.6e-1 off the side plane and
//! Stage 6 STOPped `s6-planar-loop-nonplanar`.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh, SolidId,
};

/// The prism: a diamond |x| + |y| ≤ R_BOX on z = 0, extruded H_BOX along +z.
const R_BOX: f64 = 21.0;
const H_BOX: f64 = 34.0;
/// The cutter: circle r = R_CUT on the plane through O_CUT with normal
/// N_CUT (as authored — NOT unit length; the engine normalizes), swept
/// DEPTH_CUT along −n̂ (the engine's auto-reverse toward the material).
const R_CUT: f64 = 24.0;
const O_CUT: [f64; 3] = [-0.87, -24.0, -5.1];
const N_CUT: [f64; 3] = [-0.2, -0.46, -0.87];
const DEPTH_CUT: f64 = 120.0;

fn norm(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn prism(a: &mut BrepArena) -> SolidId {
    // The sketch basis the engine derives for a +z plane: x = X̂ × ẑ = −ŷ,
    // y = ẑ × x = x̂ (a diamond is symmetric under it, kept for fidelity).
    let p = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        vec![
            Point2::new(R_BOX, 0.0),
            Point2::new(0.0, R_BOX),
            Point2::new(-R_BOX, 0.0),
            Point2::new(0.0, -R_BOX),
        ],
        vec![],
    )
    .expect("diamond profile");
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), H_BOX)
        .expect("prism extrude")
        .solid
}

fn cutter(a: &mut BrepArena) -> SolidId {
    let n = norm(N_CUT);
    // `SketchPlaneBasis::from_origin_normal`: reference ẑ (|n·ẑ| < 0.99),
    // x = ẑ × n, y = n × x.
    let x = norm(cross([0.0, 0.0, 1.0], n));
    let y = norm(cross(n, x));
    let p = Profile::circle(
        Point3::new(O_CUT[0], O_CUT[1], O_CUT[2]),
        Vector3::new(x[0], x[1], x[2]),
        Vector3::new(y[0], y[1], y[2]),
        Point2::new(0.0, 0.0),
        R_CUT,
    )
    .expect("cutter profile");
    extrude(a, &p, Vector3::new(-n[0], -n[1], -n[2]), DEPTH_CUT)
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

/// Radial residual against the cutter: distance from the axis minus r.
fn cyl_residual(p: [f64; 3]) -> f64 {
    let n = norm(N_CUT);
    let v = [p[0] - O_CUT[0], p[1] - O_CUT[1], p[2] - O_CUT[2]];
    let t = dot(v, n);
    let q = [v[0] - t * n[0], v[1] - t * n[1], v[2] - t * n[2]];
    dot(q, q).sqrt() - R_CUT
}

/// Signed distance from the side plane −x + y = R_BOX (unit normal).
fn side_residual(p: [f64; 3]) -> f64 {
    (-p[0] + p[1] - R_BOX) / std::f64::consts::SQRT_2
}

/// The exact corners: the top edge `(−R_BOX + s, s, H_BOX)`, s ∈ [0, R_BOX],
/// meets the cylinder where `cyl_residual` vanishes — a quadratic in s.
fn exact_corners() -> Vec<[f64; 3]> {
    let n = norm(N_CUT);
    let p0 = [-R_BOX - O_CUT[0], -O_CUT[1], H_BOX - O_CUT[2]];
    let d = [1.0, 1.0, 0.0];
    // |q(s)|² = r² with q = w − (w·n)n, w = p0 + s·d.
    let strip = |w: [f64; 3]| {
        let t = dot(w, n);
        [w[0] - t * n[0], w[1] - t * n[1], w[2] - t * n[2]]
    };
    let q0 = strip(p0);
    let qd = strip(d);
    let (a, b, c) = (dot(qd, qd), 2.0 * dot(q0, qd), dot(q0, q0) - R_CUT * R_CUT);
    let disc = b * b - 4.0 * a * c;
    assert!(disc > 0.0, "the cutter crosses the top edge: disc={disc}");
    let mut out = Vec::new();
    for s in [
        (-b - disc.sqrt()) / (2.0 * a),
        (-b + disc.sqrt()) / (2.0 * a),
    ] {
        if (0.0..=R_BOX).contains(&s) {
            out.push([-R_BOX + s, s, H_BOX]);
        }
    }
    out
}

#[test]
fn a_tilted_cylinder_through_a_prism_edge_keeps_the_edge_corner_exact() {
    let mut a = BrepArena::new();
    let p = prism(&mut a);
    let v_prism = mesh_signed_volume(&tessellate(&a, p).expect("prism tessellates"));
    let c = cutter(&mut a);
    let out = boolean_op(&mut a, p, c, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("prism − cylinder failed: {e:?}"));
    let report = validate_solid(&a, out).expect("output validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.euler_lhs, report.euler_rhs);

    let mesh = tessellate(&a, out).expect("output tessellates");
    let vol = mesh_signed_volume(&mesh);
    assert!(
        vol > 0.0 && vol < v_prism,
        "the cut keeps a proper part of the prism: {v_prism} → {vol}"
    );

    let expected = exact_corners();
    assert_eq!(
        expected.len(),
        1,
        "the cutter crosses the top edge once: {expected:?}"
    );

    // Every output vertex on the cap that is exactly on the cylinder and
    // chord-close to the side plane is the corner: it must lie EXACTLY on
    // the side plane (the minted junction), not be the overshooting chord
    // vertex that the §4.5.3 sweep kept in its place. The near-band is far
    // below the ellipse's sample spacing (≈1 here), so no honest sample of
    // the arc qualifies; the overshooting phantom sat 3.6e-1 off.
    let tol = 1e-9;
    let near = 1e-3;
    // No cap vertex may sit OUTSIDE the prism's side plane at all: the
    // phantom chord vertices beyond the corner are exactly what §4.5.3
    // removes.
    for p in mesh.positions.chunks_exact(3) {
        let p = [p[0], p[1], p[2]];
        if (p[2] - H_BOX).abs() <= tol {
            assert!(
                side_residual(p) <= tol,
                "cap vertex {p:?} lies outside the side plane by {:.3e}",
                side_residual(p)
            );
        }
    }
    let mut found = false;
    for p in mesh.positions.chunks_exact(3) {
        let p = [p[0], p[1], p[2]];
        if (p[2] - H_BOX).abs() > tol {
            continue;
        }
        let rc = cyl_residual(p);
        let rs = side_residual(p);
        if !(rc.abs() <= tol && rs.abs() <= near) {
            continue;
        }
        assert!(
            rs.abs() <= tol,
            "cap vertex {p:?} is on the cylinder but off the side plane by {rs:.3e}"
        );
        let e = expected[0];
        if (p[0] - e[0]).abs() <= tol && (p[1] - e[1]).abs() <= tol {
            found = true;
        }
    }
    assert!(
        found,
        "the edge corner {:?} is an exact output vertex",
        expected[0]
    );
}
