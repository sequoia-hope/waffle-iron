//! PR-SSI5 — Adversarial audit of the plane∩cone THROUGH-APEX degenerate
//! conics (the AP branch: point `Ok([])` / one Line / two Lines).
//!
//! Attacks the AP classification at and around its sub-case boundaries, all
//! reached via the public `intersect` dispatcher (`plane_cone` is private):
//!   1. point↔line↔two-line boundary sweep (k vs sinα), monotone + clean count,
//!      no premature collapse / zero-length direction as k→sinα⁻;
//!   2. AP detection band (apex exactly-vs-near on plane) — AP↔proper-conic
//!      switch across the TAU_MODEL band, clean band edge;
//!   3. two-line correctness (axis-aligned + oblique): on both surfaces,
//!      through the apex, generator angle, in-plane, unit, distinct, symmetric;
//!   4. one-line tangent correctness (k=sinα two construction ways);
//!   5. extreme half-angles (narrow / flat cones) + E1 still fires outside the
//!      valid α interval;
//!   6. I4 symmetry + I5 determinism for AP-line and AP-lines.
//!
//! ALSO independently re-derives the SSI3 AP-fixture migration verdict inline
//! (mirrors the two migrated ssi3.rs fixtures + the attack6 band intent).
//!
//! Does NOT touch production code. Reuses the ssi3 on-surface oracle (plane
//! residual + cone RADIAL residual `| |(x−apex)−h·â| − |h|·tanα |`).
//!
//! Tolerance note: TAU_MODEL = 1e-7; the absolute on-surface oracle is valid
//! while sample coords stay below ~1e8. Lines are sampled over a bounded t.

use cad_primitives::{Point3, Vector3, TAU_MODEL};
use ssi_rs::{intersect, QuadricSurface, SsiCurve, SsiError};

// ---------------------------------------------------------------------------
// Vector helpers (cad-primitives is types-only) — mirrors ssi3_adversary.
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

/// Every field of a returned curve must be finite (no NaN/Inf).
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
        SsiCurve::Circle { radius, .. } => {
            assert!(radius.is_finite(), "Circle radius non-finite: {c:?}");
        }
        SsiCurve::Ellipse {
            major_radius,
            minor_radius,
            ..
        } => {
            assert!(major_radius.is_finite() && minor_radius.is_finite());
        }
        SsiCurve::Parabola { focal_length, .. } => {
            assert!(focal_length.is_finite(), "Parabola focal non-finite: {c:?}");
        }
        SsiCurve::Hyperbola {
            semi_transverse,
            semi_conjugate,
            ..
        } => {
            assert!(semi_transverse.is_finite() && semi_conjugate.is_finite());
        }
    }
}

/// A line sampled over bounded t ∈ [−T, T] must lie on BOTH surfaces and pass
/// through the apex at t = 0.
fn assert_line_on_both_through_apex(
    line: &SsiCurve,
    a: &QuadricSurface,
    b: &QuadricSurface,
    apex: [f64; 3],
    t_max: f64,
) {
    let SsiCurve::Line { dir, .. } = line else {
        panic!("expected Line, got {line:?}");
    };
    assert!(
        (norm(dir.as_array()) - 1.0).abs() < TAU_MODEL,
        "dir not unit"
    );
    const N: usize = 128;
    for i in 0..N {
        let t = -t_max + (i as f64) / ((N - 1) as f64) * 2.0 * t_max;
        let p = line.eval(t).as_array();
        let ra = implicit_residual(a, p);
        let rb = implicit_residual(b, p);
        assert!(ra < TAU_MODEL, "t={t} off surface A (residual {ra})");
        assert!(rb < TAU_MODEL, "t={t} off surface B (residual {rb})");
    }
    let p0 = line.eval(0.0).as_array();
    assert!(
        norm(sub(p0, apex)) < TAU_MODEL,
        "line does not pass through apex: eval(0)={p0:?}, apex={apex:?}"
    );
}

fn approx(a: f64, b: f64, ctx: &str) {
    assert!((a - b).abs() < TAU_MODEL, "{ctx}: expected {a} ≈ {b}");
}

fn parallel_up_to_sign(a: [f64; 3], b: [f64; 3]) {
    assert!(
        norm(cross(a, b)) < TAU_MODEL,
        "expected {a:?} ∥ {b:?} (|cross|={})",
        norm(cross(a, b))
    );
}

fn z_cone(half_angle: f64) -> QuadricSurface {
    QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle,
    }
}

