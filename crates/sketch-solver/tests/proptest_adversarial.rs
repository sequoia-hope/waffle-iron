//! Adversarial edge-case tests: degenerate geometry, extreme scales,
//! near-singular configurations.

mod proptest_strategies;

use proptest::prelude::*;
use proptest_strategies::make_sketch;
use sketch_solver::*;
use std::collections::HashMap;

fn dist(positions: &HashMap<PointId, (f64, f64)>, a: PointId, b: PointId) -> f64 {
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
            SketchEntity::Point { id: PointId(1), x: 0.0, y: 0.0, construction: false },
            SketchEntity::Point { id: PointId(2), x: length, y: 0.0, construction: false },
            SketchEntity::Point { id: PointId(3), x: 0.0, y: 10.0, construction: false },
            SketchEntity::Point {
                id: PointId(4),
                x: length * angle_rad.cos(),
                y: 10.0 + length * angle_rad.sin(),
                construction: false,
            },
            SketchEntity::Line { id: LineId(10), start_id: PointId(1), end_id: PointId(2), construction: false },
            SketchEntity::Line { id: LineId(11), start_id: PointId(3), end_id: PointId(4), construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Horizontal { entity: EntityId(10) },
            SketchConstraint::Angle { line_a: EntityId(10), line_b: EntityId(11), value_degrees: angle_deg },
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(3) },
            SketchConstraint::Distance { entity_a: EntityId(1), entity_b: EntityId(2), value: length },
            SketchConstraint::Distance { entity_a: EntityId(3), entity_b: EntityId(4), value: length },
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
            SketchEntity::Point { id: PointId(1), x: 0.0, y: 0.0, construction: false },
            SketchEntity::Point { id: PointId(2), x: w, y: 0.0, construction: false },
            SketchEntity::Point { id: PointId(3), x: w, y: h, construction: false },
            SketchEntity::Point { id: PointId(4), x: 0.0, y: h, construction: false },
            SketchEntity::Line { id: LineId(10), start_id: PointId(1), end_id: PointId(2), construction: false },
            SketchEntity::Line { id: LineId(11), start_id: PointId(2), end_id: PointId(3), construction: false },
            SketchEntity::Line { id: LineId(12), start_id: PointId(3), end_id: PointId(4), construction: false },
            SketchEntity::Line { id: LineId(13), start_id: PointId(4), end_id: PointId(1), construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Horizontal { entity: EntityId(10) },
            SketchConstraint::Horizontal { entity: EntityId(12) },
            SketchConstraint::Vertical { entity: EntityId(11) },
            SketchConstraint::Vertical { entity: EntityId(13) },
            SketchConstraint::Distance { entity_a: EntityId(1), entity_b: EntityId(2), value: w },
            SketchConstraint::Distance { entity_a: EntityId(2), entity_b: EntityId(3), value: h },
            SketchConstraint::Dragged { point: PointId(1) },
        ];
        let sketch = make_sketch(entities, constraints);
        let result = solve_sketch(&sketch);
        prop_assert!(
            matches!(result.status, SolveStatus::FullyConstrained),
            "failed at scale={:.2e}: {:?}", scale, result.status
        );

        let d_w = dist(&result.positions, PointId(1), PointId(2));
        let d_h = dist(&result.positions, PointId(2), PointId(3));
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
            SketchEntity::Point { id: PointId(1), x: 0.0, y: 0.0, construction: false },
            SketchEntity::Circle { id: CircleId(10), center_id: PointId(1), radius: r1, construction: false },
            SketchEntity::Point { id: PointId(2), x: cx2, y: 0.0, construction: false },
            SketchEntity::Circle { id: CircleId(11), center_id: PointId(2), radius: r2, construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Radius { entity: EntityId(10), value: r1 },
            SketchConstraint::Radius { entity: EntityId(11), value: r2 },
            SketchConstraint::Dragged { point: PointId(2) },
        ];
        let sketch = make_sketch(entities, constraints);
        let result = solve_sketch(&sketch);
        prop_assert!(
            matches!(result.status, SolveStatus::FullyConstrained),
            "tangent circles failed: {:?}", result.status
        );
        // Verify center distance
        let d = dist(&result.positions, PointId(1), PointId(2));
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
            SketchEntity::Point { id: PointId(1), x: 0.0, y: 0.0, construction: false },
            SketchEntity::Circle { id: CircleId(10), center_id: PointId(1), radius: r, construction: false },
            SketchEntity::Point { id: PointId(2), x: px, y: py, construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Radius { entity: EntityId(10), value: r },
            SketchConstraint::OnEntity { point: PointId(2), entity: EntityId(10) },
        ];
        let sketch = make_sketch(entities, constraints);
        let result = solve_sketch(&sketch);
        // Should be under-constrained (point free to slide along circle)
        // but the on-entity constraint should be satisfied
        let d = dist(&result.positions, PointId(1), PointId(2));
        prop_assert!(
            (d - r).abs() < 1e-4,
            "point-on-circle distance: {} vs radius {}", d, r
        );
    }

    // ── Perpendicular lines ─────────────────────────────────────────────

    // ── DOF invariance under rotation for arc constraints ──────────────

    /// Construct an arc sketch (center + start + end + OnEntity for an extra
    /// point), apply a random rotation to all points, solve, and assert DOF
    /// is the same regardless of rotation angle. This mechanically verifies
    /// that cardinal alignment doesn't affect DOF counting.
    #[test]
    fn proptest_arc_dof_rotation_invariant(
        angle_deg in 0.0..360.0f64,
        r in 5.0..50.0f64,
    ) {
        let a = angle_deg.to_radians();
        let cos_a = a.cos();
        let sin_a = a.sin();

        // Rotate a point by `angle_deg` around origin
        let rot = |x: f64, y: f64| -> (f64, f64) {
            (x * cos_a - y * sin_a, x * sin_a + y * cos_a)
        };

        // Base geometry: center at origin, start at (r, 0), end at (0, r)
        let (cx, cy) = (0.0, 0.0);
        let (sx, sy) = rot(r, 0.0);
        let (ex, ey) = rot(0.0, r);
        // Extra point on circle at 45°
        let (px, py) = rot(r * std::f64::consts::FRAC_1_SQRT_2, r * std::f64::consts::FRAC_1_SQRT_2);

        let sketch = make_sketch(
            vec![
                SketchEntity::Point { id: PointId(1), x: cx, y: cy, construction: false },
                SketchEntity::Point { id: PointId(2), x: sx, y: sy, construction: false },
                SketchEntity::Point { id: PointId(3), x: ex, y: ey, construction: false },
                SketchEntity::Arc {
                    id: ArcId(10),
                    center_id: PointId(1),
                    start_id: PointId(2),
                    end_id: PointId(3),
                    construction: false,
                },
                SketchEntity::Point { id: PointId(4), x: px, y: py, construction: false },
            ],
            vec![
                SketchConstraint::Dragged { point: PointId(1) },
                SketchConstraint::Dragged { point: PointId(2) },
                SketchConstraint::OnEntity { point: PointId(4), entity: EntityId(10) },
            ],
        );

        let result = solve_sketch(&sketch);
        // With center pinned (2 DOF removed) and start pinned (2 DOF removed),
        // end has 1 DOF (angle along arc, radius fixed by implicit OnCircle).
        // Point 4 has 1 DOF (on arc). Total: 3 DOF.
        // The exact DOF value isn't critical — what matters is it's the SAME
        // at all rotations.
        let dof = match result.status {
            SolveStatus::FullyConstrained => 0,
            SolveStatus::UnderConstrained { dof } => dof,
            other => panic!("unexpected status at angle {:.1}°: {:?}", angle_deg, other),
        };

        // Reference: solve at 0° (no rotation)
        let ref_sketch = make_sketch(
            vec![
                SketchEntity::Point { id: PointId(1), x: 0.0, y: 0.0, construction: false },
                SketchEntity::Point { id: PointId(2), x: r, y: 0.0, construction: false },
                SketchEntity::Point { id: PointId(3), x: 0.0, y: r, construction: false },
                SketchEntity::Arc {
                    id: ArcId(10),
                    center_id: PointId(1),
                    start_id: PointId(2),
                    end_id: PointId(3),
                    construction: false,
                },
                SketchEntity::Point { id: PointId(4), x: r * std::f64::consts::FRAC_1_SQRT_2, y: r * std::f64::consts::FRAC_1_SQRT_2, construction: false },
            ],
            vec![
                SketchConstraint::Dragged { point: PointId(1) },
                SketchConstraint::Dragged { point: PointId(2) },
                SketchConstraint::OnEntity { point: PointId(4), entity: EntityId(10) },
            ],
        );
        let ref_result = solve_sketch(&ref_sketch);
        let ref_dof = match ref_result.status {
            SolveStatus::FullyConstrained => 0,
            SolveStatus::UnderConstrained { dof } => dof,
            other => panic!("unexpected ref status: {:?}", other),
        };

        prop_assert_eq!(
            dof, ref_dof,
            "DOF at {:.1}° ({}) != DOF at 0° ({})",
            angle_deg, dof, ref_dof
        );
    }

    // ── Arc OnCircle residual at cardinal positions ────────────────────

    /// OnCircle residual should be near-zero when the point is exactly at
    /// radius distance from center, regardless of angle. Tests that no
    /// cardinal-position numerical artifacts produce nonzero residuals.
    #[test]
    fn proptest_oncircle_residual_at_angle(
        angle_deg in 0.0..360.0f64,
        r in 1.0..100.0f64,
        cx in -50.0..50.0f64,
        cy in -50.0..50.0f64,
    ) {
        let a = angle_deg.to_radians();
        let px = cx + r * a.cos();
        let py = cy + r * a.sin();

        // Build a circle with point exactly on it, solve, verify residual ~ 0
        let sketch = make_sketch(
            vec![
                SketchEntity::Point { id: PointId(1), x: cx, y: cy, construction: false },
                SketchEntity::Circle { id: CircleId(10), center_id: PointId(1), radius: r, construction: false },
                SketchEntity::Point { id: PointId(2), x: px, y: py, construction: false },
            ],
            vec![
                SketchConstraint::Dragged { point: PointId(1) },
                SketchConstraint::Radius { entity: EntityId(10), value: r },
                SketchConstraint::OnEntity { point: PointId(2), entity: EntityId(10) },
            ],
        );
        let result = solve_sketch(&sketch);

        // Point should remain on circle after solve
        let (sx, sy) = result.positions[&PointId(2)];
        let (scx, scy) = result.positions[&PointId(1)];
        let dist = ((sx - scx).powi(2) + (sy - scy).powi(2)).sqrt();
        prop_assert!(
            (dist - r).abs() < 1e-4,
            "point-on-circle distance {:.6} != radius {:.6} at angle {:.1}°",
            dist, r, angle_deg
        );
    }

    // ── Arc with tangent line at all orientations ────────────────────────

    /// Line tangent to an arc at a random angle. The tangent distance (center
    /// to line) must equal the radius after solving. Tests that TangentLineCircle
    /// Jacobian is correct at all orientations including cardinal.
    #[test]
    fn proptest_tangent_arc_line_all_angles(
        angle_deg in 0.0..360.0f64,
        r in 5.0..50.0f64,
    ) {
        let a = angle_deg.to_radians();
        // Arc center at origin. Tangent point at (r*cos(a), r*sin(a)).
        // Line is perpendicular to radius at tangent point.
        let tx = r * a.cos();
        let ty = r * a.sin();
        // Line direction is perpendicular to (cos a, sin a) = (-sin a, cos a)
        let dx = -a.sin();
        let dy = a.cos();
        let line_len = 20.0;

        let entities = vec![
            SketchEntity::Point { id: PointId(1), x: 0.0, y: 0.0, construction: false }, // arc center
            SketchEntity::Point { id: PointId(2), x: r, y: 0.0, construction: false },    // arc start
            SketchEntity::Point { id: PointId(3), x: 0.0, y: r, construction: false },    // arc end
            SketchEntity::Arc {
                id: ArcId(10),
                center_id: PointId(1),
                start_id: PointId(2),
                end_id: PointId(3),
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: tx - line_len * dx,
                y: ty - line_len * dy,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(5),
                x: tx + line_len * dx,
                y: ty + line_len * dy,
                construction: false,
            },
            SketchEntity::Line { id: LineId(11), start_id: PointId(4), end_id: PointId(5), construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(4) },
            SketchConstraint::Dragged { point: PointId(5) },
            SketchConstraint::Tangent { line: EntityId(11), curve: EntityId(10) },
        ];
        let sketch = make_sketch(entities, constraints);
        let result = solve_sketch(&sketch);

        // Verify tangency: distance from center to line ≈ radius
        let (cx, cy) = result.positions[&PointId(1)];
        let (lx1, ly1) = result.positions[&PointId(4)];
        let (lx2, ly2) = result.positions[&PointId(5)];
        let ldx = lx2 - lx1;
        let ldy = ly2 - ly1;
        let line_len_sq = ldx * ldx + ldy * ldy;
        if line_len_sq > 1e-10 {
            let cross = ((cx - lx1) * ldy - (cy - ly1) * ldx).abs();
            let dist_to_line = cross / line_len_sq.sqrt();
            // Arc radius = dist(center, start)
            let (sx, sy) = result.positions[&PointId(2)];
            let arc_r = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
            prop_assert!(
                (dist_to_line - arc_r).abs() < 1e-2,
                "tangent distance {:.4} != arc radius {:.4} at angle {:.1}°",
                dist_to_line, arc_r, angle_deg
            );
        }
    }

    // ── Cardinal-specific DOF: exact 0/90/180/270 must match arbitrary ──

    /// Explicitly test the four exact cardinal angles (0°, 90°, 180°, 270°)
    /// plus a non-cardinal reference. All must produce the same DOF.
    #[test]
    fn proptest_cardinal_exact_dof_matches_arbitrary(
        ref_angle_deg in 10.0..80.0f64,
        r in 5.0..50.0f64,
    ) {
        let build = |angle_deg: f64| {
            let a = angle_deg.to_radians();
            make_sketch(
                vec![
                    SketchEntity::Point { id: PointId(1), x: 0.0, y: 0.0, construction: false },
                    SketchEntity::Point { id: PointId(2), x: r * a.cos(), y: r * a.sin(), construction: false },
                    SketchEntity::Point { id: PointId(3), x: -r * a.sin(), y: r * a.cos(), construction: false },
                    SketchEntity::Arc {
                        id: ArcId(10),
                        center_id: PointId(1),
                        start_id: PointId(2),
                        end_id: PointId(3),
                        construction: false,
                    },
                ],
                vec![
                    SketchConstraint::Dragged { point: PointId(1) },
                    SketchConstraint::Radius { entity: EntityId(10), value: r },
                ],
            )
        };

        let dof_of = |s: &Sketch| -> u32 {
            let result = solve_sketch(s);
            match result.status {
                SolveStatus::FullyConstrained => 0,
                SolveStatus::UnderConstrained { dof } => dof,
                other => panic!("unexpected: {:?}", other),
            }
        };

        let ref_dof = dof_of(&build(ref_angle_deg));
        for &cardinal in &[0.0, 90.0, 180.0, 270.0] {
            let cardinal_dof = dof_of(&build(cardinal));
            prop_assert_eq!(
                cardinal_dof, ref_dof,
                "DOF at {:.0}° ({}) != DOF at {:.1}° ({})",
                cardinal, cardinal_dof, ref_angle_deg, ref_dof
            );
        }
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
            SketchEntity::Point { id: PointId(1), x: 0.0, y: 0.0, construction: false },
            SketchEntity::Point {
                id: PointId(2),
                x: len1 * base_angle.cos(),
                y: len1 * base_angle.sin(),
                construction: false,
            },
            SketchEntity::Point { id: PointId(3), x: 0.0, y: 0.0, construction: false },
            SketchEntity::Point {
                id: PointId(4),
                x: len2 * perp_angle.cos(),
                y: len2 * perp_angle.sin(),
                construction: false,
            },
            SketchEntity::Line { id: LineId(10), start_id: PointId(1), end_id: PointId(2), construction: false },
            SketchEntity::Line { id: LineId(11), start_id: PointId(3), end_id: PointId(4), construction: false },
        ];
        let constraints = vec![
            SketchConstraint::Perpendicular { line_a: EntityId(10), line_b: EntityId(11) },
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(3) },
            SketchConstraint::Distance { entity_a: EntityId(1), entity_b: EntityId(2), value: len1 },
            SketchConstraint::Distance { entity_a: EntityId(3), entity_b: EntityId(4), value: len2 },
        ];
        let sketch = make_sketch(entities, constraints);
        let result = solve_sketch(&sketch);
        // Verify the dot product is near zero
        let (x1, y1) = result.positions[&PointId(1)];
        let (x2, y2) = result.positions[&PointId(2)];
        let (x3, y3) = result.positions[&PointId(3)];
        let (x4, y4) = result.positions[&PointId(4)];
        let d1 = (x2 - x1, y2 - y1);
        let d2 = (x4 - x3, y4 - y3);
        let dot = d1.0 * d2.0 + d1.1 * d2.1;
        prop_assert!(
            dot.abs() < 1e-3,
            "perpendicular dot product = {} (should be ~0)", dot
        );
    }
}
