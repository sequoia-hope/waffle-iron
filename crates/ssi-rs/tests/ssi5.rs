//! PR-SSI5 — RED tests for the plane∩cone THROUGH-APEX degenerate conics.
//!
//! A cutting plane through the cone apex meets the infinite double cone in a
//! degenerate conic: a single point (the apex), one tangent generator, or two
//! crossed generators. PR-SSI3/4 gated this AP branch as `Err(DegenerateInput)`;
//! PR-SSI5 replaces it with the point/line/two-lines contract (reusing
//! `SsiCurve::Line`; the point case is `Ok(vec![])`). These tests target the
//! NEW behavior via the public `intersect` dispatcher (`plane_cone` is private)
//! and FAIL while production still returns `Err(DegenerateInput)` (RED).
//!
//! Spec: specs/ssi_pr_ssi5_plane_cone_through_apex.md
//! AP branch table (apex on cutting plane, `|n̂·(apex − p)| < TAU_MODEL`):
//!   AP-pt⊥  plane ⟂ axis (s_n < TAU)              → Ok(vec![])  (apex only)
//!   AP-line tangent generator (|k| = sinα)        → one Line  { apex, m̂ }
//!   AP-lines crossed generators (|k| < sinα)      → two Lines through the apex
//!   AP-pt   steeper than cone (sinα < |k| < 1)    → Ok(vec![])  (apex only)
//! Invariants: I1 (on-surface, cone RADIAL residual + line-through-apex),
//! I2 (analytical geometry: generator angle |dir·â| = cosα, symmetry about m̂),
//! I4 (symmetry intersect(p,c) == intersect(c,p)), I5 (determinism, +ŵ first).

use cad_primitives::{Point3, Vector3, TAU_MODEL};
use ssi_rs::{intersect, QuadricSurface, SsiCurve};

// ---------------------------------------------------------------------------
// Inline vector helpers (cad-primitives has no dot/cross/norm) — mirrors ssi3.
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
// On-surface oracle (I1). Cone RADIAL residual `| |(x−apex)−h·â| − |h|·tanα |`
// (a length, h = (x−apex)·â) + plane residual `|n̂·(x − p)|`. Identical math to
// ssi3.rs::implicit_residual. â/n̂ normalized defensively.
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

// Sample a Line over t ∈ [−3, 3]; assert every sample lies on BOTH surfaces
// within TAU_MODEL, and assert eval(0) ≈ apex (the line passes through the apex).
fn assert_line_on_both_surfaces_through_apex(
    line: &SsiCurve,
    a: &QuadricSurface,
    b: &QuadricSurface,
    apex: [f64; 3],
) {
    let SsiCurve::Line { .. } = line else {
        panic!("expected Line, got {line:?}");
    };
    const N: usize = 64;
    for i in 0..N {
        let t = -3.0 + (i as f64) / ((N - 1) as f64) * 6.0;
        let p = line.eval(t).as_array();
        let ra = implicit_residual(a, p);
        let rb = implicit_residual(b, p);
        assert!(
            ra < TAU_MODEL,
            "line sample t={t} at {p:?} off surface A (residual {ra} >= TAU_MODEL)"
        );
        assert!(
            rb < TAU_MODEL,
            "line sample t={t} at {p:?} off surface B (residual {rb} >= TAU_MODEL)"
        );
    }
    // eval(0) = point = apex.
    let p0 = line.eval(0.0).as_array();
    assert!(
        norm(sub(p0, apex)) < TAU_MODEL,
        "line does not pass through apex: eval(0) = {p0:?}, apex = {apex:?}"
    );
}

fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < TAU_MODEL, "expected {a} ≈ {b}");
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

fn expect_lines(curves: &[SsiCurve]) -> Vec<(Point3, Vector3)> {
    curves
        .iter()
        .map(|c| match *c {
            SsiCurve::Line { point, dir } => (point, dir),
            ref other => panic!("expected Line, got {other:?}"),
        })
        .collect()
}

