//! Agent-link failure F7 (docs/notes/agent_bicycle_session_failures_2026_09_14.md)
//! on the real kernel: once one Cut was in the tree, editing an unrelated
//! upstream sketch re-executed every boolean after it (> 120 s per edit in the
//! app). A feature whose inputs did not change must keep its result, and that
//! kept result must stay a valid body in the kernel-v2 arena: meshed, measured,
//! and usable as an operand by a later re-execution.
//!
//! Real kernel (kernel-v2) through raw `wasm_bridge::dispatch`, the path the
//! app and the agent link use.

use feature_engine::types::*;
use modeling_ops::KernelBundle;
use std::collections::HashMap;
use std::time::Instant;
use test_harness::helpers::{mesh_volume, rect_profile, ProfileData};
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::{dispatch, EngineState, UiToEngine};

fn new_kernel() -> Box<dyn KernelBundle> {
    Box::new(kernel_v2::adapter::KernelV2Adapter::new())
}

fn last_feature_id(state: &EngineState) -> Uuid {
    state
        .engine
        .tree
        .features
        .last()
        .expect("a feature was just added")
        .id
}

fn datum_plane() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

fn finish_sketch(
    state: &mut EngineState,
    kernel: &mut dyn KernelBundle,
    origin: [f64; 3],
    normal: [f64; 3],
    (entities, positions, profiles): ProfileData,
) -> Uuid {
    dispatch(
        state,
        UiToEngine::BeginSketch {
            plane: datum_plane(),
        },
        kernel,
    );
    for entity in entities {
        dispatch(state, UiToEngine::AddSketchEntity { entity }, kernel);
    }
    dispatch(
        state,
        UiToEngine::FinishSketch {
            provenance: None,
            solved_positions: positions,
            solved_profiles: profiles,
            plane_origin: origin,
            plane_normal: normal,
            entities: vec![],
            constraints: vec![],
            projected: vec![],
        },
        kernel,
    );
    last_feature_id(state)
}

/// Concentric circles r_outer (entity 1) and r_inner (entity 2) about point 0.
fn annulus(r_outer: f64, r_inner: f64) -> (ProfileData, Region) {
    let point = SketchEntity::Point {
        id: 0,
        x: 0.0,
        y: 0.0,
        construction: false,
    };
    let circle = |id, radius| SketchEntity::Circle {
        id,
        center_id: 0,
        radius,
        construction: false,
    };
    let entities = vec![point, circle(1, r_outer), circle(2, r_inner)];
    let positions = HashMap::from([(0, (0.0, 0.0))]);
    let profile = |id: u32, radius: f64| ClosedProfile {
        entity_ids: vec![id],
        is_outer: true,
        vertex_ids: vec![],
        circle: Some(CircleProfile {
            center_u: 0.0,
            center_v: 0.0,
            radius,
        }),
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let region = compute_regions(&entities, &positions, regions::DEFAULT_CHORD_TOLERANCE)
        .into_iter()
        .find(|r| !r.holes.is_empty())
        .expect("concentric circles yield an annulus region");
    (
        (
            entities,
            positions,
            vec![profile(1, r_outer), profile(2, r_inner)],
        ),
        region,
    )
}

fn extrude(
    sketch_id: Uuid,
    depth: f64,
    combine: CombineMode,
    targets: Option<Vec<GeomRef>>,
    region: Option<Region>,
) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            combine: Some(combine),
            targets,
            sketch_id,
            profile_index: 0,
            profile_entity_ids: None,
            depth,
            direction: None,
            symmetric: false,
            cut: matches!(combine, CombineMode::Cut),
            merge: false,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region,
            regions: Vec::new(),
            depth_expr: None,
        },
    }
}

fn add(state: &mut EngineState, kernel: &mut dyn KernelBundle, operation: Operation) -> Uuid {
    dispatch(
        state,
        UiToEngine::AddFeature {
            provenance: None,
            operation,
        },
        kernel,
    );
    last_feature_id(state)
}

