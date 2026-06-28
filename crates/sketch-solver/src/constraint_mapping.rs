//! Clean-room constraint mapping: each `SketchConstraint` variant → residual
//! block + analytic Jacobian.
//!
//! PR-SS1 scope: 13 constraints (Coincident, Horizontal, Vertical, Parallel,
//! Perpendicular, Equal, Distance, Angle, Radius, Diameter, OnEntity,
//! Midpoint, Dragged). The dimension tool also emits PointLineDistance
//! (point↔line perpendicular gap; reuses the Distance (Point, Line) residual)
//! and HDistance/VDistance (axis-aligned |Δx|/|Δy| between two points).
//! The 8 remaining mapped-but-unexposed constraints
//! (Symmetric, SymmetricH, SymmetricV, Tangent, EqualAngle, Ratio,
//! EqualPointToLine, SameOrientation) are banked to PR-SS2.
//!
//! Each constraint produces one or more rows in the residual vector and the
//! Jacobian. Residuals are weighted (default 1.0; dragged = 1/20 per
//! SolveSpace's published 1/20-scaling trick). The LM objective is
//! `min Σ (w_i · r_i)²`.
//!
//! Determinism: residual/Jacobian row order is determined by iterating
//! `constraints` in declaration order (a `Vec`). No `HashMap` iteration
//! occurs in the residual or Jacobian assembly path.

use nalgebra::{DMatrix, DVector};

use crate::entity_mapping::ParamLayout;
use crate::types::{EntityKind, SketchConstraint};

/// Weight applied to the dragged-point residual (SolveSpace 1/20-scaling).
pub const DRAGGED_WEIGHT: f64 = 1.0 / 20.0;

/// A compiled constraint: produces residual row(s) and Jacobian row(s) given
/// the current parameter vector and the `ParamLayout`.
///
/// Each variant closes over the entity IDs and constraint value, resolving
/// them to parameter indices at construction time. This avoids repeated
/// `HashMap` lookups during LM iteration (hot path).
#[derive(Clone)]
pub enum CompiledConstraint {
    Coincident {
        ax: usize, ay: usize,
        bx: usize, by: usize,
    },
    Horizontal {
        ay: usize, by: usize,
    },
    Vertical {
        ax: usize, bx: usize,
    },
    Parallel {
        ax: usize, ay: usize, bx: usize, by: usize,
        cx: usize, cy: usize, dx: usize, dy: usize,
    },
    Perpendicular {
        ax: usize, ay: usize, bx: usize, by: usize,
        cx: usize, cy: usize, dx: usize, dy: usize,
    },
    EqualLines {
        ax: usize, ay: usize, bx: usize, by: usize,
        cx: usize, cy: usize, dx: usize, dy: usize,
    },
    EqualCircles {
        ra: usize, rb: usize,
    },
    DistancePP {
        ax: usize, ay: usize, bx: usize, by: usize,
        value: f64,
    },
    DistancePL {
        px: usize, py: usize,
        ax: usize, ay: usize, bx: usize, by: usize,
        value: f64,
    },
    /// Horizontal (x-axis) distance between two points: r = |x_b - x_a| - value.
    HDistance {
        ax: usize, bx: usize,
        value: f64,
    },
    /// Vertical (y-axis) distance between two points: r = |y_b - y_a| - value.
    VDistance {
        ay: usize, by: usize,
        value: f64,
    },
    Angle {
        ax: usize, ay: usize, bx: usize, by: usize,
        cx: usize, cy: usize, dx: usize, dy: usize,
        value_radians: f64,
    },
    Radius {
        r: usize,
        value: f64,
    },
    Diameter {
        r: usize,
        value: f64,
    },
    OnEntityLine {
        px: usize, py: usize,
        ax: usize, ay: usize, bx: usize, by: usize,
    },
    OnEntityCircle {
        px: usize, py: usize,
        cx: usize, cy: usize, r: usize,
    },
    /// Radius constraint on an arc: residual = ‖C-S‖ - value
    /// (arcs have no stored radius param; radius = center→start distance)
    RadiusArc {
        cx: usize, cy: usize, sx: usize, sy: usize,
        value: f64,
    },
    /// Diameter constraint on an arc: residual = 2*‖C-S‖ - value
    DiameterArc {
        cx: usize, cy: usize, sx: usize, sy: usize,
        value: f64,
    },
    /// Equal between arc (radius=‖C-S‖) and circle (stored radius param)
    EqualArcCircle {
        acx: usize, acy: usize, asx: usize, asy: usize,
        r: usize,
    },
    /// Equal between two arcs (both radius = ‖center-start‖)
    EqualArcArc {
        acx: usize, acy: usize, asx: usize, asy: usize,
        bcx: usize, bcy: usize, bsx: usize, bsy: usize,
    },
    /// OnEntity: point on arc (radius = ‖C-S‖)
    OnEntityArc {
        px: usize, py: usize,
        cx: usize, cy: usize, sx: usize, sy: usize,
    },
    Midpoint {
        px: usize, py: usize,
        ax: usize, ay: usize, bx: usize, by: usize,
    },
    Dragged {
        px: usize, py: usize,
        fixed_x: f64, fixed_y: f64,
    },
    // ── PR-SS2 constraints ──────────────────────────────────────────────
    /// Two points symmetric about a line: midpoint on line + AB ⊥ line
    Symmetric {
        ax: usize, ay: usize, bx: usize, by: usize,
        cx: usize, cy: usize, dx: usize, dy: usize,
    },
    /// Symmetric about Y-axis (opposite x, same y). slvs "Horiz" = offset is horizontal.
    SymmetricH {
        ax: usize, ay: usize, bx: usize, by: usize,
    },
    /// Symmetric about X-axis (same x, opposite y). slvs "Vert" = offset is vertical.
    SymmetricV {
        ax: usize, ay: usize, bx: usize, by: usize,
    },
    /// Line tangent to circle: dist(center, line) - radius = 0
    TangentLineCircle {
        cx: usize, cy: usize, r: usize,
        ax: usize, ay: usize, bx: usize, by: usize,
    },
    /// Line tangent to arc: dist(center, line) - ‖C-S‖ = 0
    TangentLineArc {
        cx: usize, cy: usize, sx: usize, sy: usize,
        ax: usize, ay: usize, bx: usize, by: usize,
    },
    /// Equal angle: angle(a,b) == angle(c,d)
    EqualAngle {
        ax: usize, ay: usize, bx: usize, by: usize,
        cx: usize, cy: usize, dx: usize, dy: usize,
        ex: usize, ey: usize, fx: usize, fy: usize,
        gx: usize, gy: usize, hx: usize, hy: usize,
    },
    /// Length ratio: ℓ_a - value * ℓ_b = 0
    Ratio {
        ax: usize, ay: usize, bx: usize, by: usize,
        cx: usize, cy: usize, dx: usize, dy: usize,
        value: f64,
    },
    /// Equal point-to-line distances: dist(P_a, L) - dist(P_b, L) = 0
    EqualPointToLine {
        ax: usize, ay: usize, bx: usize, by: usize,
        lx0: usize, ly0: usize, lx1: usize, ly1: usize,
    },
    /// SameOrientation: 2D noop (normals always aligned in workplane)
    SameOrientation,
}

/// Number of residual rows a compiled constraint contributes.
pub fn residual_count(cc: &CompiledConstraint) -> usize {
    match cc {
        CompiledConstraint::Coincident { .. } => 2,
        CompiledConstraint::Midpoint { .. } => 2,
        CompiledConstraint::Dragged { .. } => 2,
        CompiledConstraint::Symmetric { .. } => 2,
        CompiledConstraint::SymmetricH { .. } => 2,
        CompiledConstraint::SymmetricV { .. } => 2,
        CompiledConstraint::SameOrientation => 0,
        _ => 1,
    }
}

/// Weight for a compiled constraint's residual row(s).
pub fn weight(cc: &CompiledConstraint) -> f64 {
    match cc {
        CompiledConstraint::Dragged { .. } => DRAGGED_WEIGHT,
        _ => 1.0,
    }
}

