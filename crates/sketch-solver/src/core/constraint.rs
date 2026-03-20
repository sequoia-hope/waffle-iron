//! Constraint trait and internal constraint enum.
//!
//! `ConstraintEq` is the trait that all constraints implement.
//! `ConstraintImpl` is the enum of all concrete constraint variants,
//! storing pre-resolved typed indices from `ParamLayout`.

use nalgebra::Point2;

use super::types::*;

/// Singularity guard: clamp denominators away from zero.
pub const EPSILON: f64 = 1e-12;

/// A constraint that contributes scalar equations to the solver.
pub trait ConstraintEq {
    /// Number of scalar equations this constraint contributes.
    fn num_equations(&self) -> usize;

    /// Scale type per equation (Distance or Angle) for D_row construction.
    fn scale_types(&self) -> &[ScaleType];

    /// Compute residuals f(x). Should be zero when satisfied.
    /// `out` slice has length == `num_equations()`.
    fn residuals(&self, params: &[f64], out: &mut [f64]);

    /// Append sparse Jacobian entries: (global_row, param_col, value).
    /// `eq_offset` is this constraint's starting row in the global Jacobian.
    fn jacobian(&self, params: &[f64], eq_offset: usize, out: &mut Vec<(usize, usize, f64)>);

    /// Sparse Hessian entries: (eq_index, param_col1, param_col2, value).
    /// Only needed for nonlinear constraints. Default: empty (linear constraint).
    /// Used by restricted Hessian analysis to detect phantom DOF at cardinal positions.
    fn hessian(&self, params: &[f64], eq_offset: usize) -> Vec<(usize, usize, usize, f64)> {
        let _ = (params, eq_offset);
        Vec::new()
    }
}

/// Internal constraint representation with pre-resolved typed indices.
///
/// Built from `SketchConstraint` + `ParamLayout` by the builder.
/// All entity ID lookups are resolved at build time — the solver never
/// touches entity IDs.
pub enum ConstraintImpl {
    // ── Group 1: Linear (constant Jacobian) ──────────────────────────
    Coincident {
        p1: PointIdx,
        p2: PointIdx,
    },
    Horizontal {
        line: LineIdx,
    },
    Vertical {
        line: LineIdx,
    },
    SymmetricH {
        p1: PointIdx,
        p2: PointIdx,
    },
    SymmetricV {
        p1: PointIdx,
        p2: PointIdx,
    },
    Midpoint {
        point: PointIdx,
        line: LineIdx,
    },
    Dragged {
        point: PointIdx,
        target: Point2<f64>,
    },
    Radius {
        r: RadiusIdx,
        target: f64,
    },
    Diameter {
        r: RadiusIdx,
        target: f64,
    },
    HDistance {
        p1x: usize,
        p2x: usize,
        d: f64,
    },
    VDistance {
        p1y: usize,
        p2y: usize,
        d: f64,
    },

    // ── Group 2: Nonlinear fundamentals ──────────────────────────────
    DistancePP {
        p1: PointIdx,
        p2: PointIdx,
        d: f64,
    },
    EqualLength {
        l1: LineIdx,
        l2: LineIdx,
    },
    Parallel {
        l1: LineIdx,
        l2: LineIdx,
    },
    Perpendicular {
        l1: LineIdx,
        l2: LineIdx,
    },
    Angle {
        l1: LineIdx,
        l2: LineIdx,
        value_rad: f64,
    },

    // ── Group 3: Point-on-entity ─────────────────────────────────────
    OnLine {
        point: PointIdx,
        line: LineIdx,
    },
    OnCircle {
        point: PointIdx,
        center: PointIdx,
        radius: RadiusDef,
    },

    // ── Group 4: Normalized point-line distance ──────────────────────
    DistancePL {
        point: PointIdx,
        line: LineIdx,
        d: f64,
        /// Sign of signed distance at initial config (+1 or -1).
        /// Fixed at build time so the constraint doesn't flip sides.
        sign: f64,
    },

    // ── Group 5: Tangent ─────────────────────────────────────────────
    TangentLineCircle {
        line: LineIdx,
        center: PointIdx,
        radius: RadiusDef,
        /// Sign of signed distance at initial config (+1 or -1).
        /// Fixed at build time so the constraint doesn't flip sides.
        sign: f64,
    },
    TangentArcArc {
        c1: PointIdx,
        r1: RadiusDef,
        c2: PointIdx,
        r2: RadiusDef,
        internal: bool,
    },

    // ── Group 6: Symmetric about arbitrary line ──────────────────────
    /// 2 equations: perpendicularity (Angle-type) + midpoint-on-line (Distance-type).
    SymmetricLine {
        p1: PointIdx,
        p2: PointIdx,
        line: LineIdx,
    },

    // ── Group 7: Compound ────────────────────────────────────────────
    EqualAngle {
        l1: LineIdx,
        l2: LineIdx,
        l3: LineIdx,
        l4: LineIdx,
    },
    Ratio {
        l1: LineIdx,
        l2: LineIdx,
        k: f64,
    },
    EqualPointToLine {
        p1: PointIdx,
        p2: PointIdx,
        line: LineIdx,
    },
    /// No-op in 2D. Matches existing behavior + oracle test.
    SameOrientation,
    EqualRadius {
        r1: RadiusIdx,
        r2: RadiusIdx,
    },
}

impl ConstraintEq for ConstraintImpl {
    fn num_equations(&self) -> usize {
        match self {
            Self::Coincident { .. } => 2,
            Self::Horizontal { .. } => 1,
            Self::Vertical { .. } => 1,
            Self::SymmetricH { .. } => 2,
            Self::SymmetricV { .. } => 2,
            Self::Midpoint { .. } => 2,
            Self::Dragged { .. } => 2,
            Self::Radius { .. } => 1,
            Self::Diameter { .. } => 1,
            Self::HDistance { .. } => 1,
            Self::VDistance { .. } => 1,
            Self::DistancePP { .. } => 1,
            Self::EqualLength { .. } => 1,
            Self::Parallel { .. } => 1,
            Self::Perpendicular { .. } => 1,
            Self::Angle { .. } => 1,
            Self::OnLine { .. } => 1,
            Self::OnCircle { .. } => 1,
            Self::DistancePL { .. } => 1,
            Self::TangentLineCircle { .. } => 1,
            Self::TangentArcArc { .. } => 1,
            Self::SymmetricLine { .. } => 2,
            Self::EqualAngle { .. } => 1,
            Self::Ratio { .. } => 1,
            Self::EqualPointToLine { .. } => 1,
            Self::SameOrientation => 0,
            Self::EqualRadius { .. } => 1,
        }
    }

