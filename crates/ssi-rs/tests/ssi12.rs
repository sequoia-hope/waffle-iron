//! PR-SSI12 — cylinder ∩ cylinder, general position → the procedural
//! SURFACE-PAIR descriptor (M5, `specs/m5_surface_pair_curve.md`).
//!
//! The non-parallel cyl×cyl intersection is a degree-4 space curve with no
//! conic closed form ([#1] Patrikalakis Ch.5) EXCEPT the equal-radius,
//! coplanar-intersecting case (two ellipses, PR-SSI11). Every other
//! non-parallel configuration — unequal radius (S2) or skew axes (S3) — is
//! now returned as `SsiCurve::SurfacePair { a, b }`: the two cylinders
//! verbatim, in argument order. Per the Constitution P8 degree-4
//! clarification and [#24] Yang et al. 2025 §4.1.2/§4.3, a procedural curve
//! whose defining surfaces are exact IS an analytical representation.
//!
//! The descriptor is a pure pass-through: no numeric solve happens in ssi-rs
//! (concrete points are certified downstream by yang-rs Newton projection).
//! These tests therefore assert operand identity, argument-order preservation,
//! determinism, and that the parallel/ellipse/degenerate arms are NOT stolen.

use cad_primitives::{Point3, Vector3};
use ssi_rs::{intersect, QuadricSurface, SsiCurve};

/// `cyl(axis_point, axis_dir, r)` from raw components (need not be unit).
fn cyl(ax: f64, ay: f64, az: f64, dx: f64, dy: f64, dz: f64, r: f64) -> QuadricSurface {
    QuadricSurface::Cylinder {
        axis_point: Point3::new(ax, ay, az),
        axis_dir: Vector3::new(dx, dy, dz),
        radius: r,
    }
}

/// The sole element must be `SurfacePair { a: expect_a, b: expect_b }`.
fn assert_surface_pair(got: &[SsiCurve], expect_a: QuadricSurface, expect_b: QuadricSurface) {
    assert_eq!(got.len(), 1, "surface-pair is ONE descriptor, got {got:?}");
    match got[0] {
        SsiCurve::SurfacePair { a, b } => {
            assert_eq!(a, expect_a, "operand a preserved verbatim");
            assert_eq!(b, expect_b, "operand b preserved verbatim");
        }
        other => panic!("expected SurfacePair, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// S2 — non-parallel, UNEQUAL radius (any coplanarity) → SurfacePair.
// ---------------------------------------------------------------------------

#[test]
fn s2_perpendicular_unequal_radius() {
    // z-axis cyl r=1 ∩ x-axis cyl r=0.5 through the origin (intersecting axes,
    // but unequal radius ⇒ NOT the equal-R ellipse case).
    let a = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
    let b = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5);
    let curves = intersect(&a, &b).expect("unequal-R non-parallel ⇒ Ok(SurfacePair)");
    assert_surface_pair(&curves, a, b);
    // Argument order is preserved (a/b swap with the call order).
    let swapped = intersect(&b, &a).expect("swapped ⇒ Ok");
    assert_surface_pair(&swapped, b, a);
}

#[test]
fn s2_oblique_unequal_radius() {
    // 45°-tilted second axis, unequal radius. Still S2.
    let a = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0);
    let b = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 3.0);
    let curves = intersect(&a, &b).expect("oblique unequal-R ⇒ SurfacePair");
    assert_surface_pair(&curves, a, b);
}

// ---------------------------------------------------------------------------
// S3 — non-parallel, EQUAL radius, SKEW axes (not coplanar) → SurfacePair.
// ---------------------------------------------------------------------------

#[test]
fn s3_equal_radius_skew_axes() {
    // Perpendicular directions, z-offset so the axes do not intersect ⇒ skew.
    let a = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0);
    let b = cyl(0.0, 0.0, 5.0, 0.0, 1.0, 0.0, 2.0);
    let curves = intersect(&a, &b).expect("equal-R skew ⇒ SurfacePair");
    assert_surface_pair(&curves, a, b);
    let swapped = intersect(&b, &a).expect("swapped ⇒ Ok");
    assert_surface_pair(&swapped, b, a);
}

#[test]
fn s3_unequal_radius_skew_axes() {
    // The fully general case: skew AND unequal radius.
    let a = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.5);
    let b = cyl(0.0, 0.0, 3.0, 0.0, 1.0, 0.0, 2.7);
    let curves = intersect(&a, &b).expect("general skew unequal-R ⇒ SurfacePair");
    assert_surface_pair(&curves, a, b);
}

// ---------------------------------------------------------------------------
// The descriptor is topology-free: even DISJOINT non-parallel cylinders (no
// real intersection) return it. Membership is a downstream (Stage-3) concern
// — ssi-rs reports the curve's DEFINITION, not whether a mesh edge matches it
// (spec §branch table: "Disjoint-surface configurations return the
// descriptor too").
// ---------------------------------------------------------------------------

#[test]
fn disjoint_non_parallel_still_returns_descriptor() {
    // Two thin cylinders far apart with perpendicular, unequal radii: no real
    // curve, but the descriptor is still the correct DEFINITION.
    let a = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.5);
    let b = cyl(100.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.3);
    let curves = intersect(&a, &b).expect("disjoint non-parallel ⇒ Ok(descriptor)");
    assert_surface_pair(&curves, a, b);
}

// ---------------------------------------------------------------------------
// Determinism (I5): repeated identical calls are byte-identical.
// ---------------------------------------------------------------------------

#[test]
fn descriptor_is_deterministic() {
    let a = cyl(0.1, -0.2, 0.3, 0.0, 0.0, 1.0, 1.25);
    let b = cyl(0.0, 0.0, 0.4, 1.0, 0.0, 0.0, 0.75);
    let first = intersect(&a, &b).expect("ok");
    let second = intersect(&a, &b).expect("ok");
    assert_eq!(first, second, "same inputs ⇒ byte-identical output");
}

// ---------------------------------------------------------------------------
// The surface-pair arm does NOT steal the solved arms: equal-R coplanar
// intersecting still yields ellipses (PR-SSI11), and parallel axes still
// yield the SSI10 line/empty results.
// ---------------------------------------------------------------------------

#[test]
fn equal_r_coplanar_intersecting_still_ellipses_not_surface_pair() {
    // Perpendicular axes through the origin, EQUAL radius ⇒ two ellipses.
    let a = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0);
    let b = cyl(0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0);
    let curves = intersect(&a, &b).expect("equal-R coplanar ⇒ ellipses");
    assert_eq!(curves.len(), 2, "two ellipses, got {curves:?}");
    for c in &curves {
        assert!(
            matches!(c, SsiCurve::Ellipse { .. }),
            "equal-R coplanar stays Ellipse, not SurfacePair: {c:?}"
        );
    }
}

#[test]
fn parallel_axes_still_lines_not_surface_pair() {
    // Parallel z-axes, secant (3-4-5): two lines, never a surface-pair.
    let a = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 5.0);
    let b = cyl(6.0, 0.0, 0.0, 0.0, 0.0, 1.0, 5.0);
    let curves = intersect(&a, &b).expect("parallel secant ⇒ lines");
    assert!(
        curves.iter().all(|c| matches!(c, SsiCurve::Line { .. })),
        "parallel axes stay Line, not SurfacePair: {curves:?}"
    );
}
