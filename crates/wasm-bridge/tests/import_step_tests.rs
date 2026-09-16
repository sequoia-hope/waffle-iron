//! ImportStep + v4 sources at the bridge (task #138 SI1; v4
//! `specs/waffle_v4_document_model.md` §2.3/§2.11): the message lands an
//! ImportedBody feature whose STEP text lives in the document's `sources`
//! table (packed, hashed) and in the engine's source store; SaveDocument
//! writes it, LoadProject reads it back, ProvideSource recovers a linked
//! source, NewDocument clears it. Run with the REAL kernel-v2 adapter so the
//! imported body's mesh path is exercised end to end.

use feature_engine::types::*;
use file_format::{
    git_blob_sha1, load_document, DocumentMetadata, GitRef, Locator, SourceEntry, SourceKind, Tab,
    WaffleDocument,
};
use kernel_v2::KernelV2Adapter;
use uuid::Uuid;
use wasm_bridge::messages::*;
use wasm_bridge::*;

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

fn import_cube(state: &mut EngineState, kernel: &mut KernelV2Adapter) -> EngineToUi {
    dispatch(
        state,
        UiToEngine::ImportStep {
            file_name: "cube.step".to_string(),
            data: CUBE_STEP.to_string(),
        },
        kernel,
    )
}

/// Save through the session (S2 C3c): the metadata, the tab list and the live
/// tree are all its own, so the message carries nothing.
fn save_document(state: &mut EngineState, kernel: &mut KernelV2Adapter) -> String {
    state.set_project_name("Doc");
    state.set_display_unit("mm");
    let response = dispatch(state, UiToEngine::SaveDocument, kernel);
    match response {
        EngineToUi::SaveReady { json_data } => json_data,
        other => panic!("expected SaveReady, got {other:?}"),
    }
}

#[test]
fn import_step_message_creates_a_source_and_a_feature_that_names_it() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();

    let response = import_cube(&mut state, &mut kernel);

    // The feature landed, is named after the file, and names its source
    // instead of carrying the text.
    assert_eq!(state.engine.tree.features.len(), 1);
    let feature = &state.engine.tree.features[0];
    assert_eq!(feature.name, "Import cube.step");
    let Operation::ImportedBody { params } = &feature.operation else {
        panic!("expected ImportedBody feature");
    };
    assert!(params.blob.is_none() && params.blob_encoding.is_none());
    assert_eq!(params.scale, 1.0);
    let source_id = params.source_id.expect("feature names its source");

    // The sources table has the packed, hashed entry; the store has the text.
    assert_eq!(state.sources.len(), 1);
    let entry = &state.sources[0];
    assert_eq!(entry.id, source_id);
    assert_eq!(entry.name, "cube.step");
    assert_eq!(entry.kind, SourceKind::Step);
    assert_eq!(entry.locator, Locator::Embedded);
    assert!(entry.effective_pack());
    assert_eq!(
        entry.content_hash.as_deref(),
        Some(git_blob_sha1(CUBE_STEP.as_bytes()).as_str())
    );
    assert_eq!(
        state.engine.sources.text(source_id).as_deref(),
        Some(CUBE_STEP)
    );

    // Provenance records the import.
    assert!(matches!(
        state.engine.tree.provenance_of(feature.id),
        Some(Provenance { origin: ProvenanceOrigin::Import { source_id: s }, .. }) if *s == source_id
    ));

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

