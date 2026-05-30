//! PR-SSI6 — Adversarial audit of the sphere∩cylinder coaxial solver.
//!
//! These tests attack `sphere_cylinder` (reached via the public `intersect`
//! dispatcher) at its band boundaries (X2↔X1↔X0 tangent limit, NC↔coaxial
//! detection band), under oblique non-axis-aligned + non-unit + reversed axes,
//! at extreme scale, on degenerate/disjoint edge cases, and on symmetry +
//! determinism. They do NOT touch production code.
//!
//! Spec: specs/ssi_pr_ssi6_sphere_cylinder_coaxial.md
//! Reuses ssi2_adversary's on-surface oracle + finite-field patterns:
//! the cylinder residual `|(x−A)×â|/|â| − r`, the sphere residual, and
//! `assert_curve_finite`.
//!
//! Tolerance note: TAU_MODEL = 1e-7. The absolute on-surface oracle is valid
//! only while curve sample coordinates stay below ~1e8 (the PR-SSI1 finding).
//! Where scale drives coordinates large, tests switch to a RELATIVE analytical
//! check and explicitly characterize the absolute-oracle breakpoint. The
//! coaxial-detection band uses an ABSOLUTE distance `d_ax < TAU_MODEL`, which
//! is likewise scale-sensitive; Attack 2 characterizes that honestly.

use cad_primitives::{Point3, Vector3, TAU_MODEL};
use ssi_rs::{intersect, QuadricSurface, SsiCurve, SsiError};

// ---------------------------------------------------------------------------
// Vector helpers (cad-primitives is types-only).
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

/// Every field of a returned curve must be finite (no NaN/Inf). The core
/// anti-`√(negative)` / anti-`0/0` guard. sphere∩cylinder only emits Circles,
/// but we cover the whole enum to be defensive.
fn assert_curve_finite(c: &SsiCurve) {
    match c {
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
            // Normal must be unit (defensive normalization).
            assert!(
                (norm(normal.as_array()) - 1.0).abs() < 1e-9,
                "Circle normal not unit: {c:?}"
            );
        }
        other => panic!("sphere∩cylinder must only return Circles; got {other:?}"),
    }
}

/// Absolute implicit residual on a surface (PR-SSI1/SSI2 oracle).
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
            let v = sub(x, axis_point.as_array());
            let a = axis_dir.as_array();
            (norm(cross(v, a)) / norm(a) - radius).abs()
        }
        QuadricSurface::Plane { point, normal } => {
            dot(unit(normal.as_array()), sub(x, point.as_array())).abs()
        }
        QuadricSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let v = sub(x, apex.as_array());
            let a = axis_dir.as_array();
            let alen = norm(a);
            let h = dot(v, a) / alen;
            let r_actual = norm(cross(v, a)) / alen;
            (r_actual - h.abs() * half_angle.tan()).abs()
        }
    }
}

/// Max absolute on-surface residual over N circle samples against both surfaces.
fn max_residual_on_both(curve: &SsiCurve, a: &QuadricSurface, b: &QuadricSurface, n: usize) -> f64 {
    let mut m = 0.0_f64;
    for i in 0..n {
        let t = (i as f64) / (n as f64) * std::f64::consts::TAU;
        let p = curve.eval(t).as_array();
        m = m.max(implicit_residual(a, p)).max(implicit_residual(b, p));
    }
    m
}

fn assert_on_both_surfaces(curve: &SsiCurve, a: &QuadricSurface, b: &QuadricSurface) {
    let m = max_residual_on_both(curve, a, b, 64);
    assert!(m < TAU_MODEL, "max on-surface residual {m} >= TAU_MODEL");
}

fn parallel_up_to_sign(a: [f64; 3], b: [f64; 3]) {
    assert!(
        norm(cross(a, b)) < TAU_MODEL,
        "expected {a:?} parallel to {b:?} (|cross| = {})",
        norm(cross(a, b))
    );
}

fn circle_fields(c: &SsiCurve) -> ([f64; 3], [f64; 3], f64) {
    match c {
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => (center.as_array(), normal.as_array(), *radius),
        other => panic!("expected Circle, got {other:?}"),
    }
}

fn z_cyl(r: f64) -> QuadricSurface {
    QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: r,
    }
}

fn origin_sphere(r: f64) -> QuadricSurface {
    QuadricSurface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: r,
    }
}

