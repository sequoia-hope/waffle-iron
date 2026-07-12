//! PR-SSI7 — RED tests for the sphere∩cone coaxial solver.
//!
//! Second of the degree-4 quadric∩quadric pairs (after PR-SSI6
//! sphere∩cylinder). The general sphere∩cone intersection is a degree-4 space
//! curve, but the **coaxial** configuration (the sphere center lies on the
//! cone's axis line) reduces to **one or two circles** — exact, reusing
//! `SsiCurve::Circle`. These tests target the new coaxial behavior via the
//! public `intersect(sphere, cone)` dispatcher (`sphere_cone` is private). The
//! non-coaxial case stays a loud `Err(AnalyticalSolutionNotAvailable)` (staged;
//! general degree-4 deferred).
//!
//! Spec: specs/ssi_pr_ssi7_sphere_cone_coaxial.md
//!
//! The math (coaxial): cone apex `P`, unit axis `â`, half-angle `α`; sphere
//! center `C` on the axis line, radius `r_s`. With `h0 = (C − P)·â`, a cone
//! point at axial height `h` lies on the sphere iff
//! `sec²α·h² − 2·h0·h + (h0²−r_s²) = 0`, roots `h = (h0 ± √D)·cos²α`,
//! `D = sec²α·r_s² − h0²·tan²α`. Each root → a `Circle { center = P + h·â,
//! normal = â, radius = |h|·tanα }`. Gate on the linear gap `g = r_s − |h0|·sinα`
//! (`sign(D) = sign(g)`).
//!
//! Branches:
//!   X2 (coaxial, g > TAU → two circles, +√D first),
//!   X1 (coaxial tangent, |g| ≤ TAU → one circle at h_t = h0·cos²α),
//!   X0 (coaxial, g < −TAU → []),
//!   NC (non-coaxial → ASNA, staged),
//!   E1 (degenerate: r_s ≤ 0, bad α low/high, zero axis → Err).
//! Invariants:
//!   I1 (on-surface: sphere + cone radial residuals),
//!   I2 (analytical geometry: radius |h|tanα, centers P+h·â on axis, normal ∥ â,
//!       roots match the quadratic),
//!   I3 (branch coverage), I4 (symmetry as a set), I5 (determinism, +√D first).
//!
//! These FAIL now (RED): production returns `Err(AnalyticalSolutionNotAvailable)`
//! for every sphere∩cone pair. A separate Implementer makes them pass.

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
//   sphere residual: | |x − C| − r_s |
//   cone radial residual: | |(x − P) − ((x − P)·â)·â| − |h|·tanα |,
//     h = (x − P)·â  (the cone residual already used by the ssi6 helpers)
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
        // All curves produced by sphere∩cone are Circles, sampled over [0, 2π).
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

fn expect_single_circle(curves: &[SsiCurve]) -> (Point3, Vector3, f64) {
    assert_eq!(
        curves.len(),
        1,
        "expected exactly one curve, got {curves:?}"
    );
    match curves[0] {
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => (center, normal, radius),
        other => panic!("expected Circle, got {other:?}"),
    }
}

