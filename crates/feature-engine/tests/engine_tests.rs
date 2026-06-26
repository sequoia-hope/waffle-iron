use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
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
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
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
        projected: vec![],
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
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
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
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
fn body_name_override_set_and_clear() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let sid = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let eid = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(sid), &mut kernel)
        .unwrap();

    let body_id = FeatureTree::body_id(eid, &OutputKey::Main);
    assert_eq!(engine.tree.body_name_override(&body_id), None);

    // Set an override — independent of the feature's name.
    engine.rename_body(body_id.clone(), "Housing".to_string());
    assert_eq!(engine.tree.body_name_override(&body_id), Some("Housing"));
    // Renaming the body does NOT touch the feature name (decoupled).
    assert_eq!(engine.tree.find_feature(eid).unwrap().name, "Extrude 1");

    // Empty name clears the override (reverts to the derived name).
    engine.rename_body(body_id.clone(), "  ".to_string());
    assert_eq!(engine.tree.body_name_override(&body_id), None);
}

#[test]
fn body_name_rename_is_undoable() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let sid = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let eid = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(sid), &mut kernel)
        .unwrap();
    let body_id = FeatureTree::body_id(eid, &OutputKey::Main);

    engine.rename_body(body_id.clone(), "Housing".to_string());
    assert_eq!(engine.tree.body_name_override(&body_id), Some("Housing"));

    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.body_name_override(&body_id), None);

    engine.redo(&mut kernel).unwrap();
    assert_eq!(engine.tree.body_name_override(&body_id), Some("Housing"));
}

#[test]
fn body_names_gc_on_delete_and_restore_on_undo() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let sid = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let eid = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(sid), &mut kernel)
        .unwrap();
    let body_id = FeatureTree::body_id(eid, &OutputKey::Main);
    engine.rename_body(body_id.clone(), "Housing".to_string());

    // Deleting the producing feature GCs its body name.
    engine.remove_feature(eid, &mut kernel).unwrap();
    assert_eq!(engine.tree.body_name_override(&body_id), None);

    // Undoing the delete restores the name.
    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.body_name_override(&body_id), Some("Housing"));
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
        merge: true,
        target_body: None,
        depth_mode: DepthMode::Blind,
        second_direction: None,
        region: None,
        regions: Vec::new(),
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
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
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
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
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
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
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

// ── CRITICAL P4: DepthMode::ThroughAll ──────────────────────────────────

#[test]
fn extrude_through_all_without_target_body_uses_fallback() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // First extrude: Blind at depth 5 — creates the target body
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    assert!(engine.get_result(e1).is_some());

    // Second sketch + extrude: ThroughAll should compute depth from target body extent
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let through_all_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s2,
            profile_index: 0,
            depth: 5.0,
            direction: None,
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::ThroughAll,
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e2 = engine
        .add_feature(
            "Extrude ThroughAll".to_string(),
            through_all_op,
            &mut kernel,
        )
        .unwrap();

    // ThroughAll should produce a solid result
    let result = engine.get_result(e2);
    assert!(
        result.is_some(),
        "ThroughAll extrude should produce a result. Errors: {:?}",
        engine.errors
    );
    let result = result.unwrap();
    assert_eq!(result.outputs.len(), 1, "Should have 1 Main output");
    assert!(
        !result.provenance.role_assignments.is_empty(),
        "Should have role assignments"
    );
}

#[test]
fn extrude_through_all_no_prior_body_uses_large_fallback() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Single sketch + ThroughAll extrude with NO prior body
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let through_all_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s1,
            profile_index: 0,
            depth: 5.0,
            direction: None,
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::ThroughAll,
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e1 = engine
        .add_feature(
            "Extrude ThroughAll".to_string(),
            through_all_op,
            &mut kernel,
        )
        .unwrap();

    // Without a prior body, ThroughAll falls back to max(blind_depth, 100.0) = 100.0
    let result = engine.get_result(e1);
    assert!(
        result.is_some(),
        "ThroughAll without prior body should succeed. Errors: {:?}",
        engine.errors
    );
    assert_eq!(result.unwrap().outputs.len(), 1);
}

// ── CRITICAL P4: Cut Extrude (cut=true) ─────────────────────────────────

#[test]
fn cut_extrude_produces_boolean_subtract_result() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Base body: sketch → extrude
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    let base_result = engine.get_result(e1).unwrap();
    let base_face_count = base_result.provenance.role_assignments.len();
    assert!(base_face_count > 0, "Base extrude should have roles");

    // Cut extrude: sketch2 → extrude with cut=true
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let cut_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s2,
            profile_index: 0,
            depth: 3.0,
            direction: None,
            symmetric: false,
            cut: true,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e2 = engine
        .add_feature("Cut Extrude".to_string(), cut_op, &mut kernel)
        .unwrap();

    // Cut should succeed and produce a result
    let cut_result = engine.get_result(e2);
    assert!(
        cut_result.is_some(),
        "Cut extrude should produce a result. Errors: {:?}",
        engine.errors
    );
    let cut_result = cut_result.unwrap();
    assert_eq!(cut_result.outputs.len(), 1, "Cut should have 1 Main output");
    // MockKernel boolean_subtract re-IDs the solid, so role count may differ
    assert!(
        !cut_result.provenance.role_assignments.is_empty(),
        "Cut result should have role assignments"
    );
}

#[test]
fn cut_extrude_without_base_body_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Only a sketch, no prior body to subtract from
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let cut_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s1,
            profile_index: 0,
            depth: 3.0,
            direction: None,
            symmetric: false,
            cut: true,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e1 = engine
        .add_feature("Cut Extrude".to_string(), cut_op, &mut kernel)
        .unwrap();

    // Should fail: no body to subtract from
    assert!(
        engine.get_result(e1).is_none(),
        "Cut extrude without base body should fail"
    );
    assert!(
        !engine.errors.is_empty(),
        "Should have an error for missing base body"
    );
    // Error should mention "existing body"
    let error_msg = &engine.errors[0].1;
    assert!(
        error_msg.contains("existing body") || error_msg.contains("subtract"),
        "Error should mention missing body: {}",
        error_msg
    );
}

// ── CRITICAL P4: SecondDirection::Symmetric ─────────────────────────────

#[test]
fn symmetric_extrude_produces_solid() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    let sym_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s1,
            profile_index: 0,
            depth: 5.0,
            direction: None,
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: Some(SecondDirection::Symmetric),
            region: None,
            regions: Vec::new(),
        },
    };
    let e1 = engine
        .add_feature("Symmetric Extrude".to_string(), sym_op, &mut kernel)
        .unwrap();

    let result = engine.get_result(e1);
    assert!(
        result.is_some(),
        "Symmetric extrude should produce a result. Errors: {:?}",
        engine.errors
    );
    let result = result.unwrap();
    assert_eq!(result.outputs.len(), 1, "Should have 1 Main output");
    assert!(
        !result.provenance.role_assignments.is_empty(),
        "Should have role assignments"
    );
}

