//! PR-SSI4 — Adversarial audit of the plane∩cone UNBOUNDED sections (parabola
//! + hyperbola), the two new `SsiCurve` variants closing pair #3.
//!
//! Attacks (all via the public `intersect` dispatcher; `plane_cone` is private):
//!   1. Parabola on-surface over a WIDE parameter sweep (find the coord
//!      magnitude where the absolute oracle breaks; characterize t²/(4f) growth).
//!   2. Hyperbola two-branch correctness + on-surface (vertices on opposite
//!      nappes; mirror symmetry eval(t)/eval(−t); branches distinct).
//!   3. ellipse↔parabola boundary (huge-finite ellipse just above, clean switch
//!      to one Parabola at the boundary; no NaN/blow-up).
//!   4. parabola↔hyperbola boundary (PARA vs HYPE selection when both gd_± small).
//!   5. Extreme half-angles + scale (narrow / flat cone; large apex offset;
//!      report the absolute-oracle breakpoint).
//!   6. Oblique non-axis cone cut to a parabola AND a hyperbola (structural).
//!   7. I4 symmetry + I5 determinism for parabola and hyperbola.
//!
//! Does NOT touch production code. Tolerances are TAU_MODEL-scale (TAU_MODEL =
//! 1e-7); the absolute on-surface oracle holds while sample coords stay below
//! ~1e8 (PR-SSI1 finding), so the unbounded curves are sampled over a BOUNDED t
//! range, and wide-sweep attacks switch to a RELATIVE check beyond the ceiling.

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

/// Every field of a returned curve must be finite (no NaN/Inf), lengths > 0.
fn assert_curve_finite(c: &SsiCurve) {
    let finite = |a: [f64; 3]| a.iter().all(|v| v.is_finite());
    match c {
        SsiCurve::Line { point, dir } => {
            assert!(
                finite(point.as_array()) && finite(dir.as_array()),
                "Line non-finite: {c:?}"
            );
        }
        SsiCurve::Circle {
            center,
            normal,
            radius,
        } => {
            assert!(
                finite(center.as_array()) && finite(normal.as_array()),
                "Circle non-finite: {c:?}"
            );
            assert!(
                radius.is_finite() && *radius > 0.0,
                "Circle radius bad: {c:?}"
            );
        }
        SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            assert!(
                finite(center.as_array())
                    && finite(normal.as_array())
                    && finite(major_axis.as_array()),
                "Ellipse non-finite: {c:?}"
            );
            assert!(
                major_radius.is_finite() && minor_radius.is_finite(),
                "Ellipse radii non-finite: {c:?}"
            );
            assert!(
                *major_radius > 0.0 && *minor_radius > 0.0,
                "Ellipse radii must be > 0: {c:?}"
            );
        }
        SsiCurve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } => {
            assert!(
                finite(vertex.as_array())
                    && finite(normal.as_array())
                    && finite(axis_dir.as_array()),
                "Parabola non-finite: {c:?}"
            );
            assert!(
                focal_length.is_finite() && *focal_length > 0.0,
                "Parabola focal bad: {c:?}"
            );
        }
        SsiCurve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => {
            assert!(
                finite(center.as_array())
                    && finite(normal.as_array())
                    && finite(major_axis.as_array()),
                "Hyperbola non-finite: {c:?}"
            );
            assert!(
                semi_transverse.is_finite() && semi_conjugate.is_finite(),
                "Hyperbola lengths non-finite: {c:?}"
            );
            assert!(
                *semi_transverse > 0.0 && *semi_conjugate > 0.0,
                "Hyperbola lengths must be > 0: {c:?}"
            );
        }
    }
}

fn parallel_up_to_sign(a: [f64; 3], b: [f64; 3]) {
    assert!(
        norm(cross(a, b)) < TAU_MODEL,
        "expected {a:?} parallel to {b:?} (|cross| = {})",
        norm(cross(a, b))
    );
}

// A plane normal tilted by angle theta from +z in the x–z plane:
// n̂ = (sinθ, 0, cosθ), so k = n̂·ẑ = cosθ.
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

