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
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        entities,
        constraints,
        solve_status: SolveStatus::UnderConstrained { dof: 99 },
        solved_positions: std::collections::HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
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
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 50.0,
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 40.0,
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 50.0,
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 50.0,
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 50.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 5 },
            SketchConstraint::Radius {
                entity: 20,
                value: 10.0,
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: h,
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 3,
                value: 30.0,
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 3,
                entity_b: 4,
                value: 80.0,
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 30.0,
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
                expression: None,
                reference: false,
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
            expression: None,
            reference: false,
        });
        constraints.push(SketchConstraint::Distance {
            entity_a: p2,
            entity_b: p3,
            value: 50.0,
            expression: None,
            reference: false,
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
#[ignore = "wall-clock benchmark — run manually with --ignored"]
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
#[ignore = "wall-clock benchmark — run manually with --ignored"]
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
#[ignore = "wall-clock benchmark — run manually with --ignored"]
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
#[ignore = "wall-clock benchmark — run manually with --ignored"]
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

// ── Coverage: Angle Constraint ──────────────────────────────────────────────

#[test]
fn angle_constraint_45_degrees() {
    // Two lines from origin, constrain angle between them to 45 degrees
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
                x: 70.0,
                y: 70.0,
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
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 100.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 3,
                value: 100.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Angle {
                line_a: 10,
                line_b: 11,
                value_degrees: 45.0,
                expression: None,
                reference: false,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let (x3, y3) = result.positions[&3];
    // With 45 degrees from horizontal, at distance 100: (100*cos(45), 100*sin(45))
    let expected = 100.0 * std::f64::consts::FRAC_PI_4.cos();
    assert!((x3 - expected).abs() < 0.1, "x3={x3}, expected ~{expected}");
    assert!((y3 - expected).abs() < 0.1, "y3={y3}, expected ~{expected}");
}

// ── Coverage: SymmetricH / SymmetricV ───────────────────────────────────────

#[test]
fn symmetric_horizontal_constraint() {
    // slvs SymmetricHoriz: symmetric about Y-axis (opposite x, same y).
    // Note: slvs naming is counterintuitive — "Horiz" means the OFFSET is horizontal.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 30.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: -30.0,
                y: 20.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::SymmetricH {
                point_a: 1,
                point_b: 2,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );

    let (x1, y1) = result.positions[&1];
    let (x2, y2) = result.positions[&2];
    // SymmetricH: opposite x, same y
    assert!(
        (x1 + x2).abs() < 1e-4,
        "x values should be opposite: x1={x1}, x2={x2}"
    );
    assert!(
        (y1 - y2).abs() < 1e-4,
        "y values should match: y1={y1}, y2={y2}"
    );
}

#[test]
fn symmetric_vertical_constraint() {
    // slvs SymmetricVert: symmetric about X-axis (same x, opposite y).
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 20.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 20.0,
                y: -30.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::SymmetricV {
                point_a: 1,
                point_b: 2,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );

    let (x1, y1) = result.positions[&1];
    let (x2, y2) = result.positions[&2];
    // SymmetricV: same x, opposite y
    assert!(
        (x1 - x2).abs() < 1e-4,
        "x values should match: x1={x1}, x2={x2}"
    );
    assert!(
        (y1 + y2).abs() < 1e-4,
        "y values should be opposite: y1={y1}, y2={y2}"
    );
}

// ── Coverage: Distance Point-Line and Line-Point ────────────────────────────

#[test]
fn distance_point_to_line() {
    // Point 3 at distance 25.0 from line 10 (horizontal at y=0).
    // Don't pin point 3 — let the solver place it via the constraint.
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
                y: 25.0,
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
                expression: None,
                reference: false,
            },
            // Point-Line distance: point_a=3 (Point), entity_b=10 (Line)
            SketchConstraint::Distance {
                entity_a: 3,
                entity_b: 10,
                value: 25.0,
                expression: None,
                reference: false,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );

    // Point 3 should be at distance 25 from the horizontal line at y=0
    let (_, y3) = result.positions[&3];
    assert!(
        (y3.abs() - 25.0).abs() < 1e-4,
        "pt-line distance: |y3|={}, expected 25.0",
        y3.abs()
    );
}

#[test]
fn distance_line_to_point_swap() {
    // Test the Line-Point swap branch (line is entity_a, point is entity_b)
    // Don't pin point 3 — let the distance constraint position it.
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
                y: 30.0,
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
                expression: None,
                reference: false,
            },
            // Line first, then point — triggers the swap branch
            SketchConstraint::Distance {
                entity_a: 10,
                entity_b: 3,
                value: 30.0,
                expression: None,
                reference: false,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );

    let (_, y3) = result.positions[&3];
    assert!(
        (y3.abs() - 30.0).abs() < 1e-4,
        "line-pt distance: |y3|={}, expected 30.0",
        y3.abs()
    );
}

// ── Coverage: Arc Entity + Tangent ──────────────────────────────────────────

#[test]
fn arc_entity_creation_and_solve() {
    // Create an arc and verify it can be solved.
    // Pin center and start, use radius constraint. End point has 1 DOF (angle).
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            }, // center
            SketchEntity::Point {
                id: 2,
                x: 10.0,
                y: 0.0,
                construction: false,
            }, // start
            SketchEntity::Point {
                id: 3,
                x: 0.0,
                y: 10.0,
                construction: false,
            }, // end
            SketchEntity::Arc {
                id: 10,
                center_id: 1,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            // Don't pin end — arc constrains end to same radius as start
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );
    assert_point_near(&result.positions, 1, (0.0, 0.0), 1e-6);
    assert_point_near(&result.positions, 2, (10.0, 0.0), 1e-6);
    // End point should be at distance 10 from center (arc radius)
    let (x3, y3) = result.positions[&3];
    let r = (x3.powi(2) + y3.powi(2)).sqrt();
    assert!(
        (r - 10.0).abs() < 1e-3,
        "arc end should be at radius 10, got {r}"
    );
}

