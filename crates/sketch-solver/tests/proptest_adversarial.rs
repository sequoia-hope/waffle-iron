//! Adversarial edge-case tests: degenerate geometry, extreme scales,
//! near-singular configurations.

mod proptest_strategies;

use proptest::prelude::*;
use proptest_strategies::make_sketch;
use sketch_solver::*;
use std::collections::HashMap;

fn dist(positions: &HashMap<u32, (f64, f64)>, a: u32, b: u32) -> f64 {
    let (ax, ay) = positions[&a];
    let (bx, by) = positions[&b];
    ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt()
}

// ── Near-parallel lines with angle constraint ───────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Two nearly-parallel lines with a small angle constraint.
    /// Tests that the solver handles near-singular Jacobians.
    #[test]
    fn proptest_near_parallel_angle(
        angle_deg in 0.05..2.0f64,
        length in 10.0..100.0f64,
    ) {
        let angle_rad = angle_deg.to_radians();
        let entities = vec![
            SketchEntity::Point { id: 1, x: 0.0, y: 0.0, construction: false },
            SketchEntity::Point { id: 2, x: length, y: 0.0, construction: false },
            SketchEntity::Point { id: 3, x: 0.0, y: 10.0, construction: false },
            SketchEntity::Point {
                id: 4,
                x: length * angle_rad.cos(),
                y: 10.0 + length * angle_rad.sin(),
                construction: false,
            },
            SketchEntity::Line { id: 10, start_id: 1, end_id: 2, construction: false },
            SketchEntity::Line { id: 11, start_id: 3, end_id: 4, construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Angle { line_a: 10, line_b: 11, value_degrees: angle_deg },
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 3 },
            SketchConstraint::Distance { entity_a: 1, entity_b: 2, value: length },
            SketchConstraint::Distance { entity_a: 3, entity_b: 4, value: length },
        ];
        let sketch = make_sketch(entities, constraints);
        let result = solve_sketch(&sketch);
        prop_assert!(
            matches!(result.status, SolveStatus::FullyConstrained),
            "failed for angle={:.4} deg: {:?}", angle_deg, result.status
        );
    }

    // ── Scale invariance ────────────────────────────────────────────────

    /// The same rectangle at different scales should all solve correctly.
    #[test]
    fn proptest_scale_invariance(
        scale_exp in -3.0..3.0f64,
    ) {
        let scale = 10.0f64.powf(scale_exp);
        let w = 100.0 * scale;
        let h = 50.0 * scale;

        let entities = vec![
            SketchEntity::Point { id: 1, x: 0.0, y: 0.0, construction: false },
            SketchEntity::Point { id: 2, x: w, y: 0.0, construction: false },
            SketchEntity::Point { id: 3, x: w, y: h, construction: false },
            SketchEntity::Point { id: 4, x: 0.0, y: h, construction: false },
            SketchEntity::Line { id: 10, start_id: 1, end_id: 2, construction: false },
            SketchEntity::Line { id: 11, start_id: 2, end_id: 3, construction: false },
            SketchEntity::Line { id: 12, start_id: 3, end_id: 4, construction: false },
            SketchEntity::Line { id: 13, start_id: 4, end_id: 1, construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance { entity_a: 1, entity_b: 2, value: w },
            SketchConstraint::Distance { entity_a: 2, entity_b: 3, value: h },
            SketchConstraint::Dragged { point: 1 },
        ];
        let sketch = make_sketch(entities, constraints);
        let result = solve_sketch(&sketch);
        prop_assert!(
            matches!(result.status, SolveStatus::FullyConstrained),
            "failed at scale={:.2e}: {:?}", scale, result.status
        );

        let d_w = dist(&result.positions, 1, 2);
        let d_h = dist(&result.positions, 2, 3);
        let tol = w.abs().max(h.abs()) * 1e-5;
        prop_assert!((d_w - w).abs() < tol, "width: got {}, expected {}, scale={}", d_w, w, scale);
        prop_assert!((d_h - h).abs() < tol, "height: got {}, expected {}, scale={}", d_h, h, scale);
    }

    // ── Tangent arc-arc ─────────────────────────────────────────────────

    /// Two circles with tangent constraint (external).
    #[test]
    fn proptest_tangent_circles_external(
        r1 in 5.0..50.0f64,
        r2 in 5.0..50.0f64,
    ) {
        // Place circles so they're tangent: centers at distance r1+r2
        let cx2 = r1 + r2;
        let entities = vec![
            SketchEntity::Point { id: 1, x: 0.0, y: 0.0, construction: false },
            SketchEntity::Circle { id: 10, center_id: 1, radius: r1, construction: false },
            SketchEntity::Point { id: 2, x: cx2, y: 0.0, construction: false },
            SketchEntity::Circle { id: 11, center_id: 2, radius: r2, construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Radius { entity: 10, value: r1 },
            SketchConstraint::Radius { entity: 11, value: r2 },
            SketchConstraint::Dragged { point: 2 },
        ];
        let sketch = make_sketch(entities, constraints);
        let result = solve_sketch(&sketch);
        prop_assert!(
            matches!(result.status, SolveStatus::FullyConstrained),
            "tangent circles failed: {:?}", result.status
        );
        // Verify center distance
        let d = dist(&result.positions, 1, 2);
        prop_assert!(
            (d - cx2).abs() < 1e-5,
            "center distance: {} vs expected {}", d, cx2
        );
    }

    // ── Circle with point on entity ─────────────────────────────────────

    /// A circle with a point constrained on it.
    #[test]
    fn proptest_point_on_circle(
        r in 5.0..100.0f64,
        angle in 0.0..std::f64::consts::TAU,
    ) {
        let px = r * angle.cos();
        let py = r * angle.sin();
        let entities = vec![
            SketchEntity::Point { id: 1, x: 0.0, y: 0.0, construction: false },
            SketchEntity::Circle { id: 10, center_id: 1, radius: r, construction: false },
            SketchEntity::Point { id: 2, x: px, y: py, construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Radius { entity: 10, value: r },
            SketchConstraint::OnEntity { point: 2, entity: 10 },
        ];
        let sketch = make_sketch(entities, constraints);
        let result = solve_sketch(&sketch);
        // Should be under-constrained (point free to slide along circle)
        // but the on-entity constraint should be satisfied
        let d = dist(&result.positions, 1, 2);
        prop_assert!(
            (d - r).abs() < 1e-4,
            "point-on-circle distance: {} vs radius {}", d, r
        );
    }

    // ── Perpendicular lines ─────────────────────────────────────────────

    /// Two lines constrained perpendicular with random orientations.
    #[test]
    fn proptest_perpendicular_lines(
        base_angle in 0.0..std::f64::consts::PI,
        len1 in 10.0..100.0f64,
        len2 in 10.0..100.0f64,
    ) {
        let perp_angle = base_angle + std::f64::consts::FRAC_PI_2;
        let entities = vec![
            SketchEntity::Point { id: 1, x: 0.0, y: 0.0, construction: false },
            SketchEntity::Point {
                id: 2,
                x: len1 * base_angle.cos(),
                y: len1 * base_angle.sin(),
                construction: false,
            },
            SketchEntity::Point { id: 3, x: 0.0, y: 0.0, construction: false },
            SketchEntity::Point {
                id: 4,
                x: len2 * perp_angle.cos(),
                y: len2 * perp_angle.sin(),
                construction: false,
            },
            SketchEntity::Line { id: 10, start_id: 1, end_id: 2, construction: false },
            SketchEntity::Line { id: 11, start_id: 3, end_id: 4, construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Perpendicular { line_a: 10, line_b: 11 },
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 3 },
            SketchConstraint::Distance { entity_a: 1, entity_b: 2, value: len1 },
            SketchConstraint::Distance { entity_a: 3, entity_b: 4, value: len2 },
        ];
        let sketch = make_sketch(entities, constraints);
        let result = solve_sketch(&sketch);
        // Verify the dot product is near zero
        let (x1, y1) = result.positions[&1];
        let (x2, y2) = result.positions[&2];
        let (x3, y3) = result.positions[&3];
        let (x4, y4) = result.positions[&4];
        let d1 = (x2 - x1, y2 - y1);
        let d2 = (x4 - x3, y4 - y3);
        let dot = d1.0 * d2.0 + d1.1 * d2.1;
        prop_assert!(
            dot.abs() < 1e-3,
            "perpendicular dot product = {} (should be ~0)", dot
        );
    }
}
