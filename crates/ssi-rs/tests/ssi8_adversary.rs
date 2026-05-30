//! PR-SSI8 — Adversarial audit of the cylinder∩cone coaxial solver.
//!
//! These tests attack `cylinder_cone` (reached via the public `intersect`
//! dispatcher) at its two coaxial-detection band edges — the parallelism gate
//! `|ĉ × â| < TAU_MODEL` and the on-axis gate `d_ax < TAU_MODEL` — at the α E1
//! limits (`tanα → 0` and `→ ∞`), at very small / very large `r_c`, under
//! reversed / antiparallel / non-unit axes, at extreme coordinate scale, and via
//! a deterministic many-config sweep enforcing the anti-hack invariant (every
//! valid coaxial input ⇒ EXACTLY two circles, never one or zero). They do NOT
//! touch production code.
//!
//! Spec: specs/ssi_pr_ssi8_cylinder_cone_coaxial.md (esp. the "Characterization
//! notes (for the adversary)" + the P9/P10 anti-hack note: there is deliberately
//! NO discriminant / √ / tangent / empty branch — coaxial cyl∩cone is ALWAYS two
//! circles for valid input).
//! Research basis: Patrikalakis & Maekawa, *Shape Interrogation for CAD/M*,
//! §5.8.3 (Case F8, implicit/implicit quadric pair). The coaxial reduction
//! `|h|·tanα = r_c` ⇒ `|h| = r_c·cotα` ⇒ two circles is classical.
//!
//! Mirrors ssi7_adversary's discipline: the cylinder + cone radial-residual
//! on-surface oracle, `assert_curve_finite` (all fields finite, normal unit,
//! radius > 0), RELATIVE residual at large scale, and explicit CHARACTERIZATION
//! of every absolute-tolerance ceiling rather than forcing green.
//!
//! Tolerance note: TAU_MODEL = 1e-7. The absolute on-surface oracle is valid only
//! while curve sample coordinates stay below the measured breakpoint. MEASURED
//! for this pair (256-sample sweep, apex origin +z, α = π/4, so circle coords ~
//! r_c):
//!   r_c=1e6 : maxres ~1.2e-10  — HOLDS
//!   r_c=1e7 : maxres ~1.9e-9   — HOLDS
//!   r_c=1e8 : maxres ~1.5e-8   — HOLDS (just under TAU_MODEL=1e-7)
//!   r_c=1e9 : maxres ~1.2e-7   — BREAKS (just over TAU_MODEL)
//!   r_c=1e10: maxres ~1.9e-6   — BREAKS
//! so the absolute oracle holds through ~1e8 and first breaks at ~1e9 (same class
//! as the PR-SSI1 ceiling and the SSI7 cone pair). Relative residual stays ~1e-16
//! throughout (attack5). The coaxial-detection band (`d_ax`/`|ĉ × â|`) uses an
//! ABSOLUTE distance compared to TAU_MODEL, so it is likewise scale-sensitive:
//! MEASURED a truly-coaxial config (generic axis â = (1,2,3)/|·|, axis_point
//! displaced 7.3·â off the apex so d_ax is a difference of O(scale) terms) holds
//! through ~7e8 and flips to ASNA by ~9e8–1e9 (attack5_dax). Both are documented
//! loud-`Err` ceilings, NOT logic bugs — we assert the characterized behavior.

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

/// Every field of a returned Circle must be finite (no NaN/Inf from a leaked
/// 0/0 or ∞ division), the normal must be unit, and the radius must be strictly
/// positive. Coaxial cyl∩cone has NO degenerate zero-radius root (unlike the
/// sphere∩cone apex-grazing case), so radius > 0 always.
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
            assert!(
                (norm(normal.as_array()) - 1.0).abs() < 1e-9,
                "Circle normal not unit: {c:?}"
            );
        }
        other => panic!("cylinder∩cone must only return Circles; got {other:?}"),
    }
}

/// Absolute implicit residual on a surface (PR-SSI1/SSI2 oracle).
///   cylinder: `| dist(x, cyl axis line) − r_c |`
///   cone:     `| |(x−P) − ((x−P)·â)·â| − |h|·tanα |`, `h = (x−P)·â`
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
            (norm(sub(rel, along)) - radius).abs()
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
            let r_actual = norm(sub(rel, scale(ahat, h)));
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

/// Distance from a point to a line through `base` along unit `ahat` (perp comp).
fn dist_to_axis(point: [f64; 3], base: [f64; 3], ahat: [f64; 3]) -> f64 {
    let rel = sub(point, base);
    norm(sub(rel, scale(ahat, dot(rel, ahat))))
}

/// Build a +z cone at `apex` with half-angle `alpha`.
fn z_cone(apex: [f64; 3], alpha: f64) -> QuadricSurface {
    QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    }
}

