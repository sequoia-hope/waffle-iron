//! PR-SSI1 — Adversarial audit of the exact-SSI foundation.
//!
//! These tests attack the three PR-SSI1 solvers (`plane_plane`,
//! `plane_sphere`, `sphere_sphere`) via the public `intersect` dispatcher.
//! They do NOT touch production code; they probe near-tolerance
//! classification, extreme scale, oblique normals, the in-plane basis, and
//! symmetry/sign conventions.
//!
//! Spec: specs/ssi_pr_ssi1_foundation.md
//! DoD §1.5 (adversarial validation): near-tolerance, degenerate, no NaN.
//!
//! Each test documents which attack from the audit brief it implements and
//! what invariant would break if it failed.

use cad_primitives::{Point3, Vector3, MIN_FEATURE_SIZE, TAU_MODEL};
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

/// Assert every field of a returned curve is finite (no NaN/Inf). This is the
/// core anti-`√(negative)` / anti-`0/0` guard.
fn assert_curve_finite(c: &SsiCurve) {
    match c {
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
    }
}

/// Relative on-surface check: residual / scale < rel_tol. Used at large scale
/// where absolute TAU_MODEL is below f64 representation error.
fn implicit_residual(surf: &QuadricSurface, x: [f64; 3]) -> f64 {
    match surf {
        QuadricSurface::Plane { point, normal } => {
            dot(normal.as_array(), sub(x, point.as_array())).abs()
        }
        QuadricSurface::Sphere { center, radius } => {
            (norm(sub(x, center.as_array())) - radius).abs()
        }
    }
}

// ---------------------------------------------------------------------------
// Attack 1: near-tangent boundary classification (the classic SSI trap).
// The danger: a `d` just inside the tangent band feeding √(negative) → NaN,
// or a misclassification producing a degenerate (radius ≤ 0) circle.
// ---------------------------------------------------------------------------

#[test]
fn attack1_plane_sphere_near_tangent_sweep_no_nan() {
    // Sphere r=1 at origin; plane z = d (normal +z, point (0,0,d)).
    // Sweep d = r·(1 − ε) for shrinking ε, straddling the tangent band.
    let r = 1.0;
    let epsilons = [
        1e-2, 1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8, 1e-9, 1e-12, 1e-15, 0.0, -1e-9,
    ];
    for &eps in &epsilons {
        let d = r * (1.0 - eps); // d <= r for eps >= 0; d > r for eps < 0
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, d),
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        let sphere = QuadricSurface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: r,
        };
        let curves = intersect(&plane, &sphere)
            .unwrap_or_else(|e| panic!("eps={eps}: must not error, got {e:?}"));
        // Whatever the classification, ANY returned circle must be finite +
        // positive radius. Specifically radius² = r² − d² must be > 0.
        for c in &curves {
            assert_curve_finite(c);
            if let SsiCurve::Circle { radius, .. } = c {
                let expect_rsq = r * r - d * d;
                assert!(
                    expect_rsq > 0.0,
                    "eps={eps}: solver returned a circle but r²−d²={expect_rsq} ≤ 0"
                );
                assert!(
                    (radius * radius - expect_rsq).abs() < 1e-9,
                    "eps={eps}: radius²={} != r²−d²={expect_rsq}",
                    radius * radius
                );
            }
        }
        // Classification: for eps <= 0 (d >= r) there must be NO curve.
        if eps <= 0.0 {
            assert!(
                curves.is_empty(),
                "eps={eps} (d={d} >= r={r}): expected empty (tangent/disjoint), got {curves:?}"
            );
        }
    }
}

#[test]
fn attack1_sphere_sphere_external_tangent_sweep_no_nan() {
    // Two unit spheres; b at distance D = (r_a+r_b)·(1−ε) = 2·(1−ε) on +x.
    let (ra, rb) = (1.0, 1.0);
    let sum = ra + rb;
    let epsilons = [1e-2, 1e-4, 1e-6, 1e-7, 1e-8, 1e-9, 1e-12, 0.0, -1e-9];
    for &eps in &epsilons {
        let dd = sum * (1.0 - eps); // distance between centers
        let a = QuadricSurface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: ra,
        };
        let b = QuadricSurface::Sphere {
            center: Point3::new(dd, 0.0, 0.0),
            radius: rb,
        };
        let curves =
            intersect(&a, &b).unwrap_or_else(|e| panic!("eps={eps}: must not error, got {e:?}"));
        for c in &curves {
            assert_curve_finite(c);
        }
        if eps <= 0.0 {
            assert!(
                curves.is_empty(),
                "eps={eps} (D={dd} >= sum={sum}): expected empty, got {curves:?}"
            );
        }
    }
}

