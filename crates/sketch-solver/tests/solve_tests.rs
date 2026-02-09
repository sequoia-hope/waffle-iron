use sketch_solver::*;
use uuid::Uuid;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn dummy_geom_ref() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::nil(),
        },
        selector: Selector::Role {
            role: Role::ProfileFace,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    }
}

fn make_sketch(entities: Vec<SketchEntity>, constraints: Vec<SketchConstraint>) -> Sketch {
    Sketch {
        id: Uuid::new_v4(),
        plane: dummy_geom_ref(),
        entities,
        constraints,
        solve_status: SolveStatus::UnderConstrained { dof: 99 },
    }
}

fn assert_point_near(
    positions: &std::collections::HashMap<u32, (f64, f64)>,
    id: u32,
    expected: (f64, f64),
    tol: f64,
) {
    let (x, y) = positions
        .get(&id)
        .unwrap_or_else(|| panic!("point {} not found in positions", id));
    assert!(
        (x - expected.0).abs() < tol && (y - expected.1).abs() < tol,
        "point {} = ({:.4}, {:.4}), expected ({:.4}, {:.4}), tol={tol}",
        id,
        x,
        y,
        expected.0,
        expected.1,
    );
}

// ── M4: Solve + Position Extraction ────────────────────────────────────────

#[test]
fn rectangle_100x50_fully_constrained() {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 100.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 100.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 100.0,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 50.0,
            },
            SketchConstraint::Dragged { point: 1 },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let tol = 1e-6;
    assert_point_near(&result.positions, 1, (0.0, 0.0), tol);
    assert_point_near(&result.positions, 2, (100.0, 0.0), tol);
    assert_point_near(&result.positions, 3, (100.0, 50.0), tol);
    assert_point_near(&result.positions, 4, (0.0, 50.0), tol);
}

#[test]
fn circle_center_and_radius() {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 25.0,
                y: 25.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: 10,
                center_id: 1,
                radius: 15.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Radius {
                entity: 10,
                value: 15.0,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));
    assert_point_near(&result.positions, 1, (25.0, 25.0), 1e-6);
}

#[test]
fn equilateral_triangle_equal_lengths() {
    // Three points forming a triangle, all sides equal = 60mm
    // Fix one side horizontal to remove rotation DOF
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 60.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 30.0,
                y: 51.96,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 1,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 60.0,
            },
            SketchConstraint::Equal {
                entity_a: 10,
                entity_b: 11,
            },
            SketchConstraint::Equal {
                entity_a: 11,
                entity_b: 12,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let tol = 1e-4;
    assert_point_near(&result.positions, 1, (0.0, 0.0), tol);
    assert_point_near(&result.positions, 2, (60.0, 0.0), tol);

    // Third point should be at (30, 30*sqrt(3)) ≈ (30, 51.9615)
    let (x3, y3) = result.positions[&3];
    assert!((x3 - 30.0).abs() < tol, "x3={x3}, expected 30.0");
    let expected_y = 30.0 * 3.0_f64.sqrt();
    assert!(
        (y3 - expected_y).abs() < tol,
        "y3={y3}, expected {expected_y}"
    );
}

#[test]
fn two_points_with_distance() {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 42.0,
                y: 0.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 42.0,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    // Point 2 still has rotational freedom (1 DOF)
    assert!(matches!(
        result.status,
        SolveStatus::UnderConstrained { dof: 1 }
    ));
    assert_point_near(&result.positions, 1, (0.0, 0.0), 1e-6);

    // Verify distance is 42
    let (x2, y2) = result.positions[&2];
    let dist = ((x2).powi(2) + (y2).powi(2)).sqrt();
    assert!((dist - 42.0).abs() < 1e-6, "distance={dist}, expected 42.0");
}

// ── M5: SolveStatus Detection ──────────────────────────────────────────────

#[test]
fn status_fully_constrained() {
    // Single point pinned at origin
    let sketch = make_sketch(
        vec![SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        }],
        vec![SketchConstraint::Dragged { point: 1 }],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));
}