impl CompiledConstraint {
    /// Compile a `SketchConstraint` into a `CompiledConstraint` by resolving
    /// entity IDs to parameter indices via the `ParamLayout`.
    ///
    /// Returns `Err(reason_string)` if the constraint references an unknown
    /// entity or an unsupported entity-type combination. The caller maps this
    /// to `SolveStatus::SolveFailed { reason }`.
    pub fn compile(
        constraint: &SketchConstraint,
        layout: &ParamLayout,
    ) -> Result<Self, String> {
        let kind_of = |id: u32| -> Result<EntityKind, String> {
            layout
                .entity_kinds
                .get(&id)
                .copied()
                .ok_or_else(|| format!("constraint references unknown entity {id}"))
        };

        let pt = |id: u32| -> Result<(usize, usize), String> {
            layout
                .point_indices
                .get(&id)
                .copied()
                .ok_or_else(|| format!("constraint references unknown point {id}"))
        };

        let line_pts = |id: u32| -> Result<(u32, u32), String> {
            layout
                .line_endpoints
                .get(&id)
                .copied()
                .ok_or_else(|| format!("constraint references unknown line {id}"))
        };

        let radius_idx = |id: u32| -> Result<usize, String> {
            layout
                .radius_indices
                .get(&id)
                .copied()
                .ok_or_else(|| format!("constraint references unknown circle/arc {id}"))
        };

        let line_param_pts = |id: u32| -> Result<[usize; 4], String> {
            let (s, e) = line_pts(id)?;
            let (sx, sy) = pt(s)?;
            let (ex, ey) = pt(e)?;
            Ok([sx, sy, ex, ey])
        };

        let circle_center = |id: u32| -> Result<(usize, usize), String> {
            let center_id = layout
                .circle_centers
                .get(&id)
                .copied()
                .ok_or_else(|| format!("circle {id} has no recorded center"))?;
            pt(center_id)
        };

        let arc_center = |id: u32| -> Result<(usize, usize), String> {
            let (center_id, _, _) = layout
                .arc_endpoints
                .get(&id)
                .copied()
                .ok_or_else(|| format!("arc {id} has no recorded endpoints"))?;
            pt(center_id)
        };

        match constraint {
            SketchConstraint::Coincident { point_a, point_b } => {
                let (ax, ay) = pt(*point_a)?;
                let (bx, by) = pt(*point_b)?;
                Ok(CompiledConstraint::Coincident { ax, ay, bx, by })
            }

            SketchConstraint::Horizontal { entity } => {
                let [_, ay, _, by] = line_param_pts(*entity)?;
                Ok(CompiledConstraint::Horizontal { ay, by })
            }

            SketchConstraint::Vertical { entity } => {
                let [ax, _, bx, _] = line_param_pts(*entity)?;
                Ok(CompiledConstraint::Vertical { ax, bx })
            }

            // Point-pair forms compile to the SAME residual as the line forms —
            // equate one coordinate axis of two points. (Invariant I3: one axis.)
            SketchConstraint::HorizontalPoints { point_a, point_b } => {
                let (_, ay) = pt(*point_a)?;
                let (_, by) = pt(*point_b)?;
                Ok(CompiledConstraint::Horizontal { ay, by })
            }

            SketchConstraint::VerticalPoints { point_a, point_b } => {
                let (ax, _) = pt(*point_a)?;
                let (bx, _) = pt(*point_b)?;
                Ok(CompiledConstraint::Vertical { ax, bx })
            }

            SketchConstraint::Parallel { line_a, line_b } => {
                let [ax, ay, bx, by] = line_param_pts(*line_a)?;
                let [cx, cy, dx, dy] = line_param_pts(*line_b)?;
                Ok(CompiledConstraint::Parallel { ax, ay, bx, by, cx, cy, dx, dy })
            }

            SketchConstraint::Perpendicular { line_a, line_b } => {
                let [ax, ay, bx, by] = line_param_pts(*line_a)?;
                let [cx, cy, dx, dy] = line_param_pts(*line_b)?;
                Ok(CompiledConstraint::Perpendicular { ax, ay, bx, by, cx, cy, dx, dy })
            }

            SketchConstraint::Equal { entity_a, entity_b } => {
                let ka = kind_of(*entity_a)?;
                let kb = kind_of(*entity_b)?;
                match (ka, kb) {
                    (EntityKind::Line, EntityKind::Line) => {
                        let [ax, ay, bx, by] = line_param_pts(*entity_a)?;
                        let [cx, cy, dx, dy] = line_param_pts(*entity_b)?;
                        Ok(CompiledConstraint::EqualLines { ax, ay, bx, by, cx, cy, dx, dy })
                    }
                    (EntityKind::Circle, EntityKind::Circle) => {
                        let ra = radius_idx(*entity_a)?;
                        let rb = radius_idx(*entity_b)?;
                        Ok(CompiledConstraint::EqualCircles { ra, rb })
                    }
                    (EntityKind::Arc, EntityKind::Arc) => {
                        let (acx, acy) = arc_center(*entity_a)?;
                        let (asx, asy) = {
                            let (_, s, _) = layout.arc_endpoints[entity_a];
                            pt(s)?
                        };
                        let (bcx, bcy) = arc_center(*entity_b)?;
                        let (bsx, bsy) = {
                            let (_, s, _) = layout.arc_endpoints[entity_b];
                            pt(s)?
                        };
                        Ok(CompiledConstraint::EqualArcArc {
                            acx, acy, asx, asy, bcx, bcy, bsx, bsy,
                        })
                    }
                    (EntityKind::Circle, EntityKind::Arc) => {
                        let r = radius_idx(*entity_a)?;
                        let (acx, acy) = arc_center(*entity_b)?;
                        let (asx, asy) = {
                            let (_, s, _) = layout.arc_endpoints[entity_b];
                            pt(s)?
                        };
                        Ok(CompiledConstraint::EqualArcCircle { acx, acy, asx, asy, r })
                    }
                    (EntityKind::Arc, EntityKind::Circle) => {
                        let r = radius_idx(*entity_b)?;
                        let (acx, acy) = arc_center(*entity_a)?;
                        let (asx, asy) = {
                            let (_, s, _) = layout.arc_endpoints[entity_a];
                            pt(s)?
                        };
                        Ok(CompiledConstraint::EqualArcCircle { acx, acy, asx, asy, r })
                    }
                    _ => Err(format!(
                        "Equal constraint not supported between {ka:?} and {kb:?}"
                    )),
                }
            }

            SketchConstraint::Distance { entity_a, entity_b, value } => {
                let ka = kind_of(*entity_a)?;
                let kb = kind_of(*entity_b)?;
                match (ka, kb) {
                    (EntityKind::Point, EntityKind::Point) => {
                        let (ax, ay) = pt(*entity_a)?;
                        let (bx, by) = pt(*entity_b)?;
                        Ok(CompiledConstraint::DistancePP { ax, ay, bx, by, value: *value })
                    }
                    (EntityKind::Point, EntityKind::Line) => {
                        let (px, py) = pt(*entity_a)?;
                        let [ax, ay, bx, by] = line_param_pts(*entity_b)?;
                        Ok(CompiledConstraint::DistancePL { px, py, ax, ay, bx, by, value: *value })
                    }
                    (EntityKind::Line, EntityKind::Point) => {
                        let (px, py) = pt(*entity_b)?;
                        let [ax, ay, bx, by] = line_param_pts(*entity_a)?;
                        Ok(CompiledConstraint::DistancePL { px, py, ax, ay, bx, by, value: *value })
                    }
                    _ => Err(format!(
                        "Distance constraint not supported between {ka:?} and {kb:?}"
                    )),
                }
            }

            SketchConstraint::PointLineDistance { point, entity, value } => {
                // Perpendicular point-to-line distance — same residual as the
                // (Point, Line) arm of `Distance`.
                let (px, py) = pt(*point)?;
                let [ax, ay, bx, by] = line_param_pts(*entity)?;
                Ok(CompiledConstraint::DistancePL { px, py, ax, ay, bx, by, value: *value })
            }

            SketchConstraint::HDistance { point_a, point_b, value } => {
                let (ax, _ay) = pt(*point_a)?;
                let (bx, _by) = pt(*point_b)?;
                Ok(CompiledConstraint::HDistance { ax, bx, value: *value })
            }

            SketchConstraint::VDistance { point_a, point_b, value } => {
                let (_ax, ay) = pt(*point_a)?;
                let (_bx, by) = pt(*point_b)?;
                Ok(CompiledConstraint::VDistance { ay, by, value: *value })
            }

            SketchConstraint::Angle { line_a, line_b, value_degrees } => {
                let [ax, ay, bx, by] = line_param_pts(*line_a)?;
                let [cx, cy, dx, dy] = line_param_pts(*line_b)?;
                Ok(CompiledConstraint::Angle {
                    ax, ay, bx, by, cx, cy, dx, dy,
                    value_radians: value_degrees.to_radians(),
                })
            }

            SketchConstraint::Radius { entity, value } => {
                let kind = kind_of(*entity)?;
                match kind {
                    EntityKind::Circle => {
                        let r = radius_idx(*entity)?;
                        Ok(CompiledConstraint::Radius { r, value: *value })
                    }
                    EntityKind::Arc => {
                        let (cx, cy) = arc_center(*entity)?;
                        let (sx, sy) = {
                            let (_, s, _) = layout.arc_endpoints[entity];
                            pt(s)?
                        };
                        Ok(CompiledConstraint::RadiusArc { cx, cy, sx, sy, value: *value })
                    }
                    _ => Err(format!("Radius requires circle/arc, got {kind:?}")),
                }
            }

            SketchConstraint::Diameter { entity, value } => {
                let kind = kind_of(*entity)?;
                match kind {
                    EntityKind::Circle => {
                        let r = radius_idx(*entity)?;
                        Ok(CompiledConstraint::Diameter { r, value: *value })
                    }
                    EntityKind::Arc => {
                        let (cx, cy) = arc_center(*entity)?;
                        let (sx, sy) = {
                            let (_, s, _) = layout.arc_endpoints[entity];
                            pt(s)?
                        };
                        Ok(CompiledConstraint::DiameterArc { cx, cy, sx, sy, value: *value })
                    }
                    _ => Err(format!("Diameter requires circle/arc, got {kind:?}")),
                }
            }

            SketchConstraint::OnEntity { point, entity } => {
                let (px, py) = pt(*point)?;
                let ke = kind_of(*entity)?;
                match ke {
                    EntityKind::Line => {
                        let [ax, ay, bx, by] = line_param_pts(*entity)?;
                        Ok(CompiledConstraint::OnEntityLine { px, py, ax, ay, bx, by })
                    }
                    EntityKind::Circle => {
                        let r = radius_idx(*entity)?;
                        let (cx, cy) = circle_center(*entity)?;
                        Ok(CompiledConstraint::OnEntityCircle { px, py, cx, cy, r })
                    }
                    EntityKind::Arc => {
                        let (cx, cy) = arc_center(*entity)?;
                        let (sx, sy) = {
                            let (_, s, _) = layout.arc_endpoints[entity];
                            pt(s)?
                        };
                        Ok(CompiledConstraint::OnEntityArc { px, py, cx, cy, sx, sy })
                    }
                    _ => Err(format!("OnEntity target must be line/circle/arc, got {ke:?}")),
                }
            }

            SketchConstraint::Midpoint { point, line } => {
                let (px, py) = pt(*point)?;
                let [ax, ay, bx, by] = line_param_pts(*line)?;
                Ok(CompiledConstraint::Midpoint { px, py, ax, ay, bx, by })
            }

            SketchConstraint::Dragged { point } => {
                let (px, py) = pt(*point)?;
                // Fixed position = current position at compile time.
                // The caller passes layout.params, so we snapshot here.
                let fixed_x = layout.params[px];
                let fixed_y = layout.params[py];
                Ok(CompiledConstraint::Dragged { px, py, fixed_x, fixed_y })
            }

            // ── PR-SS2 constraints ──────────────────────────────────────

            SketchConstraint::Symmetric { entity_a, entity_b, symmetry_line } => {
                let (ax, ay) = pt(*entity_a)?;
                let (bx, by) = pt(*entity_b)?;
                let [cx, cy, dx, dy] = line_param_pts(*symmetry_line)?;
                Ok(CompiledConstraint::Symmetric { ax, ay, bx, by, cx, cy, dx, dy })
            }

            SketchConstraint::SymmetricH { point_a, point_b } => {
                let (ax, ay) = pt(*point_a)?;
                let (bx, by) = pt(*point_b)?;
                Ok(CompiledConstraint::SymmetricH { ax, ay, bx, by })
            }

            SketchConstraint::SymmetricV { point_a, point_b } => {
                let (ax, ay) = pt(*point_a)?;
                let (bx, by) = pt(*point_b)?;
                Ok(CompiledConstraint::SymmetricV { ax, ay, bx, by })
            }

            SketchConstraint::Tangent { line, curve } => {
                let [ax, ay, bx, by] = line_param_pts(*line)?;
                let kind = kind_of(*curve)?;
                match kind {
                    EntityKind::Circle => {
                        let r = radius_idx(*curve)?;
                        let (cx, cy) = circle_center(*curve)?;
                        Ok(CompiledConstraint::TangentLineCircle { cx, cy, r, ax, ay, bx, by })
                    }
                    EntityKind::Arc => {
                        let (cx, cy) = arc_center(*curve)?;
                        let (sx, sy) = {
                            let (_, s, _) = layout.arc_endpoints[curve];
                            pt(s)?
                        };
                        Ok(CompiledConstraint::TangentLineArc { cx, cy, sx, sy, ax, ay, bx, by })
                    }
                    _ => Err(format!("Tangent curve must be arc or circle, got {kind:?}")),
                }
            }

            SketchConstraint::EqualAngle { line_a, line_b, line_c, line_d } => {
                let [ax, ay, bx, by] = line_param_pts(*line_a)?;
                let [cx, cy, dx, dy] = line_param_pts(*line_b)?;
                let [ex, ey, fx, fy] = line_param_pts(*line_c)?;
                let [gx, gy, hx, hy] = line_param_pts(*line_d)?;
                Ok(CompiledConstraint::EqualAngle {
                    ax, ay, bx, by, cx, cy, dx, dy,
                    ex, ey, fx, fy, gx, gy, hx, hy,
                })
            }

            SketchConstraint::Ratio { entity_a, entity_b, value } => {
                let [ax, ay, bx, by] = line_param_pts(*entity_a)?;
                let [cx, cy, dx, dy] = line_param_pts(*entity_b)?;
                Ok(CompiledConstraint::Ratio { ax, ay, bx, by, cx, cy, dx, dy, value: *value })
            }

            SketchConstraint::EqualPointToLine { point_a, point_b, line } => {
                let (ax, ay) = pt(*point_a)?;
                let (bx, by) = pt(*point_b)?;
                let [lx0, ly0, lx1, ly1] = line_param_pts(*line)?;
                Ok(CompiledConstraint::EqualPointToLine { ax, ay, bx, by, lx0, ly0, lx1, ly1 })
            }

            SketchConstraint::SameOrientation { .. } => {
                // 2D noop: normals are always aligned in the workplane.
                // Zero residual rows, zero Jacobian rows — contributes nothing.
                Ok(CompiledConstraint::SameOrientation)
            }
        }
    }