    fn scale_types(&self) -> &[ScaleType] {
        use ScaleType::{Angle as A, Distance as D};
        match self {
            Self::Coincident { .. } => &[D, D],
            Self::Horizontal { .. } => &[D],
            Self::Vertical { .. } => &[D],
            Self::SymmetricH { .. } => &[D, D],
            Self::SymmetricV { .. } => &[D, D],
            Self::Midpoint { .. } => &[D, D],
            Self::Dragged { .. } => &[D, D],
            Self::Radius { .. } => &[D],
            Self::Diameter { .. } => &[D],
            Self::HDistance { .. } => &[D],
            Self::VDistance { .. } => &[D],
            Self::DistancePP { .. } => &[D],
            Self::EqualLength { .. } => &[D],
            Self::Parallel { .. } => &[A],
            Self::Perpendicular { .. } => &[A],
            Self::Angle { .. } => &[A],
            Self::OnLine { .. } => &[D],
            Self::OnCircle { .. } => &[D],
            Self::DistancePL { .. } => &[D],
            Self::TangentLineCircle { .. } => &[D],
            Self::TangentArcArc { .. } => &[D],
            Self::SymmetricLine { .. } => &[A, D],
            Self::EqualAngle { .. } => &[A],
            Self::Ratio { .. } => &[D],
            Self::EqualPointToLine { .. } => &[D],
            Self::SameOrientation => &[],
            Self::EqualRadius { .. } => &[D],
        }
    }

    fn residuals(&self, params: &[f64], out: &mut [f64]) {
        match self {
            // ── Group 1: Linear ──────────────────────────────────────
            Self::Coincident { p1, p2 } => {
                let a = p1.read(params);
                let b = p2.read(params);
                out[0] = a.x - b.x;
                out[1] = a.y - b.y;
            }
            Self::Horizontal { line } => {
                let d = line.delta(params);
                out[0] = d.y; // dy = 0 for horizontal
            }
            Self::Vertical { line } => {
                let d = line.delta(params);
                out[0] = d.x; // dx = 0 for vertical
            }
            // SymmetricH: symmetric about Y-axis. Opposite x, same y.
            Self::SymmetricH { p1, p2 } => {
                let a = p1.read(params);
                let b = p2.read(params);
                out[0] = a.x + b.x; // opposite x
                out[1] = a.y - b.y; // same y
            }
            // SymmetricV: symmetric about X-axis. Same x, opposite y.
            Self::SymmetricV { p1, p2 } => {
                let a = p1.read(params);
                let b = p2.read(params);
                out[0] = a.x - b.x; // same x
                out[1] = a.y + b.y; // opposite y
            }
            Self::Midpoint { point, line } => {
                let p = point.read(params);
                let s = line.start.read(params);
                let e = line.end.read(params);
                // Midpoint: p = (s + e) / 2 → p - (s+e)/2 = 0
                out[0] = p.x - (s.x + e.x) * 0.5;
                out[1] = p.y - (s.y + e.y) * 0.5;
            }
            Self::Dragged { point, target } => {
                let p = point.read(params);
                out[0] = p.x - target.x;
                out[1] = p.y - target.y;
            }
            Self::Radius { r, target } => {
                out[0] = r.read(params) - target;
            }
            Self::Diameter { r, target } => {
                // Diameter = 2 * radius
                out[0] = 2.0 * r.read(params) - target;
            }
            Self::HDistance { p1x, p2x, d } => {
                out[0] = params[*p2x] - params[*p1x] - d;
            }
            Self::VDistance { p1y, p2y, d } => {
                out[0] = params[*p2y] - params[*p1y] - d;
            }

            // ── Group 2: Nonlinear fundamentals ──────────────────────
            Self::DistancePP { p1, p2, d } => {
                let a = p1.read(params);
                let b = p2.read(params);
                let dist = nalgebra::distance(&a, &b);
                out[0] = dist - d;
            }
            Self::EqualLength { l1, l2 } => {
                out[0] = l1.length(params) - l2.length(params);
            }
            Self::Parallel { l1, l2 } => {
                let d1 = l1.delta(params);
                let d2 = l2.delta(params);
                // cross product = 0 for parallel
                out[0] = d1.x * d2.y - d1.y * d2.x;
            }
            Self::Perpendicular { l1, l2 } => {
                let d1 = l1.delta(params);
                let d2 = l2.delta(params);
                // dot product = 0 for perpendicular
                out[0] = d1.x * d2.x + d1.y * d2.y;
            }
            Self::Angle { l1, l2, value_rad } => {
                let d1 = l1.delta(params);
                let d2 = l2.delta(params);
                let cross = d1.x * d2.y - d1.y * d2.x;
                let dot = d1.x * d2.x + d1.y * d2.y;
                out[0] = cross.atan2(dot) - value_rad;
            }

            // ── Group 3: Point-on-entity ─────────────────────────────
            Self::OnLine { point, line } => {
                // Normalized cross product: cross(vp, ld) / ||ld|| = 0
                // (signed distance from point to line — zero means on the line)
                let p = point.read(params);
                let s = line.start.read(params);
                let d = line.delta(params);
                let vp = p - s;
                let cross = vp.x * d.y - vp.y * d.x;
                let line_len = d.norm().max(EPSILON);
                out[0] = cross / line_len;
            }
            Self::OnCircle {
                point,
                center,
                radius,
            } => {
                let p = point.read(params);
                let c = center.read(params);
                let dist = nalgebra::distance(&p, &c);
                let r = radius.read(params, *center);
                out[0] = dist - r;
            }

            // ── Group 4: Normalized point-line distance ──────────────
            Self::DistancePL {
                point, line, d, sign,
            } => {
                let p = point.read(params);
                let ls = line.start.read(params);
                let ld = line.delta(params);
                let vp = p - ls;
                let cross = vp.x * ld.y - vp.y * ld.x;
                let line_len = ld.norm().max(EPSILON);
                // Signed formulation: sign is fixed at build time.
                // f = sign·h - d where h = cross/D.
                out[0] = sign * cross / line_len - d;
            }

            // ── Group 5: Tangent ─────────────────────────────────────
            Self::TangentLineCircle {
                line,
                center,
                radius,
                sign,
            } => {
                // Signed formulation: sign·h - r = 0 where h = cross/D.
                // Sign is fixed at build time based on which side of the line
                // the center is on. This avoids abs/sign discontinuity.
                let c = center.read(params);
                let ls = line.start.read(params);
                let ld = line.delta(params);
                let vc = c - ls;
                let cross = vc.x * ld.y - vc.y * ld.x;
                let line_len = ld.norm().max(EPSILON);
                let r = radius.read(params, *center);
                out[0] = sign * cross / line_len - r;
            }
            Self::TangentArcArc {
                c1,
                r1,
                c2,
                r2,
                internal,
            } => {
                let center1 = c1.read(params);
                let center2 = c2.read(params);
                let dist = nalgebra::distance(&center1, &center2);
                let rv1 = r1.read(params, *c1);
                let rv2 = r2.read(params, *c2);
                if *internal {
                    out[0] = dist - (rv1 - rv2).abs();
                } else {
                    out[0] = dist - (rv1 + rv2);
                }
            }

            // ── Group 6: Symmetric about arbitrary line ──────────────
            Self::SymmetricLine { p1, p2, line } => {
                let a = p1.read(params);
                let b = p2.read(params);
                let ld = line.delta(params);
                let ls = line.start.read(params);

                // Eq 0 (Angle-type): dot(P2-P1, line_dir) = 0 (perpendicularity)
                let dp = b - a;
                out[0] = dp.x * ld.x + dp.y * ld.y;

                // Eq 1 (Distance-type): cross(line_dir, midpoint - line_start) = 0
                let mid = nalgebra::center(&a, &b);
                let vm = mid - ls;
                out[1] = ld.x * vm.y - ld.y * vm.x;
            }

            // ── Group 7: Compound ────────────────────────────────────
            Self::EqualAngle { l1, l2, l3, l4 } => {
                let d1 = l1.delta(params);
                let d2 = l2.delta(params);
                let d3 = l3.delta(params);
                let d4 = l4.delta(params);
                let cross12 = d1.x * d2.y - d1.y * d2.x;
                let dot12 = d1.x * d2.x + d1.y * d2.y;
                let cross34 = d3.x * d4.y - d3.y * d4.x;
                let dot34 = d3.x * d4.x + d3.y * d4.y;
                out[0] = cross12.atan2(dot12) - cross34.atan2(dot34);
            }
            Self::Ratio { l1, l2, k } => {
                out[0] = l1.length(params) - k * l2.length(params);
            }
            Self::EqualPointToLine { p1, p2, line } => {
                // cross(p1-p2, line_dir) = 0 (division-free equal signed distance)
                let a = p1.read(params);
                let b = p2.read(params);
                let ld = line.delta(params);
                let dp = a - b; // p1 - p2
                out[0] = dp.x * ld.y - dp.y * ld.x;
            }
            Self::SameOrientation => {
                // No-op: 0 equations, nothing to compute
            }
            Self::EqualRadius { r1, r2 } => {
                out[0] = r1.read(params) - r2.read(params);
            }
        }
    }