/// Sample a single curve over `t ∈ [−tt, tt]` (N samples); assert on BOTH
/// surfaces using an absolute oracle while sample coords stay < `coord_ceiling`,
/// else a relative oracle. Also asserts every sample is finite. Returns
/// (max coord magnitude seen, max absolute residual seen) for characterization.
fn sweep_on_both(
    curve: &SsiCurve,
    a: &QuadricSurface,
    b: &QuadricSurface,
    tt: f64,
    n: usize,
    coord_ceiling: f64,
) -> (f64, f64) {
    let mut max_coord = 0.0_f64;
    let mut max_abs_res = 0.0_f64;
    for i in 0..n {
        let t = -tt + (i as f64) / ((n - 1) as f64) * 2.0 * tt;
        let p = curve.eval(t).as_array();
        for c in p {
            assert!(c.is_finite(), "non-finite coord at t={t}: {p:?}");
            max_coord = max_coord.max(c.abs());
        }
        let coord = p.iter().fold(0.0_f64, |m, c| m.max(c.abs())).max(1.0);
        let ra = implicit_residual(a, p);
        let rb = implicit_residual(b, p);
        max_abs_res = max_abs_res.max(ra).max(rb);
        if coord < coord_ceiling {
            assert!(
                ra < TAU_MODEL,
                "t={t} {p:?} off A (abs res {ra}, coord {coord:e})"
            );
            assert!(
                rb < TAU_MODEL,
                "t={t} {p:?} off B (abs res {rb}, coord {coord:e})"
            );
        } else {
            assert!(
                ra / coord < 1e-9,
                "t={t}: rel res A {} too large",
                ra / coord
            );
            assert!(
                rb / coord < 1e-9,
                "t={t}: rel res B {} too large",
                rb / coord
            );
        }
    }
    (max_coord, max_abs_res)
}

// ===========================================================================
// Attack 1: Parabola on-surface over a WIDE parameter sweep.
//
// Canonical parabola (α=π/4, n̂=(1,0,1)/√2, plane through (0,0,1)) and an
// oblique one. Sample eval(t) over t∈[−T,T] for growing T. Coords grow like
// t²/(4f), so the absolute oracle's 1e8 ceiling is reached fast. Assert
// absolute residual < TAU_MODEL while coords < 1e8, else relative < 1e-9.
// No NaN/Inf anywhere. Characterize the breakpoint.
// ===========================================================================

#[test]
fn attack1_parabola_wide_sweep_on_surface() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 1.0),
        normal: tilted_z_normal(std::f64::consts::FRAC_PI_4), // |n̂·â| = cos45 = sinα ⇒ parabola
    };
    let curves = intersect(&plane, &cone).expect("parabola");
    assert_eq!(curves.len(), 1);
    let SsiCurve::Parabola { focal_length, .. } = curves[0] else {
        panic!("expected Parabola, got {:?}", curves[0]);
    };
    assert_curve_finite(&curves[0]);

    // Growing T. coord ≈ T²/(4f); f ≈ 0.354 ⇒ coord ≈ 0.7·T². T=1e4 ⇒ coord ≈ 7e7
    // (still < ceiling); T=2e4 ⇒ coord ≈ 2.8e8 (> ceiling ⇒ relative branch).
    for &tt in &[1.0, 3.0, 10.0, 50.0, 1.0e4_f64, 2.0e4_f64] {
        let (mc, mr) = sweep_on_both(&curves[0], &plane, &cone, tt, 256, 1.0e8);
        // Coords really do grow ~T²/(4f).
        let expected_coord = tt * tt / (4.0 * focal_length);
        if tt >= 10.0 {
            assert!(
                mc > 0.3 * expected_coord && mc < 3.0 * expected_coord,
                "T={tt}: coord {mc:e} not ~ T²/(4f) = {expected_coord:e}"
            );
        }
        // Below the ceiling the absolute residual must hold; record it.
        if mc < 1.0e8 {
            assert!(
                mr < TAU_MODEL,
                "T={tt} (coord {mc:e}): abs residual {mr:e} ≥ TAU_MODEL"
            );
        }
    }
}

