//! PR-SSI12 — Adversarial audit of the procedural surface-pair arm of
//! `cylinder_cylinder` (M5, `specs/m5_surface_pair_curve.md`).
//!
//! Attacks the boundaries where the new arm must NOT fire, and the one
//! degenerate contract it owns:
//!   - degenerate radius / axis in a would-be surface-pair config must STILL
//!     be `DegenerateInput` (E1 is checked before the non-parallel branch);
//!   - the parallelism band must not leak: just-inside the band stays the
//!     parallel line/empty path, never a spurious surface-pair;
//!   - `eval` on a SurfacePair has NO closed form ⇒ a LOUD NaN, never a
//!     plausible-but-wrong point (P9).

use cad_primitives::{Point3, Vector3, TAU_MODEL};
use ssi_rs::{intersect, QuadricSurface, SsiCurve, SsiError};

fn cyl(ax: f64, ay: f64, az: f64, dx: f64, dy: f64, dz: f64, r: f64) -> QuadricSurface {
    QuadricSurface::Cylinder {
        axis_point: Point3::new(ax, ay, az),
        axis_dir: Vector3::new(dx, dy, dz),
        radius: r,
    }
}

// ---------------------------------------------------------------------------
// E1 precedence: a degenerate cylinder in an otherwise-S2 config (perp,
// unequal) is DegenerateInput, NOT a SurfacePair over a bad surface.
// ---------------------------------------------------------------------------

#[test]
fn zero_radius_in_surface_pair_config_is_degenerate() {
    let a = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
    let bad = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0); // r = 0
    assert_eq!(intersect(&a, &bad), Err(SsiError::DegenerateInput));
    assert_eq!(intersect(&bad, &a), Err(SsiError::DegenerateInput));
}

#[test]
fn negative_radius_in_surface_pair_config_is_degenerate() {
    let a = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
    let bad = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -2.0);
    assert_eq!(intersect(&a, &bad), Err(SsiError::DegenerateInput));
}

#[test]
fn nan_radius_in_surface_pair_config_is_degenerate() {
    let a = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
    let bad = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, f64::NAN);
    assert_eq!(intersect(&a, &bad), Err(SsiError::DegenerateInput));
}

#[test]
fn nan_axis_point_in_surface_pair_config_is_degenerate() {
    // A non-finite axis point would poison the parallelism/branch logic; the
    // E1 guard must catch it before the non-parallel arm returns a descriptor
    // built over a NaN surface.
    let a = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
    let bad = cyl(f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5);
    assert_eq!(intersect(&a, &bad), Err(SsiError::DegenerateInput));
    assert_eq!(intersect(&bad, &a), Err(SsiError::DegenerateInput));
}

#[test]
fn zero_axis_dir_in_surface_pair_config_is_degenerate() {
    let a = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
    let bad = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5); // zero axis
    assert_eq!(intersect(&a, &bad), Err(SsiError::DegenerateInput));
}

// ---------------------------------------------------------------------------
// Band integrity: JUST INSIDE the parallelism band (|û₁×û₂| < TAU) stays the
// PARALLEL path (here: concentric-unequal ⇒ empty), NOT a surface-pair. A
// too-eager surface-pair arm would steal this.
// ---------------------------------------------------------------------------

#[test]
fn just_inside_parallel_band_is_not_surface_pair() {
    // Second axis tilted by sin θ = 0.5·TAU < TAU off +z ⇒ still "parallel".
    let theta = (0.5 * TAU_MODEL).asin();
    let a = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
    // Coincident axis line, unequal radius ⇒ concentric ⇒ Ok([]) (empty),
    // definitively NOT a SurfacePair.
    let b = cyl(0.0, 0.0, 0.0, theta.sin(), 0.0, theta.cos(), 2.0);
    let curves = intersect(&a, &b).expect("within-band concentric-unequal ⇒ empty");
    assert!(
        !curves
            .iter()
            .any(|c| matches!(c, SsiCurve::SurfacePair { .. })),
        "within-band pair must not be a surface-pair: {curves:?}"
    );
}

// ---------------------------------------------------------------------------
// `eval` on a SurfacePair has no closed-form parameterization ⇒ NaN (loud).
// ---------------------------------------------------------------------------

#[test]
fn eval_on_surface_pair_is_nan_not_plausible() {
    let a = cyl(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
    let b = cyl(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5);
    let sp = SsiCurve::SurfacePair { a, b };
    for &t in &[-3.0_f64, -0.5, 0.0, 0.5, 3.0] {
        let p = sp.eval(t);
        assert!(
            p.x().is_nan() && p.y().is_nan() && p.z().is_nan(),
            "eval on a procedural surface-pair must be a LOUD NaN at t={t}, got {p:?}"
        );
    }
}
