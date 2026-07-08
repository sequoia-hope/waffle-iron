//! PR-SSI11 RED suite — cylinder ∩ cylinder, EQUAL radius, coplanar
//! intersecting (non-parallel) axes → exactly two ellipses.
//!
//! Patrikalakis & Maekawa §5.8: two cylinders of equal radius `r` whose axes
//! are coplanar and intersect (non-parallel) intersect in exactly two
//! ellipses. With `β = acos(û₁·û₂) ∈ (0,π)`, intersection point `O`, frame
//! `b̂₊ = unit(û₁+û₂)`, `b̂₋ = unit(û₁−û₂)`, `ŵ = unit(û₁×û₂)`:
//!
//!   - Ellipse A (emitted FIRST): center=O, normal=b̂₋, major_axis=b̂₊,
//!     major_radius = r / sin(β/2), minor_radius = r.
//!   - Ellipse B: center=O, normal=b̂₊, major_axis=b̂₋,
//!     major_radius = r / cos(β/2), minor_radius = r.
//!
//! Everything else non-parallel stays `Err(AnalyticalSolutionNotAvailable)`
//! (unequal R, or skew axes). Parallel axes still behave per SSI10 (→ Lines).
//! Degenerate inputs → `Err(DegenerateInput)`.
//!
//! These tests exercise the public `intersect` API only. They FAIL now (RED):
//! production returns `Err(AnalyticalSolutionNotAvailable)` for every
//! non-parallel cyl∩cyl pair. A separate Implementer makes them pass.

use cad_primitives::{Point3, Vector3, TAU_MODEL};
use ssi_rs::{intersect, QuadricSurface, SsiCurve, SsiError};

// ---------------------------------------------------------------------------
// Inline vector helpers on `[f64; 3]` (cad-primitives Point3/Vector3 are
// storage-only; algebra is done on arrays, exactly as in ssi10.rs).
// ---------------------------------------------------------------------------

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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
// Implicit residual: evaluate each quadric's implicit equation at a point.
// (Mirror of ssi10's residual; the Cylinder arm is reused by the on-surface
// oracle below. The whole `QuadricSurface` match is carried for completeness.)
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

// ---------------------------------------------------------------------------
// Ellipse-specific helpers.
// ---------------------------------------------------------------------------

/// Decomposed ellipse: (center, normal, major_axis, major_radius, minor_radius).
type EllipseParts = (Point3, Vector3, Vector3, f64, f64);

/// Extract exactly two `Ellipse`s from a result curve list, panicking
/// otherwise (analogous to ssi10's `expect_two_lines`).
fn expect_two_ellipses(curves: &[SsiCurve]) -> Vec<EllipseParts> {
    let mut out = Vec::new();
    for c in curves {
        match c {
            SsiCurve::SurfacePair { .. } => unreachable!(
                "this suite's solvers never produce a surface-pair curve (M5 cyl×cyl only)"
            ),
            SsiCurve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => out.push((*center, *normal, *major_axis, *major_radius, *minor_radius)),
            other => panic!("expected Ellipse, got {other:?}"),
        }
    }
    assert_eq!(
        out.len(),
        2,
        "expected exactly two ellipses, got {}",
        out.len()
    );
    out
}

/// Decompose an `Ellipse` curve into its parameters.
fn ellipse_parts(c: &SsiCurve) -> EllipseParts {
    match c {
        SsiCurve::SurfacePair { .. } => unreachable!(
            "this suite's solvers never produce a surface-pair curve (M5 cyl×cyl only)"
        ),
        SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => (*center, *normal, *major_axis, *major_radius, *minor_radius),
        other => panic!("expected Ellipse, got {other:?}"),
    }
}