/// Classify a curve list into the AP sub-case label.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Ap {
    Point, // Ok(vec![])
    Line,  // one Line
    Lines, // two Lines
    Other, // any other curve (proper conic — a NON-AP result)
}

fn classify(curves: &[SsiCurve]) -> Ap {
    match curves.len() {
        0 => Ap::Point,
        1 => {
            if matches!(curves[0], SsiCurve::Line { .. }) {
                Ap::Line
            } else {
                Ap::Other
            }
        }
        2 => {
            if curves.iter().all(|c| matches!(c, SsiCurve::Line { .. })) {
                Ap::Lines
            } else {
                Ap::Other
            }
        }
        _ => Ap::Other,
    }
}

// An apex-on-plane cone+plane where the plane normal has k = n̂·â = `k` with
// the apex at the origin, axis +z. n̂ = (√(1−k²), 0, k) lies in x–z; the plane
// passes through the origin (apex on plane). Returns (plane, cone, n̂, â).
fn ap_setup(k: f64, alpha: f64) -> (QuadricSurface, QuadricSurface, [f64; 3], [f64; 3]) {
    let s = (1.0 - k * k).max(0.0).sqrt();
    let nrm = [s, 0.0, k];
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0), // through apex
        normal: Vector3::from(nrm),
    };
    let cone = z_cone(alpha);
    (plane, cone, nrm, [0.0, 0.0, 1.0])
}

// ===========================================================================
// Attack 1: point↔line↔two-line boundary sweep (k vs sinα).
//
// Apex on plane, axis +z, α = π/4 (sinα = √2/2). Sweep k from 0 (two lines)
// through sinα (one line) to > sinα (point). Assert:
//   - classification matches k vs sinα with a clean MONOTONE boundary
//     (every two-line k < every one-line k < every point k);
//   - never NaN/Inf, never a spurious/dropped/miscounted line;
//   - as k→sinα⁻ the two lines stay distinct + unit (no premature collapse,
//     no zero-length direction just inside the two-line region).
// ===========================================================================

