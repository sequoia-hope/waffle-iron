use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use sketch_solver::*;
use std::fs;
use std::path::Path;
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
    }
}

fn gen_rand_rect(rng: &mut StdRng, id_off: u32) -> Sketch {
    let w = rng.gen_range(50.0..200.0);
    let h = rng.gen_range(50.0..150.0);
    let x0 = rng.gen_range(-100.0..100.0);
    let y0 = rng.gen_range(-100.0..100.0);

    let entities = vec![
        SketchEntity::Point {
            id: PointId(id_off + 1),
            x: x0,
            y: y0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(id_off + 2),
            x: x0 + w,
            y: y0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(id_off + 3),
            x: x0 + w,
            y: y0 + h,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(id_off + 4),
            x: x0,
            y: y0 + h,
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(id_off + 10),
            start_id: PointId(id_off + 1),
            end_id: PointId(id_off + 2),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(id_off + 11),
            start_id: PointId(id_off + 2),
            end_id: PointId(id_off + 3),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(id_off + 12),
            start_id: PointId(id_off + 3),
            end_id: PointId(id_off + 4),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(id_off + 13),
            start_id: PointId(id_off + 4),
            end_id: PointId(id_off + 1),
            construction: false,
        },
    ];
    let constraints = vec![
        SketchConstraint::Horizontal {
            entity: EntityId(id_off + 10),
        },
        SketchConstraint::Horizontal {
            entity: EntityId(id_off + 12),
        },
        SketchConstraint::Vertical {
            entity: EntityId(id_off + 11),
        },
        SketchConstraint::Vertical {
            entity: EntityId(id_off + 13),
        },
        SketchConstraint::Distance {
            entity_a: EntityId(id_off + 1),
            entity_b: EntityId(id_off + 2),
            value: w,
        },
        SketchConstraint::Distance {
            entity_a: EntityId(id_off + 2),
            entity_b: EntityId(id_off + 3),
            value: h,
        },
        SketchConstraint::Dragged {
            point: PointId(id_off + 1),
        },
    ];
    make_sketch(entities, constraints)
}

fn gen_rand_triangle(rng: &mut StdRng, id_off: u32) -> Sketch {
    let s = rng.gen_range(40.0..120.0);
    let x0 = rng.gen_range(-50.0..50.0);
    let y0 = rng.gen_range(-50.0..50.0);

    let entities = vec![
        SketchEntity::Point {
            id: PointId(id_off + 1),
            x: x0,
            y: y0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(id_off + 2),
            x: x0 + s,
            y: y0,
            construction: false,
        },
        SketchEntity::Point {
            id: PointId(id_off + 3),
            x: x0 + s / 2.0,
            y: y0 + s * 0.866,
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(id_off + 10),
            start_id: PointId(id_off + 1),
            end_id: PointId(id_off + 2),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(id_off + 11),
            start_id: PointId(id_off + 2),
            end_id: PointId(id_off + 3),
            construction: false,
        },
        SketchEntity::Line {
            id: LineId(id_off + 12),
            start_id: PointId(id_off + 3),
            end_id: PointId(id_off + 1),
            construction: false,
        },
    ];
    let constraints = vec![
        SketchConstraint::Distance {
            entity_a: EntityId(id_off + 1),
            entity_b: EntityId(id_off + 2),
            value: s,
        },
        SketchConstraint::Distance {
            entity_a: EntityId(id_off + 2),
            entity_b: EntityId(id_off + 3),
            value: s,
        },
        SketchConstraint::Distance {
            entity_a: EntityId(id_off + 3),
            entity_b: EntityId(id_off + 1),
            value: s,
        },
        SketchConstraint::Horizontal {
            entity: EntityId(id_off + 10),
        },
        SketchConstraint::Dragged {
            point: PointId(id_off + 1),
        },
    ];
    make_sketch(entities, constraints)
}

fn gen_rand_circle(rng: &mut StdRng, id_off: u32) -> Sketch {
    let r = rng.gen_range(10.0..100.0);
    let x = rng.gen_range(-100.0..100.0);
    let y = rng.gen_range(-100.0..100.0);

    let entities = vec![
        SketchEntity::Point {
            id: PointId(id_off + 1),
            x,
            y,
            construction: false,
        },
        SketchEntity::Circle {
            id: CircleId(id_off + 10),
            center_id: PointId(id_off + 1),
            radius: r,
            construction: false,
        },
    ];
    let constraints = vec![
        SketchConstraint::Dragged {
            point: PointId(id_off + 1),
        },
        SketchConstraint::Radius {
            entity: EntityId(id_off + 10),
            value: r,
        },
    ];
    make_sketch(entities, constraints)
}

fn sketch_rectangle_100x50_fully_constrained() -> Sketch {
    let sketch = make_sketch(
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
    );
    sketch
}

