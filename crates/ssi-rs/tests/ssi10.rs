//! PR-SSI10 — RED tests for the cylinder∩cylinder PARALLEL-axis solver.
//!
//! cylinder∩cylinder is the **last** degree-4 dispatch arm still returning
//! `Err(AnalyticalSolutionNotAvailable)` for an unhandled *pair*. The general
//! cyl∩cyl intersection is a degree-4 space curve, but the **parallel-axis**
//! configuration reduces to **circle∩circle** (centre distance `d`, radii
//! `r₁, r₂`) lifted along the shared axis `û` → **lines** parallel to `û`.
//! These tests target that new behavior via the public
//! `intersect(cylinder, cylinder)` dispatcher (the solver is private). The
//! non-parallel case (general degree-4, incl. equal-R intersecting → ellipses)
//! stays a loud `Err(AnalyticalSolutionNotAvailable)` — staged, never a
//! fallback (A15.2); that increment is PR-SSI11.
//!
//! Spec: specs/ssi_pr_ssi10_cylinder_cylinder_parallel.md
//! Research basis: Patrikalakis & Maekawa, *Shape Interrogation for CAD/M*,
//! §5.8 (Surface/Surface Intersections — natural quadrics).
//!
//! The math (parallel): `û = û₁ = normalize(cyl₁.axis_dir)`, reference point
//! `c₁ = Q₁ = cyl₁.axis_point`. `rel = Q₂ − Q₁`, inter-axis perp distance
//! `d = |rel − (rel·û)·û|`. This is circle∩circle in the plane ⟂ û through Q₁:
//!   n̂ = unit(rel − (rel·û)·û)          (unit perp component of rel; d>0 only)
//!   a = (d² + r₁² − r₂²) / (2d)        (chord offset along n̂)
//!   h = √(max(0, r₁² − a²))            (half-chord; clamp absorbs ε)
//!   p̂ = û × n̂                          (unit, since û ⟂ n̂)
//! two cross-section points `Q₁ + a·n̂ ± h·p̂`, each lifted to
//! `Line { point, dir = û }`. For `x = Q₁ + a·n̂ ± h·p̂`: perp-dist to axis 1
//! = √(a²+h²) = r₁; perp-dist to axis 2 = √((a−d)²+h²) = r₂.
//!
//! Branches (gate on the LINEAR quantity `d`):
//!   E1   (degenerate: rᵢ ≤ 0 / non-finite, or zero/non-finite axis, either cyl → Err),
//!   NP   (non-parallel: |û₁ × û₂| ≥ TAU_MODEL → ASNA, staged, never a fallback),
//!   COIN (parallel, d ≤ TAU AND |r₁−r₂| ≤ TAU → Err(DegenerateInput); overlap is 2D),
//!   CONC (parallel, d ≤ TAU AND |r₁−r₂| > TAU → Ok(vec![]) empty),
//!   EMPTY(parallel, d>0, d > r₁+r₂+TAU OR d < |r₁−r₂|−TAU → Ok(vec![])),
//!   TAN  (parallel, d>0, |d−(r₁+r₂)| ≤ TAU OR |d−|r₁−r₂|| ≤ TAU → one Line at Q₁+a·n̂),
//!   SEC  (parallel, d>0, otherwise → two Lines at Q₁+a·n̂ ± h·p̂; +h·p̂ first).
//! Invariants:
//!   I1 (on-surface: each cylinder's own radial residual),
//!   I2 (analytical geometry: dir ∥ û unit; symmetric ±h·p̂ about Q₁+a·n̂; a formula),
//!   I3 (branch coverage), I4 (symmetry as a line SET), I5 (determinism, +h·p̂ first).
//!
//! These FAIL now (RED): production returns
//! `Err(AnalyticalSolutionNotAvailable)` for every cylinder∩cylinder pair. A
//! separate Implementer makes them pass.

use cad_primitives::{Point3, Vector3, TAU_MODEL};
use ssi_rs::{intersect, QuadricSurface, SsiCurve, SsiError};

// ---------------------------------------------------------------------------
// Inline vector helpers (cad-primitives has no dot/cross/norm).
// ---------------------------------------------------------------------------

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    scale(a, 1.0 / norm(a))
}

