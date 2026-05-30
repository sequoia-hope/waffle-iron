//! PR-SSI4 — RED tests for the plane∩cone UNBOUNDED sections (parabola +
//! hyperbola), completing the four proper conic sections of pair #3.
//!
//! These target the not-yet-existing `SsiCurve` variants:
//!   `SsiCurve::Parabola  { vertex, normal, axis_dir, focal_length }`
//!   `SsiCurve::Hyperbola { center, normal, major_axis, semi_transverse,
//!                          semi_conjugate }`
//! reached through the public `intersect` dispatcher (the `plane_cone` solver
//! fn is private). No new surfaces.
//!
//! Spec: specs/ssi_pr_ssi4_plane_cone_unbounded.md
//! Branch table (PARA/HYPE replace PR-SSI3's `AnalyticalSolutionNotAvailable`):
//!   PARA  parabola  (exactly one |gd_±| < TAU, one generator ∥ plane) → one  Parabola
//!   HYPE  hyperbola (gd₊.signum() ≠ gd₋.signum(), opposite nappes)    → two  Hyperbola
//!   AP    through-apex (apex on cutting plane)                        → Err(DegenerateInput)
//! Invariants: I1 (on-surface, cone RADIAL residual + plane residual),
//! I2 (analytical geometry), I3 (branch coverage), I4 (symmetry),
//! I5 (determinism).

use cad_primitives::{Point3, Vector3, TAU_MODEL};
use ssi_rs::{intersect, QuadricSurface, SsiCurve};

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
//
// The unbounded curves (Parabola/Hyperbola) are sampled over a BOUNDED `t`
// range so coordinates stay small (well inside the absolute oracle's regime):
//   Parabola  : t ∈ [−3, 3]
//   Hyperbola : each branch t ∈ [−2, 2]
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