fn sketch_circle_center_and_radius() -> Sketch {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 25.0,
                y: 25.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: CircleId(10),
                center_id: PointId(1),
                radius: 15.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Radius {
                entity: EntityId(10),
                value: 15.0,
            },
        ],
    );
    sketch
}

fn sketch_equilateral_triangle_equal_lengths() -> Sketch {
    // Three points forming a triangle, all sides equal = 60mm
    // Fix one side horizontal to remove rotation DOF
    let sketch = make_sketch(
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
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 60.0,
            },
            SketchConstraint::Equal {
                entity_a: EntityId(10),
                entity_b: EntityId(11),
            },
            SketchConstraint::Equal {
                entity_a: EntityId(11),
                entity_b: EntityId(12),
            },
        ],
    );
    sketch
}

fn sketch_two_points_with_distance() -> Sketch {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 42.0,
                y: 0.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 42.0,
            },
        ],
    );
    sketch
}

fn sketch_status_fully_constrained() -> Sketch {
    // Single point pinned at origin
    let sketch = make_sketch(
        vec![SketchEntity::Point {
            id: PointId(1),
            x: 0.0,
            y: 0.0,
            construction: false,
        }],
        vec![SketchConstraint::Dragged { point: PointId(1) }],
    );
    sketch
}

fn sketch_status_under_constrained() -> Sketch {
    // Two points, no constraints linking them — each free point has 2 DOF
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 10.0,
                y: 10.0,
                construction: false,
            },
        ],
        vec![],
    );
    sketch
}

fn sketch_status_under_constrained_single_free_point() -> Sketch {
    let sketch = make_sketch(
        vec![SketchEntity::Point {
            id: PointId(1),
            x: 5.0,
            y: 5.0,
            construction: false,
        }],
        vec![],
    );
    sketch
}

fn sketch_status_over_constrained() -> Sketch {
    // Pin a point at origin, then also constrain its distance to itself ≠ 0
    // This creates an impossible constraint
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 10.0,
                y: 0.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            // Force point 2 to be at distance 10 AND also coincident with point 1
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 10.0,
            },
            SketchConstraint::Coincident {
                point_a: PointId(1),
                point_b: PointId(2),
            },
        ],
    );
    sketch
}

fn sketch_status_rectangle_dof_count() -> Sketch {
    // Rectangle without position fix: 4 points (8 DOF) - 4 h/v constraints - 2 dimensions = 2 DOF
    let sketch = make_sketch(
        vec![
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
                y: 40.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 0.0,
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
                value: 80.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(2),
                entity_b: EntityId(3),
                value: 40.0,
            },
        ],
    );
    sketch
}

fn sketch_profile_rectangle_one_outer() -> Sketch {
    let sketch = make_sketch(
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
    );
    sketch
}

fn sketch_profile_circle_one_outer() -> Sketch {
    let sketch = make_sketch(
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
                radius: 25.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Radius {
                entity: EntityId(10),
                value: 25.0,
            },
        ],
    );
    sketch
}

fn sketch_profile_construction_geometry_excluded() -> Sketch {
    // Rectangle where one line is construction — should NOT form a closed profile
    let sketch = make_sketch(
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
                construction: true,
            }, // construction!
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
    );
    sketch
}

fn sketch_profile_construction_circle_excluded() -> Sketch {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: CircleId(10),
                center_id: PointId(1),
                radius: 20.0,
                construction: true,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Radius {
                entity: EntityId(10),
                value: 20.0,
            },
        ],
    );
    sketch
}

fn sketch_profile_rect_with_circle_hole() -> Sketch {
    // Outer rectangle + inner circle = 2 profiles (1 outer + 1 inner-ish)
    // The circle is independent, so it's always classified as outer by extract_profiles.
    // The nesting (outer vs inner/hole) is determined by containment, which for a
    // standalone circle defaults to is_outer=true. In practice the extrude step
    // does the containment test. Here we just verify both profiles are found.
    let sketch = make_sketch(
        vec![
            // Rectangle corners
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
            // Rectangle edges
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
            // Circle hole
            SketchEntity::Point {
                id: PointId(5),
                x: 50.0,
                y: 25.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: CircleId(20),
                center_id: PointId(5),
                radius: 10.0,
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
            SketchConstraint::Dragged { point: PointId(5) },
            SketchConstraint::Radius {
                entity: EntityId(20),
                value: 10.0,
            },
        ],
    );
    sketch
}

fn sketch_reference_rectangle_analytical() -> Sketch {
    // Full analytical test: 4 lines + h/v constraints + 2 distance + dragged origin
    // Expected: exact corner positions at (0,0), (200,0), (200,100), (0,100)
    let w = 200.0;
    let h = 100.0;
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: w,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: w,
                y: h,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 0.0,
                y: h,
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
                value: w,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(2),
                entity_b: EntityId(3),
                value: h,
            },
            SketchConstraint::Dragged { point: PointId(1) },
        ],
    );
    sketch
}