// ---------------------------------------------------------------------------
// On-surface oracle (I1). Samples each Line at several `t` (covering negative,
// zero, positive) and asserts every sample satisfies BOTH input cylinders
// within TAU_MODEL.
//   cylinder radial residual: | dist(x, axis line) − r |, evaluated against
//     EACH cylinder's own axis_point/axis_dir/radius.
// (The `Cylinder` branch is verbatim from ssi9; the whole match is carried.)
// ---------------------------------------------------------------------------

fn implicit_residual(surf: &QuadricSurface, x: [f64; 3]) -> f64 {
    match surf {
        QuadricSurface::Sphere { center, radius } => {
            (norm(sub(x, center.as_array())) - radius).abs()
        }
        QuadricSurface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let q = axis_point.as_array();
            let ahat = unit(axis_dir.as_array());
            let rel = sub(x, q);
            let along = scale(ahat, dot(rel, ahat));
            let perp = sub(rel, along);
            (norm(perp) - radius).abs()
        }
        QuadricSurface::Plane { point, normal } => {
            dot(unit(normal.as_array()), sub(x, point.as_array())).abs()
        }
        QuadricSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let ahat = unit(axis_dir.as_array());
            let rel = sub(x, apex.as_array());
            let h = dot(rel, ahat);
            let along = scale(ahat, h);
            let r_actual = norm(sub(rel, along));
            (r_actual - h.abs() * half_angle.tan()).abs()
        }
    }
}

fn assert_on_both_surfaces(curve: &SsiCurve, a: &QuadricSurface, b: &QuadricSurface) {
    // Lines are infinite; sample t over a spread covering negative, zero, positive.
    let ts = [-100.0, -10.0, -1.0, 0.0, 1.0, 10.0, 100.0];
    for &t in ts.iter() {
        let p = curve.eval(t).as_array();
        let ra = implicit_residual(a, p);
        let rb = implicit_residual(b, p);
        assert!(
            ra < TAU_MODEL,
            "sample t={t} at {p:?} off surface A (residual {ra} >= TAU_MODEL)"
        );
        assert!(
            rb < TAU_MODEL,
            "sample t={t} at {p:?} off surface B (residual {rb} >= TAU_MODEL)"
        );
    }
}

// ---------------------------------------------------------------------------
// Extractors / approx helpers.
// ---------------------------------------------------------------------------

fn expect_two_lines(curves: &[SsiCurve]) -> [(Point3, Vector3); 2] {
    assert_eq!(
        curves.len(),
        2,
        "expected exactly two curves, got {curves:?}"
    );
    let mut out = [(Point3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 0.0)); 2];
    for (i, c) in curves.iter().enumerate() {
        match c {
            SsiCurve::SurfacePair { .. } => unreachable!(
                "this suite's solvers never produce a surface-pair curve (M5 cyl×cyl only)"
            ),
            SsiCurve::Line { point, dir } => out[i] = (*point, *dir),
            other => panic!("expected Line, got {other:?}"),
        }
    }
    out
}

fn expect_one_line(curves: &[SsiCurve]) -> (Point3, Vector3) {
    assert_eq!(
        curves.len(),
        1,
        "expected exactly one curve, got {curves:?}"
    );
    match &curves[0] {
        SsiCurve::Line { point, dir } => (*point, *dir),
        other => panic!("expected Line, got {other:?}"),
    }
}

fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < TAU_MODEL, "expected {a} ≈ {b}");
}

// Provided for parity with ssi9's helper set; the line tests assert on the
// perpendicular (cross-section) component rather than the full point because a
// line `point`'s along-axis coordinate is solver-chosen.
#[allow(dead_code)]
fn approx_point(a: Point3, b: [f64; 3]) {
    assert!(
        norm(sub(a.as_array(), b)) < TAU_MODEL,
        "expected point {:?} ≈ {b:?}",
        a.as_array()
    );
}

// Unit vectors equal up to sign (|cross| ≈ 0).
fn parallel_up_to_sign(a: [f64; 3], b: [f64; 3]) {
    let c = cross(a, b);
    assert!(
        norm(c) < TAU_MODEL,
        "expected {a:?} parallel to {b:?} (|cross| = {})",
        norm(c)
    );
}

// ---------------------------------------------------------------------------
// line_key (I4 set-comparison). A canonical key for a Line, tolerant of:
//   (1) direction up to sign — orient d̂ into a canonical hemisphere (first
//       non-near-zero component positive), the ssi9 circle_key scheme;
//   (2) point-on-line — reduce the point to its foot ⟂ dir from the origin:
//       foot = point − (point·d̂)·d̂, so any point on the same line maps the same.
// Then quantize foot + oriented-dir components to TAU_MODEL units.
// ---------------------------------------------------------------------------

