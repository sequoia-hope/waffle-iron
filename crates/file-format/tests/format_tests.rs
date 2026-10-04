use feature_engine::types::{
    BooleanOp, BooleanParams, ChamferParams, ExtrudeParams, Feature, FeatureTree, FilletParams,
    Operation, RevolveParams, ShellParams,
};
use file_format::errors::ExportError;
use file_format::WaffleDocument;
use file_format::{
    export_step, load_document, load_project, save_document, save_project, LoadError, PreviewMesh,
    ProjectMetadata, Tab, TabKind, FORMAT_VERSION,
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
        scope: None,
    };

    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane: plane_ref,
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
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
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
        projected: vec![],
        plane_face: None,
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
                combine: None,
                targets: None,
                sketch_id,
                profile_index: 0,
                profile_entity_ids: None,
                depth: 50.0,
                direction: None,
                symmetric: false,
                cut: false,
                merge: true,
                target_body: None,
                depth_mode: feature_engine::types::DepthMode::Blind,
                second_direction: None,
                region: None,
                regions: Vec::new(),
                depth_expr: None,
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
            scope: None,
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
    assert_eq!(parsed["document"]["name"], "My Box Part");
    assert!(parsed["document"]["created"].is_string());
    assert!(parsed["document"]["modified"].is_string());
}

#[test]
fn save_includes_features_array() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let features = &parsed["tabs"][0]["kind"]["features"]["features"];
    assert!(features.is_array());
    assert_eq!(features.as_array().unwrap().len(), 2);
}

#[test]
fn save_serializes_operation_type_tags() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let features = parsed["tabs"][0]["kind"]["features"]["features"]
        .as_array()
        .unwrap();

    assert_eq!(features[0]["operation"]["type"], "Sketch");
    assert_eq!(features[1]["operation"]["type"], "Extrude");
}

#[test]
fn save_serializes_geom_refs() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test");
    let json = save_project(&tree, &meta);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let features = parsed["tabs"][0]["kind"]["features"]["features"]
        .as_array()
        .unwrap();

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
    assert_eq!(
        parsed["tabs"][0]["kind"]["features"]["features"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
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
                combine: None,
                targets: None,
                sketch_id,
                profile_index: 0,
                profile_entity_ids: None,
                depth: 25.0,
                direction: Some([0.0, 0.0, 1.0]),
                symmetric: true,
                cut: false,
                merge: true,
                target_body: None,
                depth_mode: feature_engine::types::DepthMode::Blind,
                second_direction: None,
                region: None,
                regions: Vec::new(),
                depth_expr: None,
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
                combine: None,
                targets: None,
                sketch_id,
                profile_index: 0,
                profile_entity_ids: None,
                axis_origin: [0.0, 0.0, 0.0],
                axis_origin_expr: None,
                axis_direction: [0.0, 1.0, 0.0],
                angle: std::f64::consts::PI,
                cut: false,
                merge: false,
                angle_expr: None,
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
        scope: None,
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
    let features = parsed["tabs"][0]["kind"]["features"]["features"]
        .as_array()
        .unwrap();
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
    let features = parsed["tabs"][0]["kind"]["features"]["features"]
        .as_array()
        .unwrap();
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
fn body_names_survive_round_trip() {
    let mut tree = make_simple_tree();
    let eid = tree.features[1].id;
    let body_id = FeatureTree::body_id(eid, &waffle_types::OutputKey::Main);
    tree.set_body_name(&body_id, Some("Housing".to_string()));

    let json = save_project(&tree, &ProjectMetadata::new("Named Bodies"));
    let (loaded_tree, _) = load_project(&json).unwrap();

    assert_eq!(loaded_tree.body_name_override(&body_id), Some("Housing"));
}

#[test]
fn load_old_file_without_body_names() {
    // A document saved before body_names existed has no such field; serde
    // default must fill an empty registry rather than failing to load.
    let tree = make_simple_tree();
    let json = save_project(&tree, &ProjectMetadata::new("Legacy"));

    // Strip any body_names key to emulate a pre-2a file (none is written when
    // empty anyway, but be explicit about the contract).
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    if let Some(features) = value["tabs"][0]["kind"]["features"].as_object_mut() {
        features.remove("body_names");
    }
    let stripped = serde_json::to_string(&value).unwrap();

    let (loaded_tree, _) = load_project(&stripped).unwrap();
    assert!(loaded_tree.body_names.is_empty());
    assert_eq!(loaded_tree.features.len(), 2);
}

#[test]
fn load_old_file_without_combine() {
    // A document saved before the optional-boolean fields (N-mb-1) existed has
    // no `combine`/`targets` on its extrudes. serde default must load them as
    // None (⇒ the legacy cut/merge path) rather than failing to load. This is
    // the additive-field back-compat guarantee that lets us skip a
    // FORMAT_VERSION bump (see specs/optional_booleans_multibody_extrude.md §6).
    let tree = make_simple_tree();
    let json = save_project(&tree, &ProjectMetadata::new("Legacy"));

    // Strip combine/targets from the extrude feature's params to emulate a file
    // written before those fields existed.
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let features = value["tabs"][0]["kind"]["features"]["features"]
        .as_array_mut()
        .expect("features array");
    for feat in features.iter_mut() {
        if let Some(params) = feat["operation"]["params"].as_object_mut() {
            params.remove("combine");
            params.remove("targets");
        }
    }
    let stripped = serde_json::to_string(&value).unwrap();

    let (loaded_tree, _) = load_project(&stripped).unwrap();
    match &loaded_tree.features[1].operation {
        Operation::Extrude { params } => {
            assert!(params.combine.is_none(), "combine must default to None");
            assert!(params.targets.is_none(), "targets must default to None");
        }
        other => panic!("Expected Extrude, got {:?}", other),
    }
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
        scope: None,
    };

    let sketch = Sketch {
        id: sketch_feature_id, // Same as the Feature.id
        plane: plane_ref,
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
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
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
        projected: vec![],
        plane_face: None,
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
                combine: None,
                targets: None,
                sketch_id: sketch_feature_id, // Points to Feature.id
                profile_index: 0,
                profile_entity_ids: None,
                depth: 5.0,
                direction: None,
                symmetric: false,
                cut: false,
                merge: true,
                target_body: None,
                depth_mode: feature_engine::types::DepthMode::Blind,
                second_direction: None,
                region: None,
                regions: Vec::new(),
                depth_expr: None,
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

// ── M6: Full Round-Trip Tests ──────────────────────────────────────────

// ── M5: Migration Tests ─────────────────────────────────────────────

#[test]
fn migrate_same_version_returns_tree_unchanged() {
    let tree = make_simple_tree();
    let original_len = tree.features.len();
    let result = file_format::migrate::migrate(tree, 1, 1);
    let migrated = result.unwrap();
    assert_eq!(migrated.features.len(), original_len);
}

#[test]
fn migrate_v1_to_v2_succeeds() {
    let tree = FeatureTree::new();
    let result = file_format::migrate::migrate(tree, 1, 2);
    assert!(result.is_ok(), "v1→v2 migration should succeed");
}

#[test]
fn migrate_unsupported_version_returns_error() {
    let tree = FeatureTree::new();
    // v3→v4 has no migration path
    let result = file_format::migrate::migrate(tree, 3, 4);
    assert!(result.is_err());
    if let Err(e) = result {
        let msg = e.to_string();
        assert!(msg.contains("migration failed"), "Got: {}", msg);
        assert!(msg.contains("v3"), "Should mention source version: {}", msg);
        assert!(msg.contains("v4"), "Should mention target version: {}", msg);
    }
}

#[test]
fn migrate_zero_to_one_returns_error() {
    let tree = FeatureTree::new();
    let result = file_format::migrate::migrate(tree, 0, 1);
    assert!(result.is_err());
}

#[test]
fn load_triggers_migration_path_for_old_version() {
    // Manually construct a file with version 0 to exercise the migration code path in load.rs.
    // Since FORMAT_VERSION is 1 and version 0 < 1, load_project will call migrate(tree, 0, 1),
    // which should fail because no migration path exists from v0→v1.
    let json = r#"{"format": "waffle-iron", "version": 0, "project": {"name": "old", "created": "2025-01-01T00:00:00Z", "modified": "2025-01-01T00:00:00Z"}, "features": {"features": [], "active_index": null}}"#;
    let result = load_project(json);
    assert!(
        result.is_err(),
        "Loading version 0 should trigger migration and fail"
    );
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("migration"),
        "Error should be a migration error, got: {}",
        msg
    );
}

// ── Sprint 17B: Multi-Feature & Constraint Roundtrip Tests ──────────────

/// Verify constraints survive serialization roundtrip with diverse constraint types.
#[test]
fn round_trip_preserves_all_constraint_types() {
    let sketch_id = Uuid::new_v4();

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
        scope: None,
    };

    let constraints = vec![
        SketchConstraint::Horizontal { entity: 5 },
        SketchConstraint::Vertical { entity: 6 },
        SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 42.5,
            expression: None,
            reference: false,
        },
        SketchConstraint::Parallel {
            line_a: 5,
            line_b: 7,
        },
        SketchConstraint::Perpendicular {
            line_a: 5,
            line_b: 6,
        },
        SketchConstraint::Equal {
            entity_a: 5,
            entity_b: 7,
        },
    ];

    let sketch = Sketch {
        id: sketch_id,
        plane: plane_ref,
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities: vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 10.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 10.0,
                y: 10.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 10.0,
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
        constraints: constraints.clone(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: {
            let mut m = std::collections::HashMap::new();
            m.insert(1, (0.0, 0.0));
            m.insert(2, (10.0, 0.0));
            m.insert(3, (10.0, 10.0));
            m.insert(4, (0.0, 10.0));
            m
        },
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
        projected: vec![],
        plane_face: None,
    };

    let feature = Feature {
        id: sketch_id,
        name: "Constrained Sketch".to_string(),
        operation: Operation::Sketch { sketch },
        suppressed: false,
        references: Vec::new(),
    };

    let mut tree = FeatureTree::new();
    tree.features.push(feature);

    // Save → load roundtrip
    let meta = ProjectMetadata::new("Constraint Roundtrip");
    let json = save_project(&tree, &meta);
    let (loaded_tree, _) = load_project(&json).unwrap();

    // Extract constraints from loaded sketch
    let loaded_constraints = match &loaded_tree.features[0].operation {
        Operation::Sketch { sketch } => &sketch.constraints,
        other => panic!("Expected Sketch, got {:?}", other),
    };

    assert_eq!(
        loaded_constraints.len(),
        constraints.len(),
        "Should preserve all {} constraints",
        constraints.len()
    );

    // Verify each constraint type survived
    assert!(
        matches!(
            loaded_constraints[0],
            SketchConstraint::Horizontal { entity: 5 }
        ),
        "Horizontal constraint should roundtrip"
    );
    assert!(
        matches!(
            loaded_constraints[1],
            SketchConstraint::Vertical { entity: 6 }
        ),
        "Vertical constraint should roundtrip"
    );
    match &loaded_constraints[2] {
        SketchConstraint::Distance {
            entity_a,
            entity_b,
            value,
            ..
        } => {
            assert_eq!(*entity_a, 1);
            assert_eq!(*entity_b, 2);
            assert!(
                (value - 42.5).abs() < 1e-10,
                "Distance value should be 42.5"
            );
        }
        other => panic!("Expected Distance constraint, got {:?}", other),
    }
    assert!(
        matches!(
            loaded_constraints[3],
            SketchConstraint::Parallel {
                line_a: 5,
                line_b: 7
            }
        ),
        "Parallel constraint should roundtrip"
    );
    assert!(
        matches!(
            loaded_constraints[4],
            SketchConstraint::Perpendicular {
                line_a: 5,
                line_b: 6
            }
        ),
        "Perpendicular constraint should roundtrip"
    );
    assert!(
        matches!(
            loaded_constraints[5],
            SketchConstraint::Equal {
                entity_a: 5,
                entity_b: 7
            }
        ),
        "Equal constraint should roundtrip"
    );
}

// ── V3 Document Model Tests ──────────────────────────────────────────

#[test]
fn v3_round_trip_single_tab() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("V3 Test");
    let json = save_project(&tree, &meta);

    // Verify it's v3 format
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["version"], FORMAT_VERSION);
    assert!(parsed.get("document").is_some());
    assert!(parsed.get("tabs").is_some());

    // Load back
    let (loaded_tree, loaded_meta) = load_project(&json).unwrap();
    assert_eq!(loaded_tree.features.len(), tree.features.len());
    assert_eq!(loaded_meta.name, "V3 Test");
}

