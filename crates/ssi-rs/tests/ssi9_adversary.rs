//! PR-SSI9 — Adversarial audit of the cone∩cone coaxial solver.
//!
//! These tests attack `cone_cone` (reached via the public `intersect`
//! dispatcher) at all four of its TAU_MODEL gate edges — the parallelism gate
//! `|â₂ × â₁| < TAU_MODEL`, the on-axis gate `d_ax < TAU_MODEL`, the
//! equal/unequal half-angle split `|α₁−α₂| ≤ TAU_MODEL`, and the apex-collapse
//! `|δ| ≤ TAU_MODEL` — across the double-cone axis-sign symmetry, the α E1
//! limits (`tanα → 0` and `→ ∞`), a deterministic many-config sweep, the
//! branch-symmetric argument swap, the absolute-tolerance coordinate-scale
//! ceiling (CHARACTERIZED, not force-greened), and the apex-grazing radius-0
//! point-circle (X0) collapse. They ADD tests only; they do NOT touch
//! production code, the spec, or `ssi9.rs`.
//!
//! Spec: specs/ssi_pr_ssi9_cone_cone_coaxial.md (esp. the "Characterization
//! notes (for the adversary)" + the P9/P10 anti-hack note: there is deliberately
//! NO √D sign gate, NO manufactured tangent/empty sub-branch — the discriminant
//! `(2·m₁·m₂·δ)²` is a perfect square, so coaxial unequal-α cone∩cone with
//! `|δ| > TAU_MODEL` is ALWAYS exactly two circles).
//! Research basis: Patrikalakis & Maekawa, *Shape Interrogation for CAD/M*,
//! §5.8.3 (Case F8, implicit/implicit quadric pair). The coaxial reduction
//! `|t|·tanα₁ = |t−δ|·tanα₂` ⇒ one or two circles is classical.
//!
//! Mirrors ssi8_adversary's discipline: the per-cone radial-residual on-surface
//! oracle, `assert_curve_finite`, order/sign-tolerant `circle_key`, RELATIVE
//! residual at large scale, and explicit CHARACTERIZATION of every
//! absolute-tolerance ceiling rather than forcing green.
//!
//! Tolerance note: TAU_MODEL = 1e-7. The absolute on-surface oracle is valid
//! only while circle-sample coordinates stay below the measured breakpoint.
//! MEASURED for this pair (256-sample sweep, apex on +z, α₁=π/4 m₁=1, α₂=atan(3)
//! m₂=3, so the X2 circles sit at t = (3/4)δ and (3/2)δ with radii up to ~1.5δ):
//!   |δ|=1e6 : maxres ~3e-10  — HOLDS
//!   |δ|=1e8 : maxres ~3e-8   — HOLDS (just under TAU_MODEL=1e-7)
//!   |δ|=1e9 : maxres ~3e-7   — BREAKS (just over TAU_MODEL)
//!   |δ|=1e12: maxres huge / coaxial band flips to NC — BREAKS / ASNA
//! so the absolute oracle holds through ~1e8 and first breaks at ~1e9 (same
//! class as the PR-SSI1 ceiling and the SSI7/SSI8 cone pairs). The coaxial
//! `d_ax`/parallelism band is an ABSOLUTE distance vs TAU_MODEL, so at very
//! large coordinate magnitude a truly-coaxial config can read as NC ⇒ a loud
//! ASNA (never a spurious circle). Both are documented loud ceilings, NOT logic
//! bugs — we assert the characterized behavior.

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

// ---------------------------------------------------------------------------
// On-surface oracle (I1) and structural helpers.
// ---------------------------------------------------------------------------

/// Every field of a returned Circle must be finite (no NaN/Inf from a leaked
/// 0/0 or ∞ division), the normal must be unit, and the radius must be finite
/// and ≥ 0 (X2/X1 produce strictly-positive radii; a radius-0 circle would be
/// the X0 apex case, which is represented as `Ok(vec![])` and so never emitted).
fn assert_curve_finite(c: &SsiCurve) {
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
            assert!(*radius >= 0.0, "Circle radius must be >= 0: {c:?}");
            assert!(
                (norm(normal.as_array()) - 1.0).abs() < 1e-9,
                "Circle normal not unit: {c:?}"
            );
        }
        other => panic!("cone∩cone must only return Circles; got {other:?}"),
    }
}

