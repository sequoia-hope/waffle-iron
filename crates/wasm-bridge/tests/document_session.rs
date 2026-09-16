//! The document session at the bridge (`specs/waffle_server_mode.md` §2.3 S2,
//! checkpoint C2): `EngineState` owns one `DocumentSession`, `LoadProject` and
//! `NewDocument` populate it, `SaveDocument` adopts what the UI hands over, and
//! every `ModelUpdated` reports it.
//!
//! The session's unit tests live in `src/session.rs` (C1). These check the
//! dispatch wiring: that the document's name and display unit have exactly one
//! home, and that a host could drive a tab bar from `ModelUpdated` alone.

use feature_engine::types::FeatureTree;
use file_format::{save_document_verified, DocumentMetadata, Tab, WaffleDocument};
use waffle_types::kernel::MockKernel;
use wasm_bridge::messages::*;
use wasm_bridge::*;

/// The session `ModelUpdated` reported.
fn reported(response: &EngineToUi) -> &DocumentInfo {
    match response {
        EngineToUi::ModelUpdated { document, .. } => document
            .as_ref()
            .expect("every ModelUpdated reports the session"),
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
}

#[test]
fn a_fresh_state_is_an_untitled_one_tab_session() {
    let state = EngineState::new();
    assert_eq!(state.project_name(), "Untitled");
    // A document that states no preference is shown in millimetres, as the
    // field this replaced defaulted to.
    assert_eq!(state.display_unit(), "mm");
    assert_eq!(state.session.tabs().len(), 1);
    assert_eq!(state.session.revision(), 0);
}

#[test]
fn model_updated_reports_the_session() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = dispatch(
        &mut state,
        UiToEngine::SetDocumentMeta {
            name: None,
            display_unit: Some("in".to_string()),
        },
        &mut kernel,
    );

    let info = reported(&response);
    assert_eq!(info.display_unit.as_deref(), Some("in"));
    assert_eq!(info.name, "Untitled");
    assert_eq!(info.tabs.len(), 1);
    assert_eq!(info.active_tab, state.session.active_tab_id());
    assert_eq!(info.id, state.session.document().id);
    assert!(
        info.revision >= 1,
        "a committed mutation names a new document state"
    );
}

#[test]
fn the_display_unit_has_one_home_now() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    dispatch(
        &mut state,
        UiToEngine::SetDocumentMeta {
            name: None,
            display_unit: Some("cm".to_string()),
        },
        &mut kernel,
    );

    // Not two copies that can disagree: the accessor reads the session.
    assert_eq!(state.display_unit(), "cm");
    assert_eq!(state.session.document().display_unit.as_deref(), Some("cm"));
}

#[test]
fn load_project_populates_the_session_from_the_file() {
    let mut doc = WaffleDocument::new("Loaded");
    doc.document.display_unit = Some("in".to_string());
    doc.tabs.push(Tab::part("Second", FeatureTree::new()));
    let active = doc.active_tab.clone();
    let json = save_document_verified(&doc).expect("save the fixture");

    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let response = dispatch(
        &mut state,
        UiToEngine::LoadProject { data: json },
        &mut kernel,
    );

    let info = reported(&response);
    assert_eq!(info.name, "Loaded");
    assert_eq!(info.display_unit.as_deref(), Some("in"));
    assert_eq!(info.active_tab, active);
    // Both tabs, in bar order — the inactive one's tree is in the session, not
    // on the wire.
    let names: Vec<_> = info.tabs.iter().map(|t| t.name.clone()).collect();
    assert_eq!(names, ["Part 1", "Second"]);
    // And the accessors follow the loaded document.
    assert_eq!(state.project_name(), "Loaded");
    assert_eq!(state.display_unit(), "in");
}

#[test]
fn new_document_resets_the_session() {
    let doc = WaffleDocument::new("Loaded");
    let json = save_document_verified(&doc).expect("save the fixture");
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    dispatch(
        &mut state,
        UiToEngine::LoadProject { data: json },
        &mut kernel,
    );
    let loaded_id = state.session.document().id;

    let response = dispatch(&mut state, UiToEngine::NewDocument, &mut kernel);

    let info = reported(&response);
    assert_eq!(info.name, "Untitled");
    assert_eq!(info.tabs.len(), 1);
    assert_eq!(info.revision, 0, "a new document starts a new count");
    assert_ne!(info.id, loaded_id, "a new document is a new identity");
}

