//! PR-SSI7 — Adversarial audit of the sphere∩cone coaxial solver.
//!
//! These tests attack `sphere_cone` (reached via the public `intersect`
//! dispatcher) at its band boundaries (X2↔X1↔X0 tangent limit gated on the
//! linear gap `g = r_s − |h0|·sinα`, and the NC↔coaxial detection band gated on
//! `d_ax`), under both-nappe vs single-nappe root signs, at near-0 / near-π/2
//! half-angles, under non-unit / reversed axes, at extreme scale, on the
//! apex-grazing `r_s = |h0|` degeneracy, and on symmetry + determinism. They do
//! NOT touch production code.
//!
//! Spec: specs/ssi_pr_ssi7_sphere_cone_coaxial.md
//! Mirrors ssi6_adversary's discipline: the cone radial residual + sphere
//! residual on-surface oracle, `assert_curve_finite`, RELATIVE residual at
//! large scale, and explicit CHARACTERIZATION of every absolute-tolerance
//! ceiling rather than forcing green.
//!
//! Tolerance note: TAU_MODEL = 1e-7. The absolute on-surface oracle is valid
//! only while curve sample coordinates stay below the measured breakpoint
//! (MEASURED for this pair: holds through r_s ≈ 1e8, first breaks at 1e9; see
//! attack5). Where scale drives coordinates large, tests switch to a RELATIVE
//! analytical check. The coaxial-detection band uses an ABSOLUTE distance
//! `d_ax < TAU_MODEL`, which is likewise scale-sensitive (MEASURED: a truly
//! coaxial config holds through scale 1e8, flips to ASNA at 1e9); attack2
//! characterizes that honestly.

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
/// √(negative) or 0/0), the normal must be unit, and the radius must be
/// strictly positive. This is the standard guard for proper (non-degenerate)
/// circles. The apex-grazing degeneracy (attack7) emits one radius-0 circle on
/// purpose, so that test uses `assert_curve_finite_allow_zero` instead.
fn assert_curve_finite(c: &SsiCurve) {
    assert_curve_finite_inner(c, false);
}

/// Like [`assert_curve_finite`] but permits `radius == 0` (the documented
/// apex-grazing degeneracy where one root is `h = 0`).
fn assert_curve_finite_allow_zero(c: &SsiCurve) {
    assert_curve_finite_inner(c, true);
}

fn assert_curve_finite_inner(c: &SsiCurve, allow_zero_radius: bool) {
    match c {
        SsiCurve::SurfacePair { .. } => unreachable!(
            "this suite's solvers never produce a surface-pair curve (M5 cyl×cyl only)"
        ),
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => {
            for v in center.as_array().iter().chain(normal.as_array().iter()) {
                assert!(v.is_finite(), "Circle field non-finite: {c:?}");
            }
            assert!(radius.is_finite(), "Circle radius non-finite: {c:?}");
            if allow_zero_radius {
                assert!(*radius >= 0.0, "Circle radius must be >= 0: {c:?}");
            } else {
                assert!(*radius > 0.0, "Circle radius must be > 0: {c:?}");
            }
            // Normal must be unit (defensive normalization).
            assert!(
                (norm(normal.as_array()) - 1.0).abs() < 1e-9,
                "Circle normal not unit: {c:?}"
            );
        }
        other => panic!("sphere∩cone must only return Circles; got {other:?}"),
    }
}

/// Absolute implicit residual on a surface (PR-SSI1/SSI2 oracle). For the cone
/// this is the radial residual `| |(x−P) − ((x−P)·â)·â| − |h|·tanα |`,
/// `h = (x−P)·â` — the residual already used by the ssi6/ssi7 helpers.
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
        SsiCurve::SurfacePair { .. } => unreachable!(
            "this suite's solvers never produce a surface-pair curve (M5 cyl×cyl only)"
        ),
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => (center.as_array(), normal.as_array(), *radius),
        other => panic!("expected Circle, got {other:?}"),
    }
}

/// Distance from a point to the cone axis line (perp component of `point − P`).
fn dist_to_axis(point: [f64; 3], apex: [f64; 3], ahat: [f64; 3]) -> f64 {
    let rel = sub(point, apex);
    norm(sub(rel, scale(ahat, dot(rel, ahat))))
}

fn z_cone(apex: [f64; 3], alpha: f64) -> QuadricSurface {
    QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    }
}

fn sphere_at(center: [f64; 3], r: f64) -> QuadricSurface {
    QuadricSurface::Sphere {
        center: Point3::from(center),
        radius: r,
    }
}

// ===========================================================================
// Attack 1: Tangent boundary sweep (X2↔X1↔X0) — the √D trap, at α ≠ π/4.
//
// Gate is the LINEAR gap g = r_s − |h0|·sinα. Fix apex=origin, axis=+z, a cone
// half-angle α = π/3 (so tanα = √3 ≠ 1 is genuinely exercised), and h0 = 2 (so
// C = (0,0,2)). Sweep r_s so g hits +1e-2/+1e-4/+1e-6 (X2, two circles), |g| ≤
// 1e-9 (X1, one circle), −1e-6/−1e-2 (X0, empty).
//
// Assert: correct count each side (2/1/0); every circle finite (no √neg NaN);
// radius = |h|·tanα; on both surfaces; and the two X2 circles' axial separation
// shrinks → 0 as g → 0⁺ (collapsing onto the tangent circle at h_t = h0·cos²α).
// ===========================================================================

