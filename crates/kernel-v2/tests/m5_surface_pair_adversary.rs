//! M5 increment 1 — ADVERSARY suite for the procedural surface-pair curve
//! (`specs/m5_surface_pair_curve.md`, K5–K11; FIP §6 validation phase).
//!
//! These are pathological inputs the happy-path cycle tests do not cover:
//! malformed `PairSurface` descriptors (NaN / infinite / zero / negative /
//! non-unit fields), operand-order twin violations, and degenerate /
//! ill-conditioned sampler calls. Every assertion is a TYPED error match or
//! a numeric/structural property — never a bare `is_err()`.
//!
//! Fixtures mirror the cycle's honest retag trick: extrude the kv12 vesica
//! lens (two supporting cylinders, centers (0,±1), r=√2, axes +ẑ) and retag
//! its vertical tip rulings — which genuinely lie on BOTH cylinders — with a
//! surface-pair descriptor. The tips are exactly on-curve, so any rejection
//! is attributable to the descriptor field under test, not the geometry.

use std::f64::consts::PI;

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{
    extrude, surface_pair_interior_samples, validate_solid, BrepArena, Curve, KernelV2Error,
    PairSurface, Profile, ProfileEdge, SolidId, UnitVector3,
};

const H: f64 = 3.0;

fn up() -> UnitVector3 {
    UnitVector3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    }
}

/// A vesica-tip supporting cylinder (radius √2, axis +ẑ) at the given center.
fn vesica_cyl(cy: f64) -> PairSurface {
    PairSurface::Cylinder {
        axis_point: Point3::new(0.0, cy, 0.0),
        axis_dir: up(),
        radius: 2.0_f64.sqrt(),
    }
}

/// Build the vesica prism and retag every vertical tip ruling (all four
/// half-edges) with `Curve::SurfacePair { a, b }`. Returns the arena, the
/// solid, and the slots of the retagged half-edges (walk order).
fn vesica_prism_tagged(a: PairSurface, b: PairSurface) -> (BrepArena, SolidId, Vec<usize>) {
    let r2 = 2.0_f64.sqrt();
    let pa = Point2::new(-1.0, 0.0);
    let pb = Point2::new(1.0, 0.0);
    let profile = Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            ProfileEdge::Arc {
                a: pa,
                b: pb,
                center: Point2::new(0.0, 1.0),
                radius: r2,
                ccw: true,
            },
            ProfileEdge::Arc {
                a: pb,
                b: pa,
                center: Point2::new(0.0, -1.0),
                radius: r2,
                ccw: true,
            },
        ],
        vec![],
    )
    .expect("valid vesica lens profile");
    let mut arena = BrepArena::new();
    let res = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), H)
        .expect("vesica extrude succeeds");
    validate_solid(&arena, res.solid).expect("vesica prism valid before retag");

    let mut slots = Vec::new();
    for slot in 0..arena.half_edges.len() {
        let Some(he) = arena.half_edges[slot] else {
            continue;
        };
        if !matches!(he.curve, Curve::LineSegment) {
            continue;
        }
        let p0 = arena.vertices[he.origin.0 as usize].unwrap().point;
        let next = arena.half_edges[he.next.0 as usize].unwrap();
        let p1 = arena.vertices[next.origin.0 as usize].unwrap().point;
        assert!(
            (p0.x() - p1.x()).abs() < 1e-15 && (p0.y() - p1.y()).abs() < 1e-15,
            "vesica prism LineSegments are vertical rulings"
        );
        arena.half_edges[slot].as_mut().unwrap().curve = Curve::SurfacePair { a, b };
        slots.push(slot);
    }
    assert_eq!(
        slots.len(),
        4,
        "two vertical tip edges (4 half-edges) retagged"
    );
    (arena, res.solid, slots)
}

/// Geometric cylinder residual (bypasses the descriptor math entirely) — the
/// ground truth a sample must satisfy to be "on" a unit-axis cylinder.
fn cyl_residual(p: &Point3, axis_point: Point3, axis_dir: [f64; 3], radius: f64) -> f64 {
    let d = [
        p.x() - axis_point.x(),
        p.y() - axis_point.y(),
        p.z() - axis_point.z(),
    ];
    let t = d[0] * axis_dir[0] + d[1] * axis_dir[1] + d[2] * axis_dir[2];
    let r = [
        d[0] - t * axis_dir[0],
        d[1] - t * axis_dir[1],
        d[2] - t * axis_dir[2],
    ];
    (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt() - radius
}

// ===========================================================================
// Group A — malformed descriptor fields on a geometrically-valid edge
// ===========================================================================

/// A `radius = NaN` pair descriptor must NOT slip through `validate_solid`
/// silently. It doesn't: the twin `PartialEq` (NaN ≠ NaN) rejects it —
/// though NOT the K7 residual check (see the `finding_*` note below), which
/// would let it pass because `NaN.abs() > band` is `false`.
#[test]
fn nan_radius_pair_not_silently_accepted() {
    let a = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, 1.0, 0.0),
        axis_dir: up(),
        radius: f64::NAN,
    };
    let b = vesica_cyl(-1.0);
    let (arena, solid, _) = vesica_prism_tagged(a, b);
    let err = validate_solid(&arena, solid).expect_err("NaN-radius pair must be rejected");
    assert!(
        matches!(
            err,
            KernelV2Error::CurveTwinMismatch { .. } | KernelV2Error::VertexOffSurface { .. }
        ),
        "NaN radius must fail typed, got {err:?}"
    );
}