#[test]
fn v3_save_document_two_tabs() {
    let tree1 = make_simple_tree();
    let tree2 = FeatureTree::new();
    let mut doc = WaffleDocument::new("Two Tabs");
    doc.document.display_unit = Some("mm".to_string());
    doc.tabs = vec![
        Tab::part("Part 1", tree1.clone()),
        Tab::part("Part 2", tree2),
    ];
    let tab1_id = doc.tabs[0].id.clone();
    doc.active_tab = tab1_id.clone();

    let json = save_document(&doc);
    let loaded = load_document(&json).unwrap().document;

    assert_eq!(loaded.document.name, "Two Tabs");
    assert_eq!(loaded.document.id, doc.document.id, "document id is stable");
    assert_eq!(loaded.tabs.len(), 2);
    assert_eq!(loaded.active_tab, tab1_id);
    assert_eq!(loaded.tabs[0].name, "Part 1");
    assert_eq!(loaded.tabs[1].name, "Part 2");

    // Verify first tab has features
    assert_eq!(loaded.tabs[0].features().unwrap().features.len(), 2);
}

#[test]
fn v2_to_v3_migration_via_load_document() {
    // Create a v2 file manually (flat format with "project" and "features")
    let tree = make_simple_tree();
    let v2_json = serde_json::json!({
        "format": "waffle-iron",
        "version": 2,
        "project": {
            "name": "V2 File",
            "created": "2025-01-01T00:00:00Z",
            "modified": "2025-01-01T00:00:00Z",
            "display_unit": "mm"
        },
        "features": serde_json::to_value(&tree).unwrap()
    });
    let json = serde_json::to_string(&v2_json).unwrap();

    let doc = load_document(&json).unwrap().document;
    assert_eq!(doc.document.name, "V2 File");
    assert_eq!(doc.tabs.len(), 1);
    assert_eq!(doc.tabs[0].name, "Part 1");
    assert_eq!(doc.tabs[0].features().unwrap().features.len(), 2);
    assert!(
        Uuid::parse_str(&doc.tabs[0].id).is_ok(),
        "wrapped tab gets a UUID id"
    );
    assert_eq!(doc.active_tab, doc.tabs[0].id);
}