#[test]
fn attack1_sphere_sphere_internal_tangent_sweep_no_nan() {
    // r_a=2, r_b=1; |r_a−r_b|=1. b at distance D = 1·(1+ε) on +x.
    // D just above |r_a−r_b| is transverse; at/below is contained.
    let (ra, rb) = (2.0_f64, 1.0_f64);
    let diff = (ra - rb).abs();
    let epsilons = [1e-2, 1e-4, 1e-6, 1e-7, 1e-8, 1e-9, 1e-12, 0.0, -1e-9];
    for &eps in &epsilons {
        let dd = diff * (1.0 + eps);
        let a = QuadricSurface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: ra,
        };
        let b = QuadricSurface::Sphere {
            center: Point3::new(dd, 0.0, 0.0),
            radius: rb,
        };
        let curves =
            intersect(&a, &b).unwrap_or_else(|e| panic!("eps={eps}: must not error, got {e:?}"));
        for c in &curves {
            assert_curve_finite(c);
        }
        if eps <= 0.0 {
            assert!(
                curves.is_empty(),
                "eps={eps} (D={dd} <= |ra−rb|={diff}): expected empty (contained), got {curves:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Attack 2: extreme scale.
//
// Large geometry (radius ~1e6): the solver's ANALYTICAL center/radius must be
// correct to RELATIVE tolerance. The absolute-TAU_MODEL on-surface oracle is
// expected to be too strict at this scale (f64 rep error >> 1e-7), so this
// test checks center/radius directly with a relative tolerance and documents
// at what scale the absolute oracle still holds.
// ---------------------------------------------------------------------------

#[test]
fn attack2_large_scale_plane_sphere_analytical_correct() {
    // Sphere r=1e6 centered at (1e6, 1e6, 1e6). Plane z = 1e6 + 0.5e6.
    let r = 1.0e6;
    let cz = 1.0e6;
    let d_set = 0.5e6; // signed distance center→plane along +z
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, cz + d_set),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(1.0e6, 1.0e6, cz),
        radius: r,
    };
    let curves = intersect(&plane, &sphere).expect("large plane/sphere must solve");
    assert_eq!(curves.len(), 1, "expected one circle, got {curves:?}");
    assert_curve_finite(&curves[0]);
    let SsiCurve::Circle {
        center,
        normal,
        radius,
    } = curves[0]
    else {
        panic!("expected circle");
    };
    // Analytical facts, RELATIVE tolerance (1e-12 relative ~ f64 eps headroom).
    let expect_radius = (r * r - d_set * d_set).sqrt(); // √(1e12 − 0.25e12)
    let rel = (radius - expect_radius).abs() / expect_radius;
    assert!(
        rel < 1e-12,
        "radius rel error {rel} (got {radius}, want {expect_radius})"
    );
    // center = foot of perpendicular = (1e6, 1e6, cz + d_set).
    let cexp = [1.0e6, 1.0e6, cz + d_set];
    let crel = norm(sub(center.as_array(), cexp)) / norm(cexp);
    assert!(
        crel < 1e-12,
        "center rel error {crel} (got {:?}, want {cexp:?})",
        center.as_array()
    );
    // normal ∥ +z.
    assert!(norm(cross(normal.as_array(), [0.0, 0.0, 1.0])) < 1e-12);
}

#[test]
fn attack2_large_scale_on_surface_relative_oracle() {
    // CHARACTERIZATION of the absolute-vs-relative oracle question.
    //
    // Empirically the absolute TAU_MODEL (1e-7) on-surface oracle HOLDS for
    // this plane∩sphere geometry up to scale ~1e8 (residual ~1.5e-8) and
    // BREAKS at scale ~1e9 (residual ~2.4e-7 > 1e-7). The SOLVER stays correct
    // at all scales: the RELATIVE residual (abs/scale) is ~1e-12 throughout.
    //
    // So the finding is: the solver is scale-robust; only the absolute-
    // tolerance ORACLE degrades, and only above ~1e8. This locks both halves.
    for &scale in &[1.0e6_f64, 1.0e9_f64] {
        let r = scale;
        let cz = scale;
        let d_set = 0.5 * scale;
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, cz + d_set),
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        let sphere = QuadricSurface::Sphere {
            center: Point3::new(scale, scale, cz),
            radius: r,
        };
        let curves = intersect(&plane, &sphere).unwrap();
        let curve = &curves[0];
        let mut max_abs_plane = 0.0_f64;
        let mut max_abs_sphere = 0.0_f64;
        for i in 0..256 {
            let t = (i as f64) / 256.0 * std::f64::consts::TAU;
            let p = curve.eval(t).as_array();
            max_abs_plane = max_abs_plane.max(implicit_residual(&plane, p));
            max_abs_sphere = max_abs_sphere.max(implicit_residual(&sphere, p));
        }
        // SOLVER correctness (relative): holds at every scale.
        assert!(
            max_abs_plane / scale < 1e-9 && max_abs_sphere / scale < 1e-9,
            "scale {scale:e}: relative on-surface failed: plane {} sphere {}",
            max_abs_plane / scale,
            max_abs_sphere / scale
        );
        // ORACLE behavior (absolute): holds at 1e6, breaks at 1e9.
        if scale <= 1.0e6 {
            assert!(
                max_abs_sphere < TAU_MODEL && max_abs_plane < TAU_MODEL,
                "scale {scale:e}: absolute oracle unexpectedly broke (plane \
                 {max_abs_plane}, sphere {max_abs_sphere}) — update the range"
            );
        } else {
            // scale 1e9: the absolute oracle is EXPECTED to exceed TAU_MODEL.
            assert!(
                max_abs_sphere >= TAU_MODEL,
                "scale {scale:e}: absolute oracle unexpectedly held (sphere \
                 {max_abs_sphere}); the absolute-tolerance breakpoint moved — \
                 re-characterize the scale range"
            );
        }
    }
}

