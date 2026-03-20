//! Finite-difference verification for all constraint Jacobians.
//!
//! Central differences: (f(x+h) - f(x-h)) / 2h with h = 1e-5.
//! Every analytic derivative must agree within 1e-7 of the FD approximation.
//! This is the canary test that catches wrong Jacobians.

use nalgebra::Point2;
use sketch_solver::core::constraint::{ConstraintEq, ConstraintImpl};
use sketch_solver::core::types::*;

const H: f64 = 1e-5;
const TOL: f64 = 1e-7;

/// Verify that analytic Jacobian matches finite-difference approximation.
fn verify_jacobian(c: &ConstraintImpl, params: &[f64]) {
    let n = params.len();
    let m = c.num_equations();
    if m == 0 {
        return;
    }
    let mut f_plus = vec![0.0; m];
    let mut f_minus = vec![0.0; m];
    let mut analytic_entries = Vec::new();
    c.jacobian(params, 0, &mut analytic_entries);

    for j in 0..n {
        let mut x_plus = params.to_vec();
        let mut x_minus = params.to_vec();
        x_plus[j] += H;
        x_minus[j] -= H;
        c.residuals(&x_plus, &mut f_plus);
        c.residuals(&x_minus, &mut f_minus);
        for i in 0..m {
            let fd = (f_plus[i] - f_minus[i]) / (2.0 * H);
            // Sum all entries for this (i, j) pair (sparse format may have duplicates)
            let analytic: f64 = analytic_entries
                .iter()
                .filter(|(r, c, _)| *r == i && *c == j)
                .map(|(_, _, v)| v)
                .sum();
            let err = (analytic - fd).abs();
            assert!(
                err < TOL,
                "Jacobian mismatch for {:?}: [{},{}] analytic={:.10e}, fd={:.10e}, err={:.10e}",
                constraint_name(c),
                i,
                j,
                analytic,
                fd,
                err,
            );
        }
    }
}

fn constraint_name(c: &ConstraintImpl) -> &'static str {
    match c {
        ConstraintImpl::Coincident { .. } => "Coincident",
        ConstraintImpl::Horizontal { .. } => "Horizontal",
        ConstraintImpl::Vertical { .. } => "Vertical",
        ConstraintImpl::SymmetricH { .. } => "SymmetricH",
        ConstraintImpl::SymmetricV { .. } => "SymmetricV",
        ConstraintImpl::Midpoint { .. } => "Midpoint",
        ConstraintImpl::Dragged { .. } => "Dragged",
        ConstraintImpl::Radius { .. } => "Radius",
        ConstraintImpl::Diameter { .. } => "Diameter",
        ConstraintImpl::HDistance { .. } => "HDistance",
        ConstraintImpl::VDistance { .. } => "VDistance",
        ConstraintImpl::DistancePP { .. } => "DistancePP",
        ConstraintImpl::EqualLength { .. } => "EqualLength",
        ConstraintImpl::Parallel { .. } => "Parallel",
        ConstraintImpl::Perpendicular { .. } => "Perpendicular",
        ConstraintImpl::Angle { .. } => "Angle",
        ConstraintImpl::OnLine { .. } => "OnLine",
        ConstraintImpl::OnCircle { .. } => "OnCircle",
        ConstraintImpl::DistancePL { .. } => "DistancePL",
        ConstraintImpl::TangentLineCircle { .. } => "TangentLineCircle",
        ConstraintImpl::TangentArcArc { .. } => "TangentArcArc",
        ConstraintImpl::SymmetricLine { .. } => "SymmetricLine",
        ConstraintImpl::EqualAngle { .. } => "EqualAngle",
        ConstraintImpl::Ratio { .. } => "Ratio",
        ConstraintImpl::EqualPointToLine { .. } => "EqualPointToLine",
        ConstraintImpl::SameOrientation => "SameOrientation",
        ConstraintImpl::EqualRadius { .. } => "EqualRadius",
    }
}

// ── Helper builders ──────────────────────────────────────────────────────

fn pt(offset: usize) -> PointIdx {
    PointIdx(offset)
}

