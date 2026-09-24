//! `.waffle` v4 document model (`specs/waffle_v4_document_model.md` §5
//! oracles): identity, the git-aware `sources` table, opaque preservation of
//! unknown tab/source/locator kinds, unknown-key preservation, the v3 → v4
//! migration (id minting, legacy tab ids, STEP payload lifting with git-blob
//! hashes), and the single-tree shims.

use feature_engine::types::{Feature, FeatureTree, ImportedBodyParams, Operation};
use feature_engine::Engine;
use file_format::sources::{Embed, GitHost, GitRef, Locator, SourceEntry, SourceKind};
use file_format::{
    git_blob_sha1, load_document, load_project, save_document, save_document_verified,
    save_project, LoadError, ProjectMetadata, Tab, TabKind, WaffleDocument, FORMAT_VERSION,
    MIN_READER_VERSION,
};
use uuid::Uuid;
use waffle_types::kernel::MockKernel;

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

fn v3_envelope(tabs: serde_json::Value, active: &str) -> String {
    serde_json::json!({
        "format": "waffle-iron",
        "version": 3,
        "min_reader_version": 3,
        "document": {
            "name": "V3 Doc",
            "created": "2026-01-01T00:00:00Z",
            "modified": "2026-01-02T00:00:00Z",
            "display_unit": "in"
        },
        "tabs": tabs,
        "active_tab": active
    })
    .to_string()
}

fn import_feature(name: &str, params: ImportedBodyParams) -> Feature {
    Feature {
        id: Uuid::new_v4(),
        name: name.to_string(),
        operation: Operation::ImportedBody { params },
        suppressed: false,
        references: vec![],
    }
}

// ---------------------------------------------------------------- envelope

#[test]
fn v4_envelope_round_trip_keeps_identity_sources_and_tabs() {
    let mut doc = WaffleDocument::new("Bracket");
    doc.document.display_unit = Some("mm".to_string());
    doc.sources.push(SourceEntry::linked(
        "bracket.waffle",
        SourceKind::Waffle,
        Locator::git_branch(
            "https://github.com/acme/parts.git",
            "brackets/bracket.waffle",
            "main",
        ),
    ));
    let json = save_document(&doc);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["version"], FORMAT_VERSION);
    assert_eq!(parsed["min_reader_version"], MIN_READER_VERSION);
    assert_eq!(FORMAT_VERSION, 5);
    assert_eq!(parsed["document"]["id"], doc.document.id.to_string());
    assert_eq!(parsed["sources"].as_array().unwrap().len(), 1);
    // `.git` is normalized away on the way in; host is inferred, not written.
    assert_eq!(
        parsed["sources"][0]["locator"]["remote"],
        "https://github.com/acme/parts"
    );
    assert!(parsed["sources"][0]["locator"].get("host").is_none());
    assert_eq!(parsed["sources"][0]["locator"]["ref"]["type"], "Branch");

    let loaded = load_document(&json).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let back = loaded.document;
    assert_eq!(back.document.id, doc.document.id);
    assert_eq!(back.document.display_unit.as_deref(), Some("mm"));
    assert_eq!(back.sources, doc.sources);
    assert_eq!(back.tabs.len(), 1);
    assert_eq!(back.active_tab, doc.active_tab);
    assert_eq!(
        back.sources[0].locator.git_host(),
        Some(GitHost::Github),
        "host inferred from the remote"
    );
}

#[test]
fn save_is_byte_stable_across_a_round_trip() {
    let mut doc = WaffleDocument::new("Stable");
    doc.sources.push(SourceEntry::embedded(
        "cube.step",
        SourceKind::Step,
        CUBE_STEP,
    ));
    let first = save_document(&doc);
    let second = save_document(&load_document(&first).unwrap().document);
    assert_eq!(first, second);
}

#[test]
fn verified_save_refuses_a_document_the_loader_would_reject() {
    let mut doc = WaffleDocument::new("Bad");
    doc.active_tab = "nope".to_string();
    assert!(matches!(
        save_document_verified(&doc),
        Err(LoadError::ParseError(_))
    ));
}

#[test]
fn document_id_missing_is_minted_and_reported() {
    let mut parsed: serde_json::Value =
        serde_json::from_str(&save_document(&WaffleDocument::new("NoId"))).unwrap();
    parsed["document"].as_object_mut().unwrap().remove("id");
    let loaded = load_document(&parsed.to_string()).unwrap();
    assert!(loaded
        .warnings
        .iter()
        .any(|w| w.contains("document.id was absent")));
    // The minted id is then persisted by the next save.
    let again: serde_json::Value = serde_json::from_str(&save_document(&loaded.document)).unwrap();
    assert_eq!(
        again["document"]["id"],
        loaded.document.document.id.to_string()
    );
}

#[test]
fn duplicate_source_ids_are_a_parse_error() {
    let mut doc = WaffleDocument::new("Dup");
    let entry = SourceEntry::embedded("a.step", SourceKind::Step, "A");
    doc.sources.push(entry.clone());
    doc.sources.push(entry);
    let json = save_document(&doc);
    assert!(
        matches!(load_document(&json), Err(LoadError::ParseError(m)) if m.contains("duplicate source id"))
    );
}

