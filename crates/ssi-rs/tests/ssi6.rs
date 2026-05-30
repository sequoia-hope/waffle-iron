//! PR-SSI6 — RED tests for the sphere∩cylinder coaxial solver.
//!
//! First of the degree-4 quadric∩quadric pairs. The general sphere∩cylinder
//! intersection is a degree-4 space curve, but the **coaxial** configuration
//! (cylinder axis passes through the sphere center) reduces to **circles** —
//! exact, reusing `SsiCurve::Circle`. These tests target the new coaxial
//! behavior via the public `intersect(sphere, cylinder)` dispatcher
//! (`sphere_cylinder` is private). The non-coaxial case stays a loud
//! `Err(AnalyticalSolutionNotAvailable)` (staged; general degree-4 deferred).
//!
//! Spec: specs/ssi_pr_ssi6_sphere_cylinder_coaxial.md
//! Branches: X2 (coaxial, r_s > r_c → two circles), X1 (coaxial tangent,
//! |r_s−r_c|≤TAU → one circle), X0 (coaxial, r_c > r_s → []),
//! NC (non-coaxial → ASNA), E1 (degenerate → Err).
//! Invariants: I1 (on-surface), I2 (analytical geometry), I3 (branch coverage),
//! I4 (symmetry), I5 (determinism).
//!
//! These FAIL now (RED): production returns `Err(AnalyticalSolutionNotAvailable)`
//! for every sphere∩cylinder pair. A separate Implementer makes them pass.

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
//   sphere   residual: | |x − C| − r_s |
//   cylinder residual: | dist(x, axisLine) − r_c | where
//     dist(x, axisLine) = | (x − A) − ((x − A)·â) â |  (ssi2's cylinder residual)
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
        // All curves produced by sphere∩cylinder are Circles, sampled over [0, 2π).
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

// ---------------------------------------------------------------------------
// X2 — coaxial, r_s > r_c → two circles (I1, I2, determinism order).
// ---------------------------------------------------------------------------

#[test]
fn x2_coaxial_two_circles() {
    // Sphere C=origin, r_s=2. Cylinder axis = z-axis (origin, +z), r_c=1.
    // Coaxial (d_ax=0). h = √(r_s² − r_c²) = √3. Two circles radius 1, normal +z,
    // centers (0,0,±√3), +h first ⇒ curves[0].center.z = +√3.
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let curves = intersect(&sphere, &cyl).expect("coaxial sphere/cylinder: two circles");
    let circles = expect_two_circles(&curves);

    // I1: both circles lie on BOTH surfaces.
    assert_on_both_surfaces(&curves[0], &sphere, &cyl);
    assert_on_both_surfaces(&curves[1], &sphere, &cyl);

    let h = 3.0_f64.sqrt();

    // I2: each radius == r_c, normal ∥ +z (unit), centers on the axis at ±h.
    for (center, normal, radius) in circles.iter() {
        approx(*radius, 1.0);
        parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
        approx(norm(normal.as_array()), 1.0);
        // center on the axis line (x=y=0).
        approx(center.as_array()[0], 0.0);
        approx(center.as_array()[1], 0.0);
        // |center − C| == h
        approx(norm(center.as_array()), h);
    }

    // Deterministic order: +h first.
    approx_point(circles[0].0, [0.0, 0.0, h]);
    approx_point(circles[1].0, [0.0, 0.0, -h]);

    // Symmetric about C = origin.
    let mid = scale(add(circles[0].0.as_array(), circles[1].0.as_array()), 0.5);
    approx(norm(mid), 0.0);
}

// ---------------------------------------------------------------------------
// X1 — coaxial tangent (r_c == r_s) → one circle (great circle at C) (I1, I2).
// ---------------------------------------------------------------------------

#[test]
fn x1_coaxial_tangent_one_circle_at_center() {
    // Sphere C=origin, r_s=2. Coaxial cylinder r_c=2 (= r_s). h≈0 ⇒ one circle,
    // radius 2, center == C (origin), normal ∥ axis (great-circle tangent).
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let curves = intersect(&sphere, &cyl).expect("coaxial tangent: one circle");
    let (center, normal, radius) = expect_single_circle(&curves);

    assert_on_both_surfaces(&curves[0], &sphere, &cyl);

    approx(radius, 2.0);
    approx_point(center, [0.0, 0.0, 0.0]); // == C
    parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
    approx(norm(normal.as_array()), 1.0);
}

#[test]
fn x1_coaxial_tangent_off_origin_center() {
    // Sphere C=(1,2,3), r_s=3. Coaxial cylinder along +z through C, r_c=3.
    // Tangent great circle: one circle, radius 3, center == C, normal ∥ +z.
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(1.0, 2.0, 3.0),
        radius: 3.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(1.0, 2.0, -5.0), // on the line x=1,y=2 through C
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 3.0,
    };
    let curves = intersect(&sphere, &cyl).expect("coaxial tangent off-origin: one circle");
    let (center, normal, radius) = expect_single_circle(&curves);

    assert_on_both_surfaces(&curves[0], &sphere, &cyl);

    approx(radius, 3.0);
    approx_point(center, [1.0, 2.0, 3.0]); // == C
    parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
    approx(norm(normal.as_array()), 1.0);
}

