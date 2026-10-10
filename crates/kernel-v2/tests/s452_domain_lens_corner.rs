//! Yang §4.5.2, the LOCAL form of the boundary-point domain ladder
//! (P0031's shape, 2026-10-10; spec
//! `specs/yang_45_boundary_point_domain_certificate.md` §8).
//!
//! A cylinder boss (r 10, height 8 along +z) is unioned with a box that
//! bites its right side, so the chained operand's cylinder is an
//! ARC-BOUNDED strip. A cut is then sketched on the plane x = −9 — inside
//! the cylinder by 1.0, where the plane meets it at y = ±4.359 — and
//! extruded +x through the body, so that plane is the cut's START CAP.
//! One edge of the cut profile is a line through (y, z) = (−4.30, 0) with
//! slope −1.3: it crosses the floor z = 0 at y = −4.30, 0.059 inside the
//! cap∩cylinder line, and that line at z = 0.0767.
//!
//! Four surfaces therefore meet within 0.08 of one point — the cylinder,
//! the cap plane, the floor, and the inclined cut face — and the exact
//! result has two triple corners there: C = {cylinder, cut face, cap} at
//! z = 0.0767 and T₃ = {cut face, cap, floor} on the floor, joined by a
//! 0.097 edge of the cap face. The cylinder's natural chord sagitta (N 13
//! on r 10, ≈ 0.29) is several times that clearance, so the chord mesh
//! resolves the corner as the OTHER pair of triples — {cylinder, cut face,
//! floor} 0.02 beyond the cap, and {cylinder, cap, floor} — and Stage 4
//! faithfully completes both wrong crossings on the extended surfaces: the
//! §4.5 certificate fires twice, the fixed body-wide ladder (d_ε/2, d_ε/4)
//! cannot clear them, and the natural output's cylinder patch pierces its
//! cap face (`SelfIntersectingBooleanOutput`, P0031's text).
//!
//! The local ladder clears it at its first rung: the fires name A's
//! cylinder, and three extra samples per arc of its strip — apex-centred on
//! the fire's azimuth — put a chord vertex at the corner. RED with
//! `YANG_452_LOCAL=off` (mutation-checked 2026-10-10: this fixture's wrong
//! topology is caught one gate earlier than P0031's, `TessellationFailed
//! "ring rejected by CDT (degenerate/self-intersecting)"` on the output's
//! cap face), GREEN with it.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh, SolidId,
};

const R_CYL: f64 = 10.0;
const H_CYL: f64 = 8.0;
/// The cut's start cap plane `x = X_CAP`.
const X_CAP: f64 = -9.0;
/// Where the inclined cut edge crosses the floor (y), and its slope dz/dy.
const Y_FLOOR: f64 = -4.30;
const SLOPE: f64 = -1.3;

fn cylinder(a: &mut BrepArena) -> SolidId {
    // The sketch basis the engine derives for a +z plane: x = −ŷ, y = x̂.
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Point2::new(0.0, 0.0),
        R_CYL,
    )
    .expect("circle profile");
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), H_CYL)
        .expect("cylinder extrude")
        .solid
}

/// The box boss: x ∈ [2, 16], y ∈ [−6, 6], z ∈ [−1, 12]. In the +z sketch
/// basis (u = −y, v = x): u ∈ [−6, 6], v ∈ [2, 16].
fn bite(a: &mut BrepArena) -> SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, -1.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        vec![
            Point2::new(-6.0, 2.0),
            Point2::new(6.0, 2.0),
            Point2::new(6.0, 16.0),
            Point2::new(-6.0, 16.0),
        ],
        vec![],
    )
    .expect("box profile");
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), 13.0)
        .expect("box extrude")
        .solid
}

/// z on the inclined cut edge at `y`.
fn edge_z(y: f64) -> f64 {
    SLOPE * (y - Y_FLOOR)
}