/// Absolute implicit residual on a surface (PR-SSI1 oracle). For the cone:
///   `| |(x−P) − ((x−P)·â)·â| − |h|·tanα |`, `h = (x−P)·â`.
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

/// Order/sign-tolerant canonical key for circle SET comparison: center, |radius|
/// rounded to TAU_MODEL units, plus the normal LINE (sign-canonicalized).
fn circle_key(c: &SsiCurve) -> (i64, i64, i64, i64, i64, i64, i64) {
    let (center, normal, radius) = circle_fields(c);
    let n = unit(normal);
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

fn key_set(curves: &[SsiCurve]) -> Vec<(i64, i64, i64, i64, i64, i64, i64)> {
    let mut keys: Vec<_> = curves.iter().map(circle_key).collect();
    keys.sort();
    keys
}

/// Build a cone with the given apex, axis direction, and half-angle.
fn cone(apex: [f64; 3], axis_dir: [f64; 3], alpha: f64) -> QuadricSurface {
    QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(axis_dir),
        half_angle: alpha,
    }
}

// ===========================================================================
// Attack 1: Parallelism gate boundary — cone₂'s axis tilted off cone₁'s axis
// by an angle whose SINE sits just inside vs just outside TAU_MODEL.
//
// The gate is `|â₂ × â₁| < TAU_MODEL` (an ABSOLUTE sine of the inter-axis
// angle). Apexes kept on a shared point (both at origin ⇒ d_ax = 0, δ = 0 on
// cone₁'s axis) is NOT useful here (δ=0 ⇒ X0/CO), so we keep cone₂'s apex on
// cone₁'s axis (+z) at z = 2 (δ = 2) and tilt ONLY cone₂'s axis_dir. Because
// the apex stays on the z-axis, `d_ax` measured against cone₁'s axis is still
// 0; only the parallelism term flips.
//
// MEASURED: sin = 0.9·TAU < TAU ⇒ coaxial ⇒ Ok(2 circles); sin = 1.001·TAU ≥
// TAU ⇒ NC ⇒ ASNA (strict `<`).
// ===========================================================================

#[test]
fn attack1_parallelism_gate_boundary() {
    let alpha1 = std::f64::consts::FRAC_PI_4; // m₁ = 1
    let alpha2 = 3.0_f64.atan(); // m₂ = 3 (unequal ⇒ X2)
    let c1 = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], alpha1);

    // Just INSIDE the band (tilt sine = 0.9·TAU) ⇒ coaxial ⇒ two circles. The
    // solver SNAPS the result axis to cone₁'s â (+z), ignoring the ≤TAU tilt of
    // cone₂. The emitted circles therefore lie exactly on cone₁ (on-cone₁
    // residual ~0) and on cone₂ to within the in-band slack. We assert the
    // branch + count + on-cone₁ tightness, NOT the two-surface absolute oracle
    // (the in-band tilt is the gate's whole point).
    {
        let theta = (0.9 * TAU_MODEL).asin();
        let ad2 = [theta.sin(), 0.0, theta.cos()];
        assert!(norm(cross(unit(ad2), [0.0, 0.0, 1.0])) < TAU_MODEL);
        let c2 = cone([0.0, 0.0, 2.0], ad2, alpha2);
        let curves = intersect(&c1, &c2)
            .unwrap_or_else(|e| panic!("just-inside parallelism band must be Ok, got {e:?}"));
        assert_eq!(curves.len(), 2, "just-inside ⇒ two circles");
        for cc in &curves {
            assert_curve_finite(cc);
            // On cone₁ exactly (result axis == cone₁'s axis).
            assert!(
                max_residual_on_both(cc, &c1, &c1, 64) < TAU_MODEL,
                "in-band circle must lie on cone₁ tightly"
            );
        }
    }

    // Just OUTSIDE the band (tilt sine = 1.001·TAU ≥ TAU) ⇒ NC ⇒ ASNA, no
    // spurious circle.
    {
        let theta = (1.001 * TAU_MODEL).asin();
        let ad2 = [theta.sin(), 0.0, theta.cos()];
        assert!(norm(cross(unit(ad2), [0.0, 0.0, 1.0])) >= TAU_MODEL);
        let c2 = cone([0.0, 0.0, 2.0], ad2, alpha2);
        assert_eq!(
            intersect(&c1, &c2),
            Err(SsiError::AnalyticalSolutionNotAvailable),
            "just-outside parallelism band ⇒ ASNA, not a circle"
        );
    }
}

