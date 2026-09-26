//! Re-sync of a linked KiCad board (`specs/kicad_board_link.md` §3 R1–R6,
//! oracle O7, increment C5): `ProvideSource` with new bytes on a `KicadPcb`
//! source regenerates every feature, instance and connector the board
//! derived — by rule, reconciled by id — and leaves the user's own content
//! exactly as it was. Same bytes are a no-op (R6). The record a hover
//! reads is rebuilt from the tabs, so a reload answers the same (O9's
//! offline half; the network never reaches the bridge).

use std::collections::HashMap;

use feature_engine::assembly::{AssemblyTree, AxialAnchor, Frame, Mate, MateConnector, MateKind};
use feature_engine::kicad::{
    RULE_BOARD_EXTRUDE, RULE_BOARD_OUTLINE, RULE_FOOTPRINT, RULE_MOUNTING_HOLE, X_DERIVED,
};
use feature_engine::types::{
    CombineMode, DepthMode, ExtrudeParams, Feature, Operation, ProvenanceOrigin,
};
use kernel_v2::KernelV2Adapter;
use serde_json::{json, Map, Value};
use uuid::Uuid;
use waffle_types::kernel::KernelIntrospect;
use waffle_types::{
    Anchor, ClosedProfile, GeomRef, OutputKey, ResolvePolicy, Role, Selector, Sketch, SketchEntity,
    SolveStatus, TopoKind,
};
use wasm_bridge::messages::*;
use wasm_bridge::*;

const A: &str = include_str!("../../kicad-pcb/tests/fixtures/rect_v8.kicad_pcb");
const A_PRIME: &str = include_str!("../../kicad-pcb/tests/fixtures/rect_v8_resync.kicad_pcb");

const R1_UUID: &str = "0a1b2c3d-0000-4000-8000-000000000001";
const C1_UUID: &str = "0a1b2c3d-0000-4000-8000-000000000002";
const H1_UUID: &str = "0a1b2c3d-0000-4000-8000-000000000003";
const R2_UUID: &str = "0a1b2c3d-0000-4000-8000-000000000004";
const U1_UUID: &str = "0a1b2c3d-0000-4000-8000-000000000005";

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

fn link(state: &mut EngineState, kernel: &mut KernelV2Adapter) -> Uuid {
    let reply = dispatch(
        state,
        UiToEngine::ImportKicad {
            file_name: "rect_v8.kicad_pcb".to_string(),
            data: A.to_string(),
        },
        kernel,
    );
    let (errors, feature_errors, _) = model_updated(reply);
    assert!(errors.is_empty() && feature_errors.is_empty());
    state.kicad_boards[0].source_id
}

fn provide(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    source_id: Uuid,
    data: &str,
) -> EngineToUi {
    dispatch(
        state,
        UiToEngine::ProvideSource {
            source_id,
            data: data.to_string(),
            resolved_commit: None,
        },
        kernel,
    )
}

/// The feature of the LIVE tree derived by `rule` (the last one).
fn derived_feature(state: &EngineState, rule: &str) -> Feature {
    let tree = &state.engine.tree;
    tree.features
        .iter()
        .rfind(|f| {
            matches!(
                tree.provenance.get(&f.id).map(|p| &p.origin),
                Some(ProvenanceOrigin::Derived { rule: r, .. }) if r == rule
            )
        })
        .cloned()
        .unwrap_or_else(|| panic!("no feature with rule {rule}"))
}

fn volume(state: &EngineState, kernel: &KernelV2Adapter, feature_id: Uuid) -> f64 {
    let result = &state.engine.feature_results[&feature_id];
    let (_, body) = result
        .outputs
        .iter()
        .find(|(k, _)| matches!(k, OutputKey::Main))
        .expect("Main output");
    kernel.solid_volume(&body.handle).expect("exact volume")
}