#[test]
fn attack1_point_line_twoline_boundary_sweep_monotone() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sina = alpha.sin();
    let cosa = alpha.cos();

    let mut max_lines_k = f64::NEG_INFINITY;
    let mut min_line_k = f64::INFINITY;
    let mut max_line_k = f64::NEG_INFINITY;
    let mut min_point_k = f64::INFINITY;
    let mut saw_lines = false;
    let mut saw_line = false;
    let mut saw_point = false;

    // The one-line band is gated on the dimensionless gd (|gd| < TAU_MODEL),
    // which maps to a k-window of width ≈ 2·TAU_MODEL·s_n/sinα ≈ 1.4e-7 around
    // sinα — far narrower than a uniform sweep step. So sample with a coarse
    // grid over [0, 1.3·sinα] PLUS a fine grid straddling k = sinα to guarantee
    // the one-line sub-case is hit (a uniform sweep would step over it; that is
    // a sampling artifact, not a production miscount).
    let coarse: Vec<f64> = (0..=4000)
        .map(|i| (1.3 * sina * (i as f64) / 4000.0).min(0.999_999))
        .collect();
    let fine: Vec<f64> = (-2000..=2000)
        .map(|j| sina + (j as f64) * (TAU_MODEL * 1e-3))
        .filter(|k| *k > 0.0 && *k < 1.0)
        .collect();
    let ks: Vec<f64> = coarse.into_iter().chain(fine).collect();
    for &k in &ks {
        let (plane, cone, nrm, ahat) = ap_setup(k, alpha);
        let curves = intersect(&plane, &cone)
            .unwrap_or_else(|e| panic!("k={k}: AP must never error, got {e:?}"));
        curves.iter().for_each(assert_curve_finite);
        let cls = classify(&curves);
        assert_ne!(
            cls,
            Ap::Other,
            "k={k}: AP returned a non-AP curve {curves:?}"
        );

        match cls {
            Ap::Lines => {
                saw_lines = true;
                max_lines_k = max_lines_k.max(k);
                assert_eq!(curves.len(), 2, "k={k}: two-line count");
                // Both unit, distinct, on both surfaces through apex, generator.
                let d0 = match curves[0] {
                    SsiCurve::Line { dir, .. } => dir.as_array(),
                    _ => unreachable!(),
                };
                let d1 = match curves[1] {
                    SsiCurve::Line { dir, .. } => dir.as_array(),
                    _ => unreachable!(),
                };
                approx(norm(d0), 1.0, &format!("k={k} d0 unit"));
                approx(norm(d1), 1.0, &format!("k={k} d1 unit"));
                approx(dot(d0, ahat).abs(), cosa, &format!("k={k} d0 generator"));
                approx(dot(d1, ahat).abs(), cosa, &format!("k={k} d1 generator"));
                approx(dot(d0, nrm), 0.0, &format!("k={k} d0 in-plane"));
                approx(dot(d1, nrm), 0.0, &format!("k={k} d1 in-plane"));
                // Distinct — no premature collapse to a single direction even as
                // k→sinα⁻. (Fold to the same nappe before measuring distance.)
                let d1f = if dot(d0, d1) < 0.0 {
                    scale(d1, -1.0)
                } else {
                    d1
                };
                assert!(
                    norm(sub(d0, d1f)) > TAU_MODEL,
                    "k={k}: two lines collapsed (d0={d0:?}, d1={d1:?}) — premature \
                     convergence inside the two-line region"
                );
            }
            Ap::Line => {
                saw_line = true;
                min_line_k = min_line_k.min(k);
                max_line_k = max_line_k.max(k);
                assert_eq!(curves.len(), 1, "k={k}: one-line count");
                let d = match curves[0] {
                    SsiCurve::Line { dir, .. } => dir.as_array(),
                    _ => unreachable!(),
                };
                approx(norm(d), 1.0, &format!("k={k} tangent unit"));
                approx(
                    dot(d, ahat).abs(),
                    cosa,
                    &format!("k={k} tangent generator"),
                );
                approx(dot(d, nrm), 0.0, &format!("k={k} tangent in-plane"));
            }
            Ap::Point => {
                saw_point = true;
                min_point_k = min_point_k.min(k);
            }
            Ap::Other => unreachable!(),
        }
    }

    assert!(
        saw_lines && saw_line && saw_point,
        "sweep missed a sub-case"
    );
    // Clean monotone boundary: lines region entirely below the line region,
    // which is entirely below the point region.
    assert!(
        max_lines_k < min_line_k,
        "two-line/one-line boundary not monotone: max lines k={max_lines_k}, \
         min line k={min_line_k}"
    );
    assert!(
        max_line_k < min_point_k,
        "one-line/point boundary not monotone: max line k={max_line_k}, \
         min point k={min_point_k}"
    );
    // The one-line band sits at k ≈ sinα (gate is on the dimensionless gd).
    assert!(
        (min_line_k - sina).abs() < 1e-3 && (max_line_k - sina).abs() < 1e-3,
        "one-line band not centered on sinα: [{min_line_k},{max_line_k}] vs {sina}"
    );
}

// Pin the convergence directly: at k just below sinα the two lines are distinct
// with a still-sizeable separation (sφ does not collapse to ~0 just inside the
// gate) — the directions are never zero-length.
#[test]
fn attack1b_two_lines_do_not_collapse_just_below_sina() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let sina = alpha.sin();
    // Approach sinα from below; each must still yield TWO distinct unit lines.
    for frac in [0.99, 0.999, 0.9999, 0.99999] {
        let k = sina * frac;
        let (plane, cone, _, _) = ap_setup(k, alpha);
        let curves = intersect(&plane, &cone).expect("k<sinα ⇒ two lines");
        assert_eq!(
            classify(&curves),
            Ap::Lines,
            "k={k} (frac={frac}): expected two lines, got {curves:?}"
        );
        let d0 = match curves[0] {
            SsiCurve::Line { dir, .. } => dir.as_array(),
            _ => unreachable!(),
        };
        let d1 = match curves[1] {
            SsiCurve::Line { dir, .. } => dir.as_array(),
            _ => unreachable!(),
        };
        assert!(
            norm(d0) > 0.5 && norm(d1) > 0.5,
            "k={k}: zero-length direction"
        );
        let d1f = if dot(d0, d1) < 0.0 {
            scale(d1, -1.0)
        } else {
            d1
        };
        let sep = norm(sub(d0, d1f));
        // sφ ≈ √(sinα²−k²)/s_n; at frac=0.99999 this is ≈ 1.4e-3 ⇒ sep ≈ 2·sφ.
        assert!(
            sep > 1e-4,
            "k={k} (frac={frac}): two-line separation {sep} collapsed prematurely"
        );
    }
}