fn expect_two_circles(curves: &[SsiCurve]) -> [(Point3, Vector3, f64); 2] {
    assert_eq!(
        curves.len(),
        2,
        "expected exactly two curves, got {curves:?}"
    );
    let mut out = [(Point3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 0.0), 0.0); 2];
    for (i, c) in curves.iter().enumerate() {
        match c {
            SsiCurve::SurfacePair { .. } => unreachable!(
                "this suite's solvers never produce a surface-pair curve (M5 cyl×cyl only)"
            ),
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
// X2 canonical — apex=origin, axis=+z, α=π/4 (tanα=1, sec²α=2); sphere C=origin
// (h0=0), r_s=2 ⇒ D=8 ⇒ two circles at z=±√2, radius=√2, normal +z; +√D first.
// ---------------------------------------------------------------------------

#[test]
fn x2_canonical_two_circles() {
    let alpha = std::f64::consts::FRAC_PI_4; // tanα = 1, sec²α = 2
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let curves = intersect(&sphere, &cone).expect("coaxial sphere/cone: two circles");
    let circles = expect_two_circles(&curves);

    // I1: both circles lie on BOTH surfaces (sphere + cone radial residuals).
    assert_on_both_surfaces(&curves[0], &sphere, &cone);
    assert_on_both_surfaces(&curves[1], &sphere, &cone);

    // h0 = 0 ⇒ symmetric roots h = ±√D·cos²α = ±√8·0.5 = ±√2; radius = |h|·tanα = √2.
    let h = 2.0_f64.sqrt();

    // I2: each radius == √2, normal ∥ +z (unit), centers on the axis at z=±√2.
    for (center, normal, radius) in circles.iter() {
        approx(*radius, h);
        parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
        approx(norm(normal.as_array()), 1.0);
        approx(center.as_array()[0], 0.0);
        approx(center.as_array()[1], 0.0);
    }

    // Deterministic order: +√D first ⇒ curves[0].center.z = +√2.
    approx_point(circles[0].0, [0.0, 0.0, h]);
    approx_point(circles[1].0, [0.0, 0.0, -h]);

    // Symmetric about the apex/center (h0 = 0).
    let mid = scale(add(circles[0].0.as_array(), circles[1].0.as_array()), 0.5);
    approx(norm(mid), 0.0);
}

// ---------------------------------------------------------------------------
// X2 asymmetric — apex=origin, axis=+z, α=π/4; sphere center=(0,0,3) (h0=3),
// r_s=4 ⇒ D=23 ⇒ roots h=(3±√23)/2 (one circle per nappe), +√D (larger z) first.
// Each radius = |h|·tanα = |h| (tanα=1). Roots satisfy 2h² − 6h − 7 = 0.
// ---------------------------------------------------------------------------

#[test]
fn x2_asymmetric_roots() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 3.0),
        radius: 4.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let curves = intersect(&sphere, &cone).expect("coaxial asymmetric: two circles");
    let circles = expect_two_circles(&curves);

    assert_on_both_surfaces(&curves[0], &sphere, &cone);
    assert_on_both_surfaces(&curves[1], &sphere, &cone);

    let root_d = 23.0_f64.sqrt();
    let h_plus = (3.0 + root_d) / 2.0; // ≈ 3.8979
    let h_minus = (3.0 - root_d) / 2.0; // ≈ −0.8979

    // I2: roots satisfy the quadratic 2h² − 6h − 7 = 0 (i.e. sec²α·h²−2h0·h+(h0²−r_s²)).
    let quad = |h: f64| 2.0 * h * h - 6.0 * h - 7.0;
    approx(quad(h_plus), 0.0);
    approx(quad(h_minus), 0.0);

    // +√D first: curves[0] is the larger-z root, on the axis (x=y=0).
    approx_point(circles[0].0, [0.0, 0.0, h_plus]);
    approx_point(circles[1].0, [0.0, 0.0, h_minus]);

    // radius_i = |h_i|·tanα = |h_i|; normal ∥ +z (unit).
    approx(circles[0].2, h_plus.abs());
    approx(circles[1].2, h_minus.abs());
    for (_center, normal, _radius) in circles.iter() {
        parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
        approx(norm(normal.as_array()), 1.0);
    }
}

// ---------------------------------------------------------------------------
// X2 non-unit axis — axis_dir=(0,0,5), else canonical. Defensive normalization
// ⇒ identical result to the canonical X2 test.
// ---------------------------------------------------------------------------

#[test]
fn x2_nonunit_axis() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0), // non-unit, +z
        half_angle: alpha,
    };
    let curves = intersect(&sphere, &cone).expect("coaxial non-unit axis: two circles");
    let circles = expect_two_circles(&curves);

    assert_on_both_surfaces(&curves[0], &sphere, &cone);
    assert_on_both_surfaces(&curves[1], &sphere, &cone);

    let h = 2.0_f64.sqrt();
    for (_center, normal, radius) in circles.iter() {
        approx(*radius, h);
        parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
        approx(norm(normal.as_array()), 1.0); // normalized despite |axis_dir|=5
    }
    approx_point(circles[0].0, [0.0, 0.0, h]); // +√D first
    approx_point(circles[1].0, [0.0, 0.0, -h]);
}

