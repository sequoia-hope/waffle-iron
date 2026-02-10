use feature_engine::types::{
    BooleanOp, BooleanParams, ChamferParams, ExtrudeParams, Feature, FeatureTree, FilletParams,
    Operation, RevolveParams, ShellParams,
};
use file_format::{
    export_step, load_project, save_project, LoadError, ProjectMetadata, FORMAT_VERSION,
};
use uuid::Uuid;
use waffle_types::{
    Anchor, ClosedProfile, GeomRef, OutputKey, ResolvePolicy, Role, Selector, Sketch,
    SketchConstraint, SketchEntity, SolveStatus, TopoKind,
};

// ── Helper Functions ─────────────────────────────────────────────────────

fn make_sketch_feature(name: &str) -> Feature {
    let plane_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::nil(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
    };

    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane: plane_ref,
        entities: vec![
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
        ],
        constraints: vec![
            SketchConstraint::Horizontal { entity: 5 },
            SketchConstraint::Horizontal { entity: 7 },
            SketchConstraint::Vertical { entity: 6 },
            SketchConstraint::Vertical { entity: 8 },
        ],
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: {
            let mut m = std::collections::HashMap::new();
            m.insert(1, (0.0, 0.0));
            m.insert(2, (100.0, 0.0));
            m.insert(3, (100.0, 50.0));
            m.insert(4, (0.0, 50.0));
            m
        },
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        }],
    };

    Feature {
        id: Uuid::new_v4(),
        name: name.to_string(),
        operation: Operation::Sketch { sketch },
        suppressed: false,
        references: Vec::new(),
    }
}

fn make_extrude_feature(name: &str, sketch_id: Uuid) -> Feature {
    Feature {
        id: Uuid::new_v4(),
        name: name.to_string(),
        operation: Operation::Extrude {
            params: ExtrudeParams {
                sketch_id,
                profile_index: 0,
                depth: 50.0,
                direction: None,
                symmetric: false,
                cut: false,
                target_body: None,
            },
        },
        suppressed: false,
        references: vec![GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::FeatureOutput {
                feature_id: Uuid::new_v4(),
                output_key: OutputKey::Main,
            },
            selector: Selector::Role {
                role: Role::EndCapPositive,
                index: 0,
            },
            policy: ResolvePolicy::BestEffort,
        }],
    }
}

fn make_simple_tree() -> FeatureTree {
    let sketch = make_sketch_feature("Sketch 1");
    let sketch_id = match &sketch.operation {
        Operation::Sketch { sketch } => sketch.id,
        _ => unreachable!(),
    };
    let extrude = make_extrude_feature("Extrude 1", sketch_id);

    let mut tree = FeatureTree::new();
    tree.features.push(sketch);
    tree.features.push(extrude);
    tree
}

// ── M1: JSON Schema Tests ────────────────────────────────────────────────

#[test]
fn save_produces_valid_json() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test Project");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_object());
}

#[test]
fn save_includes_format_and_version() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test Project");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["format"], "waffle-iron");
    assert_eq!(parsed["version"], FORMAT_VERSION);
}

#[test]
fn save_includes_project_metadata() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("My Box Part");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["project"]["name"], "My Box Part");
    assert!(parsed["project"]["created"].is_string());
    assert!(parsed["project"]["modified"].is_string());
}

#[test]
fn save_includes_features_array() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let features = &parsed["features"]["features"];
    assert!(features.is_array());
    assert_eq!(features.as_array().unwrap().len(), 2);
}

#[test]
fn save_serializes_operation_type_tags() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let features = parsed["features"]["features"].as_array().unwrap();

    assert_eq!(features[0]["operation"]["type"], "Sketch");
    assert_eq!(features[1]["operation"]["type"], "Extrude");
}

#[test]
fn save_serializes_geom_refs() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let features = parsed["features"]["features"].as_array().unwrap();

    let refs = &features[1]["references"];
    assert!(refs.is_array());
    assert!(!refs.as_array().unwrap().is_empty());
}