// ===========================================================================
// Attack 2: On-axis (`d_ax`) gate boundary — cone₂'s apex displaced
// PERPENDICULAR to the shared axis by just-under vs just-over TAU_MODEL. Axes
// kept EXACTLY parallel (both +z) so only `d_ax` flips. cone₂'s apex also
// carries an ALONG-axis component (z = 2) so the X2 branch (δ ≠ 0) is exercised
// on the in-band side.
//
// MEASURED at unit scale: perp = 0.9·TAU ⇒ Ok(2); perp = 1.001·TAU ⇒ ASNA.
// ===========================================================================

#[test]
fn attack2_on_axis_gate_boundary() {
    let alpha1 = std::f64::consts::FRAC_PI_4;
    let alpha2 = 3.0_f64.atan();
    let c1 = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], alpha1);

    // Just UNDER (perp 0.9·TAU off +x, plus z=2 along axis) ⇒ coaxial ⇒ X2.
    {
        let off = 0.9 * TAU_MODEL;
        let c2 = cone([off, 0.0, 2.0], [0.0, 0.0, 1.0], alpha2);
        let curves = intersect(&c1, &c2)
            .unwrap_or_else(|e| panic!("just-under d_ax band must be Ok, got {e:?}"));
        assert_eq!(curves.len(), 2, "just-under ⇒ two circles");
        for cc in &curves {
            assert_curve_finite(cc);
        }
    }

    // Just OVER along +x and along +y (each off the axis) ⇒ NC ⇒ ASNA.
    for &dir in &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        let off = 1.001 * TAU_MODEL;
        let apex2 = add(scale(dir, off), [0.0, 0.0, 2.0]);
        let c2 = cone(apex2, [0.0, 0.0, 1.0], alpha2);
        assert_eq!(
            intersect(&c1, &c2),
            Err(SsiError::AnalyticalSolutionNotAvailable),
            "just-over d_ax band ({dir:?}) ⇒ ASNA, not a circle"
        );
    }
}

// ===========================================================================
// Attack 3: equal/unequal half-angle boundary (`|α₁−α₂| ≤ TAU_MODEL` is the
// X1 gate; `>` is X2). δ fixed ≠ 0.
//
// ε just under TAU (α₂ = α₁ + 0.5·TAU) ⇒ treated EQUAL ⇒ X1 (one circle).
// ε just over TAU (α₂ = α₁ + 2·TAU)  ⇒ treated UNEQUAL ⇒ X2 (two circles).
// We assert the BRANCH (len), per the spec's contract — at the boundary the X1
// circle and the X2 pair are geometrically close (denom = m₁²−m₂² is tiny so
// the X2 roots straddle the X1 bisector), but the count is the load-bearing
// contract, not their separation.
// ===========================================================================

#[test]
fn attack3_equal_unequal_alpha_boundary() {
    let alpha1 = std::f64::consts::FRAC_PI_4;
    let delta = 2.0_f64;
    let c1 = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], alpha1);

    // Just UNDER the gate ⇒ EQUAL ⇒ X1 (one circle at the bisector t = δ/2).
    {
        let alpha2 = alpha1 + 0.5 * TAU_MODEL;
        assert!(
            (alpha1 - alpha2).abs() <= TAU_MODEL,
            "test bug: not in-band"
        );
        let c2 = cone([0.0, 0.0, delta], [0.0, 0.0, 1.0], alpha2);
        let curves = intersect(&c1, &c2).expect("equal-α (in-band) ⇒ X1");
        assert_eq!(curves.len(), 1, "|α₁−α₂| ≤ TAU ⇒ one circle (X1)");
        assert_curve_finite(&curves[0]);
        // Bisector: center.z = δ/2 = 1.
        let (center, _, _) = circle_fields(&curves[0]);
        assert!(
            (center[2] - delta / 2.0).abs() < TAU_MODEL,
            "X1 circle not at the δ/2 bisector: z={}",
            center[2]
        );
    }

    // Just OVER the gate ⇒ UNEQUAL ⇒ X2 (two circles), both finite.
    {
        let alpha2 = alpha1 + 2.0 * TAU_MODEL;
        assert!(
            (alpha1 - alpha2).abs() > TAU_MODEL,
            "test bug: not out-of-band"
        );
        let c2 = cone([0.0, 0.0, delta], [0.0, 0.0, 1.0], alpha2);
        let curves = intersect(&c1, &c2).expect("unequal-α (out-of-band) ⇒ X2");
        assert_eq!(curves.len(), 2, "|α₁−α₂| > TAU ⇒ two circles (X2)");
        for cc in &curves {
            assert_curve_finite(cc);
        }
    }
}

