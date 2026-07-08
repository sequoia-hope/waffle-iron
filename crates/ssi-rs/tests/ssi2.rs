//! PR-SSI2 — RED tests for the plane∩cylinder solver.
//!
//! These tests target the not-yet-existing API:
//! `QuadricSurface::Cylinder`, `SsiCurve::Ellipse`, and the `plane_cylinder`
//! solver reached through the public `intersect` dispatcher. As in `ssi1.rs`,
//! everything is written against `intersect` (the public dispatcher) so the
//! tests are robust to the private solver function's naming.
//!
//! Spec: specs/ssi_pr_ssi2_plane_cylinder.md
//! Branches: C1 (perpendicular→Circle), C2 (oblique→Ellipse),
//! C3a (parallel secant→two Lines), C3b (parallel tangent→one Line),
//! C3c (parallel disjoint→[]), E1 (degenerate→Err).
//! Invariants: I1 (on-surface), I2 (analytical geometry), I3 (branch coverage),
//! I4 (symmetry), I5 (determinism). Plus the newly-triggerable
//! `AnalyticalSolutionNotAvailable` path (sphere∩cylinder).

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
// On-surface oracle (I1). Samples the curve at N parameter values and asserts
// each sample satisfies BOTH input surfaces' implicit equations within
// TAU_MODEL. For a (plane, cylinder) pair, the cylinder residual is the
// distance-to-axis-line minus the radius:
//   dist(x, axisLine) = | (x − q) − ((x − q)·â) â |.
// Curves sampled: Ellipse over [0, 2π); Circle over [0, 2π); Line over [-5, 5].
// ---------------------------------------------------------------------------