/// ON-SURFACE oracle (LOAD-BEARING). Sample the ellipse densely via
/// `SsiCurve::eval` at many `t ∈ [0, 2π)` and assert each sample lies on BOTH
/// cylinders, i.e. its radial distance to each axis equals `r` within
/// `TAU_MODEL`. Reuses the cylinder residual logic from `implicit_residual`.
fn assert_ellipse_on_both(ell: &SsiCurve, c1: &QuadricSurface, c2: &QuadricSurface) {
    const SAMPLES: usize = 96;
    for i in 0..SAMPLES {
        let t = std::f64::consts::TAU * (i as f64) / (SAMPLES as f64);
        let x = ell.eval(t).as_array();
        let r1 = implicit_residual(c1, x);
        let r2 = implicit_residual(c2, x);
        assert!(
            r1 < TAU_MODEL,
            "ellipse sample t={t} at {x:?} off cyl1 by {r1} (>= TAU_MODEL)"
        );
        assert!(
            r2 < TAU_MODEL,
            "ellipse sample t={t} at {x:?} off cyl2 by {r2} (>= TAU_MODEL)"
        );
    }
}

fn quantize(x: f64) -> i64 {
    (x / TAU_MODEL).round() as i64
}

/// Canonicalize a direction up to sign so opposite directions hash equal
/// (first non-near-zero component made positive; ssi10's `line_key` scheme).
fn canon_dir(v: [f64; 3]) -> [f64; 3] {
    let d = unit(v);
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
    scale(d, s)
}

/// SET-comparison key for an ellipse: center (quantized), plane normal up to
/// sign, major axis up to sign, and the two radii. Mirrors ssi10's `line_key`
/// quantization scheme so two ellipses can be compared as an unordered set
/// across argument order / axis flips.
type EllipseKey = (
    i64,
    i64,
    i64, // center
    i64,
    i64,
    i64, // normal (up to sign)
    i64,
    i64,
    i64, // major axis (up to sign)
    i64,
    i64, // major, minor radius
);

fn ellipse_key(parts: &EllipseParts) -> EllipseKey {
    let (center, normal, major, major_r, minor_r) = *parts;
    let c = center.as_array();
    let n = canon_dir(normal.as_array());
    let m = canon_dir(major.as_array());
    (
        quantize(c[0]),
        quantize(c[1]),
        quantize(c[2]),
        quantize(n[0]),
        quantize(n[1]),
        quantize(n[2]),
        quantize(m[0]),
        quantize(m[1]),
        quantize(m[2]),
        quantize(major_r),
        quantize(minor_r),
    )
}

fn key_set(parts: &[EllipseParts]) -> std::collections::BTreeSet<EllipseKey> {
    parts.iter().map(ellipse_key).collect()
}

/// Are two directions parallel up to sign (within tolerance)?
fn parallel(a: [f64; 3], b: [f64; 3]) -> bool {
    norm(cross(unit(a), unit(b))) < 1e-9
}

// ---------------------------------------------------------------------------
// Cylinder constructor helper.
// ---------------------------------------------------------------------------

fn cyl(ax: f64, ay: f64, az: f64, dx: f64, dy: f64, dz: f64, r: f64) -> QuadricSurface {
    QuadricSurface::Cylinder {
        axis_point: Point3::new(ax, ay, az),
        axis_dir: Vector3::new(dx, dy, dz),
        radius: r,
    }
}

// ---------------------------------------------------------------------------
// Canonical 90° (β = 90°, O = origin) — two ellipses, on-surface oracle.
// cyl1 axis +x r=2; cyl2 axis +y r=2. β/2 = 45°: major = 2/sin45° = 2√2 on
// both; minor = 2. A: normal ∥ (1,−1,0), major ∥ (1,1,0). B: swapped.
// ---------------------------------------------------------------------------

