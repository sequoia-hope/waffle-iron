use serde::{Deserialize, Serialize};

use crate::constraint::{Constraint, SketchEntity};

/// A 2D sketch containing geometric entities and constraints.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sketch {
    /// Geometric entities (points, lines, circles, arcs).
    pub entities: Vec<SketchEntity>,
    /// Constraints between entities.
    pub constraints: Vec<Constraint>,
    /// Parameter values [x0, y0, x1, y1, ...].
    pub params: Vec<f64>,
    /// Number of parameters that are free (not fixed by constraints).
    param_count: usize,
}

impl Sketch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a free point to the sketch, returning its entity index.
    pub fn add_point(&mut self, x: f64, y: f64) -> usize {
        let param_idx = self.params.len();
        self.params.push(x);
        self.params.push(y);
        self.param_count += 2;
        let entity_idx = self.entities.len();
        self.entities.push(SketchEntity::Point {
            param_index: param_idx,
        });
        entity_idx
    }

    /// Add a line segment between two points, returning the line's entity index.
    /// `start` and `end` are entity indices returned by `add_point`.
    pub fn add_line(&mut self, start: usize, end: usize) -> usize {
        let start_param = match &self.entities[start] {
            SketchEntity::Point { param_index } => *param_index,
            _ => panic!("Start entity is not a point"),
        };
        let end_param = match &self.entities[end] {
            SketchEntity::Point { param_index } => *param_index,
            _ => panic!("End entity is not a point"),
        };
        let entity_idx = self.entities.len();
        self.entities.push(SketchEntity::Line {
            start_param,
            end_param,
        });
        entity_idx
    }

    /// Add a circle, returning its entity index.
    pub fn add_circle(&mut self, cx: f64, cy: f64, radius: f64) -> usize {
        let center_param = self.params.len();
        self.params.push(cx);
        self.params.push(cy);
        let radius_param = self.params.len();
        self.params.push(radius);
        self.param_count += 3;
        let entity_idx = self.entities.len();
        self.entities.push(SketchEntity::Circle {
            center_param,
            radius_param,
        });
        entity_idx
    }

    /// Add a constraint.
    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    /// Compute the total residual (sum of all constraint residuals).
    pub fn total_residual(&self) -> f64 {
        self.constraints
            .iter()
            .map(|c| c.residual(&self.params, &self.entities))
            .sum()
    }

    /// Compute degrees of freedom: parameters minus constraints.
    /// This is an approximation — actual DOF requires Jacobian rank analysis.
    pub fn approximate_dof(&self) -> i64 {
        self.param_count as i64 - self.constraints.len() as i64
    }

    /// Get the current position of a point entity.
    pub fn point_position(&self, entity_idx: usize) -> (f64, f64) {
        match &self.entities[entity_idx] {
            SketchEntity::Point { param_index } => {
                (self.params[*param_index], self.params[param_index + 1])
            }
            _ => panic!("Entity is not a point"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sketch_add_point() {
        let mut sketch = Sketch::new();
        let p = sketch.add_point(5.0, 10.0);
        let (x, y) = sketch.point_position(p);
        assert!((x - 5.0).abs() < 1e-12);
        assert!((y - 10.0).abs() < 1e-12);
    }

    #[test]
    fn test_sketch_add_line() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 0.0);
        let _line = sketch.add_line(p1, p2);
        assert_eq!(sketch.entities.len(), 3); // 2 points + 1 line
    }

    #[test]
    fn test_constraint_residual_satisfied() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 0.0);
        let line = sketch.add_line(p1, p2);

        sketch.add_constraint(Constraint::Horizontal { line });
        assert!(sketch.total_residual() < 1e-12);
    }

    #[test]
    fn test_constraint_residual_violated() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 5.0); // Not horizontal
        let line = sketch.add_line(p1, p2);

        sketch.add_constraint(Constraint::Horizontal { line });
        assert!(sketch.total_residual() > 1e-6);
    }

    #[test]
    fn test_dof_counting() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0); // 2 DOF
        let _p2 = sketch.add_point(10.0, 0.0); // 2 DOF
        // Total: 4 DOF, 0 constraints
        assert_eq!(sketch.approximate_dof(), 4);

        sketch.add_constraint(Constraint::Fixed {
            point: p1,
            x: 0.0,
            y: 0.0,
        });
        // Now 4 - 1 = 3 (approximate; Fixed removes 2 DOF in reality)
        assert_eq!(sketch.approximate_dof(), 3);
    }
}