// ---------------------------------------------------------------------------
// X0 — coaxial, r_c > r_s → Ok([]) (cylinder wider than sphere) (I2, I3).
// ---------------------------------------------------------------------------

#[test]
fn x0_coaxial_cylinder_wider_yields_empty() {
    // Sphere C=origin, r_s=1. Coaxial cylinder r_c=2 (> r_s). No contact.
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    assert_eq!(intersect(&sphere, &cyl), Ok(vec![]));
    // Symmetric order also empty.
    assert_eq!(intersect(&cyl, &sphere), Ok(vec![]));
}

// ---------------------------------------------------------------------------
// NC — non-coaxial (axis offset from sphere center) → ASNA (staged) (I3).
// ---------------------------------------------------------------------------

#[test]
fn nc_non_coaxial_yields_not_available() {
    // Sphere C=origin, r_s=2. Cylinder axis ∥ +z but offset: axis_point=(0.5,0,0)
    // ⇒ axis line x=0.5,y=0 does NOT pass through C. d_ax = 0.5 ≥ TAU_MODEL ⇒ NC.
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.5, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    assert_eq!(
        intersect(&sphere, &cyl),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
    // Symmetric order also ASNA.
    assert_eq!(
        intersect(&cyl, &sphere),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
}

// ---------------------------------------------------------------------------
// E1 — degenerate inputs → Err(DegenerateInput) (failure modes, I3).
// ---------------------------------------------------------------------------

#[test]
fn e1_nonpositive_sphere_radius_is_degenerate() {
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 0.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    assert_eq!(intersect(&sphere, &cyl), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_nonpositive_cylinder_radius_is_degenerate() {
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 0.0,
    };
    assert_eq!(intersect(&sphere, &cyl), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_axis_dir_is_degenerate() {
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    assert_eq!(intersect(&sphere, &cyl), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// Coaxial on an oblique, non-unit axis (defensive normalization + off-z-axis).
// ---------------------------------------------------------------------------

#[test]
fn x2_coaxial_nonunit_axis() {
    // Same geometry as x2 but axis_dir = (0,0,5) (non-unit, +z). Defensive
    // normalization must yield identical two-circle result.
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0),
        radius: 1.0,
    };
    let curves = intersect(&sphere, &cyl).expect("coaxial non-unit axis: two circles");
    let circles = expect_two_circles(&curves);

    assert_on_both_surfaces(&curves[0], &sphere, &cyl);
    assert_on_both_surfaces(&curves[1], &sphere, &cyl);

    let h = 3.0_f64.sqrt();
    for (_center, normal, radius) in circles.iter() {
        approx(*radius, 1.0);
        parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
        approx(norm(normal.as_array()), 1.0); // normalized despite |axis_dir|=5
    }
    approx_point(circles[0].0, [0.0, 0.0, h]); // +h first
    approx_point(circles[1].0, [0.0, 0.0, -h]);
}

#[test]
fn x2_coaxial_oblique_axis() {
    // Oblique unit axis â = (1,2,2)/3 through origin; sphere C=origin on the axis
    // line ⇒ coaxial. r_s=2, r_c=1 ⇒ h=√3. Two circles: radius 1, normal ∥ â,
    // centers C ± h·â on the axis line. Off the coordinate axes.
    let ahat = unit([1.0, 2.0, 2.0]);
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(1.0, 2.0, 2.0), // non-unit, oblique
        radius: 1.0,
    };
    let curves = intersect(&sphere, &cyl).expect("coaxial oblique axis: two circles");
    let circles = expect_two_circles(&curves);

    assert_on_both_surfaces(&curves[0], &sphere, &cyl);
    assert_on_both_surfaces(&curves[1], &sphere, &cyl);

    let h = 3.0_f64.sqrt();
    for (center, normal, radius) in circles.iter() {
        approx(*radius, 1.0);
        parallel_up_to_sign(normal.as_array(), ahat); // normal ∥ normalized axis
        approx(norm(normal.as_array()), 1.0);
        approx(norm(center.as_array()), h); // |center − C| == h
                                            // center is ON the axis line: cross(center − C, â) ≈ 0.
        parallel_up_to_sign(unit(center.as_array()), ahat);
    }
    // +h first ⇒ curves[0].center = C + h·â = h·â.
    approx_point(circles[0].0, scale(ahat, h));
    approx_point(circles[1].0, scale(ahat, -h));
}

// ---------------------------------------------------------------------------
// I4 — symmetry: intersect(sphere, cyl) == intersect(cyl, sphere) as a SET
// (order / normal-sign tolerant) for the X2 case.
// ---------------------------------------------------------------------------

#[test]
fn symmetry_x2_circle_set() {
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let ab = intersect(&sphere, &cyl).expect("ab two circles");
    let ba = intersect(&cyl, &sphere).expect("ba two circles");

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
// I5 — determinism: identical inputs → byte-identical output, +h-first order.
// ---------------------------------------------------------------------------

#[test]
fn determinism_x2_identical() {
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let first = intersect(&sphere, &cyl);
    let second = intersect(&sphere, &cyl);
    // Byte-identical structurally (PartialEq over the exact fields).
    assert_eq!(first, second, "two-circle output must be deterministic");

    let cf = first.expect("two circles");
    // +h first: curves[0].center.z == +√3.
    let h = 3.0_f64.sqrt();
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