// ===========================================================================
// Attack 4: `|δ|` apex-collapse boundary.
//   (a) Unequal α: |δ| just over TAU ⇒ X2 (two circles); just under ⇒ X0
//       (Ok(vec![])).
//   (b) Equal α:   |δ| just over TAU ⇒ X1 (one circle);  just under ⇒ CO
//       (Err(DegenerateInput)).
// The gate is on the linear quantity `|δ|`. We use 0.5·TAU just-under and
// 2·TAU just-over to be robust to fp rounding of `frac·TAU`.
// ===========================================================================

#[test]
fn attack4_delta_collapse_boundary() {
    let alpha1 = std::f64::consts::FRAC_PI_4;
    let alpha2 = 3.0_f64.atan(); // unequal
    let c1 = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], alpha1);

    // (a) Unequal α.
    {
        // |δ| just OVER TAU ⇒ X2 (two circles).
        let c2_over = cone([0.0, 0.0, 2.0 * TAU_MODEL], [0.0, 0.0, 1.0], alpha2);
        let over = intersect(&c1, &c2_over).expect("unequal, |δ| > TAU ⇒ X2");
        assert_eq!(over.len(), 2, "unequal, |δ| > TAU ⇒ two circles");
        for cc in &over {
            assert_curve_finite(cc);
        }
        // |δ| just UNDER TAU ⇒ X0 (Ok(vec![])).
        let c2_under = cone([0.0, 0.0, 0.5 * TAU_MODEL], [0.0, 0.0, 1.0], alpha2);
        assert_eq!(
            intersect(&c1, &c2_under),
            Ok(Vec::new()),
            "unequal, |δ| ≤ TAU ⇒ X0 (empty)"
        );
    }

    // (b) Equal α.
    {
        let c1e = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], alpha1);
        // |δ| just OVER TAU ⇒ X1 (one circle).
        let c2_over = cone([0.0, 0.0, 2.0 * TAU_MODEL], [0.0, 0.0, 1.0], alpha1);
        let over = intersect(&c1e, &c2_over).expect("equal, |δ| > TAU ⇒ X1");
        assert_eq!(over.len(), 1, "equal, |δ| > TAU ⇒ one circle");
        assert_curve_finite(&over[0]);
        // |δ| just UNDER TAU ⇒ CO (Err(DegenerateInput)).
        let c2_under = cone([0.0, 0.0, 0.5 * TAU_MODEL], [0.0, 0.0, 1.0], alpha1);
        assert_eq!(
            intersect(&c1e, &c2_under),
            Err(SsiError::DegenerateInput),
            "equal, |δ| ≤ TAU ⇒ CO (DegenerateInput)"
        );
    }
}

// ===========================================================================
// Attack 5: Reversed / antiparallel axis sign (double-cone symmetry). Flipping
// either cone's axis_dir to its negative keeps the axis LINE the same (still
// parallel, `|â₂ × â₁| ≈ 0`) and must leave the world-space circle SET
// unchanged: â → −â flips the sign of both â and δ, leaving t·â and the
// world-space circles invariant.
// ===========================================================================

#[test]
fn attack5_reversed_axis_sign_invariant_set() {
    let alpha1 = std::f64::consts::FRAC_PI_4; // m₁ = 1
    let alpha2 = 3.0_f64.atan(); // m₂ = 3
    let c1 = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], alpha1);
    let c2 = cone([0.0, 0.0, 2.0], [0.0, 0.0, 1.0], alpha2);
    let baseline = intersect(&c1, &c2).expect("canonical X2");
    assert_eq!(baseline.len(), 2);
    let baseline_keys = key_set(&baseline);

    // (a) Flip cone₂'s axis_dir to −z (antiparallel line). Apex unchanged on the
    // z-axis ⇒ still coaxial. World circle SET must be unchanged.
    {
        let c2_flip = cone([0.0, 0.0, 2.0], [0.0, 0.0, -1.0], alpha2);
        let flipped = intersect(&c1, &c2_flip).expect("antiparallel cone₂ axis still coaxial");
        assert_eq!(
            flipped.len(),
            2,
            "antiparallel cone₂ axis ⇒ still two circles"
        );
        for cc in &flipped {
            assert_curve_finite(cc);
            assert_on_both_surfaces(cc, &c1, &c2_flip);
        }
        assert_eq!(
            key_set(&flipped),
            baseline_keys,
            "flipping cone₂'s axis sign must not change the world circle SET"
        );
    }

    // (b) Flip cone₁'s axis_dir to −z. â → −z, so δ flips sign too; t·â and the
    // world circles are invariant. The SET must be unchanged.
    {
        let c1_flip = cone([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], alpha1);
        let flipped = intersect(&c1_flip, &c2).expect("flipped cone₁ axis still coaxial");
        assert_eq!(flipped.len(), 2, "flipped cone₁ axis ⇒ still two circles");
        for cc in &flipped {
            assert_curve_finite(cc);
            assert_on_both_surfaces(cc, &c1_flip, &c2);
        }
        assert_eq!(
            key_set(&flipped),
            baseline_keys,
            "flipping cone₁'s axis sign must not change the world circle SET"
        );
    }
}