#[test]
fn save_document_writes_the_source_and_load_project_reads_it_back() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    import_cube(&mut state, &mut kernel);
    let feature_id = state.engine.tree.features[0].id;

    let json = save_document(&mut state, &mut kernel);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["version"], 5);
    assert_eq!(parsed["document"]["name"], "Doc");
    assert_eq!(parsed["sources"].as_array().unwrap().len(), 1);
    assert!(parsed["sources"][0]["embed"]["blob"].is_string(), "packed");
    let params = &parsed["tabs"][0]["kind"]["features"]["features"][0]["operation"]["params"];
    assert_eq!(params["source_id"], parsed["sources"][0]["id"]);
    assert!(
        params.get("blob").is_none(),
        "no inline payload in a v4 file"
    );
    assert_eq!(
        parsed["tabs"][0]["kind"]["features"]["provenance"][feature_id.to_string()]["origin"]
            ["type"],
        "Import"
    );

    // A fresh engine opens it: sources adopted, embed registered, body rebuilt.
    let mut fresh = EngineState::new();
    let mut kernel2 = KernelV2Adapter::new();
    let response = dispatch(
        &mut fresh,
        UiToEngine::LoadProject { data: json },
        &mut kernel2,
    );
    assert!(
        matches!(response, EngineToUi::ModelUpdated { .. }),
        "{response:?}"
    );
    assert!(
        fresh.engine.errors.is_empty(),
        "errors: {:?}",
        fresh.engine.errors
    );
    assert_eq!(fresh.sources.len(), 1);
    assert_eq!(fresh.project_name(), "Doc");
    assert_eq!(fresh.display_unit(), "mm");
    assert!(fresh.engine.sources.contains(fresh.sources[0].id));
    assert_eq!(fresh.engine.feature_results[&feature_id].outputs.len(), 1);
}

#[test]
fn a_linked_source_without_content_is_loud_until_provide_source() {
    // A v4 document whose STEP source is linked (git, not packed) and whose
    // content the file does not carry.
    let mut doc = WaffleDocument::new("Linked");
    let mut source = SourceEntry::linked(
        "cube.step",
        SourceKind::Step,
        Locator::git_branch("https://github.com/acme/parts", "cube.step", "main"),
    );
    source.id = Uuid::new_v4();
    let source_id = source.id;
    doc.sources.push(source);
    doc.tabs[0].features_mut().unwrap().features.push(Feature {
        id: Uuid::new_v4(),
        name: "Import cube".into(),
        operation: Operation::ImportedBody {
            params: ImportedBodyParams::from_source("cube.step", source_id),
        },
        suppressed: false,
        references: vec![],
    });
    let json = file_format::save_document(&doc);

    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let response = dispatch(
        &mut state,
        UiToEngine::LoadProject { data: json },
        &mut kernel,
    );
    assert!(
        matches!(response, EngineToUi::ModelUpdated { .. }),
        "{response:?}"
    );
    assert_eq!(state.engine.errors.len(), 1, "{:?}", state.engine.errors);
    assert!(state.engine.errors[0].1.contains("SourceUnavailable"));

    // Unknown source id is refused.
    let bad = dispatch(
        &mut state,
        UiToEngine::ProvideSource {
            source_id: Uuid::new_v4(),
            data: CUBE_STEP.into(),
            resolved_commit: None,
        },
        &mut kernel,
    );
    assert!(matches!(bad, EngineToUi::Error { .. }));

    // The host fetched it: the feature recovers, the entry is hashed, and
    // the saved file stays linked (no embed) because pack is false.
    let response = dispatch(
        &mut state,
        UiToEngine::ProvideSource {
            source_id,
            data: CUBE_STEP.into(),
            resolved_commit: None,
        },
        &mut kernel,
    );
    assert!(
        matches!(response, EngineToUi::ModelUpdated { .. }),
        "{response:?}"
    );
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
    assert_eq!(
        state.sources[0].content_hash.as_deref(),
        Some(git_blob_sha1(CUBE_STEP.as_bytes()).as_str())
    );
    assert!(state.sources[0].fetched_at.is_some());

    let json = save_document(&mut state, &mut kernel);
    let reloaded = load_document(&json).unwrap().document;
    assert_eq!(reloaded.sources.len(), 1);
    assert!(
        reloaded.sources[0].embed.is_none(),
        "linked source is not packed"
    );
    assert!(matches!(reloaded.sources[0].locator, Locator::Git { .. }));
    assert_eq!(
        reloaded.sources[0].content_hash,
        state.sources[0].content_hash
    );
}