    fn jacobian(&self, params: &[f64], eq_offset: usize, out: &mut Vec<(usize, usize, f64)>) {
        let row = eq_offset;

        match self {
            // ── Group 1: Linear (constant Jacobian) ──────────────────
            Self::Coincident { p1, p2 } => {
                // f0 = p1.x - p2.x, f1 = p1.y - p2.y
                out.push((row, p1.x(), 1.0));
                out.push((row, p2.x(), -1.0));
                out.push((row + 1, p1.y(), 1.0));
                out.push((row + 1, p2.y(), -1.0));
            }
            Self::Horizontal { line } => {
                // f = end.y - start.y
                out.push((row, line.start.y(), -1.0));
                out.push((row, line.end.y(), 1.0));
            }
            Self::Vertical { line } => {
                // f = end.x - start.x
                out.push((row, line.start.x(), -1.0));
                out.push((row, line.end.x(), 1.0));
            }
            Self::SymmetricH { p1, p2 } => {
                // f0 = p1.x + p2.x, f1 = p1.y - p2.y
                out.push((row, p1.x(), 1.0));
                out.push((row, p2.x(), 1.0));
                out.push((row + 1, p1.y(), 1.0));
                out.push((row + 1, p2.y(), -1.0));
            }
            Self::SymmetricV { p1, p2 } => {
                // f0 = p1.x - p2.x, f1 = p1.y + p2.y
                out.push((row, p1.x(), 1.0));
                out.push((row, p2.x(), -1.0));
                out.push((row + 1, p1.y(), 1.0));
                out.push((row + 1, p2.y(), 1.0));
            }
            Self::Midpoint { point, line } => {
                // f0 = px - (sx+ex)/2, f1 = py - (sy+ey)/2
                out.push((row, point.x(), 1.0));
                out.push((row, line.start.x(), -0.5));
                out.push((row, line.end.x(), -0.5));
                out.push((row + 1, point.y(), 1.0));
                out.push((row + 1, line.start.y(), -0.5));
                out.push((row + 1, line.end.y(), -0.5));
            }
            Self::Dragged { point, .. } => {
                // f0 = px - tx, f1 = py - ty (tx, ty are constants)
                out.push((row, point.x(), 1.0));
                out.push((row + 1, point.y(), 1.0));
            }
            Self::Radius { r, .. } => {
                // f = r - target
                out.push((row, r.0, 1.0));
            }
            Self::Diameter { r, .. } => {
                // f = 2*r - target
                out.push((row, r.0, 2.0));
            }
            Self::HDistance { p1x, p2x, .. } => {
                // f = p2x - p1x - d
                out.push((row, *p1x, -1.0));
                out.push((row, *p2x, 1.0));
            }
            Self::VDistance { p1y, p2y, .. } => {
                // f = p2y - p1y - d
                out.push((row, *p1y, -1.0));
                out.push((row, *p2y, 1.0));
            }

            // ── Group 2: Nonlinear fundamentals ──────────────────────
            Self::DistancePP { p1, p2, .. } => {
                // f = ||p2 - p1|| - d
                let a = p1.read(params);
                let b = p2.read(params);
                let delta = b - a;
                let dist = delta.norm();
                // When points coincide, use arbitrary direction (1,0) to give
                // the solver a valid gradient for separating them.
                let (gx, gy) = if dist > EPSILON {
                    (delta.x / dist, delta.y / dist)
                } else {
                    (1.0, 0.0)
                };
                out.push((row, p1.x(), -gx));
                out.push((row, p1.y(), -gy));
                out.push((row, p2.x(), gx));
                out.push((row, p2.y(), gy));
            }
            Self::EqualLength { l1, l2 } => {
                // f = ||l1|| - ||l2||
                let d1 = l1.delta(params);
                let len1 = d1.norm();
                let d2 = l2.delta(params);
                let len2 = d2.norm();
                // When line is zero-length, use arbitrary direction
                let (g1x, g1y) = if len1 > EPSILON {
                    (d1.x / len1, d1.y / len1)
                } else {
                    (1.0, 0.0)
                };
                out.push((row, l1.start.x(), -g1x));
                out.push((row, l1.start.y(), -g1y));
                out.push((row, l1.end.x(), g1x));
                out.push((row, l1.end.y(), g1y));
                let (g2x, g2y) = if len2 > EPSILON {
                    (d2.x / len2, d2.y / len2)
                } else {
                    (1.0, 0.0)
                };
                out.push((row, l2.start.x(), g2x));
                out.push((row, l2.start.y(), g2y));
                out.push((row, l2.end.x(), -g2x));
                out.push((row, l2.end.y(), -g2y));
            }
            Self::Parallel { l1, l2 } => {
                // f = d1.x * d2.y - d1.y * d2.x (cross product, bilinear)
                let d1 = l1.delta(params);
                let d2 = l2.delta(params);
                // ∂f/∂(l1.start.x) = -d2.y (since ∂d1.x/∂l1.sx = -1)
                out.push((row, l1.start.x(), -d2.y));
                out.push((row, l1.start.y(), d2.x));
                out.push((row, l1.end.x(), d2.y));
                out.push((row, l1.end.y(), -d2.x));
                out.push((row, l2.start.x(), d1.y));
                out.push((row, l2.start.y(), -d1.x));
                out.push((row, l2.end.x(), -d1.y));
                out.push((row, l2.end.y(), d1.x));
            }
            Self::Perpendicular { l1, l2 } => {
                // f = d1.x * d2.x + d1.y * d2.y (dot product, bilinear)
                let d1 = l1.delta(params);
                let d2 = l2.delta(params);
                out.push((row, l1.start.x(), -d2.x));
                out.push((row, l1.start.y(), -d2.y));
                out.push((row, l1.end.x(), d2.x));
                out.push((row, l1.end.y(), d2.y));
                out.push((row, l2.start.x(), -d1.x));
                out.push((row, l2.start.y(), -d1.y));
                out.push((row, l2.end.x(), d1.x));
                out.push((row, l2.end.y(), d1.y));
            }
            Self::Angle { l1, l2, .. } => {
                // f = atan2(Y, X) - θ where Y = cross(d1,d2), X = dot(d1,d2)
                let d1 = l1.delta(params);
                let d2 = l2.delta(params);
                let cross = d1.x * d2.y - d1.y * d2.x;
                let dot = d1.x * d2.x + d1.y * d2.y;
                let denom = (cross * cross + dot * dot).max(EPSILON * EPSILON);
                // ∂f/∂v = (X*∂Y/∂v - Y*∂X/∂v) / (X²+Y²)
                // For l1.start: ∂d1/∂start = -1
                // ∂Y/∂(l1.sx) = -d2.y, ∂X/∂(l1.sx) = -d2.x
                let jl1sx = (dot * (-d2.y) - cross * (-d2.x)) / denom;
                let jl1sy = (dot * d2.x - cross * (-d2.y)) / denom;
                let jl1ex = -jl1sx; // end has opposite sign from start
                let jl1ey = -jl1sy;
                out.push((row, l1.start.x(), jl1sx));
                out.push((row, l1.start.y(), jl1sy));
                out.push((row, l1.end.x(), jl1ex));
                out.push((row, l1.end.y(), jl1ey));
                // For l2.start: ∂d2/∂start = -1
                // ∂Y/∂(l2.sx) = d1.y, ∂X/∂(l2.sx) = -d1.x
                let jl2sx = (dot * d1.y - cross * (-d1.x)) / denom;
                let jl2sy = (dot * (-d1.x) - cross * (-d1.y)) / denom;
                let jl2ex = -jl2sx;
                let jl2ey = -jl2sy;
                out.push((row, l2.start.x(), jl2sx));
                out.push((row, l2.start.y(), jl2sy));
                out.push((row, l2.end.x(), jl2ex));
                out.push((row, l2.end.y(), jl2ey));
            }

            // ── Group 3: Point-on-entity ─────────────────────────────
            Self::OnLine { point, line } => {
                // f = cross(vp, ld) / D  where D = ||ld||
                // Same quotient rule as DistancePL (signed, no absolute value)
                let ld = line.delta(params);
                let p = point.read(params);
                let s = line.start.read(params);
                let vp = p - s;
                let cross = vp.x * ld.y - vp.y * ld.x;
                let d_sq = ld.norm_squared().max(EPSILON * EPSILON);
                let d_len = d_sq.sqrt();
                let d_cubed = d_sq * d_len;

                // ∂f/∂px = dy/D, ∂f/∂py = -dx/D
                out.push((row, point.x(), ld.y / d_len));
                out.push((row, point.y(), -ld.x / d_len));

                // Line start: quotient rule
                out.push((
                    row,
                    line.start.x(),
                    (-ld.y + vp.y) / d_len + cross * ld.x / d_cubed,
                ));
                out.push((
                    row,
                    line.start.y(),
                    (ld.x - vp.x) / d_len + cross * ld.y / d_cubed,
                ));

                // Line end: quotient rule
                out.push((row, line.end.x(), -vp.y / d_len - cross * ld.x / d_cubed));
                out.push((row, line.end.y(), vp.x / d_len - cross * ld.y / d_cubed));
            }
            Self::OnCircle {
                point,
                center,
                radius,
            } => {
                // f = dist(point, center) - r
                let p = point.read(params);
                let c = center.read(params);
                let pc = p - c; // vector from center to point
                let d_pc = pc.norm();
                // When point coincides with center, use arbitrary direction (1,0)
                let (gx, gy) = if d_pc > EPSILON {
                    (pc.x / d_pc, pc.y / d_pc)
                } else {
                    (1.0, 0.0)
                };

                // ∂f/∂point = pc / d_pc
                out.push((row, point.x(), gx));
                out.push((row, point.y(), gy));

                // ∂f/∂center depends on radius type
                match radius {
                    RadiusDef::Param(r_idx) => {
                        // ∂f/∂center = -pc / d_pc
                        out.push((row, center.x(), -gx));
                        out.push((row, center.y(), -gy));
                        // ∂f/∂r = -1
                        out.push((row, r_idx.0, -1.0));
                    }
                    RadiusDef::Implicit(start) => {
                        // f = dist(point, center) - dist(center, start)
                        let s = start.read(params);
                        let cs = c - s; // center - start
                        let d_cs = cs.norm();
                        let (gs_x, gs_y) = if d_cs > EPSILON {
                            (cs.x / d_cs, cs.y / d_cs)
                        } else {
                            (0.0, 1.0) // different direction to avoid degeneracy
                        };
                        // ∂f/∂center = -pc/d_pc - cs/d_cs
                        out.push((row, center.x(), -gx - gs_x));
                        out.push((row, center.y(), -gy - gs_y));
                        // ∂f/∂start = cs/d_cs
                        out.push((row, start.x(), gs_x));
                        out.push((row, start.y(), gs_y));
                    }
                }
            }

            // ── Group 4: Normalized point-line distance ──────────────
            Self::DistancePL {
                point, line, sign, ..
            } => {
                // f = sign·h - d where h = cross/D
                // ∂f/∂x = sign · ∂h/∂x  (constant sign — no discontinuity)
                let p = point.read(params);
                let ls = line.start.read(params);
                let ld = line.delta(params);
                let vp = p - ls;
                let cross = vp.x * ld.y - vp.y * ld.x;
                let d_sq = ld.norm_squared().max(EPSILON * EPSILON);
                let d_len = d_sq.sqrt();
                let d_cubed = d_sq * d_len;

                let s = *sign;

                out.push((row, point.x(), s * ld.y / d_len));
                out.push((row, point.y(), s * -ld.x / d_len));

                out.push((
                    row,
                    line.start.x(),
                    s * ((-ld.y + vp.y) / d_len + cross * ld.x / d_cubed),
                ));
                out.push((
                    row,
                    line.start.y(),
                    s * ((ld.x - vp.x) / d_len + cross * ld.y / d_cubed),
                ));

                out.push((
                    row,
                    line.end.x(),
                    s * (-vp.y / d_len - cross * ld.x / d_cubed),
                ));
                out.push((
                    row,
                    line.end.y(),
                    s * (vp.x / d_len - cross * ld.y / d_cubed),
                ));
            }

            // ── Group 5: Tangent ─────────────────────────────────────
            Self::TangentLineCircle {
                line,
                center,
                radius,
                sign,
            } => {
                // f = sign·h - r where h = cross/D
                // ∂f/∂x = sign · ∂h/∂x  (constant sign — no discontinuity)
                let c = center.read(params);
                let ls = line.start.read(params);
                let ld = line.delta(params);
                let vc = c - ls;
                let cross = vc.x * ld.y - vc.y * ld.x;
                let d_sq = ld.norm_squared().max(EPSILON * EPSILON);
                let d_len = d_sq.sqrt();
                let d_cubed = d_sq * d_len;

                let s = *sign;

                // ∂h/∂center (scaled by sign)
                out.push((row, center.x(), s * ld.y / d_len));
                out.push((row, center.y(), s * -ld.x / d_len));

                // ∂h/∂line.start (scaled by sign)
                out.push((
                    row,
                    line.start.x(),
                    s * ((-ld.y + vc.y) / d_len + cross * ld.x / d_cubed),
                ));
                out.push((
                    row,
                    line.start.y(),
                    s * ((ld.x - vc.x) / d_len + cross * ld.y / d_cubed),
                ));

                // ∂h/∂line.end (scaled by sign)
                out.push((
                    row,
                    line.end.x(),
                    s * (-vc.y / d_len - cross * ld.x / d_cubed),
                ));
                out.push((
                    row,
                    line.end.y(),
                    s * (vc.x / d_len - cross * ld.y / d_cubed),
                ));

                // Radius derivatives: ∂f/∂r = -1 (or through implicit chain)
                match radius {
                    RadiusDef::Param(r_idx) => {
                        out.push((row, r_idx.0, -1.0));
                    }
                    RadiusDef::Implicit(start) => {
                        let sv = start.read(params);
                        let cv = center.read(params) - sv;
                        let d_cs = cv.norm().max(EPSILON);
                        let gr_cx = cv.x / d_cs;
                        let gr_cy = cv.y / d_cs;
                        out.push((row, center.x(), -gr_cx));
                        out.push((row, center.y(), -gr_cy));
                        out.push((row, start.x(), gr_cx));
                        out.push((row, start.y(), gr_cy));
                    }
                }
            }
            Self::TangentArcArc {
                c1,
                r1,
                c2,
                r2,
                internal,
            } => {
                // f = dist(c1, c2) - (r1 ± r2)
                // External: f = dist - (r1 + r2)
                // Internal: f = dist - |r1 - r2|
                let center1 = c1.read(params);
                let center2 = c2.read(params);
                let delta = center2 - center1;
                let dist = delta.norm();
                let (gx, gy) = if dist > EPSILON {
                    (delta.x / dist, delta.y / dist)
                } else {
                    (1.0, 0.0)
                };

                // ∂(dist)/∂c1 = -delta/dist, ∂(dist)/∂c2 = delta/dist
                out.push((row, c1.x(), -gx));
                out.push((row, c1.y(), -gy));
                out.push((row, c2.x(), gx));
                out.push((row, c2.y(), gy));

                // Radius derivatives
                let rv1 = r1.read(params, *c1);
                let rv2 = r2.read(params, *c2);

                if *internal {
                    // f = dist - |r1 - r2|
                    // ∂f/∂r1 = -sign(r1-r2), ∂f/∂r2 = sign(r1-r2)
                    let sign = if rv1 >= rv2 { 1.0 } else { -1.0 };
                    push_radius_grad(out, row, r1, *c1, params, -sign);
                    push_radius_grad(out, row, r2, *c2, params, sign);
                } else {
                    // f = dist - r1 - r2
                    // ∂f/∂r1 = -1, ∂f/∂r2 = -1
                    push_radius_grad(out, row, r1, *c1, params, -1.0);
                    push_radius_grad(out, row, r2, *c2, params, -1.0);
                }
            }

            // ── Group 6: Symmetric about arbitrary line ──────────────
            Self::SymmetricLine { p1, p2, line } => {
                let a = p1.read(params);
                let b = p2.read(params);
                let ld = line.delta(params);
                let ls = line.start.read(params);
                let dp = b - a; // p2 - p1
                let mid = nalgebra::center(&a, &b);
                let vm = mid - ls;

                // Eq 0: f0 = dot(p2-p1, line_dir) = dp.x*dx + dp.y*dy
                // ∂f0/∂p1.x = -dx, ∂f0/∂p1.y = -dy
                out.push((row, p1.x(), -ld.x));
                out.push((row, p1.y(), -ld.y));
                // ∂f0/∂p2.x = dx, ∂f0/∂p2.y = dy
                out.push((row, p2.x(), ld.x));
                out.push((row, p2.y(), ld.y));
                // ∂f0/∂ls.x = -(p2x-p1x) = -dp.x (∂dx/∂lsx = -1)
                out.push((row, line.start.x(), -dp.x));
                out.push((row, line.start.y(), -dp.y));
                out.push((row, line.end.x(), dp.x));
                out.push((row, line.end.y(), dp.y));

                // Eq 1: f1 = dx*(mid.y-lsy) - dy*(mid.x-lsx) = dx*vm.y - dy*vm.x
                let r1 = row + 1;
                // ∂f1/∂p1.x = -dy*0.5 (∂mid.x/∂p1.x = 0.5)
                out.push((r1, p1.x(), -ld.y * 0.5));
                out.push((r1, p1.y(), ld.x * 0.5));
                out.push((r1, p2.x(), -ld.y * 0.5));
                out.push((r1, p2.y(), ld.x * 0.5));
                // ∂f1/∂ls: ∂dx/∂lsx = -1, ∂lsx/∂lsx = 1
                // f1 = dx*vm.y - dy*vm.x where vm = mid - ls
                // ∂f1/∂lsx = (-1)*vm.y + dx*0 - 0*vm.x - dy*(-1) = -vm.y + ld.y
                out.push((r1, line.start.x(), -vm.y + ld.y));
                // ∂f1/∂lsy = 0 + dx*(-1) - (-1)*vm.x - 0 = -ld.x + vm.x
                out.push((r1, line.start.y(), -ld.x + vm.x));
                // ∂f1/∂lex = 1*vm.y = vm.y
                out.push((r1, line.end.x(), vm.y));
                // ∂f1/∂ley = -1*vm.x = ... wait
                // f1 = dx*vm.y - dy*vm.x, ∂dx/∂ley = 0, ∂dy/∂ley = 1
                // ∂f1/∂ley = 0 - vm.x = -vm.x
                out.push((r1, line.end.y(), -vm.x));
            }

            // ── Group 7: Compound ────────────────────────────────────
            Self::EqualAngle { l1, l2, l3, l4 } => {
                // f = atan2(Y12, X12) - atan2(Y34, X34)
                // Jacobian for l1/l2 is +Angle jacobian, for l3/l4 is -Angle jacobian
                let d1 = l1.delta(params);
                let d2 = l2.delta(params);
                let d3 = l3.delta(params);
                let d4 = l4.delta(params);

                // First pair (l1, l2) — positive contribution
                let cross12 = d1.x * d2.y - d1.y * d2.x;
                let dot12 = d1.x * d2.x + d1.y * d2.y;
                let denom12 = (cross12 * cross12 + dot12 * dot12).max(EPSILON * EPSILON);
                push_angle_jac(out, row, l1, l2, &d1, &d2, cross12, dot12, denom12, 1.0);

                // Second pair (l3, l4) — negative contribution
                let cross34 = d3.x * d4.y - d3.y * d4.x;
                let dot34 = d3.x * d4.x + d3.y * d4.y;
                let denom34 = (cross34 * cross34 + dot34 * dot34).max(EPSILON * EPSILON);
                push_angle_jac(out, row, l3, l4, &d3, &d4, cross34, dot34, denom34, -1.0);
            }
            Self::Ratio { l1, l2, k } => {
                // f = ||l1|| - k * ||l2||
                let d1 = l1.delta(params);
                let len1 = d1.norm();
                let d2 = l2.delta(params);
                let len2 = d2.norm();

                let (g1x, g1y) = if len1 > EPSILON {
                    (d1.x / len1, d1.y / len1)
                } else {
                    (1.0, 0.0)
                };
                out.push((row, l1.start.x(), -g1x));
                out.push((row, l1.start.y(), -g1y));
                out.push((row, l1.end.x(), g1x));
                out.push((row, l1.end.y(), g1y));

                let (g2ux, g2uy) = if len2 > EPSILON {
                    (d2.x / len2, d2.y / len2)
                } else {
                    (1.0, 0.0)
                };
                let g2x = k * g2ux;
                let g2y = k * g2uy;
                out.push((row, l2.start.x(), g2x));
                out.push((row, l2.start.y(), g2y));
                out.push((row, l2.end.x(), -g2x));
                out.push((row, l2.end.y(), -g2y));
            }
            Self::EqualPointToLine { p1, p2, line } => {
                // f = (p1x-p2x)*dy - (p1y-p2y)*dx (bilinear, division-free)
                let a = p1.read(params);
                let b = p2.read(params);
                let ld = line.delta(params);
                let dpx = a.x - b.x;
                let dpy = a.y - b.y;

                out.push((row, p1.x(), ld.y));
                out.push((row, p1.y(), -ld.x));
                out.push((row, p2.x(), -ld.y));
                out.push((row, p2.y(), ld.x));
                // ∂f/∂lsx = dpy (∂dx/∂lsx = -1 → -(-dpy... wait)
                // f = dpx*dy - dpy*dx
                // ∂f/∂lsx = dpx*0 - dpy*(-1) = dpy
                out.push((row, line.start.x(), dpy));
                // ∂f/∂lsy = dpx*(-1) - 0 = -dpx
                out.push((row, line.start.y(), -dpx));
                // ∂f/∂lex = -dpy
                out.push((row, line.end.x(), -dpy));
                // ∂f/∂ley = dpx
                out.push((row, line.end.y(), dpx));
            }
            Self::SameOrientation => {
                // No equations, no Jacobian entries
            }
            Self::EqualRadius { r1, r2 } => {
                // f = r1 - r2
                out.push((row, r1.0, 1.0));
                out.push((row, r2.0, -1.0));
            }
        }
    }