#[test]
fn attack1_oblique_parabola_wide_sweep() {
    // Oblique cone: off-origin apex, tilted axis. Build a plane normal whose
    // |n̂·â| = sinα exactly ⇒ parabola. Pick n̂ = cosα·û + sinα·â where û ⟂ â is
    // a unit in-plane direction; then n̂·â = sinα (and n̂ is unit).
    let alpha = 0.6_f64;
    let apex = [2.0, -1.0, 3.0];
    let ahat = unit([1.0, 2.0, 2.0]); // |·| = 3 originally
    let uhat = unit(cross(ahat, [0.0, 0.0, 1.0])); // ⟂ â, unit
    let nhat = add(scale(uhat, alpha.cos()), scale(ahat, alpha.sin()));
    assert!(
        (dot(nhat, ahat) - alpha.sin()).abs() < 1e-12,
        "setup: n̂·â = sinα"
    );
    assert!((norm(nhat) - 1.0).abs() < 1e-12, "setup: n̂ unit");

    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(ahat),
        half_angle: alpha,
    };
    let ppt = add(apex, scale(nhat, 2.0)); // plane offset from apex (not AP)
    let plane = QuadricSurface::Plane {
        point: Point3::from(ppt),
        normal: Vector3::from(nhat),
    };

    let curves = intersect(&plane, &cone).expect("oblique parabola");
    assert_eq!(
        curves.len(),
        1,
        "oblique parabola: one curve, got {curves:?}"
    );
    assert!(
        matches!(curves[0], SsiCurve::Parabola { .. }),
        "got {:?}",
        curves[0]
    );
    assert_curve_finite(&curves[0]);

    // On both surfaces over a bounded sweep (coords stay O(10²) here).
    let (mc, mr) = sweep_on_both(&curves[0], &plane, &cone, 8.0, 256, 1.0e8);
    assert!(
        mc < 1.0e8 && mr < TAU_MODEL,
        "oblique parabola off-surface: res {mr:e} coord {mc:e}"
    );
}

// ===========================================================================
// Attack 2: Hyperbola two-branch correctness + on-surface.
//
// Canonical hyperbola (α=π/4, plane x=1) and an oblique cone. Exactly two
// curves; each branch on both surfaces (cosh grows fast ⇒ keep T ≤ ~4);
// branch vertices on OPPOSITE nappes (axis-projection h opposite signs);
// branches distinct; eval(t)/eval(−t) on the same branch are mirror points
// (symmetric about the transverse axis). No NaN/Inf.
// ===========================================================================

#[test]
fn attack2_hyperbola_two_branches_mirror_and_nappes() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let curves = intersect(&plane, &cone).expect("hyperbola");
    assert_eq!(curves.len(), 2, "two branches, got {curves:?}");
    curves.iter().for_each(assert_curve_finite);

    let ahat = [0.0, 0.0, 1.0];
    let mut nappe_signs = Vec::new();
    for c in &curves {
        // On both surfaces over a bounded per-branch range.
        let (mc, mr) = sweep_on_both(c, &plane, &cone, 4.0, 128, 1.0e8);
        assert!(
            mc < 1.0e8 && mr < TAU_MODEL,
            "branch off-surface: res {mr:e} coord {mc:e}"
        );

        // eval(t) and eval(−t) are mirror points about the transverse axis:
        // they share the major-axis component and have opposite conjugate
        // component (sinh is odd). Concretely eval(t)+eval(−t) = 2·apex-axis
        // reflection ⇒ the conjugate (y) coords cancel; here that means
        // eval(t).y = −eval(−t).y and eval(t).{x,z} = eval(−t).{x,z}.
        for &t in &[0.3_f64, 1.0, 2.5] {
            let p = c.eval(t).as_array();
            let q = c.eval(-t).as_array();
            assert!((p[0] - q[0]).abs() < TAU_MODEL, "x not mirror-symmetric");
            assert!((p[2] - q[2]).abs() < TAU_MODEL, "z not mirror-symmetric");
            assert!(
                (p[1] + q[1]).abs() < TAU_MODEL,
                "y not anti-symmetric (mirror)"
            );
            // And not the same point (t≠0 ⇒ distinct).
            assert!(norm(sub(p, q)) > 1e-3, "mirror points collapsed");
        }

        // Branch vertex on the axis projection.
        let SsiCurve::Hyperbola {
            center,
            major_axis,
            semi_transverse,
            ..
        } = *c
        else {
            panic!("expected Hyperbola");
        };
        let v = add(
            center.as_array(),
            scale(major_axis.as_array(), semi_transverse),
        );
        nappe_signs.push(dot(sub(v, [0.0, 0.0, 0.0]), ahat).signum());
    }
    // Vertices on OPPOSITE nappes.
    assert!(
        nappe_signs[0] * nappe_signs[1] < 0.0,
        "branch vertices not on opposite nappes: {nappe_signs:?}"
    );

    // Branches distinct: their vertices differ.
    let vtx = |c: &SsiCurve| {
        let SsiCurve::Hyperbola {
            center,
            major_axis,
            semi_transverse,
            ..
        } = *c
        else {
            unreachable!()
        };
        add(
            center.as_array(),
            scale(major_axis.as_array(), semi_transverse),
        )
    };
    assert!(
        norm(sub(vtx(&curves[0]), vtx(&curves[1]))) > 1e-3,
        "branches not distinct"
    );
}

