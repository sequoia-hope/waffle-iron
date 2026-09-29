//! STEP-imported-body feature tests (task #138, SI1; v4 sources
//! `specs/waffle_v4_document_model.md` §2.3/§2.11) — the engine-level
//! contract: an ImportedBody feature finds its STEP text in the document's
//! source store (v4) or in its legacy inline blob (v3), parses, applies
//! placement, ingests through the Kernel trait, and produces one Main body
//! output; a missing source is a loud per-feature error, never a silent skip.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

fn cube_import_op(translation_m: [f64; 3], rotation_deg: [f64; 3]) -> Operation {
    let mut params = ImportedBodyParams::embedded("cube.step", CUBE_STEP);
    params.translation_m = translation_m;
    params.rotation_deg = rotation_deg;
    Operation::ImportedBody { params }
}

#[test]
fn imported_body_feature_produces_one_main_output() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let id = engine
        .add_feature(
            "Import cube.step".to_string(),
            cube_import_op([0.0; 3], [0.0; 3]),
            &mut kernel,
        )
        .expect("import feature adds");

    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);
    let result = engine.feature_results.get(&id).expect("result cached");
    assert_eq!(result.outputs.len(), 1);
    assert_eq!(result.outputs[0].0, waffle_types::OutputKey::Main);
    // Everything the import created is recorded for persistent naming.
    assert!(!result.provenance.created.is_empty());
}

#[test]
fn imported_body_placement_moves_signatures() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let id = engine
        .add_feature(
            "Import cube.step".to_string(),
            cube_import_op([0.5, 0.0, 0.0], [0.0; 3]),
            &mut kernel,
        )
        .expect("import feature adds");
    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);

    // The mock mirrors face centroids from the (placed) mesh: the cube is
    // 10mm at origin, so +0.5m translation puts every centroid x in
    // [0.5, 0.51].
    let created = &engine.feature_results[&id].provenance.created;
    let face_centroids: Vec<[f64; 3]> = created
        .iter()
        .filter(|e| e.kind == waffle_types::TopoKind::Face)
        .filter_map(|e| e.signature.centroid)
        .collect();
    assert!(!face_centroids.is_empty());
    for c in &face_centroids {
        assert!(
            (0.5 - 1e-9..=0.51 + 1e-9).contains(&c[0]),
            "centroid {c:?} not translated"
        );
    }
}

#[test]
fn imported_body_bad_blob_is_a_loud_feature_error() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let result = engine.add_feature(
        "Import broken".to_string(),
        Operation::ImportedBody {
            params: ImportedBodyParams {
                file_name: "broken.step".to_string(),
                source_id: None,
                blob_encoding: Some(step_import::STEP_BLOB_ENCODING.to_string()),
                blob: Some("!!!corrupt!!!".to_string()),
                translation_m: [0.0; 3],
                rotation_deg: [0.0; 3],
                scale: 1.0,
                product: None,
            },
        },
        &mut kernel,
    );
    // add_feature records rebuild errors on the engine rather than failing.
    let _ = result;
    assert!(
        !engine.errors.is_empty(),
        "corrupt blob must surface a rebuild error"
    );
}

#[test]
fn imported_body_params_serde_round_trip_legacy_shape() {
    let op = cube_import_op([0.001, 0.002, 0.003], [15.0, 0.0, 90.0]);
    let json = serde_json::to_string(&op).expect("serializes");
    assert!(json.contains("\"ImportedBody\""));
    assert!(!json.contains("source_id"), "None fields are omitted");
    let back: Operation = serde_json::from_str(&json).expect("deserializes");
    let Operation::ImportedBody { params } = back else {
        panic!("wrong variant");
    };
    assert_eq!(params.file_name, "cube.step");
    assert_eq!(params.translation_m, [0.001, 0.002, 0.003]);
    assert_eq!(params.rotation_deg, [15.0, 0.0, 90.0]);
    let text = step_import::decode_step_blob(
        params.blob_encoding.as_deref().unwrap(),
        params.blob.as_deref().unwrap(),
    )
    .unwrap();
    assert_eq!(text, CUBE_STEP);
}

/// A v3 file's ImportedBody (required `blob_encoding`/`blob`, no
/// `source_id`) still parses: the fields are optional now, not renamed.
#[test]
fn imported_body_v3_json_without_source_id_parses() {
    let v3 = serde_json::json!({
        "type": "ImportedBody",
        "params": {
            "file_name": "legacy.step",
            "blob_encoding": step_import::STEP_BLOB_ENCODING,
            "blob": step_import::encode_step_blob(CUBE_STEP),
            "translation_m": [0.0, 0.0, 0.0],
            "rotation_deg": [0.0, 0.0, 0.0],
            "scale": 1.0
        }
    });
    let op: Operation = serde_json::from_value(v3).expect("v3 shape parses");
    let Operation::ImportedBody { params } = op else {
        panic!("wrong variant");
    };
    assert!(params.source_id.is_none());
    assert!(params.has_inline_blob());
}

/// v4 shape: the STEP text is NOT in the feature — it comes from the
/// document source store, keyed by `source_id`.
#[test]
fn imported_body_resolves_content_from_the_source_store() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let source_id = Uuid::new_v4();
    engine.sources.insert_text(source_id, CUBE_STEP);

    let params = ImportedBodyParams::from_source("cube.step", source_id);
    let json = serde_json::to_string(&params).unwrap();
    assert!(!json.contains("\"blob\""), "v4 shape carries no payload");

    let id = engine
        .add_feature(
            "Import cube.step".to_string(),
            Operation::ImportedBody { params },
            &mut kernel,
        )
        .expect("adds");
    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);
    assert_eq!(engine.feature_results[&id].outputs.len(), 1);
}