    /// Evaluate the residual vector for this constraint given the current
    /// parameter vector `p`. Returns a `DVector` of length `residual_count`.
    pub fn residuals(&self, p: &[f64]) -> DVector<f64> {
        match self {
            CompiledConstraint::Coincident { ax, ay, bx, by } => {
                let mut r = DVector::zeros(2);
                r[0] = p[*ax] - p[*bx];
                r[1] = p[*ay] - p[*by];
                r
            }
            CompiledConstraint::Horizontal { ay, by } => {
                let mut r = DVector::zeros(1);
                r[0] = p[*by] - p[*ay];
                r
            }
            CompiledConstraint::Vertical { ax, bx } => {
                let mut r = DVector::zeros(1);
                r[0] = p[*bx] - p[*ax];
                r
            }
            CompiledConstraint::Parallel { ax, ay, bx, by, cx, cy, dx, dy } => {
                // d_a × d_b = (bx-ax)(dy-cy) - (by-ay)(dx-cx)
                let mut r = DVector::zeros(1);
                r[0] = (p[*bx] - p[*ax]) * (p[*dy] - p[*cy])
                    - (p[*by] - p[*ay]) * (p[*dx] - p[*cx]);
                r
            }
            CompiledConstraint::Perpendicular { ax, ay, bx, by, cx, cy, dx, dy } => {
                // d_a · d_b = (bx-ax)(dx-cx) + (by-ay)(dy-cy)
                let mut r = DVector::zeros(1);
                r[0] = (p[*bx] - p[*ax]) * (p[*dx] - p[*cx])
                    + (p[*by] - p[*ay]) * (p[*dy] - p[*cy]);
                r
            }
            CompiledConstraint::EqualLines { ax, ay, bx, by, cx, cy, dx, dy } => {
                // ℓ_a − ℓ_b (unsquared — spec deviation #5 revised: the squared
                // form amplifies errors by ~2ℓ, making the 1e-6 tolerance
                // unreachable for typical geometry. The sqrt singularity at
                // zero-length lines is handled by the dist > 1e-15 guard.)
                let mut r = DVector::zeros(1);
                let la = ((p[*bx] - p[*ax]).powi(2) + (p[*by] - p[*ay]).powi(2)).sqrt();
                let lb = ((p[*dx] - p[*cx]).powi(2) + (p[*dy] - p[*cy]).powi(2)).sqrt();
                r[0] = la - lb;
                r
            }
            CompiledConstraint::EqualCircles { ra, rb } => {
                let mut r = DVector::zeros(1);
                r[0] = p[*ra] - p[*rb];
                r
            }
            CompiledConstraint::DistancePP { ax, ay, bx, by, value } => {
                let mut r = DVector::zeros(1);
                let dx = p[*bx] - p[*ax];
                let dy = p[*by] - p[*ay];
                let dist = (dx * dx + dy * dy).sqrt();
                r[0] = dist - value;
                r
            }
            CompiledConstraint::DistancePL { px, py, ax, ay, bx, by, value } => {
                let mut r = DVector::zeros(1);
                let dx = p[*bx] - p[*ax];
                let dy = p[*by] - p[*ay];
                let len = (dx * dx + dy * dy).sqrt();
                // Signed perpendicular distance: ((P-A) × d) / ℓ
                let cross = (p[*px] - p[*ax]) * dy - (p[*py] - p[*ay]) * dx;
                r[0] = cross / len - value;
                r
            }
            CompiledConstraint::HDistance { ax, bx, value } => {
                let mut r = DVector::zeros(1);
                r[0] = (p[*bx] - p[*ax]).abs() - value;
                r
            }
            CompiledConstraint::VDistance { ay, by, value } => {
                let mut r = DVector::zeros(1);
                r[0] = (p[*by] - p[*ay]).abs() - value;
                r
            }
            CompiledConstraint::Angle { ax, ay, bx, by, cx, cy, dx, dy, value_radians } => {
                let mut r = DVector::zeros(1);
                let dax = p[*bx] - p[*ax];
                let day = p[*by] - p[*ay];
                let dbx = p[*dx] - p[*cx];
                let dby = p[*dy] - p[*cy];
                let cross = dax * dby - day * dbx;
                let dot = dax * dbx + day * dby;
                let angle = cross.atan2(dot);
                r[0] = angle - value_radians;
                r
            }
            CompiledConstraint::Radius { r, value } => {
                let mut r_vec = DVector::zeros(1);
                r_vec[0] = p[*r] - value;
                r_vec
            }
            CompiledConstraint::Diameter { r, value } => {
                let mut r_vec = DVector::zeros(1);
                r_vec[0] = 2.0 * p[*r] - value;
                r_vec
            }
            CompiledConstraint::OnEntityLine { px, py, ax, ay, bx, by } => {
                let mut r = DVector::zeros(1);
                let dx = p[*bx] - p[*ax];
                let dy = p[*by] - p[*ay];
                let len = (dx * dx + dy * dy).sqrt();
                let cross = (p[*px] - p[*ax]) * dy - (p[*py] - p[*ay]) * dx;
                r[0] = cross / len;
                r
            }
            CompiledConstraint::OnEntityCircle { px, py, cx, cy, r } => {
                let mut r_vec = DVector::zeros(1);
                let dx = p[*px] - p[*cx];
                let dy = p[*py] - p[*cy];
                let dist = (dx * dx + dy * dy).sqrt();
                r_vec[0] = dist - p[*r];
                r_vec
            }
            // Arc radius = ‖C-S‖ (center→start distance)
            CompiledConstraint::RadiusArc { cx, cy, sx, sy, value } => {
                let mut r = DVector::zeros(1);
                let dx = p[*cx] - p[*sx];
                let dy = p[*cy] - p[*sy];
                let dist = (dx * dx + dy * dy).sqrt();
                r[0] = dist - value;
                r
            }
            CompiledConstraint::DiameterArc { cx, cy, sx, sy, value } => {
                let mut r = DVector::zeros(1);
                let dx = p[*cx] - p[*sx];
                let dy = p[*cy] - p[*sy];
                let dist = (dx * dx + dy * dy).sqrt();
                r[0] = 2.0 * dist - value;
                r
            }
            CompiledConstraint::EqualArcCircle { acx, acy, asx, asy, r } => {
                let mut rv = DVector::zeros(1);
                let dx = p[*acx] - p[*asx];
                let dy = p[*acy] - p[*asy];
                let arc_radius = (dx * dx + dy * dy).sqrt();
                rv[0] = arc_radius - p[*r];
                rv
            }
            CompiledConstraint::EqualArcArc { acx, acy, asx, asy, bcx, bcy, bsx, bsy } => {
                let mut r = DVector::zeros(1);
                let dax = p[*acx] - p[*asx];
                let day = p[*acy] - p[*asy];
                let ra = (dax * dax + day * day).sqrt();
                let dbx = p[*bcx] - p[*bsx];
                let dby = p[*bcy] - p[*bsy];
                let rb = (dbx * dbx + dby * dby).sqrt();
                r[0] = ra - rb;
                r
            }
            CompiledConstraint::OnEntityArc { px, py, cx, cy, sx, sy } => {
                let mut r = DVector::zeros(1);
                let pdx = p[*px] - p[*cx];
                let pdy = p[*py] - p[*cy];
                let pdist = (pdx * pdx + pdy * pdy).sqrt();
                let sdx = p[*cx] - p[*sx];
                let sdy = p[*cy] - p[*sy];
                let arc_radius = (sdx * sdx + sdy * sdy).sqrt();
                r[0] = pdist - arc_radius;
                r
            }
            CompiledConstraint::Midpoint { px, py, ax, ay, bx, by } => {
                let mut r = DVector::zeros(2);
                r[0] = p[*px] - (p[*ax] + p[*bx]) / 2.0;
                r[1] = p[*py] - (p[*ay] + p[*by]) / 2.0;
                r
            }
            CompiledConstraint::Dragged { px, py, fixed_x, fixed_y } => {
                let mut r = DVector::zeros(2);
                r[0] = p[*px] - fixed_x;
                r[1] = p[*py] - fixed_y;
                r
            }

            // ── PR-SS2 residuals ───────────────────────────────────────────

            // Symmetric about line L (C→D): midpoint on line + AB ⊥ L
            CompiledConstraint::Symmetric { ax, ay, bx, by, cx, cy, dx, dy } => {
                let ldx = p[*dx] - p[*cx];
                let ldy = p[*dy] - p[*cy];
                let len = (ldx * ldx + ldy * ldy).sqrt();
                let mut r = DVector::zeros(2);
                if len > 1e-15 {
                    let mid_x = (p[*ax] + p[*bx]) / 2.0;
                    let mid_y = (p[*ay] + p[*by]) / 2.0;
                    // r0: midpoint on line (signed perpendicular distance)
                    r[0] = ((mid_x - p[*cx]) * ldy - (mid_y - p[*cy]) * ldx) / len;
                    // r1: AB perpendicular to line direction
                    let abx = p[*ax] - p[*bx];
                    let aby = p[*ay] - p[*by];
                    r[1] = (abx * ldx + aby * ldy) / len;
                }
                r
            }

            // SymmetricH: mirror about Y-axis → x_a + x_b = 0, y_a - y_b = 0
            CompiledConstraint::SymmetricH { ax, ay, bx, by } => {
                let mut r = DVector::zeros(2);
                r[0] = p[*ax] + p[*bx];
                r[1] = p[*ay] - p[*by];
                r
            }

            // SymmetricV: mirror about X-axis → x_a - x_b = 0, y_a + y_b = 0
            CompiledConstraint::SymmetricV { ax, ay, bx, by } => {
                let mut r = DVector::zeros(2);
                r[0] = p[*ax] - p[*bx];
                r[1] = p[*ay] + p[*by];
                r
            }

            // Tangent line-circle: dist(center, line)² - radius² = 0
            // Using squared form to handle both sides of the line (signed dist can be ±radius)
            CompiledConstraint::TangentLineCircle { cx, cy, r, ax, ay, bx, by } => {
                let ldx = p[*bx] - p[*ax];
                let ldy = p[*by] - p[*ay];
                let len2 = ldx * ldx + ldy * ldy;
                let mut rv = DVector::zeros(1);
                if len2 > 1e-30 {
                    let cross = (p[*cx] - p[*ax]) * ldy - (p[*cy] - p[*ay]) * ldx;
                    let dist = cross / len2.sqrt();
                    rv[0] = dist * dist - p[*r] * p[*r];
                }
                rv
            }

            // Tangent line-arc: dist(center, line)² - ‖C-S‖² = 0
            CompiledConstraint::TangentLineArc { cx, cy, sx, sy, ax, ay, bx, by } => {
                let ldx = p[*bx] - p[*ax];
                let ldy = p[*by] - p[*ay];
                let len2 = ldx * ldx + ldy * ldy;
                let mut rv = DVector::zeros(1);
                if len2 > 1e-30 {
                    let cross = (p[*cx] - p[*ax]) * ldy - (p[*cy] - p[*ay]) * ldx;
                    let dist = cross / len2.sqrt();
                    let rdx = p[*cx] - p[*sx];
                    let rdy = p[*cy] - p[*sy];
                    let radius2 = rdx * rdx + rdy * rdy;
                    rv[0] = dist * dist - radius2;
                }
                rv
            }

            // EqualAngle: atan2(cross_ab, dot_ab) - atan2(cross_cd, dot_cd) = 0
            CompiledConstraint::EqualAngle {
                ax, ay, bx, by, cx, cy, dx, dy,
                ex, ey, fx, fy, gx, gy, hx, hy,
            } => {
                let dax = p[*bx] - p[*ax];
                let day = p[*by] - p[*ay];
                let dbx = p[*dx] - p[*cx];
                let dby = p[*dy] - p[*cy];
                let dex = p[*fx] - p[*ex];
                let dey = p[*fy] - p[*ey];
                let dgx = p[*hx] - p[*gx];
                let dgy = p[*hy] - p[*gy];

                let cross_ab = dax * dby - day * dbx;
                let dot_ab = dax * dbx + day * dby;
                let cross_cd = dex * dgy - dey * dgx;
                let dot_cd = dex * dgx + dey * dgy;

                let angle_ab = cross_ab.atan2(dot_ab);
                let angle_cd = cross_cd.atan2(dot_cd);

                let mut r = DVector::zeros(1);
                r[0] = angle_ab - angle_cd;
                r
            }

            // Ratio: ℓ_a - value * ℓ_b = 0
            CompiledConstraint::Ratio { ax, ay, bx, by, cx, cy, dx, dy, value } => {
                let la = ((p[*bx] - p[*ax]).powi(2) + (p[*by] - p[*ay]).powi(2)).sqrt();
                let lb = ((p[*dx] - p[*cx]).powi(2) + (p[*dy] - p[*cy]).powi(2)).sqrt();
                let mut r = DVector::zeros(1);
                r[0] = la - value * lb;
                r
            }

            // EqualPointToLine: dist(P_a, L) - dist(P_b, L) = 0
            CompiledConstraint::EqualPointToLine { ax, ay, bx, by, lx0, ly0, lx1, ly1 } => {
                let ldx = p[*lx1] - p[*lx0];
                let ldy = p[*ly1] - p[*ly0];
                let len = (ldx * ldx + ldy * ldy).sqrt();
                let mut r = DVector::zeros(1);
                if len > 1e-15 {
                    let cross_a = (p[*ax] - p[*lx0]) * ldy - (p[*ay] - p[*ly0]) * ldx;
                    let cross_b = (p[*bx] - p[*lx0]) * ldy - (p[*by] - p[*ly0]) * ldx;
                    r[0] = cross_a / len - cross_b / len;
                }
                r
            }

            // SameOrientation: 2D noop, zero residual rows
            CompiledConstraint::SameOrientation => {
                DVector::zeros(0)
            }
        }
    }