/// Sample `curve` at `N` parameters over a curve-type-appropriate (bounded for
/// the unbounded conics) range, asserting each sample lies on BOTH surfaces.
/// The `match` is exhaustive over the extended `SsiCurve`.
fn assert_on_both_surfaces(curve: &SsiCurve, a: &QuadricSurface, b: &QuadricSurface) {
    const N: usize = 64;
    for i in 0..N {
        let t = match curve {
            SsiCurve::Circle { .. } | SsiCurve::Ellipse { .. } => {
                (i as f64) / (N as f64) * std::f64::consts::TAU
            }
            SsiCurve::Line { .. } => -5.0 + (i as f64) / ((N - 1) as f64) * 10.0,
            // Bounded [−3, 3] keeps the parabola's coordinates small.
            SsiCurve::Parabola { .. } => (i as f64) / ((N - 1) as f64) * 6.0 - 3.0,
            // Bounded [−2, 2] per branch (cosh/sinh grow fast).
            SsiCurve::Hyperbola { .. } => (i as f64) / ((N - 1) as f64) * 4.0 - 2.0,
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
// Extractors / approx helpers.
// ---------------------------------------------------------------------------

#[allow(clippy::type_complexity)]
fn expect_single_parabola(curves: &[SsiCurve]) -> (Point3, Vector3, Vector3, f64) {
    assert_eq!(
        curves.len(),
        1,
        "expected exactly one curve, got {curves:?}"
    );
    match curves[0] {
        SsiCurve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } => (vertex, normal, axis_dir, focal_length),
        ref other => panic!("expected Parabola, got {other:?}"),
    }
}

#[allow(clippy::type_complexity)]
fn expect_hyperbola(c: &SsiCurve) -> (Point3, Vector3, Vector3, f64, f64) {
    match c {
        SsiCurve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => (
            *center,
            *normal,
            *major_axis,
            *semi_transverse,
            *semi_conjugate,
        ),
        other => panic!("expected Hyperbola, got {other:?}"),
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
// PARA — one generator ∥ plane (|n̂·â| = sinα) ⇒ one Parabola (I2, I1).
// Spec verified: α=π/4, n̂=(1,0,1)/√2, plane through (0,0,1) →
//   vertex ≈ (0.5,0,0.5), f ≈ 1/(2√2) ≈ 0.35355, axis_dir ∥ (−1,0,1)/√2,
//   eval(1) = (0,−1,1) (on both surfaces).
// ---------------------------------------------------------------------------

#[test]
fn para_yields_one_parabola() {
    let alpha = std::f64::consts::FRAC_PI_4; // sinα = cosα = √2/2.
    let nrm = unit([1.0, 0.0, 1.0]); // |n̂·â| = 1/√2 = sinα ⇒ parabola.
    let ppt = [0.0, 0.0, 1.0];
    let apex = [0.0, 0.0, 0.0];
    let axis = [0.0, 0.0, 1.0];

    let plane = QuadricSurface::Plane {
        point: Point3::from(ppt),
        normal: Vector3::from(nrm),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(axis),
        half_angle: alpha,
    };
    let curves = intersect(&plane, &cone).expect("parabola section: one Parabola");
    let (vertex, normal, axis_dir, focal_length) = expect_single_parabola(&curves);

    // I1: on-surface oracle over the bounded t range.
    assert_on_both_surfaces(&curves[0], &plane, &cone);

    // I2: vertex on cone AND in plane.
    assert!(
        implicit_residual(&cone, vertex.as_array()) < TAU_MODEL,
        "parabola vertex {vertex:?} not on cone"
    );
    assert!(
        implicit_residual(&plane, vertex.as_array()) < TAU_MODEL,
        "parabola vertex {vertex:?} not in plane"
    );

    // Spec's verified values.
    approx_point(vertex, [0.5, 0.0, 0.5]);
    approx(focal_length, 1.0 / (2.0 * 2.0_f64.sqrt())); // 1/(2√2) ≈ 0.35355
    parallel_up_to_sign(axis_dir.as_array(), unit([-1.0, 0.0, 1.0]));

    // axis_dir is unit, in-plane (⟂ normal), focal_length > 0 finite.
    approx(norm(axis_dir.as_array()), 1.0);
    assert!(
        dot(normal.as_array(), axis_dir.as_array()).abs() < TAU_MODEL,
        "axis_dir must be in-plane (|n̂·axis_dir| ≥ TAU): {}",
        dot(normal.as_array(), axis_dir.as_array()).abs()
    );
    assert!(focal_length.is_finite() && focal_length > 0.0);

    // normal is the unit plane normal.
    parallel_up_to_sign(normal.as_array(), nrm);
    approx(norm(normal.as_array()), 1.0);

    // Independent spot-check of the spec's verified eval(1) = (0,−1,1).
    let e1 = curves[0].eval(1.0);
    approx_point(e1, [0.0, -1.0, 1.0]);
    assert!(implicit_residual(&cone, e1.as_array()) < TAU_MODEL);
    assert!(implicit_residual(&plane, e1.as_array()) < TAU_MODEL);
}

// ---------------------------------------------------------------------------
// HYPE — plane parallel to the axis (|n̂·â| = 0 < sinα) ⇒ two Hyperbola (I2, I1).
// Spec verified: α=π/4, plane x=1 (n̂=(1,0,0)) → center (1,0,0), a=b=1,
//   branch vertices (1,0,±1); the curve lies on z² − y² = 1 at x=1.
// ---------------------------------------------------------------------------

#[test]
fn hype_yields_two_hyperbola_branches() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let nrm = [1.0, 0.0, 0.0]; // ⟂ axis ⇒ |n̂·â| = 0 < sinα ⇒ hyperbola.
    let ppt = [1.0, 0.0, 0.0];
    let apex = [0.0, 0.0, 0.0];
    let axis = [0.0, 0.0, 1.0];

    let plane = QuadricSurface::Plane {
        point: Point3::from(ppt),
        normal: Vector3::from(nrm),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(axis),
        half_angle: alpha,
    };
    let curves = intersect(&plane, &cone).expect("hyperbola section: two Hyperbola");
    assert_eq!(
        curves.len(),
        2,
        "infinite double cone ⇒ two hyperbola branches, got {curves:?}"
    );

    let (c0, n0, m0, a0, b0) = expect_hyperbola(&curves[0]);
    let (c1, n1, m1, a1, b1) = expect_hyperbola(&curves[1]);

    // I1: BOTH branches on both surfaces over their bounded t range.
    assert_on_both_surfaces(&curves[0], &plane, &cone);
    assert_on_both_surfaces(&curves[1], &plane, &cone);

    // Shared center / a / b / normal across the two branches.
    approx_point(c0, c1.as_array());
    approx(a0, a1);
    approx(b0, b1);
    parallel_up_to_sign(n0.as_array(), n1.as_array());

    // Spec's verified values.
    approx_point(c0, [1.0, 0.0, 0.0]);
    approx(a0, 1.0);
    approx(b0, 1.0);

    // a, b > 0 finite.
    assert!(a0.is_finite() && a0 > 0.0 && b0.is_finite() && b0 > 0.0);

    // major_axis is unit, in-plane (⟂ normal), and OPPOSITE signs on the two
    // branches.
    for (m, n) in [(m0, n0), (m1, n1)] {
        approx(norm(m.as_array()), 1.0);
        assert!(
            dot(n.as_array(), m.as_array()).abs() < TAU_MODEL,
            "major_axis must be in-plane"
        );
    }
    // Opposite signs: m0 ≈ −m1.
    approx_point(
        Point3::from(add(m0.as_array(), m1.as_array())),
        [0.0, 0.0, 0.0],
    );

    // Each branch vertex center ± a·major_axis on the cone AND in the plane.
    let v0 = add(c0.as_array(), scale(m0.as_array(), a0));
    let v1 = add(c1.as_array(), scale(m1.as_array(), a1));
    for v in [v0, v1] {
        assert!(
            implicit_residual(&cone, v) < TAU_MODEL,
            "branch vertex {v:?} not on cone"
        );
        assert!(
            implicit_residual(&plane, v) < TAU_MODEL,
            "branch vertex {v:?} not in plane"
        );
    }
    // Verified branch vertices (1,0,±1) as an unordered pair.
    let v_hi = if v0[2] >= v1[2] { v0 } else { v1 };
    let v_lo = if v0[2] >= v1[2] { v1 } else { v0 };
    approx_point(Point3::from(v_hi), [1.0, 0.0, 1.0]);
    approx_point(Point3::from(v_lo), [1.0, 0.0, -1.0]);

    // Sanity: the curve lies on z² − y² = 1 at x = 1 (sample one branch).
    for i in 0..16 {
        let t = (i as f64) / 15.0 * 4.0 - 2.0;
        let p = curves[0].eval(t).as_array();
        approx(p[0], 1.0); // in the plane x = 1
        approx(p[2] * p[2] - p[1] * p[1], 1.0); // z² − y² = 1
    }
}

// ---------------------------------------------------------------------------
// HYPE — two-nappe check: the +major_axis branch vertex and the −major_axis
// branch vertex lie on OPPOSITE sides of the apex along the axis (genuinely
// the two nappes of the double cone).
// ---------------------------------------------------------------------------

#[test]
fn hype_branches_on_opposite_nappes() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let apex = [0.0, 0.0, 0.0];
    let axis = [0.0, 0.0, 1.0];
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(axis),
        half_angle: alpha,
    };
    let curves = intersect(&plane, &cone).expect("two Hyperbola");
    assert_eq!(curves.len(), 2);

    let (c0, _, m0, a0, _) = expect_hyperbola(&curves[0]);
    let (c1, _, m1, a1, _) = expect_hyperbola(&curves[1]);

    let ahat = unit(axis);
    // Branch vertices, projected onto the axis through the apex.
    let v0 = add(c0.as_array(), scale(m0.as_array(), a0));
    let v1 = add(c1.as_array(), scale(m1.as_array(), a1));
    let h0 = dot(sub(v0, apex), ahat);
    let h1 = dot(sub(v1, apex), ahat);

    assert!(
        h0 * h1 < 0.0,
        "branch vertices must be on opposite nappes (h0={h0}, h1={h1})"
    );
}

// ---------------------------------------------------------------------------
// Oblique non-axis cone — off-axis apex + tilted axis cut to a HYPERBOLA.
// Keeps coverage off the canonical origin/+z configuration; on-surface oracle
// + structural checks (I1, I2).
// ---------------------------------------------------------------------------

#[test]
fn oblique_non_axis_cone_hyperbola() {
    // Apex off origin; axis tilted in the y–z plane; plane parallel to the
    // axis (normal ⟂ axis) ⇒ |n̂·â| = 0 < sinα ⇒ hyperbola.
    let alpha = std::f64::consts::FRAC_PI_4;
    let apex = [2.0, -1.0, 0.5];
    let axis = unit([0.0, 1.0, 1.0]); // tilted axis in y–z.
                                      // A plane normal ⟂ the axis: pick n̂ = (0,1,−1)/√2 (⟂ axis), through a
                                      // point offset from the apex so the apex is off-plane.
    let nrm = unit([0.0, 1.0, -1.0]);
    assert!(dot(nrm, axis).abs() < TAU_MODEL, "test setup: n̂ ⟂ â");
    let ppt = add(apex, scale(nrm, 1.5)); // shift the plane off the apex.

    let plane = QuadricSurface::Plane {
        point: Point3::from(ppt),
        normal: Vector3::from(nrm),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(axis),
        half_angle: alpha,
    };
    let curves = intersect(&plane, &cone).expect("oblique cone: two Hyperbola");
    assert_eq!(curves.len(), 2, "oblique hyperbola has two branches");

    for c in &curves {
        // I1: on-surface oracle.
        assert_on_both_surfaces(c, &plane, &cone);
        // I2: structural — center in plane, major_axis unit + in-plane, a,b>0.
        let (center, normal, major, a, b) = expect_hyperbola(c);
        assert!(
            implicit_residual(&plane, center.as_array()) < TAU_MODEL,
            "hyperbola center {center:?} not in plane"
        );
        approx(norm(major.as_array()), 1.0);
        assert!(dot(normal.as_array(), major.as_array()).abs() < TAU_MODEL);
        assert!(a.is_finite() && a > 0.0 && b.is_finite() && b > 0.0);
        // Each branch vertex on both surfaces.
        let v = add(center.as_array(), scale(major.as_array(), a));
        assert!(implicit_residual(&cone, v) < TAU_MODEL);
        assert!(implicit_residual(&plane, v) < TAU_MODEL);
    }
}

// ---------------------------------------------------------------------------
// I4 — symmetry: intersect(plane, cone) == intersect(cone, plane), same curve
// set (tolerant to axis sign / branch order).
// ---------------------------------------------------------------------------

#[test]
fn symmetry_para_parabola() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 1.0),
        normal: Vector3::from(unit([1.0, 0.0, 1.0])),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let ab = intersect(&plane, &cone).unwrap();
    let ba = intersect(&cone, &plane).unwrap();
    let (v_ab, n_ab, x_ab, f_ab) = expect_single_parabola(&ab);
    let (v_ba, n_ba, x_ba, f_ba) = expect_single_parabola(&ba);

    approx_point(v_ab, v_ba.as_array());
    approx(f_ab, f_ba);
    parallel_up_to_sign(n_ab.as_array(), n_ba.as_array());
    parallel_up_to_sign(x_ab.as_array(), x_ba.as_array());
}

