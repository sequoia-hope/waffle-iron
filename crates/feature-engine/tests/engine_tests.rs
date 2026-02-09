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

// ── M5: Fallback Resolution Tests ────────────────────────────────────────

#[test]
fn resolve_with_fallback_role_succeeds() {
    use feature_engine::resolve::resolve_with_fallback;

    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();

    let sketch_id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e_id = engine
        .add_feature(
            "Extrude 1".to_string(),
            make_extrude_op(sketch_id),
            &mut kernel,
        )
        .unwrap();

    // EndCapPositive exists on extrude — should succeed without fallback
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

    let resolved = resolve_with_fallback(&geom_ref, &engine.feature_results);
    assert!(resolved.is_ok());
    assert!(resolved.unwrap().warnings.is_empty());
}

#[test]
fn resolve_with_fallback_best_effort_fallback() {
    use feature_engine::resolve::resolve_with_fallback;

    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();

    let sketch_id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e_id = engine
        .add_feature(
            "Extrude 1".to_string(),
            make_extrude_op(sketch_id),
            &mut kernel,
        )
        .unwrap();

    // RevStartFace doesn't exist on extrude, but BestEffort should fall back
    // to matching by TopoKind (Face) among created entities
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
        policy: ResolvePolicy::BestEffort,
    };

    let resolved = resolve_with_fallback(&geom_ref, &engine.feature_results);
    assert!(
        resolved.is_ok(),
        "BestEffort should fall back: {:?}",
        resolved
    );
    assert!(
        !resolved.unwrap().warnings.is_empty(),
        "Fallback should produce a warning"
    );
}

#[test]
fn resolve_with_fallback_strict_no_fallback() {
    use feature_engine::resolve::resolve_with_fallback;

    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();

    let sketch_id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e_id = engine
        .add_feature(
            "Extrude 1".to_string(),
            make_extrude_op(sketch_id),
            &mut kernel,
        )
        .unwrap();

    // RevStartFace doesn't exist, Strict should NOT fall back
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

    let resolved = resolve_with_fallback(&geom_ref, &engine.feature_results);
    assert!(resolved.is_err(), "Strict should not fall back");
}

#[test]
fn rebuild_after_edit_updates_results() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let sketch_id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e_id = engine
        .add_feature(
            "Extrude 1".to_string(),
            make_extrude_op(sketch_id),
            &mut kernel,
        )
        .unwrap();

    // Result after initial build
    assert!(engine.get_result(e_id).is_some());

    // Edit to different depth
    let new_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            depth: 20.0,
            direction: None,
            symmetric: false,
            cut: false,
            target_body: None,
        },
    };
    engine.edit_feature(e_id, new_op, &mut kernel).unwrap();

    // Result should still exist after edit + rebuild
    let result = engine.get_result(e_id);
    assert!(result.is_some());
    assert_eq!(result.unwrap().outputs.len(), 1);
}

#[test]
fn rebuild_error_on_missing_sketch() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let fake_sketch_id = Uuid::new_v4();
    // Add extrude referencing a nonexistent sketch
    let e_id = engine
        .add_feature(
            "Extrude 1".to_string(),
            make_extrude_op(fake_sketch_id),
            &mut kernel,
        )
        .unwrap();

    // The extrude should fail during rebuild, producing an error
    assert!(engine.get_result(e_id).is_none());
    assert!(!engine.errors.is_empty());
}

// ── M6: Undo/Redo Tests ─────────────────────────────────────────────────

#[test]
fn undo_add_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    assert_eq!(engine.tree.features.len(), 1);

    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 0);
    assert!(engine.get_result(id).is_none());
}

#[test]
fn redo_add_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 0);

    engine.redo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 1);
    assert_eq!(engine.tree.features[0].id, id);
    assert!(engine.get_result(id).is_some());
}

#[test]
fn undo_remove_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.remove_feature(id, &mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 0);

    // Undo the remove — feature should be restored
    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 1);
    assert_eq!(engine.tree.features[0].id, id);
    assert_eq!(engine.tree.features[0].name, "Sketch 1");
}