// ── M2: Save Tests ──────────────────────────────────────────────────────

#[test]
fn save_empty_tree() {
    let tree = FeatureTree::new();
    let meta = ProjectMetadata::new("Empty");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["features"]["features"].as_array().unwrap().len(), 0);
}

#[test]
fn save_all_operation_types() {
    let mut tree = FeatureTree::new();
    let sketch = make_sketch_feature("Sketch");
    let sketch_id = match &sketch.operation {
        Operation::Sketch { sketch } => sketch.id,
        _ => unreachable!(),
    };
    tree.features.push(sketch);

    tree.features.push(Feature {
        id: Uuid::new_v4(),
        name: "Extrude".to_string(),
        operation: Operation::Extrude {
            params: ExtrudeParams {
                sketch_id,
                profile_index: 0,
                depth: 25.0,
                direction: Some([0.0, 0.0, 1.0]),
                symmetric: true,
                cut: false,
                target_body: None,
            },
        },
        suppressed: false,
        references: Vec::new(),
    });

    tree.features.push(Feature {
        id: Uuid::new_v4(),
        name: "Revolve".to_string(),
        operation: Operation::Revolve {
            params: RevolveParams {
                sketch_id,
                profile_index: 0,
                axis_origin: [0.0, 0.0, 0.0],
                axis_direction: [0.0, 1.0, 0.0],
                angle: std::f64::consts::PI,
            },
        },
        suppressed: false,
        references: Vec::new(),
    });

    tree.features.push(Feature {
        id: Uuid::new_v4(),
        name: "Fillet".to_string(),
        operation: Operation::Fillet {
            params: FilletParams {
                edges: Vec::new(),
                radius: 2.0,
            },
        },
        suppressed: false,
        references: Vec::new(),
    });

    tree.features.push(Feature {
        id: Uuid::new_v4(),
        name: "Chamfer".to_string(),
        operation: Operation::Chamfer {
            params: ChamferParams {
                edges: Vec::new(),
                distance: 1.5,
            },
        },
        suppressed: false,
        references: Vec::new(),
    });

    tree.features.push(Feature {
        id: Uuid::new_v4(),
        name: "Shell".to_string(),
        operation: Operation::Shell {
            params: ShellParams {
                faces_to_remove: Vec::new(),
                thickness: 0.5,
            },
        },
        suppressed: false,
        references: Vec::new(),
    });

    let dummy_ref = GeomRef {
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
    };

    tree.features.push(Feature {
        id: Uuid::new_v4(),
        name: "Boolean".to_string(),
        operation: Operation::BooleanCombine {
            params: BooleanParams {
                body_a: dummy_ref.clone(),
                body_b: dummy_ref,
                operation: BooleanOp::Union,
            },
        },
        suppressed: false,
        references: Vec::new(),
    });

    let meta = ProjectMetadata::new("All Operations");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let features = parsed["features"]["features"].as_array().unwrap();
    assert_eq!(features.len(), 7);

    let types: Vec<&str> = features
        .iter()
        .map(|f| f["operation"]["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        vec![
            "Sketch",
            "Extrude",
            "Revolve",
            "Fillet",
            "Chamfer",
            "Shell",
            "BooleanCombine"
        ]
    );
}

#[test]
fn save_preserves_suppressed_flag() {
    let mut tree = make_simple_tree();
    tree.features[1].suppressed = true;

    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let features = parsed["features"]["features"].as_array().unwrap();
    assert_eq!(features[0]["suppressed"], false);
    assert_eq!(features[1]["suppressed"], true);
}

// ── M3: Load Tests ──────────────────────────────────────────────────────

#[test]
fn load_round_trip_simple_tree() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Round Trip");
    let json = save_project(&tree, &meta);

    let (loaded_tree, loaded_meta) = load_project(&json).unwrap();

    assert_eq!(loaded_tree.features.len(), tree.features.len());
    assert_eq!(loaded_meta.name, "Round Trip");
}

