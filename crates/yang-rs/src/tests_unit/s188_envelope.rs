//! #188 inc-1 — unit fixtures for the §3.2 envelope-resolution primitives
//! (`stage5_envelope`), spec `specs/yang_188_f0082_j3_envelope_selection.md`
//! §5 inc-1: "the cylinder two-plane triple-point solver + band classifier,
//! unit-tested against the F0082 pinned geometry (v925 and J3 to 9 decimals;
//! antipodality; live/dead table red/green per op). Unwired."
//!
//! F0082 ground truth (probe log 2026-07-21, `YANG_S5_OSCULATION_PROBE=walk`
//! on the failing-union tube patch info=373; spec §7):
//! - all plane equations at 9 decimals from the probe's support dump;
//! - the cylinder axis is FITTED from five top-rim circle verts (walk c=1,
//!   all at axial height 0.121657): two independent circumcenter triples
//!   agree to 2e-9, and every pinned junction vert sits at radius
//!   0.212325266 ± 1.5e-9 — validated below in
//!   `f0082_fixture_self_consistency`;
//! - pinned points are output-mesh verts printed at 9 decimals.

use crate::stage5_envelope::*;
use crate::{BoolOp, InputId, Point3, Surface, Vector3};

// ---- F0082 pinned fixture -------------------------------------------------

/// The tube (input B, face 2): axis fitted from the top-rim circle.
fn f0082_tube() -> Surface {
    Surface::Cylinder {
        axis_point: Point3::new(0.131_030_789, -0.014_609_368, 2.223_097_876),
        axis_dir: Vector3::new(0.068_213_056, -0.051_637_098, 0.996_333_573),
        radius: 0.212_325_266,
    }
}

/// A's top plane (the intersection-ellipse carrier), outward from A.
const P_INT: EnvPlane = EnvPlane {
    n: [0.050_626_810, -0.017_840_778, 0.998_558_277],
    d: -2.105_218_942,
};

/// B's bottom cap plane (the rim carrier), outward from B.
const P_ORIG: EnvPlane = EnvPlane {
    n: [-0.068_213_056, 0.051_637_098, -0.996_333_573],
    d: 2.102_982_699,
};

/// A's side walls crossing the tube, outward from A.
/// Fixture wall indices: 0 = face-364, 1 = face-365, 2 = face-366.
const WALL_364: EnvPlane = EnvPlane {
    n: [-0.998_717_641, -0.000_904_381, 0.050_618_732],
    d: -0.170_791_364,
};
const WALL_365: EnvPlane = EnvPlane {
    n: [0.0, -0.999_840_432, -0.017_863_686],
    d: -0.158_982_505,
};
const WALL_366: EnvPlane = EnvPlane {
    n: [0.998_717_641, 0.000_904_381, -0.050_618_732],
    d: -0.203_136_222,
};

fn f0082_walls() -> [EnvPlane; 3] {
    [WALL_364, WALL_365, WALL_366]
}

/// The free-space-side triple point — a bit-exact OUTPUT vertex (ring idx
/// 15, shared with the cap-disc spokes; §7.9). Probe gap zero at
/// θ = −1.088507, nearest-vert distance 5.8e-14.
const V925: [f64; 3] = [0.310_746_669, 0.090_019_612, 2.094_111_979];
/// The antipodal (J3) triple point — analytic (nearest output vert
/// 2.76e-3 away). Probe gap zero at θ = +2.053086.
const J3: [f64; 3] = [-0.065_282_249, -0.106_674_345, 2.109_662_370];

/// Minted wall-crossing junction verts (§7.2: on their wall planes to
/// ≤1.1e-9): ellipse×366, rim×366, ellipse×364, rim×364, ellipse×365 ×2.
const V921: [f64; 3] = [0.309_456_248, -0.108_996_525, 2.090_621_674];
const V949: [f64; 3] = [0.309_117_006, -0.108_430_167, 2.083_938_472];
const V937: [f64; 3] = [-0.063_991_829, -0.109_111_255, 2.109_553_406];
const V943: [f64; 3] = [-0.063_997_163, -0.109_109_265, 2.109_448_193];
const V923: [f64; 3] = [0.221_231_461, -0.196_412_058, 2.093_532_850];
const V935: [f64; 3] = [0.024_457_443, -0.196_590_246, 2.103_506_090];
/// Rim×365 NEAR-crossing output verts (rim samples ~5e-7 off the wall
/// plane, not exact junctions — pinned loosely).
const V944: [f64; 3] = [0.220_153_929, -0.196_268_126, 2.085_476_859];
const V945: [f64; 3] = [0.024_440_861, -0.196_507_304, 2.098_863_777];

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn sd(pl: &EnvPlane, p: [f64; 3]) -> f64 {
    pl.n[0] * p[0] + pl.n[1] * p[1] + pl.n[2] * p[2] + pl.d
}