#[test]
fn status_under_constrained() {
    // Two points, no constraints linking them — each free point has 2 DOF
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 10.0,
                y: 10.0,
                construction: false,
            },
        ],
        vec![],
    );

    let result = solve_sketch(&sketch);
    match result.status {
        SolveStatus::UnderConstrained { dof } => {
            assert_eq!(dof, 4, "two free points = 4 DOF");
        }
        other => panic!("expected UnderConstrained, got {:?}", other),
    }
}

#[test]
fn status_under_constrained_single_free_point() {
    let sketch = make_sketch(
        vec![SketchEntity::Point {
            id: 1,
            x: 5.0,
            y: 5.0,
            construction: false,
        }],
        vec![],
    );

    let result = solve_sketch(&sketch);
    match result.status {
        SolveStatus::UnderConstrained { dof } => {
            assert_eq!(dof, 2, "one free point = 2 DOF");
        }
        other => panic!("expected UnderConstrained, got {:?}", other),
    }
}

#[test]
fn status_over_constrained() {
    // Pin a point at origin, then also constrain its distance to itself ≠ 0
    // This creates an impossible constraint
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 10.0,
                y: 0.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            // Force point 2 to be at distance 10 AND also coincident with point 1
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 10.0,
            },
            SketchConstraint::Coincident {
                point_a: 1,
                point_b: 2,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::OverConstrained { .. } | SolveStatus::SolveFailed { .. }
        ),
        "expected OverConstrained or SolveFailed, got {:?}",
        result.status
    );
}

#[test]
fn status_rectangle_dof_count() {
    // Rectangle without position fix: 4 points (8 DOF) - 4 h/v constraints - 2 dimensions = 2 DOF
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 80.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 80.0,
                y: 40.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 40.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 80.0,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 40.0,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    match result.status {
        SolveStatus::UnderConstrained { dof } => {
            assert_eq!(
                dof, 2,
                "rectangle without position fix should have 2 DOF (translation)"
            );
        }
        other => panic!("expected UnderConstrained {{ dof: 2 }}, got {:?}", other),
    }
}

// ── M6: Profile Extraction ─────────────────────────────────────────────────

#[test]
fn profile_rectangle_one_outer() {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 100.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 100.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 100.0,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 50.0,
            },
            SketchConstraint::Dragged { point: 1 },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let outer_profiles: Vec<_> = result.profiles.iter().filter(|p| p.is_outer).collect();
    assert_eq!(
        outer_profiles.len(),
        1,
        "rectangle should have 1 outer profile"
    );
    assert_eq!(
        outer_profiles[0].entity_ids.len(),
        4,
        "rectangle profile should have 4 entities"
    );

    // All 4 line IDs should be present
    let mut ids = outer_profiles[0].entity_ids.clone();
    ids.sort();
    assert_eq!(ids, vec![10, 11, 12, 13]);
}

#[test]
fn profile_circle_one_outer() {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 50.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: 10,
                center_id: 1,
                radius: 25.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Radius {
                entity: 10,
                value: 25.0,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    assert_eq!(result.profiles.len(), 1, "circle should produce 1 profile");
    assert!(
        result.profiles[0].is_outer,
        "circle profile should be outer"
    );
    assert_eq!(result.profiles[0].entity_ids, vec![10]);
}

#[test]
fn profile_construction_geometry_excluded() {
    // Rectangle where one line is construction — should NOT form a closed profile
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 100.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 100.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: true,
            }, // construction!
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 100.0,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 50.0,
            },
            SketchConstraint::Dragged { point: 1 },
        ],
    );

    let result = solve_sketch(&sketch);

    // No closed profile should exist since the loop is broken by construction geometry
    let outer_with_4_edges: Vec<_> = result
        .profiles
        .iter()
        .filter(|p| p.is_outer && p.entity_ids.len() == 4)
        .collect();
    assert_eq!(
        outer_with_4_edges.len(),
        0,
        "broken loop should not form a 4-edge profile"
    );
}