// ===========================================================================
// Attack 6: α near both E1 limits, from the valid side, plus crossing to the
// invalid side (each cone). Inside the band, a valid UNEQUAL-α coaxial config
// gives two finite circles; at/just past the bound, the E1 gate fires.
//
// Gate (production): `α ≤ TAU_MODEL` OR `α ≥ π/2 − TAU_MODEL` (either cone) ⇒
// DegenerateInput.
// ===========================================================================

#[test]
fn attack6_alpha_near_e1_limits() {
    let delta = 2.0_f64;
    // A fixed partner cone with a benign mid-band half-angle.
    let partner = cone(
        [0.0, 0.0, delta],
        [0.0, 0.0, 1.0],
        std::f64::consts::FRAC_PI_4,
    );

    // α just INSIDE the low limit (TAU + tiny) for cone₁ ⇒ valid, unequal vs
    // partner's π/4 ⇒ two finite circles (tanα tiny ⇒ huge t, still finite).
    {
        let alpha = TAU_MODEL * 2.0; // safely > TAU, far from π/4 ⇒ unequal
        let c1 = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], alpha);
        let curves = intersect(&c1, &partner).expect("α just inside low limit ⇒ valid X2");
        assert_eq!(curves.len(), 2, "low-limit α ⇒ two circles");
        for cc in &curves {
            assert_curve_finite(cc);
        }
    }

    // α just INSIDE the high limit (π/2 − TAU − tiny) for cone₁ ⇒ valid X2.
    {
        let alpha = std::f64::consts::FRAC_PI_2 - 2.0 * TAU_MODEL;
        let c1 = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], alpha);
        let curves = intersect(&c1, &partner).expect("α just inside high limit ⇒ valid X2");
        assert_eq!(curves.len(), 2, "high-limit α ⇒ two circles");
        for cc in &curves {
            assert_curve_finite(cc);
        }
    }

    // α just PAST each limit (each cone) ⇒ DegenerateInput.
    let bad_alphas = [
        TAU_MODEL,                                     // == low bound (≤ ⇒ bad)
        0.5 * TAU_MODEL,                               // below low bound
        std::f64::consts::FRAC_PI_2 - TAU_MODEL,       // == high bound (≥ ⇒ bad)
        std::f64::consts::FRAC_PI_2 - 0.5 * TAU_MODEL, // past high bound
    ];
    for &bad in &bad_alphas {
        // cone₁ bad.
        let c1 = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], bad);
        assert_eq!(
            intersect(&c1, &partner),
            Err(SsiError::DegenerateInput),
            "cone₁ α={bad} past the E1 limit ⇒ DegenerateInput"
        );
        // cone₂ bad (swap roles).
        let c1_good = cone(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_3,
        );
        let c2 = cone([0.0, 0.0, delta], [0.0, 0.0, 1.0], bad);
        assert_eq!(
            intersect(&c1_good, &c2),
            Err(SsiError::DegenerateInput),
            "cone₂ α={bad} past the E1 limit ⇒ DegenerateInput"
        );
    }
}

// ===========================================================================
// Attack 7: Determinism sweep. A grid of coaxial configs (several unequal
// (α₁,α₂) pairs × several δ≠0 including negative, on both an axis-aligned and a
// non-axis-aligned shared axis). EVERY call must be byte-identical across two
// invocations, AND curves[0] must carry the LARGER t (recompute
// t = (center − apex₁)·â). No RNG (ssi-rs determinism rule).
// ===========================================================================

