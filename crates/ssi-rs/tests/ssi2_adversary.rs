//! PR-SSI2 — Adversarial audit of the plane∩cylinder solver.
//!
//! These tests attack `plane_cylinder` (reached via the public `intersect`
//! dispatcher) at its band boundaries (C1↔C2 perpendicular limit, C2↔C3
//! parallel limit, C3a tangent limit), under oblique non-axis-aligned axes,
//! at extreme scale, and on the ellipse `eval` frame integrity. They do NOT
//! touch production code.
//!
//! Spec: specs/ssi_pr_ssi2_plane_cylinder.md
//! Reuses ssi1_adversary's on-surface oracle + finite-field patterns:
//! the cylinder residual `|(x−q)×â|/|â| − r`, `assert_curve_finite`.
//!
//! Tolerance note: TAU_MODEL = 1e-7. The absolute on-surface oracle is valid
//! only while curve sample coordinates stay below ~1e8 (the PR-SSI1 finding).
//! Where a band drives `major_radius` huge (C2↔C3), tests switch to a RELATIVE
//! analytical check and explicitly characterize the absolute-oracle breakpoint.

use cad_primitives::{Point3, Vector3, TAU_MODEL};
use ssi_rs::{intersect, QuadricSurface, SsiCurve};

// ---------------------------------------------------------------------------
// Vector helpers (cad-primitives is types-only).
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

/// Every field of a returned curve must be finite (no NaN/Inf). The core
/// anti-`√(negative)` / anti-`0/0` / anti-`r/0` guard.
fn assert_curve_finite(c: &SsiCurve) {
    match c {
        SsiCurve::SurfacePair { .. } => unreachable!(
            "this suite's solvers never produce a surface-pair curve (M5 cyl×cyl only)"
        ),
        SsiCurve::Line { point, dir } => {
            for v in point.as_array().iter().chain(dir.as_array().iter()) {
                assert!(v.is_finite(), "Line field non-finite: {c:?}");
            }
        }
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => {
            for v in center.as_array().iter().chain(normal.as_array().iter()) {
                assert!(v.is_finite(), "Circle field non-finite: {c:?}");
            }
            assert!(radius.is_finite(), "Circle radius non-finite: {c:?}");
            assert!(*radius > 0.0, "Circle radius must be > 0: {c:?}");
        }
        SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            for v in center
                .as_array()
                .iter()
                .chain(normal.as_array().iter())
                .chain(major_axis.as_array().iter())
            {
                assert!(v.is_finite(), "Ellipse field non-finite: {c:?}");
            }
            assert!(major_radius.is_finite(), "Ellipse major non-finite: {c:?}");
            assert!(minor_radius.is_finite(), "Ellipse minor non-finite: {c:?}");
            assert!(*major_radius > 0.0, "Ellipse major must be > 0: {c:?}");
            assert!(*minor_radius > 0.0, "Ellipse minor must be > 0: {c:?}");
        }
        // Not produced by PR-SSI2 solvers; compile-keepalive for the extended
        // enum (PR-SSI4 added `Parabola`/`Hyperbola`).
        SsiCurve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } => {
            for v in vertex
                .as_array()
                .iter()
                .chain(normal.as_array().iter())
                .chain(axis_dir.as_array().iter())
            {
                assert!(v.is_finite(), "Parabola field non-finite: {c:?}");
            }
            assert!(focal_length.is_finite(), "Parabola focal non-finite: {c:?}");
            assert!(*focal_length > 0.0, "Parabola focal must be > 0: {c:?}");
        }
        SsiCurve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => {
            for v in center
                .as_array()
                .iter()
                .chain(normal.as_array().iter())
                .chain(major_axis.as_array().iter())
            {
                assert!(v.is_finite(), "Hyperbola field non-finite: {c:?}");
            }
            assert!(
                semi_transverse.is_finite(),
                "Hyperbola semi_transverse non-finite: {c:?}"
            );
            assert!(
                semi_conjugate.is_finite(),
                "Hyperbola semi_conjugate non-finite: {c:?}"
            );
            assert!(
                *semi_transverse > 0.0,
                "Hyperbola semi_transverse must be > 0: {c:?}"
            );
            assert!(
                *semi_conjugate > 0.0,
                "Hyperbola semi_conjugate must be > 0: {c:?}"
            );
        }
    }
}

/// Absolute implicit residual on a surface (PR-SSI1 oracle, extended to
/// cylinder via `|(x−q)×â|/|â| − r`).
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
            let v = sub(x, axis_point.as_array());
            let a = axis_dir.as_array();
            (norm(cross(v, a)) / norm(a) - radius).abs()
        }
        QuadricSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            // Cone RADIAL residual: | r_actual − |h|·tanα |, where
            //   h = (x − apex)·â, r_actual = |(x − apex) − h·â| = |(x−apex)×â|/|â|.
            // axis_dir normalized defensively.
            let v = sub(x, apex.as_array());
            let a = axis_dir.as_array();
            let alen = norm(a);
            let h = dot(v, a) / alen;
            let r_actual = norm(cross(v, a)) / alen;
            (r_actual - h.abs() * half_angle.tan()).abs()
        }
    }
}

