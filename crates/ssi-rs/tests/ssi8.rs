//! PR-SSI8 — RED tests for the cylinder∩cone coaxial solver.
//!
//! Third of the degree-4 quadric∩quadric pairs (after PR-SSI6 sphere∩cylinder
//! and PR-SSI7 sphere∩cone). The general cylinder∩cone intersection is a
//! degree-4 space curve, but the **coaxial** configuration (the two axis *lines*
//! coincide) reduces to **exactly two circles** — exact, reusing
//! `SsiCurve::Circle`. These tests target the new coaxial behavior via the
//! public `intersect(cylinder, cone)` dispatcher (the solver is private). The
//! non-coaxial case stays a loud `Err(AnalyticalSolutionNotAvailable)` (staged;
//! general degree-4 deferred).
//!
//! Spec: specs/ssi_pr_ssi8_cylinder_cone_coaxial.md
//! Research basis: Patrikalakis & Maekawa, *Shape Interrogation for CAD/M*,
//! §5.8.3 (Case F8, implicit/implicit quadric pair).
//!
//! The math (coaxial): cone apex `P`, unit axis `â`, half-angle `α`; cylinder
//! axis_point `A` on the cone axis line, unit axis `ĉ ∥ â`, radius `r_c`. A cone
//! point at axial height `h` has radial distance `|h|·tanα`; it lies on the
//! cylinder iff `|h|·tanα = r_c`, i.e. `|h| = r_c·cotα = r_c/tanα`. The two roots
//! `h = ± r_c·cotα` give **exactly two circles**
//! `{ center = P + h·â, normal = â, radius = r_c }`.
//!
//! CRITICAL anti-hack point (P9/P10): unlike SSI6/SSI7 there is **NO
//! discriminant, NO √, NO tangent (one-circle) branch, NO empty branch** —
//! coaxial cyl∩cone is ALWAYS exactly two circles for valid input. There is no
//! X1/X0 case; instead an explicit anti-hack invariant (I3) sweeps several valid
//! configs and asserts `len == 2` every time.
//!
//! Branches:
//!   X2 (coaxial, always two circles, h>0 nappe first),
//!   NC (non-coaxial: off-axis axis_point OR non-parallel axis → ASNA, staged),
//!   E1 (degenerate: r_c ≤ 0, bad α low/high, zero cone or cylinder axis → Err).
//! Invariants:
//!   I1 (on-surface: cylinder + cone radial residuals),
//!   I2 (analytical geometry: radius r_c, centers P±r_c·cotα·â on axis,
//!       normal ∥ â, h equal-and-opposite),
//!   I3 (branch coverage + ANTI-HACK: no coaxial config yields one/zero circles),
//!   I4 (symmetry as a set), I5 (determinism, h>0 first).
//!
//! These FAIL now (RED): production returns `Err(AnalyticalSolutionNotAvailable)`
//! for every cylinder∩cone pair. A separate Implementer makes them pass.

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
// asserts every sample satisfies BOTH input surfaces within TAU_MODEL.
//   cylinder radial residual: | |x − ((x−A)·ĉ)·ĉ − A|... | i.e.
//     | dist(x, cyl axis line) − r_c |
//   cone radial residual: | |(x − P) − ((x − P)·â)·â| − |h|·tanα |,
//     h = (x − P)·â  (the cone residual already used by the ssi6/ssi7 helpers)
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
        // All curves produced by cylinder∩cone are Circles, sampled over [0, 2π).
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

// ---------------------------------------------------------------------------
// X2 canonical (spec case 1) — cone apex=origin, axis=+z, α=π/4 (tanα=1,
// cotα=1); cylinder axis_point=origin, axis_dir=+z, r_c=2 ⇒ two circles at
// z=±2, radius 2, normal +z; h>0 nappe first.
// ---------------------------------------------------------------------------