fn implicit_residual(surf: &QuadricSurface, x: [f64; 3]) -> f64 {
    match surf {
        QuadricSurface::Plane { point, normal } => {
            // |n̂·(x − point)|, normal assumed unit.
            dot(unit(normal.as_array()), sub(x, point.as_array())).abs()
        }
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
        QuadricSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            // Cone RADIAL residual: | r_actual − |h|·tanα |, where
            //   h = (x − apex)·â, r_actual = |(x − apex) − h·â|.
            // axis_dir normalized defensively (matching the cylinder arm).
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
        let t = match curve {
            SsiCurve::SurfacePair { .. } => unreachable!(
                "this suite's solvers never produce a surface-pair curve (M5 cyl×cyl only)"
            ),
            SsiCurve::Circle { .. } | SsiCurve::Ellipse { .. } => {
                // [0, 2π)
                (i as f64) / (N as f64) * std::f64::consts::TAU
            }
            SsiCurve::Line { .. } => {
                // [-5, 5]
                -5.0 + (i as f64) / ((N - 1) as f64) * 10.0
            }
            // Not produced by PR-SSI2 solvers; compile-keepalive for the
            // extended enum (PR-SSI4 added `Parabola`/`Hyperbola`). Bounded
            // range [−3, 3].
            SsiCurve::Parabola { .. } | SsiCurve::Hyperbola { .. } => {
                (i as f64) / ((N - 1) as f64) * 6.0 - 3.0
            }
        };
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
// Extractors. Each asserts the result has exactly the expected shape.
// ---------------------------------------------------------------------------

fn expect_single_line(curves: &[SsiCurve]) -> (Point3, Vector3) {
    assert_eq!(
        curves.len(),
        1,
        "expected exactly one curve, got {curves:?}"
    );
    match curves[0] {
        SsiCurve::Line { point, dir } => (point, dir),
        other => panic!("expected Line, got {other:?}"),
    }
}

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

#[allow(clippy::type_complexity)]
fn expect_single_ellipse(curves: &[SsiCurve]) -> (Point3, Vector3, Vector3, f64, f64) {
    assert_eq!(
        curves.len(),
        1,
        "expected exactly one curve, got {curves:?}"
    );
    match curves[0] {
        SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => (center, normal, major_axis, major_radius, minor_radius),
        other => panic!("expected Ellipse, got {other:?}"),
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

// Distance from point x to the line through q with unit direction d.
fn dist_to_line(x: [f64; 3], q: [f64; 3], d: [f64; 3]) -> f64 {
    let dh = unit(d);
    let rel = sub(x, q);
    let along = scale(dh, dot(rel, dh));
    norm(sub(rel, along))
}

// ---------------------------------------------------------------------------
// C1 — perpendicular plane ∩ cylinder → one Circle (I2, I1).
// ---------------------------------------------------------------------------

#[test]
fn c1_perpendicular_yields_circle() {
    // Cylinder axis +z through origin, r = 2. Plane z = 3 (normal +z) ⟂ axis.
    // |c| = |n̂·â| = 1 ⇒ C1. Circle: center (0,0,3), normal +z, radius 2.
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let curves = intersect(&plane, &cyl).expect("perpendicular plane/cylinder: one circle");
    let (center, normal, radius) = expect_single_circle(&curves);

    // I1: on-surface oracle.
    assert_on_both_surfaces(&curves[0], &plane, &cyl);

    // I2: analytical geometry.
    approx(radius, 2.0);
    approx_point(center, [0.0, 0.0, 3.0]); // on axis AND in plane
    parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]); // normal ∥ axis
    approx(norm(normal.as_array()), 1.0);
}

// ---------------------------------------------------------------------------
// C2 — oblique plane ∩ cylinder → one Ellipse (I2, I1).
// ---------------------------------------------------------------------------

#[test]
fn c2_oblique_45deg_yields_ellipse() {
    // Cylinder axis +z through origin, r = 1.
    // Plane through origin with unit normal n̂ = (1,0,1)/√2 (45° to axis +z).
    //   c = n̂·â = 1/√2 ⇒ TAU_MODEL ≤ |c| ≤ 1−TAU_MODEL ⇒ C2.
    //   minor_radius b = r = 1.
    //   major_radius a = r/|c| = √2.
    //   center = axis ∩ plane: with q=origin on the plane (p=origin), s=0 ⇒ (0,0,0).
    //   major_axis = normalize(â − c·n̂) = normalize((0,0,1) − (1/√2)(1,0,1)/√2)
    //              = normalize((−1/2, 0, 1/2)) = (−1,0,1)/√2.
    let inv_sqrt2 = 1.0 / 2.0_f64.sqrt();
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(inv_sqrt2, 0.0, inv_sqrt2),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let curves = intersect(&plane, &cyl).expect("oblique plane/cylinder: one ellipse");
    let (center, normal, major_axis, major_radius, minor_radius) = expect_single_ellipse(&curves);

    // I1: on-surface oracle — every ellipse sample lies on BOTH surfaces.
    assert_on_both_surfaces(&curves[0], &plane, &cyl);

    // I2: analytical geometry.
    approx(minor_radius, 1.0); // b = r
    approx(major_radius, 2.0_f64.sqrt()); // a = r/|c| = √2
    approx_point(center, [0.0, 0.0, 0.0]); // axis ∩ plane

    // normal is the (unit) plane normal.
    parallel_up_to_sign(normal.as_array(), [inv_sqrt2, 0.0, inv_sqrt2]);
    approx(norm(normal.as_array()), 1.0);

    // major_axis is unit and lies in the plane (⟂ normal).
    approx(norm(major_axis.as_array()), 1.0);
    approx(dot(major_axis.as_array(), normal.as_array()), 0.0);
    parallel_up_to_sign(major_axis.as_array(), unit([-1.0, 0.0, 1.0]));

    // minor_axis = normal × major_axis ⟂ both major_axis and normal.
    let minor = cross(normal.as_array(), major_axis.as_array());
    approx(dot(minor, major_axis.as_array()), 0.0);
    approx(dot(minor, normal.as_array()), 0.0);

    // a ≥ b (semi-major ≥ semi-minor).
    assert!(
        major_radius >= minor_radius - TAU_MODEL,
        "major {major_radius} must be ≥ minor {minor_radius}"
    );
}

// ---------------------------------------------------------------------------
// C3a — plane parallel to axis, secant → two Lines (I2, I1).
// ---------------------------------------------------------------------------

#[test]
fn c3a_parallel_secant_yields_two_lines() {
    // Cylinder axis +z through origin, r = 2. Plane x = 1 (normal +x) ∥ axis,
    // distance d = 1 < r ⇒ C3a secant.
    //   ŵ = normalize(n̂ × â) = normalize((1,0,0)×(0,0,1)) = normalize((0,−1,0)) = (0,−1,0).
    //   c0 = foot of axis on plane = (1,0,0) (q=origin, signed d along +x = 1).
    //   off = √(r² − d²) = √3.
    //   Lines: c0 + off·ŵ = (1,−√3,0) then c0 − off·ŵ = (1,√3,0), each dir = â = +z.
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let curves = intersect(&plane, &cyl).expect("parallel secant plane/cylinder: two lines");
    let lines = expect_two_lines(&curves);

    // I1: on-surface oracle for BOTH lines.
    assert_on_both_surfaces(&curves[0], &plane, &cyl);
    assert_on_both_surfaces(&curves[1], &plane, &cyl);

    let off = 3.0_f64.sqrt();
    let c0 = [1.0, 0.0, 0.0];

    // Both dirs parallel to the axis and unit.
    for (_pt, dir) in lines.iter() {
        parallel_up_to_sign(dir.as_array(), [0.0, 0.0, 1.0]);
        approx(norm(dir.as_array()), 1.0);
    }

    // Each line is at distance exactly r from the axis.
    for (pt, dir) in lines.iter() {
        approx(
            dist_to_line(pt.as_array(), [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            2.0,
        );
        // its own point sits on the plane x=1.
        approx(pt.as_array()[0], 1.0);
        let _ = dir;
    }

    // Deterministic order: +ŵ first ⇒ first line at c0 + off·(0,−1,0) = (1,−√3,0).
    let (p0, _) = lines[0];
    let (p1, _) = lines[1];
    approx_point(p0, add(c0, scale([0.0, -1.0, 0.0], off)));
    approx_point(p1, sub(c0, scale([0.0, -1.0, 0.0], off)));

    // Symmetric about the foot c0.
    let mid = scale(add(p0.as_array(), p1.as_array()), 0.5);
    approx(norm(sub(mid, c0)), 0.0);
}

// ---------------------------------------------------------------------------
// C3b — plane parallel to axis, tangent → one Line (I2, I1).
// ---------------------------------------------------------------------------

#[test]
fn c3b_parallel_tangent_yields_one_line() {
    // Cylinder axis +z through origin, r = 2. Plane x = 2 (normal +x) ∥ axis,
    // distance d = 2 == r ⇒ C3b tangent. Single line at the foot c0 = (2,0,0).
    let plane = QuadricSurface::Plane {
        point: Point3::new(2.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let curves = intersect(&plane, &cyl).expect("parallel tangent plane/cylinder: one line");
    let (point, dir) = expect_single_line(&curves);

    // I1: on-surface oracle.
    assert_on_both_surfaces(&curves[0], &plane, &cyl);

    // dir parallel to axis, unit.
    parallel_up_to_sign(dir.as_array(), [0.0, 0.0, 1.0]);
    approx(norm(dir.as_array()), 1.0);

    // Tangent line at distance exactly r from the axis, at the foot.
    approx(
        dist_to_line(point.as_array(), [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        2.0,
    );
    approx(point.as_array()[0], 2.0);
}

// ---------------------------------------------------------------------------
// C3c — plane parallel to axis, disjoint → Ok([]) (I3).
// ---------------------------------------------------------------------------

#[test]
fn c3c_parallel_disjoint_yields_empty() {
    // Cylinder axis +z, r = 2. Plane x = 3 (normal +x) ∥ axis, d = 3 > r ⇒ C3c.
    let plane = QuadricSurface::Plane {
        point: Point3::new(3.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let curves = intersect(&plane, &cyl).expect("parallel disjoint plane/cylinder: Ok([])");
    assert!(curves.is_empty(), "expected no curves, got {curves:?}");
}

// ---------------------------------------------------------------------------
// E1 — degenerate inputs → Err(DegenerateInput) (failure modes).
// ---------------------------------------------------------------------------

#[test]
fn e1_nonpositive_radius_is_degenerate() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 0.0,
    };
    assert_eq!(intersect(&plane, &cyl), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_axis_dir_is_degenerate() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    assert_eq!(intersect(&plane, &cyl), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_plane_normal_is_degenerate() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 0.0),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    assert_eq!(intersect(&plane, &cyl), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// I4 — symmetry: intersect(plane, cyl) == intersect(cyl, plane), same geometry
// up to sign of dir / major_axis.
// ---------------------------------------------------------------------------

#[test]
fn symmetry_c1_circle() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let ab = intersect(&plane, &cyl).unwrap();
    let ba = intersect(&cyl, &plane).unwrap();
    let (c_ab, n_ab, r_ab) = expect_single_circle(&ab);
    let (c_ba, n_ba, r_ba) = expect_single_circle(&ba);

    approx_point(c_ab, c_ba.as_array());
    approx(r_ab, r_ba);
    parallel_up_to_sign(n_ab.as_array(), n_ba.as_array()); // normal up to sign
}

#[test]
fn symmetry_c2_ellipse() {
    let inv_sqrt2 = 1.0 / 2.0_f64.sqrt();
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(inv_sqrt2, 0.0, inv_sqrt2),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let ab = intersect(&plane, &cyl).unwrap();
    let ba = intersect(&cyl, &plane).unwrap();
    let (c_ab, n_ab, m_ab, ar_ab, br_ab) = expect_single_ellipse(&ab);
    let (c_ba, n_ba, m_ba, ar_ba, br_ba) = expect_single_ellipse(&ba);

    approx_point(c_ab, c_ba.as_array());
    approx(ar_ab, ar_ba);
    approx(br_ab, br_ba);
    parallel_up_to_sign(n_ab.as_array(), n_ba.as_array()); // normal up to sign
    parallel_up_to_sign(m_ab.as_array(), m_ba.as_array()); // major_axis up to sign
}

// ---------------------------------------------------------------------------
// I5 — determinism: identical inputs → byte-identical outputs across calls.
// ---------------------------------------------------------------------------

#[test]
fn determinism_c2_ellipse_identical() {
    let inv_sqrt2 = 1.0 / 2.0_f64.sqrt();
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(inv_sqrt2, 0.0, inv_sqrt2),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let first = intersect(&plane, &cyl);
    let second = intersect(&plane, &cyl);
    // Byte-identical structurally (PartialEq over the exact fields).
    assert_eq!(first, second, "ellipse output must be deterministic");

    // And identical at a fixed eval parameter.
    let cf = first.unwrap();
    let cs = second.unwrap();
    let t = 0.7;
    assert_eq!(cf[0].eval(t).as_array(), cs[0].eval(t).as_array());
}

#[test]
fn determinism_c3a_two_lines_identical() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let first = intersect(&plane, &cyl);
    let second = intersect(&plane, &cyl);
    // Same ordering and same fields (the +ŵ-first rule is deterministic).
    assert_eq!(first, second, "two-line output must be deterministic");
}

// ---------------------------------------------------------------------------
// AnalyticalSolutionNotAvailable — the NON-COAXIAL sphere∩cylinder path (the
// general degree-4 curve) has no solver (A15.2: loud, never a silent fallback).
//
// NOTE (PR-SSI6): the COAXIAL sphere∩cylinder case now reduces to circles, so
// this guard must use a clearly NON-coaxial config (cylinder axis offset from
// the sphere center, d_ax ≥ TAU_MODEL) to still assert ASNA. The original
// PR-SSI2 geometry (cylinder z-axis through the sphere center) was coaxial and
// would now return circles.
// ---------------------------------------------------------------------------

#[test]
fn sphere_cylinder_non_coaxial_not_available() {
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    // Cylinder axis ∥ +z but offset to x=0.5,y=0 — does NOT pass through the
    // sphere center, so d_ax = 0.5 ≥ TAU_MODEL ⇒ non-coaxial (general degree-4).
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.5, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    assert_eq!(
        intersect(&sphere, &cyl),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
    // Symmetric order also unavailable.
    assert_eq!(
        intersect(&cyl, &sphere),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
}