#[test]
fn load_preserves_feature_ids() {
    let tree = make_simple_tree();
    let original_ids: Vec<Uuid> = tree.features.iter().map(|f| f.id).collect();

    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);
    let (loaded_tree, _) = load_project(&json).unwrap();

    let loaded_ids: Vec<Uuid> = loaded_tree.features.iter().map(|f| f.id).collect();
    assert_eq!(original_ids, loaded_ids);
}

#[test]
fn load_preserves_operation_params() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);
    let (loaded_tree, _) = load_project(&json).unwrap();

    match &loaded_tree.features[1].operation {
        Operation::Extrude { params } => {
            assert_eq!(params.depth, 50.0);
            assert_eq!(params.profile_index, 0);
            assert!(!params.symmetric);
            assert!(!params.cut);
        }
        other => panic!("Expected Extrude, got {:?}", other),
    }
}

#[test]
fn load_preserves_sketch_entities_and_constraints() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);
    let (loaded_tree, _) = load_project(&json).unwrap();

    match &loaded_tree.features[0].operation {
        Operation::Sketch { sketch } => {
            assert_eq!(sketch.entities.len(), 8); // 4 points + 4 lines
            assert_eq!(sketch.constraints.len(), 4);
        }
        other => panic!("Expected Sketch, got {:?}", other),
    }
}

#[test]
fn load_preserves_geom_refs() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);
    let (loaded_tree, _) = load_project(&json).unwrap();

    assert_eq!(loaded_tree.features[1].references.len(), 1);
    let geo_ref = &loaded_tree.features[1].references[0];
    assert_eq!(geo_ref.kind, TopoKind::Face);
    assert!(matches!(geo_ref.policy, ResolvePolicy::BestEffort));
}

#[test]
fn load_rejects_unknown_format() {
    let json = r#"{"format": "not-waffle", "version": 1, "project": {"name": "x", "created": "2025-01-01T00:00:00Z", "modified": "2025-01-01T00:00:00Z"}, "features": {"features": [], "active_index": null}}"#;
    let result = load_project(json);
    assert!(matches!(result, Err(LoadError::UnknownFormat(_))));
}

#[test]
fn load_rejects_future_version() {
    let json = format!(
        r#"{{"format": "waffle-iron", "version": {}, "project": {{"name": "x", "created": "2025-01-01T00:00:00Z", "modified": "2025-01-01T00:00:00Z"}}, "features": {{"features": [], "active_index": null}}}}"#,
        FORMAT_VERSION + 1
    );
    let result = load_project(&json);
    assert!(matches!(result, Err(LoadError::FutureVersion { .. })));
}

#[test]
fn load_rejects_invalid_json() {
    let result = load_project("this is not json");
    assert!(matches!(result, Err(LoadError::ParseError(_))));
}

#[test]
fn load_preserves_active_index() {
    let mut tree = make_simple_tree();
    tree.active_index = Some(0);

    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);
    let (loaded_tree, _) = load_project(&json).unwrap();

    assert_eq!(loaded_tree.active_index, Some(0));
}

#[test]
fn load_preserves_suppressed_features() {
    let mut tree = make_simple_tree();
    tree.features[1].suppressed = true;

    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);
    let (loaded_tree, _) = load_project(&json).unwrap();

    assert!(!loaded_tree.features[0].suppressed);
    assert!(loaded_tree.features[1].suppressed);
}

// ── M4: STEP Export Tests ──────────────────────────────────────────────

