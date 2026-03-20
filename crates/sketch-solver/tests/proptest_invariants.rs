//! Property-based tests for structural invariants of the solver.

mod proptest_strategies;

use proptest::prelude::*;
use sketch_solver::core::builder::build_constraints;
use sketch_solver::core::constraint::ConstraintEq;
use sketch_solver::core::params::ParamLayout;
use sketch_solver::*;

// ── Residuals near zero after successful solve ──────────────────────────

fn max_residual(sketch: &Sketch, result: &SolvedSketch) -> f64 {
    let layout = ParamLayout::from_entities(&sketch.entities);
    let x0 = layout.initial_params(&sketch.entities);
    let constraints = build_constraints(&sketch.constraints, &sketch.entities, &layout, &x0);
    // Build param vector from solved positions
    let mut params = x0;
    for (id, &(x, y)) in &result.positions {
        // Find point index and write solved position
        for entity in &sketch.entities {
            if let SketchEntity::Point { id: eid, .. } = entity {
                if eid == id {
                    let idx = layout.point(*id);
                    params[idx.x()] = x;
                    params[idx.y()] = y;
                }
            }
        }
    }
    let mut max_r = 0.0f64;
    for c in &constraints {
        let m = c.num_equations();
        if m == 0 {
            continue;
        }
        let mut res = vec![0.0; m];
        c.residuals(&params, &mut res);
        for &r in &res {
            max_r = max_r.max(r.abs());
        }
    }
    max_r
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// After a successful solve, all constraint residuals should be near zero.
    #[test]
    fn proptest_residuals_near_zero_rectangle(
        (sketch, _, _) in proptest_strategies::arb_constrained_rectangle()
    ) {
        let result = solve_sketch(&sketch);
        prop_assert!(
            matches!(result.status, SolveStatus::FullyConstrained),
            "expected FullyConstrained"
        );
        let max_r = max_residual(&sketch, &result);
        prop_assert!(max_r < 1e-5, "max residual = {:.2e}", max_r);
    }

    #[test]
    fn proptest_residuals_near_zero_triangle(
        (sketch, _) in proptest_strategies::arb_constrained_triangle()
    ) {
        let result = solve_sketch(&sketch);
        prop_assert!(
            matches!(result.status, SolveStatus::FullyConstrained),
            "expected FullyConstrained"
        );
        let max_r = max_residual(&sketch, &result);
        // Weak springs introduce small bias (~1e-5 scale); solver tolerance is 1e-7 on scaled residuals
        prop_assert!(max_r < 1e-5, "max residual = {:.2e}", max_r);
    }

    // ── Idempotent solve ────────────────────────────────────────────────

    /// Solving twice (using solved positions as new starting positions)
    /// should produce the same result.
    #[test]
    fn proptest_idempotent_solve(
        (sketch, _, _) in proptest_strategies::arb_constrained_rectangle()
    ) {
        let result1 = solve_sketch(&sketch);
        prop_assert!(matches!(result1.status, SolveStatus::FullyConstrained));

        // Build a new sketch with solved positions as initial positions
        let mut sketch2 = sketch.clone();
        for e in &mut sketch2.entities {
            if let SketchEntity::Point { id, x, y, .. } = e {
                if let Some(&(sx, sy)) = result1.positions.get(id) {
                    *x = sx;
                    *y = sy;
                }
            }
        }

        let result2 = solve_sketch(&sketch2);
        prop_assert!(matches!(result2.status, SolveStatus::FullyConstrained));

        // Positions should match
        for (id, &(x1, y1)) in &result1.positions {
            if let Some(&(x2, y2)) = result2.positions.get(id) {
                prop_assert!(
                    (x1 - x2).abs() < 1e-8 && (y1 - y2).abs() < 1e-8,
                    "point {:?} differs: ({}, {}) vs ({}, {})", id, x1, y1, x2, y2
                );
            }
        }
    }

    // ── Under-constrained sketch has DOF > 0 ────────────────────────────

    #[test]
    fn proptest_underconstrained_has_dof(
        sketch in proptest_strategies::arb_underconstrained_line()
    ) {
        let result = solve_sketch(&sketch);
        match result.status {
            SolveStatus::UnderConstrained { dof } => {
                prop_assert!(dof > 0, "under-constrained should have dof > 0");
            }
            other => {
                prop_assert!(false, "expected UnderConstrained, got {:?}", other);
            }
        }
    }

    // ── Fully constrained → unique solution ─────────────────────────────

    /// Solve the same fully-constrained sketch from two different starting
    /// points. Both should converge to the same solution.
    /// Note: we only perturb non-dragged points since Dragged reads the
    /// initial position as its target.
    #[test]
    fn proptest_fully_constrained_unique(
        (sketch, _, _) in proptest_strategies::arb_constrained_rectangle()
    ) {
        let result1 = solve_sketch(&sketch);
        prop_assert!(matches!(result1.status, SolveStatus::FullyConstrained));

        // Find which points are dragged
        let dragged: std::collections::HashSet<PointId> = sketch.constraints.iter().filter_map(|c| {
            if let SketchConstraint::Dragged { point } = c { Some(*point) } else { None }
        }).collect();

        // Perturb starting positions of non-dragged points
        let mut sketch2 = sketch.clone();
        for (i, e) in sketch2.entities.iter_mut().enumerate() {
            if let SketchEntity::Point { id, x, y, .. } = e {
                if !dragged.contains(id) {
                    *x += (i as f64) * 5.0 - 10.0;
                    *y += (i as f64) * 3.0 - 6.0;
                }
            }
        }

        let result2 = solve_sketch(&sketch2);
        prop_assert!(matches!(result2.status, SolveStatus::FullyConstrained));

        for (id, &(x1, y1)) in &result1.positions {
            if let Some(&(x2, y2)) = result2.positions.get(id) {
                prop_assert!(
                    (x1 - x2).abs() < 1e-4 && (y1 - y2).abs() < 1e-4,
                    "point {:?} differs from different starting positions: ({}, {}) vs ({}, {})",
                    id, x1, y1, x2, y2
                );
            }
        }
    }
}