#[test]
fn the_tab_messages_drive_the_session_and_report_it() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let first = state.session.tabs()[0].id.clone();

    let r = dispatch(
        &mut state,
        UiToEngine::AddTab {
            kind: "Part".to_string(),
            name: None,
        },
        &mut kernel,
    );
    let info = reported(&r);
    assert_eq!(info.tabs.len(), 2);
    assert_eq!(info.tabs[1].name, "Part 2", "named as the tab bar's + does");
    assert_eq!(
        info.active_tab, first,
        "adding a tab does not open it; SwitchTab does"
    );
    let second = info.tabs[1].id.clone();

    let r = dispatch(
        &mut state,
        UiToEngine::RenameTab {
            tab_id: second.clone(),
            name: "Bracket".to_string(),
        },
        &mut kernel,
    );
    assert_eq!(reported(&r).tabs[1].name, "Bracket");

    let r = dispatch(
        &mut state,
        UiToEngine::MoveTab {
            tab_id: second.clone(),
            index: 0,
        },
        &mut kernel,
    );
    let order: Vec<String> = reported(&r).tabs.iter().map(|t| t.id.clone()).collect();
    assert_eq!(order, [second.clone(), first.clone()]);

    let r = dispatch(
        &mut state,
        UiToEngine::SwitchTab {
            tab_id: second.clone(),
        },
        &mut kernel,
    );
    assert_eq!(reported(&r).active_tab, second);
}

#[test]
fn the_live_tree_follows_the_tab_and_a_closed_tab_hands_over_to_its_successor() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let first = state.session.tabs()[0].id.clone();
    // A tree distinguishable from an empty one without building geometry (a
    // rollback bar at 0 slices no features, so the rebuild is still valid).
    state.engine.tree.active_index = Some(0);

    let r = dispatch(
        &mut state,
        UiToEngine::AddTab {
            kind: "Part".to_string(),
            name: None,
        },
        &mut kernel,
    );
    let second = reported(&r).tabs[1].id.clone();

    dispatch(
        &mut state,
        UiToEngine::SwitchTab {
            tab_id: second.clone(),
        },
        &mut kernel,
    );
    assert_eq!(state.engine.tree.active_index, None, "the new tab is empty");

    dispatch(
        &mut state,
        UiToEngine::SwitchTab {
            tab_id: first.clone(),
        },
        &mut kernel,
    );
    assert_eq!(
        state.engine.tree.active_index,
        Some(0),
        "the tab we left kept its tree — the wire no longer carries it"
    );

    // Closing the ACTIVE tab makes its successor active, and live.
    let r = dispatch(
        &mut state,
        UiToEngine::CloseTab { tab_id: first },
        &mut kernel,
    );
    let info = reported(&r);
    assert_eq!(info.tabs.len(), 1);
    assert_eq!(info.active_tab, second);
    assert_eq!(state.engine.tree.active_index, None);
}

#[test]
fn a_tab_id_the_document_does_not_have_is_a_loud_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let before = state.session.active_tab_id().to_string();

    let r = dispatch(
        &mut state,
        UiToEngine::SwitchTab {
            tab_id: "no-such-tab".to_string(),
        },
        &mut kernel,
    );
    match r {
        EngineToUi::Error { message, .. } => {
            assert!(message.contains("no-such-tab"), "{message}");
        }
        other => panic!("expected an Error, got {other:?}"),
    }
    assert_eq!(
        state.session.active_tab_id(),
        before,
        "a refused switch changes nothing"
    );
}

#[test]
fn save_document_adopts_the_uis_document_state() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let bracket = Tab::part("Bracket", FeatureTree::new());
    let plate = Tab::part("Plate", FeatureTree::new());
    let active = plate.id.clone();
    let response = dispatch(
        &mut state,
        UiToEngine::SaveDocument {
            document: DocumentMetadata::new("Saved").with_display_unit("ft"),
            tabs: vec![bracket, plate],
            active_tab: active.clone(),
        },
        &mut kernel,
    );
    assert!(
        matches!(response, EngineToUi::SaveReady { .. }),
        "{response:?}"
    );

    // The store is still the authority for the tab bar in C2, so a save is
    // where the session learns what it now looks like.
    assert_eq!(state.project_name(), "Saved");
    assert_eq!(state.display_unit(), "ft");
    assert_eq!(state.session.active_tab_id(), active);
    let names: Vec<_> = state.session.tabs().into_iter().map(|t| t.name).collect();
    assert_eq!(names, ["Bracket", "Plate"]);
}