/// Create a tree where sketch_id in ExtrudeParams matches the sketch Feature.id
/// (required for Engine rebuild to find the sketch result).
fn make_rebuild_compatible_tree() -> FeatureTree {
    let sketch_feature_id = Uuid::new_v4();

    let plane_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::nil(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
    };

    let sketch = Sketch {
        id: sketch_feature_id, // Same as the Feature.id
        plane: plane_ref,
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
        solved_positions: {
            let mut m = std::collections::HashMap::new();
            m.insert(1, (0.0, 0.0));
            m.insert(2, (1.0, 0.0));
            m.insert(3, (1.0, 1.0));
            m.insert(4, (0.0, 1.0));
            m
        },
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        }],
    };

    let sketch_feature = Feature {
        id: sketch_feature_id,
        name: "Sketch 1".to_string(),
        operation: Operation::Sketch { sketch },
        suppressed: false,
        references: Vec::new(),
    };

    let extrude_feature = Feature {
        id: Uuid::new_v4(),
        name: "Extrude 1".to_string(),
        operation: Operation::Extrude {
            params: ExtrudeParams {
                sketch_id: sketch_feature_id, // Points to Feature.id
                profile_index: 0,
                depth: 5.0,
                direction: None,
                symmetric: false,
                cut: false,
                target_body: None,
            },
        },
        suppressed: false,
        references: Vec::new(),
    };

    let mut tree = FeatureTree::new();
    tree.features.push(sketch_feature);
    tree.features.push(extrude_feature);
    tree
}

#[test]
fn step_export_simple_box() {
    use kernel_fork::TruckKernel;

    let tree = make_rebuild_compatible_tree();
    let mut kb = TruckKernel::new();

    let step = export_step(&tree, &mut kb).unwrap();

    assert!(step.contains("ISO-10303-21"), "Should have STEP header");
    assert!(
        step.contains("MANIFOLD_SOLID_BREP"),
        "Should have solid BREP entity"
    );
    assert!(step.contains("FACE_SURFACE"), "Should have face entities");
    assert!(step.contains("ENDSEC"), "Should have proper STEP footer");
}

#[test]
fn step_export_empty_tree_returns_error() {
    use kernel_fork::TruckKernel;

    let tree = FeatureTree::new();
    let mut kb = TruckKernel::new();

    let result = export_step(&tree, &mut kb);
    assert!(result.is_err(), "Empty tree should fail STEP export");
}

#[test]
fn step_export_suppressed_only_returns_error() {
    use kernel_fork::TruckKernel;

    let mut tree = make_simple_tree();
    // Suppress the extrude — only sketch remains, which has no solid
    tree.features[1].suppressed = true;

    let mut kb = TruckKernel::new();
    let result = export_step(&tree, &mut kb);
    // Sketch-only tree has no solid outputs
    assert!(result.is_err(), "Sketch-only tree should fail STEP export");
}

// ── M6: Full Round-Trip Tests ──────────────────────────────────────────

#[test]
fn round_trip_save_load_rebuild_produces_solid() {
    use kernel_fork::TruckKernel;

    // 1. Create a rebuild-compatible tree
    let original_tree = make_rebuild_compatible_tree();
    let meta = ProjectMetadata::new("Round Trip Rebuild");

    // 2. Save to JSON
    let json = save_project(&original_tree, &meta);

    // 3. Load back
    let (loaded_tree, loaded_meta) = load_project(&json).unwrap();
    assert_eq!(loaded_meta.name, "Round Trip Rebuild");
    assert_eq!(loaded_tree.features.len(), original_tree.features.len());

    // 4. Rebuild with TruckKernel
    let mut kb = TruckKernel::new();
    let mut engine = feature_engine::Engine::new();
    engine.tree = loaded_tree.clone();
    engine.rebuild_from_scratch(&mut kb);

    // 5. Verify the extrude produced a result
    let extrude_id = loaded_tree.features[1].id;
    let result = engine
        .get_result(extrude_id)
        .expect("Extrude should have a result after rebuild");

    // Should have exactly one Main output
    assert_eq!(result.outputs.len(), 1);
    assert_eq!(result.outputs[0].0, OutputKey::Main);

    // Should have created entities with face roles
    assert!(
        !result.provenance.created.is_empty(),
        "Extrude should create entities"
    );
    assert!(
        !result.provenance.role_assignments.is_empty(),
        "Extrude should assign roles"
    );
}

