//! Task #62 — chained swiss-cheese plates (F0086–F0090 corpus family).
//!
//! A disc plate takes SUCCESSIVE cut-cylinders, every tool sketched on the
//! SAME z=0 plane (same-normal coplanar bottom caps, the production
//! feature-engine pattern). Each cut's OUTPUT re-enters the next boolean, so
//! the chain exercises output curve recovery (`recover.rs`) on rims whose
//! vertex spacing is NON-UNIFORM: the coplanar overlay's sweep events mint
//! dense crossing clusters on the rim rings, and the z=0 rim carries
//! on-chord overlay boundary points (off-circle), so the recovered lateral
//! loses its canonical anchor and the top rim takes the closed-chain
//! 3-piece arc fallback.
//!
//! RED (this file's reason for existing): `closed_fallback_pieces` split at
//! VERTEX-COUNT thirds — with cluster spacing a "third" can subtend > π, the
//! downstream minor-side arc derivation picks the wrong side, and the
//! reassembled outer lateral's top rim walks out-and-back (net winding 0)
//! → `CurvedGeometryMismatch("cylinder patch must have exactly 0 or 2
//! axis-wrapping loops")` at step 2. GREEN: sweep-aware fallback splitting
//! (every piece < MAX_ARC_PIECE_SWEEP by ACCUMULATED sweep).
//!
//! Fixture values are F0086's bit-exact parameters (seed 30001).

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh};

const R: f64 = 1.4518544955342536;
const T: f64 = 0.4517828694874588;
const HR: f64 = 0.06748980564806449;
/// (cx, cy, cut depth) per hole — 3 through (depth > T), 2 blind.
const HOLES: [(f64, f64, f64); 5] = [
    (-0.4844834245158292, -0.3149130149828976, 1.1586804105234212),
    (
        -0.14355049103322348,
        -0.07372970251577235,
        1.0922233379071158,
    ),
    (0.0493293771266266, 0.7410538596365673, 1.046704),
    (0.8472894945087677, -0.7585876572737864, 0.214926),
    (0.5668457676559464, 1.0744567510873022, 0.300221),
];

fn cyl(a: &mut BrepArena, cx: f64, cy: f64, r: f64, z0: f64, z1: f64) -> kernel_v2::SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, z0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(cx, cy),
        r,
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z1 - z0)
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

fn run_chain(n_holes: usize) -> (BrepArena, kernel_v2::SolidId) {
    let mut a = BrepArena::new();
    let mut body = cyl(&mut a, 0.0, 0.0, R, 0.0, T);
    for (i, &(hx, hy, d)) in HOLES.iter().take(n_holes).enumerate() {
        let tool = cyl(&mut a, hx, hy, HR, 0.0, d);
        body = boolean_op(&mut a, body, tool, BoolOp::Subtract)
            .unwrap_or_else(|e| panic!("swiss-cheese cut {} failed: {e:?}", i + 1));
        validate_solid(&a, body).unwrap_or_else(|e| panic!("cut {} output invalid: {e:?}", i + 1));
    }
    (a, body)
}

/// Analytic volume with each circle discounted to its inscribed chord
/// polygon is scale-dependent; a 1.5% relative band around the ANALYTIC
/// volume rejects a dropped cap / doubled sheet / missing hole while
/// tolerating the Stage-1 chord deficit (N≈52 on the plate ⇒ 0.24%).
fn assert_volume(a: &BrepArena, s: kernel_v2::SolidId, n_holes: usize) {
    let mesh = tessellate(a, s).expect("tessellate");
    let vol = mesh_signed_volume(&mesh);
    let mut analytic = std::f64::consts::PI * R * R * T;
    for &(_, _, d) in HOLES.iter().take(n_holes) {
        analytic -= std::f64::consts::PI * HR * HR * d.min(T);
    }
    assert!(
        (vol - analytic).abs() / analytic < 0.015,
        "{n_holes}-hole plate volume {vol} outside band of analytic {analytic}"
    );
    assert!(vol > 0.0, "volume must be positive");
}

/// The minimal chained case: base disc + TWO through holes. The second cut
/// re-enters the recovered 1-hole plate — the F0086 step-2 wall.
#[test]
fn two_through_holes_chain() {
    let (a, s) = run_chain(2);
    assert_volume(&a, s, 2);
}

/// The full F0086 recipe: 3 through + 2 blind holes, all chained. GREEN
/// since M8 increment 6 (task #62): `rim_chord_ctxs` mints crossing points
/// on an ANNULAR face's rim circles (outer + per-hole), so chained outputs
/// carry pure on-circle z=0 rims that recover can circle-fuse and re-enter.
#[test]
fn full_f0086_five_hole_chain() {
    let (a, s) = run_chain(5);
    assert_volume(&a, s, 5);
}

/// Regression for the retired cut-3 re-entry wall (was: the pin
/// `third_cut_stays_loud_typed_reentry_wall`, which asserted the TYPED
/// `UnsupportedCurvedBoolean` boundary until M8 increment 6 lifted it):
/// the third chained cut — the first to re-enter a MULTI-hole recovered
/// plate — must succeed with a fully valid output.
#[test]
fn third_cut_reenters_multi_hole_plate() {
    let mut a = BrepArena::new();
    let mut body = cyl(&mut a, 0.0, 0.0, R, 0.0, T);
    for &(hx, hy, d) in HOLES.iter().take(2) {
        let tool = cyl(&mut a, hx, hy, HR, 0.0, d);
        body = boolean_op(&mut a, body, tool, BoolOp::Subtract).expect("first two cuts are green");
    }
    let (hx, hy, d) = HOLES[2];
    let tool = cyl(&mut a, hx, hy, HR, 0.0, d);
    let s = boolean_op(&mut a, body, tool, BoolOp::Subtract)
        .expect("cut 3 re-entry (multi-hole plate) regressed to an error");
    validate_solid(&a, s).expect("cut 3 succeeded but output is invalid");
}
