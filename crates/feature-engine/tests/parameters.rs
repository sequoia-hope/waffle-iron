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
            scope: None,
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities,
        constraints,
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
        plane_face: None,
    };
    sketch.recompute_derived();
    sketch
}

fn extrude_op(sketch_id: Uuid, depth: f64, depth_expr: Option<&str>) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            profile_entity_ids: None,
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

    engine.set_parameters(vec![DesignParameter::new("height", "25")], &[], &mut kernel);
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
    engine.set_parameters(vec![DesignParameter::new("height", "40")], &[], &mut kernel);
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

    engine.set_parameters(vec![DesignParameter::new("width", "20")], &[], &mut kernel);
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
    engine.set_parameters(vec![DesignParameter::new("width", "35")], &[], &mut kernel);
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
    engine.set_parameters(vec![a, b], &[], &mut kernel);
    let param_err_ids: Vec<Uuid> = engine.errors.iter().map(|(id, _)| *id).collect();
    assert!(param_err_ids.contains(&aid), "{:?}", engine.errors);
    assert!(param_err_ids.contains(&bid), "{:?}", engine.errors);
    assert!(engine
        .errors
        .iter()
        .any(|(_, m)| m.contains("circular reference")));
}

/// A rename carries its dependents, and ONE undo step carries both halves
/// back (`specs/agent_mechanical_design.md` §6 P5).
///
/// This is the half that cannot live in a unit test: the rewrite of a feature
/// field happens OUTSIDE the parameter table, so restoring the table alone
/// would leave the extrude reading a name the document no longer has — and
/// the extrude would then build at its last-good depth, silently, forever.
#[test]
fn renaming_a_parameter_rewrites_its_dependents_and_undo_restores_both() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();

    engine.set_parameters(
        vec![
            DesignParameter::new("h", "25"),
            // A name that merely STARTS with the renamed one, and a
            // dependent expression that reads both.
            DesignParameter::new("h2", "5"),
            DesignParameter::new("total", "h + h2"),
        ],
        &[],
        &mut kernel,
    );
    let sketch_fid = engine
        .add_feature(
            "Sketch1".to_string(),
            Operation::Sketch {
                sketch: rect_sketch_with_width_expr("20"),
            },
            &mut kernel,
        )
        .unwrap();
    let extrude_id = engine
        .add_feature(
            "Extrude1".to_string(),
            extrude_op(sketch_fid, 0.010, Some("h * 2")),
            &mut kernel,
        )
        .unwrap();
    assert!((engine_extrude_depth(&engine, extrude_id) - 0.050).abs() < 1e-15);

    // The rename: the table carries the new name, `renames` carries the pair.
    let renamed: Vec<DesignParameter> = engine
        .tree
        .parameters
        .iter()
        .map(|p| {
            let mut p = p.clone();
            if p.name == "h" {
                p.name = "height".to_string();
            }
            p
        })
        .collect();
    engine.set_parameters(
        renamed,
        &[("h".to_string(), "height".to_string())],
        &mut kernel,
    );

    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert_eq!(engine.tree.parameters[1].name, "h2");
    assert_eq!(
        engine.tree.parameters[2].expression, "height + h2",
        "a dependent parameter follows the rename; `h2` is not a reference to `h`"
    );
    match &engine.tree.find_feature(extrude_id).unwrap().operation {
        Operation::Extrude { params } => {
            assert_eq!(params.depth_expr.as_deref(), Some("height * 2"))
        }
        other => panic!("expected extrude, got {other:?}"),
    }
    // Still 50 mm: a rename changes names, never geometry.
    assert!((engine_extrude_depth(&engine, extrude_id) - 0.050).abs() < 1e-15);

    // One undo step restores the table AND the rewritten field.
    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.parameters[0].name, "h");
    assert_eq!(engine.tree.parameters[2].expression, "h + h2");
    match &engine.tree.find_feature(extrude_id).unwrap().operation {
        Operation::Extrude { params } => assert_eq!(params.depth_expr.as_deref(), Some("h * 2")),
        other => panic!("expected extrude, got {other:?}"),
    }
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!((engine_extrude_depth(&engine, extrude_id) - 0.050).abs() < 1e-15);

    // Redo re-applies both halves.
    engine.redo(&mut kernel).unwrap();
    assert_eq!(engine.tree.parameters[0].name, "height");
    match &engine.tree.find_feature(extrude_id).unwrap().operation {
        Operation::Extrude { params } => {
            assert_eq!(params.depth_expr.as_deref(), Some("height * 2"))
        }
        other => panic!("expected extrude, got {other:?}"),
    }
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!((engine_extrude_depth(&engine, extrude_id) - 0.050).abs() < 1e-15);
}

