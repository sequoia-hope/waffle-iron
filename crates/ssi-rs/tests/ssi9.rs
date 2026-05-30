//! PR-SSI9 — RED tests for the cone∩cone coaxial solver.
//!
//! Fourth and **last** circle-reducible degree-4 quadric∩quadric pair (after
//! PR-SSI6 sphere∩cylinder, PR-SSI7 sphere∩cone, PR-SSI8 cylinder∩cone). The
//! general cone∩cone intersection is a degree-4 space curve, but the **coaxial**
//! configuration (the two axis *lines* coincide) reduces to **one or two
//! circles** — exact, reusing `SsiCurve::Circle`. These tests target the new
//! coaxial behavior via the public `intersect(cone, cone)` dispatcher (the
//! solver is private). The non-coaxial case stays a loud
//! `Err(AnalyticalSolutionNotAvailable)` (staged; general degree-4 deferred).
//!
//! Spec: specs/ssi_pr_ssi9_cone_cone_coaxial.md
//! Research basis: Patrikalakis & Maekawa, *Shape Interrogation for CAD/M*,
//! §5.8.3 (Case F8, implicit/implicit quadric pair).
//!
//! The math (coaxial): cone₁ apex `P₁`, unit axis `â = normalize(cone₁.axis_dir)`,
//! half-angle `α₁`, `m₁ = tanα₁`. cone₂ apex `P₂`, axis ∥ `â`, half-angle `α₂`,
//! `m₂ = tanα₂`. Signed apex offset `δ = (P₂ − P₁)·â`; axial height
//! `t = (x − P₁)·â`. A point lies on both cones iff `|t|·m₁ = |t−δ|·m₂`. Both
//! sides ≥ 0, so squaring is an exact equivalence:
//! `(m₁²−m₂²)·t² + 2·m₂²·δ·t − m₂²·δ² = 0`, discriminant `(2·m₁·m₂·δ)²` (a
//! **perfect square** ⇒ always real). Each circle:
//! `center = P₁ + t·â`, `normal = â`, `radius = |t|·m₁ = |t−δ|·m₂`.
//!
//! CRITICAL anti-hack point (P9/P10): there is **NO √D sign gate, NO manufactured
//! tangent/empty sub-branch** — the discriminant is a perfect square, so the
//! unequal-α (`|α₁−α₂| > TAU_MODEL`) coaxial case with `|δ| > TAU_MODEL` is
//! **ALWAYS exactly two circles**. The only empty/degenerate outcomes come from
//! the geometric `δ → 0` apex collapse (X0 / CO), gated on the linear quantity
//! `|δ|`, and the equal-vs-unequal half-angle split, gated on `|α₁−α₂|`. An
//! explicit anti-hack invariant (I3) sweeps unequal-α × δ≠0 configs and asserts
//! `len == 2` every time.
//!
//! Branches:
//!   X2 (coaxial, unequal α, |δ|>TAU → two circles, larger-t first),
//!   X1 (coaxial, equal α, |δ|>TAU → one circle at the bisector t=δ/2),
//!   X0 (coaxial, unequal α, |δ|≤TAU → Ok(vec![]) — radius-0 apex point-circle),
//!   CO (coaxial, equal α, |δ|≤TAU → Err(DegenerateInput) — identical double cone),
//!   NC (non-coaxial: apex₂ off axis₁ OR non-parallel axes → ASNA, staged),
//!   E1 (degenerate: bad α low/high either cone, zero axis either cone → Err).
//! Invariants:
//!   I1 (on-surface: each cone's own radial residual),
//!   I2 (analytical geometry: centers on shared axis, normal ∥ â, radius equal by
//!       both formulas, X2 roots, X1 bisector),
//!   I3 (branch coverage + ANTI-HACK: unequal-α coaxial is always two circles),
//!   I4 (symmetry as a set), I5 (determinism, larger-t first).
//!
//! These FAIL now (RED): production returns `Err(AnalyticalSolutionNotAvailable)`
//! for every cone∩cone pair. A separate Implementer makes them pass.

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
// On-surface oracle (I1). Samples each Circle at N params over [0, 2π) and
// asserts every sample satisfies BOTH input cones within TAU_MODEL.
//   cone radial residual: | |(x − Pᵢ) − ((x − Pᵢ)·âᵢ)·âᵢ| − |hᵢ|·tanαᵢ |,
//     hᵢ = (x − Pᵢ)·âᵢ — evaluated against EACH cone's own apex/axis/half_angle.
// (The `Cone` branch already works for any cone; reused verbatim from ssi8.)
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
    const N: usize = 64;
    for i in 0..N {
        // All curves produced by cone∩cone (coaxial) are Circles, sampled over [0, 2π).
        let t = (i as f64) / (N as f64) * std::f64::consts::TAU;
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

fn expect_two_circles(curves: &[SsiCurve]) -> [(Point3, Vector3, f64); 2] {
    assert_eq!(
        curves.len(),
        2,
        "expected exactly two curves, got {curves:?}"
    );
    let mut out = [(Point3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 0.0), 0.0); 2];
    for (i, c) in curves.iter().enumerate() {
        match c {
            SsiCurve::Circle {
                center,
                normal,
                radius,
            } => out[i] = (*center, *normal, *radius),
            other => panic!("expected Circle, got {other:?}"),
        }
    }
    out
}

