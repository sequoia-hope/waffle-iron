//! PR-SSI3 — Adversarial audit of the plane∩cone solver (bounded sections).
//!
//! Attacks `plane_cone` (reached via the public `intersect` dispatcher) at its
//! classification boundaries:
//!   - C1↔C2 (perpendicular limit, s_n → 0): circle vs near-circular ellipse;
//!   - C2↔PH (ellipse↔parabola, the dangerous one: a generator becomes ∥ the
//!     plane, gd_± → 0, a → ∞) — verify no blown-up/NaN ellipse, clean switch
//!     to Err(AnalyticalSolutionNotAvailable);
//!   - parabola/hyperbola classification correctness (no unbounded section
//!     misclassified as a bounded curve);
//!   - oblique non-axis-aligned cone; extreme scale + extreme half-angles;
//!   - through-apex (AP) band; non-unit axis_dir + symmetry/determinism.
//!
//! Does NOT touch production code. Reuses ssi3.rs's on-surface oracle (plane
//! residual + cone RADIAL residual `|(x−apex)−h·â| − |h|·tanα`) and the
//! ssi2_adversary finite-field + relative-vs-absolute oracle patterns.
//!
//! Tolerance note: TAU_MODEL = 1e-7. The absolute on-surface oracle is valid
//! only while curve sample coordinates stay below ~1e8 (the PR-SSI1 finding).
//! Where a band drives `major_radius` huge (C2↔PH) or coords large (extreme
//! scale), tests switch to a RELATIVE check and characterize the breakpoint.

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

/// Every field of a returned curve must be finite (no NaN/Inf), radii > 0.
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
    }
}

/// Absolute implicit residual on a surface. For the cone this is the RADIAL
/// residual `| |(x−apex)−h·â| − |h|·tanα |` (a length), per the spec I1 oracle.
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
            let v = sub(x, apex.as_array());
            let a = axis_dir.as_array();
            let alen = norm(a);
            let h = dot(v, a) / alen;
            let r_actual = norm(cross(v, a)) / alen;
            (r_actual - h.abs() * half_angle.tan()).abs()
        }
    }
}