fn line_key(point: Point3, dir: Vector3) -> (i64, i64, i64, i64, i64, i64) {
    let d = unit(dir.as_array());
    // Orient direction deterministically (first non-near-zero component positive).
    let s = if d[0] > 1e-9 {
        1.0
    } else if d[0] < -1e-9 {
        -1.0
    } else if d[1] > 1e-9 {
        1.0
    } else if d[1] < -1e-9 {
        -1.0
    } else if d[2] >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let d = scale(d, s);
    // Foot of perpendicular from the origin onto the line (point-on-line invariant).
    let p = point.as_array();
    let foot = sub(p, scale(d, dot(p, d)));
    let q = |v: f64| (v / TAU_MODEL).round() as i64;
    (
        q(foot[0]),
        q(foot[1]),
        q(foot[2]),
        q(d[0]),
        q(d[1]),
        q(d[2]),
    )
}

// Collect a sorted Vec of line_keys from an Ok result (panics on non-Line).
fn key_set(curves: &[SsiCurve]) -> Vec<(i64, i64, i64, i64, i64, i64)> {
    let mut keys: Vec<_> = curves
        .iter()
        .map(|c| match c {
            SsiCurve::Line { point, dir } => line_key(*point, *dir),
            other => panic!("expected Line, got {other:?}"),
        })
        .collect();
    keys.sort();
    keys
}

// ---------------------------------------------------------------------------
// SEC canonical (spec case 1, 3-4-5) — cyl₁ Q=origin û=+z r₁=5; cyl₂ Q=(8,0,0)
// û=+z r₂=5; d=8. a = (64+25−25)/16 = 4, h = √(25−16) = 3 ⇒ points (4,±3,*),
// two lines dir +z. n̂ = +x, p̂ = û×n̂ = +y ⇒ +h·p̂ first ⇒ curves[0] at (4,+3,*),
// curves[1] at (4,−3,*).
// ---------------------------------------------------------------------------

#[test]
fn sec_canonical_two_lines() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(8.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let curves = intersect(&cyl1, &cyl2).expect("parallel cyl/cyl: two lines");
    let lines = expect_two_lines(&curves);

    // I1: both lines lie on BOTH cylinders.
    assert_on_both_surfaces(&curves[0], &cyl1, &cyl2);
    assert_on_both_surfaces(&curves[1], &cyl1, &cyl2);

    // I5 order: +h·p̂ first ⇒ curves[0] at (4,+3,*), curves[1] at (4,−3,*).
    // The point's perp-to-û (z) component (its x,y) equals (4,±3).
    let uhat = [0.0, 0.0, 1.0];
    for (i, expected_xy) in [(4.0, 3.0), (4.0, -3.0)].iter().enumerate() {
        let (point, dir) = lines[i];
        // dir ∥ +z and unit (I2/I5).
        parallel_up_to_sign(dir.as_array(), uhat);
        approx(norm(dir.as_array()), 1.0);
        // The point lies on the expected line: its (x,y) perp component is (4,±3).
        let p = point.as_array();
        let perp = sub(p, scale(uhat, dot(p, uhat))); // drop the z component
        approx(perp[0], expected_xy.0);
        approx(perp[1], expected_xy.1);
    }
}

// ---------------------------------------------------------------------------
// TAN external (spec case 2) — cyl₁ r=2 @origin +z; cyl₂ r=2 @(4,0,0) +z;
// d=4 = r₁+r₂ ⇒ ONE line at (2,0,*), dir +z.
// ---------------------------------------------------------------------------

#[test]
fn tan_external_one_line() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(4.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let curves = intersect(&cyl1, &cyl2).expect("external tangent: one line");
    let (point, dir) = expect_one_line(&curves);

    assert_on_both_surfaces(&curves[0], &cyl1, &cyl2);

    // a = (16+4−4)/8 = 2 ⇒ point at (2,0,*); dir ∥ +z, unit.
    let uhat = [0.0, 0.0, 1.0];
    parallel_up_to_sign(dir.as_array(), uhat);
    approx(norm(dir.as_array()), 1.0);
    let p = point.as_array();
    let perp = sub(p, scale(uhat, dot(p, uhat)));
    approx(perp[0], 2.0);
    approx(perp[1], 0.0);
}