#[test]
fn imported_body_without_any_content_is_a_loud_source_unavailable_error() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let source_id = Uuid::new_v4();
    let _ = engine.add_feature(
        "Import missing.step".to_string(),
        Operation::ImportedBody {
            params: ImportedBodyParams::from_source("missing.step", source_id),
        },
        &mut kernel,
    );
    assert_eq!(engine.errors.len(), 1, "errors: {:?}", engine.errors);
    let (_, msg) = &engine.errors[0];
    assert!(
        msg.contains("SourceUnavailable") && msg.contains(&source_id.to_string()),
        "the error names the class and the source id: {msg}"
    );
    // The document still loads: the feature exists, it just has no result.
    assert_eq!(engine.tree.features.len(), 1);
}

/// Resolution order (§2.3): the store wins over a legacy inline blob, so a
/// document whose source was refreshed does not silently rebuild from the
/// stale payload a v3 writer left inside the feature.
#[test]
fn source_store_wins_over_a_stale_inline_blob() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let source_id = Uuid::new_v4();
    engine.sources.insert_text(source_id, CUBE_STEP);

    let mut params = ImportedBodyParams::embedded("cube.step", "not a STEP file at all");
    params.source_id = Some(source_id);
    let _ = engine.add_feature(
        "Import cube.step".to_string(),
        Operation::ImportedBody { params },
        &mut kernel,
    );
    assert!(
        engine.errors.is_empty(),
        "store content must be used, not the stale blob: {:?}",
        engine.errors
    );
}

/// The host fetched the source after the document loaded (a linked STEP
/// file): registering the content and rebuilding recovers the feature.
#[test]
fn providing_the_source_after_load_recovers_the_feature() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let source_id = Uuid::new_v4();
    let id = engine
        .add_feature(
            "Import cube.step".to_string(),
            Operation::ImportedBody {
                params: ImportedBodyParams::from_source("cube.step", source_id),
            },
            &mut kernel,
        )
        .expect("adds");
    assert_eq!(engine.errors.len(), 1);
    assert!(!engine.feature_results.contains_key(&id));

    engine.sources.insert_text(source_id, CUBE_STEP);
    engine.rebuild_from_scratch(&mut kernel);
    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);
    assert_eq!(engine.feature_results[&id].outputs.len(), 1);
}

/// Provenance (§2.7) is a side table keyed by feature id: recorded without a
/// rebuild, garbage-collected on delete, restored by undo, and serialized
/// with the tree.
#[test]
fn provenance_is_recorded_gcd_on_delete_and_restored_by_undo() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let source_id = Uuid::new_v4();
    engine.sources.insert_text(source_id, CUBE_STEP);
    let id = engine
        .add_feature(
            "Import cube.step".to_string(),
            Operation::ImportedBody {
                params: ImportedBodyParams::from_source("cube.step", source_id),
            },
            &mut kernel,
        )
        .expect("adds");

    let prov = Provenance {
        origin: ProvenanceOrigin::Import { source_id },
        at: Some("2026-09-07T18:00:00Z".to_string()),
    };
    assert_eq!(engine.set_provenance(id, Some(prov.clone())).unwrap(), None);
    assert_eq!(engine.tree.provenance_of(id), Some(&prov));
    assert!(
        engine.set_provenance(Uuid::new_v4(), None).is_err(),
        "unknown feature is an error"
    );

    // Serialized with the tree; absent origins are simply absent.
    let json = serde_json::to_string(&engine.tree).unwrap();
    assert!(json.contains("\"provenance\""));
    let back: FeatureTree = serde_json::from_str(&json).unwrap();
    assert_eq!(back.provenance_of(id), Some(&prov));

    engine.remove_feature(id, &mut kernel).unwrap();
    assert!(engine.tree.provenance.is_empty(), "GC'd on delete");
    engine.undo(&mut kernel).unwrap();
    assert_eq!(
        engine.tree.provenance_of(id),
        Some(&prov),
        "restored by undo"
    );
    engine.redo(&mut kernel).unwrap();
    assert!(engine.tree.provenance.is_empty(), "re-GC'd on redo");
}

/// Unknown keys on the feature tree (§2.6) survive a load → save round trip.
#[test]
fn feature_tree_preserves_unknown_keys() {
    let json = serde_json::json!({
        "features": [],
        "active_index": null,
        "x-agent-notes": { "author": "test", "steps": [1, 2, 3] },
        "some_future_field": "kept"
    });
    let tree: FeatureTree = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(tree.extra.len(), 2);
    let out = serde_json::to_value(&tree).unwrap();
    assert_eq!(out["x-agent-notes"], json["x-agent-notes"]);
    assert_eq!(out["some_future_field"], "kept");
    // Known fields are NOT captured by the catch-all.
    assert!(out.get("features").is_some());
    assert!(!tree.extra.contains_key("features"));

    // An ordinary tree serializes with no extra keys at all.
    let plain = serde_json::to_value(FeatureTree::new()).unwrap();
    assert_eq!(
        plain.as_object().unwrap().len(),
        2,
        "features + active_index only"
    );
}