#[test]
fn undo_edit_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let sketch_id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e_id = engine
        .add_feature(
            "Extrude 1".to_string(),
            make_extrude_op(sketch_id),
            &mut kernel,
        )
        .unwrap();

    // Edit depth from 5.0 to 20.0
    let new_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            depth: 20.0,
            direction: None,
            symmetric: false,
            cut: false,
            target_body: None,
        },
    };
    engine.edit_feature(e_id, new_op, &mut kernel).unwrap();

    // Verify new depth
    if let Operation::Extrude { params } = &engine.tree.find_feature(e_id).unwrap().operation {
        assert_eq!(params.depth, 20.0);
    } else {
        panic!("Expected Extrude operation");
    }

    // Undo the edit — should restore old depth
    engine.undo(&mut kernel).unwrap();
    if let Operation::Extrude { params } = &engine.tree.find_feature(e_id).unwrap().operation {
        assert_eq!(params.depth, 5.0);
    } else {
        panic!("Expected Extrude operation");
    }
}

#[test]
fn undo_suppress_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    assert!(!engine.tree.features[0].suppressed);

    engine.set_suppressed(id, true, &mut kernel).unwrap();
    assert!(engine.tree.features[0].suppressed);

    engine.undo(&mut kernel).unwrap();
    assert!(!engine.tree.features[0].suppressed);
}

#[test]
fn undo_reorder_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id_a = engine
        .add_feature("A".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let id_b = engine
        .add_feature("B".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let id_c = engine
        .add_feature("C".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    // Reorder C to position 0: [C, A, B]
    engine.reorder_feature(id_c, 0, &mut kernel).unwrap();
    assert_eq!(engine.tree.features[0].id, id_c);
    assert_eq!(engine.tree.features[1].id, id_a);
    assert_eq!(engine.tree.features[2].id, id_b);

    // Undo: should restore [A, B, C]
    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features[0].id, id_a);
    assert_eq!(engine.tree.features[1].id, id_b);
    assert_eq!(engine.tree.features[2].id, id_c);
}

#[test]
fn redo_clears_on_new_command() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    engine
        .add_feature("A".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.undo(&mut kernel).unwrap();
    assert!(engine.can_redo());

    // Adding a new feature should clear redo stack
    engine
        .add_feature("B".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    assert!(!engine.can_redo());

    let result = engine.redo(&mut kernel);
    assert!(matches!(result, Err(EngineError::NothingToRedo)));
}

#[test]
fn undo_empty_returns_error() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let result = engine.undo(&mut kernel);
    assert!(matches!(result, Err(EngineError::NothingToUndo)));
}

// ── M7: Rollback Integration Tests ──────────────────────────────────────

#[test]
fn rollback_excludes_features_from_results() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id_a = engine
        .add_feature("A".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let id_b = engine
        .add_feature("B".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let id_c = engine
        .add_feature("C".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    // Rollback to index 1 — only A and B are active
    engine.set_rollback(Some(1), &mut kernel);
    assert!(engine.get_result(id_a).is_some());
    assert!(engine.get_result(id_b).is_some());
    assert!(engine.get_result(id_c).is_none());
}

#[test]
fn rollback_none_restores_all() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id_a = engine
        .add_feature("A".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let id_b = engine
        .add_feature("B".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let id_c = engine
        .add_feature("C".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.set_rollback(Some(0), &mut kernel);
    assert!(engine.get_result(id_c).is_none());

    // Restore all
    engine.set_rollback(None, &mut kernel);
    assert!(engine.get_result(id_a).is_some());
    assert!(engine.get_result(id_b).is_some());
    assert!(engine.get_result(id_c).is_some());
}

#[test]
fn rollback_is_not_undoable() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id = engine
        .add_feature("A".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    // Rollback is not recorded in undo stack
    engine.set_rollback(Some(0), &mut kernel);

    // Undo should undo the add_feature, not the rollback
    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 0);
    assert!(engine.get_result(id).is_none());
}