#[test]
fn symmetry_hype_hyperbola() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };
    let ab = intersect(&plane, &cone).unwrap();
    let ba = intersect(&cone, &plane).unwrap();
    assert_eq!(ab.len(), 2);
    assert_eq!(ba.len(), 2);

    // Compare as an UNORDERED pair of branches (matched by branch vertex).
    let branch_vertex = |c: &SsiCurve| -> [f64; 3] {
        let (center, _, major, a, _) = expect_hyperbola(c);
        add(center.as_array(), scale(major.as_array(), a))
    };
    let mut va: Vec<[f64; 3]> = ab.iter().map(branch_vertex).collect();
    let mut vb: Vec<[f64; 3]> = ba.iter().map(branch_vertex).collect();
    // Sort each pair by z so the comparison is order-independent.
    va.sort_by(|p, q| p[2].partial_cmp(&q[2]).unwrap());
    vb.sort_by(|p, q| p[2].partial_cmp(&q[2]).unwrap());
    for (p, q) in va.iter().zip(vb.iter()) {
        assert!(
            norm(sub(*p, *q)) < TAU_MODEL,
            "branch vertices differ: {p:?} vs {q:?}"
        );
    }

    // Shared center / a / b also agree.
    let (c_ab, _, _, a_ab, b_ab) = expect_hyperbola(&ab[0]);
    let (c_ba, _, _, a_ba, b_ba) = expect_hyperbola(&ba[0]);
    approx_point(c_ab, c_ba.as_array());
    approx(a_ab, a_ba);
    approx(b_ab, b_ba);
}