#[test]
fn attack2_oblique_hyperbola_two_branches() {
    // Off-axis apex + tilted axis, plane ⟂ axis-tilt-removed ⇒ k=0 ⇒ hyperbola.
    let alpha = 0.5_f64;
    let apex = [2.0, -1.0, 3.0];
    let ahat = unit([1.0, 2.0, 2.0]);
    // n̂ ⟂ â ⇒ k = 0 < sinα ⇒ hyperbola.
    let nhat = unit(cross(ahat, [1.0, 0.0, 0.0]));
    assert!(dot(nhat, ahat).abs() < 1e-12, "setup n̂ ⟂ â");
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(ahat),
        half_angle: alpha,
    };
    let ppt = add(apex, scale(nhat, 1.7));
    let plane = QuadricSurface::Plane {
        point: Point3::from(ppt),
        normal: Vector3::from(nhat),
    };
    let curves = intersect(&plane, &cone).expect("oblique hyperbola");
    assert_eq!(
        curves.len(),
        2,
        "oblique hyperbola two branches, got {curves:?}"
    );
    curves.iter().for_each(assert_curve_finite);
    for c in &curves {
        let (mc, mr) = sweep_on_both(c, &plane, &cone, 3.0, 128, 1.0e8);
        assert!(
            mc < 1.0e8 && mr < TAU_MODEL,
            "oblique branch off-surface: res {mr:e}"
        );
        let SsiCurve::Hyperbola {
            center,
            normal,
            major_axis,
            ..
        } = *c
        else {
            panic!()
        };
        // center in plane; major_axis unit + in-plane.
        assert!(
            implicit_residual(&plane, center.as_array()) < TAU_MODEL,
            "center not in plane"
        );
        assert!(
            (norm(major_axis.as_array()) - 1.0).abs() < TAU_MODEL,
            "major not unit"
        );
        assert!(
            dot(normal.as_array(), major_axis.as_array()).abs() < TAU_MODEL,
            "major not in-plane"
        );
    }
}

// ===========================================================================
// Attack 3: ellipse↔parabola boundary (the dangerous transition).
//
// Sweep k (plane tilt) from the ellipse side DOWN through k=sinα. Just above ⇒
// finite Ellipse (huge a near the boundary but finite + on-surface); AT the
// boundary ⇒ one Parabola (finite vertex + focal_length, on-surface). The switch
// is clean (no NaN, no blown-up bounded curve). Characterize continuity.
// ===========================================================================

#[test]
fn attack3_ellipse_parabola_boundary_clean_switch() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let theta_par = std::f64::consts::FRAC_PI_2 - alpha; // k = cosθ_par = sinα ⇒ parabola

    // Approach the parabola from the ellipse side (frac < 1) then hit it (=1).
    let fracs = [0.5, 0.9, 0.99, 0.999, 0.9999, 0.99999, 0.999999, 1.0];
    let mut saw_ellipse = false;
    let mut saw_parabola = false;
    let mut max_ellipse_a = 0.0_f64;

    for frac in fracs {
        let theta = theta_par * frac;
        let plane = QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 5.0),
            normal: tilted_z_normal(theta),
        };
        let curves = intersect(&plane, &cone)
            .unwrap_or_else(|e| panic!("frac={frac}: unexpected error {e:?}"));
        curves.iter().for_each(assert_curve_finite); // never NaN/Inf
        match curves[0] {
            SsiCurve::Ellipse {
                major_radius,
                minor_radius,
                ..
            } => {
                assert_eq!(curves.len(), 1, "frac={frac}: one ellipse");
                saw_ellipse = true;
                max_ellipse_a = max_ellipse_a.max(major_radius);
                assert!(major_radius.is_finite() && minor_radius > 0.0);
                assert!(major_radius >= minor_radius - TAU_MODEL, "a < b");
                // Huge but finite near the boundary, and still on-surface while
                // coords stay below the ceiling.
                if major_radius < 1.0e7 {
                    let (_, mr) = {
                        // ellipse sampled over full period
                        let mut mm = 0.0_f64;
                        for i in 0..256 {
                            let t = (i as f64) / 256.0 * std::f64::consts::TAU;
                            let p = curves[0].eval(t).as_array();
                            mm = mm
                                .max(implicit_residual(&plane, p))
                                .max(implicit_residual(&cone, p));
                        }
                        (0.0_f64, mm)
                    };
                    assert!(
                        mr < TAU_MODEL,
                        "frac={frac}: ellipse (a={major_radius:e}) off-surface {mr:e}"
                    );
                }
            }
            SsiCurve::Parabola {
                vertex,
                focal_length,
                ..
            } => {
                assert_eq!(curves.len(), 1, "frac={frac}: one parabola");
                saw_parabola = true;
                // Finite vertex + focal_length as the ellipse a→∞ just above.
                assert!(
                    vertex.as_array().iter().all(|c| c.is_finite()),
                    "parabola vertex non-finite"
                );
                assert!(
                    focal_length.is_finite() && focal_length > 0.0,
                    "parabola focal bad"
                );
                // On-surface over a bounded sweep.
                let (mc, mr) = sweep_on_both(&curves[0], &plane, &cone, 5.0, 128, 1.0e8);
                assert!(
                    mc < 1.0e8 && mr < TAU_MODEL,
                    "boundary parabola off-surface {mr:e}"
                );
            }
            ref other => panic!("frac={frac}: unexpected curve {other:?}"),
        }
    }
    assert!(
        saw_ellipse && saw_parabola,
        "boundary sweep covered both branches: e={saw_ellipse} p={saw_parabola}"
    );
    // The ellipse really does blow up toward the boundary (a→∞), confirming the
    // dangerous limit is exercised — yet stays finite.
    assert!(
        max_ellipse_a > 1.0e4 && max_ellipse_a.is_finite(),
        "near-boundary ellipse a={max_ellipse_a:e} did not grow"
    );
}

