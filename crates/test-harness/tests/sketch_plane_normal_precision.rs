//! Agent-link failures F1/F2 (docs/notes/agent_bicycle_session_failures_2026_09_14.md):
//! a sketch whose `plane_normal` is a unit vector rounded to 6 decimals
//! (|n| − 1 ≈ 5×10⁻⁷) is stored verbatim, and the engine handed it to the kernel
//! unnormalized.
//!
//! - F2: a whole-circle profile extrude was rejected with
//!   `ProfileCircleFrameNotOrthonormal` (kernel frame tolerance 1e-9).
//! - F1: an explicit-region annulus extrude BUILT, but its circle edges were
//!   scaled by |n|; the first boolean against it failed with "circle edge 0:
//!   endpoint vertex 0 is not on the circle".
//!
//! Real kernel (kernel-v2) through raw `wasm_bridge::dispatch`, the path the
//! app and the agent link use.

use feature_engine::types::*;
use modeling_ops::KernelBundle;
use std::collections::HashMap;
use test_harness::helpers::{rect_profile, ProfileData};
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::{dispatch, EngineState, UiToEngine};

/// The down-tube axis as an agent sent it: six decimals, not unit length.
const ROUNDED_NORMAL: [f64; 3] = [0.718286, 0.0, 0.695747];

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
    entities: Vec<SketchEntity>,
    positions: HashMap<u32, (f64, f64)>,
    profiles: Vec<ClosedProfile>,
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
            plane_x_axis: None,
            entities: vec![],
            constraints: vec![],
            projected: vec![],
        },
        kernel,
    );
    last_feature_id(state)
}

/// Concentric circles r_outer (entity 1) and r_inner (entity 2) about point 0.
fn circles(r_outer: f64, r_inner: Option<f64>) -> ProfileData {
    let mut entities = vec![
        SketchEntity::Point {
            id: 0,
            x: 0.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Circle {
            id: 1,
            center_id: 0,
            radius: r_outer,
            construction: false,
        },
    ];
    let circle_profile = |id: u32, r: f64| ClosedProfile {
        entity_ids: vec![id],
        is_outer: true,
        vertex_ids: vec![],
        circle: Some(CircleProfile {
            center_u: 0.0,
            center_v: 0.0,
            radius: r,
        }),
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let mut profiles = vec![circle_profile(1, r_outer)];
    if let Some(ri) = r_inner {
        entities.push(SketchEntity::Circle {
            id: 2,
            center_id: 0,
            radius: ri,
            construction: false,
        });
        profiles.push(circle_profile(2, ri));
    }
    (entities, HashMap::from([(0, (0.0, 0.0))]), profiles)
}

#[allow(clippy::too_many_arguments)]
fn add_extrude(
    state: &mut EngineState,
    kernel: &mut dyn KernelBundle,
    sketch_id: Uuid,
    depth: f64,
    combine: CombineMode,
    targets: Option<Vec<GeomRef>>,
    profile_entity_ids: Option<Vec<u32>>,
    region: Option<Region>,
) -> Uuid {
    dispatch(
        state,
        UiToEngine::AddFeature {
            provenance: None,
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    combine: Some(combine),
                    targets,
                    sketch_id,
                    profile_index: 0,
                    profile_entity_ids,
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
            },
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

/// F2: a whole-circle extrude on a 6-decimal normal must build.
#[test]
fn circle_profile_extrude_on_rounded_normal_builds() {
    let mut state = EngineState::new();
    let mut kernel = new_kernel();
    let (entities, positions, profiles) = circles(0.0159, None);
    let sketch = finish_sketch(
        &mut state,
        kernel.as_mut(),
        [0.0, 0.0, 0.0],
        ROUNDED_NORMAL,
        entities,
        positions,
        profiles,
    );
    let rod = add_extrude(
        &mut state,
        kernel.as_mut(),
        sketch,
        0.1,
        CombineMode::NewBody,
        None,
        Some(vec![1]),
        None,
    );
    assert!(
        state.engine.errors.is_empty(),
        "circle extrude on a rounded normal must build: {:?}",
        state.engine.errors
    );
    assert!(state.engine.get_result(rod).is_some());
}

/// F1: an annulus region extruded on a 6-decimal normal must be a valid
/// boolean operand (the tube is then cut by an oblique box).
#[test]
fn region_annulus_on_rounded_normal_is_a_valid_boolean_operand() {
    let mut state = EngineState::new();
    let mut kernel = new_kernel();

    let (entities, positions, profiles) = circles(0.0159, Some(0.015));
    let regions = compute_regions(&entities, &positions, regions::DEFAULT_CHORD_TOLERANCE);
    let annulus = regions
        .into_iter()
        .find(|r| !r.holes.is_empty())
        .expect("concentric circles yield an annulus region");
    let sketch = finish_sketch(
        &mut state,
        kernel.as_mut(),
        [0.0, 0.0, 0.0],
        ROUNDED_NORMAL,
        entities,
        positions,
        profiles,
    );
    let tube = add_extrude(
        &mut state,
        kernel.as_mut(),
        sketch,
        0.3,
        CombineMode::NewBody,
        None,
        None,
        Some(annulus),
    );
    assert!(
        state.engine.errors.is_empty(),
        "tube must build: {:?}",
        state.engine.errors
    );

    // Box tool z ∈ [0.10, 0.20], x ∈ [0.10, 0.30], y ∈ [−0.05, 0.05]: the tube
    // axis crosses it obliquely (no coplanar or tangent contact).
    let (entities, positions, profiles) = rect_profile(0.10, -0.05, 0.20, 0.10);
    let tool_sketch = finish_sketch(
        &mut state,
        kernel.as_mut(),
        [0.0, 0.0, 0.10],
        [0.0, 0.0, 1.0],
        entities,
        positions,
        profiles,
    );
    let cut = add_extrude(
        &mut state,
        kernel.as_mut(),
        tool_sketch,
        0.10,
        CombineMode::Cut,
        Some(vec![main_target(tube)]),
        None,
        None,
    );
    assert!(
        state.engine.errors.is_empty(),
        "cutting the rounded-normal tube must succeed: {:?}",
        state.engine.errors
    );
    let result = state.engine.get_result(cut).expect("cut result");
    assert!(
        !result.outputs.is_empty(),
        "the cut tube survives as at least one body"
    );
}