#[test]
fn attack1_tangent_band_sweep_alpha_pi3() {
    let alpha = std::f64::consts::FRAC_PI_3; // tanα = √3, sec²α = 4, cos²α = 1/4
    let tana = alpha.tan();
    let sina = alpha.sin();
    let cos2 = alpha.cos() * alpha.cos();
    let h0 = 2.0_f64;
    let apex = [0.0, 0.0, 0.0];
    let cone = z_cone(apex, alpha);
    // r_s that makes g = 0 exactly: r_s = |h0|·sinα.
    let r_tan = h0.abs() * sina;
    let h_t = h0 * cos2; // tangent circle axial height

    let cases: &[(f64, &str)] = &[
        (1e-2, "two"),
        (1e-4, "two"),
        (1e-6, "two"), // g = 1e-6 > TAU=1e-7 ⇒ still X2; √D tiny but real
        (1e-9, "one"), // |g| < TAU ⇒ X1
        (-1e-9, "one"),
        (-1e-6, "empty"), // g = −1e-6 < −TAU ⇒ X0
        (-1e-2, "empty"),
    ];

    for &(gv, kind) in cases {
        let r_s = r_tan + gv;
        let sphere = sphere_at([0.0, 0.0, h0], r_s);
        let curves = intersect(&sphere, &cone)
            .unwrap_or_else(|e| panic!("g={gv:e} ({kind}): must not error, got {e:?}"));
        for c in &curves {
            assert_curve_finite(c); // no √(negative) NaN, ever
        }
        match kind {
            "two" => {
                assert_eq!(curves.len(), 2, "g={gv:e}: expected two circles");
                for c in &curves {
                    let (center, normal, radius) = circle_fields(c);
                    // radius = |h|·tanα, where h = center.z (apex at origin, axis +z).
                    let h = center[2];
                    assert!(
                        (radius - h.abs() * tana).abs() < TAU_MODEL,
                        "g={gv:e}: radius {radius} != |h|·tanα {}",
                        h.abs() * tana
                    );
                    parallel_up_to_sign(normal, [0.0, 0.0, 1.0]);
                    // center on the axis (x = y = 0).
                    assert!(center[0].abs() < TAU_MODEL && center[1].abs() < TAU_MODEL);
                    assert_on_both_surfaces(c, &sphere, &cone);
                }
                // The two circles straddle h_t and their axial separation → 0 as g → 0⁺.
                let z0 = circle_fields(&curves[0]).0[2];
                let z1 = circle_fields(&curves[1]).0[2];
                let sep = (z0 - z1).abs();
                assert!(sep > 0.0, "g={gv:e}: circles collapsed prematurely");
                // Midpoint sits at h_t (= h0·cos²α) — the symmetry center of the roots.
                let mid = 0.5 * (z0 + z1);
                assert!(
                    (mid - h_t).abs() < 1e-9,
                    "g={gv:e}: root midpoint {mid} != h_t {h_t}"
                );
            }
            "one" => {
                assert_eq!(curves.len(), 1, "g={gv:e}: expected one circle (X1)");
                let (center, normal, radius) = circle_fields(&curves[0]);
                // X1 circle is exactly at h_t = h0·cos²α, radius |h_t|·tanα.
                assert!(
                    (center[2] - h_t).abs() < TAU_MODEL,
                    "g={gv:e}: X1 center.z {} != h_t {h_t}",
                    center[2]
                );
                assert!(
                    (radius - h_t.abs() * tana).abs() < TAU_MODEL,
                    "g={gv:e}: X1 radius {radius} != |h_t|·tanα"
                );
                parallel_up_to_sign(normal, [0.0, 0.0, 1.0]);
                assert_on_both_surfaces(&curves[0], &sphere, &cone);
            }
            "empty" => {
                assert!(
                    curves.is_empty(),
                    "g={gv:e} ({kind}): expected empty, got {curves:?}"
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn attack1_two_circles_converge_as_g_to_zero() {
    // As g → 0⁺ the two X2 circles' axial separation shrinks monotonically to 0,
    // the clean switch into the X1 band. α = π/6 here (tanα = 1/√3, another
    // non-π/4 angle), h0 = 3, apex origin +z. g = r_s − 3·sin(π/6) = r_s − 1.5.
    // g = 1.5·10^-k must stay > TAU=1e-7 ⇒ k ≤ 6 (1.5e-7 > 1e-7).
    let alpha = std::f64::consts::FRAC_PI_6;
    let sina = alpha.sin();
    let h0 = 3.0_f64;
    let r_tan = h0 * sina;
    let cone = z_cone([0.0, 0.0, 0.0], alpha);
    let mut prev_sep = f64::INFINITY;
    for k in 1..=6 {
        let g = 1.5 * 10.0_f64.powi(-k);
        let r_s = r_tan + g;
        let sphere = sphere_at([0.0, 0.0, h0], r_s);
        let curves = intersect(&sphere, &cone).unwrap();
        assert_eq!(curves.len(), 2, "k={k} g={g:e}: expected two circles");
        let z0 = circle_fields(&curves[0]).0[2];
        let z1 = circle_fields(&curves[1]).0[2];
        let sep = (z0 - z1).abs();
        assert!(
            sep.is_finite(),
            "k={k}: separation non-finite (√neg leaked?)"
        );
        assert!(
            sep < prev_sep,
            "k={k}: separation {sep} not shrinking (prev {prev_sep}) as g→0⁺"
        );
        assert!(sep > 0.0, "k={k}: circles collapsed prematurely");
        prev_sep = sep;
    }
    assert!(prev_sep < 1e-2, "final separation {prev_sep} not near zero");
}

#[test]
fn attack1_g_just_above_tau_no_negative_sqrt() {
    // THE critical danger: g just above TAU feeding √D. Walk g down to ~1.5·TAU
    // from the X2 side and confirm D stays ≥ 0 (h real/finite) and the result is
    // always exactly two finite circles — the √D gate (g > TAU ⇒ D > 0) holds.
    let alpha = std::f64::consts::FRAC_PI_4;
    let h0 = 0.0; // symmetric roots ⇒ D = sec²α·r_s², cleanest √ stress
    let cone = z_cone([0.0, 0.0, 0.0], alpha);
    // With h0 = 0: g = r_s, so r_s just above TAU drives √D = √(sec²α·r_s²).
    for &g in &[1e-1, 1e-3, 1e-5, 5e-7, 2e-7, 1.5e-7] {
        let r_s = g; // g = r_s − 0 = r_s
        let sphere = sphere_at([0.0, 0.0, h0], r_s);
        let curves = intersect(&sphere, &cone)
            .unwrap_or_else(|e| panic!("g={g:e}: must not error, got {e:?}"));
        assert_eq!(curves.len(), 2, "g={g:e}: expected two circles (X2)");
        for c in &curves {
            assert_curve_finite(c);
            let (center, _, _) = circle_fields(c);
            assert!(center[2].is_finite(), "g={g:e}: center.z non-finite (√neg)");
            // radius ~ r_s·cos²α·tanα; just confirm it is finite & positive here.
        }
        // Symmetric about apex (h0 = 0) ⇒ z values opposite-signed, equal magnitude.
        let z0 = circle_fields(&curves[0]).0[2];
        let z1 = circle_fields(&curves[1]).0[2];
        assert!(
            (z0 + z1).abs() < TAU_MODEL,
            "g={g:e}: not symmetric about apex"
        );
    }
}

// ===========================================================================
// Attack 2: Coaxial-detection band (NC↔coaxial), d_ax across TAU_MODEL.
//
// Sphere center offset perpendicular to the axis so d_ax = offset sweeps across
// TAU_MODEL. d_ax < TAU ⇒ coaxial circles; d_ax ≥ TAU ⇒ Err(ASNA). Clean
// switch, no panic, no spurious circle just over the band.
//
// CHARACTERIZATION: d_ax is an ABSOLUTE distance, so at large coordinate scale
// the coaxial split is scale-sensitive. The second test locks the MEASURED
// breakpoint (holds through 1e8, flips to ASNA at 1e9 with a generic axis).
// ===========================================================================

#[test]
fn attack2_coaxial_detection_band_unit_scale() {
    // Canonical X2 geometry: apex origin, axis +z, α = π/4, sphere r_s = 2 with
    // h0 = 0 (two circles when coaxial). Offset the sphere center along +x by
    // `off` ⇒ d_ax = off exactly.
    let alpha = std::f64::consts::FRAC_PI_4;
    let r_s = 2.0;
    let cone = z_cone([0.0, 0.0, 0.0], alpha);

    // Below the band ⇒ coaxial (two circles).
    for &off in &[0.0, 0.25 * TAU_MODEL, 0.5 * TAU_MODEL, 0.9 * TAU_MODEL] {
        let sphere = sphere_at([off, 0.0, 0.0], r_s);
        let curves = intersect(&sphere, &cone)
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

    // At/above the band ⇒ NC ⇒ the procedural SurfacePair (F10 contract; was
    // the staged ASNA). Still the point of the attack: NOT a wrong/degenerate
    // circle.
    for &off in &[TAU_MODEL, 1.0001 * TAU_MODEL, 2.0 * TAU_MODEL, 1e-3, 0.1] {
        let sphere = sphere_at([off, 0.0, 0.0], r_s);
        assert_eq!(
            intersect(&sphere, &cone),
            Ok(vec![SsiCurve::SurfacePair { a: cone, b: sphere }]),
            "off={off:e}: d_ax ≥ TAU ⇒ must be NC (SurfacePair), not a circle"
        );
    }
}

#[test]
fn attack2_barely_offset_center_is_surface_pair_not_degenerate_circle() {
    // A sphere center offset by just over TAU from the axis must yield the NC
    // SurfacePair (F10 contract; was ASNA), NOT a degenerate/near-tangent
    // circle. The danger is a misclassification that produces a geometrically
    // wrong circle near the band.
    let cone = z_cone([0.0, 0.0, 0.0], std::f64::consts::FRAC_PI_4);
    // d_ax = 1.5·TAU along +x.
    let s1 = sphere_at([1.5 * TAU_MODEL, 0.0, 0.0], 2.0);
    assert_eq!(
        intersect(&s1, &cone),
        Ok(vec![SsiCurve::SurfacePair { a: cone, b: s1 }]),
        "barely-offset center must be the NC SurfacePair, not a wrong circle"
    );
    // And a non-axis-aligned tiny offset (along +y) likewise.
    let s2 = sphere_at([0.0, 2.0 * TAU_MODEL, 0.0], 2.0);
    assert_eq!(
        intersect(&s2, &cone),
        Ok(vec![SsiCurve::SurfacePair { a: cone, b: s2 }])
    );
}

#[test]
fn attack2_coaxial_band_is_absolute_scale_sensitive() {
    // CHARACTERIZATION (not a solver bug). The coaxial discriminant
    // `d_ax = |rel − (rel·â)·â|` is compared against an ABSOLUTE TAU_MODEL.
    // d_ax is computed by subtracting two O(scale) quantities, so its floor
    // rounding error grows with the coordinate scale. At huge scale, fp noise in
    // the projection can push a *genuinely coaxial* configuration's computed
    // d_ax above TAU_MODEL → spurious ASNA (a loud, never-wrong failure mode).
    //
    // MEASURED (apex & center on a common generic axis â = (1,2,3)/|·| placed at
    // (s, 1.5s, 0.5s), sphere center = apex + 7.3·â, r_s = 6, α = π/4):
    //   scale 1e0/1e3/1e6/1e7/1e8 : Ok(2 circles)  — coaxial detected
    //   scale 1e9 / 1e10          : Err(ASNA)       — fp noise > absolute TAU
    // So the coaxial-band breakpoint for THIS pair is ~1e8 → 1e9, the same class
    // as the PR-SSI1 absolute-oracle ceiling. We lock both halves honestly.
    let alpha = std::f64::consts::FRAC_PI_4;
    let ahat = unit([1.0, 2.0, 3.0]);
    let probe = |s: f64| -> Result<Vec<SsiCurve>, SsiError> {
        let apex = [s, s * 1.5, s * 0.5];
        let center = add(apex, scale(ahat, 7.3)); // exactly on the axis line
        let sphere = sphere_at(center, 6.0);
        let cone = QuadricSurface::Cone {
            apex: Point3::from(apex),
            axis_dir: Vector3::from(ahat),
            half_angle: alpha,
        };
        intersect(&sphere, &cone)
    };

    // Holds through 1e8.
    for &s in &[0.0, 1.0, 1e3, 1e6, 1e8] {
        let r = probe(s);
        assert!(
            matches!(r, Ok(ref v) if v.len() == 2),
            "scale={s:e}: truly-coaxial config misdetected: {r:?}"
        );
        if let Ok(v) = &r {
            for c in v {
                assert_curve_finite(c);
            }
        }
    }

    // At 1e9 (past the measured flip) a truly-coaxial config reads as ASNA. We
    // do NOT force a verdict — require only a clean Result (no panic, no NaN).
    // Both Ok(2) and ASNA are acceptable; ASNA is the documented absolute-band
    // noise, NOT a logic bug. (Empirically: ASNA at 1e9 with this generic axis.)
    match probe(1e9) {
        Ok(ref v) if v.len() == 1 && matches!(v[0], SsiCurve::SurfacePair { .. }) => {
            // EXPECTED-POSSIBLE: fp noise in d_ax exceeded the absolute
            // TAU_MODEL ⇒ a truly-coaxial config read as NC, which since F10
            // returns the (exact, still-correct) SurfacePair instead of the
            // former ASNA. Documented absolute-band scale-sensitivity, not a
            // logic bug.
        }
        Ok(ref v) => {
            for c in v {
                assert_curve_finite(c);
            }
            assert_eq!(v.len(), 2, "scale=1e9: unexpected circle count");
        }
        Err(other) => panic!("scale=1e9: unexpected error {other:?}"),
    }
}

// ===========================================================================
// Attack 3: Both-nappe vs single-nappe root signs.
//
// (a) h0 small / r_s large ⇒ roots straddle 0 (one circle per nappe, opposite
//     z-signs): h0=3, r_s=4, α=π/4 ⇒ roots ≈ 3.898 (+nappe), −0.898 (−nappe).
// (b) h0 large ⇒ both roots same sign (both circles on one nappe): h0=10,
//     r_s=6, α=π/6 ⇒ roots ≈ 10.37, 4.63 (both +z).
// Verify centers land at P + h·â with correct signs, +√D first, on both
// surfaces, radius = |h|·tanα.
// ===========================================================================

#[test]
fn attack3_roots_straddle_zero_one_per_nappe() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let tana = alpha.tan();
    let sphere = sphere_at([0.0, 0.0, 3.0], 4.0); // h0 = 3
    let cone = z_cone([0.0, 0.0, 0.0], alpha);
    let curves = intersect(&sphere, &cone).expect("X2 straddling roots");
    assert_eq!(curves.len(), 2);

    // sec²α·h² − 2h0·h + (h0²−r_s²) = 0 ⇒ 2h² − 6h − 7 = 0; roots (3±√23)/2.
    let root_d = 23.0_f64.sqrt();
    let h_plus = (3.0 + root_d) / 2.0; // ≈ +3.898 (one nappe)
    let h_minus = (3.0 - root_d) / 2.0; // ≈ −0.898 (other nappe)
    assert!(h_plus > 0.0 && h_minus < 0.0, "roots should straddle 0");

    // +√D first ⇒ curves[0] is the larger root (h_plus).
    let (c0, n0, r0) = circle_fields(&curves[0]);
    let (c1, n1, r1) = circle_fields(&curves[1]);
    assert!(
        (c0[2] - h_plus).abs() < TAU_MODEL,
        "first center.z != h_plus"
    );
    assert!(
        (c1[2] - h_minus).abs() < TAU_MODEL,
        "second center.z != h_minus"
    );
    assert!((r0 - h_plus.abs() * tana).abs() < TAU_MODEL);
    assert!((r1 - h_minus.abs() * tana).abs() < TAU_MODEL);
    parallel_up_to_sign(n0, [0.0, 0.0, 1.0]);
    parallel_up_to_sign(n1, [0.0, 0.0, 1.0]);
    for c in &curves {
        assert_curve_finite(c);
        assert_on_both_surfaces(c, &sphere, &cone);
    }
}

#[test]
fn attack3_both_roots_same_nappe() {
    let alpha = std::f64::consts::FRAC_PI_6; // sec²α = 4/3, tan²α = 1/3, cos²α = 3/4
    let tana = alpha.tan();
    let sphere = sphere_at([0.0, 0.0, 10.0], 6.0); // h0 = 10
    let cone = z_cone([0.0, 0.0, 0.0], alpha);
    let curves = intersect(&sphere, &cone).expect("X2 same-nappe");
    assert_eq!(curves.len(), 2);

    // D = sec²α·r_s² − h0²·tan²α = (4/3)·36 − 100·(1/3) = 48 − 33.333 = 14.667.
    let cos2 = alpha.cos() * alpha.cos();
    let disc = 36.0 / cos2 - 100.0 * tana * tana;
    let sqrt_d = disc.sqrt();
    let h_plus = (10.0 + sqrt_d) * cos2; // ≈ 10.37
    let h_minus = (10.0 - sqrt_d) * cos2; // ≈ 4.63
    assert!(
        h_plus > 0.0 && h_minus > 0.0,
        "both roots same nappe (both +)"
    );

    let (c0, _, r0) = circle_fields(&curves[0]);
    let (c1, _, r1) = circle_fields(&curves[1]);
    assert!(
        (c0[2] - h_plus).abs() < TAU_MODEL,
        "first center.z != h_plus"
    );
    assert!(
        (c1[2] - h_minus).abs() < TAU_MODEL,
        "second center.z != h_minus"
    );
    assert!(c0[2] > 0.0 && c1[2] > 0.0, "both centers should be +z");
    assert!((r0 - h_plus.abs() * tana).abs() < TAU_MODEL);
    assert!((r1 - h_minus.abs() * tana).abs() < TAU_MODEL);
    for c in &curves {
        assert_curve_finite(c);
        assert_on_both_surfaces(c, &sphere, &cone);
    }
}

// ===========================================================================
// Attack 4: Near-0 and near-π/2 half-angle (α at the edges of the valid band).
//
// α just inside the band: tanα → 0 near 0 (tiny circles); secα → ∞ near π/2.
// Assert finite, on-surface, correct geometry. Also assert α exactly at the E1
// bounds (≤ TAU and ≥ π/2 − TAU) → DegenerateInput.
// ===========================================================================

#[test]
fn attack4_near_zero_alpha_tiny_radii() {
    // α = k·TAU just above the lower E1 bound. With h0 = 0, r_s = 2, both roots
    // are h = ±r_s·cos²α·secα ≈ ±r_s, radius = |h|·tanα ≈ r_s·tanα (tiny).
    let r_s = 2.0;
    for &k in &[2.0, 5.0, 100.0] {
        let alpha = k * TAU_MODEL;
        let cone = z_cone([0.0, 0.0, 0.0], alpha);
        let sphere = sphere_at([0.0, 0.0, 0.0], r_s);
        let curves = intersect(&sphere, &cone)
            .unwrap_or_else(|e| panic!("α={k}·TAU: must not error, got {e:?}"));
        assert_eq!(curves.len(), 2, "α={k}·TAU: expected two circles");
        for c in &curves {
            assert_curve_finite(c);
            let (center, _, radius) = circle_fields(c);
            // radius = |h|·tanα, tiny but positive & finite.
            assert!((radius - center[2].abs() * alpha.tan()).abs() < TAU_MODEL);
            assert_on_both_surfaces(c, &sphere, &cone);
        }
    }
}

#[test]
fn attack4_near_half_pi_alpha_finite_and_on_surface() {
    // α just below the upper E1 bound. With h0 = 0 the circles are at
    // h = ±r_s·cos²α·secα = ±r_s·cosα, radius = |h|·tanα = r_s·sinα ≈ r_s. The
    // secα blow-up cancels because h0 = 0; assert finite + on-surface anyway.
    let r_s = 2.0;
    for &k in &[2.0, 10.0, 100.0, 1000.0] {
        let alpha = std::f64::consts::FRAC_PI_2 - k * TAU_MODEL;
        let cone = z_cone([0.0, 0.0, 0.0], alpha);
        let sphere = sphere_at([0.0, 0.0, 0.0], r_s);
        let curves = intersect(&sphere, &cone)
            .unwrap_or_else(|e| panic!("α=π/2−{k}·TAU: must not error, got {e:?}"));
        assert_eq!(curves.len(), 2, "α=π/2−{k}·TAU: expected two circles");
        for c in &curves {
            assert_curve_finite(c);
            let (_, _, radius) = circle_fields(c);
            // radius = r_s·sinα ≈ r_s here.
            assert!(
                (radius - r_s * alpha.sin()).abs() < TAU_MODEL,
                "α=π/2−{k}·TAU: radius {radius} != r_s·sinα"
            );
            assert_on_both_surfaces(c, &sphere, &cone);
        }
    }
}

#[test]
fn attack4_alpha_exactly_at_e1_bounds_is_degenerate() {
    let sphere = sphere_at([0.0, 0.0, 0.0], 2.0);
    // α exactly at the lower bound TAU_MODEL ⇒ DegenerateInput (`α ≤ TAU`).
    let lo = z_cone([0.0, 0.0, 0.0], TAU_MODEL);
    assert_eq!(intersect(&sphere, &lo), Err(SsiError::DegenerateInput));
    // α just inside the lower bound from below (should still be degenerate).
    let lo2 = z_cone([0.0, 0.0, 0.0], 0.5 * TAU_MODEL);
    assert_eq!(intersect(&sphere, &lo2), Err(SsiError::DegenerateInput));
    // α exactly at the upper bound π/2 − TAU ⇒ DegenerateInput (`α ≥ π/2 − TAU`).
    let hi = z_cone([0.0, 0.0, 0.0], std::f64::consts::FRAC_PI_2 - TAU_MODEL);
    assert_eq!(intersect(&sphere, &hi), Err(SsiError::DegenerateInput));
    // α non-finite ⇒ DegenerateInput.
    let nan = z_cone([0.0, 0.0, 0.0], f64::NAN);
    assert_eq!(intersect(&sphere, &nan), Err(SsiError::DegenerateInput));
}

// ===========================================================================
// Attack 5: Large-coordinate / large-radius scale — relative correctness +
// absolute on-surface oracle breakpoint characterization.
//
// MEASURED (256-sample sweep; apex origin +z, α=π/4, h0=0, r_c = r_s/√2):
//   r_s=1e3 : maxres ~1.1e-13  — HOLDS
//   r_s=1e6 : maxres ~1.2e-10  — HOLDS
//   r_s=1e7 : maxres ~1.9e-9   — HOLDS
//   r_s=1e8 : maxres ~1.5e-8   — HOLDS (just under TAU_MODEL=1e-7)
//   r_s=1e9 : maxres ~1.2e-7   — BREAKS (just over TAU_MODEL)
//   r_s=1e10: maxres ~1.9e-6   — BREAKS
// So for THIS pair the absolute oracle holds through ~1e8 and first breaks at
// ~1e9 (tighter than the cylinder pair's ~1e9→1e10, because the cone radial
// residual sees the full coordinate magnitude). Relative residual stays ~1e-16
// throughout. Lock both halves.
// ===========================================================================

#[test]
fn attack5_large_scale_relative_holds() {
    // r_s = 1e6: both relative correctness and the absolute oracle hold.
    let alpha = std::f64::consts::FRAC_PI_4;
    let r_s = 1.0e6;
    let sphere = sphere_at([0.0, 0.0, 0.0], r_s);
    let cone = z_cone([0.0, 0.0, 0.0], alpha);
    let curves = intersect(&sphere, &cone).expect("large-scale X2");
    assert_eq!(curves.len(), 2);
    // h0 = 0, α = π/4 ⇒ h = ±r_s·cos²α·secα = ±r_s·cosα = ±r_s/√2.
    let h = r_s / 2.0_f64.sqrt();
    for c in &curves {
        assert_curve_finite(c);
        let (center, _, radius) = circle_fields(c);
        // radius = |h|·tanα = h (tanα = 1). Relative check.
        assert!(
            (radius - h).abs() / h < 1e-12,
            "relative radius off: {radius} vs {h}"
        );
        assert!(
            (center[2].abs() - h).abs() / h < 1e-12,
            "relative center.z off"
        );
        let m = max_residual_on_both(c, &sphere, &cone, 256);
        assert!(
            m / r_s < 1e-9,
            "large-scale relative residual {} too big",
            m / r_s
        );
        assert!(
            m < TAU_MODEL,
            "r=1e6: absolute oracle unexpectedly broke ({m})"
        );
    }
}

#[test]
fn attack5_absolute_oracle_breakpoint_characterization() {
    // CHARACTERIZATION: the absolute on-surface oracle. r_s=1e8 HOLDS (just
    // under TAU); r_s=1e9 BREAKS (just over) but the solver stays analytically
    // correct (relative residual ~1e-16). This is the absolute-tolerance ceiling,
    // NOT a logic bug — exactly the SSI6 attack3 pattern, tightened to ~1e8→1e9
    // for the cone pair.
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone([0.0, 0.0, 0.0], alpha);

    // r_s = 1e8 ⇒ absolute oracle still holds.
    {
        let r_s = 1.0e8;
        let sphere = sphere_at([0.0, 0.0, 0.0], r_s);
        let curves = intersect(&sphere, &cone).unwrap();
        let m = max_residual_on_both(&curves[0], &sphere, &cone, 256);
        assert!(
            m < TAU_MODEL,
            "r=1e8: absolute oracle unexpectedly broke ({m}); breakpoint moved"
        );
    }

    // r_s = 1e9 ⇒ absolute oracle breaks, but relative residual is tiny.
    {
        let r_s = 1.0e9;
        let sphere = sphere_at([0.0, 0.0, 0.0], r_s);
        let curves = intersect(&sphere, &cone).unwrap();
        assert_curve_finite(&curves[0]);
        let m = max_residual_on_both(&curves[0], &sphere, &cone, 256);
        assert!(
            m >= TAU_MODEL,
            "r=1e9: absolute oracle unexpectedly held ({m}); breakpoint moved"
        );
        assert!(
            m / r_s < 1e-9,
            "r=1e9: relative residual {} too big",
            m / r_s
        );
    }
}

// ===========================================================================
// Attack 6: I4 symmetry + I5 determinism under argument permutation.
//
// intersect(sphere,cone) == intersect(cone,sphere) as a circle SET across
// X2/X1/X0/NC. X2 +√D-first order byte-identical across repeats.
// ===========================================================================

fn circle_key(c: &SsiCurve) -> (i64, i64, i64, i64, i64, i64, i64) {
    let (center, normal, radius) = circle_fields(c);
    let n = unit(normal);
    // Canonicalize normal sign into the +z hemisphere (then +y, then +x).
    let s = if n[2] > 1e-9 {
        1.0
    } else if n[2] < -1e-9 {
        -1.0
    } else if n[1] >= 0.0 {
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
fn attack6_symmetry_all_branches() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone([0.0, 0.0, 0.0], alpha);

    // X2: two circles. As a SET (order/sign tolerant) ab == ba.
    let sphere_x2 = sphere_at([0.0, 0.0, 0.0], 2.0);
    let ab = intersect(&sphere_x2, &cone).unwrap();
    let ba = intersect(&cone, &sphere_x2).unwrap();
    assert_eq!(ab.len(), 2);
    assert_eq!(ba.len(), 2);
    let mut abk: Vec<_> = ab.iter().map(circle_key).collect();
    let mut bak: Vec<_> = ba.iter().map(circle_key).collect();
    abk.sort();
    bak.sort();
    assert_eq!(abk, bak, "X2 circle set must match across argument order");

    // X1: one circle. ab == ba as a set. C=(0,0,2), r_s=√2 ⇒ tangent.
    let sphere_x1 = sphere_at([0.0, 0.0, 2.0], 2.0_f64.sqrt());
    let ab1 = intersect(&sphere_x1, &cone).unwrap();
    let ba1 = intersect(&cone, &sphere_x1).unwrap();
    assert_eq!(ab1.len(), 1);
    assert_eq!(ba1.len(), 1);
    assert_eq!(
        circle_key(&ab1[0]),
        circle_key(&ba1[0]),
        "X1 circle must match across order"
    );

    // X0: empty both ways. C=(0,0,3), r_s=2 ⇒ g < 0.
    let sphere_x0 = sphere_at([0.0, 0.0, 3.0], 2.0);
    assert_eq!(intersect(&sphere_x0, &cone), Ok(vec![]));
    assert_eq!(intersect(&cone, &sphere_x0), Ok(vec![]));

    // NC: the canonical SurfacePair both ways (F10 contract; was ASNA).
    let sphere_nc = sphere_at([0.5, 0.0, 3.0], 2.0);
    let expected_nc = Ok(vec![SsiCurve::SurfacePair {
        a: cone,
        b: sphere_nc,
    }]);
    assert_eq!(intersect(&sphere_nc, &cone), expected_nc);
    assert_eq!(intersect(&cone, &sphere_nc), expected_nc);
}

#[test]
fn attack6_determinism_byte_identical_and_sqrt_d_first() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sphere = sphere_at([0.0, 0.0, 0.0], 2.0);
    let cone = z_cone([0.0, 0.0, 0.0], alpha);
    let h = 2.0_f64.sqrt(); // +√D root z

    let first = intersect(&sphere, &cone);
    for _ in 0..8 {
        let again = intersect(&sphere, &cone);
        assert_eq!(first, again, "X2 output not byte-identical across repeats");
    }
    let cf = first.expect("two circles");
    // +√D first ⇒ curves[0].center.z = +√2.
    let (c0, _, _) = circle_fields(&cf[0]);
    assert!(
        (c0[2] - h).abs() < TAU_MODEL,
        "first center.z {} != +√2 {h}",
        c0[2]
    );

    // The argument-swapped call is itself deterministic (byte-identical to its
    // own repeat), even though its order may differ from the unswapped call.
    let swapped = intersect(&cone, &sphere).expect("swapped two circles");
    let swapped2 = intersect(&cone, &sphere).expect("swapped two circles");
    assert_eq!(swapped, swapped2, "swapped call not deterministic");
}

// ===========================================================================
// Attack 7: Apex-grazing degeneracy r_s = |h0| (sphere passes through apex).
//
// apex=origin, axis=+z, α=π/4, C=(0,0,2), r_s=2 ⇒ h0=2, r_s=|h0| ⇒ constant
// term h0²−r_s²=0 ⇒ one root h=0. g = r_s − |h0|·sinα = 2 − √2 ≈ 0.586 > TAU,
// so this is X2 (NOT the X1 tangent branch). The X2 formula emits TWO circles:
// the +√D root is a proper circle (z=2, radius 2), and the −√D root is h=0 ⇒ a
// radius-0 circle AT THE APEX (a degenerate point).
//
// CHARACTERIZATION: per the spec's Characterization note, this is the documented
// at-boundary degeneracy of the reduction (the formula emits it verbatim), NOT a
// √(negative)/NaN bug. We assert the ACTUAL emitted behavior: two finite
// circles, one with radius ≈ 0 centered at the apex. This is NOT judged a
// correctness defect — it is a true zero-radius circle (a point) that genuinely
// lies on both surfaces (the apex is on the cone, and |apex − C| = |h0| = r_s
// is on the sphere). yang-rs/kernel-v2 can filter degenerate point-circles.
// ===========================================================================

#[test]
fn attack7_apex_grazing_emits_proper_plus_radius_zero_circle() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let apex = [0.0, 0.0, 0.0];
    let r_s = 2.0_f64;
    let h0 = 2.0_f64;
    let sphere = sphere_at([0.0, 0.0, h0], r_s); // r_s = |h0| ⇒ apex-grazing
    let cone = z_cone(apex, alpha);

    let g = r_s - h0.abs() * alpha.sin();
    assert!(
        g > TAU_MODEL,
        "sanity: apex-grazing here is X2, not X1 (g={g})"
    );

    let curves = intersect(&sphere, &cone).expect("apex-grazing emits circles, not Err");
    assert_eq!(curves.len(), 2, "apex-grazing X2 emits two circles");

    // Both fields finite; radius-0 permitted for the degenerate root.
    for c in &curves {
        assert_curve_finite_allow_zero(c);
    }

    // +√D first ⇒ curves[0] is the proper circle: h = (h0+√D)·cos²α = 2,
    // radius = |2|·tanα = 2, center (0,0,2). curves[1] is the radius-0 circle at
    // the apex: h = (h0−√D)·cos²α = 0, center (0,0,0), radius 0.
    let (c0, n0, r0) = circle_fields(&curves[0]);
    let (c1, _n1, r1) = circle_fields(&curves[1]);

    assert!(
        (r0 - 2.0).abs() < TAU_MODEL,
        "proper circle radius {r0} != 2"
    );
    assert!(
        (c0[2] - 2.0).abs() < TAU_MODEL,
        "proper circle center.z {} != 2",
        c0[2]
    );
    parallel_up_to_sign(n0, [0.0, 0.0, 1.0]);
    assert!(
        r0 > 0.0,
        "first circle should be the proper (positive-radius) one"
    );

    // The degenerate root: a radius-0 circle (a point) at the apex.
    assert!(
        r1.abs() < 1e-9,
        "degenerate root should be radius-0 (apex point), got {r1}"
    );
    assert!(
        norm(sub(c1, apex)) < 1e-9,
        "degenerate circle center {c1:?} should be the apex"
    );

    // The proper circle genuinely lies on both surfaces. (We do NOT sample the
    // radius-0 circle on the cone oracle: every eval(t) returns the apex, which
    // is on the cone but the residual is trivially 0; the sphere residual at the
    // apex is | |apex − C| − r_s | = | |h0| − r_s | = 0 too, so it IS on both —
    // but the radius-0 circle is a degenerate point, characterized here.)
    assert_on_both_surfaces(&curves[0], &sphere, &cone);

    // Confirm the apex itself is on both surfaces (justifies it as a valid,
    // if degenerate, intersection point — not a spurious artifact).
    assert!(
        implicit_residual(&sphere, apex) < TAU_MODEL,
        "apex must be on the sphere (sphere passes through it)"
    );
    assert!(
        implicit_residual(&cone, apex) < TAU_MODEL,
        "apex must be on the cone (it is the cone's apex)"
    );
}

// ===========================================================================
// Attack 8: Reversed / non-unit axis_dir (defensive normalization, mirrors
// ssi6_adversary attack4).
//
// axis_dir magnitude ≠ 1 and reversed (−â): results identical (up to circle
// order / normal sign) to the unit case; normal is unit. Centers as a SET
// unchanged.
// ===========================================================================

#[test]
fn attack8_nonunit_and_reversed_axis() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let r_s = 2.0;
    let sphere = sphere_at([0.0, 0.0, 0.0], r_s);
    let h = 2.0_f64.sqrt(); // |h| for h0=0, α=π/4: r_s·cosα = √2

    // Reference: unit +z axis.
    let ref_curves = intersect(&sphere, &z_cone([0.0, 0.0, 0.0], alpha)).unwrap();
    assert_eq!(ref_curves.len(), 2);

    // (a) Magnitude-7 +z axis.
    let cone_big = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 7.0),
        half_angle: alpha,
    };
    // (b) Reversed (magnitude-7 −z) axis.
    let cone_rev = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, -7.0),
        half_angle: alpha,
    };

    for (label, cone) in [("big", &cone_big), ("rev", &cone_rev)] {
        let curves = intersect(&sphere, cone).expect("non-unit/reversed axis");
        assert_eq!(curves.len(), 2, "{label}: expected two circles");
        // Centers as a SET must be {(0,0,+h),(0,0,−h)} regardless of axis sign.
        let mut centers: Vec<[f64; 3]> = curves.iter().map(|c| circle_fields(c).0).collect();
        centers.sort_by(|a, b| a[2].partial_cmp(&b[2]).unwrap());
        assert!(
            norm(sub(centers[0], [0.0, 0.0, -h])) < TAU_MODEL,
            "{label}: low center {:?}",
            centers[0]
        );
        assert!(
            norm(sub(centers[1], [0.0, 0.0, h])) < TAU_MODEL,
            "{label}: high center {:?}",
            centers[1]
        );
        for c in &curves {
            let (_, normal, radius) = circle_fields(c);
            assert!(
                (radius - h).abs() < TAU_MODEL,
                "{label}: radius {radius} != {h}"
            );
            assert!(
                (norm(normal) - 1.0).abs() < TAU_MODEL,
                "{label}: normal not unit"
            );
            parallel_up_to_sign(normal, [0.0, 0.0, 1.0]);
            assert_on_both_surfaces(c, &sphere, cone);
        }
    }
}

