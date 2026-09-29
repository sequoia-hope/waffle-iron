//! The board STEP as the component-model supply (`specs/kicad_board_link.md`
//! §3 C2–C4, §5 O5/O6/O9, increment C3): `ImportKicad` / `LinkKicadFromLocator`
//! with a `board_step` register a `Step` source whose first-level products
//! become the component Parts — each modelled footprint an instance of
//! `PartRef{source_id: <Step>, tab_id: <product>}` placed where KiCad's own
//! export puts it; a footprint the STEP does not model is a placeholder,
//! loudly. The assembly evaluates on the real kernel, the document
//! round-trips offline, and a changed STEP re-syncs the board (R5).

use feature_engine::assembly::PartRef;
use feature_engine::kicad::{RULE_FOOTPRINT, X_DERIVED};
use kernel_v2::KernelV2Adapter;
use serde_json::{json, Value};
use uuid::Uuid;
use wasm_bridge::dispatch::source_statuses;
use wasm_bridge::messages::*;
use wasm_bridge::tools::execute_tool;
use wasm_bridge::*;

const PCB: &str = include_str!("../../kicad-pcb/tests/fixtures/two_sided.kicad_pcb");
/// `kicad-cli pcb export step` (KiCad 7.0.11) of PCB with our own 10 mm
/// cube standing in for every model.
const STEP: &str = include_str!("../../kicad-pcb/tests/fixtures/two_sided.step");

fn model_updated(reply: EngineToUi) -> (Vec<String>, Vec<String>, Vec<String>) {
    let EngineToUi::ModelUpdated {
        errors,
        feature_errors,
        warnings,
        ..
    } = reply
    else {
        panic!("expected ModelUpdated, got {reply:?}");
    };
    (
        errors.iter().map(|e| format!("{e:?}")).collect(),
        feature_errors.iter().map(|e| format!("{e:?}")).collect(),
        warnings,
    )
}

fn import_with_step(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    step: &str,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let reply = dispatch(
        state,
        UiToEngine::ImportKicad {
            file_name: "two_sided.kicad_pcb".to_string(),
            data: PCB.to_string(),
            board_step: Some(BoardStepData {
                file_name: "two_sided.step".to_string(),
                data: step.to_string(),
                locator: None,
                resolved_commit: None,
            }),
        },
        kernel,
    );
    model_updated(reply)
}

fn open_assembly(state: &mut EngineState, kernel: &mut KernelV2Adapter) -> AssemblyStatus {
    let tab = state.kicad_boards[0].assembly_tab.clone();
    let reply = dispatch(state, UiToEngine::OpenAssembly { tab_id: tab }, kernel);
    let EngineToUi::ModelUpdated {
        assembly: Some(status),
        errors,
        ..
    } = reply
    else {
        panic!("expected an evaluated assembly, got {reply:?}");
    };
    assert!(errors.is_empty(), "{errors:?}");
    status
}