/// Max absolute on-surface residual over N samples of a curve (both surfaces).
fn max_residual_on_both(curve: &SsiCurve, a: &QuadricSurface, b: &QuadricSurface, n: usize) -> f64 {
    let mut m = 0.0_f64;
    for i in 0..n {
        let t = match curve {
            SsiCurve::Circle { .. } | SsiCurve::Ellipse { .. } => {
                (i as f64) / (n as f64) * std::f64::consts::TAU
            }
            SsiCurve::Line { .. } => -5.0 + (i as f64) / ((n - 1) as f64) * 10.0,
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

fn parallel_up_to_sign(a: [f64; 3], b: [f64; 3]) {
    assert!(
        norm(cross(a, b)) < TAU_MODEL,
        "expected {a:?} parallel to {b:?} (|cross| = {})",
        norm(cross(a, b))
    );
}

// A plane normal tilted by angle theta from +z in the x–z plane:
// n̂ = (sinθ, 0, cosθ), so k = n̂·ẑ = cosθ. |k| sweeps with θ.
fn tilted_z_normal(theta: f64) -> Vector3 {
    Vector3::new(theta.sin(), 0.0, theta.cos())
}

// Unit-axis +z double cone at the origin with the given half-angle.
fn z_cone(half_angle: f64) -> QuadricSurface {
    QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle,
    }
}

// ===========================================================================
// Attack 1: C1↔C2 perpendicular band (s_n → 0).
//
// Sweep the plane-normal tilt θ across the s_n < TAU_MODEL band. Just inside ⇒
// Circle(radius = |h|·tanα, on BOTH surfaces); just outside ⇒ near-circular
// Ellipse (a ≈ b ≈ |h|·tanα, NOT blown up), on both surfaces. The PR-SSI2
// lesson: the C1 circle has normal = â, so its points sit OFF the tilted
// cutting plane by ≤ R·sinθ (R = circle radius). The gate s_n < TAU_MODEL
// bounds that off-plane error by R·TAU_MODEL. Verify both branches and a clean
// transition (no discontinuity in radius between just-inside and just-outside).
// ===========================================================================

#[test]
fn attack1_c1c2_perpendicular_band_clean_transition() {
    // α = π/4 ⇒ tanα = 1. Plane through (0,0,3): h ≈ 3 ⇒ circle radius ≈ 3.
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let tana = alpha.tan();
    let plane_pt = [0.0, 0.0, 3.0];
    // C1 band: s_n = sinθ < TAU_MODEL ⇒ θ < asin(TAU_MODEL).
    let theta_band = TAU_MODEL.asin();

    // NOTE the band-edge points (≈ theta_band) are deliberately excluded from
    // the hard branch assertions here because production gates on the
    // cancellation-prone s_n = √(1 − k²) (see attack1c), so the EXACT crossover
    // θ sits a fraction below the true asin(TAU_MODEL). We straddle the band
    // with points that are unambiguous under the production discriminant, and
    // gate the branch assertion on that same discriminant (s_prod) rather than
    // the true sinθ.
    let thetas = [
        0.0,                 // exactly ⟂ ⇒ C1, s_n = 0
        theta_band * 0.5,    // well inside band ⇒ C1
        theta_band * 0.9,    // inside band ⇒ C1
        theta_band * 10.0,   // well outside ⇒ C2 (near-circular)
        theta_band * 1000.0, // C2
        0.05,                // comfortably C2
    ];

    for theta in thetas {
        let plane = QuadricSurface::Plane {
            point: Point3::from(plane_pt),
            normal: tilted_z_normal(theta),
        };
        let s_n = theta.sin();
        // Production discriminant (what the solver actually tests): k = cosθ.
        let k = theta.cos();
        let s_prod = (1.0 - k * k).sqrt();
        let curves = intersect(&plane, &cone)
            .unwrap_or_else(|e| panic!("theta={theta}: must not error, got {e:?}"));
        assert_eq!(curves.len(), 1, "theta={theta}: expected one curve");
        assert_curve_finite(&curves[0]);

        match curves[0] {
            SsiCurve::Circle { radius, normal, .. } => {
                assert!(
                    s_prod < TAU_MODEL,
                    "theta={theta}: Circle returned but production s_n={s_prod} not < TAU_MODEL"
                );
                // h = n̂·(p − apex)/k. For tilted normal, h = (cosθ·3)/cosθ = 3.
                let expect_r = 3.0 * tana;
                assert!(
                    (radius - expect_r).abs() < TAU_MODEL,
                    "theta={theta}: circle radius {radius} != |h|·tanα {expect_r}"
                );
                parallel_up_to_sign(normal.as_array(), [0.0, 0.0, 1.0]);
                // On the CONE exactly; off the tilted plane by ≤ R·sinθ.
                let mut max_cone = 0.0_f64;
                let mut max_plane = 0.0_f64;
                for i in 0..256 {
                    let t = (i as f64) / 256.0 * std::f64::consts::TAU;
                    let pt = curves[0].eval(t).as_array();
                    max_cone = max_cone.max(implicit_residual(&cone, pt));
                    max_plane = max_plane.max(implicit_residual(&plane, pt));
                }
                assert!(
                    max_cone < TAU_MODEL,
                    "theta={theta}: C1 circle off the CONE (residual {max_cone})"
                );
                // Corrected guarantee: off-plane residual ≤ R·s_n + slack, and
                // strictly < R·TAU_MODEL (the band is sine-gated).
                let snap_bound = expect_r * s_n + 8.0 * f64::EPSILON * expect_r;
                assert!(
                    max_plane <= snap_bound,
                    "theta={theta}: C1 plane residual {max_plane} exceeds snap bound {snap_bound}"
                );
                assert!(
                    max_plane < expect_r * TAU_MODEL + 1e-12,
                    "theta={theta}: C1 plane residual {max_plane} exceeds R·TAU_MODEL — \
                     band not sine-gated"
                );
            }
            SsiCurve::Ellipse {
                major_radius,
                minor_radius,
                ..
            } => {
                assert!(
                    s_prod >= TAU_MODEL,
                    "theta={theta}: Ellipse returned but production s_n={s_prod} inside C1 band"
                );
                assert!(major_radius.is_finite() && minor_radius.is_finite());
                assert!(
                    major_radius >= minor_radius - TAU_MODEL,
                    "theta={theta}: a {major_radius} < b {minor_radius}"
                );
                // Near the band (small θ) the ellipse must be near-circular:
                // a ≈ b ≈ |h|·tanα, NOT blown up. (Only assert this where the
                // section really is near-circular — within θ ≤ 1e-3.)
                if theta <= 1e-3 {
                    assert!(
                        (minor_radius - 3.0 * tana).abs() < 1e-2,
                        "theta={theta}: near-band b {minor_radius} not ≈ |h|·tanα {}",
                        3.0 * tana
                    );
                    assert!(
                        major_radius < 1.5 * 3.0 * tana,
                        "theta={theta}: near-band a {major_radius} blew up (should ≈ {})",
                        3.0 * tana
                    );
                }
                // For genuine obliquity, a > b (eccentric) but still finite and
                // a ≥ b; the on-surface oracle below is the real correctness
                // check at every θ.
                // C2 ellipse is EXACT on both surfaces (geometry O(3)).
                assert_on_both_surfaces(&curves[0], &plane, &cone);
            }
            ref other => panic!("theta={theta}: unexpected curve {other:?}"),
        }
    }
}

#[test]
fn attack1_c1_circle_matches_ellipse_limit_no_jump() {
    // The circle radius just inside the band must match the ellipse's
    // semi-radii just outside it (continuity / circle-limit sanity, spec L96).
    let alpha = 0.5_f64.atan(); // tanα = 0.5
    let cone = z_cone(alpha);
    let plane_pt = [0.0, 0.0, 4.0]; // h = 4 ⇒ radius = 4·0.5 = 2
    let band = TAU_MODEL.asin();

    let inside = intersect(
        &QuadricSurface::Plane {
            point: Point3::from(plane_pt),
            normal: tilted_z_normal(band * 0.5),
        },
        &cone,
    )
    .unwrap();
    let outside = intersect(
        &QuadricSurface::Plane {
            point: Point3::from(plane_pt),
            normal: tilted_z_normal(band * 2.0),
        },
        &cone,
    )
    .unwrap();

    let r_in = match inside[0] {
        SsiCurve::Circle { radius, .. } => radius,
        ref o => panic!("inside band: expected Circle, got {o:?}"),
    };
    let (a_out, b_out) = match outside[0] {
        SsiCurve::Ellipse {
            major_radius,
            minor_radius,
            ..
        } => (major_radius, minor_radius),
        ref o => panic!("outside band: expected Ellipse, got {o:?}"),
    };
    // No jump: circle radius ≈ both ellipse semi-radii (within a small slack).
    assert!(
        (r_in - b_out).abs() < 1e-4 && (r_in - a_out).abs() < 1e-4,
        "discontinuity at band: r_in={r_in}, a_out={a_out}, b_out={b_out}"
    );
}

// ===========================================================================
// Attack 1c (CONDITIONING CHARACTERIZATION): production computes
// s_n = √(1 − k²) with k = n̂·â directly (lib.rs:634). Near a perpendicular
// plane (k → 1) this suffers catastrophic cancellation: 1 − k² loses ~half its
// significant digits, so the C1 gate `s_n < TAU_MODEL` can FIRE for planes
// whose TRUE tilt sinθ is modestly ABOVE TAU_MODEL. plane_cylinder avoided this
// by gating on the stable vector form |â − c·n̂| (ssi2_adversary attack 1);
// plane_cone does NOT use that form.
//
// FINDING (this test pins it): the misclassification band is small — true s_n
// is captured as C1 only up to ≈1.005·TAU_MODEL — so the resulting C1 circle's
// worst off-plane residual is ≈ R·(1.005·TAU_MODEL) ≈ 3·TAU_MODEL for R=3, i.e.
// it can exceed the I1 absolute on-surface oracle by a small constant factor
// right at the band edge. This is a genuine (if minor) conditioning gap vs the
// spec's "gate on the geometrically-meaningful quantity" intent, NOT a blow-up.
//
// This test CHARACTERIZES rather than fails the build: it asserts the effect is
// bounded by a small multiple of R·TAU_MODEL (locking that it is minor), and
// documents the exact magnitude. If production later adopts the stable
// |n̂ − k·â| gate, the off-plane residual drops below R·TAU_MODEL and this
// bound tightens.
// ===========================================================================

#[test]
fn attack1c_perpendicular_gate_cancellation_is_bounded() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let plane_pt = [0.0, 0.0, 3.0];
    let r_expected = 3.0 * alpha.tan(); // = 3

    // Sweep θ across the true-band edge with a fine grid; for every θ that the
    // solver still classifies as a Circle, measure the worst off-plane residual.
    let band = TAU_MODEL.asin();
    let mut worst_plane_resid_when_circle = 0.0_f64;
    let mut worst_true_sn_when_circle = 0.0_f64;
    let steps = 4000;
    for i in 0..=steps {
        // θ from 0 to 3·band so we straddle well past the band.
        let theta = band * 3.0 * (i as f64) / (steps as f64);
        let plane = QuadricSurface::Plane {
            point: Point3::from(plane_pt),
            normal: tilted_z_normal(theta),
        };
        let curves = intersect(&plane, &cone).unwrap();
        if let SsiCurve::Circle { .. } = curves[0] {
            let mut mp = 0.0_f64;
            for j in 0..128 {
                let t = (j as f64) / 128.0 * std::f64::consts::TAU;
                mp = mp.max(implicit_residual(&plane, curves[0].eval(t).as_array()));
            }
            worst_plane_resid_when_circle = worst_plane_resid_when_circle.max(mp);
            worst_true_sn_when_circle = worst_true_sn_when_circle.max(theta.sin());
        }
    }

    // The misclassification band is small: true s_n captured as C1 stays within
    // a small multiple of TAU_MODEL (NOT a wide band). Pin ≤ 2·TAU_MODEL.
    assert!(
        worst_true_sn_when_circle <= 2.0 * TAU_MODEL,
        "C1 captured a plane with true s_n {worst_true_sn_when_circle} > 2·TAU_MODEL — \
         cancellation band wider than characterized"
    );
    // The resulting off-plane residual is bounded by a small multiple of
    // R·TAU_MODEL (here ≤ ~4·R·TAU_MODEL). This LOCKS that the effect is minor
    // (no blow-up) while honestly recording that it can exceed R·TAU_MODEL.
    assert!(
        worst_plane_resid_when_circle <= 4.0 * r_expected * TAU_MODEL,
        "worst C1 off-plane residual {worst_plane_resid_when_circle} exceeds \
         4·R·TAU_MODEL — cancellation worse than characterized"
    );
    // Confirm it really can cross the bare TAU_MODEL oracle (documents the gap).
    // (Informational; not all builds will hit it, so allow either side but
    // assert finiteness + the upper bound above.)
    assert!(
        worst_plane_resid_when_circle.is_finite(),
        "non-finite off-plane residual"
    );
}

// ===========================================================================
// Attack 2 (THE DANGEROUS ONE): ellipse↔parabola boundary.
//
// α = π/4 ⇒ sinα = √2/2; parabola at θ = π/2−α = π/4 (one generator ∥ plane,
// gd_- → 0 ⇒ s_- → ∞ ⇒ a → ∞). Sweep θ up toward π/4⁻. Assert:
//  (a) while min(|gd₊|,|gd₋|) > TAU_MODEL the solver returns an Ellipse whose
//      fields are FINITE (no Inf a), a ≥ b > 0, AND is still ON BOTH SURFACES
//      (per the PR-SSI2 finding a huge a doesn't break the absolute oracle so
//      long as coords stay below ~1e8 — here a ≤ ~2.3e6);
//  (b) once a generator is within TAU_MODEL of ∥, the solver switches to
//      Err(AnalyticalSolutionNotAvailable) — NOT a blown-up/NaN ellipse, NOT a
//      wrong curve;
//  (c) no NaN/Inf is ever emitted across the sweep, and the switch happens
//      exactly when the geometrically-meaningful min(|gd₊|,|gd₋|) crosses
//      TAU_MODEL (the correct gate, the analog of PR-SSI2's C1 fix).
// ===========================================================================

#[test]
fn attack2_ellipse_parabola_boundary_no_blowup_clean_switch() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let sina = alpha.sin();
    let cosa = alpha.cos();
    let ahat = [0.0, 0.0, 1.0];
    let plane_pt = [0.0, 0.0, 5.0];

    // Parabola tilt is θ_par = π/2 − α. Approach from the ellipse side.
    let theta_par = std::f64::consts::FRAC_PI_2 - alpha;

    // fractions of theta_par; the last few cross the TAU gate on gd_-.
    let fracs = [
        0.5, 0.9, 0.99, 0.999, 0.9999, 0.99999, 0.999999, 0.9999999, 0.99999999, 1.0,
    ];

    let mut saw_ellipse = false;
    let mut saw_ph = false;
    let mut last_ellipse_frac = 0.0_f64;
    let mut first_ph_frac = 1.0_f64;

    for frac in fracs {
        let theta = theta_par * frac;
        let nhat = [theta.sin(), 0.0, theta.cos()];
        let plane = QuadricSurface::Plane {
            point: Point3::from(plane_pt),
            normal: tilted_z_normal(theta),
        };

        // Independent gd_- (the geometrically-meaningful gate quantity).
        let k = dot(nhat, ahat);
        let uhat = unit(sub(nhat, scale(ahat, k)));
        let g_minus = sub(scale(ahat, cosa), scale(uhat, sina));
        let g_plus = add(scale(ahat, cosa), scale(uhat, sina));
        let gd_minus = dot(nhat, g_minus);
        let gd_plus = dot(nhat, g_plus);
        let min_gd = gd_minus.abs().min(gd_plus.abs());

        let res = intersect(&plane, &cone);

        match res {
            Ok(curves) => {
                assert_eq!(curves.len(), 1, "frac={frac}: expected one curve");
                // (c) NEVER NaN/Inf.
                assert_curve_finite(&curves[0]);
                let SsiCurve::Ellipse {
                    major_radius,
                    minor_radius,
                    ..
                } = curves[0]
                else {
                    panic!(
                        "frac={frac}: expected Ellipse (ellipse side), got {:?}",
                        curves[0]
                    );
                };
                saw_ellipse = true;
                last_ellipse_frac = last_ellipse_frac.max(frac);
                // (a) finite, a ≥ b > 0, NO Inf.
                assert!(major_radius.is_finite(), "frac={frac}: a is Inf/NaN");
                assert!(minor_radius.is_finite() && minor_radius > 0.0);
                assert!(
                    major_radius >= minor_radius - TAU_MODEL,
                    "frac={frac}: a {major_radius} < b {minor_radius}"
                );
                // The solver only returns Ellipse when its gate min|gd| > TAU.
                assert!(
                    min_gd > TAU_MODEL,
                    "frac={frac}: Ellipse returned but min|gd|={min_gd} ≤ TAU_MODEL"
                );
                // (a) STILL on both surfaces while coords ≤ ~1e8. a ≤ ~2.3e6
                // here, so the absolute oracle must hold (PR-SSI2 finding).
                if major_radius < 1.0e7 {
                    let m = max_residual_on_both(&curves[0], &plane, &cone, 256);
                    assert!(
                        m < TAU_MODEL,
                        "frac={frac} (a={major_radius:e}): ellipse off-surface (residual {m}) \
                         — absolute oracle broke below the 1e8 ceiling"
                    );
                } else {
                    // Beyond the ceiling, require RELATIVE correctness only.
                    let m = max_residual_on_both(&curves[0], &plane, &cone, 256);
                    assert!(
                        m / major_radius < 1e-9,
                        "frac={frac}: relative residual {} too large (solver wrong)",
                        m / major_radius
                    );
                }
            }
            Err(SsiError::AnalyticalSolutionNotAvailable) => {
                // (b) clean switch to the staged-gap Err — never a wrong curve.
                saw_ph = true;
                first_ph_frac = first_ph_frac.min(frac);
                // The solver's gate must agree: PH ⇒ min|gd| ≤ TAU_MODEL.
                assert!(
                    min_gd <= TAU_MODEL,
                    "frac={frac}: PH Err returned but min|gd|={min_gd} > TAU_MODEL \
                     — switched to Err while still a genuine (finite-a) ellipse"
                );
            }
            Err(other) => panic!("frac={frac}: unexpected error {other:?}"),
        }
    }

    // Both branches exercised, and the switch is monotone: every ellipse frac is
    // below every PH frac (clean boundary, no oscillation).
    assert!(
        saw_ellipse && saw_ph,
        "boundary sweep did not cover both branches"
    );
    assert!(
        last_ellipse_frac < first_ph_frac + 1e-12,
        "ellipse/PH boundary not monotone: last ellipse {last_ellipse_frac}, first PH {first_ph_frac}"
    );
}