/// Build a cylinder.
fn cyl(axis_point: [f64; 3], axis_dir: [f64; 3], r_c: f64) -> QuadricSurface {
    QuadricSurface::Cylinder {
        axis_point: Point3::from(axis_point),
        axis_dir: Vector3::from(axis_dir),
        radius: r_c,
    }
}

/// A valid coaxial pair on +z gives two circles at h = ±r_c·cotα; this asserts
/// the full I2/I1 geometry for one circle.
fn assert_proper_z_circle(c: &SsiCurve, alpha: f64, r_c: f64) {
    assert_curve_finite(c);
    let (center, normal, radius) = circle_fields(c);
    assert!(
        (radius - r_c).abs() < TAU_MODEL,
        "radius {radius} != r_c {r_c}"
    );
    // center on the z-axis (x = y = 0) at |z| = r_c·cotα.
    assert!(
        center[0].abs() < TAU_MODEL && center[1].abs() < TAU_MODEL,
        "center off z-axis: {center:?}"
    );
    let h_expected = r_c / alpha.tan();
    assert!(
        (center[2].abs() - h_expected).abs() < TAU_MODEL.max(h_expected * 1e-9),
        "center.z magnitude {} != r_c·cotα {h_expected}",
        center[2].abs()
    );
    parallel_up_to_sign(normal, [0.0, 0.0, 1.0]);
}

// ===========================================================================
// Attack 1: Parallelism gate boundary — cyl axis tilted off the cone axis by
// an angle whose SINE sits just inside vs just outside TAU_MODEL.
//
// The gate is `|ĉ × â| < TAU_MODEL` (an ABSOLUTE sine of the inter-axis angle).
// At unit scale, axes kept through a common point (apex = axis_point = origin)
// so ONLY the parallelism term flips.
//
// MEASURED: sin = 0.99·TAU ⇒ Ok(2); sin = TAU (== boundary) ⇒ ASNA (strict `<`).
// We use 0.9·TAU just-inside and 1.001·TAU just-outside to be robust to the fp
// rounding of `frac·TAU`.
// ===========================================================================

#[test]
fn attack1_parallelism_gate_boundary() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone([0.0, 0.0, 0.0], alpha);
    let r_c = 2.0;

    // Just INSIDE the band (sine of tilt = 0.9·TAU < TAU) ⇒ two circles.
    //
    // CHARACTERIZATION: the solver treats an in-band tilt as coaxial and SNAPS
    // the circle axis to the CONE's â (+z), ignoring the ≤TAU cyl tilt. The
    // emitted circle therefore lies exactly on the cone (on-cone residual ~0)
    // but is off the *tilted* cylinder by O(tilt·r_c) ≈ 0.9·TAU·2 ≈ 1.8e-7,
    // which is just OVER the absolute TAU_MODEL oracle. This is the documented
    // in-band slack (the coaxial gate's whole point), NOT a defect: we assert
    // the on-CONE residual is tight and the off-cylinder residual is bounded by
    // the in-band slack, rather than forcing the two-surface oracle green.
    {
        let theta = (0.9 * TAU_MODEL).asin();
        // tilt cyl axis in the x–z plane so |ĉ × â| = sinθ.
        let cd = [theta.sin(), 0.0, theta.cos()];
        // sanity: the tilt sine is genuinely just under the gate.
        assert!(norm(cross(unit(cd), [0.0, 0.0, 1.0])) < TAU_MODEL);
        let c = cyl([0.0, 0.0, 0.0], cd, r_c);
        let curves = intersect(&c, &cone)
            .unwrap_or_else(|e| panic!("just-inside parallelism band must be Ok, got {e:?}"));
        assert_eq!(curves.len(), 2, "just-inside ⇒ two circles");
        for cc in &curves {
            assert_proper_z_circle(cc, alpha, r_c); // axis snapped to cone +z
                                                    // On the CONE exactly (the snapped axis matches the cone axis).
            assert!(
                max_residual_on_both(cc, &cone, &cone, 64) < TAU_MODEL,
                "in-band circle must lie on the cone tightly"
            );
            // Off the TILTED cylinder by at most the in-band slack O(tilt·r_c).
            let mut cyl_res = 0.0_f64;
            for i in 0..64 {
                let t = (i as f64) / 64.0 * std::f64::consts::TAU;
                cyl_res = cyl_res.max(implicit_residual(&c, cc.eval(t).as_array()));
            }
            let slack = norm(cross(unit(cd), [0.0, 0.0, 1.0])) * r_c;
            assert!(
                cyl_res <= slack + 1e-12,
                "tilted-cylinder residual {cyl_res} exceeds in-band slack {slack}"
            );
        }
    }

    // Just OUTSIDE the band (sine = 1.001·TAU ≥ TAU) ⇒ ASNA, no spurious circle.
    {
        let theta = (1.001 * TAU_MODEL).asin();
        let cd = [theta.sin(), 0.0, theta.cos()];
        assert!(norm(cross(unit(cd), [0.0, 0.0, 1.0])) >= TAU_MODEL);
        let c = cyl([0.0, 0.0, 0.0], cd, r_c);
        assert_eq!(
            intersect(&c, &cone),
            Err(SsiError::AnalyticalSolutionNotAvailable),
            "just-outside parallelism band ⇒ ASNA, not a circle"
        );
    }
}

