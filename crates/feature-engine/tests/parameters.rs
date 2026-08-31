//! Engine-level tests for parameterized designs (design variables).
//!
//! Covers the public flow the wasm bridge drives: `Engine::set_parameters`
//! replaces the table, the rebuild's apply pass re-evaluates every
//! expression-driven measurement (extrude depth, sketch dimensions), and the
//! change is undoable. MockKernel provides deterministic geometry.

use feature_engine::types::*;
use feature_engine::Engine;
use std::collections::HashMap;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

/// A closed 20mm x 10mm rectangle sketch with real Line entities and a
/// width-driving Distance dimension, so the parameter pass can re-solve it.
fn rect_sketch_with_width_expr(width_expr: &str) -> Sketch {
    let entities = vec![
        SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 2,
            x: 0.02,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 3,
            x: 0.02,
            y: 0.01,
            construction: false,
        },
        SketchEntity::Point {
            id: 4,
            x: 0.0,
            y: 0.01,
            construction: false,
        },
        SketchEntity::Line {
            id: 5,
            start_id: 1,
            end_id: 2,
            construction: false,
        },
        SketchEntity::Line {
            id: 6,
            start_id: 2,
            end_id: 3,
            construction: false,
        },
        SketchEntity::Line {
            id: 7,
            start_id: 3,
            end_id: 4,
            construction: false,
        },
        SketchEntity::Line {
            id: 8,
            start_id: 4,
            end_id: 1,
            construction: false,
        },
    ];
    let constraints = vec![
        SketchConstraint::Pinned {
            point: 1,
            x: 0.0,
            y: 0.0,
        },
        SketchConstraint::Horizontal { entity: 5 },
        SketchConstraint::Horizontal { entity: 7 },
        SketchConstraint::Vertical { entity: 6 },
        SketchConstraint::Vertical { entity: 8 },
        SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 0.02,
            expression: Some(width_expr.to_string()),
            reference: false,
        },
        SketchConstraint::Distance {
            entity_a: 2,
            entity_b: 3,
            value: 0.01,
            expression: None,
            reference: false,
        },
    ];
    let mut sketch = Sketch {
        id: Uuid::new_v4(),
        plane: GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::Datum {
                datum_id: Uuid::new_v4(),
            },
            selector: Selector::Role {
                role: Role::EndCapPositive,
                index: 0,
            },
            policy: ResolvePolicy::Strict,
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        entities,
        constraints,
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
    };
    sketch.recompute_derived();
    sketch
}

fn extrude_op(sketch_id: Uuid, depth: f64, depth_expr: Option<&str>) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            depth,
            depth_expr: depth_expr.map(str::to_string),
            direction: None,
            symmetric: false,
            cut: false,
            merge: false,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            combine: Some(CombineMode::NewBody),
            targets: None,
        },
    }
}

fn engine_extrude_depth(engine: &Engine, extrude_id: Uuid) -> f64 {
    match &engine.tree.find_feature(extrude_id).unwrap().operation {
        Operation::Extrude { params } => params.depth,
        other => panic!("expected extrude, got {other:?}"),
    }
}