// ===========================================================================
// AP-lines — two crossed generators (|k| < sinα). Apex origin, axis +z, α=π/4.
// Plane x = 0 (normal (1,0,0) ⟂ axis ⇒ k = 0 < sinα) ⇒ two lines. The verified
// directions are (0, ∓1, 1)/√2: in the x=0 plane, on the cone (z² = y²),
// symmetric about m̂ = (0,0,1). (I1, I2.)
// ===========================================================================

#[test]
fn ap_lines_two_crossed_generators() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let apex = [0.0, 0.0, 0.0];
    let axis = [0.0, 0.0, 1.0];
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),   // through the apex
        normal: Vector3::new(1.0, 0.0, 0.0), // ⟂ axis ⇒ k = 0 < sinα
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(axis),
        half_angle: alpha,
    };

    let curves = intersect(&plane, &cone).expect("through-apex, k<sinα ⇒ two lines");
    let lines = expect_lines(&curves);
    assert_eq!(lines.len(), 2, "expected exactly two Lines, got {curves:?}");

    let cosa = alpha.cos(); // 1/√2
    let ahat = unit(axis);
    let d1 = lines[0].1.as_array();
    let d2 = lines[1].1.as_array();

    for ((pt, _), d) in lines.iter().zip([d1, d2]) {
        // through the apex
        assert!(
            norm(sub(pt.as_array(), apex)) < TAU_MODEL,
            "line point {:?} != apex",
            pt.as_array()
        );
        // unit direction
        approx(norm(d), 1.0);
        // generator on the cone: |dir·â| = cosα
        approx(dot(d, ahat).abs(), cosa);
        // in the x = 0 plane: dir·n̂ = 0 (n̂ = +x)
        approx(d[0], 0.0);
        // on the cone surface z² = y² (since x = 0 and α = π/4)
        approx(d[2] * d[2], d[1] * d[1]);
    }

    // I1: each line on BOTH surfaces + through the apex.
    assert_line_on_both_surfaces_through_apex(&curves[0], &plane, &cone, apex);
    assert_line_on_both_surfaces_through_apex(&curves[1], &plane, &cone, apex);

    // The two directions are (0, ∓1, 1)/√2 up to sign: both lie in x=0 and
    // satisfy z² = y² (checked above) ⇒ each ∥ (0,1,1) or (0,-1,1).
    let g_pos = unit([0.0, 1.0, 1.0]);
    let g_neg = unit([0.0, -1.0, 1.0]);
    for d in [d1, d2] {
        let matches_pos = norm(cross(d, g_pos)) < TAU_MODEL;
        let matches_neg = norm(cross(d, g_neg)) < TAU_MODEL;
        assert!(
            matches_pos || matches_neg,
            "dir {d:?} is not a verified generator (0,∓1,1)/√2"
        );
    }

    // distinct
    assert!(
        norm(sub(d1, d2)) > TAU_MODEL,
        "the two line directions must be distinct: {d1:?} vs {d2:?}"
    );

    // symmetric about m̂ = (0,0,1): normalize(d₁ + d₂) ∥ m̂.
    // (Sign-fold the two dirs to the same nappe so the sum is non-degenerate.)
    let d2_folded = if dot(d1, d2) < 0.0 {
        scale(d2, -1.0)
    } else {
        d2
    };
    let bisector = add(d1, d2_folded);
    assert!(
        norm(bisector) > TAU_MODEL,
        "bisector degenerate: {d1:?} + {d2_folded:?}"
    );
    parallel_up_to_sign(unit(bisector), [0.0, 0.0, 1.0]);
}

// ===========================================================================
// AP-line — one tangent generator (|k| = sinα). Apex origin, axis +z, α=π/4.
// Plane normal (1,0,1)/√2 ⇒ k = n̂·â = (1/√2) = sinα EXACTLY ⇒ one line.
// dir ∥ m̂ = normalize(â − k·n̂); here â − k·n̂ = (−1/2, 0, 1/2) ∥ (−1,0,1).
// |dir·â| = cosα. (I1, I2.)
// ===========================================================================