// ===========================================================================
// Attack 2: AP detection band (apex exactly-vs-near on plane).
//
// k < sinα orientation (axis +z, normal +x, k = 0). Apex at origin; plane
// point at (off,0,0) so |n̂·(apex−p)| = |off|. On-plane (off = 0) ⇒ two Lines;
// |off| ≥ TAU_MODEL ⇒ proper conic (two Hyperbola branches), NOT AP. Sweep
// `off` across the TAU_MODEL band; verify the AP↔proper-conic switch is clean
// and the band edge produces no NaN/panic.
// ===========================================================================

#[test]
fn attack2_ap_detection_band_clean_switch() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);

    let cases = [
        (0.0, Ap::Lines),
        (TAU_MODEL * 0.5, Ap::Lines),
        (TAU_MODEL * 0.9, Ap::Lines),
        (TAU_MODEL * 0.999, Ap::Lines),
        (TAU_MODEL, Ap::Other), // band edge: |off| < TAU is false ⇒ conic
        (TAU_MODEL * 1.001, Ap::Other),
        (TAU_MODEL * 1.1, Ap::Other),
        (TAU_MODEL * 2.0, Ap::Other),
        (TAU_MODEL * 100.0, Ap::Other),
    ];

    for (off, expect) in cases {
        let plane = QuadricSurface::Plane {
            point: Point3::new(off, 0.0, 0.0),
            normal: Vector3::new(1.0, 0.0, 0.0), // k = 0 < sinα
        };
        let curves = intersect(&plane, &cone)
            .unwrap_or_else(|e| panic!("off={off:e}: must not error, got {e:?}"));
        curves.iter().for_each(assert_curve_finite);
        let cls = classify(&curves);
        assert_eq!(
            cls, expect,
            "off={off:e}: AP-detection band classified {cls:?}, expected {expect:?} \
             (curves {curves:?})"
        );
        if expect == Ap::Other {
            // Proper conic for k<sinα is a hyperbola (two branches).
            assert_eq!(
                curves.len(),
                2,
                "off={off:e}: expected two hyperbola branches"
            );
            assert!(
                curves
                    .iter()
                    .all(|c| matches!(c, SsiCurve::Hyperbola { .. })),
                "off={off:e}: expected Hyperbola branches, got {curves:?}"
            );
            // semi_transverse → 0 as off → TAU⁺ but must stay > 0 and finite.
            for c in &curves {
                if let SsiCurve::Hyperbola {
                    semi_transverse,
                    semi_conjugate,
                    ..
                } = c
                {
                    assert!(
                        semi_transverse.is_finite() && *semi_transverse >= 0.0,
                        "off={off:e}: bad semi_transverse {semi_transverse}"
                    );
                    assert!(
                        semi_conjugate.is_finite() && *semi_conjugate > 0.0,
                        "off={off:e}: bad semi_conjugate {semi_conjugate}"
                    );
                }
            }
        }
    }
}