// ---------------------------------------------------------------------------
// TAN internal (spec case 3) — unequal radii with d = |r₁−r₂|. r₁=5 @origin,
// r₂=2 @(3,0,0), d=3 = |5−2| ⇒ ONE line. a = (9+25−4)/6 = 5, h = √(25−25) = 0
// ⇒ point (5,0,*): dist 5 from axis1, dist |5−3|=2 from axis2.
// ---------------------------------------------------------------------------

#[test]
fn tan_internal_one_line() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(3.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let curves = intersect(&cyl1, &cyl2).expect("internal tangent: one line");
    let (point, dir) = expect_one_line(&curves);

    assert_on_both_surfaces(&curves[0], &cyl1, &cyl2);

    let uhat = [0.0, 0.0, 1.0];
    parallel_up_to_sign(dir.as_array(), uhat);
    approx(norm(dir.as_array()), 1.0);
    let p = point.as_array();
    let perp = sub(p, scale(uhat, dot(p, uhat)));
    approx(perp[0], 5.0);
    approx(perp[1], 0.0);
}

// ---------------------------------------------------------------------------
// EMPTY disjoint (spec case 3) — cyl₁ r=1 @origin; cyl₂ r=1 @(5,0,0); d=5 >
// r₁+r₂=2 ⇒ Ok(vec![]).
// ---------------------------------------------------------------------------

#[test]
fn empty_disjoint() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(5.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    assert_eq!(intersect(&cyl1, &cyl2), Ok(Vec::new()));
    assert_eq!(intersect(&cyl2, &cyl1), Ok(Vec::new()));
}

// ---------------------------------------------------------------------------
// EMPTY contained — d < |r₁−r₂|. r₁=5 @origin, r₂=1 @(1,0,0), d=1 < 4 ⇒
// Ok(vec![]) (cyl₂ strictly inside cyl₁, no intersection curve).
// ---------------------------------------------------------------------------

#[test]
fn empty_contained() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(1.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    assert_eq!(intersect(&cyl1, &cyl2), Ok(Vec::new()));
    assert_eq!(intersect(&cyl2, &cyl1), Ok(Vec::new()));
}

// ---------------------------------------------------------------------------
// COIN coincident (spec case 4) — identical cylinders (same axis, same r) ⇒
// Err(DegenerateInput) (overlap is a 2D surface, not a curve).
// ---------------------------------------------------------------------------

#[test]
fn coin_coincident_is_degenerate() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 3.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 3.0,
    };
    assert_eq!(intersect(&cyl1, &cyl2), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// CONC concentric (spec case 4) — same axis line, unequal r (r₁=1, r₂=2 both
// @origin +z) ⇒ Ok(vec![]) (empty).
// ---------------------------------------------------------------------------

#[test]
fn conc_concentric_is_empty() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    assert_eq!(intersect(&cyl1, &cyl2), Ok(Vec::new()));
    assert_eq!(intersect(&cyl2, &cyl1), Ok(Vec::new()));
}

// ---------------------------------------------------------------------------
// NP non-parallel (spec case 5) — cyl₂ û=+x (⟂ cyl₁ +z) ⇒ ASNA (staged).
// Both argument orders.
// ---------------------------------------------------------------------------

// Contract migration (PR-SSI11): the original NP probe used two EQUAL-radius
// perpendicular intersecting cylinders, which PR-SSI11 now solves analytically
// (→ two ellipses). To preserve this test's intent — "a non-parallel cyl∩cyl
// configuration the solver does NOT reduce to lines/ellipses" — the radii are
// UNEQUAL (2 vs 3): unequal-radius non-parallel axes are the general degree-4
// curve, now returned as the procedural surface-pair descriptor (M5,
// specs/m5_surface_pair_curve.md; supersedes the staged ASNA). The descriptor
// carries the two operands VERBATIM in argument order, so equality also pins
// operand ordering (a = first arg, b = second).
#[test]
fn np_non_parallel_yields_surface_pair() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0), // ⟂ cyl₁ axis ⇒ |û₁ × û₂| = 1
        radius: 3.0,                           // ≠ cyl₁ r ⇒ general degree-4 → SurfacePair
    };
    assert_eq!(
        intersect(&cyl1, &cyl2),
        Ok(vec![SsiCurve::SurfacePair { a: cyl1, b: cyl2 }])
    );
    assert_eq!(
        intersect(&cyl2, &cyl1),
        Ok(vec![SsiCurve::SurfacePair { a: cyl2, b: cyl1 }])
    );
}