/// Max absolute on-surface residual over N samples of a curve against both
/// surfaces. Circles/ellipses sweep [0, 2π); lines sweep [-5, 5].
fn max_residual_on_both(curve: &SsiCurve, a: &QuadricSurface, b: &QuadricSurface, n: usize) -> f64 {
    let mut m = 0.0_f64;
    for i in 0..n {
        let t = match curve {
            SsiCurve::SurfacePair { .. } => unreachable!(
                "this suite's solvers never produce a surface-pair curve (M5 cyl×cyl only)"
            ),
            SsiCurve::Circle { .. } | SsiCurve::Ellipse { .. } => {
                (i as f64) / (n as f64) * std::f64::consts::TAU
            }
            SsiCurve::Line { .. } => -5.0 + (i as f64) / ((n - 1) as f64) * 10.0,
            // Not produced by PR-SSI2 solvers; compile-keepalive for the
            // extended enum (PR-SSI4). Bounded range [−3, 3].
            SsiCurve::Parabola { .. } | SsiCurve::Hyperbola { .. } => {
                (i as f64) / ((n - 1) as f64) * 6.0 - 3.0
            }
        };
        let p = curve.eval(t).as_array();
        m = m.max(implicit_residual(a, p)).max(implicit_residual(b, p));
    }
    m
}

fn assert_on_both_surfaces(curve: &SsiCurve, a: &QuadricSurface, b: &QuadricSurface) {
    let m = max_residual_on_both(curve, a, b, 64);
    assert!(m < TAU_MODEL, "max on-surface residual {m} >= TAU_MODEL");
}

// Build a plane normal that is a unit vector tilted by angle theta from +z in
// the x–z plane: n̂ = (sinθ, 0, cosθ) ⇒ c = n̂·ẑ = cosθ. So |c| sweeps with θ.
fn tilted_z_normal(theta: f64) -> Vector3 {
    Vector3::new(theta.sin(), 0.0, theta.cos())
}

fn z_cylinder(r: f64) -> QuadricSurface {
    QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: r,
    }
}

// ===========================================================================
// Attack 1: C1↔C2 band boundary (the perpendicular limit |c| → 1).
//
// The C1 (snap-to-perpendicular) circle has normal = â, so it lies on the
// cylinder exactly but OFF the tilted cutting plane by ~r·sin θ = r·√(1−c²).
// The fix (spec 5a3cded6) gates C1 on the SINE √(1−c²) < TAU_MODEL — NOT on
// 1−|c| — so that off-plane error is bounded by r·TAU_MODEL (the original
// 1−|c| band let it reach ~√(2·TAU)·r ≈ 4.5e-4·r, ~4000× tolerance). This test
// asserts the corrected guarantee: inside the band the circle is on BOTH
// surfaces within r·TAU_MODEL; just outside, the ellipse is exact.
// ===========================================================================