fn sketch_reference_circle_analytical() -> Sketch {
    let cx = 75.0;
    let cy = 30.0;
    let r = 42.0;

    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: cx,
                y: cy,
                construction: false,
            },
            SketchEntity::Circle {
                id: CircleId(10),
                center_id: PointId(1),
                radius: r,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Radius {
                entity: EntityId(10),
                value: r,
            },
        ],
    );
    sketch
}

fn sketch_reference_square_with_equal_sides() -> Sketch {
    // Square: 4 lines, all equal length, one side dimensioned
    let s = 50.0;
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: s,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: s,
                y: s,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 0.0,
                y: s,
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
                value: s,
            },
            SketchConstraint::Equal {
                entity_a: EntityId(10),
                entity_b: EntityId(11),
            },
            SketchConstraint::Dragged { point: PointId(1) },
        ],
    );
    sketch
}

fn sketch_reference_perpendicular_lines() -> Sketch {
    let sketch = make_sketch(
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
                x: 0.0,
                y: 30.0,
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
                start_id: PointId(1),
                end_id: PointId(3),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Perpendicular {
                line_a: EntityId(10),
                line_b: EntityId(11),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 50.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(3),
                value: 30.0,
            },
        ],
    );
    sketch
}

fn sketch_reference_parallel_lines() -> Sketch {
    let sketch = make_sketch(
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
                x: 0.0,
                y: 40.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 80.0,
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
                start_id: PointId(3),
                end_id: PointId(4),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(3) },
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Parallel {
                line_a: EntityId(10),
                line_b: EntityId(11),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 100.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(3),
                entity_b: EntityId(4),
                value: 80.0,
            },
        ],
    );
    sketch
}

fn sketch_reference_midpoint_constraint() -> Sketch {
    let sketch = make_sketch(
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
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 100.0,
            },
            SketchConstraint::Midpoint {
                point: PointId(3),
                line: EntityId(10),
            },
        ],
    );
    sketch
}

fn sketch_reference_symmetric_about_line() -> Sketch {
    // Two points symmetric about a vertical center line.
    // Line 10: vertical center line from (50,0) to (50,100).
    // Points 3 and 4 should mirror across this line.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 50.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 50.0,
                y: 100.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 20.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 80.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(10),
                start_id: PointId(1),
                end_id: PointId(2),
                construction: true,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            SketchConstraint::Dragged { point: PointId(3) },
            SketchConstraint::Symmetric {
                entity_a: PointId(3),
                entity_b: PointId(4),
                symmetry_line: EntityId(10),
            },
        ],
    );
    sketch
}

fn sketch_dragged_moves_under_constrained_point() -> Sketch {
    // Pin p1 at origin, distance of 50 to p2. p2 is under-constrained (1 DOF: rotation).
    // The solver should keep p2 near its initial guess (50, 0).
    let sketch = make_sketch(
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
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 50.0,
            },
        ],
    );
    sketch
}

fn sketch_dragged_respects_existing_constraints() -> Sketch {
    // Rectangle where all corners are defined by constraints.
    // Dragging p1 to origin — it should stay at (0,0) and the
    // rectangle should form around it.
    let sketch = make_sketch(
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
                x: 60.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 0.0,
                y: 30.0,
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
                value: 60.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(2),
                entity_b: EntityId(3),
                value: 30.0,
            },
            SketchConstraint::Dragged { point: PointId(1) },
        ],
    );
    sketch
}

fn sketch_empty_sketch_returns_under_constrained() -> Sketch {
    let sketch = make_sketch(vec![], vec![]);
    sketch
}

fn sketch_single_point_no_constraints() -> Sketch {
    let sketch = make_sketch(
        vec![SketchEntity::Point {
            id: PointId(1),
            x: 42.0,
            y: 17.0,
            construction: false,
        }],
        vec![],
    );
    sketch
}

fn sketch_diameter_constraint_on_circle() -> Sketch {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: CircleId(10),
                center_id: PointId(1),
                radius: 10.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Diameter {
                entity: EntityId(10),
                value: 50.0,
            },
        ],
    );
    sketch
}

fn sketch_on_entity_point_on_line() -> Sketch {
    // Point 3 constrained onto line 10 (horizontal, y=0).
    // Dragged(p1) + Horizontal + Distance fix the line. OnEntity constrains p3.y = 0.
    // p3.x remains free (1 DOF).
    let sketch = make_sketch(
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
                x: 50.0,
                y: 10.0,
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
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 100.0,
            },
            SketchConstraint::OnEntity {
                point: PointId(3),
                entity: EntityId(10),
            },
        ],
    );
    sketch
}

