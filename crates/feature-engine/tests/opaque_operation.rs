//! v4 Phase 1b (`specs/waffle_v4_document_model.md` §2.5): an operation kind
//! this build does not know is preserved verbatim as `Operation::Unknown`,
//! re-emitted byte-for-byte, and fails its rebuild loudly — so adding an
//! operation kind is no longer a `MIN_READER_VERSION` bump. A malformed KNOWN
//! kind is still a parse error.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

fn loft() -> serde_json::Value {
    serde_json::json!({
        "type": "Loft",
        "params": { "sections": [[1, 2], [3]], "ruled": false },
        "x-tool": "future-build"
    })
}

#[test]
fn unknown_operation_round_trips_verbatim() {
    let op: Operation = serde_json::from_value(loft()).unwrap();
    assert!(matches!(op, Operation::Unknown(_)));
    assert_eq!(op.type_tag(), "Loft");
    assert_eq!(serde_json::to_value(&op).unwrap(), loft());

    // Inside a Feature / FeatureTree too (the shape the file carries).
    let tree = serde_json::json!({
        "features": [{ "id": Uuid::nil(), "name": "Loft 1", "suppressed": false,
                       "references": [], "operation": loft() }],
        "active_index": null
    });
    let parsed: FeatureTree = serde_json::from_value(tree.clone()).unwrap();
    assert_eq!(parsed.features.len(), 1);
    let back = serde_json::to_value(&parsed).unwrap();
    assert_eq!(back["features"][0]["operation"], loft());
}

#[test]
fn known_tags_round_trip_as_before_and_report_their_tag() {
    let sketch_json = serde_json::to_value(Operation::Sketch {
        sketch: square_sketch(),
    })
    .unwrap();
    assert_eq!(sketch_json["type"], "Sketch");
    let op: Operation = serde_json::from_value(sketch_json.clone()).unwrap();
    assert!(matches!(op, Operation::Sketch { .. }));
    assert_eq!(op.type_tag(), "Sketch");
    assert_eq!(serde_json::to_value(&op).unwrap(), sketch_json);

    // A known tag with an incomplete payload names the tag AND the field.
    let incomplete = serde_json::json!({ "type": "DatumPlane", "params": { "name": "P" } });
    let err = serde_json::from_value::<Operation>(incomplete)
        .unwrap_err()
        .to_string();
    assert!(err.contains("operation `DatumPlane`"), "{err}");
    assert!(err.contains("missing field"), "{err}");
    for tag in OPERATION_TAGS {
        assert!(!tag.is_empty());
    }
    assert_eq!(OPERATION_TAGS.len(), 10);
}

#[test]
fn malformed_known_kind_and_missing_tag_are_parse_errors() {
    let bad = serde_json::json!({ "type": "Extrude", "params": 42 });
    let err = serde_json::from_value::<Operation>(bad)
        .unwrap_err()
        .to_string();
    assert!(err.contains("operation `Extrude`"), "{err}");

    let untagged = serde_json::json!({ "params": {} });
    let err = serde_json::from_value::<Operation>(untagged)
        .unwrap_err()
        .to_string();
    assert!(err.contains("string `type`"), "{err}");

    let not_an_object = serde_json::json!("Extrude");
    assert!(serde_json::from_value::<Operation>(not_an_object).is_err());
}

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
        plane_origin: [0.0; 3],
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
        constraints: vec![],
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
    }
}

#[test]
fn rebuild_fails_the_unknown_feature_loudly_and_builds_the_rest() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let sketch_feature = engine.tree.add_feature(
        "Sketch".into(),
        Operation::Sketch {
            sketch: square_sketch(),
        },
    );
    let loft_feature = engine
        .tree
        .add_feature("Loft 1".into(), serde_json::from_value(loft()).unwrap());
    let extrude_feature = engine.tree.add_feature(
        "Extrude".into(),
        Operation::Extrude {
            params: ExtrudeParams {
                sketch_id: sketch_feature,
                profile_index: 0,
                profile_entity_ids: None,
                depth: 0.5,
                depth_expr: None,
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
        },
    );
    engine.rebuild_from_scratch(&mut kernel);

    let (fid, msg) = engine
        .errors
        .iter()
        .find(|(fid, _)| *fid == loft_feature)
        .expect("the unknown feature errors");
    assert_eq!(*fid, loft_feature);
    assert!(
        msg.contains("operation kind `Loft` is not supported"),
        "{msg}"
    );
    assert!(!engine.feature_results.contains_key(&loft_feature));
    // The feature is still in the tree (preserved), and later features build.
    assert_eq!(engine.tree.features.len(), 3);
    assert!(engine.feature_results.contains_key(&extrude_feature));
}