#[test]
fn v1_to_v3_chain_via_load_document() {
    // v1 file with mm-scale coordinates
    let v1_json = serde_json::json!({
        "format": "waffle-iron",
        "version": 1,
        "project": {
            "name": "V1 File",
            "created": "2025-01-01T00:00:00Z",
            "modified": "2025-01-01T00:00:00Z"
        },
        "features": {
            "features": [],
            "active_index": null
        }
    });
    let json = serde_json::to_string(&v1_json).unwrap();

    let doc = load_document(&json).unwrap().document;
    assert_eq!(doc.document.name, "V1 File");
    assert_eq!(doc.tabs.len(), 1);
}

#[test]
fn v3_active_tab_validity() {
    let doc = WaffleDocument::new("Test");

    // Save with valid active_tab
    let json = save_document(&doc);
    let result = load_document(&json);
    assert!(result.is_ok());

    // Construct truly invalid JSON: active_tab names no tab
    let mut parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    parsed["active_tab"] = serde_json::Value::String(Uuid::new_v4().to_string());
    let bad_json2 = serde_json::to_string(&parsed).unwrap();
    let result2 = load_document(&bad_json2);
    assert!(
        result2.is_err(),
        "Invalid active_tab should produce an error"
    );
}

#[test]
fn v3_preview_mesh_serde() {
    let mut doc = WaffleDocument::new("Mesh Test");
    let mesh = PreviewMesh {
        vertices: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 2],
    };
    doc.tabs[0].kind = TabKind::Part {
        features: FeatureTree::new(),
        preview_mesh: Some(mesh),
    };

    let json = save_document(&doc);
    let loaded = load_document(&json).unwrap().document;

    match &loaded.tabs[0].kind {
        TabKind::Part { preview_mesh, .. } => {
            let mesh = preview_mesh.as_ref().expect("should have preview mesh");
            assert_eq!(mesh.vertices.len(), 9);
            assert_eq!(mesh.normals.len(), 9);
            assert_eq!(mesh.indices.len(), 3);
        }
        TabKind::Assembly { .. } | TabKind::Drawing { .. } | TabKind::Unknown(_) => {
            panic!("Part tab must load as Part")
        }
    }
}

#[test]
fn v3_load_project_returns_active_tab_features() {
    let tree1 = make_simple_tree();
    let tree2 = FeatureTree::new();
    let mut doc = WaffleDocument::new("Multi Tab");
    doc.tabs = vec![
        Tab::part("Part 1", tree1.clone()),
        Tab::part("Part 2", tree2),
    ];
    let tab1_id = doc.tabs[0].id.clone();
    let tab2_id = doc.tabs[1].id.clone();

    // Active tab is tab2 (empty tree)
    doc.active_tab = tab2_id;
    let json = save_document(&doc);
    let (loaded_tree, loaded_meta) = load_project(&json).unwrap();
    assert_eq!(loaded_meta.name, "Multi Tab");
    assert_eq!(
        loaded_tree.features.len(),
        0,
        "Should return active tab's (empty) features"
    );

    // Active tab is tab1 (2 features)
    doc.active_tab = tab1_id;
    let json = save_document(&doc);
    let (loaded_tree, _) = load_project(&json).unwrap();
    assert_eq!(
        loaded_tree.features.len(),
        2,
        "Should return active tab's features"
    );
}

/// Regression: the UI historically created the implicit first tab with the
/// literal id `"default"` (not a UUID). Documents saved from such a session
/// must still load — tab ids are opaque keys, not UUIDs. Previously this
/// failed with "UUID parsing failed: ... found `u` at 5".
#[test]
fn v3_non_uuid_tab_id_loads() {
    let json = r#"{
        "format": "waffle-iron",
        "version": 3,
        "document": {
            "name": "Legacy Default Tab",
            "created": "2026-01-01T00:00:00Z",
            "modified": "2026-01-01T00:00:00Z"
        },
        "tabs": [{
            "id": "default",
            "name": "Part 1",
            "kind": { "type": "Part", "features": { "features": [], "active_index": null } }
        }],
        "active_tab": "default"
    }"#;

    // load_project (used by the engine on file open) must not choke on it.
    let (tree, meta) = load_project(json).expect("non-uuid tab id should load");
    assert_eq!(meta.name, "Legacy Default Tab");
    assert_eq!(tree.features.len(), 0);

    // load_document migrates it to v4: the legacy id becomes a UUID, the
    // active tab follows, and the rewrite is reported (v4 spec §3).
    let loaded = load_document(json).expect("non-uuid tab id should load");
    let doc = loaded.document;
    assert_eq!(doc.document.name, "Legacy Default Tab");
    assert_eq!(doc.tabs.len(), 1);
    assert!(
        Uuid::parse_str(&doc.tabs[0].id).is_ok(),
        "legacy id rewritten to a UUID, got {}",
        doc.tabs[0].id
    );
    assert_eq!(doc.active_tab, doc.tabs[0].id);
    assert!(
        loaded.warnings.iter().any(|w| w.contains("`default`")),
        "rewrite is reported: {:?}",
        loaded.warnings
    );
}

/// Spec point_pair_horizontal_vertical.md I4: the new point-pair Horizontal /
/// Vertical variants survive a save → load round-trip, and an existing line
/// `Horizontal { entity }` still loads alongside them.
#[test]
fn point_pair_hv_constraints_roundtrip() {
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
        scope: None,
    };
    let constraints = vec![
        SketchConstraint::Horizontal { entity: 5 },
        SketchConstraint::HorizontalPoints {
            point_a: 1,
            point_b: 3,
        },
        SketchConstraint::VerticalPoints {
            point_a: 2,
            point_b: 4,
        },
    ];
    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane: plane_ref,
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities: vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 10.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 10.0,
                y: 10.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 10.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 5,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
        ],
        constraints: constraints.clone(),
        solve_status: SolveStatus::UnderConstrained { dof: 1 },
        solved_positions: std::collections::HashMap::new(),
        solved_profiles: vec![],
        projected: vec![],
        plane_face: None,
    };
    let feature = Feature {
        id: sketch.id,
        name: "PointPair HV".to_string(),
        operation: Operation::Sketch { sketch },
        suppressed: false,
        references: Vec::new(),
    };
    let mut tree = FeatureTree::new();
    tree.features.push(feature);

    let meta = ProjectMetadata::new("PointPair Roundtrip");
    let json = save_project(&tree, &meta);
    let (loaded_tree, _) = load_project(&json).unwrap();

    let loaded = match &loaded_tree.features[0].operation {
        Operation::Sketch { sketch } => &sketch.constraints,
        other => panic!("Expected Sketch, got {:?}", other),
    };
    assert_eq!(loaded.len(), 3, "all three constraints preserved");
    assert!(matches!(
        loaded[0],
        SketchConstraint::Horizontal { entity: 5 }
    ));
    assert!(
        matches!(
            loaded[1],
            SketchConstraint::HorizontalPoints {
                point_a: 1,
                point_b: 3
            }
        ),
        "HorizontalPoints should roundtrip, got {:?}",
        loaded[1]
    );
    assert!(
        matches!(
            loaded[2],
            SketchConstraint::VerticalPoints {
                point_a: 2,
                point_b: 4
            }
        ),
        "VerticalPoints should roundtrip, got {:?}",
        loaded[2]
    );
}

// ── STEP export through a kernel WITHOUT it ────────────────────────────
//
// kernel-v2 implements STEP export (2026-09-08: `kernel_v2::step_export`,
// analytic AP214; the semantic oracle is the truck round trip in
// `wasm-bridge/tests/step_export_roundtrip.rs`). `MockKernel` deliberately
// keeps the trait default, so this pins how the file-format layer surfaces a
// kernel's missing capability: the trait's NotSupported must reach the
// caller as `StepExportFailed` naming it — never as `NoSolid` (which would
// mean the rebuild produced no body, an unrelated failure).