// ---------------------------------------------------------------------------
// I5 — determinism: identical inputs → byte-identical output, including the
// two-Hyperbola order (`+m̂` first) and a fixed-t eval point.
// ---------------------------------------------------------------------------

#[test]
fn determinism_para_parabola_identical() {
    let mk = || {
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 1.0),
            normal: Vector3::from(unit([1.0, 0.0, 1.0])),
        };
        let cone = QuadricSurface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: std::f64::consts::FRAC_PI_4,
        };
        intersect(&plane, &cone)
    };
    let first = mk();
    let second = mk();
    assert_eq!(first, second, "parabola output must be deterministic");

    let cf = first.unwrap();
    let cs = second.unwrap();
    let t = 0.7;
    assert_eq!(cf[0].eval(t).as_array(), cs[0].eval(t).as_array());
}

#[test]
fn determinism_hype_hyperbola_identical() {
    let mk = || {
        let plane = QuadricSurface::Plane {
            point: Point3::new(1.0, 0.0, 0.0),
            normal: Vector3::new(1.0, 0.0, 0.0),
        };
        let cone = QuadricSurface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: std::f64::consts::FRAC_PI_4,
        };
        intersect(&plane, &cone)
    };
    let first = mk();
    let second = mk();
    // Byte-identical, including the two-Hyperbola order (+m̂ first).
    assert_eq!(first, second, "hyperbola output must be deterministic");

    let cf = first.unwrap();
    let cs = second.unwrap();
    assert_eq!(cf.len(), 2);
    let t = 0.5;
    for i in 0..2 {
        assert_eq!(cf[i].eval(t).as_array(), cs[i].eval(t).as_array());
    }
}