fn sketch_angle_constraint_45_degrees() -> Sketch {
    // Two lines from origin, constrain angle between them to 45 degrees
    let sketch = make_sketch(
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
                x: 70.0,
                y: 70.0,
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
                start_id: PointId(1),
                end_id: PointId(3),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 100.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(3),
                value: 100.0,
            },
            SketchConstraint::Angle {
                line_a: EntityId(10),
                line_b: EntityId(11),
                value_degrees: 45.0,
            },
        ],
    );
    sketch
}

fn sketch_symmetric_horizontal_constraint() -> Sketch {
    // slvs SymmetricHoriz: symmetric about Y-axis (opposite x, same y).
    // Note: slvs naming is counterintuitive — "Horiz" means the OFFSET is horizontal.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 30.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: -30.0,
                y: 20.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::SymmetricH {
                point_a: PointId(1),
                point_b: PointId(2),
            },
        ],
    );
    sketch
}

fn sketch_symmetric_vertical_constraint() -> Sketch {
    // slvs SymmetricVert: symmetric about X-axis (same x, opposite y).
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 20.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 20.0,
                y: -30.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::SymmetricV {
                point_a: PointId(1),
                point_b: PointId(2),
            },
        ],
    );
    sketch
}

fn sketch_distance_point_to_line() -> Sketch {
    // Point 3 at distance 25.0 from line 10 (horizontal at y=0).
    // Don't pin point 3 — let the solver place it via the constraint.
    let sketch = make_sketch(
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
                x: 50.0,
                y: 25.0,
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
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 100.0,
            },
            // Point-Line distance: point_a=3 (Point), entity_b=10 (Line)
            SketchConstraint::Distance {
                entity_a: EntityId(3),
                entity_b: EntityId(10),
                value: 25.0,
            },
        ],
    );
    sketch
}

fn sketch_distance_line_to_point_swap() -> Sketch {
    // Test the Line-Point swap branch (line is entity_a, point is entity_b)
    // Don't pin point 3 — let the distance constraint position it.
    let sketch = make_sketch(
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
                x: 50.0,
                y: 30.0,
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
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 100.0,
            },
            // Line first, then point — triggers the swap branch
            SketchConstraint::Distance {
                entity_a: EntityId(10),
                entity_b: EntityId(3),
                value: 30.0,
            },
        ],
    );
    sketch
}

fn sketch_arc_entity_creation_and_solve() -> Sketch {
    // Create an arc and verify it can be solved.
    // Pin center and start, use radius constraint. End point has 1 DOF (angle).
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            }, // center
            SketchEntity::Point {
                id: PointId(2),
                x: 10.0,
                y: 0.0,
                construction: false,
            }, // start
            SketchEntity::Point {
                id: PointId(3),
                x: 0.0,
                y: 10.0,
                construction: false,
            }, // end
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
            SketchConstraint::Dragged { point: PointId(2) },
            // Don't pin end — arc constrains end to same radius as start
        ],
    );
    sketch
}

fn sketch_tangent_arc_line_constraint() -> Sketch {
    // Arc tangent to a line. Arc centered at (0,50), radius 50, line is horizontal at y=0.
    // At tangent point (0,0), the arc meets the line tangentially.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: -50.0,
                y: 0.0,
                construction: false,
            }, // line start
            SketchEntity::Point {
                id: PointId(2),
                x: 50.0,
                y: 0.0,
                construction: false,
            }, // line end
            SketchEntity::Point {
                id: PointId(3),
                x: 0.0,
                y: 50.0,
                construction: false,
            }, // arc center
            SketchEntity::Point {
                id: PointId(4),
                x: 0.0,
                y: 0.0,
                construction: false,
            }, // arc start
            SketchEntity::Point {
                id: PointId(5),
                x: 50.0,
                y: 50.0,
                construction: false,
            }, // arc end
            SketchEntity::Line {
                id: LineId(10),
                start_id: PointId(1),
                end_id: PointId(2),
                construction: false,
            },
            SketchEntity::Arc {
                id: ArcId(11),
                center_id: PointId(3),
                start_id: PointId(4),
                end_id: PointId(5),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            SketchConstraint::Dragged { point: PointId(3) },
            SketchConstraint::Dragged { point: PointId(5) },
            SketchConstraint::Tangent {
                line: EntityId(10),
                curve: EntityId(11),
            },
        ],
    );
    sketch
}

fn sketch_equal_radius_two_circles() -> Sketch {
    let sketch = make_sketch(
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
            SketchEntity::Circle {
                id: CircleId(10),
                center_id: PointId(1),
                radius: 20.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: CircleId(11),
                center_id: PointId(2),
                radius: 15.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            SketchConstraint::Radius {
                entity: EntityId(10),
                value: 20.0,
            },
            SketchConstraint::Equal {
                entity_a: EntityId(10),
                entity_b: EntityId(11),
            },
        ],
    );
    sketch
}