#[test]
fn step_export_through_a_kernel_without_it_names_the_missing_capability() {
    use waffle_types::kernel::MockKernel;

    let tree = make_rebuild_compatible_tree();
    let mut kernel = MockKernel::new();

    let result = export_step(&tree, &mut kernel);

    match result {
        Err(ExportError::StepExportFailed(msg)) => {
            assert!(
                msg.contains("not supported"),
                "the failure must name the missing capability, got: {msg}"
            );
        }
        // NoSolid would mean the rebuild never produced a body — the export
        // would then be failing for an unrelated reason and this test would be
        // passing by accident. Discriminating the two is the whole point.
        Err(ExportError::NoSolid) => {
            panic!("rebuild produced no solid; the fixture no longer reaches the export path")
        }
        Err(other) => panic!("unexpected export error: {other:?}"),
        Ok(_) => panic!(
            "MockKernel's trait-default export_step unexpectedly SUCCEEDED — this \
             test pins the NotSupported surfacing path, not kernel-v2's export"
        ),
    }
}

// ── min_reader_version + verified save (2026-08-28 seam fixes) ───────────
// See docs/FILE_FORMAT.md §13 (forward-compat) and §14.10 (NaN poisoning).

#[test]
fn save_writes_min_reader_version() {
    use file_format::MIN_READER_VERSION;

    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Test Project");
    let parsed: serde_json::Value = serde_json::from_str(&save_project(&tree, &meta)).unwrap();
    assert_eq!(parsed["min_reader_version"], MIN_READER_VERSION);

    let mut doc = WaffleDocument::new("Doc");
    doc.tabs[0].features_mut().unwrap().features = make_simple_tree().features;
    let parsed: serde_json::Value = serde_json::from_str(&save_document(&doc)).unwrap();
    assert_eq!(parsed["min_reader_version"], MIN_READER_VERSION);
}

#[test]
fn load_refuses_file_requiring_newer_reader() {
    // version stays 3 but min_reader_version demands a newer build: both entry
    // points must refuse with FutureVersion (clean), NOT a serde ParseError.
    let json = r#"{
        "format": "waffle-iron",
        "version": 3,
        "min_reader_version": 99,
        "document": { "name": "n", "created": "2026-01-01T00:00:00Z", "modified": "2026-01-01T00:00:00Z" },
        "tabs": [],
        "active_tab": "x"
    }"#;
    for result in [load_project(json).err(), load_document(json).err()] {
        match result {
            Some(LoadError::FutureVersion {
                file_version,
                supported_version,
            }) => {
                assert_eq!(file_version, 99);
                assert_eq!(supported_version, FORMAT_VERSION);
            }
            other => panic!("expected FutureVersion, got {other:?}"),
        }
    }
}

// ── v7: entity names (N1, `specs/agent_mechanical_design.md` §5.2) ───────

/// One named face, stored the way `names::mint` stores one: a `Selector::Pid`
/// target with the authored reference kept as the fallback.
fn named_face(feature_id: Uuid) -> feature_engine::names::NamedRef {
    let authored = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    };
    feature_engine::names::NamedRef {
        target: GeomRef {
            selector: Selector::Pid {
                pid: 0x1234_5678_9abc_def0,
                root_pid: 7,
            },
            ..authored.clone()
        },
        kind: TopoKind::Face,
        fallback: Some(authored),
        created: feature_engine::types::Provenance {
            origin: feature_engine::types::ProvenanceOrigin::Agent {
                name: "n1-test".to_string(),
            },
            at: None,
        },
    }
}

#[test]
fn a_named_entity_round_trips_through_the_document() {
    let mut tree = make_simple_tree();
    let feature_id = tree.features[0].id;
    tree.set_name("plate.top_face", named_face(feature_id));

    let meta = ProjectMetadata::new("Named");
    let json = save_project(&tree, &meta);
    // The pid is written as a tagged selector, exactly as §8 documents it.
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let entry = &parsed["tabs"][0]["kind"]["features"]["names"]["plate.top_face"];
    assert_eq!(entry["target"]["selector"]["type"], "Pid");
    // Decimal STRINGS since v9 (`waffle_types::pid_str`): the id is a
    // content-seeded u64 and a JSON number in JavaScript is an f64.
    assert_eq!(
        entry["target"]["selector"]["root_pid"],
        serde_json::json!("7")
    );
    assert_eq!(
        entry["target"]["selector"]["pid"],
        serde_json::json!("1311768467463790320")
    );
    assert_eq!(entry["fallback"]["selector"]["type"], "Role");
    assert_eq!(entry["created"]["origin"]["name"], "n1-test");

    let (back, _) = load_project(&json).expect("a v7 document loads");
    let stored = back
        .named_ref("plate.top_face")
        .expect("the name came back");
    assert_eq!(stored.kind, TopoKind::Face);
    match stored.target.selector {
        Selector::Pid { pid, root_pid } => {
            // The full 64 bits survive the JSON round-trip — a pid near
            // 2^64 must not come back through an f64.
            assert_eq!(pid, 0x1234_5678_9abc_def0);
            assert_eq!(root_pid, 7);
        }
        ref other => panic!("want a Pid selector, got {other:?}"),
    }
    assert_eq!(stored.target.policy, ResolvePolicy::Strict);
    assert!(stored.fallback.is_some(), "the authored reference survives");
}

/// The reader floor moved WITH the field: a document carrying a name demands
/// at least reader 7, and a v6 build (which refuses `min_reader_version > 6`)
/// is told so cleanly instead of failing on an unknown `Selector` variant.
/// The floor has moved on since (v8, `DesignParameter.unit`), so what this
/// pins is the v6 refusal, not the exact number.
#[test]
fn a_document_with_names_demands_the_v7_reader() {
    let mut tree = make_simple_tree();
    let feature_id = tree.features[0].id;
    tree.set_name("top_face", named_face(feature_id));
    let parsed: serde_json::Value =
        serde_json::from_str(&save_project(&tree, &ProjectMetadata::new("Named"))).unwrap();
    assert_eq!(
        parsed["min_reader_version"],
        file_format::MIN_READER_VERSION
    );
    assert!(
        parsed["min_reader_version"].as_u64().unwrap() > 6,
        "a v6 reader must be refused, not handed a Pid selector it cannot parse"
    );
}

/// A document with no names is byte-identical to a pre-N1 one: the key is
/// omitted, so the whole corpus keeps loading and re-saving unchanged.
#[test]
fn a_document_without_names_writes_no_names_key() {
    let tree = make_simple_tree();
    let parsed: serde_json::Value =
        serde_json::from_str(&save_project(&tree, &ProjectMetadata::new("Plain"))).unwrap();
    assert!(
        parsed["tabs"][0]["kind"]["features"].get("names").is_none(),
        "an empty name table is not written"
    );
}

/// A pre-v7 file has no `names` key at all; it must load with an empty table
/// rather than be refused.
#[test]
fn a_pre_v7_document_loads_with_no_names() {
    let tree = make_simple_tree();
    let mut value: serde_json::Value =
        serde_json::from_str(&save_project(&tree, &ProjectMetadata::new("Old"))).unwrap();
    value["version"] = serde_json::json!(6);
    value["min_reader_version"] = serde_json::json!(6);
    let (loaded, _) =
        load_project(&serde_json::to_string(&value).unwrap()).expect("a v6 file loads");
    assert!(loaded.names.is_empty());
}