#[test]
fn x2_canonical_two_circles() {
    let alpha = std::f64::consts::FRAC_PI_4; // tanα = 1, cotα = 1
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let curves = intersect(&cylinder, &cone).expect("coaxial cylinder/cone: two circles");
    let circles = expect_two_circles(&curves);

    // I1: both circles lie on BOTH surfaces (cylinder + cone radial residuals).
    assert_on_both_surfaces(&curves[0], &cylinder, &cone);
    assert_on_both_surfaces(&curves[1], &cylinder, &cone);

    // h = ± r_c·cotα = ±2; radius = r_c = 2.
    let h = 2.0;
    let r_c = 2.0;

    // I2: each radius == r_c, normal ∥ +z (unit), centers on the axis at z=±2.
    for (center, normal, radius) in circles.iter() {
        approx(*radius, r_c);
        parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
        approx(norm(normal.as_array()), 1.0);
        approx(center.as_array()[0], 0.0);
        approx(center.as_array()[1], 0.0);
    }

    // Deterministic order: h>0 nappe first ⇒ curves[0].center.z = +2.
    approx_point(circles[0].0, [0.0, 0.0, h]);
    approx_point(circles[1].0, [0.0, 0.0, -h]);

    // The two h are equal-and-opposite (symmetric about the apex).
    let mid = scale(add(circles[0].0.as_array(), circles[1].0.as_array()), 0.5);
    approx(norm(mid), 0.0);
}

// ---------------------------------------------------------------------------
// X2 cotα≠1 (spec case 2) — apex=origin, axis=+z, α=atan(2) (tanα=2, cotα=0.5);
// cylinder axis_point=origin, axis_dir=+z, r_c=3 ⇒ circles at z=±1.5, radius 3.
// ---------------------------------------------------------------------------

#[test]
fn x2_cot_alpha_not_one() {
    let alpha = 2.0_f64.atan(); // tanα = 2, cotα = 0.5
    let r_c = 3.0;
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: r_c,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let curves = intersect(&cylinder, &cone).expect("coaxial cotα≠1: two circles");
    let circles = expect_two_circles(&curves);

    assert_on_both_surfaces(&curves[0], &cylinder, &cone);
    assert_on_both_surfaces(&curves[1], &cylinder, &cone);

    // h = ± r_c·cotα = ±(3·0.5) = ±1.5; radius = r_c = 3.
    let h = r_c / alpha.tan(); // = 1.5
    for (center, normal, radius) in circles.iter() {
        approx(*radius, r_c);
        parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
        approx(norm(normal.as_array()), 1.0);
        approx(center.as_array()[0], 0.0);
        approx(center.as_array()[1], 0.0);
    }
    // h>0 first.
    approx_point(circles[0].0, [0.0, 0.0, h]);
    approx_point(circles[1].0, [0.0, 0.0, -h]);
}

// ---------------------------------------------------------------------------
// X2 non-unit axis — cyl/cone axis_dir=(0,0,5), else canonical. Defensive
// normalization ⇒ identical result to the canonical X2 test (radius 2, z=±2).
// ---------------------------------------------------------------------------

#[test]
fn x2_nonunit_axis() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0), // non-unit, +z
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0), // non-unit, +z
        half_angle: alpha,
    };
    let curves = intersect(&cylinder, &cone).expect("coaxial non-unit axis: two circles");
    let circles = expect_two_circles(&curves);

    assert_on_both_surfaces(&curves[0], &cylinder, &cone);
    assert_on_both_surfaces(&curves[1], &cylinder, &cone);

    let h = 2.0;
    let r_c = 2.0;
    for (_center, normal, radius) in circles.iter() {
        approx(*radius, r_c);
        parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
        approx(norm(normal.as_array()), 1.0); // normalized despite |axis_dir|=5
    }
    approx_point(circles[0].0, [0.0, 0.0, h]); // h>0 first
    approx_point(circles[1].0, [0.0, 0.0, -h]);
}