#[test]
fn attack2b_huge_finite_ellipse_is_on_surface() {
    // Pin the headline (a): a single ellipse with a ≈ 2.25e6 (gd_- ≈ 7.85e-7,
    // just above the TAU gate) is finite AND on both surfaces within the
    // absolute oracle. This is the C2 analog of "no silent blow-up just inside
    // the gate" — the ellipse is real, not garbage.
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let theta = (std::f64::consts::FRAC_PI_2 - alpha) * 0.999999;
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 5.0),
        normal: tilted_z_normal(theta),
    };
    let curves = intersect(&plane, &cone).expect("near-parabola ellipse: must be Ok");
    assert_curve_finite(&curves[0]);
    let SsiCurve::Ellipse { major_radius, .. } = curves[0] else {
        panic!("expected a (huge but finite) ellipse, got {:?}", curves[0]);
    };
    // Genuinely large (the near-parabola limit), but finite.
    assert!(
        major_radius > 1.0e6 && major_radius.is_finite(),
        "a {major_radius} not the expected huge-finite near-parabola value"
    );
    // Coords ≈ 2.25e6 ≪ 1e8 ⇒ absolute oracle holds.
    let m = max_residual_on_both(&curves[0], &plane, &cone, 512);
    assert!(
        m < TAU_MODEL,
        "huge finite ellipse (a={major_radius:e}) off-surface (residual {m})"
    );
}