#[test]
fn ap_line_one_tangent_generator() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let apex = [0.0, 0.0, 0.0];
    let axis = [0.0, 0.0, 1.0];
    let nrm = unit([1.0, 0.0, 1.0]); // k = n̂·ẑ = 1/√2 = sinα
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0), // through the apex
        normal: Vector3::from(nrm),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(axis),
        half_angle: alpha,
    };

    let curves = intersect(&plane, &cone).expect("through-apex, k=sinα ⇒ one tangent line");
    let lines = expect_lines(&curves);
    assert_eq!(lines.len(), 1, "expected exactly one Line, got {curves:?}");

    let (pt, dir) = lines[0];
    let d = dir.as_array();
    let ahat = unit(axis);
    let cosa = alpha.cos();

    // through the apex
    assert!(
        norm(sub(pt.as_array(), apex)) < TAU_MODEL,
        "tangent line point {:?} != apex",
        pt.as_array()
    );
    // unit dir, generator angle, dir ∥ m̂ = normalize(â − k·n̂) ∥ (−1,0,1).
    approx(norm(d), 1.0);
    approx(dot(d, ahat).abs(), cosa);
    parallel_up_to_sign(d, unit([-1.0, 0.0, 1.0]));
    // in the plane: dir·n̂ = 0.
    approx(dot(d, nrm), 0.0);

    // I1: on BOTH surfaces + through the apex.
    assert_line_on_both_surfaces_through_apex(&curves[0], &plane, &cone, apex);
}

// ===========================================================================
// AP-pt⊥ — plane ⟂ axis through the apex (s_n < TAU) ⇒ apex only ⇒ Ok(vec![]).
// Apex origin, axis +z, α=π/4. Plane z = 0 (normal +z, k = 1 ⇒ s_n = 0).
// ===========================================================================

#[test]
fn ap_pt_perp_is_empty() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),   // through the apex
        normal: Vector3::new(0.0, 0.0, 1.0), // ⟂ axis ⇒ s_n = 0
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    assert_eq!(
        intersect(&plane, &cone),
        Ok(vec![]),
        "plane ⟂ axis through apex meets the cone only at the apex ⇒ Ok(vec![])"
    );
}

// ===========================================================================
// AP-pt — oblique, steeper than the cone (sinα < |k| < 1) ⇒ apex only ⇒ empty.
// Apex origin, axis +z, α=π/4 (sinα = √2/2 ≈ 0.707). Plane normal tilted 30°
// from +z: n̂ = (sin30°, 0, cos30°) ⇒ k = cos30° ≈ 0.866 > sinα ⇒ point.
// ===========================================================================

#[test]
fn ap_pt_oblique_is_empty() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let theta = 30.0_f64.to_radians(); // tilt of n̂ from +z ⇒ k = cosθ = cos30°
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0), // through the apex
        normal: Vector3::new(theta.sin(), 0.0, theta.cos()),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    // sanity: k = cos30° ≈ 0.866 > sinα ≈ 0.707.
    assert!(theta.cos() > alpha.sin());
    assert_eq!(
        intersect(&plane, &cone),
        Ok(vec![]),
        "oblique plane steeper than the cone through apex ⇒ point ⇒ Ok(vec![])"
    );
}

// ===========================================================================
// Oblique non-axis apex (two lines). Apex (1,2,3); axis tilted (1,2,2)/3; a
// plane through that apex with normal ⟂ axis ⇒ k = 0 < sinα ⇒ two crossed
// generators. Each Line is a generator on the cone (|dir·â| = cosα), lies in
// the cutting plane (dir·n̂ = 0), and passes through the apex. (I1, I2.)
// ===========================================================================