// ===========================================================================
// Attack 2: On-axis gate boundary — cyl axis_point displaced PERPENDICULAR to
// the shared axis by just-under vs just-over TAU_MODEL. Axes kept EXACTLY
// parallel (both +z) so only `d_ax` flips.
//
// MEASURED at unit scale: off = 0.99·TAU ⇒ Ok(2); off = TAU ⇒ ASNA.
// We use 0.9·TAU just-under and 1.001·TAU just-over.
// ===========================================================================

#[test]
fn attack2_on_axis_gate_boundary() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone([0.0, 0.0, 0.0], alpha);
    let r_c = 2.0;

    // Just UNDER (perp displacement 0.9·TAU; also displaced ALONG z, which is
    // harmless — only the perp component is d_ax) ⇒ two circles.
    {
        let off = 0.9 * TAU_MODEL;
        let c = cyl([off, 0.0, 5.0], [0.0, 0.0, 1.0], r_c);
        let curves = intersect(&c, &cone)
            .unwrap_or_else(|e| panic!("just-under d_ax band must be Ok, got {e:?}"));
        assert_eq!(curves.len(), 2, "just-under ⇒ two circles");
        for cc in &curves {
            assert_curve_finite(cc);
        }
    }

    // Just OVER along +x, and just over along +y (non-axis-aligned) ⇒ ASNA.
    for &dir in &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        let off = 1.001 * TAU_MODEL;
        let c = cyl(scale(dir, off), [0.0, 0.0, 1.0], r_c);
        assert_eq!(
            intersect(&c, &cone),
            Err(SsiError::AnalyticalSolutionNotAvailable),
            "just-over d_ax band ({dir:?}) ⇒ ASNA, not a circle"
        );
    }
}

// ===========================================================================
// Attack 3: α near both E1 limits, from the valid side, plus crossing to the
// invalid side.
//
// α = TAU·(1+ε) (tiny): cotα ≈ 1/α huge ⇒ circles at very large |h|.
// α = π/2 − TAU·(1+ε): cotα ≈ (π/2−α) small ⇒ circles collapse toward apex.
// Inside the band: always EXACTLY two finite circles, radius == r_c.
// At/just beyond the bounds (α ≤ TAU, α ≥ π/2 − TAU): DegenerateInput.
// ===========================================================================

#[test]
fn attack3_alpha_near_e1_limits_valid_side() {
    let r_c = 2.0;
    let c = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], r_c);

    // Tiny α just INSIDE the lower bound: cotα huge ⇒ huge |h|, still two
    // finite circles, radius == r_c. (α = 2·TAU, 5·TAU, 100·TAU.)
    for &k in &[2.0, 5.0, 100.0] {
        let alpha = k * TAU_MODEL;
        let cone = z_cone([0.0, 0.0, 0.0], alpha);
        let curves = intersect(&c, &cone)
            .unwrap_or_else(|e| panic!("α={k}·TAU (tiny): must be Ok, got {e:?}"));
        assert_eq!(curves.len(), 2, "α={k}·TAU: two circles");
        let (center, _, _) = circle_fields(&curves[0]);
        // |h| = r_c·cotα is large at tiny α.
        assert!(
            center[2].abs() > r_c, // cotα = 1/tan α > 1 for α < π/4
            "α={k}·TAU: expected large |h|, got {}",
            center[2].abs()
        );
        for cc in &curves {
            assert_proper_z_circle(cc, alpha, r_c);
        }
    }

    // α just INSIDE the upper bound: cotα → 0 ⇒ circles collapse toward apex
    // (small |h|), still two finite circles, radius == r_c.
    for &k in &[2.0, 10.0, 100.0, 1000.0] {
        let alpha = std::f64::consts::FRAC_PI_2 - k * TAU_MODEL;
        let cone = z_cone([0.0, 0.0, 0.0], alpha);
        let curves =
            intersect(&c, &cone).unwrap_or_else(|e| panic!("α=π/2−{k}·TAU: must be Ok, got {e:?}"));
        assert_eq!(curves.len(), 2, "α=π/2−{k}·TAU: two circles");
        let (center, _, _) = circle_fields(&curves[0]);
        // |h| = r_c·cotα is small near π/2.
        assert!(
            center[2].abs() < r_c,
            "α=π/2−{k}·TAU: expected small |h|, got {}",
            center[2].abs()
        );
        for cc in &curves {
            assert_proper_z_circle(cc, alpha, r_c);
        }
    }
}