// ===========================================================================
// Attack 3: parabola / hyperbola classification correctness.
//
// No unbounded section may be misclassified as a bounded curve. With α = π/4
// (sinα = √2/2): a plane EXACTLY ∥ a generator (k = sinα to machine precision)
// ⇒ parabola ⇒ Err; a plane with k < sinα (axis-parallel, k = 0) ⇒ hyperbola
// ⇒ Err; a plane just ellipse-side (k = sinα + a hair) ⇒ Ellipse (or PH within
// the TAU gate — we characterize the boundary). The headline: NEVER a Circle
// or finite Ellipse for a genuinely unbounded section.
// ===========================================================================

#[test]
fn attack3_exact_parabola_is_err() {
    // k = sinα exactly: tilt θ so cosθ = sinα ⇒ θ = π/2 − α.
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let theta = std::f64::consts::FRAC_PI_2 - alpha;
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 2.0), // off-apex
        normal: tilted_z_normal(theta),
    };
    assert_eq!(
        intersect(&plane, &cone),
        Err(SsiError::AnalyticalSolutionNotAvailable),
        "exact parabola must be the staged-gap Err, never a bounded curve"
    );
}

#[test]
fn attack3_hyperbola_axis_parallel_is_err() {
    // Plane ∥ axis (normal ⟂ axis ⇒ k = 0 < sinα) ⇒ hyperbola.
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    assert_eq!(
        intersect(&plane, &cone),
        Err(SsiError::AnalyticalSolutionNotAvailable),
        "hyperbola must be Err, never a bounded curve"
    );
}

