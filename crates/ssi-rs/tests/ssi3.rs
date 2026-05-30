//! PR-SSI3 — RED tests for the plane∩cone solver (bounded sections).
//!
//! These tests target the not-yet-existing API:
//! `QuadricSurface::Cone { apex, axis_dir, half_angle }`, reached through the
//! public `intersect` dispatcher (the `plane_cone` solver fn is private).
//! No new `SsiCurve` variants — PR-SSI3 reuses `Circle`/`Ellipse`.
//!
//! Spec: specs/ssi_pr_ssi3_plane_cone.md
//! Branch table:
//!   E1  invalid cone / degenerate input          → Err(DegenerateInput)
//!   AP  through-apex (apex on cutting plane)      → Err(DegenerateInput)
//!   C1  circle  (plane ⟂ axis, s_n < TAU)         → one Circle
//!   C2  ellipse (closed oblique section)          → one Ellipse
//!   PH  parabola / hyperbola (unbounded)          → Err(AnalyticalSolutionNotAvailable)
//! Invariants: I1 (on-surface, cone RADIAL residual), I2 (analytical geometry),
//! I3 (branch coverage), I4 (symmetry), I5 (determinism).

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
// TAU_MODEL. For a (plane, cone) pair:
//   - plane residual:  |n̂·(x − p)|.
//   - cone RADIAL residual (NOT the squared implicit form): with
//     h = (x − apex)·â and r_actual = |(x − apex) − h·â|, the residual is
//     | r_actual − |h|·tanα |  (a length).
// â is normalized here defensively (Cone.axis_dir need not be unit on input).
// Curves sampled: Circle / Ellipse over [0, 2π).
// ---------------------------------------------------------------------------