// --------------------------------------------------- opaque preservation

#[test]
fn unknown_tab_kind_is_preserved_verbatim_and_reported() {
    let assembly = serde_json::json!({
        "type": "Drawing",
        "sheets": [{ "id": "s1", "views": [{ "source": { "tab_id": "t0" }, "projection": "front" }] }],
        "annotations": []
    });
    let mut parsed: serde_json::Value =
        serde_json::from_str(&save_document(&WaffleDocument::new("Asm"))).unwrap();
    let part_id = parsed["tabs"][0]["id"].clone();
    parsed["tabs"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "id": "drw-tab", "name": "Drawing 1", "kind": assembly, "x-note": "kept" }));
    parsed["active_tab"] = part_id;

    let loaded = load_document(&parsed.to_string()).unwrap();
    assert!(loaded
        .warnings
        .iter()
        .any(|w| w.contains("unknown tab kind `Drawing`")));
    let doc = loaded.document;
    assert_eq!(doc.tabs.len(), 2);
    assert!(matches!(doc.tabs[1].kind, TabKind::Unknown(_)));
    assert_eq!(doc.tabs[1].kind.type_tag(), "Drawing");
    assert!(doc.tabs[1].features().is_none());

    let out: serde_json::Value = serde_json::from_str(&save_document(&doc)).unwrap();
    assert_eq!(out["tabs"][1]["kind"], assembly, "re-emitted byte-for-byte");
    assert_eq!(out["tabs"][1]["x-note"], "kept");

    // The single-tree API still opens the Part tab, and refuses to pretend
    // a Drawing tab is a part.
    assert!(load_project(&parsed.to_string()).is_ok());
    parsed["active_tab"] = serde_json::Value::String("drw-tab".into());
    assert!(
        matches!(load_project(&parsed.to_string()), Err(LoadError::ParseError(m)) if m.contains("Drawing"))
    );
}

#[test]
fn malformed_known_tab_kind_is_still_a_parse_error() {
    let mut parsed: serde_json::Value =
        serde_json::from_str(&save_document(&WaffleDocument::new("Bad"))).unwrap();
    parsed["tabs"][0]["kind"] = serde_json::json!({ "type": "Part", "features": 42 });
    assert!(
        matches!(load_document(&parsed.to_string()), Err(LoadError::ParseError(m)) if m.contains("tab kind `Part`"))
    );
    parsed["tabs"][0]["kind"] = serde_json::json!({ "features": {} });
    assert!(
        matches!(load_document(&parsed.to_string()), Err(LoadError::ParseError(m)) if m.contains("string `type`"))
    );
}

#[test]
fn unknown_source_kind_and_locator_are_preserved_and_flagged() {
    let mut doc = WaffleDocument::new("Future");
    doc.sources.push(SourceEntry::linked(
        "board.brd",
        SourceKind::Unknown(serde_json::json!({ "type": "AllegroBoard", "layers": 6 })),
        Locator::Unknown(serde_json::json!({ "type": "Ipfs", "cid": "bafy…" })),
    ));
    let json = save_document(&doc);
    let out: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(out["sources"][0]["kind"]["type"], "AllegroBoard");
    assert_eq!(out["sources"][0]["kind"]["layers"], 6);
    assert_eq!(out["sources"][0]["locator"]["cid"], "bafy…");

    let loaded = load_document(&json).unwrap();
    assert!(loaded
        .warnings
        .iter()
        .any(|w| w.contains("unknown source kind `AllegroBoard`")));
    assert!(loaded
        .warnings
        .iter()
        .any(|w| w.contains("unknown locator type `Ipfs`")));
    assert_eq!(loaded.document.sources, doc.sources);
}

#[test]
fn unknown_keys_survive_at_every_structural_level() {
    let mut parsed: serde_json::Value =
        serde_json::from_str(&save_document(&WaffleDocument::new("Extra"))).unwrap();
    parsed["x-envelope"] = serde_json::json!({ "a": 1 });
    parsed["document"]["x-doc"] = serde_json::json!("d");
    parsed["tabs"][0]["x-tab"] = serde_json::json!([1, 2]);
    parsed["tabs"][0]["kind"]["features"]["x-tree"] = serde_json::json!(true);
    parsed["sources"] = serde_json::json!([{
        "id": Uuid::new_v4().to_string(),
        "name": "s",
        "kind": { "type": "Step" },
        "locator": { "type": "Embedded" },
        "x-source": "kept"
    }]);
    let doc = load_document(&parsed.to_string()).unwrap().document;
    let out: serde_json::Value = serde_json::from_str(&save_document(&doc)).unwrap();
    assert_eq!(out["x-envelope"]["a"], 1);
    assert_eq!(out["document"]["x-doc"], "d");
    assert_eq!(out["tabs"][0]["x-tab"], serde_json::json!([1, 2]));
    assert_eq!(out["tabs"][0]["kind"]["features"]["x-tree"], true);
    assert_eq!(out["sources"][0]["x-source"], "kept");
    // Known keys are never captured as extras.
    assert!(!doc.document.extra.contains_key("name"));
    assert!(!doc.tabs[0].extra.contains_key("kind"));
    assert!(!doc.sources[0].extra.contains_key("locator"));
}