#[test]
fn load_accepts_files_without_min_reader_version() {
    // Every pre-2026-08-28 file (including the whole assay corpus) lacks the
    // field; absence must mean "no requirement".
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Legacy");
    let mut value: serde_json::Value = serde_json::from_str(&save_project(&tree, &meta)).unwrap();
    value.as_object_mut().unwrap().remove("min_reader_version");
    let json = serde_json::to_string(&value).unwrap();
    load_project(&json).expect("file without min_reader_version must load");
    load_document(&json).expect("file without min_reader_version must load");
}

#[test]
fn verified_save_round_trips_a_healthy_tree() {
    use file_format::save_project_verified;

    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Healthy");
    let json = save_project_verified(&tree, &meta).expect("healthy tree must save");
    let (loaded, _) = load_project(&json).unwrap();
    assert_eq!(loaded.features.len(), tree.features.len());
}

#[test]
fn verified_save_refuses_non_finite_floats() {
    use file_format::save_project_verified;

    // serde_json writes NaN as `null`; the raw save "succeeds" and produces a
    // file no loader will ever accept again. The verified save must catch it.
    let mut tree = make_simple_tree();
    if let Operation::Sketch { sketch } = &mut tree.features[0].operation {
        if let Some(SketchEntity::Point { x, .. }) = sketch.entities.first_mut() {
            *x = f64::NAN;
        } else {
            panic!("fixture changed: expected first entity to be a Point");
        }
    } else {
        panic!("fixture changed: expected first feature to be a Sketch");
    }

    let meta = ProjectMetadata::new("Poisoned");
    let err = save_project_verified(&tree, &meta)
        .expect_err("a NaN in the tree must be a loud save-time error");
    assert!(
        matches!(err, LoadError::ParseError(_)),
        "expected the self-check to surface the parse failure, got {err:?}"
    );
}

// ── Design parameters (variables) persistence ────────────────────────────

#[test]
fn parameters_and_dimension_expressions_round_trip() {
    use feature_engine::types::DesignParameter;

    let mut tree = make_simple_tree();
    let mut width = DesignParameter::new("width", "30");
    width.value = 30.0;
    tree.parameters = vec![width, DesignParameter::new("half", "width / 2")];

    // Attach an expression-driven + reference dimension to the sketch.
    if let feature_engine::types::Operation::Sketch { sketch } = &mut tree.features[0].operation {
        sketch.constraints.push(SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 0.030,
            expression: Some("width".to_string()),
            reference: false,
        });
        sketch.constraints.push(SketchConstraint::Radius {
            entity: 3,
            value: 0.005,
            expression: None,
            reference: true,
        });
    } else {
        panic!("first feature should be the sketch");
    }

    let meta = ProjectMetadata::new("Params Project");
    let json = save_project(&tree, &meta);
    let (loaded, _) = load_project(&json).unwrap();

    assert_eq!(loaded.parameters.len(), 2);
    assert_eq!(loaded.parameters[0].name, "width");
    assert_eq!(loaded.parameters[0].expression, "30");
    assert_eq!(loaded.parameters[0].value, 30.0);
    assert_eq!(loaded.parameters[1].expression, "width / 2");

    if let feature_engine::types::Operation::Sketch { sketch } = &loaded.features[0].operation {
        let n = sketch.constraints.len();
        assert_eq!(sketch.constraints[n - 2].expression(), Some("width"));
        assert!(!sketch.constraints[n - 2].is_reference());
        assert_eq!(sketch.constraints[n - 1].expression(), None);
        assert!(sketch.constraints[n - 1].is_reference());
    } else {
        panic!("sketch feature lost on round trip");
    }
}

#[test]
fn old_file_without_parameters_loads_with_empty_table() {
    let tree = make_simple_tree();
    let meta = ProjectMetadata::new("Old Project");
    let mut json: serde_json::Value = serde_json::from_str(&save_project(&tree, &meta)).unwrap();
    // Simulate a pre-parameters file: strip the field wherever it appears.
    for tab in json["tabs"].as_array_mut().unwrap() {
        if let Some(obj) = tab["features"].as_object_mut() {
            obj.remove("parameters");
        }
    }
    let (loaded, _) = load_project(&serde_json::to_string(&json).unwrap()).unwrap();
    assert!(loaded.parameters.is_empty());
}

// --- 3D sketch (`specs/sketch3d.md` S2) ------------------------------------