/// A user sketch ON THE BOARD'S TOP FACE: a 10 × 10 mm square, resolved
/// through the board extrude's positive end cap (persistent naming).
fn sketch_on_top_face(board_extrude: Uuid, thickness_m: f64) -> Operation {
    let corners = [
        (0.005, -0.025),
        (0.015, -0.025),
        (0.015, -0.015),
        (0.005, -0.015),
    ];
    let mut entities = Vec::new();
    let mut solved_positions = HashMap::new();
    for (i, (x, y)) in corners.iter().enumerate() {
        let id = i as u32 + 1;
        entities.push(SketchEntity::Point {
            id,
            x: *x,
            y: *y,
            construction: false,
        });
        solved_positions.insert(id, (*x, *y));
    }
    for i in 0..4u32 {
        entities.push(SketchEntity::Line {
            id: 10 + i,
            start_id: i + 1,
            end_id: (i + 1) % 4 + 1,
            construction: false,
        });
    }
    Operation::Sketch {
        sketch: Sketch {
            id: Uuid::new_v4(),
            plane: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::FeatureOutput {
                    feature_id: board_extrude,
                    output_key: OutputKey::Main,
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::Strict,
                scope: None,
            },
            plane_origin: [0.0, 0.0, thickness_m],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: Some([1.0, 0.0, 0.0]),
            entities,
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions,
            projected: Vec::new(),
            solved_profiles: vec![ClosedProfile {
                entity_ids: vec![10, 11, 12, 13],
                is_outer: true,
                vertex_ids: vec![1, 2, 3, 4],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
        },
    }
}

fn enclosure(sketch_id: Uuid) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            profile_entity_ids: Some(vec![10, 11, 12, 13]),
            depth: 0.004,
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
    }
}

fn add_feature(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    operation: Operation,
) -> Uuid {
    let before: Vec<Uuid> = state.engine.tree.features.iter().map(|f| f.id).collect();
    let reply = dispatch(
        state,
        UiToEngine::AddFeature {
            operation,
            provenance: None,
        },
        kernel,
    );
    let (errors, feature_errors, _) = model_updated(reply);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(feature_errors.is_empty(), "{feature_errors:?}");
    state
        .engine
        .tree
        .features
        .iter()
        .map(|f| f.id)
        .find(|id| !before.contains(id))
        .expect("a feature was added")
}

fn frame_connector(name: &str, instance: Uuid, origin: [f64; 3]) -> MateConnector {
    MateConnector {
        id: Uuid::new_v4(),
        name: name.to_string(),
        instance_path: vec![instance],
        geom_ref: None,
        part_connector: None,
        frame: Frame {
            origin,
            z_axis: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
        },
        anchor: AxialAnchor::Middle,
        flip_z: false,
        rotation_deg: 0.0,
        offset_m: [0.0; 3],
        extra: Map::new(),
    }
}

fn fastened(name: &str, a: Uuid, b: Uuid) -> Mate {
    Mate {
        id: Uuid::new_v4(),
        name: name.to_string(),
        kind: MateKind::Fastened {
            flip: false,
            rotation_deg: 0.0,
        },
        connectors: [a, b],
        suppressed: false,
        extra: Map::new(),
    }
}

fn instance_by_key<'a>(
    asm: &'a AssemblyTree,
    key: &str,
) -> Option<&'a feature_engine::assembly::Instance> {
    asm.instances
        .iter()
        .find(|i| i.external_key.as_deref() == Some(key))
}

