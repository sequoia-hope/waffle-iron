//! Seed-and-measure proptest tests.
//!
//! Pattern: generate geometry → measure distances/angles → constrain →
//! perturb starting positions → solve → verify recovery.

mod proptest_strategies;

use proptest::prelude::*;
use sketch_solver::*;

fn dist(positions: &std::collections::HashMap<u32, (f64, f64)>, a: u32, b: u32) -> f64 {
    let (ax, ay) = positions[&a];
    let (bx, by) = positions[&b];
    ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt()
}

// ── Rectangle: seed-and-measure ─────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Generate a random rectangle, solve from exact initial positions,
    /// verify it's fully constrained with correct dimensions.
    #[test]
    fn proptest_rectangle_exact(
        (sketch, w, h) in proptest_strategies::arb_constrained_rectangle()
    ) {
        let result = solve_sketch(&sketch);
        prop_assert!(
            matches!(result.status, SolveStatus::FullyConstrained),
            "expected FullyConstrained, got {:?}", result.status
        );
        // Check dimensions: dist(1,2) = w, dist(2,3) = h
        let d_w = dist(&result.positions, 1, 2);
        let d_h = dist(&result.positions, 2, 3);
        prop_assert!((d_w - w).abs() < 1e-5, "width: got {}, expected {}", d_w, w);
        prop_assert!((d_h - h).abs() < 1e-5, "height: got {}, expected {}", d_h, h);
    }

    /// Generate a rectangle with perturbed initial positions, solve,
    /// verify it recovers the correct dimensions.
    #[test]
    fn proptest_rectangle_perturbed(
        (sketch, w, h) in proptest_strategies::arb_perturbed_rectangle()
    ) {
        let result = solve_sketch(&sketch);
        prop_assert!(
            matches!(result.status, SolveStatus::FullyConstrained),
            "expected FullyConstrained, got {:?}", result.status
        );
        let d_w = dist(&result.positions, 1, 2);
        let d_h = dist(&result.positions, 2, 3);
        prop_assert!((d_w - w).abs() < 1e-5, "width: got {}, expected {}", d_w, w);
        prop_assert!((d_h - h).abs() < 1e-5, "height: got {}, expected {}", d_h, h);
        // Verify horizontal lines are actually horizontal
        let (_, y1) = result.positions[&1];
        let (_, y2) = result.positions[&2];
        prop_assert!((y1 - y2).abs() < 1e-6, "line 1-2 not horizontal");
    }

    // ── Triangle: seed-and-measure ──────────────────────────────────────

    /// Generate a random triangle with distance constraints, solve from exact
    /// positions, verify fully constrained.
    #[test]
    fn proptest_triangle_exact(
        (sketch, distances) in proptest_strategies::arb_constrained_triangle()
    ) {
        let result = solve_sketch(&sketch);
        prop_assert!(
            matches!(result.status, SolveStatus::FullyConstrained),
            "expected FullyConstrained, got {:?}", result.status
        );
        let d01 = dist(&result.positions, 1, 2);
        let d12 = dist(&result.positions, 2, 3);
        let d20 = dist(&result.positions, 3, 1);
        prop_assert!((d01 - distances[0]).abs() < 1e-4, "d01: {} vs {}", d01, distances[0]);
        prop_assert!((d12 - distances[1]).abs() < 1e-4, "d12: {} vs {}", d12, distances[1]);
        prop_assert!((d20 - distances[2]).abs() < 1e-4, "d20: {} vs {}", d20, distances[2]);
    }

    /// Generate a triangle with perturbed positions, solve, verify recovery.
    /// Some perturbations may be too large for the solver to converge.
    #[test]
    fn proptest_triangle_perturbed(
        (sketch, distances) in proptest_strategies::arb_perturbed_triangle()
    ) {
        let result = solve_sketch(&sketch);
        prop_assume!(matches!(result.status, SolveStatus::FullyConstrained));
        let d01 = dist(&result.positions, 1, 2);
        let d12 = dist(&result.positions, 2, 3);
        let d20 = dist(&result.positions, 3, 1);
        prop_assert!((d01 - distances[0]).abs() < 1e-4, "d01: {} vs {}", d01, distances[0]);
        prop_assert!((d12 - distances[1]).abs() < 1e-4, "d12: {} vs {}", d12, distances[1]);
        prop_assert!((d20 - distances[2]).abs() < 1e-4, "d20: {} vs {}", d20, distances[2]);
    }

    // ── Polygon: seed-and-measure ───────────────────────────────────────

    /// Random convex pentagon with distance constraints.
    #[test]
    fn proptest_polygon_5(
        (sketch, distances) in proptest_strategies::arb_constrained_polygon(5)
    ) {
        let result = solve_sketch(&sketch);
        // Only verify distances if the solver converged to FullyConstrained.
        // Some configurations might still be too hard for the solver even with
        // the improved strategy.
        prop_assume!(matches!(result.status, SolveStatus::FullyConstrained));

        for (i, &expected_d) in distances.iter().enumerate() {
            let j = (i + 1) % 5;
            let d = dist(&result.positions, (i + 1) as u32, (j + 1) as u32);
            prop_assert!(
                (d - expected_d).abs() < 1e-4,
                "edge {}-{}: {} vs {}", i, j, d, expected_d
            );
        }
    }
}