#[test]
fn attack2_tiny_scale_plane_sphere() {
    // Tiny geometry near MIN_FEATURE_SIZE (1e-6). Sphere r=1e-5 at origin,
    // plane z = 0.5e-5. d = 0.5e-5, radius = √(1e-10 − 0.25e-10).
    let r = 1.0e-5;
    let d_set = 0.5e-5;
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, d_set),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: r,
    };
    let curves = intersect(&plane, &sphere).expect("tiny plane/sphere must solve");
    assert_eq!(curves.len(), 1, "tiny: expected one circle, got {curves:?}");
    assert_curve_finite(&curves[0]);
    let SsiCurve::Circle { radius, .. } = curves[0] else {
        panic!("expected circle");
    };
    let expect = (r * r - d_set * d_set).sqrt();
    let rel = (radius - expect).abs() / expect;
    assert!(rel < 1e-9, "tiny radius rel error {rel}");
    // Reference MIN_FEATURE_SIZE so the import is load-bearing.
    assert!(r > MIN_FEATURE_SIZE / 1000.0);
}

#[test]
fn attack2_tiny_scale_sphere_sphere_absolute_tau_floor() {
    // At very small scale the ABSOLUTE TAU_MODEL band (1e-7) can swallow the
    // whole geometry. r_a=r_b=1e-4, centers 1e-4 apart → transverse
    // analytically, but the tangent band guards (sum−TAU, diff+TAU) use
    // absolute 1e-7. Here sum=2e-4, diff=0, D=1e-4: comfortably inside the
    // band → must yield a real circle. Probes that the absolute floor does
    // not spuriously empty a well-separated small intersection.
    let r = 1.0e-4;
    let a = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: r,
    };
    let b = QuadricSurface::Sphere {
        center: Point3::new(1.0e-4, 0.0, 0.0),
        radius: r,
    };
    let curves = intersect(&a, &b).expect("small sphere/sphere must solve");
    assert_eq!(
        curves.len(),
        1,
        "small transverse spheres: expected a circle, got {curves:?}"
    );
    assert_curve_finite(&curves[0]);
}