    fn hessian(&self, params: &[f64], eq_offset: usize) -> Vec<(usize, usize, usize, f64)> {
        let row = eq_offset;
        let mut out = Vec::new();

        match self {
            // ── Group 1: Linear constraints have zero Hessian ──────────
            Self::Coincident { .. }
            | Self::Horizontal { .. }
            | Self::Vertical { .. }
            | Self::SymmetricH { .. }
            | Self::SymmetricV { .. }
            | Self::Midpoint { .. }
            | Self::Dragged { .. }
            | Self::Radius { .. }
            | Self::Diameter { .. }
            | Self::HDistance { .. }
            | Self::VDistance { .. }
            | Self::SameOrientation
            | Self::EqualRadius { .. } => {}

            // ── DistancePP: f = ||Δ|| - d ──────────────────────────────
            // H(||Δ||) = (I - n̂n̂ᵀ) / ||Δ||  where n̂ = Δ/||Δ||
            Self::DistancePP { p1, p2, .. } => {
                push_norm_hessian(&mut out, row, params, &[p1.x(), p1.y()], &[p2.x(), p2.y()]);
            }

            // ── EqualLength: f = ||l1|| - ||l2|| ───────────────────────
            Self::EqualLength { l1, l2 } => {
                // +H(||l1||)
                push_norm_hessian(
                    &mut out, row, params,
                    &[l1.start.x(), l1.start.y()],
                    &[l1.end.x(), l1.end.y()],
                );
                // -H(||l2||)
                push_norm_hessian_scaled(
                    &mut out, row, params,
                    &[l2.start.x(), l2.start.y()],
                    &[l2.end.x(), l2.end.y()],
                    -1.0,
                );
            }

            // ── OnCircle (Param): f = ||p-c|| - r ──────────────────────
            Self::OnCircle { point, center, radius: RadiusDef::Param(_) } => {
                push_norm_hessian(&mut out, row, params, &[point.x(), point.y()], &[center.x(), center.y()]);
            }

            // ── OnCircle (Implicit): f = ||p-c|| - ||c-s|| ─────────────
            Self::OnCircle { point, center, radius: RadiusDef::Implicit(start) } => {
                // +H(||p-c||) with respect to p and c
                push_norm_hessian_6(
                    &mut out, row, params,
                    point.x(), point.y(),
                    center.x(), center.y(),
                    1.0, true,
                );
                // -H(||c-s||) with respect to c and s
                push_norm_hessian_6(
                    &mut out, row, params,
                    center.x(), center.y(),
                    start.x(), start.y(),
                    -1.0, true,
                );
            }

            // ── TangentArcArc: f = ||c1-c2|| - (r1 ± r2) ──────────────
            Self::TangentArcArc { c1, c2, r1, r2, internal } => {
                // H(||c2-c1||) for center distance part
                push_norm_hessian(&mut out, row, params, &[c1.x(), c1.y()], &[c2.x(), c2.y()]);
                // Radius parts: only implicit radii contribute Hessians
                let sign = if *internal {
                    let rv1 = r1.read(params, *c1);
                    let rv2 = r2.read(params, *c2);
                    if rv1 >= rv2 { -1.0 } else { 1.0 }
                } else {
                    -1.0
                };
                push_radius_hessian(&mut out, row, params, r1, *c1, sign);
                if *internal {
                    push_radius_hessian(&mut out, row, params, r2, *c2, -sign);
                } else {
                    push_radius_hessian(&mut out, row, params, r2, *c2, -1.0);
                }
            }

            // ── Ratio: f = ||l1|| - k·||l2|| ───────────────────────────
            Self::Ratio { l1, l2, k } => {
                push_norm_hessian(
                    &mut out, row, params,
                    &[l1.start.x(), l1.start.y()],
                    &[l1.end.x(), l1.end.y()],
                );
                push_norm_hessian_scaled(
                    &mut out, row, params,
                    &[l2.start.x(), l2.start.y()],
                    &[l2.end.x(), l2.end.y()],
                    -k,
                );
            }

            // ── Bilinear constraints: constant mixed Hessians ──────────
            Self::Parallel { l1, l2 } => {
                // f = d1x·d2y - d1y·d2x (bilinear in d1 and d2)
                // Mixed partials are constant ±1.
                push_bilinear_cross_hessian(&mut out, row, l1, l2);
            }
            Self::Perpendicular { l1, l2 } => {
                // f = d1x·d2x + d1y·d2y (bilinear)
                push_bilinear_dot_hessian(&mut out, row, l1, l2);
            }
            Self::EqualPointToLine { p1, p2, line } => {
                // f = (p1-p2) × line_delta = dpx·dy - dpy·dx (bilinear)
                let dp_line = LineIdx { start: *p2, end: *p1 };
                push_bilinear_cross_hessian(&mut out, row, &dp_line, line);
            }

            // ── Complex constraints (atan2, quotient) — lower priority ─
            // These are less prone to LICQ failure. Return empty for now;
            // the Hessian analysis still helps for the norm-based constraints.
            Self::Angle { .. }
            | Self::OnLine { .. }
            | Self::DistancePL { .. }
            | Self::TangentLineCircle { .. }
            | Self::SymmetricLine { .. }
            | Self::EqualAngle { .. } => {}
        }

        out
    }
}