#[test]
fn ap_lines_oblique_non_axis_apex() {
    let alpha = 0.5_f64; // sinα ≈ 0.479, cosα ≈ 0.878
    let apex = [1.0, 2.0, 3.0];
    let axis = [1.0, 2.0, 2.0]; // |·| = 3, non-unit on input
    let ahat = unit(axis);
    // A plane normal ⟂ â ⇒ k = 0 < sinα ⇒ two lines. Pick any vector ⟂ â.
    let nrm = unit(cross(ahat, [0.0, 0.0, 1.0]));
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(axis),
        half_angle: alpha,
    };
    let plane = QuadricSurface::Plane {
        point: Point3::from(apex), // plane through the apex
        normal: Vector3::from(nrm),
    };

    let curves = intersect(&plane, &cone).expect("oblique apex, k=0<sinα ⇒ two lines");
    let lines = expect_lines(&curves);
    assert_eq!(lines.len(), 2, "expected two Lines, got {curves:?}");

    let cosa = alpha.cos();
    for (pt, dir) in &lines {
        let d = dir.as_array();
        // through the apex
        assert!(
            norm(sub(pt.as_array(), apex)) < TAU_MODEL,
            "line point {:?} != apex {apex:?}",
            pt.as_array()
        );
        approx(norm(d), 1.0); // unit
        approx(dot(d, ahat).abs(), cosa); // generator on the cone
        approx(dot(d, nrm), 0.0); // in the cutting plane
    }
    // distinct
    let d1 = lines[0].1.as_array();
    let d2 = lines[1].1.as_array();
    assert!(
        norm(sub(d1, d2)) > TAU_MODEL,
        "two generators must be distinct"
    );

    // I1: each line on BOTH surfaces + through the apex.
    assert_line_on_both_surfaces_through_apex(&curves[0], &plane, &cone, apex);
    assert_line_on_both_surfaces_through_apex(&curves[1], &plane, &cone, apex);
}

// ===========================================================================
// I4 — symmetry: intersect(plane, cone) == intersect(cone, plane) for the
// AP-line and AP-lines cases. Compared as an unordered direction set up to sign.
// ===========================================================================

// Direction set of a Line vec, each folded to a canonical sign (first nonzero
// component positive) so the comparison is sign-agnostic; sorted for ordering
// independence.
fn dir_set(curves: &[SsiCurve]) -> Vec<[f64; 3]> {
    let mut v: Vec<[f64; 3]> = curves
        .iter()
        .map(|c| match *c {
            SsiCurve::Line { dir, .. } => {
                let d = dir.as_array();
                // canonical sign: make the first sufficiently-nonzero comp > 0.
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

fn assert_same_dir_set(ab: &[SsiCurve], ba: &[SsiCurve]) {
    let sa = dir_set(ab);
    let sb = dir_set(ba);
    assert_eq!(
        sa.len(),
        sb.len(),
        "different line counts: {ab:?} vs {ba:?}"
    );
    for (a, b) in sa.iter().zip(sb.iter()) {
        assert!(
            norm(sub(*a, *b)) < TAU_MODEL,
            "direction sets differ: {a:?} vs {b:?}"
        );
    }
}

#[test]
fn symmetry_ap_lines() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0), // k = 0 < sinα ⇒ two lines
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let ab = intersect(&plane, &cone).expect("two lines");
    let ba = intersect(&cone, &plane).expect("two lines (swapped)");
    assert_eq!(ab.len(), 2);
    assert_eq!(ba.len(), 2);
    assert_same_dir_set(&ab, &ba);
}

#[test]
fn symmetry_ap_line() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let nrm = unit([1.0, 0.0, 1.0]); // k = sinα ⇒ one tangent line
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::from(nrm),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let ab = intersect(&plane, &cone).expect("one line");
    let ba = intersect(&cone, &plane).expect("one line (swapped)");
    assert_eq!(ab.len(), 1);
    assert_eq!(ba.len(), 1);
    assert_same_dir_set(&ab, &ba);
}

// ===========================================================================
// I5 — determinism: identical AP-lines inputs twice ⇒ byte-identical output
// (struct fields via PartialEq, including the +ŵ-first ordering) and identical
// at a fixed eval parameter.
// ===========================================================================

#[test]
fn determinism_ap_lines_identical() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let mk = || {
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(1.0, 0.0, 0.0),
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
    // Byte-identical structurally (PartialEq over the exact fields), including
    // the deterministic two-line order (+ŵ first).
    assert_eq!(first, second, "AP two-line output must be deterministic");

    // And identical at a fixed eval parameter on each line.
    let cf = first.unwrap();
    let cs = second.unwrap();
    assert_eq!(cf.len(), 2);
    assert_eq!(cs.len(), 2);
    let t = 0.7;
    assert_eq!(cf[0].eval(t).as_array(), cs[0].eval(t).as_array());
    assert_eq!(cf[1].eval(t).as_array(), cs[1].eval(t).as_array());
}