fn implicit_residual(surf: &QuadricSurface, x: [f64; 3]) -> f64 {
    match surf {
        QuadricSurface::Plane { point, normal } => {
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
    const N: usize = 64; // ≥ 16 per the spec; 64 mirrors ssi2.rs.
    for i in 0..N {
        let t = match curve {
            SsiCurve::Circle { .. } | SsiCurve::Ellipse { .. } => {
                (i as f64) / (N as f64) * std::f64::consts::TAU
            }
            SsiCurve::Line { .. } => -5.0 + (i as f64) / ((N - 1) as f64) * 10.0,
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
// Extractors.
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

// ---------------------------------------------------------------------------
// Independent vertex construction (spec C2 §): the two symmetry-plane
// generators g_± = cosα·â ± sinα·û, û = normalize(n̂ − (n̂·â)â). Each pierces
// the cutting plane at s_± = n̂·(p − apex)/(n̂·g_±), V_± = apex + s_±·g_±.
// Used by the C2 test to check the solver against an INDEPENDENT derivation.
// ---------------------------------------------------------------------------

#[allow(clippy::type_complexity)]
fn ellipse_vertices(
    apex: [f64; 3],
    axis_dir: [f64; 3],
    half_angle: f64,
    plane_point: [f64; 3],
    plane_normal: [f64; 3],
) -> ([f64; 3], [f64; 3]) {
    let ahat = unit(axis_dir);
    let nhat = unit(plane_normal);
    let cosa = half_angle.cos();
    let sina = half_angle.sin();
    let k = dot(nhat, ahat);
    let uhat = unit(sub(nhat, scale(ahat, k)));
    let g_plus = add(scale(ahat, cosa), scale(uhat, sina));
    let g_minus = sub(scale(ahat, cosa), scale(uhat, sina));
    let rhs = dot(nhat, sub(plane_point, apex));
    let s_plus = rhs / dot(nhat, g_plus);
    let s_minus = rhs / dot(nhat, g_minus);
    let v_plus = add(apex, scale(g_plus, s_plus));
    let v_minus = add(apex, scale(g_minus, s_minus));
    (v_plus, v_minus)
}

// ---------------------------------------------------------------------------
// C1 — plane ⟂ axis → one Circle (I2, I1).
// ---------------------------------------------------------------------------

#[test]
fn c1_perpendicular_yields_circle() {
    // Double cone apex origin, axis +z, half_angle α = π/4 (tanα = 1).
    // Plane z = 3 (normal +z) ⟂ axis ⇒ s_n = 0 ⇒ C1.
    // Circle: center (0,0,3) (on axis AND in plane), normal +z (∥ axis),
    // radius = |h|·tanα = 3·1 = 3.
    let alpha = std::f64::consts::FRAC_PI_4;
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let curves = intersect(&plane, &cone).expect("perpendicular plane/cone: one circle");
    let (center, normal, radius) = expect_single_circle(&curves);

    // I1: on-surface oracle.
    assert_on_both_surfaces(&curves[0], &plane, &cone);

    // I2: analytical geometry. h = 3, tanα = 1 ⇒ radius = 3.
    approx(radius, 3.0 * alpha.tan());
    approx_point(center, [0.0, 0.0, 3.0]); // on axis AND in plane
    parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]); // normal ∥ axis
    approx(norm(normal.as_array()), 1.0);
}

#[test]
fn c1_perpendicular_clean_tan_half() {
    // Same shape with a different clean tanα: α = atan(0.5) ⇒ tanα = 0.5.
    // Plane z = 4 ⇒ radius = |h|·tanα = 4·0.5 = 2.
    let alpha = 0.5_f64.atan();
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 4.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let curves = intersect(&plane, &cone).expect("perpendicular plane/cone: one circle");
    let (center, normal, radius) = expect_single_circle(&curves);

    assert_on_both_surfaces(&curves[0], &plane, &cone);
    approx(radius, 2.0); // 4 · 0.5
    approx_point(center, [0.0, 0.0, 4.0]);
    parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
}

// ---------------------------------------------------------------------------
// C2 — oblique closed section → one Ellipse (I2, I1).
// ---------------------------------------------------------------------------

#[test]
fn c2_oblique_yields_ellipse() {
    // Cone apex origin, axis +z, half_angle α = π/4 ⇒ sinα = √2/2 ≈ 0.707.
    // Plane normal tilted 20° from +z in the x–z plane:
    //   n̂ = (sin20°, 0, cos20°) ⇒ |n̂·â| = cos20° ≈ 0.94 > sinα ⇒ CLOSED ellipse.
    // Plane offset to z = 5 region so the apex is NOT on the plane (not AP).
    let alpha = std::f64::consts::FRAC_PI_4;
    let theta = 20.0_f64.to_radians();
    let apex = [0.0, 0.0, 0.0];
    let axis = [0.0, 0.0, 1.0];
    let nrm = [theta.sin(), 0.0, theta.cos()];
    let ppt = [0.0, 0.0, 5.0]; // plane through (0,0,5); apex off-plane.

    let plane = QuadricSurface::Plane {
        point: Point3::new(ppt[0], ppt[1], ppt[2]),
        normal: Vector3::new(nrm[0], nrm[1], nrm[2]),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(apex[0], apex[1], apex[2]),
        axis_dir: Vector3::new(axis[0], axis[1], axis[2]),
        half_angle: alpha,
    };
    let curves = intersect(&plane, &cone).expect("oblique plane/cone: one ellipse");
    let (center, normal, major_axis, major_radius, minor_radius) = expect_single_ellipse(&curves);

    // I1: on-surface oracle — every ellipse sample lies on BOTH surfaces.
    assert_on_both_surfaces(&curves[0], &plane, &cone);

    // Independent vertex derivation (proves the solver, not just consistency).
    let (v_plus, v_minus) = ellipse_vertices(apex, axis, alpha, ppt, nrm);

    // Both vertices lie on the cone AND in the plane.
    for v in [v_plus, v_minus] {
        assert!(
            implicit_residual(&cone, v) < TAU_MODEL,
            "vertex {v:?} not on cone"
        );
        assert!(
            implicit_residual(&plane, v) < TAU_MODEL,
            "vertex {v:?} not in plane"
        );
    }

    // I2: analytical geometry.
    let expect_center = scale(add(v_plus, v_minus), 0.5);
    approx_point(center, expect_center); // center = midpoint of the vertices
    approx(major_radius, norm(sub(v_plus, v_minus)) / 2.0); // a = |V₊−V₋|/2

    // minor_radius b = √((d·â)²/cos²α − |d|²), d = center − apex.
    let nhat = unit(nrm);
    let ahat = unit(axis);
    let d = sub(expect_center, apex);
    let cosa = alpha.cos();
    let da = dot(d, ahat);
    let expect_b = (da * da / (cosa * cosa) - dot(d, d)).sqrt();
    approx(minor_radius, expect_b);

    // normal is the unit plane normal.
    parallel_up_to_sign(normal.as_array(), nhat);
    approx(norm(normal.as_array()), 1.0);

    // major_axis is unit, in-plane (⟂ normal), and ∥ (V₊ − V₋).
    approx(norm(major_axis.as_array()), 1.0);
    approx(dot(major_axis.as_array(), nhat), 0.0);
    parallel_up_to_sign(major_axis.as_array(), unit(sub(v_plus, v_minus)));

    // minor_axis = normal × major_axis ⟂ both major_axis and normal.
    let minor = cross(normal.as_array(), major_axis.as_array());
    approx(dot(minor, major_axis.as_array()), 0.0);
    approx(dot(minor, nhat), 0.0);

    // a ≥ b.
    assert!(
        major_radius >= minor_radius - TAU_MODEL,
        "major {major_radius} must be ≥ minor {minor_radius}"
    );
}

// ---------------------------------------------------------------------------
// AP — plane through the apex → Err(DegenerateInput) (degenerate conic).
// ---------------------------------------------------------------------------

#[test]
fn ap_through_apex_is_degenerate() {
    // Apex at (0,0,0) lies on the plane z = 0 ⇒ |n̂·(apex − p)| = 0 ⇒ AP.
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    assert_eq!(intersect(&plane, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn ap_through_apex_oblique_is_degenerate() {
    // Apex offset from origin, plane tilted but still passing through the apex.
    let apex = Point3::new(1.0, 2.0, 3.0);
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 2.0, 3.0), // plane passes through the apex
        normal: Vector3::new(0.3, 0.0, 1.0),
    };
    let cone = QuadricSurface::Cone {
        apex,
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    assert_eq!(intersect(&plane, &cone), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// PH — parabola: a generator is parallel to the plane (|n̂·â| = sinα).
// → Err(AnalyticalSolutionNotAvailable) (staged gap, PR-SSI4).
// ---------------------------------------------------------------------------

#[test]
fn ph_parabola_not_available() {
    // α = π/4 ⇒ sinα = √2/2. Choose a plane normal tilted 45° from +z so
    //   |n̂·â| = cos45° = √2/2 = sinα EXACTLY ⇒ a generator ∥ plane ⇒ parabola.
    // Apex NOT on the plane (offset to z = 2) so this is PH, not AP.
    let alpha = std::f64::consts::FRAC_PI_4;
    let theta = std::f64::consts::FRAC_PI_4; // 45° tilt of n̂ from +z.
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 2.0),
        normal: Vector3::new(theta.sin(), 0.0, theta.cos()),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    assert_eq!(
        intersect(&plane, &cone),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
}

// ---------------------------------------------------------------------------
// PH — hyperbola: plane "shallower than the cone" (|n̂·â| < sinα), e.g. a plane
// parallel to the axis (normal ⟂ axis, |n̂·â| = 0).
// → Err(AnalyticalSolutionNotAvailable) (staged gap, PR-SSI4).
// ---------------------------------------------------------------------------

#[test]
fn ph_hyperbola_not_available() {
    // Plane x = 1 (normal +x ⟂ axis +z) ⇒ |n̂·â| = 0 < sinα ⇒ hyperbola.
    // Apex at origin is off the plane (x = 0 ≠ 1) ⇒ not AP.
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    assert_eq!(
        intersect(&plane, &cone),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
}

// ---------------------------------------------------------------------------
// E1 — invalid cone / degenerate input → Err(DegenerateInput).
// ---------------------------------------------------------------------------

#[test]
fn e1_half_angle_zero_is_degenerate() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 0.0, // α ≤ TAU_MODEL ⇒ degenerate (a line, not a cone).
    };
    assert_eq!(intersect(&plane, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_half_angle_half_pi_is_degenerate() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_2, // α ≥ π/2 − TAU ⇒ degenerate (a plane).
    };
    assert_eq!(intersect(&plane, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_axis_dir_is_degenerate() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 0.0), // zero-length axis.
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    assert_eq!(intersect(&plane, &cone), Err(SsiError::DegenerateInput));
}

#[test]
fn e1_zero_plane_normal_is_degenerate() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 0.0), // zero-length normal.
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    assert_eq!(intersect(&plane, &cone), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// I4 — symmetry: intersect(plane, cone) == intersect(cone, plane), same
// geometry up to sign of major_axis / normal.
// ---------------------------------------------------------------------------

#[test]
fn symmetry_c1_circle() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let ab = intersect(&plane, &cone).unwrap();
    let ba = intersect(&cone, &plane).unwrap();
    let (c_ab, n_ab, r_ab) = expect_single_circle(&ab);
    let (c_ba, n_ba, r_ba) = expect_single_circle(&ba);

    approx_point(c_ab, c_ba.as_array());
    approx(r_ab, r_ba);
    parallel_up_to_sign(n_ab.as_array(), n_ba.as_array());
}

#[test]
fn symmetry_c2_ellipse() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let theta = 20.0_f64.to_radians();
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 5.0),
        normal: Vector3::new(theta.sin(), 0.0, theta.cos()),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let ab = intersect(&plane, &cone).unwrap();
    let ba = intersect(&cone, &plane).unwrap();
    let (c_ab, n_ab, m_ab, ar_ab, br_ab) = expect_single_ellipse(&ab);
    let (c_ba, n_ba, m_ba, ar_ba, br_ba) = expect_single_ellipse(&ba);

    approx_point(c_ab, c_ba.as_array());
    approx(ar_ab, ar_ba);
    approx(br_ab, br_ba);
    parallel_up_to_sign(n_ab.as_array(), n_ba.as_array());
    parallel_up_to_sign(m_ab.as_array(), m_ba.as_array());
}

// ---------------------------------------------------------------------------
// I5 — determinism: identical inputs → byte-identical outputs across calls.
// ---------------------------------------------------------------------------

#[test]
fn determinism_c2_ellipse_identical() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let theta = 20.0_f64.to_radians();
    let mk = || {
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 5.0),
            normal: Vector3::new(theta.sin(), 0.0, theta.cos()),
        };
        let cone = QuadricSurface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: alpha,
        };
        intersect(&plane, &cone)
    };
    let first = mk();
    let second = mk();
    // Byte-identical structurally (PartialEq over the exact fields).
    assert_eq!(first, second, "ellipse output must be deterministic");

    // And identical at a fixed eval parameter.
    let cf = first.unwrap();
    let cs = second.unwrap();
    let t = 0.7;
    assert_eq!(cf[0].eval(t).as_array(), cs[0].eval(t).as_array());
}