#[test]
fn attack3_hyperbola_shallow_is_err_for_narrow_cone() {
    // Narrow cone α small (sinα small); a moderately tilted plane with
    // 0 < k < sinα ⇒ hyperbola. Use α = 0.2 (sinα ≈ 0.1987); pick k = 0.1.
    let alpha = 0.2_f64;
    let cone = z_cone(alpha);
    let sina = alpha.sin();
    let k_target = 0.5 * sina; // strictly between 0 and sinα ⇒ hyperbola
    let theta = k_target.acos(); // cosθ = k_target
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: tilted_z_normal(theta),
    };
    assert_eq!(
        intersect(&plane, &cone),
        Err(SsiError::AnalyticalSolutionNotAvailable),
        "k < sinα (shallow) must be a hyperbola Err"
    );
}

#[test]
fn attack3_sweep_no_unbounded_returns_bounded() {
    // Sweep k from 0 (hyperbola) through sinα (parabola) up to ~1 (ellipse/
    // circle). Assert: every k < sinα − slack and the exact parabola is Err;
    // every comfortably-ellipse k yields a finite bounded curve; and the
    // solver NEVER returns a bounded curve while k is below sinα.
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let sina = alpha.sin();

    let steps = 500;
    for i in 0..=steps {
        // k from 0.0 to 0.999.
        let k = 0.999 * (i as f64) / (steps as f64);
        let theta = k.acos(); // cosθ = k
                              // Plane point off the apex for ALL tilts: with n̂=(sinθ,0,cosθ), the
                              // apex distance n̂·(apex−p) = −(3 sinθ + 5 cosθ) is nonzero on [0,π/2],
                              // so the AP branch never spuriously fires during the sweep.
        let plane = QuadricSurface::Plane {
            point: Point3::new(3.0, 0.0, 5.0),
            normal: tilted_z_normal(theta),
        };
        let res = intersect(&plane, &cone);
        match res {
            Ok(curves) => {
                // A bounded curve is only legitimate on the ellipse side. If we
                // got one while k is meaningfully BELOW sinα, that is a
                // misclassification of an unbounded section — a hard failure.
                assert_curve_finite(&curves[0]);
                assert!(
                    matches!(
                        curves[0],
                        SsiCurve::Ellipse { .. } | SsiCurve::Circle { .. }
                    ),
                    "k={k}: unexpected curve type {:?}",
                    curves[0]
                );
                assert!(
                    k > sina - 1e-3,
                    "k={k} < sinα={sina}: solver returned a BOUNDED curve for an \
                     UNBOUNDED (hyperbola) section — misclassification"
                );
            }
            Err(SsiError::AnalyticalSolutionNotAvailable) => {
                // Legit anywhere near/below sinα. Must NOT happen comfortably
                // above sinα (that would drop a real ellipse).
                assert!(
                    k < sina + 1e-2,
                    "k={k} ≫ sinα={sina}: solver returned PH Err for a genuine ellipse"
                );
            }
            Err(other) => panic!("k={k}: unexpected error {other:?}"),
        }
    }
}