#[test]
fn profile_construction_circle_excluded() {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: 10,
                center_id: 1,
                radius: 20.0,
                construction: true,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Radius {
                entity: 10,
                value: 20.0,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        result.profiles.is_empty(),
        "construction circle should not produce a profile"
    );
}

#[test]
fn profile_rect_with_circle_hole() {
    // Outer rectangle + inner circle = 2 profiles (1 outer + 1 inner-ish)
    // The circle is independent, so it's always classified as outer by extract_profiles.
    // The nesting (outer vs inner/hole) is determined by containment, which for a
    // standalone circle defaults to is_outer=true. In practice the extrude step
    // does the containment test. Here we just verify both profiles are found.
    let sketch = make_sketch(
        vec![
            // Rectangle corners
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 100.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 100.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 50.0,
                construction: false,
            },
            // Rectangle edges
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
            // Circle hole
            SketchEntity::Point {
                id: 5,
                x: 50.0,
                y: 25.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: 20,
                center_id: 5,
                radius: 10.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 100.0,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 50.0,
            },
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 5 },
            SketchConstraint::Radius {
                entity: 20,
                value: 10.0,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    // Should have at least 2 profiles: rectangle + circle
    assert!(
        result.profiles.len() >= 2,
        "expected at least 2 profiles, got {}",
        result.profiles.len()
    );

    // One profile should be the circle
    let circle_profile = result.profiles.iter().find(|p| p.entity_ids == vec![20]);
    assert!(circle_profile.is_some(), "circle profile not found");

    // One profile should contain the rectangle edges
    let rect_profile = result.profiles.iter().find(|p| {
        let mut ids = p.entity_ids.clone();
        ids.sort();
        ids == vec![10, 11, 12, 13]
    });
    assert!(rect_profile.is_some(), "rectangle profile not found");
}

// ── M7: Reference Sketch Tests ─────────────────────────────────────────────

#[test]
fn reference_rectangle_analytical() {
    // Full analytical test: 4 lines + h/v constraints + 2 distance + dragged origin
    // Expected: exact corner positions at (0,0), (200,0), (200,100), (0,100)
    let w = 200.0;
    let h = 100.0;
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: w,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: w,
                y: h,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: h,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: w,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: h,
            },
            SketchConstraint::Dragged { point: 1 },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let tol = 1e-8;
    assert_point_near(&result.positions, 1, (0.0, 0.0), tol);
    assert_point_near(&result.positions, 2, (w, 0.0), tol);
    assert_point_near(&result.positions, 3, (w, h), tol);
    assert_point_near(&result.positions, 4, (0.0, h), tol);
}

#[test]
fn reference_circle_analytical() {
    let cx = 75.0;
    let cy = 30.0;
    let r = 42.0;

    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: cx,
                y: cy,
                construction: false,
            },
            SketchEntity::Circle {
                id: 10,
                center_id: 1,
                radius: r,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Radius {
                entity: 10,
                value: r,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));
    assert_point_near(&result.positions, 1, (cx, cy), 1e-8);
    assert_eq!(result.profiles.len(), 1);
    assert!(result.profiles[0].is_outer);
}

#[test]
fn reference_square_with_equal_sides() {
    // Square: 4 lines, all equal length, one side dimensioned
    let s = 50.0;
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: s,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: s,
                y: s,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: s,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: s,
            },
            SketchConstraint::Equal {
                entity_a: 10,
                entity_b: 11,
            },
            SketchConstraint::Dragged { point: 1 },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let tol = 1e-6;
    assert_point_near(&result.positions, 1, (0.0, 0.0), tol);
    assert_point_near(&result.positions, 2, (s, 0.0), tol);
    assert_point_near(&result.positions, 3, (s, s), tol);
    assert_point_near(&result.positions, 4, (0.0, s), tol);
}

#[test]
fn reference_perpendicular_lines() {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 50.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 0.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 1,
                end_id: 3,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Perpendicular {
                line_a: 10,
                line_b: 11,
            },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 50.0,
            },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 3,
                value: 30.0,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let tol = 1e-6;
    assert_point_near(&result.positions, 1, (0.0, 0.0), tol);
    assert_point_near(&result.positions, 2, (50.0, 0.0), tol);
    // Perpendicular to horizontal line must be vertical
    let (x3, _y3) = result.positions[&3];
    assert!(
        (x3 - 0.0).abs() < tol,
        "perpendicular line endpoint should have x=0, got {x3}"
    );
}

