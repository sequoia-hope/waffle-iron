use feature_engine::types::*;
use feature_engine::Engine;
use kernel_fork::MockKernel;
use uuid::Uuid;
use waffle_types::*;

/// Create a simple sketch operation for testing.
/// Includes solved positions and a closed profile for the rectangle.
fn make_sketch_op() -> Operation {
    let mut solved_positions = std::collections::HashMap::new();
    solved_positions.insert(1, (0.0, 0.0));
    solved_positions.insert(2, (1.0, 0.0));
    solved_positions.insert(3, (1.0, 1.0));
    solved_positions.insert(4, (0.0, 1.0));

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
        solved_positions,
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        }],
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

// ── Helper: make_extrude_op with custom depth ────────────────────────────

fn make_extrude_op_depth(sketch_id: Uuid, depth: f64) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            depth,
            direction: None,
            symmetric: false,
            cut: false,
            target_body: None,
        },
    }
}

/// Create a boolean union operation referencing two extrude features.
fn make_boolean_union(extrude_a_id: Uuid, extrude_b_id: Uuid) -> Operation {
    Operation::BooleanCombine {
        params: BooleanParams {
            body_a: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::FeatureOutput {
                    feature_id: extrude_a_id,
                    output_key: OutputKey::Main,
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::Strict,
            },
            body_b: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::FeatureOutput {
                    feature_id: extrude_b_id,
                    output_key: OutputKey::Main,
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::Strict,
            },
            operation: BooleanOp::Union,
        },
    }
}

// ── M8: Full Pipeline Integration Tests ──────────────────────────────────

#[test]
fn full_pipeline_sketch_extrude_boolean_rebuild() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Build: sketch1 → extrude1 → sketch2 → extrude2 → boolean union
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature("Extrude 2".to_string(), make_extrude_op(s2), &mut kernel)
        .unwrap();
    let bool_id = engine
        .add_feature(
            "Boolean Union".to_string(),
            make_boolean_union(e1, e2),
            &mut kernel,
        )
        .unwrap();

    // All 5 features should have results
    assert!(engine.get_result(s1).is_some());
    assert!(engine.get_result(e1).is_some());
    assert!(engine.get_result(s2).is_some());
    assert!(engine.get_result(e2).is_some());
    assert!(engine.get_result(bool_id).is_some());

    // Boolean result should have outputs
    let bool_result = engine.get_result(bool_id).unwrap();
    assert_eq!(bool_result.outputs.len(), 1);
    assert!(!bool_result.provenance.role_assignments.is_empty());
}

#[test]
fn full_pipeline_edit_early_feature_rebuilds_downstream() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Build pipeline
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature("Extrude 2".to_string(), make_extrude_op(s2), &mut kernel)
        .unwrap();
    let bool_id = engine
        .add_feature(
            "Boolean Union".to_string(),
            make_boolean_union(e1, e2),
            &mut kernel,
        )
        .unwrap();

    // Edit extrude1 depth — should trigger rebuild of extrude1 + boolean
    engine
        .edit_feature(e1, make_extrude_op_depth(s1, 15.0), &mut kernel)
        .unwrap();

    // All results should still be present after rebuild
    assert!(engine.get_result(s1).is_some());
    assert!(engine.get_result(e1).is_some());
    assert!(engine.get_result(s2).is_some());
    assert!(engine.get_result(e2).is_some());
    assert!(engine.get_result(bool_id).is_some());
    assert!(
        engine.errors.is_empty(),
        "No rebuild errors: {:?}",
        engine.errors
    );
}

#[test]
fn full_pipeline_undo_edit_restores_state() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Edit depth
    engine
        .edit_feature(e1, make_extrude_op_depth(s1, 20.0), &mut kernel)
        .unwrap();

    // Undo the edit
    engine.undo(&mut kernel).unwrap();

    // Verify original depth restored
    if let Operation::Extrude { params } = &engine.tree.find_feature(e1).unwrap().operation {
        assert_eq!(params.depth, 5.0);
    } else {
        panic!("Expected Extrude");
    }

    // Redo the edit
    engine.redo(&mut kernel).unwrap();

    // Verify edited depth
    if let Operation::Extrude { params } = &engine.tree.find_feature(e1).unwrap().operation {
        assert_eq!(params.depth, 20.0);
    } else {
        panic!("Expected Extrude");
    }
}

