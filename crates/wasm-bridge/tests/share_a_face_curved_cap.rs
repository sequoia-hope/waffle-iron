//! Share-a-face auto-targeting on a CURVED-rim cap (the bearing-recess
//! scenario of `app/tests/gui/bearing-recess.spec.js`): a cut sketched on a
//! cylinder's cap must find that cylinder as its target through the
//! geometric path (spec `optional_booleans_multibody_extrude.md` §4.3(b) —
//! the app's local-face sketch carries a placeholder Datum anchor, so anchor
//! ownership cannot name the body). Measured before the fix: "GeomRef
//! resolution failed: Cut requires at least one target body" — the face
//! footprint was built from edge VERTICES and a circular cap has one seam
//! vertex. Runs on the REAL kernel-v2 adapter (MockKernel has no curved rims).

use std::collections::HashMap;

use feature_engine::types::*;
use kernel_v2::KernelV2Adapter;
use uuid::Uuid;
use waffle_types::kernel::Kernel as _;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

/// The app's placeholder plane reference for a sketch on a LOCAL face (see
/// `enterSketchMode` in the store): the snapshot origin/normal passed to
/// `FinishSketch` are authoritative.
fn local_face_placeholder_plane() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
        scope: None,
    }
}

/// A circle sketch at `(cx, cy)` of radius `r` on the plane `(origin, normal)`,
/// finished through the bridge; returns the sketch feature id.
fn circle_sketch(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    origin: [f64; 3],
    normal: [f64; 3],
    (cx, cy): (f64, f64),
    r: f64,
) -> Uuid {
    dispatch(
        state,
        UiToEngine::BeginSketch {
            plane: local_face_placeholder_plane(),
        },
        kernel,
    );
    // Center point + the circle entity; a circle profile carries its own
    // analytic descriptor (`ClosedProfile::circle`), no chord polygon needed.
    let entities = vec![
        SketchEntity::Point {
            id: 1,
            x: cx,
            y: cy,
            construction: true,
        },
        SketchEntity::Circle {
            id: 2,
            center_id: 1,
            radius: r,
            construction: false,
        },
    ];
    for e in &entities {
        dispatch(
            state,
            UiToEngine::AddSketchEntity { entity: e.clone() },
            kernel,
        );
    }
    let mut solved_positions = HashMap::new();
    solved_positions.insert(1, (cx, cy));
    let response = dispatch(
        state,
        UiToEngine::FinishSketch {
            provenance: None,
            solved_positions,
            solved_profiles: vec![ClosedProfile {
                entity_ids: vec![2],
                is_outer: true,
                vertex_ids: vec![],
                circle: Some(CircleProfile {
                    center_u: cx,
                    center_v: cy,
                    radius: r,
                }),
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            plane_origin: origin,
            plane_normal: normal,
            plane_x_axis: None,
            entities,
            constraints: vec![],
            projected: vec![],
        },
        kernel,
    );
    match response {
        EngineToUi::ModelUpdated { feature_tree, .. } => {
            feature_tree.features.last().expect("sketch feature").id
        }
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
}

fn extrude(sketch_id: Uuid, depth: f64, combine: CombineMode) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            profile_entity_ids: None,
            depth,
            depth_expr: None,
            direction: None,
            symmetric: false,
            cut: matches!(combine, CombineMode::Cut),
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: vec![],
            combine: Some(combine),
            targets: None,
        },
    }
}

fn mesh_volume(mesh: &waffle_types::kernel::RenderMesh) -> f64 {
    let p = |i: u32| {
        let i = i as usize * 3;
        [
            mesh.vertices[i] as f64,
            mesh.vertices[i + 1] as f64,
            mesh.vertices[i + 2] as f64,
        ]
    };
    mesh.indices
        .chunks(3)
        .map(|t| {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

/// Circle r=60 mm → cylinder 10 mm; a concentric r=30 mm circle sketched on
/// the BASE cap (the face `getFirstFaceRef` picks: base cap, lateral, top
/// cap) cut 4 mm deep with the dialog's default Auto targets. Real model
/// scale (meters), like the GUI canary.
#[test]
fn cut_sketched_on_a_cylinder_cap_auto_targets_the_cylinder() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();

    let base = circle_sketch(
        &mut state,
        &mut kernel,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        (0.0, 0.0),
        0.060,
    );
    dispatch(
        &mut state,
        UiToEngine::AddFeature {
            provenance: None,
            operation: extrude(base, 0.010, CombineMode::Add),
        },
        &mut kernel,
    );
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);

    // The base cap's outward normal is −z; the sketch plane snapshot is what
    // the app records when the user clicks that face.
    let recess = circle_sketch(
        &mut state,
        &mut kernel,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, -1.0],
        (0.0, 0.0),
        0.030,
    );
    let response = dispatch(
        &mut state,
        UiToEngine::AddFeature {
            provenance: None,
            operation: extrude(recess, 0.004, CombineMode::Cut),
        },
        &mut kernel,
    );
    assert!(
        matches!(response, EngineToUi::ModelUpdated { .. }),
        "cut must apply: {response:?}"
    );
    assert!(
        state.engine.errors.is_empty(),
        "the cut must find its target through the cap's curved rim: {:?}",
        state.engine.errors
    );
    assert_eq!(state.engine.tree.features.len(), 4);

    // The cylinder body was CONSUMED by the cut (one live body: the recessed
    // cylinder), with the recess volume removed.
    let cut_id = state.engine.tree.features[3].id;
    let result = state
        .engine
        .feature_results
        .get(&cut_id)
        .expect("cut result");
    assert_eq!(result.outputs.len(), 1, "one recessed body");
    assert!(state
        .engine
        .consumed_features
        .contains(&state.engine.tree.features[1].id));
    let mesh = kernel
        .tessellate(&result.outputs[0].1.handle, 0.001)
        .expect("mesh");
    let vol = mesh_volume(&mesh).abs();
    let exact = std::f64::consts::PI * (0.060 * 0.060 * 0.010 - 0.030 * 0.030 * 0.004);
    assert!(
        (vol - exact).abs() <= 3e-3 * exact,
        "recessed volume {vol} vs analytic {exact}"
    );
}