/// A 3D sketch survives a save/load with every field intact — including the
/// optional ones, which is what a path built against model geometry is made
/// of.
#[test]
fn a_3d_sketch_round_trips() {
    use waffle_types::sketch3d::{Attachment, Axis, Sketch3d, Sketch3dEntity};

    let sketch = Sketch3d::new(
        Uuid::new_v4(),
        vec![
            Sketch3dEntity::Point {
                id: 1,
                xyz: [0.1, 0.2, 0.3],
                attach: None,
                xyz_expr: Some([Some("width / 2".into()), None, None]),
                construction: true,
            },
            Sketch3dEntity::Point {
                id: 2,
                xyz: [1.0, 0.0, 0.0],
                attach: Some(Box::new(Attachment::AlongAxis {
                    from: 1,
                    axis: Axis::Z,
                    distance: 2.5,
                })),
                xyz_expr: None,
                construction: false,
            },
            Sketch3dEntity::Point {
                id: 3,
                xyz: [1.0, 1.0, 0.0],
                attach: None,
                xyz_expr: None,
                construction: false,
            },
            Sketch3dEntity::Line {
                id: 4,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            Sketch3dEntity::Arc {
                id: 5,
                start_id: 2,
                end_id: 3,
                via_id: 1,
                construction: false,
            },
            Sketch3dEntity::Fillet {
                id: 6,
                at_point_id: 2,
                radius: 0.05,
                radius_expr: Some("bend".into()),
            },
        ],
    );

    let mut tree = FeatureTree::new();
    tree.features.push(Feature {
        id: Uuid::new_v4(),
        name: "Path".to_string(),
        operation: Operation::Sketch3d {
            sketch: sketch.clone(),
        },
        suppressed: false,
        references: Default::default(),
    });
    tree.active_index = Some(1);

    let json = save_project(&tree, &ProjectMetadata::new("Test"));
    let (loaded, _) = load_project(&json).unwrap();

    let Operation::Sketch3d { sketch: back } = &loaded.features[0].operation else {
        panic!(
            "expected a Sketch3d, got {:?}",
            loaded.features[0].operation
        );
    };
    assert_eq!(back.id, sketch.id);
    assert_eq!(
        serde_json::to_value(&back.entities).unwrap(),
        serde_json::to_value(&sketch.entities).unwrap(),
        "every entity survives, optional fields included"
    );
}

/// Adding an operation kind is NOT a reader-floor bump: since v4 Phase 1b an
/// unknown `type` tag round-trips through `Operation::Unknown`, so an older
/// build keeps a document containing a 3D sketch intact and refuses only that
/// one feature's rebuild.
///
/// The numbers below are the floor as it stands, not what the 3D sketch set:
/// v7 is N1's `Selector::Pid` (`specs/agent_mechanical_design.md` §5.2,
/// 2026-10-03), a new SELECTOR variant, which §13.3 does make a bump; v8 is
/// P1's `DesignParameter.unit`, a field an old reader must not silently
/// ignore (`crates/feature-engine/tests/param_unit_floor.rs` measures why);
/// v9 is N2's `Sketch.plane_face` (§5.3 item 3), the same shape of reason —
/// a reader that drops it builds a sketch into space where this one refuses
/// (`crates/feature-engine/tests/sketch_plane_face.rs` measures it); v10 is
/// the pid REPRESENTATION flip — a `Selector::Pid`'s ids are decimal strings,
/// because a JSON number in JavaScript is an `f64` and a rounded id is a
/// different entity (`waffle_types::pid_str`); v11 is D4b's
/// `Projection::Section`/`Detail` (`specs/drawings_and_mbd.md` §8), two new
/// variants INSIDE a tab kind every reader since D4a knows — so unlike D4a's
/// new tab kind they are deserialized rather than kept opaque, and an older
/// reader fails on them
/// (`a_projection_variant_an_older_reader_does_not_know_fails_the_whole_document`);
/// v12 is P2's two parameter scopes — `DocumentMetadata.parameters` and an
/// applied `Instance.parameter_overrides` — where a reader that drops either
/// builds a different solid from the same file
/// (`crates/file-format/tests/param_scope_floor.rs`); v14 is M1's
/// `Annotation::FeatureControlFrame` — the v11 shape of the problem, a new
/// variant inside the `Drawing` tab — together with `Dimension::Mass` and
/// `Dimension::Density`, two more variants of the enum a parameter's `unit`
/// is written as
/// (`m1s_annotation_variant_and_unit_variant_each_fail_a_pre_m1_reader`).
/// What this test holds is that the writer and the floor move together and
/// only deliberately.
#[test]
fn the_3d_sketch_operation_did_not_move_the_format_floor() {
    assert_eq!(file_format::FORMAT_VERSION, 14);
    assert_eq!(file_format::MIN_READER_VERSION, 14);
}

/// v10: a pre-v10 file wrote its `Selector::Pid` ids as JSON NUMBERS, and it
/// still loads — which is what makes the flip to strings free of a migration.
///
/// This is the half of the rule that is easy to lose. Writing strings is one
/// `#[serde(with)]`; ACCEPTING both forms is a deliberate deserializer
/// (`waffle_types::pid_str`), and without it every `.waffle` and every assay
/// case written before today would fail to parse. The id here is above
/// `2^53`, so it is also the case a reader cannot fake with an `f64`.
#[test]
fn a_pre_v10_numeric_pid_still_loads() {
    let mut tree = make_simple_tree();
    let feature_id = tree.features[0].id;
    tree.set_name("plate.top_face", named_face(feature_id));
    let meta = ProjectMetadata::new("Numeric");

    // Rewrite the file the way a v7–v9 writer wrote it: bare numbers, and
    // the floor it claimed at the time.
    let mut parsed: serde_json::Value =
        serde_json::from_str(&save_project(&tree, &meta)).expect("the v10 file parses");
    let sel =
        &mut parsed["tabs"][0]["kind"]["features"]["names"]["plate.top_face"]["target"]["selector"];
    assert_eq!(sel["pid"], serde_json::json!("1311768467463790320"));
    sel["pid"] = serde_json::json!(1_311_768_467_463_790_320_u64);
    sel["root_pid"] = serde_json::json!(7);
    parsed["version"] = serde_json::json!(9);
    parsed["min_reader_version"] = serde_json::json!(9);

    let (back, _) = load_project(&parsed.to_string()).expect("a numeric-pid file loads");
    match back
        .named_ref("plate.top_face")
        .expect("the name came back")
        .target
        .selector
    {
        Selector::Pid { pid, root_pid } => {
            assert_eq!(pid, 0x1234_5678_9abc_def0);
            assert_eq!(root_pid, 7);
        }
        ref other => panic!("want a Pid selector, got {other:?}"),
    }
}

/// Neither did the `Drawing` tab kind (D4a, `specs/drawings_and_mbd.md` §8),
/// and this is the one place that records why that is a DECISION.
///
/// §13.3, and `MIN_READER_VERSION`'s own doc comment: since v4 a new tab kind
/// needs no bump, because a reader that does not know the tag keeps the whole
/// tab as `TabKind::Unknown` and re-emits it verbatim. The tempting argument
/// for a bump is that a drawing's annotation anchors persist a
/// `Selector::Pid`, and a new selector variant IS a floor bump — that is what
/// made v7. The difference is WHERE the variant sits: v7's was inside
/// `FeatureTree.names`, a defaulted field of a kind every reader knows, so an
/// old reader deserialized it and failed on the unknown variant. Inside an
/// unknown tab kind nothing is deserialized at all.
///
/// And bumping anyway would be actively worse than doing nothing. A reader
/// refuses a file whose `max(version, min_reader_version)` exceeds its own
/// `FORMAT_VERSION`, so a bump would make every older build reject the WHOLE
/// document — losing the part tabs it reads perfectly well — where today it
/// opens the document and keeps the drawing opaque. The forward-compatibility
/// mechanism of §5.3 exists for exactly this case.
/// Deliberately NOT a literal version number: the claim is that a drawing tab
/// does not move the floor, whatever the floor is. The sibling above keeps the
/// literal pin, so a bump still has to be deliberate somewhere — but it should
/// have to be deliberate in ONE place, not in every test that mentions a
/// version.
#[test]
fn a_drawing_tab_did_not_move_the_format_floor() {
    use feature_engine::drawing::Drawing;

    let mut doc = WaffleDocument::new("Drawn");
    let plain: serde_json::Value = serde_json::from_str(&save_document(&doc)).unwrap();
    doc.tabs.push(Tab::drawing("Drawing 1", Drawing::new()));
    let drawn: serde_json::Value = serde_json::from_str(&save_document(&doc)).unwrap();

    // A document WITH a drawing tab claims exactly what one without claims —
    // which is the claim an older reader acts on when it decides whether to
    // open the file at all.
    assert_eq!(drawn["version"], plain["version"]);
    assert_eq!(drawn["min_reader_version"], plain["min_reader_version"]);
    assert_eq!(drawn["version"], file_format::FORMAT_VERSION);
    assert_eq!(drawn["min_reader_version"], file_format::MIN_READER_VERSION);
    // And the tab really is in the file it claims that about.
    assert_eq!(drawn["tabs"][1]["kind"]["type"], "Drawing");
}

// ───────────────────────────────────────────────────────────────── D4b

#[test]
fn a_section_and_a_detail_view_survive_a_save_and_a_load_with_everything_on_them() {
    use feature_engine::drawing::{
        Drawing, DrawingView, NamedView, Projection, TitleBlockField, TitleBlockKey, ViewSource,
    };

    let mut drawing = Drawing::new();
    let sheet = &mut drawing.sheets[0];
    let front = DrawingView::new(
        "Front",
        ViewSource::whole_tab("part-1"),
        Projection::Named {
            view: NamedView::Front,
        },
    );
    let parent = front.id;
    sheet.views.push(front);
    sheet.views.push(DrawingView::new(
        "SECTION A-A",
        ViewSource::whole_tab("part-1"),
        Projection::Section {
            parent,
            from: [-0.002, 0.0025],
            to: [0.042, 0.0025],
            flip: true,
            label: "A".to_string(),
        },
    ));
    let mut detail = DrawingView::new(
        "DETAIL B",
        ViewSource::whole_tab("part-1"),
        Projection::Detail {
            parent,
            center: [0.01, 0.002],
            radius: 0.004,
            label: "B".to_string(),
        },
    );
    detail.scale = 2.0;
    detail.cache_key = Some("d4b-0123456789abcdef".to_string());
    sheet.views.push(detail);
    sheet.title_block.fields.push(TitleBlockField::with_text(
        TitleBlockKey::Material,
        "AISI 304",
    ));
    drawing.projection_angle = feature_engine::drawing::ProjectionAngle::First;

    let mut doc = WaffleDocument::new("Sectioned");
    doc.tabs.push(Tab::drawing("Drawing 1", drawing));
    let json = save_document(&doc);
    let back = load_document(&json).expect("a D4b drawing loads").document;
    let reloaded = save_document(&back);
    assert_eq!(
        reloaded, json,
        "a section, a detail, a title block and a cache key must all survive the round trip \
         byte for byte"
    );

    let drawn = back.tabs[1].drawing_tree().expect("the drawing");
    assert_eq!(
        drawn.projection_angle,
        feature_engine::drawing::ProjectionAngle::First
    );
    let sheet = &drawn.sheets[0];
    assert_eq!(sheet.views.len(), 3);
    match &sheet.views[1].projection {
        Projection::Section {
            from,
            to,
            flip,
            label,
            ..
        } => {
            assert_eq!((*from, *to), ([-0.002, 0.0025], [0.042, 0.0025]));
            assert!(*flip);
            assert_eq!(label, "A");
        }
        other => panic!("expected a section, got {other:?}"),
    }
    match &sheet.views[2].projection {
        Projection::Detail { radius, label, .. } => {
            assert_eq!(*radius, 0.004);
            assert_eq!(label, "B");
        }
        other => panic!("expected a detail, got {other:?}"),
    }
    assert_eq!(
        sheet.views[2].cache_key.as_deref(),
        Some("d4b-0123456789abcdef")
    );
    assert_eq!(sheet.title_block.fields.len(), 7);
    assert!(drawn.validate().is_empty(), "{:?}", drawn.validate());
}

#[test]
fn a_projection_variant_an_older_reader_does_not_know_fails_the_whole_document() {
    // WHY D4b moved the floor where D4a did not — as a measurement, not as an
    // assertion about intent.
    //
    // D4a's reason for not moving it was that a tab KIND a reader does not
    // know is kept opaque: `known_or_unknown` takes the unknown branch,
    // nothing inside the tab is deserialized, and nothing can fail. A
    // `Drawing` tab's own tag IS known to every reader since D4a, so the
    // known branch runs, the drawing IS deserialized, and a `Projection` tag
    // the reader has never heard of fails the WHOLE DOCUMENT.
    //
    // `Projection` has no opaque arm, deliberately: a view whose projection
    // cannot be read is a view that cannot be drawn, re-aimed or deleted
    // sensibly, and keeping it as a blob would put a view on the sheet that
    // nothing can do anything with. So this is what a v10 reader does in
    // front of a v11 section — and the floor bump turns it into a
    // `FutureVersion` refusal that names the remedy.
    //
    // Written with a variant NO build has, so it keeps measuring the
    // mechanism once `Section` and `Detail` are old news.
    let json = r#"{
      "format": "waffle-iron",
      "version": 11,
      "min_reader_version": 11,
      "document": { "id": "00000000-0000-4000-8000-000000000001", "name": "Future",
        "created": "2026-10-03T00:00:00Z", "modified": "2026-10-03T00:00:00Z" },
      "sources": [],
      "tabs": [
        { "id": "t1", "name": "Drawing 1", "kind": { "type": "Drawing", "drawing": {
            "sheets": [ { "id": "00000000-0000-4000-8000-000000000002", "name": "S",
              "views": [ { "id": "00000000-0000-4000-8000-000000000003", "name": "V",
                 "source": { "tab_id": "p" },
                 "projection": { "type": "Perspective", "eye": [1, 2, 3] } } ] } ] } } }
      ],
      "active_tab": "t1"
    }"#;
    let err = load_document(json).expect_err("an unknown projection cannot be read");
    let message = err.to_string();
    assert!(
        message.contains("Perspective")
            || message.contains("projection")
            || message.contains("tab kind"),
        "the refusal should name what it could not read, got: {message}"
    );

    // Where an unknown TAB KIND in the same position is kept whole — the D4a
    // case, still true, which is exactly what makes the two different.
    let opaque = json.replace("\"type\": \"Drawing\"", "\"type\": \"Schematic\"");
    let loaded = load_document(&opaque)
        .expect("an unknown tab kind is preserved, not refused")
        .document;
    let round_tripped: serde_json::Value =
        serde_json::from_str(&save_document(&loaded)).expect("it re-saves");
    assert_eq!(
        round_tripped["tabs"][0]["kind"]["type"], "Schematic",
        "an unknown tab kind is re-emitted verbatim"
    );
}