#[test]
fn board_step_products_become_the_component_parts_placed_as_kicad_placed_them() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (errors, feature_errors, warnings) = import_with_step(&mut state, &mut kernel, STEP);
    assert!(
        errors.is_empty() && feature_errors.is_empty(),
        "{errors:?} {feature_errors:?}"
    );

    // Two sources: the board and its STEP, both embedded, both available.
    let sources = source_statuses(&state);
    assert_eq!(sources.len(), 2);
    let step = sources
        .iter()
        .find(|s| s.kind == "Step")
        .expect("a Step source");
    assert_eq!(step.name, "two_sided.step");
    assert!(step.available && step.pack);
    let step_id = step.id;
    let rec = &state.kicad_boards[0];
    assert_eq!(rec.board_step, Some(step_id));
    // Only the un-modelled shape gets a placeholder Part.
    assert_eq!(
        rec.placeholder_tabs.keys().collect::<Vec<_>>(),
        ["Waffle:NoModel"]
    );

    let assembly = state.session.assembly(&rec.assembly_tab).unwrap().clone();
    let by_name = |n: &str| assembly.instances.iter().find(|i| i.name == n).unwrap();
    let r1 = by_name("R1");
    assert_eq!(
        r1.source,
        PartRef {
            source_id: Some(step_id),
            tab_id: "box".to_string()
        }
    );
    assert_eq!(by_name("C1").source.tab_id, "box");
    assert_eq!(r1.extra[X_DERIVED]["rule"], RULE_FOOTPRINT);
    // Placed where KiCad put it: on the copper top plus KiCad 7's 0.05 mm
    // standoff, rotated 37° about +Z.
    assert_eq!(r1.transform.translation_m, [20e-3, -15e-3, 1.65e-3]);
    let half = 37f64.to_radians() / 2.0;
    assert!((r1.transform.rotation_quat[2] - half.sin()).abs() < 1e-12);
    // The un-modelled footprint is a placeholder, said out loud, once.
    let u1 = by_name("U1");
    assert_eq!(u1.source.tab_id, rec.placeholder_tabs["Waffle:NoModel"]);
    let missing: Vec<&String> = warnings
        .iter()
        .filter(|w| w.contains("ComponentModelMissing"))
        .collect();
    assert_eq!(missing.len(), 1, "{warnings:?}");
    assert!(missing[0].contains("U1"));
    assert!(
        !warnings.iter().any(|w| w.contains("out of date")),
        "{warnings:?}"
    );

    // The assembly evaluates on the real kernel: the STEP product is built
    // once as a Part (mesh-backed), both occurrences place it, no errors.
    let status = open_assembly(&mut state, &mut kernel);
    assert!(status.errors.is_empty(), "{:?}", status.errors);
    assert_eq!(status.placements.len(), 4, "board, R1, C1, U1");
    assert!(
        status.parts.contains(&r1.source),
        "the STEP product is a built part: {:?}",
        status.parts
    );
    let p = &status.placements[&r1.id];
    assert!((p.translation_m[2] - 1.65e-3).abs() < 1e-12);

    // ── Round trip, offline (O9): the STEP is embedded, so a fresh engine
    // with no host rebuilds the same document and evaluates the same.
    let EngineToUi::SaveReady { json_data: saved } =
        dispatch(&mut state, UiToEngine::SaveDocument, &mut kernel)
    else {
        panic!()
    };
    let mut fresh = EngineState::new();
    let (errors, _, _) = model_updated(dispatch(
        &mut fresh,
        UiToEngine::LoadProject {
            data: saved.clone(),
        },
        &mut kernel,
    ));
    assert!(errors.is_empty(), "{errors:?}");
    let EngineToUi::SaveReady {
        json_data: saved_again,
    } = dispatch(&mut fresh, UiToEngine::SaveDocument, &mut kernel)
    else {
        panic!()
    };
    let strip = |s: &str| -> Value {
        let mut v: Value = serde_json::from_str(s).unwrap();
        if let Some(d) = v.get_mut("document").and_then(|d| d.as_object_mut()) {
            d.remove("modified");
        }
        v
    };
    assert_eq!(strip(&saved), strip(&saved_again));
    assert_eq!(fresh.kicad_boards[0].board_step, Some(step_id));
    let status = open_assembly(&mut fresh, &mut kernel);
    assert!(status.errors.is_empty(), "{:?}", status.errors);
    assert_eq!(status.placements.len(), 4);
}

/// R5: new bytes for the board STEP re-sync every board that names it —
/// here C1's occurrence is renamed away, so C1 falls back to a placeholder
/// (a new placeholder tab for its shape) while R1 keeps its id and model;
/// the same bytes are a no-op.
#[test]
fn a_changed_board_step_resyncs_the_boards_it_models() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    import_with_step(&mut state, &mut kernel, STEP);
    let rec = state.kicad_boards[0].clone();
    let step_id = rec.board_step.unwrap();
    let before = state.session.assembly(&rec.assembly_tab).unwrap().clone();
    let r1_id = before.instances.iter().find(|i| i.name == "R1").unwrap().id;
    let c1_id = before.instances.iter().find(|i| i.name == "C1").unwrap().id;

    // Same bytes: nothing regenerated.
    let (errors, _, _) = model_updated(dispatch(
        &mut state,
        UiToEngine::ProvideSource {
            source_id: step_id,
            data: STEP.to_string(),
            resolved_commit: None,
        },
        &mut kernel,
    ));
    assert!(errors.is_empty(), "{errors:?}");
    let unchanged = state.session.assembly(&rec.assembly_tab).unwrap();
    assert_eq!(
        serde_json::to_value(&unchanged.instances).unwrap(),
        serde_json::to_value(&before.instances).unwrap()
    );

    // A STEP where C1's occurrence is called C9 (no such footprint).
    let renamed = STEP.replace("'C1'", "'C9'");
    assert_ne!(renamed, STEP);
    let (errors, _, warnings) = model_updated(dispatch(
        &mut state,
        UiToEngine::ProvideSource {
            source_id: step_id,
            data: renamed,
            resolved_commit: None,
        },
        &mut kernel,
    ));
    assert!(errors.is_empty(), "{errors:?}");
    let after = state.session.assembly(&rec.assembly_tab).unwrap().clone();
    let r1 = after.instances.iter().find(|i| i.name == "R1").unwrap();
    let c1 = after.instances.iter().find(|i| i.name == "C1").unwrap();
    assert_eq!(r1.id, r1_id);
    assert_eq!(c1.id, c1_id, "the instance survives the model's loss");
    assert_eq!(r1.source.source_id, Some(step_id));
    assert_eq!(c1.source.source_id, None, "C1 is a placeholder now");
    let rec2 = &state.kicad_boards[0];
    assert_eq!(
        rec2.placeholder_tabs.keys().collect::<Vec<_>>(),
        ["Waffle:Box_Back", "Waffle:NoModel"]
    );
    assert_eq!(c1.source.tab_id, rec2.placeholder_tabs["Waffle:Box_Back"]);
    // On the bottom face by the board's own composition (no standoff).
    assert_eq!(c1.transform.translation_m[2], 0.0);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("ComponentModelMissing") && w.contains("C1")),
        "{warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("C9") && w.contains("ignored")),
        "the stray occurrence is named: {warnings:?}"
    );
}