// ------------------------------------------------------------ migration

#[test]
fn v3_to_v4_mints_identity_rewrites_default_tab_and_lifts_step_payloads() {
    let a = import_feature(
        "Import cube",
        ImportedBodyParams::embedded("cube.step", CUBE_STEP),
    );
    let b = import_feature(
        "Import cube again",
        ImportedBodyParams::embedded("cube2.step", CUBE_STEP),
    );
    let c = import_feature(
        "Import other",
        ImportedBodyParams::embedded("other.step", "ISO-10303-21;\nEND-ISO-10303-21;\n"),
    );
    let tree = FeatureTree {
        features: vec![a.clone(), b.clone(), c.clone()],
        ..Default::default()
    };
    let json = v3_envelope(
        serde_json::json!([{
            "id": "default",
            "name": "Part 1",
            "kind": { "type": "Part", "features": serde_json::to_value(&tree).unwrap() }
        }]),
        "default",
    );

    let loaded = load_document(&json).unwrap();
    let doc = loaded.document;
    assert!(Uuid::parse_str(&doc.tabs[0].id).is_ok());
    assert_eq!(doc.active_tab, doc.tabs[0].id);
    assert_eq!(doc.document.display_unit.as_deref(), Some("in"));

    // Two byte-identical payloads share one source; the third is its own.
    assert_eq!(
        doc.sources.len(),
        2,
        "{:?}",
        doc.sources.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    let cube = &doc.sources[0];
    assert_eq!(cube.kind, SourceKind::Step);
    assert_eq!(cube.locator, Locator::Embedded);
    assert!(cube.effective_pack());
    assert_eq!(
        cube.content_hash.as_deref(),
        Some(git_blob_sha1(CUBE_STEP.as_bytes()).as_str())
    );
    assert_eq!(cube.embed.as_ref().unwrap().decode().unwrap(), CUBE_STEP);
    assert_eq!(cube.name, "cube.step", "first importer names the source");

    let feats = &doc.tabs[0].features().unwrap().features;
    for (f, expect_first) in feats.iter().zip([true, true, false]) {
        let Operation::ImportedBody { params } = &f.operation else {
            panic!()
        };
        assert!(
            params.blob.is_none() && params.blob_encoding.is_none(),
            "payload lifted"
        );
        assert_eq!(
            params.source_id,
            Some(if expect_first {
                doc.sources[0].id
            } else {
                doc.sources[1].id
            })
        );
    }

    // The migrated document writes as v4 and round-trips.
    let v4 = save_document(&doc);
    let parsed: serde_json::Value = serde_json::from_str(&v4).unwrap();
    assert_eq!(parsed["version"], FORMAT_VERSION);
    assert!(
        !v4.contains("\"blob_encoding\""),
        "no inline payloads remain in features"
    );
    let again = load_document(&v4).unwrap().document;
    assert_eq!(again.sources, doc.sources);
    assert_eq!(again.document.id, doc.document.id);
}

#[test]
fn migrated_document_rebuilds_the_same_body_as_the_v3_file() {
    let tree = FeatureTree {
        features: vec![import_feature(
            "Import cube",
            ImportedBodyParams::embedded("cube.step", CUBE_STEP),
        )],
        ..Default::default()
    };
    let v3 = v3_envelope(
        serde_json::json!([{ "id": "t", "name": "P", "kind": { "type": "Part", "features": serde_json::to_value(&tree).unwrap() } }]),
        "t",
    );
    // v3 → engine (single-tree API inlines the payload back)
    let (tree_v3, _) = load_project(&v3).unwrap();
    // v3 → v4 file → engine
    let v4 = save_document(&load_document(&v3).unwrap().document);
    let (tree_v4, _) = load_project(&v4).unwrap();
    let Operation::ImportedBody { params } = &tree_v4.features[0].operation else {
        panic!()
    };
    assert!(
        params.source_id.is_some() && params.blob.is_some(),
        "inlined for storeless consumers"
    );

    let count = |tree: FeatureTree| {
        let mut kernel = MockKernel::new();
        let mut engine = Engine::new();
        engine.tree = tree;
        engine.rebuild_from_scratch(&mut kernel);
        assert!(engine.errors.is_empty(), "{:?}", engine.errors);
        engine
            .feature_results
            .values()
            .map(|r| r.provenance.created.len())
            .sum::<usize>()
    };
    assert_eq!(count(tree_v3), count(tree_v4));
}

#[test]
fn a_corrupt_legacy_payload_stays_inline_and_is_reported() {
    let mut params = ImportedBodyParams::embedded("bad.step", "x");
    params.blob = Some("!!!not base64!!!".to_string());
    let tree = FeatureTree {
        features: vec![import_feature("Bad", params)],
        ..Default::default()
    };
    let v3 = v3_envelope(
        serde_json::json!([{ "id": "t", "name": "P", "kind": { "type": "Part", "features": serde_json::to_value(&tree).unwrap() } }]),
        "t",
    );
    let loaded = load_document(&v3).unwrap();
    assert!(loaded
        .warnings
        .iter()
        .any(|w| w.contains("does not decode")));
    assert!(loaded.document.sources.is_empty());
    let Operation::ImportedBody { params } =
        &loaded.document.tabs[0].features().unwrap().features[0].operation
    else {
        panic!()
    };
    assert!(
        params.blob.is_some(),
        "left in place, will fail loudly at rebuild as before"
    );
}

// ------------------------------------------------- single-tree API shims

#[test]
fn save_project_lifts_and_load_project_inlines_step_payloads() {
    let tree = FeatureTree {
        features: vec![import_feature(
            "Import cube",
            ImportedBodyParams::embedded("cube.step", CUBE_STEP),
        )],
        ..Default::default()
    };
    let meta = ProjectMetadata::new("Single").with_display_unit("mm");
    let json = save_project(&tree, &meta);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["version"], FORMAT_VERSION);
    assert_eq!(parsed["sources"].as_array().unwrap().len(), 1);
    assert!(
        parsed["tabs"][0]["kind"]["features"]["features"][0]["operation"]["params"]
            .get("blob")
            .is_none()
    );

    let (back, meta_back) = load_project(&json).unwrap();
    assert_eq!(meta_back.name, "Single");
    assert_eq!(meta_back.display_unit.as_deref(), Some("mm"));
    let Operation::ImportedBody { params } = &back.features[0].operation else {
        panic!()
    };
    let text = step_import::decode_step_blob(
        params.blob_encoding.as_deref().unwrap(),
        params.blob.as_deref().unwrap(),
    )
    .unwrap();
    assert_eq!(
        text, CUBE_STEP,
        "STEP text byte-identical through lift + inline"
    );
}