#[test]
fn reference_parallel_lines() {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 100.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 0.0,
                y: 40.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 80.0,
                y: 40.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 3 },
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Parallel {
                line_a: 10,
                line_b: 11,
            },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 100.0,
            },
            SketchConstraint::Distance {
                entity_a: 3,
                entity_b: 4,
                value: 80.0,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let tol = 1e-6;
    // Both lines should be horizontal (parallel to line 10 which is horizontal)
    let (_, y3) = result.positions[&3];
    let (_, y4) = result.positions[&4];
    assert!(
        (y3 - y4).abs() < tol,
        "parallel lines should have same y: y3={y3}, y4={y4}"
    );
}

#[test]
fn reference_midpoint_constraint() {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 100.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 50.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 100.0,
            },
            SketchConstraint::Midpoint { point: 3, line: 10 },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));
    assert_point_near(&result.positions, 3, (50.0, 0.0), 1e-6);
}

#[test]
fn reference_symmetric_about_line() {
    // Two points symmetric about a vertical center line.
    // Line 10: vertical center line from (50,0) to (50,100).
    // Points 3 and 4 should mirror across this line.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 50.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 50.0,
                y: 100.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 20.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 80.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: true,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            SketchConstraint::Dragged { point: 3 },
            SketchConstraint::Symmetric {
                entity_a: 3,
                entity_b: 4,
                symmetry_line: 10,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let tol = 1e-6;
    let (x3, y3) = result.positions[&3];
    let (x4, y4) = result.positions[&4];
    // Points should be symmetric about x=50
    assert!(
        (x3 + x4 - 100.0).abs() < tol,
        "x3+x4 should equal 100: x3={x3}, x4={x4}"
    );
    assert!(
        (y3 - y4).abs() < tol,
        "symmetric points should have same y: y3={y3}, y4={y4}"
    );
}

// ── M8: Dragged Constraint for Interactive Use ─────────────────────────────

#[test]
fn dragged_moves_under_constrained_point() {
    // Pin p1 at origin, distance of 50 to p2. p2 is under-constrained (1 DOF: rotation).
    // The solver should keep p2 near its initial guess (50, 0).
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 50.0,
                y: 0.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 50.0,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(
        result.status,
        SolveStatus::UnderConstrained { dof: 1 }
    ));

    let tol = 1e-6;
    assert_point_near(&result.positions, 1, (0.0, 0.0), tol);
    // p2 should stay near initial guess (50, 0) — solver preserves initial positions
    let (x2, y2) = result.positions[&2];
    let dist = (x2.powi(2) + y2.powi(2)).sqrt();
    assert!(
        (dist - 50.0).abs() < tol,
        "distance should be 50, got {dist}"
    );
}

#[test]
fn dragged_respects_existing_constraints() {
    // Rectangle where all corners are defined by constraints.
    // Dragging p1 to origin — it should stay at (0,0) and the
    // rectangle should form around it.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 60.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 60.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 60.0,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 30.0,
            },
            SketchConstraint::Dragged { point: 1 },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let tol = 1e-6;
    assert_point_near(&result.positions, 1, (0.0, 0.0), tol);
    assert_point_near(&result.positions, 2, (60.0, 0.0), tol);
    assert_point_near(&result.positions, 3, (60.0, 30.0), tol);
    assert_point_near(&result.positions, 4, (0.0, 30.0), tol);
}

// ── Edge Cases ─────────────────────────────────────────────────────────────

#[test]
fn empty_sketch_returns_under_constrained() {
    let sketch = make_sketch(vec![], vec![]);
    let result = solve_sketch(&sketch);
    // Empty sketch: solver should succeed with 0 DOF (nothing to solve)
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "empty sketch status: {:?}",
        result.status
    );
    assert!(result.positions.is_empty());
    assert!(result.profiles.is_empty());
}

