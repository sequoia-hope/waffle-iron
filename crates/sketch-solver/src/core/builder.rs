//! Constraint builder: maps `SketchConstraint` → `ConstraintImpl`.
//!
//! Resolves entity IDs to typed indices using `ParamLayout` at build time.
//! The solver never touches entity IDs after this step.

use std::collections::HashMap;

use nalgebra::Point2;

use crate::types::{EntityId, EntityKind, PointId, SketchConstraint, SketchEntity};

use super::constraint::ConstraintImpl;
use super::error::ValidationError;
use super::params::ParamLayout;
use super::types::{LineIdx, RadiusDef};

/// Build internal constraint representations from sketch constraints.
///
/// Entity-type dispatch (Equal, Distance, OnEntity, etc.) inspects
/// `entity_types` to choose the correct ConstraintImpl variant.
pub fn build_constraints(
    constraints: &[SketchConstraint],
    entities: &[SketchEntity],
    layout: &ParamLayout,
    x0: &[f64],
) -> Result<Vec<ConstraintImpl>, ValidationError> {
    let entity_types = classify_entities(entities);
    let mut out = Vec::new();
    for c in constraints {
        if let Some(ci) = build_one(c, &entity_types, entities, layout, x0)? {
            out.push(ci);
        }
    }
    Ok(out)
}

/// Map each equation row back to its parent constraint index.
///
/// Multi-equation constraints (Coincident=2, Midpoint=2, etc.) produce
/// multiple rows pointing to the same constraint index.
pub fn build_eq_to_constraint_map(constraints: &[ConstraintImpl]) -> Vec<usize> {
    use super::constraint::ConstraintEq;
    let mut map = Vec::new();
    for (i, c) in constraints.iter().enumerate() {
        for _ in 0..c.num_equations() {
            map.push(i);
        }
    }
    map
}

// ── Internals ──────────────────────────────────────────────────────────────

fn classify_entities(entities: &[SketchEntity]) -> HashMap<EntityId, EntityKind> {
    let mut map = HashMap::new();
    for e in entities {
        let kind = match e {
            SketchEntity::Point { .. } => EntityKind::Point,
            SketchEntity::Line { .. } => EntityKind::Line,
            SketchEntity::Circle { .. } => EntityKind::Circle,
            SketchEntity::Arc { .. } => EntityKind::Arc,
            _ => continue,
        };
        map.insert(e.id(), kind);
    }
    map
}

fn find_point_pos(
    entities: &[SketchEntity],
    point_id: PointId,
) -> Result<(f64, f64), ValidationError> {
    for e in entities {
        if let SketchEntity::Point { id, x, y, .. } = e {
            if *id == point_id {
                return Ok((*x, *y));
            }
        }
    }
    Err(ValidationError::UnknownPoint(point_id))
}

fn find_center_id(
    entities: &[SketchEntity],
    entity_id: EntityId,
) -> Result<PointId, ValidationError> {
    for e in entities {
        if e.id() == entity_id {
            match e {
                SketchEntity::Circle { center_id, .. } => return Ok(*center_id),
                SketchEntity::Arc { center_id, .. } => return Ok(*center_id),
                _ => return Err(ValidationError::ExpectedArcOrCircle(entity_id)),
            }
        }
    }
    Err(ValidationError::UnknownEntity(entity_id))
}