// ── Jacobian helpers ─────────────────────────────────────────────────────

/// Push Jacobian entries for ∂r/∂params, scaled by `scale`.
/// Handles both Param (trivial) and Implicit (dist(center, start)) cases.
fn push_radius_grad(
    out: &mut Vec<(usize, usize, f64)>,
    row: usize,
    radius: &RadiusDef,
    center: PointIdx,
    params: &[f64],
    scale: f64,
) {
    match radius {
        RadiusDef::Param(r_idx) => {
            out.push((row, r_idx.0, scale));
        }
        RadiusDef::Implicit(start) => {
            let c = center.read(params);
            let s = start.read(params);
            let cs = c - s; // center - start
            let d = cs.norm().max(EPSILON);
            // ∂r/∂center = (center - start) / d
            out.push((row, center.x(), scale * cs.x / d));
            out.push((row, center.y(), scale * cs.y / d));
            // ∂r/∂start = -(center - start) / d = (start - center) / d
            out.push((row, start.x(), -scale * cs.x / d));
            out.push((row, start.y(), -scale * cs.y / d));
        }
    }
}

/// Push Jacobian entries for the Angle constraint's l1/l2 pair, with a sign multiplier.
#[allow(clippy::too_many_arguments)]
fn push_angle_jac(
    out: &mut Vec<(usize, usize, f64)>,
    row: usize,
    la: &LineIdx,
    lb: &LineIdx,
    da: &nalgebra::Vector2<f64>,
    db: &nalgebra::Vector2<f64>,
    cross: f64,
    dot: f64,
    denom: f64,
    sign: f64,
) {
    // ∂atan2(Y,X)/∂v = (X*∂Y/∂v - Y*∂X/∂v) / (X²+Y²)
    // For la.start: ∂da/∂start = -1
    let jla_sx = sign * (dot * (-db.y) - cross * (-db.x)) / denom;
    let jla_sy = sign * (dot * db.x - cross * (-db.y)) / denom;
    out.push((row, la.start.x(), jla_sx));
    out.push((row, la.start.y(), jla_sy));
    out.push((row, la.end.x(), -jla_sx));
    out.push((row, la.end.y(), -jla_sy));

    // For lb.start: ∂db/∂start = -1
    let jlb_sx = sign * (dot * da.y - cross * (-da.x)) / denom;
    let jlb_sy = sign * (dot * (-da.x) - cross * (-da.y)) / denom;
    out.push((row, lb.start.x(), jlb_sx));
    out.push((row, lb.start.y(), jlb_sy));
    out.push((row, lb.end.x(), -jlb_sx));
    out.push((row, lb.end.y(), -jlb_sy));
}