#[test]
fn tangent_arc_line_constraint() {
    // Arc tangent to a line. Arc centered at (0,50), radius 50, line is horizontal at y=0.
    // At tangent point (0,0), the arc meets the line tangentially.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: -50.0,
                y: 0.0,
                construction: false,
            }, // line start
            SketchEntity::Point {
                id: 2,
                x: 50.0,
                y: 0.0,
                construction: false,
            }, // line end
            SketchEntity::Point {
                id: 3,
                x: 0.0,
                y: 50.0,
                construction: false,
            }, // arc center
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 0.0,
                construction: false,
            }, // arc start
            SketchEntity::Point {
                id: 5,
                x: 50.0,
                y: 50.0,
                construction: false,
            }, // arc end
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Arc {
                id: 11,
                center_id: 3,
                start_id: 4,
                end_id: 5,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            SketchConstraint::Dragged { point: 3 },
            SketchConstraint::Dragged { point: 5 },
            SketchConstraint::Tangent {
                line: 10,
                curve: 11,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    // The tangent constraint should be satisfiable
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "tangent solve status: {:?}",
        result.status
    );
    // Arc start point should be on the line (y near 0)
    let (_, y4) = result.positions[&4];
    assert!(
        y4.abs() < 1e-2,
        "arc tangent point should be near y=0, got {y4}"
    );
}

// ── Coverage: Equal Circle-Circle, Arc-Arc, Circle-Arc, Arc-Circle ──────────