// ===========================================================================
// Attack 4: parabola↔hyperbola boundary.
//
// Sweep k from just above sinα (ellipse) to just below (hyperbola). Near k=sinα
// BOTH gd_± get small — verify PARABOLA (one |gd|<TAU) is selected vs HYPERBOLA
// (opposite signs, both ≥TAU). The solver never produces a malformed curve or
// wrong count. k<sinα ⇒ two Hyperbola; k=sinα ⇒ one Parabola; no NaN.
// ===========================================================================

#[test]
fn attack4_parabola_hyperbola_boundary_selection() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let cone = z_cone(alpha);
    let sina = alpha.sin();

    // Fine sweep of k around sinα. Plane point chosen off-apex for all tilts.
    let steps = 2000;
    let lo = sina - 0.02;
    let hi = sina + 0.02;
    let mut classes = (0usize, 0usize, 0usize); // (hyperbola, parabola, ellipse)
    for i in 0..=steps {
        let k = lo + (hi - lo) * (i as f64) / (steps as f64);
        if !(0.0..1.0).contains(&k) {
            continue;
        }
        let theta = k.acos();
        let plane = QuadricSurface::Plane {
            point: Point3::new(2.0, 0.0, 5.0),
            normal: tilted_z_normal(theta),
        };
        let curves =
            intersect(&plane, &cone).unwrap_or_else(|e| panic!("k={k}: unexpected error {e:?}"));
        curves.iter().for_each(assert_curve_finite);
        match curves[0] {
            SsiCurve::Hyperbola { .. } => {
                assert_eq!(curves.len(), 2, "k={k}: hyperbola must be two branches");
                assert!(
                    curves
                        .iter()
                        .all(|c| matches!(c, SsiCurve::Hyperbola { .. })),
                    "k={k}: mixed curve set {curves:?}"
                );
                // Hyperbola only legitimate at/below the parabola boundary.
                assert!(
                    k < sina + 1e-3,
                    "k={k} ≫ sinα: hyperbola for a genuine ellipse"
                );
                classes.0 += 1;
            }
            SsiCurve::Parabola { .. } => {
                assert_eq!(curves.len(), 1, "k={k}: parabola must be one curve");
                // Parabola only at the boundary (within the TAU gate band).
                assert!(
                    (k - sina).abs() < 1e-2,
                    "k={k}: parabola far from sinα={sina}"
                );
                classes.1 += 1;
            }
            SsiCurve::Ellipse { .. } => {
                assert_eq!(curves.len(), 1, "k={k}: ellipse must be one curve");
                assert!(
                    k > sina - 1e-3,
                    "k={k} < sinα: ellipse for an unbounded section"
                );
                classes.2 += 1;
            }
            ref other => panic!("k={k}: unexpected curve {other:?}"),
        }
    }
    // All three regions were exercised across the sweep.
    assert!(classes.0 > 0, "no hyperbola seen below sinα");
    assert!(classes.2 > 0, "no ellipse seen above sinα");
    // The parabola band is at least exercised once OR the switch is sharp
    // (ellipse directly to hyperbola within one grid step). Either is a clean
    // boundary; assert the unbounded side never produced a bounded count.
    // (classes.1 may be 0 if no grid point lands inside the TAU gate — that is
    // not a defect; the headline is no misclassification, asserted above.)
}

