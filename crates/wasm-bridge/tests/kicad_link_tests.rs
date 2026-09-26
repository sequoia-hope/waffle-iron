//! `ImportKicad` / `LinkKicadFromLocator` at the bridge
//! (`specs/kicad_board_link.md` C2): the board lands as an EXACT solid on
//! the real kernel (oracle O2: area × thickness), with Derived provenance
//! on every feature, a `KicadPcb` source, a placeholder Part per footprint
//! shape, and an assembly whose instances are keyed by footprint uuid and
//! whose mounting holes are connectors (§2.3).

use std::f64::consts::PI;

use feature_engine::kicad::{
    RULE_BOARD_CUTOUT, RULE_BOARD_EXTRUDE, RULE_BOARD_OUTLINE, RULE_FOOTPRINT, X_DERIVED,
};
use feature_engine::types::{Operation, ProvenanceOrigin};
use file_format::{GitRef, Locator, SourceKind};
use kernel_v2::KernelV2Adapter;
use waffle_types::kernel::KernelIntrospect;
use waffle_types::OutputKey;
use wasm_bridge::messages::*;
use wasm_bridge::*;

const RECT_V8: &str = include_str!("../../kicad-pcb/tests/fixtures/rect_v8.kicad_pcb");
const HOLES_V6: &str = include_str!("../../kicad-pcb/tests/fixtures/holes_v6.kicad_pcb");
const ROUNDED_V9: &str = include_str!("../../kicad-pcb/tests/fixtures/rounded_v9.kicad_pcb");
const GAP: &str = include_str!("../../kicad-pcb/tests/fixtures/gap.kicad_pcb");
const LEGACY: &str = include_str!("../../kicad-pcb/tests/fixtures/legacy_v5.kicad_pcb");

const MM2: f64 = 1e-6;

fn import(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    name: &str,
    data: &str,
) -> EngineToUi {
    dispatch(
        state,
        UiToEngine::ImportKicad {
            file_name: name.to_string(),
            data: data.to_string(),
        },
        kernel,
    )
}

/// Volume of the live tree's body produced by the feature carrying `rule`
/// (the last one, for cutouts).
fn volume_of_rule(state: &EngineState, kernel: &KernelV2Adapter, rule: &str) -> f64 {
    let tree = &state.engine.tree;
    let feature_id = tree
        .features
        .iter()
        .rfind(|f| {
            matches!(
                tree.provenance.get(&f.id).map(|p| &p.origin),
                Some(ProvenanceOrigin::Derived { rule: r, .. }) if r == rule
            )
        })
        .map(|f| f.id)
        .unwrap_or_else(|| panic!("no feature with rule {rule}"));
    let result = &state.engine.feature_results[&feature_id];
    let (_, body) = result
        .outputs
        .iter()
        .find(|(k, _)| matches!(k, OutputKey::Main))
        .expect("Main output");
    kernel.solid_volume(&body.handle).expect("exact volume")
}

#[test]
fn rect_board_is_an_exact_solid_with_derived_provenance_and_a_kicad_source() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();

    let reply = import(&mut state, &mut kernel, "rect_v8.kicad_pcb", RECT_V8);
    let EngineToUi::ModelUpdated {
        errors,
        feature_errors,
        warnings,
        document,
        ..
    } = reply
    else {
        panic!("expected ModelUpdated, got {reply:?}");
    };
    assert!(errors.is_empty(), "{errors:?}");
    assert!(feature_errors.is_empty(), "{feature_errors:?}");
    // The reader's one warning (three unknown forms) is surfaced.
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("unknown top-level forms")),
        "{warnings:?}"
    );

    // Tabs: the original "Part 1", the board, two placeholders (R_0603 and
    // C_0805 — the mounting hole is board-only and gets none), the assembly.
    let doc = document.expect("document info");
    let names: Vec<(String, String)> = doc
        .tabs
        .iter()
        .map(|t| (t.name.clone(), t.kind.clone()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Part 1".to_string(), "Part".to_string()),
            ("rect_v8".to_string(), "Part".to_string()),
            (
                "R_0603_1608Metric placeholder".to_string(),
                "Part".to_string()
            ),
            (
                "C_0805_2012Metric placeholder".to_string(),
                "Part".to_string()
            ),
            ("rect_v8 assembly".to_string(), "Assembly".to_string()),
        ]
    );
    let board_tab = &doc.tabs[1].id;
    assert_eq!(&doc.active_tab, board_tab, "the Board tab is opened");

    // The live tree IS the board: outline sketch + extrude, both Derived.
    let tree = &state.engine.tree;
    assert_eq!(tree.features.len(), 2);
    assert!(matches!(
        tree.features[0].operation,
        Operation::Sketch { .. }
    ));
    assert!(matches!(
        tree.features[1].operation,
        Operation::Extrude { .. }
    ));
    let source_id = state.sources[0].id;
    for (f, rule) in tree
        .features
        .iter()
        .zip([RULE_BOARD_OUTLINE, RULE_BOARD_EXTRUDE])
    {
        assert_eq!(
            tree.provenance[&f.id].origin,
            ProvenanceOrigin::Derived {
                source_id,
                rule: rule.to_string()
            }
        );
    }

    // O2: 50 × 30 × 1.6 mm.
    let v = volume_of_rule(&state, &kernel, RULE_BOARD_EXTRUDE);
    assert!((v - 1500.0 * MM2 * 1.6e-3).abs() < 1e-18, "{v}");

    // The source: packed, embedded, KicadPcb.
    assert_eq!(state.sources.len(), 1);
    assert_eq!(state.sources[0].kind, SourceKind::KicadPcb);
    assert_eq!(state.sources[0].locator, Locator::Embedded);
    assert!(state.sources[0].effective_pack());
    assert_eq!(
        state.engine.sources.text(source_id).as_deref(),
        Some(RECT_V8)
    );

    // The record: board meta and the component records by instance id.
    let rec = &state.kicad_boards[0];
    assert_eq!(&rec.board_tab, board_tab);
    assert_eq!(rec.board.title, "Waffle test board");
    assert_eq!(rec.board.footprint_count, 3);
    assert_eq!(rec.board.net_count, 3);
    assert_eq!(rec.components.len(), 2);
    let refs: Vec<&str> = rec
        .components
        .values()
        .map(|c| c.reference.as_str())
        .collect();
    assert!(refs.contains(&"R1") && refs.contains(&"C1"), "{refs:?}");
    let r1 = rec
        .components
        .values()
        .find(|c| c.reference == "R1")
        .unwrap();
    assert_eq!(r1.value, "10k");
    assert_eq!(r1.side, "Front");
    assert_eq!(r1.pads[0].net_name, "GND");
}