// ── Hessian helpers ─────────────────────────────────────────────────────

/// Push Hessian entries for ∂²||Δ||/∂x_i∂x_j where Δ = p2 - p1.
///
/// H(||Δ||) = (I - n̂n̂ᵀ) / ||Δ|| where n̂ = Δ/||Δ||.
/// The sign convention: p1 indices get -1 derivative, p2 indices get +1.
fn push_norm_hessian(
    out: &mut Vec<(usize, usize, usize, f64)>,
    row: usize,
    params: &[f64],
    p1_cols: &[usize; 2],  // [x, y] column indices for point 1
    p2_cols: &[usize; 2],  // [x, y] column indices for point 2
) {
    push_norm_hessian_scaled(out, row, params, p1_cols, p2_cols, 1.0);
}

/// Push Hessian entries for scale * ∂²||Δ||/∂x_i∂x_j.
fn push_norm_hessian_scaled(
    out: &mut Vec<(usize, usize, usize, f64)>,
    row: usize,
    params: &[f64],
    p1_cols: &[usize; 2],
    p2_cols: &[usize; 2],
    scale: f64,
) {
    let dx = params[p2_cols[0]] - params[p1_cols[0]];
    let dy = params[p2_cols[1]] - params[p1_cols[1]];
    let dist = (dx * dx + dy * dy).sqrt().max(EPSILON);
    let inv_d = 1.0 / dist;

    // n̂ = (dx, dy) / dist
    let nx = dx * inv_d;
    let ny = dy * inv_d;

    // H_ab = scale * (δ_ab - n_a * n_b) / dist
    let hxx = scale * (1.0 - nx * nx) * inv_d;
    let hyy = scale * (1.0 - ny * ny) * inv_d;
    let hxy = scale * (-nx * ny) * inv_d;

    // All param columns with their signs: p1 has -1, p2 has +1
    let cols = [
        (p1_cols[0], -1.0_f64), // p1.x
        (p1_cols[1], -1.0_f64), // p1.y
        (p2_cols[0], 1.0_f64),  // p2.x
        (p2_cols[1], 1.0_f64),  // p2.y
    ];
    // h_block[a][b] for the 2D Hessian
    let h_block = [[hxx, hxy], [hxy, hyy]];

    for (i, &(ci, si)) in cols.iter().enumerate() {
        let a = i % 2; // 0=x, 1=y
        for (j, &(cj, sj)) in cols.iter().enumerate() {
            if j < i {
                continue; // emit upper triangle only (symmetric)
            }
            let b = j % 2;
            let val = si * sj * h_block[a][b];
            if val.abs() > 1e-18 {
                out.push((row, ci, cj, val));
                if ci != cj {
                    out.push((row, cj, ci, val)); // symmetric entry
                }
            }
        }
    }
}

