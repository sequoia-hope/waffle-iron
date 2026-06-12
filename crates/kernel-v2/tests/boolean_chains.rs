//! PR-CHAIN — multi-boolean chains on kernel-v2 (2+ booleans per body).
//!
//! The corpus has ZERO ≥3-op SUPPORTED_CORRECT cases, but that is because
//! its multi-op cases hit OTHER walls first (curved profiles, M8 coplanar
//! stacked extrudes, KV6c/d revolves) — NOT because chaining is broken.
//! These tests pin the actual chain capability directly:
//!
//! - PLANAR chains of arbitrary depth work (union/subtract mixes, 4 deep),
//!   with exact volume oracles (axis-aligned boxes → inclusion–exclusion).
//! - PR-KV7: re-entering a boolean OUTPUT that carries a CURVED face now
//!   WORKS — output curve recovery (`recover.rs`) restores B-Rep
//!   granularity, so the chain continues through curved intermediates
//!   (the former `UnsupportedCurvedBoolean` re-entry wall).

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh};

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

fn cyl(a: &mut BrepArena, cx: f64, cy: f64, r: f64, z: (f64, f64)) -> kernel_v2::SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(cx, cy),
        r,
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .unwrap()
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

const VOL_TOL: f64 = 1e-9;

/// Two-deep union chain over a 3-box staircase (no coplanar contact).
/// Exact volume by inclusion–exclusion of axis-aligned boxes.
#[test]
fn planar_union_union_chain() {
    let mut a = BrepArena::new();
    let b1 = boxx(&mut a, (0.0, 2.0), (0.0, 2.0), (0.0, 2.0));
    let b2 = boxx(&mut a, (1.0, 3.0), (1.0, 3.0), (0.5, 2.5));
    let b3 = boxx(&mut a, (2.0, 4.0), (2.0, 4.0), (1.0, 3.0));
    let u1 = boolean_op(&mut a, b1, b2, BoolOp::Union).expect("first union");
    let out = boolean_op(&mut a, u1, b3, BoolOp::Union).expect("second union");
    validate_solid(&a, out).expect("validates");
    // |B1∪B2∪B3| = Σ|Bi| − Σ|Bi∩Bj| + |B1∩B2∩B3|
    // B1∩B2 = [1,2]×[1,2]×[0.5,2] (1.5); B2∩B3 = [2,3]×[2,3]×[1,2.5] (1.5);
    // B1∩B3 = ∅ (x only touches at 2 with B1 closed at 2 → measure 0);
    // triple = ∅.
    let expect = 8.0 + 8.0 + 8.0 - 1.5 - 1.5;
    assert!(
        (volume_of(&a, out) - expect).abs() < VOL_TOL,
        "vol {} vs {expect}",
        volume_of(&a, out)
    );
}

/// Union then subtract on the union output.
#[test]
fn planar_union_then_subtract() {
    let mut a = BrepArena::new();
    let b1 = boxx(&mut a, (0.0, 2.0), (0.0, 2.0), (0.0, 2.0));
    let b2 = boxx(&mut a, (1.0, 3.0), (1.0, 3.0), (0.5, 2.5));
    let b3 = boxx(&mut a, (0.5, 1.5), (0.5, 1.5), (-0.5, 1.0));
    let u1 = boolean_op(&mut a, b1, b2, BoolOp::Union).expect("union");
    let out = boolean_op(&mut a, u1, b3, BoolOp::Subtract).expect("subtract");
    validate_solid(&a, out).expect("validates");
    // |U| = 8 + 8 − 1.5 = 14.5. Cut removes B3∩U: B3∩B1 =
    // [0.5,1.5]×[0.5,1.5]×[0,1] (1.0) plus B3∩B2∖B1 = x,y∈[1,1.5]² z∈[0.5,1]
    // already inside B1 → nothing extra. Removed = 1.0.
    let expect = 14.5 - 1.0;
    assert!(
        (volume_of(&a, out) - expect).abs() < VOL_TOL,
        "vol {} vs {expect}",
        volume_of(&a, out)
    );
}

/// Two pockets cut sequentially from one slab.
#[test]
fn planar_subtract_subtract_two_pockets() {
    let mut a = BrepArena::new();
    let b1 = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 2.0));
    let p1 = boxx(&mut a, (0.5, 1.5), (0.5, 1.5), (1.0, 2.5));
    let p2 = boxx(&mut a, (2.5, 3.5), (2.5, 3.5), (1.0, 2.5));
    let s1 = boolean_op(&mut a, b1, p1, BoolOp::Subtract).expect("first cut");
    let out = boolean_op(&mut a, s1, p2, BoolOp::Subtract).expect("second cut");
    validate_solid(&a, out).expect("validates");
    let expect = 32.0 - 1.0 - 1.0; // each pocket removes 1×1×1 of slab
    assert!(
        (volume_of(&a, out) - expect).abs() < VOL_TOL,
        "vol {} vs {expect}",
        volume_of(&a, out)
    );
}

