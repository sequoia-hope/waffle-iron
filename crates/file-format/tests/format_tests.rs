use feature_engine::types::{
    BooleanOp, BooleanParams, ChamferParams, ExtrudeParams, Feature, FeatureTree, FilletParams,
    Operation, RevolveParams, ShellParams,
};
use file_format::{load_project, save_project, LoadError, ProjectMetadata, FORMAT_VERSION};
use uuid::Uuid;
use waffle_types::{
    Anchor, GeomRef, OutputKey, ResolvePolicy, Role, Selector, Sketch, SketchConstraint,
    SketchEntity, SolveStatus, TopoKind,
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
