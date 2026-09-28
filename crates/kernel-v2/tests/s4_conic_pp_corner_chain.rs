//! Stage-4 CONIC × plane-pair CORNER on a chained boolean (P0004's shape;
//! spec `yang_stage4_conic_triple_junction.md`, "Junction-map candidates —
//! the conic × plane-pair corner", 2026-09-28).
//!
//! A 200° frustum tube (outer cone r 3 → 2 over x 0 → 1, inner r = 1 bore,
//! swept from +y through +z) is first cut by a slab whose face plane P1 (z = 2.2 + 0.3·x) is
//! oblique to the axis: A1 gains a planar face on P1 whose boundary against
//! the cone is a CONIC crease (not a circle — no cap rim, no Stage-1 rim
//! junction). Then a box extruded along d = (0.4, 1, 0)/|d| cuts through
//! that crease: each of its two lateral faces PB (normal (1, −0.4, 0)/|n|,
//! 21.8° from the axis, steeper than the 45° generators) meets the cone in
//! an ELLIPSE and meets P1 in an exact plane∩plane segment whose endpoint
//! on the cone is the corner {cone, P1, PB}.
//!
//! Before the fix that endpoint was in ONE Stage-4 curve map (the
//! cone-ellipse) plus the plane∩plane endpoint map — which counted zero
//! toward the triple block's `n_maps` unless the curve was a LINE (R0070's
//! admission) — so it fell to the ellipse arm and slid along the ellipse off
//! P1 by the chord error, and Stage 6 STOPped `s6-planar-loop-nonplanar`
//! (P0004: 1.2e-1 at scale 3.5). RED without the `pp_conic_corner`
//! admission (mutation-checked 2026-09-28), GREEN with it: both corners are
//! exact output vertices on all three surfaces.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, Profile, RenderMesh,
    SolidId,
};

const AXIS_O: Point3 = Point3::new(0.0, 0.0, 0.0);
const AXIS_D: Vector3 = Vector3::new(1.0, 0.0, 0.0);

/// P1: z − 0.3·x = 2.2, i.e. `n1 · p = d1` with `n1 = (−0.3, 0, 1)/|n1|`.
const P1_M: f64 = 0.3;
const P1_Z0: f64 = 2.2;

/// The second cutter's extrusion direction (un-normalized).
const D2: [f64; 3] = [0.4, 1.0, 0.0];
/// Its lateral-plane normal `u = d × z` (un-normalized): (1, −0.4, 0).
const U2: [f64; 3] = [1.0, -0.4, 0.0];
/// The box's half-width across `u` and its profile origin, chosen so the
/// two lateral planes `û·p = 0.45 ± 0.15` are each crossed ONCE by the
/// crease (the +y branch crosses 0.30, the −y branch 0.60).
const HALF_W: f64 = 0.15;
const O2: [f64; 3] = [-1.115, -4.0, 0.0];
const Z_LO: f64 = 1.5;
const Z_HI: f64 = 4.0;
const DEPTH2: f64 = 8.0;

fn norm(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The frustum tube: trapezoid `(axial, radial) = (0,1),(0,3),(1,2),(1,1)`
/// revolved 200° about x. Outer wall = cone r(x) = 3 − x.
fn frustum(a: &mut BrepArena) -> SolidId {
    let p = Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(0.0, 3.0),
            Point2::new(1.0, 2.0),
            Point2::new(1.0, 1.0),
        ],
        vec![],
    )
    .expect("trapezoid profile");
    // A PARTIAL revolve (the kv6c canonical 200°): the full-turn band with
    // a window is the documented KV14 Slice E sub-slice wall, and the
    // corners live at 47°–133° from +y, inside the sweep.
    revolve(a, &p, AXIS_O, AXIS_D, 200.0_f64.to_radians())
        .expect("partial frustum")
        .solid
}

/// The oblique slab: everything above P1 (z ≥ 2.2 + 0.3·x) over the
/// frustum's footprint. Profile in P1 (frame u ∥ (1, 0, 0.3), v = ŷ),
/// extruded along P1's normal.
fn slab(a: &mut BrepArena) -> SolidId {
    let u = norm([1.0, 0.0, P1_M]);
    let n = norm([-P1_M, 0.0, 1.0]);
    let p = Profile::new(
        Point3::new(0.0, 0.0, P1_Z0),
        Vector3::new(u[0], u[1], u[2]),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(-2.0, -4.0),
            Point2::new(3.0, -4.0),
            Point2::new(3.0, 4.0),
            Point2::new(-2.0, 4.0),
        ],
        vec![],
    )
    .expect("slab profile");
    extrude(a, &p, Vector3::new(n[0], n[1], n[2]), 3.0)
        .expect("slab extrude")
        .solid
}