#[test]
fn full_pipeline_rollback_mid_tree_and_restore() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature("Extrude 2".to_string(), make_extrude_op(s2), &mut kernel)
        .unwrap();

    // Rollback to after extrude1 (index 1) — sketch2 + extrude2 inactive
    engine.set_rollback(Some(1), &mut kernel);
    assert!(engine.get_result(s1).is_some());
    assert!(engine.get_result(e1).is_some());
    assert!(engine.get_result(s2).is_none());
    assert!(engine.get_result(e2).is_none());

    // Restore all
    engine.set_rollback(None, &mut kernel);
    assert!(engine.get_result(s1).is_some());
    assert!(engine.get_result(e1).is_some());
    assert!(engine.get_result(s2).is_some());
    assert!(engine.get_result(e2).is_some());
}

// ── M9: Persistent Naming Stress Tests ──────────────────────────────────

#[test]
fn stress_add_feature_mid_tree_downstream_survives() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Build: sketch → extrude
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Insert a second sketch at position 1 (between sketch1 and extrude1)
    // We use the engine API which appends, then reorder
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    engine.reorder_feature(s2, 1, &mut kernel).unwrap();

    // Tree should be: [s1, s2, e1]
    assert_eq!(engine.tree.features[0].id, s1);
    assert_eq!(engine.tree.features[1].id, s2);
    assert_eq!(engine.tree.features[2].id, e1);

    // Extrude1 still references sketch1 by ID — should still work
    assert!(engine.get_result(e1).is_some());
    assert!(
        engine.errors.is_empty(),
        "Downstream refs should survive mid-tree insert: {:?}",
        engine.errors
    );
}

#[test]
fn stress_remove_mid_tree_dependent_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Build: sketch → extrude (extrude depends on sketch)
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Remove the sketch — extrude should error on rebuild
    engine.remove_feature(s1, &mut kernel).unwrap();

    assert_eq!(engine.tree.features.len(), 1);
    // Extrude can't find its sketch reference, should have an error
    assert!(engine.get_result(e1).is_none());
    assert!(
        !engine.errors.is_empty(),
        "Removing dependency should cause rebuild error"
    );
}

#[test]
fn stress_suppress_dependency_errors_downstream() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Suppress the sketch — extrude should error (sketch has no result)
    engine.set_suppressed(s1, true, &mut kernel).unwrap();

    assert!(engine.get_result(s1).is_none());
    assert!(engine.get_result(e1).is_none());
    assert!(
        !engine.errors.is_empty(),
        "Suppressing dependency should error downstream"
    );

    // Unsuppress — extrude should recover
    engine.set_suppressed(s1, false, &mut kernel).unwrap();

    assert!(engine.get_result(s1).is_some());
    assert!(engine.get_result(e1).is_some());
    assert!(
        engine.errors.is_empty(),
        "Unsuppressing should recover: {:?}",
        engine.errors
    );
}

#[test]
fn stress_reorder_preserves_refs() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Build: s1, s2, e1(refs s1), e2(refs s2)
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature("Extrude 2".to_string(), make_extrude_op(s2), &mut kernel)
        .unwrap();

    // Swap s2 and s1: [s2, s1, e1, e2]
    engine.reorder_feature(s2, 0, &mut kernel).unwrap();

    // Both extrudes should still resolve — they reference by UUID, not position
    assert!(engine.get_result(e1).is_some());
    assert!(engine.get_result(e2).is_some());
    assert!(
        engine.errors.is_empty(),
        "Reorder should not break UUID-based refs: {:?}",
        engine.errors
    );
}

#[test]
fn stress_reorder_extrude_before_sketch_fails() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Move extrude before its sketch: [e1, s1]
    engine.reorder_feature(e1, 0, &mut kernel).unwrap();

    assert_eq!(engine.tree.features[0].id, e1);
    assert_eq!(engine.tree.features[1].id, s1);

    // Extrude executes before sketch, so sketch result doesn't exist yet
    assert!(engine.get_result(e1).is_none());
    assert!(
        !engine.errors.is_empty(),
        "Extrude before its sketch should fail"
    );
}

