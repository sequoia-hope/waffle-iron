use serde::{Deserialize, Serialize};

/// Index into the sketch's entity list.
pub type EntityIndex = usize;

/// A 2D geometric constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Constraint {
    /// Two points are at the same location.
    Coincident {
        point_a: EntityIndex,
        point_b: EntityIndex,
    },
    /// A point lies on a line/curve.
    PointOnEntity {
        point: EntityIndex,
        entity: EntityIndex,
    },
    /// Two lines are parallel.
    Parallel {
        line_a: EntityIndex,
        line_b: EntityIndex,
    },
    /// Two lines are perpendicular.
    Perpendicular {
        line_a: EntityIndex,
        line_b: EntityIndex,
    },
    /// A line is horizontal (parallel to X axis).
    Horizontal { line: EntityIndex },
    /// A line is vertical (parallel to Y axis).
    Vertical { line: EntityIndex },
    /// Two entities have equal length/radius.
    Equal {
        entity_a: EntityIndex,
        entity_b: EntityIndex,
    },
    /// Two entities are tangent.
    Tangent {
        entity_a: EntityIndex,
        entity_b: EntityIndex,
    },
    /// Symmetric about a line.
    Symmetric {
        point_a: EntityIndex,
        point_b: EntityIndex,
        axis: EntityIndex,
    },
    /// Fixed distance between two points.
    Distance {
        point_a: EntityIndex,
        point_b: EntityIndex,
        value: f64,
    },
    /// Fixed angle between two lines.
    Angle {
        line_a: EntityIndex,
        line_b: EntityIndex,
        value: f64,
    },
    /// Fixed radius for a circle/arc.
    Radius {
        entity: EntityIndex,
        value: f64,
    },
    /// Point is fixed at a specific position.
    Fixed {
        point: EntityIndex,
        x: f64,
        y: f64,
    },
}

impl Constraint {
    /// Compute the residual for this constraint given current parameter values.
    /// Returns 0.0 when the constraint is perfectly satisfied.
    pub fn residual(&self, params: &[f64], entities: &[SketchEntity]) -> f64 {
        match self {
            Constraint::Coincident { point_a, point_b } => {
                let (ax, ay) = entity_point(entities, *point_a, params);
                let (bx, by) = entity_point(entities, *point_b, params);
                let dx = ax - bx;
                let dy = ay - by;
                dx * dx + dy * dy
            }
            Constraint::Distance {
                point_a,
                point_b,
                value,
            } => {
                let (ax, ay) = entity_point(entities, *point_a, params);
                let (bx, by) = entity_point(entities, *point_b, params);
                let dist = ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt();
                (dist - value).powi(2)
            }
            Constraint::Horizontal { line } => {
                if let SketchEntity::Line {
                    start_param,
                    end_param,
                } = &entities[*line]
                {
                    let y1 = params[start_param + 1];
                    let y2 = params[end_param + 1];
                    (y1 - y2).powi(2)
                } else {
                    0.0
                }
            }
            Constraint::Vertical { line } => {
                if let SketchEntity::Line {
                    start_param,
                    end_param,
                } = &entities[*line]
                {
                    let x1 = params[*start_param];
                    let x2 = params[*end_param];
                    (x1 - x2).powi(2)
                } else {
                    0.0
                }
            }
            Constraint::Fixed { point, x, y } => {
                let (px, py) = entity_point(entities, *point, params);
                (px - x).powi(2) + (py - y).powi(2)
            }
            Constraint::Radius { entity, value } => {
                if let SketchEntity::Circle { center_param: _, radius_param } = &entities[*entity] {
                    (params[*radius_param] - value).powi(2)
                } else {
                    0.0
                }
            }
            Constraint::Parallel { line_a, line_b } => {
                let (dx_a, dy_a) = line_direction(entities, *line_a, params);
                let (dx_b, dy_b) = line_direction(entities, *line_b, params);
                // Cross product should be zero for parallel
                (dx_a * dy_b - dy_a * dx_b).powi(2)
            }
            Constraint::Perpendicular { line_a, line_b } => {
                let (dx_a, dy_a) = line_direction(entities, *line_a, params);
                let (dx_b, dy_b) = line_direction(entities, *line_b, params);
                // Dot product should be zero for perpendicular
                (dx_a * dx_b + dy_a * dy_b).powi(2)
            }
            _ => 0.0, // TODO: implement remaining constraints
        }
    }
}

/// A sketch entity (geometric element in 2D).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SketchEntity {
    /// A point: params[param_index] = x, params[param_index+1] = y
    Point { param_index: usize },
    /// A line segment: defined by two point parameter offsets
    Line {
        start_param: usize,
        end_param: usize,
    },
    /// A circle: center + radius
    Circle {
        center_param: usize,
        radius_param: usize,
    },
    /// An arc: center + radius + start_angle + end_angle
    Arc {
        center_param: usize,
        radius_param: usize,
        start_angle_param: usize,
        end_angle_param: usize,
    },
}

fn entity_point(entities: &[SketchEntity], idx: usize, params: &[f64]) -> (f64, f64) {
    match &entities[idx] {
        SketchEntity::Point { param_index } => (params[*param_index], params[param_index + 1]),
        SketchEntity::Circle { center_param, .. } => {
            (params[*center_param], params[center_param + 1])
        }
        _ => (0.0, 0.0),
    }
}

fn line_direction(entities: &[SketchEntity], idx: usize, params: &[f64]) -> (f64, f64) {
    if let SketchEntity::Line {
        start_param,
        end_param,
    } = &entities[idx]
    {
        let dx = params[*end_param] - params[*start_param];
        let dy = params[end_param + 1] - params[start_param + 1];
        (dx, dy)
    } else {
        (1.0, 0.0)
    }
}