fn sketch_equal_radius_two_arcs() -> Sketch {
    // Two arcs with EqualRadius. Pin centers and one start each; let endpoints float.
    let sketch = make_sketch(
        vec![
            // Arc 1: center (0,0), start (10,0), end (0,10)
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 10.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 0.0,
                y: 10.0,
                construction: false,
            },
            SketchEntity::Arc {
                id: ArcId(10),
                center_id: PointId(1),
                start_id: PointId(2),
                end_id: PointId(3),
                construction: false,
            },
            // Arc 2: center (50,0), start (65,0), end (50,15)
            SketchEntity::Point {
                id: PointId(4),
                x: 50.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(5),
                x: 65.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(6),
                x: 50.0,
                y: 15.0,
                construction: false,
            },
            SketchEntity::Arc {
                id: ArcId(11),
                center_id: PointId(4),
                start_id: PointId(5),
                end_id: PointId(6),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            SketchConstraint::Dragged { point: PointId(4) },
            // EqualRadius: arc 10 and arc 11 should have the same radius
            SketchConstraint::Equal {
                entity_a: EntityId(10),
                entity_b: EntityId(11),
            },
        ],
    );
    sketch
}

fn sketch_equal_radius_circle_arc() -> Sketch {
    // Circle with radius 25, then an arc that should match via EqualRadius.
    // Don't over-constrain arc points.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: CircleId(10),
                center_id: PointId(1),
                radius: 25.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 80.0,
                y: 0.0,
                construction: false,
            }, // arc center
            SketchEntity::Point {
                id: PointId(3),
                x: 100.0,
                y: 0.0,
                construction: false,
            }, // arc start
            SketchEntity::Point {
                id: PointId(4),
                x: 80.0,
                y: 20.0,
                construction: false,
            }, // arc end
            SketchEntity::Arc {
                id: ArcId(11),
                center_id: PointId(2),
                start_id: PointId(3),
                end_id: PointId(4),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Radius {
                entity: EntityId(10),
                value: 25.0,
            },
            SketchConstraint::Dragged { point: PointId(2) },
            // Circle-Arc equal radius
            SketchConstraint::Equal {
                entity_a: EntityId(10),
                entity_b: EntityId(11),
            },
        ],
    );
    sketch
}

fn sketch_equal_radius_arc_circle() -> Sketch {
    // Arc first, Circle second — tests the (Arc, Circle) branch.
    // Pin arc center and start only. Pin circle center.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 15.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 0.0,
                y: 15.0,
                construction: false,
            },
            SketchEntity::Arc {
                id: ArcId(10),
                center_id: PointId(1),
                start_id: PointId(2),
                end_id: PointId(3),
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 60.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: CircleId(11),
                center_id: PointId(4),
                radius: 30.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            SketchConstraint::Dragged { point: PointId(4) },
            // Arc-Circle equal radius
            SketchConstraint::Equal {
                entity_a: EntityId(10),
                entity_b: EntityId(11),
            },
        ],
    );
    sketch
}

fn sketch_radius_constraint_on_arc() -> Sketch {
    // Pin only the center. The radius constraint sets the arc size.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 30.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 0.0,
                y: 30.0,
                construction: false,
            },
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
            SketchConstraint::Radius {
                entity: EntityId(10),
                value: 30.0,
            },
        ],
    );
    sketch
}

fn sketch_diameter_constraint_on_arc() -> Sketch {
    // Pin only center. Diameter constraint sets arc size.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 20.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 0.0,
                y: 20.0,
                construction: false,
            },
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
            SketchConstraint::Diameter {
                entity: EntityId(10),
                value: 40.0,
            },
        ],
    );
    sketch
}

fn sketch_on_entity_point_on_circle() -> Sketch {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: CircleId(10),
                center_id: PointId(1),
                radius: 25.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 25.0,
                y: 1.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Radius {
                entity: EntityId(10),
                value: 25.0,
            },
            SketchConstraint::OnEntity {
                point: PointId(2),
                entity: EntityId(10),
            },
        ],
    );
    sketch
}

fn sketch_on_entity_point_on_arc() -> Sketch {
    // Don't over-constrain arc. Pin center and start only.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            }, // arc center
            SketchEntity::Point {
                id: PointId(2),
                x: 20.0,
                y: 0.0,
                construction: false,
            }, // arc start
            SketchEntity::Point {
                id: PointId(3),
                x: 0.0,
                y: 20.0,
                construction: false,
            }, // arc end
            SketchEntity::Arc {
                id: ArcId(10),
                center_id: PointId(1),
                start_id: PointId(2),
                end_id: PointId(3),
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 14.0,
                y: 14.0,
                construction: false,
            }, // point on arc
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            SketchConstraint::OnEntity {
                point: PointId(4),
                entity: EntityId(10),
            },
        ],
    );
    sketch
}