#[test]
fn legacy_save_project_carries_the_sources_table_and_new_document_clears_it() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    import_cube(&mut state, &mut kernel);

    let EngineToUi::SaveReady { json_data } =
        dispatch(&mut state, UiToEngine::SaveProject, &mut kernel)
    else {
        panic!("SaveReady expected");
    };
    let parsed: serde_json::Value = serde_json::from_str(&json_data).unwrap();
    assert_eq!(parsed["sources"].as_array().unwrap().len(), 1);
    assert!(parsed["sources"][0]["embed"]["blob"].is_string());
    // The single-tree loader still gets a rebuildable tree (payload inlined).
    let (tree, _) = file_format::load_project(&json_data).unwrap();
    let Operation::ImportedBody { params } = &tree.features[0].operation else {
        panic!()
    };
    assert!(params.blob.is_some());

    dispatch(&mut state, UiToEngine::NewDocument, &mut kernel);
    assert!(state.sources.is_empty());
    assert!(state.engine.sources.is_empty());
    assert!(state.engine.tree.features.is_empty());
}

#[test]
fn an_opaque_tab_is_preserved_through_a_save() {
    // v4 §2.6: a tab of a kind this build cannot open rides load → save
    // verbatim.
    //
    // Until S2 C3c this test also covered `SaveDocument` refusing a dangling
    // `active_tab` and refusing to host the live tree on a non-Part tab. Both
    // were validations of a PAYLOAD that no longer exists: the session cannot
    // be handed a tab list that disagrees with itself, and `stash_active`
    // leaves a tab that holds no tree alone rather than erroring.
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();

    let part = Tab::part("Part 1", FeatureTree::new());
    let part_id = part.id.clone();
    let drawing: Tab = serde_json::from_value(serde_json::json!({
        "id": "drw", "name": "Drawing 1", "kind": { "type": "Drawing", "sheets": [] }
    }))
    .unwrap();
    let fixture = WaffleDocument {
        document: DocumentMetadata::new("Doc"),
        sources: Vec::new(),
        tabs: vec![part, drawing],
        active_tab: part_id,
        extra: Default::default(),
    };
    let json = file_format::save_document_verified(&fixture).expect("the fixture saves");
    dispatch(
        &mut state,
        UiToEngine::LoadProject { data: json },
        &mut kernel,
    );

    let response = dispatch(&mut state, UiToEngine::SaveDocument, &mut kernel);
    let EngineToUi::SaveReady { json_data } = response else {
        panic!("{response:?}")
    };
    let parsed: serde_json::Value = serde_json::from_str(&json_data).unwrap();
    assert_eq!(parsed["tabs"][1]["kind"]["type"], "Drawing");
    assert_eq!(parsed["tabs"][1]["kind"]["sheets"], serde_json::json!([]));
}

// ── v4 Phase 2 P2-3: source listing, resolved commits, linked STEP imports ──

#[test]
fn list_sources_reports_availability_and_provide_source_records_the_commit() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();

    // One packed (available) source from an import, one linked entry the
    // host still has to fetch.
    import_cube(&mut state, &mut kernel);
    let mut doc = load_document(&save_document(&mut state, &mut kernel))
        .unwrap()
        .document;
    let linked = SourceEntry::linked(
        "bolt.step",
        SourceKind::Step,
        Locator::git_branch(
            "https://github.com/acme/parts",
            "fasteners/bolt.step",
            "main",
        ),
    );
    let linked_id = linked.id;
    doc.sources.push(linked);
    let json = file_format::save_document(&doc);
    let mut state = EngineState::new();
    dispatch(
        &mut state,
        UiToEngine::LoadProject { data: json },
        &mut kernel,
    );

    let listed = dispatch(&mut state, UiToEngine::ListSources, &mut kernel);
    let EngineToUi::SourcesListed { sources } = listed else {
        panic!("{listed:?}")
    };
    assert_eq!(sources.len(), 2);
    let cube = sources.iter().find(|s| s.name == "cube.step").unwrap();
    assert!(cube.available && cube.pack);
    assert_eq!(cube.kind, "Step");
    assert!(cube.content_hash.is_some());
    let bolt = sources.iter().find(|s| s.id == linked_id).unwrap();
    assert!(!bolt.available && !bolt.pack);
    assert!(matches!(bolt.locator, Locator::Git { .. }));
    assert!(bolt.resolved.is_none());

    // The host fetched it at a commit: hash AND resolved commit recorded.
    let sha = "9fceb02a".repeat(5);
    let resp = dispatch(
        &mut state,
        UiToEngine::ProvideSource {
            source_id: linked_id,
            data: CUBE_STEP.to_string(),
            resolved_commit: Some(sha.to_uppercase()),
        },
        &mut kernel,
    );
    assert!(matches!(resp, EngineToUi::ModelUpdated { .. }), "{resp:?}");
    let entry = state.sources.iter().find(|s| s.id == linked_id).unwrap();
    assert_eq!(
        entry.content_hash.as_deref(),
        Some(git_blob_sha1(CUBE_STEP.as_bytes()).as_str())
    );
    assert_eq!(
        entry.resolved.as_ref().map(|r| r.commit.as_str()),
        Some(sha.as_str())
    );
    let EngineToUi::SourcesListed { sources } =
        dispatch(&mut state, UiToEngine::ListSources, &mut kernel)
    else {
        panic!()
    };
    assert!(
        sources
            .iter()
            .find(|s| s.id == linked_id)
            .unwrap()
            .available
    );
}