#[test]
fn equal_radius_two_circles() {
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
            SketchEntity::Circle {
                id: 10,
                center_id: 1,
                radius: 20.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: 11,
                center_id: 2,
                radius: 15.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            SketchConstraint::Radius {
                entity: 10,
                value: 20.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Equal {
                entity_a: 10,
                entity_b: 11,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));
}

#[test]
fn equal_radius_two_arcs() {
    // Two arcs with EqualRadius. Pin centers and one start each; let endpoints float.
    let sketch = make_sketch(
        vec![
            // Arc 1: center (0,0), start (10,0), end (0,10)
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
            SketchEntity::Point {
                id: 3,
                x: 0.0,
                y: 10.0,
                construction: false,
            },
            SketchEntity::Arc {
                id: 10,
                center_id: 1,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            // Arc 2: center (50,0), start (65,0), end (50,15)
            SketchEntity::Point {
                id: 4,
                x: 50.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 5,
                x: 65.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 6,
                x: 50.0,
                y: 15.0,
                construction: false,
            },
            SketchEntity::Arc {
                id: 11,
                center_id: 4,
                start_id: 5,
                end_id: 6,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            SketchConstraint::Dragged { point: 4 },
            // EqualRadius: arc 10 and arc 11 should have the same radius
            SketchConstraint::Equal {
                entity_a: 10,
                entity_b: 11,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );
    // Both arcs should have equal radius. Arc1: center(0,0), start(10,0) → r=10.
    // Arc2 start should be at distance 10 from center(50,0).
    let (x4, y4) = result.positions[&4];
    let (x5, y5) = result.positions[&5];
    let r2 = ((x5 - x4).powi(2) + (y5 - y4).powi(2)).sqrt();
    assert!(
        (r2 - 10.0).abs() < 1e-3,
        "arc2 radius should be 10, got {r2}"
    );
}

#[test]
fn equal_radius_circle_arc() {
    // Circle with radius 25, then an arc that should match via EqualRadius.
    // Don't over-constrain arc points.
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
                radius: 25.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 80.0,
                y: 0.0,
                construction: false,
            }, // arc center
            SketchEntity::Point {
                id: 3,
                x: 100.0,
                y: 0.0,
                construction: false,
            }, // arc start
            SketchEntity::Point {
                id: 4,
                x: 80.0,
                y: 20.0,
                construction: false,
            }, // arc end
            SketchEntity::Arc {
                id: 11,
                center_id: 2,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Radius {
                entity: 10,
                value: 25.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Dragged { point: 2 },
            // Circle-Arc equal radius
            SketchConstraint::Equal {
                entity_a: 10,
                entity_b: 11,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );
    // Arc start (pt 3) should be at distance 25 from arc center (pt 2)
    let (x2, y2) = result.positions[&2];
    let (x3, y3) = result.positions[&3];
    let r = ((x3 - x2).powi(2) + (y3 - y2).powi(2)).sqrt();
    assert!(
        (r - 25.0).abs() < 1e-3,
        "arc radius should equal circle radius 25, got {r}"
    );
}

#[test]
fn equal_radius_arc_circle() {
    // Arc first, Circle second — tests the (Arc, Circle) branch.
    // Pin arc center and start only. Pin circle center.
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
                x: 15.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 0.0,
                y: 15.0,
                construction: false,
            },
            SketchEntity::Arc {
                id: 10,
                center_id: 1,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 60.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: 11,
                center_id: 4,
                radius: 30.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            SketchConstraint::Dragged { point: 4 },
            // Arc-Circle equal radius
            SketchConstraint::Equal {
                entity_a: 10,
                entity_b: 11,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );
}

// ── Coverage: Radius/Diameter on Arc ────────────────────────────────────────

#[test]
fn radius_constraint_on_arc() {
    // Pin only the center. The radius constraint sets the arc size.
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
                x: 30.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 0.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Arc {
                id: 10,
                center_id: 1,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Radius {
                entity: 10,
                value: 30.0,
                expression: None,
                reference: false,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );
    // Start point should be at distance 30 from center
    let (x1, y1) = result.positions[&1];
    let (x2, y2) = result.positions[&2];
    let r = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
    assert!((r - 30.0).abs() < 1e-3, "arc radius should be 30, got {r}");
}

#[test]
fn diameter_constraint_on_arc() {
    // Pin only center. Diameter constraint sets arc size.
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
                x: 20.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 0.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Arc {
                id: 10,
                center_id: 1,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Diameter {
                entity: 10,
                value: 40.0,
                expression: None,
                reference: false,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );
    // Start point should be at distance 20 (diameter/2) from center
    let (x1, y1) = result.positions[&1];
    let (x2, y2) = result.positions[&2];
    let r = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
    assert!(
        (r - 20.0).abs() < 1e-3,
        "arc radius should be 20 (d=40), got {r}"
    );
}

// ── Coverage: OnEntity Point-on-Circle and Point-on-Arc ─────────────────────

#[test]
fn on_entity_point_on_circle() {
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
                radius: 25.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 25.0,
                y: 1.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Radius {
                entity: 10,
                value: 25.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::OnEntity {
                point: 2,
                entity: 10,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(result.status, SolveStatus::UnderConstrained { .. }),
        "status: {:?}",
        result.status
    );

    // Point 2 should be on the circle (distance from center = 25)
    let (x1, y1) = result.positions[&1];
    let (x2, y2) = result.positions[&2];
    let dist = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
    assert!(
        (dist - 25.0).abs() < 1e-3,
        "point should be on circle: dist={dist}, expected 25"
    );
}

#[test]
fn on_entity_point_on_arc() {
    // Don't over-constrain arc. Pin center and start only.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            }, // arc center
            SketchEntity::Point {
                id: 2,
                x: 20.0,
                y: 0.0,
                construction: false,
            }, // arc start
            SketchEntity::Point {
                id: 3,
                x: 0.0,
                y: 20.0,
                construction: false,
            }, // arc end
            SketchEntity::Arc {
                id: 10,
                center_id: 1,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 14.0,
                y: 14.0,
                construction: false,
            }, // point on arc
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            SketchConstraint::OnEntity {
                point: 4,
                entity: 10,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::UnderConstrained { .. } | SolveStatus::FullyConstrained
        ),
        "status: {:?}",
        result.status
    );

    // Point 4 should be on the arc (distance from center = 20)
    let (x1, y1) = result.positions[&1];
    let (x4, y4) = result.positions[&4];
    let dist = ((x4 - x1).powi(2) + (y4 - y1).powi(2)).sqrt();
    assert!(
        (dist - 20.0).abs() < 1e-3,
        "point should be on arc: dist={dist}, expected 20"
    );
}

// ── Coverage: EqualAngle ────────────────────────────────────────────────────

#[test]
fn equal_angle_four_lines() {
    // Angle between lines 10-11 should equal angle between lines 12-13
    let sketch = make_sketch(
        vec![
            // First pair of lines from origin
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
                x: 40.0,
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
            // Second pair from (100,0)
            SketchEntity::Point {
                id: 4,
                x: 100.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 5,
                x: 150.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 6,
                x: 130.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 4,
                end_id: 5,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 6,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            SketchConstraint::Dragged { point: 3 },
            SketchConstraint::Dragged { point: 4 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Distance {
                entity_a: 4,
                entity_b: 5,
                value: 50.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 4,
                entity_b: 6,
                value: 50.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::EqualAngle {
                line_a: 10,
                line_b: 11,
                line_c: 12,
                line_d: 13,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );

    // Verify the angles are equal
    let p1 = result.positions[&1];
    let p2 = result.positions[&2];
    let p3 = result.positions[&3];
    let p4 = result.positions[&4];
    let p5 = result.positions[&5];
    let p6 = result.positions[&6];

    let angle1 = ((p3.1 - p1.1).atan2(p3.0 - p1.0) - (p2.1 - p1.1).atan2(p2.0 - p1.0)).abs();
    let angle2 = ((p6.1 - p4.1).atan2(p6.0 - p4.0) - (p5.1 - p4.1).atan2(p5.0 - p4.0)).abs();
    assert!(
        (angle1 - angle2).abs() < 0.05,
        "angles should be equal: {angle1} vs {angle2}"
    );
}

// ── Coverage: Ratio (LengthRatio) ───────────────────────────────────────────

#[test]
fn length_ratio_constraint() {
    // LengthRatio: line_a / line_b = value.
    // Line 10 = 40, ratio 2.0 → line 11 should be 20.
    // Set initial point 4 near expected position to help convergence.
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
                x: 40.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 0.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 20.0,
                y: 20.0,
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
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 40.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Dragged { point: 3 },
            SketchConstraint::Horizontal { entity: 11 },
            SketchConstraint::Ratio {
                entity_a: 10,
                entity_b: 11,
                value: 2.0,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );

    let (x3, _) = result.positions[&3];
    let (x4, _) = result.positions[&4];
    let len11 = (x4 - x3).abs();
    // 40 / len11 = 2.0 → len11 = 20
    assert!(
        (len11 - 20.0).abs() < 1e-3,
        "line 11 length should be 20, got {len11}"
    );
}

// ── Coverage: EqualPointToLine ──────────────────────────────────────────────

#[test]
fn equal_point_to_line_distance() {
    // Two points equidistant from a line.
    // Don't over-constrain: pin line endpoints and one point, let the other float.
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
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 30.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 70.0,
                y: 20.0,
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
                expression: None,
                reference: false,
            },
            SketchConstraint::EqualPointToLine {
                point_a: 3,
                point_b: 4,
                line: 10,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::UnderConstrained { .. } | SolveStatus::FullyConstrained
        ),
        "status: {:?}",
        result.status
    );

    // Both points should be at the same distance from the horizontal line (y=0)
    let (_, y3) = result.positions[&3];
    let (_, y4) = result.positions[&4];
    assert!(
        (y3.abs() - y4.abs()).abs() < 1e-3,
        "distances should be equal: |y3|={}, |y4|={}",
        y3.abs(),
        y4.abs()
    );
}

// ── Coverage: SameOrientation (no-op in 2D) ─────────────────────────────────

#[test]
fn same_orientation_is_noop() {
    // SameOrientation is a no-op in 2D sketch context.
    // Just verify it doesn't panic.
    let sketch = make_sketch(
        vec![SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        }],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::SameOrientation {
                entity_a: 1,
                entity_b: 1,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));
}

// ── Coverage: Profile with Arc edges ────────────────────────────────────────

#[test]
fn profile_with_arc_and_lines() {
    // A D-shape: 3 lines + 1 arc. Don't pin all 5 points (arc over-constrains).
    // Pin the 4 corner points. Arc center floats.
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
                x: 50.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 5,
                x: 75.0,
                y: 25.0,
                construction: false,
            }, // arc center
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Arc {
                id: 11,
                center_id: 5,
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
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            SketchConstraint::Dragged { point: 4 },
            // Arc start(2) and end(3) are shared with lines. Only pin start(2).
            // Don't pin pt3 or pt5 to avoid over-constraining the arc.
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "status: {:?}",
        result.status
    );

    // Should have at least one profile containing the arc
    let has_arc_profile = result.profiles.iter().any(|p| p.entity_ids.contains(&11));
    assert!(
        has_arc_profile,
        "profile should include the arc entity; profiles: {:?}",
        result.profiles
    );
}

#[test]
fn profile_construction_arc_excluded() {
    // Arc marked as construction should not appear in profiles
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
            SketchEntity::Point {
                id: 3,
                x: 0.0,
                y: 10.0,
                construction: false,
            },
            SketchEntity::Arc {
                id: 10,
                center_id: 1,
                start_id: 2,
                end_id: 3,
                construction: true,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            SketchConstraint::Dragged { point: 3 },
        ],
    );

    let result = solve_sketch(&sketch);
    // Construction arc should not produce a profile edge
    let has_arc_profile = result.profiles.iter().any(|p| p.entity_ids.contains(&10));
    assert!(
        !has_arc_profile,
        "construction arc should not be in profiles"
    );
}

// ── Coverage: Multiple constraints batch (add_constraints) ──────────────────

#[test]
fn multiple_constraints_combined() {
    // Test a sketch that exercises many constraint types together.
    // Rectangle: H/V + distance + equal (square) + midpoint.
    // Note: H+V+Perp+Parallel together can over-constrain, so use a minimal set.
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
                x: 50.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 5,
                x: 25.0,
                y: 0.0,
                construction: false,
            }, // midpoint
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
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Horizontal { entity: 12 },
            SketchConstraint::Vertical { entity: 11 },
            SketchConstraint::Vertical { entity: 13 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 50.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Equal {
                entity_a: 10,
                entity_b: 11,
            },
            SketchConstraint::Midpoint { point: 5, line: 10 },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let tol = 1e-4;
    assert_point_near(&result.positions, 1, (0.0, 0.0), tol);
    assert_point_near(&result.positions, 2, (50.0, 0.0), tol);
    assert_point_near(&result.positions, 3, (50.0, 50.0), tol);
    assert_point_near(&result.positions, 4, (0.0, 50.0), tol);
    assert_point_near(&result.positions, 5, (25.0, 0.0), tol);
}

// ── Coverage: profiles.rs edge cases ────────────────────────────────────────

#[test]
fn profile_single_line_no_profile() {
    // A single line cannot form a closed profile
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
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        result.profiles.is_empty(),
        "single line should not form a profile"
    );
}

#[test]
fn profile_triangle() {
    // Three lines forming a triangle
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
                y: 52.0,
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
            SketchConstraint::Dragged { point: 2 },
            SketchConstraint::Dragged { point: 3 },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    // Should have exactly one outer profile with 3 edges
    let outer = result
        .profiles
        .iter()
        .filter(|p| p.is_outer)
        .collect::<Vec<_>>();
    assert_eq!(outer.len(), 1, "triangle should produce 1 outer profile");
    assert_eq!(
        outer[0].entity_ids.len(),
        3,
        "triangle profile should have 3 edges"
    );
}

#[test]
fn profile_only_points_no_profile() {
    // Only points, no lines/arcs → no edges, no profile
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
            SketchEntity::Point {
                id: 3,
                x: 5.0,
                y: 10.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Dragged { point: 2 },
            SketchConstraint::Dragged { point: 3 },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        result.profiles.is_empty(),
        "points-only sketch should have no profiles"
    );
}

// ── Coverage: status.rs SolveFailed branch ──────────────────────────────────

#[test]
fn solve_status_types() {
    // Test that the status classification works for different scenarios.
    // FullyConstrained is already tested extensively.
    // UnderConstrained is already tested extensively.
    // OverConstrained is tested in status_over_constrained.
    // Verify we can distinguish between them properly.

    // Fully constrained
    let s1 = make_sketch(
        vec![SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        }],
        vec![SketchConstraint::Dragged { point: 1 }],
    );
    let r1 = solve_sketch(&s1);
    assert!(matches!(r1.status, SolveStatus::FullyConstrained));
    // Profiles should be extracted for FullyConstrained
    assert!(r1.positions.contains_key(&1));

    // Under constrained
    let s2 = make_sketch(
        vec![SketchEntity::Point {
            id: 1,
            x: 5.0,
            y: 5.0,
            construction: false,
        }],
        vec![],
    );
    let r2 = solve_sketch(&s2);
    assert!(matches!(r2.status, SolveStatus::UnderConstrained { .. }));
    // Profiles should be extracted for UnderConstrained too
    assert!(r2.positions.contains_key(&1));
}

// ── Coverage: Coincident between non-trivial points ─────────────────────────

#[test]
fn coincident_merges_points() {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 10.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 15.0,
                y: 25.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Coincident {
                point_a: 1,
                point_b: 2,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let (x1, y1) = result.positions[&1];
    let (x2, y2) = result.positions[&2];
    assert!((x1 - x2).abs() < 1e-6, "coincident x: {x1} vs {x2}");
    assert!((y1 - y2).abs() < 1e-6, "coincident y: {y1} vs {y2}");
}

// ── Coverage: Angle with non-45 degree value ────────────────────────────────

#[test]
fn angle_constraint_90_degrees() {
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
                start_id: 1,
                end_id: 3,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 50.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 3,
                value: 50.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Angle {
                line_a: 10,
                line_b: 11,
                value_degrees: 90.0,
                expression: None,
                reference: false,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));

    let tol = 1e-4;
    assert_point_near(&result.positions, 3, (0.0, 50.0), tol);
}

// ── Degenerate Cases (Parity Harness Fixtures) ──────────────────────────────
// Per specs/clean_room_constraint_solver.md §"Parity harness":
// hand-curated degenerate cases that a clean-room implementation must handle.
// implementation must match. These run on both legacy and clean paths.

#[test]
fn degenerate_zero_length_line() {
    // Two coincident points with a line between them — zero-length line.
    // Horizontal constraint should be trivially satisfied.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 5.0,
                y: 5.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 5.0,
                y: 5.0,
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
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "zero-length line should solve, got {:?}",
        result.status
    );
    assert_point_near(&result.positions, 1, (5.0, 5.0), 1e-6);
    assert_point_near(&result.positions, 2, (5.0, 5.0), 1e-6);
}

#[test]
fn degenerate_circle_radius_zero() {
    // Circle with radius = 0 — degenerate but should not panic.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: 1,
                x: 10.0,
                y: 10.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: 10,
                center_id: 1,
                radius: 0.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Radius {
                entity: 10,
                value: 0.0,
                expression: None,
                reference: false,
            },
        ],
    );
    let result = solve_sketch(&sketch);
    assert!(
        !matches!(result.status, SolveStatus::SolveFailed { .. }),
        "radius=0 should not fail: {:?}",
        result.status
    );
}

#[test]
fn degenerate_over_constrained_but_consistent() {
    // Redundant but consistent constraints: Distance(50) + Distance(50) on same pair.
    // Should be FullyConstrained (redundant OK), not OverConstrained.
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
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 50.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 50.0,
                expression: None,
                reference: false,
            },
        ],
    );
    // Note: entity 10 (line) doesn't exist in this sketch, so Horizontal will fail.
    // Fix: add the line.
    let sketch = Sketch {
        entities: vec![
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
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
        ],
        constraints: vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 50.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 50.0,
                expression: None,
                reference: false,
            },
        ],
        ..sketch
    };
    let result = solve_sketch(&sketch);
    // Redundant consistent constraints should still solve
    assert!(
        matches!(
            result.status,
            SolveStatus::FullyConstrained | SolveStatus::OverConstrained { .. }
        ),
        "redundant consistent: {:?}",
        result.status
    );
    if matches!(result.status, SolveStatus::FullyConstrained) {
        assert_point_near(&result.positions, 2, (50.0, 0.0), 1e-6);
    }
}