fn sketch_equal_angle_four_lines() -> Sketch {
    // Angle between lines 10-11 should equal angle between lines 12-13
    let sketch = make_sketch(
        vec![
            // First pair of lines from origin
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
                x: 40.0,
                y: 30.0,
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
                start_id: PointId(1),
                end_id: PointId(3),
                construction: false,
            },
            // Second pair from (100,0)
            SketchEntity::Point {
                id: PointId(4),
                x: 100.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(5),
                x: 150.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(6),
                x: 130.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(12),
                start_id: PointId(4),
                end_id: PointId(5),
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(13),
                start_id: PointId(4),
                end_id: PointId(6),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            SketchConstraint::Dragged { point: PointId(3) },
            SketchConstraint::Dragged { point: PointId(4) },
            SketchConstraint::Horizontal {
                entity: EntityId(12),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(4),
                entity_b: EntityId(5),
                value: 50.0,
            },
            SketchConstraint::Distance {
                entity_a: EntityId(4),
                entity_b: EntityId(6),
                value: 50.0,
            },
            SketchConstraint::EqualAngle {
                line_a: EntityId(10),
                line_b: EntityId(11),
                line_c: EntityId(12),
                line_d: EntityId(13),
            },
        ],
    );
    sketch
}

fn sketch_length_ratio_constraint() -> Sketch {
    // LengthRatio: line_a / line_b = value.
    // Line 10 = 40, ratio 2.0 → line 11 should be 20.
    // Set initial point 4 near expected position to help convergence.
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 40.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 0.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 20.0,
                y: 20.0,
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
                start_id: PointId(3),
                end_id: PointId(4),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 40.0,
            },
            SketchConstraint::Dragged { point: PointId(3) },
            SketchConstraint::Horizontal {
                entity: EntityId(11),
            },
            SketchConstraint::Ratio {
                entity_a: EntityId(10),
                entity_b: EntityId(11),
                value: 2.0,
            },
        ],
    );
    sketch
}

fn sketch_equal_point_to_line_distance() -> Sketch {
    // Two points equidistant from a line.
    // Don't over-constrain: pin line endpoints and one point, let the other float.
    let sketch = make_sketch(
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
            SketchEntity::Line {
                id: LineId(10),
                start_id: PointId(1),
                end_id: PointId(2),
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 30.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 70.0,
                y: 20.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
            SketchConstraint::Distance {
                entity_a: EntityId(1),
                entity_b: EntityId(2),
                value: 100.0,
            },
            SketchConstraint::EqualPointToLine {
                point_a: PointId(3),
                point_b: PointId(4),
                line: EntityId(10),
            },
        ],
    );
    sketch
}

fn sketch_same_orientation_is_noop() -> Sketch {
    // SameOrientation is a no-op in 2D sketch context.
    // Just verify it doesn't panic.
    let sketch = make_sketch(
        vec![SketchEntity::Point {
            id: PointId(1),
            x: 0.0,
            y: 0.0,
            construction: false,
        }],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::SameOrientation {
                entity_a: EntityId(1),
                entity_b: EntityId(1),
            },
        ],
    );
    sketch
}

fn sketch_profile_with_arc_and_lines() -> Sketch {
    // A D-shape: 3 lines + 1 arc. Don't pin all 5 points (arc over-constrains).
    // Pin the 4 corner points. Arc center floats.
    let sketch = make_sketch(
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
                x: 50.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 0.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(5),
                x: 75.0,
                y: 25.0,
                construction: false,
            }, // arc center
            SketchEntity::Line {
                id: LineId(10),
                start_id: PointId(1),
                end_id: PointId(2),
                construction: false,
            },
            SketchEntity::Arc {
                id: ArcId(11),
                center_id: PointId(5),
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
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            SketchConstraint::Dragged { point: PointId(4) },
            // Arc start(2) and end(3) are shared with lines. Only pin start(2).
            // Don't pin pt3 or pt5 to avoid over-constraining the arc.
        ],
    );
    sketch
}

fn sketch_profile_construction_arc_excluded() -> Sketch {
    // Arc marked as construction should not appear in profiles
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 10.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 0.0,
                y: 10.0,
                construction: false,
            },
            SketchEntity::Arc {
                id: ArcId(10),
                center_id: PointId(1),
                start_id: PointId(2),
                end_id: PointId(3),
                construction: true,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            SketchConstraint::Dragged { point: PointId(3) },
        ],
    );
    sketch
}