fn expect_one_circle(curves: &[SsiCurve]) -> (Point3, Vector3, f64) {
    assert_eq!(
        curves.len(),
        1,
        "expected exactly one curve, got {curves:?}"
    );
    match &curves[0] {
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => (*center, *normal, *radius),
        other => panic!("expected Circle, got {other:?}"),
    }
}

fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < TAU_MODEL, "expected {a} ≈ {b}");
}

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

// Canonical key for set-comparison of circles, sign-/order-tolerant: a circle
// is identified by its center, |radius|, and its normal LINE (axis up to sign,
// so we orient the normal into a canonical hemisphere before rounding).
fn circle_key(center: Point3, normal: Vector3, radius: f64) -> (i64, i64, i64, i64, i64, i64, i64) {
    let n = unit(normal.as_array());
    // Orient normal deterministically (first non-near-zero component positive).
    let s = if n[0] > 1e-9 {
        1.0
    } else if n[0] < -1e-9 {
        -1.0
    } else if n[1] > 1e-9 {
        1.0
    } else if n[1] < -1e-9 {
        -1.0
    } else if n[2] >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let n = scale(n, s);
    let q = |v: f64| (v / TAU_MODEL).round() as i64;
    let c = center.as_array();
    (
        q(c[0]),
        q(c[1]),
        q(c[2]),
        q(n[0]),
        q(n[1]),
        q(n[2]),
        q(radius),
    )
}

// Distance from a point to the cone axis line (perpendicular component of
// `point − apex` w.r.t. â).
fn dist_to_axis(point: [f64; 3], apex: [f64; 3], ahat: [f64; 3]) -> f64 {
    let rel = sub(point, apex);
    norm(sub(rel, scale(ahat, dot(rel, ahat))))
}

// Collect a sorted Vec of circle_keys from an Ok result (panics on non-Circle).
fn key_set(curves: &[SsiCurve]) -> Vec<(i64, i64, i64, i64, i64, i64, i64)> {
    let mut keys: Vec<_> = curves
        .iter()
        .map(|c| match c {
            SsiCurve::Circle {
                center,
                normal,
                radius,
            } => circle_key(*center, *normal, *radius),
            other => panic!("expected Circle, got {other:?}"),
        })
        .collect();
    keys.sort();
    keys
}

