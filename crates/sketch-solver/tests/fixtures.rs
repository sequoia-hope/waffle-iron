//! Shared test fixtures for render and golden tests.

use sketch_solver::*;
use std::collections::HashMap;
use uuid::Uuid;

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

pub fn make_sketch(entities: Vec<SketchEntity>, constraints: Vec<SketchConstraint>) -> Sketch {
    Sketch {
        id: Uuid::nil(), // deterministic for golden tests
        plane: dummy_geom_ref(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        entities,
        constraints,
        solve_status: SolveStatus::UnderConstrained { dof: 99 },
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
    }
}

/// Fully constrained rectangle: 4 points, 4 lines, H/V + distance + dragged.
pub fn rectangle_sketch() -> Sketch {
    make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 100.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 100.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 0.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(10),
                start_id: PointId(1),
                end_id: PointId(2),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(11),
                start_id: PointId(2),
                end_id: PointId(3),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(12),
                start_id: PointId(3),
                end_id: PointId(4),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(13),
                start_id: PointId(4),
                end_id: PointId(1),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Horizontal {
                entity: EntityId(12),
            },
            SketchConstraint::Vertical {
                entity: EntityId(11),
            },
            SketchConstraint::Vertical {
                entity: EntityId(13),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 100.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(2),
                entity_b: EntityId(3),
                value: 50.0,
            },
            SketchConstraint::Dragged { point: PointId(1) },
        ],
    )
}

/// Circle with center and radius constraint.
pub fn circle_sketch() -> Sketch {
    make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 50.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: CircleId(10),
                center_id: PointId(1),
                radius: 30.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Radius {
                entity: EntityId(10),
                value: 30.0,
            },
        ],
    )
}

/// Equilateral triangle with all sides equal.
pub fn triangle_sketch() -> Sketch {
    make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 60.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 30.0,
                y: 51.96,
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(10),
                start_id: PointId(1),
                end_id: PointId(2),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(11),
                start_id: PointId(2),
                end_id: PointId(3),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(12),
                start_id: PointId(3),
                end_id: PointId(1),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 60.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(2),
                entity_b: EntityId(3),
                value: 60.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(3),
                entity_b: EntityId(1),
                value: 60.0,
            },
            SketchConstraint::Dragged { point: PointId(1) },
        ],
    )
}

/// Bracket with mixed constraints: lines, arcs, distance, angle, tangent.
pub fn bracket_sketch() -> Sketch {
    // Simple bracket: L-shape with rounded corner (arc tangent to two lines)
    make_sketch(
        vec![
            // L-shape outer vertices
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 80.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 80.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 30.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(5),
                x: 30.0,
                y: 60.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(6),
                x: 0.0,
                y: 60.0,
                construction: false,
            },
            // Arc center for rounded inner corner
            SketchEntity::Point {
                id: PointId(7),
                x: 30.0,
                y: 30.0,
                construction: false,
            },
            // Lines forming the L
            SketchEntity::Line {
                id: LineId(20),
                start_id: PointId(1),
                end_id: PointId(2),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(21),
                start_id: PointId(2),
                end_id: PointId(3),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(22),
                start_id: PointId(3),
                end_id: PointId(4),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(23),
                start_id: PointId(4),
                end_id: PointId(5),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(24),
                start_id: PointId(5),
                end_id: PointId(6),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(25),
                start_id: PointId(6),
                end_id: PointId(1),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal {
                entity: EntityId(20),
            },
            SketchConstraint::Horizontal {
                entity: EntityId(22),
            },
            SketchConstraint::Horizontal {
                entity: EntityId(24),
            },
            SketchConstraint::Vertical {
                entity: EntityId(21),
            },
            SketchConstraint::Vertical {
                entity: EntityId(23),
            },
            SketchConstraint::Vertical {
                entity: EntityId(25),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 80.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(2),
                entity_b: EntityId(3),
                value: 20.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(5),
                entity_b: EntityId(6),
                value: 30.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(6),
                entity_b: EntityId(1),
                value: 60.0,
            },
            SketchConstraint::Dragged { point: PointId(1) },
        ],
    )
}

/// Under-constrained: 3 points, 2 lines, only horizontal on one line.
pub fn underconstrained_sketch() -> Sketch {
    make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 50.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 25.0,
                y: 40.0,
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(10),
                start_id: PointId(1),
                end_id: PointId(2),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(11),
                start_id: PointId(2),
                end_id: PointId(3),
                construction: false,
            },
        ],
        vec![SketchConstraint::Horizontal {
            entity: EntityId(10),
        }],
    )
}

/// Over-constrained: conflicting distance constraints.
pub fn overconstrained_sketch() -> Sketch {
    make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 50.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(10),
                start_id: PointId(1),
                end_id: PointId(2),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 50.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 100.0,
            },
            SketchConstraint::Dragged { point: PointId(1) },
        ],
    )
}

/// Return all named fixtures as (name, sketch) pairs.
pub fn all_fixtures() -> Vec<(&'static str, Sketch)> {
    vec![
        ("rectangle", rectangle_sketch()),
        ("circle", circle_sketch()),
        ("triangle", triangle_sketch()),
        ("bracket", bracket_sketch()),
        ("underconstrained", underconstrained_sketch()),
        ("overconstrained", overconstrained_sketch()),
    ]
}