fn sketch_multiple_constraints_combined() -> Sketch {
    // Test a sketch that exercises many constraint types together.
    // Rectangle: H/V + distance + equal (square) + midpoint.
    // Note: H+V+Perp+Parallel together can over-constrain, so use a minimal set.
    let sketch = make_sketch(
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
                x: 50.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 0.0,
                y: 50.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(5),
                x: 25.0,
                y: 0.0,
                construction: false,
            }, // midpoint
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
            SketchConstraint::Dragged { point: PointId(1) },
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
                value: 50.0,
            },
            SketchConstraint::Equal {
                entity_a: EntityId(10),
                entity_b: EntityId(11),
            },
            SketchConstraint::Midpoint {
                point: PointId(5),
                line: EntityId(10),
            },
        ],
    );
    sketch
}

fn sketch_profile_single_line_no_profile() -> Sketch {
    // A single line cannot form a closed profile
    let sketch = make_sketch(
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
            SketchEntity::Line {
                id: LineId(10),
                start_id: PointId(1),
                end_id: PointId(2),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
        ],
    );
    sketch
}

fn sketch_profile_triangle() -> Sketch {
    // Three lines forming a triangle
    let sketch = make_sketch(
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
                y: 52.0,
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
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            SketchConstraint::Dragged { point: PointId(3) },
        ],
    );
    sketch
}

fn sketch_profile_only_points_no_profile() -> Sketch {
    // Only points, no lines/arcs → no edges, no profile
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 10.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 5.0,
                y: 10.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Dragged { point: PointId(2) },
            SketchConstraint::Dragged { point: PointId(3) },
        ],
    );
    sketch
}

fn sketch_coincident_merges_points() -> Sketch {
    let sketch = make_sketch(
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 10.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 15.0,
                y: 25.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
            SketchConstraint::Coincident {
                point_a: PointId(1),
                point_b: PointId(2),
            },
        ],
    );
    sketch
}

fn sketch_angle_constraint_90_degrees() -> Sketch {
    let sketch = make_sketch(
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
                start_id: PointId(1),
                end_id: PointId(3),
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: PointId(1) },
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
                entity_b: EntityId(3),
                value: 50.0,
            },
            SketchConstraint::Angle {
                line_a: EntityId(10),
                line_b: EntityId(11),
                value_degrees: 90.0,
            },
        ],
    );
    sketch
}