// ---------------------------------------------------------------------------
// X2 canonical (spec case 1) — cone₁ apex=origin, axis=+z, α₁=π/4 (m₁=1);
// cone₂ apex=(0,0,2) (δ=2), axis=+z, α₂=atan(3) (m₂=3). Roots t=3 and t=1.5 ⇒
// circles z=3 r=3 and z=1.5 r=1.5. Larger-t first ⇒ curves[0] is z=3 r=3.
// ---------------------------------------------------------------------------

#[test]
fn x2_canonical_two_circles() {
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4, // m₁ = 1
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0), // δ = 2
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 3.0_f64.atan(), // m₂ = 3
    };
    let curves = intersect(&cone1, &cone2).expect("coaxial cone/cone: two circles");
    let circles = expect_two_circles(&curves);

    // I1: both circles lie on BOTH cones (each cone's own radial residual).
    assert_on_both_surfaces(&curves[0], &cone1, &cone2);
    assert_on_both_surfaces(&curves[1], &cone1, &cone2);

    // I5 order: larger-t first ⇒ curves[0] at t=3 (z=3, r=3), curves[1] at t=1.5.
    approx_point(circles[0].0, [0.0, 0.0, 3.0]);
    approx(circles[0].2, 3.0);
    approx_point(circles[1].0, [0.0, 0.0, 1.5]);
    approx(circles[1].2, 1.5);

    // I2: normals ∥ +z (unit); centers on the z-axis.
    let m1 = 1.0_f64;
    let m2 = 3.0_f64;
    let delta = 2.0_f64;
    for (center, normal, radius) in circles.iter() {
        parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
        approx(norm(normal.as_array()), 1.0);
        approx(center.as_array()[0], 0.0);
        approx(center.as_array()[1], 0.0);
        // radius = |t|·m₁ = |t−δ|·m₂ (assert the two formulas agree). t = z (axis +z).
        let t = center.as_array()[2];
        approx(*radius, t.abs() * m1);
        approx(*radius, (t - delta).abs() * m2);
    }
}

// ---------------------------------------------------------------------------
// X1 (spec case 2) — equal α=π/4 both cones; cone₂ apex=(0,0,2) (δ=2). One
// circle at the bisector t=δ/2=1 ⇒ z=1, radius |1|·tan(π/4)=1.
// ---------------------------------------------------------------------------

#[test]
fn x1_equal_alpha_one_circle_at_bisector() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0), // δ = 2
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let curves = intersect(&cone1, &cone2).expect("coaxial equal-α: one circle");
    let (center, normal, radius) = expect_one_circle(&curves);

    // I1: on both cones.
    assert_on_both_surfaces(&curves[0], &cone1, &cone2);

    // I2: bisector t = δ/2 = 1 ⇒ z=1, radius = |t|·tanα = 1.
    approx_point(center, [0.0, 0.0, 1.0]);
    approx(radius, 1.0);
    parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
    approx(norm(normal.as_array()), 1.0);
}

// ---------------------------------------------------------------------------
// X2 non-unit axis — both cones axis_dir=(0,0,5), else canonical case 1.
// Defensive normalization ⇒ identical result (z=3 r=3, z=1.5 r=1.5).
// ---------------------------------------------------------------------------

#[test]
fn x2_nonunit_axis() {
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0), // non-unit, +z
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0), // non-unit, +z
        half_angle: 3.0_f64.atan(),
    };
    let curves = intersect(&cone1, &cone2).expect("coaxial non-unit axis: two circles");
    let circles = expect_two_circles(&curves);

    assert_on_both_surfaces(&curves[0], &cone1, &cone2);
    assert_on_both_surfaces(&curves[1], &cone1, &cone2);

    approx_point(circles[0].0, [0.0, 0.0, 3.0]); // larger-t first
    approx(circles[0].2, 3.0);
    approx_point(circles[1].0, [0.0, 0.0, 1.5]);
    approx(circles[1].2, 1.5);
    for (_center, normal, _radius) in circles.iter() {
        parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
        approx(norm(normal.as_array()), 1.0); // normalized despite |axis_dir|=5
    }
}