// ===========================================================================
// Attack 5: extreme half-angles + scale.
//
// (a) Narrow cone (α near TAU_MODEL above the E1 gate) cut to a hyperbola —
//     relative correctness; E1 fires outside the valid α range.
// (b) Flat cone (α near π/2 − TAU_MODEL) cut to a hyperbola — relative check.
// (c) Large cone (apex offset ~1e6) cut to a parabola — relative correctness;
//     report the absolute-oracle breakpoint.
// ===========================================================================

#[test]
fn attack5a_narrow_cone_hyperbola() {
    // Very narrow cone, just inside the E1 lower gate.
    let alpha = TAU_MODEL * 1000.0; // ≫ TAU_MODEL but a sliver cone (≈1e-4 rad)
    let cone = z_cone(alpha);
    // Plane ⟂ axis would be a circle; tilt so k = 0 (plane ∥ axis) ⇒ hyperbola.
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let curves = intersect(&plane, &cone).expect("narrow-cone hyperbola");
    assert_eq!(
        curves.len(),
        2,
        "narrow cone hyperbola two branches, got {curves:?}"
    );
    curves.iter().for_each(assert_curve_finite);
    // Coords are O(1/tanα) ≈ 1e4 (huge transverse), so use relative oracle.
    for c in &curves {
        let scale_ref = match c {
            SsiCurve::Hyperbola {
                semi_transverse, ..
            } => semi_transverse.max(1.0),
            _ => 1.0,
        };
        let mut mr = 0.0_f64;
        for i in 0..128 {
            let t = -3.0 + (i as f64) / 127.0 * 6.0;
            let p = c.eval(t).as_array();
            assert!(
                p.iter().all(|v| v.is_finite()),
                "narrow-cone branch non-finite"
            );
            mr = mr
                .max(implicit_residual(&plane, p))
                .max(implicit_residual(&cone, p));
        }
        assert!(
            mr / scale_ref < 1e-9,
            "narrow-cone relative residual {} too big",
            mr / scale_ref
        );
    }

    // E1: α below the gate ⇒ DegenerateInput.
    let cone_bad = z_cone(TAU_MODEL * 0.5);
    assert_eq!(
        intersect(&plane, &cone_bad),
        Err(SsiError::DegenerateInput),
        "α ≤ TAU_MODEL must be Err"
    );
}

#[test]
fn attack5b_flat_cone_hyperbola() {
    // Flat cone near the upper E1 gate.
    let alpha = std::f64::consts::FRAC_PI_2 - 1e-3;
    let cone = z_cone(alpha);
    // Plane ∥ axis ⇒ k = 0 < sinα ⇒ hyperbola.
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let curves = intersect(&plane, &cone).expect("flat-cone hyperbola");
    assert_eq!(
        curves.len(),
        2,
        "flat cone hyperbola two branches, got {curves:?}"
    );
    curves.iter().for_each(assert_curve_finite);
    for c in &curves {
        // Coords stay modest near t=0 (transverse a small); relative is safe.
        let scale_ref = 1.0e3_f64;
        let mut mr = 0.0_f64;
        for i in 0..128 {
            let t = -2.0 + (i as f64) / 127.0 * 4.0;
            let p = c.eval(t).as_array();
            assert!(
                p.iter().all(|v| v.is_finite()),
                "flat-cone branch non-finite"
            );
            mr = mr
                .max(implicit_residual(&plane, p))
                .max(implicit_residual(&cone, p));
        }
        assert!(
            mr / scale_ref < 1e-9,
            "flat-cone relative residual {} too big",
            mr / scale_ref
        );
    }

    // E1: α above the gate ⇒ DegenerateInput.
    let cone_bad = z_cone(std::f64::consts::FRAC_PI_2 - TAU_MODEL * 0.5);
    assert_eq!(
        intersect(&plane, &cone_bad),
        Err(SsiError::DegenerateInput),
        "α ≥ π/2−TAU must be Err"
    );
}

