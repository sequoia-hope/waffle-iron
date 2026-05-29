//! PR-SSI1 — RED tests for the exact-SSI foundation.
//!
//! These tests target the not-yet-existing public API of `ssi-rs`:
//! `QuadricSurface`, `SsiCurve`, `SsiError`, `SsiCurve::eval`, and the
//! `intersect` dispatcher. They are written against `intersect` (the public
//! dispatcher) so they are robust to internal solver function naming.
//!
//! Spec: specs/ssi_pr_ssi1_foundation.md
//! Invariants: I1 (on-surface), I2 (analytical geometry), I3 (branch coverage),
//! I4 (symmetry), I5 (determinism).

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

// ---------------------------------------------------------------------------
// On-surface oracle (I1). Samples the curve at N parameter values and asserts
// each sample satisfies BOTH input surfaces' implicit equations within
// TAU_MODEL. Circle: t over [0, 2π). Line: t over [-5, 5].
// ---------------------------------------------------------------------------

fn implicit_residual(surf: &QuadricSurface, x: [f64; 3]) -> f64 {
    match surf {
        QuadricSurface::Plane { point, normal } => {
            // |n·(x − point)|, n assumed unit.
            dot(normal.as_array(), sub(x, point.as_array())).abs()
        }
        QuadricSurface::Sphere { center, radius } => {
            // | |x − center| − radius |.
            (norm(sub(x, center.as_array())) - radius).abs()
        }
    }
}

fn assert_on_both_surfaces(curve: &SsiCurve, a: &QuadricSurface, b: &QuadricSurface) {
    const N: usize = 64;
    for i in 0..N {
        let t = match curve {
            SsiCurve::Circle { .. } => {
                // [0, 2π)
                (i as f64) / (N as f64) * std::f64::consts::TAU
            }
            SsiCurve::Line { .. } => {
                // [-5, 5]
                -5.0 + (i as f64) / ((N - 1) as f64) * 10.0
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

// Pull a single Line out of a result, asserting exactly one curve of that kind.
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

// Unit vectors equal up to sign.
fn parallel_up_to_sign(a: [f64; 3], b: [f64; 3]) {
    // a, b assumed unit; cross product magnitude ~0 means parallel.
    let c = cross(a, b);
    assert!(
        norm(c) < TAU_MODEL,
        "expected {a:?} parallel to {b:?} (|cross| = {})",
        norm(c)
    );
}

// ---------------------------------------------------------------------------
// plane_plane (spec §plane_plane table) — I3 branch coverage
// ---------------------------------------------------------------------------

#[test]
fn plane_plane_transverse_yields_y_axis_line() {
    // z=0 plane ∩ x=0 plane → the y-axis.
    let pz = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let px = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let curves = intersect(&pz, &px).expect("transverse planes intersect in a line");
    let (_point, dir) = expect_single_line(&curves);

    // I1: on-surface oracle.
    assert_on_both_surfaces(&curves[0], &pz, &px);

    // I2: dir ⟂ both normals (i.e. dir along y-axis).
    approx(dot(dir.as_array(), [0.0, 0.0, 1.0]), 0.0);
    approx(dot(dir.as_array(), [1.0, 0.0, 0.0]), 0.0);
    // dir is the y-axis up to sign.
    parallel_up_to_sign(dir.as_array(), [0.0, 1.0, 0.0]);
    // dir is unit.
    approx(norm(dir.as_array()), 1.0);
}

#[test]
fn plane_plane_parallel_distinct_yields_empty() {
    // z=0 and z=1 — parallel, distinct.
    let p0 = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let p1 = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 1.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let curves = intersect(&p0, &p1).expect("parallel distinct planes: Ok([])");
    assert!(curves.is_empty(), "expected no curves, got {curves:?}");
}

#[test]
fn plane_plane_coincident_is_degenerate() {
    // z=0 twice — coincident (overlap is 2D).
    let a = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let b = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    assert_eq!(intersect(&a, &b), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// plane_sphere (spec §plane_sphere table) — I3 branch coverage
// ---------------------------------------------------------------------------

#[test]
fn plane_sphere_transverse_yields_circle() {
    // plane z=0, sphere center (0,0,0.5) r=1.
    // d = 0.5, radius = √(1 − 0.25) = √0.75, center = (0,0,0), normal = +z.
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.5),
        radius: 1.0,
    };
    let curves = intersect(&plane, &sphere).expect("transverse plane/sphere: one circle");
    let (center, normal, radius) = expect_single_circle(&curves);

    // I1: on-surface oracle.
    assert_on_both_surfaces(&curves[0], &plane, &sphere);

    // I2: analytical geometry.
    approx(radius, 0.75_f64.sqrt()); // √0.75 ≈ 0.8660254037844386
    approx_point(center, [0.0, 0.0, 0.0]); // foot of perpendicular
    parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]); // normal ∥ plane normal
    approx(norm(normal.as_array()), 1.0);
}

#[test]
fn plane_sphere_tangent_yields_empty() {
    // plane z=0, sphere center (0,0,1) r=1 → |d| == r (point contact).
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let curves = intersect(&plane, &sphere).expect("tangent plane/sphere: Ok([])");
    assert!(curves.is_empty(), "expected no curves, got {curves:?}");
}

#[test]
fn plane_sphere_disjoint_yields_empty() {
    // plane z=0, sphere center (0,0,2) r=1 → |d| = 2 > r.
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 2.0),
        radius: 1.0,
    };
    let curves = intersect(&plane, &sphere).expect("disjoint plane/sphere: Ok([])");
    assert!(curves.is_empty(), "expected no curves, got {curves:?}");
}