/// Oracle O7, end to end on the real kernel.
#[test]
fn resync_regenerates_the_derived_content_and_keeps_the_users() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let source_id = link(&mut state, &mut kernel);
    let rec = state.kicad_boards[0].clone();

    // ── The user builds on the board ────────────────────────────────────
    let board_sketch = derived_feature(&state, RULE_BOARD_OUTLINE);
    let board_extrude = derived_feature(&state, RULE_BOARD_EXTRUDE);
    let Operation::Sketch {
        sketch: board_sketch_before,
    } = &board_sketch.operation
    else {
        panic!()
    };
    let inner_sketch_id = board_sketch_before.id;
    assert!((volume(&state, &kernel, board_extrude.id) - 50.0 * 30.0 * 1.6e-9).abs() < 1e-18);

    let user_sketch = add_feature(
        &mut state,
        &mut kernel,
        sketch_on_top_face(board_extrude.id, 1.6e-3),
    );
    let user_extrude = add_feature(&mut state, &mut kernel, enclosure(user_sketch));
    let user_features_before: Vec<Feature> = state
        .engine
        .tree
        .features
        .iter()
        .filter(|f| f.id == user_sketch || f.id == user_extrude)
        .cloned()
        .collect();
    assert_eq!(user_features_before.len(), 2);
    let enclosure_volume = volume(&state, &kernel, user_extrude);
    assert!(
        (enclosure_volume - 0.01 * 0.01 * 0.004).abs() < 1e-15,
        "{enclosure_volume}"
    );

    // Assembly: connectors on R1 and the board, a Fastened mate between them;
    // a second mate from C1 to the H1 mounting-hole connector; C1 renamed.
    let mut asm = state.session.assembly(&rec.assembly_tab).unwrap().clone();
    let r1_id = instance_by_key(&asm, R1_UUID).unwrap().id;
    let c1_id = instance_by_key(&asm, C1_UUID).unwrap().id;
    let h1_conn = asm
        .connectors
        .iter()
        .find(|c| c.extra[X_DERIVED]["rule"] == RULE_MOUNTING_HOLE)
        .unwrap()
        .id;
    let on_r1 = frame_connector("on R1", r1_id, [0.0, 0.0, 0.0]);
    let on_board = frame_connector("on board", rec.board_instance, [0.02, -0.015, 1.6e-3]);
    let on_c1 = frame_connector("on C1", c1_id, [0.0, 0.0, 0.0]);
    let mate_r1 = fastened("R1 to board", on_board.id, on_r1.id);
    let mate_h1 = fastened("C1 to H1", h1_conn, on_c1.id);
    let (on_r1_id, on_board_id, on_c1_id) = (on_r1.id, on_board.id, on_c1.id);
    let (mate_r1_id, mate_h1_id) = (mate_r1.id, mate_h1.id);
    asm.connectors.extend([on_r1, on_board, on_c1]);
    asm.mates.extend([mate_r1, mate_h1]);
    let c1 = asm.instances.iter_mut().find(|i| i.id == c1_id).unwrap();
    c1.name = "bulk cap".to_string();
    c1.suppressed = true;
    let reply = dispatch(
        &mut state,
        UiToEngine::EditAssembly {
            tab_id: rec.assembly_tab.clone(),
            assembly: asm,
        },
        &mut kernel,
    );
    model_updated(reply);
    let asm_before = state.session.assembly(&rec.assembly_tab).unwrap().clone();
    let user_instances_before: Vec<Uuid> = asm_before.instances.iter().map(|i| i.id).collect();
    let tabs_before = state.session.tabs().len();

    // ── A′ arrives (R1 moved, C1 value, R2 + U1 added, H1 deleted, 60 × 30) ─
    let reply = provide(&mut state, &mut kernel, source_id, A_PRIME);
    let (errors, feature_errors, warnings) = model_updated(reply);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(feature_errors.is_empty(), "{feature_errors:?}");

    // R1: the board regenerated IN PLACE — same feature ids, same sketch id,
    // new geometry — and the user's features re-resolve through them.
    let tree = &state.engine.tree;
    assert_eq!(state.session.active_tab_id(), rec.board_tab);
    let board_sketch_after = derived_feature(&state, RULE_BOARD_OUTLINE);
    let board_extrude_after = derived_feature(&state, RULE_BOARD_EXTRUDE);
    assert_eq!(board_sketch_after.id, board_sketch.id);
    assert_eq!(board_extrude_after.id, board_extrude.id);
    let Operation::Sketch { sketch: s } = &board_sketch_after.operation else {
        panic!()
    };
    assert_eq!(s.id, inner_sketch_id);
    assert!(
        (volume(&state, &kernel, board_extrude.id) - 60.0 * 30.0 * 1.6e-9).abs() < 1e-18,
        "the board took the new outline"
    );
    let ids: Vec<Uuid> = tree.features.iter().map(|f| f.id).collect();
    assert_eq!(
        ids,
        vec![board_sketch.id, board_extrude.id, user_sketch, user_extrude],
        "derived features first, the user's after, nothing else"
    );
    let user_features_after: Vec<Feature> = tree
        .features
        .iter()
        .filter(|f| f.id == user_sketch || f.id == user_extrude)
        .cloned()
        .collect();
    assert_eq!(
        serde_json::to_value(&user_features_after).unwrap(),
        serde_json::to_value(&user_features_before).unwrap(),
        "the user's features are byte-identical"
    );
    assert!(
        (volume(&state, &kernel, user_extrude) - enclosure_volume).abs() < 1e-18,
        "the enclosure still builds on the new board"
    );
    assert!(!tree.provenance.contains_key(&user_sketch));
    assert!(matches!(
        tree.provenance.get(&board_extrude.id).map(|p| &p.origin),
        Some(ProvenanceOrigin::Derived { rule, .. }) if rule == RULE_BOARD_EXTRUDE
    ));

    // R2–R4 on the assembly.
    let asm = state.session.assembly(&rec.assembly_tab).unwrap().clone();
    let r1 = instance_by_key(&asm, R1_UUID).expect("R1 kept");
    assert_eq!(r1.id, r1_id, "same instance id (inv. 4)");
    assert_eq!(r1.name, "R1");
    let t = r1.transform.translation_m;
    assert!(
        (t[0] - 0.025).abs() < 1e-15 && (t[1] + 0.012).abs() < 1e-15,
        "R1 moved: {t:?}"
    );
    let h = 22.5f64.to_radians();
    assert!((r1.transform.rotation_quat[2] - h.sin()).abs() < 1e-15);
    let c1 = instance_by_key(&asm, C1_UUID).expect("C1 kept");
    assert_eq!(c1.id, c1_id);
    assert_eq!(c1.name, "bulk cap", "the user's rename survives (R3)");
    assert!(c1.suppressed, "suppressed survives (R3)");
    assert_eq!(
        c1.extra[X_DERIVED]["name"], "C1",
        "the rule's own name is kept beside it"
    );
    let r2 = instance_by_key(&asm, R2_UUID).expect("R2 added (R2)");
    assert_eq!(r2.name, "R2");
    assert_eq!(
        r2.source.tab_id, r1.source.tab_id,
        "R2 shares R1's placeholder Part"
    );
    let u1 = instance_by_key(&asm, U1_UUID).expect("U1 added");
    assert_eq!(u1.name, "U1");
    assert!(
        !user_instances_before.contains(&u1.source.tab_id.parse().unwrap_or(Uuid::nil())),
        "placeholder tab is new"
    );
    assert_eq!(
        state.session.tabs().len(),
        tabs_before + 1,
        "one new placeholder tab (SOIC-8); the mounting hole never had one"
    );
    assert!(
        state
            .session
            .tabs()
            .iter()
            .any(|t| t.name == "SOIC-8_3.9x4.9mm_P1.27mm placeholder"),
        "{:?}",
        state.session.tabs()
    );
    assert!(
        instance_by_key(&asm, H1_UUID).is_none(),
        "H1 had no instance before either"
    );
    assert_eq!(
        asm.instances.len(),
        5,
        "board, R1, C1, R2, U1: {:?}",
        asm.instances.iter().map(|i| &i.name).collect::<Vec<_>>()
    );
    let board = instance_by_key(&asm, "board").unwrap();
    assert_eq!(
        board.id, rec.board_instance,
        "the grounded board instance keeps its id"
    );
    assert_eq!(
        asm.instances.iter().map(|i| i.id).collect::<Vec<_>>()[..3],
        user_instances_before[..3],
        "the retained instances keep their order"
    );

    // Connectors: the user's three kept verbatim, H1's hole gone (R4).
    assert!(asm.connectors.iter().all(|c| c.id != h1_conn));
    for id in [on_r1_id, on_board_id, on_c1_id] {
        let before = asm_before.connectors.iter().find(|c| c.id == id).unwrap();
        let after = asm
            .connectors
            .iter()
            .find(|c| c.id == id)
            .expect("user connector kept");
        assert_eq!(
            serde_json::to_value(after).unwrap(),
            serde_json::to_value(before).unwrap()
        );
    }
    assert_eq!(asm.connectors.len(), 3);
    // Mates: both left in place; the one on H1's connector is reported.
    assert!(asm.mates.iter().any(|m| m.id == mate_r1_id));
    assert!(
        asm.mates.iter().any(|m| m.id == mate_h1_id),
        "the dangling mate is NOT deleted"
    );
    assert!(
        warnings.iter().any(|w| w.starts_with("MateTargetGone")
            && w.contains("C1 to H1")
            && w.contains("board")),
        "{warnings:?}"
    );
    assert_eq!(
        warnings
            .iter()
            .filter(|w| w.starts_with("MateTargetGone"))
            .count(),
        1,
        "the R1 mate is intact: {warnings:?}"
    );

    // The record is a pure function of the new bytes: C1's value is new,
    // the count is 4, the keys are the reconciled instance ids.
    let rec2 = state.kicad_boards[0].clone();
    assert_eq!(rec2.board.footprint_count, 4);
    assert_eq!(rec2.board.rev, "B");
    assert_eq!(rec2.components[&c1_id].value, "220n");
    assert_eq!(rec2.components[&r1_id].reference, "R1");
    assert_eq!(rec2.components.len(), 4);
    assert_eq!(rec2.board_instance, rec.board_instance);
    assert_eq!(
        rec2.placeholder_tabs.len(),
        3,
        "{:?}",
        rec2.placeholder_tabs
    );
    assert_eq!(
        rec2.placeholder_tabs["Resistor_SMD:R_0603_1608Metric"],
        rec.placeholder_tabs["Resistor_SMD:R_0603_1608Metric"],
        "the R_0603 placeholder tab is the same tab"
    );

    // The assembly evaluates: the dangling mate is loud there, nothing else.
    let reply = dispatch(
        &mut state,
        UiToEngine::OpenAssembly {
            tab_id: rec.assembly_tab.clone(),
        },
        &mut kernel,
    );
    let EngineToUi::ModelUpdated {
        assembly: Some(status),
        ..
    } = reply
    else {
        panic!("{reply:?}")
    };
    // C1 is suppressed (four placed); the dangling mate is the one loud
    // error of the evaluation, named.
    assert_eq!(status.placements.len(), 4, "{:?}", status.errors);
    assert_eq!(status.errors.len(), 1, "{:?}", status.errors);
    assert!(status.errors[0].contains("C1 to H1"), "{:?}", status.errors);
    let p = &status.placements[&u1.id];
    assert!(
        (p.translation_m[0] - 0.015).abs() < 1e-12 && (p.translation_m[1] + 0.022).abs() < 1e-12
    );

    // ── Round trip: save, reload into a fresh engine, save again ─────────
    let EngineToUi::SaveReady { json_data: saved } =
        dispatch(&mut state, UiToEngine::SaveDocument, &mut kernel)
    else {
        panic!()
    };
    let mut fresh = EngineState::new();
    let reply = dispatch(
        &mut fresh,
        UiToEngine::LoadProject {
            data: saved.clone(),
        },
        &mut kernel,
    );
    let (errors, _, _) = model_updated(reply);
    assert!(errors.is_empty(), "{errors:?}");
    let EngineToUi::SaveReady {
        json_data: saved_again,
    } = dispatch(&mut fresh, UiToEngine::SaveDocument, &mut kernel)
    else {
        panic!()
    };
    let strip = |s: &str| -> Value {
        let mut v: Value = serde_json::from_str(s).unwrap();
        // The only field a reload legitimately rewrites.
        if let Some(d) = v.get_mut("document").and_then(|d| d.as_object_mut()) {
            d.remove("modified");
        }
        v
    };
    assert_eq!(
        strip(&saved),
        strip(&saved_again),
        "the document round-trips identically"
    );

    // The hover record is rebuilt from the tabs on load (inv. 6): the
    // reloaded engine answers `QueryEntityMeta` with the new values,
    // offline — no adapter, no network, only the embed.
    assert_eq!(fresh.kicad_boards.len(), 1);
    let rec3 = &fresh.kicad_boards[0];
    assert_eq!(rec3.board, rec2.board);
    assert_eq!(rec3.components, rec2.components);
    assert_eq!(rec3.placeholder_tabs, rec2.placeholder_tabs);
    assert_eq!(rec3.board_instance, rec2.board_instance);
    let reply = dispatch(
        &mut fresh,
        UiToEngine::QueryEntityMeta {
            body_id: None,
            instance_path: Some(vec![c1_id]),
        },
        &mut kernel,
    );
    let EngineToUi::EntityMeta {
        component: Some(c), ..
    } = reply
    else {
        panic!("{reply:?}")
    };
    assert_eq!(c.value, "220n");
    assert_eq!(c.reference, "C1");
}