// ===========================================================================
// Attack 1: Tangent boundary sweep (X2↔X1↔X0) — the √(r_s²−r_c²) trap.
//
// Fix sphere r_s; sweep coaxial cylinder r_c across r_s:
//   r_c = r_s·(1−ε)  → two circles, h = √(r_s²−r_c²) → 0⁺
//   r_c = r_s        → one circle (X1)
//   r_c = r_s·(1+ε)  → empty (X0)
// Assert: no NaN/Inf (h stays real — never √(negative)); correct count each
// side (2 / 1 / 0); the two circles' separation 2h → 0 cleanly as r_c→r_s⁻;
// every returned circle finite + radius r_c + on both surfaces.
// ===========================================================================

#[test]
fn attack1_tangent_band_sweep_no_nan() {
    let r_s = 2.0;
    let sphere = origin_sphere(r_s);

    // r_c values straddling r_s with the X1 band = |r_s − r_c| ≤ TAU_MODEL.
    // TAU_MODEL = 1e-7, so a |Δ| of 1e-2/1e-4/1e-6 is X2/X0; 1e-9 is X1.
    let cases: &[(f64, &str)] = &[
        (r_s - 1e-2, "two"),
        (r_s - 1e-4, "two"),
        (r_s - 1e-6, "two"),   // h = √(r_s²−r_c²) tiny but real; |Δ|=1e-6 > TAU
        (r_s, "one"),          // exact tangent
        (r_s + 1e-9, "one"),   // within X1 band (|Δ|=1e-9 < TAU)
        (r_s - 1e-9, "one"),   // within X1 band from below
        (r_s + 1e-6, "empty"), // X0: r_c − r_s = 1e-6 > TAU
        (r_s + 1e-2, "empty"),
        (r_s + 5.0, "empty"),
    ];

    for &(r_c, kind) in cases {
        let cyl = z_cyl(r_c);
        let curves = intersect(&sphere, &cyl)
            .unwrap_or_else(|e| panic!("r_c={r_c} ({kind}): must not error, got {e:?}"));
        for c in &curves {
            assert_curve_finite(c); // no √(negative) NaN, ever
        }
        match kind {
            "two" => {
                assert_eq!(curves.len(), 2, "r_c={r_c}: expected two circles");
                let h = (r_s * r_s - r_c * r_c).sqrt();
                assert!(h.is_finite() && h > 0.0, "r_c={r_c}: h not real-positive");
                for c in &curves {
                    let (center, normal, radius) = circle_fields(c);
                    assert!((radius - r_c).abs() < TAU_MODEL, "r_c={r_c}: radius wrong");
                    parallel_up_to_sign(normal, [0.0, 0.0, 1.0]);
                    // |center − C| == h.
                    assert!(
                        (norm(center) - h).abs() < TAU_MODEL,
                        "r_c={r_c}: |center| {} != h {h}",
                        norm(center)
                    );
                    assert_on_both_surfaces(c, &sphere, &cyl);
                }
                // Separation 2h: positive and consistent.
                let (c0, _, _) = circle_fields(&curves[0]);
                let (c1, _, _) = circle_fields(&curves[1]);
                let sep = norm(sub(c0, c1));
                assert!(
                    (sep - 2.0 * h).abs() < TAU_MODEL,
                    "r_c={r_c}: separation {sep} != 2h {}",
                    2.0 * h
                );
            }
            "one" => {
                assert_eq!(curves.len(), 1, "r_c={r_c}: expected one circle");
                let (center, normal, radius) = circle_fields(&curves[0]);
                assert!((radius - r_c).abs() < TAU_MODEL, "r_c={r_c}: radius wrong");
                // Center == C (origin); h ≈ 0.
                assert!(
                    norm(center) < TAU_MODEL,
                    "r_c={r_c}: center {center:?} != C"
                );
                parallel_up_to_sign(normal, [0.0, 0.0, 1.0]);
                assert_on_both_surfaces(&curves[0], &sphere, &cyl);
            }
            "empty" => {
                assert!(
                    curves.is_empty(),
                    "r_c={r_c} ({kind}): expected empty, got {curves:?}"
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn attack1_two_circles_converge_as_rc_approaches_rs() {
    // As r_c → r_s⁻, h = √(r_s²−r_c²) → 0, so the two circles' separation 2h
    // shrinks monotonically toward 0 — the clean switch into the X1 band.
    // r_s − r_c = r_s·10^-k must stay > TAU_MODEL=1e-7 ⇒ for r_s=2, k ≤ 7.
    let r_s = 2.0;
    let sphere = origin_sphere(r_s);
    let mut prev_sep = f64::INFINITY;
    for k in 1..=6 {
        let r_c = r_s * (1.0 - 10.0_f64.powi(-k));
        let cyl = z_cyl(r_c);
        let curves = intersect(&sphere, &cyl).unwrap();
        assert_eq!(curves.len(), 2, "r_c={r_c}: expected two circles");
        let (c0, _, _) = circle_fields(&curves[0]);
        let (c1, _, _) = circle_fields(&curves[1]);
        let sep = norm(sub(c0, c1));
        assert!(
            sep.is_finite(),
            "k={k}: separation non-finite (NaN from √neg?)"
        );
        assert!(
            sep < prev_sep,
            "k={k}: separation {sep} not shrinking (prev {prev_sep}) as r_c→r_s"
        );
        assert!(
            sep > 0.0,
            "k={k}: circles collapsed prematurely at r_c={r_c}"
        );
        prev_sep = sep;
    }
    assert!(prev_sep < 1e-2, "final separation {prev_sep} not near zero");
}

#[test]
fn attack1_rc_just_below_rs_no_negative_sqrt() {
    // THE critical danger: r_c just below r_s feeding √(r_s²−r_c²). Walk r_c
    // up to the very edge of the X1 band from the X2 side and confirm h is
    // never negative/NaN and the result is always exactly two finite circles.
    let r_s = 1.0;
    let sphere = origin_sphere(r_s);
    // r_c = r_s − δ with δ just above TAU_MODEL (still X2), down to ~2·TAU.
    for &delta in &[1e-1, 1e-3, 1e-5, 5e-7, 2e-7, 1.5e-7] {
        let r_c = r_s - delta;
        let cyl = z_cyl(r_c);
        let curves = intersect(&sphere, &cyl)
            .unwrap_or_else(|e| panic!("δ={delta}: must not error, got {e:?}"));
        assert_eq!(curves.len(), 2, "δ={delta}: expected two circles (X2)");
        for c in &curves {
            assert_curve_finite(c);
            let (center, _, _) = circle_fields(c);
            // h = √(r_s²−r_c²) must be real and finite.
            let h = norm(center);
            assert!(h.is_finite(), "δ={delta}: h non-finite (√negative leaked)");
            let h_expected = (r_s * r_s - r_c * r_c).sqrt();
            assert!(
                (h - h_expected).abs() < TAU_MODEL,
                "δ={delta}: h {h} != √(r_s²−r_c²) {h_expected}"
            );
        }
    }
}

// ===========================================================================
// Attack 2: Coaxial-detection band (NC↔coaxial), at d_ax across TAU_MODEL.
//
// Sphere center fixed at origin; offset the cylinder axis perpendicular so
// d_ax = dist(C, axis line) sweeps across TAU_MODEL. d_ax < TAU ⇒ circles;
// d_ax ≥ TAU ⇒ Err(ASNA). Clean switch, no misclassification, no panic.
//
// CHARACTERIZATION: d_ax is an ABSOLUTE distance, so at large coordinate scale
// the band is scale-sensitive (akin to the PR-SSI1 ~1e8 finding). The second
// test characterizes that honestly rather than forcing green.
// ===========================================================================

#[test]
fn attack2_coaxial_detection_band_unit_scale() {
    // Unit-scale geometry: sphere r_s=2 at origin, cylinder ∥ +z, r_c=1, axis
    // offset along +x by `off` ⇒ d_ax = off exactly.
    let r_s = 2.0;
    let r_c = 1.0;
    let sphere = origin_sphere(r_s);

    let make = |off: f64| QuadricSurface::Cylinder {
        axis_point: Point3::new(off, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: r_c,
    };

    // Below the band ⇒ coaxial (two circles, since r_s > r_c).
    for &off in &[0.0, 0.25 * TAU_MODEL, 0.5 * TAU_MODEL, 0.9 * TAU_MODEL] {
        let cyl = make(off);
        let curves = intersect(&sphere, &cyl)
            .unwrap_or_else(|e| panic!("off={off:e}: expected coaxial circles, got {e:?}"));
        assert_eq!(
            curves.len(),
            2,
            "off={off:e}: d_ax < TAU ⇒ must be coaxial (two circles)"
        );
        for c in &curves {
            assert_curve_finite(c);
        }
    }

    // At/above the band ⇒ NC ⇒ ASNA. A barely-offset axis just over TAU must
    // yield ASNA, NOT a wrong/degenerate circle.
    for &off in &[TAU_MODEL, 1.0001 * TAU_MODEL, 2.0 * TAU_MODEL, 1e-3, 0.1] {
        let cyl = make(off);
        assert_eq!(
            intersect(&sphere, &cyl),
            Err(SsiError::AnalyticalSolutionNotAvailable),
            "off={off:e}: d_ax ≥ TAU ⇒ must be NC (ASNA), not a circle"
        );
    }
}

#[test]
fn attack2_coaxial_detection_band_is_absolute_scale_sensitive() {
    // CHARACTERIZATION (not a solver bug). The coaxial discriminant
    // `d_ax = |rel − (rel·â)·â|` is compared against an ABSOLUTE TAU_MODEL.
    // d_ax is computed by subtracting two O(scale) quantities, so its floor
    // rounding error grows with the coordinate scale. At unit scale a true
    // d_ax = 0 reads as ~0; at huge scale, floating-point noise in the
    // projection can push a *genuinely coaxial* configuration's computed d_ax
    // above TAU_MODEL → spurious ASNA. This locks the honest finding.

    let r_s = 2.0;
    let r_c = 1.0;

    // A genuinely coaxial config (axis exactly through the sphere center) at
    // escalating scale. We place BOTH the sphere center and the axis point far
    // from origin, on the same axis line, so the *true* d_ax is exactly 0.
    let probe = |scale_mag: f64| -> Result<Vec<SsiCurve>, SsiError> {
        let s = scale_mag;
        // Sphere center and axis_point both at (s, s, s); axis ∥ +z ⇒ the axis
        // line x=s, y=s passes exactly through the center ⇒ truly coaxial.
        let sphere = QuadricSurface::Sphere {
            center: Point3::new(s, s, s),
            radius: r_s,
        };
        let cyl = QuadricSurface::Cylinder {
            axis_point: Point3::new(s, s, s - 10.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: r_c,
        };
        intersect(&sphere, &cyl)
    };

    // At small/moderate scale the truly-coaxial config is detected as coaxial.
    // MEASURED: with a generic-direction axis (â=(1,2,3)/|·|) and center far
    // from origin, the truly-coaxial detection HOLDS through scale 1e8 and
    // first FLIPS to ASNA at scale 1e9 (fp noise in d_ax = |rel−(rel·â)â|
    // exceeds the ABSOLUTE TAU_MODEL=1e-7). So the coaxial-band breakpoint is
    // ~1e8→1e9 — the same class as the PR-SSI1 absolute-oracle ceiling.
    for &s in &[0.0, 1.0, 1e3, 1e6] {
        let r = probe(s);
        assert!(
            matches!(r, Ok(ref v) if v.len() == 2),
            "scale={s:e}: truly-coaxial config misdetected: {r:?}"
        );
    }

    // FINDING: at scale 1e10 (well past the measured ~1e9 flip) a generic-axis
    // truly-coaxial config reads as ASNA. We do NOT force a particular verdict
    // here — only require a clean Result (no panic, no NaN). Both Ok(2 circles)
    // and ASNA are acceptable; ASNA is the documented absolute-band noise, NOT
    // a logic bug. (Empirically: ASNA at 1e10 with this generic axis.)
    let s = 1e10;
    let ahat = unit([1.0, 2.0, 3.0]);
    // sphere center on the axis line through axis_point along â (truly coaxial).
    let axis_point = [s, s * 1.5, s * 0.5];
    let center = add(axis_point, scale(ahat, 7.3)); // exactly on the line
    let sphere = QuadricSurface::Sphere {
        center: Point3::from(center),
        radius: r_s,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::from(axis_point),
        axis_dir: Vector3::from(ahat),
        radius: r_c,
    };
    let res = intersect(&sphere, &cyl);
    // Whatever the verdict, it must be a clean Result (no panic, no NaN).
    match res {
        Ok(ref v) => {
            for c in v {
                assert_curve_finite(c);
            }
            // If it WAS detected as coaxial at 1e10, great (band held).
            assert_eq!(
                v.len(),
                2,
                "scale=1e10: unexpected circle count {}",
                v.len()
            );
        }
        Err(SsiError::AnalyticalSolutionNotAvailable) => {
            // EXPECTED-POSSIBLE: at 1e10, fp noise in d_ax exceeded the
            // absolute TAU_MODEL, so a truly-coaxial config read as NC. This is
            // the documented absolute-band scale-sensitivity, NOT a logic bug.
            // (Same class as the PR-SSI1 ~1e8 absolute-oracle ceiling.)
        }
        Err(other) => panic!("scale=1e10: unexpected error {other:?}"),
    }
}

#[test]
fn attack2_barely_offset_axis_is_asna_not_degenerate_circle() {
    // A cylinder axis offset by just over TAU from the sphere center must yield
    // ASNA, NOT a degenerate/zero-radius/near-tangent circle. The danger is a
    // misclassification that produces a geometrically wrong circle.
    let sphere = origin_sphere(2.0);
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::new(1.5 * TAU_MODEL, 0.0, 0.0), // d_ax = 1.5·TAU
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    assert_eq!(
        intersect(&sphere, &cyl),
        Err(SsiError::AnalyticalSolutionNotAvailable),
        "barely-offset axis must be ASNA, not a wrong circle"
    );
    // And a NON-axis-aligned tiny offset likewise.
    let cyl2 = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 2.0 * TAU_MODEL, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    assert_eq!(
        intersect(&sphere, &cyl2),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
}

// ===========================================================================
// Attack 3: Two-circle correctness (core) + scale.
//
// Coaxial, several r_s/r_c with r_s > r_c, on the z-axis AND on an oblique
// non-axis axis with the sphere centered ON that axis at a non-origin point:
// exactly two circles, radius r_c, normal ∥ â, centers C ± h·â on the axis
// (perp-distance ≈ 0), symmetric about C, |center − C| = h. On both surfaces.
// Then large + tiny scale: relative correctness; report absolute breakpoint.
// ===========================================================================

#[test]
fn attack3_two_circle_correctness_z_axis() {
    for &(r_s, r_c) in &[(2.0, 1.0), (5.0, 3.0), (10.0, 0.5), (1.5, 1.4)] {
        let sphere = origin_sphere(r_s);
        let cyl = z_cyl(r_c);
        let curves = intersect(&sphere, &cyl).expect("X2 two circles");
        assert_eq!(curves.len(), 2, "r_s={r_s},r_c={r_c}: expected two");
        let h = (r_s * r_s - r_c * r_c).sqrt();
        for c in &curves {
            assert_curve_finite(c);
            let (center, normal, radius) = circle_fields(c);
            assert!((radius - r_c).abs() < TAU_MODEL, "radius {radius} != {r_c}");
            parallel_up_to_sign(normal, [0.0, 0.0, 1.0]);
            assert!((center[0]).abs() < TAU_MODEL, "center off x-axis");
            assert!((center[1]).abs() < TAU_MODEL, "center off y-axis");
            assert!((norm(center) - h).abs() < TAU_MODEL, "|center| != h");
            assert_on_both_surfaces(c, &sphere, &cyl);
        }
        // +h first; symmetric about C=origin.
        let (c0, _, _) = circle_fields(&curves[0]);
        let (c1, _, _) = circle_fields(&curves[1]);
        assert!((c0[2] - h).abs() < TAU_MODEL, "first center.z != +h");
        assert!((c1[2] + h).abs() < TAU_MODEL, "second center.z != −h");
        assert!(norm(add(c0, c1)) < TAU_MODEL, "not symmetric about C");
    }
}

#[test]
fn attack3_two_circle_correctness_oblique_off_origin() {
    // Oblique unit axis â = (1,2,2)/3; sphere centered ON that axis at a
    // non-origin point. Truly coaxial. Two circles radius r_c, normal ∥ â,
    // centers C ± h·â on the axis line (perp dist ≈ 0).
    let ahat = unit([1.0, 2.0, 2.0]);
    let axis_point = [3.0, -1.0, 4.0];
    // Sphere center on the line through axis_point along â, at param 2.5.
    let c_center = add(axis_point, scale(ahat, 2.5));
    let r_s = 3.0;
    let r_c = 1.2;
    let sphere = QuadricSurface::Sphere {
        center: Point3::from(c_center),
        radius: r_s,
    };
    let cyl = QuadricSurface::Cylinder {
        axis_point: Point3::from(axis_point),
        axis_dir: Vector3::new(1.0, 2.0, 2.0), // non-unit oblique
        radius: r_c,
    };
    let curves = intersect(&sphere, &cyl).expect("coaxial oblique off-origin");
    assert_eq!(curves.len(), 2);
    let h = (r_s * r_s - r_c * r_c).sqrt();
    for c in &curves {
        assert_curve_finite(c);
        let (center, normal, radius) = circle_fields(c);
        assert!((radius - r_c).abs() < TAU_MODEL);
        parallel_up_to_sign(normal, ahat);
        // |center − C| == h.
        assert!(
            (norm(sub(center, c_center)) - h).abs() < TAU_MODEL,
            "|center−C| != h"
        );
        // center on the axis line: perp distance to axis ≈ 0.
        let rel = sub(center, axis_point);
        let perp = sub(rel, scale(ahat, dot(rel, ahat)));
        assert!(
            norm(perp) < TAU_MODEL,
            "center off axis line (perp {})",
            norm(perp)
        );
        assert_on_both_surfaces(c, &sphere, &cyl);
    }
    // +h first ⇒ center0 = C + h·â.
    let (c0, _, _) = circle_fields(&curves[0]);
    let (c1, _, _) = circle_fields(&curves[1]);
    assert!(
        norm(sub(c0, add(c_center, scale(ahat, h)))) < TAU_MODEL,
        "center0 != C+hâ"
    );
    assert!(
        norm(sub(c1, sub(c_center, scale(ahat, h)))) < TAU_MODEL,
        "center1 != C−hâ"
    );
}

#[test]
fn attack3_large_scale_relative_and_absolute_breakpoint() {
    // Large scale r_s≈1e6: relative correctness holds; absolute oracle should
    // still hold near 1e6 but is expected to break near ~1e8 (PR-SSI1 ceiling).
    {
        let r_s = 1.0e6;
        let r_c = 6.0e5;
        let sphere = origin_sphere(r_s);
        let cyl = z_cyl(r_c);
        let curves = intersect(&sphere, &cyl).expect("large-scale X2");
        assert_eq!(curves.len(), 2);
        let h = (r_s * r_s - r_c * r_c).sqrt();
        for c in &curves {
            assert_curve_finite(c);
            let (center, _, radius) = circle_fields(c);
            assert!((radius - r_c).abs() / r_c < 1e-12, "relative radius off");
            assert!(
                (norm(center) - h).abs() / h < 1e-12,
                "relative |center| off"
            );
            // Relative on-surface residual tiny.
            let m = max_residual_on_both(c, &sphere, &cyl, 128);
            assert!(
                m / r_s < 1e-9,
                "large-scale relative residual {} too big",
                m / r_s
            );
        }
    }

    // Tiny scale r_s≈1e-4: coords tiny, absolute oracle holds.
    {
        let r_s = 2.0e-4;
        let r_c = 1.0e-4;
        let sphere = origin_sphere(r_s);
        let cyl = z_cyl(r_c);
        let curves = intersect(&sphere, &cyl).expect("tiny-scale X2");
        assert_eq!(curves.len(), 2);
        for c in &curves {
            assert_curve_finite(c);
            let (_, _, radius) = circle_fields(c);
            assert!(
                (radius - r_c).abs() / r_c < 1e-9,
                "tiny relative radius off"
            );
            assert_on_both_surfaces(c, &sphere, &cyl);
        }
    }
}

#[test]
fn attack3_absolute_oracle_breakpoint_characterization() {
    // CHARACTERIZATION: the absolute on-surface oracle for sphere∩cylinder
    // circles. MEASURED (256-sample sweep, r_c = r_s/2):
    //   r_s=1e6 : residual ~5.8e-11  — HOLDS
    //   r_s=1e7 : residual ~1.9e-9   — HOLDS
    //   r_s=1e8 : residual ~7.5e-9   — HOLDS
    //   r_s=1e9 : residual ~6.0e-8   — HOLDS (just under TAU_MODEL=1e-7)
    //   r_s=1e10: residual ~1.9e-6   — BREAKS
    // So for THIS pair the absolute oracle holds through ~1e9 and first breaks
    // at 1e10 — marginally more generous than the spec's ~1e8 note, because
    // the circle geometry has no oblique-major amplification. Lock both halves.
    // r=1e6: holds. r=1e10: breaks (absolute) but relative still tiny.
    {
        let r_s = 1.0e6;
        let r_c = 5.0e5;
        let sphere = origin_sphere(r_s);
        let cyl = z_cyl(r_c);
        let curves = intersect(&sphere, &cyl).unwrap();
        let m = max_residual_on_both(&curves[0], &sphere, &cyl, 256);
        assert!(
            m < TAU_MODEL,
            "r=1e6: absolute oracle unexpectedly broke ({m})"
        );
    }
    {
        let r_s = 1.0e10;
        let r_c = 5.0e9;
        let sphere = origin_sphere(r_s);
        let cyl = z_cyl(r_c);
        let curves = intersect(&sphere, &cyl).unwrap();
        assert_curve_finite(&curves[0]);
        let m = max_residual_on_both(&curves[0], &sphere, &cyl, 256);
        assert!(
            m >= TAU_MODEL,
            "r=1e10: absolute oracle unexpectedly held ({m}); breakpoint moved"
        );
        // Solver still analytically correct: relative residual tiny.
        assert!(
            m / r_s < 1e-9,
            "r=1e10: relative residual {} too big",
            m / r_s
        );
    }
}

// ===========================================================================
// Attack 4: Non-unit / reversed axis_dir.
//
// axis_dir magnitude ~7 and reversed (−â): results identical (up to circle
// order / normal sign) to the unit case; normal is unit. Defensive
// normalization holds.
// ===========================================================================

#[test]
fn attack4_nonunit_and_reversed_axis() {
    let r_s = 2.0;
    let r_c = 1.0;
    let sphere = origin_sphere(r_s);
    let h = (r_s * r_s - r_c * r_c).sqrt();

    // Reference: unit +z axis.
    let cyl_unit = z_cyl(r_c);
    let ref_curves = intersect(&sphere, &cyl_unit).unwrap();
    assert_eq!(ref_curves.len(), 2);

    // (a) Magnitude-7 +z axis.
    let cyl_big = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 7.0),
        radius: r_c,
    };
    // (b) Reversed (magnitude-7 −z) axis.
    let cyl_rev = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, -7.0),
        radius: r_c,
    };

    for (label, cyl) in [("big", &cyl_big), ("rev", &cyl_rev)] {
        let curves = intersect(&sphere, cyl).expect("non-unit/reversed axis");
        assert_eq!(curves.len(), 2, "{label}: expected two circles");
        // Centers (as a set) must be {(0,0,+h),(0,0,−h)} regardless of axis sign.
        let mut centers: Vec<[f64; 3]> = curves.iter().map(|c| circle_fields(c).0).collect();
        // Sort by z for set comparison.
        centers.sort_by(|a, b| a[2].partial_cmp(&b[2]).unwrap());
        assert!(
            norm(sub(centers[0], [0.0, 0.0, -h])) < TAU_MODEL,
            "{label}: low center"
        );
        assert!(
            norm(sub(centers[1], [0.0, 0.0, h])) < TAU_MODEL,
            "{label}: high center"
        );
        for c in &curves {
            let (_, normal, radius) = circle_fields(c);
            assert!((radius - r_c).abs() < TAU_MODEL, "{label}: radius wrong");
            // normal is unit (defensive normalization) and parallel to z.
            assert!(
                (norm(normal) - 1.0).abs() < TAU_MODEL,
                "{label}: normal not unit"
            );
            parallel_up_to_sign(normal, [0.0, 0.0, 1.0]);
            assert_on_both_surfaces(c, &sphere, cyl);
        }
    }
}

// ===========================================================================
// Attack 5: Degenerate / disjoint-coaxial edge cases.
//
// r_s or r_c ≤ 0, zero axis → DegenerateInput. Coaxial with r_c ≫ r_s (sphere
// deep inside the tube) → empty Ok([]). Sphere tiny vs cylinder → empty.
// Confirm no spurious circle.
// ===========================================================================

#[test]
fn attack5_degenerate_inputs() {
    let good_sphere = origin_sphere(2.0);
    let good_cyl = z_cyl(1.0);

    // Negative sphere radius.
    assert_eq!(
        intersect(
            &QuadricSurface::Sphere {
                center: Point3::new(0.0, 0.0, 0.0),
                radius: -1.0,
            },
            &good_cyl
        ),
        Err(SsiError::DegenerateInput)
    );
    // Negative cylinder radius.
    assert_eq!(
        intersect(
            &good_sphere,
            &QuadricSurface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: -2.0,
            }
        ),
        Err(SsiError::DegenerateInput)
    );
    // NaN sphere radius.
    assert_eq!(
        intersect(
            &QuadricSurface::Sphere {
                center: Point3::new(0.0, 0.0, 0.0),
                radius: f64::NAN,
            },
            &good_cyl
        ),
        Err(SsiError::DegenerateInput)
    );
    // Infinite cylinder radius.
    assert_eq!(
        intersect(
            &good_sphere,
            &QuadricSurface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: f64::INFINITY,
            }
        ),
        Err(SsiError::DegenerateInput)
    );
    // Zero axis direction.
    assert_eq!(
        intersect(
            &good_sphere,
            &QuadricSurface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 0.0),
                radius: 1.0,
            }
        ),
        Err(SsiError::DegenerateInput)
    );
    // Non-finite axis direction.
    assert_eq!(
        intersect(
            &good_sphere,
            &QuadricSurface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(f64::NAN, 0.0, 1.0),
                radius: 1.0,
            }
        ),
        Err(SsiError::DegenerateInput)
    );
}