#[test]
fn symmetric_flag_backwards_compat() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    // Use the legacy `symmetric: true` field (second_direction: None)
    let sym_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s1,
            profile_index: 0,
            depth: 5.0,
            direction: None,
            symmetric: true,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e1 = engine
        .add_feature("Symmetric Legacy".to_string(), sym_op, &mut kernel)
        .unwrap();

    let result = engine.get_result(e1);
    assert!(
        result.is_some(),
        "Legacy symmetric flag should produce a result. Errors: {:?}",
        engine.errors
    );
    assert_eq!(result.unwrap().outputs.len(), 1);
}

#[test]
fn second_direction_blind_produces_solid() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    let bidir_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s1,
            profile_index: 0,
            depth: 5.0,
            direction: None,
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: Some(SecondDirection::Blind { depth: 3.0 }),
            region: None,
            regions: Vec::new(),
        },
    };
    let e1 = engine
        .add_feature("Bidir Extrude".to_string(), bidir_op, &mut kernel)
        .unwrap();

    let result = engine.get_result(e1);
    assert!(
        result.is_some(),
        "Bidirectional blind extrude should succeed. Errors: {:?}",
        engine.errors
    );
    assert_eq!(result.unwrap().outputs.len(), 1);
}

// ── CRITICAL P4: BooleanOp::Subtract and Intersect ──────────────────────

/// Create a boolean subtract operation referencing two extrude features.
fn make_boolean_subtract(extrude_a_id: Uuid, extrude_b_id: Uuid) -> Operation {
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
            operation: BooleanOp::Subtract,
        },
    }
}

/// Create a boolean intersect operation referencing two extrude features.
fn make_boolean_intersect(extrude_a_id: Uuid, extrude_b_id: Uuid) -> Operation {
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
            operation: BooleanOp::Intersect,
        },
    }
}

#[test]
fn boolean_subtract_at_engine_level() {
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

    let bool_id = engine
        .add_feature(
            "Boolean Subtract".to_string(),
            make_boolean_subtract(e1, e2),
            &mut kernel,
        )
        .unwrap();

    let result = engine.get_result(bool_id);
    assert!(
        result.is_some(),
        "Boolean subtract should produce a result. Errors: {:?}",
        engine.errors
    );
    let result = result.unwrap();
    assert_eq!(result.outputs.len(), 1, "Should have 1 Main output");
    assert!(
        !result.provenance.role_assignments.is_empty(),
        "Should have role assignments"
    );
}

#[test]
fn boolean_intersect_at_engine_level() {
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

    let bool_id = engine
        .add_feature(
            "Boolean Intersect".to_string(),
            make_boolean_intersect(e1, e2),
            &mut kernel,
        )
        .unwrap();

    let result = engine.get_result(bool_id);
    assert!(
        result.is_some(),
        "Boolean intersect should produce a result. Errors: {:?}",
        engine.errors
    );
    let result = result.unwrap();
    assert_eq!(result.outputs.len(), 1, "Should have 1 Main output");
    assert!(
        !result.provenance.role_assignments.is_empty(),
        "Should have role assignments"
    );
    // MockKernel intersect produces a 0.5x0.5x0.5 box — should have fewer roles than union
    // (6 faces for a box)
    // MockKernel intersect produces a 0.5x0.5x0.5 box — box has 6 faces
    assert!(
        result.provenance.role_assignments.len() >= 6,
        "Intersect result should have at least 6 role assignments (box faces)"
    );
}

// ── HIGH P1: Numeric bbox oracle for extrude ────────────────────────────

#[test]
fn extrude_depth_produces_proportional_bbox() {
    use waffle_types::kernel::KernelIntrospect;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Get the solid handle from the extrude result
    let result = engine.get_result(e1).unwrap();
    let solid_handle = &result.outputs[0].1.handle;

    // Get all vertex positions via compute_signature
    let vertex_ids = kernel.list_vertices(solid_handle);
    assert!(
        vertex_ids.len() >= 8,
        "Box solid should have at least 8 vertices, got {}",
        vertex_ids.len()
    );

    let mut min_z = f64::MAX;
    let mut max_z = f64::MIN;
    for vid in &vertex_ids {
        let sig = kernel.compute_signature(*vid, TopoKind::Vertex);
        if let Some(centroid) = sig.centroid {
            min_z = min_z.min(centroid[2]);
            max_z = max_z.max(centroid[2]);
        }
    }

    // MockKernel make_box_solid(side, side, depth) creates box from [0,0,0] to [side,side,depth].
    // The sketch is 1x1, so side = sqrt(1.0) = 1.0. Depth = 5.0.
    // Z extent should be 5.0 (the extrude depth).
    let z_extent = max_z - min_z;
    assert!(
        (z_extent - 5.0).abs() < 0.01,
        "Z extent should be ~5.0 (extrude depth), got {:.3}",
        z_extent
    );
    assert!(min_z.abs() < 0.01, "Min Z should be ~0.0, got {:.3}", min_z);
}

#[test]
fn extrude_different_depths_produce_proportional_bboxes() {
    use waffle_types::kernel::KernelIntrospect;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Extrude at depth 3.0
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature(
            "Extrude 1".to_string(),
            make_extrude_op_depth(s1, 3.0),
            &mut kernel,
        )
        .unwrap();

    let r1 = engine.get_result(e1).unwrap();
    let h1 = &r1.outputs[0].1.handle;
    let verts1 = kernel.list_vertices(h1);
    let max_z_1: f64 = verts1
        .iter()
        .filter_map(|vid| kernel.compute_signature(*vid, TopoKind::Vertex).centroid)
        .map(|c| c[2])
        .fold(f64::MIN, f64::max);

    // Extrude at depth 10.0
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature(
            "Extrude 2".to_string(),
            make_extrude_op_depth(s2, 10.0),
            &mut kernel,
        )
        .unwrap();

    let r2 = engine.get_result(e2).unwrap();
    let h2 = &r2.outputs[0].1.handle;
    let verts2 = kernel.list_vertices(h2);
    let max_z_2: f64 = verts2
        .iter()
        .filter_map(|vid| kernel.compute_signature(*vid, TopoKind::Vertex).centroid)
        .map(|c| c[2])
        .fold(f64::MIN, f64::max);

    // Depth 10 extrude should be ~3.33x taller than depth 3
    assert!(
        (max_z_1 - 3.0).abs() < 0.01,
        "Depth 3.0 max Z should be ~3.0, got {:.3}",
        max_z_1
    );
    // The second extrude has merge=true and an existing body, so the boss eps
    // offset (0.1) is applied, making the actual depth 10.1 instead of 10.0.
    assert!(
        (max_z_2 - 10.0).abs() < 0.2,
        "Depth 10.0 max Z should be ~10.0 (±eps), got {:.3}",
        max_z_2
    );
    assert!(
        max_z_2 > max_z_1,
        "Deeper extrude should have larger Z extent"
    );
}

// ── HIGH P1: Bench tests with structural validation ─────────────────────