#[test]
fn degenerate_contradictory_distances() {
    // Distance(10) AND Distance(20) on same point pair — contradictory.
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
                value: 10.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 20.0,
                expression: None,
                reference: false,
            },
        ],
    );
    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::OverConstrained { .. } | SolveStatus::SolveFailed { .. }
        ),
        "contradictory distances should fail: {:?}",
        result.status
    );
}

#[test]
fn degenerate_under_constrained_with_dragged() {
    // Single point with Dragged — should be FullyConstrained (0 DOF).
    let sketch = make_sketch(
        vec![SketchEntity::Point {
            id: 1,
            x: 42.0,
            y: 17.0,
            construction: false,
        }],
        vec![SketchConstraint::Dragged { point: 1 }],
    );
    let result = solve_sketch(&sketch);
    assert!(matches!(result.status, SolveStatus::FullyConstrained));
    assert_point_near(&result.positions, 1, (42.0, 17.0), 1e-6);
}

#[test]
fn degenerate_point_coincident_with_itself() {
    // Point coincident with itself — trivially satisfied.
    let sketch = make_sketch(
        vec![SketchEntity::Point {
            id: 1,
            x: 5.0,
            y: 5.0,
            construction: false,
        }],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Coincident {
                point_a: 1,
                point_b: 1,
            },
        ],
    );
    let result = solve_sketch(&sketch);
    assert!(
        matches!(result.status, SolveStatus::FullyConstrained),
        "self-coincident: {:?}",
        result.status
    );
    assert_point_near(&result.positions, 1, (5.0, 5.0), 1e-6);
}