/// Radial residual of `p` off the fixture cylinder.
fn radial_resid(p: [f64; 3]) -> f64 {
    let ap: [f64; 3] = [0.131_030_789, -0.014_609_368, 2.223_097_876];
    let ad: [f64; 3] = [0.068_213_056, -0.051_637_098, 0.996_333_573];
    let r = 0.212_325_266;
    let an = (ad[0] * ad[0] + ad[1] * ad[1] + ad[2] * ad[2]).sqrt();
    let a = [ad[0] / an, ad[1] / an, ad[2] / an];
    let q = [p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]];
    let qa = q[0] * a[0] + q[1] * a[1] + q[2] * a[2];
    let w = [q[0] - qa * a[0], q[1] - qa * a[1], q[2] - qa * a[2]];
    (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt() - r
}

// ---- fixture validation ---------------------------------------------------

/// The fitted cylinder + 9-decimal planes reproduce the probe's ground
/// truth: every pinned junction vert lies ON the fixture cylinder, and the
/// documented wall-plane memberships hold.
#[test]
fn f0082_fixture_self_consistency() {
    for (name, p) in [
        ("v925", V925),
        ("J3", J3),
        ("v921", V921),
        ("v949", V949),
        ("v937", V937),
        ("v943", V943),
        ("v923", V923),
        ("v935", V935),
        ("v944", V944),
        ("v945", V945),
    ] {
        assert!(
            radial_resid(p).abs() < 5e-9,
            "{name} radial residual {:.3e}",
            radial_resid(p)
        );
    }
    // §7.2 wall memberships of the exact junction verts.
    assert!(sd(&WALL_366, V921).abs() < 1e-8);
    assert!(sd(&WALL_366, V949).abs() < 1e-8);
    assert!(sd(&WALL_364, V937).abs() < 1e-8);
    assert!(sd(&WALL_364, V943).abs() < 1e-8);
    assert!(sd(&WALL_365, V923).abs() < 1e-8);
    assert!(sd(&WALL_365, V935).abs() < 1e-8);
    // Curve memberships: v921/v926-class verts are on their carrier planes.
    assert!(sd(&P_INT, V921).abs() < 1e-8, "v921 on the ellipse plane");
    assert!(sd(&P_ORIG, V949).abs() < 1e-8, "v949 on the rim plane");
    assert!(sd(&P_INT, V937).abs() < 1e-8, "v937 on the ellipse plane");
    assert!(sd(&P_ORIG, V943).abs() < 1e-8, "v943 on the rim plane");
}

// ---- §3.2.1 switch-point solver ------------------------------------------

/// The two exact triple points land on v925 (bit-exact output vertex) and
/// the analytic J3 to 9 decimals, antipodal to ~π (the planes' line passes
/// through the tube axis — §1's odd sinusoid).
#[test]
fn f0082_switch_points_pin_v925_and_j3() {
    let pts = cylinder_two_plane_switch_points(&f0082_tube(), &P_INT, &P_ORIG)
        .expect("F0082 pair is transversal at the triple points");
    let (a, b) = (pts[0], pts[1]);
    // Identify by azimuth sign (probe: v925 at θ=−1.088507, J3 at +2.053086).
    let (p925, pj3) = if a.theta < 0.0 { (a, b) } else { (b, a) };
    assert!(
        dist3(p925.p, V925) < 1e-7,
        "v925 pin missed by {:.3e}",
        dist3(p925.p, V925)
    );
    assert!(
        dist3(pj3.p, J3) < 1e-7,
        "J3 pin missed by {:.3e}",
        dist3(pj3.p, J3)
    );
    assert!((p925.theta - (-1.088_507)).abs() < 1e-5);
    assert!((pj3.theta - 2.053_086).abs() < 1e-5);
    // Antipodality (π to ~6 digits).
    let dt = (p925.theta - pj3.theta).abs();
    assert!(
        (dt - std::f64::consts::PI).abs() < 2e-5,
        "not antipodal: Δθ = {dt}"
    );
    // On all three surfaces.
    for sp in [p925, pj3] {
        assert!(sd(&P_INT, sp.p).abs() < 1e-9);
        assert!(sd(&P_ORIG, sp.p).abs() < 1e-9);
        assert!(radial_resid(sp.p).abs() < 5e-9);
    }
}