/// Why M1 moved the floor to v14, measured on both of its wire breaks.
///
/// The ANNOTATION half is the v11 case over again — a new serde-tagged
/// variant inside a tab kind every reader since D4a deserializes — and D3's
/// own notes predicted it: "adding the field later is additive; adding the
/// variant is not". The UNIT half is the v8 case with more force: v8 bumped
/// because a reader must not silently ignore a declared unit, and a reader
/// that has never heard of `"Mass"` cannot read it at all.
///
/// Both are written with the tags a PRE-M1 build would have choked on, which
/// is the only way to measure the mechanism rather than assert the intent.
#[test]
fn m1s_annotation_variant_and_unit_variant_each_fail_a_pre_m1_reader() {
    // 1. The annotation variant. `Annotation` has no opaque arm, for the
    //    reason `Projection` has none: an annotation that cannot be read is
    //    one that cannot be drawn, measured or deleted sensibly, and keeping
    //    it as a blob would put a frame on the sheet nothing can act on.
    //    Written with a variant NO build has, so it keeps measuring the
    //    mechanism once `FeatureControlFrame` is old news.
    let drawing = r#"{
      "format": "waffle-iron",
      "version": 14,
      "min_reader_version": 14,
      "document": { "id": "00000000-0000-4000-8000-000000000001", "name": "Future",
        "created": "2026-10-04T00:00:00Z", "modified": "2026-10-04T00:00:00Z" },
      "sources": [],
      "tabs": [
        { "id": "t1", "name": "Drawing 1", "kind": { "type": "Drawing", "drawing": {
            "sheets": [ { "id": "00000000-0000-4000-8000-000000000002", "name": "S",
              "views": [ { "id": "00000000-0000-4000-8000-000000000003", "name": "V",
                 "source": { "tab_id": "p" },
                 "projection": { "type": "Named", "view": { "type": "Top" } },
                 "annotations": [ { "type": "SurfaceFinish", "ra_um": 1.6 } ] } ] } ] } } }
      ],
      "active_tab": "t1"
    }"#;
    let err = load_document(drawing).expect_err("an unknown annotation cannot be read");
    let message = err.to_string();
    assert!(
        message.contains("SurfaceFinish")
            || message.contains("annotation")
            || message.contains("variant"),
        "the refusal should name what it could not read, got: {message}"
    );

    // 2. The unit variant. `DesignParameter.unit` is `Option<Dimension>` — a
    //    string value, but a serde ENUM, so an unknown one is a hard parse
    //    error rather than a dropped key. Measured with a dimension no build
    //    has, for the same reason as above.
    let params = r#"{
      "format": "waffle-iron",
      "version": 14,
      "min_reader_version": 14,
      "document": { "id": "00000000-0000-4000-8000-000000000001", "name": "Future",
        "created": "2026-10-04T00:00:00Z", "modified": "2026-10-04T00:00:00Z" },
      "sources": [],
      "tabs": [
        { "id": "t1", "name": "Part 1", "kind": { "type": "Part", "features": {
            "features": [], "active_index": null,
            "parameters": [ { "id": "00000000-0000-4000-8000-00000000000a",
              "name": "glow", "expression": "4", "unit": "Luminance" } ] } } }
      ],
      "active_tab": "t1"
    }"#;
    let err = load_document(params).expect_err("an unknown dimension cannot be read");
    let message = err.to_string();
    assert!(
        message.contains("Luminance") || message.contains("variant"),
        "the refusal should name what it could not read, got: {message}"
    );

    // And the halves that are genuinely ADDITIVE read as absent rather than
    // failing, which is why they would not have moved the floor alone: a
    // document with a tolerance, a dual precision and a material table reads
    // on a build that has them, and the same document WITHOUT them reads too.
    let additive = r#"{
      "format": "waffle-iron",
      "version": 14,
      "min_reader_version": 14,
      "document": { "id": "00000000-0000-4000-8000-000000000001", "name": "M1",
        "created": "2026-10-04T00:00:00Z", "modified": "2026-10-04T00:00:00Z",
        "precision": 3, "dual_unit": "in", "dual_precision": 4 },
      "sources": [],
      "tabs": [
        { "id": "t1", "name": "Part 1", "kind": { "type": "Part", "features": {
            "features": [], "active_index": null,
            "materials": [ { "name": "Aluminium", "density_kg_m3": 2700.0 } ],
            "body_materials": { "f/main": "Aluminium" } } } }
      ],
      "active_tab": "t1"
    }"#;
    let loaded = load_document(additive)
        .expect("the additive half reads")
        .document;
    assert_eq!(loaded.document.precision, Some(3));
    assert_eq!(loaded.document.dual_unit.as_deref(), Some("in"));
    assert_eq!(loaded.document.dual_precision, Some(4));
    let file_format::metadata::TabKind::Part { features, .. } = &loaded.tabs[0].kind else {
        panic!("expected a part tab");
    };
    assert_eq!(features.density_of_body("f/main"), Ok(Some(2700.0)));
    // Re-saved, it carries both tables and all three settings.
    let again: serde_json::Value =
        serde_json::from_str(&save_document(&loaded)).expect("it re-saves");
    assert_eq!(again["document"]["precision"], 3);
    assert_eq!(
        again["tabs"][0]["kind"]["features"]["materials"][0]["density_kg_m3"],
        2700.0
    );
}