#[test]
fn attack5c_large_scale_parabola_relative() {
    // Apex offset ~1e6 ⇒ parabola coords are large from t=0. Relative
    // correctness holds at every scale; the absolute oracle breaks well below
    // here, so this is a RELATIVE check (and reports the breakpoint).
    let alpha = std::f64::consts::FRAC_PI_4;
    let apex = [1.0e6, -2.0e6, 3.0e6];
    let ahat = [0.0, 0.0, 1.0];
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(ahat),
        half_angle: alpha,
    };
    // n̂·â = sinα ⇒ parabola; tilt n̂ 45° from +z in x–z.
    let plane = QuadricSurface::Plane {
        point: Point3::from(add(apex, [0.0, 0.0, 1.0e5])), // off-apex
        normal: tilted_z_normal(std::f64::consts::FRAC_PI_4),
    };
    let curves = intersect(&plane, &cone).expect("large-scale parabola");
    assert_eq!(curves.len(), 1, "got {curves:?}");
    assert!(
        matches!(curves[0], SsiCurve::Parabola { .. }),
        "got {:?}",
        curves[0]
    );
    assert_curve_finite(&curves[0]);

    let mut max_coord = 0.0_f64;
    let mut max_abs = 0.0_f64;
    let mut max_rel = 0.0_f64;
    for i in 0..256 {
        let t = -100.0 + (i as f64) / 255.0 * 200.0;
        let p = curves[0].eval(t).as_array();
        assert!(
            p.iter().all(|v| v.is_finite()),
            "large-scale parabola non-finite"
        );
        let coord = p.iter().fold(0.0_f64, |m, c| m.max(c.abs()));
        max_coord = max_coord.max(coord);
        let r = implicit_residual(&plane, p).max(implicit_residual(&cone, p));
        max_abs = max_abs.max(r);
        max_rel = max_rel.max(r / coord.max(1.0));
    }
    // Relative correctness at scale ~1e6.
    assert!(
        max_rel < 1e-9,
        "large-scale parabola relative residual {max_rel} too big"
    );
    // Document the breakpoint: at coords ~1e6+ the absolute oracle generally
    // exceeds TAU_MODEL (this is expected per PR-SSI1); just require finiteness.
    assert!(
        max_coord > 1.0e6 && max_abs.is_finite(),
        "scale not exercised"
    );
}

// ===========================================================================
// Attack 6: oblique non-axis cone — parabola AND hyperbola, structural checks.
//
// Apex (2,−1,3), axis (1,2,2)/3 cut to a parabola and to a hyperbola; on-surface
// for all branches; parabola axis in-plane; hyperbola vertices on cone & in
// plane; center in plane.
// ===========================================================================

#[test]
fn attack6_oblique_parabola_structural() {
    let alpha = 0.5_f64;
    let apex = [2.0, -1.0, 3.0];
    let ahat = unit([1.0, 2.0, 2.0]);
    let uhat = unit(cross(ahat, [1.0, 0.0, 0.0])); // ⟂ â
    let nhat = add(scale(uhat, alpha.cos()), scale(ahat, alpha.sin())); // n̂·â = sinα ⇒ parabola
    assert!((dot(nhat, ahat) - alpha.sin()).abs() < 1e-12);
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(ahat),
        half_angle: alpha,
    };
    let ppt = add(apex, scale(nhat, 2.5));
    let plane = QuadricSurface::Plane {
        point: Point3::from(ppt),
        normal: Vector3::from(nhat),
    };
    let curves = intersect(&plane, &cone).expect("oblique parabola");
    assert_eq!(curves.len(), 1, "got {curves:?}");
    let SsiCurve::Parabola {
        vertex,
        normal,
        axis_dir,
        focal_length,
    } = curves[0]
    else {
        panic!("expected Parabola, got {:?}", curves[0]);
    };
    assert_curve_finite(&curves[0]);
    // vertex on cone & in plane.
    assert!(
        implicit_residual(&cone, vertex.as_array()) < TAU_MODEL,
        "vertex not on cone"
    );
    assert!(
        implicit_residual(&plane, vertex.as_array()) < TAU_MODEL,
        "vertex not in plane"
    );
    // axis_dir unit + in-plane (⟂ normal); focal > 0.
    assert!(
        (norm(axis_dir.as_array()) - 1.0).abs() < TAU_MODEL,
        "axis not unit"
    );
    assert!(
        dot(normal.as_array(), axis_dir.as_array()).abs() < TAU_MODEL,
        "axis not in-plane"
    );
    assert!(focal_length > 0.0 && focal_length.is_finite());
    parallel_up_to_sign(normal.as_array(), nhat);
    // On-surface over a bounded sweep.
    let (mc, mr) = sweep_on_both(&curves[0], &plane, &cone, 6.0, 128, 1.0e8);
    assert!(
        mc < 1.0e8 && mr < TAU_MODEL,
        "oblique parabola off-surface {mr:e}"
    );
}