#[test]
fn attack1_c1c2_perpendicular_band_no_blowup() {
    let r = 2.0;
    let cyl = z_cylinder(r);
    // C1 fires when sin θ = √(1−c²) < TAU_MODEL, i.e. θ < asin(TAU_MODEL) ≈ 1e-7.
    let theta_band = TAU_MODEL.asin();

    // Tilt angles straddling the band: well inside C1, just inside, just
    // outside, and well into C2.
    let thetas = [
        0.0,                // exactly perpendicular ⇒ C1, sin θ = 0
        theta_band * 0.5,   // inside band ⇒ C1
        theta_band * 0.999, // just inside band ⇒ C1
        theta_band * 1.001, // just outside band ⇒ C2
        theta_band * 100.0, // C2
        0.1,                // comfortably C2
    ];

    for theta in thetas {
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: tilted_z_normal(theta),
        };
        let c = (theta.cos()).abs();
        // The C1/C2 discriminant is |proj| = sin θ. Use theta.sin() directly:
        // (1−c²).sqrt() suffers catastrophic cancellation near c≈1, whereas
        // production computes the stabler vector form |â − c·n̂|.
        let sin_theta = theta.sin();
        let curves = intersect(&plane, &cyl)
            .unwrap_or_else(|e| panic!("theta={theta}: must not error, got {e:?}"));
        assert_eq!(curves.len(), 1, "theta={theta}: expected one curve");
        assert_curve_finite(&curves[0]);

        match curves[0] {
            SsiCurve::Circle { radius, normal, .. } => {
                // C1 branch — must be exactly r, and we must be in the
                // SINE-gated band (√(1−c²) < TAU_MODEL), not the old 1−|c| band.
                assert!(
                    sin_theta < TAU_MODEL,
                    "theta={theta}: Circle returned but sinθ={sin_theta} not < TAU_MODEL"
                );
                assert!(
                    (radius - r).abs() < TAU_MODEL,
                    "theta={theta}: circle radius {radius} != r {r}"
                );
                // The circle's normal is the axis â (snap-to-perpendicular).
                parallel_up_to_sign_local(normal.as_array(), [0.0, 0.0, 1.0]);
                let mut max_cyl = 0.0_f64;
                let mut max_plane = 0.0_f64;
                for i in 0..256 {
                    let t = (i as f64) / 256.0 * std::f64::consts::TAU;
                    let p = curves[0].eval(t).as_array();
                    max_cyl = max_cyl.max(implicit_residual(&cyl, p));
                    max_plane = max_plane.max(implicit_residual(&plane, p));
                }
                // On the cylinder exactly.
                assert!(
                    max_cyl < TAU_MODEL,
                    "theta={theta}: C1 circle off the CYLINDER (residual {max_cyl})"
                );
                // CORRECTED GUARANTEE: with the sine-gated band the off-plane
                // residual is ≤ r·sin θ < r·TAU_MODEL (a small radius-scaled
                // bound), NOT the old ~4.5e-4·r. This is the assertion that
                // fails on the pre-fix (1−|c|)-gated code.
                let snap_bound = r * sin_theta + 8.0 * f64::EPSILON * r;
                assert!(
                    max_plane <= snap_bound,
                    "theta={theta}: C1 plane residual {max_plane} exceeds snap bound {snap_bound}"
                );
                assert!(
                    max_plane < r * TAU_MODEL + 1e-12,
                    "theta={theta}: C1 plane residual {max_plane} exceeds r·TAU_MODEL \
                     — band not sine-gated"
                );
            }
            SsiCurve::Ellipse {
                major_radius,
                minor_radius,
                ..
            } => {
                // C2 branch — must be just outside the sine-gated band.
                assert!(
                    sin_theta >= TAU_MODEL,
                    "theta={theta}: Ellipse returned but sinθ={sin_theta} inside C1 band"
                );
                assert!(major_radius.is_finite() && minor_radius.is_finite());
                // minor == r exactly; major == r/|c|.
                assert!(
                    (minor_radius - r).abs() < TAU_MODEL,
                    "theta={theta}: minor {minor_radius} != r {r}"
                );
                assert!(
                    (major_radius - r / c).abs() < TAU_MODEL * (r / c),
                    "theta={theta}: major {major_radius} != r/|c| {}",
                    r / c
                );
                // Never a circle masquerading: a ≥ b, and near the band a is
                // still within a sane multiple of r (no discontinuity blow-up).
                assert!(
                    major_radius >= minor_radius - TAU_MODEL,
                    "theta={theta}: major {major_radius} < minor {minor_radius}"
                );
                assert!(
                    major_radius < 2.0 * r,
                    "theta={theta}: major {major_radius} blew up near band (should ≈ r)"
                );
                // C2 is EXACT on both surfaces: the ellipse is the true conic.
                // Geometry is O(r), coords O(major) ≤ O(2r) — absolute oracle
                // valid. This is the real correctness guarantee at the band.
                assert_on_both_surfaces(&curves[0], &plane, &cyl);
            }
            ref other => panic!("theta={theta}: unexpected curve {other:?}"),
        }
    }
}

// Local helper: unit vectors equal up to sign (|cross| ≈ 0).
fn parallel_up_to_sign_local(a: [f64; 3], b: [f64; 3]) {
    assert!(
        norm(cross(a, b)) < TAU_MODEL,
        "expected {a:?} parallel to {b:?}"
    );
}

#[test]
fn attack1_c1c2_major_radius_continuous_into_band() {
    // As |c| → 1⁻, major_radius = r/|c| → r smoothly (no jump at the C1 edge).
    let r = 1.0;
    let cyl = z_cylinder(r);
    let theta_band = (1.0 - TAU_MODEL).acos();
    // A descending sequence of θ approaching the band edge from C2 side.
    let mut prev: Option<f64> = None;
    for k in 1..=6 {
        let theta = theta_band * (1.0 + 10.0_f64.powi(-k));
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: tilted_z_normal(theta),
        };
        let curves = intersect(&plane, &cyl).unwrap();
        if let SsiCurve::Ellipse { major_radius, .. } = curves[0] {
            assert!(
                major_radius >= r - TAU_MODEL,
                "major {major_radius} < r {r}"
            );
            // Monotonically approaching r from above as θ shrinks.
            if let Some(p) = prev {
                assert!(
                    major_radius <= p + TAU_MODEL,
                    "major_radius not decreasing toward r: {major_radius} > prev {p}"
                );
            }
            prev = Some(major_radius);
            // Within a sane multiple of r near the band.
            assert!(
                major_radius < 1.001 * r,
                "major {major_radius} not ≈ r near band"
            );
        }
    }
}

