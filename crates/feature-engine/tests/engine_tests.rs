use feature_engine::types::*;
use feature_engine::Engine;
use kernel_fork::MockKernel;
use uuid::Uuid;
use waffle_types::*;

/// Create a simple sketch operation for testing.
fn make_sketch_op() -> Operation {
    let sketch = Sketch {
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
        entities: vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 1.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 1.0,
                y: 1.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 1.0,
                construction: false,
            },
        ],
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
    };
    Operation::Sketch { sketch }
}

/// Create an extrude operation referencing a sketch.
fn make_extrude_op(sketch_id: Uuid) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            depth: 5.0,
            direction: None,
            symmetric: false,
            cut: false,
            target_body: None,
        },
    }
}

// ── Feature Tree Tests ─────────────────────────────────────────────────────

#[test]
fn tree_add_feature() {
    let mut tree = FeatureTree::new();
    let id = tree.add_feature("Sketch 1".to_string(), make_sketch_op());

    assert_eq!(tree.features.len(), 1);
    assert_eq!(tree.features[0].id, id);
    assert_eq!(tree.features[0].name, "Sketch 1");
    assert!(!tree.features[0].suppressed);
}

#[test]
fn tree_add_multiple_features() {
    let mut tree = FeatureTree::new();
    let id1 = tree.add_feature("Sketch 1".to_string(), make_sketch_op());
    let id2 = tree.add_feature("Extrude 1".to_string(), make_extrude_op(id1));

    assert_eq!(tree.features.len(), 2);
    assert_eq!(tree.features[0].id, id1);
    assert_eq!(tree.features[1].id, id2);
}

#[test]
fn tree_remove_feature() {
    let mut tree = FeatureTree::new();
    let id1 = tree.add_feature("Sketch 1".to_string(), make_sketch_op());
    let _id2 = tree.add_feature("Extrude 1".to_string(), make_extrude_op(id1));

    let removed = tree.remove_feature(id1).unwrap();
    assert_eq!(removed.name, "Sketch 1");
    assert_eq!(tree.features.len(), 1);
}

#[test]
fn tree_remove_nonexistent_returns_error() {
    let mut tree = FeatureTree::new();
    let result = tree.remove_feature(Uuid::new_v4());
    assert!(matches!(result, Err(EngineError::FeatureNotFound { .. })));
}

#[test]
fn tree_reorder_feature() {
    let mut tree = FeatureTree::new();
    let id1 = tree.add_feature("A".to_string(), make_sketch_op());
    let id2 = tree.add_feature("B".to_string(), make_sketch_op());
    let id3 = tree.add_feature("C".to_string(), make_sketch_op());

    // Move C to position 0
    tree.reorder_feature(id3, 0).unwrap();
    assert_eq!(tree.features[0].id, id3);
    assert_eq!(tree.features[1].id, id1);
    assert_eq!(tree.features[2].id, id2);
}

#[test]
fn tree_suppress_feature() {
    let mut tree = FeatureTree::new();
    let id = tree.add_feature("Sketch 1".to_string(), make_sketch_op());

    tree.set_suppressed(id, true).unwrap();
    assert!(tree.features[0].suppressed);

    tree.set_suppressed(id, false).unwrap();
    assert!(!tree.features[0].suppressed);
}

#[test]
fn tree_rollback_limits_active_features() {
    let mut tree = FeatureTree::new();
    tree.add_feature("A".to_string(), make_sketch_op());
    tree.add_feature("B".to_string(), make_sketch_op());
    tree.add_feature("C".to_string(), make_sketch_op());

    assert_eq!(tree.active_features().len(), 3);

    tree.set_rollback(Some(1));
    assert_eq!(tree.active_features().len(), 2);

    tree.set_rollback(Some(0));
    assert_eq!(tree.active_features().len(), 1);

    tree.set_rollback(None);
    assert_eq!(tree.active_features().len(), 3);
}

// ── Engine Integration Tests ───────────────────────────────────────────────