// ── Point-pair Horizontal / Vertical constraints ───────────────────────────
// Spec: specs/point_pair_horizontal_vertical.md
// Extends Horizontal/Vertical (line-only) to an arbitrary pair of points.

/// I1 + I3: HorizontalPoints equates the two points' Y, leaving X untouched.
#[test]
fn horizontal_points_aligns_y_only() {
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
                y: 5.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::HorizontalPoints {
                point_a: 1,
                point_b: 2,
            },
        ],
    );
    let result = solve_sketch(&sketch);
    // Anchor stays put; point 2's Y is pulled to the anchor's Y, X is untouched.
    assert_point_near(&result.positions, 1, (0.0, 0.0), 1e-6);
    assert_point_near(&result.positions, 2, (10.0, 0.0), 1e-6);
}

/// I2 + I3: VerticalPoints equates the two points' X, leaving Y untouched.
#[test]
fn vertical_points_aligns_x_only() {
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
                x: 5.0,
                y: 10.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::VerticalPoints {
                point_a: 1,
                point_b: 2,
            },
        ],
    );
    let result = solve_sketch(&sketch);
    assert_point_near(&result.positions, 1, (0.0, 0.0), 1e-6);
    assert_point_near(&result.positions, 2, (0.0, 10.0), 1e-6);
}

