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

/// KV14 Slice B/C (spec `yang_stage1_curved_holed_patch`): a boolean OUTPUT
/// whose cylinder lateral carries a HOLE (a window cut through the wall) now
/// re-enters yang as an operand of a SECOND boolean — the former
/// `UnsupportedCurvedBoolean { reason: "curved lateral has inner loops" }` wall.
/// The window boolean produces a periodic wall strip (two encircling rim rings +
/// an interior window loop); yang Stage 1 unrolls it to (u = r·θ, v) and
/// triangulates the ribbon-with-hole. The second cut is a clean planar box, so
/// its volume decrement is EXACT — the strong re-entry oracle.
#[test]
fn curved_holed_lateral_reentry() {
    let mut a = BrepArena::new();
    // Solid cylinder r=1, z∈[0,3]; window box through the +y wall at z∈[1,2]
    // (clear of both rims), leaving a holed cylinder lateral.
    let c = cyl(&mut a, 0.0, 0.0, 1.0, (0.0, 3.0));
    let window = boxx(&mut a, (-0.4, 0.4), (0.3, 1.5), (1.0, 2.0));
    let holed = boolean_op(&mut a, c, window, BoolOp::Subtract).expect("cyl - window");
    validate_solid(&a, holed).expect("holed cylinder validates");
    let v1 = volume_of(&a, holed);

    // Analytic |cyl − window| = 3π − ∫∫ removed. The removed prism (z-height 1)
    // has cross-section {0.3 ≤ x ≤ √(1−y²), −0.4 ≤ y ≤ 0.4}; its area is
    // ∫_{-0.4}^{0.4} √(1−y²) dy − 0.3·0.8 = 0.7781228 − 0.24 = 0.5381228.
    let removed = (0.4 * (1.0f64 - 0.16).sqrt() + 0.4f64.asin()) - 0.24;
    let expect_v1 = 3.0 * std::f64::consts::PI - removed;
    // Faceting inscribes the curved wall, so the tessellated volume sits just
    // below analytic; 1% of the wall volume bounds the chord error.
    assert!(
        v1 <= expect_v1 + 1e-9 && v1 >= expect_v1 - 0.01 * 3.0 * std::f64::consts::PI,
        "holed cylinder volume {v1} vs analytic {expect_v1}"
    );

    // Second boolean RE-ENTERS the holed cylinder: a clean planar notch at the
    // top, inside the radius and clear of the window, removing exactly
    // 0.6·0.6·0.5 = 0.18. (Was the UnsupportedCurvedBoolean re-entry wall.)
    let notch = boxx(&mut a, (-0.3, 0.3), (-0.3, 0.3), (2.5, 3.5));
    let out = boolean_op(&mut a, holed, notch, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("re-enter holed cylinder: {e:?}"));
    validate_solid(&a, out).expect("re-entered result validates");
    let v2 = volume_of(&a, out);
    // The notch is planar and disjoint from the curved wall facets, so the
    // decrement is exact (the shared curved facets cancel in the difference).
    assert!(
        (v1 - v2 - 0.18).abs() < 1e-9,
        "second cut must remove exactly the 0.18 notch: v1={v1} v2={v2}"
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

/// KV14 Slice D end-to-end (spec `yang_stage1_curved_holed_patch`): a cylinder
/// lateral whose outer loop is NON-canonical (>4 edges) with NO holes must
/// re-enter yang Stage 1 as a boolean operand. A slab cut (cyl − half-space)
/// leaves a circular-segment prism whose curved wall is a 6-edge partial patch
/// (probe KV14_D_PROBE: outer_edges=6, inners=0) — the pre-Slice-D
/// `MalformedTopology` / `UnsupportedCurvedBoolean` re-entry wall. The second
/// boolean is a clean planar pocket disjoint from the curved wall, so its
/// volume decrement is EXACT (the shared curved facets cancel in the
/// difference) — the strong re-entry oracle.
#[test]
fn curved_partial_patch_no_hole_reentry() {
    let mut a = BrepArena::new();
    // Solid cylinder r=1, z∈[0,3]; slab removes x ≥ 0.3 (through both rims,
    // beyond the height), leaving the circular-segment {x < 0.3} prism. Its
    // curved wall is a partial patch with a NON-4-edge outer loop (no holes).
    let c = cyl(&mut a, 0.0, 0.0, 1.0, (0.0, 3.0));
    let slab = boxx(&mut a, (0.3, 2.0), (-2.0, 2.0), (-1.0, 4.0));
    let seg = boolean_op(&mut a, c, slab, BoolOp::Subtract).expect("cyl − slab");
    validate_solid(&a, seg).expect("segment prism validates");
    let v1 = volume_of(&a, seg);

    // Analytic cross-section = disc − cap(x ≥ 0.3): π − (acos(0.3) − 0.3·√0.91).
    let d = 0.3_f64;
    let cap = d.acos() - d * (1.0 - d * d).sqrt();
    let expect_v1 = (std::f64::consts::PI - cap) * 3.0;
    // Curved wall inscribed → tessellated volume just below analytic; 1% of the
    // full cylinder volume bounds the chord error.
    assert!(
        v1 <= expect_v1 + 1e-9 && v1 >= expect_v1 - 0.01 * 3.0 * std::f64::consts::PI,
        "segment prism volume {v1} vs analytic {expect_v1}"
    );

    // Second boolean RE-ENTERS the segment prism (its curved wall now converts
    // via the Slice D unroll+CDT path). A planar pocket open to the bottom face,
    // fully clear of the r=1 curved wall and of the x=0.3 flat cut, removes
    // exactly 0.5·0.6·1.5 = 0.45.
    let pocket = boxx(&mut a, (-0.5, 0.0), (-0.3, 0.3), (-1.0, 1.5));
    let out = boolean_op(&mut a, seg, pocket, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("re-enter segment prism (Slice D): {e:?}"));
    validate_solid(&a, out).expect("re-entered result validates");
    let v2 = volume_of(&a, out);
    // The re-entry re-facets the curved wall (representation drift ~1e-3, not the
    // exact-cancellation case), so the oracle is an analytic band on the FINAL
    // solid = segment prism − 0.45 pocket, inscribed (curved wall below analytic).
    let expect_v2 = expect_v1 - 0.45;
    assert!(
        v2 <= expect_v2 + 1e-9 && v2 >= expect_v2 - 0.01 * 3.0 * std::f64::consts::PI,
        "re-entered volume {v2} vs analytic {expect_v2} (segment − 0.45 pocket)"
    );
    // The decrement is dominated by the exact planar pocket; the residual is
    // bounded by the curved-wall re-facet drift (a whole extra/missing pocket
    // would blow this by ~0.45, a gross-error tripwire).
    assert!(
        (v1 - v2 - 0.45).abs() < 0.01,
        "pocket decrement {} must be ≈0.45 (planar + chord drift): v1={v1} v2={v2}",
        v1 - v2
    );
}

/// KV14 ellipse-arc re-entry end-to-end (spec `kv14_ellipse_arc_reentry`):
/// an OBLIQUE cylinder cut through a slab (the R0006 shape) leaves a
/// genus-1 through-tunnel whose planar caps carry elliptical holes and
/// whose tunnel wall is bounded by two encircling ellipse loops — all
/// `EllipseArc` (degree-4 conic) edges, the former
/// `UnsupportedCurvedBoolean { reason: "…degree-4 boundary (ellipse…)" }`
/// re-entry wall. The output must convert to yang (ellipse chains sample
/// into Stage-1 `rim_rings`) so a SECOND boolean succeeds; a planar notch
/// disjoint from the tunnel gives a near-exact volume decrement.
#[test]
fn ellipse_bounded_tunnel_reentry() {
    let mut a = BrepArena::new();
    let slab = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 2.0));
    // Oblique drill: unit axis d = (sinφ, 0, cosφ), tanφ = 1/2 (so the
    // plane∩cylinder sections on the z-caps are true ellipses, a = r/cosφ).
    // Profile plane through (1, 2, −1) with in-plane basis
    // x = (0,1,0), y = (−cosφ, 0, sinφ): x × y = d (right-handed).
    let s5 = 5.0_f64.sqrt();
    let (sphi, cphi) = (1.0 / s5, 2.0 / s5);
    let r = 0.6_f64;
    let p = Profile::circle(
        Point3::new(1.0, 2.0, -1.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(-cphi, 0.0, sphi),
        Point2::new(0.0, 0.0),
        r,
    )
    .unwrap();
    let drill = extrude(&mut a, &p, Vector3::new(sphi, 0.0, cphi), 5.0)
        .unwrap()
        .solid;
    let tunneled = boolean_op(&mut a, slab, drill, BoolOp::Subtract).expect("slab − oblique drill");
    validate_solid(&a, tunneled).expect("tunneled slab validates");

    // The re-entry wall was the ellipse vocabulary itself: the intermediate
    // must actually CARRY EllipseArc edges (else this test pins nothing).
    let ellipse_half_edges = a
        .half_edges
        .iter()
        .flatten()
        .filter(|h| matches!(h.curve, kernel_v2::Curve::EllipseArc { .. }))
        .count();
    assert!(
        ellipse_half_edges >= 2,
        "expected EllipseArc edges on the oblique tunnel, found {ellipse_half_edges} half-edges"
    );

    // Analytic |slab − drill| = 32 − π·r²·(2/cosφ) (slant length through the
    // 2-thick slab; the tunnel is laterally clear of all four side faces).
    let removed = std::f64::consts::PI * r * r * (2.0 / cphi);
    let expect_v1 = 32.0 - removed;
    let v1 = volume_of(&a, tunneled);
    assert!(
        (v1 - expect_v1).abs() <= 0.01 * removed,
        "tunneled volume {v1} vs analytic {expect_v1} (removed {removed})"
    );

    // Second boolean RE-ENTERS the ellipse-bounded body: a planar notch in
    // the top face, far from the tunnel (entry ellipse spans x∈[0.83,2.17],
    // exit x∈[2.83,3.17]... both at y∈[1.4,2.6]; the notch sits at
    // x,y ≤ 0.8), removing exactly 0.6·0.6·0.5 = 0.18.
    let notch = boxx(&mut a, (0.2, 0.8), (0.2, 0.8), (1.5, 2.5));
    let out = boolean_op(&mut a, tunneled, notch, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("re-enter ellipse-bounded tunnel: {e:?}"));
    validate_solid(&a, out).expect("re-entered result validates");
    let v2 = volume_of(&a, out);
    // The notch decrement is planar-exact up to the tunnel wall's re-facet
    // drift (the Slice D precedent bound).
    assert!(
        (v1 - v2 - 0.18).abs() < 0.01,
        "notch decrement {} must be ≈0.18: v1={v1} v2={v2}",
        v1 - v2
    );
}