#[test]
fn attack8_oblique_reversed_axis_off_origin() {
    // Oblique reversed axis with the apex off-origin and the sphere center ON
    // the axis line. â = −(1,2,2)/3 supplied non-unit. Coaxial, h0 = 2.5 along
    // the SUPPLIED (reversed) direction. Centers must lie on the axis line and
    // on both surfaces; normal unit.
    let alpha = std::f64::consts::FRAC_PI_4;
    let ahat_supplied = unit([-1.0, -2.0, -2.0]);
    let apex = [3.0, -1.0, 4.0];
    // Sphere center on the axis line through apex along the supplied direction.
    let c_center = add(apex, scale(ahat_supplied, 2.5));
    let r_s = 3.0;
    let sphere = sphere_at(c_center, r_s);
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::new(-3.0, -6.0, -6.0), // non-unit, reversed-oblique
        half_angle: alpha,
    };
    let curves = intersect(&sphere, &cone).expect("coaxial oblique reversed axis");
    assert_eq!(curves.len(), 2);
    for c in &curves {
        assert_curve_finite(c);
        let (center, normal, _radius) = circle_fields(c);
        assert!(
            (norm(normal) - 1.0).abs() < TAU_MODEL,
            "normal not unit: {normal:?}"
        );
        parallel_up_to_sign(normal, ahat_supplied);
        // center on the axis line: perp distance ≈ 0.
        assert!(
            dist_to_axis(center, apex, ahat_supplied) < TAU_MODEL,
            "center off axis line (perp {})",
            dist_to_axis(center, apex, ahat_supplied)
        );
        assert_on_both_surfaces(c, &sphere, &cone);
    }
}