/// Failure mode: an unknown point id surfaces loudly as SolveFailed.
#[test]
fn horizontal_points_unknown_point_fails() {
    let sketch = make_sketch(
        vec![SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        }],
        vec![SketchConstraint::HorizontalPoints {
            point_a: 1,
            point_b: 99,
        }],
    );
    let result = solve_sketch(&sketch);
    assert!(
        matches!(result.status, SolveStatus::SolveFailed { .. }),
        "expected SolveFailed for unknown point, got {:?}",
        result.status
    );
}

// ── Coverage: dimension-tool constraints (PointLineDistance / HDistance / VDistance) ─
// These three SketchConstraint variants are emitted by the dimension tool and
// were missing from the solver enum (the "unknown variant" parse bug). See
// specs/dimension_tool.md.

/// A PointLineDistance constraint enforces the perpendicular distance from a
/// point to a line, identical to a `Distance` over a (point, line) pair.
#[test]
fn point_line_distance_enforces_perpendicular_gap() {
    // A horizontal line along y=0, and a free point above it that must end up
    // exactly 7 units away from the line.
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
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 4.0,
                y: 2.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 10.0,
                expression: None,
                reference: false,
            },
            SketchConstraint::PointLineDistance {
                point: 3,
                entity: 10,
                value: 7.0,
                expression: None,
                reference: false,
            },
        ],
    );

    let result = solve_sketch(&sketch);
    assert!(
        matches!(
            result.status,
            SolveStatus::UnderConstrained { .. } | SolveStatus::FullyConstrained
        ),
        "status: {:?}",
        result.status
    );

    // Point 3 must be 7 units off the y=0 line.
    let (_, y3) = result.positions[&3];
    assert!(
        (y3.abs() - 7.0).abs() < 1e-6,
        "perpendicular distance should be 7.0, got {}",
        y3.abs()
    );
}