#[test]
fn engine_add_sketch_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let result = engine.add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel);
    assert!(result.is_ok());

    let id = result.unwrap();
    assert_eq!(engine.tree.features.len(), 1);
    // Sketch produces an empty OpResult
    let op_result = engine.get_result(id);
    assert!(op_result.is_some());
}

#[test]
fn engine_add_sketch_and_extrude() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let sketch_op = make_sketch_op();

    // The feature UUID (returned by add_feature) is what ExtrudeParams.sketch_id references
    let feature_id = engine
        .add_feature("Sketch 1".to_string(), sketch_op, &mut kernel)
        .unwrap();

    let e_id = engine
        .add_feature(
            "Extrude 1".to_string(),
            make_extrude_op(feature_id),
            &mut kernel,
        )
        .unwrap();

    assert_eq!(engine.tree.features.len(), 2);

    // Extrude should have produced an OpResult with outputs
    let extrude_result = engine.get_result(e_id);
    assert!(extrude_result.is_some());
    let result = extrude_result.unwrap();
    assert_eq!(result.outputs.len(), 1);
    assert!(!result.provenance.role_assignments.is_empty());
}

#[test]
fn engine_edit_feature_triggers_rebuild() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let sketch_op = make_sketch_op();

    let feature_id = engine
        .add_feature("Sketch 1".to_string(), sketch_op, &mut kernel)
        .unwrap();

    let e_id = engine
        .add_feature(
            "Extrude 1".to_string(),
            make_extrude_op(feature_id),
            &mut kernel,
        )
        .unwrap();

    // Edit the extrude to change depth
    let new_params = ExtrudeParams {
        sketch_id: feature_id,
        profile_index: 0,
        depth: 10.0,
        direction: Some([0.0, 0.0, 1.0]),
        symmetric: false,
        cut: false,
        target_body: None,
    };
    let result = engine.edit_feature(e_id, Operation::Extrude { params: new_params }, &mut kernel);
    assert!(result.is_ok());

    // Result should still exist after rebuild
    assert!(engine.get_result(e_id).is_some());
}

#[test]
fn engine_suppress_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.set_suppressed(id, true, &mut kernel).unwrap();
    assert!(engine.tree.features[0].suppressed);
}

#[test]
fn engine_remove_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.remove_feature(id, &mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 0);
    assert!(engine.get_result(id).is_none());
}

#[test]
fn engine_rollback() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    engine
        .add_feature("A".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    engine
        .add_feature("B".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    engine
        .add_feature("C".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.set_rollback(Some(1), &mut kernel);
    assert_eq!(engine.tree.active_features().len(), 2);
}

// ── GeomRef Resolution Tests ──────────────────────────────────────────────

#[test]
fn resolve_by_role_finds_entity() {
    use feature_engine::resolve::resolve_geom_ref;

    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();

    let sketch_op = make_sketch_op();

    let feature_id = engine
        .add_feature("Sketch 1".to_string(), sketch_op, &mut kernel)
        .unwrap();

    let e_id = engine
        .add_feature(
            "Extrude 1".to_string(),
            make_extrude_op(feature_id),
            &mut kernel,
        )
        .unwrap();

    // Create a GeomRef that points to the EndCapPositive of the extrude
    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: e_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    };

    let resolved = resolve_geom_ref(&geom_ref, &engine.feature_results);
    assert!(
        resolved.is_ok(),
        "Should resolve EndCapPositive: {:?}",
        resolved
    );
}

#[test]
fn resolve_nonexistent_role_fails() {
    use feature_engine::resolve::resolve_geom_ref;

    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();

    let sketch_op = make_sketch_op();

    let feature_id = engine
        .add_feature("Sketch 1".to_string(), sketch_op, &mut kernel)
        .unwrap();
    let e_id = engine
        .add_feature(
            "Extrude 1".to_string(),
            make_extrude_op(feature_id),
            &mut kernel,
        )
        .unwrap();

    // Try to resolve a role that doesn't exist on an extrude
    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: e_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::RevStartFace,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    };

    let resolved = resolve_geom_ref(&geom_ref, &engine.feature_results);
    assert!(resolved.is_err(), "Should fail for nonexistent role");
}