fn main_target(feature_id: Uuid) -> GeomRef {
    GeomRef {
        kind: TopoKind::Solid,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

fn operation_of(state: &EngineState, id: Uuid) -> Operation {
    state
        .engine
        .tree
        .find_feature(id)
        .expect("feature in tree")
        .operation
        .clone()
}

/// Raw handle id and triangle count of each output body.
fn bodies(state: &EngineState, id: Uuid) -> Vec<(u64, usize)> {
    state
        .engine
        .get_result(id)
        .unwrap_or_else(|| panic!("no result for {id}: {:?}", state.engine.errors))
        .outputs
        .iter()
        .map(|(_, body)| {
            let triangles = body.mesh.as_ref().map_or(0, |m| m.indices.len() / 3);
            (body.handle.raw(), triangles)
        })
        .collect()
}

#[test]
fn editing_an_unrelated_sketch_keeps_the_cut_on_the_real_kernel() {
    let mut state = EngineState::new();
    let mut kernel = new_kernel();
    let k = kernel.as_mut();

    // Tube along +Z: OD 31.8 / wall 0.9, 0.3 m.
    let (profile, region) = annulus(0.0159, 0.015);
    let tube_sketch = finish_sketch(&mut state, k, [0.0; 3], [0.0, 0.0, 1.0], profile);
    let tube = add(
        &mut state,
        k,
        extrude(tube_sketch, 0.3, CombineMode::NewBody, None, Some(region)),
    );

    // An unrelated plate well away from the tube, added before the cut.
    let plate_sketch = finish_sketch(
        &mut state,
        k,
        [0.5, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        rect_profile(0.0, 0.0, 0.05, 0.05),
    );
    let plate = add(
        &mut state,
        k,
        extrude(plate_sketch, 0.006, CombineMode::NewBody, None, None),
    );

    // Box tool x ∈ [0, 0.1], y ∈ [−0.05, 0.05], z ∈ [0.10, 0.20]: notches away
    // half of the tube wall (its x = 0 face crosses the tube axis).
    let tool_sketch = finish_sketch(
        &mut state,
        k,
        [0.0, 0.0, 0.10],
        [0.0, 0.0, 1.0],
        rect_profile(0.0, -0.05, 0.10, 0.10),
    );
    let started = Instant::now();
    let cut = add(
        &mut state,
        k,
        extrude(
            tool_sketch,
            0.10,
            CombineMode::Cut,
            Some(vec![main_target(tube)]),
            None,
        ),
    );
    let add_cut = started.elapsed();
    // Natively `dispatch` does not tessellate; the WASM entry point does it
    // after every message.
    wasm_bridge::tessellation_runner::tessellate_engine(&mut state.engine, k);
    assert!(
        state.engine.errors.is_empty(),
        "tree must build: {:?}",
        state.engine.errors
    );
    // The notch removes half the wall over 0.1 of the tube's 0.3 m: 5/6 remains.
    let tube_handle = state.engine.get_result(tube).expect("tube").outputs[0]
        .1
        .handle
        .clone();
    let tube_volume = mesh_volume(&k.tessellate(&tube_handle, 0.0001).expect("tube mesh"));
    let cut_result = state.engine.get_result(cut).expect("cut");
    assert_eq!(cut_result.outputs.len(), 1, "the notched tube is one body");
    let cut_volume = mesh_volume(cut_result.outputs[0].1.mesh.as_ref().expect("cut mesh"));
    let ratio = cut_volume / tube_volume;
    assert!(
        (ratio - 5.0 / 6.0).abs() < 0.02,
        "the tool notches the tube: V_cut / V_tube = {ratio}"
    );
    let cut_before = bodies(&state, cut);
    let plate_before = bodies(&state, plate);
    assert!(
        cut_before.iter().all(|(_, tris)| *tris > 0),
        "cut is meshed"
    );

    // Edit the unrelated plate sketch (a bigger plate).
    let started = Instant::now();
    let Operation::Sketch { mut sketch } = operation_of(&state, plate_sketch) else {
        unreachable!("plate_sketch is a sketch");
    };
    let (entities, positions, profiles) = rect_profile(0.0, 0.0, 0.08, 0.08);
    sketch.entities = entities;
    sketch.solved_positions = positions;
    sketch.solved_profiles = profiles;
    dispatch(
        &mut state,
        UiToEngine::EditFeature {
            feature_id: plate_sketch,
            operation: Operation::Sketch { sketch },
            provenance: None,
        },
        k,
    );
    wasm_bridge::tessellation_runner::tessellate_engine(&mut state.engine, k);
    let edit_plate = started.elapsed();
    eprintln!("[F7] add cut {add_cut:?}; edit unrelated sketch {edit_plate:?}");

    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
    assert_ne!(
        bodies(&state, plate)[0].0,
        plate_before[0].0,
        "plate rebuilt"
    );
    assert_eq!(
        bodies(&state, cut),
        cut_before,
        "the Cut keeps its body and mesh: its inputs did not change"
    );
    assert!(state.engine.consumed_features.contains(&tube));

    // Now lengthen the tube: the Cut names it, so it re-executes against the
    // new tube in the same arena that holds the kept bodies.
    let mut tube_op = operation_of(&state, tube);
    if let Operation::Extrude { params } = &mut tube_op {
        params.depth = 0.35;
    }
    dispatch(
        &mut state,
        UiToEngine::EditFeature {
            feature_id: tube,
            operation: tube_op,
            provenance: None,
        },
        k,
    );
    wasm_bridge::tessellation_runner::tessellate_engine(&mut state.engine, k);
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
    let cut_after = bodies(&state, cut);
    assert_ne!(cut_after[0].0, cut_before[0].0, "the Cut re-executed");

    // The incremental result matches a rebuild of everything from scratch.
    let incremental: Vec<usize> = [tube, plate, cut]
        .iter()
        .flat_map(|id| bodies(&state, *id).into_iter().map(|(_, tris)| tris))
        .collect();
    state.engine.rebuild_from_scratch(k);
    wasm_bridge::tessellation_runner::tessellate_engine(&mut state.engine, k);
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
    let scratch: Vec<usize> = [tube, plate, cut]
        .iter()
        .flat_map(|id| bodies(&state, *id).into_iter().map(|(_, tris)| tris))
        .collect();
    assert_eq!(incremental, scratch);
}