// ---------------------------------------------------------------------------
// X2 oblique off-origin — shared axis â=normalize((1,2,2)), cone₁ apex=(1,1,1),
// cone₂ apex = apex₁ + δ·â (on the line, coaxial), δ=2. α₁=π/4 (m₁=1),
// α₂=atan(3) (m₂=3) ⇒ same roots t=3, t=1.5 along the axis; centers on the
// axis, normal ∥ â, larger-t first.
// ---------------------------------------------------------------------------

#[test]
fn x2_oblique_off_origin() {
    let ahat = unit([1.0, 2.0, 2.0]);
    let apex1 = [1.0, 1.0, 1.0];
    let delta = 2.0_f64;
    let apex2 = add(apex1, scale(ahat, delta)); // on the cone₁ axis line ⇒ coaxial
    let m1 = 1.0_f64;
    let m2 = 3.0_f64;
    let cone1 = QuadricSurface::Cone {
        apex: Point3::from(apex1),
        axis_dir: Vector3::new(1.0, 2.0, 2.0), // non-unit, oblique
        half_angle: std::f64::consts::FRAC_PI_4, // m₁ = 1
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::from(apex2),
        axis_dir: Vector3::new(1.0, 2.0, 2.0), // non-unit, oblique, ∥ cone₁ axis
        half_angle: 3.0_f64.atan(),            // m₂ = 3
    };
    let curves = intersect(&cone1, &cone2).expect("coaxial oblique off-origin: two circles");
    let circles = expect_two_circles(&curves);

    assert_on_both_surfaces(&curves[0], &cone1, &cone2);
    assert_on_both_surfaces(&curves[1], &cone1, &cone2);

    // Roots t=3 (larger, first) and t=1.5 along â from apex₁.
    approx_point(circles[0].0, add(apex1, scale(ahat, 3.0)));
    approx_point(circles[1].0, add(apex1, scale(ahat, 1.5)));
    for (center, normal, radius) in circles.iter() {
        parallel_up_to_sign(normal.as_array(), ahat); // normal ∥ normalized axis
        approx(norm(normal.as_array()), 1.0);
        // center is ON the axis line: perpendicular distance to the axis ≈ 0.
        approx(dist_to_axis(center.as_array(), apex1, ahat), 0.0);
        // radius = |t|·m₁ = |t−δ|·m₂ (the two formulas agree).
        let t = dot(sub(center.as_array(), apex1), ahat);
        approx(*radius, t.abs() * m1);
        approx(*radius, (t - delta).abs() * m2);
    }
}

// ---------------------------------------------------------------------------
// X0 (spec case 4) — apex-coincident, UNEQUAL α: same apex, α₁=π/4, α₂=atan(3),
// δ=0. Both roots collapse to t=0 (radius-0 point-circle at the shared apex) ⇒
// Ok(vec![]).
// ---------------------------------------------------------------------------

#[test]
fn x0_apex_coincident_unequal_alpha_is_empty() {
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0), // δ = 0 (apexes coincide)
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 3.0_f64.atan(), // unequal α
    };
    assert_eq!(intersect(&cone1, &cone2), Ok(Vec::new()));
    // Symmetric order also empty.
    assert_eq!(intersect(&cone2, &cone1), Ok(Vec::new()));
}

// ---------------------------------------------------------------------------
// CO (spec case 3) — coincident: same apex, same α=π/4, δ=0 ⇒ identical double
// cone (overlap is a 2D surface, not a curve) ⇒ Err(DegenerateInput).
// ---------------------------------------------------------------------------