/// R6: the same bytes change nothing — not an id, not a feature, not the
/// tab list — and R5's hash bookkeeping still records the commit.
#[test]
fn the_same_bytes_are_a_no_op() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let source_id = link(&mut state, &mut kernel);
    let rec = state.kicad_boards[0].clone();
    let tree_before = serde_json::to_value(&state.engine.tree).unwrap();
    let asm_before =
        serde_json::to_value(state.session.assembly(&rec.assembly_tab).unwrap()).unwrap();
    let tabs_before = state.session.tabs();

    let reply = dispatch(
        &mut state,
        UiToEngine::ProvideSource {
            source_id,
            data: A.to_string(),
            resolved_commit: Some("ABCDEF0123456789abcdef0123456789abcdef01".to_string()),
        },
        &mut kernel,
    );
    let (errors, feature_errors, _) = model_updated(reply);
    assert!(errors.is_empty() && feature_errors.is_empty());
    assert_eq!(
        serde_json::to_value(&state.engine.tree).unwrap(),
        tree_before
    );
    assert_eq!(
        serde_json::to_value(state.session.assembly(&rec.assembly_tab).unwrap()).unwrap(),
        asm_before
    );
    assert_eq!(state.session.tabs(), tabs_before);
    let entry = state.sources.iter().find(|s| s.id == source_id).unwrap();
    assert_eq!(
        entry.resolved.as_ref().map(|r| r.commit.as_str()),
        Some("abcdef0123456789abcdef0123456789abcdef01")
    );
    assert_eq!(state.kicad_boards[0].components, rec.components);
}