/// Four booleans deep: union, subtract, union, subtract.
#[test]
fn planar_four_boolean_chain() {
    let mut a = BrepArena::new();
    let b1 = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 2.0));
    let b2 = boxx(&mut a, (3.0, 6.0), (1.0, 3.0), (0.5, 2.5));
    let p1 = boxx(&mut a, (0.5, 1.5), (0.5, 1.5), (1.0, 2.5));
    let b3 = boxx(&mut a, (-1.0, 0.5), (0.5, 2.0), (0.3, 1.7));
    let p2 = boxx(&mut a, (4.0, 5.0), (1.5, 2.5), (1.2, 2.8));
    let s = boolean_op(&mut a, b1, b2, BoolOp::Union).expect("op1 union");
    let s = boolean_op(&mut a, s, p1, BoolOp::Subtract).expect("op2 subtract");
    let s = boolean_op(&mut a, s, b3, BoolOp::Union).expect("op3 union");
    let out = boolean_op(&mut a, s, p2, BoolOp::Subtract).expect("op4 subtract");
    validate_solid(&a, out).expect("validates");
    // op1: 32 + 12 − |[3,4]×[1,3]×[0.5,2]| (3) = 41
    // op2: − |p1∩body| = 1×1×1 = 40
    // op3: + 12·... |b3| = 1.5×1.5×1.4 = 3.15; b3∩body = [0,0.5]×[0.5,2]×[0.3,1.7] = 0.5·1.5·1.4 = 1.05 → 40+3.15−1.05 = 42.1
    // op4: p2∩body = [4,5]×[1.5,2.5]×[1.2,2.5] = 1·1·1.3 = 1.3 → 40.8
    let expect = 40.8;
    assert!(
        (volume_of(&a, out) - expect).abs() < VOL_TOL,
        "vol {} vs {expect}",
        volume_of(&a, out)
    );
}

/// PR-KV7 flip (was a typed `UnsupportedCurvedBoolean` wall): a second
/// boolean on an output carrying a cylinder face, touching only planar
/// regions, now succeeds via output curve recovery.
#[test]
fn curved_output_reentry_planar_contact() {
    let mut a = BrepArena::new();
    let b1 = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 1.0));
    let c1 = cyl(&mut a, 2.0, 2.0, 0.8, (0.5, 2.0));
    let u1 = boolean_op(&mut a, b1, c1, BoolOp::Union).expect("boss union");
    let p = boxx(&mut a, (0.5, 1.2), (0.5, 1.2), (0.3, 1.5));
    let out = boolean_op(&mut a, u1, p, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("planar pocket after boss union: {e:?}"));
    validate_solid(&a, out).expect("validates");
    // slab 16 + boss-above-slab π·0.8² − pocket∩slab 0.7·0.7·0.7
    let boss_v = std::f64::consts::PI * 0.8 * 0.8;
    let expect = 16.0 + boss_v - 0.7 * 0.7 * 0.7;
    let vol = mesh_signed_volume(&tessellate(&a, out).expect("tessellate"));
    assert!(
        vol <= expect + 1e-9 && vol >= expect - 0.05 * boss_v,
        "vol {vol} vs {expect}"
    );
}

/// PR-KV7 flip: the second op cutting THROUGH the recovered boss itself —
/// the cut plane is parallel to the boss axis, so this also exercises the
/// F3 ruling-line SSI case on a RECOVERED body.
#[test]
fn curved_output_reentry_through_boss() {
    let mut a = BrepArena::new();
    let b1 = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 1.0));
    let c1 = cyl(&mut a, 2.0, 2.0, 0.8, (0.5, 2.0));
    let u1 = boolean_op(&mut a, b1, c1, BoolOp::Union).expect("boss union");
    let p = boxx(&mut a, (1.7, 2.3), (-1.0, 5.0), (1.3, 1.8));
    let out = boolean_op(&mut a, u1, p, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("cut through recovered boss: {e:?}"));
    validate_solid(&a, out).expect("validates");
    let vol = mesh_signed_volume(&tessellate(&a, out).expect("tessellate"));
    assert!(
        vol > 16.0 && vol < 16.0 + std::f64::consts::PI * 0.64,
        "vol {vol}"
    );
}

/// Chains must stay deterministic (bit-identical arenas + meshes).
#[test]
fn chain_deterministic() {
    let build = || {
        let mut a = BrepArena::new();
        let b1 = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 2.0));
        let p1 = boxx(&mut a, (0.5, 1.5), (0.5, 1.5), (1.0, 2.5));
        let p2 = boxx(&mut a, (2.5, 3.5), (2.5, 3.5), (1.0, 2.5));
        let s1 = boolean_op(&mut a, b1, p1, BoolOp::Subtract).expect("cut1");
        let s2 = boolean_op(&mut a, s1, p2, BoolOp::Subtract).expect("cut2");
        let m = tessellate(&a, s2).expect("tessellate");
        (a, m)
    };
    let (a1, m1) = build();
    let (a2, m2) = build();
    assert_eq!(a1, a2);
    assert_eq!(m1, m2);
}