#[test]
fn a_hash_mismatching_embed_is_not_inlined_and_is_reported() {
    let mut doc = WaffleDocument::new("Stale");
    let mut src = SourceEntry::embedded("cube.step", SourceKind::Step, CUBE_STEP);
    src.content_hash = Some(git_blob_sha1(b"something else"));
    let id = src.id;
    doc.sources.push(src);
    doc.tabs[0]
        .features_mut()
        .unwrap()
        .features
        .push(import_feature(
            "Import",
            ImportedBodyParams::from_source("cube.step", id),
        ));
    let json = save_document(&doc);

    let loaded = load_document(&json).unwrap();
    assert!(
        loaded
            .warnings
            .iter()
            .any(|w| w.contains("EmbedHashMismatch")),
        "{:?}",
        loaded.warnings
    );
    assert!(
        loaded.document.embedded_contents().is_empty(),
        "mismatching embed is not usable content"
    );
    let (tree, _) = load_project(&json).unwrap();
    let Operation::ImportedBody { params } = &tree.features[0].operation else {
        panic!()
    };
    assert!(
        params.blob.is_none(),
        "not inlined — the feature will fail SourceUnavailable, loudly"
    );
}

#[test]
fn embedded_contents_and_unshareable_sources() {
    let mut doc = WaffleDocument::new("Mix");
    let packed = SourceEntry::embedded("cube.step", SourceKind::Step, CUBE_STEP);
    let linked = SourceEntry::linked(
        "far.waffle",
        SourceKind::Waffle,
        Locator::git_commit("https://gitlab.com/g/r", "far.waffle", &"a".repeat(40)),
    );
    let local = SourceEntry::linked(
        "mine.waffle",
        SourceKind::Waffle,
        Locator::Local {
            provider: "local".into(),
            doc_id: "AbCdEfGh".into(),
        },
    );
    let mut local_packed = local.clone();
    local_packed.id = Uuid::new_v4();
    local_packed.pack = Some(true);
    doc.sources
        .extend([packed.clone(), linked.clone(), local.clone(), local_packed]);

    let contents = doc.embedded_contents();
    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0].0, packed.id);
    assert_eq!(contents[0].1, CUBE_STEP);

    let unshareable: Vec<Uuid> = doc.unshareable_sources().iter().map(|s| s.id).collect();
    assert_eq!(unshareable, vec![local.id], "only the unpacked Local link");
    assert!(!linked.effective_pack() && linked.locator.git_host() == Some(GitHost::Gitlab));
    assert!(matches!(&linked.locator, Locator::Git { git_ref, .. } if git_ref.is_pinned()));
}

// -------------------------------------------------------- locator rules