// ===========================================================================
// Attack 2: C2↔C3 band boundary (the parallel limit |c| → TAU_MODEL).
//
// Sweep |c| down toward TAU_MODEL from above. major_radius = r/|c| becomes
// huge. Assert ellipse fields stay finite (no Inf). Characterize where the
// ABSOLUTE on-surface oracle breaks (far ellipse point has big coordinates),
// and verify the ANALYTICAL major_radius == r/|c| with a RELATIVE tolerance
// independently.
// ===========================================================================

#[test]
fn attack2_c2c3_parallel_band_finite_and_relative_correct() {
    let r = 1.0;
    let cyl = z_cylinder(r);
    // c = cos θ; we want small |c|, i.e. θ near π/2. Set c directly: θ = acos(c).
    // Sweep c from 1e-2 down to just above TAU_MODEL.
    let cs: [f64; 8] = [1e-2, 1e-3, 1e-4, 1e-5, 1e-6, 2e-7, 1.5e-7, 1.0000001e-7];
    for &c_target in &cs {
        let theta = c_target.acos(); // cos θ = c_target
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: tilted_z_normal(theta),
        };
        let curves = intersect(&plane, &cyl)
            .unwrap_or_else(|e| panic!("c={c_target}: must not error, got {e:?}"));
        assert_eq!(curves.len(), 1, "c={c_target}: expected one curve");
        assert_curve_finite(&curves[0]); // NO Inf/NaN even with huge major

        if let SsiCurve::Ellipse {
            major_radius,
            minor_radius,
            ..
        } = curves[0]
        {
            // Actual |c| the solver saw (cos of the chosen θ; tiny rounding).
            let c_actual = theta.cos().abs();
            // ANALYTICAL correctness, RELATIVE tolerance: major == r/|c|.
            let want = r / c_actual;
            let rel = (major_radius - want).abs() / want;
            assert!(
                rel < 1e-9,
                "c={c_target}: major {major_radius} rel-err {rel} vs r/|c| {want}"
            );
            assert!(major_radius.is_finite(), "c={c_target}: major not finite");
            assert!(
                (minor_radius - r).abs() < TAU_MODEL,
                "c={c_target}: minor {minor_radius} != r"
            );
            // a ≥ b always.
            assert!(major_radius >= minor_radius);
        } else {
            panic!("c={c_target}: expected Ellipse, got {:?}", curves[0]);
        }
    }
}

#[test]
fn attack2_c2c3_absolute_oracle_breakpoint_characterization() {
    // CHARACTERIZATION (not a solver bug). FINDING: for plane_cylinder C2
    // ellipses the absolute TAU_MODEL on-surface oracle is governed by the
    // CYLINDER RADIUS r, NOT by how huge `major_radius = r/|c|` grows. The
    // ellipse `eval` keeps every sample exactly in-plane (major_axis ⟂ n̂ by
    // construction) and the cylinder residual tracks r, so a giant major axis
    // (very oblique cut) does NOT by itself break the oracle.
    //
    // Empirically (full 1024-sample sweep):
    //   r = 1   : residual ~2e-16 even at major = 1e6 (|c| = 1e-6)  — HOLDS
    //   r = 1e5 : residual ~3e-11 even at major = 1e10            — HOLDS
    //   r = 1e7 : residual ~4e-9  even at major = 1e12            — HOLDS
    //   r = 1e9 : residual exceeds TAU_MODEL                       — BREAKS
    // So the breakpoint is r ≈ 1e8 (the PR-SSI1 ceiling), reached via the
    // RADIUS, not via obliquity. This test locks both halves of that finding.

    // (a) |c| swept tiny at r = 1: absolute oracle HOLDS at every major up to
    // 1e6 (this is the surprising, stronger-than-expected result).
    let r = 1.0;
    let cyl = z_cylinder(r);
    let cs: [f64; 5] = [1e-2, 1e-3, 1e-4, 1e-5, 1e-6];
    for &c_target in &cs {
        let theta = c_target.acos();
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: tilted_z_normal(theta),
        };
        let curves = intersect(&plane, &cyl).unwrap();
        let SsiCurve::Ellipse { major_radius, .. } = curves[0] else {
            panic!("c={c_target}: expected ellipse");
        };
        assert!(major_radius >= r / c_target * (1.0 - 1e-6)); // major really is huge
        let m = max_residual_on_both(&curves[0], &plane, &cyl, 1024);
        assert!(
            m < TAU_MODEL,
            "r=1, c={c_target} (major≈{major_radius:e}): absolute oracle broke \
             (residual {m}) — obliquity unexpectedly drives the breakpoint"
        );
    }

    // (b) The breakpoint IS driven by r. At r = 1e9 the absolute oracle is
    // EXPECTED to exceed TAU_MODEL even at a mild obliquity (|c| = 1e-2),
    // while the RELATIVE residual stays tiny (solver still correct).
    {
        let big_r = 1.0e9;
        let cyl = z_cylinder(big_r);
        let theta = 1e-2_f64.acos();
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: tilted_z_normal(theta),
        };
        let curves = intersect(&plane, &cyl).unwrap();
        assert_curve_finite(&curves[0]);
        let m = max_residual_on_both(&curves[0], &plane, &cyl, 1024);
        assert!(
            m >= TAU_MODEL,
            "r=1e9: absolute oracle unexpectedly held (residual {m}); the \
             radius-driven breakpoint moved — re-characterize"
        );
        // Solver still analytically correct: relative residual tiny.
        assert!(
            m / big_r < 1e-9,
            "r=1e9: relative residual {} too big (solver actually wrong)",
            m / big_r
        );
    }
}