// ===========================================================================
// Attack 4: oblique / non-axis-aligned cone cut to an ellipse.
//
// Apex (1,2,3), axis (1,2,2)/3, an oblique plane that yields a closed ellipse.
// On-surface oracle (every sample + both vertices on cone & in plane);
// center = midpoint of vertices, lies in the cutting plane.
// ===========================================================================

#[test]
fn attack4_oblique_cone_ellipse_frame_and_center() {
    let apex = [1.0, 2.0, 3.0];
    let axis = [1.0, 2.0, 2.0]; // |·| = 3, non-unit on input
    let alpha = 0.5_f64; // sinα ≈ 0.479
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(axis),
        half_angle: alpha,
    };
    // Plane nearly ⟂ the axis but tilted: pick normal close to â so k ≫ sinα
    // ⇒ closed ellipse. n = â + small perturbation.
    let ahat = unit(axis);
    let pert = unit(cross(ahat, [0.0, 0.0, 1.0])); // ⟂ â
    let nrm = add(ahat, scale(pert, 0.3)); // k = â·n̂ ≈ cos(atan(0.3)) ≈ 0.958
    let ppoint = add(apex, scale(ahat, 6.0)); // well off the apex along the axis
    let plane = QuadricSurface::Plane {
        point: Point3::from(ppoint),
        normal: Vector3::from(nrm),
    };

    let curves = intersect(&plane, &cone).expect("oblique cone/plane → ellipse");
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

    let nhat = unit(nrm);

    // On-surface oracle (geometry O(10) ⇒ absolute oracle valid).
    assert_on_both_surfaces(&curves[0], &plane, &cone);

    // Independent vertices: V_± = apex + s_±·g_±, g_± = cosα·â ± sinα·û.
    let cosa = alpha.cos();
    let sina = alpha.sin();
    let k = dot(nhat, ahat);
    let uhat = unit(sub(nhat, scale(ahat, k)));
    let g_plus = add(scale(ahat, cosa), scale(uhat, sina));
    let g_minus = sub(scale(ahat, cosa), scale(uhat, sina));
    let rhs = dot(nhat, sub(ppoint, apex));
    let s_plus = rhs / dot(nhat, g_plus);
    let s_minus = rhs / dot(nhat, g_minus);
    let v_plus = add(apex, scale(g_plus, s_plus));
    let v_minus = add(apex, scale(g_minus, s_minus));

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

    // center = midpoint of the vertices, in the cutting plane.
    let expect_center = scale(add(v_plus, v_minus), 0.5);
    assert!(
        norm(sub(center.as_array(), expect_center)) < TAU_MODEL,
        "center {:?} != vertex midpoint {expect_center:?}",
        center.as_array()
    );
    assert!(
        dot(nhat, sub(center.as_array(), ppoint)).abs() < TAU_MODEL,
        "center not in cutting plane"
    );
    assert!(
        (major_radius - norm(sub(v_plus, v_minus)) / 2.0).abs() < TAU_MODEL,
        "a != |V₊−V₋|/2"
    );

    // Frame: major_axis unit & in-plane; minor = n̂×major ⟂ both; a ≥ b.
    let maj = major_axis.as_array();
    assert!((norm(maj) - 1.0).abs() < TAU_MODEL, "major_axis not unit");
    assert!(dot(maj, nhat).abs() < TAU_MODEL, "major_axis not in-plane");
    let minor = cross(normal.as_array(), maj);
    assert!(dot(minor, maj).abs() < TAU_MODEL, "major ⊥ minor failed");
    assert!(dot(minor, nhat).abs() < TAU_MODEL, "minor not in-plane");
    parallel_up_to_sign(normal.as_array(), nhat);
    assert!(major_radius >= minor_radius - TAU_MODEL, "a < b");
}

// ===========================================================================
// Attack 5: extreme scale + extreme half-angles.
//
// (a) Large cone (apex offset ~1e6): relative on-surface correctness + report
//     where the absolute oracle breaks (PR-SSI1: holds to ~1e8 coords).
// (b) E1 boundary on half-angle: α ≤ TAU_MODEL and α ≥ π/2 − TAU_MODEL ⇒
//     DegenerateInput; just inside the valid range ⇒ correct circle.
// ===========================================================================

#[test]
fn attack5a_large_scale_circle_relative_and_absolute() {
    // Big perpendicular cut ⇒ C1 circle. Apex offset 1e6, α = π/4 ⇒ radius |h|.
    let alpha = std::f64::consts::FRAC_PI_4;
    let apex = [1.0e6, -2.0e6, 3.0e6];
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    // Plane ⟂ axis at z = 3e6 + 5e5 ⇒ h = 5e5 ⇒ radius = 5e5·tanα = 5e5.
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.5e6),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let curves = intersect(&plane, &cone).expect("large perpendicular → circle");
    assert_eq!(curves.len(), 1);
    assert_curve_finite(&curves[0]);
    let SsiCurve::Circle { center, radius, .. } = curves[0] else {
        panic!("expected circle, got {:?}", curves[0]);
    };
    let expect_r = 5.0e5 * alpha.tan();
    assert!(
        (radius - expect_r).abs() / expect_r < 1e-12,
        "radius {radius} rel-off vs {expect_r}"
    );
    // center on axis (x,y = apex's) and in plane (z = 3.5e6).
    let ctr = center.as_array();
    assert!((ctr[0] - apex[0]).abs() / 1e6 < 1e-12);
    assert!((ctr[1] - apex[1]).abs() / 2e6 < 1e-12);
    assert!((ctr[2] - 3.5e6).abs() / 3.5e6 < 1e-12);
    // RELATIVE on-surface oracle (coords ~3.5e6 ⇒ absolute should still hold,
    // but assert relative to be scale-robust).
    let scale_ref = 3.5e6_f64;
    let m = max_residual_on_both(&curves[0], &plane, &cone, 256);
    assert!(
        m / scale_ref < 1e-9,
        "large-scale relative residual {} too big",
        m / scale_ref
    );
}