#[test]
fn import_step_from_locator_creates_a_linked_source_that_builds_and_saves_unpacked() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let sha = "9fceb02a".repeat(5);
    let resp = dispatch(
        &mut state,
        UiToEngine::ImportStepFromLocator {
            file_name: "cube.step".to_string(),
            locator: Locator::git_branch(
                "https://github.com/acme/parts",
                "parts/cube.step",
                "main",
            ),
            data: CUBE_STEP.to_string(),
            resolved_commit: Some(sha.clone()),
        },
        &mut kernel,
    );
    assert!(matches!(resp, EngineToUi::ModelUpdated { .. }), "{resp:?}");
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
    assert_eq!(state.engine.tree.features.len(), 1);
    let Operation::ImportedBody { params } = &state.engine.tree.features[0].operation else {
        panic!()
    };
    let source_id = params.source_id.expect("names its source");
    assert!(params.blob.is_none());

    // Saved: linked (no embed), hashed, resolved — and it loads back as
    // unavailable until the host provides it again.
    let json = save_document(&mut state, &mut kernel);
    let doc = load_document(&json).unwrap().document;
    let entry = doc.source(source_id).unwrap();
    assert!(matches!(entry.locator, Locator::Git { .. }));
    assert!(!entry.effective_pack());
    assert!(entry.embed.is_none());
    assert_eq!(
        entry.content_hash.as_deref(),
        Some(git_blob_sha1(CUBE_STEP.as_bytes()).as_str())
    );
    assert_eq!(
        entry.resolved.as_ref().map(|r| r.commit.as_str()),
        Some(sha.as_str())
    );
    assert!(entry.fetched_at.is_some());
    assert!(
        matches!(&state.engine.tree.provenance.get(&state.engine.tree.features[0].id).map(|p| &p.origin), Some(ProvenanceOrigin::Import { source_id: s }) if *s == source_id)
    );

    let mut fresh = EngineState::new();
    dispatch(
        &mut fresh,
        UiToEngine::LoadProject { data: json },
        &mut kernel,
    );
    assert!(
        fresh
            .engine
            .errors
            .iter()
            .any(|(_, m)| m.contains("SourceUnavailable") || m.contains("unavailable")),
        "{:?}",
        fresh.engine.errors
    );

    // A Local locator cannot be linked.
    let bad = dispatch(
        &mut state,
        UiToEngine::ImportStepFromLocator {
            file_name: "x.step".into(),
            locator: Locator::Local {
                provider: "local".into(),
                doc_id: "abc".into(),
            },
            data: CUBE_STEP.to_string(),
            resolved_commit: None,
        },
        &mut kernel,
    );
    assert!(matches!(bad, EngineToUi::Error { .. }), "{bad:?}");
}

// ── v4 Phase 2 P2-4: pack policy and pin/retarget at the bridge ───────────