#[test]
fn attack7_determinism_sweep_larger_t_first() {
    let alpha_pairs = [
        (std::f64::consts::FRAC_PI_4, 3.0_f64.atan()),
        (0.3, 1.2),
        (std::f64::consts::FRAC_PI_6, std::f64::consts::FRAC_PI_3),
        (1.3, 0.4),
    ];
    let deltas: [f64; 5] = [0.5, 2.0, -1.5, 7.0, -10.0];
    let axes: [[f64; 3]; 2] = [[0.0, 0.0, 1.0], [1.0, 2.0, 2.0]];
    let apex1_base: [[f64; 3]; 2] = [[0.0, 0.0, 0.0], [3.0, -1.0, 4.0]];

    let mut count = 0usize;
    for (axi, &raw_axis) in axes.iter().enumerate() {
        let ahat = unit(raw_axis);
        let apex1 = apex1_base[axi];
        for &(a1, a2) in &alpha_pairs {
            for &delta in &deltas {
                // apex₂ on cone₁'s axis line at offset δ ⇒ coaxial, δ = offset.
                let apex2 = add(apex1, scale(ahat, delta));
                let c1 = cone(apex1, raw_axis, a1);
                let c2 = cone(apex2, raw_axis, a2);

                let first = intersect(&c1, &c2);
                let second = intersect(&c1, &c2);
                assert_eq!(
                    first, second,
                    "non-deterministic [axi={axi} α=({a1},{a2}) δ={delta}]"
                );
                let curves = first.unwrap_or_else(|e| {
                    panic!(
                        "coaxial sweep [axi={axi} α=({a1},{a2}) δ={delta}] must be Ok, got {e:?}"
                    )
                });
                assert_eq!(
                    curves.len(),
                    2,
                    "unequal-α coaxial must be two circles [axi={axi} α=({a1},{a2}) δ={delta}]"
                );
                let t0 = dot(sub(circle_fields(&curves[0]).0, apex1), ahat);
                let t1 = dot(sub(circle_fields(&curves[1]).0, apex1), ahat);
                assert!(
                    t0 >= t1 - TAU_MODEL,
                    "larger-t must be first: t0={t0} t1={t1} [axi={axi} α=({a1},{a2}) δ={delta}]"
                );
                for cc in &curves {
                    assert_curve_finite(cc);
                }
                count += 1;
            }
        }
    }
    assert_eq!(count, 2 * 4 * 5, "sweep coverage count");
}

// ===========================================================================
// Attack 8: Symmetry sweep across branches (I4). For both an X2 and an X1
// config, intersect(c1,c2) and intersect(c2,c1) must give the same circle SET
// (circle_key). Swapping makes cone₂ the new cone₁ ⇒ â and the t-parameter
// change, but the WORLD circles are identical — the real test of the
// set-equality contract.
// ===========================================================================

#[test]
fn attack8_symmetry_across_branches() {
    // X2 config.
    {
        let c1 = cone(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_4,
        );
        let c2 = cone([0.0, 0.0, 2.0], [0.0, 0.0, 1.0], 3.0_f64.atan());
        let ab = intersect(&c1, &c2).expect("ab X2");
        let ba = intersect(&c2, &c1).expect("ba X2");
        assert_eq!(ab.len(), 2);
        assert_eq!(ba.len(), 2);
        assert_eq!(
            key_set(&ab),
            key_set(&ba),
            "X2 circle SET must be swap-invariant"
        );
    }

    // X1 config (equal α). Oblique shared axis to stress the swap parameterization.
    {
        let ahat = unit([1.0, -2.0, 2.0]);
        let apex1 = [2.0, 0.0, -1.0];
        let alpha = std::f64::consts::FRAC_PI_3;
        let apex2 = add(apex1, scale(ahat, 3.0)); // δ = 3, on the axis line
        let c1 = cone(apex1, [1.0, -2.0, 2.0], alpha);
        let c2 = cone(apex2, [1.0, -2.0, 2.0], alpha);
        let ab = intersect(&c1, &c2).expect("ab X1");
        let ba = intersect(&c2, &c1).expect("ba X1");
        assert_eq!(ab.len(), 1);
        assert_eq!(ba.len(), 1);
        assert_eq!(
            key_set(&ab),
            key_set(&ba),
            "X1 circle SET must be swap-invariant"
        );
    }
}

