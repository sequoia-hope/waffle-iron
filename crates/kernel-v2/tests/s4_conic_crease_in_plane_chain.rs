//! The P0040 family on a CONIC crease: a crease seen from the PLANAR face that
//! contains it must be divided by the crease's OTHER surface, whatever curve
//! the crease is.
//!
//! P0040 (spec `yang_451_corner_transit.md` §3aj) was a circle crease — a
//! cylinder boss's start-cap rim lying IN the big boss's top cap — whose
//! divider was the circle's own plane: a zero residual at both ends of any
//! in-plane step, so a relocation that stepped from inside the rim to 9.9
//! outside it fired nowhere. The fix presented the circle's CYLINDER as the
//! divider. This fixture is the same corner one word over: the big boss's top
//! cap is OBLIQUE, so its rim is an ELLIPSE (the crease between the lateral
//! and the oblique cap), and the small boss's start cap meets that cap at the
//! ellipse. Every certificate that read "circle" reads "planar conic" here.
//!
//! Geometry: P0040's own numbers (a z-cylinder r 450.896 h 540.379 and an
//! x-cylinder r 290.089, x ∈ [−271.499, 39.046], axis at y −112.404,
//! z 370.941) with the big cap replaced by the plane through (0, 0, 540.379)
//! whose normal is tilted by `TILT` toward +x; the small boss is swept in y.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh, SolidId,
};

const A_R: f64 = 450.896;
const A_H: f64 = 540.379;
const B_R: f64 = 290.089;
const B_X0: f64 = -271.499;
const B_X1: f64 = 39.046;
const B_Y: f64 = -112.404;
const B_Z: f64 = 370.941;

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

fn big_boss(a: &mut BrepArena) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        A_R,
    )
    .expect("big boss profile");
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), A_H + 400.0)
        .expect("big boss extrude")
        .solid
}

/// The half-space above the oblique cap plane, as a thick slab.
fn oblique_cutter(a: &mut BrepArena, tilt: f64) -> SolidId {
    let n = norm([tilt, 0.0, 1.0]);
    let (s, t) = frame(n);
    let p = Profile::new(
        Point3::new(0.0, 0.0, A_H),
        v3(s),
        v3(t),
        vec![
            Point2::new(-3000.0, -3000.0),
            Point2::new(3000.0, -3000.0),
            Point2::new(3000.0, 3000.0),
            Point2::new(-3000.0, 3000.0),
        ],
        vec![],
    )
    .expect("cutter profile");
    extrude(a, &p, v3(n), 2000.0).expect("cutter extrude").solid
}