/// Push Hessian for ||a - b|| with 6 params (a_x, a_y, b_x, b_y), scaled.
/// `positive_first`: if true, f = +||a-b||, derivative signs: a gets +1, b gets -1.
#[allow(clippy::too_many_arguments)]
fn push_norm_hessian_6(
    out: &mut Vec<(usize, usize, usize, f64)>,
    row: usize,
    params: &[f64],
    ax: usize, ay: usize,
    bx: usize, by: usize,
    scale: f64,
    _positive_first: bool,
) {
    push_norm_hessian_scaled(out, row, params, &[bx, by], &[ax, ay], scale);
}

/// Push Hessian for implicit radius term: ∂²||c-s||/∂params.
fn push_radius_hessian(
    out: &mut Vec<(usize, usize, usize, f64)>,
    row: usize,
    params: &[f64],
    radius: &RadiusDef,
    center: PointIdx,
    scale: f64,
) {
    match radius {
        RadiusDef::Param(_) => {
            // Explicit radius is linear — zero Hessian
        }
        RadiusDef::Implicit(start) => {
            // r = ||center - start||, same norm Hessian
            push_norm_hessian_scaled(
                out, row, params,
                &[start.x(), start.y()],
                &[center.x(), center.y()],
                scale,
            );
        }
    }
}