// ---- §3.2.2 band classification (§7.6) ------------------------------------

/// BOTH triple points are wall-masked (§7.3): v925 beyond face-366 and J3
/// beyond face-364, each by +1.2921e-3 (model-symmetric). Neither is a
/// free-space junction.
#[test]
fn f0082_both_triples_wall_masked() {
    let walls = f0082_walls();
    let bands = classify_bands(
        &f0082_tube(),
        &P_INT,
        &P_ORIG,
        &walls,
        BoolOp::Union,
        InputId::B,
    )
    .expect("F0082 union bands classify");
    for ct in &bands.triples {
        let near_v925 = dist3(ct.point.p, V925) < 1e-6;
        let expect_wall = if near_v925 { 2 } else { 0 }; // 366 / 364
        match ct.class {
            TripleClass::WallMasked { wall, margin } => {
                assert_eq!(wall, expect_wall, "masking wall for {:?}", ct.point.p);
                assert!(
                    (margin - 1.2921e-3).abs() < 1e-6,
                    "masking margin {margin:.6e} ≠ +1.2921e-3"
                );
            }
            TripleClass::FreeSpace => panic!("triple {:?} must be wall-masked", ct.point.p),
        }
    }
    // No FreeSpaceTriple boundary exists.
    assert!(bands
        .boundaries
        .iter()
        .all(|b| b.kind != BoundaryKind::FreeSpaceTriple));
}

/// The full F0082/Union band structure: 8 retained boundaries — the six
/// exact wall-crossing junctions (v944·v923 | v921·v949 | v937·v943 |
/// v935·v945) delimiting three wall-complex slivers plus one sliver pair
/// on the far rim seam — with the live sequence
/// [WC365, Ell, WC366, Rim, WC364, Ell, WC365, Rim]. The rim band runs
/// STRAIGHT through the masked v925 triple and through the absorbed
/// wall-crossing pairs at u≈−0.228 (v926/v951) and u≈0.229 — spec §7.4's
/// "correct = rim straight v951→v959" falls out of the classifier.
#[test]
fn f0082_union_band_structure() {
    let walls = f0082_walls();
    let bands = classify_bands(
        &f0082_tube(),
        &P_INT,
        &P_ORIG,
        &walls,
        BoolOp::Union,
        InputId::B,
    )
    .expect("F0082 union bands classify");

    assert_eq!(
        bands.boundaries.len(),
        8,
        "boundaries: {:#?}",
        bands.boundaries
    );
    // Sorted by θ the sequence starts at v944 (θ≈−2.6602).
    let expect: [([f64; 3], usize, bool, f64); 8] = [
        (V944, 1, false, 1e-5), // rim×365 (near-crossing pin)
        (V923, 1, true, 1e-7),  // ellipse×365 (exact)
        (V921, 2, true, 1e-7),  // ellipse×366 (exact)
        (V949, 2, false, 1e-7), // rim×366 (exact)
        (V937, 0, true, 1e-7),  // ellipse×364 (exact)
        (V943, 0, false, 1e-7), // rim×364 (exact)
        (V935, 1, true, 1e-7),  // ellipse×365 (exact)
        (V945, 1, false, 1e-5), // rim×365 (near-crossing pin)
    ];
    for (i, (pin, wall, on_int, tol)) in expect.iter().enumerate() {
        let b = &bands.boundaries[i];
        assert!(
            dist3(b.p, *pin) < *tol,
            "boundary {i} missed its pin by {:.3e}: {:?}",
            dist3(b.p, *pin),
            b
        );
        assert_eq!(
            b.kind,
            BoundaryKind::WallCrossing {
                wall: *wall,
                on_int_curve: *on_int
            },
            "boundary {i} kind"
        );
    }
    let expect_live = [
        BandLive::WallComplex { wall: 1 },
        BandLive::IntCurve,
        BandLive::WallComplex { wall: 2 },
        BandLive::OrigCurve,
        BandLive::WallComplex { wall: 0 },
        BandLive::IntCurve,
        BandLive::WallComplex { wall: 1 },
        BandLive::OrigCurve,
    ];
    assert_eq!(bands.live, expect_live);

    // Spot liveness at probe-verified azimuths (θ = u / r).
    for (theta, want) in [
        (-2.355, BandLive::IntCurve),  // walk v918/v920 ellipse run
        (-1.5, BandLive::OrigCurve),   // beyond-366 rim band (§7.2 CORRECT)
        (-1.088, BandLive::OrigCurve), // straight through masked v925
        (0.0, BandLive::OrigCurve),    // rim above A's top (v957/v958)
        (1.5, BandLive::OrigCurve),    // beyond-364 rim band (v954/v955)
        (2.3, BandLive::IntCurve),     // walk v934/v936 ellipse run
        (3.0, BandLive::OrigCurve),    // beyond-365 rim seam (v953)
        (-3.0, BandLive::OrigCurve),
    ] {
        assert_eq!(bands.live_at(theta), Some(want), "live at θ={theta}");
    }
}