// At the exact band edge (off = TAU_MODEL) the proper-conic hyperbola is still
// on both surfaces (no NaN / panic), confirming the switch is correct and not
// merely "didn't crash". Sampled near the apex (small a) so coords stay tiny.
#[test]
fn attack2b_band_edge_hyperbola_on_surface() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let off = TAU_MODEL; // just outside the AP band
    let plane = QuadricSurface::Plane {
        point: Point3::new(off, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let curves = intersect(&plane, &cone).expect("band-edge conic must be Ok");
    assert_eq!(curves.len(), 2);
    for c in &curves {
        assert!(
            matches!(c, SsiCurve::Hyperbola { .. }),
            "expected hyperbola: {c:?}"
        );
        // Sample the branch over a bounded range; coords O(1) ⇒ absolute oracle.
        let mut m = 0.0_f64;
        for i in 0..128 {
            let t = -2.0 + (i as f64) / 127.0 * 4.0;
            let p = c.eval(t).as_array();
            m = m
                .max(implicit_residual(&plane, p))
                .max(implicit_residual(&cone, p));
        }
        assert!(
            m < TAU_MODEL,
            "band-edge hyperbola off-surface (residual {m})"
        );
    }
}

// ===========================================================================
// Attack 3: two-line correctness (the core). Axis-aligned + oblique non-axis.
//
// For each k<sinα orientation: exactly two Lines, both through apex, each
// |dir·â| = cosα (a cone generator), each dir·n̂ = 0 (in plane), unit, distinct,
// symmetric about m̂ = normalize(â − k·n̂), on BOTH surfaces over bounded t.
// ===========================================================================

#[test]
fn attack3_two_line_correctness_axis_and_oblique() {
    struct Case {
        apex: [f64; 3],
        axis: [f64; 3],
        alpha: f64,
        // a vector ⟂ axis used as the plane normal (k = 0 < sinα ⇒ two lines)
        normal_seed: [f64; 3],
    }
    let cases = [
        // axis-aligned, k = 0
        Case {
            apex: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            alpha: std::f64::consts::FRAC_PI_4,
            normal_seed: [1.0, 0.0, 0.0],
        },
        // oblique non-axis apex + tilted axis, k = 0
        Case {
            apex: [1.0, 2.0, 3.0],
            axis: [1.0, 2.0, 2.0], // non-unit
            alpha: 0.5,
            normal_seed: [0.0, 0.0, 1.0], // crossed with â below to get ⟂ â
        },
        // another oblique, narrower cone
        Case {
            apex: [-4.0, 1.0, 2.0],
            axis: [2.0, -1.0, 3.0],
            alpha: 0.3,
            normal_seed: [1.0, 1.0, 0.0],
        },
    ];

    for (idx, c) in cases.iter().enumerate() {
        let ahat = unit(c.axis);
        let cosa = c.alpha.cos();
        // plane normal ⟂ â ⇒ k = 0 < sinα ⇒ two lines.
        let nrm = unit(cross(ahat, c.normal_seed));
        let cone = QuadricSurface::Cone {
            apex: Point3::from(c.apex),
            axis_dir: Vector3::from(c.axis),
            half_angle: c.alpha,
        };
        let plane = QuadricSurface::Plane {
            point: Point3::from(c.apex), // through the apex
            normal: Vector3::from(nrm),
        };
        let curves = intersect(&plane, &cone)
            .unwrap_or_else(|e| panic!("case {idx}: k=0<sinα ⇒ two lines, got {e:?}"));
        assert_eq!(classify(&curves), Ap::Lines, "case {idx}: {curves:?}");

        let dirs: Vec<[f64; 3]> = curves
            .iter()
            .map(|cu| match cu {
                SsiCurve::Line { dir, .. } => dir.as_array(),
                _ => panic!("case {idx}: expected Line"),
            })
            .collect();
        for d in &dirs {
            approx(norm(*d), 1.0, &format!("case {idx} unit"));
            approx(dot(*d, ahat).abs(), cosa, &format!("case {idx} generator"));
            approx(dot(*d, nrm), 0.0, &format!("case {idx} in-plane"));
        }
        // distinct
        let d1f = if dot(dirs[0], dirs[1]) < 0.0 {
            scale(dirs[1], -1.0)
        } else {
            dirs[1]
        };
        assert!(
            norm(sub(dirs[0], d1f)) > TAU_MODEL,
            "case {idx}: directions not distinct"
        );
        // symmetric about m̂ = normalize(â − k·n̂); here k=0 ⇒ m̂ = â.
        let k = dot(nrm, ahat);
        let mhat = unit(sub(ahat, scale(nrm, k)));
        let bisector = add(dirs[0], d1f);
        assert!(
            norm(bisector) > TAU_MODEL,
            "case {idx}: degenerate bisector"
        );
        parallel_up_to_sign(unit(bisector), mhat);
        // on BOTH surfaces, through the apex.
        assert_line_on_both_through_apex(&curves[0], &plane, &cone, c.apex, 5.0);
        assert_line_on_both_through_apex(&curves[1], &plane, &cone, c.apex, 5.0);
    }
}

// The two lines really are cone∩plane: for axis +z, plane through apex with
// normal ⟂ axis and α=π/4, the directions satisfy the degenerate quadric
// z² = x²+y² restricted to the plane. Use normal +y ⇒ lines lie in y=0 with
// z² = x² (since α=π/4).
#[test]
fn attack3b_two_lines_satisfy_degenerate_quadric() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 1.0, 0.0), // ⟂ axis ⇒ k=0
    };
    let curves = intersect(&plane, &cone).expect("two lines");
    assert_eq!(classify(&curves), Ap::Lines);
    for c in &curves {
        let SsiCurve::Line { dir, .. } = c else {
            panic!()
        };
        let d = dir.as_array();
        approx(d[1], 0.0, "in y=0 plane");
        // degenerate cone quadric on the plane: z² = x² + y² ⇒ z² = x².
        approx(d[2] * d[2], d[0] * d[0] + d[1] * d[1], "z²=x²+y²");
    }
}