#[test]
fn attack3_alpha_at_e1_bounds_is_degenerate() {
    let c = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 2.0);
    // α exactly at the lower bound TAU_MODEL ⇒ DegenerateInput (`α ≤ TAU`).
    assert_eq!(
        intersect(&c, &z_cone([0.0, 0.0, 0.0], TAU_MODEL)),
        Err(SsiError::DegenerateInput)
    );
    // α below the lower bound ⇒ DegenerateInput.
    assert_eq!(
        intersect(&c, &z_cone([0.0, 0.0, 0.0], 0.5 * TAU_MODEL)),
        Err(SsiError::DegenerateInput)
    );
    // α exactly at the upper bound π/2 − TAU ⇒ DegenerateInput (`α ≥ π/2 − TAU`).
    assert_eq!(
        intersect(
            &c,
            &z_cone([0.0, 0.0, 0.0], std::f64::consts::FRAC_PI_2 - TAU_MODEL)
        ),
        Err(SsiError::DegenerateInput)
    );
    // α just beyond the upper bound ⇒ DegenerateInput.
    assert_eq!(
        intersect(
            &c,
            &z_cone(
                [0.0, 0.0, 0.0],
                std::f64::consts::FRAC_PI_2 - 0.5 * TAU_MODEL
            )
        ),
        Err(SsiError::DegenerateInput)
    );
    // α non-finite ⇒ DegenerateInput.
    assert_eq!(
        intersect(&c, &z_cone([0.0, 0.0, 0.0], f64::NAN)),
        Err(SsiError::DegenerateInput)
    );
}

// ===========================================================================
// Attack 4: Very small and very large r_c (valid coaxial), α = π/3 (cotα =
// 1/√3 ≠ 1, genuinely exercised).
//
// r_c = 1e-6 keeps coords tiny (|h| = r_c·cotα ≈ 5.8e-7) ⇒ the absolute
// TAU_MODEL on-surface oracle applies. r_c = 1e6 drives |h| ≈ 5.8e5 ⇒ use a
// RELATIVE on-surface check (the absolute oracle still holds at 1e6 per the
// header table, but we exercise the relative path here too). Both: two circles,
// radius == r_c, finite.
// ===========================================================================

#[test]
fn attack4_small_and_large_rc() {
    let alpha = std::f64::consts::FRAC_PI_3; // cotα = 1/√3
    let cone = z_cone([0.0, 0.0, 0.0], alpha);

    // Small r_c: absolute oracle regime.
    {
        let r_c = 1e-6;
        let c = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], r_c);
        let curves = intersect(&c, &cone).expect("small r_c X2");
        assert_eq!(curves.len(), 2);
        for cc in &curves {
            assert_proper_z_circle(cc, alpha, r_c);
            assert_on_both_surfaces(cc, &c, &cone); // absolute TAU oracle valid
        }
    }

    // Large r_c: relative on-surface check (coords ~ 5.8e5).
    {
        let r_c = 1e6;
        let c = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], r_c);
        let curves = intersect(&c, &cone).expect("large r_c X2");
        assert_eq!(curves.len(), 2);
        for cc in &curves {
            assert_curve_finite(cc);
            let (_, _, radius) = circle_fields(cc);
            assert!((radius - r_c).abs() < TAU_MODEL, "radius != r_c");
            let m = max_residual_on_both(cc, &c, &cone, 256);
            assert!(
                m / r_c < 1e-9,
                "large r_c relative residual {} too big",
                m / r_c
            );
        }
    }
}

// ===========================================================================
// Attack 5: Large-coordinate scale ceiling — measure where the ABSOLUTE
// on-surface oracle first breaks, and where a truly-coaxial config first flips
// to ASNA via the absolute d_ax/parallel band. CHARACTERIZE, do not loosen TAU.
//
// MEASURED (apex origin +z, α = π/4, coords ~ r_c, 256 samples):
//   r_c=1e8 : maxres ~1.5e-8  — HOLDS (absolute oracle)
//   r_c=1e9 : maxres ~1.2e-7  — BREAKS (relative residual still ~1e-16)
// ===========================================================================