// ---------------------------------------------------------------------------
// X2 oblique off-origin — shared axis â=normalize((1,2,2)), apex=(1,1,1),
// cylinder axis_point=(1,1,1) (on the line, coaxial). α=π/4 (cotα=1), r_c=2 ⇒
// centers = apex ± 2·â, radius 2, normal ∥ â, centers on axis; h>0 first.
// ---------------------------------------------------------------------------

#[test]
fn x2_oblique_off_origin() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let ahat = unit([1.0, 2.0, 2.0]);
    let apex = [1.0, 1.0, 1.0];
    let r_c = 2.0;
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::from(apex), // on the cone axis line ⇒ coaxial
        axis_dir: Vector3::new(1.0, 2.0, 2.0), // non-unit, oblique, ∥ cone axis
        radius: r_c,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::new(1.0, 2.0, 2.0), // non-unit, oblique
        half_angle: alpha,
    };
    let curves = intersect(&cylinder, &cone).expect("coaxial oblique off-origin: two circles");
    let circles = expect_two_circles(&curves);

    assert_on_both_surfaces(&curves[0], &cylinder, &cone);
    assert_on_both_surfaces(&curves[1], &cylinder, &cone);

    let h = r_c / alpha.tan(); // r_c·cotα = 2 (tanα = 1)
    for (center, normal, radius) in circles.iter() {
        approx(*radius, r_c);
        parallel_up_to_sign(normal.as_array(), ahat); // normal ∥ normalized axis
        approx(norm(normal.as_array()), 1.0);
        // center is ON the axis line: perpendicular distance to the axis ≈ 0.
        approx(dist_to_axis(center.as_array(), apex, ahat), 0.0);
    }
    // h>0 first ⇒ curves[0].center = apex + h·â, curves[1] = apex − h·â.
    approx_point(circles[0].0, add(apex, scale(ahat, h)));
    approx_point(circles[1].0, sub(apex, scale(ahat, h)));
}

// ---------------------------------------------------------------------------
// NC (a) — non-coaxial: cylinder axis_point OFF the cone axis line → ASNA
// (staged). cone apex=origin/+z, cyl axis_point=(1,0,0) ⇒ d_ax=1 ≥ TAU_MODEL.
// Both argument orders.
// ---------------------------------------------------------------------------