fn linked_cube(state: &mut EngineState, kernel: &mut KernelV2Adapter, sha: &str) -> Uuid {
    dispatch(
        state,
        UiToEngine::ImportStepFromLocator {
            file_name: "cube.step".into(),
            locator: Locator::git_branch(
                "https://github.com/acme/parts",
                "parts/cube.step",
                "main",
            ),
            data: CUBE_STEP.to_string(),
            resolved_commit: Some(sha.to_string()),
        },
        kernel,
    );
    let Operation::ImportedBody { params } = &state.engine.tree.features[0].operation else {
        panic!()
    };
    params.source_id.unwrap()
}

#[test]
fn model_updated_carries_the_sources_table() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let resp = import_cube(&mut state, &mut kernel);
    let EngineToUi::ModelUpdated { sources, .. } = resp else {
        panic!("{resp:?}")
    };
    assert_eq!(sources.len(), 1);
    assert!(sources[0].available && sources[0].pack);
    assert!(matches!(sources[0].locator, Locator::Embedded));
}

#[test]
fn pack_policy_toggles_the_embed_and_embedded_sources_cannot_be_unpacked() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let sha = "9fceb02a".repeat(5);
    let id = linked_cube(&mut state, &mut kernel, &sha);

    // Linked ⇒ unpacked by default.
    let doc = load_document(&save_document(&mut state, &mut kernel))
        .unwrap()
        .document;
    assert!(doc.source(id).unwrap().embed.is_none());

    // pack: true ⇒ the file carries the content (self-contained), origin kept.
    let resp = dispatch(
        &mut state,
        UiToEngine::UpdateSourceEntry {
            source_id: id,
            pack: Some(true),
            git_ref: None,
        },
        &mut kernel,
    );
    let EngineToUi::ModelUpdated { sources, .. } = resp else {
        panic!("{resp:?}")
    };
    assert!(sources[0].pack && sources[0].available);
    let doc = load_document(&save_document(&mut state, &mut kernel))
        .unwrap()
        .document;
    let entry = doc.source(id).unwrap();
    assert!(entry.embed.is_some());
    assert!(matches!(entry.locator, Locator::Git { .. }));
    assert_eq!(
        entry.resolved.as_ref().map(|r| r.commit.as_str()),
        Some(sha.as_str())
    );

    // pack: false ⇒ linked again.
    dispatch(
        &mut state,
        UiToEngine::UpdateSourceEntry {
            source_id: id,
            pack: Some(false),
            git_ref: None,
        },
        &mut kernel,
    );
    let doc = load_document(&save_document(&mut state, &mut kernel))
        .unwrap()
        .document;
    assert!(doc.source(id).unwrap().embed.is_none());

    // An Embedded source (file picker) has no origin: unpacking is refused.
    let mut state2 = EngineState::new();
    import_cube(&mut state2, &mut kernel);
    let embedded_id = state2.sources[0].id;
    let bad = dispatch(
        &mut state2,
        UiToEngine::UpdateSourceEntry {
            source_id: embedded_id,
            pack: Some(false),
            git_ref: None,
        },
        &mut kernel,
    );
    assert!(matches!(bad, EngineToUi::Error { .. }), "{bad:?}");
    assert!(state2.sources[0].effective_pack());
}