#[test]
fn round_trip_preserves_feature_ids_through_rebuild() {
    use kernel_fork::TruckKernel;

    let original_tree = make_rebuild_compatible_tree();
    let original_ids: Vec<Uuid> = original_tree.features.iter().map(|f| f.id).collect();

    let meta = ProjectMetadata::new("ID Preservation");
    let json = save_project(&original_tree, &meta);
    let (loaded_tree, _) = load_project(&json).unwrap();

    let loaded_ids: Vec<Uuid> = loaded_tree.features.iter().map(|f| f.id).collect();
    assert_eq!(original_ids, loaded_ids);

    // Rebuild and verify results are keyed by the same IDs
    let mut kb = TruckKernel::new();
    let mut engine = feature_engine::Engine::new();
    engine.tree = loaded_tree;
    engine.rebuild_from_scratch(&mut kb);

    for id in &original_ids {
        assert!(
            engine.get_result(*id).is_some(),
            "Feature {} should have a result",
            id
        );
    }
}

#[test]
fn round_trip_step_export_matches_original() {
    use kernel_fork::TruckKernel;

    // Build original tree and export STEP
    let tree = make_rebuild_compatible_tree();
    let mut kb = TruckKernel::new();
    let original_step = export_step(&tree, &mut kb).unwrap();

    // Save → load → export STEP again
    let meta = ProjectMetadata::new("STEP Round Trip");
    let json = save_project(&tree, &meta);
    let (loaded_tree, _) = load_project(&json).unwrap();

    let mut kb2 = TruckKernel::new();
    let loaded_step = export_step(&loaded_tree, &mut kb2).unwrap();

    // Both STEP exports should contain the same structural elements
    assert!(original_step.contains("MANIFOLD_SOLID_BREP"));
    assert!(loaded_step.contains("MANIFOLD_SOLID_BREP"));

    // Count FACE_SURFACE entities — should be the same
    let original_faces = original_step.matches("FACE_SURFACE").count();
    let loaded_faces = loaded_step.matches("FACE_SURFACE").count();
    assert_eq!(
        original_faces, loaded_faces,
        "Face count should match between original and round-tripped STEP"
    );
}

#[test]
fn round_trip_rebuild_topology_matches() {
    use kernel_fork::TruckKernel;

    // Build original
    let tree = make_rebuild_compatible_tree();
    let mut kb1 = TruckKernel::new();
    let mut engine1 = feature_engine::Engine::new();
    engine1.tree = tree.clone();
    engine1.rebuild_from_scratch(&mut kb1);

    // Save → load → rebuild
    let meta = ProjectMetadata::new("Topology Match");
    let json = save_project(&tree, &meta);
    let (loaded_tree, _) = load_project(&json).unwrap();

    let mut kb2 = TruckKernel::new();
    let mut engine2 = feature_engine::Engine::new();
    engine2.tree = loaded_tree;
    engine2.rebuild_from_scratch(&mut kb2);

    // Compare extrude results
    let extrude_id = tree.features[1].id;
    let result1 = engine1.get_result(extrude_id).unwrap();
    let result2 = engine2.get_result(extrude_id).unwrap();

    // Same number of outputs
    assert_eq!(result1.outputs.len(), result2.outputs.len());

    // Same number of created entities (faces, edges, vertices)
    assert_eq!(
        result1.provenance.created.len(),
        result2.provenance.created.len(),
        "Created entity count should match"
    );

    // Same number of role assignments
    assert_eq!(
        result1.provenance.role_assignments.len(),
        result2.provenance.role_assignments.len(),
        "Role assignment count should match"
    );

    // Same set of roles (by type, ignoring kernel IDs which differ between runs)
    let mut roles1: Vec<Role> = result1
        .provenance
        .role_assignments
        .iter()
        .map(|(_, r)| r.clone())
        .collect();
    let mut roles2: Vec<Role> = result2
        .provenance
        .role_assignments
        .iter()
        .map(|(_, r)| r.clone())
        .collect();
    roles1.sort_by_key(|r| format!("{:?}", r));
    roles2.sort_by_key(|r| format!("{:?}", r));
    assert_eq!(roles1, roles2, "Role sets should match");
}