#[test]
fn locator_validation_flags_bad_refs_paths_and_schemes() {
    let bad = |loc: Locator| SourceEntry::linked("x", SourceKind::Waffle, loc).validate();
    assert!(bad(Locator::git_branch(
        "http://github.com/a/b",
        "p.waffle",
        "main"
    ))
    .iter()
    .any(|w| w.contains("https://")));
    assert!(bad(Locator::git_branch(
        "https://github.com/a/b",
        "/abs.waffle",
        "main"
    ))
    .iter()
    .any(|w| w.contains("leading `/`")));
    assert!(bad(Locator::git_branch(
        "https://github.com/a/b",
        "../up.waffle",
        "main"
    ))
    .iter()
    .any(|w| w.contains("`..`")));
    assert!(bad(Locator::git_branch(
        "https://github.com/a/b",
        "p.waffle",
        "-bad"
    ))
    .iter()
    .any(|w| w.contains("valid git ref")));
    assert!(bad(Locator::git_branch(
        "https://github.com/a/b",
        "p.waffle",
        "a..b"
    ))
    .iter()
    .any(|w| w.contains("valid git ref")));
    assert!(bad(Locator::git_branch(
        "https://github.com/a/b",
        "p.waffle",
        "has space"
    ))
    .iter()
    .any(|w| w.contains("git refuses")));
    assert!(bad(Locator::git_commit(
        "https://github.com/a/b",
        "p.waffle",
        "notahash"
    ))
    .iter()
    .any(|w| w.contains("hex")));
    assert!(bad(Locator::Url {
        url: "ftp://x".into()
    })
    .iter()
    .any(|w| w.contains("https://")));
    assert!(bad(Locator::Relative {
        path: "/etc".into()
    })
    .iter()
    .any(|w| w.contains("`/`")));
    // Good ones are silent.
    assert!(bad(Locator::git_branch(
        "https://gitlab.com/a/b.git",
        "dir/p.waffle",
        "release/1.2"
    ))
    .is_empty());
    assert!(bad(Locator::git_commit(
        "https://codeberg.org/a/b",
        "p.waffle",
        &"0123456789abcdef".repeat(4)[..40]
    ))
    .is_empty());
    assert!(bad(Locator::Relative {
        path: "../parts/p.waffle".into()
    })
    .is_empty());
    assert!(bad(Locator::Embedded).is_empty());
    // Ref kinds and tags serialize as designed.
    let v = serde_json::to_value(GitRef::Tag { name: "v1".into() }).unwrap();
    assert_eq!(v, serde_json::json!({ "type": "Tag", "name": "v1" }));
    let v = serde_json::to_value(Locator::Embedded).unwrap();
    assert_eq!(v, serde_json::json!({ "type": "Embedded" }));
}

#[test]
fn source_entry_set_content_respects_pack_policy() {
    let mut linked = SourceEntry::linked(
        "p.waffle",
        SourceKind::Waffle,
        Locator::git_branch("https://github.com/a/b", "p.waffle", "main"),
    );
    linked.set_content("hello\n");
    assert_eq!(
        linked.content_hash.as_deref(),
        Some("git-blob-sha1:ce013625030ba8dba906f756967f9e9ca394464a")
    );
    assert!(
        linked.embed.is_none(),
        "linked sources are not packed by default"
    );
    linked.pack = Some(true);
    linked.set_content("hello\n");
    assert_eq!(
        linked.embed.as_ref().map(Embed::decode),
        Some(Ok("hello\n".to_string()))
    );
}

#[test]
fn a_v4_file_that_needs_a_newer_reader_is_refused_cleanly() {
    let mut parsed: serde_json::Value =
        serde_json::from_str(&save_document(&WaffleDocument::new("New"))).unwrap();
    parsed["min_reader_version"] = serde_json::json!(FORMAT_VERSION + 1);
    assert!(matches!(
        load_document(&parsed.to_string()),
        Err(LoadError::FutureVersion { .. })
    ));
    // A Tab in a Part-only document is a UUID, and WaffleDocument::new is valid.
    assert!(Uuid::parse_str(&WaffleDocument::new("x").tabs[0].id).is_ok());
    let _ = Tab::part("p", FeatureTree::new());
}

/// JavaScript writes `2020-01-02T03:04:05.000Z`; the Rust writer must hand
/// it back byte-identical (chrono's default dropped the `.000`), while a
/// nanosecond timestamp keeps its precision.
#[test]
fn timestamps_round_trip_in_javascript_form() {
    let mut parsed: serde_json::Value =
        serde_json::from_str(&save_document(&WaffleDocument::new("T"))).unwrap();
    parsed["document"]["created"] = serde_json::json!("2020-01-02T03:04:05.000Z");
    parsed["document"]["modified"] = serde_json::json!("2026-07-05T01:21:04.049Z");
    let doc = load_document(&parsed.to_string()).unwrap().document;
    let out: serde_json::Value = serde_json::from_str(&save_document(&doc)).unwrap();
    assert_eq!(out["document"]["created"], "2020-01-02T03:04:05.000Z");
    assert_eq!(out["document"]["modified"], "2026-07-05T01:21:04.049Z");

    parsed["document"]["created"] = serde_json::json!("2026-07-05T20:52:55.175125183Z");
    let doc = load_document(&parsed.to_string()).unwrap().document;
    let out: serde_json::Value = serde_json::from_str(&save_document(&doc)).unwrap();
    assert_eq!(out["document"]["created"], "2026-07-05T20:52:55.175125183Z");

    // The legacy single-tree metadata uses the same form.
    let meta: ProjectMetadata = serde_json::from_value(serde_json::json!({
        "name": "P", "created": "2020-01-02T03:04:05.000Z", "modified": "2020-01-02T03:04:05.500Z"
    }))
    .unwrap();
    let v = serde_json::to_value(&meta).unwrap();
    assert_eq!(v["created"], "2020-01-02T03:04:05.000Z");
    assert_eq!(v["modified"], "2020-01-02T03:04:05.500Z");
}