#[test]
fn ssi11_canonical_90deg_two_ellipses_on_surface() {
    let c1 = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0);
    let c2 = cyl(0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0);
    let curves = intersect(&c1, &c2).expect("expected two ellipses");
    let ells = expect_two_ellipses(&curves);

    // On-surface oracle on BOTH ellipses, BOTH cylinders.
    for c in &curves {
        assert_ellipse_on_both(c, &c1, &c2);
    }

    // Radii: minor = 2 on both; major = 2√2 on both.
    let expected_major = 2.0 * std::f64::consts::SQRT_2;
    for (_, _, _, major_r, minor_r) in &ells {
        assert!(
            (*minor_r - 2.0).abs() < TAU_MODEL,
            "minor_radius {minor_r} != 2"
        );
        assert!(
            (*major_r - expected_major).abs() < TAU_MODEL,
            "major_radius {major_r} != 2√2"
        );
    }

    let n_minus = [1.0, -1.0, 0.0];
    let n_plus = [1.0, 1.0, 0.0];

    // A: normal ∥ b̂₋=(1,-1,0), major ∥ b̂₊=(1,1,0).
    let a = ells
        .iter()
        .find(|(_, n, _, _, _)| parallel(n.as_array(), n_minus))
        .expect("ellipse with normal ∥ (1,-1,0)");
    // B: normal ∥ b̂₊=(1,1,0), major ∥ b̂₋=(1,-1,0).
    let b = ells
        .iter()
        .find(|(_, n, _, _, _)| parallel(n.as_array(), n_plus))
        .expect("ellipse with normal ∥ (1,1,0)");

    assert!(parallel(a.1.as_array(), n_minus), "A normal ∥ (1,-1,0)");
    assert!(parallel(a.2.as_array(), n_plus), "A major ∥ (1,1,0)");
    assert!(parallel(b.1.as_array(), n_plus), "B normal ∥ (1,1,0)");
    assert!(parallel(b.2.as_array(), n_minus), "B major ∥ (1,-1,0)");

    // Spot-check A's major endpoint (eval at t=0) and minor endpoint
    // (eval at t=π/2): each at distance 2 from each axis.
    let a_curve = curves
        .iter()
        .find(|c| {
            let (_, n, _, _, _) = ellipse_parts(c);
            parallel(n.as_array(), n_minus)
        })
        .unwrap();
    let major_end = a_curve.eval(0.0).as_array();
    let minor_end = a_curve.eval(std::f64::consts::FRAC_PI_2).as_array();
    for p in [major_end, minor_end] {
        assert!(implicit_residual(&c1, p) < TAU_MODEL);
        assert!(implicit_residual(&c2, p) < TAU_MODEL);
    }
}

// ---------------------------------------------------------------------------
// Non-perpendicular 60° (β = 60°): û₁=+x, û₂=(0.5, √3/2, 0), r=2.
// β/2 = 30°: major radii r/sin30° = 4 and r/cos30° = 4/√3.
// ---------------------------------------------------------------------------

#[test]
fn ssi11_non_perpendicular_60deg_radii_and_on_surface() {
    let h = (3.0_f64).sqrt() / 2.0;
    let c1 = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0);
    let c2 = cyl(0.0, 0.0, 0.0, 0.5, h, 0.0, 2.0);
    let curves = intersect(&c1, &c2).expect("expected two ellipses");
    let ells = expect_two_ellipses(&curves);

    for c in &curves {
        assert_ellipse_on_both(c, &c1, &c2);
    }

    let major_a = 4.0; // r / sin(β/2) = 2 / 0.5
    let major_b = 4.0 / (3.0_f64).sqrt(); // r / cos(β/2) = 2 / (√3/2)

    let found_a = ells
        .iter()
        .find(|(_, _, _, mr, _)| (*mr - major_a).abs() < TAU_MODEL)
        .expect("ellipse with major_radius 4 (= r/sin30°)");
    let found_b = ells
        .iter()
        .find(|(_, _, _, mr, _)| (*mr - major_b).abs() < TAU_MODEL)
        .expect("ellipse with major_radius 4/√3 (= r/cos30°)");

    assert!((found_a.4 - 2.0).abs() < TAU_MODEL, "A minor_radius != 2");
    assert!((found_b.4 - 2.0).abs() < TAU_MODEL, "B minor_radius != 2");
}

// ---------------------------------------------------------------------------
// Equal-R but SKEW (perpendicular dirs, z-offset so axes do not intersect)
// → SurfacePair (S3), both argument orders. Not coplanar ⇒ not ellipses;
// the degree-4 curve is the procedural surface-pair descriptor (M5).
// ---------------------------------------------------------------------------

#[test]
fn ssi11_equal_r_skew_axes_surface_pair() {
    let c1 = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0);
    let c2 = cyl(0.0, 0.0, 5.0, 0.0, 1.0, 0.0, 2.0);
    assert_eq!(
        intersect(&c1, &c2),
        Ok(vec![SsiCurve::SurfacePair { a: c1, b: c2 }])
    );
    assert_eq!(
        intersect(&c2, &c1),
        Ok(vec![SsiCurve::SurfacePair { a: c2, b: c1 }])
    );
}

