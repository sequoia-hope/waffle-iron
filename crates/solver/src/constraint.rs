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
            Constraint::PointOnEntity { point, entity } => {
                let (px, py) = entity_point(entities, *point, params);
                match &entities[*entity] {
                    SketchEntity::Line { start_param, end_param } => {
                        let ax = params[*start_param];
                        let ay = params[start_param + 1];
                        let bx = params[*end_param];
                        let by = params[end_param + 1];
                        let cross = (px - ax) * (by - ay) - (py - ay) * (bx - ax);
                        cross * cross
                    }
                    SketchEntity::Circle { center_param, radius_param } => {
                        let cx = params[*center_param];
                        let cy = params[center_param + 1];
                        let r = params[*radius_param];
                        let dist_sq = (px - cx).powi(2) + (py - cy).powi(2);
                        (dist_sq - r * r).powi(2)
                    }
                    _ => 0.0,
                }
            }
            Constraint::Equal { entity_a, entity_b } => {
                let len_a = entity_length(entities, *entity_a, params);
                let len_b = entity_length(entities, *entity_b, params);
                (len_a - len_b).powi(2)
            }
            Constraint::Symmetric { point_a, point_b, axis } => {
                let (ax, ay) = entity_point(entities, *point_a, params);
                let (bx, by) = entity_point(entities, *point_b, params);
                if let SketchEntity::Line { start_param, end_param } = &entities[*axis] {
                    let lx0 = params[*start_param];
                    let ly0 = params[start_param + 1];
                    let lx1 = params[*end_param];
                    let ly1 = params[end_param + 1];
                    let dx = lx1 - lx0;
                    let dy = ly1 - ly0;
                    let len_sq = dx * dx + dy * dy;
                    if len_sq > 1e-20 {
                        let mx = (ax + bx) / 2.0;
                        let my = (ay + by) / 2.0;
                        let cross = (mx - lx0) * dy - (my - ly0) * dx;
                        let dot = (bx - ax) * dx + (by - ay) * dy;
                        cross * cross + dot * dot
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            }
            Constraint::Angle { line_a, line_b, value } => {
                let (dx_a, dy_a) = line_direction(entities, *line_a, params);
                let (dx_b, dy_b) = line_direction(entities, *line_b, params);
                let cross = dx_a * dy_b - dy_a * dx_b;
                let dot = dx_a * dx_b + dy_a * dy_b;
                let r = cross - dot * value.tan();
                r * r
            }
            Constraint::Tangent { entity_a, entity_b } => {
                tangent_residual_sq(entities, *entity_a, *entity_b, params)
            }
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

fn entity_length(entities: &[SketchEntity], idx: usize, params: &[f64]) -> f64 {
    match &entities[idx] {
        SketchEntity::Line { start_param, end_param } => {
            let dx = params[*end_param] - params[*start_param];
            let dy = params[end_param + 1] - params[start_param + 1];
            (dx * dx + dy * dy).sqrt()
        }
        SketchEntity::Circle { radius_param, .. } => params[*radius_param],
        _ => 0.0,
    }
}

/// Compute squared residual for tangent constraint between two entities.
/// Line-Circle tangent: distance from circle center to line equals radius.
/// Circle-Circle tangent: distance between centers equals sum of radii.
fn tangent_residual_sq(entities: &[SketchEntity], a: usize, b: usize, params: &[f64]) -> f64 {
    match (&entities[a], &entities[b]) {
        (SketchEntity::Line { start_param, end_param }, SketchEntity::Circle { center_param, radius_param }) |
        (SketchEntity::Circle { center_param, radius_param }, SketchEntity::Line { start_param, end_param }) => {
            let ax = params[*start_param];
            let ay = params[start_param + 1];
            let bx = params[*end_param];
            let by = params[end_param + 1];
            let cx = params[*center_param];
            let cy = params[center_param + 1];
            let r = params[*radius_param];
            let dx = bx - ax;
            let dy = by - ay;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-15 { return r * r; }
            // Signed distance from center to line
            let dist = ((cx - ax) * dy - (cy - ay) * dx).abs() / len;
            (dist - r).powi(2)
        }
        (SketchEntity::Circle { center_param: ca, radius_param: ra }, SketchEntity::Circle { center_param: cb, radius_param: rb }) => {
            let ax = params[*ca];
            let ay = params[ca + 1];
            let bx = params[*cb];
            let by = params[cb + 1];
            let r_a = params[*ra];
            let r_b = params[*rb];
            let dist = ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt();
            // External tangency: dist = ra + rb
            // Also check internal tangency and pick the closer one
            let external = (dist - (r_a + r_b)).powi(2);
            let internal = (dist - (r_a - r_b).abs()).powi(2);
            external.min(internal)
        }
        _ => 0.0,
    }
}