/// `radius = 0` is not a well-formed cylinder. The tips sit at distance √2
/// from the axis, so the K7 residual (√2) is far outside the band → typed
/// `VertexOffSurface`, never a panic or silent pass. (Twin check passes:
/// 0.0 == 0.0.)
#[test]
fn zero_radius_pair_rejected_typed() {
    let a = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, 1.0, 0.0),
        axis_dir: up(),
        radius: 0.0,
    };
    let b = vesica_cyl(-1.0);
    let (arena, solid, _) = vesica_prism_tagged(a, b);
    let err = validate_solid(&arena, solid).expect_err("zero-radius pair rejected");
    assert!(
        matches!(err, KernelV2Error::VertexOffSurface { .. }),
        "expected VertexOffSurface for zero radius, got {err:?}"
    );
}

/// A NEGATIVE radius can never be a real distance; the residual `‖r‖ − (−√2)`
/// is strictly larger than the true distance, so it is always rejected — no
/// negative radius can ever silently pass as on-surface. Typed
/// `VertexOffSurface`. (Twin check passes: −√2 == −√2.)
#[test]
fn negative_radius_pair_rejected_typed() {
    let a = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, 1.0, 0.0),
        axis_dir: up(),
        radius: -(2.0_f64.sqrt()),
    };
    let b = vesica_cyl(-1.0);
    let (arena, solid, _) = vesica_prism_tagged(a, b);
    let err = validate_solid(&arena, solid).expect_err("negative-radius pair rejected");
    assert!(
        matches!(err, KernelV2Error::VertexOffSurface { .. }),
        "expected VertexOffSurface for negative radius, got {err:?}"
    );
}

/// An infinite `axis_point` coordinate drives the residual computation to a
/// non-finite radial distance → the gradient is undefined → typed
/// `CurvedGeometryMismatch` ("on a defining surface's axis"), not a panic.
/// (Twin check passes: inf == inf in f64.)
#[test]
fn infinite_axis_point_pair_rejected_typed() {
    let a = PairSurface::Cylinder {
        axis_point: Point3::new(f64::INFINITY, 1.0, 0.0),
        axis_dir: up(),
        radius: 2.0_f64.sqrt(),
    };
    let b = vesica_cyl(-1.0);
    let (arena, solid, _) = vesica_prism_tagged(a, b);
    let err = validate_solid(&arena, solid).expect_err("infinite axis_point rejected");
    assert!(
        matches!(
            err,
            KernelV2Error::CurvedGeometryMismatch { .. } | KernelV2Error::VertexOffSurface { .. }
        ),
        "expected a typed finding for infinite axis_point, got {err:?}"
    );
}

/// An infinite `axis_dir` component makes the axial projection non-finite →
/// undefined gradient → typed `CurvedGeometryMismatch`, not a panic. (Twin
/// check passes: inf == inf.) This reaches the K7 `None` arm that the finite
/// non-unit case below skips.
#[test]
fn infinite_axis_dir_pair_rejected_typed() {
    let a = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, 1.0, 0.0),
        axis_dir: UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: f64::INFINITY,
        },
        radius: 2.0_f64.sqrt(),
    };
    let b = vesica_cyl(-1.0);
    let (arena, solid, _) = vesica_prism_tagged(a, b);
    let err = validate_solid(&arena, solid).expect_err("infinite axis_dir rejected");
    assert!(
        matches!(
            err,
            KernelV2Error::CurvedGeometryMismatch { .. } | KernelV2Error::VertexOffSurface { .. }
        ),
        "expected a typed finding for infinite axis_dir, got {err:?}"
    );
}

// ===========================================================================
// Group B — FINDING: non-unit axis_dir silently skews the residual
// ===========================================================================