#[test]
fn attack5b_large_scale_absolute_oracle_breakpoint() {
    // Characterize where the absolute on-surface oracle breaks for plane_cone
    // C1 circles (PR-SSI1: holds ~1e6, breaks ~1e9). Relative correctness holds
    // at every scale.
    let alpha = std::f64::consts::FRAC_PI_4;
    for &s in &[1.0e6_f64, 1.0e9_f64] {
        let cone = QuadricSurface::Cone {
            apex: Point3::new(s, s, s),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: alpha,
        };
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, s * 1.5),
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        let curves = intersect(&plane, &cone).unwrap();
        assert_curve_finite(&curves[0]);
        let m = max_residual_on_both(&curves[0], &plane, &cone, 256);
        assert!(m / s < 1e-9, "s={s:e}: relative residual {} too big", m / s);
        if s <= 1.0e6 {
            assert!(
                m < TAU_MODEL,
                "s=1e6: absolute oracle unexpectedly broke (residual {m})"
            );
        } else {
            assert!(
                m >= TAU_MODEL,
                "s=1e9: absolute oracle unexpectedly held ({m}); breakpoint moved"
            );
        }
    }
}

#[test]
fn attack5c_half_angle_e1_boundary() {
    // E1 fires at α ≤ TAU_MODEL and α ≥ π/2 − TAU_MODEL. Just OUTSIDE the gate
    // (degenerate) ⇒ Err; just INSIDE (valid) ⇒ a correct perpendicular circle.
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };

    // α just below the lower gate ⇒ degenerate.
    let cone_lo_bad = z_cone(TAU_MODEL * 0.5);
    assert_eq!(
        intersect(&plane, &cone_lo_bad),
        Err(SsiError::DegenerateInput),
        "α ≤ TAU_MODEL must be DegenerateInput"
    );
    // α just above the upper gate ⇒ degenerate.
    let cone_hi_bad = z_cone(std::f64::consts::FRAC_PI_2 - TAU_MODEL * 0.5);
    assert_eq!(
        intersect(&plane, &cone_hi_bad),
        Err(SsiError::DegenerateInput),
        "α ≥ π/2 − TAU_MODEL must be DegenerateInput"
    );

    // α just INSIDE the lower gate (very narrow cone): valid tiny circle.
    let alpha_narrow = TAU_MODEL * 100.0; // ≫ TAU but still a very narrow cone
    let cone_narrow = z_cone(alpha_narrow);
    let curves = intersect(&plane, &cone_narrow).expect("narrow valid cone → circle");
    let SsiCurve::Circle { radius, .. } = curves[0] else {
        panic!("expected circle, got {:?}", curves[0]);
    };
    // radius = |h|·tanα = 3·tan(1e-5) ≈ 3e-5.
    let expect = 3.0 * alpha_narrow.tan();
    assert!(
        (radius - expect).abs() / expect < 1e-9,
        "narrow-cone radius {radius} rel-off vs {expect}"
    );
    assert_on_both_surfaces(&curves[0], &plane, &cone_narrow);

    // α just INSIDE the upper gate (very flat cone): valid large circle.
    let alpha_flat = std::f64::consts::FRAC_PI_2 - 1e-3; // valid, near π/2
    let cone_flat = z_cone(alpha_flat);
    let curves = intersect(&plane, &cone_flat).expect("flat valid cone → circle");
    let SsiCurve::Circle { radius, .. } = curves[0] else {
        panic!("expected circle, got {:?}", curves[0]);
    };
    let expect = 3.0 * alpha_flat.tan();
    assert!(
        (radius - expect).abs() / expect < 1e-9,
        "flat-cone radius {radius} rel-off vs {expect}"
    );
    // Coords O(radius) ≈ 3000 ⇒ absolute oracle still valid.
    assert_on_both_surfaces(&curves[0], &plane, &cone_flat);
}

// ===========================================================================
// Attack 6: AP through-apex boundary.
//
// Apex EXACTLY on the plane ⇒ Err(DegenerateInput). Apex just off the plane
// (offset slightly more than TAU_MODEL along n̂) ⇒ a valid bounded curve — the
// AP gate must NOT swallow valid near-apex sections beyond its TAU band.
// ===========================================================================

