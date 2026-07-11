//! KV16 — hyperbola-arc boundary vocabulary + re-entry
//! (spec `kv16_hyperbola_arc_vocabulary`, N2/R0017 epic increment 6).
//!
//! A box whose side plane is PARALLEL to a cone's axis sections the lateral
//! in a hyperbola branch arc (the axis-steep conic case, [#1] Patrikalakis
//! Ch.5). The union output must assemble, validate, and render with the
//! exact `Curve::HyperbolaArc` vocabulary — and RE-ENTER a second boolean
//! (the R0017 chain shape: the corpus case cuts the hyperbola-bounded
//! auto-union output).

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, Profile, RenderMesh,
};

fn boxx(a: &mut BrepArena, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> kernel_v2::SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x.0, y.0),
            Point2::new(x.1, y.0),
            Point2::new(x.1, y.1),
            Point2::new(x.0, y.1),
        ],
        vec![],
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .unwrap()
        .solid
}

/// Full-turn frustum about +z: base disc r=2 at z=0, top disc r=1 at z=2
/// (apex would sit at z=4; tan α = 1/2). Volume = (π/3)·2·(4+2+1) = 14π/3.
fn frustum(a: &mut BrepArena) -> kernel_v2::SolidId {
    let prof = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0), // profile x = axial (z)
        Vector3::new(1.0, 0.0, 0.0), // profile y = radial
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(0.0, 2.0),
            Point2::new(2.0, 1.0),
            Point2::new(2.0, 0.0),
        ],
        vec![],
    )
    .unwrap();
    revolve(
        a,
        &prof,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0 * std::f64::consts::PI,
    )
    .expect("full-turn frustum revolve")
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

fn volume_of(a: &BrepArena, s: kernel_v2::SolidId) -> f64 {
    mesh_signed_volume(&tessellate(a, s).expect("tessellate"))
}

/// Circular-segment area of the disc radius `r` beyond the chord line at
/// distance `d` from the center (zero when the line clears the disc).
fn segment_area(r: f64, d: f64) -> f64 {
    if r <= d {
        return 0.0;
    }
    r * r * (d / r).acos() - d * (r * r - d * d).sqrt()
}

/// Simpson integral of the frustum∩box overlap: slices ⊥ the cone axis are
/// circular segments beyond the box's near plane x = d (the box covers the
/// full segment in y and x, verified by construction below).
fn overlap_volume(d: f64) -> f64 {
    // r(z) = 2 − z/2 ≥ d  ⇔  z ≤ 2(2 − d); the frustum ends at z = 2.
    let z_hi = (2.0 * (2.0 - d)).min(2.0);
    let n = 4000usize; // even
    let hstep = z_hi / n as f64;
    let f = |z: f64| segment_area(2.0 - z / 2.0, d);
    let mut sum = f(0.0) + f(z_hi);
    for k in 1..n {
        let w = if k % 2 == 1 { 4.0 } else { 2.0 };
        sum += w * f(k as f64 * hstep);
    }
    sum * hstep / 3.0
}

/// Frustum ∪ box whose near plane x = d is ∥ the axis (d < 2 = base radius
/// ⇒ the plane sections the lateral in hyperbola arcs). `d > 1` (the top
/// radius) bites the BOTTOM rim only (the surviving lateral keeps its full
/// top rim — a wrapping-loop patch); `d < 1` bites THROUGH both rims (the
/// surviving lateral is a single non-wrapping azimuth patch). The box spans
/// the full overlap in y (|y| ≤ √(4−d²) < 2.4), x (≤ 2 < 3.2) and z
/// ([0,2] ⊂ [−0.6, 2.6]); NO coplanar face pairs with the frustum.
fn union_fixture(a: &mut BrepArena, d: f64) -> (kernel_v2::SolidId, f64) {
    let fr = frustum(a);
    let bx = boxx(a, (d, 3.2), (-2.4, 2.4), (-0.6, 2.6));
    let out = boolean_op(a, fr, bx, BoolOp::Union)
        .unwrap_or_else(|e| panic!("frustum ∪ axis-parallel box (d={d}): {e:?}"));
    let expect = 14.0 * std::f64::consts::PI / 3.0 + (3.2 - d) * 4.8 * 3.2 - overlap_volume(d);
    (out, expect)
}

/// The union output assembles, validates, CARRIES the HyperbolaArc
/// vocabulary, and renders to the analytic volume.
#[test]
fn hyperbola_bounded_union_validates_and_renders() {
    let mut a = BrepArena::new();
    // d = 1.2 > top radius: bottom-rim bite — the surviving cone lateral
    // keeps its full top rim (wrapping-loop patch topology).
    let (out, expect_v) = union_fixture(&mut a, 1.2);
    validate_solid(&a, out).expect("hyperbola-bounded union validates");

    // The wall was the hyperbola vocabulary itself: the output must
    // actually CARRY HyperbolaArc edges (else this test pins nothing —
    // the KV14 lesson).
    let hyperbola_half_edges = a
        .half_edges
        .iter()
        .flatten()
        .filter(|h| matches!(h.curve, kernel_v2::Curve::HyperbolaArc { .. }))
        .count();
    assert!(
        hyperbola_half_edges >= 2,
        "expected HyperbolaArc edges on the section boundary, found {hyperbola_half_edges}"
    );

    let v = volume_of(&a, out);
    assert!(
        (v - expect_v).abs() <= 0.01 * expect_v,
        "union volume {v} vs analytic {expect_v}"
    );
}

/// R0017's chain shape: a second boolean RE-ENTERS the hyperbola-bounded
/// body. The notch overlaps only the box's far corner (pure-planar region,
/// x ≥ 2.7 > 2 clears the cone), so the decrement is exact.
#[test]
fn hyperbola_bounded_reentry_chain() {
    let mut a = BrepArena::new();
    // d = 0.8 < top radius: the bite goes THROUGH both rims, so the
    // surviving cone lateral is a single non-wrapping azimuth patch —
    // re-enterable through the KV14 unroll+CDT path. (The d > 1
    // wrapping-loop shape re-enters only when the KV14 Slice E cone
    // periodic strip lands — a separate, typed wall.)
    let (out, expect_v) = union_fixture(&mut a, 0.8);
    let v1 = volume_of(&a, out);
    assert!(
        (v1 - expect_v).abs() <= 0.01 * expect_v,
        "through-bite union volume {v1} vs analytic {expect_v}"
    );

    let notch = boxx(&mut a, (2.7, 3.7), (1.9, 2.9), (-1.1, 0.5));
    let cut = boolean_op(&mut a, out, notch, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("re-enter hyperbola-bounded body: {e:?}"));
    validate_solid(&a, cut).expect("re-entered result validates");

    // Overlap = [2.7,3.2]×[1.9,2.4]×[−0.6,0.5] → 0.5·0.5·1.1 = 0.275.
    let v2 = volume_of(&a, cut);
    assert!(
        (v1 - v2 - 0.275).abs() < 0.01,
        "notch decrement {} must be ≈0.275: v1={v1} v2={v2}",
        v1 - v2
    );
}