/// A rename whose new name is already another parameter's must NOT rewrite
/// the dependents, because the rewrite would succeed: every expression would
/// then read the OTHER parameter, resolve cleanly, and move the geometry with
/// no error anywhere near the feature.
///
/// Measured before the guard: the extrude below went from 20 mm to 198 mm and
/// the only complaint was `duplicate parameter name 'width'` on the shadowed
/// row. `parameters_set` refuses the request outright; this path has no answer
/// to refuse into, so it declines the REWRITE and leaves the dependents on a
/// name that no longer resolves — loud instead of wrong.
#[test]
fn a_rename_onto_a_taken_name_does_not_rewrite_the_dependents() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    engine.set_parameters(
        vec![
            DesignParameter::new("width", "99"),
            DesignParameter::new("w", "10"),
            DesignParameter::new("total", "w * 2"),
        ],
        &[],
        &mut kernel,
    );
    let sketch_fid = engine
        .add_feature(
            "Sketch1".to_string(),
            Operation::Sketch {
                sketch: rect_sketch_with_width_expr("20"),
            },
            &mut kernel,
        )
        .unwrap();
    let extrude_id = engine
        .add_feature(
            "Extrude1".to_string(),
            extrude_op(sketch_fid, 0.010, Some("w * 2")),
            &mut kernel,
        )
        .unwrap();
    assert!((engine_extrude_depth(&engine, extrude_id) - 0.020).abs() < 1e-15);

    // The panel's shape: the table carries the new name, `renames` the pair.
    let renamed: Vec<DesignParameter> = engine
        .tree
        .parameters
        .iter()
        .map(|p| {
            let mut p = p.clone();
            if p.name == "w" {
                p.name = "width".to_string();
            }
            p
        })
        .collect();
    engine.set_parameters(
        renamed,
        &[("w".to_string(), "width".to_string())],
        &mut kernel,
    );

    assert_eq!(
        engine.tree.parameters[2].expression, "w * 2",
        "the dependent must NOT be spliced onto the other parameter"
    );
    match &engine.tree.find_feature(extrude_id).unwrap().operation {
        Operation::Extrude { params } => assert_eq!(params.depth_expr.as_deref(), Some("w * 2")),
        other => panic!("expected extrude, got {other:?}"),
    }
    // `w` is gone from the table, so the dependents fail LOUDLY rather than
    // quietly reading 99.
    assert!(
        engine
            .errors
            .iter()
            .any(|(_, m)| m.contains("duplicate parameter name 'width'")),
        "{:?}",
        engine.errors
    );
    assert!(
        engine.errors.iter().any(|(_, m)| m.contains('w')
            && (m.contains("unknown variable") || m.contains("does not resolve"))),
        "a dependent on the vanished name must say so: {:?}",
        engine.errors
    );
    // The last-good depth is kept while the expression is broken — never the
    // other variable's value.
    assert!((engine_extrude_depth(&engine, extrude_id) - 0.020).abs() < 1e-15);
}

/// Undo and redo of a rename are stable across repetition: both records carry
/// full field texts, so replaying either direction twice lands in the same
/// place. A record that stored a DIFF rather than the text would not.
#[test]
fn undo_and_redo_of_a_rename_are_stable_when_repeated() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    engine.set_parameters(
        vec![
            DesignParameter::new("h", "25"),
            DesignParameter::new("total", "h + 5"),
        ],
        &[],
        &mut kernel,
    );
    let sketch_fid = engine
        .add_feature(
            "Sketch1".to_string(),
            Operation::Sketch {
                sketch: rect_sketch_with_width_expr("20"),
            },
            &mut kernel,
        )
        .unwrap();
    let extrude_id = engine
        .add_feature(
            "Extrude1".to_string(),
            extrude_op(sketch_fid, 0.010, Some("h * 2")),
            &mut kernel,
        )
        .unwrap();
    let renamed: Vec<DesignParameter> = engine
        .tree
        .parameters
        .iter()
        .map(|p| {
            let mut p = p.clone();
            if p.name == "h" {
                p.name = "height".to_string();
            }
            p
        })
        .collect();
    engine.set_parameters(
        renamed,
        &[("h".to_string(), "height".to_string())],
        &mut kernel,
    );

    let depth_expr =
        |engine: &Engine| match &engine.tree.find_feature(extrude_id).unwrap().operation {
            Operation::Extrude { params } => params.depth_expr.clone().unwrap(),
            other => panic!("expected extrude, got {other:?}"),
        };
    for pass in 0..2 {
        engine.undo(&mut kernel).unwrap();
        assert_eq!(engine.tree.parameters[0].name, "h", "undo pass {pass}");
        assert_eq!(engine.tree.parameters[1].expression, "h + 5");
        assert_eq!(depth_expr(&engine), "h * 2", "undo pass {pass}");
        assert!(engine.errors.is_empty(), "{:?}", engine.errors);

        engine.redo(&mut kernel).unwrap();
        assert_eq!(engine.tree.parameters[0].name, "height", "redo pass {pass}");
        assert_eq!(engine.tree.parameters[1].expression, "height + 5");
        assert_eq!(depth_expr(&engine), "height * 2", "redo pass {pass}");
        assert!(engine.errors.is_empty(), "{:?}", engine.errors);
        assert!((engine_extrude_depth(&engine, extrude_id) - 0.050).abs() < 1e-15);
    }
}
