use crate::sketch::Sketch;
use thiserror::Error;

/// Result of running the constraint solver.
#[derive(Debug)]
pub struct SolverResult {
    pub converged: bool,
    pub iterations: usize,
    pub final_residual: f64,
    pub params: Vec<f64>,
}

#[derive(Debug, Error)]
pub enum SolverError {
    #[error("Solver did not converge after {max_iterations} iterations (residual: {residual})")]
    DidNotConverge {
        max_iterations: usize,
        residual: f64,
    },
    #[error("Under-constrained system: {dof} degrees of freedom remain")]
    UnderConstrained { dof: i64 },
    #[error("Over-constrained system: constraints are contradictory")]
    OverConstrained,
}

/// Configuration for the Levenberg-Marquardt solver.
#[derive(Debug, Clone)]
pub struct SolverConfig {
    pub max_iterations: usize,
    pub tolerance: f64,
    pub lambda_initial: f64,
    pub lambda_factor: f64,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            tolerance: 1e-10,
            lambda_initial: 1e-3,
            lambda_factor: 10.0,
        }
    }
}

/// Solve the constraints in a sketch using gradient descent (simplified Levenberg-Marquardt).
pub fn solve_sketch(sketch: &mut Sketch, config: &SolverConfig) -> Result<SolverResult, SolverError> {
    let mut params = sketch.params.clone();
    let n = params.len();

    if n == 0 || sketch.constraints.is_empty() {
        return Ok(SolverResult {
            converged: true,
            iterations: 0,
            final_residual: 0.0,
            params: params.clone(),
        });
    }

    let mut lambda = config.lambda_initial;

    for iteration in 0..config.max_iterations {
        let residual = compute_total_residual(sketch, &params);

        if residual < config.tolerance {
            sketch.params = params.clone();
            return Ok(SolverResult {
                converged: true,
                iterations: iteration,
                final_residual: residual,
                params,
            });
        }

        // Compute gradient via finite differences
        let mut gradient = vec![0.0; n];
        let h = 1e-8;

        for i in 0..n {
            let orig = params[i];
            params[i] = orig + h;
            let r_plus = compute_total_residual(sketch, &params);
            params[i] = orig - h;
            let r_minus = compute_total_residual(sketch, &params);
            params[i] = orig;
            gradient[i] = (r_plus - r_minus) / (2.0 * h);
        }

        // Gradient descent with backtracking line search
        let grad_norm_sq: f64 = gradient.iter().map(|g| g * g).sum();
        if grad_norm_sq < 1e-20 {
            break; // gradient too small
        }

        // Normalize gradient and do line search
        let grad_norm = grad_norm_sq.sqrt();
        let mut step_size = residual / (grad_norm_sq + lambda * grad_norm);

        // Backtracking line search
        let mut found_better = false;
        for _ in 0..20 {
            let mut new_params = params.clone();
            for i in 0..n {
                new_params[i] -= step_size * gradient[i];
            }

            let new_residual = compute_total_residual(sketch, &new_params);

            if new_residual < residual {
                params = new_params;
                lambda = (lambda / config.lambda_factor).max(1e-10);
                found_better = true;
                break;
            }
            step_size *= 0.5;
        }

        if !found_better {
            lambda *= config.lambda_factor;
        }
    }

    let final_residual = compute_total_residual(sketch, &params);
    sketch.params = params.clone();

    if final_residual < config.tolerance {
        Ok(SolverResult {
            converged: true,
            iterations: config.max_iterations,
            final_residual,
            params,
        })
    } else {
        Err(SolverError::DidNotConverge {
            max_iterations: config.max_iterations,
            residual: final_residual,
        })
    }
}

fn compute_total_residual(sketch: &Sketch, params: &[f64]) -> f64 {
    sketch
        .constraints
        .iter()
        .map(|c| c.residual(params, &sketch.entities))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::Constraint;
    use crate::sketch::Sketch;

    #[test]
    fn test_solve_horizontal_constraint() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 5.0); // Not horizontal initially
        let line = sketch.add_line(p1, p2);

        sketch.add_constraint(Constraint::Fixed {
            point: p1,
            x: 0.0,
            y: 0.0,
        });
        sketch.add_constraint(Constraint::Horizontal { line });

        let config = SolverConfig {
            max_iterations: 200,
            ..SolverConfig::default()
        };
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver failed: {:?}", result.err());

        let result = result.unwrap();
        assert!(result.converged);
        assert!(result.final_residual < 1e-8);

        // Check that the line is now horizontal (same y values)
        let (_, y1) = sketch.point_position(p1);
        let (_, y2) = sketch.point_position(p2);
        assert!(
            (y1 - y2).abs() < 1e-4,
            "Line not horizontal: y1={}, y2={}",
            y1,
            y2
        );
    }

    #[test]
    fn test_solve_distance_constraint() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(3.0, 4.0);

        sketch.add_constraint(Constraint::Fixed {
            point: p1,
            x: 0.0,
            y: 0.0,
        });
        sketch.add_constraint(Constraint::Distance {
            point_a: p1,
            point_b: p2,
            value: 10.0,
        });

        let config = SolverConfig {
            max_iterations: 500,
            tolerance: 1e-6,
            ..SolverConfig::default()
        };
        let result = solve_sketch(&mut sketch, &config);
        assert!(result.is_ok(), "Solver error: {:?}", result.err());

        let (x2, y2) = sketch.point_position(p2);
        let dist = (x2 * x2 + y2 * y2).sqrt();
        assert!(
            (dist - 10.0).abs() < 0.1,
            "Distance is {} instead of 10.0",
            dist
        );
    }

    #[test]
    fn test_solve_already_satisfied() {
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 0.0);
        let line = sketch.add_line(p1, p2);

        sketch.add_constraint(Constraint::Horizontal { line });

        let config = SolverConfig::default();
        let result = solve_sketch(&mut sketch, &config).unwrap();
        assert!(result.converged);
        assert_eq!(result.iterations, 0);
    }
}