/// Without the masking walls the switch junctions would sit AT the triple
/// points (the §2 pairwise rule): both triples FreeSpace, two bands, and
/// azimuths that are rim-live under masking flip to ellipse-live — the
/// §7.6 discriminator.
#[test]
fn f0082_no_walls_switches_at_triples() {
    let bands = classify_bands(
        &f0082_tube(),
        &P_INT,
        &P_ORIG,
        &[],
        BoolOp::Union,
        InputId::B,
    )
    .expect("wall-free pair classifies");
    assert_eq!(bands.boundaries.len(), 2);
    assert!(bands
        .boundaries
        .iter()
        .all(|b| b.kind == BoundaryKind::FreeSpaceTriple));
    assert!(bands
        .triples
        .iter()
        .all(|t| t.class == TripleClass::FreeSpace));
    // Rim live only between the triples (rim above A's top)…
    assert_eq!(bands.live_at(0.0), Some(BandLive::OrigCurve));
    assert_eq!(bands.live_at(-2.0), Some(BandLive::IntCurve));
    // …whereas WITH walls θ=−2.0 and θ=3.0 are rim-live (beyond-wall bands).
    let walls = f0082_walls();
    let masked = classify_bands(
        &f0082_tube(),
        &P_INT,
        &P_ORIG,
        &walls,
        BoolOp::Union,
        InputId::B,
    )
    .unwrap();
    assert_eq!(masked.live_at(-2.0), Some(BandLive::OrigCurve));
    assert_eq!(bands.live_at(3.0), Some(BandLive::IntCurve));
    assert_eq!(masked.live_at(3.0), Some(BandLive::OrigCurve));
}

/// The op table (spec inc-1 "live/dead table red/green per op"): the
/// same-side max-envelope vocabulary is Union (either owner) and
/// Subtract with the patch on the BASE; Subtract-on-tool, Intersect and
/// Xor fail closed.
#[test]
fn op_table_red_green() {
    for (op, owner, ok) in [
        (BoolOp::Union, InputId::A, true),
        (BoolOp::Union, InputId::B, true),
        (BoolOp::Subtract, InputId::A, true),
        (BoolOp::Subtract, InputId::B, false),
        (BoolOp::Intersect, InputId::A, false),
        (BoolOp::Intersect, InputId::B, false),
        (BoolOp::Xor, InputId::A, false),
        (BoolOp::Xor, InputId::B, false),
    ] {
        let rule = resolve_envelope_rule(op, owner);
        assert_eq!(rule.is_ok(), ok, "rule for {op:?}/{owner:?}");
        if !ok {
            assert_eq!(rule, Err(EnvelopeError::UnsupportedOp));
            // classify_bands propagates the fail-closed verdict.
            let walls = f0082_walls();
            assert_eq!(
                classify_bands(&f0082_tube(), &P_INT, &P_ORIG, &walls, op, owner),
                Err(EnvelopeError::UnsupportedOp)
            );
        }
    }
}

// ---- synthetic benign-scale fixtures --------------------------------------