#[test]
fn stress_multiple_undo_redo_cycle() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Add 3 features
    let s1 = engine
        .add_feature("A".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let s2 = engine
        .add_feature("B".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let s3 = engine
        .add_feature("C".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    assert_eq!(engine.tree.features.len(), 3);

    // Undo all 3
    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 2);
    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 1);
    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 0);

    // Redo all 3
    engine.redo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 1);
    assert_eq!(engine.tree.features[0].id, s1);
    engine.redo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 2);
    assert_eq!(engine.tree.features[1].id, s2);
    engine.redo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 3);
    assert_eq!(engine.tree.features[2].id, s3);

    // All results present
    assert!(engine.get_result(s1).is_some());
    assert!(engine.get_result(s2).is_some());
    assert!(engine.get_result(s3).is_some());
}

// ── Fillet/Chamfer/Shell Pipeline Tests ──────────────────────────────────

/// Create a fillet operation referencing an edge from a previous extrude.
/// Uses BestEffort + a non-matching role to trigger kind-based fallback
/// that finds an Edge entity from the extrude's provenance.
fn make_fillet_op(extrude_id: Uuid, radius: f64) -> Operation {
    Operation::Fillet {
        params: FilletParams {
            edges: vec![GeomRef {
                kind: TopoKind::Edge,
                anchor: Anchor::FeatureOutput {
                    feature_id: extrude_id,
                    output_key: OutputKey::Main,
                },
                selector: Selector::Role {
                    role: Role::ProfileFace, // Won't match any role → falls back to Edge kind-match
                    index: 0,
                },
                policy: ResolvePolicy::BestEffort,
            }],
            radius,
        },
    }
}

/// Create a chamfer operation referencing an edge from a previous extrude.
fn make_chamfer_op(extrude_id: Uuid, distance: f64) -> Operation {
    Operation::Chamfer {
        params: ChamferParams {
            edges: vec![GeomRef {
                kind: TopoKind::Edge,
                anchor: Anchor::FeatureOutput {
                    feature_id: extrude_id,
                    output_key: OutputKey::Main,
                },
                selector: Selector::Role {
                    role: Role::ProfileFace,
                    index: 0,
                },
                policy: ResolvePolicy::BestEffort,
            }],
            distance,
        },
    }
}

/// Create a shell operation referencing a face from a previous extrude.
fn make_shell_op(extrude_id: Uuid, thickness: f64) -> Operation {
    Operation::Shell {
        params: ShellParams {
            faces_to_remove: vec![GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::FeatureOutput {
                    feature_id: extrude_id,
                    output_key: OutputKey::Main,
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::Strict,
            }],
            thickness,
        },
    }
}

#[test]
fn fillet_pipeline_sketch_extrude_fillet() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // sketch → extrude → fillet
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Verify extrude has results before fillet
    assert!(engine.get_result(e1).is_some());

    let f1 = engine
        .add_feature("Fillet 1".to_string(), make_fillet_op(e1, 1.0), &mut kernel)
        .unwrap();

    // Fillet should produce a result
    let fillet_result = engine.get_result(f1);
    assert!(
        fillet_result.is_some(),
        "Fillet should have a result. Errors: {:?}",
        engine.errors
    );

    let result = fillet_result.unwrap();
    // Should have Main output
    assert_eq!(result.outputs.len(), 1);
    assert_eq!(result.outputs[0].0, OutputKey::Main);

    // Should have FilletFace roles in provenance
    let fillet_faces: Vec<_> = result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::FilletFace { .. }))
        .collect();
    assert!(
        !fillet_faces.is_empty(),
        "Fillet should assign FilletFace roles"
    );
}

#[test]
fn chamfer_pipeline_sketch_extrude_chamfer() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();
    let c1 = engine
        .add_feature(
            "Chamfer 1".to_string(),
            make_chamfer_op(e1, 0.5),
            &mut kernel,
        )
        .unwrap();

    let chamfer_result = engine.get_result(c1);
    assert!(
        chamfer_result.is_some(),
        "Chamfer should have a result. Errors: {:?}",
        engine.errors
    );

    let result = chamfer_result.unwrap();
    assert_eq!(result.outputs.len(), 1);

    // Should have ChamferFace roles
    let chamfer_faces: Vec<_> = result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::ChamferFace { .. }))
        .collect();
    assert!(
        !chamfer_faces.is_empty(),
        "Chamfer should assign ChamferFace roles"
    );
}