#[test]
fn assembly_places_footprints_by_uuid_and_makes_mounting_holes_connectors() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    import(&mut state, &mut kernel, "rect_v8.kicad_pcb", RECT_V8);
    let assembly_tab = state.kicad_boards[0].assembly_tab.clone();

    let assembly = state.session.assembly(&assembly_tab).unwrap().clone();
    assert_eq!(assembly.instances.len(), 3);
    let board = &assembly.instances[0];
    assert!(board.fixed);
    assert_eq!(board.external_key.as_deref(), Some("board"));
    assert_eq!(board.source.tab_id, state.kicad_boards[0].board_tab);
    let r1 = assembly
        .instances
        .iter()
        .find(|i| i.name == "R1")
        .expect("R1 instance");
    assert_eq!(
        r1.external_key.as_deref(),
        Some("0a1b2c3d-0000-4000-8000-000000000001")
    );
    let t = r1.transform.translation_m;
    assert!(
        (t[0] - 0.020).abs() < 1e-15
            && (t[1] + 0.015).abs() < 1e-15
            && (t[2] - 1.6e-3).abs() < 1e-15,
        "{t:?}"
    );
    assert_eq!(r1.extra[X_DERIVED]["rule"], RULE_FOOTPRINT);
    let c1 = assembly.instances.iter().find(|i| i.name == "C1").unwrap();
    assert_eq!(c1.transform.translation_m, [0.035, -0.010, 0.0]);
    assert_eq!(
        r1.source.tab_id,
        state.kicad_boards[0].placeholder_tabs["Resistor_SMD:R_0603_1608Metric"]
    );

    // H1: np_thru_hole at (5, 5) mm ⇒ a connector on the board's top face.
    assert_eq!(assembly.connectors.len(), 1);
    let h = &assembly.connectors[0];
    assert_eq!(h.name, "H1 hole");
    assert_eq!(h.instance_path, vec![board.id]);
    assert_eq!(h.frame.origin, [5e-3, -5e-3, 1.6e-3]);
    assert!(assembly.validate().is_empty(), "{:?}", assembly.validate());

    // The assembly evaluates on the real kernel: placeholders build, the
    // placements are the derived transforms, no errors.
    let reply = dispatch(
        &mut state,
        UiToEngine::OpenAssembly {
            tab_id: assembly_tab,
        },
        &mut kernel,
    );
    let EngineToUi::ModelUpdated {
        assembly: Some(status),
        errors,
        ..
    } = reply
    else {
        panic!("expected an evaluated assembly, got {reply:?}");
    };
    assert!(errors.is_empty(), "{errors:?}");
    assert!(status.errors.is_empty(), "{:?}", status.errors);
    assert_eq!(status.placements.len(), 3);
    let p = &status.placements[&r1.id];
    assert!((p.translation_m[0] - 0.020).abs() < 1e-12);
    assert!((p.translation_m[2] - 1.6e-3).abs() < 1e-12);
}

#[test]
fn holes_become_cutouts_and_the_volume_loses_them() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let reply = import(&mut state, &mut kernel, "holes_v6.kicad_pcb", HOLES_V6);
    let EngineToUi::ModelUpdated {
        errors,
        feature_errors,
        ..
    } = reply
    else {
        panic!("{reply:?}");
    };
    assert!(
        errors.is_empty() && feature_errors.is_empty(),
        "{errors:?} {feature_errors:?}"
    );
    // outline sketch, board, cutouts sketch, two cuts
    assert_eq!(state.engine.tree.features.len(), 5);
    let expect = (40.0 * 25.0 - 2.0 * PI * 1.6 * 1.6) * MM2 * 1.2e-3;
    let v = volume_of_rule(&state, &kernel, RULE_BOARD_CUTOUT);
    assert!((v - expect).abs() < 1e-15, "{v} vs {expect}");
}