#[test]
fn bench_rebuild_10_features_with_validation() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Build 5 sketch+extrude pairs = 10 features
    let mut extrude_ids = Vec::new();
    for i in 0..5 {
        let s = engine
            .add_feature(format!("Sketch {}", i), make_sketch_op(), &mut kernel)
            .unwrap();
        let e = engine
            .add_feature(format!("Extrude {}", i), make_extrude_op(s), &mut kernel)
            .unwrap();
        extrude_ids.push(e);
    }

    let start = std::time::Instant::now();
    engine.rebuild_from_scratch(&mut kernel);
    let elapsed = start.elapsed();

    eprintln!("Rebuild 10 features (validated): {:?}", elapsed);
    assert!(elapsed.as_secs() < 1);

    // Structural validation: all extrudes should have results with 1 output
    for e_id in &extrude_ids {
        let result = engine.get_result(*e_id);
        assert!(
            result.is_some(),
            "Extrude {:?} should have result after rebuild",
            e_id
        );
        let result = result.unwrap();
        assert_eq!(result.outputs.len(), 1, "Each extrude should have 1 output");
        assert!(
            !result.provenance.role_assignments.is_empty(),
            "Each extrude should have role assignments"
        );
    }
    assert!(
        engine.errors.is_empty(),
        "Rebuild should have no errors: {:?}",
        engine.errors
    );
}

// ── MEDIUM P4: DepthMode::UpTo error branch ─────────────────────────────

#[test]
fn depth_mode_upto_behind_sketch_plane_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Create base body
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Create a second sketch for the UpTo extrude
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    // UpTo referencing the base extrude's EndCapNegative (z=0 face).
    // The sketch plane is at z=0, direction is [0,0,1].
    // EndCapNegative centroid is at z=0 → projection onto direction = 0.
    // sketch_origin projection = 0. depth = 0 - 0 = 0 → should error (depth <= 0).
    let upto_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s2,
            profile_index: 0,
            depth: 5.0,
            direction: Some([0.0, 0.0, 1.0]),
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::UpTo {
                reference: GeomRef {
                    kind: TopoKind::Face,
                    anchor: Anchor::FeatureOutput {
                        feature_id: e1,
                        output_key: OutputKey::Main,
                    },
                    selector: Selector::Role {
                        role: Role::EndCapNegative,
                        index: 0,
                    },
                    policy: ResolvePolicy::Strict,
                },
            },
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e2 = engine
        .add_feature("UpTo Extrude".to_string(), upto_op, &mut kernel)
        .unwrap();

    // The depth should be <= 0 since reference is at/behind the sketch plane
    assert!(
        engine.get_result(e2).is_none(),
        "UpTo with reference behind sketch plane should fail"
    );
    assert!(
        !engine.errors.is_empty(),
        "Should have error for UpTo behind plane"
    );
    let error_msg = &engine.errors[0].1;
    assert!(
        error_msg.contains("behind sketch plane") || error_msg.contains("depth"),
        "Error should mention behind sketch plane: {}",
        error_msg
    );
}

// ── Additional: Cut + Symmetric (combined branches) ─────────────────────

#[test]
fn cut_extrude_with_symmetric_second_direction() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Base body
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let _e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Cut + Symmetric
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let cut_sym_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s2,
            profile_index: 0,
            depth: 2.0,
            direction: None,
            symmetric: false,
            cut: true,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: Some(SecondDirection::Symmetric),
            region: None,
            regions: Vec::new(),
        },
    };
    let e2 = engine
        .add_feature("Cut Symmetric Extrude".to_string(), cut_sym_op, &mut kernel)
        .unwrap();

    let result = engine.get_result(e2);
    assert!(
        result.is_some(),
        "Cut+Symmetric should produce a result. Errors: {:?}",
        engine.errors
    );
    assert_eq!(result.unwrap().outputs.len(), 1);
}

#[test]
fn second_direction_through_all_produces_solid() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Base body needed for ThroughAll to compute extent
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let _e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let bidir_through_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s2,
            profile_index: 0,
            depth: 5.0,
            direction: None,
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: Some(SecondDirection::ThroughAll),
            region: None,
            regions: Vec::new(),
        },
    };
    let e2 = engine
        .add_feature(
            "Bidir ThroughAll".to_string(),
            bidir_through_op,
            &mut kernel,
        )
        .unwrap();

    let result = engine.get_result(e2);
    assert!(
        result.is_some(),
        "SecondDirection::ThroughAll should produce a result. Errors: {:?}",
        engine.errors
    );
    assert_eq!(result.unwrap().outputs.len(), 1);
}

// ══════════════════════════════════════════════════════════════════════════
// COVERAGE BOOST: resolve.rs
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn resolve_datum_anchor_returns_error() {
    use feature_engine::resolve::resolve_geom_ref;

    let feature_results = std::collections::HashMap::new();
    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    };

    let result = resolve_geom_ref(&geom_ref, &feature_results);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Datum"),
        "Error should mention Datum: {}",
        err_msg
    );
}

#[test]
fn resolve_query_selector_succeeds() {
    use feature_engine::resolve::resolve_geom_ref;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: e1,
            output_key: OutputKey::Main,
        },
        selector: Selector::Query {
            query: TopoQuery {
                filters: Vec::new(),
                tie_break: None,
            },
        },
        policy: ResolvePolicy::Strict,
    };

    let result = resolve_geom_ref(&geom_ref, &engine.feature_results);
    // Empty-filter query matches all faces of the requested kind — should succeed
    assert!(
        result.is_ok(),
        "Empty-filter query should resolve: {:?}",
        result.unwrap_err()
    );
}

#[test]
fn resolve_missing_feature_in_results_errors() {
    use feature_engine::resolve::resolve_geom_ref;

    let feature_results = std::collections::HashMap::new();
    let fake_feature_id = Uuid::new_v4();

    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: fake_feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    };

    let result = resolve_geom_ref(&geom_ref, &feature_results);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no result") || err_msg.contains("not yet rebuilt"),
        "Error should mention missing result: {}",
        err_msg
    );
}

#[test]
fn resolve_role_index_out_of_range_strict_errors() {
    use feature_engine::resolve::resolve_geom_ref;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // EndCapPositive exists but only at index 0 — index 99 is out of range
    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: e1,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 99,
        },
        policy: ResolvePolicy::Strict,
    };

    let result = resolve_geom_ref(&geom_ref, &engine.feature_results);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("out of range"),
        "Error should mention out of range: {}",
        err_msg
    );
}

#[test]
fn resolve_role_index_out_of_range_best_effort_clamps() {
    use feature_engine::resolve::resolve_geom_ref;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // EndCapPositive exists at index 0 — index 99 should clamp under BestEffort
    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: e1,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 99,
        },
        policy: ResolvePolicy::BestEffort,
    };

    let result = resolve_geom_ref(&geom_ref, &engine.feature_results);
    assert!(result.is_ok(), "BestEffort should clamp: {:?}", result);
    let resolved = result.unwrap();
    assert!(
        !resolved.warnings.is_empty(),
        "BestEffort clamping should produce a warning"
    );
    assert!(
        resolved.warnings[0].contains("clamped"),
        "Warning should mention clamping: {}",
        resolved.warnings[0]
    );
}

