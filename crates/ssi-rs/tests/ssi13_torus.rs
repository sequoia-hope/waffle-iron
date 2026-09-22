//! PR-SSI13 — the M5 TORUS arm (`specs/m5_surface_pair_curve.md` "Torus
//! arm"): `QuadricSurface::Torus` joins the pair vocabulary.
//!
//! A torus pair in general position is degree 8 with no conic closed form
//! ([#1] Patrikalakis Ch.5); it is returned as the procedural
//! `SsiCurve::SurfacePair { a, b }` (both operands verbatim, call order
//! preserved — T2/T3/T5). The one closed form kept here is the
//! PERPENDICULAR plane section (T1): the pair of parallel circles. Identical
//! tori are `DegenerateInput` (T4); invalid operands are E1.
//!
//! The descriptor arms are pure pass-throughs (no numeric solve), so these
//! tests assert operand identity, argument-order preservation, and that the
//! T1 circles and the T4/E1 rejections are not stolen.

use cad_primitives::{Point3, Vector3, TAU_MODEL};
use ssi_rs::{intersect, QuadricSurface, SsiCurve, SsiError};

fn torus(c: [f64; 3], a: [f64; 3], rr: f64, r: f64) -> QuadricSurface {
    QuadricSurface::Torus {
        center: Point3::new(c[0], c[1], c[2]),
        axis_dir: Vector3::new(a[0], a[1], a[2]),
        major_radius: rr,
        minor_radius: r,
    }
}

fn plane(p: [f64; 3], n: [f64; 3]) -> QuadricSurface {
    QuadricSurface::Plane {
        point: Point3::new(p[0], p[1], p[2]),
        normal: Vector3::new(n[0], n[1], n[2]),
    }
}

