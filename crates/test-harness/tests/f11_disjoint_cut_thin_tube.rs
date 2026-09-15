//! Agent-link failure F11 (docs/notes/agent_bicycle_session_failures_2026_09_14.md):
//! a box Cut that does not even touch an annulus tube (OD 31.8, bore r 14 mm)
//! failed with `VertexOffSurface`, while the same Cut on a r 15 mm bore passed.
//!
//! Real kernel (kernel-v2) through raw `wasm_bridge::dispatch`.

use feature_engine::types::*;
use modeling_ops::KernelBundle;
use std::collections::HashMap;
use test_harness::helpers::{rect_profile, ProfileData};
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::{dispatch, EngineState, UiToEngine};

fn last_feature_id(state: &EngineState) -> Uuid {
    state.engine.tree.features.last().expect("a feature").id
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
            plane_normal: [0.0, 0.0, 1.0],
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

/// Tube along +Z (0.3 m), then a box Cut x ∈ [0.10, 0.30] — disjoint from the tube.
/// Returns the engine errors after the Cut.
fn disjoint_cut_errors(r_inner: f64) -> Vec<String> {
    cut_errors(r_inner, 0.10)
}

/// Tube along +Z (0.3 m), then a box Cut x ∈ [tool_x, tool_x + 0.2],
/// y ∈ [−0.05, 0.05], z ∈ [0.10, 0.20]. `tool_x = 0` notches away half the
/// tube wall; `tool_x = 0.10` misses the tube. Returns the engine errors.
fn cut_errors(r_inner: f64, tool_x: f64) -> Vec<String> {
    let mut state = EngineState::new();
    let mut kernel: Box<dyn KernelBundle> = Box::new(kernel_v2::adapter::KernelV2Adapter::new());
    let k = kernel.as_mut();

    let (profile, region) = annulus(0.0159, r_inner);
    let tube_sketch = finish_sketch(&mut state, k, [0.0; 3], profile);
    let tube = add(
        &mut state,
        k,
        extrude(tube_sketch, 0.3, CombineMode::NewBody, None, Some(region)),
    );
    assert!(
        state.engine.errors.is_empty(),
        "tube: {:?}",
        state.engine.errors
    );

    let tool_sketch = finish_sketch(
        &mut state,
        k,
        [0.0, 0.0, 0.10],
        rect_profile(tool_x, -0.05, 0.20, 0.10),
    );
    add(
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
    state
        .engine
        .errors
        .iter()
        .map(|e| format!("{e:?}"))
        .collect()
}

#[test]
fn f11_disjoint_cut_on_r15_bore_succeeds() {
    let errors = disjoint_cut_errors(0.015);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn f11_notch_cut_on_r15_bore_succeeds() {
    let errors = cut_errors(0.015, 0.0);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn f11_notch_cut_on_r14_bore_succeeds() {
    let errors = cut_errors(0.014, 0.0);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn f11_disjoint_cut_on_r14_bore_succeeds() {
    let errors = disjoint_cut_errors(0.014);
    assert!(errors.is_empty(), "{errors:?}");
}