#[test]
fn attack5_absolute_oracle_breakpoint() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone([0.0, 0.0, 0.0], alpha);

    // r_c = 1e8 ⇒ absolute on-surface oracle still HOLDS.
    {
        let r_c = 1e8;
        let c = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], r_c);
        let curves = intersect(&c, &cone).unwrap();
        let m = max_residual_on_both(&curves[0], &c, &cone, 256);
        assert!(
            m < TAU_MODEL,
            "r_c=1e8: absolute oracle unexpectedly broke ({m}); breakpoint moved"
        );
    }

    // r_c = 1e9 ⇒ absolute oracle BREAKS, but the solver stays analytically
    // correct (relative residual ~1e-16, radius == r_c, finite). This is the
    // absolute-tolerance CEILING — NOT a logic bug. Do NOT loosen TAU_MODEL.
    {
        let r_c = 1e9;
        let c = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], r_c);
        let curves = intersect(&c, &cone).unwrap();
        assert_curve_finite(&curves[0]);
        let (_, _, radius) = circle_fields(&curves[0]);
        assert!((radius - r_c).abs() < TAU_MODEL, "radius != r_c at scale");
        let m = max_residual_on_both(&curves[0], &c, &cone, 256);
        assert!(
            m >= TAU_MODEL,
            "r_c=1e9: absolute oracle unexpectedly held ({m}); breakpoint moved"
        );
        assert!(
            m / r_c < 1e-9,
            "r_c=1e9: relative residual {} too big",
            m / r_c
        );
    }
}

#[test]
fn attack5_dax_band_is_absolute_scale_sensitive() {
    // CHARACTERIZATION (not a solver bug). `d_ax = |rel − (rel·â)·â|` is compared
    // to an ABSOLUTE TAU_MODEL. With the cyl axis_point displaced 7.3·â OFF the
    // apex, d_ax is a difference of O(scale) quantities, so its fp floor grows
    // with the coordinate scale. A genuinely coaxial config at huge scale can
    // therefore read as NC ⇒ a loud ASNA (never a spurious circle).
    //
    // MEASURED (â = (1,2,3)/|·|, apex = (s, 1.5s, 0.5s), axis_point = apex + 7.3·â,
    // r_c = 6, α = π/4): holds Ok(2) through s = 7e8, flips to ASNA by s = 9e8
    // (and at 1e9, 1e10). So the d_ax-band breakpoint for THIS pair is ~7e8 → 9e8,
    // the same class as the PR-SSI1 absolute-oracle ceiling. We lock both halves.
    let alpha = std::f64::consts::FRAC_PI_4;
    let ahat = unit([1.0, 2.0, 3.0]);
    let probe = |s: f64| -> Result<Vec<SsiCurve>, SsiError> {
        let apex = [s, s * 1.5, s * 0.5];
        let axis_point = add(apex, scale(ahat, 7.3)); // exactly on the axis line
        let c = cyl(axis_point, ahat, 6.0);
        let cone = QuadricSurface::Cone {
            apex: Point3::from(apex),
            axis_dir: Vector3::from(ahat),
            half_angle: alpha,
        };
        intersect(&c, &cone)
    };

    // Holds through 7e8.
    for &s in &[0.0, 1.0, 1e3, 1e6, 1e8, 7e8] {
        let r = probe(s);
        assert!(
            matches!(r, Ok(ref v) if v.len() == 2),
            "scale={s:e}: truly-coaxial config misdetected: {r:?}"
        );
        if let Ok(v) = &r {
            for cc in v {
                assert_curve_finite(cc);
            }
        }
    }

    // At 1e9 (past the measured flip) a truly-coaxial config reads as ASNA. We
    // accept either Ok(2) or ASNA (both clean Results, no panic / NaN); ASNA is
    // the documented absolute-band noise, NOT a logic bug. Empirically: ASNA.
    match probe(1e9) {
        Ok(ref v) => {
            assert_eq!(v.len(), 2, "scale=1e9: unexpected circle count");
            for cc in v {
                assert_curve_finite(cc);
            }
        }
        Err(SsiError::AnalyticalSolutionNotAvailable) => {
            // EXPECTED-POSSIBLE: fp noise in d_ax exceeded the absolute
            // TAU_MODEL ⇒ a truly-coaxial config read as NC. Documented
            // absolute-band scale-sensitivity, not a logic bug.
        }
        Err(other) => panic!("scale=1e9: unexpected error {other:?}"),
    }
}

// ===========================================================================
// Attack 6: Near-parallel-but-not axes (tilt clearly above TAU) MUST read NC.
//
// A cyl axis tilted by 1e-3 rad off the cone axis (|ĉ × â| ≈ 1e-3 ≫ TAU=1e-7)
// must be Err(ASNA), NOT a spurious pair of circles. This guards against a
// too-loose parallelism gate.
// ===========================================================================

#[test]
fn attack6_supra_tau_tilt_is_nc_not_circles() {
    let cone = z_cone([0.0, 0.0, 0.0], std::f64::consts::FRAC_PI_4);
    for &theta in &[1e-3_f64, 1e-2, 0.1] {
        let cd = [theta.sin(), 0.0, theta.cos()];
        // sanity: tilt sine is well above the TAU gate.
        assert!(norm(cross(unit(cd), [0.0, 0.0, 1.0])) > 10.0 * TAU_MODEL);
        let c = cyl([0.0, 0.0, 0.0], cd, 2.0);
        assert_eq!(
            intersect(&c, &cone),
            Err(SsiError::AnalyticalSolutionNotAvailable),
            "tilt θ={theta} (≫ TAU) ⇒ NC (ASNA), not circles"
        );
    }
}