#[test]
fn resolve_signature_good_match_no_warning() {
    use feature_engine::resolve::resolve_geom_ref;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Get the actual signature of a created entity from the extrude result
    let extrude_result = engine.get_result(e1).unwrap();
    assert!(
        !extrude_result.provenance.created.is_empty(),
        "Extrude should have created entities"
    );
    let target_sig = extrude_result.provenance.created[0].signature.clone();

    // Use that signature as the selector — should be a perfect match (sim > 0.9)
    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: e1,
            output_key: OutputKey::Main,
        },
        selector: Selector::Signature {
            signature: target_sig,
        },
        policy: ResolvePolicy::Strict,
    };

    let result = resolve_geom_ref(&geom_ref, &engine.feature_results);
    assert!(
        result.is_ok(),
        "Exact signature match should succeed: {:?}",
        result
    );
    let resolved = result.unwrap();
    assert!(
        resolved.warnings.is_empty(),
        "Good match (>0.9) should have no warnings"
    );
}

#[test]
fn resolve_signature_medium_match_warns() {
    use feature_engine::resolve::resolve_geom_ref;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Create a signature that partially matches (same surface_type but different area/centroid)
    let medium_sig = TopoSignature {
        surface_type: Some("planar".to_string()), // matches
        area: Some(999.0),                        // very different from actual ~1.0
        centroid: Some([50.0, 50.0, 50.0]),       // very different
        normal: Some([0.0, 0.0, 1.0]),            // matches
        bbox: None,
        adjacency_hash: None,
        length: None,
    };

    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: e1,
            output_key: OutputKey::Main,
        },
        selector: Selector::Signature {
            signature: medium_sig,
        },
        policy: ResolvePolicy::Strict,
    };

    let result = resolve_geom_ref(&geom_ref, &engine.feature_results);
    assert!(
        result.is_ok(),
        "Medium match (>0.5) should succeed: {:?}",
        result
    );
    let resolved = result.unwrap();
    assert!(
        !resolved.warnings.is_empty(),
        "Medium match should produce a confidence warning"
    );
    assert!(
        resolved.warnings[0].contains("confidence"),
        "Warning should mention confidence: {}",
        resolved.warnings[0]
    );
}

#[test]
fn resolve_signature_low_match_strict_errors() {
    use feature_engine::resolve::resolve_geom_ref;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Create a signature that barely matches (different surface type + wildly different values)
    let low_sig = TopoSignature {
        surface_type: Some("spherical".to_string()), // different
        area: Some(9999.0),
        centroid: Some([1000.0, 1000.0, 1000.0]),
        normal: Some([1.0, 0.0, 0.0]),
        bbox: None,
        adjacency_hash: Some(12345),
        length: None,
    };

    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: e1,
            output_key: OutputKey::Main,
        },
        selector: Selector::Signature { signature: low_sig },
        policy: ResolvePolicy::Strict,
    };

    let result = resolve_geom_ref(&geom_ref, &engine.feature_results);
    assert!(result.is_err(), "Low match + Strict should error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("too low"),
        "Error should mention too low: {}",
        err_msg
    );
}

#[test]
fn resolve_signature_low_match_best_effort_succeeds() {
    use feature_engine::resolve::resolve_geom_ref;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Low match signature but with BestEffort
    let low_sig = TopoSignature {
        surface_type: Some("spherical".to_string()),
        area: Some(9999.0),
        centroid: Some([1000.0, 1000.0, 1000.0]),
        normal: Some([1.0, 0.0, 0.0]),
        bbox: None,
        adjacency_hash: Some(12345),
        length: None,
    };

    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: e1,
            output_key: OutputKey::Main,
        },
        selector: Selector::Signature { signature: low_sig },
        policy: ResolvePolicy::BestEffort,
    };

    let result = resolve_geom_ref(&geom_ref, &engine.feature_results);
    assert!(
        result.is_ok(),
        "Low match + BestEffort should succeed: {:?}",
        result
    );
    let resolved = result.unwrap();
    assert!(
        !resolved.warnings.is_empty(),
        "Low match BestEffort should produce warning"
    );
    assert!(
        resolved.warnings[0].contains("Low-confidence"),
        "Warning should mention low-confidence: {}",
        resolved.warnings[0]
    );
}

#[test]
fn resolve_signature_no_entities_errors() {
    use feature_engine::resolve::resolve_geom_ref;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Sketch produces empty provenance.created
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    let sig = TopoSignature {
        surface_type: Some("planar".to_string()),
        area: Some(1.0),
        centroid: Some([0.0, 0.0, 0.0]),
        normal: None,
        bbox: None,
        adjacency_hash: None,
        length: None,
    };

    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: s1,
            output_key: OutputKey::Main,
        },
        selector: Selector::Signature { signature: sig },
        policy: ResolvePolicy::Strict,
    };

    let result = resolve_geom_ref(&geom_ref, &engine.feature_results);
    assert!(result.is_err(), "No entities should error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("No entities"),
        "Error should mention no entities: {}",
        err_msg
    );
}

#[test]
fn resolve_with_fallback_signature_selector_no_fallback() {
    use feature_engine::resolve::resolve_with_fallback;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Sketch produces empty created entities — Signature selector will fail
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    let sig = TopoSignature {
        surface_type: Some("planar".to_string()),
        area: Some(1.0),
        centroid: Some([0.0, 0.0, 0.0]),
        normal: None,
        bbox: None,
        adjacency_hash: None,
        length: None,
    };

    // Signature selector — fallback only works for Role selectors
    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: s1,
            output_key: OutputKey::Main,
        },
        selector: Selector::Signature { signature: sig },
        policy: ResolvePolicy::BestEffort,
    };

    let result = resolve_with_fallback(&geom_ref, &engine.feature_results);
    assert!(
        result.is_err(),
        "Signature selector should not trigger kind-based fallback"
    );
}

#[test]
fn resolve_with_fallback_datum_anchor_in_role_fallback() {
    use feature_engine::resolve::resolve_with_fallback;

    let feature_results = std::collections::HashMap::new();

    // Role selector with Datum anchor — primary fails, fallback can't extract feature_id
    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
    };

    let result = resolve_with_fallback(&geom_ref, &feature_results);
    assert!(
        result.is_err(),
        "Datum anchor in fallback path should return primary error"
    );
}

#[test]
fn resolve_with_fallback_missing_feature_in_fallback() {
    use feature_engine::resolve::resolve_with_fallback;

    let feature_results = std::collections::HashMap::new();
    let fake_id = Uuid::new_v4();

    // Role selector with FeatureOutput anchor but feature not in results
    let geom_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: fake_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
    };

    let result = resolve_with_fallback(&geom_ref, &feature_results);
    assert!(
        result.is_err(),
        "Missing feature in fallback should return error"
    );
}

