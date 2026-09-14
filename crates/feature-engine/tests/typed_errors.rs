//! Typed engine errors (`specs/waffle_mcp_server.md` ICR-2, A6.2).
//!
//! Every per-feature error the engine reports as `(feature_id, message)` also
//! exists as a `FeatureError` whose `kind` a host can branch on — in the same
//! order, with the same message — so nothing downstream ever has to parse
//! `Display` text to learn WHAT failed.

use feature_engine::types::*;
use feature_engine::Engine;
use modeling_ops::OpError;
use uuid::Uuid;
use waffle_types::kernel::{KernelError, MockKernel};
use waffle_types::*;

fn point(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}

fn line(id: u32, a: u32, b: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id: a,
        end_id: b,
        construction: false,
    }
}

/// One unit square on a datum plane, bounded by lines 10–13.
fn square_sketch() -> Sketch {
    let mut solved_positions = std::collections::HashMap::new();
    for (id, x, y) in [(1, 0.0, 0.0), (2, 1.0, 0.0), (3, 1.0, 1.0), (4, 0.0, 1.0)] {
        solved_positions.insert(id, (x, y));
    }
    Sketch {
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
        entities: vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            point(3, 1.0, 1.0),
            point(4, 0.0, 1.0),
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 3, 4),
            line(13, 4, 1),
        ],
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions,
        projected: vec![],
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
    }
}

fn extrude(sketch_id: Uuid, ids: Option<Vec<u32>>, depth_expr: Option<&str>) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            profile_entity_ids: ids,
            depth: 0.5,
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

fn missing_source_import() -> (Operation, Uuid) {
    let source_id = Uuid::new_v4();
    (
        Operation::ImportedBody {
            params: ImportedBodyParams::from_source("gone.step", source_id),
        },
        source_id,
    )
}

fn kind_for(engine: &Engine, id: Uuid) -> &ErrorKind {
    &engine
        .feature_errors
        .iter()
        .find(|e| e.feature_id == id)
        .unwrap_or_else(|| panic!("no typed error for {id}: {:?}", engine.feature_errors))
        .kind
}

/// The string list and the typed list describe the same errors, in order.
fn assert_lockstep(engine: &Engine) {
    let typed: Vec<(Uuid, String)> = engine
        .feature_errors
        .iter()
        .map(|e| (e.feature_id, e.message.clone()))
        .collect();
    assert_eq!(typed, engine.errors);
}

#[test]
fn engine_errors_map_to_kinds_through_every_wrapper() {
    let id = Uuid::new_v4();
    assert_eq!(
        ErrorKind::from(&EngineError::FeatureNotFound { id }),
        ErrorKind::FeatureNotFound { id }
    );
    assert_eq!(
        ErrorKind::from(&EngineError::ProfileAmbiguous {
            entity_ids: vec![1, 2],
            matches: 2
        }),
        ErrorKind::ProfileAmbiguous {
            entity_ids: vec![1, 2],
            matches: 2
        }
    );
    assert_eq!(
        ErrorKind::from(&EngineError::NothingToUndo),
        ErrorKind::NothingToUndo
    );

    // A capability boundary is `NotSupported` whether the kernel error reached
    // the engine directly or through a modeling op.
    let not_supported = || KernelError::NotSupported {
        operation: "coplanar boolean".to_string(),
    };
    let expected = ErrorKind::NotSupported {
        operation: "coplanar boolean".to_string(),
    };
    assert_eq!(
        ErrorKind::from(&EngineError::KernelError(not_supported())),
        expected
    );
    assert_eq!(
        ErrorKind::from(&EngineError::OpError(OpError::Kernel(not_supported()))),
        expected
    );

    assert_eq!(
        ErrorKind::from(&EngineError::OpError(OpError::Kernel(
            KernelError::BooleanEmptyResult
        ))),
        ErrorKind::BooleanEmptyResult
    );
    assert_eq!(
        ErrorKind::from(&EngineError::KernelError(KernelError::BooleanFailed {
            reason: "stage 4 stop".to_string()
        })),
        ErrorKind::KernelFailure {
            kernel: "BooleanFailed".to_string()
        }
    );
}

#[test]
fn a_profile_miss_is_typed_with_its_fields() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let sketch = engine
        .add_feature(
            "Sketch".into(),
            Operation::Sketch {
                sketch: square_sketch(),
            },
            &mut kernel,
        )
        .unwrap();
    let id = engine
        .add_feature(
            "Extrude".into(),
            extrude(sketch, Some(vec![10, 11, 12, 99]), None),
            &mut kernel,
        )
        .unwrap();

    assert_eq!(
        kind_for(&engine, id),
        &ErrorKind::ProfileNotFound {
            entity_ids: vec![10, 11, 12, 99],
            count: 1
        }
    );
    assert_lockstep(&engine);
}

#[test]
fn a_missing_source_is_typed_and_keeps_its_message() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let (op, source_id) = missing_source_import();
    let id = engine
        .add_feature("Import".into(), op, &mut kernel)
        .unwrap();

    assert_eq!(
        kind_for(&engine, id),
        &ErrorKind::SourceUnavailable {
            source_id: Some(source_id)
        }
    );
    let (_, message) = engine.errors.iter().find(|(f, _)| *f == id).unwrap();
    assert!(
        message.contains("SourceUnavailable: source"),
        "message text is unchanged for existing readers: {message}"
    );
    assert_lockstep(&engine);
}

#[test]
fn expression_errors_are_typed_for_features_and_parameters() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let sketch = engine
        .add_feature(
            "Sketch".into(),
            Operation::Sketch {
                sketch: square_sketch(),
            },
            &mut kernel,
        )
        .unwrap();
    let id = engine
        .add_feature(
            "Extrude".into(),
            extrude(sketch, None, Some("nope * 2")),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(kind_for(&engine, id), &ErrorKind::Expression);

    let param_id = Uuid::new_v4();
    engine.set_parameters(
        vec![DesignParameter {
            id: param_id,
            name: "bad".to_string(),
            expression: "1 +".to_string(),
            value: 0.0,
            error: None,
        }],
        &mut kernel,
    );
    assert_eq!(kind_for(&engine, param_id), &ErrorKind::Expression);
    assert_lockstep(&engine);
}

#[test]
fn several_errors_stay_in_lockstep_and_clear_together() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let sketch = engine
        .add_feature(
            "Sketch".into(),
            Operation::Sketch {
                sketch: square_sketch(),
            },
            &mut kernel,
        )
        .unwrap();
    engine
        .add_feature(
            "Extrude".into(),
            extrude(sketch, None, Some("nope")),
            &mut kernel,
        )
        .unwrap();
    let (op, _) = missing_source_import();
    let import = engine
        .add_feature("Import".into(), op, &mut kernel)
        .unwrap();
    assert!(
        engine.feature_errors.len() >= 2,
        "{:?}",
        engine.feature_errors
    );
    assert_lockstep(&engine);

    engine.remove_feature(import, &mut kernel).unwrap();
    assert_lockstep(&engine);
}