// ------------------------------------------ §2.9 profile addressing by ids

/// A minimal Part tab whose sketch is one unit square (lines 10–13) and whose
/// extrude addresses the loop by entity-id set with a deliberately wrong
/// `profile_index`, the way a writer that never ran the solver would.
fn agent_authored_square_extrude(profile_index: usize, ids: serde_json::Value) -> String {
    let sketch_feature = Uuid::new_v4();
    let extrude_feature = Uuid::new_v4();
    let plane = serde_json::to_value(waffle_types::GeomRef {
        kind: waffle_types::TopoKind::Face,
        anchor: waffle_types::Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: waffle_types::Selector::Role {
            role: waffle_types::roles::Role::EndCapPositive,
            index: 0,
        },
        policy: waffle_types::ResolvePolicy::Strict,
        scope: None,
    })
    .unwrap();
    let tree = serde_json::json!({
        "features": [
            { "id": sketch_feature, "name": "Sketch", "suppressed": false, "references": [],
              "operation": { "type": "Sketch", "sketch": {
                  "id": Uuid::new_v4(), "plane": plane,
                  "entities": [
                      { "type": "Point", "id": 1, "x": 0.0, "y": 0.0 },
                      { "type": "Point", "id": 2, "x": 0.01, "y": 0.0 },
                      { "type": "Point", "id": 3, "x": 0.01, "y": 0.01 },
                      { "type": "Point", "id": 4, "x": 0.0, "y": 0.01 },
                      { "type": "Line", "id": 10, "start_id": 1, "end_id": 2 },
                      { "type": "Line", "id": 11, "start_id": 2, "end_id": 3 },
                      { "type": "Line", "id": 12, "start_id": 3, "end_id": 4 },
                      { "type": "Line", "id": 13, "start_id": 4, "end_id": 1 }
                  ],
                  "constraints": [],
                  "solve_status": { "type": "FullyConstrained" }
              } } },
            { "id": extrude_feature, "name": "Extrude", "suppressed": false, "references": [],
              "operation": { "type": "Extrude", "params": {
                  "sketch_id": sketch_feature, "profile_index": profile_index,
                  "profile_entity_ids": ids,
                  "depth": 0.005, "direction": null, "symmetric": false, "cut": false,
                  "target_body": null, "combine": { "type": "NewBody" }
              } } }
        ]
    });
    let mut doc: serde_json::Value =
        serde_json::from_str(&save_document(&WaffleDocument::new("Agent"))).unwrap();
    doc["tabs"][0]["kind"]["features"] = tree;
    doc.to_string()
}

fn rebuild_errors(json: &str) -> Vec<(Uuid, String)> {
    let (tree, _) = load_project(json).unwrap();
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    engine.tree = tree;
    engine.rebuild_from_scratch(&mut kernel);
    engine.errors
}

#[test]
fn profile_entity_ids_survive_the_writer_and_drive_the_rebuild() {
    // The loop named by its edge set resolves although profile_index is bogus …
    let json = agent_authored_square_extrude(7, serde_json::json!([13, 10, 12, 11]));
    assert!(
        rebuild_errors(&json).is_empty(),
        "{:?}",
        rebuild_errors(&json)
    );

    // … the field is preserved verbatim by a load → save cycle …
    let saved = save_document(&load_document(&json).unwrap().document);
    let out: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        out["tabs"][0]["kind"]["features"]["features"][1]["operation"]["params"]
            ["profile_entity_ids"],
        serde_json::json!([13, 10, 12, 11])
    );
    assert!(rebuild_errors(&saved).is_empty());

    // … and a set no loop has is a loud per-feature error, not a silent
    // fallback to profile_index (which IS valid here).
    let bad = agent_authored_square_extrude(0, serde_json::json!([10, 11, 12]));
    let errors = rebuild_errors(&bad);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0]
            .1
            .contains("no profile is bounded by entities [10, 11, 12]"),
        "{}",
        errors[0].1
    );
}

// ---------------------------------------- §2.10 solve_status is optional

