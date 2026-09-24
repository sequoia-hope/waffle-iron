//! STEP export ORACLE: a third-party reader (truck-stepio, through
//! `step-import`) rebuilds the solid kernel-v2 wrote, and the rebuilt
//! body's face count, surface classification, bounding box and volume match
//! the exact model. The kernel's own unit tests pin the file's structure;
//! this is the semantic check — the same "reference parity" posture the
//! root CLAUDE.md asks of every port. Also the bridge-level message: every
//! live body of a Part, every placed leaf of an open assembly, and the loud
//! warning for a mesh-backed imported body.

use std::collections::HashMap;

use feature_engine::assembly::{AssemblyTree, Instance, PartRef, Transform};
use feature_engine::types::*;
use kernel_v2::KernelV2Adapter;
use serde_json::Map;
use uuid::Uuid;
use waffle_types::kernel::{
    CircleProfile, ImportedBodyData, ImportedSurface, Kernel as _, KernelSolidHandle,
    RigidPlacement, StepExportBody,
};
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

// ── kernel-side solids ──────────────────────────────────────────────────

fn make_box(
    kernel: &mut KernelV2Adapter,
    origin: [f64; 3],
    side: f64,
    height: f64,
) -> KernelSolidHandle {
    let mut positions = HashMap::new();
    positions.insert(1, (0.0, 0.0));
    positions.insert(2, (side, 0.0));
    positions.insert(3, (side, side));
    positions.insert(4, (0.0, side));
    let faces = kernel
        .make_faces_from_profiles(
            &[ClosedProfile {
                entity_ids: vec![1, 2, 3, 4],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            origin,
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("square stages");
    kernel
        .extrude_face(faces[0], [0.0, 0.0, 1.0], height)
        .expect("box")
}

fn make_cylinder(
    kernel: &mut KernelV2Adapter,
    center: (f64, f64),
    radius: f64,
    z0: f64,
    height: f64,
) -> KernelSolidHandle {
    let faces = kernel
        .make_faces_from_profiles(
            &[ClosedProfile {
                entity_ids: vec![7],
                is_outer: true,
                vertex_ids: vec![],
                circle: Some(CircleProfile {
                    center_u: center.0,
                    center_v: center.1,
                    radius,
                }),
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            [0.0, 0.0, z0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &HashMap::new(),
        )
        .expect("circle stages");
    kernel
        .extrude_face(faces[0], [0.0, 0.0, 1.0], height)
        .expect("cylinder")
}

// ── the reader's view ───────────────────────────────────────────────────

fn read_back(step_text: &str) -> ImportedBodyData {
    step_import::parse_step(step_text, "roundtrip.step").expect("truck reads what kernel-v2 wrote")
}

/// Signed volume of the per-face triangle meshes (each CCW seen from
/// outside): the solid's volume when the faces close up.
fn volume(data: &ImportedBodyData) -> f64 {
    let mut vol = 0.0;
    for shell in &data.shells {
        for face in &shell.faces {
            let p = |i: u32| {
                let i = i as usize * 3;
                [
                    face.positions[i],
                    face.positions[i + 1],
                    face.positions[i + 2],
                ]
            };
            for t in face.indices.chunks(3) {
                let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                vol += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0]))
                    / 6.0;
            }
        }
    }
    vol
}

fn bbox(data: &ImportedBodyData) -> [f64; 6] {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for shell in &data.shells {
        for face in &shell.faces {
            for p in face.positions.chunks(3) {
                for k in 0..3 {
                    b[k] = b[k].min(p[k]);
                    b[k + 3] = b[k + 3].max(p[k]);
                }
            }
        }
    }
    b
}

fn surface_counts(data: &ImportedBodyData) -> (usize, usize, usize) {
    let mut planar = 0;
    let mut cylindrical = 0;
    let mut other = 0;
    for shell in &data.shells {
        for face in &shell.faces {
            match face.surface {
                ImportedSurface::Plane { .. } => planar += 1,
                ImportedSurface::Cylindrical => cylindrical += 1,
                _ => other += 1,
            }
        }
    }
    (planar, cylindrical, other)
}

fn assert_close(actual: f64, expected: f64, rel: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= rel * expected.abs().max(1e-300),
        "{what}: {actual} vs {expected} (rel {rel})"
    );
}

fn assert_bbox(data: &ImportedBodyData, expected: [f64; 6], tol: f64) {
    let b = bbox(data);
    for k in 0..6 {
        assert!(
            (b[k] - expected[k]).abs() <= tol,
            "bbox[{k}] = {} vs {} (tol {tol}); full {b:?}",
            b[k],
            expected[k]
        );
    }
}

// ── round trips ─────────────────────────────────────────────────────────

/// A 20 × 20 × 10 mm box: six planar faces, exact bbox and volume in
/// meters (the reader scans the file's SI_UNIT millimetres).
#[test]
fn box_round_trips_through_the_truck_reader() {
    let mut kernel = KernelV2Adapter::new();
    let handle = make_box(&mut kernel, [0.0, 0.0, 0.0], 0.02, 0.01);
    let text = kernel.export_step(&handle, "box.step").expect("export");

    let data = read_back(&text);
    assert_eq!(data.face_count(), 6, "{:?}", data.warnings);
    assert_eq!(surface_counts(&data), (6, 0, 0));
    assert_bbox(&data, [0.0, 0.0, 0.0, 0.02, 0.02, 0.01], 1e-12);
    assert_close(volume(&data), 0.02 * 0.02 * 0.01, 1e-9, "box volume");
}

/// A cylinder: the analytic CIRCLE rims (closed edges on one seam vertex)
/// and the CYLINDRICAL_SURFACE lateral with its doubled seam are read as
/// two planar caps + one cylindrical face, at the right size.
#[test]
fn cylinder_round_trips_with_analytic_classification() {
    let mut kernel = KernelV2Adapter::new();
    let handle = make_cylinder(&mut kernel, (0.0, 0.0), 0.005, 0.0, 0.03);
    let text = kernel.export_step(&handle, "cyl.step").expect("export");

    let data = read_back(&text);
    assert_eq!(data.face_count(), 3, "{:?}", data.warnings);
    assert_eq!(surface_counts(&data), (2, 1, 0), "caps + lateral");
    // The reader tessellates the lateral at its own chord tolerance: the
    // inscribed polygon's extreme vertices sit a chord sag inside the true
    // rim (bbox), and it under-reads the volume by a few tenths of a percent.
    assert_bbox(&data, [-0.005, -0.005, 0.0, 0.005, 0.005, 0.03], 5e-5);
    // (Measured: the reader's ~28-gon under-reads this cylinder by 0.78 %.)
    assert_close(
        volume(&data),
        std::f64::consts::PI * 0.005 * 0.005 * 0.03,
        1e-2,
        "cylinder volume",
    );
}

/// A block with a through-hole (a boolean result): the cavity wall is
/// written with reversed sense and the hole rims as circles; the reader's
/// volume is block minus bore.
#[test]
fn drilled_block_round_trips_with_its_bore() {
    let mut kernel = KernelV2Adapter::new();
    let block = make_box(&mut kernel, [0.0, 0.0, 0.0], 0.04, 0.01);
    let drill = make_cylinder(&mut kernel, (0.02, 0.02), 0.005, -0.005, 0.02);
    let holed = kernel
        .boolean_subtract(&block, &drill)
        .expect("box minus cylinder");
    let text = kernel.export_step(&holed, "holed.step").expect("export");

    let data = read_back(&text);
    let (planar, cylindrical, other) = surface_counts(&data);
    assert_eq!(planar, 6, "{:?}", data.warnings);
    assert!(cylindrical >= 1, "the bore wall");
    assert_eq!(other, 0);
    assert_bbox(&data, [0.0, 0.0, 0.0, 0.04, 0.04, 0.01], 1e-9);
    let exact = 0.04 * 0.04 * 0.01 - std::f64::consts::PI * 0.005 * 0.005 * 0.01;
    assert_close(volume(&data), exact, 5e-3, "drilled volume");
}

/// A placement is applied to the written geometry: the same cylinder
/// exported at (0.1, 0, 0) reads back there.
#[test]
fn placed_export_reads_back_at_the_placement() {
    let mut kernel = KernelV2Adapter::new();
    let handle = make_cylinder(&mut kernel, (0.0, 0.0), 0.005, 0.0, 0.03);
    let text = kernel
        .export_step_bodies(
            &[StepExportBody {
                handle,
                name: "pin".into(),
                placement: Some(RigidPlacement {
                    translation: [0.1, 0.0, 0.0],
                    ..RigidPlacement::IDENTITY
                }),
            }],
            "placed.step",
        )
        .expect("export");
    let data = read_back(&text);
    assert_eq!(data.face_count(), 3);
    assert_bbox(&data, [0.095, -0.005, 0.0, 0.105, 0.005, 0.03], 5e-5);
}

// ── the bridge message ──────────────────────────────────────────────────

fn placeholder_plane() -> GeomRef {
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

/// A square sketch of `side` at `(x0, y0)` on the z = 0 plane; returns the
/// sketch feature id.
fn square_sketch(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    (x0, y0): (f64, f64),
    side: f64,
) -> Uuid {
    dispatch(
        state,
        UiToEngine::BeginSketch {
            plane: placeholder_plane(),
        },
        kernel,
    );
    let corners = [
        (x0, y0),
        (x0 + side, y0),
        (x0 + side, y0 + side),
        (x0, y0 + side),
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
    for e in &entities {
        dispatch(
            state,
            UiToEngine::AddSketchEntity { entity: e.clone() },
            kernel,
        );
    }
    let response = dispatch(
        state,
        UiToEngine::FinishSketch {
            provenance: None,
            solved_positions,
            solved_profiles: vec![ClosedProfile {
                entity_ids: vec![1, 2, 3, 4],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: None,
            entities,
            constraints: vec![],
            projected: vec![],
        },
        kernel,
    );
    match response {
        EngineToUi::ModelUpdated { feature_tree, .. } => feature_tree.features.last().unwrap().id,
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
}

fn new_body_extrude(sketch_id: Uuid, depth: f64) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            profile_entity_ids: None,
            depth,
            depth_expr: None,
            direction: None,
            symmetric: false,
            cut: false,
            merge: false,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: vec![],
            combine: Some(CombineMode::NewBody),
            targets: None,
        },
    }
}

fn count_solids(text: &str) -> usize {
    text.matches("MANIFOLD_SOLID_BREP(").count()
}

/// `ExportStep` writes EVERY live body of the part (not just the last), and
/// reports — rather than drops or facets — a mesh-backed imported body.
#[test]
fn export_step_message_writes_every_live_body_and_warns_about_imported_ones() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    state.set_project_name("two-bodies");

    let s1 = square_sketch(&mut state, &mut kernel, (0.0, 0.0), 0.02);
    dispatch(
        &mut state,
        UiToEngine::AddFeature {
            provenance: None,
            operation: new_body_extrude(s1, 0.01),
        },
        &mut kernel,
    );
    let s2 = square_sketch(&mut state, &mut kernel, (0.1, 0.0), 0.02);
    dispatch(
        &mut state,
        UiToEngine::AddFeature {
            provenance: None,
            operation: new_body_extrude(s2, 0.01),
        },
        &mut kernel,
    );
    dispatch(
        &mut state,
        UiToEngine::ImportStep {
            file_name: "cube.step".to_string(),
            data: CUBE_STEP.to_string(),
        },
        &mut kernel,
    );
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);

    let response = dispatch(&mut state, UiToEngine::ExportStep, &mut kernel);
    let EngineToUi::ExportReady {
        step_data,
        warnings,
    } = response
    else {
        panic!("expected ExportReady, got {response:?}");
    };
    assert_eq!(count_solids(&step_data), 2, "both extrudes");
    assert!(step_data.contains("FILE_NAME('two-bodies.step',"));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("Import cube.step") && warnings[0].contains("imported"),
        "{warnings:?}"
    );

    let data = read_back(&step_data);
    assert_eq!(data.face_count(), 12);
    // Two 20 mm squares 100 mm apart in the sketch plane, 10 mm tall: the
    // extents are 120 × 20 × 10 mm whichever world axes the sketch basis
    // maps u and v onto.
    let b = bbox(&data);
    let mut extents = [b[3] - b[0], b[4] - b[1], b[5] - b[2]];
    extents.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (got, want) in extents.iter().zip([0.01, 0.02, 0.12]) {
        assert!((got - want).abs() <= 1e-12, "extents {extents:?}");
    }
    assert_close(volume(&data), 2.0 * 0.02 * 0.02 * 0.01, 1e-9, "two boxes");
}

/// With an assembly open, `ExportStep` writes each rendered instance's
/// bodies at their solved world placements — a flat multi-body file.
#[test]
fn export_step_with_an_open_assembly_places_every_instance() {
    // The part: a 20 × 20 × 10 mm box, built once in a scratch state.
    let mut kernel = KernelV2Adapter::new();
    let part_tree = {
        let mut scratch = EngineState::new();
        let s = square_sketch(&mut scratch, &mut kernel, (0.0, 0.0), 0.02);
        dispatch(
            &mut scratch,
            UiToEngine::AddFeature {
                provenance: None,
                operation: new_body_extrude(s, 0.01),
            },
            &mut kernel,
        );
        scratch.engine.tree.clone()
    };
    // The document holds both tabs now (S2 C3b): the Part tab carries the tree
    // the instances reference — as the LIVE tree, which the switch into the
    // assembly stashes into it — and the Assembly tab carries the assembly.
    // `OpenAssembly` only names the tab.
    //
    // So the state must exist BEFORE the instances: they name ITS Part tab.
    // Taking that id from a throwaway `EngineState` names a tab this document
    // does not have, and the assembly then renders nothing at all.
    let mut state = EngineState::new();
    state.set_project_name("asm");
    state.engine.tree = part_tree;
    let part_tab = state.session.tabs()[0].id.clone();

    let instance = |name: &str, t: Transform, fixed: bool| Instance {
        id: Uuid::new_v4(),
        name: name.into(),
        source: PartRef {
            source_id: None,
            tab_id: part_tab.clone(),
        },
        transform: t,
        fixed,
        suppressed: false,
        external_key: None,
        parameter_overrides: None,
        extra: Map::new(),
    };
    let tree = AssemblyTree {
        instances: vec![
            instance("A", Transform::identity(), true),
            instance("B", Transform::translation([0.05, 0.0, 0.0]), true),
        ],
        ..Default::default()
    };

    let asm_tab = state
        .session
        .add_tab("Assembly", None)
        .expect("assembly tab");
    state
        .session
        .set_assembly(&asm_tab, tree)
        .expect("the assembly tab takes its tree");
    let r = dispatch(
        &mut state,
        UiToEngine::OpenAssembly { tab_id: asm_tab },
        &mut kernel,
    );
    assert!(matches!(r, EngineToUi::ModelUpdated { .. }), "{r:?}");

    let response = dispatch(&mut state, UiToEngine::ExportStep, &mut kernel);
    let EngineToUi::ExportReady {
        step_data,
        warnings,
    } = response
    else {
        panic!("expected ExportReady, got {response:?}");
    };
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(count_solids(&step_data), 2, "one per instance");
    assert!(
        step_data.contains("MANIFOLD_SOLID_BREP('A / "),
        "named by instance: {step_data}"
    );
    assert!(step_data.contains("MANIFOLD_SOLID_BREP('B / "));

    let data = read_back(&step_data);
    assert_eq!(data.face_count(), 12);
    // A at the origin, B shifted 50 mm in x: 0 … 20 mm and 50 … 70 mm
    // (sketch v along −y, as above).
    assert_bbox(&data, [0.0, -0.02, 0.0, 0.07, 0.0, 0.01], 1e-12);
    assert_close(
        volume(&data),
        2.0 * 0.02 * 0.02 * 0.01,
        1e-9,
        "two placed boxes",
    );
}