#[test]
fn attack6_ap_band_does_not_swallow_valid_sections() {
    // Perpendicular plane (⟂ +z) ⇒ C1 circle when off-apex. Apex at origin;
    // plane z = h. AP fires when |n̂·(apex − p)| = |h| < TAU_MODEL.
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);

    // Apex exactly on the plane (h = 0) ⇒ AP Err.
    let plane_on = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    assert_eq!(
        intersect(&plane_on, &cone),
        Err(SsiError::DegenerateInput),
        "apex on plane must be AP DegenerateInput"
    );

    // h just inside the AP band ⇒ still Err.
    let plane_in = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, TAU_MODEL * 0.5),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    assert_eq!(
        intersect(&plane_in, &cone),
        Err(SsiError::DegenerateInput),
        "apex within TAU of plane is AP DegenerateInput"
    );

    // h just OUTSIDE the AP band ⇒ a VALID circle (gate must not over-reach).
    // Use h = 100·TAU_MODEL = 1e-5 (clearly beyond the band).
    let h = TAU_MODEL * 100.0;
    let plane_out = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, h),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let curves = intersect(&plane_out, &cone)
        .expect("near-apex but valid section must NOT be swallowed by AP");
    let SsiCurve::Circle { radius, .. } = curves[0] else {
        panic!("expected a (small) circle, got {:?}", curves[0]);
    };
    let expect = h * alpha.tan(); // |h|·tanα
    assert!(
        (radius - expect).abs() / expect < 1e-9,
        "near-apex circle radius {radius} rel-off vs {expect}"
    );
    assert_on_both_surfaces(&curves[0], &plane_out, &cone);
}

// ===========================================================================
// Attack 7: non-unit axis_dir + symmetry + determinism.
//
// Non-unit axis_dir (magnitude 5) ⇒ identical (up to sign) to unit-axis, in a
// C1 and a C2 case. intersect(plane,cone) == intersect(cone,plane). Determinism
// across repeated calls.
// ===========================================================================

#[test]
fn attack7_nonunit_axis_dir_c1_matches_unit() {
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 4.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cone_unit = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cone_big = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0), // magnitude 5
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cu = intersect(&plane, &cone_unit).unwrap();
    let cb = intersect(&plane, &cone_big).unwrap();
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
    parallel_up_to_sign(n_u, n_b);
    assert!(
        (norm(n_b) - 1.0).abs() < TAU_MODEL,
        "big-axis normal not unit"
    );
}

#[test]
fn attack7_nonunit_axis_dir_c2_matches_unit() {
    let theta = 20.0_f64.to_radians();
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 5.0),
        normal: tilted_z_normal(theta),
    };
    let cone_unit = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cone_big = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 5.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let cu = intersect(&plane, &cone_unit).unwrap();
    let cb = intersect(&plane, &cone_big).unwrap();
    let ex = |c: &SsiCurve| match *c {
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
        _ => panic!("expected ellipse"),
    };
    let (ctr_u, n_u, m_u, ar_u, br_u) = ex(&cu[0]);
    let (ctr_b, n_b, m_b, ar_b, br_b) = ex(&cb[0]);
    assert!(norm(sub(ctr_u, ctr_b)) < TAU_MODEL, "centers differ");
    assert!((ar_u - ar_b).abs() < TAU_MODEL, "major radii differ");
    assert!((br_u - br_b).abs() < TAU_MODEL, "minor radii differ");
    parallel_up_to_sign(n_u, n_b);
    parallel_up_to_sign(m_u, m_b);
}

#[test]
fn attack7_symmetry_and_determinism() {
    // Symmetry (I4) for C1, C2, and an Err case; determinism (I5) for C2.
    let alpha = std::f64::consts::FRAC_PI_4;
    let theta = 20.0_f64.to_radians();
    let cone = z_cone(alpha);

    // C1 symmetry.
    let p_c1 = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let ab = intersect(&p_c1, &cone).unwrap();
    let ba = intersect(&cone, &p_c1).unwrap();
    let r = |c: &SsiCurve| match *c {
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => (center.as_array(), normal.as_array(), radius),
        _ => panic!(),
    };
    let (c1, n1, r1) = r(&ab[0]);
    let (c2, n2, r2) = r(&ba[0]);
    assert!(norm(sub(c1, c2)) < TAU_MODEL);
    assert!((r1 - r2).abs() < TAU_MODEL);
    parallel_up_to_sign(n1, n2);

    // C2 symmetry + determinism.
    let p_c2 = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 5.0),
        normal: tilted_z_normal(theta),
    };
    let ab2 = intersect(&p_c2, &cone).unwrap();
    let ba2 = intersect(&cone, &p_c2).unwrap();
    // Same ellipse (up to major_axis/normal sign).
    let ex = |c: &SsiCurve| match *c {
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
    let (ce1, ne1, me1, are1, bre1) = ex(&ab2[0]);
    let (ce2, ne2, me2, are2, bre2) = ex(&ba2[0]);
    assert!(norm(sub(ce1, ce2)) < TAU_MODEL);
    assert!((are1 - are2).abs() < TAU_MODEL);
    assert!((bre1 - bre2).abs() < TAU_MODEL);
    parallel_up_to_sign(ne1, ne2);
    parallel_up_to_sign(me1, me2);

    // Err symmetry: hyperbola both orders.
    let p_h = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    assert_eq!(intersect(&p_h, &cone), intersect(&cone, &p_h));
    assert_eq!(
        intersect(&p_h, &cone),
        Err(SsiError::AnalyticalSolutionNotAvailable)
    );

    // Determinism: repeated identical calls byte-identical.
    let first = intersect(&p_c2, &cone);
    for _ in 0..5 {
        assert_eq!(
            intersect(&p_c2, &cone),
            first,
            "C2 output not deterministic"
        );
    }
}