fn build_one(
    constraint: &SketchConstraint,
    entity_types: &HashMap<EntityId, EntityKind>,
    entities: &[SketchEntity],
    layout: &ParamLayout,
    x0: &[f64],
) -> Result<Option<ConstraintImpl>, ValidationError> {
    use ConstraintImpl as CI;
    use SketchConstraint::*;

    match constraint {
        Coincident { point_a, point_b } => Ok(Some(CI::Coincident {
            p1: layout.point(*point_a)?,
            p2: layout.point(*point_b)?,
        })),
        Horizontal { entity } => Ok(Some(CI::Horizontal {
            line: layout.line(*entity)?,
        })),
        Vertical { entity } => Ok(Some(CI::Vertical {
            line: layout.line(*entity)?,
        })),
        Parallel { line_a, line_b } => Ok(Some(CI::Parallel {
            l1: layout.line(*line_a)?,
            l2: layout.line(*line_b)?,
        })),
        Perpendicular { line_a, line_b } => Ok(Some(CI::Perpendicular {
            l1: layout.line(*line_a)?,
            l2: layout.line(*line_b)?,
        })),
        SymmetricH { point_a, point_b } => Ok(Some(CI::SymmetricH {
            p1: layout.point(*point_a)?,
            p2: layout.point(*point_b)?,
        })),
        SymmetricV { point_a, point_b } => Ok(Some(CI::SymmetricV {
            p1: layout.point(*point_a)?,
            p2: layout.point(*point_b)?,
        })),
        Symmetric {
            entity_a,
            entity_b,
            symmetry_line,
        } => Ok(Some(CI::SymmetricLine {
            p1: layout.point(*entity_a)?,
            p2: layout.point(*entity_b)?,
            line: layout.line(*symmetry_line)?,
        })),
        Midpoint { point, line } => Ok(Some(CI::Midpoint {
            point: layout.point(*point)?,
            line: layout.line(*line)?,
        })),
        Dragged { point } => {
            let (x, y) = find_point_pos(entities, *point)?;
            Ok(Some(CI::Dragged {
                point: layout.point(*point)?,
                target: Point2::new(x, y),
            }))
        }
        Angle {
            line_a,
            line_b,
            value_degrees,
        } => {
            if !value_degrees.is_finite() {
                return Err(ValidationError::InvalidAngle(*value_degrees));
            }
            Ok(Some(CI::Angle {
                l1: layout.line(*line_a)?,
                l2: layout.line(*line_b)?,
                value_rad: value_degrees.to_radians(),
            }))
        }
        SameOrientation { .. } => Ok(Some(CI::SameOrientation)),

        Equal { entity_a, entity_b } => {
            let kind_a = match entity_types.get(entity_a) {
                Some(k) => k,
                None => return Ok(None),
            };
            let kind_b = match entity_types.get(entity_b) {
                Some(k) => k,
                None => return Ok(None),
            };
            match (kind_a, kind_b) {
                (EntityKind::Line, EntityKind::Line) => Ok(Some(CI::EqualLength {
                    l1: layout.line(*entity_a)?,
                    l2: layout.line(*entity_b)?,
                })),
                (EntityKind::Circle, EntityKind::Circle) => Ok(Some(CI::EqualRadius {
                    r1: layout.radius(*entity_a)?,
                    r2: layout.radius(*entity_b)?,
                })),
                (EntityKind::Arc, EntityKind::Arc) => {
                    let (c1, s1) = layout.arc_center_start(*entity_a)?;
                    let (c2, s2) = layout.arc_center_start(*entity_b)?;
                    Ok(Some(CI::EqualLength {
                        l1: LineIdx { start: c1, end: s1 },
                        l2: LineIdx { start: c2, end: s2 },
                    }))
                }
                (EntityKind::Circle, EntityKind::Arc) => {
                    let (center, start) = layout.arc_center_start(*entity_b)?;
                    Ok(Some(CI::OnCircle {
                        point: start,
                        center,
                        radius: RadiusDef::Param(layout.radius(*entity_a)?),
                    }))
                }
                (EntityKind::Arc, EntityKind::Circle) => {
                    let (center, start) = layout.arc_center_start(*entity_a)?;
                    Ok(Some(CI::OnCircle {
                        point: start,
                        center,
                        radius: RadiusDef::Param(layout.radius(*entity_b)?),
                    }))
                }
                _ => Ok(None),
            }
        }

        Distance {
            entity_a,
            entity_b,
            value,
        } => {
            if !value.is_finite() || *value < 0.0 {
                return Err(ValidationError::InvalidDistance(*value));
            }
            let kind_a = match entity_types.get(entity_a) {
                Some(k) => k,
                None => return Ok(None),
            };
            let kind_b = match entity_types.get(entity_b) {
                Some(k) => k,
                None => return Ok(None),
            };
            match (kind_a, kind_b) {
                (EntityKind::Point, EntityKind::Point) => Ok(Some(CI::DistancePP {
                    p1: layout.point(PointId(entity_a.0))?,
                    p2: layout.point(PointId(entity_b.0))?,
                    d: *value,
                })),
                (EntityKind::Point, EntityKind::Line) => {
                    let pt = layout.point(PointId(entity_a.0))?;
                    let ln = layout.line(*entity_b)?;
                    Ok(Some(CI::DistancePL {
                        point: pt,
                        line: ln,
                        d: *value,
                        sign: cross_sign(pt, ln, x0),
                    }))
                }
                (EntityKind::Line, EntityKind::Point) => {
                    let pt = layout.point(PointId(entity_b.0))?;
                    let ln = layout.line(*entity_a)?;
                    Ok(Some(CI::DistancePL {
                        point: pt,
                        line: ln,
                        d: *value,
                        sign: cross_sign(pt, ln, x0),
                    }))
                }
                _ => Ok(None),
            }
        }

        OnEntity { point, entity } => {
            let kind = match entity_types.get(entity) {
                Some(k) => k,
                None => return Ok(None),
            };
            match kind {
                EntityKind::Line => Ok(Some(CI::OnLine {
                    point: layout.point(*point)?,
                    line: layout.line(*entity)?,
                })),
                EntityKind::Circle => {
                    let center_id = find_center_id(entities, *entity)?;
                    Ok(Some(CI::OnCircle {
                        point: layout.point(*point)?,
                        center: layout.point(center_id)?,
                        radius: layout.radius_def(*entity)?,
                    }))
                }
                EntityKind::Arc => {
                    let (center, _start) = layout.arc_center_start(*entity)?;
                    Ok(Some(CI::OnCircle {
                        point: layout.point(*point)?,
                        center,
                        radius: layout.radius_def(*entity)?,
                    }))
                }
                _ => Ok(None),
            }
        }

        Tangent { line, curve } => {
            let kind = match entity_types.get(curve) {
                Some(k) => k,
                None => return Ok(None),
            };
            match kind {
                EntityKind::Arc => {
                    let (center, _start) = layout.arc_center_start(*curve)?;
                    let ln = layout.line(*line)?;
                    Ok(Some(CI::TangentLineCircle {
                        line: ln,
                        center,
                        radius: layout.radius_def(*curve)?,
                        sign: cross_sign_center(center, ln, x0),
                    }))
                }
                EntityKind::Circle => {
                    let center_id = find_center_id(entities, *curve)?;
                    let center = layout.point(center_id)?;
                    let ln = layout.line(*line)?;
                    Ok(Some(CI::TangentLineCircle {
                        line: ln,
                        center,
                        radius: layout.radius_def(*curve)?,
                        sign: cross_sign_center(center, ln, x0),
                    }))
                }
                _ => Ok(None),
            }
        }

        Radius { entity, value } => {
            if !value.is_finite() || *value < 0.0 {
                return Err(ValidationError::InvalidRadius(*value));
            }
            let kind = match entity_types.get(entity) {
                Some(k) => k,
                None => return Ok(None),
            };
            match kind {
                EntityKind::Circle => Ok(Some(CI::Radius {
                    r: layout.radius(*entity)?,
                    target: *value,
                })),
                EntityKind::Arc => {
                    let (center, start) = layout.arc_center_start(*entity)?;
                    Ok(Some(CI::DistancePP {
                        p1: center,
                        p2: start,
                        d: *value,
                    }))
                }
                _ => Ok(None),
            }
        }

        Diameter { entity, value } => {
            if !value.is_finite() || *value < 0.0 {
                return Err(ValidationError::InvalidRadius(*value));
            }
            let kind = match entity_types.get(entity) {
                Some(k) => k,
                None => return Ok(None),
            };
            match kind {
                EntityKind::Circle => Ok(Some(CI::Diameter {
                    r: layout.radius(*entity)?,
                    target: *value,
                })),
                EntityKind::Arc => {
                    let (center, start) = layout.arc_center_start(*entity)?;
                    Ok(Some(CI::DistancePP {
                        p1: center,
                        p2: start,
                        d: *value / 2.0,
                    }))
                }
                _ => Ok(None),
            }
        }

        EqualAngle {
            line_a,
            line_b,
            line_c,
            line_d,
        } => Ok(Some(CI::EqualAngle {
            l1: layout.line(*line_a)?,
            l2: layout.line(*line_b)?,
            l3: layout.line(*line_c)?,
            l4: layout.line(*line_d)?,
        })),
        Ratio {
            entity_a,
            entity_b,
            value,
        } => {
            if !value.is_finite() {
                return Err(ValidationError::InvalidRatio(*value));
            }
            Ok(Some(CI::Ratio {
                l1: layout.line(*entity_a)?,
                l2: layout.line(*entity_b)?,
                k: *value,
            }))
        }
        EqualPointToLine {
            point_a,
            point_b,
            line,
        } => Ok(Some(CI::EqualPointToLine {
            p1: layout.point(*point_a)?,
            p2: layout.point(*point_b)?,
            line: layout.line(*line)?,
        })),
    }
}

use super::types::PointIdx;

/// Compute the sign of the cross product (point relative to line) from initial params.
/// Returns +1.0 or -1.0, used to fix the side for DistancePL constraints.
fn cross_sign(point: PointIdx, line: LineIdx, x0: &[f64]) -> f64 {
    let p = point.read(x0);
    let ls = line.start.read(x0);
    let ld = line.delta(x0);
    let vp = p - ls;
    let cross = vp.x * ld.y - vp.y * ld.x;
    if cross >= 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// Compute the sign of the cross product (center relative to line) from initial params.
/// Returns +1.0 or -1.0, used to fix the side for TangentLineCircle constraints.
fn cross_sign_center(center: PointIdx, line: LineIdx, x0: &[f64]) -> f64 {
    let c = center.read(x0);
    let ls = line.start.read(x0);
    let ld = line.delta(x0);
    let vc = c - ls;
    let cross = vc.x * ld.y - vc.y * ld.x;
    if cross >= 0.0 {
        1.0
    } else {
        -1.0
    }
}