// ===========================================================================
// Attack 3: C3a tangent boundary (d ≈ r) — the √(r²−d²) trap.
//
// Sweep parallel-plane distance d across r. d < r−TAU ⇒ two Lines (no NaN);
// |d−r| ≤ TAU ⇒ one Line; d > r+TAU ⇒ empty. No √(negative). Both lines at
// distance exactly r from the axis; as d→r⁻ the two lines converge.
// ===========================================================================

#[test]
fn attack3_c3a_tangent_band_sweep_no_nan() {
    let r = 2.0;
    let cyl = z_cylinder(r);
    // Plane normal +x (parallel to +z axis ⇒ |c| = 0 ⇒ C3). Plane x = d.
    let make_plane = |d: f64| QuadricSurface::Plane {
        point: Point3::new(d, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };

    // Distances straddling the tangent band r±TAU.
    let cases: &[(f64, &str)] = &[
        (r * (1.0 - 1e-2), "secant"),
        (r * (1.0 - 1e-4), "secant"),
        (r * (1.0 - 1e-6), "secant"),
        (r - 1e-6, "secant-near"), // d = r − 1e-6, still < r − TAU? TAU=1e-7, so 1e-6 > TAU ⇒ secant
        (r, "tangent"),
        (r + 1e-9, "tangent"), // within |d−r| ≤ TAU
        (r - 1e-9, "tangent"), // within |d−r| ≤ TAU (1e-9 < TAU=1e-7)
        (r * (1.0 + 1e-2), "disjoint"),
        (r + 1.0, "disjoint"),
    ];

    for &(d, kind) in cases {
        let plane = make_plane(d);
        let curves = intersect(&plane, &cyl)
            .unwrap_or_else(|e| panic!("d={d} ({kind}): must not error, got {e:?}"));
        for c in &curves {
            assert_curve_finite(c); // no √(negative) NaN ever
        }
        match kind {
            "secant" | "secant-near" => {
                assert_eq!(curves.len(), 2, "d={d} ({kind}): expected two lines");
                for c in &curves {
                    if let SsiCurve::Line { point, .. } = c {
                        let dist = {
                            let x = point.as_array();
                            // distance from z-axis = √(x²+y²)
                            (x[0] * x[0] + x[1] * x[1]).sqrt()
                        };
                        assert!(
                            (dist - r).abs() < TAU_MODEL,
                            "d={d}: line at dist {dist} != r {r}"
                        );
                    }
                    assert_on_both_surfaces(c, &plane, &cyl);
                }
            }
            "tangent" => {
                assert_eq!(curves.len(), 1, "d={d} ({kind}): expected one line");
                assert_on_both_surfaces(&curves[0], &plane, &cyl);
            }
            "disjoint" => {
                assert!(
                    curves.is_empty(),
                    "d={d} ({kind}): expected empty, got {curves:?}"
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn attack3_c3a_two_lines_converge_as_d_approaches_r() {
    // As d → r⁻, off = √(r²−d²) → 0, so the two lines converge toward the
    // single tangent line. Verify the separation shrinks monotonically.
    let r = 2.0;
    let cyl = z_cylinder(r);
    let mut prev_sep = f64::INFINITY;
    // d = r·(1 − 10^-k), each above r − TAU so still C3a (secant). For k large,
    // r − d = r·10^-k must stay > TAU=1e-7 ⇒ 2·10^-k > 1e-7 ⇒ k ≤ 7.
    for k in 1..=6 {
        let d = r * (1.0 - 10.0_f64.powi(-k));
        let plane = QuadricSurface::Plane {
            point: Point3::new(d, 0.0, 0.0),
            normal: Vector3::new(1.0, 0.0, 0.0),
        };
        let curves = intersect(&plane, &cyl).unwrap();
        assert_eq!(curves.len(), 2, "d={d}: expected two lines (secant)");
        let p0 = match curves[0] {
            SsiCurve::Line { point, .. } => point.as_array(),
            _ => panic!(),
        };
        let p1 = match curves[1] {
            SsiCurve::Line { point, .. } => point.as_array(),
            _ => panic!(),
        };
        let sep = norm(sub(p0, p1));
        assert!(
            sep < prev_sep,
            "k={k}: separation {sep} not shrinking (prev {prev_sep}) as d→r"
        );
        assert!(
            sep > 0.0,
            "k={k}: lines collapsed prematurely (sep 0) at d={d}"
        );
        prev_sep = sep;
    }
    // Final separation should be small (lines nearly converged).
    assert!(prev_sep < 1e-2, "final separation {prev_sep} not near zero");
}

// ===========================================================================
// Attack 3b: C3a determinism + distinctness of the two lines.
//
// The +ŵ-first ordering is stable across repeated calls AND the two lines are
// distinct (don't collapse) for d well below r.
// ===========================================================================

#[test]
fn attack3b_c3a_order_stable_and_lines_distinct() {
    let r = 3.0;
    let cyl = z_cylinder(r);
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let first = intersect(&plane, &cyl).unwrap();
    // Repeated calls: byte-identical (I5), same order.
    for _ in 0..5 {
        let again = intersect(&plane, &cyl).unwrap();
        assert_eq!(first, again, "two-line output not deterministic");
    }
    assert_eq!(first.len(), 2);
    let p0 = match first[0] {
        SsiCurve::Line { point, .. } => point.as_array(),
        _ => panic!(),
    };
    let p1 = match first[1] {
        SsiCurve::Line { point, .. } => point.as_array(),
        _ => panic!(),
    };
    // d = 1, r = 3 ⇒ off = √8 ≈ 2.83; the two lines are clearly distinct.
    let sep = norm(sub(p0, p1));
    assert!(
        sep > 1.0,
        "lines collapsed/too close (sep {sep}); expected ≈ 2·√8"
    );
    let off = (r * r - 1.0).sqrt();
    assert!(
        (sep - 2.0 * off).abs() < TAU_MODEL,
        "separation {sep} != 2·off {}",
        2.0 * off
    );
}

// ===========================================================================
// Attack 4: oblique / non-axis-aligned cylinder.
//
// Cylinder axis (1,2,2)/3 cut by an oblique plane → Ellipse on both surfaces;
// major_axis in-plane and ⟂ minor_axis; center on BOTH the axis line and the
// plane.
// ===========================================================================

#[test]
fn attack4_oblique_cylinder_ellipse_frame_and_center() {
    // Non-axis cylinder.
    let axis_pt = [0.5, -1.0, 2.0];
    let axis_dir = [1.0, 2.0, 2.0]; // |·| = 3 (non-unit on input too)
    let r = 1.5;
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::from(axis_pt),
        axis_dir: Vector3::from(axis_dir),
        radius: r,
    };
    // Oblique plane not perpendicular and not parallel to the axis.
    let pnormal = [0.3, 0.1, 0.9]; // normalized in solver
    let ppoint = [0.0, 0.0, 0.0];
    let plane = QuadricSurface::Plane {
        point: Point3::from(ppoint),
        normal: Vector3::from(pnormal),
    };

    let curves = intersect(&plane, &cyl).expect("oblique cylinder/plane → curve");
    assert_eq!(curves.len(), 1);
    assert_curve_finite(&curves[0]);

    let SsiCurve::Ellipse {
        center,
        normal,
        major_axis,
        major_radius,
        minor_radius,
    } = curves[0]
    else {
        panic!("expected ellipse, got {:?}", curves[0]);
    };

    let nhat = unit(pnormal);
    let ahat = unit(axis_dir);
    let c = dot(nhat, ahat).abs();

    // I2: minor == r, major == r/|c|.
    assert!((minor_radius - r).abs() < TAU_MODEL);
    assert!((major_radius - r / c).abs() < TAU_MODEL * (r / c));

    // major_axis is unit, in-plane (⟂ normal), and ⟂ minor_axis.
    let maj = major_axis.as_array();
    assert!((norm(maj) - 1.0).abs() < TAU_MODEL, "major_axis not unit");
    assert!(
        dot(maj, normal.as_array()).abs() < TAU_MODEL,
        "major_axis not in-plane"
    );
    let minor = cross(normal.as_array(), maj);
    assert!(dot(minor, maj).abs() < TAU_MODEL, "major ⊥ minor failed");
    assert!(
        dot(minor, normal.as_array()).abs() < TAU_MODEL,
        "minor not in-plane"
    );
    assert!(
        (norm(normal.as_array()) - 1.0).abs() < TAU_MODEL,
        "normal not unit"
    );

    // center lies on the PLANE: n̂·(center − p) = 0.
    let ctr = center.as_array();
    assert!(
        dot(nhat, sub(ctr, ppoint)).abs() < TAU_MODEL,
        "center not on plane"
    );
    // center lies on the AXIS LINE: (center − q) parallel to â, i.e. perp
    // distance to the axis line is ~0.
    let rel = sub(ctr, axis_pt);
    let along = scale(ahat, dot(rel, ahat));
    let perp = sub(rel, along);
    assert!(
        norm(perp) < TAU_MODEL,
        "center not on axis line (perp dist {})",
        norm(perp)
    );

    // On-surface oracle: geometry is O(1) so absolute oracle valid.
    assert_on_both_surfaces(&curves[0], &plane, &cyl);
}

// ===========================================================================
// Attack 5: extreme scale.
//
// radius 1e6 and ~1e-5; large axis_point offsets. Per PR-SSI1, the absolute
// on-surface oracle holds to ~1e8 coord magnitude; verify analytical
// correctness with a RELATIVE check at large scale, and report the absolute-
// oracle breakpoint.
// ===========================================================================

#[test]
fn attack5_large_scale_circle_relative_and_absolute() {
    // Perpendicular cut of a big cylinder ⇒ C1 circle of radius r.
    let r = 1.0e6;
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(1.0e6, -2.0e6, 3.0e6),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: r,
    };
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 5.0e5),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let curves = intersect(&plane, &cyl).expect("large perpendicular → circle");
    assert_eq!(curves.len(), 1);
    assert_curve_finite(&curves[0]);
    let SsiCurve::Circle { center, radius, .. } = curves[0] else {
        panic!("expected circle");
    };
    // RELATIVE radius correctness.
    assert!(
        (radius - r).abs() / r < 1e-12,
        "radius {radius} rel-off vs {r}"
    );
    // center on axis (x=1e6, y=-2e6) and in plane (z = 5e5).
    let ctr = center.as_array();
    assert!((ctr[0] - 1.0e6).abs() / 1e6 < 1e-12);
    assert!((ctr[1] + 2.0e6).abs() / 2e6 < 1e-12);
    assert!((ctr[2] - 5.0e5).abs() / 5e5 < 1e-12);

    // RELATIVE on-surface oracle at large scale (absolute would fail near 1e9
    // per PR-SSI1; here coords ≈ few×1e6 so absolute should still hold).
    let m = max_residual_on_both(&curves[0], &plane, &cyl, 256);
    assert!(
        m / r < 1e-9,
        "large-scale relative on-surface residual {} too big",
        m / r
    );
}

#[test]
fn attack5_large_scale_absolute_oracle_breakpoint() {
    // Characterize where the absolute on-surface oracle breaks for
    // plane_cylinder C1 circles. Per PR-SSI1: holds ~1e6, breaks ~1e9.
    for &r in &[1.0e6_f64, 1.0e9_f64] {
        let cyl = QuadricSurface::Cylinder {
            axis_point: Point3::new(r, r, r),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        };
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, r * 1.5),
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        let curves = intersect(&plane, &cyl).unwrap();
        let m = max_residual_on_both(&curves[0], &plane, &cyl, 256);
        // Relative correctness holds at every scale.
        assert!(m / r < 1e-9, "r={r:e}: relative residual {} too big", m / r);
        if r <= 1.0e6 {
            assert!(
                m < TAU_MODEL,
                "r=1e6: absolute oracle unexpectedly broke (residual {m})"
            );
        } else {
            // r = 1e9: absolute oracle EXPECTED to exceed TAU_MODEL.
            assert!(
                m >= TAU_MODEL,
                "r=1e9: absolute oracle unexpectedly held ({m}); breakpoint moved"
            );
        }
    }
}

#[test]
fn attack5_tiny_scale_ellipse_relative_correct() {
    // Tiny cylinder r = 1e-5, oblique cut ⇒ ellipse. Check relative + absolute
    // (coords tiny, well within absolute oracle).
    let r = 1.0e-5;
    let cyl = z_cylinder(r);
    let theta = 0.3; // c = cos 0.3 ≈ 0.955 ⇒ C2
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: tilted_z_normal(theta),
    };
    let curves = intersect(&plane, &cyl).expect("tiny oblique → ellipse");
    assert_curve_finite(&curves[0]);
    let SsiCurve::Ellipse {
        major_radius,
        minor_radius,
        ..
    } = curves[0]
    else {
        panic!("expected ellipse");
    };
    let c = theta.cos().abs();
    assert!((minor_radius - r).abs() / r < 1e-12, "minor rel-off");
    assert!(
        (major_radius - r / c).abs() / (r / c) < 1e-12,
        "major rel-off"
    );
    // Absolute oracle: coords O(1e-5) ≪ 1e8 ⇒ holds.
    assert_on_both_surfaces(&curves[0], &plane, &cyl);
}

// ===========================================================================
// Attack 6: ellipse eval frame integrity.
//
// t=0 ⇒ center + a·major_axis; t=π/2 ⇒ center + b·minor_axis. The two
// semi-axis endpoints are ⟂ about the center, at distances a and b. So eval
// builds an orthonormal-scaled frame, not skewed.
// ===========================================================================

#[test]
fn attack6_ellipse_eval_frame_orthonormal_scaled() {
    let r = 1.0;
    let cyl = z_cylinder(r);
    // A handful of obliquities.
    for theta in [0.2_f64, 0.5, 0.9, 1.2] {
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.3, -0.4, 0.0),
            normal: tilted_z_normal(theta),
        };
        let curves = intersect(&plane, &cyl).unwrap();
        let SsiCurve::Ellipse {
            center,
            major_axis,
            major_radius,
            minor_radius,
            ..
        } = curves[0]
        else {
            panic!("theta={theta}: expected ellipse");
        };
        let ctr = center.as_array();
        let p0 = curves[0].eval(0.0).as_array(); // center + a·major
        let p_quarter = curves[0].eval(std::f64::consts::FRAC_PI_2).as_array(); // center + b·minor

        let v_major = sub(p0, ctr);
        let v_minor = sub(p_quarter, ctr);

        // distances are a and b.
        assert!(
            (norm(v_major) - major_radius).abs() < TAU_MODEL,
            "theta={theta}: |t=0 endpoint − c| {} != a {major_radius}",
            norm(v_major)
        );
        assert!(
            (norm(v_minor) - minor_radius).abs() < TAU_MODEL,
            "theta={theta}: |t=π/2 endpoint − c| {} != b {minor_radius}",
            norm(v_minor)
        );
        // semi-axis endpoints perpendicular about the center (orthonormal frame).
        assert!(
            dot(v_major, v_minor).abs() < TAU_MODEL,
            "theta={theta}: semi-axes not ⟂ (dot {})",
            dot(v_major, v_minor)
        );
        // v_major direction is the stated major_axis.
        let maj_dir = unit(v_major);
        let c = cross(maj_dir, major_axis.as_array());
        assert!(norm(c) < TAU_MODEL, "theta={theta}: t=0 dir != major_axis");
    }
}