#[test]
fn a_sketch_without_solve_status_loads_and_is_solved_by_the_rebuild() {
    // Author the square by id set (no solver run) and strip `solve_status`
    // entirely — the shape a script emits.
    let json = agent_authored_square_extrude(0, serde_json::json!([10, 11, 12, 13]));
    let mut doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    let sketch = &mut doc["tabs"][0]["kind"]["features"]["features"][0]["operation"]["sketch"];
    assert!(sketch
        .as_object_mut()
        .unwrap()
        .remove("solve_status")
        .is_some());
    let json = doc.to_string();

    let loaded = load_document(&json).unwrap();
    let tree = loaded.document.tabs[0].features().unwrap();
    let Operation::Sketch { sketch } = &tree.features[0].operation else {
        panic!()
    };
    assert!(matches!(
        sketch.solve_status,
        waffle_types::SolveStatus::Unsolved
    ));

    // The engine solves it during rebuild; the status the writer then emits
    // is the solved one, never `Unsolved`.
    let (tree, _) = load_project(&json).unwrap();
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    engine.tree = tree;
    engine.rebuild_from_scratch(&mut kernel);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let Operation::Sketch { sketch } = &engine.tree.features[0].operation else {
        panic!()
    };
    assert!(
        matches!(
            sketch.solve_status,
            waffle_types::SolveStatus::UnderConstrained { .. }
                | waffle_types::SolveStatus::FullyConstrained
        ),
        "{:?}",
        sketch.solve_status
    );
    assert_eq!(sketch.solved_profiles.len(), 1);
    assert_eq!(engine.feature_results.len(), 2, "sketch + extrude");
}

// ------------------------------------ Phase 1b: opaque unknown operations

#[test]
fn unknown_operation_is_preserved_verbatim_reported_and_fails_its_rebuild_loudly() {
    let loft = serde_json::json!({
        "type": "Loft",
        "params": { "sections": [[10, 11, 12, 13]], "ruled": true },
        "x-tool": "a-newer-build"
    });
    let json = agent_authored_square_extrude(0, serde_json::json!([10, 11, 12, 13]));
    let mut doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    let features = doc["tabs"][0]["kind"]["features"]["features"]
        .as_array_mut()
        .unwrap();
    let loft_id = Uuid::new_v4();
    features.insert(
        1,
        serde_json::json!({ "id": loft_id, "name": "Loft 1", "suppressed": false,
                            "references": [], "operation": loft }),
    );
    let json = doc.to_string();

    // Loads, with a warning naming the feature and its kind.
    let loaded = load_document(&json).unwrap();
    assert!(
        loaded
            .warnings
            .iter()
            .any(|w| w.contains("Loft 1") && w.contains("unknown operation kind `Loft`")),
        "{:?}",
        loaded.warnings
    );
    let tree = loaded.document.tabs[0].features().unwrap();
    assert_eq!(tree.features.len(), 3);
    assert!(matches!(tree.features[1].operation, Operation::Unknown(_)));

    // Re-emitted verbatim by the writer.
    let saved = save_document(&loaded.document);
    let out: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        out["tabs"][0]["kind"]["features"]["features"][1]["operation"],
        loft
    );
    assert_eq!(
        out["tabs"][0]["kind"]["features"]["features"][1]["id"],
        serde_json::json!(loft_id)
    );

    // The rebuild errors THAT feature and builds the others.
    let (tree, _) = load_project(&saved).unwrap();
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    engine.tree = tree;
    engine.rebuild_from_scratch(&mut kernel);
    assert_eq!(engine.errors.len(), 1, "{:?}", engine.errors);
    assert_eq!(engine.errors[0].0, loft_id);
    assert!(
        engine.errors[0]
            .1
            .contains("operation kind `Loft` is not supported"),
        "{}",
        engine.errors[0].1
    );
    assert_eq!(engine.feature_results.len(), 2, "sketch + extrude built");
}

#[test]
fn malformed_known_operation_is_still_a_parse_error() {
    let json = agent_authored_square_extrude(0, serde_json::json!([10, 11, 12, 13]));
    let mut doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    doc["tabs"][0]["kind"]["features"]["features"][1]["operation"] =
        serde_json::json!({ "type": "Extrude", "params": 42 });
    assert!(
        matches!(load_document(&doc.to_string()), Err(LoadError::ParseError(m)) if m.contains("operation `Extrude`"))
    );
    doc["tabs"][0]["kind"]["features"]["features"][1]["operation"] =
        serde_json::json!({ "params": {} });
    assert!(
        matches!(load_document(&doc.to_string()), Err(LoadError::ParseError(m)) if m.contains("string `type`"))
    );
}

// ---------------------------------------- Phase 2: fork-time source rebase

#[test]
fn join_repo_path_normalizes_and_refuses_escapes() {
    use file_format::join_repo_path;
    assert_eq!(
        join_repo_path("brackets/bracket.waffle", "../fasteners/bolt.waffle").as_deref(),
        Some("fasteners/bolt.waffle")
    );
    assert_eq!(
        join_repo_path("brackets/bracket.waffle", "./sub/plate.waffle").as_deref(),
        Some("brackets/sub/plate.waffle")
    );
    assert_eq!(
        join_repo_path("top.waffle", "other.waffle").as_deref(),
        Some("other.waffle")
    );
    assert_eq!(
        join_repo_path("brackets/bracket.waffle", "../../escape.waffle"),
        None
    );
    assert_eq!(join_repo_path("top.waffle", "../x.waffle"), None);
    assert_eq!(join_repo_path("a/b.waffle", ".."), None);
}