fn line(s_off: usize, e_off: usize) -> LineIdx {
    LineIdx {
        start: pt(s_off),
        end: pt(e_off),
    }
}

// ── Group 1: Linear constraints ──────────────────────────────────────────

#[test]
fn fd_coincident() {
    let c = ConstraintImpl::Coincident {
        p1: pt(0),
        p2: pt(2),
    };
    verify_jacobian(&c, &[1.0, 2.0, 3.0, 4.0]);
    verify_jacobian(&c, &[0.0, 0.0, 0.0, 0.0]);
    verify_jacobian(&c, &[-5.0, 7.3, 2.1, -0.4]);
}

#[test]
fn fd_horizontal() {
    let c = ConstraintImpl::Horizontal { line: line(0, 2) };
    verify_jacobian(&c, &[0.0, 0.0, 5.0, 3.0]);
    verify_jacobian(&c, &[1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn fd_vertical() {
    let c = ConstraintImpl::Vertical { line: line(0, 2) };
    verify_jacobian(&c, &[0.0, 0.0, 3.0, 5.0]);
}

#[test]
fn fd_symmetric_h() {
    let c = ConstraintImpl::SymmetricH {
        p1: pt(0),
        p2: pt(2),
    };
    verify_jacobian(&c, &[3.0, 2.0, -3.0, 2.0]);
    verify_jacobian(&c, &[1.5, -0.7, 0.3, 4.1]);
}

#[test]
fn fd_symmetric_v() {
    let c = ConstraintImpl::SymmetricV {
        p1: pt(0),
        p2: pt(2),
    };
    verify_jacobian(&c, &[2.0, 3.0, 2.0, -3.0]);
    verify_jacobian(&c, &[1.5, -0.7, 0.3, 4.1]);
}

#[test]
fn fd_midpoint() {
    let c = ConstraintImpl::Midpoint {
        point: pt(0),
        line: line(2, 4),
    };
    verify_jacobian(&c, &[2.5, 1.5, 0.0, 0.0, 5.0, 3.0]);
    verify_jacobian(&c, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
}

#[test]
fn fd_dragged() {
    let c = ConstraintImpl::Dragged {
        point: pt(0),
        target: Point2::new(5.0, 7.0),
    };
    verify_jacobian(&c, &[5.0, 7.0]);
    verify_jacobian(&c, &[1.0, 2.0]);
}

#[test]
fn fd_radius() {
    let c = ConstraintImpl::Radius {
        r: RadiusIdx(0),
        target: 5.0,
    };
    verify_jacobian(&c, &[5.0]);
    verify_jacobian(&c, &[3.2]);
}

#[test]
fn fd_diameter() {
    let c = ConstraintImpl::Diameter {
        r: RadiusIdx(0),
        target: 10.0,
    };
    verify_jacobian(&c, &[5.0]);
    verify_jacobian(&c, &[3.2]);
}

#[test]
fn fd_hdistance() {
    let c = ConstraintImpl::HDistance {
        p1x: 0,
        p2x: 2,
        d: 5.0,
    };
    verify_jacobian(&c, &[1.0, 99.0, 6.0, 99.0]);
}

#[test]
fn fd_vdistance() {
    let c = ConstraintImpl::VDistance {
        p1y: 1,
        p2y: 3,
        d: 3.0,
    };
    verify_jacobian(&c, &[99.0, 1.0, 99.0, 4.0]);
}

// ── Group 2: Nonlinear fundamentals ──────────────────────────────────────

#[test]
fn fd_distance_pp() {
    let c = ConstraintImpl::DistancePP {
        p1: pt(0),
        p2: pt(2),
        d: 5.0,
    };
    verify_jacobian(&c, &[0.0, 0.0, 3.0, 4.0]);
    verify_jacobian(&c, &[1.0, 2.0, 4.0, 6.0]);
    verify_jacobian(&c, &[-1.0, -1.0, 2.0, 3.0]);
}

#[test]
fn fd_distance_pp_near_zero() {
    // Test small separation: points close but well above FD step size
    let c = ConstraintImpl::DistancePP {
        p1: pt(0),
        p2: pt(2),
        d: 0.0,
    };
    verify_jacobian(&c, &[0.0, 0.0, 0.01, 0.02]);
}

#[test]
fn fd_equal_length() {
    let c = ConstraintImpl::EqualLength {
        l1: line(0, 2),
        l2: line(4, 6),
    };
    verify_jacobian(&c, &[0.0, 0.0, 3.0, 4.0, 1.0, 1.0, 4.0, 5.0]);
    verify_jacobian(&c, &[1.0, 2.0, 4.0, 2.0, 0.0, 0.0, 5.0, 0.0]);
}

#[test]
fn fd_parallel() {
    let c = ConstraintImpl::Parallel {
        l1: line(0, 2),
        l2: line(4, 6),
    };
    verify_jacobian(&c, &[0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 3.0, 0.0]);
    verify_jacobian(&c, &[1.0, 2.0, 3.0, 5.0, -1.0, 0.0, 4.0, 7.0]);
}

#[test]
fn fd_perpendicular() {
    let c = ConstraintImpl::Perpendicular {
        l1: line(0, 2),
        l2: line(4, 6),
    };
    verify_jacobian(&c, &[0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    verify_jacobian(&c, &[1.0, 2.0, 3.0, 5.0, -1.0, 0.0, 4.0, 7.0]);
}

#[test]
fn fd_angle() {
    let c = ConstraintImpl::Angle {
        l1: line(0, 2),
        l2: line(4, 6),
        value_rad: std::f64::consts::FRAC_PI_4,
    };
    // Two lines at ~45 degrees
    verify_jacobian(&c, &[0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
    // Non-trivial configuration
    verify_jacobian(&c, &[1.0, 2.0, 4.0, 2.0, 0.0, 0.0, 3.0, 5.0]);
    // Another config
    verify_jacobian(&c, &[-1.0, -1.0, 2.0, 3.0, 0.5, 0.5, -0.5, 2.5]);
}

#[test]
fn fd_angle_90() {
    let c = ConstraintImpl::Angle {
        l1: line(0, 2),
        l2: line(4, 6),
        value_rad: std::f64::consts::FRAC_PI_2,
    };
    verify_jacobian(&c, &[0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0, 3.0]);
}

// ── Group 3: Point-on-entity ─────────────────────────────────────────────

#[test]
fn fd_on_line() {
    let c = ConstraintImpl::OnLine {
        point: pt(0),
        line: line(2, 4),
    };
    // Point on the line
    verify_jacobian(&c, &[0.5, 0.0, 0.0, 0.0, 1.0, 0.0]);
    // Point off the line
    verify_jacobian(&c, &[0.5, 1.0, 0.0, 0.0, 1.0, 0.0]);
    // General config
    verify_jacobian(&c, &[2.0, 3.0, 1.0, 1.0, 4.0, 5.0]);
}

#[test]
fn fd_on_circle_param_radius() {
    // Circle with explicit radius parameter at index 4
    let c = ConstraintImpl::OnCircle {
        point: pt(0),
        center: pt(2),
        radius: RadiusDef::Param(RadiusIdx(4)),
    };
    // Point on circle of radius 5 centered at (0,0)
    verify_jacobian(&c, &[3.0, 4.0, 0.0, 0.0, 5.0]);
    // Off-circle
    verify_jacobian(&c, &[1.0, 1.0, 0.0, 0.0, 5.0]);
    // Non-origin center
    verify_jacobian(&c, &[4.0, 5.0, 1.0, 2.0, 3.0]);
}

#[test]
fn fd_on_circle_implicit_radius() {
    // Arc: center at indices 0-1, point at 2-3, start at 4-5
    // radius = dist(center, start)
    let c = ConstraintImpl::OnCircle {
        point: pt(2),
        center: pt(0),
        radius: RadiusDef::Implicit(pt(4)),
    };
    // Point and start both at radius 5 from center
    verify_jacobian(&c, &[0.0, 0.0, 3.0, 4.0, 5.0, 0.0]);
    // General config
    verify_jacobian(&c, &[1.0, 1.0, 4.0, 5.0, 3.0, 2.0]);
}

// ── Group 3b: Cardinal-position FD tests ─────────────────────────────────
// Verify analytical Jacobian correctness at axis-aligned positions where
// individual partial derivatives vanish. A zero derivative component is
// directional information, not a singularity.

#[test]
fn fd_on_circle_cardinal_above() {
    // Point directly above center: (cx, cy+r). gx should be 0, gy should be 1.
    let c = ConstraintImpl::OnCircle {
        point: pt(0),
        center: pt(2),
        radius: RadiusDef::Param(RadiusIdx(4)),
    };
    // Point at (5, 10+5) = (5, 15), center at (5, 10), radius 5
    verify_jacobian(&c, &[5.0, 15.0, 5.0, 10.0, 5.0]);
}

#[test]
fn fd_on_circle_cardinal_right() {
    // Point directly right of center: (cx+r, cy). gx should be 1, gy should be 0.
    let c = ConstraintImpl::OnCircle {
        point: pt(0),
        center: pt(2),
        radius: RadiusDef::Param(RadiusIdx(4)),
    };
    // Point at (5+3, 10) = (8, 10), center at (5, 10), radius 3
    verify_jacobian(&c, &[8.0, 10.0, 5.0, 10.0, 3.0]);
}

#[test]
fn fd_on_circle_cardinal_left() {
    // Point directly left of center: (cx-r, cy).
    let c = ConstraintImpl::OnCircle {
        point: pt(0),
        center: pt(2),
        radius: RadiusDef::Param(RadiusIdx(4)),
    };
    verify_jacobian(&c, &[2.0, 10.0, 5.0, 10.0, 3.0]);
}

#[test]
fn fd_on_circle_cardinal_below() {
    // Point directly below center: (cx, cy-r).
    let c = ConstraintImpl::OnCircle {
        point: pt(0),
        center: pt(2),
        radius: RadiusDef::Param(RadiusIdx(4)),
    };
    verify_jacobian(&c, &[5.0, 7.0, 5.0, 10.0, 3.0]);
}

#[test]
fn fd_on_circle_implicit_cardinal() {
    // Arc with OnCircle::Implicit where both point and start are at cardinal positions.
    // center at (0,0), point at (0, 5) [above], start at (5, 0) [right]
    let c = ConstraintImpl::OnCircle {
        point: pt(2),
        center: pt(0),
        radius: RadiusDef::Implicit(pt(4)),
    };
    // params: cx=0, cy=0, px=0, py=5, sx=5, sy=0
    verify_jacobian(&c, &[0.0, 0.0, 0.0, 5.0, 5.0, 0.0]);
    // Both at cardinal left/above
    verify_jacobian(&c, &[0.0, 0.0, -5.0, 0.0, 0.0, 5.0]);
    // Both at cardinal below/left
    verify_jacobian(&c, &[0.0, 0.0, 0.0, -5.0, -5.0, 0.0]);
}

#[test]
fn fd_distance_pp_cardinal_vertical() {
    // Two points vertically aligned: ∂f/∂x should be 0, ∂f/∂y should be ±1.
    let c = ConstraintImpl::DistancePP {
        p1: pt(0),
        p2: pt(2),
        d: 7.0,
    };
    // p1=(3, 1), p2=(3, 8) — vertically aligned, distance 7
    verify_jacobian(&c, &[3.0, 1.0, 3.0, 8.0]);
}

#[test]
fn fd_distance_pp_cardinal_horizontal() {
    // Two points horizontally aligned: ∂f/∂y should be 0, ∂f/∂x should be ±1.
    let c = ConstraintImpl::DistancePP {
        p1: pt(0),
        p2: pt(2),
        d: 5.0,
    };
    // p1=(2, 4), p2=(7, 4) — horizontally aligned, distance 5
    verify_jacobian(&c, &[2.0, 4.0, 7.0, 4.0]);
}

// ── Group 4: Normalized point-line distance ──────────────────────────────

#[test]
fn fd_distance_pl() {
    let c = ConstraintImpl::DistancePL {
        point: pt(0),
        line: line(2, 4),
        d: 2.0,
        sign: 1.0,
    };
    // Point at distance 2 from horizontal line
    verify_jacobian(&c, &[5.0, 2.0, 0.0, 0.0, 10.0, 0.0]);
    // General config
    verify_jacobian(&c, &[3.0, 5.0, 1.0, 1.0, 4.0, 2.0]);
    // Another config
    verify_jacobian(&c, &[-1.0, 3.0, 0.0, 0.0, 2.0, 1.0]);
}

#[test]
fn fd_distance_pl_angled_line() {
    let c = ConstraintImpl::DistancePL {
        point: pt(0),
        line: line(2, 4),
        d: 1.0,
        sign: 1.0,
    };
    // Diagonal line
    verify_jacobian(&c, &[1.0, 2.0, 0.0, 0.0, 3.0, 3.0]);
}

// ── Group 5: Tangent ─────────────────────────────────────────────────────

#[test]
fn fd_tangent_line_circle_param() {
    let c = ConstraintImpl::TangentLineCircle {
        line: line(0, 2),
        center: pt(4),
        radius: RadiusDef::Param(RadiusIdx(6)),
        sign: 1.0,
    };
    // Line along x-axis, circle centered at (5, 3) with radius 3
    verify_jacobian(&c, &[0.0, 0.0, 10.0, 0.0, 5.0, 3.0, 3.0]);
    // General config
    verify_jacobian(&c, &[1.0, 1.0, 4.0, 2.0, 2.0, 5.0, 2.0]);
}

#[test]
fn fd_tangent_line_circle_implicit() {
    // Arc: center at 4-5, start at 6-7
    let c = ConstraintImpl::TangentLineCircle {
        line: line(0, 2),
        center: pt(4),
        radius: RadiusDef::Implicit(pt(6)),
        sign: 1.0,
    };
    verify_jacobian(&c, &[0.0, 0.0, 10.0, 0.0, 5.0, 3.0, 5.0, 6.0]);
    verify_jacobian(&c, &[1.0, 2.0, 4.0, 1.0, 3.0, 5.0, 6.0, 4.0]);
}

#[test]
fn fd_tangent_arc_arc_external() {
    // Two arcs with explicit radii
    let c = ConstraintImpl::TangentArcArc {
        c1: pt(0),
        r1: RadiusDef::Param(RadiusIdx(4)),
        c2: pt(2),
        r2: RadiusDef::Param(RadiusIdx(5)),
        internal: false,
    };
    verify_jacobian(&c, &[0.0, 0.0, 5.0, 0.0, 2.0, 3.0]);
    verify_jacobian(&c, &[1.0, 1.0, 4.0, 3.0, 1.5, 2.5]);
}

#[test]
fn fd_tangent_arc_arc_internal() {
    let c = ConstraintImpl::TangentArcArc {
        c1: pt(0),
        r1: RadiusDef::Param(RadiusIdx(4)),
        c2: pt(2),
        r2: RadiusDef::Param(RadiusIdx(5)),
        internal: true,
    };
    // r1 > r2
    verify_jacobian(&c, &[0.0, 0.0, 3.0, 0.0, 5.0, 2.0]);
    // r2 > r1
    verify_jacobian(&c, &[0.0, 0.0, 3.0, 0.0, 2.0, 5.0]);
}

#[test]
fn fd_tangent_arc_arc_implicit() {
    // Arc1: center=0, start=2; Arc2: center=4, start=6
    let c = ConstraintImpl::TangentArcArc {
        c1: pt(0),
        r1: RadiusDef::Implicit(pt(2)),
        c2: pt(4),
        r2: RadiusDef::Implicit(pt(6)),
        internal: false,
    };
    verify_jacobian(&c, &[0.0, 0.0, 3.0, 0.0, 8.0, 0.0, 10.0, 2.0]);
}

// ── Group 6: Symmetric about arbitrary line ──────────────────────────────

#[test]
fn fd_symmetric_line() {
    let c = ConstraintImpl::SymmetricLine {
        p1: pt(0),
        p2: pt(2),
        line: line(4, 6),
    };
    // Symmetric about a horizontal line
    verify_jacobian(&c, &[1.0, 3.0, 1.0, -1.0, 0.0, 1.0, 5.0, 1.0]);
    // Symmetric about a diagonal line
    verify_jacobian(&c, &[2.0, 4.0, 4.0, 2.0, 0.0, 0.0, 3.0, 3.0]);
    // General config
    verify_jacobian(&c, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
}

// ── Group 7: Compound ────────────────────────────────────────────────────

#[test]
fn fd_equal_angle() {
    let c = ConstraintImpl::EqualAngle {
        l1: line(0, 2),
        l2: line(4, 6),
        l3: line(8, 10),
        l4: line(12, 14),
    };
    // 16 params
    verify_jacobian(
        &c,
        &[
            0.0, 0.0, 1.0, 0.0, // l1: horizontal
            0.0, 0.0, 1.0, 1.0, // l2: 45 degrees
            0.0, 0.0, 0.0, 1.0, // l3: vertical
            0.0, 0.0, -1.0, 1.0, // l4: 135 degrees — same 45 degree angle
        ],
    );
    // General config
    verify_jacobian(
        &c,
        &[
            1.0, 2.0, 3.0, 5.0, 0.0, 1.0, 2.0, 4.0, -1.0, 0.0, 3.0, 2.0, 1.0, 1.0, 4.0, 0.0,
        ],
    );
}

#[test]
fn fd_ratio() {
    let c = ConstraintImpl::Ratio {
        l1: line(0, 2),
        l2: line(4, 6),
        k: 2.0,
    };
    verify_jacobian(&c, &[0.0, 0.0, 3.0, 4.0, 0.0, 0.0, 1.0, 2.0]);
    verify_jacobian(&c, &[1.0, 1.0, 4.0, 3.0, 2.0, 0.0, 5.0, 1.0]);
}

#[test]
fn fd_equal_point_to_line() {
    let c = ConstraintImpl::EqualPointToLine {
        p1: pt(0),
        p2: pt(2),
        line: line(4, 6),
    };
    verify_jacobian(&c, &[1.0, 3.0, 2.0, 1.0, 0.0, 0.0, 5.0, 0.0]);
    verify_jacobian(&c, &[2.0, 4.0, 3.0, 1.0, 1.0, 1.0, 4.0, 5.0]);
}

#[test]
fn fd_same_orientation() {
    let c = ConstraintImpl::SameOrientation;
    // No equations, verify_jacobian handles m=0
    verify_jacobian(&c, &[1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn fd_equal_radius() {
    let c = ConstraintImpl::EqualRadius {
        r1: RadiusIdx(0),
        r2: RadiusIdx(1),
    };
    verify_jacobian(&c, &[5.0, 3.0]);
    verify_jacobian(&c, &[7.0, 7.0]);
}

// ── Residual sanity checks ───────────────────────────────────────────────

#[test]
fn residual_coincident_satisfied() {
    let c = ConstraintImpl::Coincident {
        p1: pt(0),
        p2: pt(2),
    };
    let mut out = [0.0; 2];
    c.residuals(&[3.0, 4.0, 3.0, 4.0], &mut out);
    assert!(out[0].abs() < 1e-15);
    assert!(out[1].abs() < 1e-15);
}

#[test]
fn residual_distance_pp_satisfied() {
    let c = ConstraintImpl::DistancePP {
        p1: pt(0),
        p2: pt(2),
        d: 5.0,
    };
    let mut out = [0.0; 1];
    c.residuals(&[0.0, 0.0, 3.0, 4.0], &mut out);
    assert!(out[0].abs() < 1e-12);
}

#[test]
fn residual_parallel_satisfied() {
    let c = ConstraintImpl::Parallel {
        l1: line(0, 2),
        l2: line(4, 6),
    };
    let mut out = [0.0; 1];
    // Two parallel horizontal lines
    c.residuals(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 2.0, 1.0], &mut out);
    assert!(out[0].abs() < 1e-15);
}

#[test]
fn residual_perpendicular_satisfied() {
    let c = ConstraintImpl::Perpendicular {
        l1: line(0, 2),
        l2: line(4, 6),
    };
    let mut out = [0.0; 1];
    c.residuals(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0], &mut out);
    assert!(out[0].abs() < 1e-15);
}

#[test]
fn residual_on_line_satisfied() {
    let c = ConstraintImpl::OnLine {
        point: pt(0),
        line: line(2, 4),
    };
    let mut out = [0.0; 1];
    // Point (0.5, 0) on line from (0,0) to (1,0)
    c.residuals(&[0.5, 0.0, 0.0, 0.0, 1.0, 0.0], &mut out);
    assert!(out[0].abs() < 1e-15);
}

#[test]
fn residual_symmetric_h() {
    let c = ConstraintImpl::SymmetricH {
        p1: pt(0),
        p2: pt(2),
    };
    let mut out = [0.0; 2];
    // Symmetric about Y axis: (3, 2) and (-3, 2)
    c.residuals(&[3.0, 2.0, -3.0, 2.0], &mut out);
    assert!(out[0].abs() < 1e-15);
    assert!(out[1].abs() < 1e-15);
}

#[test]
fn residual_symmetric_v() {
    let c = ConstraintImpl::SymmetricV {
        p1: pt(0),
        p2: pt(2),
    };
    let mut out = [0.0; 2];
    // Symmetric about X axis: (2, 3) and (2, -3)
    c.residuals(&[2.0, 3.0, 2.0, -3.0], &mut out);
    assert!(out[0].abs() < 1e-15);
    assert!(out[1].abs() < 1e-15);
}

#[test]
fn residual_angle_45_degrees() {
    let c = ConstraintImpl::Angle {
        l1: line(0, 2),
        l2: line(4, 6),
        value_rad: std::f64::consts::FRAC_PI_4,
    };
    let mut out = [0.0; 1];
    // l1 = horizontal (1,0), l2 = 45 degrees (1,1)
    c.residuals(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0], &mut out);
    assert!(out[0].abs() < 1e-12, "residual = {}", out[0]);
}

// ── Hessian FD verification ───────────────────────────────────────────────
//
// For each constraint with a non-trivial Hessian, verify by finite differences:
// (J(x+h) - J(x-h)) / 2h should match the analytical Hessian entries.

const H_HESS: f64 = 1e-5;
const TOL_HESS: f64 = 1e-5;

/// Verify that analytic Hessian matches finite-difference of the Jacobian.
fn verify_hessian(c: &ConstraintImpl, params: &[f64]) {
    let n = params.len();
    let m = c.num_equations();
    if m == 0 {
        return;
    }

    // Get analytic Hessian entries
    let hessian_entries = c.hessian(params, 0);
    if hessian_entries.is_empty() {
        return; // no Hessian to verify (linear constraint or unimplemented)
    }

    // Build dense Hessian per equation from sparse entries
    let mut h_analytic = vec![vec![vec![0.0; n]; n]; m];
    for &(eq, ci, cj, val) in &hessian_entries {
        if eq < m && ci < n && cj < n {
            h_analytic[eq][ci][cj] += val;
        }
    }

    // FD Hessian: perturb each param, recompute Jacobian, difference
    for j in 0..n {
        let mut x_plus = params.to_vec();
        let mut x_minus = params.to_vec();
        x_plus[j] += H_HESS;
        x_minus[j] -= H_HESS;

        let mut jac_plus = Vec::new();
        let mut jac_minus = Vec::new();
        c.jacobian(&x_plus, 0, &mut jac_plus);
        c.jacobian(&x_minus, 0, &mut jac_minus);

        // Build dense Jacobian rows
        let mut j_plus = vec![vec![0.0; n]; m];
        let mut j_minus = vec![vec![0.0; n]; m];
        for &(r, col, val) in &jac_plus {
            if r < m && col < n {
                j_plus[r][col] += val;
            }
        }
        for &(r, col, val) in &jac_minus {
            if r < m && col < n {
                j_minus[r][col] += val;
            }
        }

        for eq in 0..m {
            for k in 0..n {
                let fd = (j_plus[eq][k] - j_minus[eq][k]) / (2.0 * H_HESS);
                let analytic = h_analytic[eq][k][j];
                let err = (analytic - fd).abs();
                // Use relative tolerance for large values
                let scale = fd.abs().max(analytic.abs()).max(1.0);
                assert!(
                    err / scale < TOL_HESS,
                    "Hessian mismatch for {:?}: eq={}, [{},{}] analytic={:.8e}, fd={:.8e}, err={:.8e}",
                    constraint_name(c), eq, k, j, analytic, fd, err,
                );
            }
        }
    }
}

#[test]
fn fd_hessian_distance_pp() {
    let c = ConstraintImpl::DistancePP { p1: pt(0), p2: pt(2), d: 5.0 };
    verify_hessian(&c, &[0.0, 0.0, 3.0, 4.0]);
    verify_hessian(&c, &[1.0, 2.0, 4.0, 6.0]);
    // Cardinal: vertically aligned
    verify_hessian(&c, &[3.0, 1.0, 3.0, 8.0]);
    // Cardinal: horizontally aligned
    verify_hessian(&c, &[2.0, 4.0, 7.0, 4.0]);
}

#[test]
fn fd_hessian_equal_length() {
    let c = ConstraintImpl::EqualLength { l1: line(0, 2), l2: line(4, 6) };
    verify_hessian(&c, &[0.0, 0.0, 3.0, 4.0, 1.0, 1.0, 4.0, 5.0]);
    verify_hessian(&c, &[1.0, 2.0, 4.0, 2.0, 0.0, 0.0, 5.0, 0.0]);
}

#[test]
fn fd_hessian_on_circle_param() {
    let c = ConstraintImpl::OnCircle {
        point: pt(0), center: pt(2),
        radius: RadiusDef::Param(RadiusIdx(4)),
    };
    verify_hessian(&c, &[3.0, 4.0, 0.0, 0.0, 5.0]);
    // Cardinal: directly above
    verify_hessian(&c, &[5.0, 15.0, 5.0, 10.0, 5.0]);
    // Cardinal: directly right
    verify_hessian(&c, &[8.0, 10.0, 5.0, 10.0, 3.0]);
}

#[test]
fn fd_hessian_on_circle_implicit() {
    let c = ConstraintImpl::OnCircle {
        point: pt(2), center: pt(0),
        radius: RadiusDef::Implicit(pt(4)),
    };
    verify_hessian(&c, &[0.0, 0.0, 3.0, 4.0, 5.0, 0.0]);
    // Cardinal: point above, start right
    verify_hessian(&c, &[0.0, 0.0, 0.0, 5.0, 5.0, 0.0]);
}

#[test]
fn fd_hessian_tangent_arc_arc() {
    let c = ConstraintImpl::TangentArcArc {
        c1: pt(0), r1: RadiusDef::Param(RadiusIdx(4)),
        c2: pt(2), r2: RadiusDef::Param(RadiusIdx(5)),
        internal: false,
    };
    verify_hessian(&c, &[0.0, 0.0, 5.0, 0.0, 2.0, 3.0]);
    verify_hessian(&c, &[1.0, 1.0, 4.0, 3.0, 1.5, 2.5]);
}

#[test]
fn fd_hessian_ratio() {
    let c = ConstraintImpl::Ratio { l1: line(0, 2), l2: line(4, 6), k: 2.0 };
    verify_hessian(&c, &[0.0, 0.0, 3.0, 4.0, 0.0, 0.0, 1.0, 2.0]);
}

#[test]
fn fd_hessian_parallel() {
    let c = ConstraintImpl::Parallel { l1: line(0, 2), l2: line(4, 6) };
    verify_hessian(&c, &[0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 3.0, 0.0]);
    verify_hessian(&c, &[1.0, 2.0, 3.0, 5.0, -1.0, 0.0, 4.0, 7.0]);
}

#[test]
fn fd_hessian_perpendicular() {
    let c = ConstraintImpl::Perpendicular { l1: line(0, 2), l2: line(4, 6) };
    verify_hessian(&c, &[0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    verify_hessian(&c, &[1.0, 2.0, 3.0, 5.0, -1.0, 0.0, 4.0, 7.0]);
}

#[test]
fn fd_hessian_equal_point_to_line() {
    let c = ConstraintImpl::EqualPointToLine { p1: pt(0), p2: pt(2), line: line(4, 6) };
    verify_hessian(&c, &[1.0, 3.0, 2.0, 1.0, 0.0, 0.0, 5.0, 0.0]);
}