// ===========================================================================
// Attack 7: non-unit input axis_dir.
//
// Spec says axis_dir "need not be unit on input". Give magnitude-5 axis_dir in
// a C1 and a C2 case; results must equal the unit-axis case up to sign.
// ===========================================================================

#[test]
fn attack7_nonunit_axis_dir_c1_matches_unit() {
    // Perpendicular plane ⇒ C1 circle. Axis +z, once unit, once ×5.
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 4.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cyl_unit = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let cyl_big = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0), // magnitude 5
        radius: 2.0,
    };
    let cu = intersect(&plane, &cyl_unit).unwrap();
    let cb = intersect(&plane, &cyl_big).unwrap();
    let (ctr_u, n_u, r_u) = match cu[0] {
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => (center.as_array(), normal.as_array(), radius),
        _ => panic!(),
    };
    let (ctr_b, n_b, r_b) = match cb[0] {
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => (center.as_array(), normal.as_array(), radius),
        _ => panic!(),
    };
    assert!(norm(sub(ctr_u, ctr_b)) < TAU_MODEL, "centers differ");
    assert!((r_u - r_b).abs() < TAU_MODEL, "radii differ");
    // normal is the normalized axis ⇒ identical (up to sign) regardless of |axis_dir|.
    assert!(norm(cross(n_u, n_b)) < TAU_MODEL, "normals not parallel");
    assert!(
        (norm(n_b) - 1.0).abs() < TAU_MODEL,
        "big-axis normal not normalized to unit"
    );
}