/// P3's ten `*_expr` sidecars survive a save and a load — the leg §13.3's
/// no-bump argument stands on.
///
/// `param_p3_fields.rs` pins the ABSENT case (a document using none of them
/// writes the bytes it always wrote) and the rename. Neither says a sidecar
/// that IS set reaches the file: a `#[serde(skip)]` where
/// `skip_serializing_if` was meant would pass both, write the evaluated
/// number and drop the driver on every save, which is the silent data loss
/// §13.3 argues cannot happen because the plain field carries the value. The
/// geometry would indeed be right; the parameter would have stopped driving
/// it.
///
/// Compared by document form rather than field by field, so a sidecar added
/// later is covered by the fixture that carries it rather than by a list
/// here.
#[test]
fn every_p3_expression_sidecar_round_trips_through_a_file() {
    let ops = [
        serde_json::json!({ "type": "Extrude", "params": {
            "sketch_id": Uuid::nil(), "profile_index": 0, "depth": 0.004,
            "symmetric": false, "cut": false, "depth_expr": "front",
            "second_direction": { "type": "Blind", "depth": 0.002, "depth_expr": "back" } }}),
        serde_json::json!({ "type": "Revolve", "params": {
            "sketch_id": Uuid::nil(), "profile_index": 0,
            "axis_origin": [0.001, 0.0, 0.0], "axis_direction": [0.0, 1.0, 0.0],
            "axis_origin_expr": ["lift", null, null],
            "angle": 360.0, "angle_expr": "turn" }}),
        serde_json::json!({ "type": "DatumPlane", "params": { "name": "Datum", "definition": {
            "method": "point-normal", "origin": [0.0, 0.0, 0.005],
            "origin_expr": [null, null, "height"], "normal": [0.0, 0.0, 1.0] }}}),
        serde_json::json!({ "type": "PatternCircular", "params": {
            "seeds": { "type": "All" },
            "axis": { "method": "explicit", "origin": [0.002, 0.0, 0.0],
                      "origin_expr": ["hub", null, null], "direction": [0.0, 0.0, 1.0] },
            "count": 4, "count_expr": "teeth", "angle_deg": 360.0, "angle_expr": "sweep" }}),
        serde_json::json!({ "type": "PatternLinear", "params": {
            "seeds": { "type": "All" },
            "direction": { "method": "explicit", "origin": [0.0, 0.0, 0.0], "direction": [1.0, 0.0, 0.0] },
            "count": 2, "count_expr": "rows", "spacing": 0.01, "spacing_expr": "pitch",
            "second": { "direction": { "method": "explicit", "origin": [0.0, 0.0, 0.0], "direction": [0.0, 1.0, 0.0] },
                        "count": 3, "count_expr": "cols", "spacing": 0.02, "spacing_expr": "pitch * 2" } }}),
        serde_json::json!({ "type": "MateConnector", "params": {
            "frame": { "origin": [0.0, 0.0, 0.0], "z_axis": [0.0, 0.0, 1.0], "x_axis": [1.0, 0.0, 0.0] },
            "rotation_deg": 15.0, "rotation_expr": "twist",
            "offset_m": [0.0, 0.0, 0.001], "offset_m_expr": [null, null, "clear"] }}),
        serde_json::json!({ "type": "ImportedBody", "params": {
            "file_name": "part.step", "step_text": "ISO-10303-21;\nEND-ISO-10303-21;\n",
            "translation_m": [0.001, 0.0, 0.0], "translation_m_expr": ["dx", null, null],
            "rotation_deg": [0.0, 0.0, 30.0], "rotation_deg_expr": [null, null, "yaw"],
            "scale": 0.5, "scale_expr": "shrink" }}),
    ];

    let mut tree = FeatureTree::new();
    for op in &ops {
        let operation: Operation =
            serde_json::from_value(op.clone()).unwrap_or_else(|e| panic!("{}: {e}", op["type"]));
        tree.features.push(Feature {
            id: Uuid::new_v4(),
            name: op["type"].as_str().unwrap().to_string(),
            operation,
            suppressed: false,
            references: Default::default(),
        });
    }
    let json = save_project(&tree, &ProjectMetadata::new("P3 sidecars"));
    let (loaded, _) = load_project(&json).unwrap();
    assert_eq!(loaded.features.len(), ops.len());

    // The FIXTURE is the oracle, not the struct. Comparing the loaded
    // operation against the original re-serialized would pass for a
    // `#[serde(skip)]` field, because both sides would then omit it — the
    // comparison would be the struct agreeing with itself. So every `*_expr`
    // the fixture states is looked up by its own path in what came back.
    let mut checked = 0usize;
    for (back, original) in loaded.features.iter().zip(ops.iter()) {
        let got = serde_json::to_value(&back.operation).unwrap();
        checked += compare_expr_keys(original, &got, &back.name, &back.name);
    }
    assert_eq!(
        checked, 17,
        "the fixtures state 17 sidecar values across the seven operations; a renamed or mistyped key would quietly lower this instead of failing"
    );
}

/// Every `*_expr` key `want` states, found at the same path in `got` with the
/// same value. Returns how many were checked, so a fixture that states none
/// cannot pass by vacuity.
fn compare_expr_keys(
    want: &serde_json::Value,
    got: &serde_json::Value,
    name: &str,
    at: &str,
) -> usize {
    let mut n = 0;
    match want {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let here = format!("{at}.{key}");
                let mine = got.get(key);
                if key.ends_with("_expr") {
                    assert_eq!(mine, Some(value), "{name}: {here} did not survive the file");
                    n += 1;
                }
                if let Some(mine) = mine {
                    n += compare_expr_keys(value, mine, name, &here);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                if let Some(mine) = got.get(i) {
                    n += compare_expr_keys(item, mine, name, &format!("{at}[{i}]"));
                }
            }
        }
        _ => {}
    }
    n
}