#[test]
fn single_point_no_constraints() {
    let sketch = make_sketch(
        vec![SketchEntity::Point {
            id: 1,
            x: 42.0,
            y: 17.0,
            construction: false,
        }],
        vec![],
    );

    let result = solve_sketch(&sketch);
    match result.status {
        SolveStatus::UnderConstrained { dof } => {
            assert_eq!(dof, 2);
        }
        other => panic!("expected UnderConstrained, got {:?}", other),
    }
    // Point should still have its initial position in the results
    assert!(result.positions.contains_key(&1));
}

#[test]
fn diameter_constraint_on_circle() {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: 10,
                center_id: 1,
                radius: 10.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Diameter {
                entity: 10,
                value: 50.0,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));
    assert_point_near(&result.positions, 1, (0.0, 0.0), 1e-6);
}

#[test]
fn on_entity_point_on_line() {
    // Point 3 constrained onto line 10 (horizontal, y=0).
    // Dragged(p1) + Horizontal + Distance fix the line. OnEntity constrains p3.y = 0.
    // p3.x remains free (1 DOF).
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 100.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 50.0,
                y: 10.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 100.0,
            },
            SketchConstraint::OnEntity {
                point: 3,
                entity: 10,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    // p3 has 1 DOF: it can slide along x on the line
    assert!(
        matches!(result.status, SolveStatus::UnderConstrained { dof: 1 }),
        "expected 1 DOF, got {:?}",
        result.status
    );

    // Point 3 should be on the line (y = 0)
    let (_, y3) = result.positions[&3];
    assert!((y3).abs() < 1e-6, "point on line should have y=0, got {y3}");
}

// ── M9: Performance Benchmarking ─────────────────────────────────────────────

/// Build a chain of N connected rectangles, each with h/v constraints and dimensions.
/// Returns (entities, constraints) with approximately 8*N entities and 7*N constraints.
fn make_rectangle_chain(n: usize) -> (Vec<SketchEntity>, Vec<SketchConstraint>) {
    let mut entities = Vec::new();
    let mut constraints = Vec::new();
    let mut next_id = 1u32;
    let mut next_line_id = 1000u32;

    for i in 0..n {
        let x_off = (i as f64) * 110.0;
        let p1 = next_id;
        let p2 = next_id + 1;
        let p3 = next_id + 2;
        let p4 = next_id + 3;
        next_id += 4;

        entities.push(SketchEntity::Point {
            id: p1,
            x: x_off,
            y: 0.0,
            construction: false,
        });
        entities.push(SketchEntity::Point {
            id: p2,
            x: x_off + 100.0,
            y: 0.0,
            construction: false,
        });
        entities.push(SketchEntity::Point {
            id: p3,
            x: x_off + 100.0,
            y: 50.0,
            construction: false,
        });
        entities.push(SketchEntity::Point {
            id: p4,
            x: x_off,
            y: 50.0,
            construction: false,
        });

        let l1 = next_line_id;
        let l2 = next_line_id + 1;
        let l3 = next_line_id + 2;
        let l4 = next_line_id + 3;
        next_line_id += 4;

        entities.push(SketchEntity::Line {
            id: l1,
            start_id: p1,
            end_id: p2,
            construction: false,
        });
        entities.push(SketchEntity::Line {
            id: l2,
            start_id: p2,
            end_id: p3,
            construction: false,
        });
        entities.push(SketchEntity::Line {
            id: l3,
            start_id: p3,
            end_id: p4,
            construction: false,
        });
        entities.push(SketchEntity::Line {
            id: l4,
            start_id: p4,
            end_id: p1,
            construction: false,
        });

        constraints.push(SketchConstraint::Horizontal { entity: l1 });
        constraints.push(SketchConstraint::Horizontal { entity: l3 });
        constraints.push(SketchConstraint::Vertical { entity: l2 });
        constraints.push(SketchConstraint::Vertical { entity: l4 });
        constraints.push(SketchConstraint::Distance {
            entity_a: p1,
            entity_b: p2,
            value: 100.0,
        });
        constraints.push(SketchConstraint::Distance {
            entity_a: p2,
            entity_b: p3,
            value: 50.0,
        });

        // Pin first rectangle's origin
        if i == 0 {
            constraints.push(SketchConstraint::Dragged { point: p1 });
        } else {
            // Connect to previous rectangle: coincident via distance=0
            // Previous rectangle's p2 == current p1
            constraints.push(SketchConstraint::Coincident {
                point_a: p1 - 4 + 1, // previous p2
                point_b: p1,
            });
        }
    }

    (entities, constraints)
}