// ===========================================================================
// Attack 4: one-line tangent correctness (k = sinα), built two ways.
//
// (a) tilt the NORMAL: n̂ = (1,0,1)/√2 ⇒ k = 1/√2 = sinα(π/4).
// (b) tilt the AXIS: keep n̂ = +z, set axis so n̂·â = sinα. For α=π/4, sinα=√2/2,
//     pick â tilted 45° from +z ⇒ k = cos45° = √2/2 = sinα.
// Each ⇒ exactly one Line through apex, dir ∥ m̂, |dir·â| = cosα, on both surfaces.
// ===========================================================================

#[test]
fn attack4_one_line_tangent_two_constructions() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cosa = alpha.cos();
    let sina = alpha.sin();

    // (a) tilt the normal.
    {
        let nrm = unit([1.0, 0.0, 1.0]); // k = 1/√2 = sinα
        let ahat = [0.0, 0.0, 1.0];
        let cone = z_cone(alpha);
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::from(nrm),
        };
        let curves = intersect(&plane, &cone).expect("k=sinα ⇒ one line");
        assert_eq!(classify(&curves), Ap::Line, "construction (a): {curves:?}");
        let SsiCurve::Line { dir, .. } = curves[0] else {
            panic!()
        };
        let d = dir.as_array();
        approx(dot(d, ahat).abs(), cosa, "(a) generator");
        approx(dot(d, nrm), 0.0, "(a) in-plane");
        let mhat = unit(sub(ahat, scale(nrm, sina)));
        parallel_up_to_sign(d, mhat);
        assert_line_on_both_through_apex(&curves[0], &plane, &cone, [0.0, 0.0, 0.0], 5.0);
    }

    // (b) tilt the axis so n̂·â = sinα, normal +z.
    {
        let nrm = [0.0, 0.0, 1.0];
        // axis tilted 45° from +z in x–z plane ⇒ â = (sin45,0,cos45); n̂·â = cos45 = sinα.
        let beta = std::f64::consts::FRAC_PI_4;
        let axis = [beta.sin(), 0.0, beta.cos()];
        let ahat = unit(axis);
        let k = dot(nrm, ahat);
        assert!((k - sina).abs() < 1e-12, "construction (b) k={k} != sinα");
        let cone = QuadricSurface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::from(axis),
            half_angle: alpha,
        };
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::from(nrm),
        };
        let curves = intersect(&plane, &cone).expect("k=sinα (axis-tilt) ⇒ one line");
        assert_eq!(classify(&curves), Ap::Line, "construction (b): {curves:?}");
        let SsiCurve::Line { dir, .. } = curves[0] else {
            panic!()
        };
        let d = dir.as_array();
        approx(dot(d, ahat).abs(), cosa, "(b) generator");
        approx(dot(d, nrm), 0.0, "(b) in-plane");
        let mhat = unit(sub(ahat, scale(nrm, k)));
        parallel_up_to_sign(d, mhat);
        assert_line_on_both_through_apex(&curves[0], &plane, &cone, [0.0, 0.0, 0.0], 5.0);
    }
}

// ===========================================================================
// Attack 5: extreme half-angles + E1.
//
// Narrow cone (α near TAU) and flat cone (α near π/2−TAU), apex on plane:
//   - k < sinα ⇒ two lines (relative on-surface check, coords kept O(1));
//   - k > sinα ⇒ point.
// E1 still fires for α outside the valid open interval.
// ===========================================================================