/// FINDING (report, do NOT fix): `pair_surface_residual_gradient`
/// (geom.rs:957) uses `axis_dir` verbatim, assuming `|axis_dir| = 1`. Nothing
/// in `validate_solid` (or the sampler) checks unit-ness. Scaling the axis
/// direction to length 2 while leaving the geometry byte-identical changes
/// the computed residual from 0 to ≈7.7 at the z=H tip, so a geometrically
/// VALID surface-pair edge is rejected with a spurious `VertexOffSurface`.
///
/// This test pins the behavior: identical tips, unit axis → valid; length-2
/// axis → rejected. The residual must not depend on `|axis_dir|`.
#[test]
fn finding_non_unit_axis_dir_skews_residual() {
    // Control: canonical unit-axis descriptors validate cleanly.
    let (arena_ok, solid_ok, _) = vesica_prism_tagged(vesica_cyl(1.0), vesica_cyl(-1.0));
    validate_solid(&arena_ok, solid_ok).expect("unit-axis vesica pair validates");

    // Same intended cylinder, axis_dir scaled to length 2 (still +ẑ).
    let skewed_a = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, 1.0, 0.0),
        axis_dir: UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: 2.0,
        },
        radius: 2.0_f64.sqrt(),
    };
    let skewed_b = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, -1.0, 0.0),
        axis_dir: UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: 2.0,
        },
        radius: 2.0_f64.sqrt(),
    };
    let (arena_bad, solid_bad, _) = vesica_prism_tagged(skewed_a, skewed_b);
    let err = validate_solid(&arena_bad, solid_bad)
        .expect_err("non-unit axis skews the residual → spurious off-surface rejection");
    assert!(
        matches!(err, KernelV2Error::VertexOffSurface { .. }),
        "non-unit axis surfaces as VertexOffSurface (geometry is valid; the \
         descriptor axis length is the only difference), got {err:?}"
    );
}

// ===========================================================================
// Group C — twin ordered-pair equality (K5)
// ===========================================================================

/// K5 / spec §Ordering: twin comparison is EXACT equality on the ORDERED
/// pair. Swapping `a ↔ b` on ONE twin (the other keeps original order) is a
/// mismatch — even though both endpoints lie on both surfaces (so K7 passes
/// and a *set*-equality twin check would wrongly accept it). Guards against a
/// set-equality regression the cycle's radius-perturbation test cannot catch.
#[test]
fn swapped_operands_one_twin_rejected() {
    let (mut arena, solid, slots) = vesica_prism_tagged(vesica_cyl(1.0), vesica_cyl(-1.0));
    // Swap a↔b on exactly one half-edge; its twin retains (a, b).
    let slot = slots[0];
    let Some(Curve::SurfacePair { a, b }) = arena.half_edges[slot].map(|he| he.curve) else {
        unreachable!("retagged slot is a surface-pair edge");
    };
    arena.half_edges[slot].as_mut().unwrap().curve = Curve::SurfacePair { a: b, b: a };
    let err =
        validate_solid(&arena, solid).expect_err("swapped-operand twin is an ordered mismatch");
    assert!(
        matches!(err, KernelV2Error::CurveTwinMismatch { .. }),
        "expected CurveTwinMismatch for swapped operands, got {err:?}"
    );
}

// ===========================================================================
// Group D — sampler (`surface_pair_interior_samples`) degeneracies (K9)
// ===========================================================================

/// The perpendicular unequal-R pair used by the cycle: x²+y²=1 ∧ x²+z²=¼.
fn perp_unequal_pair() -> (PairSurface, PairSurface) {
    (
        PairSurface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: up(),
            radius: 1.0,
        },
        PairSurface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: UnitVector3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
            radius: 0.5,
        },
    )
}

/// Assert every returned sample lies on BOTH cylinders of the perpendicular
/// pair to 1e-9 (the invariant: no silently off-surface output).
fn assert_perp_samples_on_surface(samples: &[Point3]) {
    for s in samples {
        let ra = cyl_residual(s, Point3::new(0.0, 0.0, 0.0), [0.0, 0.0, 1.0], 1.0);
        let rb = cyl_residual(s, Point3::new(0.0, 0.0, 0.0), [0.0, 1.0, 0.0], 0.5);
        assert!(
            ra.abs() < 1e-9 && rb.abs() < 1e-9,
            "sampler returned an off-surface point {s:?} (ra={ra:e}, rb={rb:e})"
        );
    }
}

/// Non-positive / non-finite chord tolerances are typed rejections with a
/// named reason (K9: no chord fallback). Zero, negative, and NaN.
#[test]
fn sampler_nonpositive_or_nonfinite_tol_rejected() {
    let (a, b) = perp_unequal_pair();
    let start = Point3::new(0.0, 1.0, 0.5);
    let end = Point3::new(0.5, 0.75_f64.sqrt(), 0.0);
    for bad in [0.0, -1e-4, f64::NAN, f64::INFINITY] {
        let err = surface_pair_interior_samples(&a, &b, start, end, bad)
            .expect_err("non-positive/non-finite tol must be a typed error");
        assert!(
            !err.is_empty(),
            "tol {bad} rejection carries a named reason"
        );
    }
}