// ---------------------------------------------------------------------------
// E1 — degenerate inputs → Err(DegenerateInput) (failure modes, I3).
// r=0 (each cyl), negative r, zero axis_dir (each cyl).
// ---------------------------------------------------------------------------

#[test]
fn e1_cyl1_zero_radius_is_degenerate() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 0.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(8.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    assert_eq!(intersect(&cyl1, &cyl2), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_cyl2_zero_radius_is_degenerate() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(8.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 0.0,
    };
    assert_eq!(intersect(&cyl1, &cyl2), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_cyl1_negative_radius_is_degenerate() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: -5.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(8.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    assert_eq!(intersect(&cyl1, &cyl2), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_cyl1_axis_dir_is_degenerate() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 0.0), // zero axis
        radius: 5.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(8.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    assert_eq!(intersect(&cyl1, &cyl2), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_cyl2_axis_dir_is_degenerate() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(8.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 0.0), // zero axis
        radius: 5.0,
    };
    assert_eq!(intersect(&cyl1, &cyl2), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// Non-unit axis — SEC canonical (3-4-5) but axis_dir=(0,0,5) on both ⇒
// defensive normalization ⇒ identical result, dirs still unit.
// ---------------------------------------------------------------------------

#[test]
fn sec_nonunit_axis() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0), // non-unit, +z
        radius: 5.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(8.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0), // non-unit, +z
        radius: 5.0,
    };
    let curves = intersect(&cyl1, &cyl2).expect("non-unit axis: two lines");
    let lines = expect_two_lines(&curves);

    assert_on_both_surfaces(&curves[0], &cyl1, &cyl2);
    assert_on_both_surfaces(&curves[1], &cyl1, &cyl2);

    let uhat = [0.0, 0.0, 1.0];
    for (i, expected_xy) in [(4.0, 3.0), (4.0, -3.0)].iter().enumerate() {
        let (point, dir) = lines[i];
        parallel_up_to_sign(dir.as_array(), uhat);
        approx(norm(dir.as_array()), 1.0); // normalized despite |axis_dir|=5
        let p = point.as_array();
        let perp = sub(p, scale(uhat, dot(p, uhat)));
        approx(perp[0], expected_xy.0);
        approx(perp[1], expected_xy.1);
    }
}

// ---------------------------------------------------------------------------
// Oblique off-origin — shared axis û = normalize((1,2,2)); cyl₁ axis_point =
// (1,1,1); cyl₂ axis_point = cyl₁ + offset, where offset = perp·d̂_perp +
// along·û (a perpendicular component of magnitude d=8 plus a non-zero
// along-axis component that must NOT change the result). Radii r₁=r₂=5 ⇒ SEC
// (3-4-5: a=4, h=3). Assert on both surfaces, dirs ∥ û, centre-line symmetry.
// ---------------------------------------------------------------------------

#[test]
fn sec_oblique_off_origin() {
    let uhat = unit([1.0, 2.0, 2.0]);
    let q1 = [1.0, 1.0, 1.0];
    // A unit vector ⟂ û (n̂). Take û × (arbitrary), normalize.
    let nhat = unit(cross(uhat, [1.0, 0.0, 0.0]));
    // Confirm n̂ ⟂ û (test sanity).
    assert!(dot(nhat, uhat).abs() < 1e-12);
    let d = 8.0_f64;
    let along = 3.7_f64; // non-zero along-axis component (must not matter)
    let q2 = add(q1, add(scale(nhat, d), scale(uhat, along)));
    let r = 5.0_f64;

    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::from(q1),
        axis_dir: Vector3::new(1.0, 2.0, 2.0), // non-unit, oblique
        radius: r,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::from(q2),
        axis_dir: Vector3::new(1.0, 2.0, 2.0), // ∥ cyl₁ axis
        radius: r,
    };
    let curves = intersect(&cyl1, &cyl2).expect("oblique off-origin: two lines");
    let lines = expect_two_lines(&curves);

    assert_on_both_surfaces(&curves[0], &cyl1, &cyl2);
    assert_on_both_surfaces(&curves[1], &cyl1, &cyl2);

    // a = (d²+r₁²−r₂²)/(2d) = 64/16 = 4, h = √(25−16) = 3. Centre line at
    // Q₁ + a·n̂; the two points are symmetric ±h·p̂ about it (p̂ = û × n̂).
    let a = 4.0_f64;
    let h = 3.0_f64;
    let centre = add(q1, scale(nhat, a));
    let phat = cross(uhat, nhat);

    for (point, dir) in lines.iter() {
        parallel_up_to_sign(dir.as_array(), uhat);
        approx(norm(dir.as_array()), 1.0);
        // The point's foot ⟂ û from the centre line lies at ±h along p̂.
        // Project (point − centre) onto the plane ⟂ û, decompose on (n̂, p̂).
        let rel = sub(point.as_array(), centre);
        let rel_perp = sub(rel, scale(uhat, dot(rel, uhat)));
        let on_n = dot(rel_perp, nhat); // should be ≈ 0 (centre is at a·n̂)
        let on_p = dot(rel_perp, phat); // should be ±h
        approx(on_n, 0.0);
        approx(on_p.abs(), h);
    }
    // Symmetry: the two +h and −h feet are opposite ⇒ they sum to the centre line.
    let f0 = lines[0].0.as_array();
    let f1 = lines[1].0.as_array();
    let mid = scale(add(f0, f1), 0.5);
    // Midpoint of the two line points lies on the centre line (perp-to-û = a·n̂).
    let mid_rel = sub(mid, q1);
    let mid_perp = sub(mid_rel, scale(uhat, dot(mid_rel, uhat)));
    approx(dot(mid_perp, nhat), a);
    approx(dot(mid_perp, phat), 0.0);
}

// ---------------------------------------------------------------------------
// I4 — symmetry: intersect(c1,c2) == intersect(c2,c1) as a line SET (order /
// point-on-line / dir-sign tolerant via line_key) for SEC and TAN configs.
// ---------------------------------------------------------------------------

#[test]
fn symmetry_sec_line_set() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(8.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let ab = intersect(&cyl1, &cyl2).expect("ab two lines");
    let ba = intersect(&cyl2, &cyl1).expect("ba two lines");
    assert_eq!(
        key_set(&ab),
        key_set(&ba),
        "SEC line SET must match across argument order"
    );
}

#[test]
fn symmetry_tan_line_set() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(4.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let ab = intersect(&cyl1, &cyl2).expect("ab one line");
    let ba = intersect(&cyl2, &cyl1).expect("ba one line");
    assert_eq!(
        key_set(&ab),
        key_set(&ba),
        "TAN line SET must match across argument order"
    );
}

// ---------------------------------------------------------------------------
// Antiparallel axis — flipping û₂ to −û₁ must not change the line SET
// (cylinder is symmetric under û → −û). SEC canonical config.
// ---------------------------------------------------------------------------

#[test]
fn antiparallel_axis_set_invariant() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let cyl2_pos = QuadricSurface::Cylinder {
        axis_point: Point3::new(8.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let cyl2_neg = QuadricSurface::Cylinder {
        axis_point: Point3::new(8.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0), // antiparallel
        radius: 5.0,
    };
    let pos = intersect(&cyl1, &cyl2_pos).expect("two lines (parallel)");
    let neg = intersect(&cyl1, &cyl2_neg).expect("two lines (antiparallel)");
    assert_eq!(
        key_set(&pos),
        key_set(&neg),
        "antiparallel û₂ must leave the line SET unchanged"
    );
}

// ---------------------------------------------------------------------------
// I5 — determinism: identical inputs → byte-identical output, +h·p̂ first.
// ---------------------------------------------------------------------------

#[test]
fn determinism_sec_identical() {
    let cyl1 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(8.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 5.0,
    };
    let first = intersect(&cyl1, &cyl2);
    let second = intersect(&cyl1, &cyl2);
    // Byte-identical structurally (PartialEq over the exact fields).
    assert_eq!(first, second, "two-line output must be deterministic");

    let cf = first.expect("two lines");
    // +h·p̂ first: curves[0] point's +y (perp) component is +3 (h·p̂ = +y here).
    match cf[0] {
        SsiCurve::Line { point, .. } => {
            let p = point.as_array();
            approx(p[1], 3.0);
        }
        other => panic!("expected Line, got {other:?}"),
    }

    // Identical at a fixed eval parameter.
    let cs = second.expect("two lines");
    let t = 0.7;
    assert_eq!(cf[0].eval(t).as_array(), cs[0].eval(t).as_array());
    assert_eq!(cf[1].eval(t).as_array(), cs[1].eval(t).as_array());
}