/// Unit right cylinder, cap z=0 (owner below-bound), partner plane
/// z = 0.1·x: free-space triples at exactly θ = ±π/2, ellipse live on the
/// cosθ > 0 half, rim live on the other.
#[test]
fn synthetic_free_space_triples_and_bands() {
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let s = (1.0f64 + 0.01).sqrt();
    let p_int = EnvPlane {
        n: [-0.1 / s, 0.0, 1.0 / s],
        d: 0.0,
    };
    let p_orig = EnvPlane {
        n: [0.0, 0.0, -1.0],
        d: 0.0,
    };
    let pts = cylinder_two_plane_switch_points(&cyl, &p_int, &p_orig).unwrap();
    for sp in pts {
        assert!(
            (sp.theta.abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-12,
            "triple at θ={}",
            sp.theta
        );
        assert!((sp.p[0].abs()) < 1e-12 && (sp.p[1].abs() - 1.0).abs() < 1e-12);
    }
    let bands = classify_bands(&cyl, &p_int, &p_orig, &[], BoolOp::Union, InputId::B).unwrap();
    assert_eq!(bands.boundaries.len(), 2);
    assert_eq!(bands.live_at(0.0), Some(BandLive::IntCurve));
    assert_eq!(
        bands.live_at(std::f64::consts::PI),
        Some(BandLive::OrigCurve)
    );
}

/// Adding a wall y ≥ 0.9 masks the θ=+π/2 triple (margin 0.1): boundaries
/// become {the free triple at −π/2, the wall crossing at asin(0.9)} — the
/// second wall zero is absorbed because the rim stays live across it
/// (beyond-wall band hands over to the direct partner-side test), and the
/// axis-parallel wall's bit-identical crossings on both curves dedup to a
/// single boundary.
#[test]
fn synthetic_masked_triple_wall_takeover() {
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let s = (1.0f64 + 0.01).sqrt();
    let p_int = EnvPlane {
        n: [-0.1 / s, 0.0, 1.0 / s],
        d: 0.0,
    };
    let p_orig = EnvPlane {
        n: [0.0, 0.0, -1.0],
        d: 0.0,
    };
    let wall = EnvPlane {
        n: [0.0, 1.0, 0.0],
        d: -0.9,
    };
    let bands = classify_bands(&cyl, &p_int, &p_orig, &[wall], BoolOp::Union, InputId::B).unwrap();
    let masked: Vec<_> = bands
        .triples
        .iter()
        .filter_map(|t| match t.class {
            TripleClass::WallMasked { wall, margin } => Some((wall, margin, t.point.theta)),
            TripleClass::FreeSpace => None,
        })
        .collect();
    assert_eq!(masked.len(), 1);
    assert_eq!(masked[0].0, 0);
    assert!((masked[0].1 - 0.1).abs() < 1e-12);
    assert!((masked[0].2 - std::f64::consts::FRAC_PI_2).abs() < 1e-12);

    assert_eq!(bands.boundaries.len(), 2, "{:#?}", bands.boundaries);
    assert_eq!(bands.boundaries[0].kind, BoundaryKind::FreeSpaceTriple);
    assert!((bands.boundaries[0].theta + std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    assert!(matches!(
        bands.boundaries[1].kind,
        BoundaryKind::WallCrossing { wall: 0, .. }
    ));
    assert!((bands.boundaries[1].theta - 0.9f64.asin()).abs() < 1e-12);
    assert_eq!(bands.live, vec![BandLive::IntCurve, BandLive::OrigCurve]);
    // Straight through the masked triple: rim live at +π/2.
    assert_eq!(
        bands.live_at(std::f64::consts::FRAC_PI_2),
        Some(BandLive::OrigCurve)
    );
}

/// Degenerate and out-of-vocabulary configurations fail CLOSED (§3.2).
#[test]
fn degenerate_configurations_fail_closed() {
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let s2 = 2.0f64.sqrt();
    let cap = EnvPlane {
        n: [0.0, 0.0, -1.0],
        d: 0.0,
    };

    // (near-)parallel pair planes: no intersection line.
    let anti_cap = EnvPlane {
        n: [0.0, 0.0, 1.0],
        d: -0.5,
    };
    assert_eq!(
        cylinder_two_plane_switch_points(&cyl, &anti_cap, &cap),
        Err(EnvelopeError::PlanesNearParallel)
    );

    // Axis-parallel pair plane: no axial profile.
    let vertical = EnvPlane {
        n: [1.0, 0.0, 0.0],
        d: -0.5,
    };
    assert_eq!(
        cylinder_two_plane_switch_points(&cyl, &vertical, &cap),
        Err(EnvelopeError::AxisParallelPairPlane)
    );

    // Pair line misses the cylinder (z = x − 5 vs z = 0 ⇒ line x=5).
    let far = EnvPlane {
        n: [-1.0 / s2, 0.0, 1.0 / s2],
        d: 5.0 / s2,
    };
    assert_eq!(
        cylinder_two_plane_switch_points(&cyl, &far, &cap),
        Err(EnvelopeError::NoTripleContact)
    );

    // Exact grazing contact (z = x − 1 ⇒ line x=1 tangent): fails closed
    // as either tangency variant depending on the rounding of disc≈0.
    let graze = EnvPlane {
        n: [-1.0 / s2, 0.0, 1.0 / s2],
        d: 1.0 / s2,
    };
    let r = cylinder_two_plane_switch_points(&cyl, &graze, &cap);
    assert!(
        matches!(
            r,
            Err(EnvelopeError::NoTripleContact) | Err(EnvelopeError::TangentTripleContact)
        ),
        "grazing line must fail closed, got {r:?}"
    );

    // A wall passing exactly through a triple point: degenerate.
    let tilt = EnvPlane {
        n: [
            -0.1 / (1.0f64 + 0.01).sqrt(),
            0.0,
            1.0 / (1.0f64 + 0.01).sqrt(),
        ],
        d: 0.0,
    };
    let wall_on_triple = EnvPlane {
        n: [0.0, 1.0, 0.0],
        d: -1.0,
    };
    assert!(matches!(
        classify_bands(
            &cyl,
            &tilt,
            &cap,
            &[wall_on_triple],
            BoolOp::Union,
            InputId::B
        ),
        Err(EnvelopeError::DegenerateBoundary { .. })
    ));

    // Non-cylinder surface.
    let plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    assert_eq!(
        cylinder_two_plane_switch_points(&plane, &tilt, &cap),
        Err(EnvelopeError::UnsupportedSurface)
    );
}

// ===========================================================================
// inc-2 — hand-built weave fixtures for the §3.3 rebuild (the e2e gate
// test's benign union already emits a CORRECT loop, which proves firing +
// idempotence but not repair; these fixtures contain deliberate F0082-§7.4
// defects the selection must fix).
// ===========================================================================

use crate::stage5_envelope::rebuild_osculating_loops;
use crate::{Curve, Mesh, PatchInfo};

fn ring(ids: &[u32]) -> Vec<Vec<(u32, u32)>> {
    vec![(0..ids.len())
        .map(|i| (ids[i], ids[(i + 1) % ids.len()]))
        .collect()]
}

fn pinfo(input: InputId, inherited: Surface, cycles: Vec<Vec<(u32, u32)>>) -> PatchInfo {
    PatchInfo {
        cycles,
        input,
        inherited,
        face_idx: 0,
        input_reversed: false,
        had_fold_sliver: false,
    }
}

/// Unit cylinder (axis z, r=1) with the tilted partner plane z = 0.05·x
/// (owner cap z = 0): the inc-1 synthetic, now with a MESH loop.
fn unit_tilted_fixture() -> (Surface, EnvPlane, EnvPlane, Curve) {
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let s = (1.0f64 + 0.0025).sqrt(); // |(-0.05, 0, 1)|
    let p_int = EnvPlane {
        n: [-0.05 / s, 0.0, 1.0 / s],
        d: 0.0,
    };
    let p_orig = EnvPlane {
        n: [0.0, 0.0, -1.0],
        d: 0.0,
    };
    // The attributed intersection conic (carrier = p_int).
    let proj = {
        // projection of ẑ onto the plane, normalized (major axis).
        let na = 1.0 / s;
        let p = [0.0 - na * (-0.05 / s), 0.0, 1.0 - na * (1.0 / s)];
        let n = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        [p[0] / n, p[1] / n, p[2] / n]
    };
    let conic = Curve::Ellipse {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(-0.05 / s, 0.0, 1.0 / s),
        major_axis: Vector3::new(proj[0], proj[1], proj[2]),
        major_radius: s / 1.0f64.max(1.0), // r / cosφ = 1 / (1/s) = s
        minor_radius: 1.0,
    };
    (cyl, p_int, p_orig, conic)
}

/// Vertex on the ellipse (tube ∩ tilted plane) at azimuth θ.
fn ell_pt(theta: f64) -> [f64; 3] {
    [theta.cos(), theta.sin(), 0.05 * theta.cos()]
}
/// Vertex on the rim (tube ∩ cap) at azimuth θ.
fn rim_pt(theta: f64) -> [f64; 3] {
    [theta.cos(), theta.sin(), 0.0]
}

/// The §7.4 PRIMARY defect in miniature: the loop detours through a
/// dead-side ellipse vert past the triple point (fold) and carries a
/// dead-side rim vert inside the ellipse band. The rebuild must drop both,
/// keep both triple junctions (mint-by-identity), and emit a monotone
/// alternating loop with true curve vocabulary on the new edges.
#[test]
fn rebuild_drops_dead_side_detour_and_reorders() {
    let (cyl, p_int, _p_orig, conic) = unit_tilted_fixture();
    let mut pts: Vec<[f64; 3]> = Vec::new();
    let mut add = |p: [f64; 3]| -> u32 {
        pts.push(p);
        (pts.len() - 1) as u32
    };
    let j1 = add([0.0, -1.0, 0.0]); // triple at θ = −π/2
    let e_m10 = add(ell_pt(-1.0));
    let e_m05 = add(ell_pt(-0.5));
    let rim_dead = add(rim_pt(-0.3)); // rim inside the ellipse band: DEAD
    let e_00 = add(ell_pt(0.0));
    let e_05 = add(ell_pt(0.5));
    let e_10 = add(ell_pt(1.0));
    let ell_dead = add(ell_pt(1.7)); // ellipse past the triple: DEAD (fold)
    let j2 = add([0.0, 1.0, 0.0]); // triple at θ = +π/2
    let r_20 = add(rim_pt(2.0));
    let r_25 = add(rim_pt(2.5));
    let r_30 = add(rim_pt(3.0));
    let r_m25 = add(rim_pt(-2.5));
    let r_m20 = add(rim_pt(-2.0));
    let mesh = Mesh {
        verts: pts.iter().map(|p| Point3::new(p[0], p[1], p[2])).collect(),
        tris: Vec::new(),
    };

    let broken = ring(&[
        j1, e_m10, e_m05, rim_dead, e_00, e_05, e_10, ell_dead, j2, r_20, r_25, r_30, r_m25, r_m20,
    ]);
    let infos = vec![
        pinfo(InputId::B, cyl, broken.clone()),
        pinfo(
            InputId::A,
            Surface::Plane {
                normal: Vector3::new(p_int.n[0], p_int.n[1], p_int.n[2]),
                d: p_int.d,
            },
            Vec::new(),
        ),
        pinfo(
            InputId::B,
            Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: 0.0,
            },
            ring(&[r_20, r_25]),
        ),
    ];
    let subdiv = vec![broken.clone(), Vec::new(), ring(&[r_20, r_25])];
    let mut curves: std::collections::BTreeMap<(u32, u32), Curve> = Default::default();
    for (s, e) in [(j1, e_m10), (e_m10, e_m05), (e_05, e_10)] {
        curves.insert(if s < e { (s, e) } else { (e, s) }, conic);
    }

    let rebuilt =
        rebuild_osculating_loops(&mesh, &infos, 0, &subdiv, &curves, crate::BoolOp::Union)
            .expect("no postcondition failure")
            .expect("the weave fixture must rebuild");

    let expected = ring(&[
        j1, e_m10, e_m05, e_00, e_05, e_10, j2, r_20, r_25, r_30, r_m25, r_m20,
    ]);
    assert_eq!(
        rebuilt.cycles, expected,
        "dead-side verts dropped, monotone order"
    );

    // New edges carry true curve vocabulary: ellipse on the int side
    // (spanning the dropped rim vert and the closed triple), circle on the
    // rim side (the cap is ⊥ the axis).
    let key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    assert_eq!(
        rebuilt.curve_overrides.get(&(0, key(e_m05, e_00))),
        Some(&conic)
    );
    assert_eq!(
        rebuilt.curve_overrides.get(&(0, key(e_10, j2))),
        Some(&conic)
    );
    // (j2, r_20) was an ORIGINAL edge (the healthy exit of the detour):
    // it keeps its original attribution — no override.
    assert!(!rebuilt.curve_overrides.contains_key(&(0, key(j2, r_20))));
    assert_eq!(rebuilt.curve_overrides.len(), 2);
}

/// Wall-complex slivers with the F0082-WC364 curve-adjacency SWAP: a
/// slightly tilted partner wall (x − 0.05z = 0.6) crosses the rim at
/// θ = ±acos(0.6) and the ellipse at ±acos(0.6/0.9975) — on each side the
/// crossing serving the ADJACENT band is the θ-far one, so pairing by θ
/// order would connect the wrong curves. The healthy wall-arc traversals
/// (with their physical micro-backsteps) must come through BYTE-IDENTICAL,
/// while a dead ellipse vert in the far rim band is dropped.
#[test]
fn rebuild_keeps_wall_sections_byte_identical_with_swap_pairing() {
    let (cyl, p_int, _p_orig, conic) = unit_tilted_fixture();
    // Wall plane x − 0.05z = 0.6, outward from partner A.
    let wn = (1.0f64 + 0.0025).sqrt();
    let wall = EnvPlane {
        n: [1.0 / wn, 0.0, -0.05 / wn],
        d: -0.6 / wn,
    };
    let th_r = 0.6f64.acos(); // rim × wall
    let x_e: f64 = 0.6 / (1.0 - 0.0025);
    let th_e = x_e.acos(); // ellipse × wall (th_e < th_r)
    let th_w = (th_r + th_e) / 2.0; // wall-arc sample azimuth
    let wall_pt = |theta: f64| -> [f64; 3] {
        // On the cylinder AND on the wall plane: v = (x − 0.6)/0.05.
        let (x, y) = (theta.cos(), theta.sin());
        [x, y, (x - 0.6) / 0.05]
    };

    let mut pts: Vec<[f64; 3]> = Vec::new();
    let mut add = |p: [f64; 3]| -> u32 {
        pts.push(p);
        (pts.len() - 1) as u32
    };
    let j1 = add([0.0, -1.0, 0.0]);
    let e_m12 = add(ell_pt(-1.2));
    let ex_m = add(ell_pt(-th_e)); // ellipse × wall junction (−side)
    let w_m = add(wall_pt(-th_w)); // wall-arc vert (−side)
    let rx_m = add(rim_pt(-th_r)); // rim × wall junction (−side)
    let r_m08 = add(rim_pt(-0.8));
    let r_00 = add(rim_pt(0.0));
    let r_08 = add(rim_pt(0.8));
    let rx_p = add(rim_pt(th_r)); // rim × wall junction (+side)
    let w_p = add(wall_pt(th_w)); // wall-arc vert (+side)
    let ex_p = add(ell_pt(th_e)); // ellipse × wall junction (+side)
    let e_12 = add(ell_pt(1.2));
    let j2 = add([0.0, 1.0, 0.0]);
    let r_22 = add(rim_pt(2.2));
    let ell_dead = add(ell_pt(2.6)); // dead ellipse vert in the rim band
    let r_30 = add(rim_pt(3.0));
    let r_m24 = add(rim_pt(-2.4));
    let mesh = Mesh {
        verts: pts.iter().map(|p| Point3::new(p[0], p[1], p[2])).collect(),
        tris: Vec::new(),
    };

    let broken = ring(&[
        j1, e_m12, ex_m, w_m, rx_m, r_m08, r_00, r_08, rx_p, w_p, ex_p, e_12, j2, r_22, ell_dead,
        r_30, r_m24,
    ]);
    let infos = vec![
        pinfo(InputId::B, cyl, broken.clone()),
        pinfo(
            InputId::A,
            Surface::Plane {
                normal: Vector3::new(p_int.n[0], p_int.n[1], p_int.n[2]),
                d: p_int.d,
            },
            Vec::new(),
        ),
        pinfo(
            InputId::B,
            Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: 0.0,
            },
            ring(&[r_00, r_08]),
        ),
        // The wall patch: shares the wall-arc vert with the loop.
        pinfo(
            InputId::A,
            Surface::Plane {
                normal: Vector3::new(wall.n[0], wall.n[1], wall.n[2]),
                d: wall.d,
            },
            ring(&[w_p, rx_p]),
        ),
    ];
    let subdiv = vec![
        broken.clone(),
        Vec::new(),
        ring(&[r_00, r_08]),
        ring(&[w_p, rx_p]),
    ];
    let mut curves: std::collections::BTreeMap<(u32, u32), Curve> = Default::default();
    for (s, e) in [(j1, e_m12), (e_m12, ex_m), (ex_p, e_12), (e_12, j2)] {
        curves.insert(if s < e { (s, e) } else { (e, s) }, conic);
    }

    let rebuilt =
        rebuild_osculating_loops(&mesh, &infos, 0, &subdiv, &curves, crate::BoolOp::Union)
            .expect("no postcondition failure")
            .expect("the wall-weave fixture must rebuild");

    // Identical to the healthy input minus the dead ellipse vert — the
    // wall sections [ex_m, w_m, rx_m] and [rx_p, w_p, ex_p] byte-identical
    // (swap pairing: each WC band's entry junction is on the PREVIOUS
    // band's curve even though it is the θ-far crossing).
    let expected = ring(&[
        j1, e_m12, ex_m, w_m, rx_m, r_m08, r_00, r_08, rx_p, w_p, ex_p, e_12, j2, r_22, r_30, r_m24,
    ]);
    assert_eq!(rebuilt.cycles, expected);

    // The only new edge spans the dropped dead vert, on the rim.
    let key = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    let rim_circle = Curve::Circle {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, -1.0),
        radius: 1.0,
    };
    assert_eq!(
        rebuilt.curve_overrides.get(&(0, key(r_22, r_30))),
        Some(&rim_circle)
    );
    assert_eq!(rebuilt.curve_overrides.len(), 1);
}