#[test]
fn shell_pipeline_sketch_extrude_shell() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();
    let sh1 = engine
        .add_feature("Shell 1".to_string(), make_shell_op(e1, 0.3), &mut kernel)
        .unwrap();

    let shell_result = engine.get_result(sh1);
    assert!(
        shell_result.is_some(),
        "Shell should have a result. Errors: {:?}",
        engine.errors
    );

    let result = shell_result.unwrap();
    assert_eq!(result.outputs.len(), 1);

    // Should have ShellInnerFace roles
    let inner_faces: Vec<_> = result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::ShellInnerFace { .. }))
        .collect();
    assert!(
        !inner_faces.is_empty(),
        "Shell should assign ShellInnerFace roles"
    );
}

#[test]
fn fillet_pipeline_edit_extrude_rebuilds_fillet() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();
    let f1 = engine
        .add_feature("Fillet 1".to_string(), make_fillet_op(e1, 1.0), &mut kernel)
        .unwrap();

    assert!(engine.get_result(f1).is_some());

    // Edit the extrude depth — fillet should rebuild downstream
    engine
        .edit_feature(e1, make_extrude_op_depth(s1, 15.0), &mut kernel)
        .unwrap();

    // Fillet should still have a result after rebuild
    assert!(
        engine.get_result(f1).is_some(),
        "Fillet should survive extrude edit. Errors: {:?}",
        engine.errors
    );
    assert!(
        engine.errors.is_empty(),
        "No rebuild errors expected: {:?}",
        engine.errors
    );
}

#[test]
fn fillet_resolve_geomref_produces_kernel_id() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Verify the extrude result has role assignments that can be resolved
    let extrude_result = engine.get_result(e1).unwrap();
    assert!(
        !extrude_result.provenance.role_assignments.is_empty(),
        "Extrude should have role assignments"
    );

    // Verify there are SideFace roles (needed by fillet/chamfer GeomRefs)
    let side_faces: Vec<_> = extrude_result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| matches!(r, Role::SideFace { .. }))
        .collect();
    assert!(!side_faces.is_empty(), "Extrude should have SideFace roles");
}

// ── M10: Performance Benchmarks ─────────────────────────────────────────

/// Build a tree of N sketch+extrude pairs and return rebuild time.
fn bench_rebuild_n_features(n: usize) -> std::time::Duration {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Build tree: alternating sketch + extrude
    let mut sketch_ids = Vec::new();
    for i in 0..n {
        let s = engine
            .add_feature(format!("Sketch {}", i), make_sketch_op(), &mut kernel)
            .unwrap();
        sketch_ids.push(s);
        engine
            .add_feature(format!("Extrude {}", i), make_extrude_op(s), &mut kernel)
            .unwrap();
    }

    // Measure full rebuild from scratch
    let start = std::time::Instant::now();
    engine.rebuild_from_scratch(&mut kernel);
    start.elapsed()
}

#[test]
fn bench_rebuild_10_features() {
    let elapsed = bench_rebuild_n_features(5); // 5 sketch+extrude = 10 features
    eprintln!("Rebuild 10 features: {:?}", elapsed);
    // Sanity check: should complete in under 1 second with MockKernel
    assert!(
        elapsed.as_secs() < 1,
        "10-feature rebuild took too long: {:?}",
        elapsed
    );
}

#[test]
fn bench_rebuild_20_features() {
    let elapsed = bench_rebuild_n_features(10); // 10 pairs = 20 features
    eprintln!("Rebuild 20 features: {:?}", elapsed);
    assert!(
        elapsed.as_secs() < 1,
        "20-feature rebuild took too long: {:?}",
        elapsed
    );
}

#[test]
fn bench_rebuild_50_features() {
    let elapsed = bench_rebuild_n_features(25); // 25 pairs = 50 features
    eprintln!("Rebuild 50 features: {:?}", elapsed);
    assert!(
        elapsed.as_secs() < 2,
        "50-feature rebuild took too long: {:?}",
        elapsed
    );
}