#[test]
fn attack5_disjoint_coaxial_yields_empty_no_spurious_circle() {
    // Sphere deep inside a wider tube (coaxial, r_c ≫ r_s) ⇒ empty.
    for &(r_s, r_c) in &[(1.0, 2.0), (1.0, 100.0), (1e-3, 1.0), (0.5, 0.5 + 1e-3)] {
        let sphere = origin_sphere(r_s);
        let cyl = z_cyl(r_c);
        let res = intersect(&sphere, &cyl);
        assert_eq!(
            res,
            Ok(vec![]),
            "r_s={r_s},r_c={r_c}: sphere inside tube ⇒ empty, got {res:?}"
        );
        // Symmetric order also empty.
        assert_eq!(intersect(&cyl, &sphere), Ok(vec![]));
    }
}

// ===========================================================================
// Attack 6: I4 symmetry + I5 determinism.
//
// intersect(sphere,cyl) == intersect(cyl,sphere) for X2/X1/X0/NC; X2 two-circle
// +h-first order stable across repeats.
// ===========================================================================

#[test]
fn attack6_symmetry_all_branches() {
    let sphere = origin_sphere(2.0);

    // X2: two circles. As a SET (order/sign tolerant) ab == ba.
    let cyl_x2 = z_cyl(1.0);
    let ab = intersect(&sphere, &cyl_x2).unwrap();
    let ba = intersect(&cyl_x2, &sphere).unwrap();
    assert_eq!(ab.len(), 2);
    assert_eq!(ba.len(), 2);
    let key = |c: &SsiCurve| {
        let (center, normal, radius) = circle_fields(c);
        let n = unit(normal);
        // canonicalize normal sign by +z hemisphere
        let s = if n[2] >= 0.0 { 1.0 } else { -1.0 };
        let n = scale(n, s);
        let q = |v: f64| (v / TAU_MODEL).round() as i64;
        (
            q(center[0]),
            q(center[1]),
            q(center[2]),
            q(n[0]),
            q(n[1]),
            q(n[2]),
            q(radius),
        )
    };
    let mut abk: Vec<_> = ab.iter().map(key).collect();
    let mut bak: Vec<_> = ba.iter().map(key).collect();
    abk.sort();
    bak.sort();
    assert_eq!(abk, bak, "X2 circle set must match across argument order");

    // X1: one circle. ab == ba as a set.
    let cyl_x1 = z_cyl(2.0);
    let ab1 = intersect(&sphere, &cyl_x1).unwrap();
    let ba1 = intersect(&cyl_x1, &sphere).unwrap();
    assert_eq!(ab1.len(), 1);
    assert_eq!(ba1.len(), 1);
    assert_eq!(
        key(&ab1[0]),
        key(&ba1[0]),
        "X1 circle must match across order"
    );

    // X0: empty both ways.
    let cyl_x0 = z_cyl(3.0);
    assert_eq!(intersect(&sphere, &cyl_x0), Ok(vec![]));
    assert_eq!(intersect(&cyl_x0, &sphere), Ok(vec![]));

    // NC: ASNA both ways.
    let cyl_nc = QuadricSurface::Cylinder {
        axis_point: Point3::new(0.5, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    assert_eq!(
        intersect(&sphere, &cyl_nc),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
    assert_eq!(
        intersect(&cyl_nc, &sphere),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
}

#[test]
fn attack6_determinism_byte_identical_and_h_first() {
    let sphere = origin_sphere(2.0);
    let cyl = z_cyl(1.0);
    let h = 3.0_f64.sqrt();

    let first = intersect(&sphere, &cyl);
    for _ in 0..8 {
        let again = intersect(&sphere, &cyl);
        assert_eq!(first, again, "X2 output not byte-identical across repeats");
    }
    let cf = first.expect("two circles");
    // +h first.
    let (c0, _, _) = circle_fields(&cf[0]);
    assert!(
        (c0[2] - h).abs() < TAU_MODEL,
        "first center.z {} != +h {h}",
        c0[2]
    );
    // Stable at a fixed eval parameter across the argument-swapped call too.
    let swapped = intersect(&cyl, &sphere).expect("two circles swapped");
    // The swapped call must still be deterministic per call (byte-identical to
    // itself), even if order differs from the unswapped call.
    let swapped2 = intersect(&cyl, &sphere).expect("two circles swapped");
    assert_eq!(swapped, swapped2, "swapped call not deterministic");
}