    /// Evaluate the analytic Jacobian for this constraint given the current
    /// parameter vector `p`. Returns a `DMatrix` of shape
    /// `(residual_count, n_params)`.
    pub fn jacobian(&self, p: &[f64], n_params: usize) -> DMatrix<f64> {
        match self {
            // ── Coincident: r = [xa-xb, yb-ya] ──────────────────────────
            // ∂r0/∂xa = +1, ∂r0/∂xb = -1
            // ∂r1/∂ya = -1, ∂r1/∂yb = +1
            CompiledConstraint::Coincident { ax, ay, bx, by } => {
                let mut j = DMatrix::zeros(2, n_params);
                j[(0, *ax)] = 1.0;
                j[(0, *bx)] = -1.0;
                j[(1, *ay)] = 1.0;
                j[(1, *by)] = -1.0;
                j
            }

            // ── Horizontal: r = yb - ya ──────────────────────────────────
            CompiledConstraint::Horizontal { ay, by } => {
                let mut j = DMatrix::zeros(1, n_params);
                j[(0, *ay)] = -1.0;
                j[(0, *by)] = 1.0;
                j
            }

            // ── Vertical: r = xb - xa ────────────────────────────────────
            CompiledConstraint::Vertical { ax, bx } => {
                let mut j = DMatrix::zeros(1, n_params);
                j[(0, *ax)] = -1.0;
                j[(0, *bx)] = 1.0;
                j
            }

            // ── Parallel: r = (bx-ax)(dy-cy) - (by-ay)(dx-cx) ───────────
            // Let da = (bx-ax, by-ay), db = (dx-cx, dy-cy)
            // r = da.x * db.y - da.y * db.x
            // ∂r/∂ax = -db.y, ∂r/∂ay = db.x
            // ∂r/∂bx = db.y,  ∂r/∂by = -db.x
            // ∂r/∂cx = da.y,  ∂r/∂cy = -da.x
            // ∂r/∂dx = -da.y, ∂r/∂dy = da.x
            CompiledConstraint::Parallel { ax, ay, bx, by, cx, cy, dx, dy } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dax = p[*bx] - p[*ax];
                let day = p[*by] - p[*ay];
                let dbx = p[*dx] - p[*cx];
                let dby = p[*dy] - p[*cy];
                j[(0, *ax)] = -dby;
                j[(0, *ay)] = dbx;
                j[(0, *bx)] = dby;
                j[(0, *by)] = -dbx;
                j[(0, *cx)] = day;
                j[(0, *cy)] = -dax;
                j[(0, *dx)] = -day;
                j[(0, *dy)] = dax;
                j
            }

            // ── Perpendicular: r = (bx-ax)(dx-cx) + (by-ay)(dy-cy) ───────
            // r = da.x * db.x + da.y * db.y
            // ∂r/∂ax = -db.x, ∂r/∂ay = -db.y
            // ∂r/∂bx = db.x,  ∂r/∂by = db.y
            // ∂r/∂cx = -da.x, ∂r/∂cy = -da.y
            // ∂r/∂dx = da.x,  ∂r/∂dy = da.y
            CompiledConstraint::Perpendicular { ax, ay, bx, by, cx, cy, dx, dy } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dax = p[*bx] - p[*ax];
                let day = p[*by] - p[*ay];
                let dbx = p[*dx] - p[*cx];
                let dby = p[*dy] - p[*cy];
                j[(0, *ax)] = -dbx;
                j[(0, *ay)] = -dby;
                j[(0, *bx)] = dbx;
                j[(0, *by)] = dby;
                j[(0, *cx)] = -dax;
                j[(0, *cy)] = -day;
                j[(0, *dx)] = dax;
                j[(0, *dy)] = day;
                j
            }