/// The cutter, sketched on `x = X_CAP` (basis u = ŷ, v = ẑ) and extruded
/// +x through the body. The profile's first edge is the inclined line; the
/// rest stays clear of every other face of the body (its bottom z = −6.89
/// is below the box's −1, its top z = 6 below the cylinder's 8, its sides
/// y = −5.5 / 8 off the box's ±6).
fn cutter(a: &mut BrepArena) -> SolidId {
    let p = Profile::new(
        Point3::new(X_CAP, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        vec![
            Point2::new(-5.5, edge_z(-5.5)),
            Point2::new(1.0, edge_z(1.0)),
            Point2::new(8.0, edge_z(1.0)),
            Point2::new(8.0, 6.0),
            Point2::new(-5.5, 6.0),
        ],
        vec![],
    )
    .expect("cutter profile");
    extrude(a, &p, Vector3::new(1.0, 0.0, 0.0), 30.0)
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

fn dist(p: &[f64], q: [f64; 3]) -> f64 {
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

#[test]
fn a_quadruple_corner_inside_the_chord_sagitta_resolves_to_the_exact_triple_pair() {
    // The exact corners the fixture is built around.
    let y_cap = -(R_CYL * R_CYL - X_CAP * X_CAP).sqrt(); // cap ∩ cylinder, y < 0
    let corner_c = [X_CAP, y_cap, edge_z(y_cap)]; // {cylinder, cut face, cap}
    let corner_t3 = [X_CAP, Y_FLOOR, 0.0]; // {cut face, cap, floor}
    let clearance = dist(&corner_c, corner_t3);
    assert!(
        clearance > 0.05 && clearance < 0.15,
        "the two exact corners are a hair apart: {clearance}"
    );
    let sag = R_CYL * (1.0 - (std::f64::consts::PI / 13.0).cos());
    assert!(
        sag > 2.0 * clearance,
        "the natural chord sagitta ({sag}) dwarfs the corner's clearance ({clearance})"
    );

    let mut a = BrepArena::new();
    let c = cylinder(&mut a);
    let b = bite(&mut a);
    let body = boolean_op(&mut a, c, b, BoolOp::Union).expect("cylinder ∪ box");
    let v_body = mesh_signed_volume(&tessellate(&a, body).expect("body tessellates"));
    let k = cutter(&mut a);
    let out = boolean_op(&mut a, body, k, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("(cylinder ∪ box) − cut failed: {e:?}"));
    let report = validate_solid(&a, out).expect("output validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.euler_lhs, report.euler_rhs);

    let mesh = tessellate(&a, out).expect("output tessellates");
    let vol = mesh_signed_volume(&mesh);
    assert!(
        vol > 0.0 && vol < v_body,
        "the cut removes a proper part of the body: {v_body} → {vol}"
    );

    // The defect's own quantity: the output carries BOTH exact corners —
    // the chord mesh had neither (it built the other pair of triples).
    let has = |q: [f64; 3]| mesh.positions.chunks_exact(3).any(|p| dist(p, q) <= 1e-6);
    assert!(
        has(corner_c),
        "the {{cylinder, cut face, cap}} corner {corner_c:?} is an output vertex"
    );
    assert!(
        has(corner_t3),
        "the {{cut face, cap, floor}} corner {corner_t3:?} is an output vertex"
    );

    // And nothing of the output lies strictly inside the cutter's
    // footprint on its start cap: every output vertex on x = X_CAP with
    // |y| below the cap∩cylinder line is on or below the inclined edge's
    // exterior side or on the floor — the sliver the wrong topology kept
    // (between the floor, the cap line and the inclined face) is cut away.
    for p in mesh.positions.chunks_exact(3) {
        if (p[0] - X_CAP).abs() > 1e-9 {
            continue;
        }
        if p[1] < y_cap - 1e-9 || p[1] > -Y_FLOOR {
            continue;
        }
        let z_edge = edge_z(p[1]);
        assert!(
            p[2] <= z_edge + 1e-9 || p[2] >= -1e-9,
            "cap vertex {p:?} sits inside the cutter's profile"
        );
    }
}