/// A board that no longer reads is refused and the document keeps what it
/// had: hash, content, tabs, record (§6).
#[test]
fn unreadable_new_bytes_are_refused_and_nothing_changes() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let source_id = link(&mut state, &mut kernel);
    let hash_before = state.sources[0].content_hash.clone();
    let tree_before = serde_json::to_value(&state.engine.tree).unwrap();

    let reply = provide(
        &mut state,
        &mut kernel,
        source_id,
        "(kicad_pcb (version 20240108)",
    );
    let EngineToUi::Error { message, .. } = reply else {
        panic!("expected a refusal, got {reply:?}")
    };
    assert!(message.contains("rect_v8.kicad_pcb"), "{message}");
    assert_eq!(state.sources[0].content_hash, hash_before);
    assert_eq!(
        state.engine.sources.text(source_id).as_deref(),
        Some(A),
        "the store keeps the readable bytes"
    );
    assert_eq!(
        serde_json::to_value(&state.engine.tree).unwrap(),
        tree_before
    );
    assert_eq!(state.kicad_boards[0].board.footprint_count, 3);
}

/// Re-sync while the ASSEMBLY tab is open: the screen shows the reconciled
/// assembly, re-evaluated, and the board tab (inactive) took the new
/// outline.
#[test]
fn resync_with_the_assembly_open_re_evaluates_it() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let source_id = link(&mut state, &mut kernel);
    let rec = state.kicad_boards[0].clone();
    dispatch(
        &mut state,
        UiToEngine::OpenAssembly {
            tab_id: rec.assembly_tab.clone(),
        },
        &mut kernel,
    );
    let reply = provide(&mut state, &mut kernel, source_id, A_PRIME);
    let EngineToUi::ModelUpdated {
        assembly: Some(status),
        errors,
        ..
    } = reply
    else {
        panic!("{reply:?}")
    };
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(status.placements.len(), 5);
    assert_eq!(state.session.active_tab_id(), rec.assembly_tab);
    // The board tab (not open) has the 60 × 30 outline: its sketch's points
    // reach x = 60 mm.
    let board = state
        .session
        .tab(&rec.board_tab)
        .unwrap()
        .features()
        .unwrap();
    let Operation::Sketch { sketch } = &board.features[0].operation else {
        panic!()
    };
    assert!(sketch
        .entities
        .iter()
        .any(|e| matches!(e, SketchEntity::Point { x, .. } if (*x - 0.06).abs() < 1e-15)));
}

