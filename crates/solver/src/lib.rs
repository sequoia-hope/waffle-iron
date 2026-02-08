pub mod constraint;
pub mod sketch;
pub mod solver;

pub use constraint::*;
pub use sketch::*;
pub use solver::*;

/// Compute the degrees of freedom of a sketch at its current parameter state.
///
/// Uses SVD-based rank analysis of the Jacobian matrix.
/// DOF = num_params - rank(Jacobian).
/// Returns 0 for a fully constrained sketch.
pub fn degrees_of_freedom(sketch: &Sketch) -> usize {
    solver::degrees_of_freedom(sketch)
}

// ── SketchSolver Trait ─────────────────────────────────────────────────────

/// Trait abstracting the 2D sketch constraint solver.
///
/// This enables mock solver implementations for testing and allows
/// alternative solver backends to be swapped in.
pub trait SketchSolver {
    /// Solve the constraints on a sketch, mutating its parameters in-place.
    /// Uses the implementation's default configuration.
    fn solve(&self, sketch: &mut Sketch) -> Result<SolverResult, SolverError>;

    /// Compute the degrees of freedom of the sketch at its current state.
    fn dof(&self, sketch: &Sketch) -> usize;
}

/// The default solver implementation using Gauss-Newton with LM damping.
pub struct DefaultSketchSolver {
    config: SolverConfig,
}

impl DefaultSketchSolver {
    /// Create a new solver with default configuration.
    pub fn new() -> Self {
        Self {
            config: SolverConfig::default(),
        }
    }

    /// Create a solver with custom configuration.
    pub fn with_config(config: SolverConfig) -> Self {
        Self { config }
    }
}

impl Default for DefaultSketchSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl SketchSolver for DefaultSketchSolver {
    fn solve(&self, sketch: &mut Sketch) -> Result<SolverResult, SolverError> {
        solve_sketch(sketch, &self.config)
    }

    fn dof(&self, sketch: &Sketch) -> usize {
        degrees_of_freedom(sketch)
    }
}

#[cfg(test)]
mod trait_tests {
    use super::*;

    #[test]
    fn test_sketch_solver_trait_solve() {
        let solver = DefaultSketchSolver::new();
        let mut sketch = Sketch::new();
        let p1 = sketch.add_point(0.0, 0.0);
        let p2 = sketch.add_point(10.0, 5.0);
        let line = sketch.add_line(p1, p2);

        sketch.add_constraint(Constraint::Fixed { point: p1, x: 0.0, y: 0.0 });
        sketch.add_constraint(Constraint::Horizontal { line });

        let result = solver.solve(&mut sketch);
        assert!(result.is_ok());
        assert!(result.unwrap().converged);
    }

    #[test]
    fn test_sketch_solver_trait_dof() {
        let solver = DefaultSketchSolver::new();
        let mut sketch = Sketch::new();
        let p = sketch.add_point(1.0, 2.0);

        // Unconstrained: 2 DOF
        assert_eq!(solver.dof(&sketch), 2);

        // Fixed: 0 DOF
        sketch.add_constraint(Constraint::Fixed { point: p, x: 1.0, y: 2.0 });
        assert_eq!(solver.dof(&sketch), 0);
    }

    #[test]
    fn test_sketch_solver_with_custom_config() {
        let config = SolverConfig {
            max_iterations: 200,
            ..SolverConfig::default()
        };
        let solver = DefaultSketchSolver::with_config(config);
        let mut sketch = Sketch::new();
        let p = sketch.add_point(1.0, 2.0);
        sketch.add_constraint(Constraint::Fixed { point: p, x: 0.0, y: 0.0 });
        let result = solver.solve(&mut sketch);
        assert!(result.is_ok());
        assert!(result.unwrap().converged);
    }
}