#[test]
fn plane_sphere_nonpositive_radius_is_degenerate() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 0.0,
    };
    assert_eq!(intersect(&plane, &sphere), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// sphere_sphere (spec §sphere_sphere table) — I3 branch coverage
// ---------------------------------------------------------------------------

#[test]
fn sphere_sphere_transverse_yields_circle() {
    // centers (0,0,0) r=1 and (1,0,0) r=1.
    // D=1, a = (1 + 1 − 1)/(2·1) = 0.5, center = (0.5,0,0),
    // radius = √(1 − 0.25) = √0.75, normal = +x.
    let a = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let b = QuadricSurface::Sphere {
        center: Point3::new(1.0, 0.0, 0.0),
        radius: 1.0,
    };
    let curves = intersect(&a, &b).expect("transverse sphere/sphere: one circle");
    let (center, normal, radius) = expect_single_circle(&curves);

    // I1: on-surface oracle.
    assert_on_both_surfaces(&curves[0], &a, &b);

    // I2: analytical geometry per spec formula.
    approx(radius, 0.75_f64.sqrt());
    approx_point(center, [0.5, 0.0, 0.0]);
    parallel_up_to_sign(normal.as_array(), [1.0, 0.0, 0.0]);
    approx(norm(normal.as_array()), 1.0);
}

#[test]
fn sphere_sphere_tangent_yields_empty() {
    // D = r_a + r_b: (0,0,0) r=1 and (2,0,0) r=1, D=2.
    let a = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let b = QuadricSurface::Sphere {
        center: Point3::new(2.0, 0.0, 0.0),
        radius: 1.0,
    };
    let curves = intersect(&a, &b).expect("tangent sphere/sphere: Ok([])");
    assert!(curves.is_empty(), "expected no curves, got {curves:?}");
}

#[test]
fn sphere_sphere_disjoint_yields_empty() {
    // D > r_a + r_b: (0,0,0) r=1 and (3,0,0) r=1, D=3.
    let a = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let b = QuadricSurface::Sphere {
        center: Point3::new(3.0, 0.0, 0.0),
        radius: 1.0,
    };
    let curves = intersect(&a, &b).expect("disjoint sphere/sphere: Ok([])");
    assert!(curves.is_empty(), "expected no curves, got {curves:?}");
}

#[test]
fn sphere_sphere_contained_yields_empty() {
    // D < |r_a − r_b|: (0,0,0) r=2 and (0.1,0,0) r=0.5, D=0.1, |r_a−r_b|=1.5.
    let a = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let b = QuadricSurface::Sphere {
        center: Point3::new(0.1, 0.0, 0.0),
        radius: 0.5,
    };
    let curves = intersect(&a, &b).expect("contained sphere/sphere: Ok([])");
    assert!(curves.is_empty(), "expected no curves, got {curves:?}");
}

#[test]
fn sphere_sphere_concentric_is_degenerate() {
    // D < TAU_MODEL: same center.
    let a = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let b = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 0.5,
    };
    assert_eq!(intersect(&a, &b), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// I4 — symmetry: intersect(a,b) and intersect(b,a) describe the same geometry.
// ---------------------------------------------------------------------------

#[test]
fn symmetry_plane_plane_line() {
    let pz = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let px = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let ab = intersect(&pz, &px).unwrap();
    let ba = intersect(&px, &pz).unwrap();
    let (pt_ab, dir_ab) = expect_single_line(&ab);
    let (pt_ba, dir_ba) = expect_single_line(&ba);

    // dir equal up to sign.
    parallel_up_to_sign(dir_ab.as_array(), dir_ba.as_array());

    // Both points lie on the same (y-axis) line: the difference must be parallel
    // to the shared direction.
    let delta = sub(pt_ab.as_array(), pt_ba.as_array());
    if norm(delta) > TAU_MODEL {
        parallel_up_to_sign(
            [
                delta[0] / norm(delta),
                delta[1] / norm(delta),
                delta[2] / norm(delta),
            ],
            dir_ab.as_array(),
        );
    }
}

#[test]
fn symmetry_sphere_sphere_circle() {
    let a = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let b = QuadricSurface::Sphere {
        center: Point3::new(1.0, 0.0, 0.0),
        radius: 1.0,
    };
    let ab = intersect(&a, &b).unwrap();
    let ba = intersect(&b, &a).unwrap();
    let (c_ab, n_ab, r_ab) = expect_single_circle(&ab);
    let (c_ba, n_ba, r_ba) = expect_single_circle(&ba);

    approx_point(c_ab, c_ba.as_array());
    approx(r_ab, r_ba);
    parallel_up_to_sign(n_ab.as_array(), n_ba.as_array()); // normal up to sign
}

// ---------------------------------------------------------------------------
// I5 — determinism: identical inputs → identical (==) outputs across calls.
// ---------------------------------------------------------------------------

#[test]
fn determinism_repeated_calls_identical() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.5),
        radius: 1.0,
    };
    let first = intersect(&plane, &sphere);
    let second = intersect(&plane, &sphere);
    assert_eq!(first, second, "intersect must be deterministic");

    let pp_first = intersect(&plane, &plane);
    let pp_second = intersect(&plane, &plane);
    assert_eq!(pp_first, pp_second);
}