// ---------------------------------------------------------------------------
// Attack 3: oblique / non-axis-aligned normals.
// ---------------------------------------------------------------------------

#[test]
fn attack3_oblique_plane_plane_line_on_both() {
    // Two oblique (non-axis) unit normals.
    let na = {
        let v = [0.3, 0.7, 0.64807]; // ~unit-ish, normalize in solver
        v
    };
    let nb = [0.8, 0.1, 0.59];
    let pa = QuadricSurface::Plane {
        point: Point3::new(1.0, -2.0, 0.5),
        normal: Vector3::from(na),
    };
    let pb = QuadricSurface::Plane {
        point: Point3::new(-0.5, 3.0, 2.0),
        normal: Vector3::from(nb),
    };
    let curves = intersect(&pa, &pb).expect("oblique transverse planes → line");
    assert_eq!(curves.len(), 1);
    assert_curve_finite(&curves[0]);
    let SsiCurve::Line { .. } = curves[0] else {
        panic!("expected line");
    };
    // On-surface oracle: sample line over [-5,5], check residual < TAU_MODEL on
    // BOTH planes (normals here are O(1), scale is O(1) → absolute tol valid).
    for i in 0..64 {
        let t = -5.0 + (i as f64) / 63.0 * 10.0;
        let p = curves[0].eval(t).as_array();
        assert!(
            implicit_residual(&pa, p) < TAU_MODEL,
            "t={t}: off plane A residual {}",
            implicit_residual(&pa, p)
        );
        assert!(
            implicit_residual(&pb, p) < TAU_MODEL,
            "t={t}: off plane B residual {}",
            implicit_residual(&pb, p)
        );
    }
}

#[test]
fn attack3_oblique_plane_sphere_circle_on_both() {
    // Oblique plane cutting a unit sphere through a chord.
    let n = [0.4, 0.5, 0.7672]; // normalized in solver
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.2, -0.1, 0.05),
        normal: Vector3::from(n),
    };
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.1, 0.2, -0.3),
        radius: 1.5,
    };
    let curves = intersect(&plane, &sphere).expect("oblique plane/sphere → circle");
    assert_eq!(curves.len(), 1);
    assert_curve_finite(&curves[0]);
    for i in 0..64 {
        let t = (i as f64) / 64.0 * std::f64::consts::TAU;
        let p = curves[0].eval(t).as_array();
        assert!(
            implicit_residual(&plane, p) < TAU_MODEL,
            "t={t}: off plane residual {}",
            implicit_residual(&plane, p)
        );
        assert!(
            implicit_residual(&sphere, p) < TAU_MODEL,
            "t={t}: off sphere residual {}",
            implicit_residual(&sphere, p)
        );
    }
}

// ---------------------------------------------------------------------------
// Attack 4: in_plane_basis edge cases (orthonormality + determinism).
// ---------------------------------------------------------------------------

#[test]
fn attack4_circle_axis_aligned_normals_orthonormal_frame() {
    // For each world axis (and negatives) as the circle normal, sample the
    // circle at t=0 and t=π/2; the two points must be equidistant from center
    // (= radius) and the chord between them must be √2·radius — i.e. the
    // in-plane basis (u,v) is orthonormal, not skewed.
    let axes = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        // near-axis (component just off zero) to probe the tie-break path
        [1e-9, 0.0, 1.0],
        [1.0, 1e-9, 1e-9],
    ];
    for axis in axes {
        // Build a circle with this normal via plane∩sphere so we use the real
        // public path. Plane through sphere center → great circle, center on
        // sphere center, radius = sphere radius.
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::from(axis),
        };
        let sphere = QuadricSurface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 2.0,
        };
        let curves = intersect(&plane, &sphere).unwrap();
        let c = &curves[0];
        let (center, radius) = match c {
            SsiCurve::Circle { center, radius, .. } => (center.as_array(), *radius),
            _ => panic!("expected circle for axis {axis:?}"),
        };
        let p0 = c.eval(0.0).as_array();
        let p1 = c.eval(std::f64::consts::FRAC_PI_2).as_array();
        let r0 = norm(sub(p0, center));
        let r1 = norm(sub(p1, center));
        assert!((r0 - radius).abs() < 1e-12, "axis {axis:?}: |p0−c| != r");
        assert!((r1 - radius).abs() < 1e-12, "axis {axis:?}: |p1−c| != r");
        let chord = norm(sub(p0, p1));
        let expect_chord = std::f64::consts::SQRT_2 * radius;
        assert!(
            (chord - expect_chord).abs() < 1e-12,
            "axis {axis:?}: chord {chord} != √2·r {expect_chord} (basis not orthonormal)"
        );
    }
}