#[test]
fn rebase_pins_relative_sources_to_the_opened_commit_and_keeps_ids() {
    use file_format::rebase_relative_sources;
    let sha = "9fceb02a".repeat(5);
    let at = chrono::DateTime::parse_from_rfc3339("2026-09-08T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut entries = vec![
        SourceEntry::linked(
            "bolt.waffle",
            SourceKind::Waffle,
            Locator::Relative {
                path: "../fasteners/bolt.waffle".into(),
            },
        ),
        SourceEntry::linked(
            "escape.waffle",
            SourceKind::Waffle,
            Locator::Relative {
                path: "../../escape.waffle".into(),
            },
        ),
        SourceEntry::linked(
            "other.waffle",
            SourceKind::Waffle,
            Locator::git_branch("https://github.com/x/y", "o.waffle", "dev"),
        ),
        SourceEntry::embedded("cube.step", SourceKind::Step, CUBE_STEP),
    ];
    let ids: Vec<Uuid> = entries.iter().map(|e| e.id).collect();
    let base = Locator::Git {
        remote: "https://github.com/acme/parts.git".into(),
        path: "brackets/bracket.waffle".into(),
        git_ref: GitRef::Branch {
            name: "main".into(),
        },
        host: Some(GitHost::Github),
    };

    let rewritten = rebase_relative_sources(&mut entries, &base, &sha.to_uppercase(), at);
    assert_eq!(
        rewritten,
        vec![ids[0]],
        "only the resolvable Relative entry"
    );

    assert_eq!(
        entries[0].locator,
        Locator::Git {
            remote: "https://github.com/acme/parts".into(),
            path: "fasteners/bolt.waffle".into(),
            git_ref: GitRef::Commit { sha: sha.clone() },
            host: Some(GitHost::Github),
        }
    );
    assert_eq!(
        entries[0]
            .resolved
            .as_ref()
            .map(|r| (r.commit.clone(), r.at)),
        Some((sha.clone(), at))
    );
    assert_eq!(entries[0].id, ids[0]);
    // An escaping relative path is left alone (still unresolvable, still reported).
    assert!(matches!(entries[1].locator, Locator::Relative { .. }));
    // Absolute and embedded entries are untouched.
    assert!(
        matches!(&entries[2].locator, Locator::Git { git_ref: GitRef::Branch { name }, .. } if name == "dev")
    );
    assert!(matches!(entries[3].locator, Locator::Embedded));

    // A non-git base rewrites nothing.
    let mut again = vec![SourceEntry::linked(
        "r",
        SourceKind::Waffle,
        Locator::Relative {
            path: "x.waffle".into(),
        },
    )];
    assert!(rebase_relative_sources(
        &mut again,
        &Locator::Url {
            url: "https://e.x/a.waffle".into()
        },
        &sha,
        at
    )
    .is_empty());
    assert!(matches!(again[0].locator, Locator::Relative { .. }));
}

/// The cached verifier refuses everything the uncached one refuses, and
/// answers the same bytes. The cache may never be a way for a corrupt
/// document to get past the check.
#[test]
fn the_cached_verifier_matches_the_uncached_one() {
    use file_format::SaveVerifier;

    let doc = WaffleDocument::new("Cached");
    let mut v = SaveVerifier::default();

    // Same answer as `save_document`, and stable when repeated: the second
    // save takes the cached path for every tab and must still produce the file.
    let first = v.save(&doc).expect("a healthy document saves");
    let second = v.save(&doc).expect("and saves again, from the cache");
    assert_eq!(first, save_document(&doc));
    assert_eq!(
        load_document(&second).unwrap().document.tabs.len(),
        doc.tabs.len()
    );

    // A document the loader would reject is still rejected, by a verifier
    // whose cache is warm with a HEALTHY version of the same document.
    let mut bad = doc.clone();
    bad.active_tab = "nope".to_string();
    assert!(
        matches!(v.save(&bad), Err(LoadError::ParseError(_))),
        "a warm cache does not excuse a broken document"
    );

    // And a tab whose own payload is unparseable is caught even though the
    // rest of the document is cached: a non-finite float serializes as `null`,
    // which is the corruption class this check exists for.
    let mut nan = doc.clone();
    let tab = nan.tabs.first_mut().expect("a new document has a tab");
    if let Some(tree) = tab.features_mut() {
        let mut p = feature_engine::types::DesignParameter::new("broken", "0");
        p.value = f64::NAN;
        tree.parameters.push(p);
        assert!(
            matches!(v.save(&nan), Err(LoadError::ParseError(_))),
            "a NaN in a changed tab is refused"
        );
    }
}