#[test]
fn attack7_nonunit_axis_dir_c2_matches_unit() {
    // Oblique plane ⇒ C2 ellipse. Axis +z, once unit, once ×5.
    let theta = 0.4;
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: tilted_z_normal(theta),
    };
    let cyl_unit = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let cyl_big = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0),
        radius: 1.0,
    };
    let cu = intersect(&plane, &cyl_unit).unwrap();
    let cb = intersect(&plane, &cyl_big).unwrap();
    let (ctr_u, n_u, m_u, ar_u, br_u) = match cu[0] {
        SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => (
            center.as_array(),
            normal.as_array(),
            major_axis.as_array(),
            major_radius,
            minor_radius,
        ),
        _ => panic!(),
    };
    let (ctr_b, n_b, m_b, ar_b, br_b) = match cb[0] {
        SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => (
            center.as_array(),
            normal.as_array(),
            major_axis.as_array(),
            major_radius,
            minor_radius,
        ),
        _ => panic!(),
    };
    assert!(norm(sub(ctr_u, ctr_b)) < TAU_MODEL, "centers differ");
    assert!((ar_u - ar_b).abs() < TAU_MODEL, "major radii differ");
    assert!((br_u - br_b).abs() < TAU_MODEL, "minor radii differ");
    assert!(norm(cross(n_u, n_b)) < TAU_MODEL, "normals not parallel");
    assert!(
        norm(cross(m_u, m_b)) < TAU_MODEL,
        "major_axes not parallel up to sign"
    );
}