fn small_boss(a: &mut BrepArena, y: f64) -> SolidId {
    let p = Profile::circle(
        Point3::new(B_X0, y, B_Z),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        Point2::new(0.0, 0.0),
        B_R,
    )
    .expect("small boss profile");
    extrude(a, &p, Vector3::new(1.0, 0.0, 0.0), B_X1 - B_X0)
        .expect("small boss extrude")
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

fn run(tilt: f64, y: f64) -> String {
    let mut a = BrepArena::new();
    let big = big_boss(&mut a);
    let cut = oblique_cutter(&mut a, tilt);
    let capped = match boolean_op(&mut a, big, cut, BoolOp::Subtract) {
        Ok(s) => s,
        Err(e) => return format!("CUT FAILED: {e:?}"),
    };
    if let Err(e) = validate_solid(&a, capped) {
        return format!("CUT INVALID: {e:?}");
    }
    let small = small_boss(&mut a, y);
    match boolean_op(&mut a, capped, small, BoolOp::Union) {
        Err(e) => format!("UNION ERR: {e:?}"),
        Ok(out) => match validate_solid(&a, out) {
            Err(e) => format!("UNION INVALID: {e:?}"),
            Ok(r) => match tessellate(&a, out) {
                Err(e) => format!("TESS ERR: {e:?}"),
                Ok(m) => format!(
                    "ok shells={} chi={}/{} vol={:.6e}",
                    r.shells,
                    r.euler_lhs,
                    r.euler_rhs,
                    mesh_signed_volume(&m)
                ),
            },
        },
    }
}

/// The oblique-cap analogue of P0040's corner: the small boss's start cap meets
/// the big boss's OBLIQUE cap at the ellipse rim, the corner sitting ~9.6
/// inside the rim as P0040's did inside its circle. CORRECT before and after
/// the 2026-10-13 generalization (measured: the ellipse rim's Stage-1 chords
/// leave no phantom here); the pin holds the family's shape in the suite so
/// the certificates' conic arms have a real customer to run on.
#[test]
fn the_oblique_cap_boss_corner_builds_and_validates() {
    let verdict = run(0.15, -150.5);
    assert!(verdict.starts_with("ok shells=1 chi=2/2"), "{verdict}");
}

/// FOLLOW-UP (found by the P0040 generalization's anchor sweep, 2026-10-13):
/// a skew cylinder boss whose far generator GRAZES the big boss's lateral from
/// inside — the small cylinder's reach `|y| + r_B` within a few units of the
/// big radius 450.896 (external tangency at y = −160.807) — STOPs loudly at
/// three different sites across y ∈ [−167.4, −159.9] with the oblique cap
/// (`InvalidBooleanOutput` Newell-normal disagreement at −167.4,
/// `TessellationFailed "surface-pair projection left the chord neighborhood"`
/// at −164.9 / −162.4 — `KV2_PAIR_SAMPLE_PROBE=1`: a cyl × cyl pair chord of
/// length 65.9 whose sample projected 75.8 away — and `ring rejected by CDT`
/// at −159.9), and once with a FLAT cap (`reassembled output would be
/// non-2-manifold` at −159.9). Not the P0040 class (no crease is crossed; the
/// census probes are silent); the Case-III / Case-V graze family of
/// `yang_172_case_iii_graze_guard.md` with the small operand INSIDE the big
/// one. Anchor it against the guard's demand before any remedy.
#[test]
#[ignore = "follow-up: internal skew-cylinder graze window — a graze-family STOP, not P0040's"]
fn an_internal_skew_cylinder_graze_window_stops_loudly() {
    for (tilt, y) in [
        (0.15, -167.404),
        (0.15, -162.404),
        (0.15, -159.904),
        (0.0, -159.904),
    ] {
        let verdict = run(tilt, y);
        assert!(
            verdict.starts_with("UNION ERR"),
            "tilt {tilt} y {y}: expected the graze STOP, got {verdict}"
        );
    }
}

/// PROBE (anchor sweep): the P0040 configuration with an oblique big cap, the
/// small boss swept in y across the ellipse rim. Run with
/// `--ignored --nocapture`; one line per fixture.
#[test]
#[ignore = "anchor sweep — one line per fixture, read the output"]
fn probe_oblique_cap_sweep() {
    for &tilt in &[0.15, 0.3] {
        for k in -4..=4 {
            let y = B_Y + 25.0 * k as f64;
            let t0 = std::time::Instant::now();
            let verdict = run(tilt, y);
            eprintln!(
                "[conic-crease] tilt={tilt} y={y:.3} {:.1}s  {verdict}",
                t0.elapsed().as_secs_f64()
            );
        }
    }
}

/// PROBE: a finer sweep around the failing y of `probe_oblique_cap_sweep`.
#[test]
#[ignore = "anchor sweep — one line per fixture, read the output"]
fn probe_oblique_cap_fine_sweep() {
    let tilt: f64 = std::env::var("PROBE_TILT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.15);
    let y0: f64 = std::env::var("PROBE_Y0")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(-177.404);
    let step: f64 = std::env::var("PROBE_STEP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.5);
    let n: i32 = std::env::var("PROBE_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12);
    for k in 0..=n {
        let y = y0 + step * k as f64;
        let t0 = std::time::Instant::now();
        let verdict = run(tilt, y);
        eprintln!(
            "[conic-crease-fine] tilt={tilt} y={y:.3} {:.1}s  {verdict}",
            t0.elapsed().as_secs_f64()
        );
    }
}

/// PROBE: one fixture, `PROBE_TILT` / `PROBE_Y`, for running under the yang
/// census knobs.
#[test]
#[ignore = "anchor probe — one fixture under the census knobs"]
fn probe_oblique_cap_one() {
    let tilt: f64 = std::env::var("PROBE_TILT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.15);
    let y: f64 = std::env::var("PROBE_Y")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(-162.404);
    eprintln!("[conic-crease-one] tilt={tilt} y={y:.3} {}", run(tilt, y));
}
