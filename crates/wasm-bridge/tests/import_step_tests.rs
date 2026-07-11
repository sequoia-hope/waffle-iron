//! ImportStep message tests (task #138, SI1): the bridge-level contract —
//! the message lands an ImportedBody feature in the tree with a compressed
//! embedded payload, and the model-updated response carries the new body.
//! Run with the REAL kernel-v2 adapter so the imported body's mesh path is
//! exercised end to end (parse → ingest → tessellate).

use feature_engine::types::*;
use kernel_v2::KernelV2Adapter;
use wasm_bridge::messages::*;
use wasm_bridge::*;

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

#[test]
fn import_step_message_creates_feature_and_body() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();

    let response = dispatch(
        &mut state,
        UiToEngine::ImportStep {
            file_name: "cube.step".to_string(),
            data: CUBE_STEP.to_string(),
        },
        &mut kernel,
    );

    // The feature landed, is named after the file, and carries a compressed
    // payload (not the raw text).
    assert_eq!(state.engine.tree.features.len(), 1);
    let feature = &state.engine.tree.features[0];
    assert_eq!(feature.name, "Import cube.step");
    let Operation::ImportedBody { params } = &feature.operation else {
        panic!("expected ImportedBody feature");
    };
    assert_eq!(params.blob_encoding, step_import::STEP_BLOB_ENCODING);
    assert!(params.blob.len() < CUBE_STEP.len());
    assert_eq!(params.scale, 1.0);

    // No rebuild errors; the import produced a real body through kernel-v2.
    assert!(
        state.engine.errors.is_empty(),
        "errors: {:?}",
        state.engine.errors
    );
    let result = state
        .engine
        .feature_results
        .get(&feature.id)
        .expect("op result");
    assert_eq!(result.outputs.len(), 1);

    // And it tessellates through the trait: a cube has 6 pick ranges.
    let handle = result.outputs[0].1.handle.clone();
    use waffle_types::kernel::Kernel as _;
    let mesh = kernel.tessellate(&handle, 0.001).expect("mesh");
    assert_eq!(mesh.face_ranges.len(), 6);
    assert!(!mesh.indices.is_empty());

    // Response is a model update (not an error).
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
}