// ===========================================================================
// Attack 7: Anti-hack reinforcement — a DETERMINISTIC integer-stepped sweep of
// many valid coaxial configs (varying α, r_c, axis direction, apex). EVERY one
// must return Ok with EXACTLY 2 circles, assert_curve_finite, radius == r_c.
// No RNG (ssi-rs rule 4: determinism). Guards against any future regression
// introducing a spurious one/zero-circle branch (the P9/P10 anti-hack invariant
// of the spec: coaxial cyl∩cone is ALWAYS two circles for valid input).
// ===========================================================================

#[test]
fn attack7_deterministic_sweep_always_two_circles() {
    // A small fixed pool of axis directions (non-unit + reversed + oblique), all
    // exercised; the cyl axis is the cone axis scaled by a deterministic factor
    // (parallel by construction). axis_point is the apex displaced ALONG the axis
    // (stays coaxial, d_ax == 0).
    let axes: [[f64; 3]; 5] = [
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -2.0],   // reversed, non-unit
        [1.0, 2.0, 3.0],    // oblique
        [-3.0, -6.0, -9.0], // antiparallel to the previous line, non-unit
        [4.0, 0.0, 3.0],
    ];
    let apexes: [[f64; 3]; 3] = [[0.0, 0.0, 0.0], [3.0, -1.0, 4.0], [-2.0, 5.0, -7.0]];

    let mut count = 0usize;
    for (ai, &raw_axis) in axes.iter().enumerate() {
        let ahat = unit(raw_axis);
        for (pi, &apex) in apexes.iter().enumerate() {
            // Step α across the valid band and r_c across several decades.
            for astep in 1..=7 {
                // α ∈ {≈0.39 .. ≈1.18} rad, all safely inside (TAU, π/2−TAU).
                let alpha = 0.2 + (astep as f64) * (1.1 / 8.0);
                for rstep in 0..5 {
                    // r_c ∈ {0.5, 1.5, 4.5, 13.5, 40.5} (deterministic geometric).
                    let r_c = 0.5 * 3.0_f64.powi(rstep);
                    // cyl axis = cone axis * factor (parallel); factor sign/scale
                    // varies deterministically to exercise non-unit + reversed.
                    let factor = if (ai + pi + astep + rstep as usize).is_multiple_of(2) {
                        2.5
                    } else {
                        -1.3
                    };
                    let cyl_axis = scale(raw_axis, factor);
                    // axis_point displaced along the axis line (coaxial: d_ax = 0).
                    let along = ((astep + rstep as usize) as f64) - 3.0;
                    let axis_point = add(apex, scale(ahat, along));

                    let cone = QuadricSurface::Cone {
                        apex: Point3::from(apex),
                        axis_dir: Vector3::from(raw_axis),
                        half_angle: alpha,
                    };
                    let c = cyl(axis_point, cyl_axis, r_c);

                    let curves = intersect(&c, &cone).unwrap_or_else(|e| {
                        panic!(
                            "coaxial sweep [ai={ai} pi={pi} α={alpha} r_c={r_c}] \
                             must be Ok, got {e:?}"
                        )
                    });
                    assert_eq!(
                        curves.len(),
                        2,
                        "ANTI-HACK: coaxial input must yield EXACTLY two circles \
                         [ai={ai} pi={pi} α={alpha} r_c={r_c}], got {}",
                        curves.len()
                    );
                    for cc in &curves {
                        assert_curve_finite(cc);
                        let (center, normal, radius) = circle_fields(cc);
                        assert!(
                            (radius - r_c).abs() < TAU_MODEL,
                            "sweep radius {radius} != r_c {r_c}"
                        );
                        parallel_up_to_sign(normal, ahat);
                        // center on the shared axis line.
                        assert!(
                            dist_to_axis(center, apex, ahat) < TAU_MODEL.max(r_c * 1e-9),
                            "sweep center off axis (perp {})",
                            dist_to_axis(center, apex, ahat)
                        );
                    }
                    // The two h values are equal-and-opposite about the apex.
                    let z0 = dot(sub(circle_fields(&curves[0]).0, apex), ahat);
                    let z1 = dot(sub(circle_fields(&curves[1]).0, apex), ahat);
                    assert!(
                        (z0 + z1).abs() < TAU_MODEL.max(r_c * 1e-9),
                        "h's not equal-and-opposite: {z0} {z1}"
                    );
                    // I5 determinism: h>0 nappe first (z0 >= 0 along â).
                    assert!(z0 >= -TAU_MODEL, "h>0 nappe must be first: z0={z0}");
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 5 * 3 * 7 * 5, "sweep coverage count");
}

// ===========================================================================
// Attack 8: Reversed / negated / antiparallel axis directions. Cone axis = −z,
// or cyl axis_dir = −(cone axis_dir). These are still parallel LINES
// (`|ĉ × â| ≈ 0`) ⇒ coaxial ⇒ two circles, normal unit, on both surfaces.
// Characterize the h>0-first ordering relative to the (sign-ambiguous) â.
// ===========================================================================

#[test]
fn attack8_reversed_and_antiparallel_axes() {
    let alpha = std::f64::consts::FRAC_PI_4; // tanα = 1 ⇒ |h| = r_c
    let r_c = 2.0;

    // (a) Cone axis = −z, cyl axis = +z (antiparallel). â = −z (the cone's axis).
    {
        let cone = QuadricSurface::Cone {
            apex: Point3::from([0.0, 0.0, 0.0]),
            axis_dir: Vector3::new(0.0, 0.0, -1.0),
            half_angle: alpha,
        };
        let c = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], r_c);
        let curves = intersect(&c, &cone).expect("antiparallel still coaxial");
        assert_eq!(curves.len(), 2);
        let ahat = [0.0, 0.0, -1.0]; // cone's normalized axis
        for cc in &curves {
            assert_curve_finite(cc);
            let (_, normal, radius) = circle_fields(cc);
            assert!((radius - r_c).abs() < TAU_MODEL);
            parallel_up_to_sign(normal, ahat);
            assert_on_both_surfaces(cc, &c, &cone);
        }
        // h>0 nappe FIRST is measured along the cone's â (= −z), so curves[0]'s
        // center sits on the −z side (h = +r_c·cotα along â = −z).
        let z0 = dot(circle_fields(&curves[0]).0, ahat);
        let z1 = dot(circle_fields(&curves[1]).0, ahat);
        assert!(z0 > 0.0 && z1 < 0.0, "h>0 nappe (along â) must be first");
        // Centers as a SET are still {(0,0,+r_c),(0,0,−r_c)} in world coords.
        let mut zs = [
            circle_fields(&curves[0]).0[2],
            circle_fields(&curves[1]).0[2],
        ];
        zs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((zs[0] + r_c).abs() < TAU_MODEL && (zs[1] - r_c).abs() < TAU_MODEL);
    }

    // (b) Cone axis +z, cyl axis = −(cone axis) supplied non-unit (antiparallel
    // line). Coaxial; results identical (as a SET) to the aligned case.
    {
        let cone = z_cone([0.0, 0.0, 0.0], alpha);
        let c = cyl([0.0, 0.0, 0.0], [0.0, 0.0, -5.0], r_c); // antiparallel, non-unit
        let curves = intersect(&c, &cone).expect("antiparallel cyl axis still coaxial");
        assert_eq!(curves.len(), 2);
        for cc in &curves {
            assert_proper_z_circle(cc, alpha, r_c);
            assert_on_both_surfaces(cc, &c, &cone);
        }
    }
}