/// The record's derived-name stamp: what the rule minted, on every
/// derived instance and connector from the first link on.
#[test]
fn derived_instances_carry_the_rule_name_from_the_link() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    link(&mut state, &mut kernel);
    let rec = state.kicad_boards[0].clone();
    let asm = state.session.assembly(&rec.assembly_tab).unwrap();
    for i in &asm.instances {
        assert_eq!(i.extra[X_DERIVED]["name"], json!(i.name), "{}", i.name);
    }
    for c in &asm.connectors {
        assert_eq!(c.extra[X_DERIVED]["name"], json!(c.name));
    }
    assert!(asm
        .instances
        .iter()
        .any(|i| i.extra[X_DERIVED]["rule"] == RULE_FOOTPRINT));
}

/// Spec §2.4: derived instances and connectors refuse every edit but
/// `name` and `suppressed` (`DerivedFeatureReadOnly`) — what a re-sync
/// would overwrite is not for the agent to set.
#[test]
fn derived_instance_and_connector_edits_beyond_name_are_refused() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    link(&mut state, &mut kernel);
    let rec = state.kicad_boards[0].clone();
    dispatch(
        &mut state,
        UiToEngine::OpenAssembly {
            tab_id: rec.assembly_tab.clone(),
        },
        &mut kernel,
    );
    let asm = state.session.assembly(&rec.assembly_tab).unwrap().clone();
    let r1 = instance_by_key(&asm, R1_UUID).unwrap().id;
    let hole = asm.connectors[0].id;
    let context = json!({ "agent_name": "resync-test" });
    let mut run = |name: &str, args: Value| {
        execute_tool(&mut state, &mut kernel, name, &args, Some(&context))
    };

    let r = run(
        "instance_edit",
        json!({ "instance_id": r1, "transform": { "translation_m": [0.0, 0.0, 0.0] } }),
    );
    assert!(r.is_error, "{r:?}");
    assert_eq!(
        r.structured_content["error"]["code"],
        "DerivedFeatureReadOnly"
    );
    assert_eq!(
        r.structured_content["error"]["details"]["refused"],
        json!(["transform"])
    );
    let r = run("instance_edit", json!({ "instance_id": r1, "fixed": true }));
    assert_eq!(
        r.structured_content["error"]["code"],
        "DerivedFeatureReadOnly"
    );
    let r = run(
        "instance_edit",
        json!({ "instance_id": r1, "name": "pull-up", "suppressed": true }),
    );
    assert!(!r.is_error, "{r:?}");

    let r = run(
        "connector_edit",
        json!({ "connector_id": hole, "flip_z": true }),
    );
    assert_eq!(
        r.structured_content["error"]["code"],
        "DerivedFeatureReadOnly"
    );
    let r = run(
        "connector_edit",
        json!({ "connector_id": hole, "offset_m": [0.0, 0.0, 0.001] }),
    );
    assert_eq!(
        r.structured_content["error"]["code"],
        "DerivedFeatureReadOnly"
    );
    let r = run(
        "connector_edit",
        json!({ "connector_id": hole, "name": "mount A" }),
    );
    assert!(!r.is_error, "{r:?}");

    let asm = state.session.assembly(&rec.assembly_tab).unwrap();
    let r1 = instance_by_key(asm, R1_UUID).unwrap();
    assert_eq!(r1.name, "pull-up");
    assert!(r1.suppressed);
    assert_eq!(asm.connectors[0].name, "mount A");
    assert!(!asm.connectors[0].flip_z);
}