// ===========================================================================
// Attack 9: CHARACTERIZE — absolute-TAU coordinate-scale ceiling (do NOT force
// green). Sweep a canonical X2 config to large coordinate magnitude (δ at 1e6,
// 1e8, 1e9, 1e12). DOCUMENT where (a) the absolute on-surface oracle (per-cone
// radial residual < TAU_MODEL) holds and where it breaks, and (b) where the
// absolute d_ax/parallelism coaxial band conservatively flips a truly-coaxial
// config to NC ⇒ ASNA. We assert the ACTUAL behavior at each scale — a loud
// never-wrong failure mode, not a tolerance widening. Mirrors ssi8_adversary's
// ~1e8→1e9 characterization.
//
// MEASURED (apex on +z, α₁=π/4 m₁=1, α₂=atan(3) m₂=3, circles at t = (3/4)δ and
// (3/2)δ, 256 samples):
//   δ=1e6 : maxres ~3e-10 — HOLDS (absolute oracle)
//   δ=1e8 : maxres ~3e-8  — HOLDS (just under TAU)
//   δ=1e9 : maxres ~3e-7  — BREAKS (relative residual stays ~1e-16)
//   δ=1e12: huge residual / coaxial band flips to ASNA
// ===========================================================================

#[test]
fn attack9_absolute_scale_ceiling_characterization() {
    let alpha1 = std::f64::consts::FRAC_PI_4; // m₁ = 1
    let alpha2 = 3.0_f64.atan(); // m₂ = 3

    let build = |delta: f64| -> (QuadricSurface, QuadricSurface) {
        (
            cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], alpha1),
            cone([0.0, 0.0, delta], [0.0, 0.0, 1.0], alpha2),
        )
    };

    // δ = 1e6, 1e8 ⇒ absolute on-surface oracle still HOLDS.
    for &delta in &[1e6_f64, 1e8] {
        let (c1, c2) = build(delta);
        let curves = intersect(&c1, &c2).expect("large-but-in-band δ X2");
        assert_eq!(curves.len(), 2);
        for cc in &curves {
            assert_curve_finite(cc);
            let m = max_residual_on_both(cc, &c1, &c2, 256);
            assert!(
                m < TAU_MODEL,
                "δ={delta:e}: absolute oracle unexpectedly broke ({m}); breakpoint moved"
            );
        }
    }

    // δ = 1e9 ⇒ absolute oracle BREAKS, but the solver stays analytically
    // correct: the result is still Ok(2) finite circles whose RELATIVE residual
    // is tiny. This is the absolute-tolerance CEILING, NOT a logic bug. Do NOT
    // loosen TAU_MODEL. (If the coaxial band itself has already flipped to ASNA
    // at this scale, that too is a loud never-wrong outcome; accept either.)
    {
        let delta = 1e9_f64;
        let (c1, c2) = build(delta);
        match intersect(&c1, &c2) {
            Ok(curves) => {
                assert_eq!(curves.len(), 2);
                let mut broke = false;
                for cc in &curves {
                    assert_curve_finite(cc);
                    let m = max_residual_on_both(cc, &c1, &c2, 256);
                    if m >= TAU_MODEL {
                        broke = true;
                    }
                    // Relative residual stays tiny regardless.
                    assert!(
                        m / delta < 1e-9,
                        "δ=1e9: relative residual {} too big",
                        m / delta
                    );
                }
                assert!(
                    broke,
                    "δ=1e9: absolute oracle unexpectedly HELD; breakpoint moved"
                );
            }
            Err(SsiError::AnalyticalSolutionNotAvailable) => {
                // The absolute coaxial band flipped a truly-coaxial config to NC.
                // Documented loud ceiling, accepted.
            }
            Err(other) => panic!("δ=1e9: unexpected error {other:?}"),
        }
    }

    // δ = 1e12 ⇒ deep past every absolute ceiling. We accept Ok(2) (with a large
    // absolute but tiny relative residual) OR ASNA (the coaxial band flipped) —
    // both are loud, never-wrong outcomes. We assert it NEVER returns a wrong
    // count, a NaN, or a non-ASNA error.
    {
        let delta = 1e12_f64;
        let (c1, c2) = build(delta);
        match intersect(&c1, &c2) {
            Ok(curves) => {
                assert_eq!(curves.len(), 2, "δ=1e12: must still be two circles if Ok");
                for cc in &curves {
                    assert_curve_finite(cc);
                    let m = max_residual_on_both(cc, &c1, &c2, 64);
                    assert!(
                        m / delta < 1e-9,
                        "δ=1e12: relative residual {} too big",
                        m / delta
                    );
                }
            }
            Err(SsiError::AnalyticalSolutionNotAvailable) => {
                // Documented absolute-band scale flip — loud ASNA, accepted.
            }
            Err(other) => panic!("δ=1e12: unexpected error {other:?}"),
        }
    }
}