// ===========================================================================
// Attack 9 (extra sharp edge): apex AND axis_point both off-origin but on the
// SAME oblique line at moderate scale — the on-axis gate must accept a nonzero
// (but harmless) ALONG-axis displacement, and centers must land on the line.
// ===========================================================================

#[test]
fn attack9_apex_and_axis_point_on_same_oblique_line() {
    let alpha = std::f64::consts::FRAC_PI_3; // cotα = 1/√3
    let r_c = 3.0;
    let ahat = unit([2.0, -1.0, 2.0]);
    let apex = [5.0, 7.0, -3.0];
    // axis_point well along the axis line from the apex (coaxial, d_ax ≈ 0).
    let axis_point = add(apex, scale(ahat, 12.0));
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(ahat),
        half_angle: alpha,
    };
    // cyl axis supplied non-unit & parallel.
    let c = cyl(axis_point, scale(ahat, 4.0), r_c);

    let curves = intersect(&c, &cone).expect("coaxial oblique off-origin");
    assert_eq!(curves.len(), 2);
    let h_expected = r_c / alpha.tan();
    for cc in &curves {
        assert_curve_finite(cc);
        let (center, normal, radius) = circle_fields(cc);
        assert!((radius - r_c).abs() < TAU_MODEL, "radius != r_c");
        assert!((norm(normal) - 1.0).abs() < TAU_MODEL, "normal not unit");
        parallel_up_to_sign(normal, ahat);
        // center on the axis line through the apex.
        assert!(
            dist_to_axis(center, apex, ahat) < TAU_MODEL,
            "center off axis line (perp {})",
            dist_to_axis(center, apex, ahat)
        );
        // |along-axis offset from apex| = r_c·cotα.
        let along = dot(sub(center, apex), ahat).abs();
        assert!(
            (along - h_expected).abs() < TAU_MODEL,
            "along-axis |h| {along} != r_c·cotα {h_expected}"
        );
        assert_on_both_surfaces(cc, &c, &cone);
    }
    // I5: h>0 nappe first (along â).
    let h0 = dot(sub(circle_fields(&curves[0]).0, apex), ahat);
    assert!(h0 > 0.0, "h>0 nappe must be first (h0={h0})");
}