/// The tilted box: rectangle `s ∈ [−HALF_W, HALF_W] × t ∈ [Z_LO, Z_HI]` in
/// the plane through `O2` spanned by `û` and `ẑ`, extruded along `d̂`.
fn tilted_box(a: &mut BrepArena) -> SolidId {
    let d = norm(D2);
    let u = norm(U2);
    let p = Profile::new(
        Point3::new(O2[0], O2[1], O2[2]),
        Vector3::new(u[0], u[1], u[2]),
        Vector3::new(0.0, 0.0, 1.0),
        vec![
            Point2::new(-HALF_W, Z_LO),
            Point2::new(HALF_W, Z_LO),
            Point2::new(HALF_W, Z_HI),
            Point2::new(-HALF_W, Z_HI),
        ],
        vec![],
    )
    .expect("tilted box profile");
    extrude(a, &p, Vector3::new(d[0], d[1], d[2]), DEPTH2)
        .expect("tilted box extrude")
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

/// Signed distance to P1 (unit normal).
fn p1_dist(p: [f64; 3]) -> f64 {
    let n = norm([-P1_M, 0.0, 1.0]);
    dot(n, p) - P1_Z0 * n[2]
}

/// Signed distances to the box's two lateral planes `û·p = û·O2 ± HALF_W`.
fn pb_dists(p: [f64; 3]) -> [f64; 2] {
    let u = norm(U2);
    let c = dot(u, O2);
    [dot(u, p) - (c - HALF_W), dot(u, p) - (c + HALF_W)]
}

/// Outer-cone residual: `sqrt(y² + z²) − (3 − x)`.
fn cone_residual(p: [f64; 3]) -> f64 {
    (p[1] * p[1] + p[2] * p[2]).sqrt() - (3.0 - p[0])
}

#[test]
fn tilted_box_through_an_oblique_cone_crease_resolves_conic_pp_corners() {
    let mut a = BrepArena::new();
    let f = frustum(&mut a);
    let s = slab(&mut a);
    let creased = boolean_op(&mut a, f, s, BoolOp::Subtract).expect("frustum − slab");
    validate_solid(&a, creased).expect("creased frustum validates");
    let v_creased = mesh_signed_volume(&tessellate(&a, creased).expect("creased tessellates"));

    // The fixture is authored so the crease exists: P1 crosses the outer
    // cone inside the tube's axial extent (the apex of the crease at
    // 3 − x = 2.2 + 0.3·x, x ≈ 0.615 ∈ (0, 1)).
    let x_apex = (3.0 - P1_Z0) / (1.0 + P1_M);
    assert!(x_apex > 0.0 && x_apex < 1.0, "crease apex x = {x_apex}");

    let b = tilted_box(&mut a);
    let out = boolean_op(&mut a, creased, b, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("creased frustum − tilted box failed: {e:?}"));
    let report = validate_solid(&a, out).expect("output validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(report.euler_lhs, report.euler_rhs);

    let mesh = tessellate(&a, out).expect("output tessellates");
    let vol = mesh_signed_volume(&mesh);
    assert!(
        vol > 0.0 && vol < v_creased,
        "the box removes material: {v_creased} → {vol}"
    );

    // Every output vertex on BOTH P1 and a lateral plane of the box is a
    // corner of the crease — it must lie on the cone too (all three
    // surfaces, the triple Newton's exactness), and there are exactly two.
    let tol = 1e-9;
    let mut corners = 0;
    let mut seen: Vec<[f64; 3]> = Vec::new();
    for p in mesh.positions.chunks_exact(3) {
        let p = [p[0], p[1], p[2]];
        if p1_dist(p).abs() > tol {
            continue;
        }
        // The P1 ∩ PB segment's OTHER end is the {x = 0 cap, P1, PB}
        // plane-triple corner — exact by construction, not on the cone.
        if p[0].abs() <= tol || (p[0] - 1.0).abs() <= tol {
            continue;
        }
        let [d_lo, d_hi] = pb_dists(p);
        if d_lo.abs() > tol && d_hi.abs() > tol {
            continue;
        }
        if seen.iter().any(|q| {
            (q[0] - p[0]).abs() <= tol && (q[1] - p[1]).abs() <= tol && (q[2] - p[2]).abs() <= tol
        }) {
            continue; // the same corner seen through another face's vertex
        }
        seen.push(p);
        corners += 1;
        let r = cone_residual(p);
        assert!(
            r.abs() <= 1e-9,
            "corner {p:?} is on P1 and a box face but off the cone by {r:.3e}"
        );
    }
    assert_eq!(
        corners, 2,
        "the fixture is authored with two conic × plane-pair corners; found {seen:?}"
    );
}