// ===========================================================================
// Attack 10: CHARACTERIZE — apex-grazing radius-0 point-circle (X0) collapse.
// Unequal α; sweep |δ| from clearly-X2 down toward the TAU_MODEL collapse and
// DOCUMENT the transition: two near-apex circles (their radii → 0 as |δ| → 0,
// since both roots ∝ δ) collapse to Ok(vec![]) once |δ| ≤ TAU_MODEL. Assert the
// ACTUAL behavior at each step.
//
// For α₁=π/4 (m₁=1), α₂=atan(3) (m₂=3): roots t = (3/4)δ and (3/2)δ, radii
// |t|·m₁ = (3/4)|δ| and (3/2)|δ| — both ∝ |δ|, so they shrink linearly toward
// the apex as |δ| → 0.
// ===========================================================================

#[test]
fn attack10_x0_apex_grazing_collapse_characterization() {
    let alpha1 = std::f64::consts::FRAC_PI_4; // m₁ = 1
    let alpha2 = 3.0_f64.atan(); // m₂ = 3
    let c1 = cone([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], alpha1);

    // Clearly-X2 region, decreasing |δ|: still two circles, radii ∝ |δ| (so they
    // shrink toward the apex), and they stay on both cones (absolute oracle holds
    // because coords are small).
    for &delta in &[1.0_f64, 1e-2, 1e-4, 1e-6] {
        let c2 = cone([0.0, 0.0, delta], [0.0, 0.0, 1.0], alpha2);
        let curves = intersect(&c1, &c2)
            .unwrap_or_else(|e| panic!("δ={delta:e}: clearly-X2 must be Ok, got {e:?}"));
        assert_eq!(curves.len(), 2, "δ={delta:e}: two circles");
        for cc in &curves {
            assert_curve_finite(cc);
            let (_, _, radius) = circle_fields(cc);
            // radius ∈ {(3/4)|δ|, (3/2)|δ|} ⇒ bounded by (3/2)|δ|, shrinking → 0.
            assert!(
                radius <= 1.5 * delta.abs() + TAU_MODEL,
                "δ={delta:e}: radius {radius} exceeds (3/2)|δ| — roots not ∝ δ"
            );
            assert_on_both_surfaces(cc, &c1, &c2);
        }
    }

    // The collapse boundary: |δ| just under TAU ⇒ X0 (Ok(vec![])); just over ⇒
    // still two (vanishingly small) circles. This is the documented transition.
    {
        let c2_under = cone([0.0, 0.0, 0.5 * TAU_MODEL], [0.0, 0.0, 1.0], alpha2);
        assert_eq!(
            intersect(&c1, &c2_under),
            Ok(Vec::new()),
            "|δ| ≤ TAU ⇒ X0 apex point-circle ⇒ Ok(vec![])"
        );
        let c2_over = cone([0.0, 0.0, 2.0 * TAU_MODEL], [0.0, 0.0, 1.0], alpha2);
        let over = intersect(&c1, &c2_over).expect("|δ| just over TAU ⇒ X2");
        assert_eq!(over.len(), 2, "|δ| just over TAU ⇒ two near-apex circles");
        for cc in &over {
            assert_curve_finite(cc);
            let (_, _, radius) = circle_fields(cc);
            // radii ~ (3/4)·2·TAU and (3/2)·2·TAU — tiny but > 0.
            assert!(radius > 0.0, "near-apex circle must have radius > 0");
            assert!(
                radius <= 3.0 * TAU_MODEL + 1e-12,
                "near-apex radius {radius} unexpectedly large"
            );
        }
        // Exactly-at: δ = TAU_MODEL ⇒ |δ| ≤ TAU (the gate is `<=`) ⇒ X0.
        let c2_at = cone([0.0, 0.0, TAU_MODEL], [0.0, 0.0, 1.0], alpha2);
        assert_eq!(
            intersect(&c1, &c2_at),
            Ok(Vec::new()),
            "|δ| == TAU (≤ gate) ⇒ X0 (empty)"
        );
    }
}