#[test]
fn pinning_to_the_resolved_commit_keeps_content_and_any_other_ref_drops_it() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let sha = "9fceb02a".repeat(5);
    let id = linked_cube(&mut state, &mut kernel, &sha);

    // Pin (UI: "pin" = ref ← resolved.commit): content and hash survive.
    let resp = dispatch(
        &mut state,
        UiToEngine::UpdateSourceEntry {
            source_id: id,
            pack: None,
            git_ref: Some(GitRef::Commit {
                sha: sha.to_uppercase(),
            }),
        },
        &mut kernel,
    );
    let EngineToUi::ModelUpdated { sources, .. } = resp else {
        panic!("{resp:?}")
    };
    assert!(sources[0].available);
    assert!(
        matches!(&sources[0].locator, Locator::Git { git_ref: GitRef::Commit { sha: s }, .. } if *s == sha)
    );
    assert_eq!(
        sources[0].resolved.as_ref().map(|r| r.commit.as_str()),
        Some(sha.as_str())
    );
    assert!(state.engine.errors.is_empty());

    // Retarget to a branch: the bytes we hold came from `sha`, not from
    // wherever `dev` points — content, hash and resolved are dropped, the
    // feature is loudly unavailable until the host provides it again.
    let resp = dispatch(
        &mut state,
        UiToEngine::UpdateSourceEntry {
            source_id: id,
            pack: None,
            git_ref: Some(GitRef::Branch { name: "dev".into() }),
        },
        &mut kernel,
    );
    let EngineToUi::ModelUpdated {
        sources, errors, ..
    } = resp
    else {
        panic!("{resp:?}")
    };
    assert!(!sources[0].available);
    assert!(sources[0].resolved.is_none() && sources[0].content_hash.is_none());
    assert!(
        errors
            .iter()
            .any(|(_, m)| m.to_lowercase().contains("unavailable")),
        "{errors:?}"
    );

    // An invalid ref name is refused and leaves the entry as it was.
    let bad = dispatch(
        &mut state,
        UiToEngine::UpdateSourceEntry {
            source_id: id,
            pack: None,
            git_ref: Some(GitRef::Branch {
                name: "-bad..name".into(),
            }),
        },
        &mut kernel,
    );
    assert!(matches!(bad, EngineToUi::Error { .. }), "{bad:?}");
    assert!(
        matches!(&state.sources[0].locator, Locator::Git { git_ref: GitRef::Branch { name }, .. } if name == "dev")
    );

    // git_ref on a non-git source is refused.
    let mut state2 = EngineState::new();
    import_cube(&mut state2, &mut kernel);
    let embedded_id = state2.sources[0].id;
    let bad = dispatch(
        &mut state2,
        UiToEngine::UpdateSourceEntry {
            source_id: embedded_id,
            pack: None,
            git_ref: Some(GitRef::Branch {
                name: "main".into(),
            }),
        },
        &mut kernel,
    );
    assert!(matches!(bad, EngineToUi::Error { .. }), "{bad:?}");
}

#[test]
fn an_active_assembly_tab_saves_without_touching_the_live_tree() {
    use feature_engine::assembly::AssemblyTree;
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    // The cube is the live tree of the document's Part tab.
    import_cube(&mut state, &mut kernel);
    let asm_id = state
        .session
        .add_tab("Assembly", None)
        .expect("an Assembly tab");
    state
        .session
        .set_assembly(&asm_id, AssemblyTree::default())
        .expect("the tab takes its assembly");
    // Opening the assembly stashes the live tree into the Part tab it came
    // from and leaves the live tree empty.
    dispatch(
        &mut state,
        UiToEngine::OpenAssembly {
            tab_id: asm_id.clone(),
        },
        &mut kernel,
    );

    let response = dispatch(&mut state, UiToEngine::SaveDocument, &mut kernel);
    let EngineToUi::SaveReady { json_data } = response else {
        panic!("{response:?}")
    };
    let doc = load_document(&json_data).unwrap().document;
    assert_eq!(doc.active_tab, asm_id);
    assert!(doc.tab(&asm_id).unwrap().assembly_tree().is_some());
    // The Part tab holds the imported cube — the live tree went to the tab it
    // belongs to, not onto the assembly (S2 C3c; before the session composed
    // the file, the UI handed over an empty Part tab here).
    assert_eq!(doc.tabs[0].features().unwrap().features.len(), 1);
    // Loading a document whose active tab is an assembly opens with an
    // empty live tree (the assembly is evaluated by OpenAssembly).
    let mut fresh = EngineState::new();
    let r = dispatch(
        &mut fresh,
        UiToEngine::LoadProject { data: json_data },
        &mut kernel,
    );
    assert!(matches!(r, EngineToUi::ModelUpdated { .. }), "{r:?}");
    assert!(fresh.engine.tree.features.is_empty());
}