/// A zero-chord call (start == end, on-curve) must not fabricate off-surface
/// samples. Any samples it returns satisfy both residuals; the natural
/// outcome is an empty interior set.
#[test]
fn sampler_zero_chord_no_off_surface_samples() {
    let (a, b) = perp_unequal_pair();
    let p = Point3::new(0.0, 1.0, 0.5); // on both cylinders
    match surface_pair_interior_samples(&a, &b, p, p, 1e-4) {
        Ok(samples) => assert_perp_samples_on_surface(&samples),
        Err(reason) => assert!(!reason.is_empty(), "typed failure carries a reason"),
    }
}

/// Endpoints displaced 0.1 OFF the true curve: the sampler projects only the
/// midpoints, so it may converge or fail — but it must NEVER return a sample
/// that is off either surface. Every Ok sample satisfies both residuals; a
/// failure is typed with a reason.
#[test]
fn sampler_off_curve_endpoints_never_silently_off_surface() {
    let (a, b) = perp_unequal_pair();
    // True on-curve points, pushed 0.1 outward in a direction off both surfaces.
    let start = Point3::new(0.0 + 0.1, 1.0 + 0.1, 0.5);
    let end = Point3::new(0.5 + 0.1, 0.75_f64.sqrt() + 0.1, 0.0);
    match surface_pair_interior_samples(&a, &b, start, end, 1e-4) {
        Ok(samples) => assert_perp_samples_on_surface(&samples),
        Err(reason) => assert!(!reason.is_empty(), "typed failure carries a reason"),
    }
}

/// A near-tangent (shallow-secant) pair: two unit cylinders whose axes are
/// 1.999999 apart intersect in two rulings a hair off the x=d/2 tangent line,
/// so the Gauss–Newton system is nearly rank-deficient (det ≈ y² ≈ 1e-6).
/// The midpoint of two points on OPPOSITE rulings is off-curve, forcing the
/// ill-conditioned solve. It must either fail typed or return only certified
/// samples — never silently off-surface, never a panic/NaN.
#[test]
fn sampler_near_tangent_no_silent_off_surface() {
    let d = 1.999_999_f64;
    let a = PairSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: up(),
        radius: 1.0,
    };
    let b = PairSurface::Cylinder {
        axis_point: Point3::new(d, 0.0, 0.0),
        axis_dir: up(),
        radius: 1.0,
    };
    // Intersection rulings sit at x = d/2, y = ±√(1 − (d/2)²).
    let x = d / 2.0;
    let y = (1.0 - x * x).sqrt();
    let start = Point3::new(x, y, 0.0);
    let end = Point3::new(x, -y, 0.0);
    match surface_pair_interior_samples(&a, &b, start, end, 1e-4) {
        Ok(samples) => {
            for s in &samples {
                assert!(
                    s.x().is_finite() && s.y().is_finite() && s.z().is_finite(),
                    "no NaN/inf sample from the ill-conditioned solve, got {s:?}"
                );
                let ra = cyl_residual(s, Point3::new(0.0, 0.0, 0.0), [0.0, 0.0, 1.0], 1.0);
                let rb = cyl_residual(s, Point3::new(d, 0.0, 0.0), [0.0, 0.0, 1.0], 1.0);
                assert!(
                    ra.abs() < 1e-9 && rb.abs() < 1e-9,
                    "near-tangent sample off surface {s:?} (ra={ra:e}, rb={rb:e})"
                );
            }
        }
        Err(reason) => assert!(
            !reason.is_empty(),
            "typed near-tangent failure carries a reason"
        ),
    }
}

/// Sanity that the shared perpendicular-pair fixture is genuinely curved and
/// the sampler certifies its interior — a positive control so the "no
/// off-surface samples" assertions above are meaningful (not vacuously
/// passing on empty output for a case that should refine).
#[test]
fn sampler_curved_control_certifies_interior() {
    let (a, b) = perp_unequal_pair();
    let p_of = |phi: f64| {
        let x = 0.5 * phi.sin();
        Point3::new(x, (1.0 - x * x).sqrt(), 0.5 * phi.cos())
    };
    let samples = surface_pair_interior_samples(&a, &b, p_of(0.0), p_of(PI / 2.0), 1e-4)
        .expect("curved perpendicular pair refines");
    assert!(samples.len() >= 3, "quarter-turn quartic needs refinement");
    assert_perp_samples_on_surface(&samples);
}