#[test]
fn set_parameters_drives_extrude_depth_with_undo_redo() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();

    engine.set_parameters(vec![DesignParameter::new("height", "25")], &mut kernel);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert_eq!(engine.tree.parameters[0].value, 25.0);

    let sketch = rect_sketch_with_width_expr("20");
    // Extrude references the sketch FEATURE's id (find_sketch_in_tree).
    let sketch_fid = engine
        .add_feature(
            "Sketch1".to_string(),
            Operation::Sketch { sketch },
            &mut kernel,
        )
        .unwrap();
    let extrude_id = engine
        .add_feature(
            "Extrude1".to_string(),
            extrude_op(sketch_fid, 0.010, Some("height")),
            &mut kernel,
        )
        .unwrap();

    // The add's rebuild evaluated the expression: 25 mm -> 0.025 m.
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!((engine_extrude_depth(&engine, extrude_id) - 0.025).abs() < 1e-15);
    assert!(
        engine.get_result(extrude_id).is_some(),
        "extrude must produce a result"
    );

    // Changing the variable re-drives the depth.
    engine.set_parameters(vec![DesignParameter::new("height", "40")], &mut kernel);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!((engine_extrude_depth(&engine, extrude_id) - 0.040).abs() < 1e-15);

    // Undo restores the old table AND the old evaluated depth.
    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.parameters[0].expression, "25");
    assert!((engine_extrude_depth(&engine, extrude_id) - 0.025).abs() < 1e-15);

    // Redo re-applies.
    engine.redo(&mut kernel).unwrap();
    assert_eq!(engine.tree.parameters[0].expression, "40");
    assert!((engine_extrude_depth(&engine, extrude_id) - 0.040).abs() < 1e-15);
}

#[test]
fn sketch_dimension_expression_flows_through_engine_rebuild() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();

    engine.set_parameters(vec![DesignParameter::new("width", "20")], &mut kernel);
    let sketch = rect_sketch_with_width_expr("width");
    let sketch_fid = engine
        .add_feature(
            "Sketch1".to_string(),
            Operation::Sketch { sketch },
            &mut kernel,
        )
        .unwrap();
    let extrude_id = engine
        .add_feature(
            "Extrude1".to_string(),
            extrude_op(sketch_fid, 0.010, None),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!(engine.get_result(extrude_id).is_some());

    // Drive the rectangle wider; the stored sketch re-solves.
    engine.set_parameters(vec![DesignParameter::new("width", "35")], &mut kernel);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let sketch = match &engine.tree.find_feature(sketch_fid).unwrap().operation {
        Operation::Sketch { sketch } => sketch,
        _ => unreachable!(),
    };
    let p2 = sketch.solved_positions.get(&2).copied().unwrap();
    assert!(
        (p2.0 - 0.035).abs() < 1e-9,
        "rectangle width must follow the variable: p2.x = {}",
        p2.0
    );
    assert!(
        !sketch.solved_profiles.is_empty(),
        "profiles must be recomputed after the re-solve"
    );
    assert!(
        engine.get_result(extrude_id).is_some(),
        "downstream extrude must rebuild"
    );
}

#[test]
fn parameter_errors_surface_loudly_and_geometry_survives() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();

    let sketch = rect_sketch_with_width_expr("20");
    let sketch_fid = engine
        .add_feature(
            "Sketch1".to_string(),
            Operation::Sketch { sketch },
            &mut kernel,
        )
        .unwrap();
    let extrude_id = engine
        .add_feature(
            "Extrude1".to_string(),
            extrude_op(sketch_fid, 0.010, Some("height")),
            &mut kernel,
        )
        .unwrap();
    // 'height' is undefined: loud error on the extrude, depth unchanged.
    assert_eq!(engine.errors.len(), 1, "{:?}", engine.errors);
    assert_eq!(engine.errors[0].0, extrude_id);
    assert!(engine.errors[0].1.contains("unknown variable 'height'"));
    assert_eq!(engine_extrude_depth(&engine, extrude_id), 0.010);
    assert!(
        engine.get_result(extrude_id).is_some(),
        "extrude still builds with its last-good depth"
    );

    // A cyclic table errors per-parameter (routed by parameter id).
    let a = DesignParameter::new("a", "b + 1");
    let b = DesignParameter::new("b", "a + 1");
    let (aid, bid) = (a.id, b.id);
    engine.set_parameters(vec![a, b], &mut kernel);
    let param_err_ids: Vec<Uuid> = engine.errors.iter().map(|(id, _)| *id).collect();
    assert!(param_err_ids.contains(&aid), "{:?}", engine.errors);
    assert!(param_err_ids.contains(&bid), "{:?}", engine.errors);
    assert!(engine
        .errors
        .iter()
        .any(|(_, m)| m.contains("circular reference")));
}
