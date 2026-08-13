use chrono::Utc;
use feature_engine::types::{
    BooleanOp, BooleanParams, ChamferParams, ExtrudeParams, Feature, FeatureTree, FilletParams,
    Operation, RevolveParams, ShellParams,
};
use file_format::errors::ExportError;
use file_format::{
    export_step, load_document, load_project, save_document, save_project, DocumentMetadata,
    LoadError, PreviewMesh, ProjectMetadata, Tab, TabKind, FORMAT_VERSION,
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
                axis_origin: [0.0, 0.0, 0.0],
                axis_direction: [0.0, 1.0, 0.0],
                angle: std::f64::consts::PI,
                cut: false,
                merge: false,
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
    };

    let sketch = Sketch {
        id: sketch_feature_id, // Same as the Feature.id
        plane: plane_ref,
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
    };

    let constraints = vec![
        SketchConstraint::Horizontal { entity: 5 },
        SketchConstraint::Vertical { entity: 6 },
        SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 42.5,
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
    assert_eq!(parsed["version"], 3);
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
    let tab1_id = Uuid::new_v4().to_string();
    let tab2_id = Uuid::new_v4().to_string();

    let doc = DocumentMetadata {
        name: "Two Tabs".to_string(),
        created: Utc::now(),
        modified: Utc::now(),
        display_unit: Some("mm".to_string()),
    };

    let tabs = vec![
        Tab {
            id: tab1_id.clone(),
            name: "Part 1".to_string(),
            kind: TabKind::Part {
                features: tree1.clone(),
                preview_mesh: None,
            },
        },
        Tab {
            id: tab2_id,
            name: "Part 2".to_string(),
            kind: TabKind::Part {
                features: tree2,
                preview_mesh: None,
            },
        },
    ];

    let json = save_document(&doc, &tabs, tab1_id.clone());
    let (loaded_doc, loaded_tabs, loaded_active) = load_document(&json).unwrap();

    assert_eq!(loaded_doc.name, "Two Tabs");
    assert_eq!(loaded_tabs.len(), 2);
    assert_eq!(loaded_active, tab1_id);
    assert_eq!(loaded_tabs[0].name, "Part 1");
    assert_eq!(loaded_tabs[1].name, "Part 2");

    // Verify first tab has features
    match &loaded_tabs[0].kind {
        TabKind::Part { features, .. } => assert_eq!(features.features.len(), 2),
    }
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

    let (doc, tabs, _active) = load_document(&json).unwrap();
    assert_eq!(doc.name, "V2 File");
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0].name, "Part 1");
    match &tabs[0].kind {
        TabKind::Part { features, .. } => assert_eq!(features.features.len(), 2),
    }
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

    let (doc, tabs, _active) = load_document(&json).unwrap();
    assert_eq!(doc.name, "V1 File");
    assert_eq!(tabs.len(), 1);
}

#[test]
fn v3_active_tab_validity() {
    let doc = DocumentMetadata {
        name: "Test".to_string(),
        created: Utc::now(),
        modified: Utc::now(),
        display_unit: None,
    };
    let tab_id = Uuid::new_v4().to_string();
    let tabs = vec![Tab {
        id: tab_id.clone(),
        name: "Part".to_string(),
        kind: TabKind::Part {
            features: FeatureTree::new(),
            preview_mesh: None,
        },
    }];

    // Save with valid active_tab
    let json = save_document(&doc, &tabs, tab_id.clone());
    let result = load_document(&json);
    assert!(result.is_ok());

    // Manually create invalid active_tab reference
    let bad_id = Uuid::new_v4();
    let _bad_json = json.replace(&tab_id, &bad_id.to_string());
    // Both tab id and active_tab got replaced, so they still match — construct truly invalid JSON
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
    let doc = DocumentMetadata {
        name: "Mesh Test".to_string(),
        created: Utc::now(),
        modified: Utc::now(),
        display_unit: None,
    };
    let tab_id = Uuid::new_v4().to_string();
    let mesh = PreviewMesh {
        vertices: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 2],
    };
    let tabs = vec![Tab {
        id: tab_id.clone(),
        name: "Part".to_string(),
        kind: TabKind::Part {
            features: FeatureTree::new(),
            preview_mesh: Some(mesh),
        },
    }];

    let json = save_document(&doc, &tabs, tab_id);
    let (_, loaded_tabs, _) = load_document(&json).unwrap();

    match &loaded_tabs[0].kind {
        TabKind::Part { preview_mesh, .. } => {
            let mesh = preview_mesh.as_ref().expect("should have preview mesh");
            assert_eq!(mesh.vertices.len(), 9);
            assert_eq!(mesh.normals.len(), 9);
            assert_eq!(mesh.indices.len(), 3);
        }
    }
}

#[test]
fn v3_load_project_returns_active_tab_features() {
    let tree1 = make_simple_tree();
    let tree2 = FeatureTree::new();
    let tab1_id = Uuid::new_v4().to_string();
    let tab2_id = Uuid::new_v4().to_string();

    let doc = DocumentMetadata {
        name: "Multi Tab".to_string(),
        created: Utc::now(),
        modified: Utc::now(),
        display_unit: None,
    };

    let tabs = vec![
        Tab {
            id: tab1_id.clone(),
            name: "Part 1".to_string(),
            kind: TabKind::Part {
                features: tree1.clone(),
                preview_mesh: None,
            },
        },
        Tab {
            id: tab2_id.clone(),
            name: "Part 2".to_string(),
            kind: TabKind::Part {
                features: tree2,
                preview_mesh: None,
            },
        },
    ];

    // Active tab is tab2 (empty tree)
    let json = save_document(&doc, &tabs, tab2_id);
    let (loaded_tree, loaded_meta) = load_project(&json).unwrap();
    assert_eq!(loaded_meta.name, "Multi Tab");
    assert_eq!(
        loaded_tree.features.len(),
        0,
        "Should return active tab's (empty) features"
    );

    // Active tab is tab1 (2 features)
    let json = save_document(&doc, &tabs, tab1_id);
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

    // load_document (document model) must round-trip the opaque id too.
    let (doc, tabs, active) = load_document(json).expect("non-uuid tab id should load");
    assert_eq!(doc.name, "Legacy Default Tab");
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0].id, "default");
    assert_eq!(active, "default");
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

// ── M4: STEP export — the NotSupported boundary ────────────────────────
//
// `make_rebuild_compatible_tree` and the `export_step` import above were left
// orphaned when this section's tests were removed: the fixture built a tree
// nothing exported, and clippy flagged both as dead. Rather than delete the
// residue, this pins the contract the module documents — kernel-v2 has no STEP
// export, so the trait default returns NotSupported and `export_step` surfaces
// it as `StepExportFailed` (root CLAUDE.md lists STEP export as a capability
// boundary, not a bug).
//
// When STEP export lands, this test FAILS — which is the point. Replace it with
// a real round-trip assertion at that time; do not relax it.

#[test]
fn step_export_reports_the_kernel_capability_gap_loudly() {
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
            "STEP export unexpectedly SUCCEEDED — if the kernel gained STEP support, \
             replace this test with a real round-trip assertion"
        ),
    }
}
