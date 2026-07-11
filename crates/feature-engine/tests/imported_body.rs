//! STEP-imported-body feature tests (task #138, SI1) — the engine-level
//! contract: an ImportedBody feature decodes its embedded blob, parses,
//! applies placement, ingests through the Kernel trait, and produces one
//! Main body output; the payload round-trips through serde.

use feature_engine::types::*;
use feature_engine::Engine;
use waffle_types::kernel::MockKernel;

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

fn cube_import_op(translation_m: [f64; 3], rotation_deg: [f64; 3]) -> Operation {
    Operation::ImportedBody {
        params: ImportedBodyParams {
            file_name: "cube.step".to_string(),
            blob_encoding: step_import::STEP_BLOB_ENCODING.to_string(),
            blob: step_import::encode_step_blob(CUBE_STEP),
            translation_m,
            rotation_deg,
            scale: 1.0,
        },
    }
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
                blob_encoding: step_import::STEP_BLOB_ENCODING.to_string(),
                blob: "!!!corrupt!!!".to_string(),
                translation_m: [0.0; 3],
                rotation_deg: [0.0; 3],
                scale: 1.0,
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
fn imported_body_params_serde_round_trip() {
    let op = cube_import_op([0.001, 0.002, 0.003], [15.0, 0.0, 90.0]);
    let json = serde_json::to_string(&op).expect("serializes");
    assert!(json.contains("\"ImportedBody\""));
    let back: Operation = serde_json::from_str(&json).expect("deserializes");
    let Operation::ImportedBody { params } = back else {
        panic!("wrong variant");
    };
    assert_eq!(params.file_name, "cube.step");
    assert_eq!(params.translation_m, [0.001, 0.002, 0.003]);
    assert_eq!(params.rotation_deg, [15.0, 0.0, 90.0]);
    let text = step_import::decode_step_blob(&params.blob_encoding, &params.blob).unwrap();
    assert_eq!(text, CUBE_STEP);
}