fn cyl(ap: [f64; 3], ad: [f64; 3], r: f64) -> QuadricSurface {
    QuadricSurface::Cylinder {
        axis_point: Point3::new(ap[0], ap[1], ap[2]),
        axis_dir: Vector3::new(ad[0], ad[1], ad[2]),
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

/// Signed distance of `p` to the torus surface (the implicit the pair
/// descriptor denotes).
fn torus_residual(t: &QuadricSurface, p: [f64; 3]) -> f64 {
    let QuadricSurface::Torus {
        center,
        axis_dir,
        major_radius,
        minor_radius,
    } = t
    else {
        unreachable!()
    };
    let c = center.as_array();
    let a = axis_dir.as_array();
    let al = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let a = [a[0] / al, a[1] / al, a[2] / al];
    let w = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
    let h = w[0] * a[0] + w[1] * a[1] + w[2] * a[2];
    let rad = [w[0] - h * a[0], w[1] - h * a[1], w[2] - h * a[2]];
    let rho = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
    ((rho - major_radius).powi(2) + h * h).sqrt() - minor_radius
}

// ---------------------------------------------------------------------------
// T1 — plane ⊥ axis: parallel circles.
// ---------------------------------------------------------------------------

#[test]
fn t1_perpendicular_plane_through_tube_centre_plane_gives_two_equators() {
    let t = torus([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 3.0, 1.0);
    let p = plane([5.0, -2.0, 0.0], [0.0, 0.0, 1.0]);
    let curves = intersect(&p, &t).expect("perpendicular plane ⇒ circles");
    assert_eq!(curves.len(), 2, "{curves:?}");
    let mut radii: Vec<f64> = curves
        .iter()
        .map(|c| match c {
            SsiCurve::Circle {
                center,
                normal,
                radius,
            } => {
                assert!((center.z()).abs() < 1e-15 && center.x().abs() < 1e-15);
                assert!((normal.z().abs() - 1.0).abs() < 1e-15);
                *radius
            }
            other => panic!("expected Circle, got {other:?}"),
        })
        .collect();
    radii.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!((radii[0] - 2.0).abs() < 1e-15 && (radii[1] - 4.0).abs() < 1e-15);
    // Every circle point satisfies the torus implicit.
    for c in &curves {
        for k in 0..12 {
            let q = c.eval(k as f64 * 0.5).as_array();
            assert!(torus_residual(&t, q).abs() < 1e-12);
        }
    }
    // Symmetry (I4): the swapped call gives the same set.
    let swapped = intersect(&t, &p).expect("swapped ⇒ Ok");
    assert_eq!(swapped.len(), 2);
}

#[test]
fn t1_perpendicular_plane_offset_along_axis_gives_two_shrunken_circles() {
    // Axis along +y, centre (1, 2, 3); plane at h = 0.6 along the axis
    // (with the plane normal pointing the OTHER way — h is axis-signed).
    let t = torus([1.0, 2.0, 3.0], [0.0, 1.0, 0.0], 3.0, 1.0);
    let p = plane([0.0, 2.6, 0.0], [0.0, -1.0, 0.0]);
    let curves = intersect(&p, &t).expect("perpendicular ⇒ circles");
    assert_eq!(curves.len(), 2);
    let s = (1.0f64 - 0.36).sqrt();
    let mut radii: Vec<f64> = Vec::new();
    for c in &curves {
        let SsiCurve::Circle { center, radius, .. } = c else {
            panic!("expected Circle, got {c:?}");
        };
        assert!((center.x() - 1.0).abs() < 1e-15);
        assert!((center.y() - 2.6).abs() < 1e-15);
        assert!((center.z() - 3.0).abs() < 1e-15);
        radii.push(*radius);
        for k in 0..12 {
            let q = c.eval(k as f64 * 0.5).as_array();
            assert!(torus_residual(&t, q).abs() < 1e-12);
        }
    }
    radii.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!((radii[0] - (3.0 - s)).abs() < 1e-14);
    assert!((radii[1] - (3.0 + s)).abs() < 1e-14);
}

#[test]
fn t1_perpendicular_plane_tangent_to_crown_gives_one_circle_and_beyond_gives_none() {
    let t = torus([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 3.0, 1.0);
    let crown = plane([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
    let curves = intersect(&crown, &t).expect("tangent crown plane ⇒ one circle");
    assert_eq!(curves.len(), 1);
    match curves[0] {
        SsiCurve::Circle { radius, center, .. } => {
            assert!((radius - 3.0).abs() < 1e-15);
            assert!((center.z() - 1.0).abs() < 1e-15);
        }
        other => panic!("expected Circle, got {other:?}"),
    }
    let beyond = plane([0.0, 0.0, 1.0 + 1e-3], [0.0, 0.0, 1.0]);
    assert!(intersect(&beyond, &t).expect("beyond ⇒ Ok").is_empty());
}

// ---------------------------------------------------------------------------
// T2 — oblique plane: the spiric section is the surface-pair descriptor.
// ---------------------------------------------------------------------------

#[test]
fn t2_oblique_plane_is_surface_pair_in_call_order() {
    let t = torus([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 3.0, 1.0);
    let p = plane([0.0, 0.0, 0.0], [1.0, 0.0, 1.0]);
    assert_surface_pair(&intersect(&p, &t).expect("oblique ⇒ SurfacePair"), p, t);
    assert_surface_pair(&intersect(&t, &p).expect("oblique ⇒ SurfacePair"), t, p);
    // A plane CONTAINING the axis (the meridian pair of circles) is not
    // special-cased: it is the descriptor too.
    let axial = plane([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
    assert_surface_pair(
        &intersect(&axial, &t).expect("axial ⇒ SurfacePair"),
        axial,
        t,
    );
}

// ---------------------------------------------------------------------------
// T3 — torus × cylinder / cone / sphere: the descriptor, call order kept.
// ---------------------------------------------------------------------------

#[test]
fn t3_torus_partners_are_surface_pairs_in_call_order() {
    let t = torus([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 2.0, 0.5);
    let c = cyl([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.3);
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 5.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.4,
    };
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(2.0, 0.0, 0.0),
        radius: 0.9,
    };
    for partner in [c, cone, sphere] {
        assert_surface_pair(
            &intersect(&t, &partner).expect("torus × partner"),
            t,
            partner,
        );
        assert_surface_pair(
            &intersect(&partner, &t).expect("partner × torus"),
            partner,
            t,
        );
    }
    // Coaxial cylinder (circles in closed form) is NOT special-cased in
    // this arm: still the exact descriptor.
    let coax = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 2.0);
    assert_surface_pair(
        &intersect(&t, &coax).expect("coaxial ⇒ SurfacePair"),
        t,
        coax,
    );
}

// ---------------------------------------------------------------------------
// T4/T5 — torus × torus.
// ---------------------------------------------------------------------------

#[test]
fn t5_distinct_tori_are_a_surface_pair_and_t4_identical_tori_are_degenerate() {
    // R0050's class: parallel axes offset by R_A − R_B.
    let a = torus([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 3.9509, 2.6339);
    let b = torus([0.1749, 0.0, 0.0], [0.0, 0.0, 1.0], 3.7759, 2.5173);
    assert_surface_pair(&intersect(&a, &b).expect("distinct tori"), a, b);
    assert_surface_pair(&intersect(&b, &a).expect("distinct tori"), b, a);
    // Same geometry with an unnormalized, reversed axis: identical ⇒ degenerate.
    let a_dup = torus([0.0, 0.0, 0.0], [0.0, 0.0, -2.0], 3.9509, 2.6339);
    assert_eq!(intersect(&a, &a_dup), Err(SsiError::DegenerateInput));
    // A radius differing by more than TAU_MODEL is a distinct torus.
    let a_near = torus(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        3.9509 + 10.0 * TAU_MODEL,
        2.6339,
    );
    assert_surface_pair(&intersect(&a, &a_near).expect("distinct radii"), a, a_near);
}

// ---------------------------------------------------------------------------
// E1 — invalid operands.
// ---------------------------------------------------------------------------

#[test]
fn e1_invalid_torus_or_partner_is_degenerate_input() {
    let good = torus([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 3.0, 1.0);
    let c = cyl([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.3);
    // Spindle torus (r ≥ R), zero minor radius, non-finite centre, zero axis.
    for bad in [
        torus([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0),
        torus([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 3.0, 0.0),
        torus([f64::NAN, 0.0, 0.0], [0.0, 0.0, 1.0], 3.0, 1.0),
        torus([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 3.0, 1.0),
    ] {
        assert_eq!(
            intersect(&bad, &c),
            Err(SsiError::DegenerateInput),
            "{bad:?}"
        );
        assert_eq!(
            intersect(&c, &bad),
            Err(SsiError::DegenerateInput),
            "{bad:?}"
        );
        assert_eq!(
            intersect(&bad, &good),
            Err(SsiError::DegenerateInput),
            "{bad:?}"
        );
    }
    // Invalid partner.
    let bad_cyl = cyl([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], -0.3);
    assert_eq!(intersect(&good, &bad_cyl), Err(SsiError::DegenerateInput));
    let bad_plane = plane([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    assert_eq!(intersect(&good, &bad_plane), Err(SsiError::DegenerateInput));
}