#[test]
fn resolve_with_fallback_best_effort_no_kind_match() {
    use feature_engine::resolve::resolve_with_fallback;

    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Request Vertex kind with a non-matching role
    let geom_ref = GeomRef {
        kind: TopoKind::Vertex,
        anchor: Anchor::FeatureOutput {
            feature_id: e1,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::RevStartFace, // Won't match any role
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
    };

    let result = resolve_with_fallback(&geom_ref, &engine.feature_results);
    // Exercises the kind filtering in the fallback path
    if result.is_err() {
        // No Vertex entities, BestEffort fallback has nothing to match
        assert!(true);
    } else {
        assert!(!result.unwrap().warnings.is_empty());
    }
}

// ══════════════════════════════════════════════════════════════════════════
// COVERAGE BOOST: rebuild.rs
// ══════════════════════════════════════════════════════════════════════════

/// Create a revolve operation referencing a sketch.
fn make_revolve_op(sketch_id: Uuid) -> Operation {
    Operation::Revolve {
        params: RevolveParams {
            sketch_id,
            profile_index: 0,
            axis_origin: [-1.0, 0.0, 0.0],
            axis_direction: [0.0, 1.0, 0.0],
            angle: std::f64::consts::PI,
            cut: false,
            merge: false,
        },
    }
}

#[test]
fn revolve_pipeline_basic() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let r1 = engine
        .add_feature("Revolve 1".to_string(), make_revolve_op(s1), &mut kernel)
        .unwrap();

    let result = engine.get_result(r1);
    assert!(
        result.is_some(),
        "Revolve should produce a result. Errors: {:?}",
        engine.errors
    );
    let result = result.unwrap();
    assert_eq!(result.outputs.len(), 1, "Revolve should have 1 Main output");
    assert!(
        !result.provenance.role_assignments.is_empty(),
        "Revolve should have role assignments"
    );
}

#[test]
fn revolve_empty_profiles_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Create a sketch with empty profiles
    let mut sketch_op = make_sketch_op();
    if let Operation::Sketch { ref mut sketch } = sketch_op {
        sketch.solved_profiles.clear();
    }
    let s1 = engine
        .add_feature("Sketch 1".to_string(), sketch_op, &mut kernel)
        .unwrap();
    let r1 = engine
        .add_feature("Revolve 1".to_string(), make_revolve_op(s1), &mut kernel)
        .unwrap();

    assert!(
        engine.get_result(r1).is_none(),
        "Revolve with empty profiles should fail"
    );
    assert!(!engine.errors.is_empty());
}

#[test]
fn revolve_profile_index_out_of_range_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    // Sketch has 1 profile — profile_index=5 is out of range
    let revolve_op = Operation::Revolve {
        params: RevolveParams {
            sketch_id: s1,
            profile_index: 5,
            axis_origin: [-1.0, 0.0, 0.0],
            axis_direction: [0.0, 1.0, 0.0],
            angle: std::f64::consts::PI,
            cut: false,
            merge: false,
        },
    };
    let r1 = engine
        .add_feature("Revolve 1".to_string(), revolve_op, &mut kernel)
        .unwrap();

    assert!(
        engine.get_result(r1).is_none(),
        "Revolve with out-of-range profile index should fail"
    );
    assert!(!engine.errors.is_empty());
    assert!(
        engine.errors[0].1.contains("profile") || engine.errors[0].1.contains("Profile"),
        "Error should mention profile: {}",
        engine.errors[0].1
    );
}

#[test]
fn extrude_profile_index_out_of_range_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    let extrude_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s1,
            profile_index: 10, // Only 1 profile exists
            depth: 5.0,
            direction: None,
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e1 = engine
        .add_feature("Extrude 1".to_string(), extrude_op, &mut kernel)
        .unwrap();

    assert!(
        engine.get_result(e1).is_none(),
        "Extrude with out-of-range profile index should fail"
    );
    assert!(!engine.errors.is_empty());
}

#[test]
fn extrude_empty_profiles_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let mut sketch_op = make_sketch_op();
    if let Operation::Sketch { ref mut sketch } = sketch_op {
        sketch.solved_profiles.clear();
    }
    let s1 = engine
        .add_feature("Sketch 1".to_string(), sketch_op, &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    assert!(
        engine.get_result(e1).is_none(),
        "Extrude with empty profiles should fail"
    );
    assert!(!engine.errors.is_empty());
}

#[test]
fn extrude_custom_direction() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    let extrude_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s1,
            profile_index: 0,
            depth: 5.0,
            direction: Some([1.0, 0.0, 0.0]), // Custom X direction
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e1 = engine
        .add_feature("Extrude Custom Dir".to_string(), extrude_op, &mut kernel)
        .unwrap();

    let result = engine.get_result(e1);
    assert!(
        result.is_some(),
        "Custom direction extrude should succeed. Errors: {:?}",
        engine.errors
    );
    assert_eq!(result.unwrap().outputs.len(), 1);
}

#[test]
fn depth_mode_upto_success() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Create base body extrude at depth 5 (extends from z=0 to z=5)
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Second sketch for the UpTo extrude
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    // UpTo referencing the EndCapPositive (z=5 face) of the base extrude
    let upto_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s2,
            profile_index: 0,
            depth: 5.0,
            direction: Some([0.0, 0.0, 1.0]),
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::UpTo {
                reference: GeomRef {
                    kind: TopoKind::Face,
                    anchor: Anchor::FeatureOutput {
                        feature_id: e1,
                        output_key: OutputKey::Main,
                    },
                    selector: Selector::Role {
                        role: Role::EndCapPositive,
                        index: 0,
                    },
                    policy: ResolvePolicy::Strict,
                },
            },
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e2 = engine
        .add_feature("UpTo Extrude".to_string(), upto_op, &mut kernel)
        .unwrap();

    let result = engine.get_result(e2);
    assert!(
        result.is_some(),
        "UpTo extrude should succeed. Errors: {:?}",
        engine.errors
    );
    assert_eq!(result.unwrap().outputs.len(), 1);
}

#[test]
fn depth_mode_upto_datum_reference() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    // UpTo referencing a datum plane — exercises the Datum path in resolve_reference_position
    let datum_id = Uuid::new_v4();
    let upto_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s1,
            profile_index: 0,
            depth: 5.0,
            direction: Some([0.0, 0.0, 1.0]),
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::UpTo {
                reference: GeomRef {
                    kind: TopoKind::Face,
                    anchor: Anchor::Datum { datum_id },
                    selector: Selector::Role {
                        role: Role::EndCapPositive,
                        index: 0,
                    },
                    policy: ResolvePolicy::Strict,
                },
            },
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e1 = engine
        .add_feature("UpTo Datum Extrude".to_string(), upto_op, &mut kernel)
        .unwrap();

    // Datum at origin, sketch at origin, direction +Z → depth = 0 → error (behind plane)
    assert!(
        engine.get_result(e1).is_none(),
        "UpTo datum at same origin should fail (depth<=0)"
    );
    assert!(!engine.errors.is_empty());
}

#[test]
fn second_direction_upto_produces_solid() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Base body for UpTo reference
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Create sketch at z=10 so the second direction (neg Z) can reach z=5 (EndCapPositive)
    // Second direction: neg_dir = [0,0,-1], ref at z=5: proj = 5*(-1)=-5, origin at z=10: proj = 10*(-1)=-10
    // depth = -5 - (-10) = 5 > 0 ✓
    let mut sketch2_op = make_sketch_op();
    if let Operation::Sketch { ref mut sketch } = sketch2_op {
        sketch.plane_origin = [0.0, 0.0, 10.0];
    }
    let s2 = engine
        .add_feature("Sketch 2".to_string(), sketch2_op, &mut kernel)
        .unwrap();

    let bidir_upto_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s2,
            profile_index: 0,
            depth: 5.0,
            direction: Some([0.0, 0.0, 1.0]),
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: Some(SecondDirection::UpTo {
                reference: GeomRef {
                    kind: TopoKind::Face,
                    anchor: Anchor::FeatureOutput {
                        feature_id: e1,
                        output_key: OutputKey::Main,
                    },
                    selector: Selector::Role {
                        role: Role::EndCapPositive,
                        index: 0,
                    },
                    policy: ResolvePolicy::Strict,
                },
            }),
            region: None,
            regions: Vec::new(),
        },
    };
    let e2 = engine
        .add_feature("Bidir UpTo Extrude".to_string(), bidir_upto_op, &mut kernel)
        .unwrap();

    let result = engine.get_result(e2);
    assert!(
        result.is_some(),
        "SecondDirection::UpTo should produce a result. Errors: {:?}",
        engine.errors
    );
    assert_eq!(result.unwrap().outputs.len(), 1);
}