// ---------------------------------------------------------------------------
// X2 oblique off-origin — â=(1,2,2)/3 (axis_dir non-unit (1,2,2)), apex off the
// origin, sphere center ON the axis line at h0=0 (C = apex). α=π/4, r_s=2 ⇒ D=8
// ⇒ h=±√2, radius=√2. Off the coordinate axes; centers = apex ± h·â on the line.
// ---------------------------------------------------------------------------

#[test]
fn x2_oblique_off_origin() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let ahat = unit([1.0, 2.0, 2.0]);
    let apex = [1.0, 1.0, 1.0];
    // Sphere center ON the axis line, coaxial, at h0 = 0 (center == apex).
    let sphere = QuadricSurface::Sphere {
        center: Point3::from(apex),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::new(1.0, 2.0, 2.0), // non-unit, oblique
        half_angle: alpha,
    };
    let curves = intersect(&sphere, &cone).expect("coaxial oblique off-origin: two circles");
    let circles = expect_two_circles(&curves);

    assert_on_both_surfaces(&curves[0], &sphere, &cone);
    assert_on_both_surfaces(&curves[1], &sphere, &cone);

    let h = 2.0_f64.sqrt(); // |h| (tanα = 1 ⇒ radius = |h|·tanα = √2)
    for (center, normal, radius) in circles.iter() {
        approx(*radius, h);
        parallel_up_to_sign(normal.as_array(), ahat); // normal ∥ normalized axis
        approx(norm(normal.as_array()), 1.0);
        // center is ON the axis line: perpendicular distance to the axis ≈ 0.
        approx(dist_to_axis(center.as_array(), apex, ahat), 0.0);
    }
    // +√D first ⇒ curves[0].center = apex + h·â, curves[1] = apex − h·â.
    approx_point(circles[0].0, add(apex, scale(ahat, h)));
    approx_point(circles[1].0, sub(apex, scale(ahat, h)));
}

// ---------------------------------------------------------------------------
// X1 tangent at center — apex=origin, axis=+z, α=π/4; C=(0,0,2) (h0=2),
// r_s=√2 (=|h0|·sinα) ⇒ g=0 ⇒ one circle at h_t=h0·cos²α=1: center (0,0,1),
// radius 1, normal +z.
// ---------------------------------------------------------------------------

#[test]
fn x1_tangent_at_center() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 2.0),
        radius: 2.0_f64.sqrt(), // = |h0|·sinα = 2·(√2/2) = √2
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let curves = intersect(&sphere, &cone).expect("coaxial tangent: one circle");
    let (center, normal, radius) = expect_single_circle(&curves);

    assert_on_both_surfaces(&curves[0], &sphere, &cone);

    // h_t = h0·cos²α = 2·0.5 = 1; radius = |h_t|·tanα = 1.
    approx(radius, 1.0);
    approx_point(center, [0.0, 0.0, 1.0]);
    parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
    approx(norm(normal.as_array()), 1.0);
}

// ---------------------------------------------------------------------------
// X1 tangent off-origin — apex=(1,2,3), axis=+z, α=π/4; sphere center on the
// axis at h0=4 (C=(1,2,7)), r_s=|h0|·sinα=4·(√2/2)=2√2 ⇒ g=0 ⇒ one circle at
// h_t=h0·cos²α=2: center=apex+2·â=(1,2,5), radius=|h_t|·tanα=2.
// ---------------------------------------------------------------------------