#[test]
fn attack6_oblique_hyperbola_structural() {
    let alpha = 0.5_f64;
    let apex = [2.0, -1.0, 3.0];
    let ahat = unit([1.0, 2.0, 2.0]);
    let nhat = unit(cross(ahat, [0.0, 1.0, 0.0])); // ⟂ â ⇒ k = 0 ⇒ hyperbola
    assert!(dot(nhat, ahat).abs() < 1e-12);
    let cone = QuadricSurface::Cone {
        apex: Point3::from(apex),
        axis_dir: Vector3::from(ahat),
        half_angle: alpha,
    };
    let ppt = add(apex, scale(nhat, 2.0));
    let plane = QuadricSurface::Plane {
        point: Point3::from(ppt),
        normal: Vector3::from(nhat),
    };
    let curves = intersect(&plane, &cone).expect("oblique hyperbola");
    assert_eq!(curves.len(), 2, "got {curves:?}");
    curves.iter().for_each(assert_curve_finite);
    for c in &curves {
        let SsiCurve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            ..
        } = *c
        else {
            panic!("expected Hyperbola, got {c:?}");
        };
        // center in plane; major_axis unit + in-plane; vertices on cone & in plane.
        assert!(
            implicit_residual(&plane, center.as_array()) < TAU_MODEL,
            "center not in plane"
        );
        assert!(
            (norm(major_axis.as_array()) - 1.0).abs() < TAU_MODEL,
            "major not unit"
        );
        assert!(
            dot(normal.as_array(), major_axis.as_array()).abs() < TAU_MODEL,
            "major not in-plane"
        );
        let v = add(
            center.as_array(),
            scale(major_axis.as_array(), semi_transverse),
        );
        assert!(
            implicit_residual(&cone, v) < TAU_MODEL,
            "vertex {v:?} not on cone"
        );
        assert!(
            implicit_residual(&plane, v) < TAU_MODEL,
            "vertex {v:?} not in plane"
        );
        let (mc, mr) = sweep_on_both(c, &plane, &cone, 3.0, 128, 1.0e8);
        assert!(
            mc < 1.0e8 && mr < TAU_MODEL,
            "oblique hyperbola branch off-surface {mr:e}"
        );
    }
}

// ===========================================================================
// Attack 7: I4 symmetry + I5 determinism for parabola and hyperbola.
// ===========================================================================

#[test]
fn attack7_parabola_symmetry_and_determinism() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let plane = QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 1.0),
        normal: tilted_z_normal(std::f64::consts::FRAC_PI_4),
    };
    let cone = z_cone(alpha);

    // I4: both argument orders byte-identical (Cone,Plane swaps to plane_cone).
    let ab = intersect(&plane, &cone).expect("parabola ab");
    let ba = intersect(&cone, &plane).expect("parabola ba");
    assert_eq!(
        ab, ba,
        "intersect(plane,cone) != intersect(cone,plane) for parabola"
    );

    // I5: determinism across repeated calls (byte-identical, incl. eval points).
    let first = intersect(&plane, &cone);
    for _ in 0..5 {
        assert_eq!(
            intersect(&plane, &cone),
            first,
            "parabola not deterministic"
        );
    }
    let c = first.unwrap();
    for &t in &[-2.0_f64, 0.0, 0.7, 3.0] {
        assert_eq!(c[0].eval(t).as_array(), c[0].eval(t).as_array());
    }
}

#[test]
fn attack7_hyperbola_symmetry_and_determinism() {
    let alpha = std::f64::consts::FRAC_PI_4;
    let plane = QuadricSurface::Plane {
        point: Point3::new(1.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
    };
    let cone = z_cone(alpha);

    // I4: both orders byte-identical, including the two-branch order (+m̂ first).
    let ab = intersect(&plane, &cone).expect("hyperbola ab");
    let ba = intersect(&cone, &plane).expect("hyperbola ba");
    assert_eq!(
        ab, ba,
        "intersect(plane,cone) != intersect(cone,plane) for hyperbola"
    );
    assert_eq!(ab.len(), 2);

    // I5: repeated calls byte-identical, including the +m̂-first branch order.
    let first = intersect(&plane, &cone);
    for _ in 0..5 {
        assert_eq!(
            intersect(&plane, &cone),
            first,
            "hyperbola not deterministic (incl. branch order)"
        );
    }
    // The +m̂-first ordering is stable: branch 0's major_axis is byte-identical
    // across calls.
    let a0 = match ab[0] {
        SsiCurve::Hyperbola { major_axis, .. } => major_axis.as_array(),
        _ => panic!(),
    };
    let b0 = match intersect(&plane, &cone).unwrap()[0] {
        SsiCurve::Hyperbola { major_axis, .. } => major_axis.as_array(),
        _ => panic!(),
    };
    assert_eq!(a0, b0, "two-Hyperbola order not stable");
}