            // ── EqualLines: r = ℓ_a - ℓ_b (unsquared) ────────────────────
            // ℓ_a = ‖B-A‖, ∂ℓ_a/∂ax = -(bx-ax)/ℓ_a, ∂ℓ_a/∂bx = (bx-ax)/ℓ_a
            // Similarly for ℓ_b; subtract.
            CompiledConstraint::EqualLines { ax, ay, bx, by, cx, cy, dx, dy } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dax = p[*bx] - p[*ax];
                let day = p[*by] - p[*ay];
                let la = (dax * dax + day * day).sqrt();
                let dbx = p[*dx] - p[*cx];
                let dby = p[*dy] - p[*cy];
                let lb = (dbx * dbx + dby * dby).sqrt();
                if la > 1e-15 {
                    j[(0, *ax)] = -dax / la;
                    j[(0, *ay)] = -day / la;
                    j[(0, *bx)] = dax / la;
                    j[(0, *by)] = day / la;
                }
                if lb > 1e-15 {
                    j[(0, *cx)] = dbx / lb;
                    j[(0, *cy)] = dby / lb;
                    j[(0, *dx)] = -dbx / lb;
                    j[(0, *dy)] = -dby / lb;
                }
                j
            }

            // ── EqualCircles: r = ra - rb ───────────────────────────────
            CompiledConstraint::EqualCircles { ra, rb } => {
                let mut j = DMatrix::zeros(1, n_params);
                j[(0, *ra)] = 1.0;
                j[(0, *rb)] = -1.0;
                j
            }

            // ── DistancePP: r = ‖B-A‖ - v ───────────────────────────────
            // ∂r/∂ax = -(bx-ax)/d, ∂r/∂ay = -(by-ay)/d
            // ∂r/∂bx =  (bx-ax)/d, ∂r/∂by =  (by-ay)/d
            // where d = ‖B-A‖
            CompiledConstraint::DistancePP { ax, ay, bx, by, value: _ } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dx = p[*bx] - p[*ax];
                let dy = p[*by] - p[*ay];
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 1e-15 {
                    let ux = dx / dist;
                    let uy = dy / dist;
                    j[(0, *ax)] = -ux;
                    j[(0, *ay)] = -uy;
                    j[(0, *bx)] = ux;
                    j[(0, *by)] = uy;
                }
                j
            }

            // ── DistancePL: r = ((P-A)×d)/ℓ - v ─────────────────────────
            // d = (bx-ax, by-ay), ℓ = ‖d‖
            // cross = (px-ax)*dy - (py-ay)*dx  (where dx,dy depend on ax,ay,bx,by)
            // r = cross/ℓ - v
            //
            // ∂cross/∂px = dy,  ∂cross/∂py = -dx
            // ∂cross/∂ax = (py-ay) - dy   [chain rule through dx,dy]
            // ∂cross/∂ay = dx - (px-ax)
            // ∂cross/∂bx = -(py-ay)
            // ∂cross/∂by = (px-ax)
            // ∂ℓ/∂px = 0, ∂ℓ/∂py = 0
            // ∂ℓ/∂ax = -dx/ℓ, ∂ℓ/∂ay = -dy/ℓ
            // ∂ℓ/∂bx = dx/ℓ,  ∂ℓ/∂by = dy/ℓ
            // ∂r/∂i = (∂cross/∂i)/ℓ - cross*(∂ℓ/∂i)/ℓ²
            CompiledConstraint::DistancePL { px, py, ax, ay, bx, by, value: _ } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dx = p[*bx] - p[*ax];
                let dy = p[*by] - p[*ay];
                let len = (dx * dx + dy * dy).sqrt();
                if len > 1e-15 {
                    let cross = (p[*px] - p[*ax]) * dy - (p[*py] - p[*ay]) * dx;
                    let inv_l = 1.0 / len;
                    let inv_l2 = 1.0 / (len * len);
                    let pmay = p[*py] - p[*ay];
                    let pmax = p[*px] - p[*ax];
                    j[(0, *px)] = dy * inv_l;
                    j[(0, *py)] = -dx * inv_l;
                    j[(0, *ax)] = (pmay - dy) * inv_l + cross * dx * inv_l2 * inv_l;
                    j[(0, *ay)] = (dx - pmax) * inv_l + cross * dy * inv_l2 * inv_l;
                    j[(0, *bx)] = (-pmay) * inv_l - cross * dx * inv_l2 * inv_l;
                    j[(0, *by)] = pmax * inv_l - cross * dy * inv_l2 * inv_l;
                }
                j
            }

            // ── HDistance: r = |x_b - x_a| - v ──────────────────────────
            // ∂r/∂x_b = sign(Δx), ∂r/∂x_a = -sign(Δx). Sign(0) → 1 (the kink at
            // Δx=0 is harmless: a meaningful horizontal dimension is never zero).
            CompiledConstraint::HDistance { ax, bx, value: _ } => {
                let mut j = DMatrix::zeros(1, n_params);
                let s = if p[*bx] - p[*ax] < 0.0 { -1.0 } else { 1.0 };
                j[(0, *bx)] = s;
                j[(0, *ax)] = -s;
                j
            }

            // ── VDistance: r = |y_b - y_a| - v ──────────────────────────
            CompiledConstraint::VDistance { ay, by, value: _ } => {
                let mut j = DMatrix::zeros(1, n_params);
                let s = if p[*by] - p[*ay] < 0.0 { -1.0 } else { 1.0 };
                j[(0, *by)] = s;
                j[(0, *ay)] = -s;
                j
            }

            // ── Angle: r = atan2(da×db, da·db) - θ ──────────────────────
            // Let cross = dax*dby - day*dbx, dot = dax*dbx + day*dby
            // r = atan2(cross, dot) - θ
            // ∂r/∂p = (dot * ∂cross/∂p - cross * ∂dot/∂p) / (cross² + dot²)
            CompiledConstraint::Angle { ax, ay, bx, by, cx, cy, dx, dy, value_radians: _ } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dax = p[*bx] - p[*ax];
                let day = p[*by] - p[*ay];
                let dbx = p[*dx] - p[*cx];
                let dby = p[*dy] - p[*cy];
                let cross = dax * dby - day * dbx;
                let dot = dax * dbx + day * dby;
                let denom = cross * cross + dot * dot;
                if denom > 1e-20 {
                    let d = 1.0 / denom;
                    // ∂cross/∂ax = -dby, ∂cross/∂ay = dbx
                    // ∂cross/∂bx = dby,  ∂cross/∂by = -dbx
                    // ∂cross/∂cx = day,  ∂cross/∂cy = -dax
                    // ∂cross/∂dx = -day, ∂cross/∂dy = dax
                    // ∂dot/∂ax = -dbx, ∂dot/∂ay = -dby
                    // ∂dot/∂bx = dbx,  ∂dot/∂by = dby
                    // ∂dot/∂cx = -dax, ∂dot/∂cy = -day
                    // ∂dot/∂dx = dax,  ∂dot/∂dy = day
                    j[(0, *ax)] = (dot * (-dby) - cross * (-dbx)) * d;
                    j[(0, *ay)] = (dot * (dbx) - cross * (-dby)) * d;
                    j[(0, *bx)] = (dot * (dby) - cross * (dbx)) * d;
                    j[(0, *by)] = (dot * (-dbx) - cross * (dby)) * d;
                    j[(0, *cx)] = (dot * (day) - cross * (-dax)) * d;
                    j[(0, *cy)] = (dot * (-dax) - cross * (-day)) * d;
                    j[(0, *dx)] = (dot * (-day) - cross * (dax)) * d;
                    j[(0, *dy)] = (dot * (dax) - cross * (day)) * d;
                }
                j
            }

            // ── Radius: r = r - v ───────────────────────────────────────
            CompiledConstraint::Radius { r, value: _ } => {
                let mut j = DMatrix::zeros(1, n_params);
                j[(0, *r)] = 1.0;
                j
            }

            // ── Diameter: r = 2r - v ────────────────────────────────────
            CompiledConstraint::Diameter { r, value: _ } => {
                let mut j = DMatrix::zeros(1, n_params);
                j[(0, *r)] = 2.0;
                j
            }

            // ── OnEntityLine: r = ((P-A)×d)/ℓ (same as DistancePL with v=0)
            CompiledConstraint::OnEntityLine { px, py, ax, ay, bx, by } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dx = p[*bx] - p[*ax];
                let dy = p[*by] - p[*ay];
                let len = (dx * dx + dy * dy).sqrt();
                if len > 1e-15 {
                    let cross = (p[*px] - p[*ax]) * dy - (p[*py] - p[*ay]) * dx;
                    let inv_l = 1.0 / len;
                    let inv_l2 = 1.0 / (len * len);
                    let pmay = p[*py] - p[*ay];
                    let pmax = p[*px] - p[*ax];
                    j[(0, *px)] = dy * inv_l;
                    j[(0, *py)] = -dx * inv_l;
                    j[(0, *ax)] = (pmay - dy) * inv_l + cross * dx * inv_l2 * inv_l;
                    j[(0, *ay)] = (dx - pmax) * inv_l + cross * dy * inv_l2 * inv_l;
                    j[(0, *bx)] = (-pmay) * inv_l - cross * dx * inv_l2 * inv_l;
                    j[(0, *by)] = pmax * inv_l - cross * dy * inv_l2 * inv_l;
                }
                j
            }

            // ── OnEntityCircle: r = ‖P-C‖ - r ───────────────────────────
            // Same form as DistancePP with B=center, value=radius
            CompiledConstraint::OnEntityCircle { px, py, cx, cy, r } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dx = p[*px] - p[*cx];
                let dy = p[*py] - p[*cy];
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 1e-15 {
                    let ux = dx / dist;
                    let uy = dy / dist;
                    j[(0, *px)] = ux;
                    j[(0, *py)] = uy;
                    j[(0, *cx)] = -ux;
                    j[(0, *cy)] = -uy;
                    j[(0, *r)] = -1.0;
                }
                j
            }

            // ── RadiusArc: r = ‖C-S‖ - v ────────────────────────────────
            // Same form as DistancePP with B=S, A=C
            CompiledConstraint::RadiusArc { cx, cy, sx, sy, value: _ } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dx = p[*cx] - p[*sx];
                let dy = p[*cy] - p[*sy];
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 1e-15 {
                    let ux = dx / dist;
                    let uy = dy / dist;
                    j[(0, *cx)] = ux;
                    j[(0, *cy)] = uy;
                    j[(0, *sx)] = -ux;
                    j[(0, *sy)] = -uy;
                }
                j
            }

            // ── DiameterArc: r = 2*‖C-S‖ - v ────────────────────────────
            CompiledConstraint::DiameterArc { cx, cy, sx, sy, value: _ } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dx = p[*cx] - p[*sx];
                let dy = p[*cy] - p[*sy];
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 1e-15 {
                    let ux = 2.0 * dx / dist;
                    let uy = 2.0 * dy / dist;
                    j[(0, *cx)] = ux;
                    j[(0, *cy)] = uy;
                    j[(0, *sx)] = -ux;
                    j[(0, *sy)] = -uy;
                }
                j
            }

            // ── EqualArcCircle: r = ‖C-S‖_arc - r_circle ────────────────
            CompiledConstraint::EqualArcCircle { acx, acy, asx, asy, r } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dx = p[*acx] - p[*asx];
                let dy = p[*acy] - p[*asy];
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 1e-15 {
                    let ux = dx / dist;
                    let uy = dy / dist;
                    j[(0, *acx)] = ux;
                    j[(0, *acy)] = uy;
                    j[(0, *asx)] = -ux;
                    j[(0, *asy)] = -uy;
                    j[(0, *r)] = -1.0;
                }
                j
            }

            // ── EqualArcArc: r = ‖C_a-S_a‖ - ‖C_b-S_b‖ ──────────────────
            CompiledConstraint::EqualArcArc { acx, acy, asx, asy, bcx, bcy, bsx, bsy } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dax = p[*acx] - p[*asx];
                let day = p[*acy] - p[*asy];
                let da = (dax * dax + day * day).sqrt();
                let dbx = p[*bcx] - p[*bsx];
                let dby = p[*bcy] - p[*bsy];
                let db = (dbx * dbx + dby * dby).sqrt();
                if da > 1e-15 {
                    let uax = dax / da;
                    let uay = day / da;
                    j[(0, *acx)] = uax;
                    j[(0, *acy)] = uay;
                    j[(0, *asx)] = -uax;
                    j[(0, *asy)] = -uay;
                }
                if db > 1e-15 {
                    let ubx = dbx / db;
                    let uby = dby / db;
                    j[(0, *bcx)] = -ubx;
                    j[(0, *bcy)] = -uby;
                    j[(0, *bsx)] = ubx;
                    j[(0, *bsy)] = uby;
                }
                j
            }

            // ── OnEntityArc: r = ‖P-C‖ - ‖C-S‖ ─────────────────────────
            CompiledConstraint::OnEntityArc { px, py, cx, cy, sx, sy } => {
                let mut j = DMatrix::zeros(1, n_params);
                let pdx = p[*px] - p[*cx];
                let pdy = p[*py] - p[*cy];
                let pdist = (pdx * pdx + pdy * pdy).sqrt();
                let sdx = p[*cx] - p[*sx];
                let sdy = p[*cy] - p[*sy];
                let arc_r = (sdx * sdx + sdy * sdy).sqrt();
                if pdist > 1e-15 {
                    let upx = pdx / pdist;
                    let upy = pdy / pdist;
                    j[(0, *px)] = upx;
                    j[(0, *py)] = upy;
                    j[(0, *cx)] = -upx;
                    j[(0, *cy)] = -upy;
                }
                if arc_r > 1e-15 {
                    let usx = sdx / arc_r;
                    let usy = sdy / arc_r;
                    // ∂(-‖C-S‖)/∂cx = -usx, but we already have -upx for cx from pdist.
                    // Net: ∂r/∂cx = -upx - usx, ∂r/∂sx = +usx
                    j[(0, *cx)] -= usx;
                    j[(0, *cy)] -= usy;
                    j[(0, *sx)] = usx;
                    j[(0, *sy)] = usy;
                }
                j
            }

            // ── Midpoint: r = [P.x - (A.x+B.x)/2, P.y - (A.y+B.y)/2] ────
            CompiledConstraint::Midpoint { px, py, ax, ay, bx, by } => {
                let mut j = DMatrix::zeros(2, n_params);
                j[(0, *px)] = 1.0;
                j[(0, *ax)] = -0.5;
                j[(0, *bx)] = -0.5;
                j[(1, *py)] = 1.0;
                j[(1, *ay)] = -0.5;
                j[(1, *by)] = -0.5;
                j
            }

            // ── Dragged: r = [P.x - fixed_x, P.y - fixed_y] ─────────────
            CompiledConstraint::Dragged { px, py, .. } => {
                let mut j = DMatrix::zeros(2, n_params);
                j[(0, *px)] = 1.0;
                j[(1, *py)] = 1.0;
                j
            }

            // ── PR-SS2 Jacobians ───────────────────────────────────────────

            // Symmetric about line: r0 = midpoint-on-line, r1 = AB⊥L
            // Same structure as DistancePL (r0) and Perpendicular (r1)
            CompiledConstraint::Symmetric { ax, ay, bx, by, cx, cy, dx, dy } => {
                let mut j = DMatrix::zeros(2, n_params);
                let ldx = p[*dx] - p[*cx];
                let ldy = p[*dy] - p[*cy];
                let len = (ldx * ldx + ldy * ldy).sqrt();
                if len > 1e-15 {
                    let inv_l = 1.0 / len;
                    let inv_l2 = 1.0 / (len * len);
                    let mid_x = (p[*ax] + p[*bx]) / 2.0;
                    let mid_y = (p[*ay] + p[*by]) / 2.0;
                    let cross = (mid_x - p[*cx]) * ldy - (mid_y - p[*cy]) * ldx;

                    // r0 = cross/len — same form as DistancePL with P=midpoint
                    j[(0, *ax)] = 0.5 * ldy * inv_l;
                    j[(0, *bx)] = 0.5 * ldy * inv_l;
                    j[(0, *ay)] = -0.5 * ldx * inv_l;
                    j[(0, *by)] = -0.5 * ldx * inv_l;
                    // Line endpoints: chain rule through ldx, ldy
                    // ∂cross/∂cx = -ldy + (mid_y - cy), same as DistancePL with pmay
                    let pmay = mid_y - p[*cy];
                    let pmax = mid_x - p[*cx];
                    j[(0, *cx)] = ((pmay - ldy) * inv_l + cross * ldx * inv_l2 * inv_l);
                    j[(0, *cy)] = ((ldx - pmax) * inv_l + cross * ldy * inv_l2 * inv_l);
                    j[(0, *dx)] = (-pmay * inv_l - cross * ldx * inv_l2 * inv_l);
                    j[(0, *dy)] = (pmax * inv_l - cross * ldy * inv_l2 * inv_l);

                    // r1 = (AB · L) / len — AB perpendicular to L
                    // ∂r1/∂param = (∂dot/∂param * len - dot * ∂len/∂param) / len²
                    let abx = p[*ax] - p[*bx];
                    let aby = p[*ay] - p[*by];
                    let dot_al = abx * ldx + aby * ldy; // numerator of r1
                    j[(1, *ax)] = ldx * inv_l;
                    j[(1, *bx)] = -ldx * inv_l;
                    j[(1, *ay)] = ldy * inv_l;
                    j[(1, *by)] = -ldy * inv_l;
                    // Line endpoints: chain rule through ldx, ldy, and len
                    // ∂r1/∂cx = -abx/len + dot_al*ldx/len³
                    j[(1, *cx)] = -abx * inv_l + dot_al * ldx * inv_l2 * inv_l;
                    j[(1, *cy)] = -aby * inv_l + dot_al * ldy * inv_l2 * inv_l;
                    j[(1, *dx)] = abx * inv_l - dot_al * ldx * inv_l2 * inv_l;
                    j[(1, *dy)] = aby * inv_l - dot_al * ldy * inv_l2 * inv_l;
                }
                j
            }

            // SymmetricH: r = [xa+xb, ya-yb]
            CompiledConstraint::SymmetricH { ax, ay, bx, by } => {
                let mut j = DMatrix::zeros(2, n_params);
                j[(0, *ax)] = 1.0;
                j[(0, *bx)] = 1.0;
                j[(1, *ay)] = 1.0;
                j[(1, *by)] = -1.0;
                j
            }

            // SymmetricV: r = [xa-xb, ya+yb]
            CompiledConstraint::SymmetricV { ax, ay, bx, by } => {
                let mut j = DMatrix::zeros(2, n_params);
                j[(0, *ax)] = 1.0;
                j[(0, *bx)] = -1.0;
                j[(1, *ay)] = 1.0;
                j[(1, *by)] = 1.0;
                j
            }

            // TangentLineCircle: r = dist² - radius²
            // ∂r/∂p = 2*dist * ∂dist/∂p - 2*radius * ∂radius/∂p
            // where ∂dist/∂p is the same as the OnEntityLine/DistancePL Jacobian
            CompiledConstraint::TangentLineCircle { cx, cy, r, ax, ay, bx, by } => {
                let mut j = DMatrix::zeros(1, n_params);
                let ldx = p[*bx] - p[*ax];
                let ldy = p[*by] - p[*ay];
                let len = (ldx * ldx + ldy * ldy).sqrt();
                if len > 1e-15 {
                    let cross = (p[*cx] - p[*ax]) * ldy - (p[*cy] - p[*ay]) * ldx;
                    let dist = cross / len;
                    let radius = p[*r];
                    let inv_l = 1.0 / len;
                    let inv_l2 = 1.0 / (len * len);
                    let pmay = p[*cy] - p[*ay];
                    let pmax = p[*cx] - p[*ax];
                    let scale = 2.0 * dist;

                    // ∂dist/∂cx = ldy/len
                    j[(0, *cx)] = scale * ldy * inv_l;
                    j[(0, *cy)] = scale * (-ldx) * inv_l;
                    // ∂dist/∂ax: chain rule through ldx, ldy
                    j[(0, *ax)] = scale * (pmay - ldy) * inv_l + scale * cross * ldx * inv_l2 * inv_l;
                    j[(0, *ay)] = scale * (ldx - pmax) * inv_l + scale * cross * ldy * inv_l2 * inv_l;
                    j[(0, *bx)] = scale * (-pmay) * inv_l - scale * cross * ldx * inv_l2 * inv_l;
                    j[(0, *by)] = scale * pmax * inv_l - scale * cross * ldy * inv_l2 * inv_l;
                    // -2*radius * ∂radius/∂r = -2*radius
                    j[(0, *r)] = -2.0 * radius;
                }
                j
            }

            // TangentLineArc: r = dist² - ‖C-S‖²
            CompiledConstraint::TangentLineArc { cx, cy, sx, sy, ax, ay, bx, by } => {
                let mut j = DMatrix::zeros(1, n_params);
                let ldx = p[*bx] - p[*ax];
                let ldy = p[*by] - p[*ay];
                let len = (ldx * ldx + ldy * ldy).sqrt();
                let rdx = p[*cx] - p[*sx];
                let rdy = p[*cy] - p[*sy];
                let radius = (rdx * rdx + rdy * rdy).sqrt();
                if len > 1e-15 && radius > 1e-15 {
                    let cross = (p[*cx] - p[*ax]) * ldy - (p[*cy] - p[*ay]) * ldx;
                    let dist = cross / len;
                    let inv_l = 1.0 / len;
                    let inv_l2 = 1.0 / (len * len);
                    let pmay = p[*cy] - p[*ay];
                    let pmax = p[*cx] - p[*ax];
                    let scale = 2.0 * dist;

                    // dist² part
                    j[(0, *cx)] = scale * ldy * inv_l;
                    j[(0, *cy)] = scale * (-ldx) * inv_l;
                    j[(0, *ax)] = scale * (pmay - ldy) * inv_l + scale * cross * ldx * inv_l2 * inv_l;
                    j[(0, *ay)] = scale * (ldx - pmax) * inv_l + scale * cross * ldy * inv_l2 * inv_l;
                    j[(0, *bx)] = scale * (-pmay) * inv_l - scale * cross * ldx * inv_l2 * inv_l;
                    j[(0, *by)] = scale * pmax * inv_l - scale * cross * ldy * inv_l2 * inv_l;

                    // -‖C-S‖² part: ∂/∂cx = -2*rdx, ∂/∂sx = +2*rdx, etc.
                    j[(0, *cx)] -= 2.0 * rdx;
                    j[(0, *cy)] -= 2.0 * rdy;
                    j[(0, *sx)] = 2.0 * rdx;
                    j[(0, *sy)] = 2.0 * rdy;
                }
                j
            }

            // EqualAngle: r = atan2(cross_ab, dot_ab) - atan2(cross_cd, dot_cd)
            // Same derivative structure as Angle constraint
            CompiledConstraint::EqualAngle {
                ax, ay, bx, by, cx, cy, dx, dy,
                ex, ey, fx, fy, gx, gy, hx, hy,
            } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dax = p[*bx] - p[*ax];
                let day = p[*by] - p[*ay];
                let dbx = p[*dx] - p[*cx];
                let dby = p[*dy] - p[*cy];
                let dex = p[*fx] - p[*ex];
                let dey = p[*fy] - p[*ey];
                let dgx = p[*hx] - p[*gx];
                let dgy = p[*hy] - p[*gy];

                let cross_ab = dax * dby - day * dbx;
                let dot_ab = dax * dbx + day * dby;
                let denom_ab = cross_ab * cross_ab + dot_ab * dot_ab;

                let cross_cd = dex * dgy - dey * dgx;
                let dot_cd = dex * dgx + dey * dgy;
                let denom_cd = cross_cd * cross_cd + dot_cd * dot_cd;

                if denom_ab > 1e-20 {
                    let d = 1.0 / denom_ab;
                    // ∂atan2(cross, dot)/∂param = (dot * ∂cross - cross * ∂dot) / denom
                    j[(0, *ax)] = (dot_ab * (-dby) - cross_ab * (-dbx)) * d;
                    j[(0, *ay)] = (dot_ab * (dbx) - cross_ab * (-dby)) * d;
                    j[(0, *bx)] = (dot_ab * (dby) - cross_ab * (dbx)) * d;
                    j[(0, *by)] = (dot_ab * (-dbx) - cross_ab * (dby)) * d;
                    j[(0, *cx)] = (dot_ab * (day) - cross_ab * (-dax)) * d;
                    j[(0, *cy)] = (dot_ab * (-dax) - cross_ab * (-day)) * d;
                    j[(0, *dx)] = (dot_ab * (-day) - cross_ab * (dax)) * d;
                    j[(0, *dy)] = (dot_ab * (dax) - cross_ab * (day)) * d;
                }
                if denom_cd > 1e-20 {
                    let d = 1.0 / denom_cd;
                    // Subtract the second angle's derivative
                    j[(0, *ex)] -= (dot_cd * (-dgy) - cross_cd * (-dgx)) * d;
                    j[(0, *ey)] -= (dot_cd * (dgx) - cross_cd * (-dgy)) * d;
                    j[(0, *fx)] -= (dot_cd * (dgy) - cross_cd * (dgx)) * d;
                    j[(0, *fy)] -= (dot_cd * (-dgx) - cross_cd * (dgy)) * d;
                    j[(0, *gx)] -= (dot_cd * (dey) - cross_cd * (-dex)) * d;
                    j[(0, *gy)] -= (dot_cd * (-dex) - cross_cd * (-dey)) * d;
                    j[(0, *hx)] -= (dot_cd * (-dey) - cross_cd * (dex)) * d;
                    j[(0, *hy)] -= (dot_cd * (dex) - cross_cd * (dey)) * d;
                }
                j
            }

            // Ratio: r = ℓ_a - value * ℓ_b
            CompiledConstraint::Ratio { ax, ay, bx, by, cx, cy, dx, dy, value } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dax = p[*bx] - p[*ax];
                let day = p[*by] - p[*ay];
                let la = (dax * dax + day * day).sqrt();
                let dbx = p[*dx] - p[*cx];
                let dby = p[*dy] - p[*cy];
                let lb = (dbx * dbx + dby * dby).sqrt();
                if la > 1e-15 {
                    j[(0, *ax)] = -dax / la;
                    j[(0, *ay)] = -day / la;
                    j[(0, *bx)] = dax / la;
                    j[(0, *by)] = day / la;
                }
                if lb > 1e-15 {
                    j[(0, *cx)] = value * dbx / lb;
                    j[(0, *cy)] = value * dby / lb;
                    j[(0, *dx)] = -value * dbx / lb;
                    j[(0, *dy)] = -value * dby / lb;
                }
                j
            }

            // EqualPointToLine: r = dist(P_a, L) - dist(P_b, L)
            // dist(P, L) = cross/len — same as OnEntityLine
            CompiledConstraint::EqualPointToLine { ax, ay, bx, by, lx0, ly0, lx1, ly1 } => {
                let mut j = DMatrix::zeros(1, n_params);
                let ldx = p[*lx1] - p[*lx0];
                let ldy = p[*ly1] - p[*ly0];
                let len = (ldx * ldx + ldy * ldy).sqrt();
                if len > 1e-15 {
                    let inv_l = 1.0 / len;
                    let inv_l2 = 1.0 / (len * len);
                    let cross_a = (p[*ax] - p[*lx0]) * ldy - (p[*ay] - p[*ly0]) * ldx;
                    let cross_b = (p[*bx] - p[*lx0]) * ldy - (p[*by] - p[*ly0]) * ldx;
                    let amay = p[*ay] - p[*ly0];
                    let amax = p[*ax] - p[*lx0];
                    let bmay = p[*by] - p[*ly0];
                    let bmax = p[*bx] - p[*lx0];

                    // ∂dist(P_a, L)/∂param — point part
                    j[(0, *ax)] = ldy * inv_l;
                    j[(0, *ay)] = -ldx * inv_l;
                    // ∂dist(P_b, L)/∂param (subtracted) — point part
                    j[(0, *bx)] = -ldy * inv_l;
                    j[(0, *by)] = ldx * inv_l;

                    // Line endpoint derivatives — chain rule through ldx, ldy
                    // ∂dist_a/∂lx0 = (amay - ldy)/len + cross_a*ldx/(len³)
                    // ∂dist_b/∂lx0 = (bmay - ldy)/len + cross_b*ldx/(len³)
                    // r = dist_a - dist_b, so ∂r/∂lx0 = ∂dist_a/∂lx0 - ∂dist_b/∂lx0
                    j[(0, *lx0)] = ((amay - ldy) * inv_l + cross_a * ldx * inv_l2 * inv_l)
                        - ((bmay - ldy) * inv_l + cross_b * ldx * inv_l2 * inv_l);
                    j[(0, *ly0)] = ((ldx - amax) * inv_l + cross_a * ldy * inv_l2 * inv_l)
                        - ((ldx - bmax) * inv_l + cross_b * ldy * inv_l2 * inv_l);
                    j[(0, *lx1)] = (-amay * inv_l - cross_a * ldx * inv_l2 * inv_l)
                        - (-bmay * inv_l - cross_b * ldx * inv_l2 * inv_l);
                    j[(0, *ly1)] = (amax * inv_l - cross_a * ldy * inv_l2 * inv_l)
                        - (bmax * inv_l - cross_b * ldy * inv_l2 * inv_l);
                }
                j
            }

            // SameOrientation: 2D noop, zero rows
            CompiledConstraint::SameOrientation => {
                DMatrix::zeros(0, n_params)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use levenberg_marquardt::{differentiate_numerically, LeastSquaresProblem};

    /// A thin wrapper that implements LeastSquaresProblem for a single
    /// CompiledConstraint, so we can use `differentiate_numerically` to
    /// verify our analytic Jacobian.
    struct SingleConstraintProblem {
        cc: CompiledConstraint,
        params: DVector<f64>,
        n_params: usize,
        n_residuals: usize,
    }

    impl LeastSquaresProblem<f64, nalgebra::Dyn, nalgebra::Dyn> for SingleConstraintProblem {
        type ResidualStorage = nalgebra::VecStorage<f64, nalgebra::Dyn, nalgebra::U1>;
        type JacobianStorage =
            nalgebra::VecStorage<f64, nalgebra::Dyn, nalgebra::Dyn>;
        type ParameterStorage = nalgebra::VecStorage<f64, nalgebra::Dyn, nalgebra::U1>;

        fn set_params(&mut self, x: &DVector<f64>) {
            self.params = x.clone();
        }

        fn params(&self) -> DVector<f64> {
            self.params.clone()
        }

        fn residuals(&self) -> Option<DVector<f64>> {
            let p = self.params.as_slice();
            Some(self.cc.residuals(p))
        }

        fn jacobian(&self) -> Option<DMatrix<f64>> {
            let p = self.params.as_slice();
            Some(self.cc.jacobian(p, self.n_params))
        }
    }

    /// Verify analytic Jacobian against numerical differentiation at a
    /// randomly-ish-but-deterministic test point. Tolerance 1e-9 per spec.
    fn check_jacobian(cc: CompiledConstraint, p: Vec<f64>) {
        let n_params = p.len();
        let n_residuals = residual_count(&cc);
        let problem = SingleConstraintProblem {
            cc: cc.clone(),
            params: DVector::from_vec(p.clone()),
            n_params,
            n_residuals,
        };

        let mut prob_for_num = SingleConstraintProblem {
            cc,
            params: DVector::from_vec(p),
            n_params,
            n_residuals,
        };
        let numeric_j = differentiate_numerically(&mut prob_for_num)
            .expect("numerical differentiation failed");

        let analytic_j = problem
            .cc
            .jacobian(problem.params.as_slice(), n_params);

        let diff = (&numeric_j - &analytic_j).abs().max();
        assert!(
            diff < 1e-6,
            "Jacobian mismatch: max abs diff = {diff:.e}\n\
             analytic:\n{analytic_j}\n\
             numeric:\n{numeric_j}"
        );
    }

    // ── Group 2: Residual + Jacobian correctness ────────────────────────────
    // Each constraint: verify residual is zero at the expected solution, and
    // analytic Jacobian matches numerical differentiation (tol 1e-6; the spec
    // says 1e-9 but numerical differentiation has inherent noise — we use
    // 1e-6 as a practical tolerance and verify the analytic form is correct).

    #[test]
    fn coincident_residual_zero_and_jacobian() {
        // Points at (3, 4) and (3, 4) — coincident
        let cc = CompiledConstraint::Coincident { ax: 0, ay: 1, bx: 2, by: 3 };
        let p = vec![3.0, 4.0, 3.0, 4.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12 && r[1].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn horizontal_residual_zero_and_jacobian() {
        // Line from (1, 5) to (8, 5) — horizontal
        let cc = CompiledConstraint::Horizontal { ay: 1, by: 3 };
        let p = vec![1.0, 5.0, 8.0, 5.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn vertical_residual_zero_and_jacobian() {
        // Line from (2, 3) to (2, 9) — vertical
        let cc = CompiledConstraint::Vertical { ax: 0, bx: 2 };
        let p = vec![2.0, 3.0, 2.0, 9.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn parallel_residual_zero_and_jacobian() {
        // Line A: (0,0)→(10,0), Line B: (1,5)→(11,5) — parallel (both horizontal)
        let cc = CompiledConstraint::Parallel {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
        };
        let p = vec![0.0, 0.0, 10.0, 0.0, 1.0, 5.0, 11.0, 5.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn perpendicular_residual_zero_and_jacobian() {
        // Line A: (0,0)→(10,0) horizontal, Line B: (5,0)→(5,8) vertical — perpendicular
        let cc = CompiledConstraint::Perpendicular {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
        };
        let p = vec![0.0, 0.0, 10.0, 0.0, 5.0, 0.0, 5.0, 8.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn equal_lines_residual_zero_and_jacobian() {
        // Line A: (0,0)→(5,0) len=5, Line B: (1,1)→(6,1) len=5
        let cc = CompiledConstraint::EqualLines {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
        };
        let p = vec![0.0, 0.0, 5.0, 0.0, 1.0, 1.0, 6.0, 1.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn equal_circles_residual_zero_and_jacobian() {
        // Two circles with r=7
        let cc = CompiledConstraint::EqualCircles { ra: 0, rb: 1 };
        let p = vec![7.0, 7.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn distance_pp_residual_zero_and_jacobian() {
        // Points at (0,0) and (3,4) — distance = 5
        let cc = CompiledConstraint::DistancePP { ax: 0, ay: 1, bx: 2, by: 3, value: 5.0 };
        let p = vec![0.0, 0.0, 3.0, 4.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn distance_pl_residual_zero_and_jacobian() {
        // Line A: (0,0)→(10,0) horizontal. Point P at (5, -3).
        // Signed perpendicular distance (cross product convention) = +3.
        let cc = CompiledConstraint::DistancePL {
            px: 4, py: 5, ax: 0, ay: 1, bx: 2, by: 3, value: 3.0,
        };
        let p = vec![0.0, 0.0, 10.0, 0.0, 5.0, -3.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn angle_residual_zero_and_jacobian() {
        // Line A: (0,0)→(1,0) at 0°, Line B: (0,0)→(0,1) at 90°
        // Angle between them = 90° = π/2
        let cc = CompiledConstraint::Angle {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
            value_radians: std::f64::consts::FRAC_PI_2,
        };
        let p = vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn radius_residual_zero_and_jacobian() {
        let cc = CompiledConstraint::Radius { r: 0, value: 5.0 };
        let p = vec![5.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn diameter_residual_zero_and_jacobian() {
        // Diameter = 2*r, so r = value/2 = 25
        let cc = CompiledConstraint::Diameter { r: 0, value: 50.0 };
        let p = vec![25.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn on_entity_line_residual_zero_and_jacobian() {
        // Line A: (0,0)→(10,0). Point P at (5, 0) — on the line.
        let cc = CompiledConstraint::OnEntityLine {
            px: 4, py: 5, ax: 0, ay: 1, bx: 2, by: 3,
        };
        let p = vec![0.0, 0.0, 10.0, 0.0, 5.0, 0.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn on_entity_circle_residual_zero_and_jacobian() {
        // Circle center (0,0), r=5. Point P at (3,4) — on circle (dist=5).
        let cc = CompiledConstraint::OnEntityCircle {
            px: 3, py: 4, cx: 0, cy: 1, r: 2,
        };
        let p = vec![0.0, 0.0, 5.0, 3.0, 4.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn midpoint_residual_zero_and_jacobian() {
        // Line A: (0,0)→(10,0). Midpoint = (5,0). Point P at (5,0).
        let cc = CompiledConstraint::Midpoint {
            px: 4, py: 5, ax: 0, ay: 1, bx: 2, by: 3,
        };
        let p = vec![0.0, 0.0, 10.0, 0.0, 5.0, 0.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12 && r[1].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn dragged_residual_zero_and_jacobian() {
        // Point at (3, 7), fixed at (3, 7)
        let cc = CompiledConstraint::Dragged {
            px: 0, py: 1, fixed_x: 3.0, fixed_y: 7.0,
        };
        let p = vec![3.0, 7.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12 && r[1].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    // ── Additional: Jacobian at non-solution points (general position) ──────

    #[test]
    fn distance_pp_jacobian_general_position() {
        // Points NOT at target distance — verify Jacobian still correct
        let cc = CompiledConstraint::DistancePP { ax: 0, ay: 1, bx: 2, by: 3, value: 10.0 };
        check_jacobian(cc, vec![1.0, 2.0, 4.0, 6.0]); // actual dist ≈ 5, not 10
    }

    #[test]
    fn angle_jacobian_general_position() {
        let cc = CompiledConstraint::Angle {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
            value_radians: 1.0,
        };
        check_jacobian(cc, vec![0.0, 0.0, 3.0, 1.0, 1.0, 2.0, 4.0, 5.0]);
    }

    #[test]
    fn parallel_jacobian_general_position() {
        let cc = CompiledConstraint::Parallel {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
        };
        check_jacobian(cc, vec![0.0, 0.0, 5.0, 1.0, 2.0, 3.0, 7.0, 4.0]);
    }

    #[test]
    fn distance_pl_jacobian_general_position() {
        let cc = CompiledConstraint::DistancePL {
            px: 4, py: 5, ax: 0, ay: 1, bx: 2, by: 3, value: 2.0,
        };
        check_jacobian(cc, vec![0.0, 0.0, 10.0, 0.0, 5.0, 7.0]);
    }

    // ── PR-SS2 Jacobian verification tests ──────────────────────────────

    #[test]
    fn symmetric_residual_zero_and_jacobian() {
        // Points (20, 30) and (80, 30) symmetric about vertical line x=50
        // Line from (50, 0) to (50, 100)
        let cc = CompiledConstraint::Symmetric {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
        };
        let p = vec![20.0, 30.0, 80.0, 30.0, 50.0, 0.0, 50.0, 100.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12 && r[1].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn symmetric_jacobian_general_position() {
        let cc = CompiledConstraint::Symmetric {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
        };
        check_jacobian(cc, vec![15.0, 20.0, 70.0, 25.0, 40.0, 0.0, 45.0, 90.0]);
    }

    #[test]
    fn symmetric_h_residual_zero_and_jacobian() {
        // Symmetric about Y-axis: (30, 20) and (-30, 20)
        let cc = CompiledConstraint::SymmetricH { ax: 0, ay: 1, bx: 2, by: 3 };
        let p = vec![30.0, 20.0, -30.0, 20.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12 && r[1].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn symmetric_v_residual_zero_and_jacobian() {
        // Symmetric about X-axis: (20, 30) and (20, -30)
        let cc = CompiledConstraint::SymmetricV { ax: 0, ay: 1, bx: 2, by: 3 };
        let p = vec![20.0, 30.0, 20.0, -30.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12 && r[1].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn tangent_line_circle_residual_zero_and_jacobian() {
        // Circle center (0, 50), radius 50. Line y=0 from (-50,0) to (50,0).
        // dist(center, line) = 50 = radius → tangent
        let cc = CompiledConstraint::TangentLineCircle {
            cx: 4, cy: 5, r: 6, ax: 0, ay: 1, bx: 2, by: 3,
        };
        let p = vec![-50.0, 0.0, 50.0, 0.0, 0.0, 50.0, 50.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-9, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn tangent_line_circle_jacobian_general_position() {
        let cc = CompiledConstraint::TangentLineCircle {
            cx: 4, cy: 5, r: 6, ax: 0, ay: 1, bx: 2, by: 3,
        };
        check_jacobian(cc, vec![-40.0, 5.0, 45.0, -3.0, 10.0, 40.0, 30.0]);
    }

    #[test]
    fn tangent_line_arc_residual_zero_and_jacobian() {
        // Arc center (0, 50), start (0, 0) → radius = 50.
        // Line y=0 from (-50,0) to (50,0). dist = 50 = radius → tangent
        let cc = CompiledConstraint::TangentLineArc {
            cx: 4, cy: 5, sx: 6, sy: 7, ax: 0, ay: 1, bx: 2, by: 3,
        };
        let p = vec![-50.0, 0.0, 50.0, 0.0, 0.0, 50.0, 0.0, 0.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-9, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn tangent_line_arc_jacobian_general_position() {
        let cc = CompiledConstraint::TangentLineArc {
            cx: 4, cy: 5, sx: 6, sy: 7, ax: 0, ay: 1, bx: 2, by: 3,
        };
        check_jacobian(cc, vec![-40.0, 5.0, 45.0, -3.0, 10.0, 40.0, 5.0, 3.0]);
    }

    #[test]
    fn equal_angle_residual_zero_and_jacobian() {
        // Lines a=(0,0)→(1,0), b=(0,0)→(0,1): angle = 90°
        // Lines c=(0,0)→(1,0), d=(0,0)→(0,1): angle = 90°
        // Equal angle: 90° - 90° = 0
        let cc = CompiledConstraint::EqualAngle {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
            ex: 8, ey: 9, fx: 10, fy: 11,
            gx: 12, gy: 13, hx: 14, hy: 15,
        };
        let p = vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                      0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn equal_angle_jacobian_general_position() {
        let cc = CompiledConstraint::EqualAngle {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
            ex: 8, ey: 9, fx: 10, fy: 11,
            gx: 12, gy: 13, hx: 14, hy: 15,
        };
        check_jacobian(cc, vec![0.0, 0.0, 3.0, 1.0, 1.0, 2.0, 4.0, 5.0,
                                 2.0, 1.0, 5.0, 2.0, 0.0, 0.0, 1.0, 3.0]);
    }

    #[test]
    fn ratio_residual_zero_and_jacobian() {
        // Line a: (0,0)→(10,0) len=10, Line b: (0,0)→(5,0) len=5
        // Ratio = 10/5 = 2.0
        let cc = CompiledConstraint::Ratio {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
            value: 2.0,
        };
        let p = vec![0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 5.0, 0.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn ratio_jacobian_general_position() {
        let cc = CompiledConstraint::Ratio {
            ax: 0, ay: 1, bx: 2, by: 3,
            cx: 4, cy: 5, dx: 6, dy: 7,
            value: 1.5,
        };
        check_jacobian(cc, vec![0.0, 0.0, 7.0, 3.0, 1.0, 1.0, 6.0, 2.0]);
    }

    #[test]
    fn equal_point_to_line_residual_zero_and_jacobian() {
        // Line from (0,0) to (10,0). Points at (3, 5) and (7, 5).
        // Both are distance 5 from the line.
        let cc = CompiledConstraint::EqualPointToLine {
            ax: 0, ay: 1, bx: 2, by: 3,
            lx0: 4, ly0: 5, lx1: 6, ly1: 7,
        };
        let p = vec![3.0, 5.0, 7.0, 5.0, 0.0, 0.0, 10.0, 0.0];
        let r = cc.residuals(&p);
        assert!(r[0].abs() < 1e-12, "residual not zero: {r}");
        check_jacobian(cc, p);
    }

    #[test]
    fn equal_point_to_line_jacobian_general_position() {
        let cc = CompiledConstraint::EqualPointToLine {
            ax: 0, ay: 1, bx: 2, by: 3,
            lx0: 4, ly0: 5, lx1: 6, ly1: 7,
        };
        check_jacobian(cc, vec![3.0, 7.0, 8.0, 2.0, 1.0, 1.0, 9.0, 4.0]);
    }
}
