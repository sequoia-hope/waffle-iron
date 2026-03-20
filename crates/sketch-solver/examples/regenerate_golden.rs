//! Regenerate all golden reference SVGs and PNGs.
//!
//! Usage:
//!   cargo run --example regenerate_golden --features render
//!
//! Writes to tests/golden/reference/{name}.svg and {name}.png.
//! Also writes scenario JSON to tests/golden/scenarios/{name}.json.

use sketch_solver::{
    render_sketch_png, render_sketch_svg, solve_sketch, Sketch, SketchConstraint, SketchEntity,
    SolveStatus,
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
    )
}

fn circle_sketch() -> Sketch {
    make_sketch(
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
                radius: 30.0,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::Radius {
                entity: 10,
                value: 30.0,
            },
        ],
    )
}

fn triangle_sketch() -> Sketch {
    make_sketch(
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
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 60.0,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 60.0,
            },
            SketchConstraint::Distance {
                entity_a: 3,
                entity_b: 1,
                value: 60.0,
            },
            SketchConstraint::Dragged { point: 1 },
        ],
    )
}

fn bracket_sketch() -> Sketch {
    make_sketch(
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
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 30.0,
                y: 20.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 5,
                x: 30.0,
                y: 60.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 6,
                x: 0.0,
                y: 60.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 7,
                x: 30.0,
                y: 30.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 20,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 21,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 22,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 23,
                start_id: 4,
                end_id: 5,
                construction: false,
            },
            SketchEntity::Line {
                id: 24,
                start_id: 5,
                end_id: 6,
                construction: false,
            },
            SketchEntity::Line {
                id: 25,
                start_id: 6,
                end_id: 1,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal { entity: 20 },
            SketchConstraint::Horizontal { entity: 22 },
            SketchConstraint::Horizontal { entity: 24 },
            SketchConstraint::Vertical { entity: 21 },
            SketchConstraint::Vertical { entity: 23 },
            SketchConstraint::Vertical { entity: 25 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 80.0,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 20.0,
            },
            SketchConstraint::Distance {
                entity_a: 5,
                entity_b: 6,
                value: 30.0,
            },
            SketchConstraint::Distance {
                entity_a: 6,
                entity_b: 1,
                value: 60.0,
            },
            SketchConstraint::Dragged { point: 1 },
        ],
    )
}

fn underconstrained_sketch() -> Sketch {
    make_sketch(
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
                x: 25.0,
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
        ],
        vec![SketchConstraint::Horizontal { entity: 10 }],
    )
}

fn overconstrained_sketch() -> Sketch {
    make_sketch(
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
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
        ],
        vec![
            SketchConstraint::Horizontal { entity: 10 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 50.0,
            },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 100.0,
            },
            SketchConstraint::Dragged { point: 1 },
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
        let solved = solve_sketch(&sketch);
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
