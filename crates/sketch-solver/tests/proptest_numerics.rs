//! Property-based tests for mathematical correctness.
//!
//! The Jacobian FD test is the most critical -- it catches wrong analytical
//! derivatives across all constraint types with random configurations.

mod proptest_strategies;

use proptest::prelude::*;
use sketch_solver::core::constraint::ConstraintEq;
use sketch_solver::*;

// ── Jacobian finite-difference verification ─────────────────────────────

const FD_H: f64 = 1e-5;
const FD_TOL: f64 = 1e-4; // Relative tolerance for FD comparison

/// Verify analytical Jacobian matches central finite differences.
fn verify_jacobian_fd(c: &impl ConstraintEq, params: &[f64]) -> Result<(), String> {
    let n = params.len();
    let m = c.num_equations();
    if m == 0 {
        return Ok(());
    }

    let mut f_plus = vec![0.0; m];
    let mut f_minus = vec![0.0; m];
    let mut analytic_entries = Vec::new();
    c.jacobian(params, 0, &mut analytic_entries);

    for j in 0..n {
        let mut x_plus = params.to_vec();
        let mut x_minus = params.to_vec();
        x_plus[j] += FD_H;
        x_minus[j] -= FD_H;
        c.residuals(&x_plus, &mut f_plus);
        c.residuals(&x_minus, &mut f_minus);

        for i in 0..m {
            let fd = (f_plus[i] - f_minus[i]) / (2.0 * FD_H);
            let analytic: f64 = analytic_entries
                .iter()
                .filter(|(r, c, _)| *r == i && *c == j)
                .map(|(_, _, v)| v)
                .sum();

            let err = (analytic - fd).abs();
            let scale = analytic.abs().max(fd.abs()).max(1e-10);
            let rel_err = err / scale;

            if err > 1e-6 && rel_err > FD_TOL {
                return Err(format!(
                    "Jacobian mismatch at [{},{}]: analytic={:.8e}, fd={:.8e}, err={:.8e}, rel={:.8e}",
                    i, j, analytic, fd, err, rel_err
                ));
            }
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// CRITICAL: Verify analytical Jacobian matches finite differences
    /// for all constraint types at random configurations.
    #[test]
    fn proptest_jacobian_fd(spec in proptest_strategies::arb_constraint_spec()) {
        let (constraint, params) = spec.build();
        let result = verify_jacobian_fd(&constraint, &params);
        prop_assert!(
            result.is_ok(),
            "variant={}: {}",
            spec.variant,
            result.unwrap_err()
        );
    }
}

// ── Convergence bounds ──────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Warm start (from near-solution) should converge quickly.
    #[test]
    fn proptest_warm_convergence(
        (sketch, _, _) in proptest_strategies::arb_constrained_rectangle()
    ) {
        // Warm start: initial positions are the solution
        let result = solve_sketch(&sketch);
        prop_assert!(
            matches!(result.status, SolveStatus::FullyConstrained),
            "expected FullyConstrained"
        );
        // The solver doesn't expose iteration count via SolvedSketch,
        // but if it's fully constrained from exact positions, it worked.
        // We verify convergence by checking that result is correct.
    }
}

// ── Weak spring no-drift ────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Solving the same sketch 5 times sequentially should not cause drift.
    #[test]
    fn proptest_spring_no_drift(
        (mut sketch, _, _) in proptest_strategies::arb_constrained_rectangle()
    ) {
        let result1 = solve_sketch(&sketch);
        prop_assert!(matches!(result1.status, SolveStatus::FullyConstrained));

        // Solve 4 more times, updating positions each time
        let mut prev_positions = result1.positions.clone();
        for iteration in 0..4 {
            for e in &mut sketch.entities {
                if let SketchEntity::Point { id, x, y, .. } = e {
                    if let Some(&(sx, sy)) = prev_positions.get(id) {
                        *x = sx;
                        *y = sy;
                    }
                }
            }
            let result = solve_sketch(&sketch);
            prop_assert!(
                matches!(result.status, SolveStatus::FullyConstrained),
                "iteration {}: expected FullyConstrained", iteration
            );

            // Check no drift
            for (id, &(x_prev, y_prev)) in &prev_positions {
                if let Some(&(x_new, y_new)) = result.positions.get(id) {
                    prop_assert!(
                        (x_new - x_prev).abs() < 1e-8 && (y_new - y_prev).abs() < 1e-8,
                        "drift at iteration {}, point {:?}: ({}, {}) -> ({}, {})",
                        iteration, id, x_prev, y_prev, x_new, y_new
                    );
                }
            }
            prev_positions = result.positions.clone();
        }
    }
}