// ===========================================================================
// Attack 10: Symmetry (I4) + determinism (I5). intersect(cyl, cone) ==
// intersect(cone, cyl) as a circle SET (order/normal-sign tolerant), and the
// X2 output is byte-identical across repeats with the h>0 nappe first. Also the
// NC verdict is symmetric.
// ===========================================================================

fn circle_key(c: &SsiCurve) -> (i64, i64, i64, i64, i64, i64, i64) {
    let (center, normal, radius) = circle_fields(c);
    let n = unit(normal);
    // Canonicalize normal sign: first non-near-zero component positive.
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
    (
        q(center[0]),
        q(center[1]),
        q(center[2]),
        q(n[0]),
        q(n[1]),
        q(n[2]),
        q(radius),
    )
}

#[test]
fn attack10_symmetry_and_determinism() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone([0.0, 0.0, 0.0], alpha);
    let r_c = 2.0;
    let c = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], r_c);

    // I4 — symmetry as a SET (cyl,cone) vs (cone,cyl).
    let ab = intersect(&c, &cone).unwrap();
    let ba = intersect(&cone, &c).unwrap();
    assert_eq!(ab.len(), 2);
    assert_eq!(ba.len(), 2);
    let mut abk: Vec<_> = ab.iter().map(circle_key).collect();
    let mut bak: Vec<_> = ba.iter().map(circle_key).collect();
    abk.sort();
    bak.sort();
    assert_eq!(abk, bak, "X2 circle set must match across argument order");

    // I5 — byte-identical across repeats, h>0 nappe first (center.z = +r_c).
    let first = intersect(&c, &cone);
    for _ in 0..8 {
        assert_eq!(first, intersect(&c, &cone), "X2 output not byte-identical");
    }
    let cf = first.unwrap();
    let (c0, _, _) = circle_fields(&cf[0]);
    assert!(
        (c0[2] - r_c).abs() < TAU_MODEL,
        "first center.z {} != +r_c {r_c} (h>0 nappe first)",
        c0[2]
    );

    // The argument-swapped call is itself deterministic.
    let swapped = intersect(&cone, &c);
    assert_eq!(
        swapped,
        intersect(&cone, &c),
        "swapped call not deterministic"
    );

    // NC verdict is symmetric both ways.
    let off_axis = cyl([0.5, 0.0, 0.0], [0.0, 0.0, 1.0], r_c);
    assert_eq!(
        intersect(&off_axis, &cone),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
    assert_eq!(
        intersect(&cone, &off_axis),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );
}

// ===========================================================================
// Attack 11: E1 degenerate inputs beyond the α bounds — r_c ≤ 0 / non-finite,
// zero / non-finite cone axis, zero / non-finite cyl axis.
// ===========================================================================

#[test]
fn attack11_degenerate_inputs() {
    let cone = z_cone([0.0, 0.0, 0.0], std::f64::consts::FRAC_PI_4);
    let cone_zero_axis = QuadricSurface::Cone {
        apex: Point3::from([0.0, 0.0, 0.0]),
        axis_dir: Vector3::new(0.0, 0.0, 0.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let good_cyl = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 2.0);

    // r_c = 0 and r_c < 0 and non-finite ⇒ DegenerateInput.
    for &bad_r in &[0.0, -1.0, f64::INFINITY, f64::NAN] {
        let c = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], bad_r);
        assert_eq!(
            intersect(&c, &cone),
            Err(SsiError::DegenerateInput),
            "r_c={bad_r} ⇒ DegenerateInput"
        );
    }

    // Zero cyl axis ⇒ DegenerateInput.
    let c_zero = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 2.0);
    assert_eq!(intersect(&c_zero, &cone), Err(SsiError::DegenerateInput));

    // Non-finite cyl axis ⇒ DegenerateInput.
    let c_nan = cyl([0.0, 0.0, 0.0], [f64::NAN, 0.0, 1.0], 2.0);
    assert_eq!(intersect(&c_nan, &cone), Err(SsiError::DegenerateInput));

    // Zero cone axis ⇒ DegenerateInput.
    assert_eq!(
        intersect(&good_cyl, &cone_zero_axis),
        Err(SsiError::DegenerateInput)
    );
}