// ---------------------------------------------------------------------------
// Unequal R intersecting axes → SurfacePair (S2), both argument orders.
// Unequal radius breaks the equal-R ellipse reduction ⇒ degree-4 curve.
// ---------------------------------------------------------------------------

#[test]
fn ssi11_unequal_r_intersecting_surface_pair() {
    let c1 = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0);
    let c2 = cyl(0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 3.0);
    assert_eq!(
        intersect(&c1, &c2),
        Ok(vec![SsiCurve::SurfacePair { a: c1, b: c2 }])
    );
    assert_eq!(
        intersect(&c2, &c1),
        Ok(vec![SsiCurve::SurfacePair { a: c2, b: c1 }])
    );
}

// ---------------------------------------------------------------------------
// Parallel still → two lines (SSI10 path intact). 3-4-5 secant config.
// ---------------------------------------------------------------------------

#[test]
fn ssi11_parallel_still_two_lines() {
    let c1 = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 5.0);
    let c2 = cyl(8.0, 0.0, 0.0, 0.0, 0.0, 1.0, 5.0);
    let curves = intersect(&c1, &c2).expect("expected two lines");
    assert_eq!(curves.len(), 2, "expected exactly two curves");
    for c in &curves {
        assert!(
            matches!(c, SsiCurve::Line { .. }),
            "expected Line, got {c:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// E1 — r = 0 (intersecting-axis config) → DegenerateInput.
// ---------------------------------------------------------------------------

#[test]
fn ssi11_e1_zero_radius_degenerate() {
    let c1 = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0);
    let c2 = cyl(0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0);
    assert_eq!(intersect(&c1, &c2), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// I4 — symmetry: equal-R 90° ellipse SET invariant under argument order.
// ---------------------------------------------------------------------------

#[test]
fn ssi11_symmetry_arg_order() {
    let c1 = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0);
    let c2 = cyl(0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0);
    let a = intersect(&c1, &c2).expect("two ellipses");
    let b = intersect(&c2, &c1).expect("two ellipses");
    assert_eq!(
        key_set(&expect_two_ellipses(&a)),
        key_set(&expect_two_ellipses(&b)),
        "ellipse SET must match across argument order"
    );
}

// ---------------------------------------------------------------------------
// I5 — determinism: identical input → byte-identical output; Ellipse A
// (normal ∥ b̂₋ = (1,-1,0)) is curves[0] for the canonical 90° case.
// ---------------------------------------------------------------------------

#[test]
fn ssi11_determinism_byte_identical_and_order() {
    let c1 = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0);
    let c2 = cyl(0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0);
    let a = intersect(&c1, &c2);
    let b = intersect(&c1, &c2);
    assert_eq!(a, b, "output must be deterministic");

    let curves = a.expect("two ellipses");
    let (_, normal0, major0, _, _) = ellipse_parts(&curves[0]);
    assert!(
        parallel(normal0.as_array(), [1.0, -1.0, 0.0]),
        "curves[0] should be ellipse A (normal ∥ (1,-1,0)), got normal {normal0:?}"
    );
    assert!(
        parallel(major0.as_array(), [1.0, 1.0, 0.0]),
        "curves[0] (ellipse A) major should be ∥ (1,1,0), got {major0:?}"
    );
}

// ---------------------------------------------------------------------------
// Contract — major_radius ≥ minor_radius on EVERY returned ellipse.
// ---------------------------------------------------------------------------

#[test]
fn ssi11_contract_major_ge_minor() {
    let cases = [
        (
            cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0),
            cyl(0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0),
        ),
        (
            cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0),
            cyl(0.0, 0.0, 0.0, 0.5, (3.0_f64).sqrt() / 2.0, 0.0, 2.0),
        ),
    ];
    for (c1, c2) in &cases {
        let curves = intersect(c1, c2).expect("two ellipses");
        for (_, _, _, major_r, minor_r) in expect_two_ellipses(&curves) {
            assert!(
                major_r + TAU_MODEL >= minor_r,
                "contract violated: major_radius {major_r} < minor_radius {minor_r}"
            );
        }
    }
}