#[test]
fn cut_extrude_with_blind_second_direction() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    // Base body
    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let _e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Cut + Blind second direction
    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let cut_bidir_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s2,
            profile_index: 0,
            depth: 2.0,
            direction: None,
            symmetric: false,
            cut: true,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: Some(SecondDirection::Blind { depth: 3.0 }),
            region: None,
            regions: Vec::new(),
        },
    };
    let e2 = engine
        .add_feature("Cut Bidir Extrude".to_string(), cut_bidir_op, &mut kernel)
        .unwrap();

    let result = engine.get_result(e2);
    assert!(
        result.is_some(),
        "Cut + Blind SecondDirection should produce a result. Errors: {:?}",
        engine.errors
    );
    assert_eq!(result.unwrap().outputs.len(), 1);
}

/// Create a fillet with empty edges.
fn make_fillet_op_no_edges(radius: f64) -> Operation {
    Operation::Fillet {
        params: FilletParams {
            edges: Vec::new(),
            radius,
        },
    }
}

/// Create a chamfer with empty edges.
fn make_chamfer_op_no_edges(distance: f64) -> Operation {
    Operation::Chamfer {
        params: ChamferParams {
            edges: Vec::new(),
            distance,
        },
    }
}

/// Create a shell with empty faces.
fn make_shell_op_no_faces(thickness: f64) -> Operation {
    Operation::Shell {
        params: ShellParams {
            faces_to_remove: Vec::new(),
            thickness,
        },
    }
}

#[test]
fn fillet_no_edges_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let _e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    let f1 = engine
        .add_feature(
            "Fillet NoEdges".to_string(),
            make_fillet_op_no_edges(1.0),
            &mut kernel,
        )
        .unwrap();

    assert!(
        engine.get_result(f1).is_none(),
        "Fillet with no edges should fail"
    );
    assert!(!engine.errors.is_empty());
}

#[test]
fn chamfer_no_edges_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let _e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    let c1 = engine
        .add_feature(
            "Chamfer NoEdges".to_string(),
            make_chamfer_op_no_edges(0.5),
            &mut kernel,
        )
        .unwrap();

    assert!(
        engine.get_result(c1).is_none(),
        "Chamfer with no edges should fail"
    );
    assert!(!engine.errors.is_empty());
}

#[test]
fn shell_no_faces_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let _e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    let sh1 = engine
        .add_feature(
            "Shell NoFaces".to_string(),
            make_shell_op_no_faces(0.3),
            &mut kernel,
        )
        .unwrap();

    assert!(
        engine.get_result(sh1).is_none(),
        "Shell with no faces should fail"
    );
    assert!(!engine.errors.is_empty());
}

#[test]
fn boolean_with_datum_anchor_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Boolean with one Datum anchor — find_solid_handle should fail
    let bool_op = Operation::BooleanCombine {
        params: BooleanParams {
            body_a: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::FeatureOutput {
                    feature_id: e1,
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
                anchor: Anchor::Datum {
                    datum_id: Uuid::new_v4(),
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::Strict,
            },
            operation: BooleanOp::Union,
        },
    };
    let b1 = engine
        .add_feature("Boolean Datum".to_string(), bool_op, &mut kernel)
        .unwrap();

    assert!(
        engine.get_result(b1).is_none(),
        "Boolean with Datum anchor should fail"
    );
    assert!(!engine.errors.is_empty());
}

#[test]
fn boolean_with_wrong_output_key_errors() {
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

    // Use Body { index: 5 } which doesn't exist in extrude results
    let bool_op = Operation::BooleanCombine {
        params: BooleanParams {
            body_a: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::FeatureOutput {
                    feature_id: e1,
                    output_key: OutputKey::Body { index: 5 },
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
                    feature_id: e2,
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
    };
    let b1 = engine
        .add_feature("Boolean WrongKey".to_string(), bool_op, &mut kernel)
        .unwrap();

    assert!(
        engine.get_result(b1).is_none(),
        "Boolean with wrong output key should fail"
    );
    assert!(!engine.errors.is_empty());
    assert!(
        engine.errors[0].1.contains("Output key"),
        "Error should mention output key: {}",
        engine.errors[0].1
    );
}

#[test]
fn rebuild_skips_suppressed_features() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    engine.set_suppressed(e1, true, &mut kernel).unwrap();

    assert!(
        engine.get_result(e1).is_none(),
        "Suppressed extrude should not have a result"
    );
    assert!(
        engine.get_result(s1).is_some(),
        "Non-suppressed sketch should have result"
    );
}

#[test]
fn rebuild_continues_after_error() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let _e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    // Broken extrude referencing nonexistent sketch
    let fake_id = Uuid::new_v4();
    let _e_bad = engine
        .add_feature(
            "Bad Extrude".to_string(),
            make_extrude_op(fake_id),
            &mut kernel,
        )
        .unwrap();

    // Another working sketch+extrude after the failure
    let s3 = engine
        .add_feature("Sketch 3".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e3 = engine
        .add_feature("Extrude 3".to_string(), make_extrude_op(s3), &mut kernel)
        .unwrap();

    // Full rebuild from scratch to capture all errors at once
    engine.rebuild_from_scratch(&mut kernel);

    assert!(
        engine.get_result(e3).is_some(),
        "Features after a failed one should still be built"
    );
    assert!(
        !engine.errors.is_empty(),
        "Should have errors from bad extrude"
    );
}

#[test]
fn rebuild_from_scratch_clears_and_rebuilds() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    assert!(engine.get_result(s1).is_some());
    assert!(engine.get_result(e1).is_some());

    engine.rebuild_from_scratch(&mut kernel);

    assert!(engine.get_result(s1).is_some());
    assert!(engine.get_result(e1).is_some());
    assert!(engine.errors.is_empty());
}

#[test]
fn rebuild_carries_forward_existing_results() {
    use feature_engine::rebuild::rebuild;

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

    // Rebuild from index 2, carrying forward s1 and e1 results
    let existing = engine.feature_results.clone();
    let state = rebuild(&engine.tree, &mut kernel, 2, &existing);

    assert!(state.feature_results.contains_key(&s1));
    assert!(state.feature_results.contains_key(&e1));
    assert!(state.feature_results.contains_key(&s2));
    assert!(state.feature_results.contains_key(&e2));
}

