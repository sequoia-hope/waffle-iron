//! Regenerate all golden reference SVGs and PNGs.
//!
//! Usage:
//!   cargo run --example regenerate_golden --features render
//!
//! Writes to tests/golden/reference/{name}.svg and {name}.png.
//! Also writes scenario JSON to tests/golden/scenarios/{name}.json.

use sketch_solver::{
    render_sketch_png, render_sketch_svg, solve_sketch, CircleId, EntityId, LineId, PointId,
    Sketch, SketchConstraint, SketchEntity, SolveStatus,
};
use std::collections::HashMap;
use uuid::Uuid;

fn dummy_geom_ref() -> sketch_solver::GeomRef {
    sketch_solver::GeomRef {
        kind: sketch_solver::TopoKind::Face,
        anchor: sketch_solver::Anchor::Datum {
            datum_id: Uuid::nil(),
        },
        selector: sketch_solver::Selector::Role {
            role: sketch_solver::Role::ProfileFace,
            index: 0,
        },
        policy: sketch_solver::ResolvePolicy::Strict,
    }
}

fn make_sketch(entities: Vec<SketchEntity>, constraints: Vec<SketchConstraint>) -> Sketch {
    Sketch {
        id: Uuid::nil(),
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

fn all_fixtures() -> Vec<(&'static str, Sketch)> {
    vec![
        ("rectangle", rectangle_sketch()),
        ("circle", circle_sketch()),
        ("triangle", triangle_sketch()),
        ("bracket", bracket_sketch()),
        ("underconstrained", underconstrained_sketch()),
        ("overconstrained", overconstrained_sketch()),
    ]
}

fn rectangle_sketch() -> Sketch {
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

fn circle_sketch() -> Sketch {
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

fn triangle_sketch() -> Sketch {
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

fn bracket_sketch() -> Sketch {
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
            SketchEntity::Point {
                id: PointId(7),
                x: 30.0,
                y: 30.0,
                construction: false,
            },
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

fn underconstrained_sketch() -> Sketch {
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

fn overconstrained_sketch() -> Sketch {
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

fn main() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let scenarios_dir = manifest.join("tests").join("golden").join("scenarios");
    let reference_dir = manifest.join("tests").join("golden").join("reference");

    std::fs::create_dir_all(&scenarios_dir).unwrap();
    std::fs::create_dir_all(&reference_dir).unwrap();

    for (name, sketch) in all_fixtures() {
        // Write scenario JSON
        let json = serde_json::to_string_pretty(&sketch).unwrap();
        let json_path = scenarios_dir.join(format!("{name}.json"));
        std::fs::write(&json_path, &json).unwrap();
        eprintln!("Wrote {}", json_path.display());

        // Solve
        let solved = solve_sketch(&sketch).expect("workbench solve");
        eprintln!(
            "  {name}: status={:?}, points={}, profiles={}",
            solved.status,
            solved.positions.len(),
            solved.profiles.len()
        );

        // Render SVG
        let svg = render_sketch_svg(&sketch, &solved);
        let svg_path = reference_dir.join(format!("{name}.svg"));
        std::fs::write(&svg_path, &svg).unwrap();
        eprintln!("  Wrote {}", svg_path.display());

        // Render PNG
        let png = render_sketch_png(&svg, 800, 600);
        let png_path = reference_dir.join(format!("{name}.png"));
        std::fs::write(&png_path, &png).unwrap();
        eprintln!("  Wrote {} ({} bytes)", png_path.display(), png.len());
    }

    eprintln!("\nDone! All golden references regenerated.");
}
