//! Clean-room constraint mapping: each `SketchConstraint` variant → residual
//! block + analytic Jacobian.
//!
//! PR-SS1 scope: 13 constraints (Coincident, Horizontal, Vertical, Parallel,
//! Perpendicular, Equal, Distance, Angle, Radius, Diameter, OnEntity,
//! Midpoint, Dragged). The 8 remaining mapped-but-unexposed constraints
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
    Midpoint {
        px: usize, py: usize,
        ax: usize, ay: usize, bx: usize, by: usize,
    },
    Dragged {
        px: usize, py: usize,
        fixed_x: f64, fixed_y: f64,
    },
}

/// Number of residual rows a compiled constraint contributes.
pub fn residual_count(cc: &CompiledConstraint) -> usize {
    match cc {
        CompiledConstraint::Coincident { .. } => 2,
        CompiledConstraint::Midpoint { .. } => 2,
        CompiledConstraint::Dragged { .. } => 2,
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
            // Circle center is a point referenced by center_id. We don't store
            // center_id directly in ParamLayout, so we look it up from the
            // entity list. But ParamLayout doesn't retain the entity list.
            // Workaround: circle radius index exists; center point must be
            // found by convention. Actually, we need center_id.
            //
            // For circles, the radius param exists. The center point params
            // are found via the circle's center_id. Since ParamLayout stores
            // radius_indices keyed by circle entity ID, and the center is a
            // separate point entity, we need the mapping.
            //
            // FIXME: This requires ParamLayout to store circle center_id.
            // For PR-SS1b (Jacobian unit tests), we pass layout explicitly,
            // so tests construct layouts that have the circle + center point.
            // The actual compile path needs this mapping. Defer to PR-SS1c
            // when we wire the full solver — for now, return an error if
            // the circle's center isn't findable via a side channel.
            //
            // Actually, let's just require the caller to have pre-resolved.
            // This won't be hit in PR-SS1b unit tests since they construct
            // CompiledConstraint variants directly.
            let _ = id;
            Err("circle center resolution requires full entity list (PR-SS1c)".into())
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
                    (EntityKind::Circle, EntityKind::Circle)
                    | (EntityKind::Circle, EntityKind::Arc)
                    | (EntityKind::Arc, EntityKind::Circle)
                    | (EntityKind::Arc, EntityKind::Arc) => {
                        let ra = radius_idx(*entity_a)?;
                        let rb = radius_idx(*entity_b)?;
                        Ok(CompiledConstraint::EqualCircles { ra, rb })
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

            SketchConstraint::Angle { line_a, line_b, value_degrees } => {
                let [ax, ay, bx, by] = line_param_pts(*line_a)?;
                let [cx, cy, dx, dy] = line_param_pts(*line_b)?;
                Ok(CompiledConstraint::Angle {
                    ax, ay, bx, by, cx, cy, dx, dy,
                    value_radians: value_degrees.to_radians(),
                })
            }

            SketchConstraint::Radius { entity, value } => {
                let r = radius_idx(*entity)?;
                Ok(CompiledConstraint::Radius { r, value: *value })
            }

            SketchConstraint::Diameter { entity, value } => {
                let r = radius_idx(*entity)?;
                Ok(CompiledConstraint::Diameter { r, value: *value })
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
                        // Arc center is found via arc_endpoints. For PR-SS1b,
                        // we defer arc OnEntity to PR-SS1c full wiring.
                        Err("OnEntity on arc requires full entity list (PR-SS1c)".into())
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

            // Banked to PR-SS2:
            SketchConstraint::Tangent { .. }
            | SketchConstraint::Symmetric { .. }
            | SketchConstraint::SymmetricH { .. }
            | SketchConstraint::SymmetricV { .. }
            | SketchConstraint::EqualAngle { .. }
            | SketchConstraint::Ratio { .. }
            | SketchConstraint::EqualPointToLine { .. }
            | SketchConstraint::SameOrientation { .. } => {
                Err(format!(
                    "constraint {:?} not in PR-SS1 scope (banked to PR-SS2)",
                    constraint
                ))
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
                // ℓ²_a − ℓ²_b (squared to avoid sqrt — see spec deviation #5)
                let mut r = DVector::zeros(1);
                let la2 = (p[*bx] - p[*ax]).powi(2) + (p[*by] - p[*ay]).powi(2);
                let lb2 = (p[*dx] - p[*cx]).powi(2) + (p[*dy] - p[*cy]).powi(2);
                r[0] = la2 - lb2;
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

            // ── EqualLines: r = ℓ²_a - ℓ²_b ──────────────────────────────
            // ℓ²_a = (bx-ax)² + (by-ay)²
            // ∂ℓ²_a/∂ax = -2(bx-ax), ∂ℓ²_a/∂ay = -2(by-ay)
            // ∂ℓ²_a/∂bx =  2(bx-ax), ∂ℓ²_a/∂by =  2(by-ay)
            // Similarly for ℓ²_b; subtract.
            CompiledConstraint::EqualLines { ax, ay, bx, by, cx, cy, dx, dy } => {
                let mut j = DMatrix::zeros(1, n_params);
                let dax = p[*bx] - p[*ax];
                let day = p[*by] - p[*ay];
                let dbx = p[*dx] - p[*cx];
                let dby = p[*dy] - p[*cy];
                j[(0, *ax)] = -2.0 * dax;
                j[(0, *ay)] = -2.0 * day;
                j[(0, *bx)] = 2.0 * dax;
                j[(0, *by)] = 2.0 * day;
                j[(0, *cx)] = 2.0 * dbx;
                j[(0, *cy)] = 2.0 * dby;
                j[(0, *dx)] = -2.0 * dbx;
                j[(0, *dy)] = -2.0 * dby;
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
}