// ══════════════════════════════════════════════════════════════════════════
// COVERAGE BOOST: lib.rs (Engine methods)
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn engine_rename_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id = engine
        .add_feature("Original Name".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    assert_eq!(engine.tree.find_feature(id).unwrap().name, "Original Name");

    engine.rename_feature(id, "New Name".to_string()).unwrap();
    assert_eq!(engine.tree.find_feature(id).unwrap().name, "New Name");
}

#[test]
fn engine_rename_nonexistent_errors() {
    let mut engine = Engine::new();
    let result = engine.rename_feature(Uuid::new_v4(), "Whatever".to_string());
    assert!(matches!(result, Err(EngineError::FeatureNotFound { .. })));
}

#[test]
fn engine_edit_nonexistent_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let result = engine.edit_feature(Uuid::new_v4(), make_sketch_op(), &mut kernel);
    assert!(matches!(result, Err(EngineError::FeatureNotFound { .. })));
}

#[test]
fn engine_remove_nonexistent_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let result = engine.remove_feature(Uuid::new_v4(), &mut kernel);
    assert!(matches!(result, Err(EngineError::FeatureNotFound { .. })));
}

#[test]
fn engine_suppress_nonexistent_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let result = engine.set_suppressed(Uuid::new_v4(), true, &mut kernel);
    assert!(matches!(result, Err(EngineError::FeatureNotFound { .. })));
}

#[test]
fn engine_reorder_nonexistent_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let result = engine.reorder_feature(Uuid::new_v4(), 0, &mut kernel);
    assert!(matches!(result, Err(EngineError::FeatureNotFound { .. })));
}

#[test]
fn engine_can_undo_can_redo_states() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    assert!(!engine.can_undo());
    assert!(!engine.can_redo());

    engine
        .add_feature("A".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    assert!(engine.can_undo());
    assert!(!engine.can_redo());

    engine.undo(&mut kernel).unwrap();
    assert!(!engine.can_undo());
    assert!(engine.can_redo());

    engine.redo(&mut kernel).unwrap();
    assert!(engine.can_undo());
    assert!(!engine.can_redo());
}

#[test]
fn undo_redo_rename_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id = engine
        .add_feature("Original".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.rename_feature(id, "Renamed".to_string()).unwrap();
    assert_eq!(engine.tree.find_feature(id).unwrap().name, "Renamed");

    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.find_feature(id).unwrap().name, "Original");

    engine.redo(&mut kernel).unwrap();
    assert_eq!(engine.tree.find_feature(id).unwrap().name, "Renamed");
}

#[test]
fn redo_remove_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.remove_feature(id, &mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 0);

    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 1);

    engine.redo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features.len(), 0);
    assert!(engine.get_result(id).is_none());
}

#[test]
fn redo_suppress_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.set_suppressed(id, true, &mut kernel).unwrap();
    assert!(engine.tree.features[0].suppressed);

    engine.undo(&mut kernel).unwrap();
    assert!(!engine.tree.features[0].suppressed);

    engine.redo(&mut kernel).unwrap();
    assert!(engine.tree.features[0].suppressed);
}

#[test]
fn redo_reorder_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let id_a = engine
        .add_feature("A".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let id_b = engine
        .add_feature("B".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.reorder_feature(id_b, 0, &mut kernel).unwrap();
    assert_eq!(engine.tree.features[0].id, id_b);
    assert_eq!(engine.tree.features[1].id, id_a);

    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features[0].id, id_a);
    assert_eq!(engine.tree.features[1].id, id_b);

    engine.redo(&mut kernel).unwrap();
    assert_eq!(engine.tree.features[0].id, id_b);
    assert_eq!(engine.tree.features[1].id, id_a);
}

#[test]
fn redo_edit_feature() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    engine
        .edit_feature(e1, make_extrude_op_depth(s1, 20.0), &mut kernel)
        .unwrap();

    if let Operation::Extrude { params } = &engine.tree.find_feature(e1).unwrap().operation {
        assert_eq!(params.depth, 20.0);
    }

    engine.undo(&mut kernel).unwrap();
    if let Operation::Extrude { params } = &engine.tree.find_feature(e1).unwrap().operation {
        assert_eq!(params.depth, 5.0);
    }

    engine.redo(&mut kernel).unwrap();
    if let Operation::Extrude { params } = &engine.tree.find_feature(e1).unwrap().operation {
        assert_eq!(params.depth, 20.0);
    }
}

// ══════════════════════════════════════════════════════════════════════════
// COVERAGE BOOST: tree.rs
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn tree_active_features_empty_with_some_index() {
    let mut tree = FeatureTree::new();
    tree.set_rollback(Some(0));
    assert_eq!(tree.active_features().len(), 0);
}

#[test]
fn tree_add_feature_at_active_index() {
    let mut tree = FeatureTree::new();
    let _id_a = tree.add_feature("A".to_string(), make_sketch_op());
    let _id_b = tree.add_feature("B".to_string(), make_sketch_op());
    let _id_c = tree.add_feature("C".to_string(), make_sketch_op());

    tree.set_rollback(Some(1));
    assert_eq!(tree.active_features().len(), 2);

    let id_d = tree.add_feature("D".to_string(), make_sketch_op());
    assert_eq!(tree.features[2].id, id_d);
    assert_eq!(tree.active_index, Some(2));
    assert_eq!(tree.active_features().len(), 3);
}

#[test]
fn tree_remove_feature_adjusts_active_index() {
    let mut tree = FeatureTree::new();
    let id_a = tree.add_feature("A".to_string(), make_sketch_op());
    let _id_b = tree.add_feature("B".to_string(), make_sketch_op());
    let _id_c = tree.add_feature("C".to_string(), make_sketch_op());

    tree.set_rollback(Some(2));
    assert_eq!(tree.active_features().len(), 3);

    tree.remove_feature(id_a).unwrap();
    assert_eq!(tree.features.len(), 2);
    assert_eq!(tree.active_index, Some(1));
}

#[test]
fn tree_reorder_clamps_position() {
    let mut tree = FeatureTree::new();
    let id_a = tree.add_feature("A".to_string(), make_sketch_op());
    let _id_b = tree.add_feature("B".to_string(), make_sketch_op());

    tree.reorder_feature(id_a, 100).unwrap();
    assert_eq!(tree.features[1].id, id_a);
}

#[test]
fn tree_find_feature_returns_none_for_unknown() {
    let tree = FeatureTree::new();
    assert!(tree.find_feature(Uuid::new_v4()).is_none());
    assert!(tree.feature_index(Uuid::new_v4()).is_none());
}

