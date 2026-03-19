//! Constraint builder: maps `SketchConstraint` → `ConstraintImpl`.
//!
//! Resolves entity IDs to typed indices using `ParamLayout` at build time.
//! The solver never touches entity IDs after this step.

use std::collections::HashMap;

use nalgebra::Point2;

use crate::types::{EntityKind, SketchConstraint, SketchEntity};

use super::constraint::ConstraintImpl;
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
) -> Vec<ConstraintImpl> {
    let entity_types = classify_entities(entities);
    constraints
        .iter()
        .filter_map(|c| build_one(c, &entity_types, entities, layout))
        .collect()
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

fn classify_entities(entities: &[SketchEntity]) -> HashMap<u32, EntityKind> {
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

fn find_point_pos(entities: &[SketchEntity], point_id: u32) -> (f64, f64) {
    for e in entities {
        if e.id() == point_id {
            if let SketchEntity::Point { x, y, .. } = e {
                return (*x, *y);
            }
        }
    }
    panic!("Point {} not found", point_id)
}

fn find_center_id(entities: &[SketchEntity], entity_id: u32) -> u32 {
    for e in entities {
        if e.id() == entity_id {
            match e {
                SketchEntity::Circle { center_id, .. } => return *center_id,
                SketchEntity::Arc { center_id, .. } => return *center_id,
                _ => panic!("Entity {} is not a circle or arc", entity_id),
            }
        }
    }
    panic!("Entity {} not found", entity_id)
}

fn build_one(
    constraint: &SketchConstraint,
    entity_types: &HashMap<u32, EntityKind>,
    entities: &[SketchEntity],
    layout: &ParamLayout,
) -> Option<ConstraintImpl> {
    use ConstraintImpl as CI;
    use SketchConstraint::*;

    match constraint {
        Coincident { point_a, point_b } => Some(CI::Coincident {
            p1: layout.point(*point_a),
            p2: layout.point(*point_b),
        }),
        Horizontal { entity } => Some(CI::Horizontal {
            line: layout.line(*entity),
        }),
        Vertical { entity } => Some(CI::Vertical {
            line: layout.line(*entity),
        }),
        Parallel { line_a, line_b } => Some(CI::Parallel {
            l1: layout.line(*line_a),
            l2: layout.line(*line_b),
        }),
        Perpendicular { line_a, line_b } => Some(CI::Perpendicular {
            l1: layout.line(*line_a),
            l2: layout.line(*line_b),
        }),
        SymmetricH { point_a, point_b } => Some(CI::SymmetricH {
            p1: layout.point(*point_a),
            p2: layout.point(*point_b),
        }),
        SymmetricV { point_a, point_b } => Some(CI::SymmetricV {
            p1: layout.point(*point_a),
            p2: layout.point(*point_b),
        }),
        Symmetric {
            entity_a,
            entity_b,
            symmetry_line,
        } => Some(CI::SymmetricLine {
            p1: layout.point(*entity_a),
            p2: layout.point(*entity_b),
            line: layout.line(*symmetry_line),
        }),
        Midpoint { point, line } => Some(CI::Midpoint {
            point: layout.point(*point),
            line: layout.line(*line),
        }),
        Dragged { point } => {
            let (x, y) = find_point_pos(entities, *point);
            Some(CI::Dragged {
                point: layout.point(*point),
                target: Point2::new(x, y),
            })
        }
        Angle {
            line_a,
            line_b,
            value_degrees,
        } => Some(CI::Angle {
            l1: layout.line(*line_a),
            l2: layout.line(*line_b),
            value_rad: value_degrees.to_radians(),
        }),
        SameOrientation { .. } => Some(CI::SameOrientation),

        Equal { entity_a, entity_b } => {
            let kind_a = entity_types.get(entity_a)?;
            let kind_b = entity_types.get(entity_b)?;
            match (kind_a, kind_b) {
                (EntityKind::Line, EntityKind::Line) => Some(CI::EqualLength {
                    l1: layout.line(*entity_a),
                    l2: layout.line(*entity_b),
                }),
                (EntityKind::Circle, EntityKind::Circle) => Some(CI::EqualRadius {
                    r1: layout.radius(*entity_a),
                    r2: layout.radius(*entity_b),
                }),
                (EntityKind::Arc, EntityKind::Arc) => {
                    let (c1, s1) = layout.arc_center_start(*entity_a);
                    let (c2, s2) = layout.arc_center_start(*entity_b);
                    Some(CI::EqualLength {
                        l1: LineIdx { start: c1, end: s1 },
                        l2: LineIdx { start: c2, end: s2 },
                    })
                }
                (EntityKind::Circle, EntityKind::Arc) => {
                    let (center, start) = layout.arc_center_start(*entity_b);
                    Some(CI::OnCircle {
                        point: start,
                        center,
                        radius: RadiusDef::Param(layout.radius(*entity_a)),
                    })
                }
                (EntityKind::Arc, EntityKind::Circle) => {
                    let (center, start) = layout.arc_center_start(*entity_a);
                    Some(CI::OnCircle {
                        point: start,
                        center,
                        radius: RadiusDef::Param(layout.radius(*entity_b)),
                    })
                }
                _ => None,
            }
        }

        Distance {
            entity_a,
            entity_b,
            value,
        } => {
            let kind_a = entity_types.get(entity_a)?;
            let kind_b = entity_types.get(entity_b)?;
            match (kind_a, kind_b) {
                (EntityKind::Point, EntityKind::Point) => Some(CI::DistancePP {
                    p1: layout.point(*entity_a),
                    p2: layout.point(*entity_b),
                    d: *value,
                }),
                (EntityKind::Point, EntityKind::Line) => Some(CI::DistancePL {
                    point: layout.point(*entity_a),
                    line: layout.line(*entity_b),
                    d: *value,
                }),
                (EntityKind::Line, EntityKind::Point) => Some(CI::DistancePL {
                    point: layout.point(*entity_b),
                    line: layout.line(*entity_a),
                    d: *value,
                }),
                _ => None,
            }
        }

        OnEntity { point, entity } => {
            let kind = entity_types.get(entity)?;
            match kind {
                EntityKind::Line => Some(CI::OnLine {
                    point: layout.point(*point),
                    line: layout.line(*entity),
                }),
                EntityKind::Circle => {
                    let center_id = find_center_id(entities, *entity);
                    Some(CI::OnCircle {
                        point: layout.point(*point),
                        center: layout.point(center_id),
                        radius: layout.radius_def(*entity),
                    })
                }
                EntityKind::Arc => {
                    let (center, _start) = layout.arc_center_start(*entity);
                    Some(CI::OnCircle {
                        point: layout.point(*point),
                        center,
                        radius: layout.radius_def(*entity),
                    })
                }
                _ => None,
            }
        }

        Tangent { line, curve } => {
            let kind = entity_types.get(curve)?;
            match kind {
                EntityKind::Arc => {
                    let (center, _start) = layout.arc_center_start(*curve);
                    Some(CI::TangentLineCircle {
                        line: layout.line(*line),
                        center,
                        radius: layout.radius_def(*curve),
                    })
                }
                EntityKind::Circle => {
                    let center_id = find_center_id(entities, *curve);
                    Some(CI::TangentLineCircle {
                        line: layout.line(*line),
                        center: layout.point(center_id),
                        radius: layout.radius_def(*curve),
                    })
                }
                _ => None,
            }
        }

        Radius { entity, value } => {
            let kind = entity_types.get(entity)?;
            match kind {
                EntityKind::Circle => Some(CI::Radius {
                    r: layout.radius(*entity),
                    target: *value,
                }),
                EntityKind::Arc => {
                    let (center, start) = layout.arc_center_start(*entity);
                    Some(CI::DistancePP {
                        p1: center,
                        p2: start,
                        d: *value,
                    })
                }
                _ => None,
            }
        }

        Diameter { entity, value } => {
            let kind = entity_types.get(entity)?;
            match kind {
                EntityKind::Circle => Some(CI::Diameter {
                    r: layout.radius(*entity),
                    target: *value,
                }),
                EntityKind::Arc => {
                    let (center, start) = layout.arc_center_start(*entity);
                    Some(CI::DistancePP {
                        p1: center,
                        p2: start,
                        d: *value / 2.0,
                    })
                }
                _ => None,
            }
        }

        EqualAngle {
            line_a,
            line_b,
            line_c,
            line_d,
        } => Some(CI::EqualAngle {
            l1: layout.line(*line_a),
            l2: layout.line(*line_b),
            l3: layout.line(*line_c),
            l4: layout.line(*line_d),
        }),
        Ratio {
            entity_a,
            entity_b,
            value,
        } => Some(CI::Ratio {
            l1: layout.line(*entity_a),
            l2: layout.line(*entity_b),
            k: *value,
        }),
        EqualPointToLine {
            point_a,
            point_b,
            line,
        } => Some(CI::EqualPointToLine {
            p1: layout.point(*point_a),
            p2: layout.point(*point_b),
            line: layout.line(*line),
        }),
    }
}