#[test]
fn attack5_extreme_half_angles_ap() {
    // Narrow cone: α small. sinα ≈ α. Use a plane ⟂ axis-projection so k=0<sinα
    // ⇒ two lines (the lines are nearly parallel to the axis for a thin cone).
    let alpha_narrow = 1.0e-4_f64; // ≫ TAU_MODEL, very thin cone
    {
        let cone = z_cone(alpha_narrow);
        let cosa = alpha_narrow.cos();
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(1.0, 0.0, 0.0), // k=0<sinα
        };
        let curves = intersect(&plane, &cone).expect("narrow cone, k=0 ⇒ two lines");
        assert_eq!(classify(&curves), Ap::Lines, "narrow: {curves:?}");
        for c in &curves {
            let SsiCurve::Line { dir, .. } = c else {
                panic!()
            };
            let d = dir.as_array();
            approx(norm(d), 1.0, "narrow unit");
            approx(dot(d, [0.0, 0.0, 1.0]).abs(), cosa, "narrow generator");
            // For a thin cone the generators are ≈ ±axis: |d·ẑ| ≈ 1.
            assert!(dot(d, [0.0, 0.0, 1.0]).abs() > 0.999);
        }
        // k > sinα ⇒ point. Use a steeper plane (k = 0.5 ≫ sinα ≈ 1e-4).
        let k = 0.5;
        let (plane_pt, cone_pt, _, _) = ap_setup(k, alpha_narrow);
        assert_eq!(
            classify(&intersect(&plane_pt, &cone_pt).unwrap()),
            Ap::Point,
            "narrow cone, k≫sinα ⇒ point"
        );
    }

    // Flat cone: α near π/2. sinα ≈ 1, cosα ≈ 0. k < sinα is easy (any k<1).
    let alpha_flat = std::f64::consts::FRAC_PI_2 - 1.0e-3;
    {
        let cone = z_cone(alpha_flat);
        let cosa = alpha_flat.cos();
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(1.0, 0.0, 0.0), // k=0<sinα
        };
        let curves = intersect(&plane, &cone).expect("flat cone, k=0 ⇒ two lines");
        assert_eq!(classify(&curves), Ap::Lines, "flat: {curves:?}");
        for c in &curves {
            let SsiCurve::Line { dir, .. } = c else {
                panic!()
            };
            let d = dir.as_array();
            approx(norm(d), 1.0, "flat unit");
            approx(dot(d, [0.0, 0.0, 1.0]).abs(), cosa, "flat generator");
            // Flat cone generators are nearly ⟂ axis: |d·ẑ| ≈ 0.
            assert!(dot(d, [0.0, 0.0, 1.0]).abs() < 1e-2);
        }
        // k > sinα for a flat cone needs k > ~0.9999995 — extremely steep. Skip
        // the point sub-case here (numerically delicate); covered for π/4 above.
    }

    // E1 still fires outside the valid open α interval, even on an AP plane.
    let ap_plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let bad_lo = z_cone(TAU_MODEL * 0.5);
    let bad_hi = z_cone(std::f64::consts::FRAC_PI_2 - TAU_MODEL * 0.5);
    assert_eq!(
        intersect(&ap_plane, &bad_lo),
        Err(SsiError::DegenerateInput),
        "α ≤ TAU_MODEL on an AP plane must still be DegenerateInput"
    );
    assert_eq!(
        intersect(&ap_plane, &bad_hi),
        Err(SsiError::DegenerateInput),
        "α ≥ π/2 − TAU_MODEL on an AP plane must still be DegenerateInput"
    );
}

// ===========================================================================
// Attack 6: I4 symmetry + I5 determinism for AP-line and AP-lines.
// ===========================================================================

// Canonical-sign + sorted direction set (sign-agnostic, order-agnostic).
fn dir_set(curves: &[SsiCurve]) -> Vec<[f64; 3]> {
    let mut v: Vec<[f64; 3]> = curves
        .iter()
        .map(|c| match *c {
            SsiCurve::Line { dir, .. } => {
                let d = dir.as_array();
                let s = if d[0].abs() > TAU_MODEL {
                    d[0].signum()
                } else if d[1].abs() > TAU_MODEL {
                    d[1].signum()
                } else {
                    d[2].signum()
                };
                scale(d, s)
            }
            ref other => panic!("expected Line, got {other:?}"),
        })
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

#[test]
fn attack6_symmetry_ap_line_and_lines() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);

    // AP-lines (k=0).
    let p_lines = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let ab = intersect(&p_lines, &cone).unwrap();
    let ba = intersect(&cone, &p_lines).unwrap();
    assert_eq!(ab.len(), 2);
    assert_eq!(ba.len(), 2);
    let sa = dir_set(&ab);
    let sb = dir_set(&ba);
    for (a, b) in sa.iter().zip(sb.iter()) {
        assert!(norm(sub(*a, *b)) < TAU_MODEL, "AP-lines dir sets differ");
    }

    // AP-line (k=sinα).
    let nrm = unit([1.0, 0.0, 1.0]);
    let p_line = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::from(nrm),
    };
    let ab = intersect(&p_line, &cone).unwrap();
    let ba = intersect(&cone, &p_line).unwrap();
    assert_eq!(ab.len(), 1);
    assert_eq!(ba.len(), 1);
    assert!(
        norm(sub(dir_set(&ab)[0], dir_set(&ba)[0])) < TAU_MODEL,
        "AP-line dir set differs across argument order"
    );
}