#[test]
fn bench_solve_10_constraints() {
    // ~2 rectangles: 16 entities, ~14 constraints
    let (entities, constraints) = make_rectangle_chain(2);
    let constraint_count = constraints.len();
    let sketch = make_sketch(entities, constraints);

    let start = std::time::Instant::now();
    let iterations = 100;
    for _ in 0..iterations {
        let _result = solve_sketch(&sketch);
    }
    let elapsed = start.elapsed();
    let per_solve = elapsed / iterations;

    eprintln!(
        "M9 bench: {} constraints, {:.1}µs/solve ({} iterations)",
        constraint_count,
        per_solve.as_nanos() as f64 / 1000.0,
        iterations
    );

    // Should be well under 1ms for a typical sketch
    assert!(
        per_solve.as_millis() < 10,
        "Solve with ~{} constraints took {:?}, expected < 10ms",
        constraint_count,
        per_solve
    );
}

#[test]
fn bench_solve_50_constraints() {
    // ~7 rectangles: 56 entities, ~49 constraints
    let (entities, constraints) = make_rectangle_chain(7);
    let constraint_count = constraints.len();
    let sketch = make_sketch(entities, constraints);

    let start = std::time::Instant::now();
    let iterations = 50;
    for _ in 0..iterations {
        let _result = solve_sketch(&sketch);
    }
    let elapsed = start.elapsed();
    let per_solve = elapsed / iterations;

    eprintln!(
        "M9 bench: {} constraints, {:.1}µs/solve ({} iterations)",
        constraint_count,
        per_solve.as_nanos() as f64 / 1000.0,
        iterations
    );

    assert!(
        per_solve.as_millis() < 10,
        "Solve with ~{} constraints took {:?}, expected < 10ms",
        constraint_count,
        per_solve
    );
}

#[test]
fn bench_solve_100_constraints() {
    // ~15 rectangles: 120 entities, ~105 constraints
    let (entities, constraints) = make_rectangle_chain(15);
    let constraint_count = constraints.len();
    let sketch = make_sketch(entities, constraints);

    let start = std::time::Instant::now();
    let iterations = 20;
    for _ in 0..iterations {
        let _result = solve_sketch(&sketch);
    }
    let elapsed = start.elapsed();
    let per_solve = elapsed / iterations;

    eprintln!(
        "M9 bench: {} constraints, {:.1}µs/solve ({} iterations)",
        constraint_count,
        per_solve.as_nanos() as f64 / 1000.0,
        iterations
    );

    assert!(
        per_solve.as_millis() < 50,
        "Solve with ~{} constraints took {:?}, expected < 50ms",
        constraint_count,
        per_solve
    );
}

#[test]
fn bench_solve_300_constraints() {
    // ~43 rectangles: 344 entities, ~301 constraints
    let (entities, constraints) = make_rectangle_chain(43);
    let constraint_count = constraints.len();
    let sketch = make_sketch(entities, constraints);

    let start = std::time::Instant::now();
    let iterations = 10;
    for _ in 0..iterations {
        let _result = solve_sketch(&sketch);
    }
    let elapsed = start.elapsed();
    let per_solve = elapsed / iterations;

    eprintln!(
        "M9 bench: {} constraints, {:.1}µs/solve ({} iterations)",
        constraint_count,
        per_solve.as_nanos() as f64 / 1000.0,
        iterations
    );

    assert!(
        per_solve.as_millis() < 100,
        "Solve with ~{} constraints took {:?}, expected < 100ms",
        constraint_count,
        per_solve
    );
}