#[test]
fn nc_off_axis_axis_point_yields_not_available() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(1.0, 0.0, 0.0), // off the z-axis ⇒ d_ax = 1
        axis_dir: Vector3::new(0.0, 0.0, 1.0),  // still ∥ cone axis
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    assert_eq!(
        intersect(&cylinder, &cone),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
    // Symmetric order also ASNA.
    assert_eq!(
        intersect(&cone, &cylinder),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
}

// ---------------------------------------------------------------------------
// NC (b) — non-parallel cylinder axis → ASNA (staged). cone axis=+z,
// cyl axis_dir=(1,0,0) (axes not parallel, |ĉ × â| = 1). axis_point=origin.
// Both argument orders.
// ---------------------------------------------------------------------------

#[test]
fn nc_non_parallel_axis_yields_not_available() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0), // ⟂ cone axis ⇒ |ĉ × â| = 1
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    assert_eq!(
        intersect(&cylinder, &cone),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
    // Symmetric order also ASNA.
    assert_eq!(
        intersect(&cone, &cylinder),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
}

// ---------------------------------------------------------------------------
// E1 — degenerate inputs → Err(DegenerateInput) (failure modes, I3).
// ---------------------------------------------------------------------------

#[test]
fn e1_zero_cylinder_radius_is_degenerate() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 0.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    assert_eq!(intersect(&cylinder, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_negative_cylinder_radius_is_degenerate() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: -1.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    assert_eq!(intersect(&cylinder, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_half_angle_too_small_is_degenerate() {
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 1e-9, // ≤ TAU_MODEL ⇒ cone degenerates to a line
    };
    assert_eq!(intersect(&cylinder, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_half_angle_too_large_is_degenerate() {
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_2 - 1e-9, // ≥ π/2 − TAU ⇒ plane
    };
    assert_eq!(intersect(&cylinder, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_cone_axis_dir_is_degenerate() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 0.0), // zero cone axis
        half_angle: alpha,
    };
    assert_eq!(intersect(&cylinder, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_cylinder_axis_dir_is_degenerate() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 0.0), // zero cylinder axis
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    assert_eq!(intersect(&cylinder, &cone), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// I3 ANTI-HACK (P9/P10) — coaxial cyl∩cone is ALWAYS exactly two circles for
// valid input. Sweep several distinct valid coaxial configs (vary α and r_c)
// and assert the result is `Ok` with `len() == 2` EVERY time — i.e. there is no
// coaxial config yielding one or zero circles (no manufactured discriminant /
// tangent / empty branch). This is the spec's explicit anti-hack requirement.
// ---------------------------------------------------------------------------

#[test]
fn anti_hack_coaxial_is_always_two_circles() {
    // α values strictly inside (0, π/2), spanning small/medium/large half-angles.
    let alphas = [
        0.2,
        std::f64::consts::FRAC_PI_6, // π/6
        std::f64::consts::FRAC_PI_4, // π/4
        0.9,
        1.4,
    ];
    // Positive cylinder radii spanning small → large.
    let radii = [0.1, 1.0, 2.0, 5.0, 100.0];

    for &alpha in alphas.iter() {
        for &r_c in radii.iter() {
            let cylinder = QuadricSurface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r_c,
            };
            let cone = QuadricSurface::Cone {
                apex: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle: alpha,
            };
            let curves = intersect(&cylinder, &cone)
                .unwrap_or_else(|e| panic!("coaxial (α={alpha}, r_c={r_c}) must be Ok, got {e:?}"));
            assert_eq!(
                curves.len(),
                2,
                "coaxial (α={alpha}, r_c={r_c}) must yield exactly two circles, got {curves:?}"
            );
            // And both are genuinely Circles (no other variant slips through).
            let circles = expect_two_circles(&curves);
            // Sanity: each radius is r_c, centers equal-and-opposite about apex.
            for (_center, _normal, radius) in circles.iter() {
                approx(*radius, r_c);
            }
            let mid = scale(add(circles[0].0.as_array(), circles[1].0.as_array()), 0.5);
            approx(norm(mid), 0.0);
        }
    }
}

// ---------------------------------------------------------------------------
// I4 — symmetry: intersect(cyl, cone) == intersect(cone, cyl) as a SET
// (order / normal-sign tolerant via circle_key) for the X2 canonical case.
// ---------------------------------------------------------------------------

#[test]
fn symmetry_x2_circle_set() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let ab = intersect(&cylinder, &cone).expect("ab two circles");
    let ba = intersect(&cone, &cylinder).expect("ba two circles");

    let mut ab_keys: Vec<_> = ab
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
    let mut ba_keys: Vec<_> = ba
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
    ab_keys.sort();
    ba_keys.sort();
    assert_eq!(
        ab_keys, ba_keys,
        "circle SET must match across argument order"
    );
}

// ---------------------------------------------------------------------------
// I5 — determinism: identical inputs → byte-identical output, h>0-first order.
// ---------------------------------------------------------------------------

#[test]
fn determinism_x2_identical() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cylinder = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let first = intersect(&cylinder, &cone);
    let second = intersect(&cylinder, &cone);
    // Byte-identical structurally (PartialEq over the exact fields).
    assert_eq!(first, second, "two-circle output must be deterministic");

    let cf = first.expect("two circles");
    // h>0 first: curves[0].center.z == +2.
    let h = 2.0;
    match cf[0] {
        SsiCurve::Circle { center, .. } => approx_point(center, [0.0, 0.0, h]),
        other => panic!("expected Circle, got {other:?}"),
    }

    // Identical at a fixed eval parameter.
    let cs = second.expect("two circles");
    let t = 0.7;
    assert_eq!(cf[0].eval(t).as_array(), cs[0].eval(t).as_array());
    assert_eq!(cf[1].eval(t).as_array(), cs[1].eval(t).as_array());
}