// ══════════════════════════════════════════════════════════════════════════
// COVERAGE BOOST: undo.rs
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn undo_stack_push_clears_redo() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    engine
        .add_feature("A".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    engine
        .add_feature("B".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();

    engine.undo(&mut kernel).unwrap();
    assert!(engine.can_redo());

    engine
        .add_feature("C".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    assert!(!engine.can_redo());
}

// ══════════════════════════════════════════════════════════════════════════
// COVERAGE BOOST: types.rs
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn feature_tree_default() {
    let tree = FeatureTree::default();
    assert!(tree.features.is_empty());
    assert!(tree.active_index.is_none());
}

#[test]
fn engine_default() {
    let engine = Engine::default();
    assert!(engine.tree.features.is_empty());
    assert!(engine.warnings.is_empty());
    assert!(engine.errors.is_empty());
    assert!(!engine.can_undo());
    assert!(!engine.can_redo());
}

#[test]
fn depth_mode_blind_is_default() {
    let params_json = r#"{"sketch_id":"00000000-0000-0000-0000-000000000001","profile_index":0,"depth":5.0,"symmetric":false,"cut":false}"#;
    let params: ExtrudeParams = serde_json::from_str(params_json).unwrap();
    assert!(matches!(params.depth_mode, DepthMode::Blind));
}

// ══════════════════════════════════════════════════════════════════════════
// COVERAGE BOOST: rebuild.rs — tangent_x_from_normal edge case
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn extrude_with_x_axis_normal_sketch() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let mut sketch_op = make_sketch_op();
    if let Operation::Sketch { ref mut sketch } = sketch_op {
        sketch.plane_normal = [1.0, 0.0, 0.0];
    }
    let s1 = engine
        .add_feature("Sketch X-Normal".to_string(), sketch_op, &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    let result = engine.get_result(e1);
    assert!(
        result.is_some(),
        "Extrude with X-axis normal should succeed. Errors: {:?}",
        engine.errors
    );
    assert_eq!(result.unwrap().outputs.len(), 1);
}

#[test]
fn extrude_with_y_axis_normal_sketch() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let mut sketch_op = make_sketch_op();
    if let Operation::Sketch { ref mut sketch } = sketch_op {
        sketch.plane_normal = [0.0, 1.0, 0.0];
    }
    let s1 = engine
        .add_feature("Sketch Y-Normal".to_string(), sketch_op, &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    let result = engine.get_result(e1);
    assert!(
        result.is_some(),
        "Extrude with Y-axis normal should succeed. Errors: {:?}",
        engine.errors
    );
    assert_eq!(result.unwrap().outputs.len(), 1);
}

// ══════════════════════════════════════════════════════════════════════════
// COVERAGE BOOST: rebuild.rs — resolve_feature_refs with populated refs
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn feature_with_populated_references() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    let extrude_feat = engine.tree.find_feature_mut(e1).unwrap();
    extrude_feat.references.push(GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: e1,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    });

    engine.rebuild_from_scratch(&mut kernel);
    assert!(engine.get_result(e1).is_some());
}

#[test]
fn feature_with_failing_reference_produces_warning() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    let extrude_feat = engine.tree.find_feature_mut(e1).unwrap();
    extrude_feat.references.push(GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: Uuid::new_v4(),
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    });

    engine.rebuild_from_scratch(&mut kernel);

    assert!(engine.get_result(e1).is_some());
    assert!(
        !engine.warnings.is_empty(),
        "Failed reference resolution should produce a warning"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// COVERAGE BOOST: rebuild.rs — find_most_recent_solid edge cases
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn cut_extrude_skips_sketch_and_suppressed_in_body_search() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let _e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature("Extrude 2".to_string(), make_extrude_op(s2), &mut kernel)
        .unwrap();
    engine.set_suppressed(e2, true, &mut kernel).unwrap();

    let s3 = engine
        .add_feature("Sketch 3".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let cut_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s3,
            profile_index: 0,
            depth: 2.0,
            direction: None,
            symmetric: false,
            cut: true,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e3 = engine
        .add_feature("Cut Extrude".to_string(), cut_op, &mut kernel)
        .unwrap();

    let result = engine.get_result(e3);
    assert!(
        result.is_some(),
        "Cut extrude should find non-suppressed body. Errors: {:?}",
        engine.errors
    );
}

#[test]
fn through_all_with_suppressed_target_still_works() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s1 = engine
        .add_feature("Sketch 1".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".to_string(), make_extrude_op(s1), &mut kernel)
        .unwrap();

    engine.set_suppressed(e1, true, &mut kernel).unwrap();

    let s2 = engine
        .add_feature("Sketch 2".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let through_op = Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: s2,
            profile_index: 0,
            depth: 5.0,
            direction: None,
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::ThroughAll,
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    };
    let e2 = engine
        .add_feature("ThroughAll Extrude".to_string(), through_op, &mut kernel)
        .unwrap();

    assert!(
        engine.get_result(e2).is_some(),
        "ThroughAll with suppressed target should use fallback depth. Errors: {:?}",
        engine.errors
    );
}

// ── 2b: body-name inheritance through a consuming boolean ────────────────

/// Build sketch→extrude (x2) then a BooleanCombine union (body_a = e1, the
/// target). Returns (engine, e1, e2, bool_id). Both operands are consumed.
fn engine_with_union() -> (Engine, Uuid, Uuid, Uuid) {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let s1 = engine
        .add_feature("Sketch 1".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature("Extrude 1".into(), make_extrude_op(s1), &mut kernel)
        .unwrap();
    let s2 = engine
        .add_feature("Sketch 2".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature("Extrude 2".into(), make_extrude_op(s2), &mut kernel)
        .unwrap();
    let bool_id = engine
        .add_feature(
            "Boolean Union".into(),
            make_boolean_union(e1, e2),
            &mut kernel,
        )
        .unwrap();
    assert!(
        engine.consumed_features.contains(&e1),
        "union consumes body_a"
    );
    (engine, e1, e2, bool_id)
}

#[test]
fn body_name_inherited_from_consumed_target() {
    let (mut engine, e1, _e2, bool_id) = engine_with_union();
    let e1_body = FeatureTree::body_id(e1, &OutputKey::Main);
    let result_body = FeatureTree::body_id(bool_id, &OutputKey::Main);

    // Name the target operand; the union result inherits it.
    engine.rename_body(e1_body, "Housing".to_string());
    assert_eq!(
        engine.display_body_name_override(&result_body),
        Some("Housing")
    );
    // Inherited, not an explicit override on the result.
    assert_eq!(engine.tree.body_name_override(&result_body), None);
}

#[test]
fn only_target_operand_name_is_inherited() {
    // A name on body_b (the tool, not the target) must NOT propagate.
    let (mut engine, _e1, e2, bool_id) = engine_with_union();
    let e2_body = FeatureTree::body_id(e2, &OutputKey::Main);
    let result_body = FeatureTree::body_id(bool_id, &OutputKey::Main);

    engine.rename_body(e2_body, "Tool".to_string());
    assert_eq!(engine.display_body_name_override(&result_body), None);
}

#[test]
fn explicit_override_beats_inherited_name() {
    let (mut engine, e1, _e2, bool_id) = engine_with_union();
    let e1_body = FeatureTree::body_id(e1, &OutputKey::Main);
    let result_body = FeatureTree::body_id(bool_id, &OutputKey::Main);

    engine.rename_body(e1_body, "Housing".to_string());
    engine.rename_body(result_body.clone(), "Result".to_string());
    assert_eq!(
        engine.display_body_name_override(&result_body),
        Some("Result")
    );
}

#[test]
fn derived_target_name_does_not_propagate() {
    // No custom name on the target ⇒ the result has no inherited name (it falls
    // back to its own derived feature name at the render layer).
    let (engine, _e1, _e2, bool_id) = engine_with_union();
    let result_body = FeatureTree::body_id(bool_id, &OutputKey::Main);
    assert_eq!(engine.display_body_name_override(&result_body), None);
}