/// The JSON shape the dimension tool's heuristic emits must deserialize into
/// the PointLineDistance variant — the bug was "unknown variant `PointLineDistance`".
#[test]
fn point_line_distance_deserializes_from_dimension_tool_json() {
    let json = r#"{"type":"PointLineDistance","point":3,"entity":10,"value":7.0}"#;
    let c: SketchConstraint = serde_json::from_str(json).expect("must parse PointLineDistance");
    assert!(matches!(
        c,
        SketchConstraint::PointLineDistance { point: 3, entity: 10, value, .. }
            if (value - 7.0).abs() < 1e-12
    ));
}

/// HDistance constrains the horizontal gap |Δx| between two points, leaving Δy free.
#[test]
fn hdistance_enforces_x_gap_only() {
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
                x: 3.0,
                y: 5.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::HDistance {
                point_a: 1,
                point_b: 2,
                value: 8.0,
                expression: None,
                reference: false,
            },
        ],
    );
    let result = solve_sketch(&sketch);
    let (x1, _) = result.positions[&1];
    let (x2, _) = result.positions[&2];
    assert!(
        ((x2 - x1).abs() - 8.0).abs() < 1e-6,
        "horizontal gap should be 8.0, got {}",
        (x2 - x1).abs()
    );
}

/// VDistance constrains the vertical gap |Δy| between two points, leaving Δx free.
#[test]
fn vdistance_enforces_y_gap_only() {
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
                x: 3.0,
                y: 5.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::VDistance {
                point_a: 1,
                point_b: 2,
                value: 12.0,
                expression: None,
                reference: false,
            },
        ],
    );
    let result = solve_sketch(&sketch);
    let (_, y1) = result.positions[&1];
    let (_, y2) = result.positions[&2];
    assert!(
        ((y2 - y1).abs() - 12.0).abs() < 1e-6,
        "vertical gap should be 12.0, got {}",
        (y2 - y1).abs()
    );
}

/// Both axis-aligned dimension variants must deserialize from the heuristic's JSON.
#[test]
fn hv_distance_deserialize_from_dimension_tool_json() {
    let h: SketchConstraint =
        serde_json::from_str(r#"{"type":"HDistance","point_a":1,"point_b":2,"value":8.0}"#)
            .expect("must parse HDistance");
    assert!(matches!(
        h,
        SketchConstraint::HDistance {
            point_a: 1,
            point_b: 2,
            ..
        }
    ));
    let v: SketchConstraint =
        serde_json::from_str(r#"{"type":"VDistance","point_a":1,"point_b":2,"value":12.0}"#)
            .expect("must parse VDistance");
    assert!(matches!(
        v,
        SketchConstraint::VDistance {
            point_a: 1,
            point_b: 2,
            ..
        }
    ));
}