/// A board STEP that does not read still lands as a source, says why, and
/// every component is a placeholder (spec §6).
#[test]
fn an_unreadable_board_step_is_loud_and_leaves_placeholders() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (errors, _, warnings) = import_with_step(&mut state, &mut kernel, "not a STEP file");
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        warnings.iter().any(|w| w.contains("BoardStepUnusable")),
        "{warnings:?}"
    );
    let sources = source_statuses(&state);
    assert_eq!(sources.len(), 2);
    let rec = &state.kicad_boards[0];
    assert_eq!(rec.board_step, None, "no models ⇒ the tree names no STEP");
    assert_eq!(rec.placeholder_tabs.len(), 3);
    let assembly = state.session.assembly(&rec.assembly_tab).unwrap();
    assert!(assembly
        .instances
        .iter()
        .all(|i| i.source.source_id.is_none()));
}

/// A `board_step` with a `Local` locator is refused like the board itself.
#[test]
fn a_local_board_step_locator_is_refused() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let reply = dispatch(
        &mut state,
        UiToEngine::ImportKicad {
            file_name: "two_sided.kicad_pcb".to_string(),
            data: PCB.to_string(),
            board_step: Some(BoardStepData {
                file_name: "two_sided.step".to_string(),
                data: STEP.to_string(),
                locator: Some(file_format::Locator::Local {
                    provider: "opfs".to_string(),
                    doc_id: "two_sided.step".to_string(),
                }),
                resolved_commit: None,
            }),
        },
        &mut kernel,
    );
    assert!(
        matches!(reply, EngineToUi::Error { .. }),
        "expected a refusal, got {reply:?}"
    );
    assert!(state.kicad_boards.is_empty());
    assert!(source_statuses(&state).is_empty());
}

/// The agent's `kicad_link` takes the STEP alongside the board.
#[test]
fn the_kicad_link_tool_takes_a_board_step() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let result = execute_tool(
        &mut state,
        &mut kernel,
        "kicad_link",
        &json!({
            "file_name": "two_sided.kicad_pcb",
            "pcb_text": PCB,
            "step_file_name": "two_sided.step",
            "step_text": STEP,
        }),
        None,
    );
    assert!(!result.is_error, "{result:?}");
    let out = result.structured_content;
    let step_id: Uuid = out["board_step_source_id"]
        .as_str()
        .expect("the STEP source id is answered")
        .parse()
        .unwrap();
    assert_eq!(state.kicad_boards[0].board_step, Some(step_id));
    assert_eq!(out["placeholder_tabs"].as_object().unwrap().len(), 1);
    assert_eq!(out["component_count"], 3);
}

#[test]
fn a_step_source_lists_its_products_as_parts_for_the_instance_chooser() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    import_with_step(&mut state, &mut kernel, STEP);
    let step_id = state.kicad_boards[0].board_step.unwrap();
    let tabs = assembly_view::source_tabs(step_id, &state.engine.sources).unwrap();
    let names: Vec<&str> = tabs.iter().map(|(id, _, _)| id.as_str()).collect();
    assert_eq!(names, ["two_sided PCB", "box"]);
    assert!(tabs.iter().all(|(_, _, kind)| kind == "Part"));
    assert!(source_statuses(&state)
        .iter()
        .any(|s| s.kind == "Step" && s.id == step_id));
}