#[test]
fn x1_tangent_off_origin() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let apex = [1.0, 2.0, 3.0];
    let ahat = [0.0, 0.0, 1.0];
    let h0 = 4.0;
    let center_c = add(apex, scale(ahat, h0)); // (1,2,7)
    let sphere = QuadricSurface::Sphere {
        center: Point3::from(center_c),
        radius: h0 * alpha.sin(), // = |h0|·sinα = 2√2
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let curves = intersect(&sphere, &cone).expect("coaxial tangent off-origin: one circle");
    let (center, normal, radius) = expect_single_circle(&curves);

    assert_on_both_surfaces(&curves[0], &sphere, &cone);

    let h_t = h0 * alpha.cos() * alpha.cos(); // h0·cos²α = 2
    approx(radius, h_t.abs() * alpha.tan()); // = 2
    approx_point(center, add(apex, scale(ahat, h_t))); // (1,2,5)
    parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
    approx(norm(normal.as_array()), 1.0);
}

// ---------------------------------------------------------------------------
// X0 empty — apex=origin, axis=+z, α=π/4; C=(0,0,3) (h0=3), r_s=2 ⇒
// g = 2 − 3·(√2/2) < 0 ⇒ Ok([]). Both argument orders.
// ---------------------------------------------------------------------------

#[test]
fn x0_empty_sphere_too_small() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 3.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    assert_eq!(intersect(&sphere, &cone), Ok(vec![]));
    // Symmetric order also empty.
    assert_eq!(intersect(&cone, &sphere), Ok(vec![]));
}

// ---------------------------------------------------------------------------
// NC — non-coaxial (sphere center OFF the axis line) → procedural
// SurfacePair (cone first), the M5 degree-4 contract. Supersedes the staged
// ASNA (design review 2026-07-12 F10; contract change, test updated with it).
// apex=origin, axis=+z, center=(0.5,0,3) ⇒ d_ax=0.5 ≥ TAU_MODEL. Both orders.
// ---------------------------------------------------------------------------

#[test]
fn nc_non_coaxial_yields_surface_pair() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.5, 0.0, 3.0), // off the z-axis ⇒ d_ax = 0.5
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let expected = Ok(vec![SsiCurve::SurfacePair {
        a: cone,
        b: sphere,
    }]);
    assert_eq!(intersect(&sphere, &cone), expected);
    // Symmetric order: same canonical pair (I4).
    assert_eq!(intersect(&cone, &sphere), expected);
}

// ---------------------------------------------------------------------------
// E1 — degenerate inputs → Err(DegenerateInput) (failure modes, I3).
// ---------------------------------------------------------------------------

#[test]
fn e1_zero_sphere_radius_is_degenerate() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 0.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    assert_eq!(intersect(&sphere, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_negative_sphere_radius_is_degenerate() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: -1.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    assert_eq!(intersect(&sphere, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_half_angle_too_small_is_degenerate() {
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 1e-9, // ≤ TAU_MODEL ⇒ cone degenerates to a line
    };
    assert_eq!(intersect(&sphere, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_half_angle_too_large_is_degenerate() {
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_2 - 1e-9, // ≥ π/2 − TAU ⇒ plane
    };
    assert_eq!(intersect(&sphere, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_axis_dir_is_degenerate() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 0.0), // zero axis
        half_angle: alpha,
    };
    assert_eq!(intersect(&sphere, &cone), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// I4 — symmetry: intersect(sphere, cone) == intersect(cone, sphere) as a SET
// (order / normal-sign tolerant) for the X2 canonical case.
// ---------------------------------------------------------------------------

#[test]
fn symmetry_x2_circle_set() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let ab = intersect(&sphere, &cone).expect("ab two circles");
    let ba = intersect(&cone, &sphere).expect("ba two circles");

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
// I5 — determinism: identical inputs → byte-identical output, +√D-first order.
// ---------------------------------------------------------------------------

#[test]
fn determinism_x2_identical() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let first = intersect(&sphere, &cone);
    let second = intersect(&sphere, &cone);
    // Byte-identical structurally (PartialEq over the exact fields).
    assert_eq!(first, second, "two-circle output must be deterministic");

    let cf = first.expect("two circles");
    // +√D first: curves[0].center.z == +√2.
    let h = 2.0_f64.sqrt();
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