#[test]
fn co_coincident_double_cone_is_degenerate() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0), // δ = 0
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha, // equal α ⇒ identical surface
    };
    assert_eq!(intersect(&cone1, &cone2), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// NC (a) — non-coaxial: cone₂ apex OFF the cone₁ axis line → ASNA (staged).
// cone₁ apex=origin/+z, cone₂ apex=(1,0,0) ⇒ d_ax=1 ≥ TAU_MODEL. Axes still ∥.
// Both argument orders.
// ---------------------------------------------------------------------------

#[test]
fn nc_off_axis_apex_yields_not_available() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(1.0, 0.0, 0.0),      // off the z-axis ⇒ d_ax = 1
        axis_dir: Vector3::new(0.0, 0.0, 1.0), // still ∥ cone₁ axis
        half_angle: 3.0_f64.atan(),
    };
    assert_eq!(
        intersect(&cone1, &cone2),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
    // Symmetric order also ASNA.
    assert_eq!(
        intersect(&cone2, &cone1),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
}

// ---------------------------------------------------------------------------
// NC (b) — non-parallel axes → ASNA (staged). cone₁ axis=+z, cone₂ axis=+x
// (|â₂ × â₁| = 1). Both argument orders.
// ---------------------------------------------------------------------------

#[test]
fn nc_non_parallel_axis_yields_not_available() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0), // ⟂ cone₁ axis ⇒ |â₂ × â₁| = 1
        half_angle: 3.0_f64.atan(),
    };
    assert_eq!(
        intersect(&cone1, &cone2),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
    // Symmetric order also ASNA.
    assert_eq!(
        intersect(&cone2, &cone1),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
}

// ---------------------------------------------------------------------------
// E1 — degenerate inputs → Err(DegenerateInput) (failure modes, I3).
// Bad α low+high (each cone) = 4 tests; zero axis (each cone) = 2 tests.
// ---------------------------------------------------------------------------

#[test]
fn e1_cone1_half_angle_too_small_is_degenerate() {
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 1e-9, // ≤ TAU_MODEL ⇒ cone degenerates to a line
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    assert_eq!(intersect(&cone1, &cone2), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_cone1_half_angle_too_large_is_degenerate() {
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_2 - 1e-9, // ≥ π/2 − TAU ⇒ plane
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    assert_eq!(intersect(&cone1, &cone2), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_cone2_half_angle_too_small_is_degenerate() {
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 1e-9, // ≤ TAU_MODEL
    };
    assert_eq!(intersect(&cone1, &cone2), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_cone2_half_angle_too_large_is_degenerate() {
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_2 - 1e-9, // ≥ π/2 − TAU
    };
    assert_eq!(intersect(&cone1, &cone2), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_cone1_axis_dir_is_degenerate() {
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 0.0), // zero axis
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 3.0_f64.atan(),
    };
    assert_eq!(intersect(&cone1, &cone2), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_cone2_axis_dir_is_degenerate() {
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0),
        axis_dir: Vector3::new(0.0, 0.0, 0.0), // zero axis
        half_angle: 3.0_f64.atan(),
    };
    assert_eq!(intersect(&cone1, &cone2), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// I3 ANTI-HACK (P9/P10) — the unequal-α coaxial case (δ ≠ 0) is ALWAYS exactly
// two circles. The squared equation's discriminant `(2·m₁·m₂·δ)²` is a perfect
// square ⇒ both roots always real; there is NO √D sign gate, NO manufactured
// tangent/empty sub-branch. Sweep several genuinely-unequal (α₁,α₂) pairs
// (|α₁−α₂| > TAU_MODEL) × several δ≠0 values (|δ| > TAU_MODEL) and assert the
// result is `Ok` with `len() == 2` EVERY time. Mirrors SSI8's anti-hack.
// ---------------------------------------------------------------------------

#[test]
fn anti_hack_unequal_alpha_is_always_two_circles() {
    // Genuinely-unequal α pairs (each strictly inside (0, π/2), |α₁−α₂| ≫ TAU).
    let alpha_pairs = [
        (std::f64::consts::FRAC_PI_4, 3.0_f64.atan()), // π/4 vs atan(3)
        (0.2, 1.4),
        (std::f64::consts::FRAC_PI_6, std::f64::consts::FRAC_PI_3), // π/6 vs π/3
        (0.5, 0.9),
        (1.3, 0.3),
    ];
    // δ ≠ 0 values spanning small → large, positive AND negative.
    let deltas: [f64; 6] = [0.5, 1.0, 2.0, -1.5, 10.0, -100.0];

    for &(a1, a2) in alpha_pairs.iter() {
        assert!(
            (a1 - a2).abs() > TAU_MODEL,
            "test bug: α pair ({a1}, {a2}) is not genuinely unequal"
        );
        for &delta in deltas.iter() {
            assert!(delta.abs() > TAU_MODEL, "test bug: δ={delta} not > TAU");
            let cone1 = QuadricSurface::Cone {
                apex: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle: a1,
            };
            let cone2 = QuadricSurface::Cone {
                apex: Point3::new(0.0, 0.0, delta), // δ along +z
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle: a2,
            };
            let curves = intersect(&cone1, &cone2).unwrap_or_else(|e| {
                panic!("unequal-α coaxial (α₁={a1}, α₂={a2}, δ={delta}) must be Ok, got {e:?}")
            });
            assert_eq!(
                curves.len(),
                2,
                "unequal-α coaxial (α₁={a1}, α₂={a2}, δ={delta}) must yield two circles, got {curves:?}"
            );
            // Both genuinely Circles; each lies on BOTH cones.
            let _ = expect_two_circles(&curves);
            assert_on_both_surfaces(&curves[0], &cone1, &cone2);
            assert_on_both_surfaces(&curves[1], &cone1, &cone2);
        }
    }
}

// ---------------------------------------------------------------------------
// I4 — symmetry: intersect(c1, c2) == intersect(c2, c1) as a SET (order /
// normal-sign tolerant via circle_key) for the X2 canonical case AND an X1 case.
// ---------------------------------------------------------------------------

#[test]
fn symmetry_x2_circle_set() {
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 3.0_f64.atan(),
    };
    let ab = intersect(&cone1, &cone2).expect("ab two circles");
    let ba = intersect(&cone2, &cone1).expect("ba two circles");
    assert_eq!(
        key_set(&ab),
        key_set(&ba),
        "X2 circle SET must match across argument order"
    );
}

#[test]
fn symmetry_x1_circle_set() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha, // equal α ⇒ X1
    };
    let ab = intersect(&cone1, &cone2).expect("ab one circle");
    let ba = intersect(&cone2, &cone1).expect("ba one circle");
    assert_eq!(
        key_set(&ab),
        key_set(&ba),
        "X1 circle SET must match across argument order"
    );
}

// ---------------------------------------------------------------------------
// I5 — determinism: identical inputs → byte-identical output, larger-t first.
// ---------------------------------------------------------------------------

#[test]
fn determinism_x2_identical() {
    let cone1 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cone2 = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 2.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 3.0_f64.atan(),
    };
    let first = intersect(&cone1, &cone2);
    let second = intersect(&cone1, &cone2);
    // Byte-identical structurally (PartialEq over the exact fields).
    assert_eq!(first, second, "two-circle output must be deterministic");

    let cf = first.expect("two circles");
    // larger-t first: curves[0].center.z == 3 (t=3 > t=1.5).
    match cf[0] {
        SsiCurve::Circle { center, .. } => approx_point(center, [0.0, 0.0, 3.0]),
        other => panic!("expected Circle, got {other:?}"),
    }

    // Identical at a fixed eval parameter.
    let cs = second.expect("two circles");
    let t = 0.7;
    assert_eq!(cf[0].eval(t).as_array(), cs[0].eval(t).as_array());
    assert_eq!(cf[1].eval(t).as_array(), cs[1].eval(t).as_array());
}