fn main() {
    let output_dir = Path::new("tests/renders");
    if !output_dir.exists() {
        fs::create_dir_all(output_dir).expect("Failed to create output directory");
    }

    let mut count = 0;
    let mut failures = 0;

    let sketches = vec![
        (
            "01_rectangle_100x50_fully_constrained",
            sketch_rectangle_100x50_fully_constrained(),
        ),
        (
            "02_circle_center_and_radius",
            sketch_circle_center_and_radius(),
        ),
        (
            "03_equilateral_triangle_equal_lengths",
            sketch_equilateral_triangle_equal_lengths(),
        ),
        (
            "04_two_points_with_distance",
            sketch_two_points_with_distance(),
        ),
        (
            "05_status_fully_constrained",
            sketch_status_fully_constrained(),
        ),
        (
            "06_status_under_constrained",
            sketch_status_under_constrained(),
        ),
        (
            "07_status_under_constrained_single_free_point",
            sketch_status_under_constrained_single_free_point(),
        ),
        (
            "08_status_over_constrained",
            sketch_status_over_constrained(),
        ),
        (
            "09_status_rectangle_dof_count",
            sketch_status_rectangle_dof_count(),
        ),
        (
            "10_profile_rectangle_one_outer",
            sketch_profile_rectangle_one_outer(),
        ),
        (
            "11_profile_circle_one_outer",
            sketch_profile_circle_one_outer(),
        ),
        (
            "12_profile_construction_geometry_excluded",
            sketch_profile_construction_geometry_excluded(),
        ),
        (
            "13_profile_construction_circle_excluded",
            sketch_profile_construction_circle_excluded(),
        ),
        (
            "14_profile_rect_with_circle_hole",
            sketch_profile_rect_with_circle_hole(),
        ),
        (
            "15_reference_rectangle_analytical",
            sketch_reference_rectangle_analytical(),
        ),
        (
            "16_reference_circle_analytical",
            sketch_reference_circle_analytical(),
        ),
        (
            "17_reference_square_with_equal_sides",
            sketch_reference_square_with_equal_sides(),
        ),
        (
            "18_reference_perpendicular_lines",
            sketch_reference_perpendicular_lines(),
        ),
        (
            "19_reference_parallel_lines",
            sketch_reference_parallel_lines(),
        ),
        (
            "20_reference_midpoint_constraint",
            sketch_reference_midpoint_constraint(),
        ),
        (
            "21_reference_symmetric_about_line",
            sketch_reference_symmetric_about_line(),
        ),
        (
            "22_dragged_moves_under_constrained_point",
            sketch_dragged_moves_under_constrained_point(),
        ),
        (
            "23_dragged_respects_existing_constraints",
            sketch_dragged_respects_existing_constraints(),
        ),
        (
            "24_empty_sketch_returns_under_constrained",
            sketch_empty_sketch_returns_under_constrained(),
        ),
        (
            "25_single_point_no_constraints",
            sketch_single_point_no_constraints(),
        ),
        (
            "26_diameter_constraint_on_circle",
            sketch_diameter_constraint_on_circle(),
        ),
        (
            "27_on_entity_point_on_line",
            sketch_on_entity_point_on_line(),
        ),
        (
            "28_angle_constraint_45_degrees",
            sketch_angle_constraint_45_degrees(),
        ),
        (
            "29_symmetric_horizontal_constraint",
            sketch_symmetric_horizontal_constraint(),
        ),
        (
            "30_symmetric_vertical_constraint",
            sketch_symmetric_vertical_constraint(),
        ),
        ("31_distance_point_to_line", sketch_distance_point_to_line()),
        (
            "32_distance_line_to_point_swap",
            sketch_distance_line_to_point_swap(),
        ),
        (
            "33_arc_entity_creation_and_solve",
            sketch_arc_entity_creation_and_solve(),
        ),
        (
            "34_tangent_arc_line_constraint",
            sketch_tangent_arc_line_constraint(),
        ),
        (
            "35_equal_radius_two_circles",
            sketch_equal_radius_two_circles(),
        ),
        ("36_equal_radius_two_arcs", sketch_equal_radius_two_arcs()),
        (
            "37_equal_radius_circle_arc",
            sketch_equal_radius_circle_arc(),
        ),
        (
            "38_equal_radius_arc_circle",
            sketch_equal_radius_arc_circle(),
        ),
        (
            "39_radius_constraint_on_arc",
            sketch_radius_constraint_on_arc(),
        ),
        (
            "40_diameter_constraint_on_arc",
            sketch_diameter_constraint_on_arc(),
        ),
        (
            "41_on_entity_point_on_circle",
            sketch_on_entity_point_on_circle(),
        ),
        ("42_on_entity_point_on_arc", sketch_on_entity_point_on_arc()),
        ("43_equal_angle_four_lines", sketch_equal_angle_four_lines()),
        (
            "44_length_ratio_constraint",
            sketch_length_ratio_constraint(),
        ),
        (
            "45_equal_point_to_line_distance",
            sketch_equal_point_to_line_distance(),
        ),
        (
            "46_same_orientation_is_noop",
            sketch_same_orientation_is_noop(),
        ),
        (
            "47_profile_with_arc_and_lines",
            sketch_profile_with_arc_and_lines(),
        ),
        (
            "48_profile_construction_arc_excluded",
            sketch_profile_construction_arc_excluded(),
        ),
        (
            "49_multiple_constraints_combined",
            sketch_multiple_constraints_combined(),
        ),
        (
            "50_profile_single_line_no_profile",
            sketch_profile_single_line_no_profile(),
        ),
        ("51_profile_triangle", sketch_profile_triangle()),
        (
            "52_profile_only_points_no_profile",
            sketch_profile_only_points_no_profile(),
        ),
        (
            "53_coincident_merges_points",
            sketch_coincident_merges_points(),
        ),
        (
            "54_angle_constraint_90_degrees",
            sketch_angle_constraint_90_degrees(),
        ),
    ];

    for (name, sketch) in sketches {
        if let Err(e) = solve_and_render(name, sketch) {
            eprintln!("Failed to generate {}: {}", name, e);
            failures += 1;
        }
        count += 1;
    }

    // Generate 50 randomized sketches
    let mut rng = StdRng::seed_from_u64(42);
    for i in 1..=50 {
        let (name, sketch) = match i % 3 {
            0 => (
                format!("rand_rect_{:02}", i),
                gen_rand_rect(&mut rng, i as u32 * 100),
            ),
            1 => (
                format!("rand_triangle_{:02}", i),
                gen_rand_triangle(&mut rng, i as u32 * 100),
            ),
            _ => (
                format!("rand_circle_{:02}", i),
                gen_rand_circle(&mut rng, i as u32 * 100),
            ),
        };
        if let Err(e) = solve_and_render(&name, sketch) {
            eprintln!("Failed to generate {}: {}", name, e);
            failures += 1;
        }
        count += 1;
    }

    eprintln!("Generated {} files ({} failures)", count, failures);
}

fn solve_and_render(name: &str, sketch: Sketch) -> Result<(), Box<dyn std::error::Error>> {
    let solved = solve_sketch(&sketch);

    let svg_str = render_sketch_svg(&sketch, &solved);
    let png_data = render_sketch_png(&svg_str, 800, 600);

    let svg_path = format!("tests/renders/{}.svg", name);
    let png_path = format!("tests/renders/{}.png", name);

    fs::write(svg_path, svg_str)?;
    fs::write(png_path, png_data)?;

    Ok(())
}