// ---------------------------------------------------------------------------
// Regression spot-check (light): the PR-SSI3 bounded sections still return
// Circle / Ellipse — the new variants do not perturb the closed-conic
// branches. (The ssi3 suite is the authoritative guarantee; this re-asserts.)
// ---------------------------------------------------------------------------

#[test]
fn regression_circle_and_ellipse_unaffected() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = QuadricSurface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: alpha,
    };

    // C1 — perpendicular plane ⇒ Circle.
    let plane_perp = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 3.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
    };
    let c = intersect(&plane_perp, &cone).expect("circle");
    assert_eq!(c.len(), 1);
    assert!(
        matches!(c[0], SsiCurve::Circle { .. }),
        "expected Circle, got {:?}",
        c[0]
    );

    // C2 — oblique closed section ⇒ Ellipse (|n̂·â| > sinα).
    let theta = 20.0_f64.to_radians();
    let plane_obl = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 5.0),
        normal: Vector3::new(theta.sin(), 0.0, theta.cos()),
    };
    let e = intersect(&plane_obl, &cone).expect("ellipse");
    assert_eq!(e.len(), 1);
    assert!(
        matches!(e[0], SsiCurve::Ellipse { .. }),
        "expected Ellipse, got {:?}",
        e[0]
    );
}