#[test]
fn attack4_eval_determinism_same_t_identical_point() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.3, -0.2, 0.1),
        normal: Vector3::new(0.2, 0.9, 0.387),
    };
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let c1 = intersect(&plane, &sphere).unwrap()[0];
    let c2 = intersect(&plane, &sphere).unwrap()[0];
    for i in 0..16 {
        let t = (i as f64) / 16.0 * std::f64::consts::TAU;
        let a = c1.eval(t).as_array();
        let b = c2.eval(t).as_array();
        // Byte-identical (I5): not just approx-equal.
        assert_eq!(a, b, "eval not deterministic at t={t}");
    }
}

// ---------------------------------------------------------------------------
// Attack 5: symmetry + sign conventions under oblique / degenerate-ish input.
// ---------------------------------------------------------------------------

#[test]
fn attack5_plane_sphere_symmetry_oblique() {
    // intersect(sphere, plane) must equal intersect(plane, sphere): same circle.
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.1, 0.2, 0.3),
        normal: Vector3::new(0.5, 0.5, 0.71),
    };
    let sphere = QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let ps = intersect(&plane, &sphere).unwrap();
    let sp = intersect(&sphere, &plane).unwrap();
    assert_eq!(ps.len(), 1);
    assert_eq!(sp.len(), 1);
    let (c0, n0, r0) = match ps[0] {
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => (center.as_array(), normal.as_array(), radius),
        _ => panic!(),
    };
    let (c1, n1, r1) = match sp[0] {
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => (center.as_array(), normal.as_array(), radius),
        _ => panic!(),
    };
    assert!(
        norm(sub(c0, c1)) < TAU_MODEL,
        "centers differ: {c0:?} vs {c1:?}"
    );
    assert!((r0 - r1).abs() < TAU_MODEL, "radii differ: {r0} vs {r1}");
    // normal up to sign.
    assert!(norm(cross(n0, n1)) < TAU_MODEL, "normals not parallel");
}

#[test]
fn attack5_plane_plane_symmetry_oblique_unoriented() {
    let pa = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(0.6, 0.8, 0.0),
    };
    let pb = QuadricSurface::Plane {
        point: Point3::new(0.0, 1.0, 0.0),
        normal: Vector3::new(0.0, 0.6, 0.8),
    };
    let ab = intersect(&pa, &pb).unwrap();
    let ba = intersect(&pb, &pa).unwrap();
    let (pt_ab, dir_ab) = match ab[0] {
        SsiCurve::Line { point, dir } => (point.as_array(), dir.as_array()),
        _ => panic!(),
    };
    let (pt_ba, dir_ba) = match ba[0] {
        SsiCurve::Line { point, dir } => (point.as_array(), dir.as_array()),
        _ => panic!(),
    };
    // dir parallel up to sign.
    assert!(norm(cross(dir_ab, dir_ba)) < TAU_MODEL, "dirs not parallel");
    // Both points lie on the same line: their difference is parallel to dir.
    let delta = sub(pt_ab, pt_ba);
    if norm(delta) > TAU_MODEL {
        assert!(
            norm(cross(delta, dir_ab)) < TAU_MODEL * norm(delta).max(1.0),
            "points not collinear along shared dir: delta {delta:?}"
        );
    }
    // Both points must be on BOTH planes.
    for pt in [pt_ab, pt_ba] {
        assert!(implicit_residual(&pa, pt) < TAU_MODEL);
        assert!(implicit_residual(&pb, pt) < TAU_MODEL);
    }
}