/// Push Hessian for bilinear cross product: f = d1×d2 = d1x·d2y - d1y·d2x.
/// All second derivatives are constant ±1 mixed partials.
fn push_bilinear_cross_hessian(
    out: &mut Vec<(usize, usize, usize, f64)>,
    row: usize,
    l1: &LineIdx,
    l2: &LineIdx,
) {
    // f = (e1x-s1x)(e2y-s2y) - (e1y-s1y)(e2x-s2x)
    // ∂²f/∂(d1x)∂(d2y) = +1, ∂²f/∂(d1y)∂(d2x) = -1
    // d1x params: s1x with sign -1, e1x with sign +1
    // d2y params: s2y with sign -1, e2y with sign +1
    // d1y params: s1y with sign -1, e1y with sign +1
    // d2x params: s2x with sign -1, e2x with sign +1

    let d1x_params: [(usize, f64); 2] = [(l1.start.x(), -1.0), (l1.end.x(), 1.0)];
    let d1y_params: [(usize, f64); 2] = [(l1.start.y(), -1.0), (l1.end.y(), 1.0)];
    let d2x_params: [(usize, f64); 2] = [(l2.start.x(), -1.0), (l2.end.x(), 1.0)];
    let d2y_params: [(usize, f64); 2] = [(l2.start.y(), -1.0), (l2.end.y(), 1.0)];

    // ∂²f/∂(d1x_param)∂(d2y_param) = sign1 * sign2 * (+1)
    for &(c1, s1) in &d1x_params {
        for &(c2, s2) in &d2y_params {
            let val = s1 * s2;
            out.push((row, c1, c2, val));
            if c1 != c2 {
                out.push((row, c2, c1, val));
            }
        }
    }

    // ∂²f/∂(d1y_param)∂(d2x_param) = sign1 * sign2 * (-1)
    for &(c1, s1) in &d1y_params {
        for &(c2, s2) in &d2x_params {
            let val = -s1 * s2;
            out.push((row, c1, c2, val));
            if c1 != c2 {
                out.push((row, c2, c1, val));
            }
        }
    }
}

/// Push Hessian for bilinear dot product: f = d1·d2 = d1x·d2x + d1y·d2y.
fn push_bilinear_dot_hessian(
    out: &mut Vec<(usize, usize, usize, f64)>,
    row: usize,
    l1: &LineIdx,
    l2: &LineIdx,
) {
    let d1x_params: [(usize, f64); 2] = [(l1.start.x(), -1.0), (l1.end.x(), 1.0)];
    let d1y_params: [(usize, f64); 2] = [(l1.start.y(), -1.0), (l1.end.y(), 1.0)];
    let d2x_params: [(usize, f64); 2] = [(l2.start.x(), -1.0), (l2.end.x(), 1.0)];
    let d2y_params: [(usize, f64); 2] = [(l2.start.y(), -1.0), (l2.end.y(), 1.0)];

    // ∂²f/∂(d1x_param)∂(d2x_param) = sign1 * sign2 * (+1)
    for &(c1, s1) in &d1x_params {
        for &(c2, s2) in &d2x_params {
            let val = s1 * s2;
            out.push((row, c1, c2, val));
            if c1 != c2 {
                out.push((row, c2, c1, val));
            }
        }
    }

    // ∂²f/∂(d1y_param)∂(d2y_param) = sign1 * sign2 * (+1)
    for &(c1, s1) in &d1y_params {
        for &(c2, s2) in &d2y_params {
            let val = s1 * s2;
            out.push((row, c1, c2, val));
            if c1 != c2 {
                out.push((row, c2, c1, val));
            }
        }
    }
}
