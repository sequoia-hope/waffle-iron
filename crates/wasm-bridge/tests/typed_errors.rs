//! ICR-2 of `specs/waffle_mcp_server.md`: typed errors across the bridge
//! (A6.2). `ModelUpdated.feature_errors` carries a `kind` for every rebuild
//! error in `errors`, and `EngineToUi::Error.kind` classifies a failed
//! command whenever the failure is an engine error — so an agent host can
//! tell `NotSupported` from `ProfileNotFound` without reading message text.

use feature_engine::types::*;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use wasm_bridge::messages::*;
use wasm_bridge::*;

fn missing_source_import() -> (Operation, Uuid) {
    let source_id = Uuid::new_v4();
    (
        Operation::ImportedBody {
            params: ImportedBodyParams::from_source("gone.step", source_id),
        },
        source_id,
    )
}

fn error_kind(response: &EngineToUi) -> Option<&ErrorKind> {
    match response {
        EngineToUi::Error { kind, .. } => kind.as_ref(),
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn model_updated_types_every_rebuild_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let (op, source_id) = missing_source_import();

    let response = dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: op,
            provenance: None,
        },
        &mut kernel,
    );

    let EngineToUi::ModelUpdated {
        errors,
        feature_errors,
        feature_id,
        ..
    } = &response
    else {
        panic!("expected ModelUpdated, got {response:?}");
    };
    let id = feature_id.expect("AddFeature names its feature");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(feature_errors.len(), 1, "{feature_errors:?}");
    assert_eq!(feature_errors[0].feature_id, id);
    assert_eq!(feature_errors[0].message, errors[0].1);
    assert_eq!(
        feature_errors[0].kind,
        ErrorKind::SourceUnavailable {
            source_id: Some(source_id)
        }
    );
}

#[test]
fn a_failed_command_carries_the_engine_error_kind() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let id = Uuid::new_v4();
    let deleted = dispatch(
        &mut state,
        UiToEngine::DeleteFeature { feature_id: id },
        &mut kernel,
    );
    assert_eq!(
        error_kind(&deleted),
        Some(&ErrorKind::FeatureNotFound { id })
    );

    let undone = dispatch(&mut state, UiToEngine::Undo, &mut kernel);
    assert_eq!(error_kind(&undone), Some(&ErrorKind::NothingToUndo));
}

#[test]
fn a_bridge_level_failure_has_no_engine_kind() {
    // Not an engine error (the host sent FinishSketch without BeginSketch):
    // `kind` is absent and the message still says what happened.
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let response = dispatch(
        &mut state,
        serde_json::from_value(serde_json::json!({"type": "FinishSketch"})).unwrap(),
        &mut kernel,
    );
    assert_eq!(error_kind(&response), None);
    let EngineToUi::Error { message, .. } = &response else {
        unreachable!()
    };
    assert!(message.contains("no active sketch"), "{message}");
}

#[test]
fn wire_shape_is_additive() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let undone = dispatch(&mut state, UiToEngine::Undo, &mut kernel);
    let json = serde_json::to_value(&undone).unwrap();
    assert_eq!(json["kind"], serde_json::json!({"type": "NothingToUndo"}));
    assert!(json["message"].is_string(), "message stays for the app");

    let clean = dispatch(&mut state, UiToEngine::NewDocument, &mut kernel);
    let json = serde_json::to_value(&clean).unwrap();
    if json["type"] == "ModelUpdated" {
        assert!(
            json.get("feature_errors").is_none(),
            "an error-free model omits feature_errors: {json}"
        );
    }
}