#[test]
fn attack6b_determinism_ap_line_and_lines() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);

    // AP-lines: byte-identical (incl. +ŵ-first order) across repeats.
    let p_lines = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let first = intersect(&p_lines, &cone);
    for _ in 0..8 {
        assert_eq!(
            intersect(&p_lines, &cone),
            first,
            "AP-lines output not deterministic (order or value drift)"
        );
    }

    // AP-line: byte-identical across repeats.
    let p_line = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::from(unit([1.0, 0.0, 1.0])),
    };
    let first_line = intersect(&p_line, &cone);
    for _ in 0..8 {
        assert_eq!(
            intersect(&p_line, &cone),
            first_line,
            "AP-line output not deterministic"
        );
    }
}

// ===========================================================================
// SSI3 AP-fixture migration review (independent re-derivation).
//
// Re-derives the two migrated ssi3.rs fixtures + the attack6 band intent from
// scratch, confirming the conic-type→new-result mapping is correct and the
// "band does not swallow valid sections" structural intent is preserved (not
// made vacuous). A failure here means the migration is unfaithful.
// ===========================================================================

#[test]
fn migration_review_ssi3_ap_perp_is_point() {
    // ssi3.rs::ap_through_apex_perp_is_point: n̂=+z, axis +z, apex on z=0.
    // k = n̂·â = 1 > sinα(π/4) and s_n = 0 ⇒ AP-pt⊥ ⇒ Ok(vec![]). FAITHFUL.
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let cone = z_cone(std::f64::consts::FRAC_PI_4);
    assert_eq!(
        intersect(&plane, &cone),
        Ok(vec![]),
        "migration: perp AP fixture must map to the point result Ok(vec![])"
    );
}

#[test]
fn migration_review_ssi3_ap_oblique_is_point() {
    // ssi3.rs::ap_through_apex_oblique_is_point: n̂=(0.3,0,1), axis +z, α=π/4,
    // apex on plane. k = |n̂·â| = 1/√1.09 ≈ 0.958 > sinα ≈ 0.707 ⇒ AP-pt ⇒
    // Ok(vec![]). Independently confirm k > sinα so the point mapping is correct.
    let alpha = std::f64::consts::FRAC_PI_4;
    let nrm = unit([0.3, 0.0, 1.0]);
    let k = dot(nrm, [0.0, 0.0, 1.0]).abs();
    assert!(
        k > alpha.sin(),
        "migration sanity: k={k} must exceed sinα={} for the point mapping",
        alpha.sin()
    );
    let apex = [1.0, 2.0, 3.0];
    let plane = QuadricSurface::Plane {
        point: Point3::from(apex),
        normal: Vector3::from(nrm),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    assert_eq!(
        intersect(&plane, &cone),
        Ok(vec![]),
        "migration: oblique-steeper AP fixture must map to Ok(vec![])"
    );
}

#[test]
fn migration_review_attack6_band_intent_preserved() {
    // ssi3_adversary::attack6 structural intent: the AP gate must NOT swallow
    // valid near-apex sections beyond its TAU band. Re-derive: on-apex ⟂ plane ⇒
    // point; just-inside ⇒ point; just-OUTSIDE the band ⇒ a VALID bounded curve
    // (a small circle). The "valid section survives" half must be non-vacuous.
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);

    // on-apex ⟂ plane ⇒ point.
    assert_eq!(
        intersect(
            &QuadricSurface::Plane {
                point: Point3::new(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
            },
            &cone
        ),
        Ok(vec![]),
        "on-apex ⟂ plane ⇒ point"
    );

    // just-inside the band ⇒ still point.
    assert_eq!(
        intersect(
            &QuadricSurface::Plane {
                point: Point3::new(0.0, 0.0, TAU_MODEL * 0.5),
                normal: Vector3::new(0.0, 0.0, 1.0),
            },
            &cone
        ),
        Ok(vec![]),
        "within-band ⇒ point"
    );

    // just-outside the band ⇒ a valid circle (the intent that must NOT be
    // vacuous). h = 100·TAU ⇒ radius = h·tanα.
    let h = TAU_MODEL * 100.0;
    let curves = intersect(
        &QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, h),
            normal: Vector3::new(0.0, 0.0, 1.0),
        },
        &cone,
    )
    .expect("near-apex but valid section must NOT be swallowed by AP");
    let SsiCurve::Circle { radius, .. } = curves[0] else {
        panic!(
            "migration intent broken: expected a valid Circle, got {:?}",
            curves[0]
        );
    };
    let expect = h * alpha.tan();
    assert!(
        (radius - expect).abs() / expect < 1e-9,
        "migration intent: valid near-apex circle radius {radius} != {expect}"
    );
}