#[test]
fn rounded_board_with_slot_and_poly_cutout_is_exact() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let reply = import(&mut state, &mut kernel, "rounded_v9.kicad_pcb", ROUNDED_V9);
    let EngineToUi::ModelUpdated {
        errors,
        feature_errors,
        ..
    } = reply
    else {
        panic!("{reply:?}");
    };
    assert!(
        errors.is_empty() && feature_errors.is_empty(),
        "{errors:?} {feature_errors:?}"
    );
    let outer = 60.0 * 40.0 - (4.0 - PI) * 25.0;
    let slot = 4.0 * 2.0 + PI;
    let poly = 25.0 + 0.5 * PI * 2.5 * 2.5;
    let expect = (outer - slot - poly) * MM2 * 1.6e-3;
    let v = volume_of_rule(&state, &kernel, RULE_BOARD_CUTOUT);
    assert!((v - expect).abs() < 1e-14, "{v} vs {expect}");
    // The outer body alone, before the cuts, is the rounded rectangle.
    let vo = volume_of_rule(&state, &kernel, RULE_BOARD_EXTRUDE);
    assert!((vo - outer * MM2 * 1.6e-3).abs() < 1e-14, "{vo}");
}

#[test]
fn an_open_outline_lands_the_sketch_only_and_says_why() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let reply = import(&mut state, &mut kernel, "gap.kicad_pcb", GAP);
    let EngineToUi::ModelUpdated { warnings, .. } = reply else {
        panic!("{reply:?}");
    };
    assert_eq!(state.engine.tree.features.len(), 1, "sketch only");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains(RULE_BOARD_EXTRUDE) && w.contains("not closed")),
        "{warnings:?}"
    );
    // The source still landed; the assembly too (no footprints ⇒ board alone).
    assert_eq!(state.sources.len(), 1);
    let asm = state
        .session
        .assembly(&state.kicad_boards[0].assembly_tab)
        .unwrap();
    assert_eq!(asm.instances.len(), 1);
}

#[test]
fn a_refused_file_lands_nothing() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let reply = import(&mut state, &mut kernel, "legacy_v5.kicad_pcb", LEGACY);
    let EngineToUi::Error { message, .. } = reply else {
        panic!("{reply:?}");
    };
    assert!(message.contains("20171130"), "{message}");
    assert!(state.sources.is_empty());
    assert!(state.kicad_boards.is_empty());
    assert_eq!(state.session.tabs().len(), 1);
}

#[test]
fn linked_board_records_its_locator_commit_and_hash() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let sha = "9fceb02a9fceb02a9fceb02a9fceb02a9fceb02a";
    let reply = dispatch(
        &mut state,
        UiToEngine::LinkKicadFromLocator {
            file_name: "rect_v8.kicad_pcb".to_string(),
            locator: Locator::Git {
                remote: "https://github.com/acme/board".to_string(),
                path: "hw/rect_v8.kicad_pcb".to_string(),
                git_ref: GitRef::Commit {
                    sha: sha.to_string(),
                },
                host: None,
            },
            data: RECT_V8.to_string(),
            resolved_commit: Some(sha.to_uppercase()),
        },
        &mut kernel,
    );
    assert!(
        matches!(reply, EngineToUi::ModelUpdated { .. }),
        "{reply:?}"
    );
    let entry = &state.sources[0];
    assert_eq!(entry.kind, SourceKind::KicadPcb);
    assert!(!entry.effective_pack());
    assert_eq!(entry.resolved.as_ref().unwrap().commit, sha);
    assert_eq!(
        entry.content_hash.as_deref(),
        Some(file_format::git_blob_sha1(RECT_V8.as_bytes()).as_str())
    );
    let v = volume_of_rule(&state, &kernel, RULE_BOARD_EXTRUDE);
    assert!((v - 1500.0 * MM2 * 1.6e-3).abs() < 1e-18);
}

#[test]
fn a_local_locator_is_refused() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let reply = dispatch(
        &mut state,
        UiToEngine::LinkKicadFromLocator {
            file_name: "rect_v8.kicad_pcb".to_string(),
            locator: Locator::Local {
                provider: "local".to_string(),
                doc_id: "abc".to_string(),
            },
            data: RECT_V8.to_string(),
            resolved_commit: None,
        },
        &mut kernel,
    );
    assert!(matches!(reply, EngineToUi::Error { .. }), "{reply:?}");
    assert!(state.sources.is_empty());
}

#[test]
fn new_document_forgets_the_board_record() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    import(&mut state, &mut kernel, "rect_v8.kicad_pcb", RECT_V8);
    assert_eq!(state.kicad_boards.len(), 1);
    dispatch(&mut state, UiToEngine::NewDocument, &mut kernel);
    assert!(state.kicad_boards.is_empty());
}
